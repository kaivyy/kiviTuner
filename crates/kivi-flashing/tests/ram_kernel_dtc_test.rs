//! Parity tests for the RAM kernel session and the DTC switch matrix.
//!
//! Ported from `ecu_engine_rust::flashing::{ram_kernel, dtc}`.
//!
//! ## How expected values were obtained
//!
//! Frame layouts follow ISO 14229: `0x34` RequestDownload with
//! `dataFormatIdentifier` and `addressAndLengthFormatIdentifier`, `0x31`
//! RoutineControl, `0x36` TransferData, `0x37` RequestTransferExit. The
//! expectations are derived from that standard plus the source's chosen routine
//! ids, not from running the code.

use kivi_flashing::dtc::{
    DtcCategory, DtcCode, DtcSwitchMatrix, DtcSwitchState, FaultDebounceIntegrator,
};
use kivi_flashing::ram_kernel::{KernelArchitecture, KernelState, RamKernelSession};

// ===========================================================================
// RAM kernel
// ===========================================================================

fn session() -> RamKernelSession {
    RamKernelSession::new(
        KernelArchitecture::RenesasSuperH,
        0xFFFF_8000,
        vec![0x11; 64],
    )
}

#[test]
fn the_upload_request_uses_service_0x34() {
    let s = session();
    let pkt = s.build_kernel_upload_request();
    assert_eq!(pkt[0], 0x34, "RequestDownload is service 0x34");
}

#[test]
fn the_upload_request_declares_uncompressed_unencrypted_and_4_byte_fields() {
    let s = session();
    let pkt = s.build_kernel_upload_request();
    assert_eq!(pkt[1], 0x00, "dataFormatIdentifier: no compression, no encryption");
    assert_eq!(pkt[2], 0x44, "addressAndLengthFormatIdentifier: 4-byte addr + 4-byte len");
}

#[test]
fn the_upload_request_carries_the_ram_address_and_payload_length() {
    let s = session();
    let pkt = s.build_kernel_upload_request();
    assert_eq!(&pkt[3..7], &0xFFFF_8000u32.to_be_bytes(), "RAM load address");
    assert_eq!(&pkt[7..11], &64u32.to_be_bytes(), "payload length");
    assert_eq!(pkt.len(), 11, "the frame has a fixed 11-byte layout");
}

#[test]
fn the_execute_command_uses_routine_control_with_routine_0x0200() {
    let mut s = session();
    let pkt = s.build_execute_kernel_command();
    assert_eq!(&pkt[0..4], &[0x31, 0x01, 0x02, 0x00], "StartRoutine 0x0200");
    assert_eq!(&pkt[4..8], &0xFFFF_8000u32.to_be_bytes());
}

#[test]
fn executing_the_kernel_moves_the_session_into_the_running_state() {
    let mut s = session();
    assert_ne!(s.state, KernelState::KernelRunning, "not running before execute");
    let _ = s.build_execute_kernel_command();
    assert_eq!(s.state, KernelState::KernelRunning);
}

#[test]
fn transfer_blocks_cover_the_whole_payload() {
    let s = session();
    let blocks = s.build_kernel_transfer_blocks(16);
    assert_eq!(blocks.len(), 4, "64 bytes in 16-byte chunks is 4 blocks");
    // Each block must start with the TransferData service id.
    for b in &blocks {
        assert_eq!(b[0], 0x36, "each chunk is a TransferData request");
    }
}

#[test]
fn an_uneven_chunk_size_still_covers_every_byte() {
    let s = session(); // 64-byte payload
    let blocks = s.build_kernel_transfer_blocks(10);
    assert_eq!(blocks.len(), 7, "64 bytes in 10-byte chunks needs 7 blocks");
}

#[test]
fn a_ram_read_command_uses_the_gm_mode_35_layout() {
    // The source frames a RAM-mediated flash read as a GM VPW/CAN message:
    // header 6D F0 10 | 0x35 (mode 35) | 0x01 | length BE u16 | 3-byte address.
    let s = session();
    let pkt = s.build_ram_read_block_command(0x0001_0000, 256);

    assert_eq!(&pkt[0..3], &[0x6D, 0xF0, 0x10], "GM VPW/CAN header");
    assert_eq!(pkt[3], 0x35, "mode 35 reads flash through a RAM pointer");
    assert_eq!(pkt[4], 0x01);
    assert_eq!(&pkt[5..7], &256u16.to_be_bytes(), "length is a big-endian u16");
    // Address is 3 bytes, most-significant first.
    assert_eq!(&pkt[7..10], &[0x01, 0x00, 0x00], "3-byte address 0x010000");
    assert_eq!(pkt.len(), 10, "the frame has a fixed 10-byte layout");
}

