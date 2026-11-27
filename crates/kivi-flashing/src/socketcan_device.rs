//! Native Linux SocketCAN (`AF_CAN` / `SOCK_RAW` / `CAN_RAW`) [`Device`] via `libc`.
//!
//! Supports both single-frame classic CAN (`ProtocolId::Can`) and software ISO 15765-2
//! segmented CAN diagnostics (`ProtocolId::Iso15765`) over raw Linux CAN sockets.
//! Returns `DeviceError::Unsupported` cleanly on non-Linux hosts.

use crate::device::{Device, DeviceError, DeviceInfo, DeviceKind, TransportKind};
use crate::j2534_types::{PassThruMsg, ProtocolId};
#[cfg(target_os = "linux")]
use crate::j2534_types::{rx_status, tx_flags};

/// Maximum classic ISO 15765-2 diagnostic payload length (12-bit length field).
pub const MAX_ISOTP_PAYLOAD_LEN: usize = 4095;

/// Encodes a diagnostic payload into padded 8-byte ISO 15765-2 CAN frames for `tx_id`.
pub fn encode_isotp_frames(tx_id: u32, payload: &[u8]) -> Result<Vec<(u32, [u8; 8])>, DeviceError> {
    if payload.is_empty() {
        return Err(DeviceError::Transport("ISO-TP payload cannot be empty".to_string()));
    }
    if payload.len() > MAX_ISOTP_PAYLOAD_LEN {
        return Err(DeviceError::Transport(format!(
            "ISO-TP payload {} bytes exceeds {} byte classic CAN limit",
            payload.len(),
            MAX_ISOTP_PAYLOAD_LEN
        )));
    }
    if payload.len() <= 7 {
        let mut frame = [0u8; 8];
        frame[0] = payload.len() as u8;
        frame[1..1 + payload.len()].copy_from_slice(payload);
        return Ok(vec![(tx_id, frame)]);
    }

    let len = payload.len();
    let mut frames = Vec::with_capacity(1 + (len - 6).div_ceil(7));
    let mut ff = [0u8; 8];
    ff[0] = 0x10 | (((len >> 8) & 0x0F) as u8);
    ff[1] = (len & 0xFF) as u8;
    ff[2..8].copy_from_slice(&payload[..6]);
    frames.push((tx_id, ff));

    let mut sn: u8 = 1;
    for chunk in payload[6..].chunks(7) {
        let mut cf = [0u8; 8];
        cf[0] = 0x20 | (sn & 0x0F);
        cf[1..1 + chunk.len()].copy_from_slice(chunk);
        frames.push((tx_id, cf));
        sn = (sn + 1) & 0x0F;
    }
    Ok(frames)
}

/// Derives the physical TX ID for a Flow Control CTS reply given configured TX & incoming RX ID.
pub fn derive_isotp_flow_control_tx_id(configured_tx_id: u32, incoming_rx_id: u32) -> u32 {
    if configured_tx_id == 0x7DF {
        incoming_rx_id.saturating_sub(8)
    } else if (incoming_rx_id >> 16) == 0x18DA && configured_tx_id == 0x18DB_33F1 {
        (incoming_rx_id & 0xFFFF_0000)
            | ((incoming_rx_id & 0xFF) << 8)
            | ((incoming_rx_id >> 8) & 0xFF)
    } else {
        configured_tx_id
    }
}

/// Returns true if `incoming_id` matches expected ECU response ID (physical or functional broadcast).
pub fn is_matching_isotp_rx_id(configured_tx_id: u32, configured_rx_id: u32, incoming_id: u32) -> bool {
    if incoming_id == configured_rx_id {
        return true;
    }
    if configured_tx_id == 0x7DF && (0x7E8..=0x7EF).contains(&incoming_id) {
        return true;
    }
    if configured_tx_id == 0x18DB_33F1 && (incoming_id >> 16) == 0x18DA {
        return true;
    }
    false
}

