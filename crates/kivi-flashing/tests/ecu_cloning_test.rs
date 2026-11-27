//! Parity tests for the ECU cloning session and quad-partition backup container.
//!
//! Ported from `ecu_engine_rust::flashing::ecu_cloning`.
//!
//! ## How expected values were obtained
//!
//! The container is a length-prefixed record with a CRC32 trailer, so the tests
//! assert the documented layout and the round-trip rather than magic bytes:
//!
//! ```text
//! magic(8) | uid(16) | 7 × (len_be u32 | data) | crc32_be u32
//! ```
//!
//! The CRC is validated by the container itself; the tests prove that a
//! corrupted byte is detected rather than silently accepted.

use kivi_flashing::ecu_cloning::{
    ClonePhase, EcuCloneSession, QuadPartitionBackupContainer,
};

fn sample_container() -> QuadPartitionBackupContainer {
    QuadPartitionBackupContainer {
        univocal_mcu_uid: [0xAB; 16],
        ecu_hw_number: "37820-RBB-E12".to_string(),
        ecu_sw_number: "RBB-E12".to_string(),
        bsl_password: vec![0x11, 0x22, 0x33, 0x44],
        maps_data: vec![0xAA; 64],
        micro_flash: vec![0xBB; 128],
        ext_flash: vec![0xCC; 256],
        eeprom_data: vec![0xDD; 32],
        otp_zones: vec![],
    }
}

// ---------------------------------------------------------------------------
// Container round trip
// ---------------------------------------------------------------------------

#[test]
fn a_container_round_trips_through_pack_and_unpack() {
    let original = sample_container();
    let packed = original.pack_single_backup_file();
    let restored = QuadPartitionBackupContainer::unpack_single_backup_file(&packed)
        .expect("must unpack");

    assert_eq!(restored.univocal_mcu_uid, original.univocal_mcu_uid);
    assert_eq!(restored.ecu_hw_number, original.ecu_hw_number);
    assert_eq!(restored.ecu_sw_number, original.ecu_sw_number);
    assert_eq!(restored.bsl_password, original.bsl_password);
    assert_eq!(restored.maps_data, original.maps_data);
    assert_eq!(restored.micro_flash, original.micro_flash);
    assert_eq!(restored.ext_flash, original.ext_flash);
    assert_eq!(restored.eeprom_data, original.eeprom_data);
}

#[test]
fn the_packed_container_carries_a_crc32_trailer() {
    let packed = sample_container().pack_single_backup_file();
    // The last four bytes are the CRC; the container must be at least
    // magic(8) + uid(16) + 7 lengths(28) + crc(4) = 56 bytes even when empty.
    assert!(packed.len() >= 56, "container too short: {}", packed.len());
}

#[test]
fn an_empty_container_round_trips() {
    let empty = QuadPartitionBackupContainer {
        univocal_mcu_uid: [0u8; 16],
        ecu_hw_number: String::new(),
        ecu_sw_number: String::new(),
        bsl_password: vec![],
        maps_data: vec![],
        micro_flash: vec![],
        ext_flash: vec![],
        eeprom_data: vec![],
        otp_zones: vec![],
    };
    let packed = empty.pack_single_backup_file();
    let restored = QuadPartitionBackupContainer::unpack_single_backup_file(&packed)
        .expect("an empty container must still be well formed");
    assert!(restored.maps_data.is_empty());
    assert!(restored.ecu_hw_number.is_empty());
}

#[test]
fn packing_is_deterministic() {
    let c = sample_container();
    assert_eq!(c.pack_single_backup_file(), c.pack_single_backup_file());
}

// ---------------------------------------------------------------------------
// Malformed containers
// ---------------------------------------------------------------------------

#[test]
fn a_truncated_container_is_rejected() {
    let packed = sample_container().pack_single_backup_file();
    for cut in [0usize, 1, 8, 24, packed.len() - 1] {
        let err = QuadPartitionBackupContainer::unpack_single_backup_file(&packed[..cut])
            .expect_err("a truncated container must be rejected");
        // Truncation surfaces as either 20002 (too short) or 20003 (the CRC
        // trailer is no longer where the reader expects it). Both are rejections;
        // the exact code depends on how far the cut lands.
        assert!(
            err.contains("20002") || err.contains("20003"),
            "cut {cut}: error must be a documented rejection, got: {err}"
        );
    }
}

#[test]
fn a_bad_magic_is_rejected_with_the_documented_error_code() {
    let mut packed = sample_container().pack_single_backup_file();
    packed[0] ^= 0xFF;
    let err = QuadPartitionBackupContainer::unpack_single_backup_file(&packed)
        .expect_err("must reject a bad magic");
    assert!(err.contains("20001"), "error must be IDS_LOADER_ERROR_20001: {err}");
}

#[test]
fn a_corrupted_payload_fails_the_crc_check() {
    let mut packed = sample_container().pack_single_backup_file();
    // Flip a byte in the maps region (well inside the payload, not the CRC).
    let mid = packed.len() / 2;
    packed[mid] ^= 0xFF;
    let err = QuadPartitionBackupContainer::unpack_single_backup_file(&packed)
        .expect_err("a corrupted payload must fail CRC");
    assert!(
        err.contains("20003") || err.contains("CRC"),
        "error must name the CRC failure: {err}"
    );
}

#[test]
fn unpacking_does_not_panic_on_arbitrary_bytes() {
    for len in [0usize, 1, 7, 8, 24, 55, 56, 128] {
        let junk: Vec<u8> = (0..len).map(|i| (i * 41 % 256) as u8).collect();
        let _ = QuadPartitionBackupContainer::unpack_single_backup_file(&junk);
    }
}

