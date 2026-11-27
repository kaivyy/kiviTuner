//! Quick OBD-II and UDS diagnostics over any attached [`Device`].
//!
//! Designed so cheap serial ELM327 / STN1110 adapters as well as J2534,
//! OpenPort 2.0, DoIP, and SocketCAN hardware share one safe diagnostic path:
//! - Battery voltage (`ATRV` / J2534 VBATT) + VIN + Calibration ID + CVN
//! - Read fault codes (DTCs) via UDS `0x19` with automatic OBD Mode `03`/`07` fallback
//! - Clear / reset fault codes via UDS `0x14` with automatic OBD Mode `04` fallback
//! - UDS `0x11` ECU reset (`Hard`, `KeyOffOn`, `Soft`)

use crate::bus_silencing::BusSilencingInterlock;
use crate::device::{Device, DeviceError};
use crate::dtc_dictionary::lookup_dtc_description;
use crate::j2534_types::{tx_flags, PassThruMsg, ProtocolId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const MAX_PENDING_RETRIES: usize = 8;

/// Errors raised while running a quick diagnostic transaction.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum QuickDiagError {
    #[error("device error: {0}")]
    Device(#[from] DeviceError),
    #[error("ECU negative response to service 0x{service:02X}: NRC 0x{nrc:02X}")]
    NegativeResponse { service: u8, nrc: u8 },
    #[error("unexpected ECU reply (expected 0x{expected:02X}, got {got:?})")]
    UnexpectedResponse { expected: u8, got: Option<u8> },
    #[error("diagnostic service unsupported: {0}")]
    Unsupported(&'static str),
}

/// One decoded Diagnostic Trouble Code reported by UDS or OBD-II.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DtcRecord {
    pub code: String,
    pub status_mask: u8,
    pub status_label: String,
    pub source: String,
    #[serde(default)]
    pub description: Option<String>,
}

/// Battery voltage and basic ECU identity strings read over OBD-II / UDS.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EcuIdentitySummary {
    pub battery_mv: Option<u32>,
    pub vin: Option<String>,
    pub cal_id: Option<String>,
    pub cvn: Option<String>,
    pub protocol: String,
    pub notes: Vec<String>,
}

/// Result of querying stored and pending DTCs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DtcReadOutcome {
    pub dtcs: Vec<DtcRecord>,
    pub protocol_used: String,
    pub notes: Vec<String>,
}

/// Result of clearing trouble codes and freeze-frame data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DtcClearOutcome {
    pub cleared: bool,
    pub protocol_used: String,
    pub notes: Vec<String>,
}

/// Sub-function choices for UDS service `0x11` (`ECUReset`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EcuResetMode {
    Hard,
    KeyOffOn,
    Soft,
}

impl EcuResetMode {
    /// Returns the ISO 14229-1 sub-function byte for UDS `0x11`.
    pub fn subfunction(self) -> u8 {
        match self {
            EcuResetMode::Hard => 0x01,
            EcuResetMode::KeyOffOn => 0x02,
            EcuResetMode::Soft => 0x03,
        }
    }

    /// Human-readable description of the reset mode.
    pub fn label(self) -> &'static str {
        match self {
            EcuResetMode::Hard => "Hard Reset (0x01)",
            EcuResetMode::KeyOffOn => "Key Off/On Reset (0x02)",
            EcuResetMode::Soft => "Soft Reset (0x03)",
        }
    }
}

/// Result of requesting a UDS `0x11` ECU reset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EcuResetOutcome {
    pub acknowledged: bool,
    pub mode: EcuResetMode,
    pub power_down_time_secs: Option<u8>,
    pub notes: Vec<String>,
}

pub(crate) fn ensure_connected(device: &dyn Device) -> Result<ProtocolId, QuickDiagError> {
    if !device.is_open() {
        return Err(DeviceError::NotOpen.into());
    }
    if !device.is_connected() {
        return Err(DeviceError::NotConnected.into());
    }
    Ok(device.connected_protocol().unwrap_or(ProtocolId::Iso15765))
}

