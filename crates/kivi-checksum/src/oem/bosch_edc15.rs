//! Bosch VAG EDC15P/V/VM/M/C (tdi41, tdi41v2, tdi41_2002) checksum engine.
//!
//! Ported from ZedSuite's `edc15p-checksum.ts`.

use super::{find_subslice, u32_le, write_u32_le, OemChecksumBlock, OemChecksumReport};

const MIN_EDC15_SIZE: usize = 0x80000;
const V41_SIG: [u8; 11] = [0x67, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x56, 0x34, 0x2E, 0x31];

fn is_empty_c3(buf: &[u8], start: usize, end: usize) -> bool {
    if end <= start + 4 || end > buf.len() {
        return true;
    }
    buf[start..end - 4].iter().all(|&b| b == 0xC3)
}

fn find_v41_sigs(buf: &[u8]) -> Vec<usize> {
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(idx) = find_subslice(buf, &V41_SIG, pos) {
        out.push(idx);
        pos = idx + 1;
    }
    out
}

fn tdi41_calc(buf: &[u8], mut addr: usize, end: usize, init_a: u16, init_b: u16) -> u32 {
    let mut seed_a = init_a;
    let mut seed_b = init_b;
    loop {
        if addr + 4 > buf.len() {
            break;
        }
        seed_a ^= u16::from_le_bytes([buf[addr], buf[addr + 1]]);
        addr += 2;

        let rot = (seed_b & 0xF) as u32;
        let carry: u16 = if rot > 0 {
            seed_a = seed_a.rotate_left(rot);
            seed_a & 1
        } else {
            0
        };
        let w2 = u16::from_le_bytes([buf[addr], buf[addr + 1]]);
        seed_b = seed_b.wrapping_sub(w2).wrapping_sub(carry);
        addr += 2;
        seed_b ^= seed_a;
        if addr >= end || addr + 4 > buf.len() {
            break;
        }

        let w3 = u16::from_le_bytes([buf[addr], buf[addr + 1]]);
        addr += 2;
        seed_a = seed_a.wrapping_sub(w3).wrapping_add(0xDAAC);
        let w4 = u16::from_le_bytes([buf[addr], buf[addr + 1]]);
        addr += 2;
        seed_b ^= w4;
        let rot2 = (seed_a & 0xF) as u32;
        if rot2 > 0 {
            seed_b = seed_b.rotate_right(rot2);
        }
        if addr >= end {
            break;
        }
    }
    seed_a = seed_a.wrapping_sub(0x8631).wrapping_add(0xDAAC);
    seed_b ^= 0xDF9B;
    ((seed_b as u32) << 16) | (seed_a as u32)
}

