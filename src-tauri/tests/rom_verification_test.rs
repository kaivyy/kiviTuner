//! Application-facing tests for ROM verification against a reference buffer.
//!
//! These exercise the real kiviTuner code path: `load_bin_data_inner` populates
//! the active `MemoryMap`, then `verify_rom_against_reference_inner` compares it
//! against a caller-supplied reference (typically a saved backup).
//!
//! No hardware, no transport, no filesystem.

use kivituner::commands::{load_bin_data_inner, verify_rom_against_reference_inner};
use kivituner::AppState;

#[test]
fn verify_rom_matches_identical_reference() {
    let state = AppState::default();
    let rom = vec![0x10, 0x20, 0x30, 0x40];
    load_bin_data_inner(&rom, &state).expect("load failed");

    let report = verify_rom_against_reference_inner(&rom, 16, &state).expect("verify failed");

    assert!(report.matches);
    assert_eq!(report.difference_count, 0);
    assert!(report.differences.is_empty());
    assert_eq!(report.active_len, 4);
    assert_eq!(report.reference_len, 4);
    // Reference is not 0xFF-filled.
    assert!(!report.reference_is_blank);
}

#[test]
fn verify_rom_reports_single_difference_with_offset() {
    let state = AppState::default();
    let rom = vec![0x10, 0x20, 0x30, 0x40];
    load_bin_data_inner(&rom, &state).expect("load failed");

    let reference = vec![0x10, 0x20, 0x00, 0x40];
    let report = verify_rom_against_reference_inner(&reference, 16, &state).expect("verify failed");

    assert!(!report.matches);
    assert_eq!(report.difference_count, 1);
    assert_eq!(report.differences[0].offset, 2);
    assert_eq!(report.differences[0].expected_local_byte, 0x30);
    assert_eq!(report.differences[0].actual_chip_byte, 0x00);
}

#[test]
fn verify_rom_respects_difference_cap() {
    let state = AppState::default();
    let rom = vec![0x00; 8];
    load_bin_data_inner(&rom, &state).expect("load failed");

    let reference = vec![0xFF; 8];
    let report = verify_rom_against_reference_inner(&reference, 3, &state).expect("verify failed");

    assert!(!report.matches);
    assert_eq!(report.difference_count, 3);
    assert_eq!(report.differences.len(), 3);
    // Differences are reported in ascending offset order.
    assert_eq!(report.differences[0].offset, 0);
    assert_eq!(report.differences[2].offset, 2);
}

#[test]
fn verify_rom_detects_length_mismatch_via_synthetic_difference() {
    let state = AppState::default();
    let rom = vec![0x01, 0x02];
    load_bin_data_inner(&rom, &state).expect("load failed");

    // Longer reference with identical compared prefix.
    let reference = vec![0x01, 0x02, 0x03];
    let report = verify_rom_against_reference_inner(&reference, 16, &state).expect("verify failed");

    assert!(!report.matches);
    assert_eq!(report.active_len, 2);
    assert_eq!(report.reference_len, 3);
    assert_eq!(report.difference_count, 1);
    assert_eq!(report.differences[0].offset, 2);
}

#[test]
fn verify_rom_flags_blank_reference() {
    let state = AppState::default();
    let rom = vec![0xFF; 16];
    load_bin_data_inner(&rom, &state).expect("load failed");

    let reference = vec![0xFF; 16];
    let report = verify_rom_against_reference_inner(&reference, 16, &state).expect("verify failed");

    assert!(report.matches);
    assert!(report.reference_is_blank);
}

#[test]
fn verify_rom_requires_loaded_binary() {
    let state = AppState::default();
    let err = verify_rom_against_reference_inner(&[0x00], 16, &state)
        .expect_err("expected error when no binary loaded");
    assert_eq!(err, "No binary loaded");
}

#[test]
fn verify_rom_does_not_mutate_active_buffer() {
    let state = AppState::default();
    let rom = vec![0xAA, 0xBB, 0xCC];
    load_bin_data_inner(&rom, &state).expect("load failed");

    let before = {
        let guard = state.memory.lock().unwrap();
        guard.as_ref().unwrap().hashes().clone()
    };

    let reference = vec![0x00, 0x00, 0x00];
    let _ = verify_rom_against_reference_inner(&reference, 16, &state).expect("verify failed");

    let after = {
        let guard = state.memory.lock().unwrap();
        guard.as_ref().unwrap().hashes().clone()
    };

    assert_eq!(before, after, "verification must not mutate the active ROM");
}

#[test]
fn verify_rom_is_deterministic() {
    let state = AppState::default();
    let rom: Vec<u8> = (0..=255u8).collect();
    load_bin_data_inner(&rom, &state).expect("load failed");

    let reference: Vec<u8> = (0..=255u8).rev().collect();
    let first = verify_rom_against_reference_inner(&reference, 8, &state).expect("verify failed");
    let second = verify_rom_against_reference_inner(&reference, 8, &state).expect("verify failed");

    assert_eq!(first, second);
    assert_eq!(first.difference_count, 8);
}