pub(crate) fn protocol_label(proto: ProtocolId) -> String {
    format!("{proto:?}")
}

pub(crate) fn exchange_diag_target(
    device: &mut dyn Device,
    proto: ProtocolId,
    tx_can_id: u32,
    tx: &[u8],
) -> Result<Vec<u8>, QuickDiagError> {
    let use_can_prefix =
        device.uses_can_id_prefix() && matches!(proto, ProtocolId::Iso15765 | ProtocolId::Can);
    let wire = if use_can_prefix {
        let mut v = Vec::with_capacity(4 + tx.len());
        v.extend_from_slice(&tx_can_id.to_be_bytes());
        v.extend_from_slice(tx);
        v
    } else {
        tx.to_vec()
    };
    let flags = if use_can_prefix {
        tx_flags::ISO15765_FRAME_PAD | if tx_can_id > 0x7FF { tx_flags::CAN_29BIT_ID } else { 0 }
    } else {
        0
    };
    device.send(&PassThruMsg::with_flags(proto, flags, wire))?;
    for _ in 0..=MAX_PENDING_RETRIES {
        let reply = device.receive()?;
        let body = if use_can_prefix {
            if reply.data.len() < 4 {
                return Err(DeviceError::Transport(
                    "CAN/ISO15765 response missing 4-byte CAN arbitration ID prefix".to_string(),
                )
                .into());
            }
            &reply.data[4..]
        } else {
            reply.data.as_slice()
        };
        if body.len() >= 3 && body[0] == 0x7F && body[2] == 0x78 {
            continue;
        }
        return Ok(body.to_vec());
    }
    Err(DeviceError::Timeout.into())
}

pub(crate) fn check_negative_response(resp: &[u8]) -> Result<(), QuickDiagError> {
    if resp.len() >= 3 && resp[0] == 0x7F {
        return Err(QuickDiagError::NegativeResponse {
            service: resp[1],
            nrc: resp[2],
        });
    }
    Ok(())
}

