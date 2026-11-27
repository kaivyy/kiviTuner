//! Extended OBD-II Mode 01/02/06/0A & ISO 14229 UDS Diagnostic Tools.
//!
//! Works uniformly over any attached [`Device`] (ELM327 USB/Bluetooth/Wi-Fi,
//! OpenPort 2.0, SAE J2534, SocketCAN, DoIP, or Raw K-Line).

use crate::bus_silencing::BusSilencingInterlock;
use crate::device::Device;
use crate::dtc_dictionary::lookup_dtc_description;
use crate::j2534_types::ProtocolId;
use crate::quick_diag::{
    check_negative_response, decode_j2012_dtc, derive_rx_can_id_for_diag, ensure_connected,
    exchange_diag_target, extract_ascii_payload, parse_obd_dtc_response, prepare_diag_target,
    protocol_label, DtcRecord, QuickDiagError,
};
use serde::{Deserialize, Serialize};

/// One decoded live OBD-II Mode 01 or Mode 02 parameter reading.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObdLivePidReading {
    pub pid: u8,
    pub pid_hex: String,
    pub name: String,
    pub category: String,
    pub value: f32,
    pub display: String,
    pub unit: String,
    pub raw_hex: String,
}

/// Snapshot of supported Mode 01 PIDs and decoded live telemetry readings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObdLiveSnapshot {
    pub protocol: String,
    pub tx_can_id: u32,
    pub supported_pids: Vec<u8>,
    pub readings: Vec<ObdLivePidReading>,
    pub notes: Vec<String>,
}

/// Individual SAE J1979 Mode 01 PID `0x01` readiness monitor status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImReadinessMonitor {
    pub name: String,
    pub supported: bool,
    pub ready: bool,
}

/// Inspection / Maintenance (I/M) Readiness + Permanent DTC (`Mode 0A`) summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImReadinessSummary {
    pub mil_on: bool,
    pub dtc_count: u8,
    pub compression_ignition: bool,
    pub monitors: Vec<ImReadinessMonitor>,
    pub permanent_dtcs: Vec<DtcRecord>,
    pub notes: Vec<String>,
}

/// Decoded Mode 02 Freeze Frame snapshot captured at fault storage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FreezeFrameSummary {
    pub freeze_dtc: Option<String>,
    pub freeze_dtc_description: Option<String>,
    pub readings: Vec<ObdLivePidReading>,
    pub notes: Vec<String>,
}

/// Decoded Mode 06 on-board test monitor row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mode06TestResult {
    pub mid: u8,
    pub tid: u8,
    pub uasid: u8,
    pub value: u16,
    pub min: u16,
    pub max: u16,
    pub passed: bool,
    pub label: String,
}

/// One responder ECU discovered during a CAN / multi-module bus scan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveredEcuModule {
    pub tx_can_id: u32,
    pub rx_can_id: u32,
    pub tx_hex: String,
    pub rx_hex: String,
    pub label: String,
    pub is_29bit: bool,
    pub vin: Option<String>,
    pub part_number: Option<String>,
    pub sw_version: Option<String>,
    pub active_session: Option<u8>,
    pub dtc_count: Option<usize>,
}

/// Read UDS DID (`0x22`) data item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UdsDidValue {
    pub did: u16,
    pub did_hex: String,
    pub name: String,
    pub raw_hex: String,
    pub ascii: Option<String>,
}

/// Outcome of UDS RoutineControl (`0x31`) execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UdsRoutineOutcome {
    pub routine_id: u16,
    pub subfunction: u8,
    pub acknowledged: bool,
    pub response_hex: String,
    pub notes: Vec<String>,
}

/// Outcome of UDS InputOutputControlByIdentifier (`0x2F`) actuator test.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UdsIoControlOutcome {
    pub did: u16,
    pub control_param: u8,
    pub acknowledged: bool,
    pub response_hex: String,
    pub notes: Vec<String>,
}

struct PidSpec {
    pid: u8,
    name: &'static str,
    category: &'static str,
    unit: &'static str,
    min_len: usize,
}

