//! Motorcycle Diagnostic Protocol Support (Honda PGM-FI, Yamaha, Kawasaki KDS, Suzuki SDS)
//! and Modified DLC-to-OBD-II Cable Pinout Reference.

use crate::device::Device;
use crate::dtc_dictionary::{lookup_dtc_description, lookup_honda_blink_code};
use crate::honda_kline::HondaProtocolFraming;
use crate::j2534_types::{PassThruMsg, ProtocolId};
use crate::moto_ecu_db::{list_moto_ecu_profiles, MotoEcuProfile};
use crate::quick_diag::{
    clear_dtcs, decode_j2012_dtc, ensure_connected, read_dtcs, DtcClearOutcome, DtcReadOutcome,
    DtcRecord, QuickDiagError,
};
use crate::uds_diagnostics::read_obd_live_snapshot;
use serde::{Deserialize, Serialize};

/// Supported motorcycle diagnostic protocol families.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotoProtocolFamily {
    HondaKline,
    YamahaKline,
    YamahaEuro5Obd,
    KawasakiKds,
    SuzukiSds,
}

impl MotoProtocolFamily {
    pub fn label(self) -> &'static str {
        match self {
            Self::HondaKline => "Honda PGM-FI K-Line (4-Pin DLC / 0x72)",
            Self::YamahaKline => "Yamaha K-Line (3/4-Pin / KWP 0x12)",
            Self::YamahaEuro5Obd => "Yamaha / Honda Euro 5 CAN OBD-II (6-Pin)",
            Self::KawasakiKds => "Kawasaki KDS K-Line (4-Pin / KWP 0x11)",
            Self::SuzukiSds => "Suzuki SDS K-Line (6-Pin / KWP 0x12)",
        }
    }
}

/// One labeled motorcycle live sensor reading.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MotoSensorMetric {
    pub key: String,
    pub label: String,
    pub value: f32,
    pub display: String,
    pub unit: String,
}

/// Unified motorcycle live telemetry snapshot across Honda, Yamaha, KDS, SDS, and Euro 5.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MotoLiveSnapshot {
    pub family: MotoProtocolFamily,
    pub protocol: String,
    pub ecm_id: Option<String>,
    pub matched_profile: Option<MotoEcuProfile>,
    pub sensors: Vec<MotoSensorMetric>,
    pub raw_hex: String,
    pub notes: Vec<String>,
}

/// Honda PGM-FI ECM ID probe result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HondaEcmIdentity {
    pub ecm_id_hex: String,
    pub matched_profile: Option<MotoEcuProfile>,
    pub raw_hex: String,
    pub notes: Vec<String>,
}

/// Pinout reference entry for modified motorcycle-to-OBD2 diagnostic harnesses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MotoCablePinout {
    pub id: String,
    pub brand_models: String,
    pub connector_type: String,
    pub protocol: String,
    pub obd16_wiring: Vec<String>,
    pub recommended_adapter: String,
    pub notes: String,
}

fn to_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn kline_exchange(
    device: &mut dyn Device,
    proto: ProtocolId,
    frame: &[u8],
) -> Result<Vec<u8>, QuickDiagError> {
    device.send(&PassThruMsg::new(proto, frame.to_vec()))?;
    let reply = device.receive()?;
    Ok(reply.data)
}

fn ensure_kline_session(
    device: &mut dyn Device,
    no_checksum: bool,
    low_ms: u64,
    high_ms: u64,
) -> Result<ProtocolId, QuickDiagError> {
    if !device.is_open() {
        return Err(crate::device::DeviceError::NotOpen.into());
    }
    device.set_kline_no_checksum(no_checksum);
    let current = device.connected_protocol();
    let proto = match current {
        Some(p @ (ProtocolId::Iso14230 | ProtocolId::Iso9141)) if device.is_connected() => p,
        _ => {
            if device.is_connected() {
                let _ = device.disconnect();
            }
            device.connect(ProtocolId::Iso14230, 10_400)?;
            ProtocolId::Iso14230
        }
    };
    match device.kline_wakeup_pulse(low_ms, high_ms) {
        Ok(()) | Err(crate::device::DeviceError::Unsupported(_)) => {}
        Err(e) => return Err(e.into()),
    }
    Ok(proto)
}

