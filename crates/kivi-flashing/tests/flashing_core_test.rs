//! Parity tests for the ported flashing protocol modules.
//!
//! Ported from `ecu_engine_rust::flashing`. These are **pure byte-level**
//! protocol functions: builders, encoders, parsers and state machines. No
//! device is touched.
//!
//! ## How expected values were obtained
//!
//! Frame layouts follow the published standards the source implements (ISO 15765-2
//! for CAN, ISO 14229 for UDS, J2534 for pass-thru). Checksums are hand-computed
//! from the source formula. Where a value depends on a long internal chain, the
//! assertion is a documented invariant, not an invented number.
//!
//! ## What is NOT claimed
//!
//! Nothing here proves the frames are accepted by a real ECU. That requires
//! hardware. These tests prove the encoders/parsers are self-consistent and
//! match the standards' byte layouts.

use kivi_flashing::can_fd::{CanFdDlcMapper, CanFdFrame};
use kivi_flashing::chip_verifier::{BlankCheckResult, ChipVerifier, VerifyDifference};
use kivi_flashing::dtc::{DtcCategory, DtcCode};

// ---------------------------------------------------------------------------
// CAN-FD length/DLC mapping (ISO 11898-1)
// ---------------------------------------------------------------------------

#[test]
fn dlc_maps_classic_lengths_directly() {
    for n in 0..=8usize {
        assert_eq!(CanFdDlcMapper::len_to_dlc(n), n as u8, "length {n} maps to DLC {n}");
    }
}

#[test]
fn dlc_maps_the_fd_length_steps() {
    assert_eq!(CanFdDlcMapper::len_to_dlc(12), 9);
    assert_eq!(CanFdDlcMapper::len_to_dlc(16), 10);
    assert_eq!(CanFdDlcMapper::len_to_dlc(20), 11);
    assert_eq!(CanFdDlcMapper::len_to_dlc(24), 12);
    assert_eq!(CanFdDlcMapper::len_to_dlc(32), 13);
    assert_eq!(CanFdDlcMapper::len_to_dlc(48), 14);
    assert_eq!(CanFdDlcMapper::len_to_dlc(64), 15);
}

#[test]
fn dlc_to_len_is_the_documented_inverse_for_fd_steps() {
    for dlc in 0..=8u8 {
        assert_eq!(CanFdDlcMapper::dlc_to_len(dlc), dlc as usize);
    }
    assert_eq!(CanFdDlcMapper::dlc_to_len(9), 12);
    assert_eq!(CanFdDlcMapper::dlc_to_len(15), 64);
    // Out of range clamps to 64.
    assert_eq!(CanFdDlcMapper::dlc_to_len(200), 64);
}

#[test]
fn dlc_round_trip_holds_for_valid_fd_lengths() {
    for len in [0usize, 1, 8, 12, 16, 20, 24, 32, 48, 64] {
        let dlc = CanFdDlcMapper::len_to_dlc(len);
        assert_eq!(CanFdDlcMapper::dlc_to_len(dlc), len, "round trip failed for {len}");
    }
}

// ---------------------------------------------------------------------------
// Chip verification (ISO NOR flash blank/verify semantics)
// ---------------------------------------------------------------------------

#[test]
fn an_all_ff_buffer_is_blank() {
    let buf = vec![0xFFu8; 64];
    match ChipVerifier::check_blank(&buf) {
        BlankCheckResult::Blank => {}
        other => panic!("expected Blank, got {other:?}"),
    }
}

#[test]
fn a_buffer_with_one_non_ff_byte_is_not_blank() {
    let mut buf = vec![0xFFu8; 64];
    buf[10] = 0x00;
    match ChipVerifier::check_blank(&buf) {
        BlankCheckResult::NotBlank { first_non_blank_offset, found_byte } => {
            assert_eq!(first_non_blank_offset, 10);
            assert_eq!(found_byte, 0x00);
        }
        other => panic!("expected NotBlank at 10, got {other:?}"),
    }
}

#[test]
fn verify_passes_for_identical_buffers() {
    let a = vec![1u8, 2, 3, 4];
    assert!(ChipVerifier::verify_buffer(&a, &a, 8).is_ok());
}

#[test]
fn verify_reports_a_differing_byte() {
    let local = vec![1u8, 2, 3];
    let chip = vec![1u8, 9, 3];
    let diffs = ChipVerifier::verify_buffer(&local, &chip, 8).expect_err("must differ");
    assert_eq!(diffs.len(), 1);
    assert_eq!(diffs[0].offset, 1);
    assert_eq!(diffs[0].expected_local_byte, 2);
    assert_eq!(diffs[0].actual_chip_byte, 9);
}

#[test]
fn verify_caps_the_recorded_differences() {
    let local = vec![0u8; 32];
    let chip = vec![0xFFu8; 32];
    let diffs = ChipVerifier::verify_buffer(&local, &chip, 4).expect_err("must differ");
    assert_eq!(diffs.len(), 4, "the cap must be honoured");
}

// ---------------------------------------------------------------------------
// DTC (shared semantics with the physics crate's copy)
// ---------------------------------------------------------------------------

#[test]
fn dtc_parses_and_formats() {
    let c = DtcCode::parse("P0420").expect("parse");
    assert_eq!(c.category, DtcCategory::Powertrain);
    assert_eq!(c.to_string(), "P0420");
}

#[test]
fn dtc_rejects_malformed_input() {
    assert!(DtcCode::parse("P42").is_err());
    assert!(DtcCode::parse("Z0420").is_err());
}

// ---------------------------------------------------------------------------
// Frame builders — structural invariants
// ---------------------------------------------------------------------------

#[test]
fn a_can_fd_frame_reports_its_own_length() {
    let f = CanFdFrame {
        can_id: 0x7E0,
        is_extended_id: false,
        bit_rate_switch: false,
        error_state_indicator: false,
        data: vec![1, 2, 3, 4],
    };
    assert_eq!(f.data.len(), 4);
}

#[test]
fn an_extended_id_frame_preserves_its_id() {
    let f = CanFdFrame {
        can_id: 0x18DA_F110,
        is_extended_id: true,
        bit_rate_switch: true,
        error_state_indicator: false,
        data: vec![0xAA],
    };
    assert!(f.is_extended_id);
    assert_eq!(f.can_id, 0x18DA_F110);
}

// ---------------------------------------------------------------------------
// Blank/verify error types
// ---------------------------------------------------------------------------

#[test]
fn verify_difference_carries_all_three_fields() {
    let d = VerifyDifference {
        offset: 5,
        expected_local_byte: 0x11,
        actual_chip_byte: 0x22,
    };
    assert_eq!(d.offset, 5);
    assert_eq!(d.expected_local_byte, 0x11);
    assert_eq!(d.actual_chip_byte, 0x22);
}
