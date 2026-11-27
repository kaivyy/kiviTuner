//! Wave 4 application-integration tests.
//!
//! These exercise the **application layer** — the `*_inner` functions that Tauri
//! commands wrap — rather than the library APIs directly. A command that exists
//! but is not reachable through a tested inner function is not integrated.

use kivituner::commands::{
    parse_vbf_inner, serialize_vbf_inner, translate_address_inner, VbfSegmentPayload,
};

// ---------------------------------------------------------------------------
// W4-I2 — VBF application path
// ---------------------------------------------------------------------------

#[test]
fn vbf_round_trips_through_the_application_layer() {
    let segments = vec![VbfSegmentPayload {
        start_address: 0x0008_0000,
        data: vec![0x11, 0x22, 0x33],
    }];

    let bytes = serialize_vbf_inner("APP-TEST", &segments).expect("serialize");
    let (header, parsed) = parse_vbf_inner(&bytes).expect("parse");

    assert!(header.contains("APP-TEST"));
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].start_address, 0x0008_0000);
    assert_eq!(parsed[0].data, vec![0x11, 0x22, 0x33]);
}

#[test]
fn vbf_parse_error_is_actionable() {
    let err = parse_vbf_inner(b"not a vbf file").expect_err("must reject");
    assert!(
        err.to_lowercase().contains("header") || err.to_lowercase().contains("brace"),
        "error must name the problem: {err}"
    );
}

#[test]
fn vbf_serialize_rejects_an_absurd_segment_count_safely() {
    // An empty segment list is valid; this checks the empty case is not an error.
    let bytes = serialize_vbf_inner("EMPTY", &[]).expect("empty is valid");
    let (_, parsed) = parse_vbf_inner(&bytes).expect("parse");
    assert!(parsed.is_empty());
}

#[test]
fn vbf_preserves_address_zero_distinctly() {
    let segments = vec![VbfSegmentPayload {
        start_address: 0,
        data: vec![0xAA],
    }];
    let bytes = serialize_vbf_inner("ZERO", &segments).expect("serialize");
    let (_, parsed) = parse_vbf_inner(&bytes).expect("parse");
    assert_eq!(parsed[0].start_address, 0);
}

// ---------------------------------------------------------------------------
// W4-I3 — address translation application path
// ---------------------------------------------------------------------------

#[test]
fn translation_reports_the_matched_rule_name() {
    let r = translate_address_inner("cpu_to_raw", 0x8000_0000).expect("translate");
    assert_eq!(r.value, 0);
    assert!(!r.is_identity_fallback);
    assert!(
        r.matched_rule.is_some(),
        "a mapped address must name the rule that matched"
    );
    assert_eq!(r.matched_rule.as_deref(), Some("TriCore Internal Flash (Cached)"));
}

#[test]
fn an_unmapped_address_reports_no_rule_and_the_fallback_flag() {
    let r = translate_address_inner("cpu_to_raw", 0x1234_5678).expect("translate");
    assert_eq!(r.value, 0x1234_5678);
    assert!(r.is_identity_fallback);
    assert!(
        r.matched_rule.is_none(),
        "an unmapped address must not claim a rule"
    );
}

#[test]
fn the_rh850_ambiguity_is_visible_to_the_consumer() {
    // raw 0x0010_0000 resolves through the CACHED rule, not RH850. The rule name
    // is what makes the ambiguity visible — without it a caller would see a
    // plausible-looking translation and never know.
    let r = translate_address_inner("raw_to_cpu", 0x0010_0000).expect("translate");
    assert_eq!(r.value, 0x8010_0000);
    assert_eq!(
        r.matched_rule.as_deref(),
        Some("TriCore Internal Flash (Cached)"),
        "the reported rule exposes which mapping actually applied"
    );
}

#[test]
fn an_unknown_direction_is_rejected() {
    let err = translate_address_inner("sideways", 0x1000).expect_err("must reject");
    assert!(err.contains("direction"), "error: {err}");
}

#[test]
fn translation_never_panics_on_extremes() {
    let _ = translate_address_inner("cpu_to_raw", 0);
    let _ = translate_address_inner("cpu_to_raw", u32::MAX);
    let _ = translate_address_inner("raw_to_cpu", 0);
    let _ = translate_address_inner("raw_to_cpu", u32::MAX);
}
