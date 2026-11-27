//! Tests for the `uds` and `can_fd` modules of `kivi-flashing`.
//!
//! Every expected value is derived from ISO 14229-1 (UDS) and ISO 15765-2
//! (ISO-TP) as published, or computed by hand from the documented formula —
//! never by running the implementation first.

use kivi_flashing::can_fd::{CanFdDlcMapper, CanFdFlowStatus, CanFdFrame, CanFdIsoTp};
use kivi_flashing::uds::{
    uds_did, uds_sid, EcuIdentificationParser, EcuIdentityInfo, EcuModuleRole, FlashingState,
    FunctionalAddressResolver, SecurityLockoutWatchdog, SeedKeyAlgorithm, TesterPresentPacer,
    UdsFlashingSession, UdsNegativeResponseCode,
};

// ===========================================================================
// ISO-TP single frame
// ===========================================================================

#[test]
fn a_single_frame_encodes_the_length_in_the_low_nibble() {
    // ISO 15765-2: SF PCI is 0x0L where L is the payload length (1..=7).
    let frame = CanFdIsoTp::build_single_frame(0x7E0, &[0x02, 0x10, 0x03]).unwrap();
    assert_eq!(frame.data[0], 0x03, "PCI high nibble 0 = SF, low = length");
    assert_eq!(&frame.data[1..4], &[0x02, 0x10, 0x03]);
}

#[test]
fn a_single_frame_carries_the_can_id() {
    let frame = CanFdIsoTp::build_single_frame(0x7E0, &[0xAA]).unwrap();
    assert_eq!(frame.can_id, 0x7E0);
}

#[test]
fn a_single_frame_round_trips() {
    let payload = [0x02u8, 0x50, 0x03];
    let frame = CanFdIsoTp::build_single_frame(0x7E8, &payload).unwrap();
    assert_eq!(CanFdIsoTp::parse_single_frame(&frame).unwrap(), payload.to_vec());
}

#[test]
fn a_single_frame_rejects_an_empty_payload() {
    // An SF with length 0 is not valid ISO-TP.
    assert!(CanFdIsoTp::build_single_frame(0x7E0, &[]).is_err());
}

// ===========================================================================
// ISO-TP first frame
// ===========================================================================

#[test]
fn a_first_frame_encodes_a_12bit_length() {
    // ISO 15765-2: FF PCI is 0x1XYZ where XYZ is the 12-bit total length.
    // CAN FD needs > 62 bytes before a First Frame is required at all.
    let payload = [0xAAu8; 100];
    let frame = CanFdIsoTp::build_first_frame(0x7E0, payload.len(), &payload[..62]).unwrap();
    assert_eq!(frame.data[0] & 0xF0, 0x10, "high nibble 1 = FirstFrame");
    let len12 = (((frame.data[0] & 0x0F) as usize) << 8) | frame.data[1] as usize;
    assert_eq!(len12, 100, "the 12-bit field must carry the total length");
}

#[test]
fn a_first_frame_round_trips_its_length_and_prefix() {
    let payload: Vec<u8> = (0..100).map(|i| i as u8).collect();
    let frame = CanFdIsoTp::build_first_frame(0x7E0, payload.len(), &payload[..62]).unwrap();
    let (total, prefix) = CanFdIsoTp::parse_first_frame(&frame).unwrap();
    assert_eq!(total, 100);
    assert!(!prefix.is_empty());
    assert_eq!(&prefix[..], &payload[..prefix.len()]);
}

// ===========================================================================
// ISO-TP consecutive frame
// ===========================================================================

#[test]
fn a_consecutive_frame_encodes_the_sequence_number() {
    // ISO 15765-2: CF PCI is 0x2N where N is the sequence number, mod 16.
    let frame = CanFdIsoTp::build_consecutive_frame(0x7E0, 1, &[0xBB; 7]).unwrap();
    assert_eq!(frame.data[0], 0x21, "SN 1 must encode as 0x21");
}

#[test]
fn the_sequence_number_wraps_after_fifteen() {
    let frame = CanFdIsoTp::build_consecutive_frame(0x7E0, 16, &[0xCC; 7]).unwrap();
    assert_eq!(frame.data[0], 0x20, "SN 16 wraps to 0");
}

#[test]
fn a_consecutive_frame_round_trips() {
    let frame = CanFdIsoTp::build_consecutive_frame(0x7E0, 3, &[0x11, 0x22, 0x33]).unwrap();
    let (sn, data) = CanFdIsoTp::parse_consecutive_frame(&frame).unwrap();
    assert_eq!(sn, 3);
    assert_eq!(&data[..3], &[0x11, 0x22, 0x33]);
}

// ===========================================================================
// ISO-TP flow control
// ===========================================================================

#[test]
fn a_flow_control_frame_encodes_the_continue_status() {
    // ISO 15765-2: FC PCI is 0x3S where S = 0 continue, 1 wait, 2 overflow.
    let frame = CanFdIsoTp::build_flow_control_frame(0x7E0, CanFdFlowStatus::ContinueToSend, 0, 0);
    assert_eq!(frame.data[0], 0x30, "continue-to-send is status 0");
}

