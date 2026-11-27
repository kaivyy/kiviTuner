//! Tauri commands for quick OBD-II / UDS diagnostics over any attached transport
//! (including ELM327 USB/serial cables, OpenPort 2.0, J2534, DoIP, and SocketCAN).

use crate::flash_commands::FlashEngineState;
use kivi_flashing::dtc_dictionary::{lookup_dtc_description, DtcDescription};
use kivi_flashing::quick_diag::{
    clear_dtcs_for_can_id, ecu_reset_for_can_id, read_dtcs_for_can_id, read_ecu_identity_for_can_id,
    DtcClearOutcome, DtcReadOutcome, EcuIdentitySummary, EcuResetMode, EcuResetOutcome,
};
use kivi_flashing::serial_elm_device::{probe_elm_capabilities, ElmCapabilityReport};
use kivi_flashing::uds_diagnostics::{
    read_freeze_frame, read_im_readiness, read_mode06_monitors, read_obd_live_snapshot,
    scan_ecu_modules, uds_enter_session, uds_io_control, uds_read_standard_dids,
    uds_routine_control, DiscoveredEcuModule, FreezeFrameSummary, ImReadinessSummary,
    Mode06TestResult, ObdLiveSnapshot, UdsDidValue, UdsIoControlOutcome, UdsRoutineOutcome,
};
use tauri::State;

/// Reads battery voltage, VIN, Calibration ID, and CVN from the specified or default (`0x7E0`) ECU.
pub fn diag_read_ecu_info_for_can_id_inner(
    tx_can_id: Option<u32>,
    state: &FlashEngineState,
) -> Result<EcuIdentitySummary, String> {
    let target = tx_can_id.unwrap_or(0x0000_07E0);
    state.with_attached_device(|dev| read_ecu_identity_for_can_id(dev, target).map_err(|e| e.to_string()))
}

/// Reads battery voltage, VIN, Calibration ID, and CVN using the active adapter (`0x7E0`).
pub fn diag_read_ecu_info_inner(state: &FlashEngineState) -> Result<EcuIdentitySummary, String> {
    diag_read_ecu_info_for_can_id_inner(None, state)
}

/// Reads stored/confirmed and pending DTCs via UDS `0x19` with OBD Mode `03`/`07` fallback.
pub fn diag_read_dtcs_for_can_id_inner(
    tx_can_id: Option<u32>,
    state: &FlashEngineState,
) -> Result<DtcReadOutcome, String> {
    let target = tx_can_id.unwrap_or(0x0000_07E0);
    state.with_attached_device(|dev| read_dtcs_for_can_id(dev, target).map_err(|e| e.to_string()))
}

/// Reads stored/confirmed and pending DTCs from the default (`0x7E0`) ECU.
pub fn diag_read_dtcs_inner(state: &FlashEngineState) -> Result<DtcReadOutcome, String> {
    diag_read_dtcs_for_can_id_inner(None, state)
}

/// Clears DTCs and freeze frames via UDS `0x14` with OBD Mode `04` fallback.
pub fn diag_clear_dtcs_for_can_id_inner(
    tx_can_id: Option<u32>,
    state: &FlashEngineState,
) -> Result<DtcClearOutcome, String> {
    let target = tx_can_id.unwrap_or(0x0000_07E0);
    state.with_attached_device(|dev| clear_dtcs_for_can_id(dev, target).map_err(|e| e.to_string()))
}

/// Clears DTCs and freeze frames from the default (`0x7E0`) ECU.
pub fn diag_clear_dtcs_inner(state: &FlashEngineState) -> Result<DtcClearOutcome, String> {
    diag_clear_dtcs_for_can_id_inner(None, state)
}

