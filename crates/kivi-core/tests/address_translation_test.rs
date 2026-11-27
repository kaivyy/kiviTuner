//! Parity tests for CPU/raw address translation.
//!
//! Ported from `ecu_engine_rust::math::address_translation`
//! (`MemoryAddressTranslator::cpu_to_raw`, `raw_to_cpu`, `MemorySegmentRule`).
//!
//! ## How expected values were obtained
//!
//! The three default segments are read from the source and the arithmetic is
//! done by hand:
//!
//! | Segment | CPU range | Raw offset |
//! |---|---|---|
//! | TriCore cached | `0x8000_0000..=0x803F_FFFF` | `0x0000_0000` |
//! | TriCore non-cached | `0xA000_0000..=0xA03F_FFFF` | `0x0000_0000` |
//! | RH850 data flash | `0xFF20_0000..=0xFF20_7FFF` | `0x0010_0000` |
//!
//! ## Documented deviation
//!
//! The source returns a bare `Ok(u32)` for **unmapped** addresses, silently
//! applying an identity mapping. The number is preserved here, but the caller is
//! told whether the mapping was explicit: [`Translation::is_identity_fallback`].
//! Silent identity is indistinguishable from a real mapping, and this project
//! does not report unverified alignments as verified.
//!
//! `BusDescrambler` is **not** ported — it is a security-sensitive component and
//! remains blocked.

use kivi_core::address_translation::{
    cpu_to_raw, raw_to_cpu, MemoryAddressTranslator, MemorySegmentRule, Translation,
};

fn default_translator() -> MemoryAddressTranslator {
    MemoryAddressTranslator::default()
}

// ---------------------------------------------------------------------------
// CPU -> raw
// ---------------------------------------------------------------------------

#[test]
fn cached_flash_start_maps_to_offset_zero() {
    let t = default_translator();
    let r = cpu_to_raw(&t, 0x8000_0000).expect("map");
    assert_eq!(r.value, 0x0000_0000);
    assert!(!r.is_identity_fallback);
}

#[test]
fn cached_flash_interior_maps_by_delta() {
    let t = default_translator();
    // 0x8000_1234 - 0x8000_0000 = 0x1234
    let r = cpu_to_raw(&t, 0x8000_1234).expect("map");
    assert_eq!(r.value, 0x0000_1234);
    assert!(!r.is_identity_fallback);
}

#[test]
fn cached_flash_last_byte_maps_to_the_end_of_the_4mb_window() {
    let t = default_translator();
    let r = cpu_to_raw(&t, 0x803F_FFFF).expect("map");
    assert_eq!(r.value, 0x003F_FFFF);
}

#[test]
fn non_cached_flash_shares_the_same_raw_origin() {
    let t = default_translator();
    let r = cpu_to_raw(&t, 0xA000_1000).expect("map");
    assert_eq!(r.value, 0x0000_1000);
}

#[test]
fn rh850_data_flash_uses_its_own_raw_offset() {
    let t = default_translator();
    // 0xFF20_0000 - 0xFF20_0000 = 0, plus raw offset 0x0010_0000
    let r = cpu_to_raw(&t, 0xFF20_0000).expect("map");
    assert_eq!(r.value, 0x0010_0000);
}

#[test]
fn rh850_interior_adds_the_delta_to_the_raw_offset() {
    let t = default_translator();
    let r = cpu_to_raw(&t, 0xFF20_0010).expect("map");
    assert_eq!(r.value, 0x0010_0010);
}

// ---------------------------------------------------------------------------
// Raw -> CPU
// ---------------------------------------------------------------------------

#[test]
fn raw_zero_maps_back_into_the_first_matching_segment() {
    let t = default_translator();
    // The cached segment is first, so raw 0 resolves there.
    let r = raw_to_cpu(&t, 0x0000_0000).expect("map");
    assert_eq!(r.value, 0x8000_0000);
    assert!(!r.is_identity_fallback);
}

