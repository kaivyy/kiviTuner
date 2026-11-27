//! Raw USB-UART K-Line cable driver (FTDI / CH340 KKL 409.1) implementing [`Device`].
//!
//! Designed for Honda PGM-FI 4-pin DLC (`10,400` / `62,500` bps, `70 ms` LOW + `120 ms` HIGH
//! wake-up break pulse) and legacy Yamaha 3-pin (`10,400` / `15,625` bps) single-wire K-Line ECUs
//! where generic ELM327 AT-command adapters cannot bit-bang physical line break wake-ups or handle
//! non-ISO Honda checksums.

use crate::device::{Device, DeviceError, DeviceInfo, DeviceKind, TransportKind};
use crate::j2534_types::{PassThruMsg, ProtocolId};
use serialport::{ClearBuffer, DataBits, FlowControl, Parity, SerialPort, StopBits};
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::time::{Duration, Instant};

/// Standard Honda Keihin / Shindengen and ISO KWP2000 initial K-Line bitrate.
pub const DEFAULT_KLINE_BAUD: u32 = 10_400;

/// Legacy Yamaha YDT K-Line diagnostic bitrate.
pub const YAMAHA_YDT_KLINE_BAUD: u32 = 15_625;

/// Strips the single-wire half-duplex K-Line TX echo prefix from `rx` when present.
pub fn strip_kline_tx_echo(rx: &[u8], tx: &[u8]) -> Vec<u8> {
    if !tx.is_empty() && rx.len() >= tx.len() && rx[..tx.len()] == *tx {
        rx[tx.len()..].to_vec()
    } else {
        rx.to_vec()
    }
}

/// Validates whether a `ProtocolId` uses single-wire K-Line signaling.
pub fn is_supported_kline_protocol(protocol: ProtocolId) -> bool {
    matches!(protocol, ProtocolId::Iso14230 | ProtocolId::Iso9141)
}

/// Direct raw USB-UART K-Line cable transport (`KKL 409.1` / `L9637D` / `Si9243`).
pub struct RawKLineDevice {
    port_path: String,
    baud_rate: u32,
    port: Option<Box<dyn SerialPort>>,
    protocol: Option<ProtocolId>,
    kline_no_checksum: bool,
    rx_acc: Vec<u8>,
    rx_queue: VecDeque<PassThruMsg>,
}

impl Drop for RawKLineDevice {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

impl RawKLineDevice {
    pub fn new(port_path: impl Into<String>, baud_rate: u32) -> Self {
        let baud = if baud_rate > 0 {
            baud_rate
        } else {
            DEFAULT_KLINE_BAUD
        };
        Self {
            port_path: port_path.into(),
            baud_rate: baud,
            port: None,
            protocol: None,
            kline_no_checksum: false,
            rx_acc: Vec::with_capacity(256),
            rx_queue: VecDeque::with_capacity(16),
        }
    }

    pub fn baud_rate(&self) -> u32 {
        self.baud_rate
    }
}

impl Device for RawKLineDevice {
    fn open(&mut self) -> Result<(), DeviceError> {
        if self.port.is_some() {
            return Err(DeviceError::AlreadyOpen);
        }
        let port = serialport::new(&self.port_path, self.baud_rate)
            .data_bits(DataBits::Eight)
            .parity(Parity::None)
            .stop_bits(StopBits::One)
            .flow_control(FlowControl::None)
            .timeout(Duration::from_millis(25))
            .open()
            .map_err(|e| DeviceError::NotFound(format!("{}: {e}", self.port_path)))?;
        let _ = port.clear(ClearBuffer::All);
        self.rx_acc.clear();
        self.rx_queue.clear();
        self.port = Some(port);
        Ok(())
    }

    fn close(&mut self) -> Result<(), DeviceError> {
        self.port = None;
        self.protocol = None;
        self.rx_acc.clear();
        self.rx_queue.clear();
        Ok(())
    }

