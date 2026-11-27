//! Application-facing tests for interpolation exposed through the Tauri
//! command layer.
//!
//! Verifies the ported algorithms are reachable from a real kiviTuner path and
//! that the calculation is pure: it must not mutate the active ROM, the loaded
//! map definitions, or any other `AppState` field.
//!
//! Expected values match the oracle-derived fixtures in
//! `crates/kivi-defs/tests/interp_parity_test.rs`.

use kivituner::commands::{interpolate_map_at_inner, load_bin_data_inner, parse_xdf_content_inner};
use kivituner::AppState;

const XDF_2D: &str = r#"<?xml version="1.0"?>
<XDFFORMAT version="1.60">
  <XDFHEADER><deflist><description>t</description></deflist></XDFHEADER>
  <XDFTABLE uniqueid="0x4000" flags="0x1">
    <title>Fuel Map</title>
    <XDFAXIS id="x">
      <EMBEDDEDDATA mmedelementsizebits="16" mmedmajorstridebits="0" />
      <indexcount>4</indexcount>
      <units>rpm</units>
      <MATH equation="X"><VAR id="X"/></MATH>
      <LABEL index="0" value="1000" />
      <LABEL index="1" value="2000" />
      <LABEL index="2" value="3000" />
      <LABEL index="3" value="4000" />
    </XDFAXIS>
    <XDFAXIS id="y">
      <EMBEDDEDDATA mmedelementsizebits="16" mmedmajorstridebits="0" />
      <indexcount>3</indexcount>
      <units>load</units>
      <MATH equation="X"><VAR id="X"/></MATH>
      <LABEL index="0" value="20" />
      <LABEL index="1" value="40" />
      <LABEL index="2" value="60" />
    </XDFAXIS>
    <XDFAXIS id="z">
      <EMBEDDEDDATA mmedaddress="0x4000" mmedelementsizebits="16"
                    mmedrowcount="3" mmedcolcount="4" mmedmajorstridebits="16" />
      <indexcount>3</indexcount>
      <MATH equation="X"><VAR id="X"/></MATH>
    </XDFAXIS>
  </XDFTABLE>
</XDFFORMAT>"#;

fn make_rom() -> Vec<u8> {
    // 12 cells, 16-bit big-endian, row-major: 10,15,20,25 / 20,25,30,35 / 30,35,40,45
    let values: [u16; 12] = [10, 15, 20, 25, 20, 25, 30, 35, 30, 35, 40, 45];
    let mut rom = vec![0u8; 0x4000 + 24];
    for (i, v) in values.iter().enumerate() {
        let off = 0x4000 + i * 2;
        rom[off] = (v >> 8) as u8;
        rom[off + 1] = (v & 0xFF) as u8;
    }
    rom
}

// NOTE: `kivi-defs::xdf` currently parses `<LABEL>` entries but discards them:
// Slice 5A added the conservative XDF axis breakpoint policy, so this fixture's
// `<LABEL>` values are now accepted as physical breakpoints:
//   x axis: 1000, 2000, 3000, 4000 rpm
//   y axis: 20, 40, 60 load
// Coordinates below are therefore engineering units, not cell indices.

fn setup() -> (AppState, String) {
    let state = AppState::default();
    load_bin_data_inner(&make_rom(), &state).expect("load failed");
    let maps = parse_xdf_content_inner(XDF_2D, &state).expect("xdf parse failed");
    assert_eq!(maps.len(), 1);
    let id = maps[0].id.clone();
    assert_eq!(maps[0].rows, 3);
    assert_eq!(maps[0].cols, 4);
    // Slice 5A: breakpoints must be present for this fixture.
    assert_eq!(
        maps[0].x_axis.as_ref().unwrap().values,
        vec![1000.0, 2000.0, 3000.0, 4000.0]
    );
    assert_eq!(
        maps[0].y_axis.as_ref().unwrap().values,
        vec![20.0, 40.0, 60.0]
    );
    (state, id)
}

#[test]
fn interpolate_map_exact_grid_point() {
    let (state, map_id) = setup();
    // Engineering (2000 rpm, 40 load) is cell (col 1, row 1) -> value 25.
    let r = interpolate_map_at_inner(&map_id, 2000.0, 40.0, &state).expect("interp failed");
    assert_eq!(r.value, 25.0);
    assert_eq!(r.x_low, 1);
    assert_eq!(r.x_high, 2);
    assert_eq!(r.y_low, 1);
    assert_eq!(r.y_high, 2);
    assert!(!r.x_clamped);
    assert!(!r.y_clamped);
}

#[test]
fn interpolate_map_midpoint() {
    let (state, map_id) = setup();
    // (2500 rpm, 50 load) is the centre of cells 25/30/35/40 -> 32.5.
    let r = interpolate_map_at_inner(&map_id, 2500.0, 50.0, &state).expect("interp failed");
    assert_eq!(r.value, 32.5);
    assert_eq!(r.x_t, 0.5);
    assert_eq!(r.y_t, 0.5);
}

#[test]
fn interpolate_map_clamps_out_of_range() {
    let (state, map_id) = setup();
    // Below the physical axis minimum (1000 rpm / 20 load).
    let below = interpolate_map_at_inner(&map_id, 0.0, 0.0, &state).expect("interp failed");
    assert_eq!(below.value, 10.0);
    assert!(below.x_clamped);
    assert!(below.y_clamped);

    // Above the physical axis maximum (4000 rpm / 60 load).
    let above = interpolate_map_at_inner(&map_id, 9999.0, 9999.0, &state).expect("interp failed");
    assert_eq!(above.value, 45.0);
    assert!(above.x_clamped);
    assert!(above.y_clamped);
}

#[test]
fn interpolate_map_unknown_map_is_error() {
    let (state, _) = setup();
    let err = interpolate_map_at_inner("does_not_exist", 2000.0, 40.0, &state)
        .expect_err("unknown map must fail");
    assert!(err.contains("Map not found"), "unexpected error: {}", err);
}

#[test]
fn interpolate_map_requires_loaded_binary() {
    let state = AppState::default();
    let err = interpolate_map_at_inner("map_4000", 2000.0, 40.0, &state)
        .expect_err("must fail without ROM");
    assert_eq!(err, "No binary loaded");
}

#[test]
fn interpolate_map_does_not_mutate_rom_or_definitions() {
    let (state, map_id) = setup();

    let rom_before = {
        let g = state.memory.lock().unwrap();
        g.as_ref().unwrap().hashes().clone()
    };
    let maps_before = {
        let g = state.maps.lock().unwrap();
        g.clone()
    };

    let _ = interpolate_map_at_inner(&map_id, 2750.0, 45.0, &state).expect("interp failed");

    let rom_after = {
        let g = state.memory.lock().unwrap();
        g.as_ref().unwrap().hashes().clone()
    };
    let maps_after = {
        let g = state.maps.lock().unwrap();
        g.clone()
    };

    assert_eq!(
        rom_before, rom_after,
        "interpolation must not mutate the ROM"
    );
    assert_eq!(
        maps_before, maps_after,
        "interpolation must not mutate map definitions"
    );
}

#[test]
fn interpolate_map_is_deterministic() {
    let (state, map_id) = setup();
    let a = interpolate_map_at_inner(&map_id, 2750.0, 45.0, &state).expect("interp failed");
    let b = interpolate_map_at_inner(&map_id, 2750.0, 45.0, &state).expect("interp failed");
    assert_eq!(a, b);
}
