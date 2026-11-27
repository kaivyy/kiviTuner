//! Rolling checksum with a ROM-resident lookup table.
//!
//! ## Where this comes from
//!
//! `NefMotoOpenSource/Checksum/BaseChecksum.cs` — `CalculateRollingChecksumForRange`.
//! The loop is:
//!
//! ```text
//! for each byte b in [start, start+len):
//!     seed = u32_le_at(table_addr + ((b ^ (checksum & 0xFF)) << 2))
//!     checksum = (checksum >> 8) ^ seed
//! ```
//!
//! ## Why this shape exists
//!
//! That is a CRC-32 in table-driven form — but the 256-entry table lives **inside
//! the ROM**, not in the tool. Bosch ME7 stores it there so the polynomial can
//! differ per calibration. A tool that assumes a fixed CRC-32 polynomial produces
//! a wrong checksum on those ECUs, and a wrong checksum means the ECU rejects the
//! image.
//!
//! ## What kiviTuner had before
//!
//! Fixed-polynomial CRC-32 (four variants), arithmetic sums, ME7 page checksums.
//! None of them read the table from the ROM, so this whole class of ME7 image was
//! un-checksummable.

/// Why a rolling checksum could not be computed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RollingError {
    /// The data range runs past the end of the ROM.
    RangeOutOfBounds { start: usize, length: usize, rom_len: usize },
    /// The 256-entry table runs past the end of the ROM.
    TableOutOfBounds { table_addr: usize, rom_len: usize },
}

impl std::fmt::Display for RollingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RollingError::RangeOutOfBounds {
                start,
                length,
                rom_len,
            } => write!(
                f,
                "checksum range [{start:#x}, +{length:#x}) runs past the {rom_len:#x}-byte ROM"
            ),
            RollingError::TableOutOfBounds { table_addr, rom_len } => write!(
                f,
                "lookup table at {table_addr:#x} needs 1024 bytes but the ROM is only {rom_len:#x}"
            ),
        }
    }
}

impl std::error::Error for RollingError {}

/// Read one little-endian `u32` from `rom` at `off`.
fn u32_le(rom: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([rom[off], rom[off + 1], rom[off + 2], rom[off + 3]])
}

/// Compute the rolling checksum over `rom[start .. start+length]`.
///
/// `table_addr` points at the 256-entry `u32` table stored in the ROM.
/// `initial` seeds the running value; pass `0` for a fresh computation.
///
/// Returns the running value after the last byte.
pub fn rolling_checksum(
    rom: &[u8],
    start: usize,
    length: usize,
    table_addr: usize,
    initial: u32,
) -> Result<u32, RollingError> {
    // The table occupies 256 * 4 bytes.
    const TABLE_BYTES: usize = 256 * 4;
    if table_addr.saturating_add(TABLE_BYTES) > rom.len() {
        return Err(RollingError::TableOutOfBounds {
            table_addr,
            rom_len: rom.len(),
        });
    }
    if start.saturating_add(length) > rom.len() {
        return Err(RollingError::RangeOutOfBounds {
            start,
            length,
            rom_len: rom.len(),
        });
    }

    let mut checksum = initial;
    for i in 0..length {
        let b = rom[start + i];
        // Index is the byte XOR the low byte of the running value.
        let idx = (b ^ (checksum as u8)) as usize;
        let seed = u32_le(rom, table_addr + idx * 4);
        checksum = (checksum >> 8) ^ seed;
    }
    Ok(checksum)
}

/// Verify that `rom[start .. start+length]` already carries the expected value
/// at `stored_addr`.
///
/// A convenience over [`rolling_checksum`] for the common "is this image's
/// checksum valid" question.
pub fn verify_rolling_checksum(
    rom: &[u8],
    start: usize,
    length: usize,
    table_addr: usize,
    stored_addr: usize,
) -> Result<bool, RollingError> {
    if stored_addr.saturating_add(4) > rom.len() {
        return Err(RollingError::RangeOutOfBounds {
            start: stored_addr,
            length: 4,
            rom_len: rom.len(),
        });
    }
    let computed = rolling_checksum(rom, start, length, table_addr, 0)?;
    Ok(computed == u32_le(rom, stored_addr))
}

/// Sum every byte across several address ranges.
///
/// NefMoto's `MultiRangeChecksum` shape: one stored value covers a set of
/// non-contiguous regions, and the value is the plain byte sum of all of them.
/// ME7 uses this where the protected data is split by tables that are not
/// checksummed.
///
/// Returns the 32-bit wrapping sum.
pub fn multi_range_byte_sum(
    rom: &[u8],
    ranges: &[(usize, usize)],
) -> Result<u32, RollingError> {
    let mut sum: u32 = 0;
    for &(start, length) in ranges {
        if start.saturating_add(length) > rom.len() {
            return Err(RollingError::RangeOutOfBounds {
                start,
                length,
                rom_len: rom.len(),
            });
        }
        for &b in &rom[start..start + length] {
            sum = sum.wrapping_add(b as u32);
        }
    }
    Ok(sum)
}
