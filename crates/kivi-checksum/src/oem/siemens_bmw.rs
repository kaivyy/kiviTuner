//! BMW Siemens (MS41, MS43, GS20 TCU, SMG2 TCU, BMS46 K-CAN packet) & Mini Siemens EMS2000.

use super::{find_subslice, u16_be, u16_le, u32_le, write_u16_be, write_u16_le, OemChecksumBlock, OemChecksumReport};
use crate::primitives::crc16_arc;

// ===========================================================================
// 1. BMW Siemens MS41 (256 KiB full / 24 KiB calibration partial)
// ===========================================================================

const MS41_FULL_SIZE: usize = 256 * 1024;
const MS41_PARTIAL_SIZE: usize = 24 * 1024;
const MS41_CAL_MAGIC: [u8; 4] = [0x4E, 0x00, 0xFF, 0xFF];

struct Ms41CalEntry {
    from: usize,
    store: usize,
    calc: u16,
}

fn ms41_trim_end(rom: &[u8], anchor: usize) -> usize {
    let mut i = anchor.min(rom.len().saturating_sub(1));
    while i > 0 && rom[i] == 0xFF {
        i -= 1;
    }
    i + 1
}

fn ms41_cal_walk(rom: &[u8], start: usize) -> (Vec<Ms41CalEntry>, bool) {
    let mut entries = Vec::new();
    if start + 0x10 > rom.len() {
        return (entries, false);
    }
    let init = u16_be(rom, start + 0x0E);
    let mut pos = start;
    for _ in 0..20 {
        if pos + 2 > rom.len() {
            break;
        }
        let ss = u16_le(rom, pos) as usize;
        if ss == 0xFFFF {
            return (entries, true);
        }
        let store = start + ss;
        if store + 2 > rom.len() || store < pos {
            break;
        }
        entries.push(Ms41CalEntry {
            from: pos,
            store,
            calc: crc16_arc(&rom[pos..store], init),
        });
        pos = store + 2;
    }
    (entries, false)
}

fn ms41_program_computed(rom: &[u8]) -> u16 {
    let init = u16_be(rom, 0x6066);
    let end1 = ms41_trim_end(rom, 0x7FFF).max(0x6100);
    let mut s = crc16_arc(&rom[0x6100..end1], init);
    let end2 = ms41_trim_end(rom, 0x3FFF);
    s = crc16_arc(&rom[0x0000..end2], s);
    let mut buf = vec![0u8; 0x20000];
    let banks = [
        (0x24000usize, 0x00000usize),
        (0x20000, 0x04000),
        (0x2C000, 0x08000),
        (0x28000, 0x0C000),
        (0x34000, 0x10000),
        (0x30000, 0x14000),
        (0x3C000, 0x18000),
        (0x38000, 0x1C000),
    ];
    for (src, dst) in banks {
        buf[dst..dst + 0x4000].copy_from_slice(&rom[src..src + 0x4000]);
    }
    let end3 = ms41_trim_end(&buf, 0x1FFFF);
    crc16_arc(&buf[..end3], s)
}