#[test]
fn the_ram_read_address_is_truncated_to_three_bytes() {
    // Only the low 24 bits of the address are transmitted.
    let s = session();
    let pkt = s.build_ram_read_block_command(0x00AB_CDEF, 1);
    assert_eq!(&pkt[7..10], &[0xAB, 0xCD, 0xEF]);
}

#[test]
fn kernel_frames_are_deterministic() {
    let a = session();
    let b = session();
    assert_eq!(a.build_kernel_upload_request(), b.build_kernel_upload_request());
}

// ===========================================================================
// DTC
// ===========================================================================

#[test]
fn a_standard_code_round_trips_through_string() {
    for text in ["P0420", "P1234", "P2000", "P3000", "C0300", "C2123", "B1000", "U0100", "U3456"] {
        let c = DtcCode::parse(text).expect("parse");
        assert_eq!(c.to_string(), text, "round trip failed for {text}");
    }
}

#[test]
fn each_prefix_maps_to_its_category() {
    assert_eq!(DtcCode::parse("P0001").unwrap().category, DtcCategory::Powertrain);
    assert_eq!(DtcCode::parse("C0001").unwrap().category, DtcCategory::Chassis);
    assert_eq!(DtcCode::parse("B0001").unwrap().category, DtcCategory::Body);
    assert_eq!(DtcCode::parse("U0001").unwrap().category, DtcCategory::Network);
}

#[test]
fn the_second_digit_selects_standard_versus_manufacturer() {
    assert!(DtcCode::parse("P0001").unwrap().is_standard);
    assert!(!DtcCode::parse("P1001").unwrap().is_standard);
    assert!(DtcCode::parse("P2001").unwrap().is_standard);
    assert!(!DtcCode::parse("P3001").unwrap().is_standard);
}

#[test]
fn malformed_codes_are_rejected() {
    assert!(DtcCode::parse("P042").is_err(), "too short");
    assert!(DtcCode::parse("P04201").is_err(), "too long");
    assert!(DtcCode::parse("X0420").is_err(), "bad prefix");
    assert!(DtcCode::parse("P9420").is_err(), "bad second digit");
    assert!(DtcCode::parse("P0ZZZ").is_err(), "non-hex digits");
}

#[test]
fn the_switch_states_have_the_documented_values() {
    assert_eq!(DtcSwitchState::Disabled as u8, 0x00);
    assert_eq!(DtcSwitchState::ActiveNoMil as u8, 0x01);
    assert_eq!(DtcSwitchState::ActiveMilTwoTrip as u8, 0x02);
    assert_eq!(DtcSwitchState::ActiveMilImmediate as u8, 0x03);
}

#[test]
fn patching_a_dtc_switch_writes_zero_and_touches_nothing_else() {
    let mut rom = vec![0x02u8; 16];
    DtcSwitchMatrix::patch_dtc_disable(&mut rom, 5).expect("patch");
    assert_eq!(rom[5], 0x00, "the switch byte must be disabled");
    assert_eq!(rom[4], 0x02, "neighbouring bytes must be untouched");
    assert_eq!(rom[6], 0x02);
}

#[test]
fn patching_out_of_bounds_is_an_error_not_a_panic() {
    let mut rom = vec![0u8; 4];
    let err = DtcSwitchMatrix::patch_dtc_disable(&mut rom, 10).expect_err("must reject");
    assert!(err.contains("exceeds"), "error: {err}");
}

#[test]
fn patching_the_last_byte_is_allowed() {
    let mut rom = vec![0x01u8; 4];
    DtcSwitchMatrix::patch_dtc_disable(&mut rom, 3).expect("the last byte is in range");
    assert_eq!(rom[3], 0x00);
}

// ---------------------------------------------------------------------------
// Debounce integrator
// ---------------------------------------------------------------------------

#[test]
fn the_debounce_trips_exactly_at_its_threshold() {
    let mut d = FaultDebounceIntegrator::default();
    let threshold = d.trip_threshold;
    for _ in 0..threshold - 1 {
        assert!(!d.update(true), "must not trip before the threshold");
    }
    assert!(d.update(true), "the threshold-th positive must trip");
}

#[test]
fn the_debounce_counter_is_capped_above_and_below() {
    let mut up = FaultDebounceIntegrator::default();
    for _ in 0..1000 {
        up.update(true);
    }
    assert_eq!(up.fault_counter, up.trip_threshold + 5);

    let mut down = FaultDebounceIntegrator::default();
    for _ in 0..1000 {
        down.update(false);
    }
    assert_eq!(down.fault_counter, down.clear_threshold - 5);
}

#[test]
fn resetting_the_debounce_clears_the_counter() {
    let mut d = FaultDebounceIntegrator::default();
    for _ in 0..100 {
        d.update(true);
    }
    d.reset();
    assert_eq!(d.fault_counter, 0);
    assert!(!d.update(true), "one symptom after reset must not trip");
}
