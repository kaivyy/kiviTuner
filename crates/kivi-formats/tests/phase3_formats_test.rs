use kivi_formats::bmw_spdaten::{export_intel_hex, export_motorola_s3, parse_hex_or_srec, HexSegment};
use kivi_formats::container::SgoContainer;
use kivi_formats::gm_e92_rle::{compress, decompress, is_wrapped, unwrap, wrap};
use kivi_formats::ols_project::parse_ols;
use kivi_formats::sgo_frf_pack::{
    frf_pack_odx, frf_unpack_odx, lzss_compress, lzss_decompress, pack_sgo_container,
    SgoPackBlockSpec,
};
use std::collections::BTreeMap;

#[test]
fn gm_e92_rle_roundtrip_and_wrapper() {
    let mut raw = vec![0xFFu8; 300];
    raw.extend_from_slice(&[0x12, 0x34].repeat(40));
    raw.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF].repeat(30));
    raw.extend_from_slice(b"CalibrationPayload12345");

    let coded = compress(&raw);
    assert!(coded.len() < raw.len());
    let wrapped = wrap(&coded);
    assert!(is_wrapped(&wrapped));
    let decoded = decompress(unwrap(&wrapped).unwrap()).unwrap();
    assert_eq!(decoded, raw);
}

#[test]
fn bmw_spdaten_ihex_and_srec_roundtrip() {
    let mut meta = BTreeMap::new();
    meta.insert("ZB-NUMBER".to_string(), "7545150".to_string());
    meta.insert("P-SGBD".to_string(), "MS43".to_string());
    let segs = vec![
        HexSegment {
            address: 0x0001_0000,
            data: (0u8..=63).collect(),
        },
        HexSegment {
            address: 0x0002_0400,
            data: vec![0xAA; 32],
        },
    ];

    let hex_text = export_intel_hex(&segs, Some(&meta), 16);
    let parsed_hex = parse_hex_or_srec(&hex_text).unwrap();
    assert_eq!(parsed_hex.format, "bmw_spdaten_ihex");
    assert_eq!(parsed_hex.metadata.get("ZB-NUMBER").unwrap(), "7545150");
    assert_eq!(parsed_hex.segments, segs);
    let rom = parsed_hex.assemble_rom(0x0001_0000, 0x10500, 0xFF).unwrap();
    assert_eq!(&rom[..64], segs[0].data.as_slice());
    assert_eq!(&rom[0x10400..0x10420], segs[1].data.as_slice());

    let srec = export_motorola_s3(&segs, Some("BMW_MS43"), 16);
    let parsed_srec = parse_hex_or_srec(&srec).unwrap();
    assert_eq!(parsed_srec.format, "motorola_srec");
    assert_eq!(parsed_srec.segments, segs);
}

#[test]
fn sgo_frf_lzss_pack_unpack_roundtrip() {
    let plain = b"Calibration Data Repeating Pattern Repeating Pattern 0123456789ABCDEF".repeat(4);
    let comp = lzss_compress(&plain, false, true);
    let decomp = lzss_decompress(&comp).unwrap();
    assert_eq!(decomp, plain);

    let sgo = pack_sgo_container(
        0x0001_0000,
        "8K0907115N",
        "0003",
        500_000,
        0x07E0,
        0x07E8,
        &[0x68, 0x01, 0x93],
        &[SgoPackBlockSpec {
            block_address: 1,
            crypt_mode: 0,
            erase_start: 0x08_0000,
            erase_end: 0x08_FFFF,
            prog_start: 0x08_0000,
            prog_end: 0x08_FFFF,
            compress_bcb: true,
            raw_data: plain.clone(),
        }],
    );
    assert!(SgoContainer::verify_sgo_whole_file_checksum(&sgo).unwrap());
    let parsed = SgoContainer::parse_sgo(&sgo).unwrap();
    assert_eq!(parsed.part_number, "8K0907115N");
    assert_eq!(parsed.blocks.len(), 1);
    assert_eq!(parsed.blocks[0].data, plain);

    let odx = b"<FLASH><ECU-MEMS></ECU-MEMS></FLASH>";
    let key = b"KIVI_TEST_FRF_KEY";
    let frf = frf_pack_odx(odx, "test_ecu.odx", key).unwrap();
    let (entry_name, unpacked) = frf_unpack_odx(&frf, key).unwrap();
    assert_eq!(entry_name, "test_ecu.odx");
    assert_eq!(unpacked, odx);
}

#[test]
fn ols_project_header_metadata_and_segment_assembly() {
    let mut ols = vec![0u8; 0x300];
    ols[0..4].copy_from_slice(&0x0000_000Bu32.to_le_bytes());
    ols[4..15].copy_from_slice(b"WinOLS File");
    ols[0x0F] = 0;
    ols[0x10..0x14].copy_from_slice(&100u32.to_le_bytes());
    let ols_len = ols.len() as u32;
    ols[0x14..0x18].copy_from_slice(&ols_len.to_le_bytes());

    let parsed = parse_ols(&ols).unwrap();
    assert_eq!(parsed.header.format_version, 100);
}
