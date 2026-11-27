//! Wave 8: application-layer tests for the engineering toolkit commands.
//!
//! These exercise the `*_inner` functions the Tauri commands wrap, so a command
//! that exists but is not reachable through a tested path is not counted as
//! integrated.

use kivituner::toolkit::{
    compute_air_mass_inner, compute_fuel_pulse_inner, compute_histogram_inner,
    compute_spark_timing_inner, compute_torque_inner, decode_sensor_inner,
    describe_flash_target_view_inner, FuelPulseRequest,
};

// ---------------------------------------------------------------------------
// Fuel pulse
// ---------------------------------------------------------------------------

fn pulse_req() -> FuelPulseRequest {
    FuelPulseRequest {
        air_mass_g: 0.15,
        target_lambda: 0.88,
        battery_volts: 13.8,
        rail_diff_kpa: 300.0,
        cycle_dt_sec: 120.0 / 7000.0,
        fuel: "e10".to_string(),
    }
}

#[test]
fn fuel_pulse_returns_the_expected_decomposition() {
    let r = compute_fuel_pulse_inner(&pulse_req()).expect("pulse");
    assert!(r.gross_ms > 0.0);
    assert!(
        (r.gross_ms - (r.effective_ms + r.dead_time_ms)).abs() < 1e-9,
        "gross must equal effective + dead time"
    );
    assert!((r.stoich_afr - 14.13).abs() < 1e-9, "E10 stoich is 14.13");
}

#[test]
fn fuel_pulse_rejects_unknown_fuel() {
    let mut req = pulse_req();
    req.fuel = "rocket".to_string();
    let err = compute_fuel_pulse_inner(&req).expect_err("must reject");
    assert!(err.contains("Unknown fuel type"), "error: {err}");
}

#[test]
fn fuel_pulse_rejects_negative_air_mass() {
    let mut req = pulse_req();
    req.air_mass_g = -1.0;
    assert!(compute_fuel_pulse_inner(&req).is_err());
}

#[test]
fn fuel_pulse_rejects_non_positive_lambda() {
    let mut req = pulse_req();
    req.target_lambda = 0.0;
    assert!(compute_fuel_pulse_inner(&req).is_err());
}

#[test]
fn fuel_pulse_rejects_non_finite_inputs() {
    let mut req = pulse_req();
    req.air_mass_g = f64::NAN;
    assert!(compute_fuel_pulse_inner(&req).is_err());
}

// ---------------------------------------------------------------------------
// Air mass
// ---------------------------------------------------------------------------

#[test]
fn air_mass_for_a_155cc_single_is_in_the_physical_range() {
    let r = compute_air_mass_inner(155.0, 1, 100.0, 0.85, 30.0, 85.0, 7000.0)
        .expect("air mass");
    assert!(r.air_mass_g > 0.12 && r.air_mass_g < 0.18, "got {}", r.air_mass_g);
    assert!(r.charge_temp_k > 273.15);
    assert!(r.implied_ve > 0.0);
}

#[test]
fn air_mass_rejects_zero_cylinders() {
    assert!(compute_air_mass_inner(155.0, 0, 100.0, 0.85, 30.0, 85.0, 7000.0).is_err());
}

#[test]
fn air_mass_rejects_zero_rpm() {
    assert!(compute_air_mass_inner(155.0, 1, 100.0, 0.85, 30.0, 85.0, 0.0).is_err());
}

// ---------------------------------------------------------------------------
// Spark timing
// ---------------------------------------------------------------------------

#[test]
fn spark_timing_applies_corrections() {
    let r = compute_spark_timing_inner(20.0, 3.0, -2.0, 0.0).expect("timing");
    assert!((r.commanded_btdc - 21.0).abs() < 1e-9);
    assert_eq!(r.knock_retard_deg, 0.0);
}

#[test]
fn spark_timing_subtracts_knock_retard() {
    let r = compute_spark_timing_inner(20.0, 0.0, 0.0, 5.0).expect("timing");
    assert!((r.commanded_btdc - 15.0).abs() < 1e-9);
    assert_eq!(r.knock_retard_deg, 5.0);
}

#[test]
fn spark_timing_clamps_to_the_hard_limits() {
    let high = compute_spark_timing_inner(50.0, 10.0, 5.0, 0.0).expect("timing");
    assert_eq!(high.commanded_btdc, 55.0);
    let low = compute_spark_timing_inner(-30.0, 0.0, 0.0, 0.0).expect("timing");
    assert_eq!(low.commanded_btdc, -10.0);
}

