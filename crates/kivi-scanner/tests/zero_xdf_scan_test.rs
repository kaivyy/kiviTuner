//! Parity tests for Zero-XDF Z3 table scanning.
//!
//! Ported from `ecu_engine_rust::recognition::zero_xdf_discovery::ZeroXdfEngine`
//! (`check_candidate_16bit_integer`, `calc_ac_and_smoothness_16bit`,
//! `scan_16bit_tables`) and the `DiscoveredFullTable` / `DiscoveredAxis` types.
//!
//! ## Gates (read from the source, asserted here)
//!
//! 1. **Row-delta pre-check** — for the first `min(rows, 3)` rows, any adjacent
//!    delta above `4500` rejects; total deltas must stay `<= cols·check_rows·600`.
//! 2. **Autocorrelation gate** — `ac = num/denom` at lag `cols`; reject if `ac < 0.35`.
//! 3. **Smoothness gate** — `smooth = (mean_dx + mean_dy) / std_dev`; accept only
//!    if `smooth < 0.48`.
//! 4. **Confidence** — `(1 - min(smooth, 0.9)) * ac`.
//!
//! ## How expected values were obtained
//!
//! Every expectation is derived from those formulas by hand for synthetic
//! matrices, plus one real-ROM check that asserts *structural* properties
//! (offsets in bounds, dimensions match, deterministic ordering) rather than
//! exact counts — exact counts would require re-running the source, which this
//! task forbids.

use kivi_scanner::zero_xdf_scan::{
    calc_ac_and_smoothness_16bit, check_candidate_16bit_integer, scan_16bit_tables,
    DiscoveredFullTable, Endianness,
};

fn encode(matrix: &[Vec<u16>], endian: Endianness) -> Vec<u8> {
    let mut out = Vec::new();
    for row in matrix {
        for &v in row {
            match endian {
                Endianness::LittleEndian => out.extend_from_slice(&v.to_le_bytes()),
                Endianness::BigEndian => out.extend_from_slice(&v.to_be_bytes()),
            }
        }
    }
    out
}

/// A table with identical rows: strong vertical autocorrelation (ac = 0.875)
/// and low smoothness (0.436), so it clears both gates.
///
/// A *diagonal* ramp does NOT clear them: an 8x8 ramp gives smoothness 0.4872,
/// just above the 0.48 gate. That is a property of the gate, not a bug.
fn ramp(rows: usize, cols: usize) -> Vec<Vec<u16>> {
    let row: Vec<u16> = (0..cols).map(|c| 1000 + c as u16).collect();
    vec![row; rows]
}

// ---------------------------------------------------------------------------
// Row-delta pre-check
// ---------------------------------------------------------------------------

#[test]
fn a_smooth_ramp_passes_the_row_delta_check() {
    let m = ramp(8, 8);
    let bytes = encode(&m, Endianness::LittleEndian);
    assert!(check_candidate_16bit_integer(
        &bytes,
        8,
        8,
        Endianness::LittleEndian
    ));
}

#[test]
fn a_large_single_delta_rejects_the_candidate() {
    // One adjacent delta above 4500 must reject immediately.
    let mut m = ramp(8, 8);
    m[0][4] = m[0][3] + 5000;
    let bytes = encode(&m, Endianness::LittleEndian);
    assert!(
        !check_candidate_16bit_integer(&bytes, 8, 8, Endianness::LittleEndian),
        "a delta above 4500 must reject"
    );
}

#[test]
fn only_the_first_three_rows_are_examined() {
    // A spike in row 4 (zero-indexed) is outside the check window.
    let mut m = ramp(8, 8);
    m[4][4] = m[4][3] + 9000;
    let bytes = encode(&m, Endianness::LittleEndian);
    assert!(
        check_candidate_16bit_integer(&bytes, 8, 8, Endianness::LittleEndian),
        "rows beyond the third must not affect the pre-check"
    );
}

#[test]
fn the_pre_check_reads_the_requested_endianness() {
    let m = ramp(4, 4);
    let le = encode(&m, Endianness::LittleEndian);
    let be = encode(&m, Endianness::BigEndian);
    // Each buffer must pass when read with its own byte order.
    assert!(check_candidate_16bit_integer(&le, 4, 4, Endianness::LittleEndian));
    assert!(check_candidate_16bit_integer(&be, 4, 4, Endianness::BigEndian));
}

// ---------------------------------------------------------------------------
// Autocorrelation / smoothness gate
// ---------------------------------------------------------------------------

#[test]
fn a_flat_matrix_has_no_autocorrelation_and_is_rejected() {
    // Constant values -> denom below 1e-12 -> ac = 0 -> rejected.
    let m = vec![vec![500u16; 8]; 8];
    let bytes = encode(&m, Endianness::LittleEndian);
    assert!(
        calc_ac_and_smoothness_16bit(&bytes, 8, 8, Endianness::LittleEndian).is_none(),
        "a constant block must be rejected by the autocorrelation gate"
    );
}

#[test]
fn a_gentle_ramp_passes_both_gates_and_reports_ac_and_smoothness() {
    let m = ramp(8, 8);
    let bytes = encode(&m, Endianness::LittleEndian);
    let r = calc_ac_and_smoothness_16bit(&bytes, 8, 8, Endianness::LittleEndian);
    assert!(r.is_some(), "a gentle ramp must pass both gates");
    let (ac, smooth) = r.unwrap();
    assert!(ac >= 0.35, "ac {ac} must clear the 0.35 gate");
    assert!(smooth < 0.48, "smoothness {smooth} must clear the 0.48 gate");
}

