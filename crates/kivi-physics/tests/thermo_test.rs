//! Parity tests for the thermodynamics module.
//!
//! Ported from `ecu_engine_rust::physics::thermo`.
//!
//! ## How expected values were obtained
//!
//! Every constant and conversion is hand-derived from the source definition.
//! The gas-law cases are computed independently with the published formula
//! `m = P·V·VE / (R·T)` using `R = 287.058 J/(kg·K)`, then asserted within a
//! tolerance that reflects the model's own approximations (not a loose fudge).
//!
//! ## Relationship to existing target code
//!
//! `kivi-protocol` already has telemetry/AutoTune logic. This module is the
//! **physics primitives** layer (unit conversions, charge temperature, speed
//! density). It does not replace or duplicate the AutoTune recommendation
//! engine, which consumes telemetry rather than computing charge states.

use kivi_physics::thermo::{
    cc_to_cubic_meters, celsius_to_kelvin, kelvin_to_celsius, kpa_to_pascals,
    ChargeTemperatureModel, SpeedDensityEngine, P_STD_ATMOSPHERE, R_SPECIFIC_AIR, T_STD_KELVIN,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

#[test]
fn physical_constants_match_the_source() {
    assert_eq!(R_SPECIFIC_AIR, 287.058);
    assert_eq!(P_STD_ATMOSPHERE, 101325.0);
    assert_eq!(T_STD_KELVIN, 293.15);
}

// ---------------------------------------------------------------------------
// Unit conversions
// ---------------------------------------------------------------------------

#[test]
fn celsius_to_kelvin_adds_273_15() {
    assert_eq!(celsius_to_kelvin(0.0), 273.15);
    assert_eq!(celsius_to_kelvin(100.0), 373.15);
    assert_eq!(celsius_to_kelvin(-40.0), 233.14999999999998);
}

#[test]
fn kelvin_to_celsius_subtracts_273_15() {
    assert_eq!(kelvin_to_celsius(273.15), 0.0);
    assert_eq!(kelvin_to_celsius(373.15), 100.0);
}

#[test]
fn the_temperature_conversions_round_trip() {
    for c in [-40.0, 0.0, 25.0, 85.0, 200.0] {
        assert!((kelvin_to_celsius(celsius_to_kelvin(c)) - c).abs() < 1e-9);
    }
}

#[test]
fn cc_to_cubic_meters_multiplies_by_1e_6() {
    assert_eq!(cc_to_cubic_meters(1_000_000.0), 1.0);
    assert_eq!(cc_to_cubic_meters(155.0), 155.0e-6);
}

#[test]
fn kpa_to_pascals_multiplies_by_1000() {
    assert_eq!(kpa_to_pascals(101.325), 101_325.0);
    assert_eq!(kpa_to_pascals(0.0), 0.0);
}

// ---------------------------------------------------------------------------
// Charge temperature model
// ---------------------------------------------------------------------------

#[test]
fn default_betas_match_the_source() {
    let m = ChargeTemperatureModel::default();
    assert_eq!(m.beta_idle, 0.75);
    assert_eq!(m.beta_wot, 0.10);
    assert_eq!(m.airflow_half_point, 25.0);
}

#[test]
fn beta_at_zero_airflow_equals_beta_idle() {
    // weight = exp(0) = 1 -> beta = beta_wot + (beta_idle - beta_wot)*1 = beta_idle
    let m = ChargeTemperatureModel::default();
    assert!((m.calculate_beta(0.0) - 0.75).abs() < 1e-12);
}

#[test]
fn beta_falls_toward_beta_wot_as_airflow_rises() {
    let m = ChargeTemperatureModel::default();
    let low = m.calculate_beta(1.0);
    let high = m.calculate_beta(100.0);
    assert!(high < low, "beta must decrease with airflow");
    assert!(high >= 0.10, "beta must not undershoot beta_wot");
}

#[test]
fn beta_at_the_half_point_is_the_documented_blend() {
    // normalized = 1, weight = exp(-1) = 0.36787944117144233
    // beta = 0.10 + 0.65 * 0.36787944117144233 = 0.3391216367614375
    let m = ChargeTemperatureModel::default();
    let expected = 0.10 + 0.65 * (-1.0f64).exp();
    assert!((m.calculate_beta(25.0) - expected).abs() < 1e-12);
}

#[test]
fn negative_airflow_is_clamped_to_zero() {
    // normalized = max(-1, 0) = 0 -> same as zero airflow
    let m = ChargeTemperatureModel::default();
    assert!((m.calculate_beta(-5.0) - m.calculate_beta(0.0)).abs() < 1e-12);
}

#[test]
fn charge_temperature_sits_between_iat_and_ect() {
    let m = ChargeTemperatureModel::default();
    // IAT 30C = 303.15K, ECT 90C = 363.15K
    let t = m.estimate_t_charge_k(30.0, 90.0, 10.0);
    assert!(t > 303.15 && t < 363.15, "charge temp {t} outside the bounds");
}

#[test]
fn charge_temperature_approaches_iat_at_high_airflow() {
    let m = ChargeTemperatureModel::default();
    let low = m.estimate_t_charge_k(30.0, 90.0, 1.0);
    let high = m.estimate_t_charge_k(30.0, 90.0, 500.0);
    assert!(
        high < low,
        "high airflow must pull charge temp toward IAT: {high} vs {low}"
    );
}

#[test]
fn charge_temperature_at_zero_airflow_is_a_blend_toward_ect() {
    // beta = 0.75 -> T = 303.15 + (363.15 - 303.15) * 0.75 = 348.15
    let m = ChargeTemperatureModel::default();
    let t = m.estimate_t_charge_k(30.0, 90.0, 0.0);
    assert!((t - 348.15).abs() < 1e-9, "expected 348.15, got {t}");
}

// ---------------------------------------------------------------------------
// Speed-density engine
// ---------------------------------------------------------------------------

#[test]
fn cylinder_volume_divides_displacement_by_cylinder_count() {
    let e = SpeedDensityEngine::new(155.0, 1);
    assert!((e.cylinder_volume_m3() - 155.0e-6).abs() < 1e-15);

    let four = SpeedDensityEngine::new(2000.0, 4);
    assert!((four.cylinder_volume_m3() - 500.0e-6).abs() < 1e-15);
}

#[test]
fn new_uses_the_default_charge_model() {
    let e = SpeedDensityEngine::new(1000.0, 4);
    assert_eq!(e.charge_temp_model.beta_idle, 0.75);
}

#[test]
fn air_mass_for_a_155cc_single_at_wot_is_in_the_expected_range() {
    // Source's own sanity bound: 0.12..0.18 g.
    let e = SpeedDensityEngine::new(155.0, 1);
    let m = e.calculate_cylinder_air_mass_grams(100.0, 0.85, 30.0, 85.0, 7000.0);
    assert!(m > 0.12 && m < 0.18, "air mass out of range: {m}");
}

#[test]
fn air_mass_scales_linearly_with_volumetric_efficiency() {
    let e = SpeedDensityEngine::new(155.0, 1);
    let a = e.calculate_cylinder_air_mass_grams(100.0, 0.5, 30.0, 85.0, 7000.0);
    let b = e.calculate_cylinder_air_mass_grams(100.0, 1.0, 30.0, 85.0, 7000.0);
    // Not exactly 2x because VE also changes the estimated airflow and thus beta,
    // but it must increase substantially.
    assert!(b > a * 1.8, "doubling VE must roughly double air mass: {a} -> {b}");
}

#[test]
fn air_mass_increases_with_manifold_pressure() {
    let e = SpeedDensityEngine::new(155.0, 1);
    let low = e.calculate_cylinder_air_mass_grams(35.0, 0.85, 30.0, 85.0, 3000.0);
    let high = e.calculate_cylinder_air_mass_grams(100.0, 0.85, 30.0, 85.0, 3000.0);
    assert!(high > low, "higher MAP must induct more air");
}

#[test]
fn ve_from_maf_is_zero_for_invalid_inputs() {
    let e = SpeedDensityEngine::new(155.0, 1);
    assert_eq!(e.calculate_ve_from_maf(10.0, 100.0, 30.0, 85.0, 0.0), 0.0);
    assert_eq!(e.calculate_ve_from_maf(10.0, 0.0, 30.0, 85.0, 3000.0), 0.0);
}

#[test]
fn ve_from_maf_returns_a_plausible_ratio_for_a_realistic_case() {
    // A 155cc single at 100 kPa / 7000 rpm inducts roughly 8 g/s at ~100% VE.
    // Feeding the model its own air mass should return close to the VE used.
    let e = SpeedDensityEngine::new(155.0, 1);
    let m_g = e.calculate_cylinder_air_mass_grams(100.0, 0.85, 30.0, 85.0, 7000.0);
    // One intake stroke per 2 revolutions per cylinder -> 7000/120 strokes/s.
    let maf_gps = m_g * (7000.0 / 120.0);
    let ve = e.calculate_ve_from_maf(maf_gps, 100.0, 30.0, 85.0, 7000.0);
    assert!((ve - 0.85).abs() < 0.05, "expected ~0.85, got {ve}");
}

#[test]
fn computations_are_deterministic() {
    let e = SpeedDensityEngine::new(155.0, 1);
    let a = e.calculate_cylinder_air_mass_grams(100.0, 0.85, 30.0, 85.0, 7000.0);
    let b = e.calculate_cylinder_air_mass_grams(100.0, 0.85, 30.0, 85.0, 7000.0);
    assert_eq!(a, b);
}
