//! Application-path tests for the read-only smoothing analysis command.
//!
//! Verifies the ported smoothing algorithms are reachable from a real kiviTuner
//! execution path and that the analysis is strictly read-only: it must not
//! mutate ROM bytes, `MemoryMap`, map definitions, or any `AppState` field, and
//! it must never write results back to a calibration table.
//!
//! Expected values match the oracle-derived fixtures in
//! `crates/kivi-defs/tests/smoothing_parity_test.rs`.

use kivituner::commands::{
    analyze_map_smoothing_inner, load_bin_data_inner, parse_xdf_content_inner,
};
use kivituner::AppState;

const XDF_2D: &str = r#"<?xml version="1.0"?>
<XDFFORMAT version="1.60">
  <XDFTABLE uniqueid="0x4000" flags="0x1">
    <title>Spike Map</title>
    <XDFAXIS id="x">
      <EMBEDDEDDATA mmedelementsizebits="16" mmedmajorstridebits="0" />
      <indexcount>3</indexcount>
      <MATH equation="X"><VAR id="X"/></MATH>
    </XDFAXIS>
    <XDFAXIS id="y">
      <EMBEDDEDDATA mmedelementsizebits="16" mmedmajorstridebits="0" />
      <indexcount>3</indexcount>
      <MATH equation="X"><VAR id="X"/></MATH>
    </XDFAXIS>
    <XDFAXIS id="z">
      <EMBEDDEDDATA mmedaddress="0x4000" mmedelementsizebits="16"
                    mmedrowcount="3" mmedcolcount="3" mmedmajorstridebits="16" />
      <indexcount>3</indexcount>
      <MATH equation="X"><VAR id="X"/></MATH>
    </XDFAXIS>
  </XDFTABLE>
</XDFFORMAT>"#;

fn make_rom() -> Vec<u8> {
    // 3x3, 16-bit big-endian row-major: 10,10,10 / 10,50,10 / 10,10,10
    let values: [u16; 9] = [10, 10, 10, 10, 50, 10, 10, 10, 10];
    let mut rom = vec![0u8; 0x4000 + 18];
    for (i, v) in values.iter().enumerate() {
        let off = 0x4000 + i * 2;
        rom[off] = (v >> 8) as u8;
        rom[off + 1] = (v & 0xFF) as u8;
    }
    rom
}

fn setup() -> (AppState, String) {
    let state = AppState::default();
    load_bin_data_inner(&make_rom(), &state).expect("load failed");
    let maps = parse_xdf_content_inner(XDF_2D, &state).expect("xdf parse failed");
    assert_eq!(maps.len(), 1);
    assert_eq!(maps[0].rows, 3);
    assert_eq!(maps[0].cols, 3);
    (state, maps[0].id.clone())
}

#[test]
fn smoothing_analysis_reports_laplacian_spike() {
    let (state, map_id) = setup();
    let r = analyze_map_smoothing_inner(&map_id, 0.5, None, &state).expect("analysis failed");

    assert_eq!(r.rows, 3);
    assert_eq!(r.cols, 3);
    // Centre spike: 10+10+10+10 - 4*50 = -160
    assert_eq!(r.laplacian[1][1], -160.0);
    // Borders are zero
    assert_eq!(r.laplacian[0][0], 0.0);
    // Max absolute curvature is 160
    assert_eq!(r.max_abs_curvature, 160.0);
    // Spike cell located at (1,1)
    assert_eq!(r.max_curvature_row, 1);
    assert_eq!(r.max_curvature_col, 1);
}

#[test]
fn smoothing_analysis_preview_does_not_change_source_matrix() {
    let (state, map_id) = setup();
    let r = analyze_map_smoothing_inner(&map_id, 0.5, None, &state).expect("analysis failed");

    // Original matrix preserved in the report
    assert_eq!(r.original[1][1], 50.0);
    // Preview is the smoothed matrix, centre becomes 30
    assert_eq!(r.smoothed[1][1], 30.0);
    // They are distinct
    assert_ne!(r.original[1][1], r.smoothed[1][1]);
}

#[test]
fn smoothing_analysis_does_not_mutate_rom_or_definitions() {
    let (state, map_id) = setup();

    let rom_before = {
        let g = state.memory.lock().unwrap();
        g.as_ref().unwrap().hashes().clone()
    };
    let maps_before = {
        let g = state.maps.lock().unwrap();
        g.clone()
    };

    let _ = analyze_map_smoothing_inner(&map_id, 0.5, None, &state).expect("analysis failed");

    let rom_after = {
        let g = state.memory.lock().unwrap();
        g.as_ref().unwrap().hashes().clone()
    };
    let maps_after = {
        let g = state.maps.lock().unwrap();
        g.clone()
    };

    assert_eq!(rom_before, rom_after, "ROM must not mutate");
    assert_eq!(maps_before, maps_after, "definitions must not mutate");
}

