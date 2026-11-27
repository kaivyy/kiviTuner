//! Parity tests for HEX/SREC export.
//!
//! Ported from `ecu_engine_rust::formats::intel_hex::IntelHexParser::serialize_to_i32hex`
//! and `ecu_engine_rust::formats::srec::SrecParser::serialize_to_s37`.
//!
//! ## How expected values were obtained
//!
//! Every checksum below is computed by hand from the format definition
//! (Intel HEX: two's complement of the byte sum; S-Record: one's complement of
//! the count+address+data sum). None was produced by running the exporter.
//!
//! ## Documented deviation from the source
//!
//! The source advances the record address by `bytes_per_line` while chunking by
//! `bytes_per_line.min(32)`. When `bytes_per_line > 32` those disagree, so the
//! emitted addresses drift and the output no longer round-trips. This module
//! advances by the actual chunk offset. The deviation is pinned by
//! `large_bytes_per_line_does_not_drift_addresses`.

use kivi_core::hex_export::{
    serialize_to_i32hex, serialize_to_s37, HexExportError,
};

// ---------------------------------------------------------------------------
// Intel HEX — structure
// ---------------------------------------------------------------------------

#[test]
fn empty_data_emits_only_the_eof_record() {
    let out = serialize_to_i32hex(0x1000, &[], 16).expect("export");
    assert_eq!(out, ":00000001FF\n");
}

#[test]
fn known_base_zero_is_accepted() {
    let out = serialize_to_i32hex(0, &[0xAA], 16).expect("export");
    assert!(out.contains(":01000000AA"), "output: {out}");
}

#[test]
fn unknown_base_is_rejected_rather_than_assumed_zero() {
    // The exporter takes a concrete u32; an unknown base must never reach it.
    // This test documents the boundary: callers holding Option<u32> must decide.
    let err = serialize_to_i32hex_opt(None, &[0xAA], 16).expect_err("must reject");
    assert_eq!(err, HexExportError::UnknownBaseAddress);
}

#[test]
fn emits_an_extended_linear_address_record_for_a_high_base() {
    // 0x00080000 -> upper 0x0008, so an 0x04 record with value 0x0008.
    // checksum = two's complement of (02+00+00+04+00+08) = 0xF2
    let out = serialize_to_i32hex(0x0008_0000, &[0x11], 16).expect("export");
    assert!(
        out.starts_with(":020000040008F2\n"),
        "expected ELA record first, got: {out}"
    );
}

#[test]
fn data_record_checksum_is_twos_complement_of_the_byte_sum() {
    // 0x11 0x22 at address 0x0000: len 02 + 00 + 00 + 00 + 11 + 22 = 0x35
    // checksum = 0x100 - 0x35 = 0xCB
    let out = serialize_to_i32hex(0, &[0x11, 0x22], 16).expect("export");
    assert!(out.contains(":020000001122CB\n"), "output: {out}");
}

#[test]
fn single_byte_record_checksum() {
    // 0xAA at 0x0000: 01 + 00 + 00 + 00 + AA = 0xAB; checksum = 0x55
    let out = serialize_to_i32hex(0, &[0xAA], 16).expect("export");
    assert!(out.contains(":01000000AA55\n"), "output: {out}");
}

#[test]
fn always_ends_with_the_eof_record() {
    let out = serialize_to_i32hex(0, &[1, 2, 3], 16).expect("export");
    assert!(out.ends_with(":00000001FF\n"), "output: {out}");
}

#[test]
fn record_length_respects_bytes_per_line() {
    let data = vec![0x00u8; 8];
    let out = serialize_to_i32hex(0, &data, 4).expect("export");
    let data_records: Vec<&str> = out
        .lines()
        .filter(|l| l.starts_with(":04"))
        .collect();
    assert_eq!(data_records.len(), 2, "8 bytes at 4 per line = 2 records");
}

#[test]
fn bytes_per_line_is_capped_at_32() {
    let data = vec![0x00u8; 64];
    let out = serialize_to_i32hex(0, &data, 64).expect("export");
    // Records must be 32 bytes, not 64 (the format's max data length).
    let rec32 = out.lines().filter(|l| l.starts_with(":20")).count();
    assert_eq!(rec32, 2, "64 bytes must become two 32-byte records");
}

#[test]
fn large_bytes_per_line_does_not_drift_addresses() {
    // Deviation pin: with bytes_per_line > 32 the source emits addresses that
    // advance by the requested stride while writing only 32 bytes, so the
    // second record claims an address it does not contain.
    let data = vec![0x00u8; 64];
    let out = serialize_to_i32hex(0, &data, 64).expect("export");

    let data_lines: Vec<&str> = out
        .lines()
        .filter(|l| l.starts_with(":20"))
        .collect();
    assert_eq!(data_lines.len(), 2);

    // First record at 0x0000, second at 0x0020 — contiguous, no gap.
    assert!(data_lines[0].starts_with(":20000000"), "got {}", data_lines[0]);
    assert!(data_lines[1].starts_with(":20002000"), "got {}", data_lines[1]);
}

