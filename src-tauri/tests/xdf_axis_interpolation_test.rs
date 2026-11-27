//! Application-path tests for the XDF axis breakpoint policy.
//!
//! Verifies that `interpolate_map_at_inner` uses **engineering coordinates**
//! when the parser accepts numeric breakpoints, and falls back to index
//! coordinates when the policy rejects them.
//!
//! Expected values follow from the Slice 4 interpolation semantics: out-of-range
//! coordinates clamp, in-range coordinates interpolate bilinearly.

use kivituner::commands::{interpolate_map_at_inner, load_bin_data_inner, parse_xdf_content_inner};
use kivituner::AppState;

/// Builds an XDF whose x axis has `labels` (as `(index, value)` pairs).
fn xdf_with_x_labels(labels: &[(usize, &str)], indexcount: usize) -> String {
    let mut label_xml = String::new();
    for (i, v) in labels {
        label_xml.push_str(&format!(r#"<LABEL index="{}" value="{}" />"#, i, v));
    }
    format!(
        r#"<?xml version="1.0"?>
<XDFFORMAT version="1.60">
  <XDFTABLE uniqueid="0x4000" flags="0x1">
    <title>Fuel Map</title>
    <XDFAXIS id="x">
      <EMBEDDEDDATA mmedelementsizebits="16" mmedmajorstridebits="0" />
      <indexcount>{}</indexcount>
      <units>rpm</units>
      {}
      <MATH equation="X"><VAR id="X"/></MATH>
    </XDFAXIS>
    <XDFAXIS id="y">
      <EMBEDDEDDATA mmedelementsizebits="16" mmedmajorstridebits="0" />
      <indexcount>3</indexcount>
      <units>load</units>
      <MATH equation="X"><VAR id="X"/></MATH>
    </XDFAXIS>
    <XDFAXIS id="z">
      <EMBEDDEDDATA mmedaddress="0x4000" mmedelementsizebits="16"
                    mmedrowcount="3" mmedcolcount="4" mmedmajorstridebits="16" />
      <indexcount>3</indexcount>
      <MATH equation="X"><VAR id="X"/></MATH>
    </XDFAXIS>
  </XDFTABLE>
</XDFFORMAT>"#,
        indexcount, label_xml
    )
}

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

fn setup(xdf: &str) -> (AppState, String) {
    let state = AppState::default();
    load_bin_data_inner(&make_rom(), &state).expect("load failed");
    let maps = parse_xdf_content_inner(xdf, &state).expect("xdf parse failed");
    assert_eq!(maps.len(), 1);
    (state, maps[0].id.clone())
}

#[test]
fn interpolation_uses_engineering_coordinates_when_breakpoints_accepted() {
    // x axis: 1000..4000 rpm at indices 0..3 (strictly increasing, complete).
    let xdf = xdf_with_x_labels(&[(0, "1000"), (1, "2000"), (2, "3000"), (3, "4000")], 4);
    let (state, map_id) = setup(&xdf);

    // 2000 rpm is index 1; y axis has no labels -> index coordinates.
    let exact = interpolate_map_at_inner(&map_id, 2000.0, 1.0, &state).expect("interp");
    assert_eq!(exact.value, 25.0, "2000rpm/row1 must hit cell value 25");
    assert_eq!(exact.x_low, 1);
    assert_eq!(exact.x_high, 2);

    // 2500 rpm sits halfway between 2000 and 3000 -> x_t = 0.5.
    let mid = interpolate_map_at_inner(&map_id, 2500.0, 1.0, &state).expect("interp");
    assert!(
        (mid.x_t - 0.5).abs() < 1e-12,
        "x_t should be 0.5, got {}",
        mid.x_t
    );
    assert_eq!(mid.value, 27.5, "halfway between 25 and 30");
}

#[test]
fn interpolation_falls_back_to_index_coordinates_for_display_labels() {
    // "On"/"Off" is a display label, not a breakpoint -> fallback to indices.
    let xdf = xdf_with_x_labels(&[(0, "On"), (1, "Off")], 4);
    let (state, map_id) = setup(&xdf);

    // With index fallback the axis is 0,1,2,3 regardless of the labels.
    let exact = interpolate_map_at_inner(&map_id, 1.0, 1.0, &state).expect("interp");
    assert_eq!(exact.value, 25.0);
    assert_eq!(exact.x_low, 1);

    // Engineering coordinate 2000 would clamp to the top of a 0..3 axis.
    let clamped = interpolate_map_at_inner(&map_id, 2000.0, 1.0, &state).expect("interp");
    assert!(clamped.x_clamped, "2000 must clamp on a 0..3 index axis");
    assert_eq!(clamped.x_t, 1.0);
}

#[test]
fn interpolation_falls_back_for_constant_placeholder_labels() {
    // Real-world placeholder: every label is "0".
    let xdf = xdf_with_x_labels(&[(0, "0"), (1, "0"), (2, "0"), (3, "0")], 4);
    let (state, map_id) = setup(&xdf);

    let r = interpolate_map_at_inner(&map_id, 1.0, 1.0, &state).expect("interp");
    assert_eq!(r.value, 25.0);
    assert_eq!(r.x_low, 1, "must use index coordinates, not all-zero axis");
}

#[test]
fn descending_axis_is_rejected_and_falls_back_to_indices() {
    // Descending breakpoints are rejected by the policy because
    // `search_interval` assumes an ascending axis (the source engine shares
    // this limitation). The map must therefore use index coordinates.
    let xdf = xdf_with_x_labels(&[(0, "4000"), (1, "3000"), (2, "2000"), (3, "1000")], 4);
    let (state, map_id) = setup(&xdf);

    let r = interpolate_map_at_inner(&map_id, 1.0, 1.0, &state).expect("interp");
    assert_eq!(
        r.x_low, 1,
        "must use index coordinates, not the descending axis"
    );
    assert_eq!(r.value, 25.0);
}

#[test]
fn interpolation_does_not_mutate_rom_or_definitions_with_breakpoints() {
    let xdf = xdf_with_x_labels(&[(0, "1000"), (1, "2000"), (2, "3000"), (3, "4000")], 4);
    let (state, map_id) = setup(&xdf);

    let rom_before = {
        let g = state.memory.lock().unwrap();
        g.as_ref().unwrap().hashes().clone()
    };
    let maps_before = {
        let g = state.maps.lock().unwrap();
        g.clone()
    };

    let _ = interpolate_map_at_inner(&map_id, 2750.0, 1.5, &state).expect("interp");

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
