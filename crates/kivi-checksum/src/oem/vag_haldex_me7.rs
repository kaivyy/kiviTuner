//! VAG Haldex 4Motion (VW_Flash) and IronFelix Bosch/Sagem verification engines
//! (ME7.XX CRC32+sum8, Bosch M3.x-5.x, M7.9.7, M7.9.8, China M7.9.7, ME7.4.5, Sagem Samand).

use super::{
    find_all, find_subslice, u16_be, u16_le, u32_le, write_u16_be, write_u16_le, write_u32_le,
    OemChecksumBlock, OemChecksumReport,
};
use crate::primitives::compute_crc32_with_initial;

// ===========================================================================
// 1. VAG Haldex 4Motion CAL/ASW 16-bit LE One's-Complement Word Sum
// ===========================================================================

fn haldex_location(block_num: u8) -> Option<usize> {
    match block_num {
        2 => Some(0x10),  // CAL
        3 => Some(0x200), // ASW
        _ => None,
    }
}

/// Verifies one Haldex 4Motion block (`block_num = 2` for CAL, `3` for ASW).
pub fn verify_haldex_block(rom: &[u8], block_num: u8) -> Result<OemChecksumReport, String> {
    let cs_loc = haldex_location(block_num).ok_or_else(|| "Haldex block must be 2 (CAL) or 3 (ASW)".to_string())?;
    if rom.len() < cs_loc + 0x0A || !(rom.len() - 0x0A).is_multiple_of(2) {
        return Err(format!("Invalid Haldex block length {} for block {block_num}", rom.len()));
    }
    let stored = u16_le(rom, cs_loc + 0x08);
    let mut sum: u16 = 0;
    let mut filtered = Vec::with_capacity(rom.len() - 0x0A);
    filtered.extend_from_slice(&rom[..cs_loc]);
    filtered.extend_from_slice(&rom[cs_loc + 0x0A..]);
    let (chunks, _) = filtered.as_chunks::<2>();
    for &chunk in chunks {
        sum = sum.wrapping_add(u16::from_le_bytes(chunk));
    }
    let computed = 0xFFFFu16.wrapping_sub(sum);
    let ok = stored == computed;
    Ok(OemChecksumReport {
        algorithm_id: "vag_haldex".to_string(),
        algorithm_name: "VAG Haldex 4Motion Block Checksum".to_string(),
        family: "VAG Haldex".to_string(),
        variant: if block_num == 2 { "CAL (block 2)".to_string() } else { "ASW (block 3)".to_string() },
        blocks: vec![OemChecksumBlock {
            label: format!("Haldex Block {block_num}"),
            start: 0,
            end: rom.len() - 1,
            store_offset: Some(cs_loc + 0x08),
            stored: Some(stored as u32),
            computed: Some(computed as u32),
            width_bits: 16,
            correctable: true,
            ok,
            status: if ok { "ok" } else { "stale" }.to_string(),
        }],
        all_ok: ok,
        notes: vec![],
    })
}

pub fn repair_haldex_block(rom: &[u8], block_num: u8) -> Result<(Vec<u8>, OemChecksumReport), String> {
    let rep = verify_haldex_block(rom, block_num)?;
    let mut out = rom.to_vec();
    if let (Some(off), Some(val)) = (rep.blocks[0].store_offset, rep.blocks[0].computed) {
        write_u16_le(&mut out, off, val as u16);
    }
    let after = verify_haldex_block(&out, block_num)?;
    Ok((out, after))
}

// ===========================================================================
// 2. IronFelix Profiles (M3.x-5.x, M7.9.7, China M7.9.7, M7.9.8, ME7.4.5, Sagem, ME7.XX CRC32)
// ===========================================================================

fn sum8(d: &[u8], s: usize, e_incl: usize) -> u32 {
    d[s..=e_incl].iter().fold(0u32, |acc, &b| acc.wrapping_add(b as u32))
}

