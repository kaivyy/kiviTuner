//! Parity tests for the limiter scanner.
//!
//! Ported from `ecu_engine_rust::recognition::zero_xdf_discovery::ZeroXdfEngine::scan_limiters`
//! and `DiscoveredLimiter`.
//!
//! ## What this scanner actually does (read from the source, not inferred)
//!
//! **Bike branch** (`is_bike = true`) — genuinely general: scans kept ranges in
//! 2-byte steps for an adjacent soft/hard cut pair where
//! `w1 ∈ [32000, 35000]`, `w2 ∈ [37000, 40000]` and `w2 > w1 + 3000`.
//! There is also a hardcoded special case at offset `0x172EA6`.
//!
//! **Automotive branch** (`is_bike = false`) — **ROM-specific**: it matches only
//! when the value *and* the exact address both hit, e.g. `val == 28000` at
//! `0x1D4EA0` (BMW Bosch MEVD17). On any other ROM it returns nothing.
//!
//! That asymmetry is preserved, not "fixed": widening the automotive branch
//! would invent behaviour the source does not have.
//!
//! ## How expected values were obtained
//!
//! Offsets and RPM conversions are hand-computed:
//! `rpm = raw / 2.56` for bikes, `rpm = raw * 0.25` for automotive.

use kivi_scanner::limiter_scan::{scan_limiters, DiscoveredLimiter};

/// Builds a ROM large enough to survive the 4 KiB pre-filter, with `bytes`
/// written at `offset`.
fn rom_with(offset: usize, bytes: &[u8], len: usize) -> Vec<u8> {
    // A varied, non-blank filler so blocks survive pre-filtering.
    let mut rom: Vec<u8> = (0..len).map(|i| (i % 251) as u8).collect();
    rom[offset..offset + bytes.len()].copy_from_slice(bytes);
    rom
}

fn le(v: u16) -> [u8; 2] {
    v.to_le_bytes()
}

// ---------------------------------------------------------------------------
// Bike branch — general range detection
// ---------------------------------------------------------------------------

#[test]
fn bike_scan_finds_a_soft_and_hard_cut_pair() {
    // w1 = 33678 (0x838E) is inside [32000,35000]; w2 = 38229 inside
    // [37000,40000]; 38229 > 33678 + 3000 = 36678 -> matches.
    let mut pair = Vec::new();
    pair.extend_from_slice(&le(33678));
    pair.extend_from_slice(&le(38229));

    let rom = rom_with(0x2000, &pair, 16384);
    let found = scan_limiters(&rom, true);

    assert_eq!(found.len(), 2, "a matched pair yields two limiter records");
    assert_eq!(found[0].limiter_type, "Soft Cut Rev Limiter");
    assert_eq!(found[1].limiter_type, "Hard Cut Rev Limiter");
    assert_eq!(found[0].raw_value, 33678);
    assert_eq!(found[1].raw_value, 38229);
}

#[test]
fn bike_rpm_conversion_divides_by_2_56() {
    let mut pair = Vec::new();
    pair.extend_from_slice(&le(33678));
    pair.extend_from_slice(&le(38229));
    let rom = rom_with(0x2000, &pair, 16384);

    let found = scan_limiters(&rom, true);
    // 33678 / 2.56 = 13155.46875
    assert!((found[0].converted_rpm - 33678.0 / 2.56).abs() < 1e-9);
    // 38229 / 2.56 = 14933.203125
    assert!((found[1].converted_rpm - 38229.0 / 2.56).abs() < 1e-9);
}

#[test]
fn bike_scan_ignores_values_below_the_soft_cut_window() {
    // w1 = 10000 is outside [32000,35000].
    let mut pair = Vec::new();
    pair.extend_from_slice(&le(10000));
    pair.extend_from_slice(&le(38229));
    let rom = rom_with(0x2000, &pair, 16384);

    assert!(
        scan_limiters(&rom, true).is_empty(),
        "w1 below the window must not match"
    );
}

#[test]
fn bike_scan_requires_the_hard_cut_to_exceed_the_soft_cut_by_3000() {
    // w1 = 34000, w2 = 36500 -> 36500 is not > 34000 + 3000 = 37000.
    let mut pair = Vec::new();
    pair.extend_from_slice(&le(34000));
    pair.extend_from_slice(&le(36500));
    let rom = rom_with(0x2000, &pair, 16384);

    assert!(
        scan_limiters(&rom, true).is_empty(),
        "gap under 3000 must not match"
    );
}

