use crate::obd::command::{AdapterCommand, ObdCommand};
use serde::{Deserialize, Serialize};

/// High-level classification of OBD and hardware adapter commands by their operational side effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CommandClassification {
    /// Resets hardware adapter state (e.g., reboots microcontroller, restores defaults).
    AdapterReset,
    /// Mutates adapter communication parameters (echo, linefeeds, protocol, timing).
    AdapterConfiguration,
    /// Read-only device identification queries (firmware version, serial number, battery voltage).
    AdapterIdentification,
    /// Read-only SAE J1979 ECU diagnostic or vehicle information requests.
    EcuDiagnosticRequest,
}

/// Classifies a typed OBD command into its operational category.
pub fn classify_command(cmd: &ObdCommand) -> CommandClassification {
    match cmd {
        ObdCommand::Mode01(_) | ObdCommand::Mode09(_) => {
            CommandClassification::EcuDiagnosticRequest
        }
        ObdCommand::Adapter(adapter_cmd) => match adapter_cmd {
            AdapterCommand::Reset => CommandClassification::AdapterReset,
            AdapterCommand::EchoOff
            | AdapterCommand::LinefeedsOff
            | AdapterCommand::SpacesOn
            | AdapterCommand::HeadersOff
            | AdapterCommand::AdaptiveTimingAuto
            | AdapterCommand::SetProtocolCan11_500
            | AdapterCommand::SetProtocolAuto => CommandClassification::AdapterConfiguration,
            AdapterCommand::ReadVoltage
            | AdapterCommand::ReadProtocol
            | AdapterCommand::QueryElmVersion
            | AdapterCommand::QueryStnDeviceId
            | AdapterCommand::QueryStnFirmwareVersion
            | AdapterCommand::QueryStnSerialNumber => CommandClassification::AdapterIdentification,
        },
    }
}
