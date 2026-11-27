//! Multi-ECU auto-detection, verification, and repair checksum engines.
//!
//! Covers 8 verified motorcycle and powersports/OEM calibration checksum families:
//! `honda_keihin_moto`, `yamaha_mt`, `bosch_mse6`, `bosch_mse8`, `bosch_mg1`
//! (with both CRC rewrite and GF(2) keep-CRC compensation), `dellorto_ecm`,
//! `italjet_suzuki`, and `qh250`.

use crate::primitives::{compute_crc32, compute_crc32_with_initial};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Strategy when repairing a block checksum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChecksumRepairMode {
    /// Recompute and overwrite the stored checksum word/dword.
    UpdateChecksum,
    /// Preserve the stored CRC and solve an unused 32-bit slot via GF(2) linear algebra (`bosch_mg1`).
    CompensateKeepCrc,
}

/// Static descriptor of one registered multi-ECU checksum engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChecksumEngineInfo {
    pub id: String,
    pub name: String,
    pub status: String,
    pub description: String,
    pub supports_keep_crc_mode: bool,
}

/// Verification result for one checksum-protected region in a ROM.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChecksumBlockStatus {
    pub label: String,
    pub offset: usize,
    pub start: usize,
    pub end: usize,
    pub stored: u32,
    pub computed: u32,
    pub width_bits: u8,
    pub ok: bool,
}

/// Result returned when an algorithm recognises a ROM layout or completes verification/repair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectedChecksumMatch {
    pub algorithm_id: String,
    pub algorithm_name: String,
    pub status: String,
    pub notes: String,
    pub supports_keep_crc_mode: bool,
    pub blocks: Vec<ChecksumBlockStatus>,
    pub all_ok: bool,
}

fn find_subslice(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() || haystack.len() < from + needle.len() {
        return None;
    }
    haystack[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|pos| from + pos)
}

#[inline]
fn read_u16_be(bytes: &[u8], off: usize) -> u16 {
    u16::from_be_bytes([bytes[off], bytes[off + 1]])
}

#[inline]
fn read_u32_be(bytes: &[u8], off: usize) -> u32 {
    u32::from_be_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]])
}

#[inline]
fn write_u16_be(bytes: &mut [u8], off: usize, v: u16) {
    let b = v.to_be_bytes();
    bytes[off] = b[0];
    bytes[off + 1] = b[1];
}

#[inline]
fn write_u32_be(bytes: &mut [u8], off: usize, v: u32) {
    let b = v.to_be_bytes();
    bytes[off..off + 4].copy_from_slice(&b);
}

// ---------------------------------------------------------------------------
// 1. honda_keihin_moto (X-ADV 750 512 KiB & ADV350 768 KiB)
// ---------------------------------------------------------------------------

fn honda_keihin_moto_layout(rom: &[u8]) -> Option<(&'static str, usize, usize)> {
    let n = rom.len();
    if n < 16 {
        return None;
    }
    let loc = n - 8;
    for (i, &b) in rom[n - 16..n].iter().enumerate() {
        let abs = n - 16 + i;
        if abs != loc && b != 0xFF {
            return None;
        }
    }
    if n == 0x80000 && find_subslice(rom, b"MKH", 0).is_some() {
        return Some(("X-ADV 750", 0, loc));
    }
    if n == 0xC0000 && n >= 0x80 {
        let id_off = n - 0x80;
        let is_kn6 = &rom[id_off..id_off + 3] == b"KN6"
            && rom[id_off + 3..id_off + 8]
                .iter()
                .all(|&c| c.is_ascii_digit() || c.is_ascii_uppercase());
        if is_kn6 {
            return Some(("ADV350", 0x10000, loc));
        }
    }
    None
}

fn honda_keihin_moto_sum(rom: &[u8], start: usize, loc: usize) -> u8 {
    let mut sum: u32 = 0;
    for (i, &b) in rom.iter().enumerate().skip(start) {
        if i != loc {
            sum = sum.wrapping_add(b as u32);
        }
    }
    0u8.wrapping_sub((sum & 0xFF) as u8)
}