#[test]
fn raw_interior_maps_by_delta() {
    let t = default_translator();
    let r = raw_to_cpu(&t, 0x0000_1234).expect("map");
    assert_eq!(r.value, 0x8000_1234);
}

#[test]
fn raw_offset_inside_the_cached_window_uses_the_cached_rule() {
    // SOURCE DEFECT (preserved): the RH850 rule declares raw offset 0x0010_0000,
    // which lies inside the cached window (raw 0x0000_0000..=0x003F_FFFF). The
    // first-match rule therefore resolves it as cached, and the RH850 segment is
    // unreachable through `raw_to_cpu`. Reproduced faithfully rather than
    // "fixed", because silently changing it would alter translation output.
    let t = default_translator();
    let r = raw_to_cpu(&t, 0x0010_0000).expect("map");
    assert_eq!(
        r.value, 0x8010_0000,
        "first matching rule (cached) wins, per source semantics"
    );
    assert!(!r.is_identity_fallback);
}

#[test]
fn raw_beyond_every_window_falls_back_to_identity() {
    // raw 0x0090_0000 is past the cached window and below nothing else.
    let t = default_translator();
    let r = raw_to_cpu(&t, 0x0090_0000).expect("map");
    assert_eq!(r.value, 0x0090_0000);
    assert!(r.is_identity_fallback);
}

// ---------------------------------------------------------------------------
// Round trip
// ---------------------------------------------------------------------------

#[test]
fn cpu_to_raw_then_raw_to_cpu_round_trips_inside_the_cached_window() {
    let t = default_translator();
    for cpu in [0x8000_0000u32, 0x8000_1234, 0x8010_0000, 0x803F_FFFF] {
        let raw = cpu_to_raw(&t, cpu).expect("to raw");
        let back = raw_to_cpu(&t, raw.value).expect("to cpu");
        assert_eq!(back.value, cpu, "round trip failed for {cpu:#010X}");
    }
}

#[test]
fn the_rh850_window_does_not_round_trip_because_its_raw_range_overlaps() {
    // Documented consequence of the source defect above: cpu -> raw works, but
    // raw -> cpu resolves through the cached rule, so the pair is not an inverse.
    // This is asserted so that any future "fix" to the rule set is visible.
    let t = default_translator();
    let cpu = 0xFF20_0000u32;
    let raw = cpu_to_raw(&t, cpu).expect("to raw");
    assert_eq!(raw.value, 0x0010_0000);

    let back = raw_to_cpu(&t, raw.value).expect("to cpu");
    assert_ne!(
        back.value, cpu,
        "the RH850 raw range overlaps the cached window, so it is not invertible"
    );
    assert_eq!(back.value, 0x8010_0000);
}

// ---------------------------------------------------------------------------
// Unmapped addresses
// ---------------------------------------------------------------------------

#[test]
fn an_unmapped_cpu_address_falls_back_to_identity_and_says_so() {
    let t = default_translator();
    let r = cpu_to_raw(&t, 0x1234_5678).expect("map");
    assert_eq!(r.value, 0x1234_5678, "identity value is preserved");
    assert!(
        r.is_identity_fallback,
        "the caller must be told this was not an explicit mapping"
    );
}

#[test]
fn an_unmapped_raw_offset_falls_back_to_identity_and_says_so() {
    let t = default_translator();
    let r = raw_to_cpu(&t, 0x0090_0000).expect("map");
    assert_eq!(r.value, 0x0090_0000);
    assert!(r.is_identity_fallback);
}

#[test]
fn a_mapped_address_is_not_flagged_as_a_fallback() {
    let t = default_translator();
    assert!(!cpu_to_raw(&t, 0x8000_0000).expect("map").is_identity_fallback);
    assert!(!raw_to_cpu(&t, 0x0000_0000).expect("map").is_identity_fallback);
}

// ---------------------------------------------------------------------------
// Boundary behaviour
// ---------------------------------------------------------------------------

