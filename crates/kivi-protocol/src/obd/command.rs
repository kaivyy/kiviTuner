use serde::{Deserialize, Serialize};

/// Supported SAE J1979 Mode 01 parameter identifiers (PIDs).
///
/// Strictly read-only engine diagnostics and supported-PID bitmap queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Mode01Pid {
    SupportedPids01_20,
    MonitorStatus,
    FreezeDtc,
    FuelSystemStatus,
    EngineLoad,
    CoolantTemp,
    ShortTermFuelTrimBank1,
    LongTermFuelTrimBank1,
    ShortTermFuelTrimBank2,
    LongTermFuelTrimBank2,
    FuelPressure,
    ManifoldPressure,
    EngineRpm,
    VehicleSpeed,
    TimingAdvance,
    IntakeAirTemp,
    MafAirFlow,
    ThrottlePosition,
    O2SensorsPresent,
    O2Bank1Sensor1,
    O2Bank1Sensor2,
    O2Bank2Sensor1,
    O2Bank2Sensor2,
    ObdStandards,
    EngineRunTime,
    SupportedPids21_40,
    DistanceWithMil,
    FuelRailPressureVacuum,
    FuelRailGaugePressure,
    O2WidebandVoltage1,
    CommandedEgr,
    EgrError,
    EvapPurge,
    FuelTankLevel,
    WarmupsSinceClear,
    DistanceSinceClear,
    BarometricPressure,
    WidebandLambda1,
    CatalystTempBank1Sensor1,
    CatalystTempBank2Sensor1,
    SupportedPids41_60,
    MonitorStatusThisDrive,
    ControlModuleVoltage,
    AbsoluteLoad,
    CommandedEquivalenceRatio,
    RelativeThrottlePosition,
    AmbientAirTemp,
    AbsoluteThrottleB,
    AccelPedalD,
    AccelPedalE,
    CommandedThrottleActuator,
    TimeWithMil,
    TimeSinceClear,
    EthanolFuelPct,
    RelativePedalPosition,
    EngineOilTemp,
    FuelInjectionTiming,
    EngineFuelRate,
    SupportedPids61_80,
    DriverDemandTorque,
    ActualEngineTorque,
    ReferenceTorque,
}

const ALL_SENSOR_PIDS: &[Mode01Pid] = &[
    Mode01Pid::MonitorStatus,
    Mode01Pid::FreezeDtc,
    Mode01Pid::FuelSystemStatus,
    Mode01Pid::EngineLoad,
    Mode01Pid::CoolantTemp,
    Mode01Pid::ShortTermFuelTrimBank1,
    Mode01Pid::LongTermFuelTrimBank1,
    Mode01Pid::ShortTermFuelTrimBank2,
    Mode01Pid::LongTermFuelTrimBank2,
    Mode01Pid::FuelPressure,
    Mode01Pid::ManifoldPressure,
    Mode01Pid::EngineRpm,
    Mode01Pid::VehicleSpeed,
    Mode01Pid::TimingAdvance,
    Mode01Pid::IntakeAirTemp,
    Mode01Pid::MafAirFlow,
    Mode01Pid::ThrottlePosition,
    Mode01Pid::O2SensorsPresent,
    Mode01Pid::O2Bank1Sensor1,
    Mode01Pid::O2Bank1Sensor2,
    Mode01Pid::O2Bank2Sensor1,
    Mode01Pid::O2Bank2Sensor2,
    Mode01Pid::ObdStandards,
    Mode01Pid::EngineRunTime,
    Mode01Pid::DistanceWithMil,
    Mode01Pid::FuelRailPressureVacuum,
    Mode01Pid::FuelRailGaugePressure,
    Mode01Pid::O2WidebandVoltage1,
    Mode01Pid::CommandedEgr,
    Mode01Pid::EgrError,
    Mode01Pid::EvapPurge,
    Mode01Pid::FuelTankLevel,
    Mode01Pid::WarmupsSinceClear,
    Mode01Pid::DistanceSinceClear,
    Mode01Pid::BarometricPressure,
    Mode01Pid::WidebandLambda1,
    Mode01Pid::CatalystTempBank1Sensor1,
    Mode01Pid::CatalystTempBank2Sensor1,
    Mode01Pid::MonitorStatusThisDrive,
    Mode01Pid::ControlModuleVoltage,
    Mode01Pid::AbsoluteLoad,
    Mode01Pid::CommandedEquivalenceRatio,
    Mode01Pid::RelativeThrottlePosition,
    Mode01Pid::AmbientAirTemp,
    Mode01Pid::AbsoluteThrottleB,
    Mode01Pid::AccelPedalD,
    Mode01Pid::AccelPedalE,
    Mode01Pid::CommandedThrottleActuator,
    Mode01Pid::TimeWithMil,
    Mode01Pid::TimeSinceClear,
    Mode01Pid::EthanolFuelPct,
    Mode01Pid::RelativePedalPosition,
    Mode01Pid::EngineOilTemp,
    Mode01Pid::FuelInjectionTiming,
    Mode01Pid::EngineFuelRate,
    Mode01Pid::DriverDemandTorque,
    Mode01Pid::ActualEngineTorque,
    Mode01Pid::ReferenceTorque,
];

