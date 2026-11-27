//! Real ELM327 / STN1110 / OBDLink serial adapter implementing [`Device`].
//!
//! Uses `serialport` AT command bridge and [`crate::elm_device::ElmDevice`] parsers.
//! Supports live telemetry, ECU identification, and read-only UDS Service 0x23/0x22.

use crate::device::{Device, DeviceError, DeviceInfo, DeviceKind, TransportKind};
use crate::elm_device::{ElmDevice, ElmResponse, ElmStatus};
use crate::j2534_types::{PassThruMsg, ProtocolId};
use serialport::SerialPort;
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::time::{Duration, Instant};

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

pub fn derive_elm_rx_can_id(tx_id: u32) -> u32 {
    if tx_id == 0x7DF {
        0x7E8
    } else if (tx_id >> 16) == 0x18DA {
        (tx_id & 0xFFFF_0000) | ((tx_id & 0xFF) << 8) | ((tx_id >> 8) & 0xFF)
    } else {
        tx_id.wrapping_add(8)
    }
}

pub struct SerialElmDevice {
    port_path: String,
    baud_rate: u32,
    port: Option<Box<dyn SerialPort>>,
    protocol: Option<ProtocolId>,
    adapter_id: Option<String>,
    current_tx_can_id: Option<u32>,
    can_baud_rate: u32,
    can_is_29bit: bool,
    rx_queue: VecDeque<PassThruMsg>,
}

impl Drop for SerialElmDevice {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

impl SerialElmDevice {
    pub fn new(port_path: impl Into<String>, baud_rate: u32) -> Self {
        Self {
            port_path: port_path.into(),
            baud_rate,
            port: None,
            protocol: None,
            adapter_id: None,
            current_tx_can_id: None,
            can_baud_rate: 500_000,
            can_is_29bit: false,
            rx_queue: VecDeque::with_capacity(32),
        }
    }

    fn send_at(&mut self, cmd: &str, timeout_ms: u64) -> Result<ElmResponse, DeviceError> {
        let raw = self.exchange_raw(&ElmDevice::encode(cmd), timeout_ms)?;
        Ok(ElmResponse::parse(&raw))
    }