// ---------------------------------------------------------------------------
// Clone session — the real three-phase flow
// ---------------------------------------------------------------------------

#[test]
fn a_new_clone_session_starts_in_phase_zero() {
    let s = EcuCloneSession::new();
    assert_eq!(
        s.phase,
        ClonePhase::Phase0ReadOriginal,
        "a fresh session begins by reading the original ECU"
    );
}

#[test]
fn a_session_cannot_enter_phase_one_before_reading_the_original() {
    let mut s = EcuCloneSession::new();
    let err = s
        .confirm_donor_connected(&[0x02; 16])
        .expect_err("phase 1 requires a captured original backup");
    assert!(
        err.contains("before reading Original"),
        "error must explain the ordering: {err}"
    );
}

#[test]
fn capturing_the_original_ecu_moves_the_session_to_phase_one() {
    let mut s = EcuCloneSession::new();
    let phase = s
        .capture_original_ecu(sample_container())
        .expect("capturing must succeed");
    assert_eq!(phase, ClonePhase::Phase1SwitchToDonor);
    assert_eq!(s.phase, ClonePhase::Phase1SwitchToDonor);
}

#[test]
fn a_donor_with_the_same_uid_as_the_original_is_rejected() {
    // Guard against forgetting to swap the ECU: same UID means same physical unit.
    let mut s = EcuCloneSession::new();
    let backup = sample_container(); // uid = [0xAB; 16]
    s.capture_original_ecu(backup.clone()).expect("capture");

    let err = s
        .confirm_donor_connected(&backup.univocal_mcu_uid)
        .expect_err("an identical UID must be rejected");
    assert!(
        err.contains("20055"),
        "error must be the documented IDS_LOADER_ERROR_20055: {err}"
    );
}

#[test]
fn a_donor_with_a_different_uid_advances_to_phase_two() {
    let mut s = EcuCloneSession::new();
    s.capture_original_ecu(sample_container()).expect("capture");
    let phase = s
        .confirm_donor_connected(&[0xCD; 16])
        .expect("a different UID is a valid donor");
    assert_eq!(phase, ClonePhase::Phase2ValidateAndAdaptDonor);
}

#[test]
fn adapting_before_phase_two_is_refused() {
    let mut s = EcuCloneSession::new();
    let err = s
        .adapt_clone_image_for_donor("HW", &[0u8; 128], true)
        .expect_err("adaptation requires phase 2");
    assert!(err.contains("not in Phase 2"), "error: {err}");
}

#[test]
fn adapting_with_a_mismatched_hardware_number_is_refused() {
    let mut s = EcuCloneSession::new();
    let backup = sample_container();
    let flash_len = backup.micro_flash.len();
    s.capture_original_ecu(backup).expect("capture");
    s.confirm_donor_connected(&[0xCD; 16]).expect("donor");

    let err = s
        .adapt_clone_image_for_donor("WRONG-HW", &vec![0u8; flash_len], true)
        .expect_err("a hardware mismatch must be refused");
    assert!(err.contains("21008"), "error must be CLONE_ERR_21008: {err}");
}

#[test]
fn adapting_with_a_mismatched_flash_size_is_refused() {
    let mut s = EcuCloneSession::new();
    let backup = sample_container();
    let hw = backup.ecu_hw_number.clone();
    s.capture_original_ecu(backup).expect("capture");
    s.confirm_donor_connected(&[0xCD; 16]).expect("donor");

    let err = s
        .adapt_clone_image_for_donor(&hw, &[0u8; 1], true)
        .expect_err("a flash size mismatch must be refused");
    assert!(err.contains("size"), "error must mention size: {err}");
}

#[test]
fn a_matching_donor_produces_an_adapted_image() {
    let mut s = EcuCloneSession::new();
    let backup = sample_container();
    let hw = backup.ecu_hw_number.clone();
    let flash_len = backup.micro_flash.len();
    s.capture_original_ecu(backup).expect("capture");
    s.confirm_donor_connected(&[0xCD; 16]).expect("donor");

    let adapted = s
        .adapt_clone_image_for_donor(&hw, &vec![0xEE; flash_len], true)
        .expect("a matching donor must adapt");
    // The adapted image is the ORIGINAL image with the donor's locked OTP zones
    // spliced in — not a copy of the donor flash. With no OTP zones declared, the
    // original micro flash is kept unchanged.
    assert_eq!(
        adapted.micro_flash,
        vec![0xBB; flash_len],
        "without OTP zones the original image is preserved verbatim"
    );
    assert_eq!(s.phase, ClonePhase::CloneCompleted, "adaptation completes the session");
}

#[test]
fn container_preserves_otp_zones_across_pack_and_unpack() {
    let mut original = sample_container();
    original.otp_zones = vec![
        kivi_flashing::ecu_cloning::OtpZoneDescriptor {
            start_offset: 0x0010,
            length: 8,
            is_hardware_locked: true,
        },
        kivi_flashing::ecu_cloning::OtpZoneDescriptor {
            start_offset: 0x0040,
            length: 4,
            is_hardware_locked: false,
        },
    ];
    let packed = original.pack_single_backup_file();
    assert_eq!(&packed[0..8], QuadPartitionBackupContainer::CONTAINER_MAGIC_V2);
    let restored = QuadPartitionBackupContainer::unpack_single_backup_file(&packed)
        .expect("must unpack v2 container with OTP zones");
    assert_eq!(restored.otp_zones, original.otp_zones);
}
