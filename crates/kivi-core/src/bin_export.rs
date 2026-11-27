//! Binary image export and dump-integrity utilities.
//!
//! Ported from `ecu_engine_rust::formats::mod.rs::BinExportEngine`:
//! `swap_16bit_words_in_place`, `split_odd_even_eprom`, `merge_odd_even_eprom`,
//! `merge_partial_read_into_full_bin`, `validate_dumped_bin_integrity`.
//!
//! These operate on calibration images offline. Nothing here touches hardware.
//!
//! ## Preserved source behaviour
//!
//! * Odd-length buffers are rejected for word swap and EPROM split.
//! * EPROM banks must be the same size to merge.
//! * A partial read is placed at `partial_read_address - physical_base_address`.
//! * Integrity thresholds: at least 1024 bytes, no more than 98% of a single
//!   blank value, and no 256-byte mirror loop covering 90% of pages.
//!
//! ## Deviation
//!
//! The source exposes only a mutating `swap_16bit_words_in_place`. A pure
//! [`swap_16bit_words`] is added for analysis use; the mutating form is kept so
//! in-place callers retain source semantics.

use thiserror::Error;

/// Errors from the binary export utilities.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BinExportError {
    /// Word swap needs an even byte count.
    #[error("Binary length must be a multiple of 2 bytes for 16-bit word swap")]
    OddLengthForWordSwap,
    /// EPROM split needs an even byte count.
    #[error("Binary length must be even to split into Odd/Even EPROM banks")]
    OddLengthForEpromSplit,
    /// The two EPROM banks differ in size.
    #[error("Even (.LO) and Odd (.HI) EPROM buffers must have identical size")]
    BankSizeMismatch,
    /// Target ROM size was zero.
    #[error("Target ROM size must be non-zero")]
    ZeroTargetSize,
    /// The supplied base ROM does not match the requested target size.
    #[error("Base ROM size does not match target_rom_size")]
    BaseSizeMismatch,
    /// The partial read starts below the physical base address.
    #[error("partial_read_address is below physical_base_address")]
    PartialBelowBase,
    /// The partial read would run past the target buffer.
    #[error("Partial read slice exceeds target_rom_size boundary")]
    PartialExceedsTarget,
    /// Dump is too small to be a plausible ECU image.
    #[error("DUMP_TOO_SMALL: Valid ECU .bin dump must be at least 1024 bytes")]
    DumpTooSmall,
    /// More than 98% of the dump is 0xFF.
    #[error("BLANK_FLASH_DUMP_0xFF: >98% of dumped .bin is 0xFF (chip unread or boot pin error)")]
    BlankFlashDump,
    /// More than 98% of the dump is 0x00.
    #[error("LOCKED_ZERO_DUMP_0x00: >98% of dumped .bin is 0x00 (MCU security read-protection active)")]
    LockedZeroDump,
    /// The dump repeats one 256-byte page, the classic locked-readout trap.
    #[error("FAKE_MIRROR_READOUT_TRAP: ECU returned repeating 256-byte dummy pages (SecurityAccess locked)")]
    FakeMirrorReadout,
}

/// Swaps the two bytes of every 16-bit word, returning a new buffer.
///
/// Pure: the input is never mutated.
pub fn swap_16bit_words(bin: &[u8]) -> Result<Vec<u8>, BinExportError> {
    if !bin.len().is_multiple_of(2) {
        return Err(BinExportError::OddLengthForWordSwap);
    }
    let mut out = bin.to_vec();
    for chunk in out.as_chunks_mut::<2>().0 {
        chunk.swap(0, 1);
    }
    Ok(out)
}

/// Swaps the two bytes of every 16-bit word in place.
pub fn swap_16bit_words_in_place(bin: &mut [u8]) -> Result<(), BinExportError> {
    if !bin.len().is_multiple_of(2) {
        return Err(BinExportError::OddLengthForWordSwap);
    }
    for chunk in bin.as_chunks_mut::<2>().0 {
        chunk.swap(0, 1);
    }
    Ok(())
}