impl Mode01Pid {
    /// Returns all readable non-bitmap Mode 01 sensor PIDs in SAE J1979 order.
    pub const fn all_sensor_pids() -> &'static [Mode01Pid] {
        ALL_SENSOR_PIDS
    }

    /// Resolves a raw 1-byte Mode 01 PID number if supported by the codec.
    pub const fn from_u8(raw: u8) -> Option<Self> {
        match raw {
            0x00 => Some(Self::SupportedPids01_20),
            0x01 => Some(Self::MonitorStatus),
            0x02 => Some(Self::FreezeDtc),
            0x03 => Some(Self::FuelSystemStatus),
            0x04 => Some(Self::EngineLoad),
            0x05 => Some(Self::CoolantTemp),
            0x06 => Some(Self::ShortTermFuelTrimBank1),
            0x07 => Some(Self::LongTermFuelTrimBank1),
            0x08 => Some(Self::ShortTermFuelTrimBank2),
            0x09 => Some(Self::LongTermFuelTrimBank2),
            0x0A => Some(Self::FuelPressure),
            0x0B => Some(Self::ManifoldPressure),
            0x0C => Some(Self::EngineRpm),
            0x0D => Some(Self::VehicleSpeed),
            0x0E => Some(Self::TimingAdvance),
            0x0F => Some(Self::IntakeAirTemp),
            0x10 => Some(Self::MafAirFlow),
            0x11 => Some(Self::ThrottlePosition),
            0x13 => Some(Self::O2SensorsPresent),
            0x14 => Some(Self::O2Bank1Sensor1),
            0x15 => Some(Self::O2Bank1Sensor2),
            0x16 => Some(Self::O2Bank2Sensor1),
            0x17 => Some(Self::O2Bank2Sensor2),
            0x1C => Some(Self::ObdStandards),
            0x1F => Some(Self::EngineRunTime),
            0x20 => Some(Self::SupportedPids21_40),
            0x21 => Some(Self::DistanceWithMil),
            0x22 => Some(Self::FuelRailPressureVacuum),
            0x23 => Some(Self::FuelRailGaugePressure),
            0x24 => Some(Self::O2WidebandVoltage1),
            0x2C => Some(Self::CommandedEgr),
            0x2D => Some(Self::EgrError),
            0x2E => Some(Self::EvapPurge),
            0x2F => Some(Self::FuelTankLevel),
            0x30 => Some(Self::WarmupsSinceClear),
            0x31 => Some(Self::DistanceSinceClear),
            0x33 => Some(Self::BarometricPressure),
            0x34 => Some(Self::WidebandLambda1),
            0x3C => Some(Self::CatalystTempBank1Sensor1),
            0x3D => Some(Self::CatalystTempBank2Sensor1),
            0x40 => Some(Self::SupportedPids41_60),
            0x41 => Some(Self::MonitorStatusThisDrive),
            0x42 => Some(Self::ControlModuleVoltage),
            0x43 => Some(Self::AbsoluteLoad),
            0x44 => Some(Self::CommandedEquivalenceRatio),
            0x45 => Some(Self::RelativeThrottlePosition),
            0x46 => Some(Self::AmbientAirTemp),
            0x47 => Some(Self::AbsoluteThrottleB),
            0x49 => Some(Self::AccelPedalD),
            0x4A => Some(Self::AccelPedalE),
            0x4C => Some(Self::CommandedThrottleActuator),
            0x4D => Some(Self::TimeWithMil),
            0x4E => Some(Self::TimeSinceClear),
            0x52 => Some(Self::EthanolFuelPct),
            0x5A => Some(Self::RelativePedalPosition),
            0x5C => Some(Self::EngineOilTemp),
            0x5D => Some(Self::FuelInjectionTiming),
            0x5E => Some(Self::EngineFuelRate),
            0x60 => Some(Self::SupportedPids61_80),
            0x61 => Some(Self::DriverDemandTorque),
            0x62 => Some(Self::ActualEngineTorque),
            0x63 => Some(Self::ReferenceTorque),
            _ => None,
        }
    }

    /// Returns `true` if this PID is a supported-PIDs bitmask query (`0x00/0x20/0x40/0x60`).
    pub const fn is_bitmap(&self) -> bool {
        matches!(
            self,
            Self::SupportedPids01_20
                | Self::SupportedPids21_40
                | Self::SupportedPids41_60
                | Self::SupportedPids61_80
        )
    }

    /// Returns the 1-byte raw hexadecimal PID value.
    pub const fn pid(&self) -> u8 {
        match self {
            Self::SupportedPids01_20 => 0x00,
            Self::MonitorStatus => 0x01,
            Self::FreezeDtc => 0x02,
            Self::FuelSystemStatus => 0x03,
            Self::EngineLoad => 0x04,
            Self::CoolantTemp => 0x05,
            Self::ShortTermFuelTrimBank1 => 0x06,
            Self::LongTermFuelTrimBank1 => 0x07,
            Self::ShortTermFuelTrimBank2 => 0x08,
            Self::LongTermFuelTrimBank2 => 0x09,
            Self::FuelPressure => 0x0A,
            Self::ManifoldPressure => 0x0B,
            Self::EngineRpm => 0x0C,
            Self::VehicleSpeed => 0x0D,
            Self::TimingAdvance => 0x0E,
            Self::IntakeAirTemp => 0x0F,
            Self::MafAirFlow => 0x10,
            Self::ThrottlePosition => 0x11,
            Self::O2SensorsPresent => 0x13,
            Self::O2Bank1Sensor1 => 0x14,
            Self::O2Bank1Sensor2 => 0x15,
            Self::O2Bank2Sensor1 => 0x16,
            Self::O2Bank2Sensor2 => 0x17,
            Self::ObdStandards => 0x1C,
            Self::EngineRunTime => 0x1F,
            Self::SupportedPids21_40 => 0x20,
            Self::DistanceWithMil => 0x21,
            Self::FuelRailPressureVacuum => 0x22,
            Self::FuelRailGaugePressure => 0x23,
            Self::O2WidebandVoltage1 => 0x24,
            Self::CommandedEgr => 0x2C,
            Self::EgrError => 0x2D,
            Self::EvapPurge => 0x2E,
            Self::FuelTankLevel => 0x2F,
            Self::WarmupsSinceClear => 0x30,
            Self::DistanceSinceClear => 0x31,
            Self::BarometricPressure => 0x33,
            Self::WidebandLambda1 => 0x34,
            Self::CatalystTempBank1Sensor1 => 0x3C,
            Self::CatalystTempBank2Sensor1 => 0x3D,
            Self::SupportedPids41_60 => 0x40,
            Self::MonitorStatusThisDrive => 0x41,
            Self::ControlModuleVoltage => 0x42,
            Self::AbsoluteLoad => 0x43,
            Self::CommandedEquivalenceRatio => 0x44,
            Self::RelativeThrottlePosition => 0x45,
            Self::AmbientAirTemp => 0x46,
            Self::AbsoluteThrottleB => 0x47,
            Self::AccelPedalD => 0x49,
            Self::AccelPedalE => 0x4A,
            Self::CommandedThrottleActuator => 0x4C,
            Self::TimeWithMil => 0x4D,
            Self::TimeSinceClear => 0x4E,
            Self::EthanolFuelPct => 0x52,
            Self::RelativePedalPosition => 0x5A,
            Self::EngineOilTemp => 0x5C,
            Self::FuelInjectionTiming => 0x5D,
            Self::EngineFuelRate => 0x5E,
            Self::SupportedPids61_80 => 0x60,
            Self::DriverDemandTorque => 0x61,
            Self::ActualEngineTorque => 0x62,
            Self::ReferenceTorque => 0x63,
        }
    }

    /// Human-readable metric name.
    pub const fn label(&self) -> &'static str {
        match self {
            Self::SupportedPids01_20 => "Supported PIDs 01-20",
            Self::MonitorStatus => "Monitor Status & DTC Count",
            Self::FreezeDtc => "Freeze Frame Trigger DTC",
            Self::FuelSystemStatus => "Fuel System Loop Status",
            Self::EngineLoad => "Calculated Engine Load",
            Self::CoolantTemp => "Engine Coolant Temp",
            Self::ShortTermFuelTrimBank1 => "Short Term Fuel Trim B1",
            Self::LongTermFuelTrimBank1 => "Long Term Fuel Trim B1",
            Self::ShortTermFuelTrimBank2 => "Short Term Fuel Trim B2",
            Self::LongTermFuelTrimBank2 => "Long Term Fuel Trim B2",
            Self::FuelPressure => "Fuel Rail Pressure (Gauge)",
            Self::ManifoldPressure => "Intake Manifold Pressure (MAP)",
            Self::EngineRpm => "Engine Speed (RPM)",
            Self::VehicleSpeed => "Vehicle Speed",
            Self::TimingAdvance => "Ignition Timing Advance",
            Self::IntakeAirTemp => "Intake Air Temp (IAT)",
            Self::MafAirFlow => "Mass Air Flow (MAF)",
            Self::ThrottlePosition => "Throttle Position (TPS)",
            Self::O2SensorsPresent => "O2 Sensors Present Mask",
            Self::O2Bank1Sensor1 => "O2 Bank 1 Sensor 1",
            Self::O2Bank1Sensor2 => "O2 Bank 1 Sensor 2",
            Self::O2Bank2Sensor1 => "O2 Bank 2 Sensor 1",
            Self::O2Bank2Sensor2 => "O2 Bank 2 Sensor 2",
            Self::ObdStandards => "OBD Standard Byte",
            Self::EngineRunTime => "Run Time Since Engine Start",
            Self::SupportedPids21_40 => "Supported PIDs 21-40",
            Self::DistanceWithMil => "Distance Traveled with MIL",
            Self::FuelRailPressureVacuum => "Fuel Rail Pressure (Rel. Vacuum)",
            Self::FuelRailGaugePressure => "Fuel Rail Pressure (Direct/Common)",
            Self::O2WidebandVoltage1 => "Wideband O2S1 Voltage & Lambda",
            Self::CommandedEgr => "Commanded EGR",
            Self::EgrError => "EGR Error",
            Self::EvapPurge => "Commanded EVAP Purge",
            Self::FuelTankLevel => "Fuel Tank Level Input",
            Self::WarmupsSinceClear => "Warm-ups Since DTC Clear",
            Self::DistanceSinceClear => "Distance Since DTC Clear",
            Self::BarometricPressure => "Barometric Pressure",
            Self::WidebandLambda1 => "Wideband O2S1 Current & Lambda",
            Self::CatalystTempBank1Sensor1 => "Catalyst Temp B1S1",
            Self::CatalystTempBank2Sensor1 => "Catalyst Temp B2S1",
            Self::SupportedPids41_60 => "Supported PIDs 41-60",
            Self::MonitorStatusThisDrive => "Monitor Status This Drive",
            Self::ControlModuleVoltage => "Control Module Voltage",
            Self::AbsoluteLoad => "Absolute Load Value",
            Self::CommandedEquivalenceRatio => "Commanded AFR Ratio (Lambda)",
            Self::RelativeThrottlePosition => "Relative Throttle Position",
            Self::AmbientAirTemp => "Ambient Air Temp",
            Self::AbsoluteThrottleB => "Absolute Throttle Position B",
            Self::AccelPedalD => "Accelerator Pedal Position D",
            Self::AccelPedalE => "Accelerator Pedal Position E",
            Self::CommandedThrottleActuator => "Commanded Throttle Actuator",
            Self::TimeWithMil => "Time Run with MIL On",
            Self::TimeSinceClear => "Time Since DTC Clear",
            Self::EthanolFuelPct => "Ethanol Fuel Percentage",
            Self::RelativePedalPosition => "Relative Accelerator Pedal",
            Self::EngineOilTemp => "Engine Oil Temp",
            Self::FuelInjectionTiming => "Fuel Injection Timing",
            Self::EngineFuelRate => "Engine Fuel Rate",
            Self::SupportedPids61_80 => "Supported PIDs 61-80",
            Self::DriverDemandTorque => "Driver Demand Engine Torque",
            Self::ActualEngineTorque => "Actual Engine Torque",
            Self::ReferenceTorque => "Engine Reference Torque",
        }
    }

    /// Physical unit string for display.
    pub const fn unit(&self) -> &'static str {
        match self {
            Self::SupportedPids01_20
            | Self::SupportedPids21_40
            | Self::SupportedPids41_60
            | Self::SupportedPids61_80 => "mask",
            Self::MonitorStatus | Self::MonitorStatusThisDrive => "dtcs",
            Self::FreezeDtc => "dtc",
            Self::FuelSystemStatus | Self::O2SensorsPresent | Self::ObdStandards => "code",
            Self::EngineLoad
            | Self::ShortTermFuelTrimBank1
            | Self::LongTermFuelTrimBank1
            | Self::ShortTermFuelTrimBank2
            | Self::LongTermFuelTrimBank2
            | Self::ThrottlePosition
            | Self::CommandedEgr
            | Self::EgrError
            | Self::EvapPurge
            | Self::FuelTankLevel
            | Self::AbsoluteLoad
            | Self::RelativeThrottlePosition
            | Self::AbsoluteThrottleB
            | Self::AccelPedalD
            | Self::AccelPedalE
            | Self::CommandedThrottleActuator
            | Self::EthanolFuelPct
            | Self::RelativePedalPosition
            | Self::DriverDemandTorque
            | Self::ActualEngineTorque => "%",
            Self::CoolantTemp
            | Self::IntakeAirTemp
            | Self::CatalystTempBank1Sensor1
            | Self::CatalystTempBank2Sensor1
            | Self::AmbientAirTemp
            | Self::EngineOilTemp => "C",
            Self::FuelPressure
            | Self::ManifoldPressure
            | Self::FuelRailPressureVacuum
            | Self::FuelRailGaugePressure
            | Self::BarometricPressure => "kPa",
            Self::EngineRpm => "RPM",
            Self::VehicleSpeed => "km/h",
            Self::TimingAdvance | Self::FuelInjectionTiming => "deg",
            Self::MafAirFlow => "g/s",
            Self::O2Bank1Sensor1
            | Self::O2Bank1Sensor2
            | Self::O2Bank2Sensor1
            | Self::O2Bank2Sensor2
            | Self::ControlModuleVoltage => "V",
            Self::EngineRunTime => "s",
            Self::DistanceWithMil | Self::DistanceSinceClear => "km",
            Self::O2WidebandVoltage1
            | Self::WidebandLambda1
            | Self::CommandedEquivalenceRatio => "lambda",
            Self::WarmupsSinceClear => "cycles",
            Self::TimeWithMil | Self::TimeSinceClear => "min",
            Self::EngineFuelRate => "L/h",
            Self::ReferenceTorque => "Nm",
        }
    }

    /// Logical UI grouping category.
    pub const fn category(&self) -> &'static str {
        match self {
            Self::EngineRpm
            | Self::EngineLoad
            | Self::AbsoluteLoad
            | Self::VehicleSpeed
            | Self::EngineRunTime
            | Self::DriverDemandTorque
            | Self::ActualEngineTorque
            | Self::ReferenceTorque => "Engine & Load",
            Self::CoolantTemp
            | Self::IntakeAirTemp
            | Self::AmbientAirTemp
            | Self::EngineOilTemp
            | Self::CatalystTempBank1Sensor1
            | Self::CatalystTempBank2Sensor1 => "Temperatures",
            Self::ManifoldPressure
            | Self::MafAirFlow
            | Self::BarometricPressure
            | Self::ThrottlePosition
            | Self::RelativeThrottlePosition
            | Self::AbsoluteThrottleB
            | Self::AccelPedalD
            | Self::AccelPedalE
            | Self::CommandedThrottleActuator
            | Self::RelativePedalPosition => "Airflow & Throttle",
            Self::FuelSystemStatus
            | Self::ShortTermFuelTrimBank1
            | Self::LongTermFuelTrimBank1
            | Self::ShortTermFuelTrimBank2
            | Self::LongTermFuelTrimBank2
            | Self::FuelPressure
            | Self::FuelRailPressureVacuum
            | Self::FuelRailGaugePressure
            | Self::TimingAdvance
            | Self::CommandedEquivalenceRatio
            | Self::EthanolFuelPct
            | Self::FuelInjectionTiming
            | Self::EngineFuelRate => "Fuel & Ignition",
            Self::O2SensorsPresent
            | Self::O2Bank1Sensor1
            | Self::O2Bank1Sensor2
            | Self::O2Bank2Sensor1
            | Self::O2Bank2Sensor2
            | Self::O2WidebandVoltage1
            | Self::WidebandLambda1 => "O2 & Lambda",
            Self::CommandedEgr | Self::EgrError | Self::EvapPurge | Self::FuelTankLevel => {
                "Emissions & EVAP"
            }
            Self::MonitorStatus
            | Self::MonitorStatusThisDrive
            | Self::FreezeDtc
            | Self::ObdStandards
            | Self::DistanceWithMil
            | Self::WarmupsSinceClear
            | Self::DistanceSinceClear
            | Self::ControlModuleVoltage
            | Self::TimeWithMil
            | Self::TimeSinceClear
            | Self::SupportedPids01_20
            | Self::SupportedPids21_40
            | Self::SupportedPids41_60
            | Self::SupportedPids61_80 => "Electrical & MIL",
        }
    }
}

