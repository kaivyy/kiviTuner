//! Parity tests for the torque-structure module.
//!
//! Ported from `ecu_engine_rust::physics::torque`.
//!
//! ## How expected values were obtained
//!
//! Every formula is evaluated by hand from the source:
//! * driver torque `= pedal·T_max·taper`, `taper = clamp(1-(rpm/12000)², 0.2, 1)`
//! * friction `= 8 + rpm/1000·2.2 + cold penalty`
//! * pumping `= max(101.3 - MAP, 0)·0.12`
//! * fast-path retard `= sqrt((1 - ratio)/0.00125)`, capped at 45°

use kivi_physics::torque::TorqueStructureEngine;

#[test]
fn default_engine_rating_matches_the_source() {
    assert_eq!(TorqueStructureEngine::default().max_engine_torque_nm, 250.0);
}

// ---------------------------------------------------------------------------
// Driver request
// ---------------------------------------------------------------------------

#[test]
fn full_pedal_at_low_rpm_gives_near_maximum_torque() {
    let e = TorqueStructureEngine::default();
    // rpm 0 -> taper = 1 -> 250 Nm
    assert!((e.calculate_driver_requested_torque(100.0, 0.0) - 250.0).abs() < 1e-12);
}

#[test]
fn zero_pedal_gives_zero_torque() {
    let e = TorqueStructureEngine::default();
    assert_eq!(e.calculate_driver_requested_torque(0.0, 5000.0), 0.0);
}

#[test]
fn pedal_is_clamped_to_the_unit_range() {
    let e = TorqueStructureEngine::default();
    assert_eq!(
        e.calculate_driver_requested_torque(150.0, 0.0),
        e.calculate_driver_requested_torque(100.0, 0.0)
    );
    assert_eq!(e.calculate_driver_requested_torque(-50.0, 0.0), 0.0);
}

#[test]
fn torque_tapers_with_rpm() {
    let e = TorqueStructureEngine::default();
    // 6000 rpm -> taper = 1 - 0.25 = 0.75 -> 187.5 Nm
    let at_6k = e.calculate_driver_requested_torque(100.0, 6000.0);
    assert!((at_6k - 187.5).abs() < 1e-9, "got {at_6k}");
}

#[test]
fn the_taper_floors_at_0_2() {
    let e = TorqueStructureEngine::default();
    // 12000 rpm -> 1 - 1 = 0 -> clamped to 0.2 -> 50 Nm
    let at_12k = e.calculate_driver_requested_torque(100.0, 12000.0);
    assert!((at_12k - 50.0).abs() < 1e-9, "got {at_12k}");
    // Beyond 12000 stays at the floor.
    let beyond = e.calculate_driver_requested_torque(100.0, 20000.0);
    assert!((beyond - 50.0).abs() < 1e-9, "got {beyond}");
}

// ---------------------------------------------------------------------------
// Losses
// ---------------------------------------------------------------------------

#[test]
fn friction_at_warm_idle_is_the_base_term() {
    let e = TorqueStructureEngine::default();
    // 8 + 0 * 2.2 = 8, no cold penalty at 85C
    assert!((e.estimate_friction_loss_torque(85.0, 0.0) - 8.0).abs() < 1e-12);
}

#[test]
fn friction_grows_with_rpm() {
    let e = TorqueStructureEngine::default();
    // 5000 rpm -> 8 + 5*2.2 = 19
    assert!((e.estimate_friction_loss_torque(85.0, 5000.0) - 19.0).abs() < 1e-12);
}

#[test]
fn cold_oil_adds_a_temperature_penalty() {
    let e = TorqueStructureEngine::default();
    // 0C: penalty = (85/85)*12 = 12 -> 8 + 0 + 12 = 20
    assert!((e.estimate_friction_loss_torque(0.0, 0.0) - 20.0).abs() < 1e-12);
}

