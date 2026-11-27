//! GM Delco / Delphi PCM checksum & CVN engines (P01, P59, P10, P08, P11, P12, E54, E38, E39, E92).
//!
//! Validates and repairs standard GM Delco/Delphi segment checksum and CVN layouts.

use super::{find_subslice, u16_be, u32_be, write_u16_be, OemChecksumBlock, OemChecksumReport};
use crate::primitives::crc16_arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GmPcmFamily {
    P01,
    P59,
    P10,
    P08,
    P11,
    P12,
    BlackBox,
    E54,
    E38,
    E39,
    E92,
}

impl GmPcmFamily {
    pub fn label(self) -> &'static str {
        match self {
            Self::P01 => "GM P01 (512 KiB)",
            Self::P59 => "GM P59 (1 MiB)",
            Self::P10 => "GM P10 (512 KiB)",
            Self::P08 => "GM P08 (512 KiB)",
            Self::P11 => "GM P11",
            Self::P12 => "GM P12 (1 MiB)",
            Self::BlackBox => "GM Vortec BlackBox (512 KiB)",
            Self::E54 => "GM E54 (512 KiB)",
            Self::E38 => "GM E38 (2 MiB)",
            Self::E39 => "GM E39/E39a (3 MiB)",
            Self::E92 => "GM E92 (4 MiB)",
        }
    }
}

const P01_SEGMENTS: [&str; 8] = [
    "Operating System",
    "Engine Calibration",
    "Engine Diagnostics",
    "Transmission Calibration",
    "Transmission Diagnostics",
    "Fuel System",
    "System",
    "Speedometer",
];

const P10_SEGMENTS: [&str; 5] = [
    "Operating System",
    "Engine Calibration",
    "Engine Diagnostics",
    "System",
    "Speedometer",
];

fn validate_range_word_sum(
    rom: &[u8],
    family: GmPcmFamily,
    start: usize,
    mut end: usize,
    storage: usize,
    label: &str,
) -> Option<OemChecksumBlock> {
    if storage + 2 > rom.len() || start >= rom.len() || end >= rom.len() {
        return None;
    }
    let stored = u16_be(rom, storage);
    let mut sum = 0u16;
    let mut addr = start;
    while addr <= end {
        if addr == storage {
            addr += 2;
        }
        match family {
            GmPcmFamily::P01 | GmPcmFamily::P59 => {
                if addr == 0x4000 {
                    addr = 0x20000;
                }
            }
            GmPcmFamily::P08 => {
                if addr == 0x4000 {
                    addr = 0x8010;
                }
            }
            GmPcmFamily::P10 => match addr {
                0x4000 => addr = 0x20000,
                0x7FFFA => end = 0x7FFFA,
                _ => {}
            },
            GmPcmFamily::P11 => match addr {
                0x4000 => addr = 0x8002,
                0x18000 => addr = 0x20000,
                _ => {}
            },
            _ => {}
        }
        if addr + 1 >= rom.len() || addr > end {
            break;
        }
        sum = sum.wrapping_add(u16_be(rom, addr));
        addr += 2;
    }
    let expected = 0u16.wrapping_sub(sum);
    Some(OemChecksumBlock {
        label: label.to_string(),
        start,
        end,
        store_offset: Some(storage),
        stored: Some(stored as u32),
        computed: Some(expected as u32),
        width_bits: 16,
        correctable: true,
        ok: stored == expected,
        status: if stored == expected { "ok" } else { "stale" }.to_string(),
    })
}