/// Executes Honda PGM-FI K-Line 70ms/120ms wakeup pulse + `FE 04 72 8C` + `72 05 00 F0 99` handshake.
pub fn honda_kline_init_session(device: &mut dyn Device) -> Result<Vec<String>, QuickDiagError> {
    let proto = ensure_kline_session(device, true, 70, 120)?;
    let mut notes = Vec::new();
    let wakeup = vec![0xFE, 0x04, 0x72, 0x8C];
    if let Err(err) = device.send(&PassThruMsg::new(proto, wakeup)) {
        notes.push(format!("Honda wakeup transmit notice: {err}"));
    }
    let init_cmd = vec![0x72, 0x05, 0x00, 0xF0, 0x99];
    match kline_exchange(device, proto, &init_cmd) {
        Ok(resp) => notes.push(format!("Honda K-Line init reply: {}", to_hex(&resp))),
        Err(err) => notes.push(format!("Honda K-Line init tolerance warning: {err}")),
    }
    Ok(notes)
}

fn match_honda_profile(ecm_hex: &str) -> Option<MotoEcuProfile> {
    let upper = ecm_hex.to_ascii_uppercase();
    list_moto_ecu_profiles().into_iter().find(|p| {
        p.ecm_id_hex
            .as_deref()
            .is_some_and(|id| id.eq_ignore_ascii_case(&upper))
    })
}

/// Reads the 5-byte Honda PGM-FI ECM Identifier via table `0x00` (`72 05 71 00 18`).
pub fn honda_kline_read_ecm_id(device: &mut dyn Device) -> Result<HondaEcmIdentity, QuickDiagError> {
    let notes = honda_kline_init_session(device)?;
    let proto = device.connected_protocol().unwrap_or(ProtocolId::Iso14230);
    let req = HondaProtocolFraming::build_packet(0x72, 0x71, 0x00, &[]);
    let resp = kline_exchange(device, proto, &req)?;

    let body = if resp.len() >= 10 && HondaProtocolFraming::verify_packet(&resp) {
        &resp[4..resp.len() - 1]
    } else if resp.len() >= 9 {
        &resp[4..9]
    } else {
        &resp[..]
    };
    let ecm_id_hex: String = body.iter().take(5).map(|b| format!("{b:02X}")).collect();
    let matched_profile = match_honda_profile(&ecm_id_hex);

    Ok(HondaEcmIdentity {
        ecm_id_hex,
        matched_profile,
        raw_hex: to_hex(&resp),
        notes,
    })
}

fn metric(key: &str, label: &str, value: f32, display: String, unit: &str) -> MotoSensorMetric {
    MotoSensorMetric {
        key: key.to_string(),
        label: label.to_string(),
        value,
        display,
        unit: unit.to_string(),
    }
}