/// Parsed ISO 15765-2 Flow Control parameters `(status, block_size, st_min_ms)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlowControlFrame {
    pub flow_status: u8,
    pub block_size: u8,
    pub st_min_ms: u64,
}

pub fn parse_isotp_flow_control(data: &[u8]) -> Option<FlowControlFrame> {
    if data.len() < 3 || (data[0] & 0xF0) != 0x30 {
        return None;
    }
    let st_raw = data[2];
    let st_min_ms = if st_raw <= 0x7F {
        u64::from(st_raw)
    } else if (0xF1..=0xF9).contains(&st_raw) {
        1
    } else {
        10
    };
    Some(FlowControlFrame {
        flow_status: data[0] & 0x0F,
        block_size: data[1],
        st_min_ms,
    })
}

/// Stateful single/multi-frame ISO 15765-2 RX reassembler.
#[derive(Debug, Default)]
pub struct IsoTpRxReassembler {
    expected_len: usize,
    next_sn: u8,
    active_rx_id: Option<u32>,
    buf: Vec<u8>,
}

/// Outcome of feeding one raw CAN frame into [`IsoTpRxReassembler`].
#[derive(Debug, PartialEq, Eq)]
pub enum IsoTpRxStep {
    Incomplete { send_flow_control_to: Option<u32> },
    Complete { rx_id: u32, payload: Vec<u8> },
    Ignored,
}

impl IsoTpRxReassembler {
    pub fn reset(&mut self) {
        self.expected_len = 0;
        self.next_sn = 1;
        self.active_rx_id = None;
        self.buf.clear();
    }

    pub fn feed(
        &mut self,
        configured_tx_id: u32,
        configured_rx_id: u32,
        incoming_id: u32,
        data: &[u8],
    ) -> IsoTpRxStep {
        if data.is_empty() || !is_matching_isotp_rx_id(configured_tx_id, configured_rx_id, incoming_id) {
            return IsoTpRxStep::Ignored;
        }
        let pci_type = data[0] >> 4;
        match pci_type {
            0x0 => {
                let len = (data[0] & 0x0F) as usize;
                if len == 0 || 1 + len > data.len() {
                    return IsoTpRxStep::Ignored;
                }
                self.reset();
                IsoTpRxStep::Complete {
                    rx_id: incoming_id,
                    payload: data[1..1 + len].to_vec(),
                }
            }
            0x1 => {
                if data.len() < 8 {
                    return IsoTpRxStep::Ignored;
                }
                let total_len = (((data[0] & 0x0F) as usize) << 8) | usize::from(data[1]);
                if total_len < 8 || total_len > MAX_ISOTP_PAYLOAD_LEN {
                    return IsoTpRxStep::Ignored;
                }
                self.reset();
                self.expected_len = total_len;
                self.next_sn = 1;
                self.active_rx_id = Some(incoming_id);
                self.buf.extend_from_slice(&data[2..8]);
                let fc_tx = derive_isotp_flow_control_tx_id(configured_tx_id, incoming_id);
                IsoTpRxStep::Incomplete {
                    send_flow_control_to: Some(fc_tx),
                }
            }
            0x2 => {
                if self.active_rx_id != Some(incoming_id) {
                    return IsoTpRxStep::Ignored;
                }
                let sn = data[0] & 0x0F;
                if sn != self.next_sn {
                    self.reset();
                    return IsoTpRxStep::Ignored;
                }
                self.next_sn = (self.next_sn + 1) & 0x0F;
                let remaining = self.expected_len.saturating_sub(self.buf.len());
                let take = remaining.min(data.len().saturating_sub(1));
                self.buf.extend_from_slice(&data[1..1 + take]);
                if self.buf.len() >= self.expected_len {
                    let payload = self.buf[..self.expected_len].to_vec();
                    self.reset();
                    IsoTpRxStep::Complete {
                        rx_id: incoming_id,
                        payload,
                    }
                } else {
                    IsoTpRxStep::Incomplete {
                        send_flow_control_to: None,
                    }
                }
            }
            _ => IsoTpRxStep::Ignored,
        }
    }
}