pub fn detect_gm_family(rom: &[u8]) -> Option<GmPcmFamily> {
    match rom.len() {
        0x80000 => {
            if rom[0x1FFFE] == 0x4A && rom[0x1FFFF] == 0xFC && rom[0x7FFFE] == 0x4A && rom[0x7FFFF] == 0xFC {
                if rom[0x3FFC..=0x3FFF] == [0, 0, 0, 0] {
                    return Some(GmPcmFamily::E54);
                }
                if rom[0x20002] == 0 && rom[0x20003] == 1 && rom[0x2000A] == 1 && rom[0x2000B] == 0 {
                    return Some(GmPcmFamily::BlackBox);
                }
                return Some(GmPcmFamily::P01);
            }
            if rom[0x17FFE] == 0x55
                && rom[0x17FFF] == 0x55
                && rom[0x7FFFC..=0x7FFFF] == [0xA5, 0x5A, 0xA5, 0xA5]
                && rom[0x534] == 0
                && rom[0x535] == 0
            {
                return Some(GmPcmFamily::P10);
            }
            if rom[0x7FFFC..=0x7FFFF] == [0xA5, 0x5A, 0xA5, 0xA5] {
                return Some(GmPcmFamily::P08);
            }
            None
        }
        0x10_0000 => {
            if rom[0x1FFFE] == 0x4A && rom[0x1FFFF] == 0xFC && rom[0xFFFFE] == 0x4A && rom[0xFFFFF] == 0xFC {
                return Some(GmPcmFamily::P59);
            }
            if rom[0xFFFF8] == 0xAA && rom[0xFFFF9] == 0x55 {
                return Some(GmPcmFamily::P12);
            }
            None
        }
        0x20_0000 => collect_e38_segments(rom).ok().map(|_| GmPcmFamily::E38),
        0x30_0000 => collect_e39_segments(rom).ok().map(|_| GmPcmFamily::E39),
        0x40_0000 => collect_e92_sums(rom).ok().map(|_| GmPcmFamily::E92),
        _ => None,
    }
}

fn collect_e38_segments(rom: &[u8]) -> Result<Vec<(&'static str, usize, usize)>, ()> {
    if rom.len() != 0x20_0000 {
        return Err(());
    }
    const INDEX: usize = 0x10000;
    const LAYOUT: [(usize, usize, &str); 6] = [
        (0x24, 0x28, "Operating System"),
        (0x48, 0x4C, "Engine Operations"),
        (0x6B, 0x6F, "Engine Diagnostics"),
        (0x8E, 0x92, "Fuel System"),
        (0xB1, 0xB5, "System"),
        (0xD4, 0xD8, "Speedometer"),
    ];
    let mut out = Vec::with_capacity(6);
    for (so, eo, name) in LAYOUT {
        let s = u32_be(rom, INDEX + so) as usize;
        let e = u32_be(rom, INDEX + eo) as usize;
        if s >= rom.len() || e >= rom.len() || (s & 1) != 0 || (e & 1) == 0 || e <= s || e - s < 0x24 {
            return Err(());
        }
        out.push((name, s, e));
    }
    Ok(out)
}

fn gm_cvn(rom: &[u8], ranges: &[(usize, usize)]) -> u16 {
    let mut crc = 0u16;
    for &(s, e) in ranges {
        if s <= e && e < rom.len() {
            crc = crc16_arc(&rom[s..=e], crc);
        }
    }
    crc.swap_bytes()
}

fn verify_e38(rom: &[u8]) -> Option<OemChecksumReport> {
    let segs = collect_e38_segments(rom).ok()?;
    let mut blocks = Vec::with_capacity(12);
    let mut notes = Vec::new();
    for (name, s, e) in segs {
        if let Some(b) = validate_range_word_sum(rom, GmPcmFamily::E38, s + 2, e, s, name) {
            blocks.push(b);
        }
        let cvn_stored = u16_be(rom, s + 0x1E);
        let cvn_calc = gm_cvn(rom, &[(s + 2, s + 0x1D), (s + 0x20, e)]);
        blocks.push(OemChecksumBlock {
            label: format!("{name} CVN"),
            start: s + 2,
            end: e,
            store_offset: Some(s + 0x1E),
            stored: Some(cvn_stored as u32),
            computed: Some(cvn_calc as u32),
            width_bits: 16,
            correctable: true,
            ok: cvn_stored == cvn_calc,
            status: if cvn_stored == cvn_calc { "ok" } else { "cvn_mismatch" }.to_string(),
        });
    }
    if blocks.iter().any(|b| b.label.ends_with("CVN") && !b.ok) {
        notes.push("At least one E38 CVN differs from factory calibration.".to_string());
    }
    let all_ok = blocks.iter().all(|b| b.ok);
    Some(OemChecksumReport {
        algorithm_id: "gm_e38".to_string(),
        algorithm_name: GmPcmFamily::E38.label().to_string(),
        family: "GM Delco".to_string(),
        variant: "E38 Sum + CVN".to_string(),
        blocks,
        all_ok,
        notes,
    })
}

fn read_be_block_pairs(rom: &[u8], ptr: usize, count: usize) -> Result<Vec<(usize, usize)>, ()> {
    if ptr + count * 8 > rom.len() {
        return Err(());
    }
    let mut blocks = Vec::with_capacity(count);
    for i in 0..count {
        let s = u32_be(rom, ptr + i * 8) as usize;
        let e = u32_be(rom, ptr + i * 8 + 4) as usize;
        if s > e || e >= rom.len() {
            return Err(());
        }
        blocks.push((s, e));
    }
    Ok(blocks)
}

