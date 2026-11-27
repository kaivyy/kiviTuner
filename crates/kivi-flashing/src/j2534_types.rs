//! J2534 type layer: protocol IDs, error codes, message flags and the wire
//! message struct.
//!
//! ## Specification
//!
//! Constants follow the published SAE J2534-1 v04.04 PassThru API specification.
//!
//! ## Why a separate module
//!
//! `j2534_channel.rs` already builds ISO-TP frames, but it deliberately has no
//! notion of a *device*. These types are the vocabulary a device layer needs:
//! what protocol to speak, what a failure looks like, and what a message is on
//! the wire.

use std::fmt;

/// SAE J2534 protocol identifier (`ProtocolID` in the C header).
///
/// Only the values the C header defines are represented. Unknown values are
/// preserved verbatim through [`ProtocolId::Unknown`] so a device that reports
/// something exotic round-trips instead of being silently coerced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProtocolId {
    J1850Vpw,
    J1850Pwm,
    Iso9141,
    Iso14230,
    Can,
    Iso15765,
    SciAEngine,
    SciATrans,
    SciBEngine,
    SciBTrans,
    Unknown(u32),
}

impl ProtocolId {
    /// The raw numeric value the J2534 driver expects.
    pub fn as_u32(self) -> u32 {
        match self {
            ProtocolId::J1850Vpw => 0x01,
            ProtocolId::J1850Pwm => 0x02,
            ProtocolId::Iso9141 => 0x03,
            ProtocolId::Iso14230 => 0x04,
            ProtocolId::Can => 0x05,
            ProtocolId::Iso15765 => 0x06,
            ProtocolId::SciAEngine => 0x07,
            ProtocolId::SciATrans => 0x08,
            ProtocolId::SciBEngine => 0x09,
            ProtocolId::SciBTrans => 0x0A,
            ProtocolId::Unknown(v) => v,
        }
    }

    /// Decode a raw protocol value.
    pub fn from_u32(value: u32) -> Self {
        match value {
            0x01 => ProtocolId::J1850Vpw,
            0x02 => ProtocolId::J1850Pwm,
            0x03 => ProtocolId::Iso9141,
            0x04 => ProtocolId::Iso14230,
            0x05 => ProtocolId::Can,
            0x06 => ProtocolId::Iso15765,
            0x07 => ProtocolId::SciAEngine,
            0x08 => ProtocolId::SciATrans,
            0x09 => ProtocolId::SciBEngine,
            0x0A => ProtocolId::SciBTrans,
            other => ProtocolId::Unknown(other),
        }
    }

    /// Whether this protocol carries ISO 15765-2 segmented transfers.
    ///
    /// Only these can carry a multi-frame kernel upload; the rest are either
    /// single-frame (J1850) or not diagnostic at all (SCI).
    pub fn is_iso15765(self) -> bool {
        matches!(self, ProtocolId::Iso15765)
    }
}

