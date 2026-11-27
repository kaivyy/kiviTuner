use serde::{Deserialize, Serialize};

/// Supported SAE J1979 Mode 01 parameter identifiers (PIDs).
///
/// This enum is strictly closed and limited to read-only engine metrics
/// and supported-PID bitmap queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Mode01Pid {
    /// Supported PIDs in range 0x01..=0x20 (Base 0x00).
    SupportedPids01_20,
    /// Supported PIDs in range 0x21..=0x40 (Base 0x20).
    SupportedPids21_40,
    /// Supported PIDs in range 0x41..=0x60 (Base 0x40).
    SupportedPids41_60,
    /// Engine coolant temperature in °C (0x05).
    CoolantTemp,
    /// Intake manifold absolute pressure in kPa (0x0B).
    ManifoldPressure,
    /// Engine speed in RPM (0x0C).
    EngineRpm,
    /// Vehicle speed in km/h (0x0D).
    VehicleSpeed,
    /// Intake air temperature in °C (0x0F).
    IntakeAirTemp,
    /// Absolute throttle position in % (0x11).
    ThrottlePosition,
    /// Wideband O2 sensor 1 equivalence ratio and current (0x34).
    WidebandLambda1,
    /// Control module / battery voltage in Volts (0x42).
    ControlModuleVoltage,
}

impl Mode01Pid {
    /// Returns the 1-byte raw hexadecimal PID value.
    pub const fn pid(&self) -> u8 {
        match self {
            Self::SupportedPids01_20 => 0x00,
            Self::CoolantTemp => 0x05,
            Self::ManifoldPressure => 0x0B,
            Self::EngineRpm => 0x0C,
            Self::VehicleSpeed => 0x0D,
            Self::IntakeAirTemp => 0x0F,
            Self::ThrottlePosition => 0x11,
            Self::SupportedPids21_40 => 0x20,
            Self::SupportedPids41_60 => 0x40,
            Self::WidebandLambda1 => 0x34,
            Self::ControlModuleVoltage => 0x42,
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
    /// Reset adapter to power-on state ("ATZ\r").
    Reset,
    /// Disable character echo ("ATE0\r").
    EchoOff,
    /// Disable line feeds ("ATL0\r").
    LinefeedsOff,
    /// Enable space delimiters between hex bytes ("ATS1\r").
    SpacesOn,
    /// Disable CAN headers display ("ATH0\r").
    HeadersOff,
    /// Set adaptive timing to mode 1 (standard) ("ATAT1\r").
    AdaptiveTimingAuto,
    /// Set protocol to ISO 15765-4 CAN (11 bit ID, 500 kbaud) ("ATSP6\r").
    SetProtocolCan11_500,
    /// Set protocol to automatic search ("ATSP0\r").
    SetProtocolAuto,
    /// Read adapter input battery voltage ("ATRV\r").
    ReadVoltage,
    /// Read active protocol number ("ATDPN\r").
    ReadProtocol,
    /// Query ELM327 identity / version string ("ATI\r").
    QueryElmVersion,
    /// Query STN device hardware identifier ("STDI\r").
    QueryStnDeviceId,
    /// Query STN firmware version string ("STI\r").
    QueryStnFirmwareVersion,
    /// Query STN device serial number ("STSN\r").
    QueryStnSerialNumber,
}

/// Closed typed read-only OBD-II and adapter command domain.
///
/// Ensures compile-time restriction against arbitrary string transmission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ObdCommand {
    /// Mode 01 diagnostic request.
    Mode01(Mode01Pid),
    /// Mode 09 vehicle information request.
    Mode09(Mode09Item),
    /// Hardware adapter configuration or query command.
    Adapter(AdapterCommand),
}

impl ObdCommand {
    /// Encodes the command into a deterministic ASCII string terminated by carriage return (`\r`).
    ///
    /// All returned slices are static string literals.
    pub const fn encode(&self) -> &'static str {
        match self {
            Self::Mode01(pid) => match pid {
                Mode01Pid::SupportedPids01_20 => "01 00\r",
                Mode01Pid::SupportedPids21_40 => "01 20\r",
                Mode01Pid::SupportedPids41_60 => "01 40\r",
                Mode01Pid::CoolantTemp => "01 05\r",
                Mode01Pid::ManifoldPressure => "01 0B\r",
                Mode01Pid::EngineRpm => "01 0C\r",
                Mode01Pid::VehicleSpeed => "01 0D\r",
                Mode01Pid::IntakeAirTemp => "01 0F\r",
                Mode01Pid::ThrottlePosition => "01 11\r",
                Mode01Pid::WidebandLambda1 => "01 34\r",
                Mode01Pid::ControlModuleVoltage => "01 42\r",
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