type SegmentRangeList = Vec<(String, usize, Vec<(usize, usize)>)>;

fn collect_e39_segments(rom: &[u8]) -> Result<(SegmentRangeList, SegmentRangeList), ()> {
    if rom.len() != 0x30_0000 {
        return Err(());
    }
    const CAL: [(&str, usize, usize, usize); 5] = [
        ("System", 0xCB05A, 0xCB072, 0xCB062),
        ("Fuel", 0xCB07E, 0xCB096, 0xCB086),
        ("Speedo", 0xCB0A2, 0xCB0BA, 0xCB0AA),
        ("EngineDiag", 0xCB0C6, 0xCB0DE, 0xCB0CE),
        ("Engine", 0xCB0EA, 0xCB102, 0xCB0F2),
    ];
    let mut sums = vec![("OS".to_string(), 0xCB000usize, read_be_block_pairs(rom, 0xCB046, 2)?)];
    let mut cvns = vec![("OS CVN".to_string(), 0xCB020usize, read_be_block_pairs(rom, 0xCB02E, 3)?)];
    for (name, addr_ptr, s_ptr, c_ptr) in CAL {
        let start = u32_be(rom, addr_ptr) as usize;
        if start + 0x22 > rom.len() {
            return Err(());
        }
        sums.push((name.to_string(), start, read_be_block_pairs(rom, s_ptr, 1)?));
        cvns.push((format!("{name} CVN"), start + 0x20, read_be_block_pairs(rom, c_ptr, 2)?));
    }
    Ok((sums, cvns))
}

