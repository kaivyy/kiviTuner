//! NOR flash target descriptors.
//!
//! Ported from `ecu_engine_rust::flashing::bench_protocol`
//! (`MemoryBlockType`, `MemoryRangeDescriptor`, `SiliconChipInfo`,
//! `FlashSiliconClassifier`). Only the chip-identification table and the
//! boot-block safety predicate are reproduced; the source file's S-Box,
//! seed-key solvers, and bootloader patchers are deliberately **not** included.
//!
//! ## Known source data defect (preserved)
//!
//! The `AMD Am29F400BT` entry declares `total_size_bytes = 524_288` while its
//! block lengths sum to `327_680`, leaving a `0x30000` gap between `0x40000`
//! and `0x70000`. The table is reproduced verbatim so that parity holds; the
//! inconsistency is asserted by `flash_target_parity_test.rs` and must not be
//! silently corrected without an explicit decision.
//!
//! This module performs no I/O and is deterministic.

use thiserror::Error;

/// Functional type of a physical flash block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlashBlockType {
    /// Bootloader block. Never safe to erase or write.
    Boot,
    /// Parameter / configuration block.
    Parameter,
    /// Calibration data block.
    Calibration,
    /// Operating system / program block.
    OperatingSystem,
}

/// A physical flash block descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlashBlockDescriptor {
    /// Byte offset of the block within the chip.
    pub start_address: u32,
    /// Length of the block in bytes.
    pub length: u32,
    /// Functional type of the block.
    pub block_type: FlashBlockType,
}

/// Identity and partition layout of a NOR flash chip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlashTargetInfo {
    /// JEDEC manufacturer/device identifier.
    pub jedec_id: u32,
    /// Human-readable model name.
    pub model_name: &'static str,
    /// Declared total chip size in bytes.
    pub total_size_bytes: u32,
    /// Block partition table.
    pub blocks: Vec<FlashBlockDescriptor>,
}

/// Failure modes of [`describe_flash_target`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum FlashTargetError {
    /// The JEDEC id is not present in the known-chip table.
    #[error("Unknown silicon flash chip ID")]
    UnknownChipId,
}

const fn block(
    start_address: u32,
    length: u32,
    block_type: FlashBlockType,
) -> FlashBlockDescriptor {
    FlashBlockDescriptor {
        start_address,
        length,
        block_type,
    }
}

/// Resolves a JEDEC chip id to its partition layout.
///
/// Known chips (verbatim from the source table):
/// `0x00892274` / `0x00892275` (Intel 28F200BX),
/// `0x00894471` (Intel 28F400B),
/// `0x00012258` (AMD Am29F400BT Top-Boot),
/// `0x000122D7` (AMD Am29F800BB Bottom-Boot).
pub fn describe_flash_target(chip_id: u32) -> Result<FlashTargetInfo, FlashTargetError> {
    use FlashBlockType::{Boot, Calibration, OperatingSystem, Parameter};

    let blocks = match chip_id {
        // Intel 28F200BX (256 KiB)
        0x0089_2274 | 0x0089_2275 => vec![
            block(0x00000, 0x04000, Boot),
            block(0x04000, 0x02000, Parameter),
            block(0x06000, 0x02000, Parameter),
            block(0x08000, 0x18000, Calibration),
            block(0x20000, 0x20000, OperatingSystem),
        ],

        // Intel 28F400B (512 KiB)
        0x0089_4471 => vec![
            block(0x00000, 0x04000, Boot),
            block(0x04000, 0x02000, Parameter),
            block(0x06000, 0x02000, Parameter),
            block(0x08000, 0x18000, Calibration),
            block(0x20000, 0x20000, OperatingSystem),
            block(0x40000, 0x20000, OperatingSystem),
            block(0x60000, 0x20000, OperatingSystem),
        ],

        // AMD / Spansion Am29F400BT (512 KiB Top-Boot)
        // NOTE: block lengths sum to 0x50000, not 0x80000. Preserved verbatim.
        0x0001_2258 => vec![
            block(0x00000, 0x10000, OperatingSystem),
            block(0x10000, 0x10000, OperatingSystem),
            block(0x20000, 0x10000, OperatingSystem),
            block(0x30000, 0x10000, Calibration),
            block(0x70000, 0x08000, Parameter),
            block(0x78000, 0x04000, Parameter),
            block(0x7C000, 0x04000, Boot),
        ],

        // AMD / Spansion Am29F800BB (1 MiB Bottom-Boot)
        0x0001_22D7 => vec![
            block(0x00000, 0x04000, Boot),
            block(0x04000, 0x02000, Parameter),
            block(0x06000, 0x02000, Parameter),
            block(0x08000, 0x18000, Calibration),
            block(0x20000, 0x20000, OperatingSystem),
            block(0x40000, 0x40000, OperatingSystem),
            block(0x80000, 0x80000, OperatingSystem),
        ],

        _ => return Err(FlashTargetError::UnknownChipId),
    };

    let (model_name, total_size_bytes) = match chip_id {
        0x0089_2274 | 0x0089_2275 => ("Intel 28F200BX 256KiB", 256 * 1024),
        0x0089_4471 => ("Intel 28F400B 512KiB", 512 * 1024),
        0x0001_2258 => ("AMD Am29F400BT 512KiB Top-Boot", 512 * 1024),
        0x0001_22D7 => ("AMD Am29F800BB 1024KiB Bottom-Boot", 1024 * 1024),
        _ => return Err(FlashTargetError::UnknownChipId),
    };

    Ok(FlashTargetInfo {
        jedec_id: chip_id,
        model_name,
        total_size_bytes,
        blocks,
    })
}

/// Returns `true` unless the block is a bootloader block.
///
/// Boot blocks are never safe to erase or write.
pub fn is_block_write_safe(block: &FlashBlockDescriptor) -> bool {
    block.block_type != FlashBlockType::Boot
}