fn sum16le_u32(d: &[u8], s: usize, e_incl: usize) -> u32 {
    let slice = &d[s..=e_incl];
    let mut acc = 0u32;
    let (chunks, rem) = slice.as_chunks::<2>();
    for &ch in chunks {
        acc = acc.wrapping_add(u16::from_le_bytes(ch) as u32);
    }
    if let Some(&last) = rem.first() {
        acc = acc.wrapping_add(last as u32);
    }
    acc
}

fn sumb_pages(d: &[u8], s: usize, e_incl: usize) -> u32 {
    let mut total = 0u32;
    let mut pos = s;
    while pos < e_incl {
        let p_end = (pos + 0x1FFF).min(e_incl);
        total = total.wrapping_add(u16_be(d, pos) as u32);
        if p_end > pos {
            total = total.wrapping_add(u16_be(d, p_end - 1) as u32);
        }
        pos += 0x2000;
    }
    total
}

fn verify_pair_inv(rom: &[u8], store: usize) -> Option<(u32, u32)> {
    if store + 8 > rom.len() {
        return None;
    }
    let stored = u32_le(rom, store);
    let inv = u32_le(rom, store + 4);
    if stored != !inv {
        None
    } else {
        Some((stored, inv))
    }
}

fn locate_bosch_me3x_layout(rom: &[u8]) -> Option<(usize, usize, usize, usize)> {
    if rom.len() != 0x20000 && rom.len() != 0x40000 {
        return None;
    }
    let (st, end, sum_store) = [(0xBF00usize, 0xBEFFusize), (0xDF00, 0xDEFF)]
        .into_iter()
        .find_map(|(st, en)| {
            if st + 6 <= rom.len()
                && u16_be(rom, st) == u16_be(rom, st + 2).wrapping_add(u16_be(rom, st + 4))
            {
                Some((st, en, st + 4))
            } else {
                None
            }
        })?;
    if end >= rom.len() {
        return None;
    }
    let start = (0x4000usize..end).step_by(0x80).find(|&a| u16_le(rom, a) as usize == a)?;
    Some((st, start, end, sum_store))
}

pub fn verify_bosch_me3x(rom: &[u8]) -> Option<OemChecksumReport> {
    let (_st, start, end, sum_store) = locate_bosch_me3x_layout(rom)?;
    let calc = (sum8(rom, start, end) & 0xFFFF) as u16;
    let stored = u16_be(rom, sum_store);
    let ok = stored == calc;
    Some(OemChecksumReport {
        algorithm_id: "bosch_me3x".to_string(),
        algorithm_name: "Bosch M3.x / M5.x (Sum8)".to_string(),
        family: "Bosch Legacy".to_string(),
        variant: format!("{} KiB", rom.len() / 1024),
        blocks: vec![OemChecksumBlock {
            label: "Primary Sum8".to_string(),
            start,
            end,
            store_offset: Some(sum_store),
            stored: Some(stored as u32),
            computed: Some(calc as u32),
            width_bits: 16,
            correctable: true,
            ok,
            status: if ok { "ok" } else { "stale" }.to_string(),
        }],
        all_ok: ok,
        notes: vec![],
    })
}

pub fn repair_bosch_me3x(rom: &[u8]) -> Result<(Vec<u8>, OemChecksumReport), String> {
    let (st, start, end, sum_store) = locate_bosch_me3x_layout(rom)
        .ok_or_else(|| "ROM does not match Bosch M3.x/M5.x layout".to_string())?;
    let mut out = rom.to_vec();
    let calc = (sum8(&out, start, end) & 0xFFFF) as u16;
    write_u16_be(&mut out, sum_store, calc);
    let header_sum = u16_be(&out, st + 2).wrapping_add(calc);
    write_u16_be(&mut out, st, header_sum);
    let rep = verify_bosch_me3x(&out)
        .ok_or_else(|| "Failed to verify repaired Bosch M3.x/M5.x ROM".to_string())?;
    Ok((out, rep))
}

