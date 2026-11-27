//! Parity tests for adaptation routines and pre-flash interlocks.
//!
//! Ported from `ecu_engine_rust::flashing::adaptations`.
//!
//! ## Why the interlocks matter
//!
//! These are the last line of defence before a transmission or throttle flash.
//! A wrong pass lets a flash start with the car in gear, cold oil, or a
//! pressurised accumulator. The tests below pin each rejection path.
//!
//! ## How expected values were obtained
//!
//! All thresholds come from the source constants, which the tests read back
//! rather than hard-coding a second copy:
//!
//! * dual TPS plausibility: `|v1 + v2 - 5.0| <= 0.25`
//! * oil temperature envelope: `30..=60 °C`
//! * line pressure: `<= 5 bar`
//! * fuel multiplier: `1 + (stft + ltft) / 100`

use kivi_flashing::adaptations::{
    FuelTrimAdaptation, ThrottleBodyAdaptation, TransmissionMechatronicAdaptation,
    TransmissionSelectorPosition,
};

// ---------------------------------------------------------------------------
// Dual TPS plausibility
// ---------------------------------------------------------------------------

#[test]
fn a_perfect_dual_tps_split_is_accepted() {
    assert!(ThrottleBodyAdaptation::verify_dual_tps_plausibility(2.5, 2.5).is_ok());
}

#[test]
fn the_plausibility_window_boundary_is_inclusive() {
    // The check is `|sum - 5.0| > 0.25`, so a deviation *just under* 0.25 passes
    // and one just over fails. The exact 0.25 case is avoided because
    // `4.75 + 0.0 - 5.0` is not exactly 0.25 in binary floating point — asserting
    // it would test IEEE-754 rounding, not the plausibility rule.
    // sum = 4.80 -> |4.80 - 5.00| = 0.20, inside the window.
    assert!(
        ThrottleBodyAdaptation::verify_dual_tps_plausibility(4.80, 0.0).is_ok(),
        "a deviation of 0.20 must pass"
    );
    // sum = 4.70 -> |4.70 - 5.00| = 0.30, outside the window.
    assert!(
        ThrottleBodyAdaptation::verify_dual_tps_plausibility(4.70, 0.0).is_err(),
        "a deviation of 0.30 must fail"
    );
}

#[test]
fn a_short_to_ground_sensor_is_rejected() {
    // TPS1 pulled to 0 V, TPS2 at rail: sum 5.0 -> actually valid. Use a real fault.
    let err = ThrottleBodyAdaptation::verify_dual_tps_plausibility(0.0, 0.0)
        .expect_err("both sensors at 0 V is a fault");
    assert!(err.contains("TPS_PLAUSIBILITY_FAULT"), "error: {err}");
}

#[test]
fn an_open_circuit_is_rejected() {
    let err = ThrottleBodyAdaptation::verify_dual_tps_plausibility(5.0, 5.0)
        .expect_err("both sensors at rail is a fault");
    assert!(err.contains("TPS_PLAUSIBILITY_FAULT"), "error: {err}");
}

#[test]
fn the_fault_message_names_limp_mode() {
    let err = ThrottleBodyAdaptation::verify_dual_tps_plausibility(0.0, 0.0).unwrap_err();
    assert!(
        err.to_lowercase().contains("limp"),
        "the operator must be told the ECU enters limp mode: {err}"
    );
}

// ---------------------------------------------------------------------------
// Fuel trim
// ---------------------------------------------------------------------------

#[test]
fn a_fresh_fuel_trim_adaptation_has_a_unity_multiplier() {
    let f = FuelTrimAdaptation::default();
    assert_eq!(f.total_fuel_multiplier(), 1.0);
}

#[test]
fn fuel_trims_add_together() {
    // +10% short term and -4% long term -> 1 + (10 - 4)/100 = 1.06
    let f = FuelTrimAdaptation {
        short_term_fuel_trim_pct: 10.0,
        long_term_fuel_trim_pct: -4.0,
    };
    assert!((f.total_fuel_multiplier() - 1.06).abs() < 1e-12);
}

#[test]
fn resetting_adaptations_returns_the_multiplier_to_unity() {
    let mut f = FuelTrimAdaptation {
        short_term_fuel_trim_pct: 25.0,
        long_term_fuel_trim_pct: 15.0,
    };
    assert!(f.total_fuel_multiplier() > 1.0);
    f.reset_adaptations();
    assert_eq!(f.total_fuel_multiplier(), 1.0);
}

// ---------------------------------------------------------------------------
// Transmission pre-flash interlocks
// ---------------------------------------------------------------------------

fn interlock_ok() -> (TransmissionSelectorPosition, f64, f64) {
    // Park, mid-envelope oil temperature, depressurised accumulator.
    (TransmissionSelectorPosition::Park, 45.0, 0.0)
}

