//! Real ELM327 / STN1110 / OBDLink adapter over Serial/USB/Bluetooth SPP and Wi-Fi TCP.
//!
//! Uses a shared AT command bridge core over any `Read + Write` stream together with
//! [`crate::elm_device::ElmDevice`] parsers. Hardened for low-cost ELM327 clones
//! (auto-baud `38400`/`115200`/`9600`, NUL-byte filtering during reset, `ATS0` fallback,
//! echoed-command tolerance, extended search/K-Line timeouts, and capability probing).
//! Supports live telemetry, ECU identification, DTC/UDS diagnostics, and read-only UDS `0x23`.

use crate::device::{Device, DeviceError, DeviceInfo, DeviceKind, TransportKind};
use crate::elm_device::{ElmDevice, ElmResponse, ElmStatus};
use crate::j2534_types::{PassThruMsg, ProtocolId};
use serde::{Deserialize, Serialize};
use serialport::{DataBits, FlowControl, Parity, StopBits};
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

/// Standard ELM327 serial baud probe order (clones/Bluetooth SPP default to 38,400;
/// genuine STN/OBDLink and flashed clones default to 115,200; legacy UARTs use 9,600).
pub const ELM_AUTO_BAUD_CANDIDATES: &[u32] = &[38_400, 115_200, 9_600, 230_400, 500_000];

/// Default TCP endpoint used by generic ELM327 Wi-Fi adapters.
pub const DEFAULT_ELM_WIFI_ENDPOINT: &str = "192.168.0.10:35000";

/// Selectable vehicle bus protocol profile for ELM327 adapters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ElmBusProfile {
    /// Automatic OBD-II protocol search (`ATSP0`).
    #[serde(rename = "auto")]
    #[default]
    Auto,
    /// ISO 15765-4 CAN 11-bit ID, 500 kbps (`ATSP6` - most 2008+ cars & Euro 5 bikes).
    #[serde(rename = "can_11bit_500k")]
    Can11Bit500k,
    /// ISO 15765-4 CAN 29-bit ID, 500 kbps (`ATSP7` - Honda CAN / select Euro 5 bikes).
    #[serde(rename = "can_29bit_500k")]
    Can29Bit500k,
    /// ISO 15765-4 CAN 11-bit ID, 250 kbps (`ATSP8`).
    #[serde(rename = "can_11bit_250k")]
    Can11Bit250k,
    /// ISO 15765-4 CAN 29-bit ID, 250 kbps (`ATSP9`).
    #[serde(rename = "can_29bit_250k")]
    Can29Bit250k,
    /// ISO 14230-4 KWP2000 Fast Init, 10.4 kbps (`ATSP5` - Yamaha 3/4-pin OBD & K-Line cars).
    #[serde(rename = "kwp_fast_init")]
    KwpFastInit,
    /// ISO 14230-4 KWP2000 5-Baud Init, 10.4 kbps (`ATSP4`).
    #[serde(rename = "kwp_5baud_init")]
    Kwp5BaudInit,
    /// ISO 9141-2 5-Baud Init, 10.4 kbps (`ATSP3` - older K-Line vehicles).
    #[serde(rename = "iso9141")]
    Iso9141,
    /// SAE J1850 PWM 41.6 kbps (`ATSP1`).
    #[serde(rename = "j1850_pwm")]
    J1850Pwm,
    /// SAE J1850 VPW 10.4 kbps (`ATSP2`).
    #[serde(rename = "j1850_vpw")]
    J1850Vpw,
    /// Kawasaki KDS KWP2000 K-Line (`ATSP5` + `ATSH 81 11 F1` + optional wakeup).
    #[serde(rename = "kawasaki_kds")]
    KawasakiKds,
    /// Suzuki SDS KWP2000 K-Line (`ATSP5` + `ATSH 81 12 F1` + optional wakeup).
    #[serde(rename = "suzuki_sds")]
    SuzukiSds,
    /// Experimental Honda 4-pin Keihin K-Line via ELM327 (`ATSP5` + `ATIB 10` + `ATKW0` + `ATSH 72`).
    /// For 100% reliable Honda 4-pin K-Line, prefer a raw FTDI KKL 409.1 cable.
    #[serde(rename = "honda_kline_experimental")]
    HondaKLineExperimental,
}

impl ElmBusProfile {
    /// Maps profile to ELM327 `ATSP` selection command.
    pub const fn atsp_command(self) -> &'static str {
        match self {
            Self::Auto => "ATSP0",
            Self::Can11Bit500k => "ATSP6",
            Self::Can29Bit500k => "ATSP7",
            Self::Can11Bit250k => "ATSP8",
            Self::Can29Bit250k => "ATSP9",
            Self::KwpFastInit | Self::KawasakiKds | Self::SuzukiSds | Self::HondaKLineExperimental => {
                "ATSP5"
            }
            Self::Kwp5BaudInit => "ATSP4",
            Self::Iso9141 => "ATSP3",
            Self::J1850Pwm => "ATSP1",
            Self::J1850Vpw => "ATSP2",
        }
    }

    /// Maps profile to J2534 [`ProtocolId`] expected by upper diagnostic layers.
    pub const fn protocol_id(self) -> ProtocolId {
        match self {
            Self::Auto
            | Self::Can11Bit500k
            | Self::Can29Bit500k
            | Self::Can11Bit250k
            | Self::Can29Bit250k => ProtocolId::Iso15765,
            Self::KwpFastInit
            | Self::Kwp5BaudInit
            | Self::KawasakiKds
            | Self::SuzukiSds
            | Self::HondaKLineExperimental => ProtocolId::Iso14230,
            Self::Iso9141 => ProtocolId::Iso9141,
            Self::J1850Pwm => ProtocolId::J1850Pwm,
            Self::J1850Vpw => ProtocolId::J1850Vpw,
        }
    }

    /// Target vehicle bus bitrate in bps.
    pub const fn bus_baud_rate(self) -> u32 {
        match self {
            Self::Auto | Self::Can11Bit500k | Self::Can29Bit500k => 500_000,
            Self::Can11Bit250k | Self::Can29Bit250k => 250_000,
            Self::KwpFastInit
            | Self::Kwp5BaudInit
            | Self::Iso9141
            | Self::J1850Vpw
            | Self::KawasakiKds
            | Self::SuzukiSds
            | Self::HondaKLineExperimental => 10_400,
            Self::J1850Pwm => 41_600,
        }
    }

    /// True for 29-bit CAN extended-ID profiles.
    pub const fn is_29bit(self) -> bool {
        matches!(self, Self::Can29Bit500k | Self::Can29Bit250k)
    }

    /// Additional AT setup commands required after `ATSP` (best-effort on clones).
    pub const fn custom_init_commands(self) -> &'static [&'static str] {
        match self {
            Self::KawasakiKds => &["ATSH 81 11 F1", "ATWM 81 11 F1 3E"],
            Self::SuzukiSds => &["ATSH 81 12 F1", "ATWM 81 12 F1 3E"],
            Self::HondaKLineExperimental => &["ATIB 10", "ATKW0", "ATSH 72"],
            _ => &[],
        }
    }
}

