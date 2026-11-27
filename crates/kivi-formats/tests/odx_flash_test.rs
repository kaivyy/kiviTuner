//! ODX 2.0.1 FLASH parsing — the document inside a VAG `.frf`.
//!
//! ## Source
//!
//! `VW_Flash/extractodx.py` and the `simos-suite` research note
//! `bin-to-frf-sgo-packing.md` document the shape:
//!
//! ```text
//! FLASH > ECU-MEMS > ECU-MEM > MEM > SESSIONS > SESSION
//!       > { SECURITYS, DATABLOCKS, FLASHDATAS }
//! FLASHDATA: SHORT-NAME, <DATA>UPPERCASEHEX</DATA>,
//!            <ENCRYPT-COMPRESS-METHOD> (the DFI byte)
//! DATABLOCK: SHORT-NAME (DB_n), FLASHDATA-REF,
//!            SEGMENTS/SEGMENT/SOURCE-START-ADDRESS
//! ```
//!
//! kiviTuner could decrypt an FRF to bytes but had no way to read the ODX
//! inside it, so an FRF could be unlocked and still not used.

use kivi_formats::odx_flash::{parse_odx_flash, DataFormatIdentifier, OdxError};

/// A minimal but structurally faithful ODX FLASH document.
const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ODX MODEL-VERSION="2.2.0">
  <FLASH>
    <ECU-MEMS>
      <ECU-MEM>
        <MEM>
          <SESSIONS>
            <SESSION SHORT-NAME="Session_1">
              <DATABLOCKS>
                <DATABLOCK>
                  <SHORT-NAME>DB_1</SHORT-NAME>
                  <FLASHDATA-REF><SNREF SHORT-NAME="FD_1"/></FLASHDATA-REF>
                  <SEGMENTS><SEGMENT><SOURCE-START-ADDRESS>1</SOURCE-START-ADDRESS></SEGMENT></SEGMENTS>
                </DATABLOCK>
              </DATABLOCKS>
              <FLASHDATAS>
                <FLASHDATA>
                  <SHORT-NAME>FD_1</SHORT-NAME>
                  <DATAFORMAT SELECTION="BINARY"/>
                  <ENCRYPT-COMPRESS-METHOD>00</ENCRYPT-COMPRESS-METHOD>
                  <DATA>DEADBEEF</DATA>
                </FLASHDATA>
              </FLASHDATAS>
            </SESSION>
          </SESSIONS>
        </MEM>
      </ECU-MEM>
    </ECU-MEMS>
  </FLASH>
</ODX>
"#;

#[test]
fn a_minimal_odx_parses() {
    let doc = parse_odx_flash(SAMPLE).expect("parse");
    assert_eq!(doc.flash_data.len(), 1);
    assert_eq!(doc.data_blocks.len(), 1);
}

#[test]
fn the_short_name_is_captured() {
    let doc = parse_odx_flash(SAMPLE).expect("parse");
    assert_eq!(doc.flash_data[0].short_name, "FD_1");
}

#[test]
fn the_data_hex_is_decoded_to_bytes() {
    let doc = parse_odx_flash(SAMPLE).expect("parse");
    assert_eq!(doc.flash_data[0].data, vec![0xDE, 0xAD, 0xBE, 0xEF]);
}

#[test]
fn the_dfi_byte_is_decoded() {
    let doc = parse_odx_flash(SAMPLE).expect("parse");
    assert_eq!(doc.flash_data[0].method, DataFormatIdentifier::Raw);
}

#[test]
fn the_datablock_links_to_its_flashdata() {
    let doc = parse_odx_flash(SAMPLE).expect("parse");
    let db = &doc.data_blocks[0];
    assert_eq!(db.short_name, "DB_1");
    assert_eq!(db.flashdata_ref, "FD_1");
    assert_eq!(db.source_start_address, 1);
}

// ---------------------------------------------------------------------------
// DFI decoding
// ---------------------------------------------------------------------------

#[test]
fn dfi_00_is_raw() {
    assert_eq!(DataFormatIdentifier::from_byte(0x00), DataFormatIdentifier::Raw);
}

#[test]
fn dfi_01_is_simos_counter_xor() {
    assert_eq!(
        DataFormatIdentifier::from_byte(0x01),
        DataFormatIdentifier::SimosCounterXor
    );
}

#[test]
fn dfi_aa_is_lzss_aes() {
    assert_eq!(
        DataFormatIdentifier::from_byte(0xAA),
        DataFormatIdentifier::LzssAes
    );
}

#[test]
fn an_unknown_dfi_is_preserved() {
    assert_eq!(
        DataFormatIdentifier::from_byte(0x7F),
        DataFormatIdentifier::Unknown(0x7F)
    );
}

// ---------------------------------------------------------------------------
// Rejection
// ---------------------------------------------------------------------------

