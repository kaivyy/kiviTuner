//! Value-level verification of the ROM read and write path.
//!
//! ## Why this file exists
//!
//! Every existing oracle test (`bin_xdf_647_pairs_test`, `a2l_oracle_test`)
//! compares **addresses**, not **values**. An address comparison cannot catch a
//! decoding bug: if the scanner says "a table starts at 0x84" and the definition
//! says "a table starts at 0x84", the match succeeds whether the cells are
//! decoded as u16 big-endian, u16 little-endian, or u8.
//!
//! The path a tuner actually sees is different and had **no test at all**:
//! `load_bin_data` → `extract_map_matrix` → `read_value` → `read_f64` →
//! `evaluate_scaling`. This file pins that path.
//!
//! ## How the fixtures distinguish bugs
//!
//! Each byte pattern is chosen so that a wrong decode produces a *different*
//! number, never the same one by coincidence:
//!
//! | Bytes | u16 BE | u16 LE | u8 pair |
//! |---|---:|---:|---:|
//! | `12 34` | 4660 | 13330 | 18, 52 |
//! | `FF FE` | 65534 | 65279 | 255, 254 |
//!
//! A big/little mix-up or a width mix-up therefore fails loudly.

use kivituner::commands::{
    extract_map_matrix_inner, extract_original_map_matrix_inner, load_bin_data_inner,
    save_bin_data_inner, update_map_matrix_inner, AppState,
};
use kivi_core::types::{DType, Endian};
use kivi_defs::model::{MapDef, Scaling};

/// A ROM large enough for the offsets used below.
fn rom() -> Vec<u8> {
    vec![0u8; 512]
}

fn map(address: usize, rows: usize, cols: usize, dtype: DType, endian: Endian) -> MapDef {
    MapDef {
        id: "t".into(),
        name: "test".into(),
        category: "Test".into(),
        address,
        rows,
        cols,
        dtype,
        endian,
        column_major: false,
        major_stride_bits: 0,
        minor_stride_bits: 0,
        scaling: Scaling::raw(dtype),
        x_axis: None,
        y_axis: None,
    }
}

/// Loads `rom` into state and writes `bytes` at `address`.
fn state_with(address: usize, bytes: &[u8]) -> AppState {
    let state = AppState::default();
    let mut data = rom();
    data[address..address + bytes.len()].copy_from_slice(bytes);
    load_bin_data_inner(&data, &state).expect("load failed");
    state
}

#[test]
fn u16_big_endian_is_decoded_in_big_endian_order() {
    // 0x1234 = 4660 big-endian; little-endian would give 13330.
    let state = state_with(0x40, &[0x12, 0x34]);
    let m = map(0x40, 1, 1, DType::U16, Endian::Big);
    let matrix = extract_map_matrix_inner(&m, &state).expect("extract failed");
    assert_eq!(matrix[0][0], 4660.0, "u16 BE must decode 0x1234 as 4660");
}

#[test]
fn u16_little_endian_is_decoded_in_little_endian_order() {
    let state = state_with(0x40, &[0x12, 0x34]);
    let m = map(0x40, 1, 1, DType::U16, Endian::Little);
    let matrix = extract_map_matrix_inner(&m, &state).expect("extract failed");
    assert_eq!(matrix[0][0], 13330.0, "u16 LE must decode 0x1234 as 13330");
}

#[test]
fn u8_reads_one_byte_not_two() {
    // Reading this as u16 would give 4660; as u8 it must be 18.
    let state = state_with(0x40, &[0x12, 0x34]);
    let m = map(0x40, 1, 2, DType::U8, Endian::Big);
    let matrix = extract_map_matrix_inner(&m, &state).expect("extract failed");
    assert_eq!(matrix[0], vec![18.0, 52.0], "u8 must read 0x12 and 0x34 separately");
}

#[test]
fn i16_negative_values_are_signed() {
    // 0xFFFE as i16 = -2; as u16 it would be 65534.
    let state = state_with(0x40, &[0xFF, 0xFE]);
    let m = map(0x40, 1, 1, DType::I16, Endian::Big);
    let matrix = extract_map_matrix_inner(&m, &state).expect("extract failed");
    assert_eq!(matrix[0][0], -2.0, "i16 must be sign-extended");
}

#[test]
fn u16_does_not_sign_extend() {
    let state = state_with(0x40, &[0xFF, 0xFE]);
    let m = map(0x40, 1, 1, DType::U16, Endian::Big);
    let matrix = extract_map_matrix_inner(&m, &state).expect("extract failed");
    assert_eq!(matrix[0][0], 65534.0, "u16 must stay unsigned");
}

#[test]
fn i8_negative_values_are_signed() {
    let state = state_with(0x40, &[0xFE, 0x7F]);
    let m = map(0x40, 1, 2, DType::I8, Endian::Big);
    let matrix = extract_map_matrix_inner(&m, &state).expect("extract failed");
    assert_eq!(matrix[0], vec![-2.0, 127.0]);
}

#[test]
fn f32_is_decoded_as_ieee754() {
    let mut bytes = [0u8; 4];
    bytes.copy_from_slice(&1.5f32.to_be_bytes());
    let state = state_with(0x40, &bytes);
    let m = map(0x40, 1, 1, DType::F32, Endian::Big);
    let matrix = extract_map_matrix_inner(&m, &state).expect("extract failed");
    assert!((matrix[0][0] - 1.5).abs() < 1e-6);
}

#[test]
fn cells_are_read_row_major_by_default() {
    // Four cells, each distinguishable by its high byte.
    let state = state_with(0x40, &[0x00, 0x01, 0x00, 0x02, 0x00, 0x03, 0x00, 0x04]);
    let m = map(0x40, 2, 2, DType::U16, Endian::Big);
    let matrix = extract_map_matrix_inner(&m, &state).expect("extract failed");
    assert_eq!(matrix, vec![vec![1.0, 2.0], vec![3.0, 4.0]]);
}

