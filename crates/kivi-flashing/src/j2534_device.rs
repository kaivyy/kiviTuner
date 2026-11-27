//! J2534 → [`Device`] adapter, behind the `j2534-ffi` feature.
//!
//! ## Why this module exists
//!
//! [`crate::j2534_ffi`] exposes the raw entry points; [`crate::device`] defines
//! the trait the flash engine drives. This is the join. Without it the two
//! halves of the chain cannot meet, and the "colok kabel" step stays theoretical.
//!
//! ## Lifecycle mapping
//!
//! | `Device` call | J2534 calls |
//! |---|---|
//! | `open` | `LoadLibrary` + `PassThruOpen` |
//! | `connect` | `PassThruConnect` (+ `PassThruStartMsgFilter` for ISO-TP) |
//! | `send` | `PassThruWriteMsgs` |
//! | `receive` | `PassThruReadMsgs` |
//! | `disconnect` | `PassThruDisconnect` |
//! | `close` | `PassThruClose` + `FreeLibrary` (via `Drop` of the handle) |
//!
//! ## Cleanup ordering
//!
//! `close` disconnects the channel **before** closing the device, and both are
//! attempted even if the first fails. A driver left with an open channel can
//! keep the bus busy; a driver left open can keep the interface powered.
//!
//! ## What is NOT verified
//!
//! No method here has been executed against a physical interface. On this Linux
//! host a Windows PE cannot be loaded at all, so every path that requires a real
//! driver is exercised only up to the loader's error. That boundary is stated in
//! the module docs and in the crate docs; it is not glossed over.

use crate::device::{Device, DeviceError, DeviceInfo, DeviceKind, TransportKind};
use crate::j2534_abi::PassthruMsgAbi;
use crate::j2534_ffi::{FfiError, J2534EntryPoints};
use crate::j2534_types::{
    connect_flags, filter_type, rx_status, tx_flags, J2534Error, PassThruMsg, ProtocolId,
};

/// Map a driver status into the device-layer error vocabulary.
pub fn map_driver_error(e: J2534Error) -> DeviceError {
    DeviceError::Driver(e)
}

/// Map an FFI failure into the device-layer error vocabulary.
fn map_ffi_error(e: FfiError) -> DeviceError {
    match e {
        FfiError::Load { path, message } => DeviceError::NotFound(format!("{path}: {message}")),
        FfiError::MissingSymbol { symbol, message } => {
            DeviceError::Transport(format!("missing symbol '{symbol}': {message}"))
        }
        FfiError::Driver(code) => DeviceError::Driver(J2534Error::from_u32(code)),
        FfiError::DriverWithMessage { code, message } => {
            DeviceError::Transport(format!("driver status {code:#04x}: {message}"))
        }
    }
}

/// A real J2534 interface presented through the [`Device`] trait.
///
/// Construct with [`J2534Device::unloaded`] and call [`Device::open`] to load
/// the driver. Loading is deferred so a device picker can list a driver that is
/// present on disk but not yet opened.
pub struct J2534Device {
    path: String,
    name: String,
    entry: Option<J2534EntryPoints>,
    device_id: Option<u32>,
    channel_id: Option<u32>,
    protocol: Option<ProtocolId>,
    read_timeout_ms: u32,
    tx_can_id: u32,
    rx_can_id: u32,
    firmware_version: Option<String>,
    dll_version: Option<String>,
}