/// Supported SAE J1979 Mode 09 vehicle information identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Mode09Item {
    /// Supported Mode 09 items in range 0x01..=0x20 (Base 0x00).
    SupportedPids09_00,
    /// Vehicle Identification Number (VIN) (0x02).
    Vin,
    /// Calibration Identification (Cal ID) (0x04).
    CalibrationId,
}

impl Mode09Item {
    /// Returns the 1-byte raw hexadecimal item value.
    pub const fn item(&self) -> u8 {
        match self {
            Self::SupportedPids09_00 => 0x00,
            Self::Vin => 0x02,
            Self::CalibrationId => 0x04,
        }
    }
}

/// Allowed hardware adapter setup and query commands.
///
/// Restricted to read-only initialization, configuration, and device identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AdapterCommand {
    Reset,
    EchoOff,
    LinefeedsOff,
    SpacesOn,
    HeadersOff,
    AdaptiveTimingAuto,
    SetProtocolCan11_500,
    SetProtocolCan29_500,
    SetProtocolCan11_250,
    SetProtocolCan29_250,
    SetProtocolKwpFastInit,
    SetProtocolKwp5BaudInit,
    SetProtocolIso9141,
    SetProtocolAuto,
    ReadVoltage,
    ReadProtocol,
    QueryElmVersion,
    QueryStnDeviceId,
    QueryStnFirmwareVersion,
    QueryStnSerialNumber,
}

