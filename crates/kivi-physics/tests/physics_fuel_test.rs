//! Parity tests for the fuel metering module.
//!
//! Ported from `ecu_engine_rust::physics::fuel`.
//!
//! ## How expected values were obtained
//!
//! AFR and density tables are asserted directly against the source constants.
//! Flow-rate and dead-time expectations are hand-computed from the documented
//! formulas (`Q·√(P/P_ref)`, linear interpolation). Pulse-width checks use the
//! source's own stated sanity ranges plus an exact algebraic identity.

use kivi_physics::fuel::{FuelController, FuelInjector, FuelType, WallWettingModel};

// ---------------------------------------------------------------------------
// Fuel type tables
// ---------------------------------------------------------------------------

#[test]
fn stoichiometric_afr_matches_the_source_table() {
    assert_eq!(FuelType::PureGasoline.stoichiometric_afr(), 14.7);
    assert_eq!(FuelType::E10Gasoline.stoichiometric_afr(), 14.13);
    assert_eq!(FuelType::E85Ethanol.stoichiometric_afr(), 9.76);
    assert_eq!(FuelType::PureMethanol.stoichiometric_afr(), 6.47);
    assert_eq!(FuelType::Diesel.stoichiometric_afr(), 14.5);
    assert_eq!(FuelType::Custom(12.0).stoichiometric_afr(), 12.0);
}

#[test]
fn density_matches_the_source_table() {
    assert_eq!(FuelType::PureGasoline.density_g_per_cc(), 0.737);
    assert_eq!(FuelType::E10Gasoline.density_g_per_cc(), 0.745);
    assert_eq!(FuelType::E85Ethanol.density_g_per_cc(), 0.785);
    assert_eq!(FuelType::PureMethanol.density_g_per_cc(), 0.792);
    assert_eq!(FuelType::Diesel.density_g_per_cc(), 0.832);
    // Custom falls back to a nominal 0.740 rather than panicking.
    assert_eq!(FuelType::Custom(12.0).density_g_per_cc(), 0.740);
}

// ---------------------------------------------------------------------------
// Injector
// ---------------------------------------------------------------------------

#[test]
fn default_injector_matches_the_source() {
    let i = FuelInjector::default();
    assert_eq!(i.flow_rate_cc_min, 200.0);
    assert_eq!(i.ref_pressure_kpa, 300.0);
    assert_eq!(i.dead_time_volts, [8.0, 10.0, 12.0, 13.5, 14.0, 16.0]);
    assert_eq!(i.dead_time_ms, [1.85, 1.42, 1.08, 0.92, 0.88, 0.74]);
}

#[test]
fn flow_rate_at_reference_pressure_is_the_unscaled_conversion() {
    // 200 cc/min at 300 kPa (scale = 1) for gasoline (0.737 g/cc):
    // 200 * 0.737 * 1000 / 60000 = 2.45666... mg/ms
    let i = FuelInjector::default();
    let f = i.flow_rate_mg_per_ms(&FuelType::PureGasoline, 300.0);
    let expected = (200.0 * 0.737 * 1000.0) / 60000.0;
    assert!((f - expected).abs() < 1e-12, "got {f}, expected {expected}");
}

#[test]
fn flow_rate_scales_with_the_square_root_of_pressure() {
    // Doubling pressure multiplies flow by sqrt(2).
    let i = FuelInjector::default();
    let base = i.flow_rate_mg_per_ms(&FuelType::PureGasoline, 300.0);
    let doubled = i.flow_rate_mg_per_ms(&FuelType::PureGasoline, 600.0);
    assert!(
        (doubled / base - 2.0f64.sqrt()).abs() < 1e-12,
        "expected sqrt(2) scaling, got {}",
        doubled / base
    );
}

#[test]
fn flow_rate_clamps_the_pressure_ratio_at_0_1() {
    // A near-zero pressure must not collapse flow to zero.
    let i = FuelInjector::default();
    let near_zero = i.flow_rate_mg_per_ms(&FuelType::PureGasoline, 0.0);
    let at_0_1 = i.flow_rate_mg_per_ms(&FuelType::PureGasoline, 30.0); // 0.1 * 300
    assert!((near_zero - at_0_1).abs() < 1e-12);
}

#[test]
fn dead_time_is_clamped_below_the_first_and_above_the_last_point() {
    let i = FuelInjector::default();
    assert_eq!(i.get_dead_time_ms(6.0), 1.85, "below range clamps to first");
    assert_eq!(i.get_dead_time_ms(20.0), 0.74, "above range clamps to last");
}

