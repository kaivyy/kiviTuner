//! B4a: `HexIngestResult.base_address` must distinguish "known zero" from
//! "unknown".
//!
//! Before B4a the field was a bare `u32` initialised to `0` for raw binary and
//! for empty input, so a raw BIN and a HEX image that genuinely starts at
//! address 0 were indistinguishable. These tests pin the corrected contract.
//!
//! Expected values come from the design specification
//! (`docs/migration/2026-10-27-rom-address-design.md` §5), not from running the
//! implementation.

use kivi_core::hex_ingest::{
    ingest_firmware, ingest_hex_text, ingest_srec_text, FirmwareFormat,
};

/// Builds an Intel HEX line with a correct two's-complement checksum.
/// `rec` is the full payload: `[addr_hi, addr_lo, rectype, len, data...]`.
fn hex_line(rec: &[u8]) -> String {
    let mut v = rec.to_vec();
    let sum: u8 = v.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    v.push((!sum).wrapping_add(1));
    format!(
        ":{}\n",
        v.iter().map(|b| format!("{:02X}", b)).collect::<String>()
    )
}

/// Data record: `[byte_count, addr_hi, addr_lo, 0x00, data...]`.
fn hex_data(addr: u16, data: &[u8]) -> String {
    let mut rec = vec![
        data.len() as u8,
        (addr >> 8) as u8,
        (addr & 0xFF) as u8,
        0x00,
    ];
    rec.extend_from_slice(data);
    hex_line(&rec)
}

fn srec_line(rtype: char, rec: &[u8]) -> String {
    let mut v = rec.to_vec();
    let sum: u8 = v.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    v.push(!sum);
    format!(
        "S{}{}\n",
        rtype,
        v.iter().map(|b| format!("{:02X}", b)).collect::<String>()
    )
}

// ---------------------------------------------------------------------------
// HEX
// ---------------------------------------------------------------------------

#[test]
fn hex_starting_at_zero_reports_known_zero() {
    let text = format!("{}:00000001FF\n", hex_data(0x0000, &[1, 2, 3]));
    let r = ingest_hex_text(&text, 0xFF).expect("parse");
    assert_eq!(
        r.base_address,
        Some(0),
        "a record at address 0 is a *known* base of zero"
    );
}

#[test]
fn hex_starting_at_nonzero_reports_that_address() {
    let text = format!("{}:00000001FF\n", hex_data(0x1000, &[1, 2, 3]));
    let r = ingest_hex_text(&text, 0xFF).expect("parse");
    assert_eq!(r.base_address, Some(0x1000));
}

#[test]
fn hex_with_no_data_records_reports_unknown() {
    // EOF record only: no data was supplied, so no address is known.
    let r = ingest_hex_text(":00000001FF\n", 0xFF).expect("parse");
    assert_eq!(
        r.base_address,
        None,
        "an image with no data records has no known base"
    );
    assert!(r.bytes.is_empty());
}

#[test]
fn hex_empty_input_reports_unknown() {
    let r = ingest_hex_text("", 0xFF).expect("parse");
    assert_eq!(r.base_address, None);
    assert!(r.bytes.is_empty());
}

#[test]
fn hex_entry_point_is_not_mistaken_for_base_address() {
    // Start-address record 0x05 carries the entry point, not the image base.
    let start = hex_line(&[0x04, 0x00, 0x00, 0x05, 0x00, 0x00, 0x12, 0x34]);
    let text = format!("{}{}:00000001FF\n", hex_data(0x2000, &[9]), start);
    let r = ingest_hex_text(&text, 0xFF).expect("parse");
    assert_eq!(r.base_address, Some(0x2000), "base comes from data records");
    assert_eq!(r.entry_point, Some(0x1234), "entry point is separate");
}

#[test]
fn hex_gap_padding_is_unchanged_by_b4a() {
    // Two records 0x10 apart; the gap must still be filled with the pad byte.
    let text = format!(
        "{}{}:00000001FF\n",
        hex_data(0x0000, &[0xAA]),
        hex_data(0x0010, &[0xBB])
    );
    let r = ingest_hex_text(&text, 0xFF).expect("parse");
    assert_eq!(r.base_address, Some(0));
    assert_eq!(r.bytes.len(), 0x11);
    assert_eq!(r.bytes[0], 0xAA);
    assert_eq!(r.bytes[0x10], 0xBB);
    assert_eq!(r.bytes[1], 0xFF, "gap still padded");
}

// ---------------------------------------------------------------------------
// S-Record
// ---------------------------------------------------------------------------

#[test]
fn srec_starting_at_nonzero_reports_that_address() {
    // S1 record, 2-byte address, 3 data bytes at 0x2000.
    let line = srec_line('1', &[0x06, 0x20, 0x00, 0x11, 0x22, 0x33]);
    let text = format!("{}S9030000FC\n", line);
    let r = ingest_srec_text(&text, 0xFF).expect("parse");
    assert_eq!(r.base_address, Some(0x2000));
}

#[test]
fn srec_with_no_data_records_reports_unknown() {
    let r = ingest_srec_text("S9030000FC\n", 0xFF).expect("parse");
    assert_eq!(r.base_address, None);
    assert!(r.bytes.is_empty());
}

// ---------------------------------------------------------------------------
// Unified entry point: raw binary has no inherent address
// ---------------------------------------------------------------------------

#[test]
fn raw_binary_reports_unknown_base() {
    let r = ingest_firmware(&[0xDE, 0xAD, 0xBE, 0xEF], 0xFF).expect("ingest");
    assert_eq!(r.format, FirmwareFormat::RawBinary);
    assert_eq!(
        r.base_address,
        None,
        "a raw BIN carries no absolute address"
    );
    assert_eq!(r.bytes, vec![0xDE, 0xAD, 0xBE, 0xEF]);
}

#[test]
fn raw_binary_does_not_inherit_an_address_from_previous_input() {
    let hex_text = format!("{}:00000001FF\n", hex_data(0x5000, &[1]));
    let first = ingest_firmware(hex_text.as_bytes(), 0xFF).expect("hex");
    assert_eq!(first.base_address, Some(0x5000));

    let second = ingest_firmware(&[0x01, 0x02], 0xFF).expect("bin");
    assert_eq!(
        second.base_address,
        None,
        "a later raw BIN must not inherit the previous image's address"
    );
}