/// A J2534 failure, mirroring the `ERR_*` codes in the SAE J2534-1 v04.04 C header (`0x00..=0x1A`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum J2534Error {
    /// 0x00 — function completed successfully.
    Success,
    /// 0x01 — the function option is not supported.
    NotSupported,
    /// 0x02 — the channel handle is not recognised.
    InvalidChannelId,
    /// 0x03 — the protocol identifier is not recognised.
    InvalidProtocolId,
    /// 0x04 — a required pointer was null.
    NullParameter,
    /// 0x05 — an ioctl parameter value is not recognised.
    InvalidIoctlValue,
    /// 0x06 — a flags field contains an invalid value.
    InvalidFlags,
    /// 0x07 — unspecified failure; call `PassThruGetLastError` for detail.
    Failed,
    /// 0x08 — communication with the hardware interface has been lost.
    DeviceNotConnected,
    /// 0x09 — operation timed out before completion.
    Timeout,
    /// 0x0A — message structure or payload invalid.
    InvalidMsg,
    /// 0x0B — periodic message time interval out of range.
    InvalidTimeInterval,
    /// 0x0C — device channel or filter limit exceeded.
    ExceededLimit,
    /// 0x0D — periodic message ID not recognised.
    InvalidMsgId,
    /// 0x0E — device is already open by another client or handle.
    DeviceInUse,
    /// 0x0F — ioctl ID is not recognised.
    InvalidIoctlId,
    /// 0x10 — receive buffer contains no matching messages.
    BufferEmpty,
    /// 0x11 — transmit queue full.
    BufferFull,
    /// 0x12 — receive queue overflowed; oldest data lost.
    BufferOverflow,
    /// 0x13 — pin number invalid for this hardware.
    PinInvalid,
    /// 0x14 — protocol channel is already open.
    ChannelInUse,
    /// 0x15 — message ProtocolID does not match channel protocol.
    MsgProtocolId,
    /// 0x16 — filter ID not recognised.
    InvalidFilterId,
    /// 0x17 — ISO 15765 transfer attempted without an active FlowControl filter.
    NoFlowControl,
    /// 0x18 — filter pattern matches an existing filter.
    NotUnique,
    /// 0x19 — requested baud rate unsupported.
    InvalidBaudrate,
    /// 0x1A — device handle not recognised.
    InvalidDeviceId,
    /// Any value outside the documented range.
    Unknown(u32),
}

impl J2534Error {
    pub fn from_u32(code: u32) -> Self {
        match code {
            0x00 => J2534Error::Success,
            0x01 => J2534Error::NotSupported,
            0x02 => J2534Error::InvalidChannelId,
            0x03 => J2534Error::InvalidProtocolId,
            0x04 => J2534Error::NullParameter,
            0x05 => J2534Error::InvalidIoctlValue,
            0x06 => J2534Error::InvalidFlags,
            0x07 => J2534Error::Failed,
            0x08 => J2534Error::DeviceNotConnected,
            0x09 => J2534Error::Timeout,
            0x0A => J2534Error::InvalidMsg,
            0x0B => J2534Error::InvalidTimeInterval,
            0x0C => J2534Error::ExceededLimit,
            0x0D => J2534Error::InvalidMsgId,
            0x0E => J2534Error::DeviceInUse,
            0x0F => J2534Error::InvalidIoctlId,
            0x10 => J2534Error::BufferEmpty,
            0x11 => J2534Error::BufferFull,
            0x12 => J2534Error::BufferOverflow,
            0x13 => J2534Error::PinInvalid,
            0x14 => J2534Error::ChannelInUse,
            0x15 => J2534Error::MsgProtocolId,
            0x16 => J2534Error::InvalidFilterId,
            0x17 => J2534Error::NoFlowControl,
            0x18 => J2534Error::NotUnique,
            0x19 => J2534Error::InvalidBaudrate,
            0x1A => J2534Error::InvalidDeviceId,
            other => J2534Error::Unknown(other),
        }
    }

    pub fn as_u32(self) -> u32 {
        match self {
            J2534Error::Success => 0x00,
            J2534Error::NotSupported => 0x01,
            J2534Error::InvalidChannelId => 0x02,
            J2534Error::InvalidProtocolId => 0x03,
            J2534Error::NullParameter => 0x04,
            J2534Error::InvalidIoctlValue => 0x05,
            J2534Error::InvalidFlags => 0x06,
            J2534Error::Failed => 0x07,
            J2534Error::DeviceNotConnected => 0x08,
            J2534Error::Timeout => 0x09,
            J2534Error::InvalidMsg => 0x0A,
            J2534Error::InvalidTimeInterval => 0x0B,
            J2534Error::ExceededLimit => 0x0C,
            J2534Error::InvalidMsgId => 0x0D,
            J2534Error::DeviceInUse => 0x0E,
            J2534Error::InvalidIoctlId => 0x0F,
            J2534Error::BufferEmpty => 0x10,
            J2534Error::BufferFull => 0x11,
            J2534Error::BufferOverflow => 0x12,
            J2534Error::PinInvalid => 0x13,
            J2534Error::ChannelInUse => 0x14,
            J2534Error::MsgProtocolId => 0x15,
            J2534Error::InvalidFilterId => 0x16,
            J2534Error::NoFlowControl => 0x17,
            J2534Error::NotUnique => 0x18,
            J2534Error::InvalidBaudrate => 0x19,
            J2534Error::InvalidDeviceId => 0x1A,
            J2534Error::Unknown(v) => v,
        }
    }

