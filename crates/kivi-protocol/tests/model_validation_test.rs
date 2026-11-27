use kivi_protocol::errors::ProtocolError;
use kivi_protocol::model::{ConnectionState, EcuIdentity, TelemetryFrame, TelemetryStatusFlags};
use kivi_protocol::validation::{validate_telemetry_frame, SensorRangeError};

#[test]
fn test_valid_telemetry_frame_passes_validation() {
    let frame = TelemetryFrame {
        timestamp_ms: 1000,
        sequence: 1,
        is_simulated: true,
        rpm: 3500.0,
        tps: 25.5,
        map_kpa: 65.0,
        ect_c: 82.0,
        iat_c: 30.0,
        battery_volts: 14.1,
        ignition_advance_deg: 28.0,
        injector_pulse_width_ms: 4.2,
        afr: 14.7,
        speed_kph: 45.0,
        gear: 3,
        status_flags: TelemetryStatusFlags::default(),
    };

    let report = validate_telemetry_frame(&frame);
    assert!(report.is_valid());
    assert!(report.errors.is_empty());
}

#[test]
fn test_out_of_range_sensors_detected_without_panic() {
    let frame = TelemetryFrame {
        rpm: 25000.0,       // Melebihi batas max 20,000 RPM
        battery_volts: 4.2, // Di bawah batas min 6.0V
        tps: 110.0,         // Melebihi 100%
        ..Default::default()
    };

    let report = validate_telemetry_frame(&frame);
    assert!(!report.is_valid());
    assert_eq!(report.errors.len(), 3);
    assert!(report.errors.iter().any(|e| matches!(e, SensorRangeError::RpmOutOfRange(_))));
    assert!(report.errors.iter().any(|e| matches!(e, SensorRangeError::BatteryOutOfRange(_))));
    assert!(report.errors.iter().any(|e| matches!(e, SensorRangeError::TpsOutOfRange(_))));
}

#[test]
fn test_all_sensor_range_errors_covered() {
    let frame = TelemetryFrame {
        rpm: -1.0,
        tps: -0.5,
        map_kpa: 5.0,
        ect_c: -50.0,
        iat_c: 130.0,
        battery_volts: 19.5,
        afr: 5.0,
        ..Default::default()
    };

    let report = validate_telemetry_frame(&frame);
    assert!(!report.is_valid());
    assert_eq!(report.errors.len(), 7);

    assert!(report.errors.iter().any(|e| matches!(e, SensorRangeError::RpmOutOfRange(_))));
    assert!(report.errors.iter().any(|e| matches!(e, SensorRangeError::TpsOutOfRange(_))));
    assert!(report.errors.iter().any(|e| matches!(e, SensorRangeError::MapOutOfRange(_))));
    assert!(report.errors.iter().any(|e| matches!(e, SensorRangeError::EctOutOfRange(_))));
    assert!(report.errors.iter().any(|e| matches!(e, SensorRangeError::IatOutOfRange(_))));
    assert!(report.errors.iter().any(|e| matches!(e, SensorRangeError::BatteryOutOfRange(_))));
    assert!(report.errors.iter().any(|e| matches!(e, SensorRangeError::AfrOutOfRange(_))));
}

#[test]
fn test_boundary_values_pass_validation() {
    let frame = TelemetryFrame {
        timestamp_ms: 500,
        sequence: 42,
        is_simulated: false,
        rpm: 0.0,
        tps: 0.0,
        map_kpa: 10.0,
        ect_c: -40.0,
        iat_c: -40.0,
        battery_volts: 6.0,
        ignition_advance_deg: 0.0,
        injector_pulse_width_ms: 0.0,
        afr: 8.0,
        speed_kph: 0.0,
        gear: 1,
        status_flags: TelemetryStatusFlags::default(),
    };
    assert!(validate_telemetry_frame(&frame).is_valid());

    let frame_max = TelemetryFrame {
        timestamp_ms: 500,
        sequence: 43,
        is_simulated: false,
        rpm: 20000.0,
        tps: 100.0,
        map_kpa: 350.0,
        ect_c: 150.0,
        iat_c: 120.0,
        battery_volts: 18.0,
        ignition_advance_deg: 50.0,
        injector_pulse_width_ms: 25.0,
        afr: 25.0,
        speed_kph: 300.0,
        gear: 6,
        status_flags: TelemetryStatusFlags::default(),
    };
    assert!(validate_telemetry_frame(&frame_max).is_valid());
}

#[test]
fn test_serialization_roundtrip() {
    let frame = TelemetryFrame::default();
    let json = serde_json::to_string(&frame).expect("serialize telemetry frame");
    let deserialized: TelemetryFrame = serde_json::from_str(&json).expect("deserialize telemetry frame");
    assert_eq!(frame, deserialized);

    let identity = EcuIdentity {
        ecu_id: "ECU-TEST-01".to_string(),
        vin: Some("VIN123456789".to_string()),
        software_version: "1.0.0".to_string(),
        hardware_version: "revB".to_string(),
        protocol_name: "virtual-bench".to_string(),
        is_simulated: true,
    };
    let json_id = serde_json::to_string(&identity).expect("serialize ecu identity");
    let des_id: EcuIdentity = serde_json::from_str(&json_id).expect("deserialize ecu identity");
    assert_eq!(identity, des_id);

    let conn_state = ConnectionState::Connected {
        identity: identity.clone(),
    };
    let json_state = serde_json::to_string(&conn_state).expect("serialize connection state");
    let des_state: ConnectionState = serde_json::from_str(&json_state).expect("deserialize connection state");
    assert_eq!(conn_state, des_state);
}