fn parse_honda_table11(data: &[u8]) -> Vec<MotoSensorMetric> {
    // Strip optional [0x02, len, 0x71, table] header + trailing checksum when full packet provided
    let payload = if data.len() >= 16 && HondaProtocolFraming::verify_packet(data) {
        &data[4..data.len() - 1]
    } else if data.len() > 4 && data[0] == 0x02 && data[2] == 0x71 {
        &data[4..]
    } else {
        data
    };
    let mut out = Vec::new();
    if payload.len() >= 2 {
        let rpm = f32::from(u16::from_be_bytes([payload[0], payload[1]]));
        out.push(metric("rpm", "Engine Speed", rpm, format!("{rpm:.0} RPM"), "RPM"));
    }
    if payload.len() >= 4 {
        let tps_v = f32::from(payload[2]) * 5.0 / 255.0;
        let tps_pct = f32::from(payload[3]) * 0.5;
        out.push(metric("tps_volt", "TPS Voltage", tps_v, format!("{tps_v:.2} V"), "V"));
        out.push(metric("tps_pct", "Throttle Opening", tps_pct, format!("{tps_pct:.1} %"), "%"));
    }
    if payload.len() >= 6 {
        let ect_v = f32::from(payload[4]) * 5.0 / 255.0;
        let ect_c = f32::from(payload[5]) - 40.0;
        out.push(metric("ect_volt", "ECT Voltage", ect_v, format!("{ect_v:.2} V"), "V"));
        out.push(metric("ect_c", "Coolant / Oil Temp", ect_c, format!("{ect_c:.0} C"), "C"));
    }
    if payload.len() >= 8 {
        let iat_v = f32::from(payload[6]) * 5.0 / 255.0;
        let iat_c = f32::from(payload[7]) - 40.0;
        out.push(metric("iat_volt", "IAT Voltage", iat_v, format!("{iat_v:.2} V"), "V"));
        out.push(metric("iat_c", "Intake Air Temp", iat_c, format!("{iat_c:.0} C"), "C"));
    }
    if payload.len() >= 10 {
        let map_v = f32::from(payload[8]) * 5.0 / 255.0;
        let map_kpa = f32::from(payload[9]);
        out.push(metric("map_volt", "MAP Voltage", map_v, format!("{map_v:.2} V"), "V"));
        out.push(metric("map_kpa", "Manifold Pressure", map_kpa, format!("{map_kpa:.0} kPa"), "kPa"));
    }
    if payload.len() >= 13 {
        let batt_v = f32::from(payload[12]) / 10.0;
        out.push(metric("battery_v", "Battery Voltage", batt_v, format!("{batt_v:.1} V"), "V"));
    }
    if payload.len() >= 14 {
        let spd = f32::from(payload[13]);
        out.push(metric("speed_kph", "Vehicle Speed", spd, format!("{spd:.0} km/h"), "km/h"));
    }
    if payload.len() >= 16 {
        let inj_ms = f32::from(u16::from_be_bytes([payload[14], payload[15]])) * 0.1;
        out.push(metric("inj_ms", "Injector Pulse", inj_ms, format!("{inj_ms:.1} ms"), "ms"));
    }
    if payload.len() >= 17 {
        let ign_deg = f32::from(payload[16]) * 0.5 - 64.0;
        out.push(metric("ign_deg", "Ignition Advance", ign_deg, format!("{ign_deg:+.1} deg"), "deg"));
    }
    if payload.len() >= 19 {
        let o2_v = f32::from(payload[18]) * 5.0 / 255.0;
        out.push(metric("o2_v", "O2 Sensor Voltage", o2_v, format!("{o2_v:.2} V"), "V"));
    }
    out
}

fn kwp_fast_init(device: &mut dyn Device, family: MotoProtocolFamily) -> Result<(ProtocolId, Vec<String>), QuickDiagError> {
    let proto = ensure_kline_session(device, false, 25, 25)?;
    let mut notes = Vec::new();
    let target_addr = match family {
        MotoProtocolFamily::KawasakiKds => 0x11,
        _ => 0x12,
    };
    let start_comm = [0x81, target_addr, 0xF1, 0x81];
    match kline_exchange(device, proto, &start_comm) {
        Ok(r) => notes.push(format!("KWP StartCommunication ({target_addr:02X}): {}", to_hex(&r))),
        Err(e) => notes.push(format!("KWP StartCommunication warning: {e}")),
    }
    if matches!(family, MotoProtocolFamily::KawasakiKds | MotoProtocolFamily::SuzukiSds) {
        let diag_cmd = if family == MotoProtocolFamily::KawasakiKds {
            vec![0x10, 0x80]
        } else {
            vec![0x10, 0x01]
        };
        if let Ok(r) = kline_exchange(device, proto, &diag_cmd) {
            notes.push(format!("StartDiagnosticSession: {}", to_hex(&r)));
        }
    }
    Ok((proto, notes))
}