fn verify_honda_keihin_moto(rom: &[u8]) -> Option<DetectedChecksumMatch> {
    let (model, start, loc) = honda_keihin_moto_layout(rom)?;
    let stored = rom[loc] as u32;
    let computed = honda_keihin_moto_sum(rom, start, loc) as u32;
    let ok = stored == computed;
    Some(DetectedChecksumMatch {
        algorithm_id: "honda_keihin_moto".to_string(),
        algorithm_name: "Honda (Keihin)".to_string(),
        status: "confirmed".to_string(),
        notes: format!("{model} 8-bit checksum at end - 8"),
        supports_keep_crc_mode: false,
        blocks: vec![ChecksumBlockStatus {
            label: format!("{model}: byte sum = 0 (mod 256)"),
            offset: loc,
            start,
            end: rom.len() - 1,
            stored,
            computed,
            width_bits: 8,
            ok,
        }],
        all_ok: ok,
    })
}

fn repair_honda_keihin_moto(rom: &[u8]) -> Result<(Vec<u8>, DetectedChecksumMatch), String> {
    let (_model, start, loc) =
        honda_keihin_moto_layout(rom).ok_or_else(|| "not a Honda Keihin X-ADV/ADV350 image".to_string())?;
    let mut out = rom.to_vec();
    out[loc] = honda_keihin_moto_sum(&out, start, loc);
    let m = verify_honda_keihin_moto(&out).ok_or_else(|| "self-verify failed".to_string())?;
    Ok((out, m))
}

// ---------------------------------------------------------------------------
// 2. yamaha_mt (MT-25 / MT-07 / MT-09 — 1 MiB target 0x5AA5, 896 KiB target 0xE81F)
// ---------------------------------------------------------------------------

fn yamaha_mt_layout(rom: &[u8]) -> Option<(usize, u16)> {
    let n = rom.len();
    let target = match n {
        0x100000 => 0x5AA5u16,
        0xE0000 => 0xE81Fu16,
        _ => return None,
    };
    let id_off = n - 16;
    if !rom[id_off..id_off + 8]
        .iter()
        .all(|&c| c.is_ascii_digit() || c.is_ascii_uppercase())
    {
        return None;
    }
    if rom[id_off + 4] != b'Y' || rom[n - 6] != 0x64 || rom[n - 5] != 0x02 {
        return None;
    }
    Some((n - 8, target))
}

fn yamaha_mt_compute(rom: &[u8], loc: usize, target: u16) -> u16 {
    let mut sum: u32 = 0;
    let mut i = 0usize;
    while i + 1 < rom.len() {
        if i != loc {
            sum = sum.wrapping_add(read_u16_be(rom, i) as u32);
        }
        i += 2;
    }
    target.wrapping_sub((sum & 0xFFFF) as u16)
}

fn verify_yamaha_mt(rom: &[u8]) -> Option<DetectedChecksumMatch> {
    let (loc, target) = yamaha_mt_layout(rom)?;
    let stored = read_u16_be(rom, loc) as u32;
    let computed = yamaha_mt_compute(rom, loc, target) as u32;
    let ok = stored == computed;
    Some(DetectedChecksumMatch {
        algorithm_id: "yamaha_mt".to_string(),
        algorithm_name: "Yamaha MT series".to_string(),
        status: "confirmed".to_string(),
        notes: format!("16-bit BE word sum of image = 0x{target:04X}"),
        supports_keep_crc_mode: false,
        blocks: vec![ChecksumBlockStatus {
            label: format!("Word sum = 0x{target:04X}"),
            offset: loc,
            start: 0,
            end: rom.len() - 1,
            stored,
            computed,
            width_bits: 16,
            ok,
        }],
        all_ok: ok,
    })
}

fn repair_yamaha_mt(rom: &[u8]) -> Result<(Vec<u8>, DetectedChecksumMatch), String> {
    let (loc, target) =
        yamaha_mt_layout(rom).ok_or_else(|| "not a Yamaha MT series image".to_string())?;
    let mut out = rom.to_vec();
    let v = yamaha_mt_compute(&out, loc, target);
    write_u16_be(&mut out, loc, v);
    let m = verify_yamaha_mt(&out).ok_or_else(|| "self-verify failed".to_string())?;
    Ok((out, m))
}

// ---------------------------------------------------------------------------
// 3. bosch_mse6 ([start:u32][end:u32][cs:u32][~cs:u32] at 0x140000..=0x140400)
// ---------------------------------------------------------------------------

fn mse6_sum16be(rom: &[u8], start: usize, end: usize) -> u32 {
    let mut total: u64 = 0;
    let mut off = start;
    while off < end {
        if off + 1 < end && off + 1 < rom.len() {
            total += read_u16_be(rom, off) as u64;
        }
        off += 2;
    }
    (total & 0xFFFF_FFFF) as u32
}