    fn exchange_raw(&mut self, cmd_with_cr: &str, timeout_ms: u64) -> Result<String, DeviceError> {
        let port = self.port.as_mut().ok_or(DeviceError::NotOpen)?;
        port.write_all(cmd_with_cr.as_bytes())
            .map_err(|e| DeviceError::Transport(format!("ELM serial write error: {e}")))?;
        let _ = port.flush();

        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        let mut buf = Vec::with_capacity(256);
        let mut byte = [0u8; 64];
        while Instant::now() < deadline {
            match port.read(&mut byte) {
                Ok(n) if n > 0 => {
                    buf.extend_from_slice(&byte[..n]);
                    if buf.contains(&b'>') {
                        return Ok(String::from_utf8_lossy(&buf).to_string());
                    }
                }
                Ok(_) => {}
                Err(ref e)
                    if e.kind() == std::io::ErrorKind::TimedOut
                        || e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => {
                    return Err(DeviceError::Transport(format!("ELM serial read error: {e}")));
                }
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        Err(DeviceError::Timeout)
    }
}

impl Device for SerialElmDevice {
    fn open(&mut self) -> Result<(), DeviceError> {
        if self.port.is_some() {
            return Err(DeviceError::AlreadyOpen);
        }
        let port = serialport::new(&self.port_path, self.baud_rate)
            .timeout(Duration::from_millis(50))
            .open()
            .map_err(|e| DeviceError::NotFound(format!("{}: {e}", self.port_path)))?;
        let _ = port.clear(serialport::ClearBuffer::Input);
        self.port = Some(port);

        for cmd in ElmDevice::init_sequence() {
            let rep = match self.send_at(cmd, 1500) {
                Ok(r) => r,
                Err(e) => {
                    self.port = None;
                    return Err(e);
                }
            };
            if matches!(cmd, "ATE0" | "ATL0" | "ATS0") && rep.status == ElmStatus::Error {
                self.port = None;
                return Err(DeviceError::Transport(format!(
                    "ELM rejected init command '{cmd}': {}",
                    rep.text
                )));
            }
            if cmd == "ATZ" && !rep.text.is_empty() {
                self.adapter_id = Some(rep.text);
            }
        }
        self.current_tx_can_id = None;
        self.can_is_29bit = false;
        Ok(())
    }

    fn close(&mut self) -> Result<(), DeviceError> {
        if self.port.is_some() {
            let _ = self.send_at("ATPC", 300);
        }
        self.port = None;
        self.protocol = None;
        self.current_tx_can_id = None;
        self.can_is_29bit = false;
        self.rx_queue.clear();
        Ok(())
    }

    fn connect(&mut self, protocol: ProtocolId, baud_rate: u32) -> Result<(), DeviceError> {
        if self.port.is_none() {
            return Err(DeviceError::NotOpen);
        }
        self.can_baud_rate = baud_rate;
        self.can_is_29bit = false;
        self.current_tx_can_id = None;
        let atsp = match protocol {
            ProtocolId::Iso15765 | ProtocolId::Can => elm_can_protocol_cmd(baud_rate, false),
            ProtocolId::Iso14230 => "ATSP5",
            ProtocolId::Iso9141 => "ATSP3",
            ProtocolId::J1850Pwm => "ATSP1",
            ProtocolId::J1850Vpw => "ATSP2",
            _ => "ATSP0",
        };
        let rep = self.send_at(atsp, 1000)?;
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
            let caf = self.send_at("ATCAF1", 800)?;
            if caf.status == ElmStatus::Error {
                return Err(DeviceError::Transport(format!(
                    "ELM rejected ISO-TP ATCAF1: {}",
                    caf.text
                )));
            }
        }
        self.protocol = Some(protocol);
        Ok(())
    }

    fn disconnect(&mut self) -> Result<(), DeviceError> {
        if self.port.is_none() {
            return Err(DeviceError::NotOpen);
        }
        let _ = self.send_at("ATPC", 500);
        self.protocol = None;
        self.current_tx_can_id = None;
        self.can_is_29bit = false;
        self.rx_queue.clear();
        Ok(())
    }

    fn send(&mut self, msg: &PassThruMsg) -> Result<(), DeviceError> {
        if self.port.is_none() {
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
            if wants_29 != self.can_is_29bit {
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
        let raw = self.exchange_raw(&format!("{hex_payload}\r"), 1100)?;
        let rep = ElmResponse::parse(&raw);
        if rep.status == ElmStatus::NoData || rep.status == ElmStatus::Stopped {
            return Err(DeviceError::Timeout);
        }
        if rep.status == ElmStatus::Error {
            return Err(DeviceError::Transport(format!("ELM error for TX '{hex_payload}'")));
        }
        if !rep.bytes.is_empty() {
            let rx_data = if let Some(tx_id) = tx_id {
                let mut v = Vec::with_capacity(4 + rep.bytes.len());
                v.extend_from_slice(&derive_elm_rx_can_id(tx_id).to_be_bytes());
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
        if self.port.is_none() {
            return Err(DeviceError::NotOpen);
        }
        if self.protocol.is_none() {
            return Err(DeviceError::NotConnected);
        }
        self.rx_queue.pop_front().ok_or(DeviceError::Timeout)
    }

    fn read_voltage_millivolts(&mut self) -> Result<u32, DeviceError> {
        if self.port.is_none() {
            return Err(DeviceError::NotOpen);
        }
        let rep = self.send_at("ATRV", 1000)?;
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

    fn info(&self) -> DeviceInfo {
        DeviceInfo {
            kind: DeviceKind::Serial,
            name: format!("Serial OBD ({})", self.port_path),
            transport: TransportKind::Serial,
            firmware_version: self.adapter_id.clone(),
            dll_version: Some("elm327-serial".to_string()),
        }
    }

    fn is_open(&self) -> bool {
        self.port.is_some()
    }

    fn is_connected(&self) -> bool {
        self.protocol.is_some()
    }

    fn connected_protocol(&self) -> Option<ProtocolId> {
        self.protocol
    }

    fn is_mock(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

        assert_eq!(derive_elm_rx_can_id(0x7E0), 0x7E8);
        assert_eq!(derive_elm_rx_can_id(0x7DF), 0x7E8);
        assert_eq!(derive_elm_rx_can_id(0x18DA_10F1), 0x18DA_F110);
    }
}