    /// `true` for the zero code. Callers must check this rather than `is_ok()`,
    /// because a driver returns its status *as the return value*.
    pub fn is_success(self) -> bool {
        matches!(self, J2534Error::Success)
    }
}

impl fmt::Display for J2534Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            J2534Error::Success => "success",
            J2534Error::NotSupported => "function option not supported",
            J2534Error::InvalidChannelId => "channel identifier not recognised",
            J2534Error::InvalidProtocolId => "protocol identifier not recognised",
            J2534Error::NullParameter => "null parameter",
            J2534Error::InvalidIoctlValue => "invalid ioctl value",
            J2534Error::InvalidFlags => "invalid flags",
            J2534Error::Failed => "unspecified failure (see PassThruGetLastError)",
            J2534Error::DeviceNotConnected => "device not connected",
            J2534Error::Timeout => "device operation timed out",
            J2534Error::InvalidMsg => "invalid message structure or payload",
            J2534Error::InvalidTimeInterval => "invalid periodic time interval",
            J2534Error::ExceededLimit => "device limit exceeded",
            J2534Error::InvalidMsgId => "invalid periodic message identifier",
            J2534Error::DeviceInUse => "device already in use",
            J2534Error::InvalidIoctlId => "invalid ioctl identifier",
            J2534Error::BufferEmpty => "receive buffer empty",
            J2534Error::BufferFull => "transmit buffer full",
            J2534Error::BufferOverflow => "receive buffer overflow",
            J2534Error::PinInvalid => "invalid J1962 pin number",
            J2534Error::ChannelInUse => "channel already in use",
            J2534Error::MsgProtocolId => "message protocol does not match channel",
            J2534Error::InvalidFilterId => "invalid filter identifier",
            J2534Error::NoFlowControl => "ISO 15765 flow control filter not configured",
            J2534Error::NotUnique => "filter pattern not unique",
            J2534Error::InvalidBaudrate => "invalid or unsupported baud rate",
            J2534Error::InvalidDeviceId => "invalid device identifier",
            J2534Error::Unknown(v) => return write!(f, "unknown J2534 error {v:#04x}"),
        };
        f.write_str(text)
    }
}

impl std::error::Error for J2534Error {}

/// Transmit flags (`TxFlags` in the C header).
///
/// Kept as a plain bitmask because a driver validates the combination; modelling
/// them as a Rust enum would lose the ability to express "both bits set".
pub mod tx_flags {
    /// Transmit as a 29-bit CAN identifier.
    pub const CAN_29BIT_ID: u32 = 0x0000_0100;
    /// ISO 15765-2: the first data byte is an extended address.
    pub const ISO15765_ADDR_TYPE: u32 = 0x0000_0080;
    /// ISO 15765-2: zero-pad FlowControl, Single and Last frames to full CAN size.
    pub const ISO15765_FRAME_PAD: u32 = 0x0000_0040;
    /// Wait only the P3min timer rather than the full inter-byte timeout.
    pub const WAIT_P3_MIN_ONLY: u32 = 0x0000_0200;
}

/// Receive-status flags (`RxStatus` in the C header).
pub mod rx_status {
    /// This entry is a transmit confirmation, not a received frame.
    pub const TX_MSG_TYPE: u32 = 0x0000_0001;
    /// ISO 15765-2: this is the first frame of a segmented message.
    pub const START_OF_MESSAGE: u32 = 0x0000_0002;
    /// The bus reported a break condition.
    pub const RX_BREAK: u32 = 0x0000_0004;
    /// Transmit completed.
    pub const TX_DONE: u32 = 0x0000_0008;
    /// ISO 15765-2 padding violation.
    pub const ISO15765_PADDING_ERROR: u32 = 0x0000_0010;
    /// The frame carried an extended address byte.
    pub const ISO15765_ADDR_TYPE: u32 = 0x0000_0080;
    /// The identifier was 29-bit.
    pub const CAN_29BIT_ID: u32 = 0x0000_0100;
}