#[derive(Clone, Copy)]
struct Mse6Record {
    offset: usize,
    start: usize,
    end: usize,
    stored: u32,
}

fn mse6_find_records(rom: &[u8]) -> Vec<Mse6Record> {
    const SCAN_START: usize = 0x140000;
    const SCAN_END: usize = 0x140400;
    const MIN_SPAN: usize = 0x1000;

    if rom.len() < SCAN_START + 16 {
        return Vec::new();
    }
    let limit = SCAN_END.min(rom.len() - 16);
    let mut records = Vec::new();
    let mut off = SCAN_START;
    while off <= limit {
        let start = read_u32_be(rom, off) as usize;
        let end = read_u32_be(rom, off + 4) as usize;
        let cs = read_u32_be(rom, off + 8);
        let ncs = read_u32_be(rom, off + 12);
        if (cs ^ ncs) == 0xFFFF_FFFF
            && start < end
            && end < rom.len()
            && (end - start) >= MIN_SPAN
        {
            records.push(Mse6Record {
                offset: off,
                start,
                end,
                stored: cs,
            });
        }
        off += 4;
    }
    records
}

fn verify_bosch_mse6(rom: &[u8]) -> Option<DetectedChecksumMatch> {
    let recs = mse6_find_records(rom);
    if recs.is_empty() {
        return None;
    }
    let blocks: Vec<ChecksumBlockStatus> = recs
        .iter()
        .map(|r| {
            let computed = mse6_sum16be(rom, r.start, r.end);
            ChecksumBlockStatus {
                label: format!("MSE6 @0x{:06X}", r.offset),
                offset: r.offset + 8,
                start: r.start,
                end: r.end.saturating_sub(1),
                stored: r.stored,
                computed,
                width_bits: 32,
                ok: r.stored == computed,
            }
        })
        .collect();
    let all_ok = blocks.iter().all(|b| b.ok);
    Some(DetectedChecksumMatch {
        algorithm_id: "bosch_mse6".to_string(),
        algorithm_name: "Bosch MSE6.0".to_string(),
        status: "confirmed".to_string(),
        notes: "Structural [start][end][sum16be][~sum16be] descriptors around 0x140000".to_string(),
        supports_keep_crc_mode: false,
        blocks,
        all_ok,
    })
}

fn repair_bosch_mse6(rom: &[u8]) -> Result<(Vec<u8>, DetectedChecksumMatch), String> {
    let recs = mse6_find_records(rom);
    if recs.is_empty() {
        return Err("no Bosch MSE6.0 checksum records found".to_string());
    }
    let mut out = rom.to_vec();
    for r in &recs {
        let computed = mse6_sum16be(&out, r.start, r.end);
        write_u32_be(&mut out, r.offset + 8, computed);
        write_u32_be(&mut out, r.offset + 12, !computed);
    }
    let m = verify_bosch_mse6(&out).ok_or_else(|| "self-verify failed".to_string())?;
    Ok((out, m))
}

// ---------------------------------------------------------------------------
// 4. bosch_mse8 (0x40000 calibration block, footer sig @0x3FFE0, CRC @0x3FFF8 ^ 0xA18AFC77)
// ---------------------------------------------------------------------------

fn mse8_find_blocks(rom: &[u8]) -> Vec<usize> {
    const BLOCK_SIZE: usize = 0x40000;
    const FOOTER_REL: usize = 0x3FFE0;
    const FOOTER_SIG: [u8; 8] = [0x12, 0x34, 0x56, 0x78, 0x12, 0x34, 0x10, 0x08];

    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(pos) = find_subslice(rom, &FOOTER_SIG, from) {
        if pos >= FOOTER_REL {
            let block = pos - FOOTER_REL;
            if block + BLOCK_SIZE <= rom.len() {
                out.push(block);
            }
        }
        from = pos + 1;
    }
    out
}

fn mse8_compute(rom: &[u8], block: usize) -> u32 {
    const CRC_REL: usize = 0x3FFF8;
    const K: u32 = 0xA18A_FC77;
    compute_crc32(&rom[block..block + CRC_REL]) ^ K
}

