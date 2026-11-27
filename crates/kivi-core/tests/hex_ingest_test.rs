//! Behavioral parity tests for the ported Intel HEX / S-Record ingest.
//!
//! **Expected values come from executing the unmodified source** through a
//! disposable oracle harness (`/tmp/parity-slice6`). Where the target
//! intentionally deviates for safety, that is marked explicitly.
//!
//! ## Intentional safety deviations from the source
//!
//! 1. **Intel HEX oversize byte count.** The source slices
//!    `bytes[4..4 + byte_count]` without checking the payload length, so a
//!    record declaring more bytes than it carries **panics**. The target
//!    validates first and returns an error. Verified: oracle
//!    `hex_oversize_count => ***PANIC***`.
//! 2. **S-Record bare "S" line.** The source calls `line.chars().nth(1).unwrap()`,
//!    which **panics** on a line consisting of just `S`. The target validates
//!    the minimum line shape first and returns an error. Verified: oracle
//!    `srec_bare_S => ***PANIC***`.
//!
//! Everything else preserves source behavior for valid input.

use kivi_core::hex_ingest::{
    detect_format, ingest_firmware, ingest_hex_text, ingest_srec_text, FirmwareFormat,
    HexIngestError,
};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Builds an Intel HEX line with a correct two's-complement checksum.
fn hex_line(rec: &[u8]) -> String {
    let mut v = rec.to_vec();
    let sum: u8 = v.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    v.push((!sum).wrapping_add(1));
    format!(
        ":{}\n",
        v.iter().map(|b| format!("{:02X}", b)).collect::<String>()
    )
}

/// Builds an S-Record line: `S<type><count><payload...><checksum>`.
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

// ===========================================================================
// Format detection
// ===========================================================================

#[test]
fn detect_format_recognizes_intel_hex() {
    assert_eq!(
        detect_format(b":0400000001020304F2\n:00000001FF\n"),
        FirmwareFormat::IntelHex
    );
}

#[test]
fn detect_format_recognizes_srec() {
    assert_eq!(
        detect_format(b"S00600004844521B\nS9030000FC\n"),
        FirmwareFormat::SRecord
    );
}

#[test]
fn detect_format_treats_binary_as_raw() {
    // Non-text bytes must not be misread as HEX/SREC.
    assert_eq!(
        detect_format(&[0x00, 0xFF, 0x12, 0x34]),
        FirmwareFormat::RawBinary
    );
    // Printable but not a HEX/SREC line.
    assert_eq!(detect_format(b"hello world"), FirmwareFormat::RawBinary);
    // Empty.
    assert_eq!(detect_format(b""), FirmwareFormat::RawBinary);
}

#[test]
fn detect_format_does_not_silently_fallback_on_malformed_text() {
    // Starts with ':' but the body is not valid HEX -> must NOT be RawBinary,
    // so ingest_firmware reports an error instead of comparing ASCII bytes.
    assert_eq!(detect_format(b":ZZZZ\n"), FirmwareFormat::IntelHex);
    // Starts with 'S' but is malformed.
    assert_eq!(detect_format(b"S\n"), FirmwareFormat::SRecord);
}

// ===========================================================================
// Intel HEX — parity with source
// ===========================================================================

#[test]
fn parity_hex_valid_single_record() {
    let text = format!(
        "{}:00000001FF\n",
        hex_line(&[0x04, 0x00, 0x00, 0x00, 0x01, 0x02, 0x03, 0x04])
    );
    let r = ingest_hex_text(&text, 0xFF).expect("parse failed");
    assert_eq!(r.base_address, Some(0x0));
    assert_eq!(hex(&r.bytes), "01020304");
    assert_eq!(r.format, FirmwareFormat::IntelHex);
    assert!(r.entry_point.is_none());
}

#[test]
fn parity_hex_extended_linear_address() {
    let text = format!(
        "{}{}:00000001FF\n",
        hex_line(&[0x02, 0x00, 0x00, 0x04, 0x00, 0x01]),
        hex_line(&[0x04, 0x00, 0x00, 0x00, 0xAA, 0xBB, 0xCC, 0xDD])
    );
    let r = ingest_hex_text(&text, 0xFF).expect("parse failed");
    assert_eq!(r.base_address, Some(0x10000));
    assert_eq!(hex(&r.bytes), "aabbccdd");
}