pub(crate) fn extract_ascii_payload(bytes: &[u8]) -> Option<String> {
    let cleaned: String = bytes
        .iter()
        .copied()
        .filter(|&b| b != 0x00 && (b.is_ascii_graphic() || b == b' '))
        .map(|b| b as char)
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub(crate) fn parse_mode09_ascii(resp: &[u8], expected_pid: u8) -> Option<String> {
    if resp.len() < 3 || resp[0] != 0x49 || resp[1] != expected_pid {
        return None;
    }
    let slice = if resp.len() >= 4 && resp[2] <= 0x10 {
        &resp[3..]
    } else {
        &resp[2..]
    };
    extract_ascii_payload(slice)
}

fn parse_mode09_cvn(resp: &[u8]) -> Option<String> {
    if resp.len() < 4 || resp[0] != 0x49 || resp[1] != 0x06 {
        return None;
    }
    let slice = if resp.len() >= 7 && resp[2] <= 0x08 && (resp.len() - 3) % 4 == 0 {
        &resp[3..]
    } else {
        &resp[2..]
    };
    if slice.is_empty() {
        return None;
    }
    Some(
        slice
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(""),
    )
}

pub(crate) fn decode_j2012_dtc(hi: u8, lo: u8) -> Option<String> {
    if hi == 0 && lo == 0 {
        return None;
    }
    let prefix = match (hi >> 6) & 0x03 {
        0 => 'P',
        1 => 'C',
        2 => 'B',
        _ => 'U',
    };
    let d1 = (hi >> 4) & 0x03;
    let d2 = hi & 0x0F;
    Some(format!("{prefix}{d1}{d2:X}{lo:02X}"))
}

pub(crate) fn parse_obd_dtc_response(
    resp: &[u8],
    expected_sid: u8,
    status_mask: u8,
    status_label: &str,
    source: &str,
) -> Result<Vec<DtcRecord>, QuickDiagError> {
    check_negative_response(resp)?;
    if resp.first().copied() != Some(expected_sid) {
        return Err(QuickDiagError::UnexpectedResponse {
            expected: expected_sid,
            got: resp.first().copied(),
        });
    }

    // CAN (ISO 15765-4) replies include a DTC count byte: `[0x43, N, hi1, lo1, ...]`,
    // whereas K-Line / ISO 9141 replies pack fixed 2-byte pairs immediately after `0x43`.
    let body = if resp.len() >= 2 && (resp.len() - 2) == 2 * usize::from(resp[1]) {
        &resp[2..]
    } else if resp.len() >= 2 && (resp.len() - 1) % 2 == 0 {
        &resp[1..]
    } else {
        &resp[resp.len().min(2)..]
    };

    let mut out = Vec::new();
    for pair in body.chunks_exact(2) {
        if let Some(code) = decode_j2012_dtc(pair[0], pair[1]) {
            let desc = lookup_dtc_description(&code).summary;
            out.push(DtcRecord {
                code,
                status_mask,
                status_label: status_label.to_string(),
                source: source.to_string(),
                description: Some(desc),
            });
        }
    }
    Ok(out)
}

fn uds_status_label(mask: u8) -> String {
    if mask & 0x08 != 0 {
        "confirmed".to_string()
    } else if mask & 0x04 != 0 {
        "pending".to_string()
    } else {
        format!("uds(0x{mask:02X})")
    }
}

pub(crate) fn derive_rx_can_id_for_diag(tx_id: u32) -> u32 {
    if tx_id == 0x7DF {
        0x7E8
    } else if (tx_id >> 16) == 0x18DA {
        (tx_id & 0xFFFF_0000) | ((tx_id & 0xFF) << 8) | ((tx_id >> 8) & 0xFF)
    } else {
        tx_id.wrapping_add(8)
    }
}

pub(crate) fn prepare_diag_target(device: &mut dyn Device, proto: ProtocolId, tx_can_id: u32) -> Result<(), QuickDiagError> {
    if tx_can_id != 0x0000_07E0 && matches!(proto, ProtocolId::Iso15765 | ProtocolId::Can) {
        device.configure_can_ids(tx_can_id, derive_rx_can_id_for_diag(tx_can_id))?;
    }
    Ok(())
}

/// Reads battery voltage, VIN, Calibration ID, and CVN from the attached device.
pub fn read_ecu_identity(device: &mut dyn Device) -> Result<EcuIdentitySummary, QuickDiagError> {
    read_ecu_identity_for_can_id(device, 0x0000_07E0)
}

/// Reads battery voltage, VIN, Calibration ID, and CVN from a specific ECU TX CAN ID.
pub fn read_ecu_identity_for_can_id(
    device: &mut dyn Device,
    tx_can_id: u32,
) -> Result<EcuIdentitySummary, QuickDiagError> {
    let proto = ensure_connected(device)?;
    prepare_diag_target(device, proto, tx_can_id)?;
    let mut notes = Vec::new();

    let battery_mv = match device.read_voltage_millivolts() {
        Ok(mv) => Some(mv),
        Err(err) => {
            notes.push(format!("Battery voltage unavailable: {err}"));
            None
        }
    };

    let mut vin = None;
    match exchange_diag_target(device, proto, tx_can_id, &[0x22, 0xF1, 0x90]) {
        Ok(resp) if resp.len() >= 4 && resp[0] == 0x62 && resp[1] == 0xF1 && resp[2] == 0x90 => {
            vin = extract_ascii_payload(&resp[3..]);
        }
        Ok(resp) => {
            if let Err(err) = check_negative_response(&resp) {
                notes.push(format!("UDS VIN (0xF190) declined: {err}"));
            }
        }
        Err(err) => notes.push(format!("UDS VIN (0xF190) failed: {err}")),
    }

    if vin.is_none() {
        match exchange_diag_target(device, proto, tx_can_id, &[0x09, 0x02]) {
            Ok(resp) => {
                vin = parse_mode09_ascii(&resp, 0x02);
                if vin.is_none() {
                    notes.push("OBD Mode 09 PID 02 returned no printable VIN.".to_string());
                }
            }
            Err(err) => notes.push(format!("OBD Mode 09 VIN failed: {err}")),
        }
    }

    let mut cal_id = None;
    match exchange_diag_target(device, proto, tx_can_id, &[0x09, 0x04]) {
        Ok(resp) => cal_id = parse_mode09_ascii(&resp, 0x04),
        Err(err) => notes.push(format!("OBD Mode 09 Cal-ID failed: {err}")),
    }

    if cal_id.is_none() {
        if let Ok(resp) = exchange_diag_target(device, proto, tx_can_id, &[0x22, 0xF1, 0x81]) {
            if resp.len() >= 4 && resp[0] == 0x62 && resp[1] == 0xF1 && resp[2] == 0x81 {
                cal_id = extract_ascii_payload(&resp[3..]);
            }
        }
    }

    let mut cvn = None;
    match exchange_diag_target(device, proto, tx_can_id, &[0x09, 0x06]) {
        Ok(resp) => cvn = parse_mode09_cvn(&resp),
        Err(err) => notes.push(format!("OBD Mode 09 CVN failed: {err}")),
    }

    Ok(EcuIdentitySummary {
        battery_mv,
        vin,
        cal_id,
        cvn,
        protocol: protocol_label(proto),
        notes,
    })
}

/// Reads DTCs using UDS `0x19` first, falling back to OBD-II Mode `03`/`07`.
pub fn read_dtcs(device: &mut dyn Device) -> Result<DtcReadOutcome, QuickDiagError> {
    read_dtcs_for_can_id(device, 0x0000_07E0)
}

/// Reads DTCs from a specific ECU TX CAN ID using UDS `0x19` or OBD-II Mode `03`/`07`.
pub fn read_dtcs_for_can_id(
    device: &mut dyn Device,
    tx_can_id: u32,
) -> Result<DtcReadOutcome, QuickDiagError> {
    let proto = ensure_connected(device)?;
    prepare_diag_target(device, proto, tx_can_id)?;
    let mut notes = Vec::new();

    let uds_req = BusSilencingInterlock::build_read_dtc_by_status_mask(0xFF);
    match exchange_diag_target(device, proto, tx_can_id, &uds_req) {
        Ok(resp) if resp.len() >= 2 && resp[0] == 0x59 && resp[1] == 0x02 => {
            let parsed = BusSilencingInterlock::parse_dtc_list_response(&resp)
                .map_err(|_| QuickDiagError::UnexpectedResponse {
                    expected: 0x59,
                    got: resp.first().copied(),
                })?;
            let dtcs = parsed
                .into_iter()
                .filter(|(code, _)| code != "P0000")
                .map(|(code, status)| {
                    let desc = lookup_dtc_description(&code).summary;
                    DtcRecord {
                        code,
                        status_mask: status,
                        status_label: uds_status_label(status),
                        source: "uds_0x19".to_string(),
                        description: Some(desc),
                    }
                })
                .collect();
            return Ok(DtcReadOutcome {
                dtcs,
                protocol_used: "UDS 0x19 (ReadDTCInformation)".to_string(),
                notes,
            });
        }
        Ok(resp) => {
            if let Err(err) = check_negative_response(&resp) {
                notes.push(format!("UDS 0x19 unavailable ({err}); trying OBD Mode 03/07."));
            } else {
                notes.push("Unexpected UDS 0x19 response; trying OBD Mode 03/07.".to_string());
            }
        }
        Err(err) => notes.push(format!("UDS 0x19 failed ({err}); trying OBD Mode 03/07.")),
    }

    let mode03_resp = exchange_diag_target(device, proto, tx_can_id, &[0x03])?;
    let mut dtcs = parse_obd_dtc_response(&mode03_resp, 0x43, 0x08, "confirmed", "obd_mode03")?;

    match exchange_diag_target(device, proto, tx_can_id, &[0x07]) {
        Ok(mode07_resp) => match parse_obd_dtc_response(
            &mode07_resp,
            0x47,
            0x04,
            "pending",
            "obd_mode07",
        ) {
            Ok(pending) => {
                for rec in pending {
                    if !dtcs.iter().any(|existing| existing.code == rec.code) {
                        dtcs.push(rec);
                    }
                }
            }
            Err(err) => notes.push(format!("Pending DTC scan (Mode 07) skipped: {err}")),
        },
        Err(err) => notes.push(format!("Pending DTC scan (Mode 07) unavailable: {err}")),
    }

    Ok(DtcReadOutcome {
        dtcs,
        protocol_used: "OBD-II Mode 03/07".to_string(),
        notes,
    })
}

/// Clears fault codes using UDS `0x14` first, falling back to OBD-II Mode `04`.
pub fn clear_dtcs(device: &mut dyn Device) -> Result<DtcClearOutcome, QuickDiagError> {
    clear_dtcs_for_can_id(device, 0x0000_07E0)
}

/// Clears fault codes on a specific ECU TX CAN ID using UDS `0x14` or OBD-II Mode `04`.
pub fn clear_dtcs_for_can_id(
    device: &mut dyn Device,
    tx_can_id: u32,
) -> Result<DtcClearOutcome, QuickDiagError> {
    let proto = ensure_connected(device)?;
    prepare_diag_target(device, proto, tx_can_id)?;
    let mut notes = Vec::new();

    let uds_clear = BusSilencingInterlock::build_clear_all_dtcs();
    match exchange_diag_target(device, proto, tx_can_id, &uds_clear) {
        Ok(resp) if resp.first().copied() == Some(0x54) => {
            return Ok(DtcClearOutcome {
                cleared: true,
                protocol_used: "UDS 0x14 (ClearDiagnosticInformation)".to_string(),
                notes,
            });
        }
        Ok(resp) => {
            if let Err(err) = check_negative_response(&resp) {
                notes.push(format!("UDS 0x14 unavailable ({err}); trying OBD Mode 04."));
            } else {
                notes.push("Unexpected UDS 0x14 response; trying OBD Mode 04.".to_string());
            }
        }
        Err(err) => notes.push(format!("UDS 0x14 failed ({err}); trying OBD Mode 04.")),
    }

    let mode04_resp = exchange_diag_target(device, proto, tx_can_id, &[0x04])?;
    check_negative_response(&mode04_resp)?;
    if mode04_resp.first().copied() != Some(0x44) {
        return Err(QuickDiagError::UnexpectedResponse {
            expected: 0x44,
            got: mode04_resp.first().copied(),
        });
    }

    Ok(DtcClearOutcome {
        cleared: true,
        protocol_used: "OBD-II Mode 04".to_string(),
        notes,
    })
}

/// Requests an ECU reset (`Hard`, `KeyOffOn`, or `Soft`) via UDS service `0x11`.
pub fn ecu_reset(device: &mut dyn Device, mode: EcuResetMode) -> Result<EcuResetOutcome, QuickDiagError> {
    ecu_reset_for_can_id(device, 0x0000_07E0, mode)
}

/// Requests a UDS service `0x11` ECU reset against a specific ECU TX CAN ID.
pub fn ecu_reset_for_can_id(
    device: &mut dyn Device,
    tx_can_id: u32,
    mode: EcuResetMode,
) -> Result<EcuResetOutcome, QuickDiagError> {
    let proto = ensure_connected(device)?;
    prepare_diag_target(device, proto, tx_can_id)?;
    let resp = exchange_diag_target(device, proto, tx_can_id, &[0x11, mode.subfunction()])?;
    check_negative_response(&resp)?;

    if resp.len() < 2 || resp[0] != 0x51 {
        return Err(QuickDiagError::UnexpectedResponse {
            expected: 0x51,
            got: resp.first().copied(),
        });
    }

    let mut notes = vec![format!("ECU acknowledged {}.", mode.label())];
    let power_down_time_secs = resp.get(2).copied();
    if let Some(secs) = power_down_time_secs {
        notes.push(format!("ECU reported power-down window: {secs}s."));
    }

    Ok(EcuResetOutcome {
        acknowledged: true,
        mode,
        power_down_time_secs,
        notes,
    })
}
