//! Tactrix OpenPort 2.0 native USB-CDC serial driver (`at...` firmware protocol).
//!
//! Ported from `libj2534_openport2/op2.c` and verified against `nikolakozina_j2534`.
//! Connects directly to `/dev/ttyACM*` or COM port without requiring external
//! `op20pt32.dll` / `.so` wrappers.

use crate::device::{Device, DeviceError, DeviceInfo, DeviceKind, TransportKind};
use crate::j2534_abi::PASSTHRU_MSG_DATA_CAPACITY;
use crate::j2534_types::{connect_flags, tx_flags, J2534Error, PassThruMsg, ProtocolId};
use serialport::SerialPort;
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::time::{Duration, Instant};

pub fn op2_connect_flags(protocol: ProtocolId, is_29bit: bool, kline_no_checksum: bool) -> u32 {
    if is_29bit && matches!(protocol, ProtocolId::Can | ProtocolId::Iso15765) {
        connect_flags::CAN_29BIT_ID
    } else if kline_no_checksum && matches!(protocol, ProtocolId::Iso9141 | ProtocolId::Iso14230) {
        connect_flags::ISO9141_NO_CHECKSUM | connect_flags::ISO9141_K_LINE_ONLY
    } else {
        0
    }
}

pub fn effective_op2_tx_flags(proto: ProtocolId, requested_tx_flags: u32, data: &[u8]) -> u32 {
    let mut flags = requested_tx_flags;
    if proto.is_iso15765() && flags == 0 {
        flags |= tx_flags::ISO15765_FRAME_PAD;
    }
    if matches!(proto, ProtocolId::Iso15765 | ProtocolId::Can) && data.len() >= 4 {
        let can_id = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        if can_id > 0x7FF {
            flags |= tx_flags::CAN_29BIT_ID;
        }
    }
    flags
}

pub fn parse_op2_vbatt_reply(rep: &str, expected_seq: u32) -> Result<u32, DeviceError> {
    let tokens: Vec<&str> = rep.split_whitespace().collect();
    let seq_str = expected_seq.to_string();
    if tokens.len() >= 4
        && tokens[0] == "arr"
        && tokens[1] == "16"
        && tokens.last().copied() == Some(seq_str.as_str())
    {
        if let Ok(mv) = tokens[2].parse::<u32>() {
            if (1_000..=40_000).contains(&mv) {
                return Ok(mv);
            }
        }
    }
    Err(DeviceError::Transport(format!(
        "OpenPort 2.0 returned invalid VBATT response: '{rep}'"
    )))
}

pub fn build_op2_filter_command(
    protocol: ProtocolId,
    tx_can_id: u32,
    rx_can_id: u32,
) -> Option<(String, Vec<u8>)> {
    let is_29bit = tx_can_id > 0x7FF || rx_can_id > 0x7FF;
    match protocol {
        ProtocolId::Iso15765 => {
            let flags = tx_flags::ISO15765_FRAME_PAD
                | if is_29bit { tx_flags::CAN_29BIT_ID } else { 0 };
            let cmd = format!("atf{} 3 {flags} 4", protocol.as_u32());
            let mut body = Vec::with_capacity(12);
            body.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
            body.extend_from_slice(&rx_can_id.to_be_bytes());
            body.extend_from_slice(&tx_can_id.to_be_bytes());
            Some((cmd, body))
        }
        ProtocolId::Can => {
            let flags = if is_29bit { tx_flags::CAN_29BIT_ID } else { 0 };
            let cmd = format!("atf{} 1 {flags} 4", protocol.as_u32());
            Some((cmd, vec![0u8; 8]))
        }
        ProtocolId::Iso9141 | ProtocolId::Iso14230 => {
            let cmd = format!("atf{} 1 0 1", protocol.as_u32());
            Some((cmd, vec![0x00, 0x00]))
        }
        _ => None,
    }
}