#[test]
fn parity_hex_extended_segment_address() {
    let text = format!(
        "{}{}:00000001FF\n",
        hex_line(&[0x02, 0x00, 0x00, 0x02, 0x10, 0x00]),
        hex_line(&[0x02, 0x00, 0x10, 0x00, 0x11, 0x22])
    );
    let r = ingest_hex_text(&text, 0xFF).expect("parse failed");
    // segment 0x1000 << 4 = 0x10000, plus offset 0x10 -> 0x10010
    assert_eq!(r.base_address, Some(0x10010));
    assert_eq!(hex(&r.bytes), "1122");
}

#[test]
fn parity_hex_start_linear_address_entry_point() {
    let text = format!(
        "{}{}:00000001FF\n",
        hex_line(&[0x04, 0x00, 0x00, 0x05, 0x12, 0x34, 0x56, 0x78]),
        hex_line(&[0x02, 0x00, 0x00, 0x00, 0x99, 0x88])
    );
    let r = ingest_hex_text(&text, 0xFF).expect("parse failed");
    assert_eq!(r.entry_point, Some(0x1234_5678));
}

#[test]
fn parity_hex_gap_padding() {
    let text = format!(
        "{}{}:00000001FF\n",
        hex_line(&[0x02, 0x00, 0x00, 0x00, 0xAA, 0xBB]),
        hex_line(&[0x02, 0x00, 0x10, 0x00, 0xCC, 0xDD])
    );
    let r = ingest_hex_text(&text, 0xFF).expect("parse failed");
    // Oracle: base=0x0 len=18 first=aa mid=ff last=dd
    assert_eq!(r.base_address, Some(0x0));
    assert_eq!(r.bytes.len(), 18);
    assert_eq!(r.bytes[0], 0xAA);
    assert_eq!(r.bytes[9], 0xFF, "gap must be padded");
    assert_eq!(r.bytes[17], 0xDD);
}

#[test]
fn parity_hex_empty_and_eof_only() {
    let empty = ingest_hex_text("", 0xFF).expect("empty ok");
    assert_eq!(empty.base_address, None);
    assert!(empty.bytes.is_empty());

    let eof = ingest_hex_text(":00000001FF\n", 0xFF).expect("eof ok");
    assert_eq!(eof.base_address, None);
    assert!(eof.bytes.is_empty());
}

#[test]
fn parity_hex_error_messages_match_source() {
    assert_eq!(
        ingest_hex_text("0400000001020304F2\n", 0xFF)
            .unwrap_err()
            .to_string(),
        "Line 1: missing leading colon ':'"
    );
    assert_eq!(
        ingest_hex_text(":040000000102030\n", 0xFF)
            .unwrap_err()
            .to_string(),
        "Line 1: invalid line length"
    );
    assert_eq!(
        ingest_hex_text(":0400000001020304FF\n", 0xFF)
            .unwrap_err()
            .to_string(),
        "Line 1: checksum mismatch (sum = 0x0D)"
    );
    assert_eq!(
        ingest_hex_text(":0400000001020304ZZ\n", 0xFF)
            .unwrap_err()
            .to_string(),
        "Line 1: invalid hex digit: invalid digit found in string"
    );
}

#[test]
fn parity_hex_unknown_record_type_is_ignored() {
    // Source ignores unrecognised record types (0x06).
    let text = hex_line(&[0x02, 0x00, 0x00, 0x06, 0x00, 0x00]);
    let r = ingest_hex_text(&text, 0xFF).expect("unknown type must not fail");
    assert_eq!(r.base_address, None);
    assert!(r.bytes.is_empty());
}

// ===========================================================================
// Intel HEX — INTENTIONAL DEVIATION (source panics)
// ===========================================================================

#[test]
fn deviation_hex_oversize_byte_count_errors_instead_of_panicking() {
    // Source: `bytes[4..4 + byte_count]` with byte_count=0x10 but only 4 data
    // bytes -> PANIC (verified by oracle).
    // Target: validated first -> typed error.
    let err = ingest_hex_text(":1000000001020304E6\n", 0xFF)
        .expect_err("oversize byte count must be rejected");
    assert!(
        matches!(err, HexIngestError::RecordTooShort { .. }),
        "expected RecordTooShort, got {:?}",
        err
    );
    // The message must be actionable and mention the line.
    assert!(err.to_string().contains("Line 1"), "message: {}", err);
}

// ===========================================================================
// S-Record — parity with source
// ===========================================================================