    fn connect(&mut self, protocol: ProtocolId, baud_rate: u32) -> Result<(), DeviceError> {
        let Some(port) = self.port.as_mut() else {
            return Err(DeviceError::NotOpen);
        };
        if !is_supported_kline_protocol(protocol) {
            return Err(DeviceError::Unsupported(
                "Raw K-Line USB serial cable supports only Iso14230 / Iso9141",
            ));
        }
        let target_baud = if baud_rate > 0 {
            baud_rate
        } else {
            self.baud_rate
        };
        port.set_baud_rate(target_baud).map_err(|e| {
            DeviceError::Transport(format!(
                "USB-UART adapter '{}' rejected {target_baud} bps K-Line baud rate: {e}",
                self.port_path
            ))
        })?;
        let _ = port.clear(ClearBuffer::Input);
        self.rx_acc.clear();
        self.rx_queue.clear();
        self.baud_rate = target_baud;
        self.protocol = Some(protocol);
        Ok(())
    }

    fn disconnect(&mut self) -> Result<(), DeviceError> {
        if self.port.is_none() {
            return Err(DeviceError::NotOpen);
        }
        self.protocol = None;
        self.rx_acc.clear();
        self.rx_queue.clear();
        Ok(())
    }

    fn kline_wakeup_pulse(&mut self, low_ms: u64, high_ms: u64) -> Result<(), DeviceError> {
        let Some(port) = self.port.as_mut() else {
            return Err(DeviceError::NotOpen);
        };
        let _ = port.clear(ClearBuffer::All);
        self.rx_acc.clear();
        self.rx_queue.clear();

        port.set_break().map_err(|e| {
            DeviceError::Transport(format!("K-Line set_break failed on '{}': {e}", self.port_path))
        })?;
        if low_ms > 0 {
            std::thread::sleep(Duration::from_millis(low_ms));
        }
        port.clear_break().map_err(|e| {
            DeviceError::Transport(format!("K-Line clear_break failed on '{}': {e}", self.port_path))
        })?;
        if high_ms > 0 {
            std::thread::sleep(Duration::from_millis(high_ms));
        }
        let _ = port.clear(ClearBuffer::Input);
        Ok(())
    }

    fn send(&mut self, msg: &PassThruMsg) -> Result<(), DeviceError> {
        let Some(port) = self.port.as_mut() else {
            return Err(DeviceError::NotOpen);
        };
        let proto = self.protocol.ok_or(DeviceError::NotConnected)?;
        if msg.data.is_empty() {
            return Err(DeviceError::Transport("K-Line TX payload is empty".to_string()));
        }
        self.rx_acc.clear();
        port.write_all(&msg.data)
            .map_err(|e| DeviceError::Transport(format!("K-Line write error: {e}")))?;
        let _ = port.flush();

        // Drain expected single-wire echo while preserving any immediate ECU reply bytes.
        let echo_deadline = Instant::now() + Duration::from_millis(140);
        let mut raw = Vec::with_capacity(msg.data.len() + 32);
        let mut tmp = [0u8; 64];
        while Instant::now() < echo_deadline && raw.len() < msg.data.len() {
            match port.read(&mut tmp) {
                Ok(n) if n > 0 => raw.extend_from_slice(&tmp[..n]),
                Ok(_) => break,
                Err(ref e)
                    if e.kind() == std::io::ErrorKind::TimedOut
                        || e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => {
                    return Err(DeviceError::Transport(format!("K-Line echo read error: {e}")));
                }
            }
        }
        let trailing = strip_kline_tx_echo(&raw, &msg.data);
        if !trailing.is_empty() {
            self.rx_acc.extend_from_slice(&trailing);
            let _ = proto;
        }
        Ok(())
    }

