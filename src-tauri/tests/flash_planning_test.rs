//! Application-facing tests for differential flash planning and flash target
//! description.
//!
//! These exercise the real kiviTuner code path: the active ROM loaded into
//! `AppState` is compared against a tuned buffer, and the resulting plan is
//! reported through the same serializable shape the frontend receives.
//!
//! No hardware, no transport, no filesystem.

use kivituner::commands::{
    describe_flash_target_inner, load_bin_data_inner, plan_differential_flash_inner,
};
use kivituner::AppState;

#[test]
fn plan_differential_requires_loaded_binary() {
    let state = AppState::default();
    let err = plan_differential_flash_inner(&[0x00; 8], &[4, 4], &state)
        .expect_err("expected error when no binary loaded");
    assert_eq!(err, "No binary loaded");
}

#[test]
fn plan_differential_reports_modified_sectors() {
    let state = AppState::default();
    let stock = vec![0xAA; 16];
    load_bin_data_inner(&stock, &state).expect("load failed");

    let mut tuned = stock.clone();
    tuned[12] = 0xBB; // inside sector 1 (bytes 8..16)

    let report = plan_differential_flash_inner(&tuned, &[8, 8], &state).expect("plan failed");

    assert_eq!(report.sectors.len(), 2);
    assert_eq!(report.sectors[0].sector_index, 0);
    assert!(!report.sectors[0].is_modified);
    assert_eq!(report.sectors[1].sector_index, 1);
    assert!(report.sectors[1].is_modified);
    assert_eq!(report.sectors[1].modified_bytes_count, 1);
    assert_eq!(report.sectors[1].start_address, 8);
    assert_eq!(report.total_bytes_to_flash, 8);
    assert_eq!(report.rom_len, 16);
}

#[test]
fn plan_differential_rejects_length_mismatch() {
    let state = AppState::default();
    load_bin_data_inner(&[0x00; 8], &state).expect("load failed");

    let err = plan_differential_flash_inner(&[0x00; 4], &[8], &state)
        .expect_err("length mismatch must be rejected");
    assert_eq!(
        err,
        "Stock and Tuned ROM sizes must match for differential planning"
    );
}

#[test]
fn plan_differential_all_identical_reports_zero_bytes() {
    let state = AppState::default();
    let rom = vec![0x11; 32];
    load_bin_data_inner(&rom, &state).expect("load failed");

    let report = plan_differential_flash_inner(&rom, &[16, 16], &state).expect("plan failed");
    assert_eq!(report.total_bytes_to_flash, 0);
    assert!(report.sectors.iter().all(|s| !s.is_modified));
}

#[test]
fn plan_differential_does_not_mutate_active_buffer() {
    let state = AppState::default();
    let rom = vec![0x22; 8];
    load_bin_data_inner(&rom, &state).expect("load failed");

    let before = {
        let guard = state.memory.lock().unwrap();
        guard.as_ref().unwrap().hashes().clone()
    };

    let tuned = vec![0x33; 8];
    let _ = plan_differential_flash_inner(&tuned, &[8], &state).expect("plan failed");

    let after = {
        let guard = state.memory.lock().unwrap();
        guard.as_ref().unwrap().hashes().clone()
    };
    assert_eq!(before, after, "planning must not mutate the active ROM");
}

#[test]
fn describe_flash_target_known_chip() {
    let info = describe_flash_target_inner(0x0089_4471).expect("identify failed");
    assert_eq!(info.model_name, "Intel 28F400B 512KiB");
    assert_eq!(info.total_size_bytes, 524_288);
    assert_eq!(info.blocks.len(), 7);
    assert_eq!(info.boot_block_count, 1);
    assert_eq!(info.write_safe_block_count, 6);
}

#[test]
fn describe_flash_target_unknown_chip_is_error() {
    let err = describe_flash_target_inner(0xDEAD_BEEF).expect_err("must reject unknown id");
    assert_eq!(err, "Unknown silicon flash chip ID");
}

#[test]
fn describe_flash_target_amd_top_boot_has_boot_last() {
    let info = describe_flash_target_inner(0x0001_2258).expect("identify failed");
    assert_eq!(info.boot_block_count, 1);
    // Top-boot layout: the protected block is the final entry.
    let last = info.blocks.last().expect("blocks");
    assert_eq!(last.block_type, "Boot");
    assert!(!last.write_safe);
}

#[test]
fn plan_is_deterministic() {
    let state = AppState::default();
    let stock: Vec<u8> = (0..64u8).collect();
    load_bin_data_inner(&stock, &state).expect("load failed");
    let tuned: Vec<u8> = (0..64u8).map(|b| b.wrapping_add(1)).collect();

    let first = plan_differential_flash_inner(&tuned, &[16, 16, 16, 16], &state).expect("plan");
    let second = plan_differential_flash_inner(&tuned, &[16, 16, 16, 16], &state).expect("plan");
    assert_eq!(first, second);
    assert_eq!(first.total_bytes_to_flash, 64);
}