/// Issues a UDS `0x11` ECU reset (`Hard`, `KeyOffOn`, or `Soft`) to the target ECU.
pub fn diag_ecu_reset_for_can_id_inner(
    mode: EcuResetMode,
    tx_can_id: Option<u32>,
    state: &FlashEngineState,
) -> Result<EcuResetOutcome, String> {
    let target = tx_can_id.unwrap_or(0x0000_07E0);
    state.with_attached_device(|dev| ecu_reset_for_can_id(dev, target, mode).map_err(|e| e.to_string()))
}

/// Issues a UDS `0x11` ECU reset (`Hard`, `KeyOffOn`, or `Soft`) to `0x7E0`.
pub fn diag_ecu_reset_inner(
    mode: EcuResetMode,
    state: &FlashEngineState,
) -> Result<EcuResetOutcome, String> {
    diag_ecu_reset_for_can_id_inner(mode, None, state)
}

/// Looks up offline J2012 / OEM DTC descriptions.
pub fn diag_lookup_dtc_inner(code: &str) -> DtcDescription {
    lookup_dtc_description(code)
}

/// Reads supported OBD Mode 01 live sensor PIDs (`0x04..=0x63`).
pub fn diag_read_live_pids_inner(
    tx_can_id: Option<u32>,
    pids: Option<Vec<u8>>,
    state: &FlashEngineState,
) -> Result<ObdLiveSnapshot, String> {
    let target = tx_can_id.unwrap_or(0x0000_07E0);
    state.with_attached_device(|dev| {
        read_obd_live_snapshot(dev, target, pids.as_deref()).map_err(|e| e.to_string())
    })
}

/// Reads Mode 01 PID `0x01` I/M readiness monitors and Mode `0x0A` permanent DTCs.
pub fn diag_read_im_readiness_inner(
    tx_can_id: Option<u32>,
    state: &FlashEngineState,
) -> Result<ImReadinessSummary, String> {
    let target = tx_can_id.unwrap_or(0x0000_07E0);
    state.with_attached_device(|dev| read_im_readiness(dev, target).map_err(|e| e.to_string()))
}

/// Reads Mode 02 Freeze Frame snapshot (frame `0x00`).
pub fn diag_read_freeze_frame_inner(
    tx_can_id: Option<u32>,
    state: &FlashEngineState,
) -> Result<FreezeFrameSummary, String> {
    let target = tx_can_id.unwrap_or(0x0000_07E0);
    state.with_attached_device(|dev| read_freeze_frame(dev, target).map_err(|e| e.to_string()))
}

/// Reads Mode 06 CAN on-board monitor test values.
pub fn diag_read_mode06_monitors_inner(
    tx_can_id: Option<u32>,
    mids: Option<Vec<u8>>,
    state: &FlashEngineState,
) -> Result<Vec<Mode06TestResult>, String> {
    let target = tx_can_id.unwrap_or(0x0000_07E0);
    state.with_attached_device(|dev| {
        read_mode06_monitors(dev, target, mids.as_deref()).map_err(|e| e.to_string())
    })
}

/// Scans standard 11-bit (`0x7E0..=0x7E7`) and optional 29-bit ECUs on the active bus.
pub fn diag_scan_ecu_modules_inner(
    include_29bit: bool,
    state: &FlashEngineState,
) -> Result<Vec<DiscoveredEcuModule>, String> {
    state.with_attached_device(|dev| scan_ecu_modules(dev, include_29bit).map_err(|e| e.to_string()))
}

/// Switches UDS DiagnosticSessionControl (`0x10`).
pub fn diag_uds_enter_session_inner(
    tx_can_id: Option<u32>,
    session_subfunction: u8,
    state: &FlashEngineState,
) -> Result<String, String> {
    let target = tx_can_id.unwrap_or(0x0000_07E0);
    state.with_attached_device(|dev| {
        uds_enter_session(dev, target, session_subfunction).map_err(|e| e.to_string())
    })
}

