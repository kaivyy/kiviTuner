//! Native Linux SocketCAN (`AF_CAN` / `SOCK_RAW` / `CAN_RAW`) [`Device`] via `libc`.
//!
//! Uses only workspace `libc` on Linux (`can0`, `vcan0`, etc.). Returns
//! `DeviceError::Unsupported` cleanly on non-Linux hosts.

use crate::device::{Device, DeviceError, DeviceInfo, DeviceKind, TransportKind};
use crate::j2534_types::{PassThruMsg, ProtocolId};
#[cfg(target_os = "linux")]
use crate::j2534_types::{rx_status, tx_flags};

pub struct SocketCanDevice {
    interface: String,
    #[cfg(target_os = "linux")]
    fd: i32,
    #[cfg(not(target_os = "linux"))]
    _open_unused: bool,
    protocol: Option<ProtocolId>,
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
        if protocol.is_iso15765() {
            return Err(DeviceError::Unsupported(
                "SocketCAN CAN_RAW does not implement ISO 15765-2 segmentation; use J2534 or CAN_ISOTP",
            ));
        }
        if !matches!(protocol, ProtocolId::Can) {
            return Err(DeviceError::Unsupported(
                "SocketCAN CAN_RAW supports only ProtocolId::Can",
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
            if self.protocol.is_none() {
                return Err(DeviceError::NotConnected);
            }
            if msg.data.len() < 4 {
                return Err(DeviceError::Transport("CAN frame must carry 4-byte ID header".to_string()));
            }
            let can_id = u32::from_be_bytes([msg.data[0], msg.data[1], msg.data[2], msg.data[3]]);
            let payload = &msg.data[4..];
            if payload.len() > 8 {
                return Err(DeviceError::Transport(format!(
                    "SocketCAN CAN_RAW frame payload {} bytes exceeds 8-byte classic CAN MTU",
                    payload.len()
                )));
            }
            let mut frame: libc::can_frame = unsafe { std::mem::zeroed() };
            frame.can_id = if can_id > libc::CAN_SFF_MASK || (msg.tx_flags & tx_flags::CAN_29BIT_ID) != 0 {
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
                    return Err(DeviceError::Transport(format!(
                        "SocketCAN write() failed: {err}"
                    )));
                }
                if (n as usize) != frame_size {
                    return Err(DeviceError::Transport(format!(
                        "SocketCAN short write: {n} of {frame_size} bytes"
                    )));
                }
                return Ok(());
            }
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
            let frame_size = std::mem::size_of::<libc::can_frame>();
            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1200);
            loop {
                if std::time::Instant::now() >= deadline {
                    return Err(DeviceError::Timeout);
                }
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
                let mut data = Vec::with_capacity(4 + dlc);
                data.extend_from_slice(&raw_id.to_be_bytes());
                data.extend_from_slice(&frame.data[..dlc]);
                let mut msg = PassThruMsg::new(proto, data);
                if is_extended {
                    msg.rx_status |= rx_status::CAN_29BIT_ID;
                }
                return Ok(msg);
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
            firmware_version: Some("CAN_RAW".to_string()),
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
}