fn verify_bosch_mse8(rom: &[u8]) -> Option<DetectedChecksumMatch> {
    const CRC_REL: usize = 0x3FFF8;
    let blocks_idx = mse8_find_blocks(rom);
    if blocks_idx.is_empty() {
        return None;
    }
    let blocks: Vec<ChecksumBlockStatus> = blocks_idx
        .into_iter()
        .map(|block| {
            let off = block + CRC_REL;
            let stored = read_u32_be(rom, off);
            let computed = mse8_compute(rom, block);
            ChecksumBlockStatus {
                label: format!("MSE8 Cal Block @0x{block:06X}"),
                offset: off,
                start: block,
                end: off - 1,
                stored,
                computed,
                width_bits: 32,
                ok: stored == computed,
            }
        })
        .collect();
    let all_ok = blocks.iter().all(|b| b.ok);
    Some(DetectedChecksumMatch {
        algorithm_id: "bosch_mse8".to_string(),
        algorithm_name: "Bosch MSE8.0".to_string(),
        status: "confirmed".to_string(),
        notes: "256 KiB calibration block with footer 1234567812341008 @+0x3FFE0".to_string(),
        supports_keep_crc_mode: false,
        blocks,
        all_ok,
    })
}

fn repair_bosch_mse8(rom: &[u8]) -> Result<(Vec<u8>, DetectedChecksumMatch), String> {
    const CRC_REL: usize = 0x3FFF8;
    let blocks = mse8_find_blocks(rom);
    if blocks.is_empty() {
        return Err("no Bosch MSE8.0 calibration block found".to_string());
    }
    let mut out = rom.to_vec();
    for block in blocks {
        let computed = mse8_compute(&out, block);
        write_u32_be(&mut out, block + CRC_REL, computed);
    }
    let m = verify_bosch_mse8(&out).ok_or_else(|| "self-verify failed".to_string())?;
    Ok((out, m))
}

// ---------------------------------------------------------------------------
// 5. bosch_mg1 (DEADBEEF..0x15C*12345678 + update OR GF(2) keep-CRC compensation)
// ---------------------------------------------------------------------------

const MG1_SEED: u32 = 0x533F_71C8;

fn mg1_find_blocks(rom: &[u8]) -> Vec<(usize, usize)> {
    const STEP: usize = 0x40000;
    const FILL_LEN: usize = 0x15C;
    const HEAD: [u8; 4] = [0xDE, 0xAD, 0xBE, 0xEF];
    const FILL: [u8; 4] = [0x12, 0x34, 0x56, 0x78];
    const MSE8_FOOTER: [u8; 4] = [0x12, 0x34, 0x10, 0x08];

    let mut blocks = Vec::new();
    let mut from = 0usize;
    while let Some(p) = find_subslice(rom, &FILL, from) {
        let mut e = p;
        while e + 4 <= rom.len() && rom[e..e + 4] == FILL {
            e += 4;
        }
        if e - p == FILL_LEN
            && e + 4 <= rom.len()
            && rom[e..e + 4] != MSE8_FOOTER
        {
            let mut s = (p / STEP) * STEP;
            loop {
                if s + 4 <= rom.len() && rom[s..s + 4] == HEAD {
                    blocks.push((s, e));
                    break;
                }
                if s < STEP {
                    break;
                }
                s -= STEP;
            }
        }
        from = if e > p { e } else { p + 1 };
    }
    blocks
}

#[inline]
fn mg1_compute(rom: &[u8], start: usize, crc_off: usize) -> u32 {
    compute_crc32_with_initial(&rom[start..crc_off], MG1_SEED)
}

fn mg1_slot_for(rom: &[u8], start: usize, crc_off: usize) -> Option<usize> {
    const FA8: [u8; 8] = [0xFA; 8];
    let fa_abs = find_subslice(&rom[..crc_off], &FA8, start)?;
    if fa_abs >= start + 4 {
        Some(fa_abs - 4)
    } else {
        None
    }
}