/// Splits a 16-bit image into its even (`.LO`) and odd (`.HI`) 8-bit banks.
pub fn split_odd_even_eprom(bin: &[u8]) -> Result<(Vec<u8>, Vec<u8>), BinExportError> {
    if !bin.len().is_multiple_of(2) {
        return Err(BinExportError::OddLengthForEpromSplit);
    }
    let mut even_lo = Vec::with_capacity(bin.len() / 2);
    let mut odd_hi = Vec::with_capacity(bin.len() / 2);
    for chunk in bin.as_chunks::<2>().0 {
        even_lo.push(chunk[0]);
        odd_hi.push(chunk[1]);
    }
    Ok((even_lo, odd_hi))
}

/// Interleaves the even (`.LO`) and odd (`.HI`) banks into a 16-bit image.
pub fn merge_odd_even_eprom(even_lo: &[u8], odd_hi: &[u8]) -> Result<Vec<u8>, BinExportError> {
    if even_lo.len() != odd_hi.len() {
        return Err(BinExportError::BankSizeMismatch);
    }
    let mut merged = Vec::with_capacity(even_lo.len() * 2);
    for (&lo, &hi) in even_lo.iter().zip(odd_hi.iter()) {
        merged.push(lo);
        merged.push(hi);
    }
    Ok(merged)
}

/// Places a partial calibration read at its exact offset in a full-size image.
///
/// When no base ROM is supplied the target is blank-filled with `0xFF`.
/// The destination offset is `partial_read_address - physical_base_address`.
pub fn merge_partial_read_into_full_bin(
    base_or_blank_rom: Option<&[u8]>,
    target_rom_size: usize,
    physical_base_address: u32,
    partial_read_address: u32,
    partial_data: &[u8],
) -> Result<Vec<u8>, BinExportError> {
    if target_rom_size == 0 {
        return Err(BinExportError::ZeroTargetSize);
    }
    let mut full_bin = match base_or_blank_rom {
        Some(base) => {
            if base.len() != target_rom_size {
                return Err(BinExportError::BaseSizeMismatch);
            }
            base.to_vec()
        }
        None => vec![0xFFu8; target_rom_size],
    };

    let offset = partial_read_address
        .checked_sub(physical_base_address)
        .ok_or(BinExportError::PartialBelowBase)? as usize;
    let end = offset.saturating_add(partial_data.len());
    if end > target_rom_size {
        return Err(BinExportError::PartialExceedsTarget);
    }

    full_bin[offset..end].copy_from_slice(partial_data);
    Ok(full_bin)
}

/// Rejects dumps that are almost certainly not a real ECU readout.
///
/// Catches three failure modes seen in practice: a flash chip that was never
/// read (>98% `0xFF`), a security-locked MCU returning zeros (>98% `0x00`), and
/// a locked readout that replays one 256-byte page across the whole image.
pub fn validate_dumped_bin_integrity(rom_bytes: &[u8]) -> Result<(), BinExportError> {
    if rom_bytes.len() < 1024 {
        return Err(BinExportError::DumpTooSmall);
    }

    let ff_count = rom_bytes.iter().filter(|&&b| b == 0xFF).count();
    let zero_count = rom_bytes.iter().filter(|&&b| b == 0x00).count();
    let threshold = (rom_bytes.len() * 98) / 100;

    if ff_count >= threshold {
        return Err(BinExportError::BlankFlashDump);
    }
    if zero_count >= threshold {
        return Err(BinExportError::LockedZeroDump);
    }

    let total_pages = rom_bytes.len() / 256;
    if total_pages >= 8 {
        let first_page = &rom_bytes[0..256];
        let is_non_trivial = first_page.iter().any(|&b| b != 0xFF && b != 0x00);
        if is_non_trivial {
            let identical_pages = rom_bytes
                .as_chunks::<256>()
                .0
                .iter()
                .filter(|&&page| page == first_page)
                .count();
            if identical_pages * 10 >= total_pages * 9 {
                return Err(BinExportError::FakeMirrorReadout);
            }
        }
    }

    Ok(())
}