#[test]
fn bike_scan_ignores_a_hard_cut_above_its_window() {
    // w2 = 41000 exceeds the [37000,40000] window.
    let mut pair = Vec::new();
    pair.extend_from_slice(&le(33678));
    pair.extend_from_slice(&le(41000));
    let rom = rom_with(0x2000, &pair, 16384);

    assert!(scan_limiters(&rom, true).is_empty());
}

#[test]
fn bike_scan_reports_adjacent_addresses_two_bytes_apart() {
    let mut pair = Vec::new();
    pair.extend_from_slice(&le(33678));
    pair.extend_from_slice(&le(38229));
    let rom = rom_with(0x2000, &pair, 16384);

    let found = scan_limiters(&rom, true);
    assert_eq!(found[1].address, found[0].address + 2);
}

#[test]
fn bike_scan_is_deterministic() {
    let mut pair = Vec::new();
    pair.extend_from_slice(&le(33678));
    pair.extend_from_slice(&le(38229));
    let rom = rom_with(0x2000, &pair, 16384);

    let a = scan_limiters(&rom, true);
    let b = scan_limiters(&rom, true);
    assert_eq!(a, b);
}

// ---------------------------------------------------------------------------
// Automotive branch — ROM-specific, preserved as-is
// ---------------------------------------------------------------------------

#[test]
fn automotive_scan_matches_only_the_exact_known_address() {
    // The source requires val == 28000 AND offset == 0x1D4EA0.
    let rom_len = 0x1D4EA0 + 64;
    let rom = rom_with(0x1D4EA0, &le(28000), rom_len);

    let found = scan_limiters(&rom, false);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].address, 0x1D4EA0);
    assert_eq!(found[0].raw_value, 28000);
    // 28000 * 0.25 = 7000 RPM
    assert!((found[0].converted_rpm - 7000.0).abs() < 1e-9);
    assert!(found[0].limiter_type.contains("Workshop Test"));
}

#[test]
fn automotive_scan_ignores_the_same_value_at_a_different_address() {
    // Same value, wrong address -> no match. This is the ROM-specificity that
    // is preserved rather than generalised.
    let rom = rom_with(0x2000, &le(28000), 0x1D4EA0 + 64);
    assert!(
        scan_limiters(&rom, false).is_empty(),
        "the automotive branch is address-locked in the source"
    );
}

#[test]
fn automotive_scan_ignores_a_different_value_at_the_known_address() {
    let rom = rom_with(0x1D4EA0, &le(27999), 0x1D4EA0 + 64);
    assert!(scan_limiters(&rom, false).is_empty());
}

#[test]
fn automotive_rpm_conversion_multiplies_by_0_25() {
    let rom = rom_with(0x1D4FB0, &le(32000), 0x1D4FB0 + 64);
    let found = scan_limiters(&rom, false);
    assert_eq!(found.len(), 1);
    // 32000 * 0.25 = 8000 RPM
    assert!((found[0].converted_rpm - 8000.0).abs() < 1e-9);
}

// ---------------------------------------------------------------------------
// Robustness
// ---------------------------------------------------------------------------

#[test]
fn empty_rom_yields_no_limiters() {
    assert!(scan_limiters(&[], true).is_empty());
    assert!(scan_limiters(&[], false).is_empty());
}

#[test]
fn a_tiny_rom_yields_no_limiters() {
    assert!(scan_limiters(&[0u8; 4], true).is_empty());
}

#[test]
fn scanning_does_not_mutate_the_input() {
    let rom = rom_with(0x2000, &le(33678), 16384);
    let copy = rom.clone();
    let _ = scan_limiters(&rom, true);
    assert_eq!(rom, copy);
}

#[test]
fn an_all_blank_rom_is_pruned_and_yields_nothing() {
    // A fully blank ROM is pruned by the pre-filter, so no scanning happens.
    let rom = vec![0xFFu8; 16384];
    assert!(scan_limiters(&rom, true).is_empty());
}

#[test]
fn discovered_limiter_exposes_the_documented_fields() {
    let l = DiscoveredLimiter {
        address: 0x1000,
        raw_value: 33678,
        converted_rpm: 13155.46875,
        limiter_type: "Soft Cut Rev Limiter",
    };
    assert_eq!(l.address, 0x1000);
    assert_eq!(l.raw_value, 33678);
    assert!(l.converted_rpm > 0.0);
    assert!(!l.limiter_type.is_empty());
}