#[test]
fn smoothing_analysis_rejects_invalid_alpha() {
    let (state, map_id) = setup();
    let err = analyze_map_smoothing_inner(&map_id, 1.5, None, &state).expect_err("alpha must fail");
    assert!(
        err.contains("Alpha smoothing factor"),
        "unexpected error: {}",
        err
    );
}

#[test]
fn smoothing_analysis_requires_loaded_binary() {
    let state = AppState::default();
    let err = analyze_map_smoothing_inner("map_4000", 0.5, None, &state).expect_err("must fail");
    assert_eq!(err, "No binary loaded");
}

#[test]
fn smoothing_analysis_unknown_map_is_error() {
    let (state, _) = setup();
    let err = analyze_map_smoothing_inner("nope", 0.5, None, &state).expect_err("must fail");
    assert!(err.contains("Map not found"), "unexpected error: {}", err);
}

#[test]
fn smoothing_analysis_is_deterministic() {
    let (state, map_id) = setup();
    let a = analyze_map_smoothing_inner(&map_id, 0.37, None, &state).expect("analysis failed");
    let b = analyze_map_smoothing_inner(&map_id, 0.37, None, &state).expect("analysis failed");
    assert_eq!(a, b);
}

// ---------------------------------------------------------------------------
// Batch B1 follow-up: the preview must analyze the matrix the editor displays,
// not a possibly-stale copy from AppState.
// ---------------------------------------------------------------------------

#[test]
fn supplied_matrix_overrides_app_state_for_preview() {
    let (state, map_id) = setup();

    // The editor shows an edited matrix that has NOT been written to AppState.
    let edited = vec![
        vec![100.0, 100.0, 100.0],
        vec![100.0, 200.0, 100.0],
        vec![100.0, 100.0, 100.0],
    ];

    let r = analyze_map_smoothing_inner(&map_id, 0.5, Some(&edited), &state)
        .expect("analysis failed");

    // The report must describe the supplied matrix, not the ROM contents
    // (which hold 10/10/10, 10/50/10, 10/10/10).
    assert_eq!(r.original[1][1], 200.0, "must analyze the supplied matrix");
    assert_eq!(r.smoothed[1][1], 150.0, "0.5*200 + 0.5*100");
    assert_eq!(r.max_abs_curvature, 400.0, "100+100+100+100 - 4*200");
}

#[test]
fn absent_matrix_falls_back_to_app_state() {
    let (state, map_id) = setup();
    let r = analyze_map_smoothing_inner(&map_id, 0.5, None, &state).expect("analysis failed");
    assert_eq!(r.original[1][1], 50.0, "must read from the ROM");
    assert_eq!(r.smoothed[1][1], 30.0);
}

#[test]
fn supplied_matrix_is_validated_for_non_finite_values() {
    let (state, map_id) = setup();
    let bad = vec![vec![1.0, f64::NAN], vec![2.0, 3.0]];
    let err = analyze_map_smoothing_inner(&map_id, 0.5, Some(&bad), &state)
        .expect_err("non-finite matrix must be rejected");
    assert!(err.contains("non-finite"), "error: {}", err);
}

#[test]
fn supplied_matrix_rejects_empty() {
    let (state, map_id) = setup();
    let empty: Vec<Vec<f64>> = vec![];
    let err = analyze_map_smoothing_inner(&map_id, 0.5, Some(&empty), &state)
        .expect_err("empty matrix must be rejected");
    assert!(err.contains("empty"), "error: {}", err);
}

#[test]
fn preview_does_not_mutate_app_state_when_matrix_supplied() {
    let (state, map_id) = setup();
    let rom_before = {
        let g = state.memory.lock().unwrap();
        g.as_ref().unwrap().hashes().clone()
    };

    let edited = vec![vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0], vec![7.0, 8.0, 9.0]];
    let _ = analyze_map_smoothing_inner(&map_id, 0.5, Some(&edited), &state).expect("ok");

    let rom_after = {
        let g = state.memory.lock().unwrap();
        g.as_ref().unwrap().hashes().clone()
    };
    assert_eq!(rom_before, rom_after, "preview must not write to the ROM");
}