fn mg1_compensate(buf: &mut [u8], start: usize, crc_off: usize, slot: usize, target: u32) -> Result<(), String> {
    let base = mg1_compute(buf, start, crc_off);
    let mut need = base ^ target;
    if need == 0 {
        return Ok(());
    }

    // Build GF(2) pivot basis for flipping each of the 32 bits at `slot`.
    let mut basis: BTreeMap<u32, (u32, u32)> = BTreeMap::new();
    for bit in 0..32u32 {
        let byte_idx = slot + (bit as usize >> 3);
        let mask = 0x80u8 >> (bit % 8);
        buf[byte_idx] ^= mask;
        let mut eff = mg1_compute(buf, start, crc_off) ^ base;
        buf[byte_idx] ^= mask;

        let mut comb = 1u32 << bit;
        for (&hb, &(e, c)) in basis.iter().rev() {
            if ((eff >> hb) & 1) != 0 {
                eff ^= e;
                comb ^= c;
            }
        }
        if eff != 0 {
            let hb = 31 - eff.leading_zeros();
            basis.insert(hb, (eff, comb));
        }
    }

    let mut x = 0u32;
    for (&hb, &(e, c)) in basis.iter().rev() {
        if ((need >> hb) & 1) != 0 {
            need ^= e;
            x ^= c;
        }
    }
    if need != 0 {
        return Err(format!("cannot solve GF(2) CRC compensation at 0x{slot:06X}"));
    }

    for bit in 0..32u32 {
        if ((x >> bit) & 1) != 0 {
            let byte_idx = slot + (bit as usize >> 3);
            let mask = 0x80u8 >> (bit % 8);
            buf[byte_idx] ^= mask;
        }
    }
    Ok(())
}

fn verify_bosch_mg1(rom: &[u8]) -> Option<DetectedChecksumMatch> {
    let blk_list = mg1_find_blocks(rom);
    if blk_list.is_empty() {
        return None;
    }
    let blocks: Vec<ChecksumBlockStatus> = blk_list
        .into_iter()
        .map(|(s, crc_off)| {
            let stored = read_u32_be(rom, crc_off);
            let computed = mg1_compute(rom, s, crc_off);
            ChecksumBlockStatus {
                label: format!("MG1 Block @0x{s:06X}"),
                offset: crc_off,
                start: s,
                end: crc_off - 1,
                stored,
                computed,
                width_bits: 32,
                ok: stored == computed,
            }
        })
        .collect();
    let all_ok = blocks.iter().all(|b| b.ok);
    Some(DetectedChecksumMatch {
        algorithm_id: "bosch_mg1".to_string(),
        algorithm_name: "Bosch MG1 (MG1CA094)".to_string(),
        status: "confirmed".to_string(),
        notes: "CRC-32 (zlib seed 0x533F71C8) across DEADBEEF blocks; fixes CRCs, not RSA signature".to_string(),
        supports_keep_crc_mode: true,
        blocks,
        all_ok,
    })
}

fn repair_bosch_mg1(
    rom: &[u8],
    mode: ChecksumRepairMode,
) -> Result<(Vec<u8>, DetectedChecksumMatch), String> {
    let blk_list = mg1_find_blocks(rom);
    if blk_list.is_empty() {
        return Err("no Bosch MG1 block found".to_string());
    }
    let mut out = rom.to_vec();
    for (s, crc_off) in blk_list {
        let stored = read_u32_be(&out, crc_off);
        let computed = mg1_compute(&out, s, crc_off);
        if computed == stored {
            continue;
        }
        match mode {
            ChecksumRepairMode::UpdateChecksum => {
                write_u32_be(&mut out, crc_off, computed);
            }
            ChecksumRepairMode::CompensateKeepCrc => {
                let p = mg1_slot_for(&out, s, crc_off).ok_or_else(|| {
                    format!("keep mode: no FA.. signature slot in MG1 block @0x{s:06X}; use UpdateChecksum")
                })?;
                let is_zero_slot = out[p..p + 4] == [0, 0, 0, 0];
                if !(is_zero_slot || (crc_off - s) == 0x7F18) {
                    return Err(format!(
                        "keep mode: slot @0x{p:06X} in block @0x{s:06X} is not free; use UpdateChecksum"
                    ));
                }
                mg1_compensate(&mut out, s, crc_off, p, stored)?;
            }
        }
    }
    let m = verify_bosch_mg1(&out).ok_or_else(|| "self-verify failed".to_string())?;
    Ok((out, m))
}

// ---------------------------------------------------------------------------
// 6. dellorto_ecm (0xC0000, block @0x80000 "JL", CRC16 ~byte_sum @0x921FE)
// ---------------------------------------------------------------------------

fn dellorto_recognised(rom: &[u8]) -> bool {
    const SIG: &[u8] = b"DellOrto 32bit Engine Management System";
    const BLOCK: usize = 0x80000;
    rom.len() == 0xC0000
        && &rom[BLOCK..BLOCK + 2] == b"JL"
        && find_subslice(rom, SIG, 0).is_some()
}

