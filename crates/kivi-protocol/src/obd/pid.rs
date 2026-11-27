use crate::obd::command::Mode01Pid;
use crate::obd::errors::ObdCodecError;
use serde::{Deserialize, Serialize};

/// Wideband oxygen sensor measurement sample decoded from Mode 01 PID 0x34.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WidebandO2Sample {
    /// Equivalence ratio ($\lambda$). Stoichiometric is 1.0, rich < 1.0, lean > 1.0.
    pub equivalence_ratio: f32,
    /// Sensor pumping current in milliamperes (mA).
    pub sensor_current_ma: f32,
}

impl WidebandO2Sample {
    /// Constructs a new wideband sample with explicit equivalence ratio and current.
    pub const fn new(equivalence_ratio: f32, sensor_current_ma: f32) -> Self {
        Self {
            equivalence_ratio,
            sensor_current_ma,
        }
    }

    /// Computes the estimated air-fuel ratio (AFR) given a fuel stoichiometric ratio.
    ///
    /// For pure gasoline, `stoich_ratio` is typically 14.7.
    /// For E10 ethanol blend, `stoich_ratio` is typically 14.08.
    ///
    /// Rejects non-positive, NaN, and infinite stoichiometric ratios or equivalence ratios.
    pub fn estimated_afr(&self, stoich_ratio: f32) -> Result<f32, ObdCodecError> {
        if stoich_ratio.is_nan() || stoich_ratio.is_infinite() || stoich_ratio <= 0.0 {
            return Err(ObdCodecError::InvalidStoichiometricRatio(format!(
                "{}",
                stoich_ratio
            )));
        }
        if self.equivalence_ratio.is_nan()
            || self.equivalence_ratio.is_infinite()
            || self.equivalence_ratio <= 0.0
        {
            return Err(ObdCodecError::InvalidStoichiometricRatio(format!(
                "invalid equivalence ratio: {}",
                self.equivalence_ratio
            )));
        }
        Ok(self.equivalence_ratio * stoich_ratio)
    }
}

/// Strongly-typed decoded values from SAE J1979 Mode 01 diagnostic payloads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DecodedMode01Value {
    /// Engine speed in RPM.
    EngineRpm(f32),
    /// Absolute throttle position in percent (0.0 to 100.0%).
    ThrottlePosition(f32),
    /// Manifold absolute pressure in kPa.
    ManifoldPressure(u8),
    /// Engine coolant temperature in °C (-40.0 to 215.0°C).
    CoolantTemp(f32),
    /// Intake air temperature in °C (-40.0 to 215.0°C).
    IntakeAirTemp(f32),
    /// Control module / battery voltage in Volts.
    ControlModuleVoltage(f32),
    /// Vehicle speed in km/h.
    VehicleSpeed(f32),
    /// Wideband O2 sensor equivalence ratio ($\lambda$) and current (mA).
    WidebandLambda1(WidebandO2Sample),
}

/// Decodes and strictly validates a Mode 01 diagnostic payload for the specified PID.
///
/// Ensures:
/// - Exact expected payload byte length (service byte + PID byte + formula data bytes).
/// - Correct positive response service byte (`0x41`).
/// - Echoed PID matches the requested PID.
pub fn decode_mode01_payload(
    pid: Mode01Pid,
    payload: &[u8],
) -> Result<DecodedMode01Value, ObdCodecError> {
    let expected_data_len = match pid {
        Mode01Pid::CoolantTemp => 1,
        Mode01Pid::ManifoldPressure => 1,
        Mode01Pid::EngineRpm => 2,
        Mode01Pid::VehicleSpeed => 1,
        Mode01Pid::IntakeAirTemp => 1,
        Mode01Pid::ThrottlePosition => 1,
        Mode01Pid::WidebandLambda1 => 4,
        Mode01Pid::ControlModuleVoltage => 2,
        Mode01Pid::SupportedPids01_20 | Mode01Pid::SupportedPids21_40 | Mode01Pid::SupportedPids41_60 => 4,
    };

    let expected_total_len = 2 + expected_data_len;

    if payload.len() < expected_total_len {
        return Err(ObdCodecError::TruncatedPayload {
            expected: expected_total_len,
            actual: payload.len(),
        });
    }
    if payload.len() > expected_total_len {
        return Err(ObdCodecError::OversizedPayload {
            expected: expected_total_len,
            actual: payload.len(),
        });
    }

    if payload[0] != 0x41 {
        return Err(ObdCodecError::UnexpectedService {
            expected: 0x41,
            actual: payload[0],
        });
    }

    if payload[1] != pid.pid() {
        return Err(ObdCodecError::UnexpectedPid {
            expected: pid.pid(),
            actual: payload[1],
        });
    }

    let data = &payload[2..];

    match pid {
        Mode01Pid::EngineRpm => {
            let a = data[0] as u16;
            let b = data[1] as u16;
            let rpm = ((a * 256 + b) as f32) / 4.0;
            Ok(DecodedMode01Value::EngineRpm(rpm))
        }
        Mode01Pid::ThrottlePosition => {
            let a = data[0] as f32;
            let pct = (a * 100.0) / 255.0;
            Ok(DecodedMode01Value::ThrottlePosition(pct))
        }
        Mode01Pid::ManifoldPressure => {
            let kpa = data[0];
            Ok(DecodedMode01Value::ManifoldPressure(kpa))
        }
        Mode01Pid::CoolantTemp => {
            let temp_c = (data[0] as f32) - 40.0;
            Ok(DecodedMode01Value::CoolantTemp(temp_c))
        }
        Mode01Pid::IntakeAirTemp => {
            let temp_c = (data[0] as f32) - 40.0;
            Ok(DecodedMode01Value::IntakeAirTemp(temp_c))
        }
        Mode01Pid::ControlModuleVoltage => {
            let a = data[0] as u16;
            let b = data[1] as u16;
            let volts = ((a * 256 + b) as f32) / 1000.0;
            Ok(DecodedMode01Value::ControlModuleVoltage(volts))
        }
        Mode01Pid::VehicleSpeed => {
            let kmh = data[0] as f32;
            Ok(DecodedMode01Value::VehicleSpeed(kmh))
        }
        Mode01Pid::WidebandLambda1 => {
            let a = data[0] as u16;
            let b = data[1] as u16;
            let c = data[2] as u16;
            let d = data[3] as u16;

            let lambda = ((a * 256 + b) as f32) / 32768.0;
            let current_ma = (((c * 256 + d) as f32) / 256.0) - 128.0;

            Ok(DecodedMode01Value::WidebandLambda1(
                WidebandO2Sample::new(lambda, current_ma),
            ))
        }
        Mode01Pid::SupportedPids01_20 | Mode01Pid::SupportedPids21_40 | Mode01Pid::SupportedPids41_60 => {
            Err(ObdCodecError::UnexpectedPid {
                expected: 0x00,
                actual: pid.pid(),
            })
        }
    }
}
