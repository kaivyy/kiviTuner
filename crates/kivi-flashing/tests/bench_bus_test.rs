//! Parity tests for bench power interlocks and bus-silencing frames.
//!
//! Ported from `ecu_engine_rust::flashing::{bench_hardware, bus_silencing}`.
//!
//! ## How expected values were obtained
//!
//! * Bench thresholds come from the source constants (12.2 V floor, 14.8 V
//!   ceiling, 50 mV ripple).
//! * CAN termination: a bench harness has **one** 120 Ω resistor; an in-vehicle
//!   bus has **two in parallel = 60 Ω**. Both are asserted at their tolerance
//!   boundaries.
//! * Silencing frames follow ISO 14229: `0x85` ControlDTCSetting,
//!   `0x28` CommunicationControl, `0x14` ClearDiagnosticInformation.

use kivi_flashing::bench_hardware::{
    BenchPowerSupplyMonitor, CanBusImpedance, VehicleElectricalClass,
};
use kivi_flashing::bus_silencing::{
    BusSilencingInterlock, CommunicationControlType, DtcSettingType,
};

// ---------------------------------------------------------------------------
// Bench power interlocks
// ---------------------------------------------------------------------------

fn car_supply() -> BenchPowerSupplyMonitor {
    BenchPowerSupplyMonitor::for_vehicle_class(VehicleElectricalClass::PassengerCar12V)
}

#[test]
fn a_healthy_12v_supply_is_safe_to_flash() {
    assert!(car_supply().is_safe_for_flashing(13.5, 0.010).is_ok());
}

#[test]
fn undervoltage_is_refused_with_a_brownout_warning() {
    let err = car_supply()
        .is_safe_for_flashing(11.0, 0.0)
        .expect_err("11 V must be refused");
    assert!(err.contains("BROWNOUT"), "error: {err}");
}

#[test]
fn the_voltage_floor_is_inclusive() {
    let m = car_supply();
    assert!(
        m.is_safe_for_flashing(m.min_flashing_voltage, 0.0).is_ok(),
        "exactly the floor voltage must pass"
    );
    assert!(
        m.is_safe_for_flashing(m.min_flashing_voltage - 0.01, 0.0).is_err(),
        "just below the floor must fail"
    );
}

#[test]
fn overvoltage_is_refused_with_a_component_warning() {
    let m = car_supply();
    let err = m
        .is_safe_for_flashing(m.max_flashing_voltage + 1.0, 0.0)
        .expect_err("must refuse overvoltage");
    assert!(err.contains("OVERVOLTAGE"), "error: {err}");
}

#[test]
fn excessive_ripple_is_refused() {
    let m = car_supply();
    let err = m
        .is_safe_for_flashing(13.5, m.max_ripple_voltage + 0.01)
        .expect_err("must refuse ripple");
    assert!(err.contains("RIPPLE"), "error: {err}");
}

#[test]
fn voltage_is_checked_before_ripple() {
    // Both faults present: the voltage fault must be reported first.
    let m = car_supply();
    let err = m
        .is_safe_for_flashing(9.0, 1.0)
        .expect_err("must refuse");
    assert!(err.contains("BROWNOUT"), "voltage must win: {err}");
}

#[test]
fn each_vehicle_class_has_a_plausible_floor() {
    for class in [
        VehicleElectricalClass::PassengerCar12V,
        VehicleElectricalClass::Motorcycle12V,
        VehicleElectricalClass::HeavyDutyTruck24V,
    ] {
        let m = BenchPowerSupplyMonitor::for_vehicle_class(class);
        assert!(
            m.min_flashing_voltage > 0.0 && m.max_flashing_voltage > m.min_flashing_voltage,
            "{class:?} has an incoherent voltage window"
        );
    }
}

// ---------------------------------------------------------------------------
// CAN termination
// ---------------------------------------------------------------------------

#[test]
fn a_bench_harness_needs_one_120_ohm_resistor() {
    assert!(CanBusImpedance::verify_termination_resistance(120.0, true).is_ok());
}

#[test]
fn a_bench_harness_rejects_a_60_ohm_reading() {
    // 60 Ω means two resistors are fitted — that is the in-vehicle topology.
    let err = CanBusImpedance::verify_termination_resistance(60.0, true)
        .expect_err("60 ohm is wrong on a bench");
    assert!(err.contains("120"), "error must state the expected value: {err}");
}

#[test]
fn an_in_vehicle_bus_needs_60_ohm_total() {
    assert!(CanBusImpedance::verify_termination_resistance(60.0, false).is_ok());
}

#[test]
fn an_in_vehicle_bus_rejects_a_120_ohm_reading() {
    assert!(CanBusImpedance::verify_termination_resistance(120.0, false).is_err());
}

