//! Parity tests for the ported float-matrix and twin-map engines.
//!
//! Ported from `ecu_engine_rust::math::float_map` (`FloatMapEngine`) and
//! `ecu_engine_rust::math::twin_maps` (`CoupledMapPair`, `MapCouplingMode`).
//!
//! ## How expected values were obtained
//!
//! Byte layouts are derived from IEEE-754 definitions (not from running the
//! implementation): 1.5f32 little-endian is `00 00 C0 3F`, and so on. The
//! twin-map arithmetic is hand-computed from the source formulas.
//!
//! ## Documented deviation
//!
//! `write_float_matrix` mutates the caller's ROM buffer in the source. Here it is
//! exposed as `write_float_matrix_into`, returning a new buffer, matching this
//! crate's read-only convention. Byte layout is identical.

use kivi_defs::float_map::{
    read_float_matrix, write_float_matrix_into, FloatMapFormat,
};
use kivi_defs::twin_maps::{apply_synchronized_edit, CoupledMapPair, MapCouplingMode};

// ---------------------------------------------------------------------------
// float_map — decoding
// ---------------------------------------------------------------------------

#[test]
fn reads_float32_little_endian() {
    // 1.5f32 = 0x3FC00000 -> LE bytes 00 00 C0 3F
    // 2.75f32 = 0x40300000 -> LE bytes 00 00 30 40
    let rom = [0x00, 0x00, 0xC0, 0x3F, 0x00, 0x00, 0x30, 0x40];
    let m = read_float_matrix(&rom, 0, 1, 2, FloatMapFormat::Float32LittleEndian).expect("read");
    assert_eq!(m, vec![vec![1.5f64, 2.75f64]]);
}

#[test]
fn reads_float32_big_endian() {
    // 1.5f32 BE bytes: 3F C0 00 00
    let rom = [0x3F, 0xC0, 0x00, 0x00];
    let m = read_float_matrix(&rom, 0, 1, 1, FloatMapFormat::Float32BigEndian).expect("read");
    assert_eq!(m[0][0], 1.5f64);
}

#[test]
fn reads_float64_little_endian() {
    // 1.5f64 = 0x3FF8000000000000 -> LE bytes 00 00 00 00 00 00 F8 3F
    let rom = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xF8, 0x3F];
    let m = read_float_matrix(&rom, 0, 1, 1, FloatMapFormat::Float64LittleEndian).expect("read");
    assert_eq!(m[0][0], 1.5f64);
}

