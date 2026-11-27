//! Bosch EDC16 (U1 / U31 / U34 / C8 / C31 / C39) checksum verifier & repairer.
//!
//! Ported from ZedSuite's `edc16-checksum.ts`.
//! Protected regions carry magic `FA DE CA FE CA FE AF FE` at `start + 0x3C`, with
//! big-endian bounds `[start_addr, end_addr]` stored immediately before the magic.
//! The sum of big-endian 32-bit words across `[start..=end]` must equal `0xD01FE500`,
//! balanced at the final dword `end + 1 - 4`.

use super::{u32_be, write_u32_be, OemChecksumBlock, OemChecksumReport};

const EDC16_MAGIC: [u8; 8] = [0xFA, 0xDE, 0xCA, 0xFE, 0xCA, 0xFE, 0xAF, 0xFE];
const MAGIC_OFFSET: usize = 0x3C;
const REGION_ALIGN: usize = 0x100;
const TARGET_SUM: u32 = 0xD01F_E500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edc16Region {
    pub start: usize,
    pub end: usize,
    pub checksum_offset: usize,
}

/// Locates every valid aligned EDC16 checksum region in `rom`.
pub fn find_edc16_regions(rom: &[u8]) -> Vec<Edc16Region> {
    let mut regions = Vec::new();
    if rom.len() < 8 + EDC16_MAGIC.len() {
        return regions;
    }
    for i in 8..=rom.len() - EDC16_MAGIC.len() {
        if rom[i..i + EDC16_MAGIC.len()] != EDC16_MAGIC {
            continue;
        }
        let start_addr = u32_be(rom, i - 8) as usize;
        let end_addr = u32_be(rom, i - 4) as usize;
        let Some(anchor_off) = i.checked_sub(MAGIC_OFFSET) else {
            continue;
        };
        if start_addr < anchor_off || end_addr <= start_addr {
            continue;
        }
        let base = start_addr - anchor_off;
        let start = start_addr - base;
        let end = end_addr - base;
        if end >= rom.len()
            || !start.is_multiple_of(REGION_ALIGN)
            || !(end + 1).is_multiple_of(REGION_ALIGN)
            || !(end + 1 - start).is_multiple_of(4)
        {
            continue;
        }
        regions.push(Edc16Region {
            start,
            end,
            checksum_offset: end + 1 - 4,
        });
    }
    regions
}

fn sum_u32_be_inclusive(rom: &[u8], start: usize, end: usize) -> u32 {
    let mut sum = 0u32;
    let mut i = start;
    while i + 3 <= end && i + 3 < rom.len() {
        sum = sum.wrapping_add(u32_be(rom, i));
        i += 4;
    }
    sum
}

pub fn verify_edc16(rom: &[u8]) -> Option<OemChecksumReport> {
    let regions = find_edc16_regions(rom);
    if regions.is_empty() {
        return None;
    }
    let blocks: Vec<OemChecksumBlock> = regions
        .into_iter()
        .enumerate()
        .map(|(idx, r)| {
            let sum = sum_u32_be_inclusive(rom, r.start, r.end);
            let stored = u32_be(rom, r.checksum_offset);
            let expected = stored.wrapping_add(TARGET_SUM.wrapping_sub(sum));
            let ok = sum == TARGET_SUM;
            OemChecksumBlock {
                label: format!("EDC16 Region {}", idx + 1),
                start: r.start,
                end: r.end,
                store_offset: Some(r.checksum_offset),
                stored: Some(stored),
                computed: Some(expected),
                width_bits: 32,
                correctable: true,
                ok,
                status: if ok { "ok" } else { "stale" }.to_string(),
            }
        })
        .collect();
    let all_ok = blocks.iter().all(|b| b.ok);
    Some(OemChecksumReport {
        algorithm_id: "bosch_edc16".to_string(),
        algorithm_name: "Bosch EDC16U (FADECAFE descriptor)".to_string(),
        family: "Bosch EDC16".to_string(),
        variant: format!("{} KiB", rom.len() / 1024),
        blocks,
        all_ok,
        notes: vec![],
    })
}

pub fn repair_edc16(rom: &[u8]) -> Result<(Vec<u8>, OemChecksumReport), String> {
    let report = verify_edc16(rom).ok_or_else(|| "No Bosch EDC16 FADECAFE regions detected".to_string())?;
    let mut out = rom.to_vec();
    for b in &report.blocks {
        if !b.ok {
            if let (Some(off), Some(expected)) = (b.store_offset, b.computed) {
                write_u32_be(&mut out, off, expected);
            }
        }
    }
    let after = verify_edc16(&out).ok_or_else(|| "EDC16 post-repair verify failed".to_string())?;
    Ok((out, after))
}