/// Hardware & firmware capability probe result for an attached ELM327 / STN adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElmCapabilityReport {
    pub adapter_banner: String,
    pub chip_tier: String,
    pub active_protocol: String,
    pub supports_atsh: bool,
    pub supports_atcaf0: bool,
    pub supports_kline_atsp5: bool,
    pub supports_atbi: bool,
    pub supports_st_commands: bool,
    pub warnings: Vec<String>,
}

/// Normalizes user-supplied ELM327 Wi-Fi host/port strings, defaulting to `192.168.0.10:35000`.
pub fn normalize_elm_wifi_endpoint(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return DEFAULT_ELM_WIFI_ENDPOINT.to_string();
    }
    if trimmed.starts_with('[') || trimmed.matches(':').count() == 1 {
        trimmed.to_string()
    } else if trimmed.matches(':').count() > 1 {
        format!("[{trimmed}]:35000")
    } else {
        format!("{trimmed}:35000")
    }
}

pub fn elm_can_protocol_cmd(baud_rate: u32, is_29bit: bool) -> &'static str {
    match (baud_rate, is_29bit) {
        (250_000, false) => "ATSP8",
        (250_000, true) => "ATSP9",
        (_, true) => "ATSP7",
        (_, false) => "ATSP6",
    }
}

pub fn elm_header_commands(tx_id: u32) -> Vec<String> {
    if tx_id > 0x7FF {
        vec![
            format!("ATCP {:02X}", (tx_id >> 24) & 0x1F),
            format!("ATSH {:06X}", tx_id & 0x00FF_FFFF),
        ]
    } else {
        vec![format!("ATSH {:03X}", tx_id)]
    }
}

pub fn elm_rx_filter_command(rx_id: u32) -> String {
    if rx_id > 0x7FF {
        format!("ATCRA {:08X}", rx_id)
    } else {
        format!("ATCRA {:03X}", rx_id)
    }
}

pub fn derive_elm_rx_can_id(tx_id: u32) -> u32 {
    if tx_id == 0x7DF {
        0x7E8
    } else if (tx_id >> 16) == 0x18DA {
        (tx_id & 0xFFFF_0000) | ((tx_id & 0xFF) << 8) | ((tx_id >> 8) & 0xFF)
    } else {
        tx_id.wrapping_add(8)
    }
}

trait ElmStream: Read + Write + Send {}
impl<T: Read + Write + Send + ?Sized> ElmStream for T {}

fn strip_echo_line(raw: &str, sent_cmd: &str) -> String {
    let expected = sent_cmd.trim().to_ascii_uppercase();
    let expected_compact: String = expected.chars().filter(|c| !c.is_whitespace()).collect();
    let sanitized: String = raw
        .chars()
        .filter(|&c| c == '\r' || c == '\n' || c == '\t' || (' '..='~').contains(&c))
        .collect();
    let mut lines: Vec<&str> = sanitized
        .split(['\r', '\n'])
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();
    if let Some(first) = lines.first() {
        let first_upper = first.to_ascii_uppercase();
        let first_compact: String = first_upper.chars().filter(|c| !c.is_whitespace()).collect();
        if first_upper == expected || (!expected_compact.is_empty() && first_compact == expected_compact)
        {
            lines.remove(0);
        }
    }
    lines.join("\r")
}

/// Shared ELM327 AT-bridge session logic reusable across serial (USB/BT SPP) and TCP (Wi-Fi).
struct ElmBridgeCore {
    stream: Option<Box<dyn ElmStream>>,
    protocol: Option<ProtocolId>,
    bus_profile: Option<ElmBusProfile>,
    adapter_id: Option<String>,
    current_tx_can_id: Option<u32>,
    configured_rx_can_id: Option<u32>,
    can_baud_rate: u32,
    can_is_29bit: bool,
    first_bus_tx: bool,
    clone_warnings: Vec<String>,
    rx_queue: VecDeque<PassThruMsg>,
}

impl ElmBridgeCore {
    fn new(bus_profile: Option<ElmBusProfile>) -> Self {
        Self {
            stream: None,
            protocol: None,
            bus_profile,
            adapter_id: None,
            current_tx_can_id: None,
            configured_rx_can_id: None,
            can_baud_rate: bus_profile.map_or(500_000, ElmBusProfile::bus_baud_rate),
            can_is_29bit: bus_profile.is_some_and(ElmBusProfile::is_29bit),
            first_bus_tx: true,
            clone_warnings: Vec::new(),
            rx_queue: VecDeque::with_capacity(32),
        }
    }

