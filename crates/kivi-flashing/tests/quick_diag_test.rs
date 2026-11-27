//! Integration tests for quick ELM327 / UDS / OBD-II diagnostics.

use kivi_flashing::device::{Device, DeviceError, DeviceKind, MockDevice, ScriptedReply, TransportKind};
use kivi_flashing::j2534_types::{PassThruMsg, ProtocolId};
use kivi_flashing::quick_diag::{
    clear_dtcs, ecu_reset, read_dtcs, read_ecu_identity, EcuResetMode, QuickDiagError,
};

fn connected_bench() -> MockDevice {
    let mut d = MockDevice::new(DeviceKind::Serial, "ELM327 Test Bench", TransportKind::Serial);
    d.open().unwrap();
    d.connect(ProtocolId::Iso15765, 500_000).unwrap();
    d.set_voltage_millivolts(12_740);
    d
}

fn reply(data: Vec<u8>) -> ScriptedReply {
    ScriptedReply::Message(PassThruMsg::new(ProtocolId::Iso15765, data))
}

#[test]
fn reads_battery_vin_cal_id_and_cvn_with_obd_fallback_when_uds_vin_declined() {
    let mut dev = connected_bench();
    // Query sequence in read_ecu_identity:
    // 1) [0x22, 0xF1, 0x90] -> NRC 0x11 (serviceNotSupported)
    // 2) [0x09, 0x02] -> OBD VIN (17 chars)
    // 3) [0x09, 0x04] -> OBD Cal-ID
    // 4) [0x09, 0x06] -> OBD CVN
    dev.set_script(vec![
        reply(vec![0x7F, 0x22, 0x11]),
        reply({
            let mut v = vec![0x49, 0x02, 0x01];
            v.extend_from_slice(b"JH2PC4000MK123456");
            v
        }),
        reply({
            let mut v = vec![0x49, 0x04, 0x01];
            v.extend_from_slice(b"37805-KZR-6010");
            v
        }),
        reply(vec![0x49, 0x06, 0x01, 0x1A, 0x2B, 0x3C, 0x4D]),
    ]);

    let info = read_ecu_identity(&mut dev).unwrap();
    assert_eq!(info.battery_mv, Some(12_740));
    assert_eq!(info.vin.as_deref(), Some("JH2PC4000MK123456"));
    assert_eq!(info.cal_id.as_deref(), Some("37805-KZR-6010"));
    assert_eq!(info.cvn.as_deref(), Some("1A2B3C4D"));
    assert_eq!(info.protocol, "Iso15765");
}

#[test]
fn reads_dtcs_from_uds_0x19_when_supported() {
    let mut dev = connected_bench();
    // UDS 0x59 0x02 <availabilityMask> [3-byte DTC + 1-byte status]...
    // 0x01, 0x71, 0x00, 0x08 -> P0171 confirmed (0x08)
    // 0xC1, 0x00, 0x00, 0x04 -> U0100 pending (0x04)
    dev.set_script(vec![reply(vec![
        0x59, 0x02, 0xFF, 0x01, 0x71, 0x00, 0x08, 0xC1, 0x00, 0x00, 0x04,
    ])]);

    let outcome = read_dtcs(&mut dev).unwrap();
    assert_eq!(outcome.protocol_used, "UDS 0x19 (ReadDTCInformation)");
    assert_eq!(outcome.dtcs.len(), 2);
    assert_eq!(outcome.dtcs[0].code, "P0171");
    assert_eq!(outcome.dtcs[0].status_label, "confirmed");
    assert_eq!(outcome.dtcs[1].code, "U0100");
    assert_eq!(outcome.dtcs[1].status_label, "pending");
}

#[test]
fn reads_dtcs_via_obd_mode03_and_mode07_fallback_and_ignores_zero_padding() {
    let mut dev = connected_bench();
    dev.set_script(vec![
        // UDS 0x19 rejected -> NRC 0x11
        reply(vec![0x7F, 0x19, 0x11]),
        // Mode 03 CAN reply with count=2, P0300 & B0120
        reply(vec![0x43, 0x02, 0x03, 0x00, 0x81, 0x20]),
        // Mode 07 K-line padded reply: P0171 + 0x0000 padding + P0300 duplicate
        reply(vec![0x47, 0x01, 0x71, 0x00, 0x00, 0x03, 0x00]),
    ]);

    let outcome = read_dtcs(&mut dev).unwrap();
    assert_eq!(outcome.protocol_used, "OBD-II Mode 03/07");
    assert_eq!(outcome.dtcs.len(), 3);
    assert_eq!(outcome.dtcs[0].code, "P0300");
    assert_eq!(outcome.dtcs[0].source, "obd_mode03");
    assert_eq!(outcome.dtcs[1].code, "B0120");
    assert_eq!(outcome.dtcs[2].code, "P0171");
    assert_eq!(outcome.dtcs[2].source, "obd_mode07");
    assert_eq!(outcome.dtcs[2].status_label, "pending");
}

#[test]
fn clears_dtcs_via_uds_0x14_or_obd_mode04_fallback() {
    let mut dev = connected_bench();
    dev.set_script(vec![reply(vec![0x54])]);
    let uds_res = clear_dtcs(&mut dev).unwrap();
    assert!(uds_res.cleared);
    assert_eq!(uds_res.protocol_used, "UDS 0x14 (ClearDiagnosticInformation)");

    // Fallback when 0x14 returns NRC 0x11
    dev.set_script(vec![reply(vec![0x7F, 0x14, 0x11]), reply(vec![0x44])]);
    let obd_res = clear_dtcs(&mut dev).unwrap();
    assert!(obd_res.cleared);
    assert_eq!(obd_res.protocol_used, "OBD-II Mode 04");
}

#[test]
fn ecu_reset_supports_soft_keyoffon_hard_and_propagates_negative_response() {
    let mut dev = connected_bench();
    dev.set_script(vec![
        reply(vec![0x51, 0x03]),
        reply(vec![0x51, 0x02]),
        reply(vec![0x51, 0x01, 0x05]),
        reply(vec![0x7F, 0x11, 0x22]),
    ]);

    let soft = ecu_reset(&mut dev, EcuResetMode::Soft).unwrap();
    assert!(soft.acknowledged);
    assert_eq!(soft.mode, EcuResetMode::Soft);
    assert_eq!(soft.power_down_time_secs, None);

    let key = ecu_reset(&mut dev, EcuResetMode::KeyOffOn).unwrap();
    assert_eq!(key.mode, EcuResetMode::KeyOffOn);

    let hard = ecu_reset(&mut dev, EcuResetMode::Hard).unwrap();
    assert_eq!(hard.mode, EcuResetMode::Hard);
    assert_eq!(hard.power_down_time_secs, Some(5));

    let err = ecu_reset(&mut dev, EcuResetMode::Hard).unwrap_err();
    assert_eq!(
        err,
        QuickDiagError::NegativeResponse {
            service: 0x11,
            nrc: 0x22
        }
    );

    // Ensure transmitted frames used sub-functions 0x03, 0x02, 0x01
    let sent = dev.sent_messages();
    assert_eq!(sent[0].data, vec![0x11, 0x03]);
    assert_eq!(sent[1].data, vec![0x11, 0x02]);
    assert_eq!(sent[2].data, vec![0x11, 0x01]);
}

#[test]
fn disconnected_device_fails_loudly() {
    let mut dev = MockDevice::new(DeviceKind::Serial, "Disconnected", TransportKind::Serial);
    let err = read_dtcs(&mut dev).unwrap_err();
    assert_eq!(err, QuickDiagError::Device(DeviceError::NotOpen));
}