fn hyundai_m797_total(rom: &[u8]) -> Option<u32> {
    if rom.len() != 0x80000 {
        return None;
    }
    const SIG1: [u8; 16] = [0, 0, 0x80, 0, 0xFD, 0x5E, 0x80, 0, 0, 0x80, 0x80, 0, 0xFF, 0xFB, 0x80, 0];
    const SIG2: [u8; 16] = [0, 0, 0x80, 0, 0xFF, 0x5F, 0x80, 0, 0, 0x80, 0x80, 0, 0xFF, 0xFB, 0x80, 0];
    let off = find_subslice(rom, &SIG1, 0).or_else(|| find_subslice(rom, &SIG2, 0))?;
    if off.checked_add(40).is_none_or(|end| end > rom.len()) {
        return None;
    }
    let mut total = 0u32;
    for i in 0..5 {
        let s = (u32_le(rom, off + 8 * i) as usize).checked_sub(0x800000)?;
        let e = (u32_le(rom, off + 8 * i + 4) as usize).checked_sub(0x800000)?;
        if e >= rom.len() || s >= e {
            return None;
        }
        total = total.wrapping_add(sum16le_u32(rom, s, e));
    }
    Some(total)
}

pub fn verify_hyundai_m797(rom: &[u8]) -> Option<OemChecksumReport> {
    let total = hyundai_m797_total(rom)?;
    let (stored, _) = verify_pair_inv(rom, 0x7FFE0)?;
    let ok = stored == total;
    Some(OemChecksumReport {
        algorithm_id: "bosch_m797".to_string(),
        algorithm_name: "Hyundai Bosch M7.9.7".to_string(),
        family: "Bosch M7.9".to_string(),
        variant: "512 KiB".to_string(),
        blocks: vec![OemChecksumBlock {
            label: "Main Tail".to_string(),
            start: 0,
            end: 0x7FFFF,
            store_offset: Some(0x7FFE0),
            stored: Some(stored),
            computed: Some(total),
            width_bits: 32,
            correctable: true,
            ok,
            status: if ok { "ok" } else { "stale" }.to_string(),
        }],
        all_ok: ok,
        notes: vec![],
    })
}

pub fn repair_hyundai_m797(rom: &[u8]) -> Result<(Vec<u8>, OemChecksumReport), String> {
    let mut out = rom.to_vec();
    if out.len() != 0x80000 {
        return Err("Hyundai M7.9.7 requires a 512 KiB ROM".to_string());
    }
    // Writing any (w, !w) pair keeps 16-bit LE pair sum constant (0x1FFFE) even if inside a zone.
    write_u32_le(&mut out, 0x7FFE0, 0);
    write_u32_le(&mut out, 0x7FFE4, !0u32);
    let total = hyundai_m797_total(&out)
        .ok_or_else(|| "ROM does not match Hyundai M7.9.7 signature/zones".to_string())?;
    write_u32_le(&mut out, 0x7FFE0, total);
    write_u32_le(&mut out, 0x7FFE4, !total);
    let rep = verify_hyundai_m797(&out)
        .ok_or_else(|| "Failed to verify repaired Hyundai M7.9.7 ROM".to_string())?;
    Ok((out, rep))
}

fn china_m797_total(rom: &[u8]) -> Option<u32> {
    if rom.len() != 0x10_0000 {
        return None;
    }
    const SIG: [u8; 16] = [0, 0, 0x80, 0, 0xFF, 0xFF, 0x80, 0, 0, 0, 0x81, 0, 0xFF, 0xFF, 0x8F, 0];
    find_subslice(rom, &SIG, 0)?;
    Some(sum16le_u32(rom, 0, 0xFFFF).wrapping_add(sum16le_u32(rom, 0x10000, 0xFFFFF)))
}

