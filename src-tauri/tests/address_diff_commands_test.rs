//! Application-path tests for the address-safe binary diff.
//!
//! Verifies that the address gate is wired into the Tauri command path and that
//! rejecting a comparison leaves application state untouched.

use kivituner::commands::{
    compare_rom_binaries_with_address_inner, ingest_firmware_inner, load_bin_data_inner,
};
use kivituner::AppState;

fn hex_line(rec: &[u8]) -> String {
    let mut v = rec.to_vec();
    let sum: u8 = v.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    v.push((!sum).wrapping_add(1));
    format!(
        ":{}\n",
        v.iter().map(|b| format!("{:02X}", b)).collect::<String>()
    )
}

/// Builds a HEX file whose data starts at `base` (via extended linear address).
fn hex_at(base: u32, data: &[u8]) -> String {
    let upper = (base >> 16) as u16;
    let offset = (base & 0xFFFF) as u16;
    let ext = hex_line(&[
        0x02,
        0x00,
        0x00,
        0x04,
        (upper >> 8) as u8,
        (upper & 0xFF) as u8,
    ]);
    let mut rec = vec![
        data.len() as u8,
        (offset >> 8) as u8,
        (offset & 0xFF) as u8,
        0x00,
    ];
    rec.extend_from_slice(data);
    format!("{}{}:00000001FF\n", ext, hex_line(&rec))
}

#[test]
fn bin_to_bin_still_compares() {
    let base = vec![0x00, 0x11, 0x22, 0x33];
    let target = vec![0x00, 0x11, 0xFF, 0x33];

    let r = compare_rom_binaries_with_address_inner(&base, None, &target, None, 4)
        .expect("BIN-to-BIN must still work");

    assert_eq!(r.compatibility, "UnknownBases");
    assert_eq!(r.regions.len(), 1);
}

#[test]
fn conflicting_hex_bases_are_rejected() {
    let a = ingest_firmware_inner(hex_at(0x8000, &[0x01, 0x02, 0x03]).as_bytes()).expect("a");
    let b = ingest_firmware_inner(hex_at(0x9000, &[0x01, 0x02, 0x03]).as_bytes()).expect("b");
    assert_eq!(a.base_address, Some(0x8000));
    assert_eq!(b.base_address, Some(0x9000));

    let err = compare_rom_binaries_with_address_inner(
        &a.bytes,
        a.base_address,
        &b.bytes,
        b.base_address,
        4,
    )
    .expect_err("conflicting bases must be rejected");

    assert!(err.contains("0x8000"), "error: {}", err);
    assert!(err.contains("0x9000"), "error: {}", err);
}

#[test]
fn matching_hex_bases_are_compared() {
    let a = ingest_firmware_inner(hex_at(0x8000, &[0x01, 0x02, 0x03]).as_bytes()).expect("a");
    let b = ingest_firmware_inner(hex_at(0x8000, &[0x01, 0xFF, 0x03]).as_bytes()).expect("b");

    let r = compare_rom_binaries_with_address_inner(
        &a.bytes,
        a.base_address,
        &b.bytes,
        b.base_address,
        4,
    )
    .expect("matching bases must be allowed");

    assert!(r.compatibility.starts_with("MatchingBases"));
    assert_eq!(r.regions.len(), 1);
}

#[test]
fn hex_vs_bin_is_rejected_as_unverifiable() {
    let hex = ingest_firmware_inner(hex_at(0x8000, &[0x01, 0x02]).as_bytes()).expect("hex");
    let bin = vec![0x01, 0x02];

    let err =
        compare_rom_binaries_with_address_inner(&hex.bytes, hex.base_address, &bin, None, 4)
            .expect_err("HEX vs BIN must be rejected");

    assert!(
        err.contains("alignment") || err.contains("address"),
        "error: {}",
        err
    );
}

#[test]
fn empty_input_is_rejected() {
    let err = compare_rom_binaries_with_address_inner(&[0x01], None, &[], None, 4)
        .expect_err("empty target must be rejected");
    assert!(err.contains("empty"), "error: {}", err);
}

#[test]
fn rejection_does_not_mutate_app_state() {
    let state = AppState::default();
    load_bin_data_inner(&[0x11; 16], &state).expect("load failed");

    let rom_before = {
        let g = state.memory.lock().unwrap();
        g.as_ref().unwrap().hashes().clone()
    };

    let _ = compare_rom_binaries_with_address_inner(&[0x01], Some(0x100), &[0x02], Some(0x200), 4)
        .expect_err("must reject");

    let rom_after = {
        let g = state.memory.lock().unwrap();
        g.as_ref().unwrap().hashes().clone()
    };
    assert_eq!(rom_before, rom_after, "rejection must not mutate the ROM");
}

#[test]
fn rejection_does_not_mutate_input_slices() {
    let base = vec![0x01, 0x02, 0x03];
    let target = vec![0x04, 0x05, 0x06];
    let base_snap = base.clone();
    let target_snap = target.clone();

    let _ = compare_rom_binaries_with_address_inner(&base, Some(0x100), &target, Some(0x900), 4);

    assert_eq!(base, base_snap);
    assert_eq!(target, target_snap);
}
