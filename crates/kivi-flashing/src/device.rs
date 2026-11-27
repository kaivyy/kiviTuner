//! Device abstraction — the seam that was missing.
//!
//! ## Why this module exists
//!
//! Every other module in this crate builds or parses bytes. None of them can
//! carry those bytes to an ECU. This module defines the contract a transport
//! must satisfy, plus a deterministic in-memory implementation so the whole
//! chain can be exercised without hardware.
//!
//! ## Design
//!
//! [`Device`] is deliberately object-safe: the engine holds `Box<dyn Device>`
//! so a J2534 DLL, a serial port, SocketCAN, and a DoIP socket are interchangeable
//! behind one uniform transport contract.
//!
//! ## What is real and what is not
//!
//! [`MockDevice`] is a scripted stand-in. It is **not** a simulator of ECU
//! behaviour — it replays exactly the replies a test supplies, and times out
//! when the script runs dry. That makes it useful for pinning the orchestrator's
//! control flow and useless for claiming protocol conformance.

use crate::j2534_types::{J2534Error, PassThruMsg, ProtocolId};

/// What kind of physical interface a device speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeviceKind {
    /// A SAE J2534 pass-thru interface (Scanmatik, Tactrix, Mongoose, …).
    J2534,
    /// Native OpenPort 2.0 USB-CDC serial device.
    OpenPort2,
    /// A raw serial / USB-CDC adapter (ELM327, OBDX Pro, …).
    Serial,
    /// Diagnostics over IP (ISO 13400).
    DoIp,
    /// Native Linux SocketCAN interface (`can0`, `vcan0`, ...).
    SocketCan,
    /// A test double.
    #[cfg(any(test, feature = "test-doubles"))]
    Mock,
}

impl DeviceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            DeviceKind::J2534 => "j2534",
            DeviceKind::OpenPort2 => "openport2",
            DeviceKind::Serial => "serial",
            DeviceKind::DoIp => "doip",
            DeviceKind::SocketCan => "socketcan",
            #[cfg(any(test, feature = "test-doubles"))]
            DeviceKind::Mock => "mock",
        }
    }
}

/// The physical link beneath a device, for display and for choosing defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransportKind {
    Usb,
    Serial,
    Ethernet,
    Bluetooth,
    Can,
    /// A test double with no physical link.
    #[cfg(any(test, feature = "test-doubles"))]
    InMemory,
}

impl TransportKind {
    pub fn as_str(self) -> &'static str {
        match self {
            TransportKind::Usb => "Usb",
            TransportKind::Serial => "Serial",
            TransportKind::Ethernet => "Ethernet",
            TransportKind::Bluetooth => "Bluetooth",
            TransportKind::Can => "Can",
            #[cfg(any(test, feature = "test-doubles"))]
            TransportKind::InMemory => "InMemory",
        }
    }
}

/// Static description of a device, suitable for showing in a picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    pub kind: DeviceKind,
    pub name: String,
    pub transport: TransportKind,
    /// Vendor-reported firmware string, when the driver supplies one.
    pub firmware_version: Option<String>,
    /// Vendor-reported DLL version, when the driver supplies one.
    pub dll_version: Option<String>,
}

/// Everything that can go wrong at the device layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceError {
    /// A lifecycle call arrived out of order.
    NotOpen,
    /// A second `open` on an already-open device.
    AlreadyOpen,
    /// A send/receive arrived before `connect`.
    NotConnected,
    /// No reply arrived within the caller's deadline.
    Timeout,
    /// The driver returned a non-zero status.
    Driver(J2534Error),
    /// The device is not present at the configured path.
    NotFound(String),
    /// The transport itself failed (I/O, disconnection).
    Transport(String),
    /// The device does not implement an optional capability.
    Unsupported(&'static str),
}

impl std::fmt::Display for DeviceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeviceError::NotOpen => f.write_str("device is not open"),
            DeviceError::AlreadyOpen => f.write_str("device is already open"),
            DeviceError::NotConnected => f.write_str("no channel is connected"),
            DeviceError::Timeout => f.write_str("the device did not reply in time"),
            DeviceError::Driver(e) => write!(f, "driver error: {e}"),
            DeviceError::NotFound(p) => write!(f, "device not found at '{p}'"),
            DeviceError::Transport(m) => write!(f, "transport failure: {m}"),
            DeviceError::Unsupported(what) => write!(f, "unsupported: {what}"),
        }
    }
}

