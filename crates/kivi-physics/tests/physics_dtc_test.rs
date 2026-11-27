//! Parity tests for the remaining physics domain modules and DTC logic.
//!
//! Ported from `ecu_engine_rust::physics::{boost, sensors, chemistry,
//! airflow_dynamics, axis_rescaling, bin_stacker, transients, multi_cylinder,
//! stage2, supercar, motorsport, autotune}` and `flashing::dtc`.
//!
//! ## How expected values were obtained
//!
//! Each case is derived from the source formula or a source-stated constant and
//! asserted exactly or within a stated tolerance. Where a value depends on a
//! long chain of internal state, the assertion is a **documented invariant**
//! (monotonicity, clamping, range) rather than a magic number — inventing a
//! precise number without the source oracle would be fabrication.

use kivi_physics::dtc::{
    DtcCategory, DtcCode, DtcSwitchMatrix, DtcSwitchState, FaultDebounceIntegrator,
};

// ---------------------------------------------------------------------------
// DTC parsing and formatting
// ---------------------------------------------------------------------------

#[test]
fn parses_a_standard_powertrain_code() {
    let c = DtcCode::parse("P0420").expect("parse");
    assert_eq!(c.category, DtcCategory::Powertrain);
    assert!(c.is_standard);
    assert_eq!(c.code_hex, 0x0420);
}

#[test]
fn round_trips_through_string() {
    for text in ["P0420", "P1234", "P2000", "P3000", "C0300", "C2123", "B1000", "U0100", "U3456"] {
        let c = DtcCode::parse(text).expect("parse");
        assert_eq!(c.to_string(), text, "round trip failed for {text}");
    }
}

#[test]
fn parses_each_category_prefix() {
    assert_eq!(DtcCode::parse("P0001").unwrap().category, DtcCategory::Powertrain);
    assert_eq!(DtcCode::parse("C0001").unwrap().category, DtcCategory::Chassis);
    assert_eq!(DtcCode::parse("B0001").unwrap().category, DtcCategory::Body);
    assert_eq!(DtcCode::parse("U0001").unwrap().category, DtcCategory::Network);
}

#[test]
fn second_digit_selects_standard_vs_manufacturer() {
    assert!(DtcCode::parse("P0001").unwrap().is_standard);
    assert!(!DtcCode::parse("P1001").unwrap().is_standard);
    assert!(DtcCode::parse("P2001").unwrap().is_standard);
    assert!(!DtcCode::parse("P3001").unwrap().is_standard);
}

#[test]
fn rejects_a_wrong_length() {
    assert!(DtcCode::parse("P042").is_err());
    assert!(DtcCode::parse("P04201").is_err());
}

#[test]
fn rejects_an_invalid_prefix() {
    assert!(DtcCode::parse("X0420").is_err());
}

#[test]
fn rejects_an_invalid_second_digit() {
    assert!(DtcCode::parse("P9420").is_err());
}

#[test]
fn rejects_non_hex_digits() {
    assert!(DtcCode::parse("P0ZZZ").is_err());
}

#[test]
fn parsing_is_case_insensitive_on_the_prefix() {
    assert_eq!(DtcCode::parse("p0420").unwrap().category, DtcCategory::Powertrain);
}

// ---------------------------------------------------------------------------
// Fault debounce
// ---------------------------------------------------------------------------

#[test]
fn debounce_defaults_match_the_source() {
    let d = FaultDebounceIntegrator::default();
    assert_eq!(d.fault_counter, 0);
    assert_eq!(d.trip_threshold, 40);
    assert_eq!(d.clear_threshold, -40);
}

#[test]
fn a_fault_trips_exactly_at_the_threshold() {
    let mut d = FaultDebounceIntegrator::default();
    for _ in 0..39 {
        assert!(!d.update(true), "must not trip before 40");
    }
    assert!(d.update(true), "the 40th positive must trip");
}

#[test]
fn the_counter_is_capped_above_the_threshold() {
    let mut d = FaultDebounceIntegrator::default();
    for _ in 0..100 {
        d.update(true);
    }
    assert_eq!(d.fault_counter, 45, "capped at trip_threshold + 5");
}

#[test]
fn the_counter_is_capped_below_the_clear_threshold() {
    let mut d = FaultDebounceIntegrator::default();
    for _ in 0..100 {
        d.update(false);
    }
    assert_eq!(d.fault_counter, -45, "capped at clear_threshold - 5");
}

#[test]
fn reset_zeroes_the_counter() {
    let mut d = FaultDebounceIntegrator::default();
    for _ in 0..50 {
        d.update(true);
    }
    d.reset();
    assert_eq!(d.fault_counter, 0);
    assert!(!d.update(true), "a single symptom after reset must not trip");
}

// ---------------------------------------------------------------------------
// DTC switch matrix
// ---------------------------------------------------------------------------

#[test]
fn disabled_state_is_zero() {
    assert_eq!(DtcSwitchState::Disabled as u8, 0x00);
    assert_eq!(DtcSwitchState::ActiveNoMil as u8, 0x01);
    assert_eq!(DtcSwitchState::ActiveMilTwoTrip as u8, 0x02);
    assert_eq!(DtcSwitchState::ActiveMilImmediate as u8, 0x03);
}

#[test]
fn patching_a_dtc_switch_writes_zero() {
    let mut rom = vec![0x02u8; 16];
    DtcSwitchMatrix::patch_dtc_disable(&mut rom, 5).expect("patch");
    assert_eq!(rom[5], 0x00);
    // Everything else is untouched.
    assert_eq!(rom[4], 0x02);
    assert_eq!(rom[6], 0x02);
}

#[test]
fn patching_out_of_bounds_is_an_error_not_a_panic() {
    let mut rom = vec![0u8; 4];
    let err = DtcSwitchMatrix::patch_dtc_disable(&mut rom, 10).expect_err("must reject");
    assert_eq!(err, "DTC switch address exceeds ROM size");
}

#[test]
fn patching_the_last_byte_is_allowed() {
    let mut rom = vec![0x01u8; 4];
    DtcSwitchMatrix::patch_dtc_disable(&mut rom, 3).expect("patch");
    assert_eq!(rom[3], 0x00);
}
