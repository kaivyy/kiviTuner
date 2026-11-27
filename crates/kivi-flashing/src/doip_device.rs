//! Real ISO 13400 Diagnostics over IP (DoIP) hardware device over TCP port 13400.
//!
//! Uses [`crate::doip_transport::DoIpMessage`] for Routing Activation (`0x0005` -> `0x0006`)
//! and UDS Diagnostic Messages (`0x8001` / `0x8002` / `0x8003`).

use crate::device::{Device, DeviceError, DeviceInfo, DeviceKind, TransportKind};
use crate::doip_transport::{DoIpMessage, DoIpPayloadType};
use crate::j2534_types::{PassThruMsg, ProtocolId};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

pub fn normalize_doip_endpoint(raw: &str) -> String {
    let hp = raw.trim();
    if hp.starts_with('[') || hp.matches(':').count() == 1 {
        hp.to_string()
    } else if hp.matches(':').count() > 1 {
        format!("[{hp}]:{}", DoIpMessage::DOIP_TCP_PORT)
    } else {
        format!("{hp}:{}", DoIpMessage::DOIP_TCP_PORT)
    }
}

pub fn derive_doip_rx_can_id(last_tx_can_id: u32) -> u32 {
    if last_tx_can_id == 0x7DF {
        0x7E8
    } else if (last_tx_can_id >> 16) == 0x18DA {
        (last_tx_can_id & 0xFFFF_0000)
            | ((last_tx_can_id & 0xFF) << 8)
            | ((last_tx_can_id >> 8) & 0xFF)
    } else {
        last_tx_can_id.wrapping_add(8)
    }
}

pub struct DoIpDevice {
    host_port: String,
    source_address: u16,
    target_address: u16,
    stream: Option<TcpStream>,
    protocol: Option<ProtocolId>,
    routing_active: bool,
    last_tx_can_id: u32,
}

impl DoIpDevice {
    pub fn new(host_port: impl Into<String>, source_address: u16, target_address: u16) -> Self {
        let hp = host_port.into();
        let normalized = normalize_doip_endpoint(&hp);
        Self {
            host_port: normalized,
            source_address,
            target_address,
            stream: None,
            protocol: None,
            routing_active: false,
            last_tx_can_id: 0x07E0,
        }
    }

    fn read_doip_frame(&mut self) -> Result<DoIpMessage, DeviceError> {
        let s = self.stream.as_mut().ok_or(DeviceError::NotOpen)?;
        let mut hdr = [0u8; 8];
        s.read_exact(&mut hdr).map_err(|e| {
            if matches!(e.kind(), std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock) {
                DeviceError::Timeout
            } else {
                DeviceError::Transport(format!("DoIP header read failed: {e}"))
            }
        })?;
        let plen = u32::from_be_bytes([hdr[4], hdr[5], hdr[6], hdr[7]]) as usize;
        if plen > 1024 * 1024 {
            return Err(DeviceError::Transport("DoIP payload exceeds 1 MiB safety limit".to_string()));
        }
        let mut full = Vec::with_capacity(8 + plen);
        full.extend_from_slice(&hdr);
        if plen > 0 {
            let mut body = vec![0u8; plen];
            s.read_exact(&mut body)
                .map_err(|e| DeviceError::Transport(format!("DoIP body read failed: {e}")))?;
            full.extend_from_slice(&body);
        }
        DoIpMessage::decode(&full).map_err(|e| DeviceError::Transport(e.to_string()))
    }
}