impl Drop for J2534Device {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

impl J2534Device {
    /// An adapter for `path` that has not loaded the driver yet.
    pub fn unloaded(path: impl Into<String>) -> Self {
        let path = path.into();
        let name = std::path::Path::new(&path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("J2534 driver")
            .to_string();
        Self {
            path,
            name,
            entry: None,
            device_id: None,
            channel_id: None,
            protocol: None,
            read_timeout_ms: 1000,
            tx_can_id: 0x07E0,
            rx_can_id: 0x07E8,
            firmware_version: None,
            dll_version: None,
        }
    }

    /// Override default ISO-TP CAN arbitration IDs (`tx_id` default `0x7E0`, `rx_id` default `0x7E8`).
    pub fn with_can_ids(mut self, tx_id: u32, rx_id: u32) -> Self {
        self.tx_can_id = tx_id;
        self.rx_can_id = rx_id;
        self
    }

    /// Describe a driver without loading it.
    pub fn describe(path: impl AsRef<std::path::Path>) -> DeviceInfo {
        let name = path
            .as_ref()
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("J2534 driver")
            .to_string();
        DeviceInfo {
            kind: DeviceKind::J2534,
            name,
            transport: TransportKind::Usb,
            firmware_version: None,
            dll_version: None,
        }
    }

    /// Override the `PassThruReadMsgs` timeout.
    pub fn with_read_timeout_ms(mut self, ms: u32) -> Self {
        self.read_timeout_ms = ms;
        self
    }

    /// The driver path this adapter was built for.
    pub fn path(&self) -> &str {
        &self.path
    }
}

impl Device for J2534Device {
    fn open(&mut self) -> Result<(), DeviceError> {
        if self.entry.is_some() {
            return Err(DeviceError::AlreadyOpen);
        }
        let entry = J2534EntryPoints::load(&self.path).map_err(map_ffi_error)?;
        let device_id = entry.open_device(None).map_err(map_ffi_error)?;
        let (fw_ver, dll_ver) = match entry.read_version(device_id) {
            Ok((fw, dll, _api)) => (
                (!fw.trim().is_empty()).then_some(fw),
                (!dll.trim().is_empty()).then_some(dll),
            ),
            Err(_) => (None, None),
        };
        self.entry = Some(entry);
        self.device_id = Some(device_id);
        self.firmware_version = fw_ver;
        self.dll_version = dll_ver;
        Ok(())
    }