pub fn verify_ms41(rom: &[u8]) -> Option<OemChecksumReport> {
    if rom.len() != MS41_FULL_SIZE && rom.len() != MS41_PARTIAL_SIZE {
        return None;
    }
    let cal_start = find_subslice(rom, &MS41_CAL_MAGIC, 0)?;
    let (entries, terminated) = ms41_cal_walk(rom, cal_start);
    if !terminated || entries.len() < 4 {
        return None;
    }
    let mut blocks = Vec::new();
    let mut notes = Vec::new();
    if rom.len() == MS41_FULL_SIZE {
        let boot_calc = crc16_arc(&rom[0x4000..0x5C14], 0x4711);
        let boot_stored = u16_le(rom, 0x5C80);
        blocks.push(OemChecksumBlock {
            label: "Boot sector".to_string(),
            start: 0x4000,
            end: 0x5C13,
            store_offset: Some(0x5C80),
            stored: Some(boot_stored as u32),
            computed: Some(boot_calc as u32),
            width_bits: 16,
            correctable: true,
            ok: boot_stored == boot_calc,
            status: if boot_stored == boot_calc { "ok" } else { "stale" }.to_string(),
        });
        let prog_calc = ms41_program_computed(rom);
        let prog_stored = u16_le(rom, 0x6050);
        blocks.push(OemChecksumBlock {
            label: "Program (report-only)".to_string(),
            start: 0x0000,
            end: 0x3FFFF,
            store_offset: Some(0x6050),
            stored: Some(prog_stored as u32),
            computed: Some(prog_calc as u32),
            width_bits: 16,
            correctable: false,
            ok: prog_stored == prog_calc,
            status: if prog_stored == prog_calc { "ok" } else { "stale" }.to_string(),
        });
    } else {
        notes.push("24 KiB calibration partial: boot and program live outside partial read.".to_string());
    }
    for (idx, e) in entries.iter().enumerate() {
        let stored = u16_le(rom, e.store);
        blocks.push(OemChecksumBlock {
            label: format!("Calibration {idx}"),
            start: e.from,
            end: e.store.saturating_sub(1),
            store_offset: Some(e.store),
            stored: Some(stored as u32),
            computed: Some(e.calc as u32),
            width_bits: 16,
            correctable: true,
            ok: stored == e.calc,
            status: if stored == e.calc { "ok" } else { "stale" }.to_string(),
        });
    }
    let all_ok = blocks.iter().all(|b| b.ok);
    Some(OemChecksumReport {
        algorithm_id: "bmw_ms41".to_string(),
        algorithm_name: "BMW Siemens MS41 (CRC-16/ARC)".to_string(),
        family: "Siemens BMW".to_string(),
        variant: if rom.len() == MS41_FULL_SIZE { "256 KiB full" } else { "24 KiB partial" }.to_string(),
        blocks,
        all_ok,
        notes,
    })
}

pub fn repair_ms41(rom: &[u8]) -> Result<(Vec<u8>, OemChecksumReport), String> {
    if verify_ms41(rom).is_none() {
        return Err("ROM does not match BMW Siemens MS41 layout".to_string());
    }
    let mut out = rom.to_vec();
    if out.len() == MS41_FULL_SIZE {
        let boot_calc = crc16_arc(&out[0x4000..0x5C14], 0x4711);
        write_u16_le(&mut out, 0x5C80, boot_calc);
    }
    let cal_start = find_subslice(&out, &MS41_CAL_MAGIC, 0)
        .ok_or_else(|| "MS41 calibration table disappeared".to_string())?;
    let (entries, _) = ms41_cal_walk(&out, cal_start);
    for e in entries {
        let cur = crc16_arc(&out[e.from..e.store], u16_be(&out, cal_start + 0x0E));
        write_u16_le(&mut out, e.store, cur);
    }
    let report = verify_ms41(&out).ok_or_else(|| "MS41 post-repair verify failed".to_string())?;
    Ok((out, report))
}

// ===========================================================================
// 2. BMW Siemens MS43 (512 KiB)
// ===========================================================================

const MS43_LEN: usize = 0x80000;
const MS43_BOOT_SLOT: usize = 0x3C24;
const MS43_PROG_SLOT: usize = 0x6FDE0;
const MS43_CAL_SLOT: usize = 0x73FE0;
const MS43_PROG_BASE: usize = 0x80000;

fn ms43_read_blocks(rom: &[u8], slot: usize, mem_base: usize) -> Option<Vec<(usize, usize)>> {
    if slot + 4 > rom.len() {
        return None;
    }
    let count = u16_le(rom, slot + 2) as usize;
    if count == 0 || count > 16 || slot + 4 + count * 8 > rom.len() {
        return None;
    }
    let mut blocks = Vec::with_capacity(count);
    for i in 0..count {
        let off = slot + 4 + i * 8;
        let s = u32_le(rom, off) as usize;
        let e = u32_le(rom, off + 4) as usize;
        if s < mem_base || e < s {
            return None;
        }
        let s = s - mem_base;
        let e = e - mem_base;
        if e >= rom.len() {
            return None;
        }
        blocks.push((s, e));
    }
    Some(blocks)
}