fn dellorto_compute(rom: &[u8]) -> u16 {
    const BLOCK: usize = 0x80000;
    const LOC: usize = 0x921FE;
    let mut sum: u32 = 0;
    for &b in &rom[BLOCK..LOC] {
        sum = sum.wrapping_add(b as u32);
    }
    (!sum & 0xFFFF) as u16
}

fn verify_dellorto_ecm(rom: &[u8]) -> Option<DetectedChecksumMatch> {
    const BLOCK: usize = 0x80000;
    const LOC: usize = 0x921FE;
    if !dellorto_recognised(rom) {
        return None;
    }
    let stored = read_u16_be(rom, LOC) as u32;
    let computed = dellorto_compute(rom) as u32;
    let ok = stored == computed;
    Some(DetectedChecksumMatch {
        algorithm_id: "dellorto_ecm".to_string(),
        algorithm_name: "Dell'Orto ECM".to_string(),
        status: "confirmed".to_string(),
        notes: "One's complement byte sum of 0x80000..0x921FD stored BE at 0x921FE".to_string(),
        supports_keep_crc_mode: false,
        blocks: vec![ChecksumBlockStatus {
            label: "Calibration block @0x80000".to_string(),
            offset: LOC,
            start: BLOCK,
            end: LOC - 1,
            stored,
            computed,
            width_bits: 16,
            ok,
        }],
        all_ok: ok,
    })
}

fn repair_dellorto_ecm(rom: &[u8]) -> Result<(Vec<u8>, DetectedChecksumMatch), String> {
    const LOC: usize = 0x921FE;
    if !dellorto_recognised(rom) {
        return Err("not a Dell'Orto ECM image".to_string());
    }
    let mut out = rom.to_vec();
    let v = dellorto_compute(&out);
    write_u16_be(&mut out, LOC, v);
    let m = verify_dellorto_ecm(&out).ok_or_else(|| "self-verify failed".to_string())?;
    Ok((out, m))
}

// ---------------------------------------------------------------------------
// 7. italjet_suzuki (0x80000, AA 55 CC 33 + 12 ASCII + LE checksum/complement)
// ---------------------------------------------------------------------------

fn italjet_find_loc(rom: &[u8]) -> Option<usize> {
    const MARK: [u8; 4] = [0xAA, 0x55, 0xCC, 0x33];
    const START: usize = 0x20000;
    if rom.len() != 0x80000 {
        return None;
    }
    let mut from = 0usize;
    while let Some(p) = find_subslice(rom, &MARK, from) {
        let loc = p + 16;
        if loc >= START
            && loc + 4 <= rom.len()
            && rom[p + 4..loc].iter().all(|&b| (0x20..=0x7E).contains(&b))
            && (rom[loc] ^ rom[loc + 2]) == 0xFF
            && (rom[loc + 1] ^ rom[loc + 3]) == 0xFF
        {
            return Some(loc);
        }
        from = p + 1;
    }
    None
}

fn italjet_compute(rom: &[u8], loc: usize) -> u16 {
    const START: usize = 0x20000;
    let mut sum: u32 = 0x1FE;
    for (i, &b) in rom.iter().enumerate().skip(START) {
        if i < loc || i >= loc + 4 {
            sum = sum.wrapping_add(b as u32);
        }
    }
    (sum & 0xFFFF) as u16
}

fn verify_italjet_suzuki(rom: &[u8]) -> Option<DetectedChecksumMatch> {
    const START: usize = 0x20000;
    let loc = italjet_find_loc(rom)?;
    let stored = u16::from_le_bytes([rom[loc], rom[loc + 1]]) as u32;
    let computed = italjet_compute(rom, loc) as u32;
    let ok = stored == computed;
    Some(DetectedChecksumMatch {
        algorithm_id: "italjet_suzuki".to_string(),
        algorithm_name: "Italjet / Suzuki-type ECU".to_string(),
        status: "confirmed".to_string(),
        notes: "16-bit LE byte sum 0x20000..end with ~checksum pair next to AA55CC33 part ID".to_string(),
        supports_keep_crc_mode: false,
        blocks: vec![ChecksumBlockStatus {
            label: "Byte sum 0x20000-end".to_string(),
            offset: loc,
            start: START,
            end: rom.len() - 1,
            stored,
            computed,
            width_bits: 16,
            ok,
        }],
        all_ok: ok,
    })
}

