//! B4a: optional ROM base-address metadata.
//!
//! These tests pin the semantics required by
//! `docs/migration/2026-10-27-rom-address-design.md`:
//!
//! * `Some(0)` means the base address is **known to be zero**.
//! * `Some(n)` means the base address is **known**.
//! * `None` means the base address is **unknown**, and must never be silently
//!   converted to zero.
//!
//! Expected values are derived from the design specification, not from running
//! the implementation.

use kivi_core::memory_map::MemoryMap;

// ---------------------------------------------------------------------------
// MemoryMap: known vs unknown
// ---------------------------------------------------------------------------

#[test]
fn unknown_base_is_none_not_zero() {
    let map = MemoryMap::new(vec![1, 2, 3, 4]);
    assert_eq!(
        map.base_address(),
        None,
        "a raw buffer carries no address; unknown must not become Some(0)"
    );
}

#[test]
fn known_zero_base_is_distinct_from_unknown() {
    let map = MemoryMap::with_base(vec![1, 2, 3, 4], Some(0));
    assert_eq!(
        map.base_address(),
        Some(0),
        "an explicitly known zero address is not the same as unknown"
    );
    assert_ne!(
        map.base_address(),
        MemoryMap::new(vec![1, 2, 3, 4]).base_address(),
        "known zero and unknown must remain distinguishable"
    );
}

#[test]
fn known_nonzero_base_is_preserved() {
    let map = MemoryMap::with_base(vec![9, 9], Some(0x10000));
    assert_eq!(map.base_address(), Some(0x10000));
}

#[test]
fn base_address_survives_clone() {
    let map = MemoryMap::with_base(vec![1], Some(0x8000));
    let copy = map.clone();
    assert_eq!(copy.base_address(), Some(0x8000));
}

#[test]
fn set_base_address_updates_metadata_without_touching_data() {
    let mut map = MemoryMap::new(vec![1, 2, 3]);
    assert_eq!(map.base_address(), None);

    map.set_base_address(Some(0x4000));
    assert_eq!(map.base_address(), Some(0x4000));
    assert_eq!(map.as_slice(), &[1, 2, 3], "data must be untouched");

    map.set_base_address(None);
    assert_eq!(map.base_address(), None, "clearing must be possible");
}

#[test]
fn rebuilding_from_bytes_does_not_inherit_a_previous_base() {
    // Mirrors the checksum-fix path, which rebuilds the map from mutated bytes.
    let original = MemoryMap::with_base(vec![1, 2, 3], Some(0x8000));
    let rebuilt = MemoryMap::new(original.as_slice().to_vec());
    assert_eq!(
        rebuilt.base_address(),
        None,
        "a rebuilt map must not silently inherit the previous address"
    );
}