#[test]
fn parity_srec_s1_valid() {
    let text = srec_line('1', &[0x07, 0x00, 0x10, 0xDE, 0xAD, 0xBE, 0xEF]);
    let r = ingest_srec_text(&text, 0xFF).expect("parse failed");
    assert_eq!(r.base_address, Some(0x10));
    assert_eq!(hex(&r.bytes), "deadbeef");
    assert_eq!(r.format, FirmwareFormat::SRecord);
}

#[test]
fn parity_srec_s2_and_s3_address_widths() {
    let s2 = srec_line('2', &[0x08, 0x01, 0x00, 0x00, 0x11, 0x22, 0x33, 0x44]);
    let r2 = ingest_srec_text(&s2, 0xFF).expect("s2 failed");
    assert_eq!(r2.base_address, Some(0x10000));
    assert_eq!(hex(&r2.bytes), "11223344");

    let s3 = srec_line('3', &[0x09, 0x00, 0x01, 0x00, 0x00, 0xAB, 0xCD, 0xEF, 0x01]);
    let r3 = ingest_srec_text(&s3, 0xFF).expect("s3 failed");
    assert_eq!(r3.base_address, Some(0x10000));
    assert_eq!(hex(&r3.bytes), "abcdef01");
}

#[test]
fn parity_srec_s0_header_comment() {
    let text = srec_line('0', &[0x06, 0x00, 0x00, b'H', b'D', b'R']);
    let r = ingest_srec_text(&text, 0xFF).expect("s0 failed");
    // Header carries no data; parser records no bytes.
    assert!(r.bytes.is_empty());
}

#[test]
fn parity_srec_empty_and_termination_only() {
    let empty = ingest_srec_text("", 0xFF).expect("empty ok");
    assert_eq!(empty.base_address, None);
    assert!(empty.bytes.is_empty());

    let term = ingest_srec_text("S9030000FC\n", 0xFF).expect("term ok");
    assert_eq!(term.base_address, None);
    assert!(term.bytes.is_empty());
}

#[test]
fn parity_srec_error_messages_match_source() {
    assert_eq!(
        ingest_srec_text("9030000FC\n", 0xFF)
            .unwrap_err()
            .to_string(),
        "Line 1: missing leading 'S'"
    );
    assert_eq!(
        ingest_srec_text("S9\n", 0xFF).unwrap_err().to_string(),
        "Line 1: invalid payload length"
    );
    assert_eq!(
        ingest_srec_text("S1050010DEADBEEE\n", 0xFF)
            .unwrap_err()
            .to_string(),
        "Line 1: checksum mismatch (sum = 0x4C, expected 0xFF)"
    );
    assert_eq!(
        ingest_srec_text("S1050010ZZ\n", 0xFF)
            .unwrap_err()
            .to_string(),
        "Line 1: invalid hex: invalid digit found in string"
    );
}

#[test]
fn parity_srec_count_mismatch() {
    let text = srec_line('1', &[0x09, 0x00, 0x10, 0xDE, 0xAD]);
    assert_eq!(
        ingest_srec_text(&text, 0xFF).unwrap_err().to_string(),
        "Line 1: byte count field mismatch"
    );
}

#[test]
fn parity_srec_address_arithmetic_at_ffff() {
    // Oracle: base=0xFFFF len=4 (wraps past 0xFFFF silently in source).
    let text = srec_line('1', &[0x07, 0xFF, 0xFF, 0x11, 0x22, 0x33, 0x44]);
    let r = ingest_srec_text(&text, 0xFF).expect("parse failed");
    assert_eq!(r.base_address, Some(0xFFFF));
    assert_eq!(r.bytes.len(), 4);
}

// ===========================================================================
// S-Record — INTENTIONAL DEVIATION (source panics)
// ===========================================================================

#[test]
fn deviation_srec_bare_s_errors_instead_of_panicking() {
    // Source: `line.chars().nth(1).unwrap()` -> PANIC (verified by oracle).
    let err = ingest_srec_text("S\n", 0xFF).expect_err("bare S must be rejected");
    assert!(
        matches!(err, HexIngestError::MalformedLine { .. }),
        "expected MalformedLine, got {:?}",
        err
    );
}

// ===========================================================================
// ingest_firmware — dispatch
// ===========================================================================