pub fn verify_china_m797(rom: &[u8]) -> Option<OemChecksumReport> {
    let total = china_m797_total(rom)?;
    let (stored, _) = verify_pair_inv(rom, 0xFFFE8)?;
    let ok = stored == total;
    Some(OemChecksumReport {
        algorithm_id: "bosch_china_m797".to_string(),
        algorithm_name: "China Bosch M7.9.7 (1 MiB)".to_string(),
        family: "Bosch M7.9".to_string(),
        variant: "1 MiB".to_string(),
        blocks: vec![OemChecksumBlock {
            label: "Main Tail".to_string(),
            start: 0,
            end: 0xFFFFF,
            store_offset: Some(0xFFFE8),
            stored: Some(stored),
            computed: Some(total),
            width_bits: 32,
            correctable: true,
            ok,
            status: if ok { "ok" } else { "stale" }.to_string(),
        }],
        all_ok: ok,
        notes: vec![],
    })
}

pub fn repair_china_m797(rom: &[u8]) -> Result<(Vec<u8>, OemChecksumReport), String> {
    let mut out = rom.to_vec();
    if out.len() != 0x10_0000 {
        return Err("China M7.9.7 requires a 1 MiB ROM".to_string());
    }
    // 0x10000..=0xFFFFF spans the store at 0xFFFE8..0xFFFEE; (0, !0) fixes the pair sum to 0x1FFFE.
    write_u32_le(&mut out, 0xFFFE8, 0);
    write_u32_le(&mut out, 0xFFFEC, !0u32);
    let total = china_m797_total(&out)
        .ok_or_else(|| "ROM does not match China M7.9.7 signature".to_string())?;
    write_u32_le(&mut out, 0xFFFE8, total);
    write_u32_le(&mut out, 0xFFFEC, !total);
    let rep = verify_china_m797(&out)
        .ok_or_else(|| "Failed to verify repaired China M7.9.7 ROM".to_string())?;
    Ok((out, rep))
}

struct Me745Layout {
    shifted: bool,
    zones: [(usize, usize); 3],
    page_store: usize,
    main_zones: [(usize, usize); 3],
    main_store: usize,
    boot_r: (usize, usize),
    boot_st: usize,
    tail_r: (usize, usize),
    tail_st: usize,
}

fn citroen_me745_layout(rom: &[u8]) -> Option<Me745Layout> {
    if rom.len() != 0xD0000 {
        return None;
    }
    const SIG1: [u8; 16] = [0x88, 0xE0, 0x88, 0xD0, 0xE6, 0xFC, 0x00, 0x20, 0xE0, 0x9D, 0xE6, 0xFE, 0xFF, 0xFF, 0xE0, 0xAF];
    const SIG2: [u8; 16] = [0x88, 0x50, 0x88, 0x40, 0xE6, 0xFC, 0x00, 0x20, 0xE0, 0x9D, 0xE6, 0xFE, 0xFF, 0xFF, 0xE0, 0xAF];
    const CS1: [u8; 8] = [0, 0, 0, 0, 0xFF, 0x3F, 0, 0];
    if find_subslice(rom, &SIG1, 0).is_none() && find_subslice(rom, &SIG2, 0).is_none() {
        return None;
    }
    let shifted = find_subslice(rom, &CS1, 0).is_some_and(|p| p < 0xC0000);
    if shifted {
        Some(Me745Layout {
            shifted: true,
            zones: [(0x02000usize, 0x07FFFusize), (0x10000, 0x7FFFF), (0x82000, 0x9FFFF)],
            page_store: 0xA7FFAusize,
            main_zones: [(0x00000usize, 0x07FFFusize), (0x10000, 0x7FFFF), (0x82000, 0x9FFFF)],
            main_store: 0xBFFD8usize,
            boot_r: (0x8000usize, 0xF9CFusize),
            boot_st: 0xFFFCusize,
            tail_r: (0xA0000usize, 0xBFFD3usize),
            tail_st: 0xBFFD4usize,
        })
    } else {
        Some(Me745Layout {
            shifted: false,
            zones: [(0x02000, 0x07FFF), (0x20000, 0x8FFFF), (0x92000, 0xAFFFF)],
            page_store: 0xB7FFA,
            main_zones: [(0x00000, 0x07FFF), (0x20000, 0x8FFFF), (0x92000, 0xAFFFF)],
            main_store: 0xCFFD8,
            boot_r: (0x18000, 0x1F9CF),
            boot_st: 0x1FFFC,
            tail_r: (0xB0000, 0xCFFD3),
            tail_st: 0xCFFD4,
        })
    }
}