#[test]
fn a_flow_control_wait_status_is_encoded() {
    let frame = CanFdIsoTp::build_flow_control_frame(0x7E0, CanFdFlowStatus::Wait, 0, 0);
    assert_eq!(frame.data[0], 0x31, "wait is status 1");
}

#[test]
fn a_flow_control_frame_round_trips_its_block_size_and_stmin() {
    let frame = CanFdIsoTp::build_flow_control_frame(0x7E0, CanFdFlowStatus::ContinueToSend, 8, 20);
    let (status, bs, stmin) = CanFdIsoTp::parse_flow_control_frame(&frame).unwrap();
    assert_eq!(bs, 8);
    assert_eq!(stmin, 20);
    assert!(matches!(status, CanFdFlowStatus::ContinueToSend));
}

// ===========================================================================
// ISO-TP segmentation round trip
// ===========================================================================

#[test]
fn a_long_payload_segments_and_reassembles_exactly() {
    // 100 bytes needs an FF plus several CFs.
    // CAN FD carries 62 bytes in the first frame and 63 per consecutive frame,
    // so 200 bytes needs an FF plus at least two CFs.
    let payload: Vec<u8> = (0..200u16).map(|i| (i % 251) as u8).collect();
    let frames = CanFdIsoTp::segment_multi_frame(0x7E0, &payload).unwrap();
    assert!(frames.len() >= 3, "200 bytes cannot fit in one or two frames");
    let restored = CanFdIsoTp::reassemble_multi_frame(&frames).unwrap();
    assert_eq!(restored, payload);
}

#[test]
fn a_payload_of_exactly_one_frame_stays_a_single_frame() {
    let payload = vec![0xAAu8; 7];
    let frames = CanFdIsoTp::segment_multi_frame(0x7E0, &payload).unwrap();
    assert_eq!(frames.len(), 1);
    assert_eq!(CanFdIsoTp::reassemble_multi_frame(&frames).unwrap(), payload);
}

// ===========================================================================
// DLC mapping
// ===========================================================================

#[test]
fn classic_dlc_maps_directly_up_to_eight() {
    for dlc in 0u8..=8 {
        assert_eq!(CanFdDlcMapper::dlc_to_len(dlc), dlc as usize);
    }
}

#[test]
fn the_fd_dlc_steps_match_iso_11898_1() {
    // 9..=15 map to 12, 16, 20, 24, 32, 48, 64.
    assert_eq!(CanFdDlcMapper::dlc_to_len(9), 12);
    assert_eq!(CanFdDlcMapper::dlc_to_len(10), 16);
    assert_eq!(CanFdDlcMapper::dlc_to_len(11), 20);
    assert_eq!(CanFdDlcMapper::dlc_to_len(12), 24);
    assert_eq!(CanFdDlcMapper::dlc_to_len(13), 32);
    assert_eq!(CanFdDlcMapper::dlc_to_len(14), 48);
    assert_eq!(CanFdDlcMapper::dlc_to_len(15), 64);
}

// ===========================================================================
// UDS seed/key
// ===========================================================================

#[test]
fn a_zero_seed_means_already_unlocked() {
    let alg = SeedKeyAlgorithm::default();
    assert_eq!(alg.compute_key(0), 0);
}

#[test]
fn the_seed_key_algorithm_is_rotate_then_xor() {
    // Documented formula: key = seed.rotate_left(5) ^ 0x48A35912.
    let alg = SeedKeyAlgorithm::default();
    let seed = 0x1234_5678u32;
    let expected = seed.rotate_left(5) ^ 0x48A3_5912;
    assert_eq!(alg.compute_key(seed), expected);
}

#[test]
fn different_seeds_give_different_keys() {
    let alg = SeedKeyAlgorithm::default();
    assert_ne!(alg.compute_key(0x1111_1111), alg.compute_key(0x2222_2222));
}

// ===========================================================================
// UDS flashing session state machine
// ===========================================================================

#[test]
fn a_new_session_is_idle() {
    let s = UdsFlashingSession::new(vec![0u8; 16], 0x8000);
    assert!(matches!(s.state, FlashingState::Idle));
}

#[test]
fn the_first_request_is_extended_diagnostic_session() {
    // ISO 14229-1: service 0x10, sub-function 0x03 = extended session.
    let mut s = UdsFlashingSession::new(vec![0u8; 16], 0x8000);
    let frame = s.build_next_request().unwrap();
    assert_eq!(frame, vec![uds_sid::DIAGNOSTIC_SESSION_CONTROL, 0x03]);
}

#[test]
fn the_session_reports_progress_within_range() {
    let s = UdsFlashingSession::new(vec![0u8; 100], 0x8000);
    let ratio = s.progress_ratio();
    assert!((0.0..=1.0).contains(&ratio));
}

// ===========================================================================
// Tester present
// ===========================================================================

