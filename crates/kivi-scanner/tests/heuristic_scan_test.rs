//! Parity tests for the 16-bit heuristic ROM scanner.
//!
//! Ported from `ecu_engine_rust::recognition::heuristic::HeuristicMapDetector::scan_rom_16bit`
//! and its `scan_rom_16bit_le` / `scan_rom_16bit_be` wrappers.
//!
//! ## How expected values were obtained
//!
//! The scan gates are read from the source and asserted directly:
//! * entropy must fall in `[1.5, 6.8]`,
//! * `smoothness < 0.38`,
//! * `autocorrelation(flat, w) > 0.40`.
//!
//! Synthetic tables below are constructed to satisfy or violate exactly one gate
//! at a time, so each expectation follows from the rule rather than from running
//! the implementation.
//!
//! ## Documented deviation
//!
//! The source sorts with `partial_cmp(..).unwrap()`, which **panics** if any score
//! is NaN. This module uses a total order. Refusing to crash on degenerate input
//! is not a semantic change to the output for valid input.

use kivi_scanner::heuristic_scan::{scan_rom_16bit, scan_rom_16bit_be, scan_rom_16bit_le};

/// Builds a byte buffer from a 16-bit matrix in the requested byte order.
fn encode(matrix: &[Vec<u16>], little_endian: bool) -> Vec<u8> {
    let mut out = Vec::new();
    for row in matrix {
        for &v in row {
            if little_endian {
                out.extend_from_slice(&v.to_le_bytes());
            } else {
                out.extend_from_slice(&v.to_be_bytes());
            }
        }
    }
    out
}

/// A smooth, gently ramping table: neighbouring cells differ by 1, so smoothness
/// is low and autocorrelation at the row width is high.
fn ramp_table(rows: usize, cols: usize) -> Vec<Vec<u16>> {
    let mut base = 1000u16;
    let mut m = Vec::with_capacity(rows);
    for _ in 0..rows {
        let mut row = Vec::with_capacity(cols);
        for _ in 0..cols {
            row.push(base);
            base += 1;
        }
        m.push(row);
    }
    m
}

// ---------------------------------------------------------------------------
// Basic detection
// ---------------------------------------------------------------------------

#[test]
fn detects_a_smooth_little_endian_table() {
    // 16x16 ramp: entropy 5.19, smoothness 0.23, autocorr 0.81 — all gates pass.
    // (An 8x8 ramp is too small: smoothness 0.49 exceeds the 0.38 gate.)
    let table = ramp_table(16, 16);
    let rom = encode(&table, true);

    let found = scan_rom_16bit_le(&rom, &[16], 16, 16);
    assert!(!found.is_empty(), "a smooth 16x16 ramp must be detected");

    let first = &found[0];
    assert_eq!(first.cols, 16);
    assert_eq!(first.rows, 16);
    assert!(first.is_little_endian);
    assert_eq!(first.offset, 0);
}

#[test]
fn detects_a_smooth_big_endian_table() {
    let table = ramp_table(16, 16);
    let rom = encode(&table, false);

    let found = scan_rom_16bit_be(&rom, &[16], 16, 16);
    assert!(!found.is_empty(), "BE ramp must be detected");
    assert!(!found[0].is_little_endian);
}

#[test]
fn the_same_table_in_each_byte_order_is_reported_with_that_order() {
    let table = ramp_table(16, 16);
    let le = encode(&table, true);
    let be = encode(&table, false);

    let le_found = scan_rom_16bit_le(&le, &[16], 16, 16);
    let be_found = scan_rom_16bit_be(&be, &[16], 16, 16);

    assert!(!le_found.is_empty());
    assert!(!be_found.is_empty());
    assert!(le_found[0].is_little_endian);
    assert!(!be_found[0].is_little_endian);
}

// ---------------------------------------------------------------------------
// Insufficient input
// ---------------------------------------------------------------------------

#[test]
fn empty_input_yields_no_candidates() {
    assert!(scan_rom_16bit_le(&[], &[8], 8, 8).is_empty());
}

#[test]
fn input_shorter_than_one_window_yields_no_candidates() {
    // 16x16 window needs 512 bytes.
    let rom = vec![0u8; 511];
    assert!(scan_rom_16bit_le(&rom, &[16], 16, 16).is_empty());
}

#[test]
fn an_empty_width_list_yields_no_candidates() {
    let rom = vec![0u8; 1024];
    assert!(scan_rom_16bit_le(&rom, &[], 16, 16).is_empty());
}

#[test]
fn min_rows_greater_than_max_rows_yields_no_candidates() {
    let table = ramp_table(16, 16);
    let rom = encode(&table, true);
    assert!(scan_rom_16bit_le(&rom, &[16], 20, 4).is_empty());
}