    fn receive(&mut self) -> Result<PassThruMsg, DeviceError> {
        let Some(port) = self.port.as_mut() else {
            return Err(DeviceError::NotOpen);
        };
        let proto = self.protocol.ok_or(DeviceError::NotConnected)?;
        if let Some(msg) = self.rx_queue.pop_front() {
            return Ok(msg);
        }

        let deadline = Instant::now() + Duration::from_millis(1200);
        let mut last_rx: Option<Instant> = (!self.rx_acc.is_empty()).then(Instant::now);
        let mut tmp = [0u8; 128];

        while Instant::now() < deadline {
            match port.read(&mut tmp) {
                Ok(n) if n > 0 => {
                    self.rx_acc.extend_from_slice(&tmp[..n]);
                    last_rx = Some(Instant::now());
                }
                Ok(_) => {
                    if !self.rx_acc.is_empty() {
                        break;
                    }
                }
                Err(ref e)
                    if e.kind() == std::io::ErrorKind::TimedOut
                        || e.kind() == std::io::ErrorKind::WouldBlock =>
                {
                    if let Some(ts) = last_rx {
                        if ts.elapsed() >= Duration::from_millis(40) && !self.rx_acc.is_empty() {
                            break;
                        }
                    }
                }
                Err(e) => return Err(DeviceError::Transport(format!("K-Line read error: {e}"))),
            }
        }

        if self.rx_acc.is_empty() {
            return Err(DeviceError::Timeout);
        }
        let packet = std::mem::take(&mut self.rx_acc);
        Ok(PassThruMsg::new(proto, packet))
    }

    fn read_voltage_millivolts(&mut self) -> Result<u32, DeviceError> {
        if self.port.is_none() {
            return Err(DeviceError::NotOpen);
        }
        Err(DeviceError::Unsupported(
            "Passive USB-UART K-Line cables do not measure battery voltage",
        ))
    }

    fn info(&self) -> DeviceInfo {
        DeviceInfo {
            kind: DeviceKind::KLineSerial,
            name: format!("Raw K-Line USB ({}, {} bps)", self.port_path, self.baud_rate),
            transport: TransportKind::Serial,
            firmware_version: Some("KKL-UART".to_string()),
            dll_version: None,
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

    fn uses_can_id_prefix(&self) -> bool {
        false
    }

    fn supports_flashing(&self) -> bool {
        true
    }

    fn set_kline_no_checksum(&mut self, enabled: bool) {
        self.kline_no_checksum = enabled;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_kline_echo_removes_tx_prefix_and_keeps_ecu_reply() {
        let tx = [0x72, 0x05, 0x71, 0x11, 0x07];
        let mut wire = tx.to_vec();
        wire.extend_from_slice(&[0x02, 0x06, 0x71, 0x11, 0xAA, 0x76]);
        assert_eq!(
            strip_kline_tx_echo(&wire, &tx),
            vec![0x02, 0x06, 0x71, 0x11, 0xAA, 0x76]
        );
        assert_eq!(strip_kline_tx_echo(&[0x02, 0x04, 0x00, 0xFA], &tx), vec![0x02, 0x04, 0x00, 0xFA]);
    }

    #[test]
    fn kline_protocol_guard_accepts_only_iso9141_and_iso14230() {
        assert!(is_supported_kline_protocol(ProtocolId::Iso14230));
        assert!(is_supported_kline_protocol(ProtocolId::Iso9141));
        assert!(!is_supported_kline_protocol(ProtocolId::Iso15765));
        assert!(!is_supported_kline_protocol(ProtocolId::Can));
    }

    #[test]
    fn fresh_raw_kline_device_reports_honest_capabilities() {
        let dev = RawKLineDevice::new("/dev/ttyUSB0", 0);
        assert_eq!(dev.baud_rate(), DEFAULT_KLINE_BAUD);
        assert_eq!(dev.info().kind, DeviceKind::KLineSerial);
        assert!(!dev.uses_can_id_prefix());
        assert!(dev.supports_flashing());
    }
}
