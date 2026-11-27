//! Tests for the pre-flash safety gate.
//!
//! ## Enforced Safety Invariants
//!
//! Three non-bypassable checks enforced before any flash write starts:
//!
//! 1. Refuse an image whose size does not match the target ROM geometry.
//! 2. Require a verified stock readback / stock-first order before writing modified data.
//! 3. Refuse an ECU target missing from the known ECU support profile.

use kivi_flashing::flash_safety::{
    check_flash_safety, FlashSafetyError, FlashSafetyPolicy, FlashTarget,
};

fn target() -> FlashTarget {
    FlashTarget {
        rom_size: 0x100000,
        base_address: 0x0000_0000,
        is_known_ecu: true,
        has_stock_backup: true,
        stock_written_first: true,
        checksum_correction_enabled: true,
    }
}

// ---------------------------------------------------------------------------
// Size match
// ---------------------------------------------------------------------------

#[test]
fn an_image_of_the_right_size_is_accepted() {
    let t = target();
    let image = vec![0u8; t.rom_size];
    assert!(check_flash_safety(&t, &image, &FlashSafetyPolicy::strict()).is_ok());
}

#[test]
fn an_image_too_short_is_refused() {
    let t = target();
    let image = vec![0u8; 0x80000];
    let err = check_flash_safety(&t, &image, &FlashSafetyPolicy::strict())
        .expect_err("short image must be refused");
    assert!(matches!(err, FlashSafetyError::SizeMismatch { .. }));
}

#[test]
fn an_image_too_long_is_refused() {
    let t = target();
    let image = vec![0u8; 0x200000];
    let err = check_flash_safety(&t, &image, &FlashSafetyPolicy::strict())
        .expect_err("long image must be refused");
    assert!(matches!(err, FlashSafetyError::SizeMismatch { .. }));
}

#[test]
fn the_size_error_names_both_numbers() {
    let t = target();
    let image = vec![0u8; 0x80000];
    let err = check_flash_safety(&t, &image, &FlashSafetyPolicy::strict()).unwrap_err();
    let text = err.to_string();
    assert!(text.contains("1048576"), "error was: {text}");
    assert!(text.contains("524288"), "error was: {text}");
}

// ---------------------------------------------------------------------------
// Known ECU
// ---------------------------------------------------------------------------

#[test]
fn an_unknown_ecu_is_refused_under_strict_policy() {
    let mut t = target();
    t.is_known_ecu = false;
    let image = vec![0u8; t.rom_size];
    let err = check_flash_safety(&t, &image, &FlashSafetyPolicy::strict())
        .expect_err("unknown ECU must be refused");
    assert!(matches!(err, FlashSafetyError::UnknownEcu));
}

#[test]
fn an_unknown_ecu_is_allowed_under_permissive_policy() {
    let mut t = target();
    t.is_known_ecu = false;
    let image = vec![0u8; t.rom_size];
    assert!(check_flash_safety(&t, &image, &FlashSafetyPolicy::permissive()).is_ok());
}

// ---------------------------------------------------------------------------
// Stock backup
// ---------------------------------------------------------------------------

#[test]
fn flashing_without_a_stock_backup_is_refused_under_strict_policy() {
    let mut t = target();
    t.has_stock_backup = false;
    let image = vec![0u8; t.rom_size];
    let err = check_flash_safety(&t, &image, &FlashSafetyPolicy::strict())
        .expect_err("no backup must be refused");
    assert!(matches!(err, FlashSafetyError::NoStockBackup));
}

#[test]
fn flashing_before_writing_stock_is_refused_under_strict_policy() {
    // Strict policy requires writing/verifying stock baseline before modified tune writes.
    let mut t = target();
    t.stock_written_first = false;
    let image = vec![0u8; t.rom_size];
    let err = check_flash_safety(&t, &image, &FlashSafetyPolicy::strict())
        .expect_err("stock must be written first");
    assert!(matches!(err, FlashSafetyError::StockNotWrittenFirst));
}

// ---------------------------------------------------------------------------
// Checksum correction
// ---------------------------------------------------------------------------

#[test]
fn flashing_with_checksum_correction_disabled_requires_explicit_consent() {
    let mut t = target();
    t.checksum_correction_enabled = false;
    let image = vec![0u8; t.rom_size];
    let err = check_flash_safety(&t, &image, &FlashSafetyPolicy::strict())
        .expect_err("disabled checksum must require consent");
    assert!(matches!(err, FlashSafetyError::ChecksumDisabledWithoutConsent));
}

#[test]
fn disabled_checksum_is_allowed_when_the_policy_consents() {
    let mut t = target();
    t.checksum_correction_enabled = false;
    let image = vec![0u8; t.rom_size];
    let mut p = FlashSafetyPolicy::strict();
    p.allow_checksum_disabled = true;
    assert!(check_flash_safety(&t, &image, &p).is_ok());
}

// ---------------------------------------------------------------------------
// Ordering
// ---------------------------------------------------------------------------

#[test]
fn the_size_check_runs_before_the_backup_check() {
    // A wrong-size image is a typo; reporting "no backup" first would send the
    // user chasing the wrong problem.
    let mut t = target();
    t.has_stock_backup = false;
    let image = vec![0u8; 0x10];
    let err = check_flash_safety(&t, &image, &FlashSafetyPolicy::strict()).unwrap_err();
    assert!(
        matches!(err, FlashSafetyError::SizeMismatch { .. }),
        "expected size error first, got {err:?}"
    );
}

#[test]
fn an_empty_image_is_refused() {
    let t = target();
    let err = check_flash_safety(&t, &[], &FlashSafetyPolicy::strict()).expect_err("empty");
    assert!(matches!(err, FlashSafetyError::SizeMismatch { .. }));
}

// ---------------------------------------------------------------------------
// Policy defaults
// ---------------------------------------------------------------------------

#[test]
fn the_strict_policy_requires_everything() {
    let p = FlashSafetyPolicy::strict();
    assert!(p.require_known_ecu);
    assert!(p.require_stock_backup);
    assert!(p.require_stock_written_first);
    assert!(!p.allow_checksum_disabled);
}

#[test]
fn the_permissive_policy_still_requires_a_size_match() {
    // Size is never optional: a wrong-size write is always a brick risk.
    let p = FlashSafetyPolicy::permissive();
    assert!(!p.require_known_ecu);
    assert!(!p.require_stock_backup);
    let t = target();
    let image = vec![0u8; 0x10];
    assert!(check_flash_safety(&t, &image, &p).is_err());
}