pub struct SocketCanDevice {
    interface: String,
    #[cfg(target_os = "linux")]
    fd: i32,
    #[cfg(not(target_os = "linux"))]
    _open_unused: bool,
    protocol: Option<ProtocolId>,
    tx_can_id: u32,
    rx_can_id: u32,
}

impl SocketCanDevice {
    pub fn new(interface: impl Into<String>) -> Self {
        Self {
            interface: interface.into(),
            #[cfg(target_os = "linux")]
            fd: -1,
            #[cfg(not(target_os = "linux"))]
            _open_unused: false,
            protocol: None,
            tx_can_id: 0x07E0,
            rx_can_id: 0x07E8,
        }
    }

    pub fn configured_can_ids(&self) -> (u32, u32) {
        (self.tx_can_id, self.rx_can_id)
    }

    #[cfg(target_os = "linux")]
    fn write_raw_can_frame(&self, can_id: u32, is_29bit_flag: bool, payload: &[u8]) -> Result<(), DeviceError> {
        if payload.len() > 8 {
            return Err(DeviceError::Transport(format!(
                "SocketCAN CAN_RAW frame payload {} bytes exceeds 8-byte classic CAN MTU",
                payload.len()
            )));
        }
        let mut frame: libc::can_frame = unsafe { std::mem::zeroed() };
        frame.can_id = if can_id > libc::CAN_SFF_MASK || is_29bit_flag {
            (can_id & libc::CAN_EFF_MASK) | libc::CAN_EFF_FLAG
        } else {
            can_id & libc::CAN_SFF_MASK
        };
        let dlc = payload.len();
        frame.can_dlc = dlc as u8;
        frame.data[..dlc].copy_from_slice(payload);
        let frame_size = std::mem::size_of::<libc::can_frame>();
        loop {
            let n = unsafe {
                libc::write(
                    self.fd,
                    (&frame as *const libc::can_frame).cast(),
                    frame_size,
                )
            };
            if n < 0 {
                let err = std::io::Error::last_os_error();
                if err.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(DeviceError::Transport(format!("SocketCAN write() failed: {err}")));
            }
            if (n as usize) != frame_size {
                return Err(DeviceError::Transport(format!(
                    "SocketCAN short write: {n} of {frame_size} bytes"
                )));
            }
            return Ok(());
        }
    }

    #[cfg(target_os = "linux")]
    fn read_raw_can_frame(&self) -> Result<(u32, bool, Vec<u8>), DeviceError> {
        let frame_size = std::mem::size_of::<libc::can_frame>();
        loop {
            let mut frame: libc::can_frame = unsafe { std::mem::zeroed() };
            let n = unsafe {
                libc::read(
                    self.fd,
                    (&mut frame as *mut libc::can_frame).cast(),
                    frame_size,
                )
            };
            if n < 0 {
                let err = std::io::Error::last_os_error();
                if err.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                if matches!(
                    err.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) {
                    return Err(DeviceError::Timeout);
                }
                return Err(DeviceError::Transport(format!("SocketCAN read() failed: {err}")));
            }
            if (n as usize) < frame_size {
                return Err(DeviceError::Transport(format!(
                    "SocketCAN EOF / short read: {n} of {frame_size} bytes"
                )));
            }
            if (frame.can_id & (libc::CAN_ERR_FLAG | libc::CAN_RTR_FLAG)) != 0 {
                continue;
            }
            let is_extended = (frame.can_id & libc::CAN_EFF_FLAG) != 0;
            let raw_id = if is_extended {
                frame.can_id & libc::CAN_EFF_MASK
            } else {
                frame.can_id & libc::CAN_SFF_MASK
            };
            let dlc = (frame.can_dlc as usize).min(8);
            return Ok((raw_id, is_extended, frame.data[..dlc].to_vec()));
        }
    }
}

