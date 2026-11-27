//! Regression tests for the conservative XDF axis breakpoint policy.
//!
//! These exercise the **production parser** (`kivi_defs::xdf::parse_xdf_full`),
//! not a duplicate of the policy.
//!
//! ## Policy under test (kiviTuner compatibility policy, not universal XDF semantics)
//!
//! Axis `values` are populated from `<LABEL>` only when ALL hold:
//! 1. label count == axis point count
//! 2. every label has an explicit `index` and `value`
//! 3. indices are exactly `0..n-1` (no duplicates, no gaps)
//! 4. every value parses as a finite `f64`
//! 5. values are strictly increasing OR strictly decreasing
//! 6. n >= 2 and values are not all identical
//!
//! Otherwise `values` stays empty and callers fall back to index coordinates.

use kivi_defs::model::MapDef;
use kivi_defs::xdf::parse_xdf_full;

/// Wraps axis `<XDFAXIS>` fragments into a single 2D table and returns the map.
fn parse_axes(x_axis_xml: &str, y_axis_xml: &str) -> MapDef {
    let xml = format!(
        r#"<XDFFORMAT version="1.50">
  <XDFTABLE uniqueid="0x100" flags="0x0">
    <title>Test Map</title>
    <XDFAXIS id="x">{}</XDFAXIS>
    <XDFAXIS id="y">{}</XDFAXIS>
    <XDFAXIS id="z">
      <EMBEDDEDDATA mmedaddress="0x3000" mmedelementsizebits="8"
                    mmedrowcount="2" mmedcolcount="2" />
    </XDFAXIS>
  </XDFTABLE>
</XDFFORMAT>"#,
        x_axis_xml, y_axis_xml
    );
    let defs = parse_xdf_full(&xml).expect("parse failed");
    assert_eq!(defs.maps.len(), 1, "expected exactly one map");
    defs.maps.into_iter().next().unwrap()
}