pub fn verify_citroen_me745(rom: &[u8]) -> Option<OemChecksumReport> {
    let layout = citroen_me745_layout(rom)?;
    let (
        shifted,
        zones,
        page_store,
        main_zones,
        main_store,
        boot_r,
        boot_st,
        tail_r,
        tail_st,
    ) = (
        layout.shifted,
        layout.zones,
        layout.page_store,
        layout.main_zones,
        layout.main_store,
        layout.boot_r,
        layout.boot_st,
        layout.tail_r,
        layout.tail_st,
    );
    let mut page_tot = 0u32;
    for (s, e) in zones {
        page_tot = page_tot.wrapping_add(sumb_pages(rom, s, e));
    }
    let mut crc = 0u32;
    for (s, e) in main_zones {
        crc = compute_crc32_with_initial(&rom[s..=e], crc);
    }
    let boot_crc = compute_crc32_with_initial(&rom[boot_r.0..=boot_r.1], 0);
    let tail_crc = compute_crc32_with_initial(&rom[tail_r.0..=tail_r.1], 0);

    let items = [
        ("ME7.4.5 Page Blocks", 0x02000, zones[2].1, page_store, page_tot),
        ("ME7.4.5 CRC-32 Main", 0, main_zones[2].1, main_store, crc),
        ("ME7.4.5 CRC-32 Boot", boot_r.0, boot_r.1, boot_st, boot_crc),
        ("ME7.4.5 CRC-32 Tail", tail_r.0, tail_r.1, tail_st, tail_crc),
    ];
    let blocks: Vec<OemChecksumBlock> = items
        .into_iter()
        .map(|(lbl, st, en, loc, calc)| {
            let stored = u32_le(rom, loc);
            OemChecksumBlock {
                label: lbl.to_string(),
                start: st,
                end: en,
                store_offset: Some(loc),
                stored: Some(stored),
                computed: Some(calc),
                width_bits: 32,
                correctable: true,
                ok: stored == calc,
                status: if stored == calc { "ok" } else { "stale" }.to_string(),
            }
        })
        .collect();
    let all_ok = blocks.iter().all(|b| b.ok);
    Some(OemChecksumReport {
        algorithm_id: "bosch_me745".to_string(),
        algorithm_name: "Citroen Bosch ME7.4.5 (832 KiB)".to_string(),
        family: "Bosch ME7".to_string(),
        variant: if shifted { "Shifted" } else { "Standard" }.to_string(),
        blocks,
        all_ok,
        notes: vec![],
    })
}

pub fn repair_citroen_me745(rom: &[u8]) -> Result<(Vec<u8>, OemChecksumReport), String> {
    let layout = citroen_me745_layout(rom)
        .ok_or_else(|| "ROM does not match Citroen ME7.4.5 layout".to_string())?;
    let (zones, page_store, main_zones, main_store, boot_r, boot_st, tail_r, tail_st) = (
        layout.zones,
        layout.page_store,
        layout.main_zones,
        layout.main_store,
        layout.boot_r,
        layout.boot_st,
        layout.tail_r,
        layout.tail_st,
    );
    let mut out = rom.to_vec();
    let mut page_tot = 0u32;
    for (s, e) in zones {
        page_tot = page_tot.wrapping_add(sumb_pages(&out, s, e));
    }
    let mut main_crc = 0u32;
    for (s, e) in main_zones {
        main_crc = compute_crc32_with_initial(&out[s..=e], main_crc);
    }
    let boot_crc = compute_crc32_with_initial(&out[boot_r.0..=boot_r.1], 0);
    // Write page_store inside tail_r BEFORE computing tail_crc so tail matches the updated page sum.
    write_u32_le(&mut out, page_store, page_tot);
    write_u32_le(&mut out, main_store, main_crc);
    write_u32_le(&mut out, boot_st, boot_crc);
    let tail_crc = compute_crc32_with_initial(&out[tail_r.0..=tail_r.1], 0);
    write_u32_le(&mut out, tail_st, tail_crc);
    let rep = verify_citroen_me745(&out)
        .ok_or_else(|| "Failed to verify repaired Citroen ME7.4.5 ROM".to_string())?;
    Ok((out, rep))
}

