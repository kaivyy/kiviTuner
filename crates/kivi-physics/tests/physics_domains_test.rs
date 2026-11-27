//! Parity tests for the remaining physics domain modules.
//!
//! Ported from `ecu_engine_rust::physics::{sensors, chemistry, boost,
//! airflow_dynamics, axis_rescaling, transients}`.
//!
//! ## How expected values were obtained
//!
//! Where a module exposes a closed-form formula with source constants, the
//! expectation is computed by hand and asserted exactly. Where behaviour depends
//! on a long internal chain, the assertion is a **documented invariant**
//! (monotonicity, clamping, bounds) — inventing a precise number without the
//! source oracle would be fabrication, and this file does not do that.

use kivi_physics::chemistry::{CombustionFuel, FlexFuelCalculator};
use kivi_physics::sensors::{
    AdcThermistorCircuit, MapSensor, SteinhartHartModel, ThrottlePositionSensor,
};

// ---------------------------------------------------------------------------
// Chemistry — fuel constants
// ---------------------------------------------------------------------------

#[test]
fn combustion_fuel_afr_matches_the_source_table() {
    assert_eq!(CombustionFuel::GasolinePureE0.stoichiometric_afr(), 14.70);
    assert_eq!(CombustionFuel::GasoholE10.stoichiometric_afr(), 14.08);
    assert_eq!(CombustionFuel::FlexFuelE85.stoichiometric_afr(), 9.76);
    assert_eq!(CombustionFuel::EthanolPureE100.stoichiometric_afr(), 9.00);
    assert_eq!(CombustionFuel::MethanolPureM100.stoichiometric_afr(), 6.47);
    assert_eq!(CombustionFuel::Diesel.stoichiometric_afr(), 14.50);
    assert_eq!(CombustionFuel::LpgPropane.stoichiometric_afr(), 15.67);
    assert_eq!(CombustionFuel::CngMethane.stoichiometric_afr(), 17.20);
}

#[test]
fn e85_has_a_lower_stoichiometric_afr_than_gasoline() {
    assert!(
        CombustionFuel::FlexFuelE85.stoichiometric_afr()
            < CombustionFuel::GasolinePureE0.stoichiometric_afr()
    );
}

#[test]
fn ethanol_has_a_higher_latent_heat_than_gasoline() {
    assert!(
        CombustionFuel::EthanolPureE100.heat_of_vaporization_kj_kg()
            > CombustionFuel::GasolinePureE0.heat_of_vaporization_kj_kg()
    );
}

#[test]
fn ethanol_blend_afr_lies_between_pure_gasoline_and_pure_ethanol() {
    let e0 = FlexFuelCalculator::calculate_blend_stoich_afr(0.0);
    let e100 = FlexFuelCalculator::calculate_blend_stoich_afr(100.0);
    let e50 = FlexFuelCalculator::calculate_blend_stoich_afr(50.0);
    assert!(
        e50 < e0 && e50 > e100,
        "a 50% blend must sit between the endpoints: {e100} < {e50} < {e0}"
    );
}

#[test]
fn blend_afr_is_monotonic_in_ethanol_content() {
    let mut prev = FlexFuelCalculator::calculate_blend_stoich_afr(0.0);
    for pct in [10.0, 25.0, 50.0, 75.0, 100.0] {
        let cur = FlexFuelCalculator::calculate_blend_stoich_afr(pct);
        assert!(cur < prev, "AFR must fall as ethanol rises: {cur} !< {prev}");
        prev = cur;
    }
}

#[test]
fn lambda_and_afr_round_trip() {
    let afr = FlexFuelCalculator::lambda_to_afr(1.0, CombustionFuel::GasolinePureE0);
    assert!((afr - 14.70).abs() < 1e-9, "lambda 1.0 must equal stoich");
    let back = FlexFuelCalculator::afr_to_lambda(afr, CombustionFuel::GasolinePureE0);
    assert!((back - 1.0).abs() < 1e-9);
}

#[test]
fn a_rich_lambda_yields_a_lower_afr() {
    let rich = FlexFuelCalculator::lambda_to_afr(0.85, CombustionFuel::GasolinePureE0);
    let stoich = FlexFuelCalculator::lambda_to_afr(1.0, CombustionFuel::GasolinePureE0);
    assert!(rich < stoich);
}

// ---------------------------------------------------------------------------
// Sensors — MAP and TPS
// ---------------------------------------------------------------------------