pub struct LiveOpenPort2Device {
    port_path: String,
    port: Option<Box<dyn SerialPort>>,
    firmware_version: Option<String>,
    protocol: Option<ProtocolId>,
    last_baud_rate: u32,
    kline_no_checksum: bool,
    channel_slot: u32,
    seq: u32,
    tx_can_id: u32,
    rx_can_id: u32,
    rx_queue: VecDeque<PassThruMsg>,
    ascii_queue: VecDeque<String>,
    rx_buf: Vec<u8>,
    partial_rx: Vec<u8>,
    partial_rx_proto: Option<ProtocolId>,
    partial_rx_ts: u32,
}

impl Drop for LiveOpenPort2Device {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

impl LiveOpenPort2Device {
    pub fn new(port_path: impl Into<String>) -> Self {
        Self {
            port_path: port_path.into(),
            port: None,
            firmware_version: None,
            protocol: None,
            last_baud_rate: 500_000,
            kline_no_checksum: false,
            channel_slot: 0,
            seq: 1,
            tx_can_id: 0x07E0,
            rx_can_id: 0x07E8,
            rx_queue: VecDeque::with_capacity(256),
            ascii_queue: VecDeque::with_capacity(32),
            rx_buf: Vec::with_capacity(4096),
            partial_rx: Vec::with_capacity(4128),
            partial_rx_proto: None,
            partial_rx_ts: 0,
        }
    }

    pub fn with_can_ids(mut self, tx_id: u32, rx_id: u32) -> Self {
        self.tx_can_id = tx_id;
        self.rx_can_id = rx_id;
        self
    }

    fn next_seq(&mut self) -> u32 {
        let cur = self.seq;
        self.seq = if self.seq >= 0xFFFF { 1 } else { self.seq + 1 };
        cur
    }

