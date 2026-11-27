//! Parity tests for the ignition module.
//!
//! Ported from `ecu_engine_rust::physics::ignition`.
//!
//! ## How expected values were obtained
//!
//! Wiebe parameters are asserted from the source. MFB values are hand-computed
//! from `x_b = 1 - exp(-a·prog^(m+1))`, and the MFB50 inversion is checked
//! algebraically. Knock and clamp behaviour is asserted at the exact boundaries.

use kivi_physics::ignition::{
    IgnitionController, KnockRetardController, WiebeCombustionModel,
};

// ---------------------------------------------------------------------------
// Wiebe model
// ---------------------------------------------------------------------------

#[test]
fn wiebe_defaults_match_the_source() {
    let w = WiebeCombustionModel::default();
    assert_eq!(w.a, 5.0);
    assert_eq!(w.m, 2.0);
    assert_eq!(w.duration_degrees, 70.0);
}

#[test]
fn nothing_is_burned_before_the_spark() {
    let w = WiebeCombustionModel::default();
    // theta_0 = -28; at -30 ATDC we are before the spark.
    assert_eq!(w.mass_fraction_burned(-30.0, 28.0), 0.0);
}

#[test]
fn everything_is_burned_after_the_duration() {
    let w = WiebeCombustionModel::default();
    // theta_0 = -28, duration 70 -> fully burned by +42 ATDC.
    assert_eq!(w.mass_fraction_burned(50.0, 28.0), 1.0);
}

#[test]
fn mass_fraction_at_the_spark_angle_is_zero() {
    let w = WiebeCombustionModel::default();
    assert_eq!(w.mass_fraction_burned(-28.0, 28.0), 0.0);
}

#[test]
fn mass_fraction_is_monotonic_over_the_burn() {
    let w = WiebeCombustionModel::default();
    let mut prev = -1.0;
    for deg in [-20.0, -10.0, 0.0, 10.0, 20.0, 30.0] {
        let x = w.mass_fraction_burned(deg, 28.0);
        assert!(x >= prev, "MFB must not decrease: {x} < {prev} at {deg}");
        assert!((0.0..=1.0).contains(&x));
        prev = x;
    }
}

#[test]
fn mass_fraction_matches_the_hand_computed_wiebe_value() {
    // prog = (0 - (-28)) / 70 = 0.4
    // x = 1 - exp(-5 * 0.4^3) = 1 - exp(-0.32)
    let w = WiebeCombustionModel::default();
    let x = w.mass_fraction_burned(0.0, 28.0);
    let expected = 1.0 - (-5.0 * 0.4f64.powi(3)).exp();
    assert!((x - expected).abs() < 1e-12, "got {x}, expected {expected}");
}

#[test]
fn mfb50_lands_in_the_source_sanity_window() {
    let w = WiebeCombustionModel::default();
    let a = w.calculate_mfb50_angle_atdc(28.0);
    assert!(a > 6.0 && a < 12.0, "MFB50 angle out of range: {a}");
}

#[test]
fn mfb50_inverts_the_wiebe_equation() {
    // Feeding the MFB50 angle back through mass_fraction_burned must give 0.5.
    let w = WiebeCombustionModel::default();
    let a50 = w.calculate_mfb50_angle_atdc(28.0);
    let x = w.mass_fraction_burned(a50, 28.0);
    assert!((x - 0.5).abs() < 1e-9, "expected MFB 0.5 at MFB50, got {x}");
}

#[test]
fn more_advance_moves_mfb50_earlier() {
    let w = WiebeCombustionModel::default();
    let early = w.calculate_mfb50_angle_atdc(35.0);
    let late = w.calculate_mfb50_angle_atdc(20.0);
    assert!(early < late, "more advance must move MFB50 earlier");
}

// ---------------------------------------------------------------------------
// Knock retard
// ---------------------------------------------------------------------------

#[test]
fn knock_defaults_match_the_source() {
    let k = KnockRetardController::default();
    assert_eq!(k.current_retard_deg, 0.0);
    assert_eq!(k.retard_step_deg, 2.0);
    assert_eq!(k.max_retard_deg, 12.0);
    assert_eq!(k.recovery_rate_deg_per_sec, 0.75);
}

#[test]
fn a_knock_event_adds_one_step() {
    let mut k = KnockRetardController::default();
    k.on_knock_detected(1.0);
    assert_eq!(k.current_retard_deg, 2.0);
}

#[test]
fn a_severity_below_one_is_floored_at_one() {
    let mut k = KnockRetardController::default();
    k.on_knock_detected(0.1);
    assert_eq!(k.current_retard_deg, 2.0, "severity is clamped to >= 1.0");
}

#[test]
fn a_severe_knock_multiplies_the_step() {
    let mut k = KnockRetardController::default();
    k.on_knock_detected(3.0);
    assert_eq!(k.current_retard_deg, 6.0);
}

#[test]
fn retard_saturates_at_the_maximum() {
    let mut k = KnockRetardController::default();
    for _ in 0..20 {
        k.on_knock_detected(1.0);
    }
    assert_eq!(k.current_retard_deg, 12.0);
}

#[test]
fn recovery_decays_at_the_configured_rate() {
    let mut k = KnockRetardController::default();
    k.on_knock_detected(1.0); // 2.0 deg
    k.update_recovery(2.0); // -1.5 deg
    assert!((k.current_retard_deg - 0.5).abs() < 1e-9);
}

#[test]
fn recovery_never_goes_below_zero() {
    let mut k = KnockRetardController::default();
    k.on_knock_detected(1.0);
    k.update_recovery(100.0);
    assert_eq!(k.current_retard_deg, 0.0);
}

#[test]
fn recovery_with_no_retard_is_a_no_op() {
    let mut k = KnockRetardController::default();
    k.update_recovery(5.0);
    assert_eq!(k.current_retard_deg, 0.0);
}

// ---------------------------------------------------------------------------
// Ignition controller
// ---------------------------------------------------------------------------

#[test]
fn ignition_defaults_match_the_source() {
    let c = IgnitionController::default();
    assert_eq!(c.max_spark_advance_btdc, 55.0);
    assert_eq!(c.min_spark_advance_btdc, -10.0);
}

#[test]
fn corrections_are_summed_onto_the_base_timing() {
    let c = IgnitionController::default();
    let t = c.calculate_commanded_spark_timing(20.0, 3.0, -2.0);
    assert!((t - 21.0).abs() < 1e-12);
}

#[test]
fn knock_retard_is_subtracted() {
    let mut c = IgnitionController::default();
    c.knock_controller.on_knock_detected(1.0); // 2 deg retard
    let t = c.calculate_commanded_spark_timing(20.0, 0.0, 0.0);
    assert!((t - 18.0).abs() < 1e-12);
}

#[test]
fn timing_is_clamped_to_the_maximum() {
    let c = IgnitionController::default();
    let t = c.calculate_commanded_spark_timing(50.0, 10.0, 5.0);
    assert_eq!(t, 55.0);
}

#[test]
fn timing_is_clamped_to_the_minimum() {
    let c = IgnitionController::default();
    let t = c.calculate_commanded_spark_timing(-20.0, 0.0, 0.0);
    assert_eq!(t, -10.0);
}

#[test]
fn heavy_knock_cannot_push_timing_below_the_floor() {
    let mut c = IgnitionController::default();
    for _ in 0..10 {
        c.knock_controller.on_knock_detected(1.0); // saturates at 12 deg
    }
    let t = c.calculate_commanded_spark_timing(-5.0, 0.0, 0.0);
    assert_eq!(t, -10.0, "the hard floor must hold");
}