#[test]
fn a_document_without_flash_is_rejected() {
    // Remove the FLASH element entirely: renaming it to <NOTFLASH> would still
    // contain the substring "<FLASH", which is what the parser looks for.
    let no_flash = SAMPLE
        .replace("<FLASH>", "<WRAPPER>")
        .replace("</FLASH>", "</WRAPPER>");
    assert!(matches!(
        parse_odx_flash(&no_flash).unwrap_err(),
        OdxError::MissingElement(_)
    ));
}

#[test]
fn an_odd_length_hex_payload_is_rejected() {
    let bad = SAMPLE.replace("DEADBEEF", "DEADBEE");
    assert!(matches!(
        parse_odx_flash(&bad).unwrap_err(),
        OdxError::InvalidHex(_)
    ));
}

#[test]
fn a_non_hex_payload_is_rejected() {
    let bad = SAMPLE.replace("DEADBEEF", "ZZZZZZZZ");
    assert!(parse_odx_flash(&bad).is_err());
}

#[test]
fn an_empty_payload_is_allowed() {
    // A block may legitimately carry no data (a placeholder).
    let empty = SAMPLE.replace("DEADBEEF", "");
    let doc = parse_odx_flash(&empty).expect("parse");
    assert!(doc.flash_data[0].data.is_empty());
}

#[test]
fn multiple_flashdata_blocks_are_all_captured() {
    let two = SAMPLE.replace(
        "</FLASHDATAS>",
        r#"<FLASHDATA><SHORT-NAME>FD_2</SHORT-NAME>
             <ENCRYPT-COMPRESS-METHOD>01</ENCRYPT-COMPRESS-METHOD>
             <DATA>0102</DATA></FLASHDATA></FLASHDATAS>"#,
    );
    let doc = parse_odx_flash(&two).expect("parse");
    assert_eq!(doc.flash_data.len(), 2);
    assert_eq!(doc.flash_data[1].short_name, "FD_2");
    assert_eq!(doc.flash_data[1].method, DataFormatIdentifier::SimosCounterXor);
}

// ---------------------------------------------------------------------------
// The full FRF path: decrypt → detect ZIP → parse ODX
// ---------------------------------------------------------------------------

#[test]
fn a_decrypted_frf_payload_can_be_classified_and_its_odx_read() {
    use kivi_formats::container::{frf_plaintext_kind, FrfPlaintext, SgoContainer};

    // Build the plaintext an FRF carries: a ZIP whose stored entry is the ODX.
    // A real ZIP is used so the test exercises the same bytes a loader sees.
    let odx = SAMPLE.as_bytes();
    let mut zip: Vec<u8> = Vec::new();
    // Local file header (stored, no compression).
    zip.extend_from_slice(&[0x50, 0x4B, 0x03, 0x04]);
    zip.extend_from_slice(&20u16.to_le_bytes()); // version needed
    zip.extend_from_slice(&0u16.to_le_bytes()); // flags
    zip.extend_from_slice(&0u16.to_le_bytes()); // method = stored
    zip.extend_from_slice(&0u16.to_le_bytes()); // time
    zip.extend_from_slice(&0u16.to_le_bytes()); // date
    let crc = crc32_of(odx);
    zip.extend_from_slice(&crc.to_le_bytes());
    zip.extend_from_slice(&(odx.len() as u32).to_le_bytes()); // compressed size
    zip.extend_from_slice(&(odx.len() as u32).to_le_bytes()); // uncompressed size
    zip.extend_from_slice(&8u16.to_le_bytes()); // name length
    zip.extend_from_slice(&0u16.to_le_bytes()); // extra length
    zip.extend_from_slice(b"flash.od");
    zip.extend_from_slice(odx);

    // The loader's path: encrypt, decrypt, classify, then parse.
    let key = b"VAG_TEST_KEY";
    let encrypted = SgoContainer::transform_frf_stream(&zip, key).expect("encrypt");
    let decrypted = SgoContainer::transform_frf_stream(&encrypted, key).expect("decrypt");

    assert_eq!(frf_plaintext_kind(&decrypted), FrfPlaintext::Zip);

    // Strip the local header and read the ODX.
    let name_len = 8usize;
    let header_len = 30 + name_len;
    let odx_bytes = &decrypted[header_len..header_len + odx.len()];
    let odx_text = std::str::from_utf8(odx_bytes).expect("odx is utf-8");

    let doc = parse_odx_flash(odx_text).expect("parse odx");
    assert_eq!(doc.flash_data.len(), 1);
    assert_eq!(doc.flash_data[0].data, vec![0xDE, 0xAD, 0xBE, 0xEF]);
}

/// Minimal CRC-32 (IEEE) for the ZIP header, so the fixture is well-formed.
fn crc32_of(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}
