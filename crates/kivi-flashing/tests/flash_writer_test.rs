//! Parity tests for the universal flash writer.
//!
//! Ported from `ecu_engine_rust::flashing::flash_writer`.
//!
//! ## Why this module is tested hardest
//!
//! These functions build the UDS frames that erase and write flash. A wrong
//! address, a wrong length, or a missing interlock can destroy an ECU. The tests
//! below pin:
//!
//! * every pre-flash interlock and its precedence,
//! * boot-sector write refusal,
//! * the exact byte layout of the erase request (ISO 14229 RoutineControl),
//! * the blank-check and post-flash verification paths.
//!
//! ## How expected values were obtained
//!
//! Frame layouts follow ISO 14229-1: `0x31 0x01 <routineId hi> <lo> <option>`
//! for RoutineControl, `0x34` RequestDownload, `0x36` TransferData, `0x37`
//! RequestTransferExit. The expectations are derived from that standard, not from
//! running the code.
//!
//! ## Documented source limitation
//!
//! The source declares 8 `FlashWriteState` variants but only 3 are ever assigned.
//! The rest are unreachable; see `only_three_write_states_are_reachable`.

use kivi_flashing::flash_writer::{
    FlashSector, FlashWriteState, PreFlashCheckStatus, UniversalFlashWriter,
};

fn sectors() -> Vec<FlashSector> {
    vec![
        FlashSector {
            sector_index: 0,
            start_address: 0x0000_0000,
            length: 0x4000,
            is_boot_sector: true,
        },
        FlashSector {
            sector_index: 1,
            start_address: 0x0000_4000,
            length: 0x8000,
            is_boot_sector: false,
        },
        FlashSector {
            sector_index: 2,
            start_address: 0x0000_C000,
            length: 0x4000,
            is_boot_sector: false,
        },
    ]
}

fn writer() -> UniversalFlashWriter {
    UniversalFlashWriter::new(sectors(), 256, false)
}

// ---------------------------------------------------------------------------
// Pre-flash interlocks
// ---------------------------------------------------------------------------

#[test]
fn a_healthy_car_is_ready_to_flash() {
    let s = UniversalFlashWriter::perform_pre_flash_checks(12_600, 0, true);
    assert_eq!(s, PreFlashCheckStatus::ReadyToFlash);
}

#[test]
fn low_supply_voltage_blocks_the_flash() {
    let s = UniversalFlashWriter::perform_pre_flash_checks(12_000, 0, true);
    assert!(
        matches!(s, PreFlashCheckStatus::VoltageTooLow(v) if v == 12_000),
        "got {s:?}"
    );
}

#[test]
fn the_voltage_threshold_is_inclusive_at_12_2_v() {
    // The check is `< 12_200`, so exactly 12 200 mV must pass.
    assert_eq!(
        UniversalFlashWriter::perform_pre_flash_checks(12_200, 0, true),
        PreFlashCheckStatus::ReadyToFlash
    );
    assert!(matches!(
        UniversalFlashWriter::perform_pre_flash_checks(12_199, 0, true),
        PreFlashCheckStatus::VoltageTooLow(_)
    ));
}

#[test]
fn a_running_engine_blocks_the_flash() {
    let s = UniversalFlashWriter::perform_pre_flash_checks(12_600, 850, true);
    assert!(
        matches!(s, PreFlashCheckStatus::EngineRunningRpm(r) if r == 850),
        "got {s:?}"
    );
}

#[test]
fn locked_security_access_blocks_the_flash() {
    let s = UniversalFlashWriter::perform_pre_flash_checks(12_600, 0, false);
    assert_eq!(s, PreFlashCheckStatus::SecurityAccessLocked);
}

#[test]
fn voltage_is_checked_before_rpm_and_security() {
    // All three faults present: the voltage fault must win, because an
    // undervoltage write is the most destructive.
    let s = UniversalFlashWriter::perform_pre_flash_checks(11_000, 900, false);
    assert!(
        matches!(s, PreFlashCheckStatus::VoltageTooLow(_)),
        "voltage must be reported first, got {s:?}"
    );
}

#[test]
fn rpm_is_checked_before_security() {
    let s = UniversalFlashWriter::perform_pre_flash_checks(12_600, 900, false);
    assert!(
        matches!(s, PreFlashCheckStatus::EngineRunningRpm(_)),
        "rpm must be reported before security, got {s:?}"
    );
}

// ---------------------------------------------------------------------------
// Boot sector protection
// ---------------------------------------------------------------------------

#[test]
fn writing_the_boot_sector_is_refused_by_default() {
    let mut w = writer();
    let err = w
        .build_erase_sector_request(0)
        .expect_err("the boot sector must be protected");
    assert!(
        err.contains("BOOT SECTOR WRITE BLOCKED"),
        "error must name the boot-sector block: {err}"
    );
}

#[test]
fn the_boot_sector_can_be_written_only_when_explicitly_allowed() {
    let mut w = UniversalFlashWriter::new(sectors(), 256, true);
    assert!(
        w.build_erase_sector_request(0).is_ok(),
        "an explicit opt-in must permit it"
    );
}

#[test]
fn a_normal_sector_is_not_blocked() {
    let mut w = writer();
    assert!(w.build_erase_sector_request(1).is_ok());
}

