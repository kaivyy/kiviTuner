//! Bosch ME7 95040 SPI EEPROM page checksums.
//!
//! Ported from `ecu_engine_rust::math::checksum` (`BoschMe7EepromChecksum`).
//!
//! ## What this validates
//!
//! A 512-byte 95040 EEPROM dump split into 32 pages of 16 bytes. Each page
//! carries a 16-bit two's-complement checksum in its last two bytes, little
//! endian. Pages 28 and 29 hold the VAG part number in ASCII and are exempt.
//!
//! ## Preserved source behaviour
//!
//! * The checksum sums bytes `0..=13` of the page, adds the page index, applies
//!   the descriptor's `0x40` adjustment, then negates (two's complement).
//! * `fix_all_page_checksums` silently skips a page whose computation errors,
//!   and counts only pages whose stored value actually changed.
//!
//! This module performs no I/O. `fix_all_page_checksums` mutates the supplied
//! EEPROM buffer by design — it is the one intentional mutator in this crate,
//! matching the source.

use thiserror::Error;

/// Errors from the ME7 EEPROM checksum routines.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EepromChecksumError {
    /// Page index is 32 or greater.
    #[error("Page index out of bounds")]
    PageOutOfBounds,
    /// Buffer is shorter than one 16-byte page.
    #[error("Data buffer smaller than 16-byte page")]
    BufferTooSmall,
    /// The page's descriptor does not mark it as carrying a checksum.
    #[error("Page has no checksum descriptor")]
    NoDescriptor,
    /// Buffer is shorter than the full 512-byte EEPROM.
    #[error("EEPROM buffer must be at least 512 bytes")]
    EepromTooSmall,
}

/// Full EEPROM size in bytes.
pub const EEPROM_SIZE: usize = 512;
/// Page size in bytes.
pub const PAGE_SIZE: usize = 16;
/// Number of pages.
pub const PAGE_COUNT: usize = 32;

/// Per-page descriptors. Bit 0 marks "has checksum"; bit 6 (0x40) applies the
/// `-1` adjustment before negation.
pub const PAGE_DESCRIPTORS: [u16; 32] = [
    0xFF18, 0x0017, 0x0117, 0x0207, 0x0307, 0x0437, 0x0533, 0x06B7, 0x06F7, 0x07B3, 0x07F3, 0x08B7,
    0x08F7, 0x09B3, 0x09F3, 0x0AB3, 0x0AF3, 0x0B32, 0x0B10, 0x0B10, 0x0B10, 0x0C37, 0x0D33, 0x0E33,
    0x0F33, 0x1033, 0x1133, 0x1233, 0x1235, 0x1235, 0x13B7, 0x13F7,
];

/// Pages holding the VAG part number; they carry no page checksum.
pub fn is_page_exempt(page: usize) -> bool {
    page == 28 || page == 29
}

/// Computes the 16-bit checksum for one 16-byte page.
pub fn calculate_page_checksum(data: &[u8], page: usize) -> Result<u16, EepromChecksumError> {
    if page >= PAGE_COUNT {
        return Err(EepromChecksumError::PageOutOfBounds);
    }
    let offset = page * PAGE_SIZE;
    if offset + PAGE_SIZE > data.len() {
        return Err(EepromChecksumError::BufferTooSmall);
    }

    let desc = PAGE_DESCRIPTORS[page];
    if (desc & 0x0001) == 0 {
        return Err(EepromChecksumError::NoDescriptor);
    }

    let mut sum: u16 = 0;
    for i in 0..=13 {
        sum = sum.wrapping_add(data[offset + i] as u16);
    }
    sum = sum.wrapping_add(page as u16);

    if (desc & 0x0040) != 0 {
        sum = sum.wrapping_sub(1);
    }

    Ok((!sum).wrapping_add(1))
}

/// Recomputes and writes every enforced page checksum in a 512-byte dump.
///
/// Returns how many pages actually changed. Pages 28 and 29 are skipped.
/// A page whose computation errors is silently skipped, matching the source.
pub fn fix_all_page_checksums(eeprom: &mut [u8]) -> Result<usize, EepromChecksumError> {
    if eeprom.len() < EEPROM_SIZE {
        return Err(EepromChecksumError::EepromTooSmall);
    }

    let mut fixed_count = 0;
    for (page, &desc) in PAGE_DESCRIPTORS.iter().enumerate() {
        if is_page_exempt(page) {
            continue;
        }
        if (desc & 0x0001) != 0 {
            if let Ok(expected) = calculate_page_checksum(eeprom, page) {
                let offset = page * PAGE_SIZE;
                let stored = u16::from_le_bytes([eeprom[offset + 14], eeprom[offset + 15]]);
                if stored != expected {
                    let bytes = expected.to_le_bytes();
                    eeprom[offset + 14] = bytes[0];
                    eeprom[offset + 15] = bytes[1];
                    fixed_count += 1;
                }
            }
        }
    }
    Ok(fixed_count)
}