pub fn verify_sagem_samand(rom: &[u8]) -> Option<OemChecksumReport> {
    if rom.len() != 0xD0000 {
        return None;
    }
    const SIG: [u8; 16] = [0xDC, 0x06, 0xA8, 0x41, 0x08, 0x12, 0x18, 0x60, 0xC0, 0x85, 0x00, 0x35, 0xC0, 0x95, 0x00, 0x35];
    find_subslice(rom, &SIG, 0)?;
    let calc1 = ((sum8(rom, 0x2000, 0x3FFF) + sum8(rom, 0x10000, 0x6FFFD)) & 0xFFFF) as u16;
    let calc2 = (sum8(rom, 0x72806, 0x7FFFF) & 0xFFFF) as u16;
    let st1 = u16_le(rom, 0x6FFFE);
    let st2 = u16_le(rom, 0x72804);
    let blocks = vec![
        OemChecksumBlock {
            label: "Samand Sum8 Main".to_string(),
            start: 0x2000,
            end: 0x6FFFD,
            store_offset: Some(0x6FFFE),
            stored: Some(st1 as u32),
            computed: Some(calc1 as u32),
            width_bits: 16,
            correctable: true,
            ok: st1 == calc1,
            status: if st1 == calc1 { "ok" } else { "stale" }.to_string(),
        },
        OemChecksumBlock {
            label: "Samand Sum8 Tail".to_string(),
            start: 0x72806,
            end: 0x7FFFF,
            store_offset: Some(0x72804),
            stored: Some(st2 as u32),
            computed: Some(calc2 as u32),
            width_bits: 16,
            correctable: true,
            ok: st2 == calc2,
            status: if st2 == calc2 { "ok" } else { "stale" }.to_string(),
        },
    ];
    let all_ok = blocks.iter().all(|b| b.ok);
    Some(OemChecksumReport {
        algorithm_id: "sagem_samand".to_string(),
        algorithm_name: "Sagem Iran Khodro Samand (832 KiB)".to_string(),
        family: "Sagem".to_string(),
        variant: "832 KiB".to_string(),
        blocks,
        all_ok,
        notes: vec![],
    })
}

pub fn repair_sagem_samand(rom: &[u8]) -> Result<(Vec<u8>, OemChecksumReport), String> {
    let initial = verify_sagem_samand(rom)
        .ok_or_else(|| "ROM does not match Sagem Samand layout".to_string())?;
    let mut out = rom.to_vec();
    for b in &initial.blocks {
        if let (Some(loc), Some(val)) = (b.store_offset, b.computed) {
            write_u16_le(&mut out, loc, val as u16);
        }
    }
    let rep = verify_sagem_samand(&out)
        .ok_or_else(|| "Failed to verify repaired Sagem Samand ROM".to_string())?;
    Ok((out, rep))
}