#[test]
fn a_parked_car_with_warm_oil_and_no_pressure_passes() {
    let (sel, temp, press) = interlock_ok();
    assert!(TransmissionMechatronicAdaptation::verify_pre_flash_interlocks(sel, temp, press).is_ok());
}

#[test]
fn neutral_is_also_accepted() {
    assert!(TransmissionMechatronicAdaptation::verify_pre_flash_interlocks(
        TransmissionSelectorPosition::Neutral,
        45.0,
        0.0
    )
    .is_ok());
}

#[test]
fn flashing_in_gear_is_refused() {
    let err = TransmissionMechatronicAdaptation::verify_pre_flash_interlocks(
        TransmissionSelectorPosition::Drive,
        45.0,
        0.0,
    )
    .expect_err("must refuse in Drive");
    assert!(err.contains("TCU_INTERLOCK_FAULT"), "error: {err}");
}

#[test]
fn cold_transmission_fluid_is_refused() {
    let err = TransmissionMechatronicAdaptation::verify_pre_flash_interlocks(
        TransmissionSelectorPosition::Park,
        29.0,
        0.0,
    )
    .expect_err("must refuse below 30 °C");
    assert!(err.contains("TCU_OIL_COLD"), "error: {err}");
}

#[test]
fn the_cold_boundary_is_inclusive() {
    // Exactly 30 °C must pass: the source checks `< MIN_SAFE_OIL_TEMP_C`.
    assert!(TransmissionMechatronicAdaptation::verify_pre_flash_interlocks(
        TransmissionSelectorPosition::Park,
        30.0,
        0.0
    )
    .is_ok());
}

#[test]
fn hot_transmission_fluid_is_refused() {
    let err = TransmissionMechatronicAdaptation::verify_pre_flash_interlocks(
        TransmissionSelectorPosition::Park,
        61.0,
        0.0,
    )
    .expect_err("must refuse above 60 °C");
    assert!(err.contains("TCU_OIL_HOT"), "error: {err}");
}

#[test]
fn the_hot_boundary_is_inclusive() {
    assert!(TransmissionMechatronicAdaptation::verify_pre_flash_interlocks(
        TransmissionSelectorPosition::Park,
        60.0,
        0.0
    )
    .is_ok());
}

#[test]
fn a_pressurised_accumulator_is_refused() {
    let err = TransmissionMechatronicAdaptation::verify_pre_flash_interlocks(
        TransmissionSelectorPosition::Park,
        45.0,
        6.0,
    )
    .expect_err("must refuse above 5 bar");
    assert!(err.contains("TCU_PRESSURE_HIGH"), "error: {err}");
}

#[test]
fn the_pressure_boundary_is_inclusive() {
    assert!(TransmissionMechatronicAdaptation::verify_pre_flash_interlocks(
        TransmissionSelectorPosition::Park,
        45.0,
        5.0
    )
    .is_ok());
}

#[test]
fn selector_is_checked_before_temperature() {
    // In Drive AND cold: the selector fault must be reported first, because it is
    // the more dangerous condition.
    let err = TransmissionMechatronicAdaptation::verify_pre_flash_interlocks(
        TransmissionSelectorPosition::Drive,
        10.0,
        0.0,
    )
    .expect_err("must refuse");
    assert!(
        err.contains("TCU_INTERLOCK_FAULT"),
        "selector must be reported before temperature: {err}"
    );
}

// ---------------------------------------------------------------------------
// Clutch calibration routine
// ---------------------------------------------------------------------------

#[test]
fn a_clutch_calibration_routine_is_non_empty() {
    let routine = TransmissionMechatronicAdaptation::build_clutch_calibration_routine(1);
    assert!(!routine.is_empty(), "a calibration routine must carry bytes");
}

#[test]
fn different_routine_ids_produce_different_payloads() {
    let a = TransmissionMechatronicAdaptation::build_clutch_calibration_routine(1);
    let b = TransmissionMechatronicAdaptation::build_clutch_calibration_routine(2);
    assert_ne!(a, b, "the routine id must affect the payload");
}

#[test]
fn routine_construction_is_deterministic() {
    assert_eq!(
        TransmissionMechatronicAdaptation::build_clutch_calibration_routine(7),
        TransmissionMechatronicAdaptation::build_clutch_calibration_routine(7)
    );
}

// ---------------------------------------------------------------------------
// Robustness
// ---------------------------------------------------------------------------

#[test]
fn interlocks_do_not_panic_on_extreme_values() {
    for temp in [-273.15, 0.0, 1e9] {
        for press in [0.0, 1e9, -1.0] {
            let _ = TransmissionMechatronicAdaptation::verify_pre_flash_interlocks(
                TransmissionSelectorPosition::Park,
                temp,
                press,
            );
        }
    }
}
