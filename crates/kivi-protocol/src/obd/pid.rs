use crate::obd::command::Mode01Pid;
use crate::obd::errors::ObdCodecError;
use serde::{Deserialize, Serialize};

/// Wideband oxygen sensor measurement sample decoded from Mode 01 PID 0x34.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WidebandO2Sample {
    /// Equivalence ratio (lambda). Stoichiometric is 1.0, rich < 1.0, lean > 1.0.
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

    /// Computes estimated air-fuel ratio (AFR) given a fuel stoichiometric ratio.
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

/// Decoded monitor readiness & MIL summary from Mode 01 PID 0x01 or 0x41.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonitorStatusSample {
    pub mil_on: bool,
    pub dtc_count: u8,
    pub compression_ignition: bool,
    pub raw_bytes: [u8; 4],
}

/// Decoded narrowband oxygen sensor reading from Mode 01 PID 0x14..=0x17.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NarrowbandO2Sample {
    pub voltage_v: f32,
    pub stft_pct: Option<f32>,
}

/// Strongly-typed decoded values from SAE J1979 Mode 01 diagnostic payloads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DecodedMode01Value {
    EngineRpm(f32),
    ThrottlePosition(f32),
    ManifoldPressure(u8),
    CoolantTemp(f32),
    IntakeAirTemp(f32),
    ControlModuleVoltage(f32),
    VehicleSpeed(f32),
    WidebandLambda1(WidebandO2Sample),
    TimingAdvance(f32),
    MonitorStatus(MonitorStatusSample),
    NarrowbandO2 { sensor: u8, sample: NarrowbandO2Sample },
    WidebandVoltage1 { equivalence_ratio: f32, voltage_v: f32 },
    FuelSystemStatus(u8, u8),
    FreezeDtcCode(String),
    Scalar { pid: Mode01Pid, value: f32 },
}

impl DecodedMode01Value {
    /// Returns the numeric primary reading when representable as `f32`.
    pub fn numeric_value(&self) -> Option<f32> {
        match self {
            Self::EngineRpm(v)
            | Self::ThrottlePosition(v)
            | Self::CoolantTemp(v)
            | Self::IntakeAirTemp(v)
            | Self::ControlModuleVoltage(v)
            | Self::VehicleSpeed(v)
            | Self::TimingAdvance(v)
            | Self::Scalar { value: v, .. } => Some(*v),
            Self::ManifoldPressure(kpa) => Some(f32::from(*kpa)),
            Self::WidebandLambda1(s) => Some(s.equivalence_ratio),
            Self::MonitorStatus(m) => Some(f32::from(m.dtc_count)),
            Self::NarrowbandO2 { sample, .. } => Some(sample.voltage_v),
            Self::WidebandVoltage1 {
                equivalence_ratio, ..
            } => Some(*equivalence_ratio),
            Self::FuelSystemStatus(b1, _) => Some(f32::from(*b1)),
            Self::FreezeDtcCode(_) => None,
        }
    }

    /// Formats the decoded sample concisely for operator display.
    pub fn display_value(&self) -> String {
        match self {
            Self::EngineRpm(v) => format!("{v:.0} RPM"),
            Self::ThrottlePosition(v) => format!("{v:.1} %"),
            Self::ManifoldPressure(v) => format!("{v} kPa"),
            Self::CoolantTemp(v) | Self::IntakeAirTemp(v) => format!("{v:.1} C"),
            Self::ControlModuleVoltage(v) => format!("{v:.2} V"),
            Self::VehicleSpeed(v) => format!("{v:.0} km/h"),
            Self::WidebandLambda1(s) => {
                format!("{:.3} lambda ({:+.1} mA)", s.equivalence_ratio, s.sensor_current_ma)
            }
            Self::TimingAdvance(v) => format!("{v:+.1} deg"),
            Self::MonitorStatus(m) => {
                format!(
                    "MIL {}, {} DTC(s)",
                    if m.mil_on { "ON" } else { "OFF" },
                    m.dtc_count
                )
            }
            Self::NarrowbandO2 { sample, .. } => match sample.stft_pct {
                Some(stft) => format!("{:.3} V ({stft:+.1}% STFT)", sample.voltage_v),
                None => format!("{:.3} V", sample.voltage_v),
            },
            Self::WidebandVoltage1 {
                equivalence_ratio,
                voltage_v,
            } => format!("{equivalence_ratio:.3} lambda ({voltage_v:.2} V)"),
            Self::FuelSystemStatus(b1, b2) => format!("B1={b1}, B2={b2}"),
            Self::FreezeDtcCode(code) => code.clone(),
            Self::Scalar { pid, value } => {
                let unit = pid.unit();
                if matches!(
                    pid,
                    Mode01Pid::EngineRunTime
                        | Mode01Pid::DistanceWithMil
                        | Mode01Pid::WarmupsSinceClear
                        | Mode01Pid::DistanceSinceClear
                        | Mode01Pid::TimeWithMil
                        | Mode01Pid::TimeSinceClear
                        | Mode01Pid::ReferenceTorque
                        | Mode01Pid::O2SensorsPresent
                        | Mode01Pid::ObdStandards
                ) {
                    format!("{value:.0} {unit}")
                } else {
                    format!("{value:.2} {unit}")
                }
            }
        }
    }
}