#[test]
fn the_byte_just_below_a_segment_start_is_unmapped() {
    let t = default_translator();
    let r = cpu_to_raw(&t, 0x7FFF_FFFF).expect("map");
    assert!(r.is_identity_fallback, "0x7FFFFFFF is below the cached window");
}

#[test]
fn the_byte_just_above_a_segment_end_is_unmapped() {
    let t = default_translator();
    let r = cpu_to_raw(&t, 0x8040_0000).expect("map");
    assert!(r.is_identity_fallback, "0x80400000 is past the cached window");
}

#[test]
fn address_zero_is_a_valid_cpu_address_and_not_treated_as_unknown() {
    let t = default_translator();
    let r = cpu_to_raw(&t, 0).expect("map");
    assert_eq!(r.value, 0);
    assert!(r.is_identity_fallback, "0 is unmapped, not a known mapping");
}

// ---------------------------------------------------------------------------
// Overflow
// ---------------------------------------------------------------------------

#[test]
fn a_rule_that_would_overflow_returns_an_error() {
    let t = MemoryAddressTranslator {
        segments: vec![MemorySegmentRule {
            name: "overflow".to_string(),
            cpu_start_address: 0x8000_0000,
            cpu_end_address: 0x8000_FFFF,
            raw_file_offset: 0xFFFF_FFF0,
        }],
    };
    let err = cpu_to_raw(&t, 0x8000_0100).expect_err("must reject");
    assert!(err.contains("overflow"), "unexpected error: {err}");
}

#[test]
fn a_translator_with_no_segments_is_pure_identity() {
    let t = MemoryAddressTranslator { segments: vec![] };
    let r = cpu_to_raw(&t, 0xDEAD_BEEF).expect("map");
    assert_eq!(r.value, 0xDEAD_BEEF);
    assert!(r.is_identity_fallback);
}

// ---------------------------------------------------------------------------
// Custom rules and ordering
// ---------------------------------------------------------------------------

#[test]
fn the_first_matching_segment_wins() {
    // Two rules covering the same CPU range; the first must win.
    let t = MemoryAddressTranslator {
        segments: vec![
            MemorySegmentRule {
                name: "first".to_string(),
                cpu_start_address: 0x1000,
                cpu_end_address: 0x1FFF,
                raw_file_offset: 0x0000,
            },
            MemorySegmentRule {
                name: "second".to_string(),
                cpu_start_address: 0x1000,
                cpu_end_address: 0x1FFF,
                raw_file_offset: 0x9000,
            },
        ],
    };
    let r = cpu_to_raw(&t, 0x1000).expect("map");
    assert_eq!(r.value, 0x0000, "first rule must take precedence");
}

#[test]
fn a_custom_single_rule_translates_correctly() {
    let t = MemoryAddressTranslator {
        segments: vec![MemorySegmentRule {
            name: "custom".to_string(),
            cpu_start_address: 0x0800_0000,
            cpu_end_address: 0x080F_FFFF,
            raw_file_offset: 0x0002_0000,
        }],
    };
    assert_eq!(cpu_to_raw(&t, 0x0800_0000).expect("map").value, 0x0002_0000);
    assert_eq!(cpu_to_raw(&t, 0x0800_0010).expect("map").value, 0x0002_0010);
}

#[test]
fn translation_is_deterministic() {
    let t = default_translator();
    let a = cpu_to_raw(&t, 0x8000_1234).expect("map");
    let b = cpu_to_raw(&t, 0x8000_1234).expect("map");
    assert_eq!(a, b);
}

#[test]
fn the_translation_result_carries_the_value_and_the_fallback_flag() {
    // Type-level guard: the result must expose both, so a caller cannot
    // accidentally read the value without being able to see the flag.
    let t = default_translator();
    let mapped: Translation = cpu_to_raw(&t, 0x8000_0000).expect("map");
    let fallback: Translation = cpu_to_raw(&t, 0x1).expect("map");
    assert!(!mapped.is_identity_fallback);
    assert!(fallback.is_identity_fallback);
}
