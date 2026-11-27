//! B4a: base-address metadata serialization compatibility.
//!
//! Lives here rather than in `kivi-core` because `serde_json` is already a
//! dependency of `kivituner` (src-tauri) and B4a must not add dependencies.
//!
//! The legacy-payload test is the important one: data written before B4a has
//! no `base_address` field, and must still deserialize — as *unknown*, never
//! as zero.

use kivi_core::memory_map::MemoryMap;

#[test]
fn known_base_survives_serde_round_trip() {
    let map = MemoryMap::with_base(vec![1, 2, 3], Some(0x1234));
    let json = serde_json::to_string(&map).expect("serialize");
    let back: MemoryMap = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.base_address(), Some(0x1234));
    assert_eq!(back.as_slice(), &[1, 2, 3]);
}

#[test]
fn unknown_base_survives_serde_round_trip() {
    let map = MemoryMap::new(vec![7]);
    let json = serde_json::to_string(&map).expect("serialize");
    let back: MemoryMap = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.base_address(), None);
}

#[test]
fn known_zero_base_survives_serde_round_trip() {
    let map = MemoryMap::with_base(vec![5], Some(0));
    let json = serde_json::to_string(&map).expect("serialize");
    let back: MemoryMap = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(
        back.base_address(),
        Some(0),
        "known zero must not decay to unknown across a round trip"
    );
}

#[test]
fn legacy_payload_without_base_address_deserializes_as_unknown() {
    // Shape of a payload written before B4a: only `data` and `hashes`.
    let current = MemoryMap::new(vec![1, 2, 3]);
    let mut value: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&current).expect("serialize")).expect("parse");
    let obj = value.as_object_mut().expect("object");
    obj.remove("base_address");

    let back: MemoryMap = serde_json::from_value(value).expect("legacy payload must still load");
    assert_eq!(
        back.base_address(),
        None,
        "legacy data has no address knowledge and must read as unknown"
    );
    assert_eq!(back.as_slice(), &[1, 2, 3]);
}