fn expected_data_len(pid: Mode01Pid) -> usize {
    match pid {
        Mode01Pid::SupportedPids01_20
        | Mode01Pid::MonitorStatus
        | Mode01Pid::SupportedPids21_40
        | Mode01Pid::O2WidebandVoltage1
        | Mode01Pid::WidebandLambda1
        | Mode01Pid::SupportedPids41_60
        | Mode01Pid::MonitorStatusThisDrive
        | Mode01Pid::SupportedPids61_80 => 4,
        Mode01Pid::FreezeDtc
        | Mode01Pid::FuelSystemStatus
        | Mode01Pid::EngineRpm
        | Mode01Pid::MafAirFlow
        | Mode01Pid::O2Bank1Sensor1
        | Mode01Pid::O2Bank1Sensor2
        | Mode01Pid::O2Bank2Sensor1
        | Mode01Pid::O2Bank2Sensor2
        | Mode01Pid::EngineRunTime
        | Mode01Pid::DistanceWithMil
        | Mode01Pid::FuelRailPressureVacuum
        | Mode01Pid::FuelRailGaugePressure
        | Mode01Pid::DistanceSinceClear
        | Mode01Pid::CatalystTempBank1Sensor1
        | Mode01Pid::CatalystTempBank2Sensor1
        | Mode01Pid::ControlModuleVoltage
        | Mode01Pid::AbsoluteLoad
        | Mode01Pid::CommandedEquivalenceRatio
        | Mode01Pid::TimeWithMil
        | Mode01Pid::TimeSinceClear
        | Mode01Pid::FuelInjectionTiming
        | Mode01Pid::EngineFuelRate
        | Mode01Pid::ReferenceTorque => 2,
        Mode01Pid::EngineLoad
        | Mode01Pid::CoolantTemp
        | Mode01Pid::ShortTermFuelTrimBank1
        | Mode01Pid::LongTermFuelTrimBank1
        | Mode01Pid::ShortTermFuelTrimBank2
        | Mode01Pid::LongTermFuelTrimBank2
        | Mode01Pid::FuelPressure
        | Mode01Pid::ManifoldPressure
        | Mode01Pid::VehicleSpeed
        | Mode01Pid::TimingAdvance
        | Mode01Pid::IntakeAirTemp
        | Mode01Pid::ThrottlePosition
        | Mode01Pid::O2SensorsPresent
        | Mode01Pid::ObdStandards
        | Mode01Pid::CommandedEgr
        | Mode01Pid::EgrError
        | Mode01Pid::EvapPurge
        | Mode01Pid::FuelTankLevel
        | Mode01Pid::WarmupsSinceClear
        | Mode01Pid::BarometricPressure
        | Mode01Pid::RelativeThrottlePosition
        | Mode01Pid::AmbientAirTemp
        | Mode01Pid::AbsoluteThrottleB
        | Mode01Pid::AccelPedalD
        | Mode01Pid::AccelPedalE
        | Mode01Pid::CommandedThrottleActuator
        | Mode01Pid::EthanolFuelPct
        | Mode01Pid::RelativePedalPosition
        | Mode01Pid::EngineOilTemp
        | Mode01Pid::DriverDemandTorque
        | Mode01Pid::ActualEngineTorque => 1,
    }
}