#[allow(clippy::too_many_arguments)]
fn tdi41_2002_calc(
    buf: &[u8],
    start: usize,
    end: usize,
    seed_a: u16,
    seed_b: u16,
    seed_c: u16,
    seed_d: u16,
    first_pass: bool,
) -> u32 {
    let mut count = start / 2;
    let end_count = end / 2;
    let mut addr = start;
    let mut var1 = 0u16;
    let mut var2 = 0u16;

    if count != end_count {
        var1 = seed_a;
        var2 = seed_b;
        if start == 0x8000 {
            var1 ^= 0xD565;
            var2 = var2.wrapping_add(0x308A);
        }
        loop {
            if addr + 4 > buf.len() {
                break;
            }
            var1 ^= u16::from_le_bytes([buf[addr], buf[addr + 1]]);
            addr += 2;
            count += 1;
            let shift = var2 & 0xF;
            let var4 = if shift > 0 {
                let top = var1 >> 15;
                var1 = var1.rotate_left(shift as u32);
                top
            } else {
                0
            };
            let w2 = u16::from_le_bytes([buf[addr], buf[addr + 1]]);
            var2 = var2.wrapping_sub(var4.wrapping_add(w2));
            var2 ^= var1;
            addr += 2;
            count += 1;
            if count > end_count || addr + 4 > buf.len() {
                break;
            }
            let var5 = u16::from_le_bytes([buf[addr], buf[addr + 1]]);
            addr += 4;
            var1 = var1.wrapping_add(0xFFFFu16.wrapping_sub(var5).wrapping_add(0xDAAD));
            var2 ^= u16::from_le_bytes([buf[addr - 2], buf[addr - 1]]);
            count += 2;
            let shift2 = var1 & 0xF;
            if shift2 > 0 {
                var2 = var2.rotate_right(shift2 as u32);
            }
            if count > end_count {
                break;
            }
        }
    }

    if start == 0 {
        var1 = var1.wrapping_sub(0x79CF);
        var2 = var2.wrapping_sub(0x1033);
    }

    if !first_pass {
        let var5 = if (seed_c & 0xF) > 0 {
            seed_d.rotate_left((seed_c & 0xF) as u32)
        } else {
            seed_d
        };
        var1 = var1.wrapping_sub(seed_c).wrapping_add(0xDAAD).wrapping_sub(1);
        (var1 as u32) | (((var5 ^ var2) as u32) << 16)
    } else {
        (var1 as u32) | ((var2 as u32) << 16)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Edc15Variant {
    Tdi41,
    Tdi41V2,
    Tdi41_2002,
}

pub fn detect_edc15_variant(rom: &[u8]) -> Option<Edc15Variant> {
    if rom.len() < MIN_EDC15_SIZE {
        return None;
    }
    let sigs = find_v41_sigs(rom);
    if sigs.is_empty() {
        return None;
    }
    if sigs.iter().any(|&p| p == 0x58001 || p == 0x64001) {
        return Some(Edc15Variant::Tdi41V2);
    }
    let mut store_addr = 0x4FFFBusize;
    while store_addr + 16 < rom.len() {
        if &rom[store_addr + 13..store_addr + 17] == b"V4.1" {
            return Some(Edc15Variant::Tdi41_2002);
        }
        store_addr += 0x10000;
    }
    let v1_empty = [0x4C000usize, 0x5C000, 0x6C000]
        .iter()
        .filter(|&&a| a + 0x10000 <= rom.len() && is_empty_c3(rom, a, a + 0x10000))
        .count();
    let v2_empty = [0x58000usize, 0x64000, 0x70000]
        .iter()
        .filter(|&&a| a + 0x10000 <= rom.len() && is_empty_c3(rom, a, a + 0x10000))
        .count();
    if v2_empty < v1_empty {
        Some(Edc15Variant::Tdi41V2)
    } else {
        Some(Edc15Variant::Tdi41)
    }
}

fn run_tdi41_array(buf: &mut [u8], addrs: &[usize], skip_c3: bool) -> Vec<OemChecksumBlock> {
    let mut seed_a = 0u16;
    let mut seed_b = 0u16;
    let mut first = true;
    let mut blocks = Vec::new();
    for (idx, win) in addrs.windows(2).enumerate() {
        let s = win[0];
        let e = win[1];
        if e > buf.len() || e <= s + 4 {
            continue;
        }
        if !first {
            seed_a |= 0x8631;
            seed_b |= 0xEFCD;
        }
        if skip_c3 && is_empty_c3(buf, s, e) {
            continue;
        }
        let stored = u32_le(buf, e - 4);
        let computed = tdi41_calc(buf, s, e - 4, seed_a, seed_b);
        let ok = stored == computed || stored == 0xC3C3_C3C3;
        blocks.push(OemChecksumBlock {
            label: format!("EDC15P Zone {}", idx + 1),
            start: s,
            end: e - 5,
            store_offset: Some(e - 4),
            stored: Some(stored),
            computed: Some(computed),
            width_bits: 32,
            correctable: stored != 0xC3C3_C3C3,
            ok,
            status: if ok { "ok" } else { "stale" }.to_string(),
        });
        first = false;
    }
    blocks
}

fn run_tdi41_2002(buf: &mut [u8]) -> Vec<OemChecksumBlock> {
    let s1 = tdi41_2002_calc(buf, 0x14000, 0x4BFFE, 0x8631, 0xEFCD, 0, 0, true);
    let (s1_msb, s1_lsb) = (((s1 >> 16) & 0xFFFF) as u16, (s1 & 0xFFFF) as u16);
    let s2 = tdi41_2002_calc(buf, 0, 0x7FFE, 0, 0, 0, 0, true);
    let (s2_msb, s2_lsb) = (((s2 >> 16) & 0xFFFF) as u16, (s2 & 0xFFFF) as u16);

    let mut blocks = Vec::new();
    let specs = [
        ("2002 Zone 1", 0x8000usize, 0xFFFBusize, 0xFFFCusize, s2_lsb, s2_msb, 0x4531u16, 0x3550u16),
        ("2002 Zone 2", 0x10000usize, 0x13FFBusize, 0x13FFCusize, 0, 0, 0x8631, 0xEFCD),
    ];
    for (name, start, end, store, a, b, c, d) in specs {
        let stored = u32_le(buf, store);
        let calc = tdi41_2002_calc(buf, start, end, a, b, c, d, false);
        blocks.push(OemChecksumBlock {
            label: name.to_string(),
            start,
            end,
            store_offset: Some(store),
            stored: Some(stored),
            computed: Some(calc),
            width_bits: 32,
            correctable: true,
            ok: stored == calc,
            status: if stored == calc { "ok" } else { "stale" }.to_string(),
        });
    }
    let mut store_addr = 0x4FFFBusize;
    while store_addr + 17 <= buf.len() {
        if &buf[store_addr + 13..store_addr + 17] == b"V4.1" {
            let sub = [
                (store_addr - 0x3FFB, store_addr, store_addr + 1),
                (store_addr + 5, store_addr + 0xB80, store_addr + 2945),
                (store_addr + 0xB85, store_addr + 0xC000, store_addr + 49153),
            ];
            for (idx, (st, en, sa)) in sub.into_iter().enumerate() {
                if sa + 4 <= buf.len() && en <= buf.len() {
                    let stored = u32_le(buf, sa);
                    let calc = tdi41_2002_calc(buf, st, en, s1_lsb, s1_msb, s1_lsb, s1_msb, false);
                    blocks.push(OemChecksumBlock {
                        label: format!("2002 Block @{store_addr:#X}#{idx}"),
                        start: st,
                        end: en,
                        store_offset: Some(sa),
                        stored: Some(stored),
                        computed: Some(calc),
                        width_bits: 32,
                        correctable: true,
                        ok: stored == calc,
                        status: if stored == calc { "ok" } else { "stale" }.to_string(),
                    });
                }
            }
        }
        store_addr += 0x10000;
    }
    blocks
}

const TDI41_ADDRS: [usize; 12] = [
    0x10000, 0x14000, 0x4C000, 0x50000, 0x50B80, 0x5C000, 0x60000, 0x60B80, 0x6C000, 0x70000,
    0x70B80, 0x7C000,
];
const TDI41V2_ADDRS: [usize; 8] = [
    0x10000, 0x14000, 0x58000, 0x58B80, 0x64000, 0x70000, 0x70B80, 0x7C000,
];

pub fn verify_edc15(rom: &[u8]) -> Option<OemChecksumReport> {
    let variant = detect_edc15_variant(rom)?;
    let mut copy = rom.to_vec();
    let (blocks, var_name) = match variant {
        Edc15Variant::Tdi41 => (run_tdi41_array(&mut copy, &TDI41_ADDRS, false), "tdi41"),
        Edc15Variant::Tdi41V2 => (run_tdi41_array(&mut copy, &TDI41V2_ADDRS, true), "tdi41v2"),
        Edc15Variant::Tdi41_2002 => (run_tdi41_2002(&mut copy), "tdi41_2002"),
    };
    let all_ok = !blocks.is_empty() && blocks.iter().all(|b| b.ok);
    Some(OemChecksumReport {
        algorithm_id: "bosch_edc15".to_string(),
        algorithm_name: "Bosch VAG EDC15P/V/VM".to_string(),
        family: "Bosch EDC15".to_string(),
        variant: var_name.to_string(),
        blocks,
        all_ok,
        notes: vec![],
    })
}

pub fn repair_edc15(rom: &[u8]) -> Result<(Vec<u8>, OemChecksumReport), String> {
    let report = verify_edc15(rom).ok_or_else(|| "ROM does not match Bosch EDC15P V4.1 layout".to_string())?;
    let mut out = rom.to_vec();
    for b in &report.blocks {
        if b.correctable {
            if let (Some(off), Some(val)) = (b.store_offset, b.computed) {
                write_u32_le(&mut out, off, val);
            }
        }
    }
    let after = verify_edc15(&out).ok_or_else(|| "EDC15 post-repair verify failed".to_string())?;
    Ok((out, after))
}
