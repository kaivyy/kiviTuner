//! Address-compatibility tests for the read-only binary diff path.
//!
//! ## The ambiguity being fixed
//!
//! `compare_binaries` compares **vector offsets**. It has no notion of physical
//! address. Slice 6A's HEX/SREC ingest reports a `base_address`, but that value
//! was never consulted before comparing, so two inputs covering different
//! physical ranges could be diffed as if they were aligned.
//!
//! ## Policy implemented
//!
//! Comparisons are allowed only when the address ranges can be shown compatible:
//!
//! | Base | Target | Result |
//! |---|---|---|
//! | unknown | unknown | **allowed** (existing BIN behaviour) |
//! | known A | known A | **allowed** |
//! | known A | known B (A != B) | **rejected** — conflicting bases |
//! | unknown | known | **rejected** — cannot establish alignment |
//! | known | unknown | **rejected** — cannot establish alignment |
//!
//! A raw BIN's offset zero is **not** treated as a proven physical address; it
//! is "unknown", which is why mixed comparisons are rejected.
//!
//! Equal bases are necessary but **not sufficient** to prove full address
//! correctness — see the module docs on `diff_with_address_check`.

use kivi_diff::address::{
    diff_with_address_check, AddressCompatibility, AddressRange, DiffRejection,
};
use kivi_diff::engine::compare_binaries;

// ---------------------------------------------------------------------------
// Policy: allowed comparisons
// ---------------------------------------------------------------------------

#[test]
fn bin_to_bin_unknown_bases_remains_allowed() {
    let base = vec![0x00, 0x11, 0x22, 0x33];
    let target = vec![0x00, 0x11, 0xFF, 0x33];

    let out = diff_with_address_check(&base, None, &target, None, 4)
        .expect("BIN-to-BIN must stay allowed");

    assert_eq!(out.compatibility, AddressCompatibility::UnknownBases);
    assert_eq!(out.regions.len(), 1);
    assert_eq!(out.regions[0].start_addr, 2);
}

#[test]
fn matching_known_bases_are_allowed() {
    let base = vec![0xAA, 0xBB, 0xCC];
    let target = vec![0xAA, 0x00, 0xCC];

    let out = diff_with_address_check(&base, Some(0x8000), &target, Some(0x8000), 4)
        .expect("matching bases must be allowed");

    assert_eq!(
        out.compatibility,
        AddressCompatibility::MatchingBases(0x8000)
    );
    assert_eq!(out.regions.len(), 1);
    assert_eq!(out.regions[0].start_addr, 1);
}

#[test]
fn matching_known_bases_with_unequal_lengths_follow_existing_semantics() {
    // compare_binaries treats the extra tail as changed bytes. That existing
    // behaviour is preserved; only the address gate is new.
    let base = vec![0x01, 0x02];
    let target = vec![0x01, 0x02, 0x03, 0x04];

    let out = diff_with_address_check(&base, Some(0x100), &target, Some(0x100), 4)
        .expect("equal bases allowed regardless of length");

    assert_eq!(
        out.compatibility,
        AddressCompatibility::MatchingBases(0x100)
    );
    // The tail beyond min_len is reported as a difference by the existing engine.
    assert!(!out.regions.is_empty(), "extra tail must surface as a diff");
}

// ---------------------------------------------------------------------------
// Policy: rejected comparisons
// ---------------------------------------------------------------------------

#[test]
fn conflicting_known_bases_are_rejected() {
    let base = vec![0x01, 0x02, 0x03];
    let target = vec![0x01, 0x02, 0x03];

    let err = diff_with_address_check(&base, Some(0x8000), &target, Some(0x9000), 4)
        .expect_err("conflicting bases must be rejected");

    assert_eq!(
        err,
        DiffRejection::ConflictingBases {
            base: 0x8000,
            target: 0x9000
        }
    );
}

