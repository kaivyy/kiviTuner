//! Tauri commands for quick OBD-II / UDS diagnostics over any attached transport
//! (including ELM327 USB/serial cables, OpenPort 2.0, J2534, DoIP, and SocketCAN).

use crate::flash_commands::FlashEngineState;
use kivi_flashing::quick_diag::{
    clear_dtcs, ecu_reset, read_dtcs, read_ecu_identity, DtcClearOutcome, DtcReadOutcome,
    EcuIdentitySummary, EcuResetMode, EcuResetOutcome,
};
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
}