fn decode_j2012_pair(hi: u8, lo: u8) -> String {
    if hi == 0 && lo == 0 {
        return "P0000".to_string();
    }
    let prefix = match (hi >> 6) & 0x03 {
        0 => 'P',
        1 => 'C',
        2 => 'B',
        _ => 'U',
    };
    let d1 = (hi >> 4) & 0x03;
    let d2 = hi & 0x0F;
    format!("{prefix}{d1}{d2:X}{lo:02X}")
}

fn u16_ab(data: &[u8]) -> f32 {
    f32::from((u16::from(data[0]) << 8) | u16::from(data[1]))
}

/// Decodes and strictly validates a Mode 01 diagnostic payload for the specified PID.
pub fn decode_mode01_payload(
    pid: Mode01Pid,
    payload: &[u8],
) -> Result<DecodedMode01Value, ObdCodecError> {
    let expected_total_len = 2 + expected_data_len(pid);

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
    let a = f32::from(data[0]);

    match pid {
        Mode01Pid::EngineRpm => Ok(DecodedMode01Value::EngineRpm(u16_ab(data) / 4.0)),
        Mode01Pid::ThrottlePosition => {
            Ok(DecodedMode01Value::ThrottlePosition((a * 100.0) / 255.0))
        }
        Mode01Pid::ManifoldPressure => Ok(DecodedMode01Value::ManifoldPressure(data[0])),
        Mode01Pid::CoolantTemp => Ok(DecodedMode01Value::CoolantTemp(a - 40.0)),
        Mode01Pid::IntakeAirTemp => Ok(DecodedMode01Value::IntakeAirTemp(a - 40.0)),
        Mode01Pid::ControlModuleVoltage => {
            Ok(DecodedMode01Value::ControlModuleVoltage(u16_ab(data) / 1000.0))
        }
        Mode01Pid::VehicleSpeed => Ok(DecodedMode01Value::VehicleSpeed(a)),
        Mode01Pid::TimingAdvance => Ok(DecodedMode01Value::TimingAdvance(a / 2.0 - 64.0)),
        Mode01Pid::WidebandLambda1 => {
            let lambda = u16_ab(&data[0..2]) / 32768.0;
            let current_ma = (u16_ab(&data[2..4]) / 256.0) - 128.0;
            Ok(DecodedMode01Value::WidebandLambda1(WidebandO2Sample::new(
                lambda, current_ma,
            )))
        }
        Mode01Pid::O2WidebandVoltage1 => {
            let lambda = u16_ab(&data[0..2]) / 32768.0;
            let voltage_v = (u16_ab(&data[2..4]) * 8.0) / 65536.0;
            Ok(DecodedMode01Value::WidebandVoltage1 {
                equivalence_ratio: lambda,
                voltage_v,
            })
        }
        Mode01Pid::MonitorStatus | Mode01Pid::MonitorStatusThisDrive => {
            Ok(DecodedMode01Value::MonitorStatus(MonitorStatusSample {
                mil_on: (data[0] & 0x80) != 0,
                dtc_count: data[0] & 0x7F,
                compression_ignition: (data[1] & 0x08) != 0,
                raw_bytes: [data[0], data[1], data[2], data[3]],
            }))
        }
        Mode01Pid::FreezeDtc => Ok(DecodedMode01Value::FreezeDtcCode(decode_j2012_pair(
            data[0], data[1],
        ))),
        Mode01Pid::FuelSystemStatus => Ok(DecodedMode01Value::FuelSystemStatus(data[0], data[1])),
        Mode01Pid::O2Bank1Sensor1
        | Mode01Pid::O2Bank1Sensor2
        | Mode01Pid::O2Bank2Sensor1
        | Mode01Pid::O2Bank2Sensor2 => {
            let sensor = match pid {
                Mode01Pid::O2Bank1Sensor1 => 1,
                Mode01Pid::O2Bank1Sensor2 => 2,
                Mode01Pid::O2Bank2Sensor1 => 3,
                _ => 4,
            };
            let stft_pct = if data[1] == 0xFF {
                None
            } else {
                Some(f32::from(data[1]) * 100.0 / 128.0 - 100.0)
            };
            Ok(DecodedMode01Value::NarrowbandO2 {
                sensor,
                sample: NarrowbandO2Sample {
                    voltage_v: a / 200.0,
                    stft_pct,
                },
            })
        }
        Mode01Pid::EngineLoad
        | Mode01Pid::CommandedEgr
        | Mode01Pid::EvapPurge
        | Mode01Pid::FuelTankLevel
        | Mode01Pid::RelativeThrottlePosition
        | Mode01Pid::AbsoluteThrottleB
        | Mode01Pid::AccelPedalD
        | Mode01Pid::AccelPedalE
        | Mode01Pid::CommandedThrottleActuator
        | Mode01Pid::EthanolFuelPct
        | Mode01Pid::RelativePedalPosition => Ok(DecodedMode01Value::Scalar {
            pid,
            value: (a * 100.0) / 255.0,
        }),
        Mode01Pid::ShortTermFuelTrimBank1
        | Mode01Pid::LongTermFuelTrimBank1
        | Mode01Pid::ShortTermFuelTrimBank2
        | Mode01Pid::LongTermFuelTrimBank2
        | Mode01Pid::EgrError => Ok(DecodedMode01Value::Scalar {
            pid,
            value: (a * 100.0) / 128.0 - 100.0,
        }),
        Mode01Pid::FuelPressure => Ok(DecodedMode01Value::Scalar {
            pid,
            value: 3.0 * a,
        }),
        Mode01Pid::MafAirFlow => Ok(DecodedMode01Value::Scalar {
            pid,
            value: u16_ab(data) / 100.0,
        }),
        Mode01Pid::O2SensorsPresent
        | Mode01Pid::ObdStandards
        | Mode01Pid::WarmupsSinceClear
        | Mode01Pid::BarometricPressure => Ok(DecodedMode01Value::Scalar { pid, value: a }),
        Mode01Pid::EngineRunTime
        | Mode01Pid::DistanceWithMil
        | Mode01Pid::DistanceSinceClear
        | Mode01Pid::TimeWithMil
        | Mode01Pid::TimeSinceClear
        | Mode01Pid::ReferenceTorque => Ok(DecodedMode01Value::Scalar {
            pid,
            value: u16_ab(data),
        }),
        Mode01Pid::FuelRailPressureVacuum => Ok(DecodedMode01Value::Scalar {
            pid,
            value: u16_ab(data) * 0.079,
        }),
        Mode01Pid::FuelRailGaugePressure => Ok(DecodedMode01Value::Scalar {
            pid,
            value: u16_ab(data) * 10.0,
        }),
        Mode01Pid::CatalystTempBank1Sensor1 | Mode01Pid::CatalystTempBank2Sensor1 => {
            Ok(DecodedMode01Value::Scalar {
                pid,
                value: (u16_ab(data) / 10.0) - 40.0,
            })
        }
        Mode01Pid::AbsoluteLoad => Ok(DecodedMode01Value::Scalar {
            pid,
            value: (u16_ab(data) * 100.0) / 255.0,
        }),
        Mode01Pid::CommandedEquivalenceRatio => Ok(DecodedMode01Value::Scalar {
            pid,
            value: (u16_ab(data) * 2.0) / 65536.0,
        }),
        Mode01Pid::AmbientAirTemp | Mode01Pid::EngineOilTemp => Ok(DecodedMode01Value::Scalar {
            pid,
            value: a - 40.0,
        }),
        Mode01Pid::FuelInjectionTiming => Ok(DecodedMode01Value::Scalar {
            pid,
            value: (u16_ab(data) / 128.0) - 210.0,
        }),
        Mode01Pid::EngineFuelRate => Ok(DecodedMode01Value::Scalar {
            pid,
            value: u16_ab(data) / 20.0,
        }),
        Mode01Pid::DriverDemandTorque | Mode01Pid::ActualEngineTorque => {
            Ok(DecodedMode01Value::Scalar {
                pid,
                value: a - 125.0,
            })
        }
        Mode01Pid::SupportedPids01_20
        | Mode01Pid::SupportedPids21_40
        | Mode01Pid::SupportedPids41_60
        | Mode01Pid::SupportedPids61_80 => Err(ObdCodecError::UnexpectedPid {
            expected: 0x00,
            actual: pid.pid(),
        }),
    }
}
