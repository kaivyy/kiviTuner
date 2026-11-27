use crate::primitives::sum_modulo_256;
use thiserror::Error;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum KeihinChecksumError {
    #[error("Buffer too small for Keihin ROM")]
    BufferTooSmall,
}

/// Keihin 8-bit Two's Complement:
/// The entire ROM (all bytes modulo 256) must sum to 0x00.
/// The checksum byte is located at the very last byte of the ROM.
pub fn verify_keihin_checksum(rom: &[u8]) -> bool {
    if rom.is_empty() {
        return false;
    }
    sum_modulo_256(rom) == 0x00
}

/// Computes the expected checksum byte for a Keihin ROM image of at least 1024 bytes.
pub fn compute_keihin_checksum_byte(rom: &[u8]) -> Result<u8, KeihinChecksumError> {
    if rom.len() < 1024 {
        return Err(KeihinChecksumError::BufferTooSmall);
    }
    let last_idx = rom.len() - 1;
    let partial_sum = sum_modulo_256(&rom[..last_idx]);
    Ok((0x100u32.wrapping_sub(partial_sum as u32) & 0xFF) as u8)
}

pub fn fix_keihin_checksum(rom: &mut [u8]) -> Result<(), KeihinChecksumError> {
    let target = compute_keihin_checksum_byte(rom)?;
    let last_idx = rom.len() - 1;
    rom[last_idx] = target;
    Ok(())
}