const MODE01_SPECS: &[PidSpec] = &[
    PidSpec { pid: 0x04, name: "Calculated Engine Load", category: "Engine & Load", unit: "%", min_len: 1 },
    PidSpec { pid: 0x05, name: "Engine Coolant Temp", category: "Temperatures", unit: "C", min_len: 1 },
    PidSpec { pid: 0x06, name: "Short Term Fuel Trim B1", category: "Fuel & Ignition", unit: "%", min_len: 1 },
    PidSpec { pid: 0x07, name: "Long Term Fuel Trim B1", category: "Fuel & Ignition", unit: "%", min_len: 1 },
    PidSpec { pid: 0x08, name: "Short Term Fuel Trim B2", category: "Fuel & Ignition", unit: "%", min_len: 1 },
    PidSpec { pid: 0x09, name: "Long Term Fuel Trim B2", category: "Fuel & Ignition", unit: "%", min_len: 1 },
    PidSpec { pid: 0x0A, name: "Fuel Rail Pressure (Gauge)", category: "Fuel & Ignition", unit: "kPa", min_len: 1 },
    PidSpec { pid: 0x0B, name: "Intake Manifold Pressure (MAP)", category: "Airflow & Throttle", unit: "kPa", min_len: 1 },
    PidSpec { pid: 0x0C, name: "Engine Speed (RPM)", category: "Engine & Load", unit: "RPM", min_len: 2 },
    PidSpec { pid: 0x0D, name: "Vehicle Speed", category: "Engine & Load", unit: "km/h", min_len: 1 },
    PidSpec { pid: 0x0E, name: "Ignition Timing Advance", category: "Fuel & Ignition", unit: "deg", min_len: 1 },
    PidSpec { pid: 0x0F, name: "Intake Air Temp (IAT)", category: "Temperatures", unit: "C", min_len: 1 },
    PidSpec { pid: 0x10, name: "Mass Air Flow (MAF)", category: "Airflow & Throttle", unit: "g/s", min_len: 2 },
    PidSpec { pid: 0x11, name: "Throttle Position (TPS)", category: "Airflow & Throttle", unit: "%", min_len: 1 },
    PidSpec { pid: 0x14, name: "O2 Bank 1 Sensor 1", category: "O2 & Lambda", unit: "V", min_len: 2 },
    PidSpec { pid: 0x15, name: "O2 Bank 1 Sensor 2", category: "O2 & Lambda", unit: "V", min_len: 2 },
    PidSpec { pid: 0x16, name: "O2 Bank 2 Sensor 1", category: "O2 & Lambda", unit: "V", min_len: 2 },
    PidSpec { pid: 0x17, name: "O2 Bank 2 Sensor 2", category: "O2 & Lambda", unit: "V", min_len: 2 },
    PidSpec { pid: 0x1F, name: "Run Time Since Start", category: "Engine & Load", unit: "s", min_len: 2 },
    PidSpec { pid: 0x21, name: "Distance With MIL On", category: "Electrical & MIL", unit: "km", min_len: 2 },
    PidSpec { pid: 0x22, name: "Fuel Rail Pressure (Rel. Vac)", category: "Fuel & Ignition", unit: "kPa", min_len: 2 },
    PidSpec { pid: 0x23, name: "Fuel Rail Gauge Pressure", category: "Fuel & Ignition", unit: "kPa", min_len: 2 },
    PidSpec { pid: 0x24, name: "Wideband O2S1 Voltage & Lambda", category: "O2 & Lambda", unit: "lambda", min_len: 4 },
    PidSpec { pid: 0x2C, name: "Commanded EGR", category: "Emissions & EVAP", unit: "%", min_len: 1 },
    PidSpec { pid: 0x2D, name: "EGR Error", category: "Emissions & EVAP", unit: "%", min_len: 1 },
    PidSpec { pid: 0x2E, name: "Commanded EVAP Purge", category: "Emissions & EVAP", unit: "%", min_len: 1 },
    PidSpec { pid: 0x2F, name: "Fuel Tank Level", category: "Emissions & EVAP", unit: "%", min_len: 1 },
    PidSpec { pid: 0x30, name: "Warm-ups Since Clear", category: "Electrical & MIL", unit: "cycles", min_len: 1 },
    PidSpec { pid: 0x31, name: "Distance Since DTC Clear", category: "Electrical & MIL", unit: "km", min_len: 2 },
    PidSpec { pid: 0x33, name: "Barometric Pressure", category: "Airflow & Throttle", unit: "kPa", min_len: 1 },
    PidSpec { pid: 0x34, name: "Wideband O2S1 Current & Lambda", category: "O2 & Lambda", unit: "lambda", min_len: 4 },
    PidSpec { pid: 0x3C, name: "Catalyst Temp B1S1", category: "Temperatures", unit: "C", min_len: 2 },
    PidSpec { pid: 0x3D, name: "Catalyst Temp B2S1", category: "Temperatures", unit: "C", min_len: 2 },
    PidSpec { pid: 0x42, name: "Control Module Voltage", category: "Electrical & MIL", unit: "V", min_len: 2 },
    PidSpec { pid: 0x43, name: "Absolute Load", category: "Engine & Load", unit: "%", min_len: 2 },
    PidSpec { pid: 0x44, name: "Commanded Equiv. Ratio", category: "Fuel & Ignition", unit: "lambda", min_len: 2 },
    PidSpec { pid: 0x45, name: "Relative Throttle Position", category: "Airflow & Throttle", unit: "%", min_len: 1 },
    PidSpec { pid: 0x46, name: "Ambient Air Temp", category: "Temperatures", unit: "C", min_len: 1 },
    PidSpec { pid: 0x47, name: "Absolute Throttle Pos B", category: "Airflow & Throttle", unit: "%", min_len: 1 },
    PidSpec { pid: 0x49, name: "Accel Pedal Position D", category: "Airflow & Throttle", unit: "%", min_len: 1 },
    PidSpec { pid: 0x4A, name: "Accel Pedal Position E", category: "Airflow & Throttle", unit: "%", min_len: 1 },
    PidSpec { pid: 0x4C, name: "Commanded Throttle Actuator", category: "Airflow & Throttle", unit: "%", min_len: 1 },
    PidSpec { pid: 0x4D, name: "Time With MIL On", category: "Electrical & MIL", unit: "min", min_len: 2 },
    PidSpec { pid: 0x4E, name: "Time Since DTC Clear", category: "Electrical & MIL", unit: "min", min_len: 2 },
    PidSpec { pid: 0x52, name: "Ethanol Fuel Percentage", category: "Fuel & Ignition", unit: "%", min_len: 1 },
    PidSpec { pid: 0x5A, name: "Relative Accelerator Pedal", category: "Airflow & Throttle", unit: "%", min_len: 1 },
    PidSpec { pid: 0x5C, name: "Engine Oil Temp", category: "Temperatures", unit: "C", min_len: 1 },
    PidSpec { pid: 0x5D, name: "Fuel Injection Timing", category: "Fuel & Ignition", unit: "deg", min_len: 2 },
    PidSpec { pid: 0x5E, name: "Engine Fuel Rate", category: "Fuel & Ignition", unit: "L/h", min_len: 2 },
    PidSpec { pid: 0x61, name: "Driver Demand Torque", category: "Engine & Load", unit: "%", min_len: 1 },
    PidSpec { pid: 0x62, name: "Actual Engine Torque", category: "Engine & Load", unit: "%", min_len: 1 },
    PidSpec { pid: 0x63, name: "Engine Reference Torque", category: "Engine & Load", unit: "Nm", min_len: 2 },
];