#[cfg(target_os = "linux")]
impl Drop for SocketCanDevice {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

impl Device for SocketCanDevice {
    fn open(&mut self) -> Result<(), DeviceError> {
        #[cfg(target_os = "linux")]
        {
            use std::ffi::CString;
            if self.fd >= 0 {
                return Err(DeviceError::AlreadyOpen);
            }
            let c_iface = CString::new(self.interface.as_str())
                .map_err(|_| DeviceError::NotFound("Invalid CAN interface name".to_string()))?;
            let ifindex = unsafe { libc::if_nametoindex(c_iface.as_ptr()) };
            if ifindex == 0 {
                return Err(DeviceError::NotFound(format!(
                    "SocketCAN interface '{}' not found",
                    self.interface
                )));
            }
            let fd = unsafe { libc::socket(libc::PF_CAN, libc::SOCK_RAW, libc::CAN_RAW) };
            if fd < 0 {
                return Err(DeviceError::Transport(format!(
                    "SocketCAN socket() failed: {}",
                    std::io::Error::last_os_error()
                )));
            }
            let mut addr: libc::sockaddr_can = unsafe { std::mem::zeroed() };
            addr.can_family = libc::AF_CAN as libc::sa_family_t;
            addr.can_ifindex = ifindex as libc::c_int;
            let rc = unsafe {
                libc::bind(
                    fd,
                    (&addr as *const libc::sockaddr_can).cast(),
                    std::mem::size_of::<libc::sockaddr_can>() as libc::socklen_t,
                )
            };
            if rc < 0 {
                let err = std::io::Error::last_os_error();
                unsafe { libc::close(fd) };
                return Err(DeviceError::Transport(format!("SocketCAN bind() failed: {err}")));
            }
            let tv = libc::timeval {
                tv_sec: 1,
                tv_usec: 0,
            };
            unsafe {
                libc::setsockopt(
                    fd,
                    libc::SOL_SOCKET,
                    libc::SO_RCVTIMEO,
                    (&tv as *const libc::timeval).cast(),
                    std::mem::size_of::<libc::timeval>() as libc::socklen_t,
                );
                libc::setsockopt(
                    fd,
                    libc::SOL_SOCKET,
                    libc::SO_SNDTIMEO,
                    (&tv as *const libc::timeval).cast(),
                    std::mem::size_of::<libc::timeval>() as libc::socklen_t,
                );
            }
            self.fd = fd;
            Ok(())
        }
        #[cfg(not(target_os = "linux"))]
        {
            Err(DeviceError::Unsupported("SocketCAN requires Linux"))
        }
    }

    fn close(&mut self) -> Result<(), DeviceError> {
        #[cfg(target_os = "linux")]
        {
            if self.fd >= 0 {
                unsafe { libc::close(self.fd) };
                self.fd = -1;
            }
        }
        self.protocol = None;
        Ok(())
    }

    fn connect(&mut self, protocol: ProtocolId, _baud_rate: u32) -> Result<(), DeviceError> {
        if !self.is_open() {
            return Err(DeviceError::NotOpen);
        }
        if !matches!(protocol, ProtocolId::Can | ProtocolId::Iso15765) {
            return Err(DeviceError::Unsupported(
                "SocketCAN supports only ProtocolId::Can and ProtocolId::Iso15765",
            ));
        }
        self.protocol = Some(protocol);
        Ok(())
    }

    fn disconnect(&mut self) -> Result<(), DeviceError> {
        if !self.is_open() {
            return Err(DeviceError::NotOpen);
        }
        self.protocol = None;
        Ok(())
    }