#[test]
fn reads_float64_big_endian() {
    let rom = [0x3F, 0xF8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    let m = read_float_matrix(&rom, 0, 1, 1, FloatMapFormat::Float64BigEndian).expect("read");
    assert_eq!(m[0][0], 1.5f64);
}

#[test]
fn reads_a_two_by_two_matrix_row_major() {
    // Four float32 LE values: 1.0, 2.0, 3.0, 4.0
    let rom = [
        0x00, 0x00, 0x80, 0x3F, // 1.0
        0x00, 0x00, 0x00, 0x40, // 2.0
        0x00, 0x00, 0x40, 0x40, // 3.0
        0x00, 0x00, 0x80, 0x40, // 4.0
    ];
    let m = read_float_matrix(&rom, 0, 2, 2, FloatMapFormat::Float32LittleEndian).expect("read");
    assert_eq!(m, vec![vec![1.0, 2.0], vec![3.0, 4.0]]);
}

#[test]
fn reads_from_a_nonzero_offset() {
    let rom = [0xFF, 0xFF, 0x00, 0x00, 0x80, 0x3F];
    let m = read_float_matrix(&rom, 2, 1, 1, FloatMapFormat::Float32LittleEndian).expect("read");
    assert_eq!(m[0][0], 1.0f64);
}

#[test]
fn read_rejects_a_matrix_that_exceeds_the_buffer() {
    let rom = [0u8; 4];
    let err = read_float_matrix(&rom, 0, 2, 2, FloatMapFormat::Float32LittleEndian)
        .expect_err("must reject");
    assert_eq!(err, "Float matrix exceeds ROM buffer bounds");
}

// ---------------------------------------------------------------------------
// float_map — encoding
// ---------------------------------------------------------------------------

#[test]
fn writes_float32_little_endian_bytes() {
    let rom = vec![0u8; 4];
    let out = write_float_matrix_into(
        &rom,
        0,
        &[vec![1.5f64]],
        FloatMapFormat::Float32LittleEndian,
    )
    .expect("write");
    assert_eq!(out, vec![0x00, 0x00, 0xC0, 0x3F]);
}

#[test]
fn writes_float32_big_endian_bytes() {
    let rom = vec![0u8; 4];
    let out =
        write_float_matrix_into(&rom, 0, &[vec![1.5f64]], FloatMapFormat::Float32BigEndian)
            .expect("write");
    assert_eq!(out, vec![0x3F, 0xC0, 0x00, 0x00]);
}

#[test]
fn write_then_read_round_trips_float32() {
    let rom = vec![0u8; 16];
    let original = vec![vec![1.5f64, 2.75f64], vec![14.7f64, 21.0f64]];

    let written =
        write_float_matrix_into(&rom, 0, &original, FloatMapFormat::Float32LittleEndian)
            .expect("write");
    let decoded =
        read_float_matrix(&written, 0, 2, 2, FloatMapFormat::Float32LittleEndian).expect("read");

    for r in 0..2 {
        for c in 0..2 {
            assert!(
                (decoded[r][c] - original[r][c]).abs() < 1e-5,
                "mismatch at [{r}][{c}]"
            );
        }
    }
}

#[test]
fn write_then_read_round_trips_float64_exactly() {
    let rom = vec![0u8; 16];
    let original = vec![vec![1.5f64, -2.25f64]];

    let written =
        write_float_matrix_into(&rom, 0, &original, FloatMapFormat::Float64LittleEndian)
            .expect("write");
    let decoded =
        read_float_matrix(&written, 0, 1, 2, FloatMapFormat::Float64LittleEndian).expect("read");

    // f64 round-trips bit-exactly.
    assert_eq!(decoded, original);
}

#[test]
fn write_leaves_the_input_buffer_untouched() {
    let rom = vec![0u8; 8];
    let original = rom.clone();
    let _ = write_float_matrix_into(&rom, 0, &[vec![1.0f64]], FloatMapFormat::Float32LittleEndian)
        .expect("write");
    assert_eq!(rom, original);
}

#[test]
fn write_rejects_a_matrix_that_exceeds_the_buffer() {
    let rom = vec![0u8; 4];
    let err = write_float_matrix_into(
        &rom,
        0,
        &[vec![1.0f64, 2.0f64]],
        FloatMapFormat::Float32LittleEndian,
    )
    .expect_err("must reject");
    assert_eq!(err, "Float matrix exceeds ROM buffer bounds");
}

#[test]
fn write_of_an_empty_matrix_is_a_no_op() {
    let rom = vec![0xAAu8; 4];
    let out = write_float_matrix_into(&rom, 0, &[], FloatMapFormat::Float32LittleEndian)
        .expect("write");
    assert_eq!(out, rom);
}

// ---------------------------------------------------------------------------
// twin_maps
// ---------------------------------------------------------------------------

#[test]
fn absolute_sync_copies_the_master_value() {
    let mut master = vec![vec![10.0; 2]; 2];
    let mut slave = vec![vec![10.5; 2]; 2];
    let pair = CoupledMapPair::new("m", "s", 2, 2, MapCouplingMode::AbsoluteSync);

    pair.apply_synchronized_edit(&mut master, &mut slave, 0, 0, 20.0)
        .expect("edit");

    assert_eq!(master[0][0], 20.0);
    assert_eq!(slave[0][0], 20.0, "slave is set exactly equal");
}

#[test]
fn identical_delta_adds_the_master_delta_to_the_slave() {
    // Source fixture: master 10.0 -> 11.2 (delta +1.2), slave 10.5 -> 11.7.
    let mut master = vec![vec![10.0; 4]; 4];
    let mut slave = vec![vec![10.5; 4]; 4];
    let pair = CoupledMapPair::new("Cyl1", "Cyl2", 4, 4, MapCouplingMode::IdenticalDelta);

    pair.apply_synchronized_edit(&mut master, &mut slave, 1, 2, 11.2)
        .expect("edit");

    assert_eq!(master[1][2], 11.2);
    assert!((slave[1][2] - 11.7).abs() < 1e-6);
}

#[test]
fn percentage_delta_scales_the_slave_by_the_master_ratio() {
    // master 10.0 -> 12.0 is +20%; slave 10.5 * 1.2 = 12.6
    let mut master = vec![vec![10.0; 2]; 2];
    let mut slave = vec![vec![10.5; 2]; 2];
    let pair = CoupledMapPair::new("m", "s", 2, 2, MapCouplingMode::PercentageDelta);

    pair.apply_synchronized_edit(&mut master, &mut slave, 0, 0, 12.0)
        .expect("edit");

    assert!((slave[0][0] - 12.6).abs() < 1e-6);
}

#[test]
fn percentage_delta_falls_back_to_additive_when_master_is_zero() {
    // |old_master| <= 1e-9 takes the additive branch: slave 5.0 + 3.0 = 8.0
    let mut master = vec![vec![0.0; 2]; 2];
    let mut slave = vec![vec![5.0; 2]; 2];
    let pair = CoupledMapPair::new("m", "s", 2, 2, MapCouplingMode::PercentageDelta);

    pair.apply_synchronized_edit(&mut master, &mut slave, 0, 0, 3.0)
        .expect("edit");

    assert_eq!(slave[0][0], 8.0);
}

#[test]
fn edit_rejects_out_of_bounds_coordinates() {
    let mut master = vec![vec![10.0; 2]; 2];
    let mut slave = vec![vec![10.0; 2]; 2];
    let pair = CoupledMapPair::new("m", "s", 2, 2, MapCouplingMode::AbsoluteSync);

    let err = pair
        .apply_synchronized_edit(&mut master, &mut slave, 2, 0, 1.0)
        .expect_err("must reject");
    assert_eq!(err, "Edit coordinates out of bounds");
}

#[test]
fn move_cursor_tracks_position_and_validates_bounds() {
    let mut pair = CoupledMapPair::new("m", "s", 3, 4, MapCouplingMode::AbsoluteSync);
    assert_eq!(pair.current_cursor_row, 0);
    assert_eq!(pair.current_cursor_col, 0);

    pair.move_cursor(2, 3).expect("move");
    assert_eq!(pair.current_cursor_row, 2);
    assert_eq!(pair.current_cursor_col, 3);

    let err = pair.move_cursor(3, 0).expect_err("must reject");
    assert_eq!(err, "Cursor coordinates out of bounds");
}

#[test]
fn new_initialises_cursor_at_origin_and_copies_ids() {
    let pair = CoupledMapPair::new("Cyl1", "Cyl2", 4, 4, MapCouplingMode::IdenticalDelta);
    assert_eq!(pair.master_map_id, "Cyl1");
    assert_eq!(pair.slave_map_id, "Cyl2");
    assert_eq!(pair.rows, 4);
    assert_eq!(pair.cols, 4);
    assert_eq!(pair.current_cursor_row, 0);
    assert_eq!(pair.current_cursor_col, 0);
}

#[test]
fn a_free_function_form_is_available() {
    let mut master = vec![vec![10.0; 2]; 2];
    let mut slave = vec![vec![10.0; 2]; 2];

    apply_synchronized_edit(
        MapCouplingMode::AbsoluteSync,
        2,
        2,
        &mut master,
        &mut slave,
        0,
        1,
        42.0,
    )
    .expect("edit");

    assert_eq!(master[0][1], 42.0);
    assert_eq!(slave[0][1], 42.0);
}