fn to_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn find_pid_spec(pid: u8) -> PidSpec {
    for spec in MODE01_SPECS {
        if spec.pid == pid {
            return PidSpec {
                pid: spec.pid,
                name: spec.name,
                category: spec.category,
                unit: spec.unit,
                min_len: spec.min_len,
            };
        }
    }
    PidSpec {
        pid,
        name: "Custom OBD PID",
        category: "Custom",
        unit: "raw",
        min_len: 1,
    }
}

fn decode_obd_pid_data(pid: u8, data: &[u8]) -> Option<ObdLivePidReading> {
    let spec = find_pid_spec(pid);
    if data.len() < spec.min_len {
        return None;
    }
    let a = f32::from(data[0]);
    let ab = if data.len() >= 2 {
        f32::from((u16::from(data[0]) << 8) | u16::from(data[1]))
    } else {
        a
    };
    let cd = if data.len() >= 4 {
        f32::from((u16::from(data[2]) << 8) | u16::from(data[3]))
    } else {
        0.0
    };

    let (value, display) = match pid {
        0x04 | 0x11 | 0x2C | 0x2E | 0x2F | 0x45 | 0x47 | 0x49 | 0x4A | 0x4C | 0x52 | 0x5A => {
            let v = (a * 100.0) / 255.0;
            (v, format!("{v:.1} %"))
        }
        0x05 | 0x0F | 0x46 | 0x5C => {
            let v = a - 40.0;
            (v, format!("{v:.1} C"))
        }
        0x06..=0x09 | 0x2D => {
            let v = (a * 100.0) / 128.0 - 100.0;
            (v, format!("{v:+.1} %"))
        }
        0x0A => {
            let v = a * 3.0;
            (v, format!("{v:.0} kPa"))
        }
        0x0B | 0x33 => (a, format!("{a:.0} kPa")),
        0x0C => {
            let v = ab / 4.0;
            (v, format!("{v:.0} RPM"))
        }
        0x0D => (a, format!("{a:.0} km/h")),
        0x0E => {
            let v = a / 2.0 - 64.0;
            (v, format!("{v:+.1} deg"))
        }
        0x10 => {
            let v = ab / 100.0;
            (v, format!("{v:.2} g/s"))
        }
        0x14..=0x17 => {
            let volts = a / 200.0;
            let label = if data[1] == 0xFF {
                format!("{volts:.3} V")
            } else {
                let stft = f32::from(data[1]) * 100.0 / 128.0 - 100.0;
                format!("{volts:.3} V ({stft:+.1}%)")
            };
            (volts, label)
        }
        0x1F | 0x21 | 0x31 | 0x4D | 0x4E | 0x63 => {
            (ab, format!("{ab:.0} {}", spec.unit))
        }
        0x22 => {
            let v = ab * 0.079;
            (v, format!("{v:.1} kPa"))
        }
        0x23 => {
            let v = ab * 10.0;
            (v, format!("{v:.0} kPa"))
        }
        0x24 => {
            let lambda = ab / 32768.0;
            let volts = (cd * 8.0) / 65536.0;
            (lambda, format!("{lambda:.3} ({volts:.2} V)"))
        }
        0x30 => (a, format!("{a:.0} cycles")),
        0x34 => {
            let lambda = ab / 32768.0;
            let current = (cd / 256.0) - 128.0;
            (lambda, format!("{lambda:.3} ({current:+.1} mA)"))
        }
        0x3C | 0x3D => {
            let v = (ab / 10.0) - 40.0;
            (v, format!("{v:.1} C"))
        }
        0x42 => {
            let v = ab / 1000.0;
            (v, format!("{v:.2} V"))
        }
        0x43 => {
            let v = (ab * 100.0) / 255.0;
            (v, format!("{v:.1} %"))
        }
        0x44 => {
            let v = (ab * 2.0) / 65536.0;
            (v, format!("{v:.3} lambda"))
        }
        0x5D => {
            let v = (ab / 128.0) - 210.0;
            (v, format!("{v:+.1} deg"))
        }
        0x5E => {
            let v = ab / 20.0;
            (v, format!("{v:.2} L/h"))
        }
        0x61 | 0x62 => {
            let v = a - 125.0;
            (v, format!("{v:+.0} %"))
        }
        _ => (a, to_hex(data)),
    };

    Some(ObdLivePidReading {
        pid,
        pid_hex: format!("0x{pid:02X}"),
        name: spec.name.to_string(),
        category: spec.category.to_string(),
        value,
        display,
        unit: spec.unit.to_string(),
        raw_hex: to_hex(data),
    })
}

fn collect_bitmap_pids(base: u8, mask: &[u8], out: &mut Vec<u8>) {
    if mask.len() < 4 {
        return;
    }
    for byte_idx in 0..4 {
        let b = mask[byte_idx];
        for bit in 0..8 {
            if (b & (1 << (7 - bit))) != 0 {
                let offset = (byte_idx * 8 + bit + 1) as u8;
                let pid = base.wrapping_add(offset);
                if pid != base.wrapping_add(0x20) {
                    out.push(pid);
                }
            }
        }
    }
}

