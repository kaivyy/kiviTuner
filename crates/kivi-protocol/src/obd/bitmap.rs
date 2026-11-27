use crate::obd::errors::ObdCodecError;
use serde::{Deserialize, Serialize};

/// Bit coordinates of a PID within a 4-byte (32-bit) supported-PIDs bitmap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PidBitmapCoordinates {
    /// Byte index within the 4-byte array (0 = Byte A, 1 = Byte B, 2 = Byte C, 3 = Byte D).
    pub byte_index: usize,
    /// Bit index within the byte (7 = MSB, 0 = LSB).
    pub bit_index: u8,
    /// Single-bit mask corresponding to `1 << bit_index`.
    pub bit_mask: u8,
    /// Big-endian global bit index (31 = MSB of Byte A, 0 = LSB of Byte D).
    pub global_bit_index: u8,
}

/// Computes the exact bitmap coordinates for a given PID and base PID range.
///
/// Under SAE J1979:
/// - Base PID 0x00 covers PIDs 0x01 through 0x20.
/// - Base PID 0x20 covers PIDs 0x21 through 0x40.
/// - Base PID 0x40 covers PIDs 0x41 through 0x60.
///
/// Formula:
/// - First PID: $P_{\text{first}} = P_{\text{base}} + 1$
/// - Last PID: $P_{\text{last}} = P_{\text{base}} + 0x20$
/// - Offset: $O = P - P_{\text{first}}$
/// - Byte index: $O / 8$
/// - Bit index: $7 - (O \bmod 8)$
/// - Bit mask: $1 \ll \text{bit\_index}$
/// - Global bit index: $31 - O$
pub fn calculate_pid_bitmap_coordinates(
    pid: u8,
    base_pid: u8,
) -> Result<PidBitmapCoordinates, ObdCodecError> {
    let first_pid = base_pid
        .checked_add(1)
        .ok_or(ObdCodecError::PidOutOfRange { pid, base: base_pid })?;
    let last_pid = base_pid
        .checked_add(0x20)
        .ok_or(ObdCodecError::PidOutOfRange { pid, base: base_pid })?;

    if pid < first_pid || pid > last_pid {
        return Err(ObdCodecError::PidOutOfRange { pid, base: base_pid });
    }

    let offset = pid - first_pid; // 0..=31
    let byte_index = (offset / 8) as usize;
    let bit_index = 7 - (offset % 8);
    let bit_mask = 1 << bit_index;
    let global_bit_index = 31 - offset;

    Ok(PidBitmapCoordinates {
        byte_index,
        bit_index,
        bit_mask,
        global_bit_index,
    })
}

/// Checks whether a specific PID is supported according to a raw 4-byte bitmap.
pub fn is_pid_supported_in_bitmap(
    pid: u8,
    base_pid: u8,
    bitmap: &[u8; 4],
) -> Result<bool, ObdCodecError> {
    let coords = calculate_pid_bitmap_coordinates(pid, base_pid)?;
    Ok((bitmap[coords.byte_index] & coords.bit_mask) != 0)
}

/// A parsed 4-byte Supported-PIDs bitmap corresponding to a specific base range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SupportedPidsBitmap {
    /// Base PID identifier (e.g., 0x00, 0x20, 0x40).
    pub base_pid: u8,
    /// Raw 4-byte bitmap payload (Bytes A, B, C, D).
    pub bytes: [u8; 4],
}

impl SupportedPidsBitmap {
    /// Constructs a bitmap from a base PID and a 4-byte array.
    pub const fn from_bytes(base_pid: u8, bytes: [u8; 4]) -> Self {
        Self { base_pid, bytes }
    }

    /// Decodes and strictly validates a Mode 01 diagnostic payload for supported PIDs.
    ///
    /// Expected format: `[0x41, base_pid, byte_a, byte_b, byte_c, byte_d]` (exactly 6 bytes).
    pub fn decode_from_payload(payload: &[u8]) -> Result<Self, ObdCodecError> {
        if payload.len() < 6 {
            return Err(ObdCodecError::TruncatedPayload {
                expected: 6,
                actual: payload.len(),
            });
        }
        if payload.len() > 6 {
            return Err(ObdCodecError::OversizedPayload {
                expected: 6,
                actual: payload.len(),
            });
        }
        if payload[0] != 0x41 {
            return Err(ObdCodecError::UnexpectedService {
                expected: 0x41,
                actual: payload[0],
            });
        }

        let base_pid = payload[1];
        let bytes = [payload[2], payload[3], payload[4], payload[5]];
        Ok(Self { base_pid, bytes })
    }

    /// Checks if a PID is supported within this bitmap's range.
    ///
    /// Returns `false` if the PID is outside this range or if the corresponding bit is 0.
    pub fn is_supported(&self, pid: u8) -> bool {
        match calculate_pid_bitmap_coordinates(pid, self.base_pid) {
            Ok(coords) => (self.bytes[coords.byte_index] & coords.bit_mask) != 0,
            Err(_) => false,
        }
    }

    /// Indicates whether the continuation bit is set (Byte D, bit 0).
    ///
    /// When true, the ECU supports querying the subsequent 32-PID range (e.g. PID 0x20 -> 0x21..0x40).
    pub fn has_next_bitmap_range(&self) -> bool {
        (self.bytes[3] & 0x01) != 0
    }
}