    fn close(&mut self) -> Result<(), DeviceError> {
        // Disconnect first, then close. Both are attempted even if the first
        // fails, because a channel left open keeps the bus busy.
        let mut first_error = None;

        if let (Some(entry), Some(channel)) = (self.entry.as_ref(), self.channel_id) {
            if let Err(e) = entry.disconnect(channel) {
                first_error = Some(map_ffi_error(e));
            }
        }
        self.channel_id = None;
        self.protocol = None;

        if let (Some(entry), Some(device)) = (self.entry.as_ref(), self.device_id) {
            if let Err(e) = entry.close_device(device) {
                if first_error.is_none() {
                    first_error = Some(map_ffi_error(e));
                }
            }
        }
        self.device_id = None;
        self.firmware_version = None;
        self.dll_version = None;
        // Dropping the handle runs FreeLibrary, which is the only teardown that
        // must never be skipped.
        self.entry = None;

        match first_error {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    fn connect(&mut self, protocol: ProtocolId, baud_rate: u32) -> Result<(), DeviceError> {
        let device_id = self.device_id.ok_or(DeviceError::NotOpen)?;
        let entry = self.entry.as_ref().ok_or(DeviceError::NotOpen)?;
        if let Some(old_channel) = self.channel_id.take() {
            let _ = entry.disconnect(old_channel);
            self.protocol = None;
        }

        let is_29bit = self.tx_can_id > 0x7FF || self.rx_can_id > 0x7FF;
        let conn_flags = if is_29bit && (protocol.is_iso15765() || protocol == ProtocolId::Can) {
            connect_flags::CAN_29BIT_ID
        } else {
            0
        };
        let channel = entry
            .connect(device_id, protocol.as_u32(), conn_flags, baud_rate)
            .map_err(map_ffi_error)?;
        self.channel_id = Some(channel);
        self.protocol = Some(protocol);

        // SAE J2534 blocks all incoming RX frames until a channel filter is active:
        // ISO 15765-2 needs a flow-control filter; raw CAN and K-Line/VPW/PWM need
        // a permissive pass-all filter so ECUs can respond without dropping frames.
        let filter_res = if protocol.is_iso15765() {
            let flags = tx_flags::ISO15765_FRAME_PAD
                | if is_29bit { tx_flags::CAN_29BIT_ID } else { 0 };
            let mask = PassthruMsgAbi::from_msg(&PassThruMsg::with_flags(
                ProtocolId::Iso15765,
                flags,
                vec![0xFF, 0xFF, 0xFF, 0xFF],
            ));
            let pattern = PassthruMsgAbi::from_msg(&PassThruMsg::with_flags(
                ProtocolId::Iso15765,
                flags,
                self.rx_can_id.to_be_bytes().to_vec(),
            ));
            let flow = PassthruMsgAbi::from_msg(&PassThruMsg::with_flags(
                ProtocolId::Iso15765,
                flags,
                self.tx_can_id.to_be_bytes().to_vec(),
            ));
            entry.start_msg_filter(
                channel,
                filter_type::FLOW_CONTROL_FILTER,
                Some(&mask),
                Some(&pattern),
                Some(&flow),
            )
        } else if protocol == ProtocolId::Can {
            let can_flags = if is_29bit { tx_flags::CAN_29BIT_ID } else { 0 };
            let mask = PassthruMsgAbi::from_msg(&PassThruMsg::with_flags(
                protocol,
                can_flags,
                vec![0, 0, 0, 0],
            ));
            let pattern = PassthruMsgAbi::from_msg(&PassThruMsg::with_flags(
                protocol,
                can_flags,
                vec![0, 0, 0, 0],
            ));
            entry.start_msg_filter(
                channel,
                filter_type::PASS_FILTER,
                Some(&mask),
                Some(&pattern),
                None,
            )
        } else {
            let mask = PassthruMsgAbi::from_msg(&PassThruMsg::new(protocol, vec![0]));
            let pattern = PassthruMsgAbi::from_msg(&PassThruMsg::new(protocol, vec![0]));
            entry.start_msg_filter(
                channel,
                filter_type::PASS_FILTER,
                Some(&mask),
                Some(&pattern),
                None,
            )
        };

        if let Err(e) = filter_res {
            // Roll the channel back so the adapter does not claim a connected state it cannot use.
            let _ = entry.disconnect(channel);
            self.channel_id = None;
            self.protocol = None;
            return Err(map_ffi_error(e));
        }

        Ok(())
    }

    fn disconnect(&mut self) -> Result<(), DeviceError> {
        let entry = self.entry.as_ref().ok_or(DeviceError::NotOpen)?;
        let channel = self.channel_id.ok_or(DeviceError::NotConnected)?;
        let result = entry.disconnect(channel).map_err(map_ffi_error);
        self.channel_id = None;
        self.protocol = None;
        result
    }

    fn send(&mut self, msg: &PassThruMsg) -> Result<(), DeviceError> {
        let entry = self.entry.as_ref().ok_or(DeviceError::NotOpen)?;
        let channel = self.channel_id.ok_or(DeviceError::NotConnected)?;
        let abi = PassthruMsgAbi::try_from_msg(msg)?;
        entry
            .write_msg(channel, &abi, 2000)
            .map(|_| ())
            .map_err(map_ffi_error)
    }

    fn receive(&mut self) -> Result<PassThruMsg, DeviceError> {
        let timeout = self.read_timeout_ms;
        let entry = self.entry.as_ref().ok_or(DeviceError::NotOpen)?;
        let channel = self.channel_id.ok_or(DeviceError::NotConnected)?;
        let deadline = std::time::Instant::now()
            + std::time::Duration::from_millis(timeout as u64);
        loop {
            let rem_ms = deadline
                .saturating_duration_since(std::time::Instant::now())
                .as_millis()
                .max(1)
                .min(u32::MAX as u128) as u32;
            match entry.read_msg(channel, rem_ms) {
                Ok(abi) => {
                    let skip_flags =
                        rx_status::TX_MSG_TYPE | rx_status::START_OF_MESSAGE | rx_status::TX_DONE;
                    if (abi.rx_status & skip_flags) != 0 || abi.data_size == 0 {
                        if std::time::Instant::now() >= deadline {
                            return Err(DeviceError::Timeout);
                        }
                        continue;
                    }
                    return Ok(abi.to_msg());
                }
                Err(FfiError::Driver(0x10))
                | Err(FfiError::DriverWithMessage { code: 0x10, .. }) => {
                    if std::time::Instant::now() >= deadline {
                        return Err(DeviceError::Timeout);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(2));
                    continue;
                }
                Err(FfiError::Driver(0x09))
                | Err(FfiError::DriverWithMessage { code: 0x09, .. }) => {
                    return Err(DeviceError::Timeout);
                }
                Err(e) => return Err(map_ffi_error(e)),
            }
        }
    }

    fn read_voltage_millivolts(&mut self) -> Result<u32, DeviceError> {
        let device_id = self.device_id.ok_or(DeviceError::NotOpen)?;
        let entry = self.entry.as_ref().ok_or(DeviceError::NotOpen)?;
        entry
            .read_voltage_millivolts(device_id)
            .map_err(map_ffi_error)
    }

    fn info(&self) -> DeviceInfo {
        DeviceInfo {
            kind: DeviceKind::J2534,
            name: self.name.clone(),
            transport: TransportKind::Usb,
            firmware_version: self.firmware_version.clone(),
            dll_version: self.dll_version.clone(),
        }
    }

    fn is_open(&self) -> bool {
        self.device_id.is_some()
    }

    fn is_connected(&self) -> bool {
        self.channel_id.is_some()
    }

    fn connected_protocol(&self) -> Option<ProtocolId> {
        self.protocol
    }

    fn is_mock(&self) -> bool {
        false
    }
}
