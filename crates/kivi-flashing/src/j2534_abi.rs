//! C-ABI wire struct for `PASSTHRU_MSG`.
//!
//! ## Why a second message type exists
//!
//! [`crate::j2534_types::PassThruMsg`] carries `Vec<u8>`, which is right for a
//! Rust API and wrong across an FFI boundary: a vendor DLL reads a flat block
//! through a C pointer, and a `Vec` is a three-word fat pointer that means
//! nothing to it.
//!
//! This module provides the flat layout the driver actually expects.
//!
//! ## Layout specification
//!
//! Follows the published SAE J2534-1 v04.04 C ABI (`J2534.h`):
//!
//! ```c
//! typedef struct {
//!     unsigned long ProtocolID;      /* offset 0  */
//!     unsigned long RxStatus;        /* offset 4  */
//!     unsigned long TxFlags;         /* offset 8  */
//!     unsigned long Timestamp;       /* offset 12 */
//!     unsigned long DataSize;        /* offset 16 */
//!     unsigned long ExtraDataIndex;  /* offset 20 */
//!     unsigned char Data[4128];      /* offset 24 */
//! } PASSTHRU_MSG;                    /* sizeof == 4152 */
//! ```
//!
//! Matches standard 32-bit `unsigned long` ABI bindings (`PassThruMsg` with
//! six `u32` fields followed by `Data[4128]`).
//!
//! ## Why `u32` and not `usize`
//!
//! The C specification uses `unsigned long`, which is 32-bit on Windows (LLP64)
//! and on 32-bit x86 builds (`i686-pc-windows-msvc`). Using `usize` would make
//! the struct 8 bytes larger per field on a 64-bit host and shift every offset.

use crate::j2534_types::{PassThruMsg, ProtocolId};

/// Payload capacity of one `PASSTHRU_MSG`, from `Data[4128]`.
pub const PASSTHRU_MSG_DATA_CAPACITY: usize = 4128;

/// Offset of the `Data` array inside the struct: six 4-byte fields.
pub const PASSTHRU_MSG_DATA_OFFSET: usize = 24;

/// Total size of the C struct.
pub const PASSTHRU_MSG_ABI_SIZE: usize = PASSTHRU_MSG_DATA_OFFSET + PASSTHRU_MSG_DATA_CAPACITY;

/// The flat, fixed-layout message a J2534 driver reads and writes.
///
/// `#[repr(C)]` and all-`u32` header fields make the Rust layout identical to
/// the C one on every target this project builds for.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct PassthruMsgAbi {
    pub protocol_id: u32,
    pub rx_status: u32,
    pub tx_flags: u32,
    pub timestamp: u32,
    pub data_size: u32,
    pub extra_data_index: u32,
    pub data: [u8; PASSTHRU_MSG_DATA_CAPACITY],
}

impl PassthruMsgAbi {
    /// An all-zero message. `data_size == 0` means "no payload".
    pub fn zeroed() -> Self {
        Self {
            protocol_id: 0,
            rx_status: 0,
            tx_flags: 0,
            timestamp: 0,
            data_size: 0,
            extra_data_index: 0,
            data: [0u8; PASSTHRU_MSG_DATA_CAPACITY],
        }
    }

    /// Fallibly copy a Rust-friendly message into the flat layout, rejecting payloads > 4128 bytes.
    pub fn try_from_msg(msg: &PassThruMsg) -> Result<Self, crate::device::DeviceError> {
        if msg.data.len() > PASSTHRU_MSG_DATA_CAPACITY {
            return Err(crate::device::DeviceError::Transport(format!(
                "J2534 payload {} bytes exceeds {} byte PASSTHRU_MSG capacity",
                msg.data.len(),
                PASSTHRU_MSG_DATA_CAPACITY
            )));
        }
        Ok(Self::from_msg(msg))
    }

    /// Copy a Rust-friendly message into the flat layout.
    ///
    /// A payload longer than [`PASSTHRU_MSG_DATA_CAPACITY`] is truncated and
    /// `data_size` is clamped to match, so the driver never reads past the
    /// array. Prefer [`Self::try_from_msg`] on real hardware TX paths.
    pub fn from_msg(msg: &PassThruMsg) -> Self {
        let mut out = Self::zeroed();
        out.protocol_id = msg.protocol_id.as_u32();
        out.rx_status = msg.rx_status;
        out.tx_flags = msg.tx_flags;
        out.timestamp = msg.timestamp;
        out.extra_data_index = msg.extra_data_index;

        let n = msg.data.len().min(PASSTHRU_MSG_DATA_CAPACITY);
        out.data[..n].copy_from_slice(&msg.data[..n]);
        out.data_size = n as u32;
        out
    }

    /// Read the flat layout back into a Rust-friendly message.
    ///
    /// `data_size` is clamped to the array length: a driver that reports more
    /// than 4128 is broken, and trusting it would index out of bounds.
    pub fn to_msg(&self) -> PassThruMsg {
        let n = (self.data_size as usize).min(PASSTHRU_MSG_DATA_CAPACITY);
        PassThruMsg {
            protocol_id: ProtocolId::from_u32(self.protocol_id),
            rx_status: self.rx_status,
            tx_flags: self.tx_flags,
            timestamp: self.timestamp,
            data: self.data[..n].to_vec(),
            extra_data_index: self.extra_data_index,
        }
    }
}

impl Default for PassthruMsgAbi {
    fn default() -> Self {
        Self::zeroed()
    }
}

impl std::fmt::Debug for PassthruMsgAbi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Printing 4128 bytes would drown the log; show the live prefix only.
        let n = (self.data_size as usize).min(PASSTHRU_MSG_DATA_CAPACITY);
        f.debug_struct("PassthruMsgAbi")
            .field("protocol_id", &format_args!("{:#04x}", self.protocol_id))
            .field("rx_status", &format_args!("{:#06x}", self.rx_status))
            .field("tx_flags", &format_args!("{:#06x}", self.tx_flags))
            .field("timestamp", &self.timestamp)
            .field("data_size", &self.data_size)
            .field("extra_data_index", &self.extra_data_index)
            .field("data", &&self.data[..n])
            .finish()
    }
}