/// Closed typed read-only OBD-II and adapter command domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ObdCommand {
    Mode01(Mode01Pid),
    Mode09(Mode09Item),
    Adapter(AdapterCommand),
}

impl ObdCommand {
    /// Encodes the command into a deterministic ASCII string terminated by `\r`.
    pub const fn encode(&self) -> &'static str {
        match self {
            Self::Mode01(pid) => match pid {
                Mode01Pid::SupportedPids01_20 => "01 00\r",
                Mode01Pid::MonitorStatus => "01 01\r",
                Mode01Pid::FreezeDtc => "01 02\r",
                Mode01Pid::FuelSystemStatus => "01 03\r",
                Mode01Pid::EngineLoad => "01 04\r",
                Mode01Pid::CoolantTemp => "01 05\r",
                Mode01Pid::ShortTermFuelTrimBank1 => "01 06\r",
                Mode01Pid::LongTermFuelTrimBank1 => "01 07\r",
                Mode01Pid::ShortTermFuelTrimBank2 => "01 08\r",
                Mode01Pid::LongTermFuelTrimBank2 => "01 09\r",
                Mode01Pid::FuelPressure => "01 0A\r",
                Mode01Pid::ManifoldPressure => "01 0B\r",
                Mode01Pid::EngineRpm => "01 0C\r",
                Mode01Pid::VehicleSpeed => "01 0D\r",
                Mode01Pid::TimingAdvance => "01 0E\r",
                Mode01Pid::IntakeAirTemp => "01 0F\r",
                Mode01Pid::MafAirFlow => "01 10\r",
                Mode01Pid::ThrottlePosition => "01 11\r",
                Mode01Pid::O2SensorsPresent => "01 13\r",
                Mode01Pid::O2Bank1Sensor1 => "01 14\r",
                Mode01Pid::O2Bank1Sensor2 => "01 15\r",
                Mode01Pid::O2Bank2Sensor1 => "01 16\r",
                Mode01Pid::O2Bank2Sensor2 => "01 17\r",
                Mode01Pid::ObdStandards => "01 1C\r",
                Mode01Pid::EngineRunTime => "01 1F\r",
                Mode01Pid::SupportedPids21_40 => "01 20\r",
                Mode01Pid::DistanceWithMil => "01 21\r",
                Mode01Pid::FuelRailPressureVacuum => "01 22\r",
                Mode01Pid::FuelRailGaugePressure => "01 23\r",
                Mode01Pid::O2WidebandVoltage1 => "01 24\r",
                Mode01Pid::CommandedEgr => "01 2C\r",
                Mode01Pid::EgrError => "01 2D\r",
                Mode01Pid::EvapPurge => "01 2E\r",
                Mode01Pid::FuelTankLevel => "01 2F\r",
                Mode01Pid::WarmupsSinceClear => "01 30\r",
                Mode01Pid::DistanceSinceClear => "01 31\r",
            Mode01Pid::BarometricPressure => "01 33\r",
                Mode01Pid::WidebandLambda1 => "01 34\r",
                Mode01Pid::CatalystTempBank1Sensor1 => "01 3C\r",
                Mode01Pid::CatalystTempBank2Sensor1 => "01 3D\r",
                Mode01Pid::SupportedPids41_60 => "01 40\r",
                Mode01Pid::MonitorStatusThisDrive => "01 41\r",
                Mode01Pid::ControlModuleVoltage => "01 42\r",
                Mode01Pid::AbsoluteLoad => "01 43\r",
                Mode01Pid::CommandedEquivalenceRatio => "01 44\r",
                Mode01Pid::RelativeThrottlePosition => "01 45\r",
                Mode01Pid::AmbientAirTemp => "01 46\r",
                Mode01Pid::AbsoluteThrottleB => "01 47\r",
                Mode01Pid::AccelPedalD => "01 49\r",
                Mode01Pid::AccelPedalE => "01 4A\r",
                Mode01Pid::CommandedThrottleActuator => "01 4C\r",
                Mode01Pid::TimeWithMil => "01 4D\r",
                Mode01Pid::TimeSinceClear => "01 4E\r",
                Mode01Pid::EthanolFuelPct => "01 52\r",
                Mode01Pid::RelativePedalPosition => "01 5A\r",
                Mode01Pid::EngineOilTemp => "01 5C\r",
                Mode01Pid::FuelInjectionTiming => "01 5D\r",
                Mode01Pid::EngineFuelRate => "01 5E\r",
                Mode01Pid::SupportedPids61_80 => "01 60\r",
                Mode01Pid::DriverDemandTorque => "01 61\r",
                Mode01Pid::ActualEngineTorque => "01 62\r",
                Mode01Pid::ReferenceTorque => "01 63\r",
            },
            Self::Mode09(item) => match item {
                Mode09Item::SupportedPids09_00 => "09 00\r",
                Mode09Item::Vin => "09 02\r",
                Mode09Item::CalibrationId => "09 04\r",
            },
            Self::Adapter(cmd) => match cmd {
                AdapterCommand::Reset => "ATZ\r",
                AdapterCommand::EchoOff => "ATE0\r",
                AdapterCommand::LinefeedsOff => "ATL0\r",
                AdapterCommand::SpacesOn => "ATS1\r",
                AdapterCommand::HeadersOff => "ATH0\r",
                AdapterCommand::AdaptiveTimingAuto => "ATAT1\r",
                AdapterCommand::SetProtocolCan11_500 => "ATSP6\r",
                AdapterCommand::SetProtocolCan29_500 => "ATSP7\r",
                AdapterCommand::SetProtocolCan11_250 => "ATSP8\r",
                AdapterCommand::SetProtocolCan29_250 => "ATSP9\r",
                AdapterCommand::SetProtocolKwpFastInit => "ATSP5\r",
                AdapterCommand::SetProtocolKwp5BaudInit => "ATSP4\r",
                AdapterCommand::SetProtocolIso9141 => "ATSP3\r",
                AdapterCommand::SetProtocolAuto => "ATSP0\r",
                AdapterCommand::ReadVoltage => "ATRV\r",
                AdapterCommand::ReadProtocol => "ATDPN\r",
                AdapterCommand::QueryElmVersion => "ATI\r",
                AdapterCommand::QueryStnDeviceId => "STDI\r",
                AdapterCommand::QueryStnFirmwareVersion => "STI\r",
                AdapterCommand::QueryStnSerialNumber => "STSN\r",
            },
        }
    }
}