#[test]
fn unknown_base_on_one_side_is_rejected() {
    let base = vec![0x01, 0x02];
    let target = vec![0x01, 0x02];

    // unknown base vs known base
    let e1 = diff_with_address_check(&base, None, &target, Some(0x8000), 4)
        .expect_err("unknown vs known must be rejected");
    assert_eq!(
        e1,
        DiffRejection::UnverifiableAlignment {
            base: None,
            target: Some(0x8000)
        }
    );

    // known base vs unknown base
    let e2 = diff_with_address_check(&base, Some(0x8000), &target, None, 4)
        .expect_err("known vs unknown must be rejected");
    assert_eq!(
        e2,
        DiffRejection::UnverifiableAlignment {
            base: Some(0x8000),
            target: None
        }
    );
}

// ---------------------------------------------------------------------------
// Empty input
// ---------------------------------------------------------------------------

#[test]
fn empty_target_is_rejected() {
    let base = vec![0x01, 0x02];
    let err = diff_with_address_check(&base, None, &[], None, 4)
        .expect_err("empty target must be rejected");
    assert_eq!(err, DiffRejection::EmptyInput { which: "target" });
}

#[test]
fn empty_base_is_rejected() {
    let target = vec![0x01, 0x02];
    let err = diff_with_address_check(&[], None, &target, None, 4)
        .expect_err("empty base must be rejected");
    assert_eq!(err, DiffRejection::EmptyInput { which: "base" });
}

// ---------------------------------------------------------------------------
// Rejection messages are actionable
// ---------------------------------------------------------------------------

#[test]
fn rejection_messages_are_actionable() {
    let e = DiffRejection::ConflictingBases {
        base: 0x8000,
        target: 0x9000,
    };
    let msg = e.to_string();
    assert!(msg.contains("0x8000"), "message: {}", msg);
    assert!(msg.contains("0x9000"), "message: {}", msg);

    let e2 = DiffRejection::UnverifiableAlignment {
        base: None,
        target: Some(0x8000),
    };
    let msg2 = e2.to_string();
    assert!(
        msg2.contains("address") || msg2.contains("align"),
        "message must explain the problem: {}",
        msg2
    );
}

// ---------------------------------------------------------------------------
// AddressRange helper
// ---------------------------------------------------------------------------

#[test]
fn address_range_covers_expected_span() {
    let r = AddressRange::new(0x8000, 4).expect("range");
    assert_eq!(r.start, 0x8000);
    assert_eq!(r.end_exclusive(), 0x8004);
    assert_eq!(r.len(), 4);
}

#[test]
fn address_range_rejects_overflow() {
    assert!(AddressRange::new(u32::MAX, 2).is_none());
}

// ---------------------------------------------------------------------------
// Non-mutation and determinism
// ---------------------------------------------------------------------------

#[test]
fn diff_does_not_mutate_inputs() {
    let base = vec![0x01, 0x02, 0x03];
    let target = vec![0x01, 0xFF, 0x03];
    let base_snapshot = base.clone();
    let target_snapshot = target.clone();

    let _ = diff_with_address_check(&base, Some(0x100), &target, Some(0x100), 4);

    assert_eq!(base, base_snapshot, "base must not mutate");
    assert_eq!(target, target_snapshot, "target must not mutate");
}

#[test]
fn diff_is_deterministic() {
    let base = vec![0x00; 32];
    let target = vec![0xFF; 32];
    let a = diff_with_address_check(&base, None, &target, None, 4).expect("a");
    let b = diff_with_address_check(&base, None, &target, None, 4).expect("b");
    assert_eq!(a, b);
}

// ---------------------------------------------------------------------------
// The underlying engine is unchanged
// ---------------------------------------------------------------------------

#[test]
fn underlying_compare_binaries_still_matches_existing_behaviour() {
    // Guard: the address gate must not alter the engine itself.
    let base = vec![0x00, 0x11, 0x22];
    let target = vec![0x00, 0xFF, 0x22];
    let direct = compare_binaries(&base, &target, 4);
    let gated = diff_with_address_check(&base, None, &target, None, 4)
        .expect("allowed")
        .regions;
    assert_eq!(direct, gated, "gated path must reuse the same engine");
}