    fn send(&mut self, msg: &PassThruMsg) -> Result<(), DeviceError> {
        #[cfg(target_os = "linux")]
        {
            if self.fd < 0 {
                return Err(DeviceError::NotOpen);
            }
            let proto = self.protocol.ok_or(DeviceError::NotConnected)?;
            if msg.data.len() < 4 {
                return Err(DeviceError::Transport("CAN frame must carry 4-byte ID header".to_string()));
            }
            let can_id = u32::from_be_bytes([msg.data[0], msg.data[1], msg.data[2], msg.data[3]]);
            let is_29 = can_id > libc::CAN_SFF_MASK || (msg.tx_flags & tx_flags::CAN_29BIT_ID) != 0;
            let payload = &msg.data[4..];

            if !proto.is_iso15765() {
                return self.write_raw_can_frame(can_id, is_29, payload);
            }

            let frames = encode_isotp_frames(can_id, payload)?;
            if frames.len() == 1 {
                return self.write_raw_can_frame(can_id, is_29, &frames[0].1);
            }

            // Segmented ISO 15765-2 TX: First Frame -> wait FlowControl -> Consecutive Frames.
            self.write_raw_can_frame(can_id, is_29, &frames[0].1)?;
            let mut idx = 1usize;
            while idx < frames.len() {
                let fc_deadline = std::time::Instant::now() + std::time::Duration::from_millis(800);
                let fc = loop {
                    if std::time::Instant::now() >= fc_deadline {
                        return Err(DeviceError::Timeout);
                    }
                    let (rx_id, _, rx_data) = self.read_raw_can_frame()?;
                    if !is_matching_isotp_rx_id(can_id, self.rx_can_id, rx_id) {
                        continue;
                    }
                    if let Some(flow) = parse_isotp_flow_control(&rx_data) {
                        if flow.flow_status == 0x00 {
                            break flow;
                        }
                        if flow.flow_status == 0x01 {
                            continue; // Wait (WT)
                        }
                        return Err(DeviceError::Transport("ISO-TP FlowControl buffer overflow (0x32)".into()));
                    }
                };
                let limit = if fc.block_size == 0 {
                    frames.len() - idx
                } else {
                    usize::from(fc.block_size)
                };
                for _ in 0..limit {
                    if idx >= frames.len() {
                        break;
                    }
                    self.write_raw_can_frame(can_id, is_29, &frames[idx].1)?;
                    idx += 1;
                    if idx < frames.len() && fc.st_min_ms > 0 {
                        std::thread::sleep(std::time::Duration::from_millis(fc.st_min_ms));
                    }
                }
            }
            Ok(())
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = msg;
            Err(DeviceError::Unsupported("SocketCAN requires Linux"))
        }
    }

