//! Arithmetic checksum engine (SUM, one's complement, two's complement).
//!
//! Ported from `ecu_engine_rust::math::checksum`
//! (`ChecksumEngine::calculate_arithmetic_checksum`, `ChecksumEngine::inject_checksum`).
//!
//! ## Deviation from the source
//!
//! `inject_checksum` mutates the caller's buffer. This crate is read-only by
//! convention, so it is exposed as [`inject_checksum_into`], which returns a new
//! buffer. Byte layout and validation are identical.
//!
//! ## Preserved source behaviour
//!
//! * Element stride follows `element_bits`; a trailing partial element is not
//!   summed (`curr + stride <= rom.len()`).
//! * Accumulation wraps at `u64` and is then masked to `checksum_bits`.
//! * Two's complement is `(!masked).wrapping_add(1) & mask`; one's complement is
//!   `!masked & mask`.
//! * CRC algorithms are rejected here — they live in the CRC primitives module.

use thiserror::Error;

/// Arithmetic checksum variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChecksumAlgorithm {
    /// Plain addition modulo the checksum width.
    Sum,
    /// Two's complement of the sum (negation).
    TwosComplement,
    /// One's complement of the sum (bitwise NOT).
    OnesComplement,
    /// CRC-16 CCITT — rejected by this engine.
    Crc16Ccitt,
    /// CRC-16 Modbus — rejected by this engine.
    Crc16Modbus,
    /// CRC-32 IEEE — rejected by this engine.
    Crc32Ieee,
    /// CRC-32C Castagnoli — rejected by this engine.
    Crc32Castagnoli,
}

/// Width of a checksum element or output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChecksumBits {
    /// 8 bits.
    Bits8,
    /// 16 bits.
    Bits16,
    /// 32 bits.
    Bits32,
}

impl ChecksumBits {
    /// Mask covering the width.
    pub fn mask(&self) -> u64 {
        match self {
            ChecksumBits::Bits8 => 0xFF,
            ChecksumBits::Bits16 => 0xFFFF,
            ChecksumBits::Bits32 => 0xFFFF_FFFF,
        }
    }

    /// Width in bytes.
    pub fn bytes(&self) -> usize {
        match self {
            ChecksumBits::Bits8 => 1,
            ChecksumBits::Bits16 => 2,
            ChecksumBits::Bits32 => 4,
        }
    }
}

/// Errors from the arithmetic checksum engine.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ArithmeticChecksumError {
    /// `start > end`, or `end` is past the end of the buffer.
    #[error("Start or end address out of bounds")]
    OutOfBounds,
    /// A CRC variant was passed to the arithmetic engine.
    #[error("Unsupported arithmetic checksum algorithm")]
    UnsupportedAlgorithm,
    /// Writing the checksum would run past the end of the buffer.
    #[error("Store address exceeds ROM buffer capacity")]
    StoreOverflow,
}

/// Computes a linear checksum over `[start_addr, end_addr]` (inclusive).
///
/// `element_bits` is the summation stride; `checksum_bits` masks the result.
/// `is_little_endian` selects byte order for 16- and 32-bit elements.
pub fn calculate_arithmetic_checksum(
    rom: &[u8],
    algo: ChecksumAlgorithm,
    start_addr: usize,
    end_addr: usize,
    element_bits: ChecksumBits,
    checksum_bits: ChecksumBits,
    is_little_endian: bool,
) -> Result<u64, ArithmeticChecksumError> {
    if start_addr > end_addr || end_addr >= rom.len() {
        return Err(ArithmeticChecksumError::OutOfBounds);
    }

    let stride = element_bits.bytes();
    let mut accumulator: u64 = 0;

    let mut curr = start_addr;
    while curr
        .checked_add(stride)
        .is_some_and(|next| next <= end_addr.saturating_add(1) && next <= rom.len())
    {
        let val: u64 = match element_bits {
            ChecksumBits::Bits8 => rom[curr] as u64,
            ChecksumBits::Bits16 => {
                let b0 = rom[curr] as u64;
                let b1 = rom[curr + 1] as u64;
                if is_little_endian {
                    b0 | (b1 << 8)
                } else {
                    (b0 << 8) | b1
                }
            }
            ChecksumBits::Bits32 => {
                let b0 = rom[curr] as u64;
                let b1 = rom[curr + 1] as u64;
                let b2 = rom[curr + 2] as u64;
                let b3 = rom[curr + 3] as u64;
                if is_little_endian {
                    b0 | (b1 << 8) | (b2 << 16) | (b3 << 24)
                } else {
                    (b0 << 24) | (b1 << 16) | (b2 << 8) | b3
                }
            }
        };
        accumulator = accumulator.wrapping_add(val);
        curr += stride;
    }

    let mask = checksum_bits.mask();
    let masked_acc = accumulator & mask;

    match algo {
        ChecksumAlgorithm::Sum => Ok(masked_acc),
        ChecksumAlgorithm::TwosComplement => Ok((!masked_acc).wrapping_add(1) & mask),
        ChecksumAlgorithm::OnesComplement => Ok((!masked_acc) & mask),
        _ => Err(ArithmeticChecksumError::UnsupportedAlgorithm),
    }
}

/// Writes `checksum` at `store_addr` and returns the resulting buffer.
///
/// Pure: the supplied buffer is never mutated.
pub fn inject_checksum_into(
    rom: &[u8],
    store_addr: usize,
    checksum: u64,
    bits: ChecksumBits,
    is_little_endian: bool,
) -> Result<Vec<u8>, ArithmeticChecksumError> {
    let len = bits.bytes();
    if store_addr.checked_add(len).is_none_or(|e| e > rom.len()) {
        return Err(ArithmeticChecksumError::StoreOverflow);
    }

    let mut out = rom.to_vec();
    match bits {
        ChecksumBits::Bits8 => {
            out[store_addr] = checksum as u8;
        }
        ChecksumBits::Bits16 => {
            let val16 = checksum as u16;
            let bytes = if is_little_endian {
                val16.to_le_bytes()
            } else {
                val16.to_be_bytes()
            };
            out[store_addr..store_addr + 2].copy_from_slice(&bytes);
        }
        ChecksumBits::Bits32 => {
            let val32 = checksum as u32;
            let bytes = if is_little_endian {
                val32.to_le_bytes()
            } else {
                val32.to_be_bytes()
            };
            out[store_addr..store_addr + 4].copy_from_slice(&bytes);
        }
    }
    Ok(out)
}

/// Sums little-endian 32-bit words over a set of inclusive byte intervals.
///
/// Used by Siemens Simos ECM3 calibration checksums. Accumulation wraps at `u64`;
/// a trailing partial word at an interval end is not counted.
pub fn calculate_simos_ecm3(cal_data: &[u8], intervals: &[(usize, usize)]) -> u64 {
    let mut sum: u64 = 0;
    for &(start, end) in intervals {
        let mut curr = start;
        while curr + 4 <= end + 1 && curr + 4 <= cal_data.len() {
            let word = u32::from_le_bytes([
                cal_data[curr],
                cal_data[curr + 1],
                cal_data[curr + 2],
                cal_data[curr + 3],
            ]) as u64;
            sum = sum.wrapping_add(word);
            curr += 4;
        }
    }
    sum
}