// ---------------------------------------------------------------------------
// Gate behaviour
// ---------------------------------------------------------------------------

#[test]
fn a_constant_block_is_rejected_by_the_entropy_gate() {
    // All-zero: entropy 0.0, below the 1.5 floor.
    let rom = vec![0u8; 16 * 16 * 2];
    assert!(
        scan_rom_16bit_le(&rom, &[16], 16, 16).is_empty(),
        "entropy 0 must be rejected"
    );
}

#[test]
fn a_pure_noise_block_is_rejected() {
    // Deterministic pseudo-noise with high entropy and no spatial structure.
    let mut state: u32 = 0x1234_5678;
    let mut rom = Vec::with_capacity(16 * 16 * 2);
    for _ in 0..(16 * 16 * 2) {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        rom.push((state >> 24) as u8);
    }
    let found = scan_rom_16bit_le(&rom, &[16], 16, 16);
    assert!(
        found.is_empty(),
        "high-entropy unstructured data must not be reported as a table"
    );
}

#[test]
fn a_spike_block_is_rejected_by_the_smoothness_gate() {
    // Flat 1000 with one large spike: entropy is low but smoothness is high.
    let mut table = vec![vec![1000u16; 16]; 16];
    table[8][8] = 60000;
    let rom = encode(&table, true);
    let found = scan_rom_16bit_le(&rom, &[16], 16, 16);
    assert!(
        found.is_empty(),
        "a single spike must fail the smoothness gate"
    );
}

// ---------------------------------------------------------------------------
// Candidate shape and ordering
// ---------------------------------------------------------------------------

#[test]
fn every_candidate_reports_consistent_dimensions() {
    let table = ramp_table(16, 16);
    let rom = encode(&table, true);
    for c in scan_rom_16bit_le(&rom, &[16], 16, 16) {
        assert_eq!(c.rows, 16);
        assert_eq!(c.cols, 16);
        assert_eq!(c.rows * c.cols * 2, 512, "byte span must match dimensions");
    }
}

#[test]
fn candidates_are_returned_in_ascending_offset_order() {
    let table = ramp_table(16, 16);
    let mut rom = vec![0u8; 64]; // padding before the table
    rom.extend(encode(&table, true));

    let found = scan_rom_16bit_le(&rom, &[16], 16, 16);
    let offsets: Vec<usize> = found.iter().map(|c| c.offset).collect();
    let mut sorted = offsets.clone();
    sorted.sort_unstable();
    assert_eq!(offsets, sorted, "output must be offset-ordered");
}

#[test]
fn overlapping_candidates_are_suppressed() {
    // A single smooth table must not be reported many times over.
    let table = ramp_table(16, 16);
    let rom = encode(&table, true);
    let found = scan_rom_16bit_le(&rom, &[16], 16, 16);
    assert!(
        found.len() <= 2,
        "overlap suppression must keep the list short, got {}",
        found.len()
    );
}

#[test]
fn scanning_is_deterministic() {
    let table = ramp_table(16, 16);
    let rom = encode(&table, true);
    let a = scan_rom_16bit_le(&rom, &[16], 16, 16);
    let b = scan_rom_16bit_le(&rom, &[16], 16, 16);
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(b.iter()) {
        assert_eq!(x.offset, y.offset);
        assert_eq!(x.rows, y.rows);
        assert_eq!(x.cols, y.cols);
    }
}

#[test]
fn scanning_does_not_mutate_the_input() {
    let table = ramp_table(16, 16);
    let rom = encode(&table, true);
    let copy = rom.clone();
    let _ = scan_rom_16bit_le(&rom, &[16], 16, 16);
    assert_eq!(rom, copy);
}

#[test]
fn a_candidate_never_extends_past_the_buffer() {
    let table = ramp_table(16, 16);
    let rom = encode(&table, true);
    for c in scan_rom_16bit_le(&rom, &[16], 16, 16) {
        let end = c.offset + c.rows * c.cols * 2;
        assert!(end <= rom.len(), "candidate must stay inside the buffer");
    }
}

#[test]
fn degenerate_dimensions_do_not_panic() {
    // Guards the deviation: the source would panic on a NaN score during sort.
    let rom = vec![0u8; 256];
    let _ = scan_rom_16bit_le(&rom, &[1], 1, 3);
    let _ = scan_rom_16bit_be(&rom, &[1], 1, 3);
    let _ = scan_rom_16bit(&rom, &[1], 1, 1, true);
}

#[test]
fn explicit_endianness_parameter_matches_the_wrappers() {
    let table = ramp_table(16, 16);
    let le = encode(&table, true);
    let via_le = scan_rom_16bit(&le, &[16], 16, 16, true);
    let via_wrapper = scan_rom_16bit_le(&le, &[16], 16, 16);
    assert_eq!(via_le.len(), via_wrapper.len());
}
