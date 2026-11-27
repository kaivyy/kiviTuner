//! Integration tests for Phase 2-3 workspace commands:
//! declarative custom checksum plugins, calibration patch/transfer scripts with Undo,
//! and Stock ROM SHA-256 + signature registry verification.

use kivi_checksum::custom_plugin::sample_custom_checksum_plugin_json;
use kivi_scanner::calibration_script::sample_calibration_script_json;
use kivituner::commands::{
    apply_calibration_script_workspace_inner, dry_run_calibration_script_inner,
    load_bin_data_inner, match_stock_rom_registry_inner, repair_custom_checksum_plugin_inner,
    verify_custom_checksum_plugin_inner, AppState,
};

#[test]
fn custom_checksum_plugin_workspace_ipc_verifies_repairs_and_pushes_undo() {
    let state = AppState::default();
    load_bin_data_inner(&vec![0x22u8; 256], &state).expect("load rom");

    let plugin = sample_custom_checksum_plugin_json();
    let initial = verify_custom_checksum_plugin_inner(&plugin, None, &state).expect("verify");
    assert!(!initial.all_valid);

    let repaired =
        repair_custom_checksum_plugin_inner(&plugin, true, None, &state).expect("repair");
    assert!(repaired.report.all_valid);
    assert!(repaired.sha256.is_some());

    let reverified =
        verify_custom_checksum_plugin_inner(&plugin, None, &state).expect("reverify");
    assert!(reverified.all_valid);
    assert!(state.undo.lock().unwrap().status().can_undo);
}

#[test]
fn calibration_script_workspace_ipc_dry_runs_and_applies_atomically_with_undo() {
    let state = AppState::default();
    let mut rom = vec![0x00u8; 256];
    rom[16..22].copy_from_slice(&[0xAA, 0xBB, 0x77, 0xDD, 0x1E, 0x00]);
    rom[48..56].copy_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF, 10, 20, 30, 40]);
    load_bin_data_inner(&rom, &state).expect("load rom");

    let script = sample_calibration_script_json();
    let dry = dry_run_calibration_script_inner(&script, None, &state).expect("dry run");
    assert!(dry.can_apply);
    assert_eq!(dry.total_bytes_changed, 5);

    let applied = apply_calibration_script_workspace_inner(&script, &state).expect("apply");
    assert!(applied.can_undo);
    assert_eq!(applied.dry_run.total_bytes_changed, 5);

    let mem = state.memory.lock().unwrap();
    let bytes = mem.as_ref().unwrap().as_slice();
    assert_eq!(&bytes[20..22], &[0x22, 0x00]);
    assert_eq!(&bytes[52..56], &[12, 22, 34, 44]);
}

#[test]
fn stock_rom_registry_ipc_matches_active_workspace_sha256_and_identity() {
    let state = AppState::default();
    let mut rom = vec![0x11u8; 512];
    rom[32..40].copy_from_slice(b"K64J108 ");
    load_bin_data_inner(&rom, &state).expect("load rom");

    let initial = match_stock_rom_registry_inner(None, None, &state).expect("builtin match");
    assert_eq!(initial.rom_size, 512);
    assert!(initial.identity_tokens.iter().any(|t| t.contains("K64J108")));

    let pack_json = serde_json::json!({
        "schema_version": 1,
        "pack_name": "Bench Stock Pack",
        "entries": [{
            "id": "k64_test",
            "label": "Bench K64",
            "manufacturer": "Honda",
            "ecu_family": "Keihin",
            "sw_version": "K64J108",
            "size_bytes": 512,
            "sha256": initial.sha256,
            "identity_tokens": ["K64J108"],
            "map_signatures": []
        }]
    })
    .to_string();

    let exact =
        match_stock_rom_registry_inner(Some(&pack_json), None, &state).expect("exact match");
    assert_eq!(
        format!("{:?}", exact.integrity_status),
        "ExactStockMatch"
    );
}