fn crc16_arc_ranges(rom: &[u8], ranges: &[(usize, usize)], mut init: u16) -> u16 {
    for &(s, e) in ranges {
        init = crc16_arc(&rom[s..=e], init);
    }
    init
}

pub fn verify_ms43(rom: &[u8]) -> Option<OemChecksumReport> {
    if rom.len() != MS43_LEN {
        return None;
    }
    let boot_blocks = ms43_read_blocks(rom, MS43_BOOT_SLOT, 0)?;
    let prog_blocks = ms43_read_blocks(rom, MS43_PROG_SLOT, MS43_PROG_BASE)?;
    let cal_blocks = ms43_read_blocks(rom, MS43_CAL_SLOT, 0)?;

    let specs = [
        ("Boot", MS43_BOOT_SLOT, 0x3FE6usize, boot_blocks),
        ("Program", MS43_PROG_SLOT, 0x3C34usize, prog_blocks),
        ("Calibration", MS43_CAL_SLOT, 0x6FFBFusize, cal_blocks),
    ];

    let mut blocks = Vec::with_capacity(3);
    for (label, slot, init_off, ranges) in specs {
        let stored = u16_le(rom, slot);
        let computed = crc16_arc_ranges(rom, &ranges, u16_be(rom, init_off));
        let start = ranges.first().map(|r| r.0).unwrap_or(0);
        let end = ranges.last().map(|r| r.1).unwrap_or(0);
        blocks.push(OemChecksumBlock {
            label: label.to_string(),
            start,
            end,
            store_offset: Some(slot),
            stored: Some(stored as u32),
            computed: Some(computed as u32),
            width_bits: 16,
            correctable: true,
            ok: stored == computed,
            status: if stored == computed { "ok" } else { "stale" }.to_string(),
        });
    }
    let all_ok = blocks.iter().all(|b| b.ok);
    Some(OemChecksumReport {
        algorithm_id: "bmw_ms43".to_string(),
        algorithm_name: "BMW Siemens MS43 (CRC-16/ARC)".to_string(),
        family: "Siemens BMW".to_string(),
        variant: "512 KiB full".to_string(),
        blocks,
        all_ok,
        notes: vec![
            "Boot @0x3C24 (init BE16 @0x3FE6), Program @0x6FDE0 (init @0x3C34), Cal @0x73FE0 (init @0x6FFBF).".to_string(),
        ],
    })
}

pub fn repair_ms43(rom: &[u8]) -> Result<(Vec<u8>, OemChecksumReport), String> {
    if verify_ms43(rom).is_none() {
        return Err("ROM does not match BMW Siemens MS43 layout".to_string());
    }
    let mut out = rom.to_vec();
    let specs = [
        (MS43_BOOT_SLOT, 0x3FE6usize, 0usize),
        (MS43_PROG_SLOT, 0x3C34usize, MS43_PROG_BASE),
        (MS43_CAL_SLOT, 0x6FFBFusize, 0usize),
    ];
    for (slot, init_off, base) in specs {
        let ranges = ms43_read_blocks(&out, slot, base)
            .ok_or_else(|| format!("invalid MS43 descriptor at {slot:#X}"))?;
        let calc = crc16_arc_ranges(&out, &ranges, u16_be(&out, init_off));
        write_u16_le(&mut out, slot, calc);
    }
    let report = verify_ms43(&out).ok_or_else(|| "MS43 post-repair verify failed".to_string())?;
    Ok((out, report))
}

// ===========================================================================
// 3. BMW Siemens GS20 TCU (64 KiB data BE16 @0x0D, 256 KiB prog) & SMG2
// ===========================================================================

const GS20_DATA_SIZE: usize = 65_536;
const GS20_CODE_SIZE: usize = 262_144;
const SMG2_DATA_SIZE: usize = 32_768;