impl std::error::Error for DeviceError {}

/// The contract every transport must satisfy.
///
/// Object-safe on purpose: [`crate::flash_engine`] owns a `Box<dyn Device>`.
pub trait Device {
    /// Load the driver and prepare for a channel.
    fn open(&mut self) -> Result<(), DeviceError>;
    /// Release the driver. Must be idempotent.
    fn close(&mut self) -> Result<(), DeviceError>;
    /// Open a channel on `protocol` at `baud_rate` bits per second.
    fn connect(&mut self, protocol: ProtocolId, baud_rate: u32) -> Result<(), DeviceError>;
    /// Close the channel, leaving the driver loaded.
    fn disconnect(&mut self) -> Result<(), DeviceError>;
    /// Transmit one message.
    fn send(&mut self, msg: &PassThruMsg) -> Result<(), DeviceError>;
    /// Receive one message, or [`DeviceError::Timeout`].
    fn receive(&mut self) -> Result<PassThruMsg, DeviceError>;
    /// Battery voltage in millivolts — the pre-flash interlock input.
    fn read_voltage_millivolts(&mut self) -> Result<u32, DeviceError>;
    /// Static description for a device picker.
    fn info(&self) -> DeviceInfo;
    fn is_open(&self) -> bool;
    fn is_connected(&self) -> bool;
    fn connected_protocol(&self) -> Option<ProtocolId>;
    /// `true` only for test doubles. Used to keep a mock out of a real flash.
    fn is_mock(&self) -> bool;
    /// Whether `Can` / `Iso15765` frames carry a 4-byte BE CAN arbitration ID prefix on TX and RX.
    fn uses_can_id_prefix(&self) -> bool {
        true
    }
}

/// One scripted device reply.
#[cfg(any(test, feature = "test-doubles"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptedReply {
    /// Deliver this message.
    Message(PassThruMsg),
    /// Fail with this driver status.
    Error(J2534Error),
}

/// A deterministic in-memory device for tests.
///
/// Replays a fixed script and times out once it is exhausted, so a test that
/// under-supplies replies fails loudly instead of hanging.
#[cfg(any(test, feature = "test-doubles"))]
#[derive(Debug, Clone)]
pub struct MockDevice {
    info: DeviceInfo,
    open: bool,
    protocol: Option<ProtocolId>,
    script: Vec<ScriptedReply>,
    cursor: usize,
    sent: Vec<PassThruMsg>,
    voltage_mv: u32,
    /// Simulate `open` failing.
    pub fail_open: Option<DeviceError>,
    /// Simulate `connect` failing.
    pub fail_connect: Option<DeviceError>,
    can_id_prefix: bool,
}

#[cfg(any(test, feature = "test-doubles"))]
impl MockDevice {
    pub fn new(kind: DeviceKind, name: impl Into<String>, transport: TransportKind) -> Self {
        Self {
            info: DeviceInfo {
                kind,
                name: name.into(),
                transport,
                firmware_version: None,
                dll_version: None,
            },
            open: false,
            protocol: None,
            script: Vec::new(),
            cursor: 0,
            sent: Vec::new(),
            voltage_mv: 12_600,
            fail_open: None,
            fail_connect: None,
            can_id_prefix: false,
        }
    }

    /// Configure whether mock CAN/ISO15765 frames expect and return a 4-byte CAN ID prefix.
    pub fn with_can_id_prefix(mut self, enabled: bool) -> Self {
        self.can_id_prefix = enabled;
        self
    }

    /// Replace the reply script and rewind the cursor.
    pub fn set_script(&mut self, script: Vec<ScriptedReply>) {
        self.script = script;
        self.cursor = 0;
    }

    /// Append replies without disturbing the cursor.
    pub fn push_reply(&mut self, reply: ScriptedReply) {
        self.script.push(reply);
    }

    /// Set what [`Device::read_voltage_millivolts`] will report.
    pub fn set_voltage_millivolts(&mut self, mv: u32) {
        self.voltage_mv = mv;
    }

    /// Every message sent so far, in order.
    pub fn sent_messages(&self) -> Vec<PassThruMsg> {
        self.sent.clone()
    }

    /// How many scripted replies remain.
    pub fn remaining_replies(&self) -> usize {
        self.script.len().saturating_sub(self.cursor)
    }
}

