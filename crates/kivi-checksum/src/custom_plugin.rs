//! Declarative JSON custom checksum plugin engine.
//!
//! Enables tuning engineers to add, verify, and repair arbitrary range-based ECU
//! checksums and balance words via pure JSON definitions without recompiling Rust.

use crate::primitives::{
    compute_crc32, crc16_arc, crc16_ccitt, crc32_castagnoli, crc32_simos, crc8_maxim,
};
use kivi_core::types::Endian;
use serde::{Deserialize, Serialize};

pub const MAX_CUSTOM_PLUGIN_BLOCKS: usize = 64;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CustomEndianness {
    Big,
    Little,
}

impl From<CustomEndianness> for Endian {
    fn from(value: CustomEndianness) -> Self {
        match value {
            CustomEndianness::Big => Endian::Big,
            CustomEndianness::Little => Endian::Little,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CustomChecksumAlgorithm {
    Sum,
    TwosComplement,
    OnesComplement,
    Xor,
    Crc8Maxim,
    Crc16Ccitt,
    Crc16Arc,
    Crc32Ieee,
    Crc32Castagnoli,
    Crc32Simos,
}

fn default_exclude_store() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomChecksumBlock {
    pub name: String,
    pub start: usize,
    pub end_inclusive: usize,
    pub store_offset: usize,
    pub width_bits: u8,
    pub endianness: CustomEndianness,
    pub algorithm: CustomChecksumAlgorithm,
    #[serde(default = "default_exclude_store")]
    pub exclude_store_from_range: bool,
    #[serde(default)]
    pub target_magic: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomChecksumPlugin {
    pub id: String,
    pub name: String,
    pub family: String,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub min_rom_size: Option<usize>,
    #[serde(default)]
    pub max_rom_size: Option<usize>,
    pub blocks: Vec<CustomChecksumBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomChecksumBlockResult {
    pub name: String,
    pub start: usize,
    pub end_inclusive: usize,
    pub store_offset: usize,
    pub width_bits: u8,
    pub stored_hex: String,
    pub computed_hex: String,
    pub valid: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomChecksumReport {
    pub plugin_id: String,
    pub plugin_name: String,
    pub family: String,
    pub blocks: Vec<CustomChecksumBlockResult>,
    pub all_valid: bool,
}

/// Returns a self-contained example plugin suitable for loading in the workbench UI.
pub fn sample_custom_checksum_plugin_json() -> String {
    serde_json::to_string_pretty(&CustomChecksumPlugin {
        id: "example_dual_block_v1".into(),
        name: "Example Dual-Block ECU Plugin".into(),
        family: "Generic 16-bit + CRC32".into(),
        notes: Some(
            "Adjust start/end_inclusive/store_offset addresses for your ROM layout.".into(),
        ),
        min_rom_size: Some(256),
        max_rom_size: None,
        blocks: vec![
            CustomChecksumBlock {
                name: "Calibration Sum16 Two's Complement".into(),
                start: 0x0000,
                end_inclusive: 0x007F,
                store_offset: 0x007E,
                width_bits: 16,
                endianness: CustomEndianness::Big,
                algorithm: CustomChecksumAlgorithm::TwosComplement,
                exclude_store_from_range: true,
                target_magic: None,
            },
            CustomChecksumBlock {
                name: "Code Footer CRC32 IEEE".into(),
                start: 0x0080,
                end_inclusive: 0x00FB,
                store_offset: 0x00FC,
                width_bits: 32,
                endianness: CustomEndianness::Little,
                algorithm: CustomChecksumAlgorithm::Crc32Ieee,
                exclude_store_from_range: true,
                target_magic: None,
            },
        ],
    })
    .unwrap_or_else(|_| "{}".to_string())
}

/// Parses and validates a JSON declarative checksum plugin definition.
pub fn parse_custom_checksum_plugin(json: &str) -> Result<CustomChecksumPlugin, String> {
    let plugin: CustomChecksumPlugin =
        serde_json::from_str(json).map_err(|e| format!("Invalid custom checksum JSON: {e}"))?;
    if plugin.id.trim().is_empty() {
        return Err("Plugin 'id' cannot be empty".to_string());
    }
    if plugin.name.trim().is_empty() {
        return Err("Plugin 'name' cannot be empty".to_string());
    }
    if plugin.blocks.is_empty() {
        return Err("Plugin must define at least one checksum block".to_string());
    }
    if plugin.blocks.len() > MAX_CUSTOM_PLUGIN_BLOCKS {
        return Err(format!(
            "Plugin defines {} blocks (max {})",
            plugin.blocks.len(),
            MAX_CUSTOM_PLUGIN_BLOCKS
        ));
    }
    for blk in &plugin.blocks {
        validate_block_spec(blk)?;
    }
    Ok(plugin)
}

fn validate_block_spec(blk: &CustomChecksumBlock) -> Result<(), String> {
    if blk.name.trim().is_empty() {
        return Err("Checksum block name cannot be empty".to_string());
    }
    if blk.end_inclusive < blk.start {
        return Err(format!(
            "Block '{}': end_inclusive ({:#X}) is before start ({:#X})",
            blk.name, blk.end_inclusive, blk.start
        ));
    }
    if !matches!(blk.width_bits, 8 | 16 | 32) {
        return Err(format!(
            "Block '{}': width_bits must be 8, 16, or 32 (got {})",
            blk.name, blk.width_bits
        ));
    }
    let width_ok = match blk.algorithm {
        CustomChecksumAlgorithm::Crc8Maxim => blk.width_bits == 8,
        CustomChecksumAlgorithm::Crc16Ccitt | CustomChecksumAlgorithm::Crc16Arc => {
            blk.width_bits == 16
        }
        CustomChecksumAlgorithm::Crc32Ieee
        | CustomChecksumAlgorithm::Crc32Castagnoli
        | CustomChecksumAlgorithm::Crc32Simos => blk.width_bits == 32,
        CustomChecksumAlgorithm::Sum
        | CustomChecksumAlgorithm::TwosComplement
        | CustomChecksumAlgorithm::OnesComplement
        | CustomChecksumAlgorithm::Xor => true,
    };
    if !width_ok {
        return Err(format!(
            "Block '{}': algorithm {:?} is incompatible with {}-bit width",
            blk.name, blk.algorithm, blk.width_bits
        ));
    }
    if blk.target_magic.is_some()
        && !matches!(
            blk.algorithm,
            CustomChecksumAlgorithm::Sum | CustomChecksumAlgorithm::TwosComplement
        )
    {
        return Err(format!(
            "Block '{}': target_magic is only supported with Sum or TwosComplement algorithms",
            blk.name
        ));
    }
    Ok(())
}

fn width_mask(width_bits: u8) -> u32 {
    match width_bits {
        8 => 0x0000_00FF,
        16 => 0x0000_FFFF,
        32 => 0xFFFF_FFFF,
        _ => 0,
    }
}

fn format_word_hex(val: u32, width_bits: u8) -> String {
    match width_bits {
        8 => format!("0x{:02X}", val & 0xFF),
        16 => format!("0x{:04X}", val & 0xFFFF),
        _ => format!("0x{val:08X}"),
    }
}

fn read_stored_word(rom: &[u8], blk: &CustomChecksumBlock) -> Result<u32, String> {
    let width_bytes = (blk.width_bits / 8) as usize;
    let end = blk
        .store_offset
        .checked_add(width_bytes)
        .ok_or_else(|| format!("Block '{}': store_offset overflow", blk.name))?;
    if end > rom.len() {
        return Err(format!(
            "Block '{}': store_offset {:#X}..{:#X} exceeds ROM length {:#X}",
            blk.name,
            blk.store_offset,
            end,
            rom.len()
        ));
    }
    Ok(match (blk.width_bits, blk.endianness) {
        (8, _) => rom[blk.store_offset] as u32,
        (16, CustomEndianness::Big) => {
            u16::from_be_bytes([rom[blk.store_offset], rom[blk.store_offset + 1]]) as u32
        }
        (16, CustomEndianness::Little) => {
            u16::from_le_bytes([rom[blk.store_offset], rom[blk.store_offset + 1]]) as u32
        }
        (32, CustomEndianness::Big) => u32::from_be_bytes([
            rom[blk.store_offset],
            rom[blk.store_offset + 1],
            rom[blk.store_offset + 2],
            rom[blk.store_offset + 3],
        ]),
        (32, CustomEndianness::Little) => u32::from_le_bytes([
            rom[blk.store_offset],
            rom[blk.store_offset + 1],
            rom[blk.store_offset + 2],
            rom[blk.store_offset + 3],
        ]),
        _ => return Err("Unsupported width".to_string()),
    })
}

fn write_stored_word(rom: &mut [u8], blk: &CustomChecksumBlock, val: u32) -> Result<(), String> {
    let width_bytes = (blk.width_bits / 8) as usize;
    let end = blk
        .store_offset
        .checked_add(width_bytes)
        .ok_or_else(|| format!("Block '{}': store_offset overflow", blk.name))?;
    if end > rom.len() {
        return Err(format!(
            "Block '{}': store_offset exceeds ROM length",
            blk.name
        ));
    }
    match (blk.width_bits, blk.endianness) {
        (8, _) => rom[blk.store_offset] = (val & 0xFF) as u8,
        (16, CustomEndianness::Big) => {
            rom[blk.store_offset..end].copy_from_slice(&(val as u16).to_be_bytes());
        }
        (16, CustomEndianness::Little) => {
            rom[blk.store_offset..end].copy_from_slice(&(val as u16).to_le_bytes());
        }
        (32, CustomEndianness::Big) => {
            rom[blk.store_offset..end].copy_from_slice(&val.to_be_bytes());
        }
        (32, CustomEndianness::Little) => {
            rom[blk.store_offset..end].copy_from_slice(&val.to_le_bytes());
        }
        _ => return Err("Unsupported width".to_string()),
    }
    Ok(())
}

fn build_effective_bytes(rom: &[u8], blk: &CustomChecksumBlock) -> Result<Vec<u8>, String> {
    if blk.end_inclusive >= rom.len() {
        return Err(format!(
            "Block '{}': end_inclusive {:#X} exceeds ROM length {:#X}",
            blk.name,
            blk.end_inclusive,
            rom.len()
        ));
    }
    let width_bytes = (blk.width_bits / 8) as usize;
    let store_end = blk
        .store_offset
        .checked_add(width_bytes)
        .ok_or_else(|| format!("Block '{}': store_offset overflow", blk.name))?;
    let range_end = blk
        .end_inclusive
        .checked_add(1)
        .ok_or_else(|| format!("Block '{}': range overflow", blk.name))?;

    if !blk.exclude_store_from_range || store_end <= blk.start || blk.store_offset >= range_end {
        return Ok(rom[blk.start..range_end].to_vec());
    }

    let mut slice = Vec::with_capacity(range_end - blk.start);
    if blk.start < blk.store_offset {
        slice.extend_from_slice(&rom[blk.start..blk.store_offset]);
    }
    if store_end < range_end {
        slice.extend_from_slice(&rom[store_end..range_end]);
    }
    Ok(slice)
}

fn sum_words(bytes: &[u8], width_bits: u8, endian: CustomEndianness) -> u32 {
    match width_bits {
        8 => bytes.iter().fold(0u32, |acc, &b| acc.wrapping_add(b as u32)),
        16 => {
            let mut sum = 0u32;
            let (chunks, rem) = bytes.as_chunks::<2>();
            for c in chunks {
                let w = match endian {
                    CustomEndianness::Big => u16::from_be_bytes(*c),
                    CustomEndianness::Little => u16::from_le_bytes(*c),
                };
                sum = sum.wrapping_add(w as u32);
            }
            if !rem.is_empty() {
                let w = match endian {
                    CustomEndianness::Big => u16::from_be_bytes([rem[0], 0]),
                    CustomEndianness::Little => u16::from_le_bytes([rem[0], 0]),
                };
                sum = sum.wrapping_add(w as u32);
            }
            sum & 0xFFFF
        }
        32 => {
            let mut sum = 0u32;
            let (chunks, rem) = bytes.as_chunks::<4>();
            for c in chunks {
                let w = match endian {
                    CustomEndianness::Big => u32::from_be_bytes(*c),
                    CustomEndianness::Little => u32::from_le_bytes(*c),
                };
                sum = sum.wrapping_add(w);
            }
            if !rem.is_empty() {
                let mut tail = [0u8; 4];
                tail[..rem.len()].copy_from_slice(rem);
                let w = match endian {
                    CustomEndianness::Big => u32::from_be_bytes(tail),
                    CustomEndianness::Little => u32::from_le_bytes(tail),
                };
                sum = sum.wrapping_add(w);
            }
            sum
        }
        _ => 0,
    }
}

fn xor_words(bytes: &[u8], width_bits: u8, endian: CustomEndianness) -> u32 {
    match width_bits {
        8 => bytes.iter().fold(0u32, |acc, &b| acc ^ (b as u32)),
        16 => {
            let mut acc = 0u32;
            let (chunks, rem) = bytes.as_chunks::<2>();
            for c in chunks {
                let w = match endian {
                    CustomEndianness::Big => u16::from_be_bytes(*c),
                    CustomEndianness::Little => u16::from_le_bytes(*c),
                };
                acc ^= w as u32;
            }
            if !rem.is_empty() {
                let w = match endian {
                    CustomEndianness::Big => u16::from_be_bytes([rem[0], 0]),
                    CustomEndianness::Little => u16::from_le_bytes([rem[0], 0]),
                };
                acc ^= w as u32;
            }
            acc & 0xFFFF
        }
        32 => {
            let mut acc = 0u32;
            let (chunks, rem) = bytes.as_chunks::<4>();
            for c in chunks {
                let w = match endian {
                    CustomEndianness::Big => u32::from_be_bytes(*c),
                    CustomEndianness::Little => u32::from_le_bytes(*c),
                };
                acc ^= w;
            }
            if !rem.is_empty() {
                let mut tail = [0u8; 4];
                tail[..rem.len()].copy_from_slice(rem);
                let w = match endian {
                    CustomEndianness::Big => u32::from_be_bytes(tail),
                    CustomEndianness::Little => u32::from_le_bytes(tail),
                };
                acc ^= w;
            }
            acc
        }
        _ => 0,
    }
}

fn compute_block_value(rom: &[u8], blk: &CustomChecksumBlock) -> Result<u32, String> {
    let data = build_effective_bytes(rom, blk)?;
    let mask = width_mask(blk.width_bits);
    let computed = match blk.algorithm {
        CustomChecksumAlgorithm::Sum => {
            let sum = sum_words(&data, blk.width_bits, blk.endianness);
            if let Some(magic) = blk.target_magic {
                magic.wrapping_sub(sum)
            } else {
                sum
            }
        }
        CustomChecksumAlgorithm::TwosComplement => {
            let sum = sum_words(&data, blk.width_bits, blk.endianness);
            let base = blk.target_magic.unwrap_or(0);
            base.wrapping_sub(sum)
        }
        CustomChecksumAlgorithm::OnesComplement => {
            !sum_words(&data, blk.width_bits, blk.endianness)
        }
        CustomChecksumAlgorithm::Xor => xor_words(&data, blk.width_bits, blk.endianness),
        CustomChecksumAlgorithm::Crc8Maxim => crc8_maxim(&data, 0) as u32,
        CustomChecksumAlgorithm::Crc16Ccitt => crc16_ccitt(&data) as u32,
        CustomChecksumAlgorithm::Crc16Arc => crc16_arc(&data, 0) as u32,
        CustomChecksumAlgorithm::Crc32Ieee => compute_crc32(&data),
        CustomChecksumAlgorithm::Crc32Castagnoli => crc32_castagnoli(&data),
        CustomChecksumAlgorithm::Crc32Simos => crc32_simos(&data),
    };
    Ok(computed & mask)
}

fn check_rom_size(rom: &[u8], plugin: &CustomChecksumPlugin) -> Result<(), String> {
    if rom.is_empty() {
        return Err("ROM buffer cannot be empty".to_string());
    }
    if let Some(min) = plugin.min_rom_size {
        if rom.len() < min {
            return Err(format!(
                "ROM size ({:#X}) is smaller than plugin minimum ({:#X})",
                rom.len(),
                min
            ));
        }
    }
    if let Some(max) = plugin.max_rom_size {
        if rom.len() > max {
            return Err(format!(
                "ROM size ({:#X}) exceeds plugin maximum ({:#X})",
                rom.len(),
                max
            ));
        }
    }
    Ok(())
}

/// Verifies every block defined in `plugin` against `rom`.
pub fn verify_custom_checksum_plugin(
    rom: &[u8],
    plugin: &CustomChecksumPlugin,
) -> Result<CustomChecksumReport, String> {
    check_rom_size(rom, plugin)?;
    let mut results = Vec::with_capacity(plugin.blocks.len());
    let mut all_valid = true;

    for blk in &plugin.blocks {
        validate_block_spec(blk)?;
        let stored = read_stored_word(rom, blk)?;
        let computed = compute_block_value(rom, blk)?;
        let valid = stored == computed;
        all_valid &= valid;
        results.push(CustomChecksumBlockResult {
            name: blk.name.clone(),
            start: blk.start,
            end_inclusive: blk.end_inclusive,
            store_offset: blk.store_offset,
            width_bits: blk.width_bits,
            stored_hex: format_word_hex(stored, blk.width_bits),
            computed_hex: format_word_hex(computed, blk.width_bits),
            valid,
        });
    }

    Ok(CustomChecksumReport {
        plugin_id: plugin.id.clone(),
        plugin_name: plugin.name.clone(),
        family: plugin.family.clone(),
        blocks: results,
        all_valid,
    })
}

/// Repairs every checksum block defined in `plugin` inside `rom` and re-verifies.
pub fn repair_custom_checksum_plugin(
    rom: &mut [u8],
    plugin: &CustomChecksumPlugin,
) -> Result<CustomChecksumReport, String> {
    check_rom_size(rom, plugin)?;
    for blk in &plugin.blocks {
        validate_block_spec(blk)?;
        let computed = compute_block_value(rom, blk)?;
        write_stored_word(rom, blk, computed)?;
    }
    verify_custom_checksum_plugin(rom, plugin)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_plugin_roundtrip_repairs_and_verifies() {
        let json = sample_custom_checksum_plugin_json();
        let plugin = parse_custom_checksum_plugin(&json).expect("valid sample plugin");

        let mut rom = vec![0x22u8; 256];
        let initial = verify_custom_checksum_plugin(&rom, &plugin).expect("verify");
        assert!(!initial.all_valid);

        let repaired = repair_custom_checksum_plugin(&mut rom, &plugin).expect("repair");
        assert!(repaired.all_valid);
        assert_eq!(repaired.blocks.len(), 2);
        assert!(repaired.blocks.iter().all(|b| b.valid));

        rom[0x10] ^= 0xFF;
        let corrupted = verify_custom_checksum_plugin(&rom, &plugin).expect("verify corrupted");
        assert!(!corrupted.all_valid);
        assert!(!corrupted.blocks[0].valid);
        assert!(corrupted.blocks[1].valid);
    }

    #[test]
    fn target_magic_balance_word_keeps_total_sum_constant() {
        let plugin = CustomChecksumPlugin {
            id: "balance_16".into(),
            name: "Balance 16-bit".into(),
            family: "Generic".into(),
            notes: None,
            min_rom_size: Some(64),
            max_rom_size: None,
            blocks: vec![CustomChecksumBlock {
                name: "Sum16 Balance 0x5AA5".into(),
                start: 0x00,
                end_inclusive: 0x3F,
                store_offset: 0x3E,
                width_bits: 16,
                endianness: CustomEndianness::Big,
                algorithm: CustomChecksumAlgorithm::Sum,
                exclude_store_from_range: true,
                target_magic: Some(0x5AA5),
            }],
        };

        let mut rom: Vec<u8> = (0u8..64).collect();
        let rep = repair_custom_checksum_plugin(&mut rom, &plugin).expect("repair balance");
        assert!(rep.all_valid);

        let total_sum: u16 = rom
            .as_chunks::<2>()
            .0
            .iter()
            .fold(0u16, |acc, c| acc.wrapping_add(u16::from_be_bytes([c[0], c[1]])));
        assert_eq!(total_sum, 0x5AA5);
    }

    #[test]
    fn supports_crc16_ccitt_and_crc32_castagnoli() {
        let plugin = CustomChecksumPlugin {
            id: "crc_mix".into(),
            name: "CRC Mix".into(),
            family: "Multi".into(),
            notes: None,
            min_rom_size: None,
            max_rom_size: None,
            blocks: vec![
                CustomChecksumBlock {
                    name: "CRC16-CCITT".into(),
                    start: 0,
                    end_inclusive: 61,
                    store_offset: 62,
                    width_bits: 16,
                    endianness: CustomEndianness::Big,
                    algorithm: CustomChecksumAlgorithm::Crc16Ccitt,
                    exclude_store_from_range: false,
                    target_magic: None,
                },
                CustomChecksumBlock {
                    name: "CRC32-C".into(),
                    start: 64,
                    end_inclusive: 123,
                    store_offset: 124,
                    width_bits: 32,
                    endianness: CustomEndianness::Little,
                    algorithm: CustomChecksumAlgorithm::Crc32Castagnoli,
                    exclude_store_from_range: true,
                    target_magic: None,
                },
            ],
        };

        let mut rom = vec![0x42u8; 128];
        let report = repair_custom_checksum_plugin(&mut rom, &plugin).expect("repair crc mix");
        assert!(report.all_valid);
        assert_eq!(
            u16::from_be_bytes([rom[62], rom[63]]),
            crc16_ccitt(&rom[0..=61])
        );
        assert_eq!(
            u32::from_le_bytes([rom[124], rom[125], rom[126], rom[127]]),
            crc32_castagnoli(&rom[64..=123])
        );
    }

    #[test]
    fn rejects_invalid_plugin_widths_and_out_of_bounds_ranges() {
        let bad_json = r#"{
            "id": "bad",
            "name": "Bad Width",
            "family": "Test",
            "blocks": [{
                "name": "Mismatch",
                "start": 0,
                "end_inclusive": 15,
                "store_offset": 14,
                "width_bits": 16,
                "endianness": "Big",
                "algorithm": "Crc32Ieee"
            }]
        }"#;
        assert!(parse_custom_checksum_plugin(bad_json).is_err());

        let oob_plugin = parse_custom_checksum_plugin(&sample_custom_checksum_plugin_json()).unwrap();
        let small = vec![0u8; 32];
        assert!(verify_custom_checksum_plugin(&small, &oob_plugin).is_err());
    }
}