/// Reads standard or custom UDS DIDs via service `0x22`.
pub fn diag_uds_read_dids_inner(
    tx_can_id: Option<u32>,
    dids: Option<Vec<u16>>,
    state: &FlashEngineState,
) -> Result<Vec<UdsDidValue>, String> {
    let target = tx_can_id.unwrap_or(0x0000_07E0);
    state.with_attached_device(|dev| {
        uds_read_standard_dids(dev, target, dids.as_deref()).map_err(|e| e.to_string())
    })
}

/// Runs UDS RoutineControl (`0x31`) for service resets and adaptations.
pub fn diag_uds_routine_control_inner(
    tx_can_id: Option<u32>,
    subfunction: u8,
    routine_id: u16,
    option_record: Option<Vec<u8>>,
    state: &FlashEngineState,
) -> Result<UdsRoutineOutcome, String> {
    let target = tx_can_id.unwrap_or(0x0000_07E0);
    let rec = option_record.unwrap_or_default();
    state.with_attached_device(|dev| {
        uds_routine_control(dev, target, subfunction, routine_id, &rec).map_err(|e| e.to_string())
    })
}

/// Runs UDS InputOutputControlByIdentifier (`0x2F`) for actuator tests.
pub fn diag_uds_io_control_inner(
    tx_can_id: Option<u32>,
    did: u16,
    control_param: u8,
    control_state: Option<Vec<u8>>,
    state: &FlashEngineState,
) -> Result<UdsIoControlOutcome, String> {
    let target = tx_can_id.unwrap_or(0x0000_07E0);
    let st = control_state.unwrap_or_default();
    state.with_attached_device(|dev| {
        uds_io_control(dev, target, did, control_param, &st).map_err(|e| e.to_string())
    })
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
pub fn diag_read_ecu_info(
    tx_can_id: Option<u32>,
    state: State<'_, FlashEngineState>,
) -> Result<EcuIdentitySummary, String> {
    diag_read_ecu_info_for_can_id_inner(tx_can_id, &state)
}

#[tauri::command]
pub fn diag_read_dtcs(
    tx_can_id: Option<u32>,
    state: State<'_, FlashEngineState>,
) -> Result<DtcReadOutcome, String> {
    diag_read_dtcs_for_can_id_inner(tx_can_id, &state)
}

#[tauri::command]
pub fn diag_clear_dtcs(
    tx_can_id: Option<u32>,
    state: State<'_, FlashEngineState>,
) -> Result<DtcClearOutcome, String> {
    diag_clear_dtcs_for_can_id_inner(tx_can_id, &state)
}

#[tauri::command]
pub fn diag_ecu_reset(
    mode: EcuResetMode,
    tx_can_id: Option<u32>,
    state: State<'_, FlashEngineState>,
) -> Result<EcuResetOutcome, String> {
    diag_ecu_reset_for_can_id_inner(mode, tx_can_id, &state)
}

#[tauri::command]
pub fn diag_lookup_dtc(code: String) -> DtcDescription {
    diag_lookup_dtc_inner(&code)
}

#[tauri::command]
pub fn diag_read_live_pids(
    tx_can_id: Option<u32>,
    pids: Option<Vec<u8>>,
    state: State<'_, FlashEngineState>,
) -> Result<ObdLiveSnapshot, String> {
    diag_read_live_pids_inner(tx_can_id, pids, &state)
}

#[tauri::command]
pub fn diag_read_im_readiness(
    tx_can_id: Option<u32>,
    state: State<'_, FlashEngineState>,
) -> Result<ImReadinessSummary, String> {
    diag_read_im_readiness_inner(tx_can_id, &state)
}

#[tauri::command]
pub fn diag_read_freeze_frame(
    tx_can_id: Option<u32>,
    state: State<'_, FlashEngineState>,
) -> Result<FreezeFrameSummary, String> {
    diag_read_freeze_frame_inner(tx_can_id, &state)
}

#[tauri::command]
pub fn diag_read_mode06_monitors(
    tx_can_id: Option<u32>,
    mids: Option<Vec<u8>>,
    state: State<'_, FlashEngineState>,
) -> Result<Vec<Mode06TestResult>, String> {
    diag_read_mode06_monitors_inner(tx_can_id, mids, &state)
}

#[tauri::command]
pub fn diag_scan_ecu_modules(
    include_29bit: Option<bool>,
    state: State<'_, FlashEngineState>,
) -> Result<Vec<DiscoveredEcuModule>, String> {
    diag_scan_ecu_modules_inner(include_29bit.unwrap_or(false), &state)
}

#[tauri::command]
pub fn diag_uds_enter_session(
    tx_can_id: Option<u32>,
    session_subfunction: u8,
    state: State<'_, FlashEngineState>,
) -> Result<String, String> {
    diag_uds_enter_session_inner(tx_can_id, session_subfunction, &state)
}

#[tauri::command]
pub fn diag_uds_read_dids(
    tx_can_id: Option<u32>,
    dids: Option<Vec<u16>>,
    state: State<'_, FlashEngineState>,
) -> Result<Vec<UdsDidValue>, String> {
    diag_uds_read_dids_inner(tx_can_id, dids, &state)
}

#[tauri::command]
pub fn diag_uds_routine_control(
    tx_can_id: Option<u32>,
    subfunction: u8,
    routine_id: u16,
    option_record: Option<Vec<u8>>,
    state: State<'_, FlashEngineState>,
) -> Result<UdsRoutineOutcome, String> {
    diag_uds_routine_control_inner(tx_can_id, subfunction, routine_id, option_record, &state)
}

#[tauri::command]
pub fn diag_uds_io_control(
    tx_can_id: Option<u32>,
    did: u16,
    control_param: u8,
    control_state: Option<Vec<u8>>,
    state: State<'_, FlashEngineState>,
) -> Result<UdsIoControlOutcome, String> {
    diag_uds_io_control_inner(tx_can_id, did, control_param, control_state, &state)
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

    #[test]
    fn phase2_diag_wrappers_cover_dtc_lookup_live_pids_readiness_and_uds_services() {
        let info = diag_lookup_dtc_inner("P0300");
        assert!(info.summary.contains("Misfire"));

        let state = attach_scripted_elm(vec![
            // live pids: bitmap 01-20 (RPM only) + RPM 2500
            reply(vec![0x41, 0x00, 0x00, 0x10, 0x00, 0x00]),
            reply(vec![0x41, 0x0C, 0x27, 0x10]),
            // readiness 01 01 + mode 0A empty
            reply(vec![0x41, 0x01, 0x00, 0x07, 0x01, 0x00]),
            reply(vec![0x4A, 0x00]),
            // uds session 0x03 + did 0xF187 + routine 0xFF00 + io 0x1234
            reply(vec![0x50, 0x03, 0x00, 0x32]),
            reply(vec![0x62, 0xF1, 0x87, b'K', b'I', b'V', b'I']),
            reply(vec![0x71, 0x01, 0xFF, 0x00]),
            reply(vec![0x6F, 0x12, 0x34, 0x00]),
        ]);

        let live = diag_read_live_pids_inner(None, None, &state).unwrap();
        assert_eq!(live.readings.len(), 1);
        assert_eq!(live.readings[0].value, 2500.0);

        let readiness = diag_read_im_readiness_inner(None, &state).unwrap();
        assert!(!readiness.mil_on);
        assert_eq!(readiness.monitors.len(), 4);

        let sess = diag_uds_enter_session_inner(None, 0x03, &state).unwrap();
        assert!(sess.contains("0x03"));
        let dids = diag_uds_read_dids_inner(None, Some(vec![0xF187]), &state).unwrap();
        assert_eq!(dids[0].ascii.as_deref(), Some("KIVI"));
        let routine = diag_uds_routine_control_inner(None, 0x01, 0xFF00, None, &state).unwrap();
        assert!(routine.acknowledged);
        let io = diag_uds_io_control_inner(None, 0x1234, 0x00, None, &state).unwrap();
        assert!(io.acknowledged);
    }
}