fn repair_italjet_suzuki(rom: &[u8]) -> Result<(Vec<u8>, DetectedChecksumMatch), String> {
    let loc = italjet_find_loc(rom).ok_or_else(|| "not an Italjet / Suzuki-type image".to_string())?;
    let mut out = rom.to_vec();
    let v = italjet_compute(&out, loc);
    let le = v.to_le_bytes();
    out[loc] = le[0];
    out[loc + 1] = le[1];
    out[loc + 2] = !le[0];
    out[loc + 3] = !le[1];
    let m = verify_italjet_suzuki(&out).ok_or_else(|| "self-verify failed".to_string())?;
    Ok((out, m))
}

// ---------------------------------------------------------------------------
// 8. qh250 (0x140000, AAAA..5554AA45, BE word sum 0..0x7FF0 == 0 @0x7FEE)
// ---------------------------------------------------------------------------

fn qh250_recognised(rom: &[u8]) -> bool {
    rom.len() == 0x140000
        && rom[0] == 0xAA
        && rom[1] == 0xAA
        && rom[0x7FFC..0x8000] == [0x55, 0x54, 0xAA, 0x45]
}

fn qh250_compute(rom: &[u8]) -> u16 {
    const LOC: usize = 0x7FEE;
    const END: usize = 0x7FF0;
    let mut sum: u32 = 0;
    let mut i = 0usize;
    while i < END {
        if i != LOC {
            sum = sum.wrapping_add(read_u16_be(rom, i) as u32);
        }
        i += 2;
    }
    0u16.wrapping_sub((sum & 0xFFFF) as u16)
}

fn verify_qh250(rom: &[u8]) -> Option<DetectedChecksumMatch> {
    const LOC: usize = 0x7FEE;
    const END: usize = 0x7FF0;
    if !qh250_recognised(rom) {
        return None;
    }
    let stored = read_u16_be(rom, LOC) as u32;
    let computed = qh250_compute(rom) as u32;
    let ok = stored == computed;
    Some(DetectedChecksumMatch {
        algorithm_id: "qh250".to_string(),
        algorithm_name: "QH250 (E2400)".to_string(),
        status: "confirmed".to_string(),
        notes: "16-bit BE word sum of 0x0000..0x7FEF equals 0 mod 65536".to_string(),
        supports_keep_crc_mode: false,
        blocks: vec![ChecksumBlockStatus {
            label: "Data block word sum = 0".to_string(),
            offset: LOC,
            start: 0,
            end: END - 1,
            stored,
            computed,
            width_bits: 16,
            ok,
        }],
        all_ok: ok,
    })
}

fn repair_qh250(rom: &[u8]) -> Result<(Vec<u8>, DetectedChecksumMatch), String> {
    const LOC: usize = 0x7FEE;
    if !qh250_recognised(rom) {
        return Err("not a QH250 image".to_string());
    }
    let mut out = rom.to_vec();
    let v = qh250_compute(&out);
    write_u16_be(&mut out, LOC, v);
    let m = verify_qh250(&out).ok_or_else(|| "self-verify failed".to_string())?;
    Ok((out, m))
}

// ---------------------------------------------------------------------------
// Public Dispatch Registry
// ---------------------------------------------------------------------------