#[test]
fn column_major_orders_cells_by_column() {
    let state = state_with(0x40, &[0x00, 0x01, 0x00, 0x02, 0x00, 0x03, 0x00, 0x04]);
    let mut m = map(0x40, 2, 2, DType::U16, Endian::Big);
    m.column_major = true;
    let matrix = extract_map_matrix_inner(&m, &state).expect("extract failed");
    // Column-major: index c*rows + r, so cell (0,0)=1, (0,1)=3, (1,0)=2, (1,1)=4.
    assert_eq!(matrix, vec![vec![1.0, 3.0], vec![2.0, 4.0]]);
}

#[test]
fn the_address_offsets_every_cell() {
    // Data written at 0x40 must not be read when the map starts at 0x50.
    let state = state_with(0x40, &[0x00, 0x01]);
    let m = map(0x50, 1, 1, DType::U16, Endian::Big);
    let matrix = extract_map_matrix_inner(&m, &state).expect("extract failed");
    assert_eq!(matrix[0][0], 0.0, "a map at 0x50 must not read the bytes at 0x40");
}

#[test]
fn scaling_is_applied_to_the_raw_value() {
    // X * 0.1 with 1 decimal: 0x0064 = 100 -> 10.0
    let state = state_with(0x40, &[0x00, 0x64]);
    let mut m = map(0x40, 1, 1, DType::U16, Endian::Big);
    m.scaling = Scaling {
        equation: "X*0.1".into(),
        unit: "ms".into(),
        decimals: 1,
    };
    let matrix = extract_map_matrix_inner(&m, &state).expect("extract failed");
    assert_eq!(matrix[0][0], 10.0);
}

#[test]
fn reading_past_the_end_is_refused_not_panicked() {
    let state = state_with(0, &[]);
    // 512-byte ROM, u16 at 511 needs bytes 511..513.
    let m = map(511, 1, 1, DType::U16, Endian::Big);
    let err = extract_map_matrix_inner(&m, &state).unwrap_err();
    assert!(!err.is_empty(), "an out-of-bounds read must return an error");
}

#[test]
fn writing_then_reading_round_trips_exactly() {
    let state = state_with(0x40, &[0x00, 0x00, 0x00, 0x00]);
    let m = map(0x40, 1, 2, DType::U16, Endian::Big);

    update_map_matrix_inner(&m, &[vec![1000.0, 2000.0]], &state).expect("write failed");
    let matrix = extract_map_matrix_inner(&m, &state).expect("read failed");
    assert_eq!(matrix, vec![vec![1000.0, 2000.0]]);
}

#[test]
fn writing_preserves_big_endian_byte_order_in_the_rom() {
    // 0x03E8 = 1000 must land as 03 E8, not E8 03.
    let state = state_with(0x40, &[0x00, 0x00, 0x00, 0x00]);
    let m = map(0x40, 1, 2, DType::U16, Endian::Big);
    update_map_matrix_inner(&m, &[vec![1000.0, 2000.0]], &state).expect("write failed");

    let saved = save_bin_data_inner(&state).expect("save failed");
    assert_eq!(&saved[0x40..0x44], &[0x03, 0xE8, 0x07, 0xD0]);
}

#[test]
fn writing_preserves_little_endian_byte_order_in_the_rom() {
    let state = state_with(0x40, &[0x00, 0x00, 0x00, 0x00]);
    let m = map(0x40, 1, 2, DType::U16, Endian::Little);
    update_map_matrix_inner(&m, &[vec![1000.0, 2000.0]], &state).expect("write failed");

    let saved = save_bin_data_inner(&state).expect("save failed");
    assert_eq!(&saved[0x40..0x44], &[0xE8, 0x03, 0xD0, 0x07]);
}

#[test]
fn writing_leaves_bytes_outside_the_map_untouched() {
    let mut data = rom();
    data[0x3E] = 0xAA;
    data[0x42] = 0xBB;
    let state = AppState::default();
    load_bin_data_inner(&data, &state).expect("load failed");

    let m = map(0x40, 1, 1, DType::U16, Endian::Big);
    update_map_matrix_inner(&m, &[vec![7.0]], &state).expect("write failed");

    let saved = save_bin_data_inner(&state).expect("save failed");
    assert_eq!(saved[0x3E], 0xAA, "byte before the map must survive");
    assert_eq!(saved[0x42], 0xBB, "byte after the map must survive");
    assert_eq!(&saved[0x40..0x42], &[0x00, 0x07]);
}

#[test]
fn an_unloaded_state_reports_an_error_instead_of_panicking() {
    let state = AppState::default();
    let m = map(0, 1, 1, DType::U8, Endian::Big);
    let err = extract_map_matrix_inner(&m, &state).unwrap_err();
    assert!(err.contains("No binary"), "unexpected message: {err}");
}

#[test]
fn original_matrix_remains_immutable_after_edits() {
    let state = state_with(0x40, &[0x00, 0x64, 0x00, 0xC8]);
    let m = map(0x40, 1, 2, DType::U16, Endian::Big);
    update_map_matrix_inner(&m, &[vec![999.0, 1234.0]], &state).expect("write failed");

    let live = extract_map_matrix_inner(&m, &state).expect("live read failed");
    let stock = extract_original_map_matrix_inner(&m, &state).expect("stock read failed");
    assert_eq!(live, vec![vec![999.0, 1234.0]]);
    assert_eq!(stock, vec![vec![100.0, 200.0]]);
}