    fn send_cmd_wait_reply_with_seq(
        &mut self,
        cmd: &str,
        body: Option<&[u8]>,
        timeout_ms: u64,
        expect_ari: bool,
    ) -> Result<(String, u32), DeviceError> {
        let seq = self.next_seq();
        let line = format!("{cmd} {seq}\r\n");
        let port = self.port.as_mut().ok_or(DeviceError::NotOpen)?;
        port.write_all(line.as_bytes())
            .map_err(|e| DeviceError::Transport(format!("OP2 write error: {e}")))?;
        if let Some(payload) = body {
            port.write_all(payload)
                .map_err(|e| DeviceError::Transport(format!("OP2 payload write error: {e}")))?;
        }
        let _ = port.flush();

        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        while Instant::now() < deadline {
            self.pump_serial(deadline)?;
            if let Some(reply) = self.pop_ascii_reply(seq, expect_ari) {
                if let Some(err_rest) = reply.strip_prefix("are") {
                    let code = err_rest
                        .split_whitespace()
                        .next()
                        .and_then(|t| t.parse::<u32>().ok())
                        .unwrap_or(0x07);
                    return Err(DeviceError::Driver(J2534Error::from_u32(code)));
                }
                return Ok((reply, seq));
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        Err(DeviceError::Timeout)
    }

    fn send_cmd_wait_reply(
        &mut self,
        cmd: &str,
        body: Option<&[u8]>,
        timeout_ms: u64,
    ) -> Result<String, DeviceError> {
        let expect_ari = cmd == "ati";
        Ok(self
            .send_cmd_wait_reply_with_seq(cmd, body, timeout_ms, expect_ari)?
            .0)
    }

    fn pump_serial(&mut self, deadline: Instant) -> Result<(), DeviceError> {
        let port = self.port.as_mut().ok_or(DeviceError::NotOpen)?;
        let mut tmp = [0u8; 512];
        match port.read(&mut tmp) {
            Ok(n) if n > 0 => {
                self.rx_buf.extend_from_slice(&tmp[..n]);
                self.consume_tokens();
            }
            Ok(_) => {}
            Err(ref e)
                if e.kind() == std::io::ErrorKind::TimedOut
                    || e.kind() == std::io::ErrorKind::WouldBlock =>
            {
                let _ = deadline;
            }
            Err(e) => return Err(DeviceError::Transport(format!("OP2 read error: {e}"))),
        }
        Ok(())
    }

    fn consume_tokens(&mut self) {
        // Parse mixed stream: binary frames 'a''r'<proto:'1'..'6'><len> vs ASCII lines 'aro'/'are'/'ari'...\n
        while let Some(ar_pos) = self.rx_buf.windows(2).position(|w| w == b"ar") {
            if ar_pos > 0 {
                self.rx_buf.drain(..ar_pos);
            }
            if self.rx_buf.len() < 3 {
                break;
            }
            let kind = self.rx_buf[2];
            if (b'1'..=b'6').contains(&kind) {
                if self.rx_buf.len() < 4 {
                    break;
                }
                let len = self.rx_buf[3] as usize;
                if len == 0 {
                    self.rx_buf.drain(..4);
                    continue;
                }
                if self.rx_buf.len() < 4 + len {
                    break;
                }
                let proto = ProtocolId::from_u32((kind - b'0') as u32);
                let pkt_type = self.rx_buf[4];
                match pkt_type {
                    // TX_DONE (0x10), TX_LB (0x20), LB_END (0x60), NORM_START (0x80), TX_LB_START (0xA0)
                    0x10 | 0x20 | 0x60 | 0x80 | 0xA0 => {}
                    0x00 => {
                        if proto == ProtocolId::Can && len >= 9 {
                            let ts = u32::from_be_bytes([
                                self.rx_buf[5],
                                self.rx_buf[6],
                                self.rx_buf[7],
                                self.rx_buf[8],
                            ]);
                            let mut data = self.rx_buf[9..4 + len].to_vec();
                            if data.len() > PASSTHRU_MSG_DATA_CAPACITY {
                                data.truncate(PASSTHRU_MSG_DATA_CAPACITY);
                            }
                            let mut msg = PassThruMsg::new(proto, data);
                            msg.timestamp = ts;
                            self.rx_queue.push_back(msg);
                        } else if proto == ProtocolId::Iso15765 && len >= 5 {
                            let ts = u32::from_be_bytes([
                                self.rx_buf[5],
                                self.rx_buf[6],
                                self.rx_buf[7],
                                self.rx_buf[8],
                            ]);
                            if self.partial_rx_proto != Some(proto) {
                                self.partial_rx.clear();
                                self.partial_rx_proto = Some(proto);
                            }
                            self.partial_rx_ts = ts;
                            let rem = PASSTHRU_MSG_DATA_CAPACITY.saturating_sub(self.partial_rx.len());
                            let tail = &self.rx_buf[9..4 + len];
                            self.partial_rx.extend_from_slice(&tail[..tail.len().min(rem)]);
                        } else if matches!(proto, ProtocolId::Iso9141 | ProtocolId::Iso14230) {
                            if self.partial_rx_proto != Some(proto) {
                                self.partial_rx.clear();
                                self.partial_rx_proto = Some(proto);
                                self.partial_rx_ts = 0;
                            }
                            let rem = PASSTHRU_MSG_DATA_CAPACITY.saturating_sub(self.partial_rx.len());
                            let tail = &self.rx_buf[5..4 + len];
                            self.partial_rx.extend_from_slice(&tail[..tail.len().min(rem)]);
                        }
                    }
                    0x40 | 0x44 => {
                        if len >= 5 {
                            let ts = u32::from_be_bytes([
                                self.rx_buf[5],
                                self.rx_buf[6],
                                self.rx_buf[7],
                                self.rx_buf[8],
                            ]);
                            let mut data = if self.partial_rx_proto == Some(proto) {
                                std::mem::take(&mut self.partial_rx)
                            } else {
                                Vec::new()
                            };
                            self.partial_rx_proto = None;
                            let rem = PASSTHRU_MSG_DATA_CAPACITY.saturating_sub(data.len());
                            let tail = &self.rx_buf[9..4 + len];
                            data.extend_from_slice(&tail[..tail.len().min(rem)]);
                            if !data.is_empty() {
                                let mut msg = PassThruMsg::new(proto, data);
                                msg.timestamp = ts;
                                self.rx_queue.push_back(msg);
                            }
                        }
                    }
                    _ => {}
                }
                self.rx_buf.drain(..4 + len);
            } else {
                let Some(nl_rel) = self.rx_buf.iter().position(|&b| b == b'\n') else {
                    break;
                };
                let line_bytes: Vec<u8> = self.rx_buf.drain(..=nl_rel).collect();
                let text = String::from_utf8_lossy(&line_bytes).trim().to_string();
                if !text.is_empty() {
                    self.ascii_queue.push_back(text);
                }
            }
        }
    }

    fn pop_ascii_reply(&mut self, expected_seq: u32, expect_ari: bool) -> Option<String> {
        let seq_str = expected_seq.to_string();
        if let Some(idx) = self.ascii_queue.iter().position(|text| {
            if expect_ari {
                text.starts_with("ari")
            } else {
                text.split_whitespace().last() == Some(seq_str.as_str())
            }
        }) {
            return self.ascii_queue.remove(idx);
        }
        None
    }
}

impl Device for LiveOpenPort2Device {
    fn open(&mut self) -> Result<(), DeviceError> {
        if self.port.is_some() {
            return Err(DeviceError::AlreadyOpen);
        }
        let mut port = serialport::new(&self.port_path, 115_200)
            .timeout(Duration::from_millis(50))
            .open()
            .map_err(|e| DeviceError::NotFound(format!("{}: {e}", self.port_path)))?;
        let _ = port.write_data_terminal_ready(true);
        let _ = port.write_request_to_send(true);
        let _ = port.write_all(b"\r\n");
        let _ = port.flush();
        std::thread::sleep(Duration::from_millis(60));
        let _ = port.clear(serialport::ClearBuffer::Input);
        self.port = Some(port);

        let info = match self.send_cmd_wait_reply("ati", None, 1500) {
            Ok(i) => i,
            Err(e) => {
                self.port = None;
                return Err(e);
            }
        };
        self.firmware_version = Some(info);
        let _ = self.send_cmd_wait_reply("ata", None, 1000);
        Ok(())
    }

    fn close(&mut self) -> Result<(), DeviceError> {
        if self.port.is_some() {
            let _ = self.disconnect();
            let _ = self.send_cmd_wait_reply("ata", None, 500);
        }
        self.port = None;
        self.protocol = None;
        self.rx_queue.clear();
        self.ascii_queue.clear();
        self.rx_buf.clear();
        self.partial_rx.clear();
        self.partial_rx_proto = None;
        self.partial_rx_ts = 0;
        Ok(())
    }

    fn connect(&mut self, protocol: ProtocolId, baud_rate: u32) -> Result<(), DeviceError> {
        if self.port.is_none() {
            return Err(DeviceError::NotOpen);
        }
        if matches!(protocol, ProtocolId::J1850Vpw | ProtocolId::J1850Pwm) {
            return Err(DeviceError::Unsupported("OpenPort 2.0 does not support J1850"));
        }
        if self.protocol.is_some() {
            let _ = self.disconnect();
        }
        self.rx_queue.clear();
        self.partial_rx.clear();
        self.partial_rx_proto = None;

        let is_29bit = self.tx_can_id > 0x7FF || self.rx_can_id > 0x7FF;
        let conn_flag = op2_connect_flags(protocol, is_29bit, self.kline_no_checksum);
        self.last_baud_rate = baud_rate;
        let cmd = format!(
            "ato{} {} {} {}",
            protocol.as_u32(),
            conn_flag,
            baud_rate,
            self.channel_slot
        );
        let _ = self.send_cmd_wait_reply(&cmd, None, 2500)?;

        if let Some((fcmd, fbody)) =
            build_op2_filter_command(protocol, self.tx_can_id, self.rx_can_id)
        {
            if let Err(e) = self.send_cmd_wait_reply(&fcmd, Some(&fbody), 1500) {
                let _ = self.send_cmd_wait_reply(&format!("atc{}", protocol.as_u32()), None, 1000);
                return Err(e);
            }
        }
        self.protocol = Some(protocol);
        Ok(())
    }

    fn disconnect(&mut self) -> Result<(), DeviceError> {
        if self.port.is_none() {
            return Err(DeviceError::NotOpen);
        }
        if let Some(proto) = self.protocol.take() {
            let _ = self.send_cmd_wait_reply(&format!("atc{}", proto.as_u32()), None, 1000);
        }
        self.partial_rx.clear();
        self.partial_rx_proto = None;
        self.partial_rx_ts = 0;
        Ok(())
    }

    fn send(&mut self, msg: &PassThruMsg) -> Result<(), DeviceError> {
        if self.port.is_none() {
            return Err(DeviceError::NotOpen);
        }
        let proto = self.protocol.ok_or(DeviceError::NotConnected)?;
        if msg.data.len() > PASSTHRU_MSG_DATA_CAPACITY {
            return Err(DeviceError::Transport(format!(
                "OpenPort 2.0 payload {} bytes exceeds {} byte limit",
                msg.data.len(),
                PASSTHRU_MSG_DATA_CAPACITY
            )));
        }
        let eff_flags = effective_op2_tx_flags(proto, msg.tx_flags, &msg.data);
        let cmd = format!(
            "att{} {} {} 2000000",
            proto.as_u32(),
            msg.data.len(),
            eff_flags
        );
        let _ = self.send_cmd_wait_reply(&cmd, Some(&msg.data), 2500)?;
        Ok(())
    }

    fn receive(&mut self) -> Result<PassThruMsg, DeviceError> {
        if self.port.is_none() {
            return Err(DeviceError::NotOpen);
        }
        let connected_proto = self.protocol.ok_or(DeviceError::NotConnected)?;
        let deadline = Instant::now() + Duration::from_millis(1200);
        while Instant::now() < deadline {
            if let Some(pos) = self
                .rx_queue
                .iter()
                .position(|m| m.protocol_id == connected_proto)
            {
                if let Some(m) = self.rx_queue.remove(pos) {
                    return Ok(m);
                }
            }
            self.pump_serial(deadline)?;
            std::thread::sleep(Duration::from_millis(2));
        }
        Err(DeviceError::Timeout)
    }

    fn read_voltage_millivolts(&mut self) -> Result<u32, DeviceError> {
        if self.port.is_none() {
            return Err(DeviceError::NotOpen);
        }
        let (rep, seq) = self.send_cmd_wait_reply_with_seq("atr 16", None, 1000, false)?;
        parse_op2_vbatt_reply(&rep, seq)
    }

    fn info(&self) -> DeviceInfo {
        DeviceInfo {
            kind: DeviceKind::OpenPort2,
            name: format!("Tactrix OpenPort 2.0 ({})", self.port_path),
            transport: TransportKind::Usb,
            firmware_version: self.firmware_version.clone(),
            dll_version: Some("native-op2-serial".to_string()),
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

    fn configure_can_ids(&mut self, tx_id: u32, rx_id: u32) -> Result<(), DeviceError> {
        let changed = self.tx_can_id != tx_id || self.rx_can_id != rx_id;
        self.tx_can_id = tx_id;
        self.rx_can_id = rx_id;
        if changed {
            if let Some(proto) = self.protocol {
                if matches!(proto, ProtocolId::Iso15765 | ProtocolId::Can) {
                    let baud = self.last_baud_rate;
                    self.connect(proto, baud)?;
                }
            }
        }
        Ok(())
    }

    fn set_kline_no_checksum(&mut self, enabled: bool) {
        self.kline_no_checksum = enabled;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn op2_connect_and_tx_flags_handle_kline_nochecksum_and_can_29bit_pad() {
        assert_eq!(
            op2_connect_flags(ProtocolId::Iso14230, false, true),
            connect_flags::ISO9141_NO_CHECKSUM | connect_flags::ISO9141_K_LINE_ONLY
        );
        assert_eq!(
            op2_connect_flags(ProtocolId::Iso15765, true, false),
            connect_flags::CAN_29BIT_ID
        );
        assert_eq!(
            effective_op2_tx_flags(ProtocolId::Iso15765, 0, &[0x00, 0x00, 0x07, 0xE0, 0x01, 0x00]),
            tx_flags::ISO15765_FRAME_PAD
        );
        assert_eq!(
            effective_op2_tx_flags(
                ProtocolId::Iso15765,
                0,
                &[0x18, 0xDA, 0x10, 0xF1, 0x22, 0xF1, 0x90]
            ),
            tx_flags::ISO15765_FRAME_PAD | tx_flags::CAN_29BIT_ID
        );
    }

    #[test]
    fn vbatt_reply_parser_requires_pin_16_and_trailing_seq() {
        assert_eq!(parse_op2_vbatt_reply("arr 16 12650 7", 7).unwrap(), 12650);
        assert!(parse_op2_vbatt_reply("aro 7", 7).is_err());
        assert!(parse_op2_vbatt_reply("arr 17 12650 7", 7).is_err());
        assert!(parse_op2_vbatt_reply("arr 16 12650 8", 7).is_err());
        assert!(parse_op2_vbatt_reply("arr 16 500 7", 7).is_err());
    }

    #[test]
    fn op2_filter_builder_matches_nikolakozina_syntax() {
        let (cmd, body) = build_op2_filter_command(ProtocolId::Iso15765, 0x7E0, 0x7E8).unwrap();
        assert_eq!(cmd, "atf6 3 64 4");
        assert_eq!(body.len(), 12);
        assert_eq!(&body[0..4], &[0xFF, 0xFF, 0xFF, 0xFF]);
        assert_eq!(&body[4..8], &[0x00, 0x00, 0x07, 0xE8]);
        assert_eq!(&body[8..12], &[0x00, 0x00, 0x07, 0xE0]);

        let (cmd29, _) =
            build_op2_filter_command(ProtocolId::Iso15765, 0x18DA10F1, 0x18DAF110).unwrap();
        assert_eq!(cmd29, "atf6 3 320 4");

        let (kcmd, kbody) = build_op2_filter_command(ProtocolId::Iso14230, 0x7E0, 0x7E8).unwrap();
        assert_eq!(kcmd, "atf4 1 0 1");
        assert_eq!(kbody, vec![0x00, 0x00]);
    }

    #[test]
    fn pop_ascii_reply_matches_trailing_seq_without_pin_collision() {
        let mut dev = LiveOpenPort2Device::new("/dev/null");
        dev.ascii_queue.push_back("arr 16 12400 2".to_string());
        // Must NOT match seq=16 against the pin column "16"
        assert!(dev.pop_ascii_reply(16, false).is_none());
        assert_eq!(
            dev.pop_ascii_reply(2, false).as_deref(),
            Some("arr 16 12400 2")
        );
    }

    #[test]
    fn consume_tokens_skips_tx_done_and_reassembles_iso15765_and_kline() {
        let mut dev = LiveOpenPort2Device::new("/dev/null");
        // TX_DONE (0x10) on ISO15765 must be discarded
        dev.rx_buf.extend_from_slice(b"ar6\x05\x10\x00\x00\x00\x01");
        // START_OF_MESSAGE (0x80) on ISO15765 must be discarded
        dev.rx_buf.extend_from_slice(b"ar6\x05\x80\x00\x00\x00\x02");
        // NORM_MSG (0x00) chunk + RX_MSG_END_IND (0x40) on ISO15765
        dev.rx_buf
            .extend_from_slice(b"ar6\x0B\x00\x00\x00\x00\x10\x00\x00\x07\xE8\x50\x03");
        dev.rx_buf
            .extend_from_slice(b"ar6\x07\x40\x00\x00\x00\x20\x00\xC8");
        // K-Line 0x00 (no timestamp!) + 0x40 (has timestamp)
        dev.rx_buf.extend_from_slice(b"ar4\x04\x00\xAA\xBB\xCC");
        dev.rx_buf.extend_from_slice(b"ar4\x06\x40\x00\x00\x00\x30\xDD");

        dev.consume_tokens();
        assert_eq!(dev.rx_queue.len(), 2);

        let m_iso = dev.rx_queue.pop_front().unwrap();
        assert_eq!(m_iso.protocol_id, ProtocolId::Iso15765);
        assert_eq!(m_iso.timestamp, 0x20);
        assert_eq!(m_iso.data, vec![0x00, 0x00, 0x07, 0xE8, 0x50, 0x03, 0x00, 0xC8]);

        let m_kline = dev.rx_queue.pop_front().unwrap();
        assert_eq!(m_kline.protocol_id, ProtocolId::Iso14230);
        assert_eq!(m_kline.timestamp, 0x30);
        assert_eq!(m_kline.data, vec![0xAA, 0xBB, 0xCC, 0xDD]);
    }
}
