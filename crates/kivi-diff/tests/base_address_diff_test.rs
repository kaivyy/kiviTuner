//! B4a: base-address propagation into the comparison path.
//!
//! The address gate itself (`diff_with_address_check`) already had correct
//! policy before B4a. What B4a adds is a way for a *raw BIN* to carry a known
//! base, and for `HexIngestResult.base_address` to distinguish known-zero from
//! unknown. These tests prove the values reach the gate with the right meaning.
//!
//! Expected values come from the design table in
//! `docs/migration/2026-10-27-rom-address-design.md` §6.

use kivi_core::hex_ingest::ingest_firmware;
use kivi_core::memory_map::MemoryMap;
use kivi_diff::address::{diff_with_address_check, AddressCompatibility, DiffRejection};

fn bytes_a() -> Vec<u8> {
    vec![1, 2, 3, 4, 5, 6, 7, 8]
}

fn bytes_b() -> Vec<u8> {
    vec![1, 2, 9, 4, 5, 6, 7, 8]
}

// ---------------------------------------------------------------------------
// Unknown vs unknown — the documented legacy contract must be preserved
// ---------------------------------------------------------------------------

#[test]
fn unknown_vs_unknown_still_allows_offset_comparison() {
    let base = MemoryMap::new(bytes_a());
    let target = MemoryMap::new(bytes_b());

    let gated = diff_with_address_check(
        base.as_slice(),
        base.base_address(),
        target.as_slice(),
        target.base_address(),
        4,
    )
    .expect("legacy BIN-to-BIN must still work");

    assert_eq!(gated.compatibility, AddressCompatibility::UnknownBases);
    assert!(!gated.regions.is_empty());
}

// ---------------------------------------------------------------------------
// Known vs known
// ---------------------------------------------------------------------------

#[test]
fn matching_known_bases_allow_comparison() {
    let base = MemoryMap::with_base(bytes_a(), Some(0x8000));
    let target = MemoryMap::with_base(bytes_b(), Some(0x8000));

    let gated = diff_with_address_check(
        base.as_slice(),
        base.base_address(),
        target.as_slice(),
        target.base_address(),
        4,
    )
    .expect("matching bases must compare");

    assert_eq!(gated.compatibility, AddressCompatibility::MatchingBases(0x8000));
}

#[test]
fn conflicting_known_bases_are_rejected() {
    let base = MemoryMap::with_base(bytes_a(), Some(0x8000));
    let target = MemoryMap::with_base(bytes_b(), Some(0x9000));

    let err = diff_with_address_check(
        base.as_slice(),
        base.base_address(),
        target.as_slice(),
        target.base_address(),
        4,
    )
    .expect_err("conflicting bases must be rejected");

    assert_eq!(
        err,
        DiffRejection::ConflictingBases {
            base: 0x8000,
            target: 0x9000
        }
    );
}

// ---------------------------------------------------------------------------
// Known zero is a real address, not "unknown"
// ---------------------------------------------------------------------------

#[test]
fn address_zero_vs_address_zero_is_treated_as_known_and_allowed() {
    let base = MemoryMap::with_base(bytes_a(), Some(0));
    let target = MemoryMap::with_base(bytes_b(), Some(0));

    let gated = diff_with_address_check(
        base.as_slice(),
        base.base_address(),
        target.as_slice(),
        target.base_address(),
        4,
    )
    .expect("known zero on both sides must compare");

    assert_eq!(gated.compatibility, AddressCompatibility::MatchingBases(0));
}

#[test]
fn address_zero_vs_unknown_is_unverifiable_not_equal() {
    let base = MemoryMap::with_base(bytes_a(), Some(0));
    let target = MemoryMap::new(bytes_b());

    let err = diff_with_address_check(
        base.as_slice(),
        base.base_address(),
        target.as_slice(),
        target.base_address(),
        4,
    )
    .expect_err("known zero vs unknown must not be treated as equal");

    assert_eq!(
        err,
        DiffRejection::UnverifiableAlignment {
            base: Some(0),
            target: None
        }
    );
}

#[test]
fn unknown_vs_known_is_rejected() {
    let base = MemoryMap::new(bytes_a());
    let target = MemoryMap::with_base(bytes_b(), Some(0x4000));

    let err = diff_with_address_check(
        base.as_slice(),
        base.base_address(),
        target.as_slice(),
        target.base_address(),
        4,
    )
    .expect_err("unknown vs known must be rejected");

    assert_eq!(
        err,
        DiffRejection::UnverifiableAlignment {
            base: None,
            target: Some(0x4000)
        }
    );
}

// ---------------------------------------------------------------------------
// HEX/SREC metadata reaches the gate
// ---------------------------------------------------------------------------

#[test]
fn hex_base_address_flows_into_comparison() {
    // :020000040001F9 -> upper address 0x0001, then data at offset 0x0000.
    let text = ":020000040001F9\n:01000000AA55\n:00000001FF\n";
    let ingested = ingest_firmware(text.as_bytes(), 0xFF).expect("parse");
    assert_eq!(ingested.base_address, Some(0x0001_0000));

    let workspace = MemoryMap::with_base(bytes_a(), Some(0x0001_0000));

    let gated = diff_with_address_check(
        workspace.as_slice(),
        workspace.base_address(),
        &ingested.bytes,
        ingested.base_address,
        4,
    )
    .expect("matching HEX base must compare");

    assert_eq!(
        gated.compatibility,
        AddressCompatibility::MatchingBases(0x0001_0000)
    );
}

#[test]
fn hex_base_mismatch_with_workspace_is_rejected() {
    let text = ":020000040001F9\n:01000000AA55\n:00000001FF\n";
    let ingested = ingest_firmware(text.as_bytes(), 0xFF).expect("parse");

    let workspace = MemoryMap::with_base(bytes_a(), Some(0x0000_0000));

    let err = diff_with_address_check(
        workspace.as_slice(),
        workspace.base_address(),
        &ingested.bytes,
        ingested.base_address,
        4,
    )
    .expect_err("mismatched bases must be rejected");

    assert_eq!(
        err,
        DiffRejection::ConflictingBases {
            base: 0x0000_0000,
            target: 0x0001_0000
        }
    );
}

// ---------------------------------------------------------------------------
// Raw BIN stays unknown
// ---------------------------------------------------------------------------

#[test]
fn raw_bin_against_addressed_hex_is_unverifiable() {
    let bin = ingest_firmware(&[0xAA, 0xBB], 0xFF).expect("bin");
    assert_eq!(bin.base_address, None, "raw BIN has no address");

    let hex_text = ":01000000AA55\n:00000001FF\n";
    let hex = ingest_firmware(hex_text.as_bytes(), 0xFF).expect("hex");
    assert_eq!(hex.base_address, Some(0));

    let err = diff_with_address_check(
        &bin.bytes,
        bin.base_address,
        &hex.bytes,
        hex.base_address,
        4,
    )
    .expect_err("unknown vs known must be rejected");

    assert!(matches!(err, DiffRejection::UnverifiableAlignment { .. }));
}