fn collect_e92_sums(rom: &[u8]) -> Result<(SegmentRangeList, SegmentRangeList), ()> {
    if rom.len() != 0x40_0000 {
        return Err(());
    }
    type E92Spec = (&'static str, usize, usize, usize, usize, usize, usize, usize);
    const E92: [E92Spec; 6] = [
        ("OS", 0xC0126, 0x120, 0xC0136, 3, 0x100, 0xC014E, 2),
        ("System", 0xC0162, 0x020, 0xC016A, 2, 0x000, 0xC017A, 1),
        ("Fuel", 0xC0186, 0x020, 0xC018E, 2, 0x000, 0xC019E, 1),
        ("Speedo", 0xC01AA, 0x020, 0xC01B2, 2, 0x000, 0xC01C2, 1),
        ("EngineDiag", 0xC01CE, 0x020, 0xC01D6, 2, 0x000, 0xC01E6, 1),
        ("Engine", 0xC01F2, 0x020, 0xC01FA, 2, 0x000, 0xC020A, 1),
    ];
    let mut sums = Vec::new();
    let mut cvns = Vec::new();
    for (name, ap, cvn_o, c_ptr, c_cnt, sum_o, s_ptr, s_cnt) in E92 {
        let start = u32_be(rom, ap) as usize;
        if start + cvn_o + 2 > rom.len() {
            return Err(());
        }
        sums.push((name.to_string(), start + sum_o, read_be_block_pairs(rom, s_ptr, s_cnt)?));
        cvns.push((format!("{name} CVN"), start + cvn_o, read_be_block_pairs(rom, c_ptr, c_cnt)?));
    }
    Ok((sums, cvns))
}

fn eval_twos_word_blocks(rom: &[u8], ranges: &[(usize, usize)]) -> u16 {
    let mut total = 0u16;
    for &(s, e) in ranges {
        let mut a = s;
        while a < e && a + 1 < rom.len() {
            total = total.wrapping_add(u16_be(rom, a));
            a += 2;
        }
    }
    (!total).wrapping_add(1)
}

fn build_multi_block_report(
    id: &str,
    name: &str,
    variant: &str,
    rom: &[u8],
    sums: SegmentRangeList,
    cvns: SegmentRangeList,
) -> OemChecksumReport {
    let mut blocks = Vec::new();
    for (label, store, ranges) in sums {
        let stored = u16_be(rom, store);
        let calc = eval_twos_word_blocks(rom, &ranges);
        blocks.push(OemChecksumBlock {
            label,
            start: ranges.first().map(|r| r.0).unwrap_or(0),
            end: ranges.last().map(|r| r.1).unwrap_or(0),
            store_offset: Some(store),
            stored: Some(stored as u32),
            computed: Some(calc as u32),
            width_bits: 16,
            correctable: true,
            ok: stored == calc,
            status: if stored == calc { "ok" } else { "stale" }.to_string(),
        });
    }
    for (label, store, ranges) in cvns {
        let stored = u16_be(rom, store);
        let calc = gm_cvn(rom, &ranges);
        blocks.push(OemChecksumBlock {
            label,
            start: ranges.first().map(|r| r.0).unwrap_or(0),
            end: ranges.last().map(|r| r.1).unwrap_or(0),
            store_offset: Some(store),
            stored: Some(stored as u32),
            computed: Some(calc as u32),
            width_bits: 16,
            correctable: true,
            ok: stored == calc,
            status: if stored == calc { "ok" } else { "cvn_mismatch" }.to_string(),
        });
    }
    let all_ok = blocks.iter().all(|b| b.ok);
    OemChecksumReport {
        algorithm_id: id.to_string(),
        algorithm_name: name.to_string(),
        family: "GM Delco".to_string(),
        variant: variant.to_string(),
        blocks,
        all_ok,
        notes: vec![],
    }
}

pub fn verify_gm_pcm(rom: &[u8]) -> Option<OemChecksumReport> {
    let family = detect_gm_family(rom)?;
    let (id, blocks) = match family {
        GmPcmFamily::P01 | GmPcmFamily::P59 => {
            let mut b = Vec::new();
            for (seg, seg_name) in P01_SEGMENTS.iter().enumerate() {
                let s = u32_be(rom, 0x50C + seg * 8) as usize;
                let e = u32_be(rom, 0x50C + seg * 8 + 4) as usize;
                let cs_addr = if s == 0 { 0x500 } else { s };
                let r_start = if s != 0 { s + 2 } else { 0 };
                b.push(validate_range_word_sum(rom, family, r_start, e, cs_addr, seg_name)?);
            }
            (if family == GmPcmFamily::P01 { "gm_p01" } else { "gm_p59" }, b)
        }
        GmPcmFamily::P10 => {
            let mut b = Vec::new();
            for (seg, seg_name) in P10_SEGMENTS.iter().enumerate() {
                let s = u32_be(rom, 0x546 + seg * 8) as usize;
                let e = u32_be(rom, 0x546 + seg * 8 + 4) as usize;
                let cs_addr = if s == 0 { 0x52A } else { s };
                let r_start = if s != 0 { s + 2 } else { 0 };
                b.push(validate_range_word_sum(rom, family, r_start, e, cs_addr, seg_name)?);
            }
            ("gm_p10", b)
        }
        GmPcmFamily::P08 => {
            let mut acc = 0u16;
            let mut addr = 0usize;
            while addr <= 0x7FFFB {
                if addr == 0x4000 {
                    addr = 0x8010;
                }
                acc = acc.wrapping_add(rom[addr] as u16);
                addr += 1;
            }
            let stored = u16_be(rom, 0x8004);
            let b = vec![OemChecksumBlock {
                label: "Whole File ByteSum".to_string(),
                start: 0,
                end: 0x7FFFB,
                store_offset: Some(0x8004),
                stored: Some(stored as u32),
                computed: Some(acc as u32),
                width_bits: 16,
                correctable: true,
                ok: stored == acc,
                status: if stored == acc { "ok" } else { "stale" }.to_string(),
            }];
            ("gm_p08", b)
        }
        GmPcmFamily::E54 => {
            let specs = [
                (0x20002, 0x6FFFF, 0x20000, "Operating System"),
                (0x8002, 0x19FFF, 0x8000, "Engine Calibration"),
                (0x1A002, 0x1C7FF, 0x1A000, "Engine Diagnostics"),
                (0x1C002, 0x1DFFF, 0x1C000, "Fuel"),
                (0x1E002, 0x1EFFF, 0x1E000, "System"),
                (0x1F002, 0x1FFEF, 0x1F000, "Speedometer"),
            ];
            let mut b = Vec::new();
            for (s, e, st, name) in specs {
                b.push(validate_range_word_sum(rom, family, s, e, st, name)?);
            }
            ("gm_e54", b)
        }
        GmPcmFamily::E38 => return verify_e38(rom),
        GmPcmFamily::E39 => {
            let (sums, cvns) = collect_e39_segments(rom).ok()?;
            return Some(build_multi_block_report("gm_e39", GmPcmFamily::E39.label(), "E39 Sum + CVN", rom, sums, cvns));
        }
        GmPcmFamily::E92 => {
            let (sums, cvns) = collect_e92_sums(rom).ok()?;
            return Some(build_multi_block_report("gm_e92", GmPcmFamily::E92.label(), "E92 Sum + CVN", rom, sums, cvns));
        }
        GmPcmFamily::P12 => {
            let ranges = [
                (0x922, 0x900, 0x94A, 2usize, "Boot Block"),
                (0x8022, 0, 0x804A, 2, "Operating System"),
                (0x80C4, 0, 0x80E4, 2, "Engine Calibration"),
                (0x80F7, 0, 0x8117, 2, "Engine Diagnostics"),
                (0x812A, 0, 0x814A, 2, "Transmission Calibration"),
                (0x815D, 0, 0x817D, 2, "Transmission Diagnostics"),
                (0x805E, 0, 0x807E, 2, "Speedometer"),
                (0x8091, 0, 0x80B1, 2, "System"),
            ];
            let mut b = Vec::new();
            for (seg, off, idx, blk_cnt, name) in ranges {
                let sumaddr = (u32_be(rom, seg) as usize).saturating_add(off);
                if sumaddr + 2 > rom.len() {
                    return None;
                }
                let pairs = read_be_block_pairs(rom, idx, blk_cnt).ok()?;
                let stored = u16_be(rom, sumaddr);
                let mut acc = 0u16;
                for &(st, en) in &pairs {
                    let mut a = st;
                    while a <= en && a + 1 < rom.len() {
                        acc = acc.wrapping_add(u16_be(rom, a));
                        a += 2;
                    }
                }
                let calc = 0u16.wrapping_sub(acc);
                b.push(OemChecksumBlock {
                    label: name.to_string(),
                    start: pairs.first().map(|p| p.0).unwrap_or(0),
                    end: pairs.last().map(|p| p.1).unwrap_or(0),
                    store_offset: Some(sumaddr),
                    stored: Some(stored as u32),
                    computed: Some(calc as u32),
                    width_bits: 16,
                    correctable: true,
                    ok: stored == calc,
                    status: if stored == calc { "ok" } else { "stale" }.to_string(),
                });
            }
            ("gm_p12", b)
        }
        GmPcmFamily::BlackBox => {
            let mut b = Vec::new();
            for (seg, seg_name) in P01_SEGMENTS.iter().take(7).enumerate() {
                let s = u32_be(rom, 0x2000C + seg * 8) as usize;
                let e = u32_be(rom, 0x2000C + seg * 8 + 4) as usize;
                b.push(validate_range_word_sum(rom, family, s + 2, e, s, seg_name)?);
            }
            ("gm_blackbox", b)
        }
        GmPcmFamily::P11 => return None,
    };
    let _ = find_subslice;
    let all_ok = !blocks.is_empty() && blocks.iter().all(|b| b.ok);
    Some(OemChecksumReport {
        algorithm_id: id.to_string(),
        algorithm_name: family.label().to_string(),
        family: "GM Delco".to_string(),
        variant: "GM Delco/Delphi verified layout".to_string(),
        blocks,
        all_ok,
        notes: vec![],
    })
}

pub fn repair_gm_pcm(rom: &[u8]) -> Result<(Vec<u8>, OemChecksumReport), String> {
    let report = verify_gm_pcm(rom).ok_or_else(|| "ROM does not match a known GM Delco PCM signature".to_string())?;
    let mut out = rom.to_vec();
    // Write CVNs first because segment word sums cover the CVN slot (e.g. E38 `s + 0x1E`),
    // while CVNs exclude the segment checksum store slot.
    for b in &report.blocks {
        if b.label.ends_with("CVN") {
            if let (Some(off), Some(val)) = (b.store_offset, b.computed) {
                write_u16_be(&mut out, off, val as u16);
            }
        }
    }
    let mid = verify_gm_pcm(&out).ok_or_else(|| "GM PCM mid-repair check failed".to_string())?;
    for b in &mid.blocks {
        if !b.label.ends_with("CVN") {
            if let (Some(off), Some(val)) = (b.store_offset, b.computed) {
                write_u16_be(&mut out, off, val as u16);
            }
        }
    }
    let final_rep = verify_gm_pcm(&out).ok_or_else(|| "GM PCM post-repair check failed".to_string())?;
    Ok((out, final_rep))
}
