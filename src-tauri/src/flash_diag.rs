//! Tauri commands for quick OBD-II / UDS diagnostics over any attached transport
//! (including ELM327 USB/serial cables, OpenPort 2.0, J2534, DoIP, and SocketCAN).

use crate::flash_commands::FlashEngineState;
use kivi_flashing::quick_diag::{
    clear_dtcs, ecu_reset, read_dtcs, read_ecu_identity, DtcClearOutcome, DtcReadOutcome,
    EcuIdentitySummary, EcuResetMode, EcuResetOutcome,
};
use kivi_flashing::serial_elm_device::{probe_elm_capabilities, ElmCapabilityReport};
use tauri::State;

/// Reads battery voltage, VIN, Calibration ID, and CVN using the active adapter.
pub fn diag_read_ecu_info_inner(state: &FlashEngineState) -> Result<EcuIdentitySummary, String> {
    state.with_attached_device(|dev| read_ecu_identity(dev).map_err(|e| e.to_string()))
}

/// Reads stored/confirmed and pending DTCs via UDS `0x19` with OBD Mode `03`/`07` fallback.
pub fn diag_read_dtcs_inner(state: &FlashEngineState) -> Result<DtcReadOutcome, String> {
    state.with_attached_device(|dev| read_dtcs(dev).map_err(|e| e.to_string()))
}

/// Clears DTCs and freeze frames via UDS `0x14` with OBD Mode `04` fallback.
pub fn diag_clear_dtcs_inner(state: &FlashEngineState) -> Result<DtcClearOutcome, String> {
    state.with_attached_device(|dev| clear_dtcs(dev).map_err(|e| e.to_string()))
}

/// Issues a UDS `0x11` ECU reset (`Hard`, `KeyOffOn`, or `Soft`).
pub fn diag_ecu_reset_inner(
    mode: EcuResetMode,
    state: &FlashEngineState,
) -> Result<EcuResetOutcome, String> {
    state.with_attached_device(|dev| ecu_reset(dev, mode).map_err(|e| e.to_string()))
}

/// Probes ELM327 / STN clone firmware capabilities (`ATSH`, `ATCAF0`, K-Line, `STDI`).
pub fn diag_probe_elm_capabilities_inner(
    state: &FlashEngineState,
) -> Result<ElmCapabilityReport, String> {
    state.with_attached_device(|dev| probe_elm_capabilities(dev).map_err(|e| e.to_string()))
}

/// Sends a raw ASCII AT / OBD / K-Line text command on an attached ELM327 bridge.
pub fn diag_send_raw_command_inner(
    command: &str,
    state: &FlashEngineState,
) -> Result<String, String> {
    if command.trim().is_empty() {
        return Err("Raw command must not be empty".to_string());
    }
    state.with_attached_device(|dev| dev.send_raw_command(command).map_err(|e| e.to_string()))
}

#[tauri::command]
pub fn diag_read_ecu_info(state: State<'_, FlashEngineState>) -> Result<EcuIdentitySummary, String> {
    diag_read_ecu_info_inner(&state)
}

#[tauri::command]
pub fn diag_read_dtcs(state: State<'_, FlashEngineState>) -> Result<DtcReadOutcome, String> {
    diag_read_dtcs_inner(&state)
}

#[tauri::command]
pub fn diag_clear_dtcs(state: State<'_, FlashEngineState>) -> Result<DtcClearOutcome, String> {
    diag_clear_dtcs_inner(&state)
}

#[tauri::command]
pub fn diag_ecu_reset(
    mode: EcuResetMode,
    state: State<'_, FlashEngineState>,
) -> Result<EcuResetOutcome, String> {
    diag_ecu_reset_inner(mode, &state)
}

#[tauri::command]
pub fn diag_probe_elm_capabilities(
    state: State<'_, FlashEngineState>,
) -> Result<ElmCapabilityReport, String> {
    diag_probe_elm_capabilities_inner(&state)
}