fn vag_me7xx_zones(rom: &[u8]) -> Option<[(usize, usize, usize); 3]> {
    if rom.len() != 0x80000 && rom.len() != 0x10_0000 {
        return None;
    }
    const REC_SIG1: [u8; 13] = [0x00, 0x00, 0x80, 0x00, 0xFF, 0xFB, 0x80, 0x00, 0x00, 0x00, 0x82, 0x00, 0xFF];
    const EB_SIG1: [u8; 14] = [0x20, 0x54, 0xC4, 0x50, 0x02, 0x00, 0xF0, 0xC8, 0xF0, 0xD9, 0xD4, 0xE0, 0x02, 0x00];
    const CS_SIG6: [u8; 8] = [0xE6, 0xF4, 0xFF, 0xFF, 0xE6, 0xF5, 0xFF, 0xFF];
    const CS_SIG1: [u8; 14] = [0xF0, 0x54, 0x5C, 0x25, 0x20, 0x54, 0x5C, 0x25, 0x20, 0x54, 0x5C, 0x45, 0xF0, 0x85];

    if find_subslice(rom, &REC_SIG1, 0).is_none() || find_subslice(rom, &EB_SIG1, 0).is_none() {
        return None;
    }
    let cs6 = find_all(rom, &CS_SIG6);
    let cs1 = find_all(rom, &CS_SIG1);
    if cs6.len() < 3 || cs1.len() < 3 {
        return None;
    }
    let base = 0x800000usize;
    let mut zones = [(0usize, 0usize, 0usize); 3];
    for idx in 0..3 {
        if cs6[idx].checked_add(0x28).is_none_or(|end| end > rom.len())
            || cs1[idx].checked_add(0x3C).is_none_or(|end| end > rom.len())
        {
            return None;
        }
        let store = (u16_le(rom, cs6[idx] + 0x22) as usize + ((u16_le(rom, cs6[idx] + 0x26) as usize) << 16))
            .checked_sub(base)?;
        let (s, e) = if idx == 0 {
            let p = cs1[0];
            let hi = ((rom[p + 0x14] as usize) << 16) + ((rom[p + 0x15] as usize) << 24);
            (
                hi.checked_sub(base)?,
                (u16_le(rom, p + 0x38) as usize + hi).checked_sub(base + 1)?,
            )
        } else {
            let p = cs1[idx];
            let hi = ((rom[p + 0x16] as usize) << 16) + ((rom[p + 0x17] as usize) << 24);
            (
                (u16_le(rom, p + 0x12) as usize + hi).checked_sub(base)?,
                (u16_le(rom, p + 0x3A) as usize + hi).checked_sub(base + 1)?,
            )
        };
        if e >= rom.len() || store + 4 > rom.len() || s > e {
            return None;
        }
        zones[idx] = (s, e, store);
    }
    Some(zones)
}

pub fn verify_vag_me7xx_crc(rom: &[u8]) -> Option<OemChecksumReport> {
    let zones = vag_me7xx_zones(rom)?;
    let mut blocks = Vec::new();
    for (idx, &(s, e, store)) in zones.iter().enumerate() {
        let stored = u32_le(rom, store);
        let calc = compute_crc32_with_initial(&rom[s..=e], 0);
        blocks.push(OemChecksumBlock {
            label: format!("ME7.XX CRC32 Zone {}", idx + 1),
            start: s,
            end: e,
            store_offset: Some(store),
            stored: Some(stored),
            computed: Some(calc),
            width_bits: 32,
            correctable: true,
            ok: stored == calc,
            status: if stored == calc { "ok" } else { "stale" }.to_string(),
        });
    }
    let all_ok = blocks.iter().all(|b| b.ok);
    Some(OemChecksumReport {
        algorithm_id: "vag_me7xx_crc".to_string(),
        algorithm_name: "VAG Bosch ME7.XX CRC-32".to_string(),
        family: "Bosch ME7".to_string(),
        variant: "Subtype 1".to_string(),
        blocks,
        all_ok,
        notes: vec![],
    })
}

pub fn repair_vag_me7xx_crc(rom: &[u8]) -> Result<(Vec<u8>, OemChecksumReport), String> {
    let zones = vag_me7xx_zones(rom)
        .ok_or_else(|| "ROM does not match VAG ME7.XX CRC-32 layout".to_string())?;
    let mut out = rom.to_vec();
    for (s, e, store) in zones {
        let calc = compute_crc32_with_initial(&out[s..=e], 0);
        write_u32_le(&mut out, store, calc);
    }
    let rep = verify_vag_me7xx_crc(&out)
        .ok_or_else(|| "Failed to verify repaired VAG ME7.XX ROM".to_string())?;
    Ok((out, rep))
}