/// Reads live motorcycle engine/sensor data across Honda K-Line, Yamaha K-Line, KDS, SDS, or Euro 5 OBD-II.
pub fn moto_read_live_sensors(
    device: &mut dyn Device,
    family: MotoProtocolFamily,
) -> Result<MotoLiveSnapshot, QuickDiagError> {
    if family == MotoProtocolFamily::YamahaEuro5Obd {
        let proto = ensure_connected(device)?;
        let obd = read_obd_live_snapshot(device, 0x07E0, Some(&[0x0C, 0x05, 0x0B, 0x0F, 0x11, 0x0E, 0x42, 0x0D]))?;
        let sensors = obd
            .readings
            .into_iter()
            .map(|r| metric(&r.pid_hex, &r.name, r.value, r.display, &r.unit))
            .collect();
        return Ok(MotoLiveSnapshot {
            family,
            protocol: format!("{proto:?} (Euro 5 CAN OBD-II)"),
            ecm_id: None,
            matched_profile: None,
            sensors,
            raw_hex: String::new(),
            notes: obd.notes,
        });
    }

    if family == MotoProtocolFamily::HondaKline {
        let mut notes = honda_kline_init_session(device)?;
        let proto = device.connected_protocol().unwrap_or(ProtocolId::Iso14230);
        let req_id = HondaProtocolFraming::build_packet(0x72, 0x71, 0x00, &[]);
        let mut ecm_id = None;
        let mut matched_profile = None;
        if let Ok(id_resp) = kline_exchange(device, proto, &req_id) {
            let body = if id_resp.len() >= 10 && HondaProtocolFraming::verify_packet(&id_resp) {
                &id_resp[4..id_resp.len() - 1]
            } else {
                &id_resp[id_resp.len().min(4)..]
            };
            let hex: String = body.iter().take(5).map(|b| format!("{b:02X}")).collect();
            if !hex.is_empty() {
                matched_profile = match_honda_profile(&hex);
                ecm_id = Some(hex);
            }
        }
        let req_t11 = HondaProtocolFraming::build_packet(0x72, 0x71, 0x11, &[]);
        let t11_resp = kline_exchange(device, proto, &req_t11)?;
        let sensors = parse_honda_table11(&t11_resp);
        if sensors.is_empty() {
            notes.push("Honda Table 0x11 returned short frame; showing raw hex payload.".to_string());
        }
        return Ok(MotoLiveSnapshot {
            family,
            protocol: "Honda PGM-FI K-Line 10400 bps".to_string(),
            ecm_id,
            matched_profile,
            sensors,
            raw_hex: to_hex(&t11_resp),
            notes,
        });
    }

    let (proto, mut notes) = kwp_fast_init(device, family)?;
    let mut sensors = Vec::new();
    let mut raw_chunks = Vec::new();
    match family {
        MotoProtocolFamily::KawasakiKds => {
            for (pid, label, unit) in [
                (0x04u8, "Throttle Position", "%"),
                (0x05u8, "Intake Air Pressure", "kPa"),
                (0x06u8, "Engine Coolant Temp", "C"),
                (0x07u8, "Intake Air Temp", "C"),
                (0x09u8, "Engine Speed", "RPM"),
                (0x0Au8, "Battery Voltage", "V"),
            ] {
                if let Ok(resp) = kline_exchange(device, proto, &[0x21, pid]) {
                    raw_chunks.push(to_hex(&resp));
                    if resp.len() >= 3 && resp[0] == 0x61 && resp[1] == pid {
                        let (val, disp) = match pid {
                            0x09 => {
                                let rpm = if resp.len() >= 4 {
                                    f32::from(u16::from_be_bytes([resp[2], resp[3]]))
                                } else {
                                    f32::from(resp[2]) * 100.0
                                };
                                (rpm, format!("{rpm:.0} RPM"))
                            }
                            0x06 | 0x07 => {
                                let c = (f32::from(resp[2]) - 48.0) / 1.6;
                                (c, format!("{c:.1} C"))
                            }
                            0x0A => {
                                let v = f32::from(resp[2]) / 12.75;
                                (v, format!("{v:.2} V"))
                            }
                            _ => {
                                let p = f32::from(resp[2]) * 100.0 / 255.0;
                                (p, format!("{p:.1} {unit}"))
                            }
                        };
                        sensors.push(metric(&format!("kds_{pid:02x}"), label, val, disp, unit));
                    }
                }
            }
        }
        MotoProtocolFamily::SuzukiSds => {
            let resp = kline_exchange(device, proto, &[0x21, 0x08])?;
            raw_chunks.push(to_hex(&resp));
            if resp.len() >= 20 && resp[0] == 0x61 && resp[1] == 0x08 {
                let rpm = f32::from(resp[17]) * 25500.0 / 255.0;
                let tps = (f32::from(resp[19]) - 55.0) * 100.0 / 170.0;
                sensors.push(metric("rpm", "Engine Speed", rpm, format!("{rpm:.0} RPM"), "RPM"));
                sensors.push(metric("tps", "Throttle Opening", tps, format!("{tps:.1} %"), "%"));
            } else {
                notes.push("Received compact Suzuki SDS frame; inspect raw hex.".to_string());
            }
        }
        MotoProtocolFamily::YamahaKline => {
            let resp = kline_exchange(device, proto, &[0x21, 0x01])?;
            raw_chunks.push(to_hex(&resp));
            if resp.len() >= 6 && resp[0] == 0x61 {
                let rpm = f32::from(u16::from_be_bytes([resp[2], resp[3]]));
                let ect = f32::from(resp[4]) - 40.0;
                sensors.push(metric("rpm", "Engine Speed", rpm, format!("{rpm:.0} RPM"), "RPM"));
                sensors.push(metric("ect_c", "Coolant Temp", ect, format!("{ect:.0} C"), "C"));
            }
        }
        _ => {}
    }

    Ok(MotoLiveSnapshot {
        family,
        protocol: family.label().to_string(),
        ecm_id: None,
        matched_profile: None,
        sensors,
        raw_hex: raw_chunks.join(" | "),
        notes,
    })
}

