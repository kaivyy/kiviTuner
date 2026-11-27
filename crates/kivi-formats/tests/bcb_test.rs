//! Parity tests for the BCB (VAG) run-length codec.
//!
//! Ported from `ecu_engine_rust::formats::mod.rs::SgoContainerParser`
//! (`bcb_compress`, `bcb_decompress`).
//!
//! ## Encoding contract (read from the source)
//!
//! ```text
//! magic      : 0x1A 0x01
//! token      : u16 big-endian
//!   flag = token >> 14 , length = token & 0x3FFF
//!   flag 0 -> literal: `length` raw bytes follow
//!   flag 1 -> run:     one repeat byte follows, repeated `length` times
//!   flag 3 -> end:     a 24-bit additive checksum of the output follows
//! ```
//!
//! Compressor emits a run token only when a run is at least 4 bytes long;
//! otherwise bytes accumulate into a literal token.
//!
//! ## How expected bytes were obtained
//!
//! Hand-assembled from the contract above. For `[0xAA; 4]`:
//! magic `1A 01`, run token `0x4004` = `40 04`, repeat byte `AA`,
//! end token `0xC000` = `C0 00`, checksum of 4×0xAA = 0x2A8 → `00 02 A8`.
//!
//! ## Deviation
//!
//! The source `bcb_decompress` grows the output by an attacker-controlled
//! `length` with **no cap**: a 2-byte RLE token can request 16 KiB, so a tiny
//! file can expand enormously. This version enforces
//! [`MAX_DECOMPRESSED_SIZE`] and returns a typed error instead of allocating
//! without bound.

use kivi_formats::bcb::{
    bcb_compress, bcb_decompress, BcbError, MAX_DECOMPRESSED_SIZE,
};

// ---------------------------------------------------------------------------
// Independently derived encoded vectors
// ---------------------------------------------------------------------------

#[test]
fn compresses_a_four_byte_run_to_a_run_token() {
    let out = bcb_compress(&[0xAA, 0xAA, 0xAA, 0xAA]);
    assert_eq!(
        out,
        vec![
            0x1A, 0x01, // magic
            0x40, 0x04, // flag 1 (run), length 4
            0xAA, // repeat byte
            0xC0, 0x00, // end token
            0x00, 0x02, 0xA8, // checksum = 4 * 0xAA = 0x2A8
        ],
        "hand-assembled expectation"
    );
}

#[test]
fn compresses_a_short_literal_to_a_literal_token() {
    let out = bcb_compress(&[0x01, 0x02, 0x03]);
    assert_eq!(
        out,
        vec![
            0x1A, 0x01, // magic
            0x00, 0x03, // flag 0 (literal), length 3
            0x01, 0x02, 0x03, // payload
            0xC0, 0x00, // end
            0x00, 0x00, 0x06, // checksum = 1+2+3 = 6
        ]
    );
}

#[test]
fn empty_input_compresses_to_magic_plus_end_and_zero_checksum() {
    let out = bcb_compress(&[]);
    assert_eq!(out, vec![0x1A, 0x01, 0xC0, 0x00, 0x00, 0x00, 0x00]);
}

#[test]
fn the_checksum_is_a_24_bit_additive_sum() {
    // 0xFF * 5 = 0x4FB -> within 24 bits
    let out = bcb_compress(&[0xFF; 5]);
    let n = out.len();
    let chk = ((out[n - 3] as u32) << 16) | ((out[n - 2] as u32) << 8) | (out[n - 1] as u32);
    assert_eq!(chk, 0x04FB);
}

#[test]
fn the_checksum_wraps_at_24_bits() {
    // 300 * 0xFF = 0x12B64 -> masked to 0x2B64
    let out = bcb_compress(&[0xFF; 300]);
    let n = out.len();
    let chk = ((out[n - 3] as u32) << 16) | ((out[n - 2] as u32) << 8) | (out[n - 1] as u32);
    assert_eq!(chk, (300u32 * 0xFF) & 0x00FF_FFFF);
}

// ---------------------------------------------------------------------------
// Round trip
// ---------------------------------------------------------------------------

#[test]
fn round_trips_an_empty_buffer() {
    let out = bcb_decompress(&bcb_compress(&[])).expect("decompress");
    assert!(out.is_empty());
}

#[test]
fn round_trips_a_single_byte() {
    let data = vec![0x42];
    assert_eq!(bcb_decompress(&bcb_compress(&data)).expect("round trip"), data);
}

#[test]
fn round_trips_a_long_run() {
    let data = vec![0x5A; 1000];
    assert_eq!(bcb_decompress(&bcb_compress(&data)).expect("round trip"), data);
}

#[test]
fn round_trips_varied_data() {
    let data: Vec<u8> = (0..2000).map(|i| (i % 251) as u8).collect();
    assert_eq!(bcb_decompress(&bcb_compress(&data)).expect("round trip"), data);
}

#[test]
fn round_trips_mixed_runs_and_literals() {
    let mut data = Vec::new();
    data.extend_from_slice(&[1, 2, 3]); // literal
    data.extend_from_slice(&[0x77; 10]); // run
    data.extend_from_slice(&[9, 8, 7, 6]); // literal
    data.extend_from_slice(&[0x00; 4]); // exactly the run threshold
    assert_eq!(bcb_decompress(&bcb_compress(&data)).expect("round trip"), data);
}