fn query_supported_obd_pids(
    device: &mut dyn Device,
    proto: ProtocolId,
    tx_can_id: u32,
) -> Vec<u8> {
    let mut supported = Vec::new();
    for base in [0x00u8, 0x20, 0x40, 0x60] {
        let Ok(resp) = exchange_diag_target(device, proto, tx_can_id, &[0x01, base]) else {
            break;
        };
        if resp.len() >= 6 && resp[0] == 0x41 && resp[1] == base {
            let mask = &resp[2..6];
            collect_bitmap_pids(base, mask, &mut supported);
            let has_next = (mask[3] & 0x01) != 0;
            if !has_next {
                break;
            }
        } else {
            break;
        }
    }
    supported
}

/// Scans supported OBD-II Mode 01 PIDs and reads a live snapshot across up to 52+ SAE sensors.
pub fn read_obd_live_snapshot(
    device: &mut dyn Device,
    tx_can_id: u32,
    requested_pids: Option<&[u8]>,
) -> Result<ObdLiveSnapshot, QuickDiagError> {
    let proto = ensure_connected(device)?;
    prepare_diag_target(device, proto, tx_can_id)?;
    let mut notes = Vec::new();

    let supported_pids = query_supported_obd_pids(device, proto, tx_can_id);
    let pids_to_poll: Vec<u8> = match requested_pids {
        Some(list) if !list.is_empty() => list.to_vec(),
        _ if !supported_pids.is_empty() => MODE01_SPECS
            .iter()
            .filter(|s| supported_pids.contains(&s.pid))
            .map(|s| s.pid)
            .collect(),
        _ => {
            notes.push("ECU did not return PID bitmaps; polling essential OBD PIDs directly.".to_string());
            vec![0x0C, 0x0D, 0x05, 0x0B, 0x0F, 0x11, 0x0E, 0x06, 0x07, 0x42]
        }
    };

    let mut readings = Vec::new();
    for pid in pids_to_poll {
        match exchange_diag_target(device, proto, tx_can_id, &[0x01, pid]) {
            Ok(resp) if resp.len() >= 3 && resp[0] == 0x41 && resp[1] == pid => {
                if let Some(decoded) = decode_obd_pid_data(pid, &resp[2..]) {
                    readings.push(decoded);
                }
            }
            Ok(resp) => {
                let _ = check_negative_response(&resp);
            }
            Err(_) => {}
        }
    }

    Ok(ObdLiveSnapshot {
        protocol: protocol_label(proto),
        tx_can_id,
        supported_pids,
        readings,
        notes,
    })
}

/// Reads OBD Mode 01 PID `0x01` I/M readiness monitors and Mode `0x0A` permanent DTCs.
pub fn read_im_readiness(
    device: &mut dyn Device,
    tx_can_id: u32,
) -> Result<ImReadinessSummary, QuickDiagError> {
    let proto = ensure_connected(device)?;
    prepare_diag_target(device, proto, tx_can_id)?;
    let mut notes = Vec::new();

    let resp = exchange_diag_target(device, proto, tx_can_id, &[0x01, 0x01])?;
    check_negative_response(&resp)?;
    if resp.len() < 6 || resp[0] != 0x41 || resp[1] != 0x01 {
        return Err(QuickDiagError::UnexpectedResponse {
            expected: 0x41,
            got: resp.first().copied(),
        });
    }

    let a = resp[2];
    let b = resp[3];
    let c = resp[4];
    let d = resp[5];
    let mil_on = (a & 0x80) != 0;
    let dtc_count = a & 0x7F;
    let compression_ignition = (b & 0x08) != 0;

    let mut monitors = vec![
        ImReadinessMonitor {
            name: "Misfire".to_string(),
            supported: (b & 0x01) != 0,
            ready: (b & 0x10) == 0,
        },
        ImReadinessMonitor {
            name: "Fuel System".to_string(),
            supported: (b & 0x02) != 0,
            ready: (b & 0x20) == 0,
        },
        ImReadinessMonitor {
            name: "Components".to_string(),
            supported: (b & 0x04) != 0,
            ready: (b & 0x40) == 0,
        },
    ];

    let extended_names: [&str; 8] = if compression_ignition {
        [
            "NMHC Catalyst",
            "NOx/SCR Monitor",
            "Reserved",
            "Boost Pressure",
            "Reserved 2",
            "Exhaust Gas Sensor",
            "PM Filter",
            "EGR/VVT System",
        ]
    } else {
        [
            "Catalyst",
            "Heated Catalyst",
            "Evaporative System",
            "Secondary Air System",
            "A/C Refrigerant",
            "Oxygen Sensor",
            "Oxygen Sensor Heater",
            "EGR/VVT System",
        ]
    };

    for (bit, label) in extended_names.iter().enumerate() {
        let mask = 1u8 << bit;
        if (c & mask) != 0 {
            monitors.push(ImReadinessMonitor {
                name: (*label).to_string(),
                supported: true,
                ready: (d & mask) == 0,
            });
        }
    }

    let mut permanent_dtcs = Vec::new();
    match exchange_diag_target(device, proto, tx_can_id, &[0x0A]) {
        Ok(perm_resp) => {
            if let Ok(list) = parse_obd_dtc_response(
                &perm_resp,
                0x4A,
                0x80,
                "permanent",
                "obd_mode0A",
            ) {
                permanent_dtcs = list;
            }
        }
        Err(err) => notes.push(format!("Permanent DTC scan (Mode 0A) unavailable: {err}")),
    }

    Ok(ImReadinessSummary {
        mil_on,
        dtc_count,
        compression_ignition,
        monitors,
        permanent_dtcs,
        notes,
    })
}