#[test]
fn ingest_firmware_parses_hex() {
    let text = format!(
        "{}:00000001FF\n",
        hex_line(&[0x02, 0x00, 0x00, 0x00, 0xAB, 0xCD])
    );
    let r = ingest_firmware(text.as_bytes(), 0xFF).expect("ingest failed");
    assert_eq!(r.format, FirmwareFormat::IntelHex);
    assert_eq!(hex(&r.bytes), "abcd");
}

#[test]
fn ingest_firmware_parses_srec() {
    let text = srec_line('1', &[0x05, 0x00, 0x00, 0xAB, 0xCD]);
    let r = ingest_firmware(text.as_bytes(), 0xFF).expect("ingest failed");
    assert_eq!(r.format, FirmwareFormat::SRecord);
    assert_eq!(hex(&r.bytes), "abcd");
}

#[test]
fn ingest_firmware_passes_binary_through_unchanged() {
    let bin = vec![0x00, 0xFF, 0x12, 0x34, 0x56];
    let r = ingest_firmware(&bin, 0xFF).expect("ingest failed");
    assert_eq!(r.format, FirmwareFormat::RawBinary);
    assert_eq!(r.bytes, bin, "raw binary must be byte-for-byte identical");
    assert_eq!(r.base_address, None);
    assert!(r.entry_point.is_none());
}

#[test]
fn ingest_firmware_rejects_malformed_hex_rather_than_treating_as_binary() {
    // Starts with ':' -> detected as IntelHex -> parse fails -> error.
    let err = ingest_firmware(b":ZZZZ\n", 0xFF).expect_err("malformed hex must error");
    assert!(matches!(
        err,
        HexIngestError::Hex(_) | HexIngestError::MalformedLine { .. }
    ));
}

#[test]
fn ingest_firmware_rejects_malformed_srec() {
    let err = ingest_firmware(b"S\n", 0xFF).expect_err("malformed srec must error");
    assert!(matches!(
        err,
        HexIngestError::Srec(_) | HexIngestError::MalformedLine { .. }
    ));
}

// ===========================================================================
// Safety limits
// ===========================================================================

#[test]
fn oversized_address_range_is_rejected_before_allocation() {
    // Two data records far apart: low at 0x0000, high at 0x0FFF_0000.
    // Span is ~256 MB, above the 64 MB cap. Must be rejected, not allocated.
    let text = format!(
        "{}{}{}:00000001FF\n",
        hex_line(&[0x01, 0x00, 0x00, 0x00, 0x01]),
        hex_line(&[0x02, 0x00, 0x00, 0x04, 0x0F, 0xFF]),
        hex_line(&[0x01, 0x00, 0x00, 0x00, 0x02])
    );
    let err = ingest_hex_text(&text, 0xFF).expect_err("oversized span must be rejected");
    assert!(
        matches!(err, HexIngestError::AddressRangeTooLarge { .. }),
        "expected AddressRangeTooLarge, got {:?}",
        err
    );
}

#[test]
fn size_limit_matches_ingest_binary_limit() {
    use kivi_core::hex_ingest::MAX_FIRMWARE_SPAN;
    use kivi_core::ingest::MAX_BINARY_SIZE;
    assert_eq!(
        MAX_FIRMWARE_SPAN, MAX_BINARY_SIZE,
        "limit must reuse the ROM size cap"
    );
}

// ===========================================================================
// Integration with ingest_binary
// ===========================================================================

#[test]
fn parsed_output_feeds_ingest_binary() {
    use kivi_core::ingest::ingest_binary;
    let text = format!(
        "{}:00000001FF\n",
        hex_line(&[0x04, 0x00, 0x00, 0x00, 0x11, 0x22, 0x33, 0x44])
    );
    let r = ingest_firmware(text.as_bytes(), 0xFF).expect("ingest failed");
    let mem = ingest_binary(&r.bytes).expect("ingest_binary rejected parsed output");
    assert_eq!(mem.len(), 4);
    assert_eq!(mem.as_slice(), &[0x11, 0x22, 0x33, 0x44]);
}

// ===========================================================================
// Determinism
// ===========================================================================

#[test]
fn parsing_is_deterministic() {
    let text = format!(
        "{}{}:00000001FF\n",
        hex_line(&[0x02, 0x00, 0x00, 0x00, 0xAA, 0xBB]),
        hex_line(&[0x02, 0x00, 0x10, 0x00, 0xCC, 0xDD])
    );
    let a = ingest_hex_text(&text, 0xFF).expect("a");
    let b = ingest_hex_text(&text, 0xFF).expect("b");
    assert_eq!(a, b);
}