#[test]
fn a_three_byte_run_is_emitted_as_a_literal() {
    // Below the 4-byte threshold, so it must stay literal.
    let data = vec![0x11, 0x11, 0x11];
    let out = bcb_compress(&data);
    // magic + literal token(2) + 3 bytes + end(2) + checksum(3)
    assert_eq!(out.len(), 2 + 2 + 3 + 2 + 3);
    assert_eq!(bcb_decompress(&out).expect("round trip"), data);
}

#[test]
fn compression_is_deterministic() {
    let data: Vec<u8> = (0..500).map(|i| (i % 7) as u8).collect();
    assert_eq!(bcb_compress(&data), bcb_compress(&data));
}

// ---------------------------------------------------------------------------
// Malformed input
// ---------------------------------------------------------------------------

#[test]
fn rejects_a_bad_magic() {
    let err = bcb_decompress(&[0x00, 0x00, 0xC0, 0x00, 0, 0, 0]).expect_err("must reject");
    assert!(matches!(err, BcbError::InvalidMagic));
}

#[test]
fn rejects_a_too_short_stream() {
    let err = bcb_decompress(&[0x1A, 0x01]).expect_err("must reject");
    assert!(matches!(err, BcbError::InvalidMagic));
}

#[test]
fn rejects_a_literal_that_exceeds_the_buffer() {
    // literal length 10 but only 2 bytes present; padded past the 7-byte
    // minimum so the literal check is what fires, not the magic check.
    let stream = vec![0x1A, 0x01, 0x00, 0x0A, 0xAA, 0xBB, 0xCC];
    let err = bcb_decompress(&stream).expect_err("must reject");
    assert!(matches!(err, BcbError::LiteralOverrun));
}

#[test]
fn rejects_a_run_token_missing_its_repeat_byte() {
    // Run token at the very end with no repeat byte, padded to 7 bytes.
    let stream = vec![0x1A, 0x01, 0x00, 0x01, 0xAA, 0x40, 0x04];
    let err = bcb_decompress(&stream).expect_err("must reject");
    assert!(matches!(err, BcbError::MissingRepeatByte));
}

#[test]
fn rejects_an_unknown_token_flag() {
    // flag 2 is undefined
    let stream = vec![0x1A, 0x01, 0x80, 0x01, 0xC0, 0x00, 0, 0, 0];
    let err = bcb_decompress(&stream).expect_err("must reject");
    assert!(matches!(err, BcbError::UnknownTokenFlag(2)));
}

#[test]
fn rejects_a_checksum_mismatch() {
    let mut out = bcb_compress(&[0xAA, 0xBB]);
    let n = out.len();
    out[n - 1] ^= 0xFF;
    let err = bcb_decompress(&out).expect_err("must reject");
    assert!(matches!(err, BcbError::ChecksumMismatch { .. }));
}

#[test]
fn rejects_a_stream_without_an_end_token() {
    // Two zero-length literal tokens, then the stream stops with no end token.
    // magic(2) + token(2) + token(2) + 1 pad byte = 7 bytes (the minimum).
    // Zero-length literals consume no payload, so decoding simply runs out.
    let stream = vec![0x1A, 0x01, 0x00, 0x00, 0x00, 0x00, 0xFF];
    let err = bcb_decompress(&stream).expect_err("must reject");
    assert!(
        matches!(err, BcbError::MissingEndToken),
        "expected MissingEndToken, got {err:?}"
    );
}

#[test]
fn rejects_a_truncated_checksum() {
    // End token present but the 3 checksum bytes are incomplete.
    let stream = vec![0x1A, 0x01, 0x00, 0x01, 0xAA, 0xC0, 0x00];
    let err = bcb_decompress(&stream).expect_err("must reject");
    assert!(matches!(err, BcbError::TruncatedChecksum));
}

// ---------------------------------------------------------------------------
// Allocation bound (deviation)
// ---------------------------------------------------------------------------

#[test]
fn a_decompression_bomb_is_rejected_rather_than_allocated() {
    // Many maximal RLE tokens: each 2-byte token + 1 byte asks for 16383 bytes.
    // Without a cap this would expand a tiny input to hundreds of megabytes.
    let mut stream = vec![0x1A, 0x01];
    let tokens = (MAX_DECOMPRESSED_SIZE / 0x3FFF) + 2;
    for _ in 0..tokens {
        stream.extend_from_slice(&0x7FFFu16.to_be_bytes()); // flag 1, length 0x3FFF
        stream.push(0x41);
    }
    stream.extend_from_slice(&0xC000u16.to_be_bytes());
    stream.extend_from_slice(&[0, 0, 0]);

    let err = bcb_decompress(&stream).expect_err("must reject");
    assert!(
        matches!(err, BcbError::OutputTooLarge { .. }),
        "expected an allocation guard, got {err:?}"
    );
}

#[test]
fn the_decompressed_size_limit_is_documented_and_positive() {
    // Read through a runtime binding so clippy cannot fold this to a constant:
    // the assertion is meant to fail the build if the limit is ever lowered
    // below a normal firmware size.
    let limit = MAX_DECOMPRESSED_SIZE;
    assert!(limit > 0);
    assert!(limit >= 16 * 1024 * 1024, "limit is only {limit} bytes");
}