/// Reads OBD Mode 02 Freeze Frame snapshot (frame index `0x00`).
pub fn read_freeze_frame(
    device: &mut dyn Device,
    tx_can_id: u32,
) -> Result<FreezeFrameSummary, QuickDiagError> {
    let proto = ensure_connected(device)?;
    prepare_diag_target(device, proto, tx_can_id)?;
    let mut notes = Vec::new();

    let mut freeze_dtc = None;
    let mut freeze_dtc_description = None;
    match exchange_diag_target(device, proto, tx_can_id, &[0x02, 0x02, 0x00]) {
        Ok(resp) if resp.len() >= 5 && resp[0] == 0x42 && resp[1] == 0x02 => {
            if let Some(code) = decode_j2012_dtc(resp[3], resp[4]) {
                freeze_dtc_description = Some(lookup_dtc_description(&code).summary);
                freeze_dtc = Some(code);
            } else {
                notes.push("Freeze frame trigger DTC is P0000 (no captured fault snapshot).".to_string());
            }
        }
        Ok(resp) => {
            if let Err(err) = check_negative_response(&resp) {
                notes.push(format!("Mode 02 Freeze DTC declined: {err}"));
            }
        }
        Err(err) => notes.push(format!("Mode 02 Freeze DTC failed: {err}")),
    }

    let mut readings = Vec::new();
    for pid in [0x04u8, 0x05, 0x06, 0x07, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x11, 0x42] {
        if let Ok(resp) = exchange_diag_target(device, proto, tx_can_id, &[0x02, pid, 0x00]) {
            if resp.len() >= 4 && resp[0] == 0x42 && resp[1] == pid {
                if let Some(decoded) = decode_obd_pid_data(pid, &resp[3..]) {
                    readings.push(decoded);
                }
            }
        }
    }

    Ok(FreezeFrameSummary {
        freeze_dtc,
        freeze_dtc_description,
        readings,
        notes,
    })
}

fn mode06_mid_label(mid: u8) -> &'static str {
    match mid {
        0x01 => "O2 Bank 1 Sensor 1",
        0x02 => "O2 Bank 1 Sensor 2",
        0x05 => "O2 Bank 2 Sensor 1",
        0x21 => "Catalyst Monitor B1",
        0x22 => "Catalyst Monitor B2",
        0x31 => "EGR Monitor B1",
        0x3C => "EVAP Monitor (0.020\")",
        0xA1 => "Misfire Cylinder 1",
        0xA2 => "Misfire Cylinder 2",
        0xA3 => "Misfire Cylinder 3",
        0xA4 => "Misfire Cylinder 4",
        _ => "OBD On-Board Monitor",
    }
}

/// Reads SAE J1979 Mode 06 CAN on-board monitoring test records.
pub fn read_mode06_monitors(
    device: &mut dyn Device,
    tx_can_id: u32,
    mids: Option<&[u8]>,
) -> Result<Vec<Mode06TestResult>, QuickDiagError> {
    let proto = ensure_connected(device)?;
    prepare_diag_target(device, proto, tx_can_id)?;

    let probe_mids = mids.unwrap_or(&[0x01, 0x02, 0x21, 0x31, 0x3C, 0xA1, 0xA2, 0xA3, 0xA4]);
    let mut results = Vec::new();
    for &mid in probe_mids {
        let Ok(resp) = exchange_diag_target(device, proto, tx_can_id, &[0x06, mid]) else {
            continue;
        };
        if resp.first().copied() != Some(0x46) || resp.len() < 10 {
            continue;
        }
        // Standard ISO 15765-4 Mode 06 tuple: [MID(1), TID(1), UASID(1), VAL(2), MIN(2), MAX(2)] = 9 bytes
        for chunk in resp[1..].chunks_exact(9) {
            let val = u16::from_be_bytes([chunk[3], chunk[4]]);
            let min = u16::from_be_bytes([chunk[5], chunk[6]]);
            let max = u16::from_be_bytes([chunk[7], chunk[8]]);
            results.push(Mode06TestResult {
                mid: chunk[0],
                tid: chunk[1],
                uasid: chunk[2],
                value: val,
                min,
                max,
                passed: val >= min && val <= max,
                label: mode06_mid_label(chunk[0]).to_string(),
            });
        }
    }
    Ok(results)
}

fn ecu_slot_label(tx_id: u32) -> &'static str {
    match tx_id {
        0x07E0 => "ECM / Engine",
        0x07E1 => "TCM / Transmission",
        0x07E2 => "ABS / Stability",
        0x07E3 => "Hybrid / HV Battery",
        0x07E4 => "BCM / Chassis",
        0x07E5 => "IPC / Cluster",
        0x07E6 => "SRS / Airbag",
        0x07E7 => "Gateway / Auxiliary",
        0x18DA_10F1 => "29-bit ECM (0x10)",
        0x18DA_18F1 => "29-bit TCM (0x18)",
        0x18DA_28F1 => "29-bit ABS (0x28)",
        _ => "ECU Module",
    }
}