#[test]
fn dead_time_interpolates_linearly_between_points() {
    let i = FuelInjector::default();
    // Between 12.0V (1.08ms) and 13.5V (0.92ms), midpoint 12.75V:
    // 1.08 + 0.5 * (0.92 - 1.08) = 1.00
    let mid = i.get_dead_time_ms(12.75);
    assert!((mid - 1.00).abs() < 1e-12, "expected 1.00, got {mid}");
}

#[test]
fn dead_time_at_an_exact_node_returns_that_node() {
    let i = FuelInjector::default();
    assert!((i.get_dead_time_ms(13.5) - 0.92).abs() < 1e-12);
}

// ---------------------------------------------------------------------------
// Wall wetting
// ---------------------------------------------------------------------------

#[test]
fn wall_wetting_defaults_match_the_source() {
    let w = WallWettingModel::default();
    assert_eq!(w.x_fraction, 0.28);
    assert_eq!(w.tau_seconds, 0.22);
    assert_eq!(w.puddle_mass_mg, 0.0);
}

#[test]
fn an_empty_puddle_requires_injection_grossed_up_by_the_stick_fraction() {
    // puddle 0 -> m_evap = 0 -> m_injected = desired / (1 - 0.28)
    let mut w = WallWettingModel::default();
    let injected = w.calculate_injected_fuel_mg(10.0, 0.01);
    let expected = 10.0 / 0.72;
    assert!((injected - expected).abs() < 1e-12, "got {injected}");
}

#[test]
fn wall_wetting_accumulates_a_puddle_over_successive_cycles() {
    let mut w = WallWettingModel::default();
    let _ = w.calculate_injected_fuel_mg(10.0, 0.01);
    assert!(w.puddle_mass_mg > 0.0, "a puddle must form");
}

#[test]
fn wall_wetting_never_returns_a_negative_injection() {
    let mut w = WallWettingModel {
        // Huge existing puddle with zero demand: evaporation exceeds demand.
        puddle_mass_mg: 1000.0,
        ..Default::default()
    };
    let injected = w.calculate_injected_fuel_mg(0.0, 0.05);
    assert!(injected >= 0.0, "injection must be clamped at zero");
}

#[test]
fn the_puddle_can_never_go_negative() {
    let mut w = WallWettingModel {
        puddle_mass_mg: 0.001,
        ..Default::default()
    };
    let _ = w.calculate_injected_fuel_mg(0.0, 1.0);
    assert!(w.puddle_mass_mg >= 0.0);
}

// ---------------------------------------------------------------------------
// Fuel controller
// ---------------------------------------------------------------------------

#[test]
fn pulse_width_is_gross_equals_effective_plus_dead_time() {
    let mut c = FuelController::new(FuelType::E10Gasoline, FuelInjector::default());
    let (gross, eff, dead) = c.calculate_pulse_width_ms(0.15, 0.88, 13.8, 300.0, 120.0 / 7000.0);
    assert!(
        (gross - (eff + dead)).abs() < 1e-12,
        "gross {gross} must equal effective {eff} + dead {dead}"
    );
}

#[test]
fn pulse_width_lands_in_the_source_sanity_range() {
    let mut c = FuelController::new(FuelType::E10Gasoline, FuelInjector::default());
    let (gross, _, dead) = c.calculate_pulse_width_ms(0.15, 0.88, 13.8, 300.0, 120.0 / 7000.0);
    assert!(gross > 3.0 && gross < 12.0, "gross PW out of range: {gross}");
    assert!(dead > 0.7 && dead < 1.1, "dead time out of range: {dead}");
}

#[test]
fn a_richer_lambda_requires_more_fuel() {
    let mut lean = FuelController::new(FuelType::PureGasoline, FuelInjector::default());
    let mut rich = FuelController::new(FuelType::PureGasoline, FuelInjector::default());
    let (lean_pw, _, _) = lean.calculate_pulse_width_ms(0.15, 1.05, 13.8, 300.0, 0.017);
    let (rich_pw, _, _) = rich.calculate_pulse_width_ms(0.15, 0.85, 13.8, 300.0, 0.017);
    assert!(rich_pw > lean_pw, "richer target must need a longer pulse");
}

#[test]
fn new_attaches_a_default_wall_wetting_model() {
    let c = FuelController::new(FuelType::Diesel, FuelInjector::default());
    assert_eq!(c.wall_wetting.x_fraction, 0.28);
}
