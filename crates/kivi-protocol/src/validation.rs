use crate::model::TelemetryFrame;
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq)]
pub enum SensorRangeError {
    #[error("RPM {0} out of valid range (0.0..20000.0)")]
    RpmOutOfRange(f32),
    #[error("TPS {0}% out of valid range (0.0..100.0)")]
    TpsOutOfRange(f32),
    #[error("MAP {0} kPa out of valid range (10.0..350.0)")]
    MapOutOfRange(f32),
    #[error("ECT {0} C out of valid range (-40.0..150.0)")]
    EctOutOfRange(f32),
    #[error("IAT {0} C out of valid range (-40.0..120.0)")]
    IatOutOfRange(f32),
    #[error("Battery {0} V out of valid range (6.0..18.0)")]
    BatteryOutOfRange(f32),
    #[error("AFR {0} out of valid range (8.0..25.0)")]
    AfrOutOfRange(f32),
}

#[derive(Debug, Clone, Default)]
pub struct ValidationReport {
    pub errors: Vec<SensorRangeError>,
}

impl ValidationReport {
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }
}

pub fn validate_telemetry_frame(frame: &TelemetryFrame) -> ValidationReport {
    let mut errors = Vec::new();
    if !(0.0..=20000.0).contains(&frame.rpm) {
        errors.push(SensorRangeError::RpmOutOfRange(frame.rpm));
    }
    if !(0.0..=100.0).contains(&frame.tps) {
        errors.push(SensorRangeError::TpsOutOfRange(frame.tps));
    }
    if !(10.0..=350.0).contains(&frame.map_kpa) {
        errors.push(SensorRangeError::MapOutOfRange(frame.map_kpa));
    }
    if !(-40.0..=150.0).contains(&frame.ect_c) {
        errors.push(SensorRangeError::EctOutOfRange(frame.ect_c));
    }
    if !(-40.0..=120.0).contains(&frame.iat_c) {
        errors.push(SensorRangeError::IatOutOfRange(frame.iat_c));
    }
    if !(6.0..=18.0).contains(&frame.battery_volts) {
        errors.push(SensorRangeError::BatteryOutOfRange(frame.battery_volts));
    }
    // AFR uses NaN as an unmeasured/unavailable sentinel; finite values must be within [8.0, 25.0]
    if !frame.afr.is_nan() && !(8.0..=25.0).contains(&frame.afr) {
        errors.push(SensorRangeError::AfrOutOfRange(frame.afr));
    }
    ValidationReport { errors }
}