/// Scans standard 11-bit (`0x7E0..=0x7E7`) and optional 29-bit CAN ECU physical addresses.
pub fn scan_ecu_modules(
    device: &mut dyn Device,
    include_29bit: bool,
) -> Result<Vec<DiscoveredEcuModule>, QuickDiagError> {
    let proto = ensure_connected(device)?;
    let mut targets = vec![0x07E0, 0x07E1, 0x07E2, 0x07E3, 0x07E4, 0x07E5, 0x07E6, 0x07E7];
    if include_29bit {
        targets.extend([0x18DA_10F1, 0x18DA_18F1, 0x18DA_28F1]);
    }

    let mut modules = Vec::new();
    for tx_id in targets {
        let rx_id = derive_rx_can_id_for_diag(tx_id);
        if device.configure_can_ids(tx_id, rx_id).is_err() {
            continue;
        }
        // Probe TesterPresent (`0x3E 0x00`) or UDS `0x10 0x01`
        let mut responded = false;
        let mut active_session = None;
        if let Ok(tp_resp) = exchange_diag_target(device, proto, tx_id, &[0x3E, 0x00]) {
            if tp_resp.first().copied() == Some(0x7E) || tp_resp.first().copied() == Some(0x7F) {
                responded = true;
            }
        }
        if !responded {
            if let Ok(sess_resp) = exchange_diag_target(device, proto, tx_id, &[0x10, 0x01]) {
                if sess_resp.len() >= 2 && sess_resp[0] == 0x50 {
                    responded = true;
                    active_session = Some(sess_resp[1]);
                } else if sess_resp.first().copied() == Some(0x7F) {
                    responded = true;
                }
            }
        }
        if !responded {
            continue;
        }

        let read_did_ascii = |dev: &mut dyn Device, did: u16| -> Option<String> {
            let hi = (did >> 8) as u8;
            let lo = (did & 0xFF) as u8;
            let resp = exchange_diag_target(dev, proto, tx_id, &[0x22, hi, lo]).ok()?;
            if resp.len() >= 4 && resp[0] == 0x62 && resp[1] == hi && resp[2] == lo {
                extract_ascii_payload(&resp[3..])
            } else {
                None
            }
        };

        let vin = read_did_ascii(device, 0xF190);
        let part_number = read_did_ascii(device, 0xF187).or_else(|| read_did_ascii(device, 0xF191));
        let sw_version = read_did_ascii(device, 0xF189).or_else(|| read_did_ascii(device, 0xF195));
        let dtc_count = exchange_diag_target(
            device,
            proto,
            tx_id,
            &BusSilencingInterlock::build_read_dtc_by_status_mask(0xFF),
        )
        .ok()
        .and_then(|resp| {
            if resp.len() >= 2 && resp[0] == 0x59 {
                BusSilencingInterlock::parse_dtc_list_response(&resp)
                    .ok()
                    .map(|list| list.into_iter().filter(|(c, _)| c != "P0000").count())
            } else {
                None
            }
        });

        modules.push(DiscoveredEcuModule {
            tx_can_id: tx_id,
            rx_can_id: rx_id,
            tx_hex: format!("0x{tx_id:X}"),
            rx_hex: format!("0x{rx_id:X}"),
            label: ecu_slot_label(tx_id).to_string(),
            is_29bit: tx_id > 0x7FF,
            vin,
            part_number,
            sw_version,
            active_session,
            dtc_count,
        });
    }

    let _ = device.configure_can_ids(0x07E0, 0x07E8);
    Ok(modules)
}

/// Switches UDS DiagnosticSessionControl (`0x10` subfunction: `0x01` Default, `0x02` Programming, `0x03` Extended).
pub fn uds_enter_session(
    device: &mut dyn Device,
    tx_can_id: u32,
    session_subfunction: u8,
) -> Result<String, QuickDiagError> {
    let proto = ensure_connected(device)?;
    prepare_diag_target(device, proto, tx_can_id)?;
    let resp = exchange_diag_target(device, proto, tx_can_id, &[0x10, session_subfunction])?;
    check_negative_response(&resp)?;
    if resp.len() < 2 || resp[0] != 0x50 {
        return Err(QuickDiagError::UnexpectedResponse {
            expected: 0x50,
            got: resp.first().copied(),
        });
    }
    Ok(format!("0x{:02X} ({})", resp[1], to_hex(&resp)))
}

/// Sends UDS TesterPresent (`0x3E 0x00`) to keep an extended diagnostic session awake.
pub fn uds_tester_present(device: &mut dyn Device, tx_can_id: u32) -> Result<bool, QuickDiagError> {
    let proto = ensure_connected(device)?;
    prepare_diag_target(device, proto, tx_can_id)?;
    let resp = exchange_diag_target(device, proto, tx_can_id, &[0x3E, 0x00])?;
    check_negative_response(&resp)?;
    Ok(resp.first().copied() == Some(0x7E))
}

fn standard_did_name(did: u16) -> &'static str {
    match did {
        0xF180 => "Boot Software ID",
        0xF181 => "Application Software ID",
        0xF186 => "Active Diagnostic Session",
        0xF187 => "Spare Part Number",
        0xF189 => "ECU Software Version",
        0xF18A => "System Supplier ID",
        0xF18C => "ECU Serial Number",
        0xF190 => "VIN",
        0xF191 => "Hardware Version Number",
        0xF194 => "Supplier SW Number",
        0xF197 => "System / Engine Name",
        _ => "Custom DID",
    }
}