    fn drain_wakeup_prompt(&mut self, wait_ms: u64) {
        let Some(stream) = self.stream.as_mut() else {
            return;
        };
        let _ = stream.write_all(b"\r");
        let _ = stream.flush();
        std::thread::sleep(Duration::from_millis(60));
        let deadline = Instant::now() + Duration::from_millis(wait_ms);
        let mut scratch = [0u8; 128];
        let mut saw_any = false;
        while Instant::now() < deadline {
            match stream.read(&mut scratch) {
                Ok(n) if n > 0 => {
                    saw_any = true;
                    if scratch[..n].contains(&b'>') {
                        break;
                    }
                }
                Ok(_) => break,
                Err(ref e)
                    if e.kind() == std::io::ErrorKind::TimedOut
                        || e.kind() == std::io::ErrorKind::WouldBlock =>
                {
                    if !saw_any {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    }

    fn initialize_stream(
        &mut self,
        stream: Box<dyn ElmStream>,
        atz_timeout_ms: u64,
        send_wakeup: bool,
    ) -> Result<(), DeviceError> {
        self.stream = Some(stream);
        self.clone_warnings.clear();

        if send_wakeup {
            self.drain_wakeup_prompt(150);
        }

        let reset_rep = match self.send_at("ATZ", atz_timeout_ms) {
            Ok(r) => r,
            Err(e) => {
                self.stream = None;
                return Err(e);
            }
        };
        if !reset_rep.text.is_empty() && reset_rep.status != ElmStatus::Error {
            self.adapter_id = Some(reset_rep.text);
        } else if reset_rep.status == ElmStatus::Error {
            self.stream = None;
            return Err(DeviceError::Transport(format!(
                "ELM rejected ATZ reset: {}",
                reset_rep.text
            )));
        }

        for cmd in ["ATE0", "ATL0", "ATH0"] {
            match self.send_at(cmd, 1200) {
                Ok(rep) if rep.status == ElmStatus::Error => {
                    self.clone_warnings.push(format!(
                        "Clone adapter rejected '{cmd}' ({}); continuing with echo/header tolerance.",
                        rep.text
                    ));
                }
                Ok(_) => {}
                Err(e) => {
                    self.stream = None;
                    return Err(e);
                }
            }
        }

        // Prefer compact hex (`ATS0`); fall back gracefully to `ATS1` on picky clones
        match self.send_at("ATS0", 1000) {
            Ok(rep) if rep.status == ElmStatus::Error => {
                self.clone_warnings
                    .push("Adapter rejected ATS0; falling back to spaced output (ATS1).".into());
                let _ = self.send_at("ATS1", 1000);
            }
            Ok(_) => {}
            Err(e) => {
                self.stream = None;
                return Err(e);
            }
        }

        // Allow long (>7 byte) OBD/UDS messages; older/fake v2.1 clones may answer '?'
        if let Ok(rep) = self.send_at("ATAL", 800) {
            if rep.status == ElmStatus::Error {
                self.clone_warnings
                    .push("Adapter does not support ATAL (long frame allowance).".into());
            }
        }

        // Apply initial protocol search mode (`ATSP0` unless caller pinned another profile)
        let init_atsp = self
            .bus_profile
            .map_or("ATSP0", ElmBusProfile::atsp_command);
        let _ = self.send_at(init_atsp, 1000);

        self.current_tx_can_id = None;
        self.configured_rx_can_id = None;
        self.first_bus_tx = true;
        Ok(())
    }

    fn send_at(&mut self, cmd: &str, timeout_ms: u64) -> Result<ElmResponse, DeviceError> {
        let raw = self.exchange_raw(&ElmDevice::encode(cmd), timeout_ms)?;
        let stripped = strip_echo_line(&raw, cmd);
        Ok(ElmResponse::parse(&stripped))
    }

    fn exchange_raw(&mut self, cmd_with_cr: &str, timeout_ms: u64) -> Result<String, DeviceError> {
        let stream = self.stream.as_mut().ok_or(DeviceError::NotOpen)?;
        stream
            .write_all(cmd_with_cr.as_bytes())
            .map_err(|e| DeviceError::Transport(format!("ELM stream write error: {e}")))?;
        let _ = stream.flush();

        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        let mut buf = Vec::with_capacity(256);
        let mut chunk = [0u8; 128];
        let mut eof_reads = 0u8;

        while Instant::now() < deadline {
            match stream.read(&mut chunk) {
                Ok(0) => {
                    eof_reads = eof_reads.saturating_add(1);
                    if eof_reads >= 4 {
                        if buf.contains(&b'>') {
                            return Ok(String::from_utf8_lossy(&buf).to_string());
                        }
                        return Err(DeviceError::Transport(
                            "ELM transport closed unexpectedly (EOF)".to_string(),
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                Ok(n) => {
                    eof_reads = 0;
                    for &b in &chunk[..n] {
                        if b != 0x00 {
                            buf.push(b);
                        }
                    }
                    if buf.contains(&b'>') {
                        return Ok(String::from_utf8_lossy(&buf).to_string());
                    }
                }
                Err(ref e)
                    if e.kind() == std::io::ErrorKind::TimedOut
                        || e.kind() == std::io::ErrorKind::WouldBlock =>
                {
                    eof_reads = 0;
                }
                Err(e) => {
                    return Err(DeviceError::Transport(format!("ELM stream read error: {e}")));
                }
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        Err(DeviceError::Timeout)
    }

    fn close(&mut self) -> Result<(), DeviceError> {
        if self.stream.is_some() {
            let _ = self.send_at("ATPC", 300);
        }
        self.stream = None;
        self.protocol = None;
        self.current_tx_can_id = None;
        self.configured_rx_can_id = None;
        self.can_is_29bit = false;
        self.first_bus_tx = true;
        self.rx_queue.clear();
        Ok(())
    }

    fn connect(&mut self, protocol: ProtocolId, baud_rate: u32) -> Result<(), DeviceError> {
        if self.stream.is_none() {
            return Err(DeviceError::NotOpen);
        }
        self.rx_queue.clear();
        self.first_bus_tx = true;
        self.current_tx_can_id = None;

        if let Some(profile) = self.bus_profile {
            let atsp = profile.atsp_command();
            let rep = self.send_at(atsp, 1200)?;
            if !rep.is_ok() && rep.status == ElmStatus::Error {
                return Err(DeviceError::Transport(format!(
                    "ELM rejected protocol select '{atsp}': {}",
                    rep.text
                )));
            }
            for &extra in profile.custom_init_commands() {
                let erep = self.send_at(extra, 1000)?;
                if erep.status == ElmStatus::Error {
                    self.clone_warnings.push(format!(
                        "Adapter rejected '{extra}' for profile {:?}: {}",
                        profile, erep.text
                    ));
                }
            }
            let effective_proto = profile.protocol_id();
            self.can_baud_rate = profile.bus_baud_rate();
            self.can_is_29bit = profile.is_29bit();
            if matches!(effective_proto, ProtocolId::Iso15765) {
                if let Ok(caf) = self.send_at("ATCAF1", 800) {
                    if caf.status == ElmStatus::Error {
                        self.clone_warnings.push(
                            "Adapter rejected ATCAF1; relying on default ISO-TP auto-formatting."
                                .into(),
                        );
                    }
                }
            }
            self.protocol = Some(effective_proto);
            return Ok(());
        }

        self.can_baud_rate = if baud_rate > 0 { baud_rate } else { 500_000 };
        self.can_is_29bit = false;
        let atsp = match protocol {
            ProtocolId::Iso15765 | ProtocolId::Can => elm_can_protocol_cmd(self.can_baud_rate, false),
            ProtocolId::Iso14230 => "ATSP5",
            ProtocolId::Iso9141 => "ATSP3",
            ProtocolId::J1850Pwm => "ATSP1",
            ProtocolId::J1850Vpw => "ATSP2",
            _ => "ATSP0",
        };
        let rep = self.send_at(atsp, 1200)?;
        if !rep.is_ok() && rep.status == ElmStatus::Error {
            return Err(DeviceError::Transport(format!(
                "ELM rejected protocol select '{atsp}': {}",
                rep.text
            )));
        }
        if protocol == ProtocolId::Can {
            let caf = self.send_at("ATCAF0", 800)?;
            if caf.status == ElmStatus::Error {
                return Err(DeviceError::Transport(format!(
                    "ELM rejected raw CAN ATCAF0: {}",
                    caf.text
                )));
            }
        } else if protocol == ProtocolId::Iso15765 {
            if let Ok(caf) = self.send_at("ATCAF1", 800) {
                if caf.status == ElmStatus::Error {
                    self.clone_warnings.push(
                        "Adapter rejected ATCAF1; relying on default ISO-TP auto-formatting.".into(),
                    );
                }
            }
        }
        self.protocol = Some(protocol);
        Ok(())
    }

    fn disconnect(&mut self) -> Result<(), DeviceError> {
        if self.stream.is_none() {
            return Err(DeviceError::NotOpen);
        }
        let _ = self.send_at("ATPC", 500);
        self.protocol = None;
        self.current_tx_can_id = None;
        self.configured_rx_can_id = None;
        self.can_is_29bit = false;
        self.first_bus_tx = true;
        self.rx_queue.clear();
        Ok(())
    }

    fn apply_can_ids(&mut self, tx_id: u32, rx_id: u32) -> Result<(), DeviceError> {
        self.configured_rx_can_id = Some(rx_id);
        if self.stream.is_none() || self.protocol.is_none() {
            self.current_tx_can_id = None;
            return Ok(());
        }
        let proto = self.protocol.unwrap_or(ProtocolId::Iso15765);
        if !(proto.is_iso15765() || proto == ProtocolId::Can) {
            self.current_tx_can_id = Some(tx_id);
            return Ok(());
        }

        let wants_29 = tx_id > 0x7FF || rx_id > 0x7FF;
        if wants_29 != self.can_is_29bit && self.bus_profile != Some(ElmBusProfile::Auto) {
            let proto_cmd = elm_can_protocol_cmd(self.can_baud_rate, wants_29);
            let sp_rep = self.send_at(proto_cmd, 1000)?;
            if sp_rep.status == ElmStatus::Error {
                return Err(DeviceError::Transport(format!(
                    "ELM rejected '{proto_cmd}': {}",
                    sp_rep.text
                )));
            }
            self.can_is_29bit = wants_29;
        }
        for hdr_cmd in elm_header_commands(tx_id) {
            let hrep = self.send_at(&hdr_cmd, 800)?;
            if hrep.status == ElmStatus::Error {
                return Err(DeviceError::Transport(format!(
                    "ELM rejected header command '{hdr_cmd}': {}",
                    hrep.text
                )));
            }
        }
        let cra_cmd = elm_rx_filter_command(rx_id);
        if let Ok(rep) = self.send_at(&cra_cmd, 800) {
            if rep.status == ElmStatus::Error {
                self.clone_warnings
                    .push(format!("Adapter rejected '{cra_cmd}'; using default CAN RX mask."));
            }
        }
        self.current_tx_can_id = Some(tx_id);
        Ok(())
    }

    fn send(&mut self, msg: &PassThruMsg) -> Result<(), DeviceError> {
        if self.stream.is_none() {
            return Err(DeviceError::NotOpen);
        }
        let proto = self.protocol.ok_or(DeviceError::NotConnected)?;
        let can_prefixed = proto.is_iso15765() || proto == ProtocolId::Can;
        let (tx_id, body) = if can_prefixed {
            if msg.data.len() < 4 {
                return Err(DeviceError::Transport(
                    "CAN/ISO15765 frame must carry 4-byte CAN arbitration ID prefix".to_string(),
                ));
            }
            let tx_id = u32::from_be_bytes([msg.data[0], msg.data[1], msg.data[2], msg.data[3]]);
            let wants_29 = tx_id > 0x7FF;
            if wants_29 != self.can_is_29bit && self.bus_profile != Some(ElmBusProfile::Auto) {
                let proto_cmd = elm_can_protocol_cmd(self.can_baud_rate, wants_29);
                let sp_rep = self.send_at(proto_cmd, 1000)?;
                if sp_rep.status == ElmStatus::Error {
                    return Err(DeviceError::Transport(format!(
                        "ELM rejected '{proto_cmd}': {}",
                        sp_rep.text
                    )));
                }
                self.can_is_29bit = wants_29;
                self.current_tx_can_id = None;
            }
            if self.current_tx_can_id != Some(tx_id) {
                for hdr_cmd in elm_header_commands(tx_id) {
                    let hrep = self.send_at(&hdr_cmd, 800)?;
                    if hrep.status == ElmStatus::Error {
                        return Err(DeviceError::Transport(format!(
                            "ELM rejected header command '{hdr_cmd}': {}",
                            hrep.text
                        )));
                    }
                }
                self.current_tx_can_id = Some(tx_id);
            }
            (Some(tx_id), &msg.data[4..])
        } else {
            (None, msg.data.as_slice())
        };

        let hex_payload: String = body.iter().map(|b| format!("{b:02X}")).collect();
        let timeout_ms = if self.first_bus_tx
            || self.bus_profile == Some(ElmBusProfile::Auto)
            || matches!(proto, ProtocolId::Iso14230 | ProtocolId::Iso9141)
        {
            4_800
        } else {
            1_500
        };
        let raw = self.exchange_raw(&format!("{hex_payload}\r"), timeout_ms)?;
        let stripped = strip_echo_line(&raw, &hex_payload);
        let rep = ElmResponse::parse(&stripped);
        if rep.status == ElmStatus::NoData || rep.status == ElmStatus::Stopped {
            return Err(DeviceError::Timeout);
        }
        if rep.status == ElmStatus::Error {
            return Err(DeviceError::Transport(format!(
                "ELM error for TX '{hex_payload}': {}",
                rep.text
            )));
        }
        self.first_bus_tx = false;
        if !rep.bytes.is_empty() {
            let rx_data = if let Some(tx_id) = tx_id {
                let rx_id = self
                    .configured_rx_can_id
                    .unwrap_or_else(|| derive_elm_rx_can_id(tx_id));
                let mut v = Vec::with_capacity(4 + rep.bytes.len());
                v.extend_from_slice(&rx_id.to_be_bytes());
                v.extend_from_slice(&rep.bytes);
                v
            } else {
                rep.bytes
            };
            self.rx_queue.push_back(PassThruMsg::new(proto, rx_data));
        }
        Ok(())
    }

    fn receive(&mut self) -> Result<PassThruMsg, DeviceError> {
        if self.stream.is_none() {
            return Err(DeviceError::NotOpen);
        }
        if self.protocol.is_none() {
            return Err(DeviceError::NotConnected);
        }
        self.rx_queue.pop_front().ok_or(DeviceError::Timeout)
    }

    fn read_voltage_millivolts(&mut self) -> Result<u32, DeviceError> {
        if self.stream.is_none() {
            return Err(DeviceError::NotOpen);
        }
        let rep = self.send_at("ATRV", 1200)?;
        if rep.status != ElmStatus::Ok {
            return Err(DeviceError::Transport(format!(
                "ELM327 ATRV failed with status {:?}",
                rep.status
            )));
        }
        let clean: String = rep
            .text
            .chars()
            .filter(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        let volts = clean.parse::<f64>().map_err(|_| {
            DeviceError::Transport(format!("ELM327 ATRV unparseable voltage: '{}'", rep.text))
        })?;
        if !volts.is_finite() || volts <= 0.0 || volts > 40.0 {
            return Err(DeviceError::Transport(format!(
                "ELM327 ATRV out-of-range voltage reading: {volts}V"
            )));
        }
        Ok((volts * 1000.0).round() as u32)
    }

    fn kline_wakeup(&mut self) -> Result<(), DeviceError> {
        if self.stream.is_none() {
            return Err(DeviceError::NotOpen);
        }
        let cmd = match self.bus_profile {
            Some(ElmBusProfile::HondaKLineExperimental) => "ATBI",
            _ => "ATFI",
        };
        let rep = self.send_at(cmd, 3000)?;
        if rep.status == ElmStatus::Error {
            return Err(DeviceError::Transport(format!(
                "ELM K-Line init '{cmd}' failed: {}",
                rep.text
            )));
        }
        Ok(())
    }

    fn send_raw_command(&mut self, cmd: &str) -> Result<String, DeviceError> {
        if self.stream.is_none() {
            return Err(DeviceError::NotOpen);
        }
        let trimmed = cmd.trim();
        let is_at = trimmed.len() >= 2
            && (trimmed[..2].eq_ignore_ascii_case("AT")
                || trimmed[..2].eq_ignore_ascii_case("ST"));
        let timeout_ms = if is_at { 1800 } else { 4800 };
        let raw = self.exchange_raw(&ElmDevice::encode(trimmed), timeout_ms)?;
        Ok(strip_echo_line(&raw, trimmed))
    }
}

/// Real serial / USB / Bluetooth SPP ELM327 / STN adapter implementing [`Device`].
pub struct SerialElmDevice {
    port_path: String,
    baud_rate: u32,
    core: ElmBridgeCore,
    is_bluetooth: bool,
}

impl Drop for SerialElmDevice {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

impl SerialElmDevice {
    pub fn new(port_path: impl Into<String>, baud_rate: u32) -> Self {
        let port_path = port_path.into();
        let lower = port_path.to_ascii_lowercase();
        let is_bluetooth = lower.contains("rfcomm") || lower.contains("bluetooth");
        Self {
            port_path,
            baud_rate,
            core: ElmBridgeCore::new(None),
            is_bluetooth,
        }
    }

    /// Selects a specific [`ElmBusProfile`] (for example `Auto`, `Can29Bit500k`, or `KwpFastInit`).
    pub fn with_bus_profile(mut self, profile: ElmBusProfile) -> Self {
        self.core.bus_profile = Some(profile);
        self.core.can_baud_rate = profile.bus_baud_rate();
        self.core.can_is_29bit = profile.is_29bit();
        self
    }

    /// Flags whether this serial port backs a Bluetooth SPP dongle.
    pub fn with_bluetooth_hint(mut self, is_bt: bool) -> Self {
        self.is_bluetooth = is_bt;
        self
    }

    pub fn detected_baud_rate(&self) -> u32 {
        self.baud_rate
    }

    pub fn bus_profile(&self) -> Option<ElmBusProfile> {
        self.core.bus_profile
    }

    pub fn clone_warnings(&self) -> &[String] {
        &self.core.clone_warnings
    }

    fn open_at_baud(&mut self, atz_timeout_ms: u64) -> Result<(), DeviceError> {
        if self.core.stream.is_some() {
            return Err(DeviceError::AlreadyOpen);
        }
        let port = serialport::new(&self.port_path, self.baud_rate)
            .data_bits(DataBits::Eight)
            .parity(Parity::None)
            .stop_bits(StopBits::One)
            .flow_control(FlowControl::None)
            .timeout(Duration::from_millis(50))
            .open()
            .map_err(|e| DeviceError::NotFound(format!("{}: {e}", self.port_path)))?;
        let _ = port.clear(serialport::ClearBuffer::Input);
        self.core.initialize_stream(Box::new(port), atz_timeout_ms, true)
    }

    /// Opens an ELM327 serial port by automatically probing [`ELM_AUTO_BAUD_CANDIDATES`]
    /// (`38400`, `115200`, `9600`, `230400`, `500000`) until `ATZ` responds cleanly.
    pub fn open_auto_baud(
        port_path: impl Into<String>,
        profile: Option<ElmBusProfile>,
    ) -> Result<(Self, u32), DeviceError> {
        let port_str = port_path.into();
        let mut last_err = DeviceError::Timeout;
        for &baud in ELM_AUTO_BAUD_CANDIDATES {
            let mut dev = Self::new(port_str.clone(), baud);
            if let Some(p) = profile {
                dev = dev.with_bus_profile(p);
            }
            match dev.open_at_baud(950) {
                Ok(()) => return Ok((dev, baud)),
                Err(e @ DeviceError::NotFound(_)) => return Err(e),
                Err(e) => {
                    let _ = dev.close();
                    last_err = e;
                }
            }
        }
        Err(last_err)
    }
}

impl Device for SerialElmDevice {
    fn open(&mut self) -> Result<(), DeviceError> {
        self.open_at_baud(2200)
    }

    fn close(&mut self) -> Result<(), DeviceError> {
        self.core.close()
    }

    fn connect(&mut self, protocol: ProtocolId, baud_rate: u32) -> Result<(), DeviceError> {
        self.core.connect(protocol, baud_rate)
    }

    fn disconnect(&mut self) -> Result<(), DeviceError> {
        self.core.disconnect()
    }

    fn send(&mut self, msg: &PassThruMsg) -> Result<(), DeviceError> {
        self.core.send(msg)
    }

    fn receive(&mut self) -> Result<PassThruMsg, DeviceError> {
        self.core.receive()
    }

    fn read_voltage_millivolts(&mut self) -> Result<u32, DeviceError> {
        self.core.read_voltage_millivolts()
    }

    fn info(&self) -> DeviceInfo {
        let proto_tag = self
            .core
            .bus_profile
            .map_or_else(String::new, |p| format!(", {:?}", p));
        DeviceInfo {
            kind: DeviceKind::Serial,
            name: format!("ELM327 Serial ({}, {} bps{})", self.port_path, self.baud_rate, proto_tag),
            transport: if self.is_bluetooth {
                TransportKind::Bluetooth
            } else {
                TransportKind::Serial
            },
            firmware_version: self.core.adapter_id.clone(),
            dll_version: Some("elm327-serial".to_string()),
        }
    }

    fn is_open(&self) -> bool {
        self.core.stream.is_some()
    }

    fn is_connected(&self) -> bool {
        self.core.protocol.is_some()
    }

    fn connected_protocol(&self) -> Option<ProtocolId> {
        self.core.protocol
    }

    fn is_mock(&self) -> bool {
        false
    }

    fn supports_flashing(&self) -> bool {
        false
    }

    fn configure_can_ids(&mut self, tx_id: u32, rx_id: u32) -> Result<(), DeviceError> {
        self.core.apply_can_ids(tx_id, rx_id)
    }

    fn kline_wakeup_pulse(&mut self, _low_ms: u64, _high_ms: u64) -> Result<(), DeviceError> {
        self.core.kline_wakeup()
    }

    fn send_raw_command(&mut self, cmd: &str) -> Result<String, DeviceError> {
        self.core.send_raw_command(cmd)
    }
}

/// Wireless Wi-Fi ELM327 / STN adapter over TCP socket (typically `192.168.0.10:35000`).
pub struct WifiElmDevice {
    endpoint: String,
    core: ElmBridgeCore,
}

impl Drop for WifiElmDevice {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

impl WifiElmDevice {
    pub fn new(endpoint: impl Into<String>) -> Self {
        let raw = endpoint.into();
        Self {
            endpoint: normalize_elm_wifi_endpoint(&raw),
            core: ElmBridgeCore::new(None),
        }
    }

    pub fn with_bus_profile(mut self, profile: ElmBusProfile) -> Self {
        self.core.bus_profile = Some(profile);
        self.core.can_baud_rate = profile.bus_baud_rate();
        self.core.can_is_29bit = profile.is_29bit();
        self
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn clone_warnings(&self) -> &[String] {
        &self.core.clone_warnings
    }
}

impl Device for WifiElmDevice {
    fn open(&mut self) -> Result<(), DeviceError> {
        if self.core.stream.is_some() {
            return Err(DeviceError::AlreadyOpen);
        }
        let addr = self
            .endpoint
            .to_socket_addrs()
            .map_err(|e| {
                DeviceError::NotFound(format!("invalid ELM327 Wi-Fi endpoint '{}': {e}", self.endpoint))
            })?
            .next()
            .ok_or_else(|| {
                DeviceError::NotFound(format!("ELM327 Wi-Fi endpoint '{}' resolved empty", self.endpoint))
            })?;
        let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(3)).map_err(|e| {
            DeviceError::Transport(format!("ELM327 Wi-Fi TCP connect to {} failed: {e}", self.endpoint))
        })?;
        let _ = stream.set_nodelay(true);
        let _ = stream.set_read_timeout(Some(Duration::from_millis(50)));
        let _ = stream.set_write_timeout(Some(Duration::from_millis(1500)));
        self.core.initialize_stream(Box::new(stream), 2200, true)
    }

    fn close(&mut self) -> Result<(), DeviceError> {
        self.core.close()
    }

    fn connect(&mut self, protocol: ProtocolId, baud_rate: u32) -> Result<(), DeviceError> {
        self.core.connect(protocol, baud_rate)
    }

    fn disconnect(&mut self) -> Result<(), DeviceError> {
        self.core.disconnect()
    }

    fn send(&mut self, msg: &PassThruMsg) -> Result<(), DeviceError> {
        self.core.send(msg)
    }

    fn receive(&mut self) -> Result<PassThruMsg, DeviceError> {
        self.core.receive()
    }

    fn read_voltage_millivolts(&mut self) -> Result<u32, DeviceError> {
        self.core.read_voltage_millivolts()
    }

    fn info(&self) -> DeviceInfo {
        DeviceInfo {
            kind: DeviceKind::ElmWifi,
            name: format!("ELM327 Wi-Fi ({})", self.endpoint),
            transport: TransportKind::Wifi,
            firmware_version: self.core.adapter_id.clone(),
            dll_version: Some("elm327-wifi-tcp".to_string()),
        }
    }

    fn is_open(&self) -> bool {
        self.core.stream.is_some()
    }

    fn is_connected(&self) -> bool {
        self.core.protocol.is_some()
    }

    fn connected_protocol(&self) -> Option<ProtocolId> {
        self.core.protocol
    }

    fn is_mock(&self) -> bool {
        false
    }

    fn supports_flashing(&self) -> bool {
        false
    }

    fn configure_can_ids(&mut self, tx_id: u32, rx_id: u32) -> Result<(), DeviceError> {
        self.core.apply_can_ids(tx_id, rx_id)
    }

    fn kline_wakeup_pulse(&mut self, _low_ms: u64, _high_ms: u64) -> Result<(), DeviceError> {
        self.core.kline_wakeup()
    }

    fn send_raw_command(&mut self, cmd: &str) -> Result<String, DeviceError> {
        self.core.send_raw_command(cmd)
    }
}

fn raw_at_ok(dev: &mut dyn Device, cmd: &str) -> (bool, String) {
    match dev.send_raw_command(cmd) {
        Ok(raw) => {
            let rep = ElmResponse::parse(&raw);
            let ok = rep.status != ElmStatus::Error && !rep.text.contains('?');
            (ok, rep.text)
        }
        Err(e) => (false, e.to_string()),
    }
}

/// Probes an attached ELM327/STN adapter to detect clone firmware limitations (`ATSH`,
/// `ATCAF0`, K-Line `ATTP5`, `ATBI`, and STN `STDI` commands) while restoring prior state.
pub fn probe_elm_capabilities(dev: &mut dyn Device) -> Result<ElmCapabilityReport, DeviceError> {
    if !dev.is_open() {
        return Err(DeviceError::NotOpen);
    }
    let mut warnings = Vec::new();
    let ati_raw = dev.send_raw_command("ATI")?;
    let banner_text = ElmResponse::parse(&ati_raw).text;
    let (_, prev_dpn) = raw_at_ok(dev, "ATDPN");
    let prev_proto_char = prev_dpn
        .trim()
        .trim_start_matches('A')
        .chars()
        .next()
        .filter(|c| c.is_ascii_hexdigit());

    let (st_ok, st_text) = raw_at_ok(dev, "STDI");
    let supports_st_commands = st_ok && !st_text.is_empty() && !st_text.eq_ignore_ascii_case("?");

    let (supports_atsh, _) = raw_at_ok(dev, "ATSH 7E0");
    let (supports_atcaf0, _) = raw_at_ok(dev, "ATCAF0");
    let _ = raw_at_ok(dev, "ATCAF1");

    let (supports_kline_atsp5, _) = raw_at_ok(dev, "ATTP5");
    let (supports_atbi, _) = raw_at_ok(dev, "ATBI");

    // Restore original active protocol selection so probing is non-destructive.
    if let Some(ch) = prev_proto_char {
        let _ = raw_at_ok(dev, &format!("ATTP{ch}"));
    } else {
        let _ = raw_at_ok(dev, "ATSP0");
    }

    let banner_upper = banner_text.to_ascii_uppercase();
    let chip_tier = if supports_st_commands || banner_upper.contains("STN") || banner_upper.contains("OBDLINK") {
        "stn_obdlink".to_string()
    } else if supports_atsh && supports_atcaf0 && supports_kline_atsp5 {
        "pic18_v15_compatible".to_string()
    } else {
        "limited_clone_v21".to_string()
    };

    if !supports_atsh || !supports_atcaf0 {
        warnings.push(
            "Clone rejected ATSH or ATCAF0: custom multi-ECU CAN IDs and motorcycle K-Line headers may fail."
                .to_string(),
        );
    }
    if !supports_kline_atsp5 {
        warnings.push(
            "Adapter rejected ISO 14230-4 KWP2000 (ATTP5); K-Line vehicles and motorcycles are unsupported on this clone."
                .to_string(),
        );
    }
    if !supports_atbi {
        warnings.push("Adapter rejected ATBI (bypass-init raw K-Line mode unsupported).".to_string());
    }

    Ok(ElmCapabilityReport {
        adapter_banner: if banner_text.is_empty() {
            "Unknown ELM adapter".to_string()
        } else {
            banner_text
        },
        chip_tier,
        active_protocol: prev_dpn,
        supports_atsh,
        supports_atcaf0,
        supports_kline_atsp5,
        supports_atbi,
        supports_st_commands,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;

    fn read_cr_command<R: Read>(reader: &mut BufReader<R>) -> String {
        let mut out = Vec::new();
        let _ = reader.read_until(b'\r', &mut out);
        String::from_utf8_lossy(&out).trim().to_string()
    }

    #[test]
    fn elm_protocol_and_header_commands_handle_11bit_29bit_and_baud() {
        assert_eq!(elm_can_protocol_cmd(500_000, false), "ATSP6");
        assert_eq!(elm_can_protocol_cmd(500_000, true), "ATSP7");
        assert_eq!(elm_can_protocol_cmd(250_000, false), "ATSP8");
        assert_eq!(elm_can_protocol_cmd(250_000, true), "ATSP9");

        assert_eq!(elm_header_commands(0x7E0), vec!["ATSH 7E0"]);
        assert_eq!(
            elm_header_commands(0x18DA_10F1),
            vec!["ATCP 18", "ATSH DA10F1"]
        );
        assert_eq!(elm_rx_filter_command(0x7E8), "ATCRA 7E8");
        assert_eq!(elm_rx_filter_command(0x18DA_F110), "ATCRA 18DAF110");

        assert_eq!(derive_elm_rx_can_id(0x7E0), 0x7E8);
        assert_eq!(derive_elm_rx_can_id(0x7DF), 0x7E8);
        assert_eq!(derive_elm_rx_can_id(0x18DA_10F1), 0x18DA_F110);
    }

    #[test]
    fn normalize_wifi_endpoint_handles_defaults_ipv4_and_ipv6() {
        assert_eq!(normalize_elm_wifi_endpoint(""), "192.168.0.10:35000");
        assert_eq!(normalize_elm_wifi_endpoint("192.168.0.10"), "192.168.0.10:35000");
        assert_eq!(normalize_elm_wifi_endpoint("127.0.0.1:4123"), "127.0.0.1:4123");
        assert_eq!(normalize_elm_wifi_endpoint("::1"), "[::1]:35000");
        assert_eq!(normalize_elm_wifi_endpoint("[::1]:35000"), "[::1]:35000");
    }

    #[test]
    fn bus_profile_mappings_cover_can_kwp_kds_sds_and_honda_experimental() {
        assert_eq!(ElmBusProfile::Auto.atsp_command(), "ATSP0");
        assert_eq!(ElmBusProfile::Can29Bit500k.atsp_command(), "ATSP7");
        assert!(ElmBusProfile::Can29Bit500k.is_29bit());
        assert_eq!(ElmBusProfile::KawasakiKds.protocol_id(), ProtocolId::Iso14230);
        assert!(ElmBusProfile::KawasakiKds
            .custom_init_commands()
            .contains(&"ATSH 81 11 F1"));
        assert!(ElmBusProfile::SuzukiSds
            .custom_init_commands()
            .contains(&"ATSH 81 12 F1"));
        assert!(ElmBusProfile::HondaKLineExperimental
            .custom_init_commands()
            .contains(&"ATSH 72"));
    }

    #[test]
    fn wifi_elm_loopback_handles_quirky_v21_clone_nul_bytes_ats0_fallback_and_capability_probe() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let addr = listener.local_addr().unwrap();

        let srv = std::thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(sock.try_clone().unwrap());

            loop {
                let cmd = read_cr_command(&mut reader);
                if cmd.is_empty() {
                    continue;
                }
                let reply: &[u8] = match cmd.as_str() {
                    "ATZ" => b"\x00\x00ATZ\r\x00ELM327 v2.1\r\r>",
                    "ATE0" => b"ATE0\rOK\r\r>",
                    "ATL0" | "ATH0" | "ATSP0" | "ATSP6" | "ATCAF1" | "ATSH 7E0" | "ATCRA 7E8" => {
                        b"OK\r\r>"
                    }
                    // Simulate limited v2.1 clone rejecting ATS0, ATAL, ATCAF0, ATBI, STDI
                    "ATS0" | "ATAL" | "ATCAF0" | "ATBI" | "STDI" => b"?\r\r>",
                    "ATS1" => b"OK\r\r>",
                    "ATRV" => b"12.7V\r\r>",
                    "ATI" => b"ELM327 v2.1\r\r>",
                    "ATDPN" => b"A6\r\r>",
                    "ATTP5" | "ATTP6" => b"OK\r\r>",
                    "010C" => b"SEARCHING...\r41 0C 1A F8\r\r>",
                    "ATPC" => {
                        let _ = sock.write_all(b"OK\r\r>");
                        break;
                    }
                    _ => b"OK\r\r>",
                };
                if sock.write_all(reply).is_err() {
                    break;
                }
                let _ = sock.flush();
            }
        });

        let mut dev = WifiElmDevice::new(addr.to_string()).with_bus_profile(ElmBusProfile::Auto);
        assert!(!dev.supports_flashing(), "ELM Wi-Fi must never claim flash support");

        dev.open().expect("open loopback wifi elm");
        assert!(dev.is_open());
        assert!(
            dev.clone_warnings().iter().any(|w| w.contains("ATS0")),
            "expected ATS0 fallback warning, got {:?}",
            dev.clone_warnings()
        );

        dev.connect(ProtocolId::Iso15765, 500_000).expect("connect");
        assert_eq!(dev.read_voltage_millivolts().unwrap(), 12_700);

        let caps = probe_elm_capabilities(&mut dev).expect("capability probe");
        assert_eq!(caps.chip_tier, "limited_clone_v21");
        assert!(!caps.supports_atcaf0);
        assert!(!caps.supports_atbi);
        assert!(!caps.supports_st_commands);
        assert!(caps.supports_atsh);

        // TX CAN frame with auto-header & response re-prefix
        dev.send(&PassThruMsg::new(
            ProtocolId::Iso15765,
            vec![0x00, 0x00, 0x07, 0xE0, 0x01, 0x0C],
        ))
        .expect("send OBD RPM");
        let rx = dev.receive().expect("receive RPM");
        assert_eq!(rx.data, vec![0x00, 0x00, 0x07, 0xE8, 0x41, 0x0C, 0x1A, 0xF8]);

        dev.close().expect("close");
        srv.join().unwrap();
    }
}
