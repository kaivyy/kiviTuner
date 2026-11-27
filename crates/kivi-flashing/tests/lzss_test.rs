//! Parity tests for the LZSS container codec.
//!
//! Ported from `ecu_engine_rust::flashing::lzss_container`.
//!
//! ## How expected values were obtained
//!
//! LZSS has no fixed output for a given input beyond its own algorithm, so the
//! assertions here are **invariants** that any correct LZSS implementation must
//! satisfy, plus hand-checkable structural facts:
//!
//! * decompress(compress(x), len(x)) == x for every input,
//! * compressed output is never larger than the input plus the flag overhead for
//!   incompressible data (each 8 tokens cost 1 flag byte),
//! * a highly repetitive input compresses smaller than the original,
//! * empty input round-trips to empty.
//!
//! These are stated as invariants rather than magic byte vectors because a
//! specific byte vector would only prove the encoder matches itself.

use kivi_flashing::lzss_container::SimosLzssEngine;

// ---------------------------------------------------------------------------
// Round trip
// ---------------------------------------------------------------------------

#[test]
fn empty_input_round_trips() {
    let c = SimosLzssEngine::compress(&[]);
    assert!(c.is_empty(), "empty input must produce empty output");
    let d = SimosLzssEngine::decompress(&c, 0).expect("decompress");
    assert!(d.is_empty());
}

#[test]
fn a_single_byte_round_trips() {
    let data = vec![0x42u8];
    let c = SimosLzssEngine::compress(&data);
    let d = SimosLzssEngine::decompress(&c, data.len()).expect("decompress");
    assert_eq!(d, data);
}

#[test]
fn a_short_literal_run_round_trips() {
    let data = b"ABCD".to_vec();
    let c = SimosLzssEngine::compress(&data);
    let d = SimosLzssEngine::decompress(&c, data.len()).expect("decompress");
    assert_eq!(d, data);
}

#[test]
fn a_long_repetitive_input_round_trips() {
    let data = vec![0xAAu8; 4096];
    let c = SimosLzssEngine::compress(&data);
    let d = SimosLzssEngine::decompress(&c, data.len()).expect("decompress");
    assert_eq!(d, data);
}

#[test]
fn a_varied_input_round_trips() {
    let data: Vec<u8> = (0..2048).map(|i| (i % 251) as u8).collect();
    let c = SimosLzssEngine::compress(&data);
    let d = SimosLzssEngine::decompress(&c, data.len()).expect("decompress");
    assert_eq!(d, data);
}

#[test]
fn a_patterned_input_round_trips() {
    // Repeating 8-byte pattern exercises the back-reference path.
    let pattern = b"ECU-CAL!";
    let mut data = Vec::new();
    for _ in 0..256 {
        data.extend_from_slice(pattern);
    }
    let c = SimosLzssEngine::compress(&data);
    let d = SimosLzssEngine::decompress(&c, data.len()).expect("decompress");
    assert_eq!(d, data);
}

#[test]
fn binary_rom_like_data_round_trips() {
    // A pseudo-random but deterministic buffer, like a real calibration image.
    let mut state: u32 = 0x1234_5678;
    let data: Vec<u8> = (0..8192)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 24) as u8
        })
        .collect();
    let c = SimosLzssEngine::compress(&data);
    let d = SimosLzssEngine::decompress(&c, data.len()).expect("decompress");
    assert_eq!(d, data);
}

// ---------------------------------------------------------------------------
// Compression effectiveness (invariants)
// ---------------------------------------------------------------------------

#[test]
fn a_highly_repetitive_input_compresses_smaller() {
    let data = vec![0x00u8; 8192];
    let c = SimosLzssEngine::compress(&data);
    assert!(
        c.len() < data.len() / 4,
        "8 KiB of zeros should compress far below {} bytes, got {}",
        data.len() / 4,
        c.len()
    );
}

#[test]
fn incompressible_data_does_not_explode() {
    // Pseudo-random bytes cannot be back-referenced, so the worst case is the
    // input plus one flag byte per 8 tokens.
    let mut state: u32 = 0xDEAD_BEEF;
    let data: Vec<u8> = (0..4096)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 24) as u8
        })
        .collect();
    let c = SimosLzssEngine::compress(&data);
    let overhead = data.len().div_ceil(8);
    assert!(
        c.len() <= data.len() + overhead,
        "compressed {} exceeds input {} + overhead {}",
        c.len(),
        data.len(),
        overhead
    );
}

#[test]
fn compression_is_deterministic() {
    let data: Vec<u8> = (0..1024).map(|i| (i % 37) as u8).collect();
    assert_eq!(
        SimosLzssEngine::compress(&data),
        SimosLzssEngine::compress(&data),
        "the same input must always produce the same output"
    );
}

// ---------------------------------------------------------------------------
// Malformed input
// ---------------------------------------------------------------------------

#[test]
fn decompressing_empty_input_with_a_zero_length_is_allowed() {
    let d = SimosLzssEngine::decompress(&[], 0).expect("zero length is valid");
    assert!(d.is_empty());
}

#[test]
fn a_truncated_back_reference_is_rejected() {
    // A flag byte with bit 0 set, followed by only one byte: the 2-byte
    // back-reference is incomplete.
    let stream = vec![0x01u8, 0x00];
    let err = SimosLzssEngine::decompress(&stream, 8).expect_err("must reject");
    assert!(err.contains("Truncated"), "error: {err}");
}

#[test]
fn decompression_stops_at_the_expected_length() {
    // Compress a known buffer, then ask for fewer bytes: the result must be
    // truncated to exactly that length rather than overrunning.
    let data = vec![0x55u8; 64];
    let c = SimosLzssEngine::compress(&data);
    let d = SimosLzssEngine::decompress(&c, 10).expect("decompress");
    assert_eq!(d.len(), 10, "output must not exceed the expected length");
    assert!(d.iter().all(|&b| b == 0x55));
}

#[test]
fn an_incorrect_expected_length_is_rejected() {
    // The decoder validates the final size against `expected_len` and returns a
    // typed error rather than silently returning a short or long buffer. That is
    // stricter than a permissive decoder, and it is the documented contract.
    let data = b"SHORT".to_vec();
    let c = SimosLzssEngine::compress(&data);

    let err = SimosLzssEngine::decompress(&c, 1000).expect_err("must reject");
    assert!(
        err.contains("does not match"),
        "error must name the size mismatch: {err}"
    );
}

#[test]
fn the_exact_expected_length_is_accepted() {
    let data = b"SHORT".to_vec();
    let c = SimosLzssEngine::compress(&data);
    let d = SimosLzssEngine::decompress(&c, data.len()).expect("exact length is valid");
    assert_eq!(d, data);
}

#[test]
fn decompression_does_not_panic_on_arbitrary_bytes() {
    for len in [0usize, 1, 2, 3, 8, 64] {
        let stream: Vec<u8> = (0..len).map(|i| (i * 37 % 256) as u8).collect();
        // Any outcome is acceptable as long as it does not panic.
        let _ = SimosLzssEngine::decompress(&stream, 128);
    }
}