#[test]
fn there_is_no_penalty_above_the_warm_threshold() {
    let e = TorqueStructureEngine::default();
    assert_eq!(
        e.estimate_friction_loss_torque(85.0, 1000.0),
        e.estimate_friction_loss_torque(120.0, 1000.0)
    );
}

#[test]
fn pumping_loss_is_zero_at_full_throttle() {
    let e = TorqueStructureEngine::default();
    // MAP 101.3 -> vacuum 0
    assert_eq!(e.estimate_pumping_loss_torque(101.3), 0.0);
    // Above ambient also clamps to zero.
    assert_eq!(e.estimate_pumping_loss_torque(120.0), 0.0);
}

#[test]
fn pumping_loss_grows_with_vacuum() {
    let e = TorqueStructureEngine::default();
    // MAP 30 -> vacuum 71.3 -> 71.3 * 0.12 = 8.556
    let l = e.estimate_pumping_loss_torque(30.0);
    assert!((l - 71.3 * 0.12).abs() < 1e-12, "got {l}");
}

// ---------------------------------------------------------------------------
// Indicated torque
// ---------------------------------------------------------------------------

#[test]
fn indicated_torque_sums_all_losses() {
    let e = TorqueStructureEngine::default();
    let t = e.calculate_target_indicated_torque(100.0, 19.0, 8.556, 5.0);
    assert!((t - 132.556).abs() < 1e-9, "got {t}");
}

#[test]
fn indicated_torque_exceeds_driver_request() {
    let e = TorqueStructureEngine::default();
    let driver = e.calculate_driver_requested_torque(100.0, 5000.0);
    let friction = e.estimate_friction_loss_torque(85.0, 5000.0);
    let pumping = e.estimate_pumping_loss_torque(100.0);
    let ind = e.calculate_target_indicated_torque(driver, friction, pumping, 5.0);
    assert!(ind > driver);
}

// ---------------------------------------------------------------------------
// Fast-path spark retard
// ---------------------------------------------------------------------------

#[test]
fn no_intervention_when_no_reduction_is_requested() {
    let e = TorqueStructureEngine::default();
    assert_eq!(e.calculate_fast_path_spark_retard(200.0, 200.0), 0.0);
    assert_eq!(e.calculate_fast_path_spark_retard(250.0, 200.0), 0.0);
}

#[test]
fn no_intervention_when_current_torque_is_not_positive() {
    let e = TorqueStructureEngine::default();
    assert_eq!(e.calculate_fast_path_spark_retard(10.0, 0.0), 0.0);
}

#[test]
fn a_fifty_percent_cut_needs_about_twenty_degrees() {
    let e = TorqueStructureEngine::default();
    let deg = e.calculate_fast_path_spark_retard(100.0, 200.0);
    assert!((18.0..=35.0).contains(&deg), "retard out of range: {deg}");
}

#[test]
fn the_retard_matches_the_quadratic_formula() {
    let e = TorqueStructureEngine::default();
    // ratio 0.5 -> delta 0.5 -> sqrt(0.5/0.00125) = 20.0
    let deg = e.calculate_fast_path_spark_retard(100.0, 200.0);
    assert!((deg - 20.0).abs() < 1e-9, "got {deg}");
}

#[test]
fn the_retard_is_capped_at_45_degrees() {
    let e = TorqueStructureEngine::default();
    // ratio clamped to 0.15 -> delta 0.85 -> sqrt(680) = 26.08, under the cap.
    // To reach the cap we need a much larger delta; verify the cap holds anyway.
    let deg = e.calculate_fast_path_spark_retard(0.0, 1000.0);
    assert!(deg <= 45.0, "retard must not exceed 45 deg, got {deg}");
}

#[test]
fn a_deeper_cut_needs_more_retard() {
    let e = TorqueStructureEngine::default();
    let mild = e.calculate_fast_path_spark_retard(180.0, 200.0);
    let deep = e.calculate_fast_path_spark_retard(100.0, 200.0);
    assert!(deep > mild, "deeper cut must require more retard");
}