/// Reads standard identification or user-specified UDS DIDs via `0x22 ReadDataByIdentifier`.
pub fn uds_read_standard_dids(
    device: &mut dyn Device,
    tx_can_id: u32,
    custom_dids: Option<&[u16]>,
) -> Result<Vec<UdsDidValue>, QuickDiagError> {
    let proto = ensure_connected(device)?;
    prepare_diag_target(device, proto, tx_can_id)?;

    let dids = custom_dids.unwrap_or(&[
        0xF186, 0xF190, 0xF187, 0xF189, 0xF191, 0xF181, 0xF18C, 0xF197,
    ]);
    let mut out = Vec::new();
    for &did in dids {
        let hi = (did >> 8) as u8;
        let lo = (did & 0xFF) as u8;
        let Ok(resp) = exchange_diag_target(device, proto, tx_can_id, &[0x22, hi, lo]) else {
            continue;
        };
        if resp.len() >= 4 && resp[0] == 0x62 && resp[1] == hi && resp[2] == lo {
            let payload = &resp[3..];
            out.push(UdsDidValue {
                did,
                did_hex: format!("0x{did:04X}"),
                name: standard_did_name(did).to_string(),
                raw_hex: to_hex(payload),
                ascii: extract_ascii_payload(payload),
            });
        }
    }
    Ok(out)
}

/// Executes UDS RoutineControl (`0x31`: subfunction `0x01` Start / `0x02` Stop / `0x03` RequestResults).
pub fn uds_routine_control(
    device: &mut dyn Device,
    tx_can_id: u32,
    subfunction: u8,
    routine_id: u16,
    option_record: &[u8],
) -> Result<UdsRoutineOutcome, QuickDiagError> {
    let proto = ensure_connected(device)?;
    prepare_diag_target(device, proto, tx_can_id)?;

    let mut req = Vec::with_capacity(4 + option_record.len());
    req.push(0x31);
    req.push(subfunction);
    req.push((routine_id >> 8) as u8);
    req.push((routine_id & 0xFF) as u8);
    req.extend_from_slice(option_record);

    let resp = exchange_diag_target(device, proto, tx_can_id, &req)?;
    check_negative_response(&resp)?;
    if resp.len() < 4 || resp[0] != 0x71 {
        return Err(QuickDiagError::UnexpectedResponse {
            expected: 0x71,
            got: resp.first().copied(),
        });
    }

    Ok(UdsRoutineOutcome {
        routine_id,
        subfunction,
        acknowledged: true,
        response_hex: to_hex(&resp),
        notes: vec![format!(
            "Routine 0x{routine_id:04X} subfunction 0x{subfunction:02X} acknowledged."
        )],
    })
}