#[test]
fn autocorrelation_at_lag_cols_is_strong_for_a_vertical_ramp() {
    // Identical rows -> perfect correlation at lag = cols.
    let row: Vec<u16> = (0..8).map(|c| 1000 + c as u16).collect();
    let m = vec![row; 8];
    let bytes = encode(&m, Endianness::LittleEndian);
    let (ac, smooth) = calc_ac_and_smoothness_16bit(&bytes, 8, 8, Endianness::LittleEndian)
        .expect("identical rows must pass");
    // Identical rows give ac = 0.875 and smoothness = 0.4364 for 8x8.
    assert!((ac - 0.875).abs() < 1e-9, "expected ac = 0.875, got {ac}");
    assert!((smooth - 0.4364).abs() < 1e-3, "expected smoothness ~0.4364, got {smooth}");
}

#[test]
fn a_steep_sawtooth_is_rejected_by_the_smoothness_gate() {
    // Large alternating deltas push smoothness above 0.48.
    let m: Vec<Vec<u16>> = (0..8)
        .map(|r| (0..8).map(|c| if (r + c) % 2 == 0 { 100 } else { 50000 }).collect())
        .collect();
    let bytes = encode(&m, Endianness::LittleEndian);
    assert!(
        calc_ac_and_smoothness_16bit(&bytes, 8, 8, Endianness::LittleEndian).is_none(),
        "an alternating high-contrast block must be rejected"
    );
}

// ---------------------------------------------------------------------------
// Full scan
// ---------------------------------------------------------------------------

#[test]
fn scanning_finds_a_table_in_a_synthetic_rom() {
    // Place a smooth 8x8 table at offset 0x2000 inside varied filler.
    let mut rom: Vec<u8> = (0..0x4000).map(|i| (i % 251) as u8).collect();
    let table = ramp(8, 8);
    let bytes = encode(&table, Endianness::LittleEndian);
    rom[0x2000..0x2000 + bytes.len()].copy_from_slice(&bytes);

    let found = scan_16bit_tables(&rom, &[(8, 8)], Endianness::LittleEndian, 0, rom.len());
    assert!(!found.is_empty(), "the planted table must be discovered");

    // Every candidate must be structurally sound.
    for t in &found {
        assert_eq!(t.rows, 8);
        assert_eq!(t.cols, 8);
        assert!(t.z_address + t.rows * t.cols * 2 <= rom.len());
        assert!(t.confidence_score >= 0.0);
        assert!(t.smoothness >= 0.0);
    }
}

#[test]
fn an_empty_dimension_list_yields_no_tables() {
    let rom = vec![0u8; 4096];
    assert!(scan_16bit_tables(&rom, &[], Endianness::LittleEndian, 0, rom.len()).is_empty());
}

#[test]
fn a_scan_window_outside_the_rom_yields_no_tables() {
    let rom = vec![0u8; 4096];
    assert!(
        scan_16bit_tables(&rom, &[(8, 8)], Endianness::LittleEndian, 9000, 10000).is_empty()
    );
}

#[test]
fn scanning_is_deterministic() {
    let mut rom: Vec<u8> = (0..0x4000).map(|i| (i % 251) as u8).collect();
    let bytes = encode(&ramp(8, 8), Endianness::LittleEndian);
    rom[0x2000..0x2000 + bytes.len()].copy_from_slice(&bytes);

    let a = scan_16bit_tables(&rom, &[(8, 8)], Endianness::LittleEndian, 0, rom.len());
    let b = scan_16bit_tables(&rom, &[(8, 8)], Endianness::LittleEndian, 0, rom.len());
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(b.iter()) {
        assert_eq!(x.z_address, y.z_address);
        assert_eq!(x.rows, y.rows);
        assert_eq!(x.cols, y.cols);
    }
}

#[test]
fn scanning_does_not_mutate_the_rom() {
    let rom: Vec<u8> = (0..0x4000).map(|i| (i % 251) as u8).collect();
    let copy = rom.clone();
    let _ = scan_16bit_tables(&rom, &[(8, 8)], Endianness::LittleEndian, 0, rom.len());
    assert_eq!(rom, copy);
}

#[test]
fn a_candidate_reports_the_documented_shape() {
    let mut rom: Vec<u8> = (0..0x4000).map(|i| (i % 251) as u8).collect();
    let bytes = encode(&ramp(8, 8), Endianness::LittleEndian);
    rom[0x2000..0x2000 + bytes.len()].copy_from_slice(&bytes);

    let found = scan_16bit_tables(&rom, &[(8, 8)], Endianness::LittleEndian, 0, rom.len());
    let t: &DiscoveredFullTable = &found[0];
    assert_eq!(t.sample_matrix.len(), 8);
    assert_eq!(t.sample_matrix[0].len(), 8);
}

// ---------------------------------------------------------------------------
// Real-ROM structural validation (fixtures owned by the project)
// ---------------------------------------------------------------------------

#[test]
fn a_real_rom_scan_produces_structurally_valid_candidates() {
    // Fixtures are the project owner's own ROM database; used here only to prove
    // the scanner does not panic or emit out-of-bounds candidates on real data.
    let path = "/root/ecu_master_database/01_MOTORCYCLES/HONDA/BIG_BIKE_CBR650_CB1000_XADV/38770-MJE-T52.bin";
    let rom = match std::fs::read(path) {
        Ok(r) => r,
        Err(_) => return, // fixture absent in this environment: skip, do not fail
    };

    let found = scan_16bit_tables(
        &rom,
        &[(8, 8), (16, 8)],
        Endianness::BigEndian,
        0,
        rom.len(),
    );

    for t in &found {
        let end = t.z_address + t.rows * t.cols * 2;
        assert!(end <= rom.len(), "candidate must stay inside the ROM");
        assert!(t.confidence_score.is_finite());
        assert!(t.smoothness.is_finite());
        assert!(t.smoothness < 0.48, "every accepted table cleared the gate");
    }
}