/// Lists all 8 registered multi-ECU checksum engines.
pub fn list_supported_checksum_engines() -> Vec<ChecksumEngineInfo> {
    vec![
        ChecksumEngineInfo {
            id: "honda_keihin_moto".to_string(),
            name: "Honda (Keihin)".to_string(),
            status: "confirmed".to_string(),
            description: "X-ADV 750 (512 KiB MKH) & ADV350 (768 KiB KN6) 8-bit sum = 0 mod 256".to_string(),
            supports_keep_crc_mode: false,
        },
        ChecksumEngineInfo {
            id: "yamaha_mt".to_string(),
            name: "Yamaha MT series".to_string(),
            status: "confirmed".to_string(),
            description: "MT-25 / MT-07 / MT-09 16-bit BE word sum = 0x5AA5 (1 MiB) or 0xE81F (896 KiB)".to_string(),
            supports_keep_crc_mode: false,
        },
        ChecksumEngineInfo {
            id: "bosch_mse6".to_string(),
            name: "Bosch MSE6.0".to_string(),
            status: "confirmed".to_string(),
            description: "16-bit BE word sum across [start][end][cs][~cs] descriptors".to_string(),
            supports_keep_crc_mode: false,
        },
        ChecksumEngineInfo {
            id: "bosch_mse8".to_string(),
            name: "Bosch MSE8.0".to_string(),
            status: "confirmed".to_string(),
            description: "256 KiB block CRC-32 ^ 0xA18AFC77 with 1234567812341008 footer".to_string(),
            supports_keep_crc_mode: false,
        },
        ChecksumEngineInfo {
            id: "bosch_mg1".to_string(),
            name: "Bosch MG1 (MG1CA094)".to_string(),
            status: "confirmed".to_string(),
            description: "CRC-32 (seed 0x533F71C8) over DEADBEEF blocks with update & GF(2) keep-CRC modes".to_string(),
            supports_keep_crc_mode: true,
        },
        ChecksumEngineInfo {
            id: "dellorto_ecm".to_string(),
            name: "Dell'Orto ECM".to_string(),
            status: "confirmed".to_string(),
            description: "Bajaj/Dell'Orto 0xC0000 image ~byte_sum of 0x80000..0x921FD at 0x921FE".to_string(),
            supports_keep_crc_mode: false,
        },
        ChecksumEngineInfo {
            id: "italjet_suzuki".to_string(),
            name: "Italjet / Suzuki-type ECU".to_string(),
            status: "confirmed".to_string(),
            description: "0x80000 image 16-bit LE byte sum 0x20000..end with complement next to AA55CC33".to_string(),
            supports_keep_crc_mode: false,
        },
        ChecksumEngineInfo {
            id: "qh250".to_string(),
            name: "QH250 (E2400)".to_string(),
            status: "confirmed".to_string(),
            description: "0x140000 image 16-bit BE word sum of 0..0x7FEF = 0 stored at 0x7FEE".to_string(),
            supports_keep_crc_mode: false,
        },
    ]
}

/// Auto-detects matching checksum layouts in `rom` and reports per-block verification status.
pub fn auto_detect_checksums(rom: &[u8]) -> Vec<DetectedChecksumMatch> {
    let mut out = Vec::new();
    for verifier in [
        verify_honda_keihin_moto,
        verify_yamaha_mt,
        verify_bosch_mse6,
        verify_bosch_mse8,
        verify_bosch_mg1,
        verify_dellorto_ecm,
        verify_italjet_suzuki,
        verify_qh250,
    ] {
        if let Some(m) = verifier(rom) {
            out.push(m);
        }
    }
    out
}

/// Runs verification for a single named checksum engine.
pub fn verify_checksum_by_id(rom: &[u8], algorithm_id: &str) -> Result<DetectedChecksumMatch, String> {
    let res = match algorithm_id {
        "honda_keihin_moto" => verify_honda_keihin_moto(rom),
        "yamaha_mt" => verify_yamaha_mt(rom),
        "bosch_mse6" => verify_bosch_mse6(rom),
        "bosch_mse8" => verify_bosch_mse8(rom),
        "bosch_mg1" => verify_bosch_mg1(rom),
        "dellorto_ecm" => verify_dellorto_ecm(rom),
        "italjet_suzuki" => verify_italjet_suzuki(rom),
        "qh250" => verify_qh250(rom),
        other => return Err(format!("unknown checksum engine '{other}'")),
    };
    res.ok_or_else(|| format!("ROM does not match layout signature for '{algorithm_id}'"))
}

/// Repairs checksums for `algorithm_id` and returns `(repaired_bytes, verification_report)`.
pub fn repair_checksum_by_id(
    rom: &[u8],
    algorithm_id: &str,
    mode: ChecksumRepairMode,
) -> Result<(Vec<u8>, DetectedChecksumMatch), String> {
    if mode == ChecksumRepairMode::CompensateKeepCrc && algorithm_id != "bosch_mg1" {
        return Err(format!(
            "CompensateKeepCrc mode is only supported by 'bosch_mg1', got '{algorithm_id}'"
        ));
    }
    match algorithm_id {
        "honda_keihin_moto" => repair_honda_keihin_moto(rom),
        "yamaha_mt" => repair_yamaha_mt(rom),
        "bosch_mse6" => repair_bosch_mse6(rom),
        "bosch_mse8" => repair_bosch_mse8(rom),
        "bosch_mg1" => repair_bosch_mg1(rom, mode),
        "dellorto_ecm" => repair_dellorto_ecm(rom),
        "italjet_suzuki" => repair_italjet_suzuki(rom),
        "qh250" => repair_qh250(rom),
        other => Err(format!("unknown checksum engine '{other}'")),
    }
}