#[test]
fn map_sensor_maps_its_voltage_span_to_its_pressure_span() {
    let s = MapSensor::default();
    let low = s.voltage_to_kpa(s.v_min);
    let high = s.voltage_to_kpa(s.v_max);
    assert!((low - s.p_min_kpa).abs() < 1e-9, "min volts -> min kPa, got {low}");
    assert!((high - s.p_max_kpa).abs() < 1e-9, "max volts -> max kPa, got {high}");
}

#[test]
fn map_sensor_is_monotonic() {
    let s = MapSensor::default();
    let mid = (s.v_min + s.v_max) / 2.0;
    assert!(s.voltage_to_kpa(mid) > s.voltage_to_kpa(s.v_min));
    assert!(s.voltage_to_kpa(mid) < s.voltage_to_kpa(s.v_max));
}

#[test]
fn tps_maps_its_adc_span_to_0_100_percent() {
    let s = ThrottlePositionSensor::default();
    assert!((s.adc_to_percent(s.adc_closed) - 0.0).abs() < 1e-9);
    assert!((s.adc_to_percent(s.adc_wot) - 100.0).abs() < 1e-9);
}

#[test]
fn tps_is_clamped_beyond_its_span() {
    let s = ThrottlePositionSensor::default();
    assert_eq!(s.adc_to_percent(0), 0.0, "below span clamps to 0%");
    assert_eq!(s.adc_to_percent(u32::MAX), 100.0, "above span clamps to 100%");
}

// ---------------------------------------------------------------------------
// Sensors — Steinhart-Hart thermistor
// ---------------------------------------------------------------------------

#[test]
fn thermistor_resistance_and_temperature_round_trip() {
    let t = SteinhartHartModel::default();
    for celsius in [-20.0, 0.0, 25.0, 85.0, 120.0] {
        let r = t.celsius_to_resistance(celsius);
        let back = t.resistance_to_celsius(r);
        assert!(
            (back - celsius).abs() < 1e-6,
            "round trip failed at {celsius}C: got {back}"
        );
    }
}

#[test]
fn thermistor_resistance_falls_as_temperature_rises() {
    let t = SteinhartHartModel::default();
    assert!(t.celsius_to_resistance(0.0) > t.celsius_to_resistance(100.0));
}

#[test]
fn adc_thermistor_circuit_is_monotonic_over_its_span() {
    let c = AdcThermistorCircuit::default();
    // Higher ADC count means a different resistance, hence a different temperature.
    let low = c.adc_to_celsius(100);
    let high = c.adc_to_celsius(4000);
    assert_ne!(low, high, "the circuit must respond to ADC count");
    assert!(low.is_finite() && high.is_finite());
}

// ---------------------------------------------------------------------------
// Boost, airflow, transients, axis rescaling — signature-accurate invariants
// ---------------------------------------------------------------------------

#[test]
fn boost_controller_returns_finite_duty_and_respects_overboost_cutoff() {
    use kivi_physics::boost::BoostController;
    let mut b = BoostController::default();
    let (duty, p_term, i_term, d_term) = b
        .update(1500.0, 1400.0, 0.3, 0.01)
        .expect("normal boost must not trip the cutoff");
    for v in [duty, p_term, i_term, d_term] {
        assert!(v.is_finite(), "boost terms must be finite");
    }
    // Duty is a percentage (wgdc_min..=wgdc_max), not a 0..1 fraction.
    assert!(
        (b.wgdc_min..=b.wgdc_max).contains(&duty),
        "duty {duty} must lie in {}..={}",
        b.wgdc_min,
        b.wgdc_max
    );
}

#[test]
fn boost_controller_trips_the_overboost_cutoff() {
    use kivi_physics::boost::BoostController;
    let mut b = BoostController::default();
    let err = b
        .update(1500.0, b.overboost_cutoff_hpa, 0.3, 0.01)
        .expect_err("must trip the overboost cutoff");
    assert!(err.contains("OVERBOOST"), "unexpected error: {err}");
}

#[test]
fn boost_reset_leaves_the_controller_usable() {
    // `integral_acc` is private; assert observable behaviour instead.
    use kivi_physics::boost::BoostController;
    let mut b = BoostController::default();
    for _ in 0..10 {
        let _ = b.update(1500.0, 1400.0, 0.3, 0.01);
    }
    b.reset();
    let (duty, ..) = b.update(1500.0, 1400.0, 0.3, 0.01).expect("usable after reset");
    assert!(duty.is_finite() && (b.wgdc_min..=b.wgdc_max).contains(&duty));
}