fn labels(values: &[&str]) -> String {
    let mut s = String::new();
    for (i, v) in values.iter().enumerate() {
        s.push_str(&format!(r#"<LABEL index="{}" value="{}" />"#, i, v));
    }
    s
}

fn axis_with(indexcount: usize, label_xml: &str) -> String {
    format!(
        r#"<EMBEDDEDDATA mmedelementsizebits="8" mmedmajorstridebits="-32" />
           <indexcount>{}</indexcount>{}"#,
        indexcount, label_xml
    )
}

fn no_labels_axis(indexcount: usize) -> String {
    format!(
        r#"<EMBEDDEDDATA mmedelementsizebits="8" mmedmajorstridebits="-32" />
           <indexcount>{}</indexcount>"#,
        indexcount
    )
}

// ---------------------------------------------------------------------------
// ACCEPTED cases
// ---------------------------------------------------------------------------

#[test]
fn accepts_complete_strictly_increasing_labels() {
    let map = parse_axes(
        &axis_with(4, &labels(&["1000", "2000", "3000", "4000"])),
        &no_labels_axis(2),
    );
    let x = map.x_axis.as_ref().expect("x axis");
    assert_eq!(x.values, vec![1000.0, 2000.0, 3000.0, 4000.0]);
}

#[test]
fn rejects_strictly_decreasing_labels() {
    // `search_interval` assumes ascending breakpoints. The source engine has
    // the same limitation (verified by direct probe), so accepting a
    // descending axis would produce silently wrong interpolation.
    let map = parse_axes(
        &axis_with(3, &labels(&["900", "500", "100"])),
        &no_labels_axis(2),
    );
    let x = map.x_axis.as_ref().expect("x axis");
    assert!(
        x.values.is_empty(),
        "descending axis must keep fallback (search_interval assumes ascending)"
    );
}

#[test]
fn accepts_fractional_and_negative_values() {
    let map = parse_axes(
        &axis_with(3, &labels(&["-10.5", "0", "12.25"])),
        &no_labels_axis(2),
    );
    let x = map.x_axis.as_ref().expect("x axis");
    assert_eq!(x.values, vec![-10.5, 0.0, 12.25]);
}

#[test]
fn accepts_two_point_axis() {
    let map = parse_axes(&axis_with(2, &labels(&["0", "1"])), &no_labels_axis(2));
    let x = map.x_axis.as_ref().expect("x axis");
    assert_eq!(x.values, vec![0.0, 1.0]);
}

#[test]
fn label_index_maps_to_axis_position_in_order() {
    // Labels are emitted out of document order but indices are 0..n-1;
    // the parser must place each value at its index position, not append.
    let scrambled = r#"<LABEL index="2" value="300" /><LABEL index="0" value="100" /><LABEL index="1" value="200" />"#;
    let map = parse_axes(&axis_with(3, scrambled), &no_labels_axis(2));
    let x = map.x_axis.as_ref().expect("x axis");
    assert_eq!(x.values, vec![100.0, 200.0, 300.0]);
}

// ---------------------------------------------------------------------------
// REJECTED cases -> fallback (empty values)
// ---------------------------------------------------------------------------

#[test]
fn rejects_missing_labels() {
    let map = parse_axes(&no_labels_axis(4), &no_labels_axis(2));
    let x = map.x_axis.as_ref().expect("x axis");
    assert!(x.values.is_empty(), "no labels must keep fallback");
}

#[test]
fn rejects_incomplete_labels() {
    // 3 labels for indexcount 4
    let map = parse_axes(
        &axis_with(4, &labels(&["10", "20", "30"])),
        &no_labels_axis(2),
    );
    let x = map.x_axis.as_ref().expect("x axis");
    assert!(x.values.is_empty(), "incomplete labels must keep fallback");
}

#[test]
fn rejects_duplicate_indices() {
    let dup = r#"<LABEL index="0" value="10" /><LABEL index="0" value="20" /><LABEL index="1" value="30" />"#;
    let map = parse_axes(&axis_with(3, dup), &no_labels_axis(2));
    let x = map.x_axis.as_ref().expect("x axis");
    assert!(x.values.is_empty(), "duplicate indices must keep fallback");
}

#[test]
fn rejects_out_of_range_indices() {
    let bad = r#"<LABEL index="0" value="10" /><LABEL index="1" value="20" /><LABEL index="9" value="30" />"#;
    let map = parse_axes(&axis_with(3, bad), &no_labels_axis(2));
    let x = map.x_axis.as_ref().expect("x axis");
    assert!(x.values.is_empty(), "out-of-range index must keep fallback");
}

#[test]
fn rejects_index_gap() {
    let gap = r#"<LABEL index="0" value="10" /><LABEL index="2" value="30" /><LABEL index="3" value="40" />"#;
    let map = parse_axes(&axis_with(3, gap), &no_labels_axis(2));
    let x = map.x_axis.as_ref().expect("x axis");
    assert!(x.values.is_empty(), "index gap must keep fallback");
}

#[test]
fn rejects_duplicate_values() {
    let map = parse_axes(
        &axis_with(3, &labels(&["10", "10", "30"])),
        &no_labels_axis(2),
    );
    let x = map.x_axis.as_ref().expect("x axis");
    assert!(x.values.is_empty(), "duplicate values must keep fallback");
}

#[test]
fn rejects_constant_values() {
    // The dominant real-world placeholder pattern: all "0".
    let map = parse_axes(
        &axis_with(5, &labels(&["0", "0", "0", "0", "0"])),
        &no_labels_axis(2),
    );
    let x = map.x_axis.as_ref().expect("x axis");
    assert!(x.values.is_empty(), "constant axis must keep fallback");
}

#[test]
fn rejects_non_monotonic_values() {
    let map = parse_axes(
        &axis_with(4, &labels(&["0", "100", "50", "200"])),
        &no_labels_axis(2),
    );
    let x = map.x_axis.as_ref().expect("x axis");
    assert!(x.values.is_empty(), "non-monotonic must keep fallback");
}

#[test]
fn rejects_nan_strings() {
    let map = parse_axes(&axis_with(2, &labels(&["NaN", "100"])), &no_labels_axis(2));
    let x = map.x_axis.as_ref().expect("x axis");
    assert!(x.values.is_empty(), "NaN must keep fallback");
}

#[test]
fn rejects_infinity_strings() {
    let map = parse_axes(&axis_with(2, &labels(&["inf", "100"])), &no_labels_axis(2));
    let x = map.x_axis.as_ref().expect("x axis");
    assert!(x.values.is_empty(), "inf must keep fallback");
}

#[test]
fn rejects_non_numeric_display_labels() {
    let map = parse_axes(&axis_with(2, &labels(&["On", "Off"])), &no_labels_axis(2));
    let x = map.x_axis.as_ref().expect("x axis");
    assert!(x.values.is_empty(), "display labels must keep fallback");
}

#[test]
fn rejects_percent_formatted_labels() {
    let map = parse_axes(
        &axis_with(3, &labels(&["0.0%", "0.5%", "1.0%"])),
        &no_labels_axis(2),
    );
    let x = map.x_axis.as_ref().expect("x axis");
    assert!(x.values.is_empty(), "percent labels must keep fallback");
}

#[test]
fn rejects_empty_string_labels() {
    let map = parse_axes(&axis_with(2, &labels(&["", ""])), &no_labels_axis(2));
    let x = map.x_axis.as_ref().expect("x axis");
    assert!(x.values.is_empty(), "empty labels must keep fallback");
}

#[test]
fn rejects_one_point_axis() {
    let map = parse_axes(&axis_with(1, &labels(&["5"])), &no_labels_axis(2));
    let x = map.x_axis.as_ref().expect("x axis");
    assert!(x.values.is_empty(), "one-point axis must keep fallback");
}

#[test]
fn rejects_when_indexcount_absent_even_if_labels_look_valid() {
    // Without indexcount there is no authoritative point count; the policy
    // requires an exact match against a known count.
    let x = format!(
        r#"<EMBEDDEDDATA mmedelementsizebits="8" mmedmajorstridebits="-32" />{}"#,
        labels(&["10", "20", "30"])
    );
    let map = parse_axes(&x, &no_labels_axis(2));
    let xa = map.x_axis.as_ref().expect("x axis");
    assert!(
        xa.values.is_empty(),
        "missing indexcount must keep fallback"
    );
}

// ---------------------------------------------------------------------------
// Independence: x and y are evaluated separately
// ---------------------------------------------------------------------------

#[test]
fn x_and_y_axes_are_evaluated_independently() {
    let map = parse_axes(
        &axis_with(3, &labels(&["100", "200", "300"])),
        &axis_with(2, &labels(&["On", "Off"])),
    );
    let x = map.x_axis.as_ref().expect("x axis");
    let y = map.y_axis.as_ref().expect("y axis");
    assert_eq!(x.values, vec![100.0, 200.0, 300.0], "x must be accepted");
    assert!(y.values.is_empty(), "y display labels must be rejected");
}

// ---------------------------------------------------------------------------
// JSON compatibility
// ---------------------------------------------------------------------------

#[test]
fn json_roundtrip_preserves_breakpoints() {
    let map = parse_axes(
        &axis_with(3, &labels(&["10", "20", "30"])),
        &no_labels_axis(2),
    );
    let json =
        kivi_defs::kivi_json::maps_to_json(std::slice::from_ref(&map)).expect("serialize failed");
    let back = kivi_defs::kivi_json::maps_from_json(&json).expect("deserialize failed");
    assert_eq!(back.len(), 1);
    assert_eq!(
        back[0].x_axis.as_ref().unwrap().values,
        vec![10.0, 20.0, 30.0]
    );
}

#[test]
fn json_roundtrip_preserves_empty_fallback() {
    let map = parse_axes(&axis_with(2, &labels(&["On", "Off"])), &no_labels_axis(2));
    let json =
        kivi_defs::kivi_json::maps_to_json(std::slice::from_ref(&map)).expect("serialize failed");
    let back = kivi_defs::kivi_json::maps_from_json(&json).expect("deserialize failed");
    assert!(
        back[0].x_axis.as_ref().unwrap().values.is_empty(),
        "empty values must survive JSON round-trip"
    );
}