impl Device for DoIpDevice {
    fn open(&mut self) -> Result<(), DeviceError> {
        if self.stream.is_some() {
            return Err(DeviceError::AlreadyOpen);
        }
        let addr = self
            .host_port
            .to_socket_addrs()
            .map_err(|e| {
                DeviceError::NotFound(format!("invalid DoIP endpoint '{}': {e}", self.host_port))
            })?
            .next()
            .ok_or_else(|| {
                DeviceError::NotFound(format!("DoIP endpoint '{}' resolved empty", self.host_port))
            })?;
        let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(3))
            .map_err(|e| DeviceError::Transport(format!("DoIP TCP connect to {} failed: {e}", self.host_port)))?;
        let _ = stream.set_read_timeout(Some(Duration::from_millis(2000)));
        let _ = stream.set_write_timeout(Some(Duration::from_millis(2000)));
        let _ = stream.set_nodelay(true);
        self.stream = Some(stream);
        Ok(())
    }

    fn close(&mut self) -> Result<(), DeviceError> {
        self.stream = None;
        self.protocol = None;
        self.routing_active = false;
        self.last_tx_can_id = 0x07E0;
        Ok(())
    }

    fn connect(&mut self, protocol: ProtocolId, _baud_rate: u32) -> Result<(), DeviceError> {
        if self.stream.is_none() {
            return Err(DeviceError::NotOpen);
        }
        if self.routing_active {
            self.protocol = Some(protocol);
            return Ok(());
        }
        let mut req_payload = Vec::with_capacity(7);
        req_payload.extend_from_slice(&self.source_address.to_be_bytes());
        req_payload.push(0x00); // default activation type
        req_payload.extend_from_slice(&[0u8; 4]);
        let msg = DoIpMessage::new(DoIpPayloadType::RoutingActivationRequest, req_payload);
        self.stream
            .as_mut()
            .ok_or(DeviceError::NotOpen)?
            .write_all(&msg.encode())
            .map_err(|e| DeviceError::Transport(format!("DoIP routing activation TX failed: {e}")))?;

        for _ in 0..4 {
            let reply = self.read_doip_frame()?;
            match reply.payload_type {
                DoIpPayloadType::AliveCheckRequest => {
                    let resp = DoIpMessage::new(
                        DoIpPayloadType::AliveCheckResponse,
                        self.source_address.to_be_bytes().to_vec(),
                    );
                    if let Some(s) = self.stream.as_mut() {
                        s.write_all(&resp.encode()).map_err(|e| {
                            DeviceError::Transport(format!("DoIP alive-check TX failed: {e}"))
                        })?;
                    }
                }
                DoIpPayloadType::GenericHeaderNack => {
                    let code = reply.payload.first().copied().unwrap_or(0xFF);
                    return Err(DeviceError::Transport(format!(
                        "DoIP routing activation rejected with GenericHeaderNack (0x{code:02X})"
                    )));
                }
                DoIpPayloadType::RoutingActivationResponse => {
                    if reply.payload.len() < 5 || reply.payload[4] != 0x10 {
                        let code = reply.payload.get(4).copied().unwrap_or(0xFF);
                        return Err(DeviceError::Transport(format!(
                            "DoIP routing activation invalid/rejected (len={}, code=0x{code:02X})",
                            reply.payload.len()
                        )));
                    }
                    self.routing_active = true;
                    self.protocol = Some(protocol);
                    return Ok(());
                }
                other => {
                    return Err(DeviceError::Transport(format!(
                        "Unexpected DoIP routing response: {other:?}"
                    )));
                }
            }
        }
        Err(DeviceError::Timeout)
    }

    fn disconnect(&mut self) -> Result<(), DeviceError> {
        if self.stream.is_none() {
            return Err(DeviceError::NotOpen);
        }
        self.routing_active = false;
        self.protocol = None;
        self.last_tx_can_id = 0x07E0;
        Ok(())
    }

    fn send(&mut self, msg: &PassThruMsg) -> Result<(), DeviceError> {
        let Some(stream) = self.stream.as_mut() else {
            return Err(DeviceError::NotOpen);
        };
        if !self.routing_active {
            return Err(DeviceError::NotConnected);
        }
        let needs_can_prefix = msg.protocol_id.is_iso15765() || msg.protocol_id == ProtocolId::Can;
        let uds = if needs_can_prefix {
            if msg.data.len() < 4 {
                return Err(DeviceError::Transport(
                    "DoIP CAN/ISO15765 TX requires 4-byte CAN arbitration ID prefix".to_string(),
                ));
            }
            self.last_tx_can_id =
                u32::from_be_bytes([msg.data[0], msg.data[1], msg.data[2], msg.data[3]]);
            &msg.data[4..]
        } else {
            msg.data.as_slice()
        };
        let mut payload = Vec::with_capacity(4 + uds.len());
        payload.extend_from_slice(&self.source_address.to_be_bytes());
        payload.extend_from_slice(&self.target_address.to_be_bytes());
        payload.extend_from_slice(uds);
        let frame = DoIpMessage::new(DoIpPayloadType::DiagnosticMessage, payload);
        stream
            .write_all(&frame.encode())
            .map_err(|e| DeviceError::Transport(format!("DoIP diagnostic TX failed: {e}")))?;
        Ok(())
    }

    fn receive(&mut self) -> Result<PassThruMsg, DeviceError> {
        if self.stream.is_none() {
            return Err(DeviceError::NotOpen);
        }
        let proto = self.protocol.ok_or(DeviceError::NotConnected)?;
        let deadline = Instant::now() + Duration::from_millis(2500);
        for _ in 0..64 {
            if Instant::now() >= deadline {
                return Err(DeviceError::Timeout);
            }
            let frame = self.read_doip_frame()?;
            match frame.payload_type {
                DoIpPayloadType::DiagnosticMessagePositiveAck => continue,
                DoIpPayloadType::DiagnosticMessageNegativeAck => {
                    let nack = frame.payload.get(4).copied().unwrap_or(0xFF);
                    return Err(DeviceError::Transport(format!(
                        "DoIP diagnostic NACK (0x{nack:02X})"
                    )));
                }
                DoIpPayloadType::GenericHeaderNack => {
                    let nack = frame.payload.first().copied().unwrap_or(0xFF);
                    return Err(DeviceError::Transport(format!(
                        "DoIP generic header NACK (0x{nack:02X})"
                    )));
                }
                DoIpPayloadType::AliveCheckRequest => {
                    let resp = DoIpMessage::new(
                        DoIpPayloadType::AliveCheckResponse,
                        self.source_address.to_be_bytes().to_vec(),
                    );
                    if let Some(s) = self.stream.as_mut() {
                        s.write_all(&resp.encode()).map_err(|e| {
                            DeviceError::Transport(format!("DoIP alive-check TX failed: {e}"))
                        })?;
                    }
                    continue;
                }
                DoIpPayloadType::DiagnosticMessage => {
                    if frame.payload.len() < 4 {
                        return Err(DeviceError::Transport("Truncated DoIP diagnostic payload".to_string()));
                    }
                    let src = u16::from_be_bytes([frame.payload[0], frame.payload[1]]);
                    let dst = u16::from_be_bytes([frame.payload[2], frame.payload[3]]);
                    if src != self.target_address || dst != self.source_address {
                        continue;
                    }
                    let uds = &frame.payload[4..];
                    let out = if proto.is_iso15765() || proto == ProtocolId::Can {
                        let rx_id = derive_doip_rx_can_id(self.last_tx_can_id);
                        let mut prefixed = Vec::with_capacity(4 + uds.len());
                        prefixed.extend_from_slice(&rx_id.to_be_bytes());
                        prefixed.extend_from_slice(uds);
                        prefixed
                    } else {
                        uds.to_vec()
                    };
                    return Ok(PassThruMsg::new(proto, out));
                }
                _ => continue,
            }
        }
        Err(DeviceError::Timeout)
    }

    fn read_voltage_millivolts(&mut self) -> Result<u32, DeviceError> {
        if self.stream.is_none() {
            return Err(DeviceError::NotOpen);
        }
        Err(DeviceError::Unsupported(
            "DoIP Ethernet interface does not measure battery voltage",
        ))
    }

    fn info(&self) -> DeviceInfo {
        DeviceInfo {
            kind: DeviceKind::DoIp,
            name: format!("DoIP Ethernet ({})", self.host_port),
            transport: TransportKind::Ethernet,
            firmware_version: Some("ISO-13400-2".to_string()),
            dll_version: None,
        }
    }

    fn is_open(&self) -> bool {
        self.stream.is_some()
    }

    fn is_connected(&self) -> bool {
        self.routing_active && self.protocol.is_some()
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
    use std::net::TcpListener;

    #[test]
    fn endpoint_normalization_and_rx_id_derivation() {
        assert_eq!(normalize_doip_endpoint("localhost"), "localhost:13400");
        assert_eq!(normalize_doip_endpoint("192.168.1.5"), "192.168.1.5:13400");
        assert_eq!(normalize_doip_endpoint("127.0.0.1:13400"), "127.0.0.1:13400");
        assert_eq!(normalize_doip_endpoint("::1"), "[::1]:13400");
        assert_eq!(normalize_doip_endpoint("[::1]:13400"), "[::1]:13400");

        assert_eq!(derive_doip_rx_can_id(0x7E0), 0x7E8);
        assert_eq!(derive_doip_rx_can_id(0x7DF), 0x7E8);
        assert_eq!(derive_doip_rx_can_id(0x18DA_10F1), 0x18DA_F110);
    }

    #[test]
    fn loopback_doip_routing_alive_check_ack_and_idempotent_connect() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let addr = listener.local_addr().unwrap();

        let srv = std::thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            let mut hdr = [0u8; 8];
            sock.read_exact(&mut hdr).unwrap();
            let req = DoIpMessage::decode(&hdr).unwrap_or_else(|_| {
                let len = u32::from_be_bytes([hdr[4], hdr[5], hdr[6], hdr[7]]) as usize;
                let mut buf = vec![0u8; 8 + len];
                buf[..8].copy_from_slice(&hdr);
                sock.read_exact(&mut buf[8..]).unwrap();
                DoIpMessage::decode(&buf).unwrap()
            });
            assert_eq!(req.payload_type, DoIpPayloadType::RoutingActivationRequest);

            // Send AliveCheckRequest first, verify client replies AliveCheckResponse
            let alive = DoIpMessage::new(DoIpPayloadType::AliveCheckRequest, vec![]);
            sock.write_all(&alive.encode()).unwrap();

            let mut ack_hdr = [0u8; 8];
            sock.read_exact(&mut ack_hdr).unwrap();
            let ack_len = u32::from_be_bytes([ack_hdr[4], ack_hdr[5], ack_hdr[6], ack_hdr[7]]) as usize;
            let mut ack_buf = vec![0u8; 8 + ack_len];
            ack_buf[..8].copy_from_slice(&ack_hdr);
            sock.read_exact(&mut ack_buf[8..]).unwrap();
            let alive_resp = DoIpMessage::decode(&ack_buf).unwrap();
            assert_eq!(alive_resp.payload_type, DoIpPayloadType::AliveCheckResponse);
            assert_eq!(alive_resp.payload, 0x0E80u16.to_be_bytes());

            // Send RoutingActivationResponse (accepted: 0x10)
            let mut ra = Vec::new();
            ra.extend_from_slice(&0x0E80u16.to_be_bytes());
            ra.extend_from_slice(&0x1010u16.to_be_bytes());
            ra.push(0x10);
            ra.extend_from_slice(&[0u8; 4]);
            sock.write_all(&DoIpMessage::new(DoIpPayloadType::RoutingActivationResponse, ra).encode())
                .unwrap();

            // Read diagnostic message from client
            sock.read_exact(&mut hdr).unwrap();
            let dlen = u32::from_be_bytes([hdr[4], hdr[5], hdr[6], hdr[7]]) as usize;
            let mut dbuf = vec![0u8; 8 + dlen];
            dbuf[..8].copy_from_slice(&hdr);
            sock.read_exact(&mut dbuf[8..]).unwrap();
            let diag = DoIpMessage::decode(&dbuf).unwrap();
            assert_eq!(diag.payload_type, DoIpPayloadType::DiagnosticMessage);

            // Send PositiveAck then UDS reply
            sock.write_all(
                &DoIpMessage::new(
                    DoIpPayloadType::DiagnosticMessagePositiveAck,
                    vec![0x10, 0x10, 0x0E, 0x80, 0x00],
                )
                .encode(),
            )
            .unwrap();
            sock.write_all(
                &DoIpMessage::new(
                    DoIpPayloadType::DiagnosticMessage,
                    vec![0x10, 0x10, 0x0E, 0x80, 0x50, 0x03],
                )
                .encode(),
            )
            .unwrap();
        });

        let mut dev = DoIpDevice::new(addr.to_string(), 0x0E80, 0x1010);
        dev.open().unwrap();
        dev.connect(ProtocolId::Iso15765, 500_000).unwrap();
        // Idempotent second connect must not re-send RoutingActivation
        dev.connect(ProtocolId::Iso15765, 500_000).unwrap();

        dev.send(&PassThruMsg::new(
            ProtocolId::Iso15765,
            vec![0x00, 0x00, 0x07, 0xE0, 0x10, 0x03],
        ))
        .unwrap();
        let rx = dev.receive().unwrap();
        assert_eq!(rx.data, vec![0x00, 0x00, 0x07, 0xE8, 0x50, 0x03]);
        dev.close().unwrap();
        srv.join().unwrap();
    }
}