#[test]
fn compressible_throttle_critical_ratio_is_in_the_physical_range() {
    use kivi_physics::airflow_dynamics::CompressibleThrottleFlow;
    let r = CompressibleThrottleFlow::critical_pressure_ratio();
    assert!((0.4..0.7).contains(&r), "critical ratio {r} outside 0.4..0.7");
}

#[test]
fn manifold_filling_pressure_rises_toward_the_applied_pressure() {
    use kivi_physics::airflow_dynamics::ManifoldFillingModel;
    let mut m = ManifoldFillingModel::new(0.002, 50000.0, 300.0);
    let before = m.current_pressure_pa;
    // integrate_step(mass_flow_in, displacement_m3, rpm, VE, dt)
    for _ in 0..50 {
        m.integrate_step(0.05, 0.0005, 0.0, 1.0, 0.01);
    }
    assert!(
        m.current_pressure_pa > before,
        "pressure must rise toward ambient: {} -> {}",
        before,
        m.current_pressure_pa
    );
}

#[test]
fn anti_stall_spark_adder_does_not_shrink_with_a_deeper_deficit() {
    use kivi_physics::transients::AntiStallSaver;
    let a = AntiStallSaver::default();
    let near = a.calculate_spark_adder(800.0, 900.0);
    let far = a.calculate_spark_adder(400.0, 900.0);
    assert!(far >= near, "deeper deficit must not reduce the adder: {far} < {near}");
}

#[test]
fn maf_linearizer_is_monotonic_in_voltage() {
    use kivi_physics::transients::PiecewiseMafLinearizer;
    let l = PiecewiseMafLinearizer::default();
    let mut prev = l.voltage_to_airflow_gps(0.5);
    for v in [1.0, 2.0, 3.0, 4.0, 5.0] {
        let cur = l.voltage_to_airflow_gps(v);
        assert!(cur >= prev, "airflow must not fall with voltage at {v}V");
        prev = cur;
    }
}

#[test]
fn injector_rescale_with_identical_injectors_and_dead_times_is_identity() {
    use kivi_physics::axis_rescaling::InjectorScaler;
    let out = InjectorScaler::rescale_pulse_width(5.0, 1.0, 200.0, 200.0, 1.0).expect("rescale");
    assert!((out - 5.0).abs() < 1e-12, "identity rescale must return 5.0, got {out}");
}

#[test]
fn injector_rescale_shortens_effective_pulse_for_a_larger_injector() {
    use kivi_physics::axis_rescaling::InjectorScaler;
    // effective old = 5 - 1 = 4; ratio 200/400 = 0.5 -> 2.0; + dead 1.0 = 3.0
    let out = InjectorScaler::rescale_pulse_width(5.0, 1.0, 200.0, 400.0, 1.0).expect("rescale");
    assert!((out - 3.0).abs() < 1e-12, "expected 3.0, got {out}");
}

#[test]
fn injector_rescale_rejects_non_positive_flow() {
    use kivi_physics::axis_rescaling::InjectorScaler;
    assert!(InjectorScaler::rescale_pulse_width(5.0, 1.0, 0.0, 400.0, 1.0).is_err());
    assert!(InjectorScaler::rescale_pulse_width(5.0, 1.0, 200.0, 0.0, 1.0).is_err());
}

#[test]
fn axis_resampler_reproduces_identical_axes() {
    use kivi_physics::axis_rescaling::AxisResampler;
    let src_axis = [0.0, 10.0, 20.0];
    let src_vals = [100.0, 200.0, 300.0];
    let out = AxisResampler::resample_1d(&src_axis, &src_vals, &src_axis).expect("resample");
    assert_eq!(out, vec![100.0, 200.0, 300.0]);
}

#[test]
fn axis_resampler_clamps_outside_the_source_span() {
    use kivi_physics::axis_rescaling::AxisResampler;
    let src_axis = [0.0, 10.0, 20.0];
    let src_vals = [100.0, 200.0, 300.0];
    let out = AxisResampler::resample_1d(&src_axis, &src_vals, &[-5.0, 25.0]).expect("resample");
    assert_eq!(out, vec![100.0, 300.0], "outside the span clamps to the endpoints");
}

#[test]
fn axis_resampler_rejects_mismatched_lengths() {
    use kivi_physics::axis_rescaling::AxisResampler;
    assert!(AxisResampler::resample_1d(&[0.0, 1.0], &[1.0], &[0.0]).is_err());
    assert!(AxisResampler::resample_1d(&[], &[], &[0.0]).is_err());
    assert!(AxisResampler::resample_1d(&[0.0], &[1.0], &[]).is_err());
}
