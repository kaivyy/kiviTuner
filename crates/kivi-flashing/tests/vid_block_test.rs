//! Parity tests for VIN / VID block handling.
//!
//! Ported from `ecu_engine_rust::flashing::vid_block`.
//!
//! ## How expected values were obtained
//!
//! VIN validity is defined by ISO 3779: exactly 17 characters, digits or
//! uppercase letters, and **never** `I`, `O` or `Q` (to avoid confusion with 1
//! and 0). Every expectation below is derived from that rule, not from running
//! the implementation.

use kivi_flashing::vid_block::VehicleIdentityBlockManager;

// ---------------------------------------------------------------------------
// ISO 3779 VIN validation
// ---------------------------------------------------------------------------

#[test]
fn a_valid_17_character_vin_is_accepted() {
    // "1HGCM82633A004352" is a real-format Honda VIN: 17 chars, no I/O/Q.
    assert!(VehicleIdentityBlockManager::is_valid_iso3779_vin(b"1HGCM82633A004352"));
}

#[test]
fn a_vin_shorter_or_longer_than_17_is_rejected() {
    assert!(!VehicleIdentityBlockManager::is_valid_iso3779_vin(b"1HGCM82633A00435")); // 16
    assert!(!VehicleIdentityBlockManager::is_valid_iso3779_vin(b"1HGCM82633A0043521")); // 18
    assert!(!VehicleIdentityBlockManager::is_valid_iso3779_vin(b""));
}

#[test]
fn the_letters_i_o_and_q_are_never_valid() {
    // ISO 3779 excludes them so they cannot be confused with 1 and 0.
    assert!(!VehicleIdentityBlockManager::is_valid_iso3779_vin(b"1HGCM82633A00435I"));
    assert!(!VehicleIdentityBlockManager::is_valid_iso3779_vin(b"1HGCM82633A00435O"));
    assert!(!VehicleIdentityBlockManager::is_valid_iso3779_vin(b"1HGCM82633A00435Q"));
}

#[test]
fn lowercase_letters_are_rejected() {
    assert!(!VehicleIdentityBlockManager::is_valid_iso3779_vin(b"1hgcm82633a004352"));
}

#[test]
fn non_alphanumeric_bytes_are_rejected() {
    assert!(!VehicleIdentityBlockManager::is_valid_iso3779_vin(b"1HGCM82633A00435-"));
    assert!(!VehicleIdentityBlockManager::is_valid_iso3779_vin(b"1HGCM82633A00435 "));
    assert!(!VehicleIdentityBlockManager::is_valid_iso3779_vin(b"1HGCM82633A0043\x00\x00"));
}

#[test]
fn an_all_digit_vin_is_accepted() {
    // Digits are always allowed by the rule.
    assert!(VehicleIdentityBlockManager::is_valid_iso3779_vin(b"12345678901234567"));
}

// ---------------------------------------------------------------------------
// locate_vin
// ---------------------------------------------------------------------------

#[test]
fn locate_vin_finds_a_vin_at_offset_zero() {
    let rom = b"1HGCM82633A004352".to_vec();
    let found = VehicleIdentityBlockManager::locate_vin(&rom).expect("must find");
    assert_eq!(found.0, 0);
    assert_eq!(found.1, "1HGCM82633A004352");
}

#[test]
fn locate_vin_finds_a_vin_at_a_nonzero_offset() {
    let mut rom = vec![0xFFu8; 32];
    rom.extend_from_slice(b"1HGCM82633A004352");
    rom.extend_from_slice(&[0x00; 8]);
    let found = VehicleIdentityBlockManager::locate_vin(&rom).expect("must find");
    assert_eq!(found.0, 32, "the reported offset must be the VIN start");
    assert_eq!(found.1, "1HGCM82633A004352");
}

#[test]
fn locate_vin_returns_none_for_a_rom_too_short_to_hold_one() {
    let rom = vec![0x41u8; 16];
    assert!(VehicleIdentityBlockManager::locate_vin(&rom).is_none());
}

#[test]
fn locate_vin_returns_none_when_no_valid_vin_is_present() {
    // 0xFF is not a valid VIN character.
    let rom = vec![0xFFu8; 256];
    assert!(VehicleIdentityBlockManager::locate_vin(&rom).is_none());
}

#[test]
fn locate_vin_returns_the_first_match_when_several_are_present() {
    let mut rom = Vec::new();
    rom.extend_from_slice(b"1HGCM82633A004352");
    rom.push(b'X');
    rom.extend_from_slice(b"2HGCM82633A004353");
    let found = VehicleIdentityBlockManager::locate_vin(&rom).expect("must find");
    assert_eq!(found.0, 0, "the earliest offset wins");
    assert_eq!(found.1, "1HGCM82633A004352");
}

// ---------------------------------------------------------------------------
// Robustness
// ---------------------------------------------------------------------------

#[test]
fn validation_does_not_panic_on_arbitrary_bytes() {
    for len in [0usize, 1, 16, 17, 18, 64] {
        let data: Vec<u8> = (0..len).map(|i| (i * 7 % 256) as u8).collect();
        let _ = VehicleIdentityBlockManager::is_valid_iso3779_vin(&data);
        let _ = VehicleIdentityBlockManager::locate_vin(&data);
    }
}