pub fn verify_gs20(rom: &[u8]) -> Option<OemChecksumReport> {
    if rom.len() == GS20_DATA_SIZE {
        // Verified byte-for-byte against GS20_90C0_64KB_corrected.bin:
        // stored as big-endian u16 at 0x0D (NOT little-endian).
        let ranges = [(0x2Ausize, 0xFFC7usize)];
        let stored = u16_be(rom, 0x0D);
        let computed = crc16_arc_ranges(rom, &ranges, 0x0000);
        return Some(OemChecksumReport {
            algorithm_id: "bmw_gs20".to_string(),
            algorithm_name: "BMW Siemens GS20 TCU (CRC-16/ARC)".to_string(),
            family: "Siemens BMW".to_string(),
            variant: "64 KiB calibration".to_string(),
            blocks: vec![OemChecksumBlock {
                label: "GS20 64K Calibration".to_string(),
                start: 0x2A,
                end: 0xFFC7,
                store_offset: Some(0x0D),
                stored: Some(stored as u32),
                computed: Some(computed as u32),
                width_bits: 16,
                correctable: true,
                ok: stored == computed,
                status: if stored == computed { "ok" } else { "stale" }.to_string(),
            }],
            all_ok: stored == computed,
            notes: vec!["Verified against real E46 330i/M54B30 GS20 64 KiB dump (BE16 @0x0D).".to_string()],
        });
    }
    if rom.len() == GS20_CODE_SIZE {
        let ranges = [(0x0000usize, 0x01FFusize), (0x0280, 0x3FDFF)];
        let store = 261_836usize;
        let stored = u16_be(rom, store);
        let computed = crc16_arc_ranges(rom, &ranges, 0x0000);
        return Some(OemChecksumReport {
            algorithm_id: "bmw_gs20".to_string(),
            algorithm_name: "BMW Siemens GS20 TCU (CRC-16/ARC)".to_string(),
            family: "Siemens BMW".to_string(),
            variant: "256 KiB program".to_string(),
            blocks: vec![OemChecksumBlock {
                label: "GS20 256K Program".to_string(),
                start: 0x0000,
                end: 0x3FDFF,
                store_offset: Some(store),
                stored: Some(stored as u32),
                computed: Some(computed as u32),
                width_bits: 16,
                correctable: true,
                ok: stored == computed,
                status: if stored == computed { "ok" } else { "stale" }.to_string(),
            }],
            all_ok: stored == computed,
            notes: vec![],
        });
    }
    None
}

/// Signature-gated GS20 detector for `auto_detect_oem_checksums`.
/// Keeps explicit `verify_gs20` unchanged while preventing false triggers on
/// non-BMW 64 KiB / 256 KiB ROMs (e.g., VW M3.8.3, Honda Keihin, Suzuki Denso).
pub fn auto_detect_gs20(rom: &[u8]) -> Option<OemChecksumReport> {
    if rom.len() == GS20_DATA_SIZE {
        let has_pad = rom[0x0F..0x28].iter().all(|&b| b == 0xFF);
        let has_marker = find_subslice(rom, b"GADC", 0).is_some()
            || find_subslice(rom, b"GMPT", 0).is_some()
            || find_subslice(rom, b"GS20", 0).is_some()
            || find_subslice(rom, b"SIEMENS", 0).is_some();
        if has_pad && has_marker {
            return verify_gs20(rom);
        }
        return None;
    }
    if rom.len() == GS20_CODE_SIZE {
        let has_marker = find_subslice(rom, b"GS20", 0).is_some()
            || find_subslice(rom, b"GMPT", 0).is_some()
            || (find_subslice(rom, b"SIEMENS", 0).is_some()
                && find_subslice(rom, b"GADC", 0).is_some());
        if has_marker {
            return verify_gs20(rom);
        }
    }
    None
}