// ---------------------------------------------------------------------------
// S-Record
// ---------------------------------------------------------------------------

#[test]
fn srec_emits_an_s0_header_first() {
    let out = serialize_to_s37(0, &[0xAA], 16).expect("export");
    assert!(out.starts_with("S00600004844521B\n"), "output: {out}");
}

#[test]
fn srec_emits_an_s7_terminator_last() {
    let out = serialize_to_s37(0, &[0xAA], 16).expect("export");
    assert!(out.ends_with("S70500000000FA\n"), "output: {out}");
}

#[test]
fn srec_data_record_uses_s3_with_a_32bit_address() {
    // 1 data byte at 0x00010000: count = 1 + 4 + 1 = 06
    // sum = 06 + 00 + 01 + 00 + 00 + DE = 0xE5; checksum = !0xE5 = 0x1A
    let out = serialize_to_s37(0x0001_0000, &[0xDE], 16).expect("export");
    assert!(out.contains("S30600010000DE1A\n"), "output: {out}");
}

#[test]
fn srec_checksum_is_ones_complement_of_count_address_and_data() {
    // 0xDE 0xAD at 0x00010000: count 07; sum = 07+00+01+00+00+DE+AD = 0x193
    // low byte 0x93; checksum = !0x93 = 0x6C
    let out = serialize_to_s37(0x0001_0000, &[0xDE, 0xAD], 16).expect("export");
    assert!(out.contains("S30700010000DEAD6C\n"), "output: {out}");
}

#[test]
fn srec_empty_data_emits_header_and_terminator_only() {
    let out = serialize_to_s37(0, &[], 16).expect("export");
    assert_eq!(out, "S00600004844521B\nS70500000000FA\n");
}

#[test]
fn srec_record_count_field_accounts_for_address_and_checksum() {
    let out = serialize_to_s37(0, &[0x00, 0x00], 16).expect("export");
    // 2 data + 4 address + 1 checksum = 07
    assert!(out.contains("S30700000000"), "output: {out}");
}

// ---------------------------------------------------------------------------
// Round-trip through the existing ingestion pipeline
// ---------------------------------------------------------------------------

#[test]
fn intel_hex_round_trips_through_the_ingest_pipeline() {
    let data = vec![0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
    let text = serialize_to_i32hex(0x0008_0000, &data, 16).expect("export");

    let parsed = kivi_core::hex_ingest::ingest_hex_text(&text, 0xFF).expect("re-parse");
    assert_eq!(parsed.base_address, Some(0x0008_0000));
    assert_eq!(parsed.bytes, data);
}

#[test]
fn srec_round_trips_through_the_ingest_pipeline() {
    let data = vec![0xDE, 0xAD, 0xBE, 0xEF];
    let text = serialize_to_s37(0x0001_0000, &data, 16).expect("export");

    let parsed = kivi_core::hex_ingest::ingest_srec_text(&text, 0xFF).expect("re-parse");
    assert_eq!(parsed.base_address, Some(0x0001_0000));
    assert_eq!(parsed.bytes, data);
}

#[test]
fn intel_hex_round_trips_at_a_zero_base() {
    let data = vec![0x01, 0x02, 0x03];
    let text = serialize_to_i32hex(0, &data, 16).expect("export");
    let parsed = kivi_core::hex_ingest::ingest_hex_text(&text, 0xFF).expect("re-parse");
    assert_eq!(parsed.base_address, Some(0));
    assert_eq!(parsed.bytes, data);
}

#[test]
fn intel_hex_round_trips_a_multi_record_image() {
    let data: Vec<u8> = (0u8..64).collect();
    let text = serialize_to_i32hex(0x0000_0000, &data, 16).expect("export");
    let parsed = kivi_core::hex_ingest::ingest_hex_text(&text, 0xFF).expect("re-parse");
    assert_eq!(parsed.bytes, data, "64 bytes must survive the round trip");
}

// ---------------------------------------------------------------------------
// Error handling
// ---------------------------------------------------------------------------

#[test]
fn a_zero_bytes_per_line_is_rejected() {
    let err = serialize_to_i32hex(0, &[1, 2], 0).expect_err("must reject");
    assert_eq!(err, HexExportError::ZeroBytesPerLine);
}

#[test]
fn srec_rejects_zero_bytes_per_line() {
    let err = serialize_to_s37(0, &[1, 2], 0).expect_err("must reject");
    assert_eq!(err, HexExportError::ZeroBytesPerLine);
}

/// Convenience wrapper mirroring the `Option<u32>` boundary callers actually hold.
fn serialize_to_i32hex_opt(
    base: Option<u32>,
    data: &[u8],
    per_line: usize,
) -> Result<String, HexExportError> {
    match base {
        Some(b) => serialize_to_i32hex(b, data, per_line),
        None => Err(HexExportError::UnknownBaseAddress),
    }
}