#[test]
fn spark_timing_rejects_negative_knock_retard() {
    assert!(compute_spark_timing_inner(20.0, 0.0, 0.0, -5.0).is_err());
}

#[test]
fn spark_timing_reports_mfb50() {
    let r = compute_spark_timing_inner(28.0, 0.0, 0.0, 0.0).expect("timing");
    assert!(r.mfb50_atdc > 6.0 && r.mfb50_atdc < 12.0, "got {}", r.mfb50_atdc);
}

// ---------------------------------------------------------------------------
// Flash target description
// ---------------------------------------------------------------------------

#[test]
fn flash_target_describes_a_known_chip() {
    // AMD Am29F400BT: the source declares 524288 but blocks sum to 327680.
    let v = describe_flash_target_view_inner(0x0001_2258).expect("describe");
    assert_eq!(v.total_size_bytes, 524_288);
    assert_eq!(
        v.blocks_total_bytes, 327_680,
        "the preserved Am29F400BT gap must remain visible"
    );
    assert!(!v.blocks.is_empty());
    assert!(v.model_name.contains("Am29F400BT"));
}

#[test]
fn flash_target_marks_the_boot_block_unsafe_to_write() {
    let v = describe_flash_target_view_inner(0x0001_2258).expect("describe");
    let boot = v
        .blocks
        .iter()
        .find(|b| b.block_type == "Boot")
        .expect("a boot block must exist");
    assert!(!boot.write_safe, "the boot block must never be writable");
}

#[test]
fn flash_target_rejects_an_unknown_chip() {
    assert!(describe_flash_target_view_inner(0xDEAD_BEEF).is_err());
}

// ---------------------------------------------------------------------------
// Histogram
// ---------------------------------------------------------------------------

#[test]
fn histogram_counts_hits_per_cell() {
    let rows = vec![0.0, 1000.0, 2000.0];
    let cols = vec![0.0, 50.0, 100.0];
    let points = vec![(500.0, 25.0), (500.0, 25.0), (1500.0, 75.0)];

    let r = compute_histogram_inner(rows, cols, &points).expect("histogram");
    assert_eq!(r.total_samples, 3);
    let total: u32 = r.cells.iter().map(|c| c.hits).sum();
    assert_eq!(total, 3, "every sample must land in exactly one cell");
}

#[test]
fn histogram_rejects_degenerate_axes() {
    assert!(compute_histogram_inner(vec![0.0], vec![0.0, 1.0], &[(0.0, 0.0)]).is_err());
}

#[test]
fn histogram_rejects_an_empty_sample_set() {
    assert!(compute_histogram_inner(vec![0.0, 1.0], vec![0.0, 1.0], &[]).is_err());
}

// ---------------------------------------------------------------------------
// Torque
// ---------------------------------------------------------------------------

#[test]
fn torque_indicated_exceeds_driver_request() {
    let r = compute_torque_inner(100.0, 5000.0, 100.0, 85.0, 5.0, 100.0).expect("torque");
    assert!(r.indicated_nm > r.driver_torque_nm);
    assert!(r.friction_nm > 0.0);
}

#[test]
fn torque_reports_a_fast_path_retard_for_a_cut() {
    let r = compute_torque_inner(100.0, 5000.0, 100.0, 85.0, 0.0, 100.0).expect("torque");
    assert!(
        r.fast_path_retard_deg >= 0.0 && r.fast_path_retard_deg <= 45.0,
        "retard out of range: {}",
        r.fast_path_retard_deg
    );
}

// ---------------------------------------------------------------------------
// Sensor decode
// ---------------------------------------------------------------------------

#[test]
fn decode_map_voltage_to_kpa() {
    let r = decode_sensor_inner("map", 4.5).expect("decode");
    assert_eq!(r.unit, "kPa");
    assert!(r.value > 0.0);
}

#[test]
fn decode_tps_adc_to_percent() {
    let r = decode_sensor_inner("tps", 4095.0).expect("decode");
    assert_eq!(r.unit, "%");
    assert!((0.0..=100.0).contains(&r.value));
}

#[test]
fn decode_thermistor_adc_to_celsius() {
    let r = decode_sensor_inner("thermistor", 2000.0).expect("decode");
    assert_eq!(r.unit, "°C");
    assert!(r.value.is_finite());
}