/// Maximum payload a `PASSTHRU_MSG` can carry, from the C header's `Data[4128]`.
pub const PASSTHRU_MSG_MAX_DATA: usize = 4128;

/// A message on the J2534 wire, mirroring the C `PASSTHRU_MSG` struct.
///
/// The `Data` field is a `Vec` rather than a fixed array: the C struct always
/// carries 4128 bytes of storage, but sending 4 KB of padding on every frame
/// would be wasteful and would obscure the actual payload in logs. The length is
/// carried explicitly, exactly as `DataSize` does in C.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassThruMsg {
    pub protocol_id: ProtocolId,
    pub rx_status: u32,
    pub tx_flags: u32,
    /// Microseconds, device clock. Zero on transmit.
    pub timestamp: u32,
    pub data: Vec<u8>,
    /// Index in `data` where extra data (CRC/checksum) begins, or 0.
    pub extra_data_index: u32,
}

impl PassThruMsg {
    /// Build a transmit message with no flags.
    pub fn new(protocol_id: ProtocolId, data: Vec<u8>) -> Self {
        Self {
            protocol_id,
            rx_status: 0,
            tx_flags: 0,
            timestamp: 0,
            data,
            extra_data_index: 0,
        }
    }

    /// Build a transmit message with explicit flags.
    pub fn with_flags(protocol_id: ProtocolId, tx_flags: u32, data: Vec<u8>) -> Self {
        Self {
            tx_flags,
            ..Self::new(protocol_id, data)
        }
    }

    /// Whether this message is a transmit confirmation rather than a receive.
    pub fn is_tx_confirmation(&self) -> bool {
        self.rx_status & rx_status::TX_MSG_TYPE != 0
    }

    /// Whether this message begins an ISO 15765-2 segmented transfer.
    pub fn is_first_frame(&self) -> bool {
        self.rx_status & rx_status::START_OF_MESSAGE != 0
    }

    /// Payload length, as `DataSize` would report it.
    pub fn data_size(&self) -> usize {
        self.data.len()
    }
}

/// Connect flags (`ConnectFlag` in the C header).
pub mod connect_flags {
    /// The CAN identifier is 29-bit.
    pub const CAN_29BIT_ID: u32 = 0x0000_0100;
    /// ISO 9141: the ECU does not expect a checksum byte.
    pub const ISO9141_NO_CHECKSUM: u32 = 0x0000_0200;
    /// Listen for both 11-bit and 29-bit identifiers.
    pub const CAN_ID_BOTH: u32 = 0x0000_0800;
    /// ISO 9141: use the K line only, not L.
    pub const ISO9141_K_LINE_ONLY: u32 = 0x0000_1000;
}

/// Ioctl identifiers used by the device layer.
pub mod ioctl {
    pub const GET_CONFIG: u32 = 0x01;
    pub const SET_CONFIG: u32 = 0x02;
    pub const READ_VBATT: u32 = 0x03;
    pub const FIVE_BAUD_INIT: u32 = 0x04;
    pub const FAST_INIT: u32 = 0x05;
    pub const CLEAR_TX_BUFFER: u32 = 0x07;
    pub const CLEAR_RX_BUFFER: u32 = 0x08;
    pub const CLEAR_PERIODIC_MSGS: u32 = 0x09;
    pub const CLEAR_MSG_FILTERS: u32 = 0x0A;
    pub const CLEAR_FUNCT_MSG_LOOKUP_TABLE: u32 = 0x0B;
    pub const READ_PROG_VOLTAGE: u32 = 0x0E;
}

/// Filter types for `PassThruStartMsgFilter`.
pub mod filter_type {
    pub const PASS_FILTER: u32 = 0x01;
    pub const BLOCK_FILTER: u32 = 0x02;
    pub const FLOW_CONTROL_FILTER: u32 = 0x03;
}