#[tauri::command]
pub fn diag_send_raw_command(
    command: String,
    state: State<'_, FlashEngineState>,
) -> Result<String, String> {
    diag_send_raw_command_inner(&command, &state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flash_commands::TransportChoice;
    use kivi_flashing::device::{Device, DeviceKind, MockDevice, ScriptedReply, TransportKind};
    use kivi_flashing::hardware_kernel_session::{SharedDevice, UdsHardwareKernelSession};
    use kivi_flashing::j2534_types::{PassThruMsg, ProtocolId};

    fn attach_scripted_elm(script: Vec<ScriptedReply>) -> FlashEngineState {
        let mut dev = MockDevice::new(DeviceKind::Serial, "ELM327 Test", TransportKind::Serial);
        dev.open().unwrap();
        dev.connect(ProtocolId::Iso15765, 500_000).unwrap();
        dev.set_voltage_millivolts(12_650);
        dev.set_script(script);

        let dummy = MockDevice::new(DeviceKind::Serial, "Dummy Session", TransportKind::Serial);
        let session = UdsHardwareKernelSession::new(SharedDevice::new(Box::new(dummy)), 0x10_000);

        let state = FlashEngineState::default();
        state.attach_custom_transport(Box::new(dev), Box::new(session), TransportChoice::Serial);
        state
    }

    fn reply(bytes: Vec<u8>) -> ScriptedReply {
        ScriptedReply::Message(PassThruMsg::new(ProtocolId::Iso15765, bytes))
    }

    #[test]
    fn requires_an_attached_device_before_running_diagnostics() {
        let state = FlashEngineState::default();
        let err = diag_read_dtcs_inner(&state).unwrap_err();
        assert!(err.contains("No flash/OBD device attached"), "got {err}");
    }

    #[test]
    fn tauri_diag_wrappers_roundtrip_info_dtc_clear_and_reset() {
        let mut vin = vec![0x62, 0xF1, 0x90];
        vin.extend_from_slice(b"MLHJC7500L5000123");

        let state = attach_scripted_elm(vec![
            // read_ecu_identity: UDS VIN + Mode09 Cal-ID + Mode09 CVN
            reply(vin),
            reply({
                let mut r = vec![0x49, 0x04, 0x01];
                r.extend_from_slice(b"38770-K15-901");
                r
            }),
            reply(vec![0x49, 0x06, 0x01, 0xAA, 0xBB, 0xCC, 0xDD]),
            // read_dtcs: UDS 0x19 -> 1 DTC P0171
            reply(vec![0x59, 0x02, 0xFF, 0x01, 0x71, 0x00, 0x08]),
            // clear_dtcs: UDS 0x14 positive
            reply(vec![0x54]),
            // ecu_reset: Soft reset positive
            reply(vec![0x51, 0x03]),
        ]);

        let info = diag_read_ecu_info_inner(&state).unwrap();
        assert_eq!(info.battery_mv, Some(12_650));
        assert_eq!(info.vin.as_deref(), Some("MLHJC7500L5000123"));
        assert_eq!(info.cal_id.as_deref(), Some("38770-K15-901"));
        assert_eq!(info.cvn.as_deref(), Some("AABBCCDD"));

        let dtc_res = diag_read_dtcs_inner(&state).unwrap();
        assert_eq!(dtc_res.dtcs.len(), 1);
        assert_eq!(dtc_res.dtcs[0].code, "P0171");

        let clear_res = diag_clear_dtcs_inner(&state).unwrap();
        assert!(clear_res.cleared);

        let reset_res = diag_ecu_reset_inner(EcuResetMode::Soft, &state).unwrap();
        assert!(reset_res.acknowledged);
        assert_eq!(reset_res.mode, EcuResetMode::Soft);
    }

    #[test]
    fn raw_command_and_capability_probe_handle_elm_and_non_elm_transports() {
        let mock_state = attach_scripted_elm(vec![]);
        let blank_err = diag_send_raw_command_inner("   ", &mock_state).unwrap_err();
        assert!(blank_err.contains("must not be empty"), "got {blank_err}");
        let probe_err = diag_probe_elm_capabilities_inner(&mock_state).unwrap_err();
        assert!(!probe_err.is_empty());

        struct CloneElmStub(MockDevice);
        impl Device for CloneElmStub {
            fn open(&mut self) -> Result<(), kivi_flashing::device::DeviceError> {
                self.0.open()
            }
            fn close(&mut self) -> Result<(), kivi_flashing::device::DeviceError> {
                self.0.close()
            }
            fn connect(
                &mut self,
                p: ProtocolId,
                b: u32,
            ) -> Result<(), kivi_flashing::device::DeviceError> {
                self.0.connect(p, b)
            }
            fn disconnect(&mut self) -> Result<(), kivi_flashing::device::DeviceError> {
                self.0.disconnect()
            }
            fn send(&mut self, m: &PassThruMsg) -> Result<(), kivi_flashing::device::DeviceError> {
                self.0.send(m)
            }
            fn receive(&mut self) -> Result<PassThruMsg, kivi_flashing::device::DeviceError> {
                self.0.receive()
            }
            fn read_voltage_millivolts(&mut self) -> Result<u32, kivi_flashing::device::DeviceError> {
                self.0.read_voltage_millivolts()
            }
            fn info(&self) -> kivi_flashing::device::DeviceInfo {
                self.0.info()
            }
            fn is_open(&self) -> bool {
                self.0.is_open()
            }
            fn is_connected(&self) -> bool {
                self.0.is_connected()
            }
            fn connected_protocol(&self) -> Option<ProtocolId> {
                self.0.connected_protocol()
            }
            fn is_mock(&self) -> bool {
                true
            }
            fn supports_flashing(&self) -> bool {
                false
            }
            fn send_raw_command(
                &mut self,
                cmd: &str,
            ) -> Result<String, kivi_flashing::device::DeviceError> {
                match cmd.trim() {
                    "ATI" => Ok("ELM327 v2.1".to_string()),
                    "ATDPN" => Ok("A6".to_string()),
                    "STDI" | "ATCAF0" | "ATTP5" | "ATBI" => Ok("?".to_string()),
                    _ => Ok("OK".to_string()),
                }
            }
        }

        let mut raw_dev = MockDevice::new(DeviceKind::Serial, "Clone ELM Stub", TransportKind::Serial);
        raw_dev.open().unwrap();
        raw_dev.connect(ProtocolId::Iso15765, 500_000).unwrap();
        let dummy = MockDevice::new(DeviceKind::Serial, "Dummy", TransportKind::Serial);
        let session = UdsHardwareKernelSession::new(SharedDevice::new(Box::new(dummy)), 0x10_000);
        let state = FlashEngineState::default();
        state.attach_custom_transport(
            Box::new(CloneElmStub(raw_dev)),
            Box::new(session),
            TransportChoice::Serial,
        );

        assert_eq!(diag_send_raw_command_inner("ATI", &state).unwrap(), "ELM327 v2.1");
        let caps = diag_probe_elm_capabilities_inner(&state).unwrap();
        assert_eq!(caps.adapter_banner, "ELM327 v2.1");
        assert_eq!(caps.chip_tier, "limited_clone_v21");
        assert!(caps.supports_atsh);
        assert!(!caps.supports_atcaf0);
        assert!(!caps.supports_kline_atsp5);
        assert!(!caps.supports_atbi);
        assert!(!caps.supports_st_commands);
        assert!(caps.warnings.len() >= 2);
    }
}