#[test]
fn tester_present_is_due_immediately_on_the_first_check() {
    let pacer = TesterPresentPacer::default();
    assert!(pacer.is_time_to_send(0));
}

#[test]
fn tester_present_uses_the_suppress_positive_response_bit() {
    // ISO 14229-1: sub-function 0x80 = suppressPosRspMsgIndicationBit set.
    let mut pacer = TesterPresentPacer::default();
    let frame = pacer.build_tester_present(1000);
    assert_eq!(frame, vec![uds_sid::TESTER_PRESENT, 0x80]);
}

#[test]
fn tester_present_is_not_due_before_the_interval_elapses() {
    let mut pacer = TesterPresentPacer::default();
    pacer.build_tester_present(1000);
    assert!(!pacer.is_time_to_send(1500), "only 500 ms elapsed of 2000");
    assert!(pacer.is_time_to_send(3000), "2000 ms elapsed");
}

// ===========================================================================
// DID parsing
// ===========================================================================

#[test]
fn a_read_did_request_is_service_22_plus_the_did() {
    let frame = EcuIdentificationParser::build_read_did_request(uds_did::VIN_DATA_IDENTIFIER);
    assert_eq!(frame, vec![0x22, 0xF1, 0x90]);
}

#[test]
fn the_vin_did_is_f190() {
    assert_eq!(uds_did::VIN_DATA_IDENTIFIER, 0xF190);
}

#[test]
fn a_did_response_is_parsed() {
    // 0x62 = positive response to 0x22, then the DID, then the data.
    let response = vec![0x62, 0xF1, 0x90, b'A', b'B', b'C'];
    let mut info = EcuIdentityInfo::default();
    let did = EcuIdentificationParser::parse_did_response(&response, &mut info).unwrap();
    assert_eq!(did, uds_did::VIN_DATA_IDENTIFIER);
}

// ===========================================================================
// Security lockout watchdog
// ===========================================================================

#[test]
fn a_negative_security_response_arms_the_watchdog() {
    // `ExceededNumberOfAttempts` is itself the lockout signal, so the handler
    // reports it as an error rather than swallowing it.
    let mut w = SecurityLockoutWatchdog::default();
    let result = w.handle_security_response(UdsNegativeResponseCode::ExceededNumberOfAttempts, 0);
    assert!(result.is_err(), "an exceeded-attempts response is a lockout");
    assert!(w.remaining_lockout_seconds(0) > 0);
}

#[test]
fn three_invalid_keys_engage_the_penalty_timer() {
    // The first two are tolerated; the third arms the lockout.
    let mut w = SecurityLockoutWatchdog::default();
    assert!(w
        .handle_security_response(UdsNegativeResponseCode::InvalidKey, 0)
        .is_ok());
    assert!(w
        .handle_security_response(UdsNegativeResponseCode::InvalidKey, 10)
        .is_ok());
    assert!(w
        .handle_security_response(UdsNegativeResponseCode::InvalidKey, 20)
        .is_err());
    assert!(w.remaining_lockout_seconds(20) > 0);
}

#[test]
fn the_lockout_expires_after_its_delay() {
    let mut w = SecurityLockoutWatchdog::default();
    // The handler returns `Err` on a lockout; the watchdog is still armed.
    let _ = w.handle_security_response(UdsNegativeResponseCode::ExceededNumberOfAttempts, 0);
    let remaining = w.remaining_lockout_seconds(0);
    assert!(
        remaining > 0,
        "the watchdog must have armed before this check"
    );
    assert_eq!(
        w.remaining_lockout_seconds(remaining * 1000),
        0,
        "the lockout must expire once its delay has elapsed"
    );
}

// ===========================================================================
// Functional addressing
// ===========================================================================

#[test]
fn a_11bit_response_resolves_to_a_module_role() {
    // 0x7E8 = the first ECU responding to a functional 0x7DF request.
    let resolved = FunctionalAddressResolver::resolve_11bit_response(0x7E8);
    assert!(resolved.is_ok());
}

#[test]
fn an_out_of_range_response_id_is_rejected() {
    assert!(FunctionalAddressResolver::resolve_11bit_response(0x0001).is_err());
}

#[test]
fn the_functional_scan_request_uses_the_obd_broadcast_id() {
    let (id, payload) = FunctionalAddressResolver::build_functional_scan_request();
    assert_eq!(id, 0x7DF, "0x7DF is the OBD-II functional broadcast id");
    assert_eq!(payload.first(), Some(&0x02), "SF with 2 payload bytes");
}

#[test]
fn the_ecu_module_role_enum_is_usable() {
    let role = EcuModuleRole::EngineControlModule;
    assert!(matches!(role, EcuModuleRole::EngineControlModule));
}

// ===========================================================================
// Frame helper
// ===========================================================================

#[test]
fn a_frame_reports_its_own_length() {
    let frame = CanFdFrame::new(0x7E0, false, false, vec![0x01, 0x02]);
    assert_eq!(frame.data.len(), 2);
}