#[test]
fn test_protocol_error_formatting() {
    let err = ProtocolError::DeviceNotFound("/dev/ttyUSB0".to_string());
    assert_eq!(err.to_string(), "Device not found: /dev/ttyUSB0");

    let timeout_err = ProtocolError::Timeout(250);
    assert_eq!(timeout_err.to_string(), "Communication timed out (250 ms)");
}

#[test]
fn test_unmeasured_afr_nan_passes_validation() {
    let frame = TelemetryFrame {
        timestamp_ms: 1000,
        sequence: 1,
        is_simulated: false,
        rpm: 3500.0,
        tps: 25.5,
        map_kpa: 65.0,
        ect_c: 82.0,
        iat_c: 30.0,
        battery_volts: 14.1,
        ignition_advance_deg: 28.0,
        injector_pulse_width_ms: 4.2,
        afr: f32::NAN,
        speed_kph: 45.0,
        gear: 3,
        status_flags: TelemetryStatusFlags::default(),
    };

    let report = validate_telemetry_frame(&frame);
    assert!(
        report.is_valid(),
        "TelemetryFrame with afr=NaN should pass validation as unmeasured sentinel"
    );
    assert!(report.errors.is_empty());
}

#[test]
fn test_afr_finite_boundaries_and_invalid_values() {
    let base_frame = TelemetryFrame {
        timestamp_ms: 1000,
        sequence: 1,
        is_simulated: false,
        rpm: 3500.0,
        tps: 25.5,
        map_kpa: 65.0,
        ect_c: 82.0,
        iat_c: 30.0,
        battery_volts: 14.1,
        ignition_advance_deg: 28.0,
        injector_pulse_width_ms: 4.2,
        afr: 14.7,
        speed_kph: 45.0,
        gear: 3,
        status_flags: TelemetryStatusFlags::default(),
    };

    // 1. Boundary 8.0 accepted
    let mut f_min = base_frame.clone();
    f_min.afr = 8.0;
    assert!(validate_telemetry_frame(&f_min).is_valid());

    // 2. Boundary 25.0 accepted
    let mut f_max = base_frame.clone();
    f_max.afr = 25.0;
    assert!(validate_telemetry_frame(&f_max).is_valid());

    // 3. Below 8.0 rejected
    let mut f_below = base_frame.clone();
    f_below.afr = 7.99;
    let rep_below = validate_telemetry_frame(&f_below);
    assert!(!rep_below.is_valid());
    assert!(rep_below.errors.iter().any(|e| matches!(e, SensorRangeError::AfrOutOfRange(_))));

    // 4. Above 25.0 rejected
    let mut f_above = base_frame.clone();
    f_above.afr = 25.01;
    let rep_above = validate_telemetry_frame(&f_above);
    assert!(!rep_above.is_valid());
    assert!(rep_above.errors.iter().any(|e| matches!(e, SensorRangeError::AfrOutOfRange(_))));

    // 5. Positive Infinity rejected
    let mut f_pos_inf = base_frame.clone();
    f_pos_inf.afr = f32::INFINITY;
    let rep_pos_inf = validate_telemetry_frame(&f_pos_inf);
    assert!(!rep_pos_inf.is_valid());
    assert!(rep_pos_inf.errors.iter().any(|e| matches!(e, SensorRangeError::AfrOutOfRange(_))));

    // 6. Negative Infinity rejected
    let mut f_neg_inf = base_frame;
    f_neg_inf.afr = f32::NEG_INFINITY;
    let rep_neg_inf = validate_telemetry_frame(&f_neg_inf);
    assert!(!rep_neg_inf.is_valid());
    assert!(rep_neg_inf.errors.iter().any(|e| matches!(e, SensorRangeError::AfrOutOfRange(_))));
}

#[test]
fn test_unmeasured_afr_does_not_mask_other_field_errors() {
    let frame = TelemetryFrame {
        timestamp_ms: 1000,
        sequence: 1,
        is_simulated: false,
        rpm: 25000.0, // Out of range
        tps: 110.0,   // Out of range
        map_kpa: 5.0, // Out of range
        ect_c: 82.0,
        iat_c: 30.0,
        battery_volts: 4.5, // Out of range
        ignition_advance_deg: 28.0,
        injector_pulse_width_ms: 4.2,
        afr: f32::NAN, // Unmeasured sentinel
        speed_kph: 45.0,
        gear: 3,
        status_flags: TelemetryStatusFlags::default(),
    };

    let report = validate_telemetry_frame(&frame);
    assert!(!report.is_valid());
    assert_eq!(report.errors.len(), 4);
    assert!(report.errors.iter().any(|e| matches!(e, SensorRangeError::RpmOutOfRange(_))));
    assert!(report.errors.iter().any(|e| matches!(e, SensorRangeError::TpsOutOfRange(_))));
    assert!(report.errors.iter().any(|e| matches!(e, SensorRangeError::MapOutOfRange(_))));
    assert!(report.errors.iter().any(|e| matches!(e, SensorRangeError::BatteryOutOfRange(_))));
    // AFR must NOT produce an error since it is NaN
    assert!(!report.errors.iter().any(|e| matches!(e, SensorRangeError::AfrOutOfRange(_))));

    // Non-finite (NaN) in other fields like RPM must still be rejected
    let nan_rpm_frame = TelemetryFrame {
        rpm: f32::NAN,
        afr: f32::NAN,
        ..Default::default()
    };
    let nan_rpm_report = validate_telemetry_frame(&nan_rpm_frame);
    assert!(!nan_rpm_report.is_valid());
    assert!(nan_rpm_report.errors.iter().any(|e| matches!(e, SensorRangeError::RpmOutOfRange(_))));
}