#[test]
fn termination_tolerance_is_bounded_on_both_sides() {
    // Bench: 120 ± 15.
    assert!(CanBusImpedance::verify_termination_resistance(134.0, true).is_ok());
    assert!(CanBusImpedance::verify_termination_resistance(136.0, true).is_err());
    // Vehicle: 60 ± 10.
    assert!(CanBusImpedance::verify_termination_resistance(69.0, false).is_ok());
    assert!(CanBusImpedance::verify_termination_resistance(71.0, false).is_err());
}

// ---------------------------------------------------------------------------
// Bus silencing frames
// ---------------------------------------------------------------------------

#[test]
fn control_dtc_setting_uses_service_0x85() {
    let frame = BusSilencingInterlock::build_control_dtc_setting(DtcSettingType::Off);
    assert_eq!(frame[0], 0x85, "ControlDTCSetting is service 0x85");
    assert_eq!(frame[1], DtcSettingType::Off as u8, "the sub-function is echoed");
    assert_eq!(frame.len(), 2);
}

#[test]
fn dtc_setting_on_and_off_differ_only_in_the_sub_function() {
    let on = BusSilencingInterlock::build_control_dtc_setting(DtcSettingType::On);
    let off = BusSilencingInterlock::build_control_dtc_setting(DtcSettingType::Off);
    assert_eq!(on[0], off[0], "same service id");
    assert_ne!(on[1], off[1], "different sub-function");
}

#[test]
fn communication_control_uses_service_0x28_with_three_bytes() {
    let frame = BusSilencingInterlock::build_communication_control(
        CommunicationControlType::DisableRxAndTx,
        0x01,
    );
    assert_eq!(frame[0], 0x28, "CommunicationControl is service 0x28");
    assert_eq!(frame[1], CommunicationControlType::DisableRxAndTx as u8);
    assert_eq!(frame[2], 0x01, "communication type is echoed");
    assert_eq!(frame.len(), 3);
}

#[test]
fn clear_all_dtcs_uses_service_0x14_with_the_all_groups_mask() {
    let frame = BusSilencingInterlock::build_clear_all_dtcs();
    assert_eq!(frame, vec![0x14, 0xFF, 0xFF, 0xFF], "ClearDiagnosticInformation, all groups");
}

#[test]
fn the_read_dtc_frame_uses_service_0x19_with_the_supplied_mask() {
    let frame = BusSilencingInterlock::build_read_dtc_by_status_mask(0x09);
    assert_eq!(frame[0], 0x19, "ReadDTCInformation is service 0x19");
    assert_eq!(frame[1], 0x02, "sub-function reportDTCByStatusMask is 0x02");
    assert_eq!(frame[2], 0x09, "the status mask is echoed");
}

#[test]
fn the_pre_flash_sequence_silences_the_bus() {
    let seq = BusSilencingInterlock::build_pre_flash_silencing_sequence();
    assert!(!seq.is_empty(), "the sequence must carry frames");
    // It must at least disable DTC setting and normal communication.
    let has_dtc = seq.iter().any(|f| f.first() == Some(&0x85));
    let has_comm = seq.iter().any(|f| f.first() == Some(&0x28));
    assert!(has_dtc, "pre-flash must disable DTC setting");
    assert!(has_comm, "pre-flash must disable normal communication");
}

#[test]
fn the_post_flash_sequence_restores_the_bus() {
    let seq = BusSilencingInterlock::build_post_flash_restoration_sequence();
    assert!(!seq.is_empty(), "the sequence must carry frames");
    // It must re-enable DTC setting and normal communication.
    let has_dtc = seq.iter().any(|f| f.first() == Some(&0x85));
    let has_comm = seq.iter().any(|f| f.first() == Some(&0x28));
    assert!(has_dtc, "post-flash must re-enable DTC setting");
    assert!(has_comm, "post-flash must re-enable communication");
}

#[test]
fn the_two_sequences_differ() {
    let pre = BusSilencingInterlock::build_pre_flash_silencing_sequence();
    let post = BusSilencingInterlock::build_post_flash_restoration_sequence();
    assert_ne!(pre, post, "silencing and restoration must not be identical");
}

#[test]
fn a_truncated_dtc_list_response_is_rejected() {
    let err = BusSilencingInterlock::parse_dtc_list_response(&[0x59])
        .expect_err("a 1-byte response cannot carry a DTC list");
    assert!(!err.is_empty());
}

#[test]
fn dtc_list_parsing_does_not_panic_on_arbitrary_bytes() {
    for len in [0usize, 1, 2, 3, 7, 16] {
        let data: Vec<u8> = (0..len).map(|i| (i * 17 % 256) as u8).collect();
        let _ = BusSilencingInterlock::parse_dtc_list_response(&data);
    }
}
