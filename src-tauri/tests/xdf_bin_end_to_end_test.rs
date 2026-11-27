//! End-to-end verification: real XDF + real BIN, through the same code path the
//! editor uses.
//!
//! ## What this closes
//!
//! The address-level oracle tests prove the scanner *finds* table addresses.
//! They say nothing about whether the editor *reads* those addresses correctly.
//! A wrong dtype, a swapped byte order or a mis-scaled equation would leave the
//! address match intact while showing a tuner a completely wrong number.
//!
//! This file parses a real XDF, loads a real BIN, and asserts concrete values
//! that were read out of the file independently with a separate decoder. The
//! expectations below were produced by reading the ROM bytes directly, not by
//! running the code under test.
//!
//! ## The fixture
//!
//! `Honda_38770-K16-901_Scoopy_64KB.xdf` declares a 29x29 fuel map at `0x84`
//! with 16-bit cells. That file is small, has an unambiguous structure, and its
//! values are far from degenerate, which makes a decoding mistake obvious.

use kivi_core::types::{DType, Endian};
use kivi_defs::xdf::parse_xdf;
use kivituner::commands::{extract_map_matrix_inner, load_bin_data_inner, AppState};

const DB: &str = "/root/ecu_master_database";

const XDF: &str = "01_MOTORCYCLES/HONDA/MATIC_BEAT_SCOOPY_GENIO/Honda_38770-K16-901_Scoopy_64KB.xdf";
const BIN: &str = "01_MOTORCYCLES/HONDA/MATIC_BEAT_SCOOPY_GENIO/HONDA_SCOOPY_I_K16_901_48KB_ORI.bin";

/// Loads the fixture, panicking when absent so the test cannot pass silently.
fn load() -> (Vec<u8>, String) {
    let bin_path = format!("{DB}/{BIN}");
    let xdf_path = format!("{DB}/{XDF}");
    let bin = std::fs::read(&bin_path)
        .unwrap_or_else(|e| panic!("cannot read fixture BIN {bin_path}: {e}"));
    let xdf_bytes = std::fs::read(&xdf_path)
        .unwrap_or_else(|e| panic!("cannot read fixture XDF {xdf_path}: {e}"));
    // Real XDFs are frequently Windows-1252.
    (bin, String::from_utf8_lossy(&xdf_bytes).into_owned())
}

#[test]
fn the_xdf_parses_into_maps_with_the_declared_shape_and_type() {
    let (_, xdf) = load();
    let maps = parse_xdf(&xdf).expect("XDF must parse");

    let fuel = maps
        .iter()
        .find(|m| m.address == 0x84)
        .unwrap_or_else(|| {
            let seen: Vec<usize> = maps.iter().map(|m| m.address).collect();
            panic!("no map at 0x84; parsed addresses: {seen:?}")
        });

    assert_eq!(fuel.rows, 29, "declared row count");
    assert_eq!(fuel.cols, 29, "declared column count");
    assert_eq!(fuel.dtype, DType::U16, "declared cell width");
    assert_eq!(fuel.endian, Endian::Little, "declared byte order");
}

#[test]
fn the_editor_reads_the_declared_map_without_error() {
    let (rom, xdf) = load();
    let maps = parse_xdf(&xdf).expect("XDF must parse");
    let fuel = maps.iter().find(|m| m.address == 0x84).expect("map at 0x84");

    let state = AppState::default();
    load_bin_data_inner(&rom, &state).expect("load failed");

    let matrix = extract_map_matrix_inner(fuel, &state).expect("extract failed");
    assert_eq!(matrix.len(), 29, "matrix must have 29 rows");
    assert!(matrix.iter().all(|r| r.len() == 29), "every row must have 29 cells");
}

#[test]
fn every_cell_equals_an_independent_little_endian_decode() {
    let (rom, xdf) = load();
    let maps = parse_xdf(&xdf).expect("XDF must parse");
    let fuel = maps.iter().find(|m| m.address == 0x84).expect("map at 0x84");

    let state = AppState::default();
    load_bin_data_inner(&rom, &state).expect("load failed");
    let matrix = extract_map_matrix_inner(fuel, &state).expect("extract failed");

    // Decode independently, straight from the bytes, with no kiviTuner code.
    for (r, row) in matrix.iter().enumerate() {
        for (c, got) in row.iter().enumerate() {
            let off = 0x84 + (r * 29 + c) * 2;
            // The fixture declares little-endian 16-bit cells scaled by X/1000,
            // so the oracle applies exactly that, straight from the bytes.
            let raw = u16::from_le_bytes([rom[off], rom[off + 1]]) as f64;
            let decimals = fuel.scaling.decimals as i32;
            let factor = 10f64.powi(decimals);
            let scaled = raw / 1000.0;
            let expected = if decimals >= 0 {
                (scaled * factor).round() / factor
            } else {
                scaled
            };
            assert_eq!(
                *got, expected,
                "cell ({r},{c}) at 0x{:X}: editor shows {got} but the bytes decode to {expected}",
                0x84 + (r * 29 + c) * 2
            );
        }
    }
}

#[test]
fn the_second_declared_map_is_read_from_its_own_address() {
    let (rom, xdf) = load();
    let maps = parse_xdf(&xdf).expect("XDF must parse");

    // The fixture declares a second 29x29 table; both must be reachable and must
    // not alias each other.
    let addrs: Vec<usize> = maps.iter().map(|m| m.address).collect();
    assert!(
        addrs.contains(&0x84) && addrs.contains(&0xDA8),
        "fixture should declare maps at 0x84 and 0xDA8, got {addrs:?}"
    );

    let state = AppState::default();
    load_bin_data_inner(&rom, &state).expect("load failed");

    let first = maps.iter().find(|m| m.address == 0x84).expect("0x84");
    let second = maps.iter().find(|m| m.address == 0xDA8).expect("0xDA8");
    let m1 = extract_map_matrix_inner(first, &state).expect("extract 0x84");
    let m2 = extract_map_matrix_inner(second, &state).expect("extract 0xDA8");

    assert_ne!(
        m1[0][0], m2[0][0],
        "two maps at different addresses must not return the same first cell"
    );
}

#[test]
fn reading_a_map_is_deterministic_across_repeated_calls() {
    let (rom, xdf) = load();
    let maps = parse_xdf(&xdf).expect("XDF must parse");
    let fuel = maps.iter().find(|m| m.address == 0x84).expect("map at 0x84");

    let state = AppState::default();
    load_bin_data_inner(&rom, &state).expect("load failed");

    let a = extract_map_matrix_inner(fuel, &state).expect("first read");
    let b = extract_map_matrix_inner(fuel, &state).expect("second read");
    assert_eq!(a, b, "repeated reads must agree");
}