/// Reads motorcycle DTCs across Honda K-Line Table `0x73`/`0x74`, KWP `0x18`, or Euro 5 CAN OBD-II.
pub fn moto_read_dtcs(
    device: &mut dyn Device,
    family: MotoProtocolFamily,
) -> Result<DtcReadOutcome, QuickDiagError> {
    if family == MotoProtocolFamily::YamahaEuro5Obd {
        return read_dtcs(device);
    }

    if family == MotoProtocolFamily::HondaKline {
        let mut notes = honda_kline_init_session(device)?;
        let proto = device.connected_protocol().unwrap_or(ProtocolId::Iso14230);
        let mut dtcs = Vec::new();
        for (table, status_label, source) in [
            (0x73u8, "confirmed", "honda_table73"),
            (0x74u8, "stored_past", "honda_table74"),
        ] {
            let req = HondaProtocolFraming::build_packet(0x72, 0x71, table, &[]);
            match kline_exchange(device, proto, &req) {
                Ok(resp) => {
                    let body = if resp.len() >= 6 && HondaProtocolFraming::verify_packet(&resp) {
                        &resp[4..resp.len() - 1]
                    } else {
                        &resp[resp.len().min(4)..]
                    };
                    for pair in body.chunks_exact(2) {
                        if pair[0] == 0 && pair[1] == 0 {
                            continue;
                        }
                        let desc = lookup_honda_blink_code(pair[0], pair[1]);
                        dtcs.push(DtcRecord {
                            code: desc.code,
                            status_mask: if table == 0x73 { 0x08 } else { 0x04 },
                            status_label: status_label.to_string(),
                            source: source.to_string(),
                            description: Some(desc.summary),
                        });
                    }
                }
                Err(err) => notes.push(format!("Honda Table 0x{table:02X} skipped: {err}")),
            }
        }
        return Ok(DtcReadOutcome {
            dtcs,
            protocol_used: "Honda PGM-FI K-Line (Table 0x73/0x74)".to_string(),
            notes,
        });
    }

    let (proto, mut notes) = kwp_fast_init(device, family)?;
    let resp = kline_exchange(device, proto, &[0x18, 0x00, 0xFF, 0x00])?;
    let mut dtcs = Vec::new();
    if resp.first().copied() == Some(0x58) && resp.len() >= 5 {
        for chunk in resp[2..].chunks_exact(3) {
            if let Some(code) = decode_j2012_dtc(chunk[0], chunk[1]) {
                let desc = lookup_dtc_description(&code).summary;
                dtcs.push(DtcRecord {
                    code,
                    status_mask: chunk[2],
                    status_label: "kwp_stored".to_string(),
                    source: "kwp_0x18".to_string(),
                    description: Some(desc),
                });
            }
        }
    } else {
        notes.push(format!("KWP 0x18 reply: {}", to_hex(&resp)));
    }

    Ok(DtcReadOutcome {
        dtcs,
        protocol_used: family.label().to_string(),
        notes,
    })
}