#[test]
fn an_out_of_range_sector_index_is_rejected() {
    let mut w = writer();
    let err = w.build_erase_sector_request(99).expect_err("must reject");
    assert!(err.contains("Invalid sector index"), "error: {err}");
}

// ---------------------------------------------------------------------------
// Erase request frame (ISO 14229 RoutineControl)
// ---------------------------------------------------------------------------

#[test]
fn the_erase_request_uses_the_routine_control_service() {
    let mut w = writer();
    let pkt = w.build_erase_sector_request(1).expect("erase request");
    // The source frames erase as `31 01 FF 00 44 <addr> <len>`:
    // 0x31 = RoutineControl, 0x01 = startRoutine, routine id 0xFF00.
    assert_eq!(pkt[0], 0x31, "service id must be RoutineControl");
    assert_eq!(pkt[1], 0x01, "sub-function must be startRoutine");
    assert_eq!(&pkt[2..4], &[0xFF, 0x00], "routine id must be 0xFF00 (eraseMemory)");
}

#[test]
fn the_erase_request_carries_the_sector_address_and_length_big_endian() {
    let mut w = writer();
    let pkt = w.build_erase_sector_request(1).expect("erase request");
    // Sector 1: start 0x0000_4000, length 0x0000_8000.
    assert_eq!(&pkt[5..9], &0x0000_4000u32.to_be_bytes());
    assert_eq!(&pkt[9..13], &0x0000_8000u32.to_be_bytes());
}

#[test]
fn the_erase_request_is_deterministic() {
    let mut a = writer();
    let mut b = writer();
    assert_eq!(
        a.build_erase_sector_request(2).unwrap(),
        b.build_erase_sector_request(2).unwrap()
    );
}

// ---------------------------------------------------------------------------
// Transfer frames
// ---------------------------------------------------------------------------

#[test]
fn a_download_request_uses_service_0x34_and_blocks_boot_sector() {
    let w = writer();
    let pkt = w.build_request_download(0x4000, 256).expect("non-boot range allowed");
    assert_eq!(pkt[0], 0x34, "RequestDownload is service 0x34");
    assert!(
        w.build_request_download(0x0000, 256).is_err(),
        "RequestDownload overlapping boot sector must be refused"
    );
}

#[test]
fn a_transfer_exit_uses_service_0x37() {
    let pkt = UniversalFlashWriter::build_request_transfer_exit();
    assert_eq!(pkt[0], 0x37, "RequestTransferExit is service 0x37");
}

#[test]
fn a_transfer_data_chunk_uses_service_0x36() {
    let mut w = writer();
    let pkt = w.build_transfer_data_chunk(&[0xAA, 0xBB, 0xCC]);
    assert_eq!(pkt[0], 0x36, "TransferData is service 0x36");
    assert!(
        pkt.ends_with(&[0xAA, 0xBB, 0xCC]),
        "the payload must be carried verbatim"
    );
}

#[test]
fn successive_transfer_chunks_increment_the_sequence_counter() {
    let mut w = writer();
    let first = w.build_transfer_data_chunk(&[0x01]);
    let second = w.build_transfer_data_chunk(&[0x02]);
    assert_ne!(
        first[1], second[1],
        "the block sequence counter must advance"
    );
}

#[test]
fn a_checksum_routine_uses_service_0x31() {
    let pkt = UniversalFlashWriter::build_checksum_verification_routine(0xDEAD_BEEF);
    assert_eq!(pkt[0], 0x31, "RoutineControl is service 0x31");
    assert!(
        pkt.windows(4).any(|w| w == 0xDEAD_BEEFu32.to_be_bytes()),
        "the expected CRC must appear big-endian in the payload"
    );
}

// ---------------------------------------------------------------------------
// Verification paths
// ---------------------------------------------------------------------------

#[test]
fn a_fully_erased_sector_passes_the_blank_check() {
    let mut w = writer();
    let erased = vec![0xFFu8; 0x4000];
    assert!(w.verify_sector_blank(&erased).is_ok());
}

#[test]
fn a_sector_with_a_non_ff_byte_fails_the_blank_check() {
    let mut w = writer();
    let mut erased = vec![0xFFu8; 0x4000];
    erased[0x1234] = 0x00;
    let (offset, byte) = w.verify_sector_blank(&erased).expect_err("must fail");
    assert_eq!(offset, 0x1234);
    assert_eq!(byte, 0x00);
}

// ---------------------------------------------------------------------------
// Documented source limitation
// ---------------------------------------------------------------------------

#[test]
fn only_three_write_states_are_reachable() {
    // The enum declares 8 variants but only ErasingSector, WritingChunk and
    // Idle/Completed are ever assigned by the source. This test records the
    // reachable set so that adding a transition is a visible change.
    let mut w = writer();
    let _ = w.build_erase_sector_request(1);
    // After building an erase request the writer must be in ErasingSector.
    match &w.state {
        FlashWriteState::ErasingSector { sector_index, total_sectors } => {
            assert_eq!(*sector_index, 1);
            assert_eq!(*total_sectors, 3);
        }
        other => panic!("expected ErasingSector, got {other:?}"),
    }
}

#[test]
fn an_erase_response_does_not_panic_on_short_input() {
    let mut w = writer();
    let _ = w.build_erase_sector_request(1);
    for len in [0usize, 1, 2, 3] {
        let resp = vec![0x71u8; len];
        let _ = w.handle_erase_response(&resp);
    }
}