#[test]
fn decode_rejects_an_unknown_sensor() {
    assert!(decode_sensor_inner("flux", 1.0).is_err());
}

// ---------------------------------------------------------------------------
// Wave 11: throttle flow and preset validation
// ---------------------------------------------------------------------------

use kivituner::toolkit::{compute_throttle_flow_inner, validate_preset_inner, CalcPreset};

#[test]
fn throttle_flow_returns_a_physical_mass_flow() {
    // 40 mm throttle (~0.00126 m²), 101325 Pa, 300 K, manifold at 50 kPa.
    let r = compute_throttle_flow_inner(0.00126, 101_325.0, 300.0, 50_000.0)
        .expect("throttle flow");
    assert!(r.mass_flow_kg_s > 0.0, "flow must be positive");
    assert!(r.mass_flow_kg_s < 1.0, "a 40mm throttle cannot flow 1 kg/s");
    assert!((r.pressure_ratio - 50_000.0 / 101_325.0).abs() < 1e-9);
}

#[test]
fn throttle_flow_reports_choking_at_low_pressure_ratio() {
    // Manifold at 30% of ambient is well below the ~0.53 critical ratio.
    let r = compute_throttle_flow_inner(0.001, 101_325.0, 300.0, 30_000.0)
        .expect("throttle flow");
    assert!(r.is_choked, "flow must be choked below the critical ratio");
    assert!((0.4..0.7).contains(&r.critical_ratio));
}

#[test]
fn throttle_flow_is_not_choked_at_a_high_pressure_ratio() {
    let r = compute_throttle_flow_inner(0.001, 101_325.0, 300.0, 95_000.0)
        .expect("throttle flow");
    assert!(!r.is_choked);
}

#[test]
fn throttle_flow_rejects_a_zero_area() {
    assert!(compute_throttle_flow_inner(0.0, 101_325.0, 300.0, 50_000.0).is_err());
}

#[test]
fn throttle_flow_rejects_a_non_physical_temperature() {
    assert!(compute_throttle_flow_inner(0.001, 101_325.0, 100.0, 50_000.0).is_err());
}

#[test]
fn throttle_flow_rejects_negative_manifold_pressure() {
    assert!(compute_throttle_flow_inner(0.001, 101_325.0, 300.0, -1.0).is_err());
}

#[test]
fn throttle_flow_rejects_nan() {
    assert!(compute_throttle_flow_inner(f64::NAN, 101_325.0, 300.0, 50_000.0).is_err());
}

fn preset(name: &str, tab: &str) -> CalcPreset {
    let mut values = std::collections::BTreeMap::new();
    values.insert("air_mass".to_string(), 0.15);
    CalcPreset {
        name: name.to_string(),
        tab: tab.to_string(),
        values,
    }
}

#[test]
fn a_valid_preset_is_accepted() {
    assert!(validate_preset_inner(&preset("WOT pull", "fuel"), &["fuel", "air"]).is_ok());
}

#[test]
fn an_empty_preset_name_is_rejected() {
    let err = validate_preset_inner(&preset("   ", "fuel"), &["fuel"]).expect_err("must reject");
    assert!(err.contains("empty"), "error: {err}");
}

#[test]
fn an_over_long_preset_name_is_rejected() {
    let long = "x".repeat(65);
    let err = validate_preset_inner(&preset(&long, "fuel"), &["fuel"]).expect_err("must reject");
    assert!(err.contains("too long"), "error: {err}");
}

#[test]
fn an_unknown_tab_is_rejected_and_the_valid_ones_are_listed() {
    let err = validate_preset_inner(&preset("p", "nope"), &["fuel", "air"]).expect_err("must reject");
    assert!(err.contains("Unknown tab"), "error: {err}");
    assert!(err.contains("fuel"), "the error must list valid tabs: {err}");
}

#[test]
fn a_preset_with_no_values_is_rejected() {
    let mut p = preset("empty", "fuel");
    p.values.clear();
    let err = validate_preset_inner(&p, &["fuel"]).expect_err("must reject");
    assert!(err.contains("at least one value"), "error: {err}");
}

#[test]
fn a_non_finite_preset_value_is_rejected() {
    let mut p = preset("bad", "fuel");
    p.values.insert("lambda".to_string(), f64::NAN);
    let err = validate_preset_inner(&p, &["fuel"]).expect_err("must reject");
    assert!(err.contains("finite"), "error: {err}");
}