/// Clears motorcycle DTCs across Honda K-Line (`0x60`), KWP (`0x14`), or Euro 5 OBD-II.
pub fn moto_clear_dtcs(
    device: &mut dyn Device,
    family: MotoProtocolFamily,
) -> Result<DtcClearOutcome, QuickDiagError> {
    if family == MotoProtocolFamily::YamahaEuro5Obd {
        return clear_dtcs(device);
    }

    if family == MotoProtocolFamily::HondaKline {
        let mut notes = honda_kline_init_session(device)?;
        let proto = device.connected_protocol().unwrap_or(ProtocolId::Iso14230);
        let cmd1 = vec![0x72, 0x05, 0x60, 0x01, 0x28];
        let resp = kline_exchange(device, proto, &cmd1)?;
        let cleared = resp.get(2).copied() == Some(0x60) || resp.first().copied() == Some(0x02);
        notes.push(format!("Honda Clear DTC reply: {}", to_hex(&resp)));
        notes.push(
            "For manual TPS/ECM memory reset on older 4-pin Honda bikes, follow official SCS jumper procedure if ECU rejects live clear."
                .to_string(),
        );
        return Ok(DtcClearOutcome {
            cleared,
            protocol_used: "Honda PGM-FI K-Line (0x60 Clear)".to_string(),
            notes,
        });
    }

    let (proto, mut notes) = kwp_fast_init(device, family)?;
    let resp = kline_exchange(device, proto, &[0x14, 0xFF, 0x00])?;
    let cleared = resp.first().copied() == Some(0x54);
    notes.push(format!("KWP 0x14 Clear reply: {}", to_hex(&resp)));
    Ok(DtcClearOutcome {
        cleared,
        protocol_used: family.label().to_string(),
        notes,
    })
}