pub fn repair_gs20(rom: &[u8]) -> Result<(Vec<u8>, OemChecksumReport), String> {
    let mut out = rom.to_vec();
    if rom.len() == GS20_DATA_SIZE {
        let c = crc16_arc_ranges(&out, &[(0x2A, 0xFFC7)], 0x0000);
        write_u16_be(&mut out, 0x0D, c);
    } else if rom.len() == GS20_CODE_SIZE {
        let c = crc16_arc_ranges(&out, &[(0x0000, 0x01FF), (0x0280, 0x3FDFF)], 0x0000);
        write_u16_be(&mut out, 261_836, c);
    } else {
        return Err("ROM size must be 64 KiB or 256 KiB for GS20".to_string());
    }
    let report = verify_gs20(&out).ok_or_else(|| "GS20 post-repair verify failed".to_string())?;
    Ok((out, report))
}

pub fn verify_smg2(rom: &[u8]) -> Option<OemChecksumReport> {
    if rom.len() != SMG2_DATA_SIZE {
        return None;
    }
    let stored = u16_le(rom, 8320);
    let computed = crc16_arc_ranges(rom, &[(8416, 30911)], 0x7878);
    Some(OemChecksumReport {
        algorithm_id: "bmw_smg2".to_string(),
        algorithm_name: "BMW Siemens SMG II TCU (CRC-16/ARC)".to_string(),
        family: "Siemens BMW".to_string(),
        variant: "32 KiB data".to_string(),
        blocks: vec![OemChecksumBlock {
            label: "SMG2 32K Data".to_string(),
            start: 8416,
            end: 30911,
            store_offset: Some(8320),
            stored: Some(stored as u32),
            computed: Some(computed as u32),
            width_bits: 16,
            correctable: true,
            ok: stored == computed,
            status: if stored == computed { "ok" } else { "stale" }.to_string(),
        }],
        all_ok: stored == computed,
        notes: vec![],
    })
}

/// Signature-gated SMG II detector for `auto_detect_oem_checksums`.
pub fn auto_detect_smg2(rom: &[u8]) -> Option<OemChecksumReport> {
    if rom.len() != SMG2_DATA_SIZE {
        return None;
    }
    let has_marker = find_subslice(rom, b"SMG", 0).is_some()
        || find_subslice(rom, b"SIEMENS", 0).is_some()
        || find_subslice(rom, b"GS30", 0).is_some()
        || find_subslice(rom, b"MSS54", 0).is_some();
    if has_marker {
        verify_smg2(rom)
    } else {
        None
    }
}

pub fn repair_smg2(rom: &[u8]) -> Result<(Vec<u8>, OemChecksumReport), String> {
    if rom.len() != SMG2_DATA_SIZE {
        return Err("ROM size must be 32 KiB for SMG2".to_string());
    }
    let mut out = rom.to_vec();
    let calc = crc16_arc_ranges(&out, &[(8416, 30911)], 0x7878);
    write_u16_le(&mut out, 8320, calc);
    let report = verify_smg2(&out).ok_or_else(|| "SMG2 verify failed".to_string())?;
    Ok((out, report))
}

// ===========================================================================
// 4. Mini Siemens EMS2000 (512 KiB, 3x CRC-16/ARC)
// ===========================================================================

const EMS2K_SIZE: usize = 0x80000;
const EMS2K_CRC_TABLE: usize = 0x11232;

fn ems2k_far(rom: &[u8], i: usize) -> Option<usize> {
    if i + 4 > rom.len() {
        return None;
    }
    let a = (u16_le(rom, i + 2) as usize) * 0x10000 + (u16_le(rom, i) as usize);
    Some(if a >= 0x80000 { a - 0x80000 } else { a })
}

