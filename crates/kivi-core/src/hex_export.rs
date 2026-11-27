//! Intel HEX and Motorola S-Record export.
//!
//! Ported from `ecu_engine_rust::formats::intel_hex::IntelHexParser::serialize_to_i32hex`
//! and `ecu_engine_rust::formats::srec::SrecParser::serialize_to_s37`.
//!
//! These complete the parse half already present in [`crate::hex_ingest`]: data
//! ingested from HEX/SREC can now be written back out.
//!
//! ## Deviation from the source (data-integrity fix)
//!
//! The source advances each record's address by the requested `bytes_per_line`
//! while chunking by `bytes_per_line.min(32)`. When `bytes_per_line > 32` the
//! two disagree: the emitted addresses skip forward by the requested stride
//! while only 32 bytes are written, so the output describes data at addresses it
//! does not contain and no longer round-trips. This module advances by the
//! actual chunk offset. Emitting corrupt addresses is a defect, not a contract.
//!
//! ## Address semantics
//!
//! Both functions take a concrete `u32` base. Callers holding `Option<u32>` must
//! decide explicitly; passing `None` is not representable and must not be
//! silently converted to zero. [`HexExportError::UnknownBaseAddress`] exists so
//! that decision is visible at the call site.

use thiserror::Error;

/// Errors from the export functions.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum HexExportError {
    /// `bytes_per_line` was zero, which would make no progress.
    #[error("bytes_per_line must be greater than zero")]
    ZeroBytesPerLine,
    /// A caller tried to export an image whose base address is unknown.
    #[error("Cannot export without a known base address")]
    UnknownBaseAddress,
    /// The image extends past the 32-bit address space.
    #[error("Image extends beyond the 32-bit address space")]
    AddressOverflow,
    /// Target ROM size was zero.
    #[error("Target ROM size must be greater than 0")]
    ZeroTargetSize,
    /// A record address is below the declared physical base.
    #[error("HEX record address is below specified physical_base_address")]
    RecordBelowBase,
    /// A record would land past the target buffer.
    #[error("HEX record offset exceeds target_rom_size boundary")]
    RecordExceedsTarget,
}

/// Serializes contiguous data as Intel HEX with 32-bit extended addressing.
///
/// Emits a `0x04` extended-linear-address record whenever the upper 16 address
/// bits change, `0x00` data records of at most 32 bytes, and a terminating
/// `0x01` EOF record.
pub fn serialize_to_i32hex(
    start_address: u32,
    data: &[u8],
    bytes_per_line: usize,
) -> Result<String, HexExportError> {
    if bytes_per_line == 0 {
        return Err(HexExportError::ZeroBytesPerLine);
    }
    let per_line = bytes_per_line.min(32);

    // Reject an image that would run past the 32-bit space rather than wrap.
    let end = (data.len() as u64) + start_address as u64;
    if end > u32::MAX as u64 + 1 {
        return Err(HexExportError::AddressOverflow);
    }

    let mut out = String::new();
    let mut current_upper: Option<u16> = None;
    let mut offset = 0usize;

    while offset < data.len() {
        let addr = start_address + offset as u32;
        let upper = ((addr & 0xFFFF_0000) >> 16) as u16;
        let lower = (addr & 0x0000_FFFF) as u16;
        let remaining_in_bank = 0x1_0000 - lower as usize;
        let take = per_line.min(data.len() - offset).min(remaining_in_bank);
        let chunk = &data[offset..offset + take];

        if current_upper != Some(upper) {
            current_upper = Some(upper);
            let high_b = (upper >> 8) as u8;
            let low_b = (upper & 0xFF) as u8;
            let sum = 0x02u8
                .wrapping_add(0)
                .wrapping_add(0)
                .wrapping_add(0x04)
                .wrapping_add(high_b)
                .wrapping_add(low_b);
            let chk = (256u16.wrapping_sub(sum as u16)) as u8;
            out.push_str(&format!(":02000004{:02X}{:02X}{:02X}\n", high_b, low_b, chk));
        }

        let len = chunk.len() as u8;
        let addr_h = (lower >> 8) as u8;
        let addr_l = (lower & 0xFF) as u8;
        let mut sum = len
            .wrapping_add(addr_h)
            .wrapping_add(addr_l)
            .wrapping_add(0x00);

        let mut line_hex = format!(":{:02X}{:04X}00", len, lower);
        for &b in chunk {
            sum = sum.wrapping_add(b);
            line_hex.push_str(&format!("{:02X}", b));
        }
        let chk = (256u16.wrapping_sub(sum as u16)) as u8;
        line_hex.push_str(&format!("{:02X}\n", chk));
        out.push_str(&line_hex);
        offset += take;
    }

    out.push_str(":00000001FF\n");
    Ok(out)
}

/// Serializes contiguous data as Motorola S-Record (S3 data, S7 termination).
///
/// Emits an `S0` header, `S3` data records of at most 32 bytes with 32-bit
/// addresses, and an `S7` terminator.
pub fn serialize_to_s37(
    start_address: u32,
    data: &[u8],
    bytes_per_line: usize,
) -> Result<String, HexExportError> {
    if bytes_per_line == 0 {
        return Err(HexExportError::ZeroBytesPerLine);
    }
    let per_line = bytes_per_line.min(32);

    let end = (data.len() as u64) + start_address as u64;
    if end > u32::MAX as u64 + 1 {
        return Err(HexExportError::AddressOverflow);
    }

    let mut out = String::new();
    out.push_str("S00600004844521B\n"); // "HDR"

    for (chunk_idx, chunk) in data.chunks(per_line).enumerate() {
        let addr = start_address + (chunk_idx * per_line) as u32;
        let count = (chunk.len() + 4 + 1) as u8;

        let a3 = (addr >> 24) as u8;
        let a2 = (addr >> 16) as u8;
        let a1 = (addr >> 8) as u8;
        let a0 = (addr & 0xFF) as u8;

        let mut sum = count
            .wrapping_add(a3)
            .wrapping_add(a2)
            .wrapping_add(a1)
            .wrapping_add(a0);
        let mut line = format!("S3{:02X}{:08X}", count, addr);

        for &b in chunk {
            sum = sum.wrapping_add(b);
            line.push_str(&format!("{:02X}", b));
        }

        let chk = !sum;
        line.push_str(&format!("{:02X}\n", chk));
        out.push_str(&line);
    }

    out.push_str("S70500000000FA\n");
    Ok(out)
}

/// Rebases a sparse HEX/SREC image into a full-size flat buffer.
///
/// Each record is placed at `record_address - physical_base_address`. A record
/// below the base, or one that would land past `target_rom_size`, is an error
/// rather than a silent truncation.
///
/// `records` is `(address, byte)` in any order; the output is blank-filled with
/// `pad_byte` before placement.
pub fn to_rebased_full_bin(
    records: &[(u32, u8)],
    physical_base_address: u32,
    target_rom_size: usize,
    pad_byte: u8,
) -> Result<Vec<u8>, HexExportError> {
    if target_rom_size == 0 {
        return Err(HexExportError::ZeroTargetSize);
    }
    let mut bin = vec![pad_byte; target_rom_size];
    for &(addr, b) in records {
        let offset = addr
            .checked_sub(physical_base_address)
            .ok_or(HexExportError::RecordBelowBase)? as usize;
        if offset >= target_rom_size {
            return Err(HexExportError::RecordExceedsTarget);
        }
        bin[offset] = b;
    }
    Ok(bin)
}