/// Executes UDS InputOutputControlByIdentifier (`0x2F`) for safe active actuator tests.
/// `control_param`: `0x00` ReturnControlToECU, `0x01` ResetToDefault, `0x02` FreezeCurrentState, `0x03` ShortTermAdjustment.
pub fn uds_io_control(
    device: &mut dyn Device,
    tx_can_id: u32,
    did: u16,
    control_param: u8,
    control_state: &[u8],
) -> Result<UdsIoControlOutcome, QuickDiagError> {
    let proto = ensure_connected(device)?;
    prepare_diag_target(device, proto, tx_can_id)?;

    let mut req = Vec::with_capacity(4 + control_state.len());
    req.push(0x2F);
    req.push((did >> 8) as u8);
    req.push((did & 0xFF) as u8);
    req.push(control_param);
    if control_param == 0x03 {
        req.extend_from_slice(control_state);
    }

    let resp = exchange_diag_target(device, proto, tx_can_id, &req)?;
    check_negative_response(&resp)?;
    if resp.len() < 4 || resp[0] != 0x6F {
        return Err(QuickDiagError::UnexpectedResponse {
            expected: 0x6F,
            got: resp.first().copied(),
        });
    }

    Ok(UdsIoControlOutcome {
        did,
        control_param,
        acknowledged: true,
        response_hex: to_hex(&resp),
        notes: vec![format!(
            "IOControl DID 0x{did:04X} param 0x{control_param:02X} acknowledged."
        )],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::{DeviceKind, MockDevice, ScriptedReply, TransportKind};
    use crate::j2534_types::PassThruMsg;

    fn connected_mock() -> MockDevice {
        let mut dev = MockDevice::new(DeviceKind::Serial, "Diag Test", TransportKind::Serial);
        dev.open().unwrap();
        dev.connect(ProtocolId::Iso15765, 500_000).unwrap();
        dev
    }

    fn msg(bytes: Vec<u8>) -> ScriptedReply {
        ScriptedReply::Message(PassThruMsg::new(ProtocolId::Iso15765, bytes))
    }

    #[test]
    fn reads_live_obd_snapshot_across_bitmaps_and_decodes_sensors() {
        let mut dev = connected_mock();
        dev.set_script(vec![
            // Supported PIDs 01-20: 0x05 (Coolant), 0x0C (RPM), 0x11 (TPS)
            // 0x05 = byte0 bit3 (0x08), 0x0C = byte1 bit4 (0x10), 0x11 = byte2 bit7 (0x80), no 0x20 bit
            msg(vec![0x41, 0x00, 0x08, 0x10, 0x80, 0x00]),
            // Requested sensors in order: 0x05, 0x0C, 0x11
            msg(vec![0x41, 0x05, 130]),        // 90 C
            msg(vec![0x41, 0x0C, 0x1F, 0x40]), // 2000 RPM
            msg(vec![0x41, 0x11, 255]),        // 100.0 %
        ]);

        let snap = read_obd_live_snapshot(&mut dev, 0x7E0, None).unwrap();
        assert_eq!(snap.supported_pids, vec![0x05, 0x0C, 0x11]);
        assert_eq!(snap.readings.len(), 3);
        assert_eq!(snap.readings[0].value, 90.0);
        assert_eq!(snap.readings[1].value, 2000.0);
        assert!((snap.readings[2].value - 100.0).abs() < 0.01);
    }

    #[test]
    fn reads_im_readiness_freeze_frame_and_mode06() {
        let mut dev = connected_mock();
        dev.set_script(vec![
            // Mode 01 01: MIL on, 2 DTCs, Misfire/Fuel/Components supported & ready, Catalyst+O2 supported, O2 not ready
            msg(vec![0x41, 0x01, 0x82, 0x07, 0x21, 0x20]),
            // Mode 0A: P0420 permanent
            msg(vec![0x4A, 0x01, 0x04, 0x20]),
        ]);
        let im = read_im_readiness(&mut dev, 0x7E0).unwrap();
        assert!(im.mil_on);
        assert_eq!(im.dtc_count, 2);
        assert_eq!(im.monitors.len(), 5);
        assert_eq!(im.permanent_dtcs.len(), 1);
        assert_eq!(im.permanent_dtcs[0].code, "P0420");

        dev.set_script(vec![
            msg(vec![0x42, 0x02, 0x00, 0x03, 0x01]),
            msg(vec![0x42, 0x04, 0x00, 128]),
            msg(vec![0x42, 0x05, 0x00, 125]),
            msg(vec![0x7F, 0x02, 0x12]),
            msg(vec![0x7F, 0x02, 0x12]),
            msg(vec![0x7F, 0x02, 0x12]),
            msg(vec![0x42, 0x0C, 0x00, 0x0F, 0xA0]),
            msg(vec![0x7F, 0x02, 0x12]),
            msg(vec![0x7F, 0x02, 0x12]),
            msg(vec![0x7F, 0x02, 0x12]),
            msg(vec![0x7F, 0x02, 0x12]),
            msg(vec![0x7F, 0x02, 0x12]),
        ]);
        let ff = read_freeze_frame(&mut dev, 0x7E0).unwrap();
        assert_eq!(ff.freeze_dtc.as_deref(), Some("P0301"));
        assert!(ff.freeze_dtc_description.is_some());
        assert_eq!(ff.readings.len(), 3);

        dev.set_script(vec![
            msg(vec![0x46, 0x01, 0x01, 0x0A, 0x00, 0x64, 0x00, 0x32, 0x00, 0xC8]),
        ]);
        let m06 = read_mode06_monitors(&mut dev, 0x7E0, Some(&[0x01])).unwrap();
        assert_eq!(m06.len(), 1);
        assert!(m06[0].passed);
        assert_eq!(m06[0].value, 100);
    }

    #[test]
    fn uds_services_handle_response_pending_and_routine_and_io_control() {
        let mut dev = connected_mock();
        dev.set_script(vec![
            msg(vec![0x50, 0x03, 0x00, 0x32]),
            msg(vec![0x7E, 0x00]),
            msg(vec![0x62, 0xF1, 0x90, b'T', b'E', b'S', b'T', b'V', b'I', b'N']),
            msg(vec![0x7F, 0x31, 0x78]),
            msg(vec![0x71, 0x01, 0xFF, 0x00, 0x00]),
            msg(vec![0x6F, 0x12, 0x34, 0x03, 0x64]),
        ]);

        assert!(uds_enter_session(&mut dev, 0x7E0, 0x03).unwrap().contains("0x03"));
        assert!(uds_tester_present(&mut dev, 0x7E0).unwrap());
        let dids = uds_read_standard_dids(&mut dev, 0x7E0, Some(&[0xF190])).unwrap();
        assert_eq!(dids[0].ascii.as_deref(), Some("TESTVIN"));
        let r = uds_routine_control(&mut dev, 0x7E0, 0x01, 0xFF00, &[]).unwrap();
        assert!(r.acknowledged);
        let io = uds_io_control(&mut dev, 0x7E0, 0x1234, 0x03, &[0x64]).unwrap();
        assert!(io.acknowledged);
    }

    #[test]
    fn scans_multi_ecu_modules_and_restores_default_7e0() {
        let mut dev = connected_mock();
        // Only 0x7E0 and 0x7E1 respond; rest return timeout/empty
        dev.set_script(vec![
            // 0x7E0: TP positive + F190 + F187 + F189 + UDS DTCs (none)
            msg(vec![0x7E, 0x00]),
            msg(vec![0x62, 0xF1, 0x90, b'E', b'C', b'M', b'1']),
            msg(vec![0x62, 0xF1, 0x87, b'3', b'8', b'7', b'7', b'0']),
            msg(vec![0x62, 0xF1, 0x89, b'S', b'W', b'1']),
            msg(vec![0x59, 0x02, 0xFF]),
            // 0x7E1: TP NRC 0x11 (still confirms ECU alive!) + F190 NRC + F187 + F191 NRC + F189 + F195 NRC + DTC 1
            msg(vec![0x7F, 0x3E, 0x11]),
            msg(vec![0x7F, 0x22, 0x31]),
            msg(vec![0x62, 0xF1, 0x87, b'T', b'C', b'U', b'1']),
            msg(vec![0x7F, 0x22, 0x31]),
            msg(vec![0x7F, 0x22, 0x31]),
            msg(vec![0x59, 0x02, 0xFF, 0x07, 0x00, 0x00, 0x08]),
        ]);

        let mods = scan_ecu_modules(&mut dev, false).unwrap();
        assert_eq!(mods.len(), 2);
        assert_eq!(mods[0].tx_can_id, 0x07E0);
        assert_eq!(mods[0].vin.as_deref(), Some("ECM1"));
        assert_eq!(mods[1].tx_can_id, 0x07E1);
        assert_eq!(mods[1].dtc_count, Some(1));
    }
}
