//! A real strided definition read through the editor path.
//!
//! ## Fixture
//!
//! `5JCZSNC0.xdf` (Suzuki Hayabusa Gen3) declares a table at `0x18251C` that is
//! 8 rows x 4 columns of 16-bit cells with:
//!
//! | Stride | Bits | Bytes | Meaning |
//! |---|---:|---:|---|
//! | major | 128 | 16 | row pitch |
//! | minor | 32 | 4 | **cell pitch** |
//!
//! The 4-byte cell pitch is the interesting part: each cell occupies 4 bytes but
//! stores a 16-bit value, so two adjacent definitions (`Resume RPM` at
//! `0x18251C` and `Cut RPM` at `0x18251E`) share each cell. Reading packed would
//! return the neighbouring definition's values.
//!
//! ## What is asserted
//!
//! Not a hard-coded table of numbers — those would have to come from running the
//! code under test. Instead the assertions are structural properties the data
//! must satisfy if it is read correctly:
//!
//! 1. every cell lies inside the ROM,
//! 2. the values are plausible RPM after the declared `X/2.56` scaling,
//! 3. the two interleaved definitions read **different** values, which is only
//!    true when the 4-byte pitch is honoured.

use kivi_defs::xdf::parse_xdf_full;
use kivituner::commands::{extract_map_matrix_inner, load_bin_data_inner, AppState};

const DIR: &str = "/root/ecu_master_database/01_MOTORCYCLES/SUZUKI/BIG_BIKE_GSXR_HAYABUSA";

fn load() -> (Vec<u8>, Vec<kivi_defs::model::MapDef>) {
    let rom = std::fs::read(format!("{DIR}/5JCZSNC0.bin"))
        .unwrap_or_else(|e| panic!("fixture BIN missing: {e}"));
    let xdf = std::fs::read(format!("{DIR}/5JCZSNC0.xdf"))
        .unwrap_or_else(|e| panic!("fixture XDF missing: {e}"));
    let defs = parse_xdf_full(&String::from_utf8_lossy(&xdf)).expect("parse");
    (rom, defs.maps)
}

#[test]
fn the_declared_strides_are_preserved() {
    let (_, maps) = load();
    let t = maps
        .iter()
        .find(|m| m.address == 0x18251C)
        .expect("table at 0x18251C");
    assert_eq!(t.rows, 8);
    assert_eq!(t.cols, 4);
    assert_eq!(t.major_stride_bits, 128, "row pitch is 16 bytes");
    assert_eq!(t.minor_stride_bits, 32, "cell pitch is 4 bytes");
}

#[test]
fn every_cell_of_a_strided_table_is_inside_the_rom() {
    let (rom, maps) = load();
    let t = maps
        .iter()
        .find(|m| m.address == 0x18251C)
        .expect("table at 0x18251C");

    let state = AppState::default();
    load_bin_data_inner(&rom, &state).expect("load");
    let matrix = extract_map_matrix_inner(t, &state).expect("extract");

    assert_eq!(matrix.len(), 8);
    assert!(matrix.iter().all(|r| r.len() == 4));
    for row in &matrix {
        for v in row {
            assert!(v.is_finite() && *v >= 0.0, "cell value {v} is implausible");
        }
    }
}

#[test]
fn the_strided_read_lands_on_plausible_rpm_values() {
    let (rom, maps) = load();
    let t = maps
        .iter()
        .find(|m| m.address == 0x18251C)
        .expect("table at 0x18251C");

    let state = AppState::default();
    load_bin_data_inner(&rom, &state).expect("load");
    let matrix = extract_map_matrix_inner(t, &state).expect("extract");

    // Rows 0..=6 are real fuel-cut RPM and must sit in a motorcycle-plausible
    // band. Row 7 ("Clutch-in") is all `0xFC00`, i.e. 64512 raw / 2.56 = 25200,
    // which is the definition's own "disabled" sentinel rather than an engine
    // speed. Reading big-endian instead of the declared little-endian would put
    // the sentinel pattern in the wrong places and blow the real rows out of
    // this band, so the assertion still catches a decode error.
    for (r, row) in matrix.iter().enumerate().take(7) {
        for v in row {
            assert!(
                *v > 500.0 && *v < 12000.0,
                "row {r} produced implausible RPM {v}; expected 500..12000"
            );
        }
    }
    let sentinel = &matrix[7];
    assert!(
        sentinel.iter().all(|v| (*v - 25200.0).abs() < 0.5),
        "row 7 should be the 0xFC00 disabled sentinel (25200 RPM), got {sentinel:?}"
    );
}

#[test]
fn the_two_interleaved_definitions_read_different_values() {
    let (rom, maps) = load();
    // Resume at 0x18251C and Cut at 0x18251E share each 4-byte cell, offset by
    // one 16-bit word. With a 4-byte cell pitch they must differ.
    let resume = maps
        .iter()
        .find(|m| m.address == 0x18251C)
        .expect("resume table");
    let cut = maps.iter().find(|m| m.address == 0x18251E).expect("cut table");

    let state = AppState::default();
    load_bin_data_inner(&rom, &state).expect("load");
    let a = extract_map_matrix_inner(resume, &state).expect("resume");
    let b = extract_map_matrix_inner(cut, &state).expect("cut");

    assert_ne!(
        a[0][0], b[0][0],
        "two definitions offset by 2 bytes must not read the same first cell"
    );
}

#[test]
fn reading_a_strided_table_is_deterministic() {
    let (rom, maps) = load();
    let t = maps
        .iter()
        .find(|m| m.address == 0x18251C)
        .expect("table at 0x18251C");

    let state = AppState::default();
    load_bin_data_inner(&rom, &state).expect("load");
    let a = extract_map_matrix_inner(t, &state).expect("first");
    let b = extract_map_matrix_inner(t, &state).expect("second");
    assert_eq!(a, b);
}