#[cfg(any(test, feature = "test-doubles"))]
impl Device for MockDevice {
    fn open(&mut self) -> Result<(), DeviceError> {
        if let Some(err) = self.fail_open.clone() {
            return Err(err);
        }
        if self.open {
            return Err(DeviceError::AlreadyOpen);
        }
        self.open = true;
        Ok(())
    }

    fn close(&mut self) -> Result<(), DeviceError> {
        // Idempotent by contract: teardown runs on paths that already failed.
        self.open = false;
        self.protocol = None;
        Ok(())
    }

    fn connect(&mut self, protocol: ProtocolId, _baud_rate: u32) -> Result<(), DeviceError> {
        if !self.open {
            return Err(DeviceError::NotOpen);
        }
        if let Some(err) = self.fail_connect.clone() {
            return Err(err);
        }
        self.protocol = Some(protocol);
        Ok(())
    }

    fn disconnect(&mut self) -> Result<(), DeviceError> {
        if !self.open {
            return Err(DeviceError::NotOpen);
        }
        self.protocol = None;
        Ok(())
    }

    fn send(&mut self, msg: &PassThruMsg) -> Result<(), DeviceError> {
        if !self.open {
            return Err(DeviceError::NotOpen);
        }
        if self.protocol.is_none() {
            return Err(DeviceError::NotConnected);
        }
        self.sent.push(msg.clone());
        Ok(())
    }

    fn receive(&mut self) -> Result<PassThruMsg, DeviceError> {
        if !self.open {
            return Err(DeviceError::NotOpen);
        }
        if self.protocol.is_none() {
            return Err(DeviceError::NotConnected);
        }
        match self.script.get(self.cursor) {
            Some(ScriptedReply::Message(m)) => {
                self.cursor += 1;
                Ok(m.clone())
            }
            Some(ScriptedReply::Error(e)) => {
                let e = *e;
                self.cursor += 1;
                Err(DeviceError::Driver(e))
            }
            None => Err(DeviceError::Timeout),
        }
    }

    fn read_voltage_millivolts(&mut self) -> Result<u32, DeviceError> {
        if !self.open {
            return Err(DeviceError::NotOpen);
        }
        Ok(self.voltage_mv)
    }

    fn info(&self) -> DeviceInfo {
        self.info.clone()
    }

    fn is_open(&self) -> bool {
        self.open
    }

    fn is_connected(&self) -> bool {
        self.protocol.is_some()
    }

    fn connected_protocol(&self) -> Option<ProtocolId> {
        self.protocol
    }

    fn is_mock(&self) -> bool {
        true
    }

    fn uses_can_id_prefix(&self) -> bool {
        self.can_id_prefix
    }
}

/// Forwarding implementation so a `Box<dyn Device>` is itself a `Device`.
///
/// Without this, a struct holding `Box<dyn Device>` cannot be passed to
/// [`crate::flash_engine::dump_ecu`], which takes `&mut D where D: Device`. The
/// alternative — making the engine generic over `Box<dyn Device>` everywhere —
/// would leak the storage choice into every call site.
impl<T: Device + ?Sized> Device for Box<T> {
    fn open(&mut self) -> Result<(), DeviceError> {
        (**self).open()
    }
    fn close(&mut self) -> Result<(), DeviceError> {
        (**self).close()
    }
    fn connect(&mut self, protocol: ProtocolId, baud_rate: u32) -> Result<(), DeviceError> {
        (**self).connect(protocol, baud_rate)
    }
    fn disconnect(&mut self) -> Result<(), DeviceError> {
        (**self).disconnect()
    }
    fn send(&mut self, msg: &PassThruMsg) -> Result<(), DeviceError> {
        (**self).send(msg)
    }
    fn receive(&mut self) -> Result<PassThruMsg, DeviceError> {
        (**self).receive()
    }
    fn read_voltage_millivolts(&mut self) -> Result<u32, DeviceError> {
        (**self).read_voltage_millivolts()
    }
    fn info(&self) -> DeviceInfo {
        (**self).info()
    }
    fn is_open(&self) -> bool {
        (**self).is_open()
    }
    fn is_connected(&self) -> bool {
        (**self).is_connected()
    }
    fn connected_protocol(&self) -> Option<ProtocolId> {
        (**self).connected_protocol()
    }
    fn is_mock(&self) -> bool {
        (**self).is_mock()
    }

    fn uses_can_id_prefix(&self) -> bool {
        (**self).uses_can_id_prefix()
    }
}