/// Complete modified DLC-to-OBD-II cable pinout & adapter recommendation guide for Southeast Asian motorcycles.
pub fn list_moto_cable_pinouts() -> Vec<MotoCablePinout> {
    vec![
        MotoCablePinout {
            id: "honda_4pin_red".into(),
            brand_models: "Honda PGM-FI Pre-Euro5 (Beat/Vario/PCX/CB150R/CBR150R/CBR250RR MC51)".into(),
            connector_type: "4-Pin Red DLC (Sumitomo HM 090)".into(),
            protocol: "Honda Proprietary K-Line 10400 bps (70ms LOW / 120ms HIGH wakeup)".into(),
            obd16_wiring: vec![
                "DLC +12V (Black/White or Red/White) -> OBD Pin 16 (+12V)".into(),
                "DLC GND (Green) -> OBD Pin 4 & Pin 5 (Signal/Chassis GND)".into(),
                "DLC K-Line (Light Green/Black or Orange/White) -> OBD Pin 7 (K-Line)".into(),
                "DLC SCS (Brown) -> Leave OPEN for scanner (short to GND only for blink/TP reset)".into(),
            ],
            recommended_adapter: "Raw K-Line FTDI KKL 409.1 or Tactrix OpenPort 2.0 (ELM327 often lacks 70/120ms wakeup)".into(),
            notes: "Cheap Shopee 4-pin-to-OBD16 adapter cables work best with Raw K-Line USB or OpenPort 2.0. Check multimeter continuity to OBD Pin 7!".into(),
        },
        MotoCablePinout {
            id: "honda_6pin_euro5".into(),
            brand_models: "Honda Euro 5 (ADV 160, PCX 160, Stylo 160, Vario 160, CBR250RR SP)".into(),
            connector_type: "6-Pin Red Euro 5 DLC".into(),
            protocol: "ISO 15765-4 CAN 11-bit 500 kbps (+ optional legacy K-Line on older hybrids)".into(),
            obd16_wiring: vec![
                "DLC +12V -> OBD Pin 16 (+12V)".into(),
                "DLC GND -> OBD Pin 4 & Pin 5 (GND)".into(),
                "DLC CAN-H -> OBD Pin 6 (CAN High)".into(),
                "DLC CAN-L -> OBD Pin 14 (CAN Low)".into(),
            ],
            recommended_adapter: "ELM327 v1.5 (USB/BT/Wi-Fi), OpenPort 2.0, or J2534".into(),
            notes: "Standard OBD Mode 01/03/04/09 works cleanly even on cheap ELM327 v1.5 CAN adapters.".into(),
        },
        MotoCablePinout {
            id: "yamaha_3pin_4pin".into(),
            brand_models: "Yamaha Pre-Euro5 (NMAX Old, Aerox Old, XMAX, R15/Vixion, MT-15, R25)".into(),
            connector_type: "3-Pin / 4-Pin Triangle/Square Yamaha DLC".into(),
            protocol: "ISO 14230-4 KWP2000 10400 bps (12V K-Line, ECU 0x12)".into(),
            obd16_wiring: vec![
                "DLC +12V Switched -> OBD Pin 16 (+12V)".into(),
                "DLC GND -> OBD Pin 4 & Pin 5 (GND)".into(),
                "DLC K-Line (White/Red or Light Blue) -> OBD Pin 7 (K-Line)".into(),
            ],
            recommended_adapter: "Raw K-Line KKL FTDI, OpenPort 2.0, or genuine ELM327 v1.5 (avoid v2.1 clones lacking ATTP5)".into(),
            notes: "Note: some 1-wire 3-pin Yamaha bikes carry only K-Line (no +12V); externally feed Pin 16 from battery +12V.".into(),
        },
        MotoCablePinout {
            id: "yamaha_6pin_euro5".into(),
            brand_models: "Yamaha Euro 5 (R15M, MT-07/09, R7, Connected/Euro5 NMAX/XMAX)".into(),
            connector_type: "6-Pin Red ISO 19689 Euro 5 DLC".into(),
            protocol: "ISO 15765-4 CAN 11-bit 500 kbps".into(),
            obd16_wiring: vec![
                "Pin 1 +12V -> OBD Pin 16".into(),
                "Pin 2 GND -> OBD Pin 4 & 5".into(),
                "Pin 5 CAN-L -> OBD Pin 14".into(),
                "Pin 6 CAN-H -> OBD Pin 6".into(),
            ],
            recommended_adapter: "ELM327 USB/BT/Wi-Fi, OpenPort 2.0, J2534".into(),
            notes: "Fully compatible with ELM327 CAN profile, UDS 0x19 DTC scan, and live Mode 01 telemetry.".into(),
        },
        MotoCablePinout {
            id: "kawasaki_4pin_6pin".into(),
            brand_models: "Kawasaki KDS (Ninja 250FI/ZX-25R/Z900/KLX230)".into(),
            connector_type: "4-Pin Black KDS / 6-Pin Euro 5".into(),
            protocol: "KWP2000 KDS (ECU 0x11) or Euro 5 CAN 500 kbps".into(),
            obd16_wiring: vec![
                "DLC +12V -> OBD Pin 16".into(),
                "DLC GND -> OBD Pin 4 & 5".into(),
                "KDS Line -> OBD Pin 7 (or CAN-H/L on Pin 6/14 for 6-pin Euro 5)".into(),
            ],
            recommended_adapter: "Raw K-Line KKL / OpenPort 2.0 / genuine ELM327 v1.5 (Kawasaki KDS profile)".into(),
            notes: "Some Shopee KDS cables leave Pin 4 unconnected while Pin 5 is wired: jumper OBD Pin 4 & Pin 5 on your adapter cable if adapter does not power on.".into(),
        },
        MotoCablePinout {
            id: "suzuki_6pin_sds".into(),
            brand_models: "Suzuki SDS (GSX-R150/Satria FU FI/V-Strom/GSX-R600/1000)".into(),
            connector_type: "6-Pin White SDS / Euro 5 Red".into(),
            protocol: "Suzuki SDS KWP2000 (ECU 0x12) or Euro 5 CAN".into(),
            obd16_wiring: vec![
                "DLC +12V -> OBD Pin 16".into(),
                "DLC GND -> OBD Pin 4 & 5".into(),
                "SDS Mode Select (Dealer Switch) -> Short to GND for dash dealer mode, leave open for K-Line scan".into(),
                "SDS K-Line -> OBD Pin 7".into(),
            ],
            recommended_adapter: "Raw K-Line FTDI / OpenPort 2.0 / genuine ELM327 v1.5 (Suzuki SDS profile)".into(),
            notes: "Verify +12V ignition voltage arrives at OBD Pin 16 before scanning.".into(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::{DeviceKind, MockDevice, ScriptedReply, TransportKind};

    fn mock_kline_device() -> MockDevice {
        let mut dev = MockDevice::new(DeviceKind::KLineSerial, "KKL Mock", TransportKind::Serial);
        dev.open().unwrap();
        dev.connect(ProtocolId::Iso14230, 10_400).unwrap();
        dev
    }

    fn kline_reply(data: Vec<u8>) -> ScriptedReply {
        ScriptedReply::Message(PassThruMsg::new(ProtocolId::Iso14230, data))
    }

    #[test]
    fn honda_kline_ecm_id_table11_dtcs_and_clear_roundtrip() {
        let mut dev = mock_kline_device();
        // Reply 1: init [02 04 00 FA]
        // Reply 2: ECM ID [02 0A 71 00 01 00 2B 01 01 cs]
        let mut ecm_reply = vec![0x02, 0x0A, 0x71, 0x00, 0x01, 0x00, 0x2B, 0x01, 0x01];
        ecm_reply.push(HondaProtocolFraming::checksum_8bit(&ecm_reply));
        // Reply 3: init again in moto_read_live_sensors
        // Reply 4: ECM ID again
        // Reply 5: Table 0x11: RPM=2000(07D0), TPS=2.5V(80)/50%(64), ECT=1.0V(33)/85C(7D), IAT=2.0V(66)/30C(46), MAP=100(64), +2 bytes, batt=12.6V(7E), spd=0, inj=3.0ms(001E), ign=+12deg(98)
        let mut t11 = vec![
            0x02, 23, 0x71, 0x11, 0x07, 0xD0, 0x80, 0x64, 0x33, 0x7D, 0x66, 0x46, 0x66, 0x64,
            0x00, 0x00, 126, 0, 0x00, 0x1E, 152, 0,
        ];
        t11.push(HondaProtocolFraming::checksum_8bit(&t11));

        dev.set_script(vec![
            kline_reply(vec![0x02, 0x04, 0x00, 0xFA]),
            kline_reply(ecm_reply),
            kline_reply(vec![0x02, 0x04, 0x00, 0xFA]),
            kline_reply(vec![0x02, 0x0A, 0x71, 0x00, 0x01, 0x00, 0x2B, 0x01, 0x01, 0x57]),
            kline_reply(t11),
        ]);

        let id = honda_kline_read_ecm_id(&mut dev).unwrap();
        assert_eq!(id.ecm_id_hex, "01002B0101");
        assert_eq!(id.matched_profile.unwrap().id, "honda_cbr1000rr_06_07");

        let live = moto_read_live_sensors(&mut dev, MotoProtocolFamily::HondaKline).unwrap();
        assert_eq!(live.ecm_id.as_deref(), Some("01002B0101"));
        assert!(live.sensors.iter().any(|s| s.key == "rpm" && s.value == 2000.0));
        assert!(live.sensors.iter().any(|s| s.key == "ect_c" && s.value == 85.0));

        // DTCs + clear
        let mut t73 = vec![0x02, 7, 0x71, 0x73, 8, 1];
        t73.push(HondaProtocolFraming::checksum_8bit(&t73));
        let mut t74 = vec![0x02, 7, 0x71, 0x74, 0, 0];
        t74.push(HondaProtocolFraming::checksum_8bit(&t74));
        dev.set_script(vec![
            kline_reply(vec![0x02, 0x04, 0x00, 0xFA]),
            kline_reply(t73),
            kline_reply(t74),
            kline_reply(vec![0x02, 0x04, 0x00, 0xFA]),
            kline_reply(vec![0x02, 0x05, 0x60, 0x00, 0x99]),
        ]);

        let dtcs = moto_read_dtcs(&mut dev, MotoProtocolFamily::HondaKline).unwrap();
        assert_eq!(dtcs.dtcs.len(), 1);
        assert_eq!(dtcs.dtcs[0].code, "Honda 8-1");
        assert!(dtcs.dtcs[0].description.as_deref().unwrap().contains("TP"));

        let clr = moto_clear_dtcs(&mut dev, MotoProtocolFamily::HondaKline).unwrap();
        assert!(clr.cleared);
    }

    #[test]
    fn kds_sds_yamaha_and_pinouts_work() {
        let mut dev = mock_kline_device();
        dev.set_script(vec![
            kline_reply(vec![0xC1, 0xEA, 0x8F]),
            kline_reply(vec![0x50, 0x80]),
            kline_reply(vec![0x61, 0x04, 128]),
            kline_reply(vec![0x61, 0x05, 100]),
            kline_reply(vec![0x61, 0x06, 176]),
            kline_reply(vec![0x61, 0x07, 96]),
            kline_reply(vec![0x61, 0x09, 0x0B, 0xB8]),
            kline_reply(vec![0x61, 0x0A, 160]),
        ]);
        let kds = moto_read_live_sensors(&mut dev, MotoProtocolFamily::KawasakiKds).unwrap();
        assert_eq!(kds.sensors.len(), 6);
        assert!(kds.sensors.iter().any(|s| s.key == "kds_09" && s.value == 3000.0));

        let pinouts = list_moto_cable_pinouts();
        assert!(pinouts.len() >= 6);
        assert!(pinouts.iter().any(|p| p.id == "honda_4pin_red"));
        assert!(pinouts.iter().any(|p| p.id == "yamaha_6pin_euro5"));
    }
}
