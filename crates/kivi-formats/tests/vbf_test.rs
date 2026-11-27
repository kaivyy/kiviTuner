//! Parity tests for the VBF container codec.
//!
//! Ported from `ecu_engine_rust::formats::mod.rs::VbfContainerCodec`
//! (`serialize_to_vbf`, `parse_vbf_segments`) and `FirmwareMemorySegment`.
//!
//! ## Format contract (read from the source, not inferred)
//!
//! ```text
//! vbf_version = 2.4;
//! header {
//!     sw_part_number = "<id>";
//!     num_segments = <n>;
//! }
//! <repeat per segment>
//!   start_address  u32 big-endian
//!   data_len       u32 big-endian
//!   data           <len> bytes
//!   crc16_ccitt    u16 big-endian   (CCITT-FALSE: init 0xFFFF, poly 0x1021)
//! ```
//!
//! ## How expected values were obtained
//!
//! The CRC values below were computed independently with a standalone
//! implementation of CCITT-FALSE (see the doc comment on each test) and the byte
//! layout is derived from the contract above. None was produced by running the
//! codec.

use kivi_formats::vbf::{
    parse_vbf_segments, serialize_to_vbf, FirmwareMemorySegment, VbfError,
};

/// Reference CRC-16/CCITT-FALSE, written independently of the crate under test.
fn reference_crc16_ccitt(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &b in data {
        crc ^= (b as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

fn seg(addr: u32, data: &[u8]) -> FirmwareMemorySegment {
    FirmwareMemorySegment {
        start_address: addr,
        data: data.to_vec(),
        crc16_ccitt: reference_crc16_ccitt(data),
    }
}

// ---------------------------------------------------------------------------
// Serialization: structure and independently derived bytes
// ---------------------------------------------------------------------------

#[test]
fn header_is_ascii_and_reports_the_segment_count() {
    let out = serialize_to_vbf("ABC123", &[seg(0x1000, &[0xAA])]);
    let text = String::from_utf8_lossy(&out);

    assert!(text.starts_with("vbf_version = 2.4;\nheader {\n"));
    assert!(text.contains("sw_part_number = \"ABC123\";"));
    assert!(text.contains("num_segments = 1;"));
    assert!(text.contains("}"), "header must close with a brace");
}

#[test]
fn an_empty_segment_list_serializes_a_zero_count_header() {
    let out = serialize_to_vbf("EMPTY", &[]);
    let text = String::from_utf8_lossy(&out);
    assert!(text.contains("num_segments = 0;"));
    // No binary payload follows the header.
    let header_end = out.iter().position(|&b| b == b'}').expect("brace") + 1;
    assert_eq!(out.len(), header_end, "no bytes after the header");
}

#[test]
fn segment_bytes_follow_the_documented_layout() {
    let data = [0xDE, 0xAD];
    let crc = reference_crc16_ccitt(&data);
    let out = serialize_to_vbf("P1", &[seg(0x0001_0000, &data)]);

    let header_end = out.iter().position(|&b| b == b'}').expect("brace") + 1;
    let body = &out[header_end..];

    // 4 addr + 4 len + 2 data + 2 crc = 12 bytes
    assert_eq!(body.len(), 12);

    assert_eq!(&body[0..4], &0x0001_0000u32.to_be_bytes(), "big-endian address");
    assert_eq!(&body[4..8], &2u32.to_be_bytes(), "big-endian length");
    assert_eq!(&body[8..10], &data, "payload");
    assert_eq!(&body[10..12], &crc.to_be_bytes(), "big-endian CRC16");
}

#[test]
fn multiple_segments_are_concatenated_in_order() {
    let s1 = seg(0x1000, &[0x01]);
    let s2 = seg(0x2000, &[0x02, 0x03]);
    let out = serialize_to_vbf("MULTI", &[s1, s2]);

    let text = String::from_utf8_lossy(&out);
    assert!(text.contains("num_segments = 2;"));

    let header_end = out.iter().position(|&b| b == b'}').expect("brace") + 1;
    let body = &out[header_end..];
    // 4+4+1+2 = 11, then 4+4+2+2 = 12 -> 23
    assert_eq!(body.len(), 23);

    assert_eq!(&body[0..4], &0x1000u32.to_be_bytes());
    assert_eq!(&body[4..8], &1u32.to_be_bytes());
    assert_eq!(body[8], 0x01);
}

#[test]
fn serialization_is_deterministic() {
    let segs = vec![seg(0x1000, &[1, 2, 3])];
    assert_eq!(
        serialize_to_vbf("X", &segs),
        serialize_to_vbf("X", &segs),
        "identical input must produce identical bytes"
    );
}

#[test]
fn a_zero_length_segment_is_permitted() {
    let out = serialize_to_vbf("Z", &[seg(0x1000, &[])]);
    let (_, parsed) = parse_vbf_segments(&out).expect("round trip");
    assert_eq!(parsed.len(), 1);
    assert!(parsed[0].data.is_empty());
}

#[test]
fn address_zero_is_preserved_as_a_real_address() {
    let out = serialize_to_vbf("ZERO", &[seg(0, &[0xAA])]);
    let (_, parsed) = parse_vbf_segments(&out).expect("round trip");
    assert_eq!(
        parsed[0].start_address, 0,
        "a known address of zero must not become unknown"
    );
}

// ---------------------------------------------------------------------------
// Round trip
// ---------------------------------------------------------------------------

#[test]
fn single_segment_round_trips() {
    let original = vec![seg(0x0008_0000, &[0x11, 0x22, 0x33])];
    let bytes = serialize_to_vbf("PART-1", &original);
    let (header, parsed) = parse_vbf_segments(&bytes).expect("parse");

    assert!(header.contains("PART-1"));
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].start_address, 0x0008_0000);
    assert_eq!(parsed[0].data, vec![0x11, 0x22, 0x33]);
    assert_eq!(parsed[0].crc16_ccitt, reference_crc16_ccitt(&[0x11, 0x22, 0x33]));
}

#[test]
fn multi_segment_round_trips_in_order() {
    let original = vec![
        seg(0x1000, &[1, 2]),
        seg(0x2000, &[3, 4, 5]),
        seg(0x3000, &[]),
    ];
    let bytes = serialize_to_vbf("MULTI", &original);
    let (_, parsed) = parse_vbf_segments(&bytes).expect("parse");

    assert_eq!(parsed.len(), 3);
    assert_eq!(parsed[0].start_address, 0x1000);
    assert_eq!(parsed[1].start_address, 0x2000);
    assert_eq!(parsed[2].start_address, 0x3000);
    assert_eq!(parsed[1].data, vec![3, 4, 5]);
}

#[test]
fn a_large_payload_round_trips() {
    let data: Vec<u8> = (0..4096).map(|i| (i % 251) as u8).collect();
    let bytes = serialize_to_vbf("BIG", &[seg(0x0001_0000, &data)]);
    let (_, parsed) = parse_vbf_segments(&bytes).expect("parse");
    assert_eq!(parsed[0].data, data);
}

// ---------------------------------------------------------------------------
// Malformed input
// ---------------------------------------------------------------------------

#[test]
fn empty_input_is_rejected() {
    let err = parse_vbf_segments(&[]).expect_err("must reject");
    assert_eq!(err, VbfError::MissingHeaderTerminator);
}

#[test]
fn input_without_a_header_brace_is_rejected() {
    let err = parse_vbf_segments(b"no braces here").expect_err("must reject");
    assert_eq!(err, VbfError::MissingHeaderTerminator);
}

#[test]
fn a_truncated_segment_header_is_rejected() {
    // Valid header, then only 4 bytes where 8 are required.
    let mut bytes = serialize_to_vbf("T", &[]);
    bytes.extend_from_slice(&[0x00, 0x00, 0x10]);
    let err = parse_vbf_segments(&bytes).expect_err("must reject");
    assert_eq!(err, VbfError::TruncatedSegmentHeader);
}

#[test]
fn a_truncated_payload_is_rejected() {
    let mut bytes = serialize_to_vbf("T", &[]);
    bytes.extend_from_slice(&0x1000u32.to_be_bytes());
    bytes.extend_from_slice(&100u32.to_be_bytes()); // claims 100 bytes
    bytes.extend_from_slice(&[0xAA, 0xBB]); // supplies 2
    let err = parse_vbf_segments(&bytes).expect_err("must reject");
    assert_eq!(err, VbfError::TruncatedPayload);
}

#[test]
fn a_crc_mismatch_is_rejected() {
    let data = [0x11, 0x22];
    let mut bytes = serialize_to_vbf("T", &[seg(0x1000, &data)]);
    // Corrupt the final CRC byte.
    let last = bytes.len() - 1;
    bytes[last] ^= 0xFF;

    let err = parse_vbf_segments(&bytes).expect_err("must reject");
    assert_eq!(err, VbfError::CrcMismatch);
}

#[test]
fn an_absurd_length_does_not_overflow_or_allocate() {
    let mut bytes = serialize_to_vbf("T", &[]);
    bytes.extend_from_slice(&0u32.to_be_bytes());
    bytes.extend_from_slice(&u32::MAX.to_be_bytes()); // length 0xFFFFFFFF
    bytes.extend_from_slice(&[0u8; 8]);

    // Must be a clean error, not a panic or a huge allocation.
    let err = parse_vbf_segments(&bytes).expect_err("must reject");
    assert_eq!(err, VbfError::TruncatedPayload);
}

#[test]
fn trailing_partial_bytes_after_a_valid_segment_are_rejected() {
    let mut bytes = serialize_to_vbf("T", &[seg(0x1000, &[0xAA])]);
    bytes.push(0x00); // stray byte
    let err = parse_vbf_segments(&bytes).expect_err("must reject");
    assert_eq!(err, VbfError::TruncatedSegmentHeader);
}

#[test]
fn parsing_does_not_mutate_its_input() {
    let bytes = serialize_to_vbf("T", &[seg(0x1000, &[1, 2, 3])]);
    let copy = bytes.clone();
    let _ = parse_vbf_segments(&bytes);
    assert_eq!(bytes, copy);
}