    fn receive(&mut self) -> Result<PassThruMsg, DeviceError> {
        #[cfg(target_os = "linux")]
        {
            if self.fd < 0 {
                return Err(DeviceError::NotOpen);
            }
            let proto = self.protocol.ok_or(DeviceError::NotConnected)?;
            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1200);
            let mut isotp = IsoTpRxReassembler::default();
            loop {
                if std::time::Instant::now() >= deadline {
                    return Err(DeviceError::Timeout);
                }
                let (raw_id, is_extended, payload) = self.read_raw_can_frame()?;
                if !proto.is_iso15765() {
                    let mut data = Vec::with_capacity(4 + payload.len());
                    data.extend_from_slice(&raw_id.to_be_bytes());
                    data.extend_from_slice(&payload);
                    let mut msg = PassThruMsg::new(proto, data);
                    if is_extended {
                        msg.rx_status |= rx_status::CAN_29BIT_ID;
                    }
                    return Ok(msg);
                }

                match isotp.feed(self.tx_can_id, self.rx_can_id, raw_id, &payload) {
                    IsoTpRxStep::Complete { rx_id, payload: assembled } => {
                        let mut data = Vec::with_capacity(4 + assembled.len());
                        data.extend_from_slice(&rx_id.to_be_bytes());
                        data.extend_from_slice(&assembled);
                        let mut msg = PassThruMsg::new(proto, data);
                        if is_extended {
                            msg.rx_status |= rx_status::CAN_29BIT_ID;
                        }
                        return Ok(msg);
                    }
                    IsoTpRxStep::Incomplete { send_flow_control_to: Some(fc_tx) } => {
                        let fc_frame = [0x30u8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
                        let _ = self.write_raw_can_frame(fc_tx, fc_tx > libc::CAN_SFF_MASK, &fc_frame);
                    }
                    IsoTpRxStep::Incomplete { send_flow_control_to: None } | IsoTpRxStep::Ignored => {}
                }
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            Err(DeviceError::Unsupported("SocketCAN requires Linux"))
        }
    }

    fn read_voltage_millivolts(&mut self) -> Result<u32, DeviceError> {
        if !self.is_open() {
            return Err(DeviceError::NotOpen);
        }
        Err(DeviceError::Unsupported(
            "SocketCAN interface has no battery voltage sensor",
        ))
    }

    fn info(&self) -> DeviceInfo {
        DeviceInfo {
            kind: DeviceKind::SocketCan,
            name: format!("Linux SocketCAN ({})", self.interface),
            transport: TransportKind::Can,
            firmware_version: Some("CAN_RAW+ISOTP".to_string()),
            dll_version: None,
        }
    }

    fn is_open(&self) -> bool {
        #[cfg(target_os = "linux")]
        {
            self.fd >= 0
        }
        #[cfg(not(target_os = "linux"))]
        {
            false
        }
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
        self.tx_can_id = tx_id;
        self.rx_can_id = rx_id;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_isotp_handles_single_and_multiframe_with_zero_padding() {
        let sf = encode_isotp_frames(0x7E0, &[0x10, 0x03]).unwrap();
        assert_eq!(sf.len(), 1);
        assert_eq!(sf[0], (0x7E0, [0x02, 0x10, 0x03, 0, 0, 0, 0, 0]));

        let payload: Vec<u8> = (0u8..16).collect();
        let mf = encode_isotp_frames(0x7E0, &payload).unwrap();
        assert_eq!(mf.len(), 3);
        assert_eq!(mf[0].1, [0x10, 16, 0, 1, 2, 3, 4, 5]);
        assert_eq!(mf[1].1, [0x21, 6, 7, 8, 9, 10, 11, 12]);
        assert_eq!(mf[2].1, [0x22, 13, 14, 15, 0, 0, 0, 0]);
    }

    #[test]
    fn isotp_reassembler_handles_functional_broadcast_and_multiframe_fc() {
        let mut rx = IsoTpRxReassembler::default();
        // Broadcast single frame reply from 0x7E9 while configured TX=0x7DF
        let step1 = rx.feed(0x7DF, 0x7E8, 0x7E9, &[0x03, 0x7F, 0x22, 0x11, 0, 0, 0, 0]);
        assert_eq!(
            step1,
            IsoTpRxStep::Complete {
                rx_id: 0x7E9,
                payload: vec![0x7F, 0x22, 0x11],
            }
        );

        // Multi-frame VIN (20 bytes = 0x14) from 0x7E8
        let s1 = rx.feed(0x7E0, 0x7E8, 0x7E8, &[0x10, 20, b'A', b'B', b'C', b'D', b'E', b'F']);
        assert_eq!(
            s1,
            IsoTpRxStep::Incomplete {
                send_flow_control_to: Some(0x7E0),
            }
        );
        let s2 = rx.feed(0x7E0, 0x7E8, 0x7E8, &[0x21, b'G', b'H', b'I', b'J', b'K', b'L', b'M']);
        assert_eq!(
            s2,
            IsoTpRxStep::Incomplete {
                send_flow_control_to: None,
            }
        );
        let s3 = rx.feed(0x7E0, 0x7E8, 0x7E8, &[0x22, b'N', b'O', b'P', b'Q', b'R', b'S', b'T']);
        assert_eq!(
            s3,
            IsoTpRxStep::Complete {
                rx_id: 0x7E8,
                payload: b"ABCDEFGHIJKLMNOPQRST".to_vec(),
            }
        );
    }

    #[test]
    fn configure_can_ids_tracks_active_tx_rx_ids() {
        let mut dev = SocketCanDevice::new("vcan0");
        assert_eq!(dev.configured_can_ids(), (0x7E0, 0x7E8));
        dev.configure_can_ids(0x18DA_10F1, 0x18DA_F110).unwrap();
        assert_eq!(dev.configured_can_ids(), (0x18DA_10F1, 0x18DA_F110));
    }
}