type Ems2kRegion = (&'static str, usize, u16, Vec<(usize, usize)>, bool);

fn ems2k_regions(rom: &[u8]) -> Option<Vec<Ems2kRegion>> {
    if rom.len() != EMS2K_SIZE {
        return None;
    }
    for i in 0u16..256 {
        let expected = crc16_arc(&[i as u8], 0);
        if u16_le(rom, EMS2K_CRC_TABLE + 2 * i as usize) != expected {
            return None;
        }
    }
    let read_list = |base: usize, count: usize| -> Option<Vec<(usize, usize)>> {
        if count == 0 || count > 32 || base + 8 * count > rom.len() {
            return None;
        }
        let mut out = Vec::with_capacity(count);
        for k in 0..count {
            let s = ems2k_far(rom, base + 8 * k)?;
            let e = ems2k_far(rom, base + 4 + 8 * k)?;
            if s > e || e >= rom.len() {
                return None;
            }
            out.push((s, e));
        }
        Some(out)
    };
    let boot_s = ems2k_far(rom, 0x3ED8)?;
    let boot_e = ems2k_far(rom, 0x3EDC)?;
    if boot_s > boot_e || boot_e >= rom.len() {
        return None;
    }
    let prog_seed_at = ems2k_far(rom, 0x10B9C)?;
    let cal_seed_at = ems2k_far(rom, 0x10BBC)?;
    if prog_seed_at + 2 > rom.len() || cal_seed_at + 2 > rom.len() {
        return None;
    }
    Some(vec![
        ("boot", 0x3ED4, u16_le(rom, 0x3FE6), vec![(boot_s, boot_e)], false),
        (
            "prog",
            0x10010,
            u16_le(rom, prog_seed_at),
            read_list(0x10014, u16_le(rom, 0x10012) as usize)?,
            false,
        ),
        (
            "cal",
            0x77FE0,
            u16_le(rom, cal_seed_at),
            read_list(0x77FE4, rom[0x77FE2] as usize)?,
            true,
        ),
    ])
}

pub fn verify_ems2k(rom: &[u8]) -> Option<OemChecksumReport> {
    let regs = ems2k_regions(rom)?;
    let mut blocks = Vec::with_capacity(3);
    for (name, at, seed, ranges, correctable) in regs {
        let stored = u16_le(rom, at);
        let computed = crc16_arc_ranges(rom, &ranges, seed);
        let start = ranges.first().map(|r| r.0).unwrap_or(0);
        let end = ranges.last().map(|r| r.1).unwrap_or(0);
        blocks.push(OemChecksumBlock {
            label: format!("EMS2000 {name}"),
            start,
            end,
            store_offset: Some(at),
            stored: Some(stored as u32),
            computed: Some(computed as u32),
            width_bits: 16,
            correctable,
            ok: stored == computed,
            status: if stored == computed { "ok" } else { "stale" }.to_string(),
        });
    }
    let all_ok = blocks.iter().all(|b| b.ok);
    Some(OemChecksumReport {
        algorithm_id: "mini_ems2k".to_string(),
        algorithm_name: "Mini Siemens EMS2000 (3x CRC-16/ARC)".to_string(),
        family: "Siemens BMW".to_string(),
        variant: "512 KiB".to_string(),
        blocks,
        all_ok,
        notes: vec!["Repair updates calibration checksum @0x77FE0 only (safety policy).".to_string()],
    })
}

pub fn repair_ems2k(rom: &[u8]) -> Result<(Vec<u8>, OemChecksumReport), String> {
    let regs = ems2k_regions(rom).ok_or_else(|| "ROM does not match Mini EMS2000 layout".to_string())?;
    let mut out = rom.to_vec();
    for (name, at, seed, ranges, correctable) in regs {
        if name == "cal" && correctable {
            let calc = crc16_arc_ranges(&out, &ranges, seed);
            write_u16_le(&mut out, at, calc);
        }
    }
    let report = verify_ems2k(&out).ok_or_else(|| "EMS2000 verify failed".to_string())?;
    Ok((out, report))
}

/// BMW MS45/BMS46 K-CAN frame XOR helper checksum (8-bit XOR across header+payload).
pub fn bms46_packet_xor_checksum(frame_without_cs: &[u8]) -> u8 {
    frame_without_cs.iter().fold(0u8, |acc, &b| acc ^ b)
}
