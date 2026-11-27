//! Subaru Hitachi (M32R K-Line/CAN, SH7058, SH72543R, M32R CAN TCU),
//! Subaru Denso SH7055 TCU, and Mitsubishi MH8104 CAN CVT TCU checksum engines.
//!
//! Ported from FastECU `modules/checksum/checksum_*.cpp`.

use super::{u16_be, u32_be, write_u16_be, write_u32_be, OemChecksumBlock, OemChecksumReport};
use crate::denso_descriptor::{
    all_checksums_disabled, correct_descriptor_checksums_with_mode, discover_denso_descriptor_table,
    sum_words_diesel, verify_descriptor_checksums, verify_diesel_descriptor_checksums,
    DISABLED_MAGIC,
};

fn sum32_be_step4_excluding(rom: &[u8], start: usize, end: usize, ex_start: usize, ex_end: usize) -> (u32, u32) {
    let mut sum = 0u32;
    let mut xor = 0u32;
    let mut i = start;
    while i + 4 <= end && i + 4 <= rom.len() {
        if i < ex_start || i > ex_end {
            let w = u32_be(rom, i);
            sum = sum.wrapping_add(w);
            xor ^= w;
        }
        i += 4;
    }
    (sum, xor)
}

fn sum8_xor8_excluding(rom: &[u8], ex_start: usize, ex_end: usize) -> (u8, u8) {
    let mut s = 0u8;
    let mut x = 0u8;
    for (i, &b) in rom.iter().enumerate() {
        if i < ex_start || i > ex_end {
            s = s.wrapping_add(b);
            x ^= b;
        }
    }
    (s, x)
}

// ===========================================================================
// 1. Subaru Hitachi M32R K-Line (512 KiB)
// ===========================================================================

pub fn verify_hitachi_m32r_kline(rom: &[u8]) -> Result<OemChecksumReport, String> {
    if rom.len() != 0x80000 {
        return Err("Hitachi M32R K-Line requires 512 KiB ROM".to_string());
    }
    let (s1, x2) = sum8_xor8_excluding(rom, 0x8100, 0x8101);
    let (sum3, _) = sum32_be_step4_excluding(rom, 0x4000, rom.len(), 0x8100, 0x8103);
    let bal_stored = u16_be(rom, 0x7FFFA);
    let bal_needed = bal_stored.wrapping_add(0x5AA5u16.wrapping_sub(sum3 as u16));
    let blocks = vec![
        OemChecksumBlock {
            label: "Checksum 1 (8-bit sum @0x8100)".to_string(),
            start: 0,
            end: 0x7FFFF,
            store_offset: Some(0x8100),
            stored: Some(rom[0x8100] as u32),
            computed: Some(s1 as u32),
            width_bits: 8,
            correctable: true,
            ok: rom[0x8100] == s1,
            status: if rom[0x8100] == s1 { "ok" } else { "stale" }.to_string(),
        },
        OemChecksumBlock {
            label: "Checksum 2 (8-bit XOR @0x8101)".to_string(),
            start: 0,
            end: 0x7FFFF,
            store_offset: Some(0x8101),
            stored: Some(rom[0x8101] as u32),
            computed: Some(x2 as u32),
            width_bits: 8,
            correctable: true,
            ok: rom[0x8101] == x2,
            status: if rom[0x8101] == x2 { "ok" } else { "stale" }.to_string(),
        },
        OemChecksumBlock {
            label: "Checksum 3 (balance @0x7FFFA -> 0x5AA5)".to_string(),
            start: 0x4000,
            end: 0x7FFFF,
            store_offset: Some(0x7FFFA),
            stored: Some(bal_stored as u32),
            computed: Some(bal_needed as u32),
            width_bits: 16,
            correctable: true,
            ok: (sum3 as u16) == 0x5AA5,
            status: if (sum3 as u16) == 0x5AA5 { "ok" } else { "stale" }.to_string(),
        },
    ];
    let all_ok = blocks.iter().all(|b| b.ok);
    Ok(OemChecksumReport {
        algorithm_id: "subaru_hitachi_m32r_kline".to_string(),
        algorithm_name: "Subaru Hitachi M32R K-Line".to_string(),
        family: "Subaru Hitachi".to_string(),
        variant: "512 KiB K-Line".to_string(),
        blocks,
        all_ok,
        notes: vec![],
    })
}

pub fn repair_hitachi_m32r_kline(rom: &[u8]) -> Result<(Vec<u8>, OemChecksumReport), String> {
    if rom.len() != 0x80000 {
        return Err("Hitachi M32R K-Line requires 512 KiB ROM".to_string());
    }
    let mut out = rom.to_vec();
    let (s1, x2) = sum8_xor8_excluding(&out, 0x8100, 0x8101);
    out[0x8100] = s1;
    out[0x8101] = x2;
    let (sum3, _) = sum32_be_step4_excluding(&out, 0x4000, out.len(), 0x8100, 0x8103);
    if (sum3 as u16) != 0x5AA5 {
        let bal = u16_be(&out, 0x7FFFA).wrapping_add(0x5AA5u16.wrapping_sub(sum3 as u16));
        write_u16_be(&mut out, 0x7FFFA, bal);
        // Recompute byte sum & XOR after modifying 0x7FFFA balance word
        let (s1b, x2b) = sum8_xor8_excluding(&out, 0x8100, 0x8101);
        out[0x8100] = s1b;
        out[0x8101] = x2b;
    }
    let rep = verify_hitachi_m32r_kline(&out)?;
    Ok((out, rep))
}

// ===========================================================================
// 2. Subaru Hitachi M32R CAN (512 KiB) & SH7058 CAN (1 MiB)
// ===========================================================================

struct HitachiCanSpec<'a> {
    id: &'a str,
    name: &'a str,
    region1: (usize, usize),
    ex: (usize, usize),
    locs: (usize, usize, usize, usize, usize),
    sum3_end: usize,
    balance_is_32bit: bool,
}

fn verify_hitachi_can_common(rom: &[u8], spec: &HitachiCanSpec<'_>) -> Result<OemChecksumReport, String> {
    let &HitachiCanSpec {
        id,
        name,
        region1,
        ex,
        locs,
        sum3_end,
        balance_is_32bit,
    } = spec;
    let (s1, x2) = sum32_be_step4_excluding(rom, region1.0, region1.1, usize::MAX, 0);
    let (sum5, _) = sum32_be_step4_excluding(rom, 0x4000, rom.len(), ex.0, ex.1);
    let (s3, x4) = sum32_be_step4_excluding(rom, 0, sum3_end, ex.0, ex.1);

    let (a1, a2, a3, a4, a5) = locs;
    let mut blocks = vec![
        OemChecksumBlock {
            label: "Boot/Header 32-bit Sum".to_string(),
            start: region1.0,
            end: region1.1 - 1,
            store_offset: Some(a1),
            stored: Some(u32_be(rom, a1)),
            computed: Some(s1),
            width_bits: 32,
            correctable: true,
            ok: u32_be(rom, a1) == s1,
            status: if u32_be(rom, a1) == s1 { "ok" } else { "stale" }.to_string(),
        },
        OemChecksumBlock {
            label: "Boot/Header 32-bit XOR".to_string(),
            start: region1.0,
            end: region1.1 - 1,
            store_offset: Some(a2),
            stored: Some(u32_be(rom, a2)),
            computed: Some(x2),
            width_bits: 32,
            correctable: true,
            ok: u32_be(rom, a2) == x2,
            status: if u32_be(rom, a2) == x2 { "ok" } else { "stale" }.to_string(),
        },
        if balance_is_32bit {
            let bal_stored = u32_be(rom, a5);
            let bal_needed = bal_stored.wrapping_add(DISABLED_MAGIC.wrapping_sub(sum5));
            OemChecksumBlock {
                label: "Main Balance32 -> 0x5AA5A55A".to_string(),
                start: 0x4000,
                end: rom.len() - 1,
                store_offset: Some(a5),
                stored: Some(bal_stored),
                computed: Some(bal_needed),
                width_bits: 32,
                correctable: true,
                ok: sum5 == DISABLED_MAGIC,
                status: if sum5 == DISABLED_MAGIC { "ok" } else { "stale" }.to_string(),
            }
        } else {
            let bal_stored = u16_be(rom, a5);
            let bal_needed = bal_stored.wrapping_add(0x5AA5u16.wrapping_sub(sum5 as u16));
            OemChecksumBlock {
                label: "Main Balance16 -> 0x5AA5".to_string(),
                start: 0x4000,
                end: rom.len() - 1,
                store_offset: Some(a5),
                stored: Some(bal_stored as u32),
                computed: Some(bal_needed as u32),
                width_bits: 16,
                correctable: true,
                ok: (sum5 as u16) == 0x5AA5,
                status: if (sum5 as u16) == 0x5AA5 { "ok" } else { "stale" }.to_string(),
            }
        },
        OemChecksumBlock {
            label: "Full ROM 32-bit Sum".to_string(),
            start: 0,
            end: sum3_end - 1,
            store_offset: Some(a3),
            stored: Some(u32_be(rom, a3)),
            computed: Some(s3),
            width_bits: 32,
            correctable: true,
            ok: u32_be(rom, a3) == s3,
            status: if u32_be(rom, a3) == s3 { "ok" } else { "stale" }.to_string(),
        },
        OemChecksumBlock {
            label: "Full ROM 32-bit XOR".to_string(),
            start: 0,
            end: sum3_end - 1,
            store_offset: Some(a4),
            stored: Some(u32_be(rom, a4)),
            computed: Some(x4),
            width_bits: 32,
            correctable: true,
            ok: u32_be(rom, a4) == x4,
            status: if u32_be(rom, a4) == x4 { "ok" } else { "stale" }.to_string(),
        },
    ];
    if id == "subaru_hitachi_m32r_can" {
        let (hi, lo) = sum8_xor8_excluding(rom, 0x10000, 0x10003);
        let c6 = ((hi as u16) << 8) | (lo as u16);
        let st6 = u16_be(rom, 0x10000);
        blocks.push(OemChecksumBlock {
            label: "Byte Sum/XOR @0x10000 (report)".to_string(),
            start: 0,
            end: rom.len() - 1,
            store_offset: Some(0x10000),
            stored: Some(st6 as u32),
            computed: Some(c6 as u32),
            width_bits: 16,
            correctable: true,
            ok: st6 == c6,
            status: if st6 == c6 { "ok" } else { "stale" }.to_string(),
        });
    }
    let all_ok = blocks.iter().all(|b| b.ok);
    Ok(OemChecksumReport {
        algorithm_id: id.to_string(),
        algorithm_name: name.to_string(),
        family: "Subaru Hitachi".to_string(),
        variant: format!("{} KiB", rom.len() / 1024),
        blocks,
        all_ok,
        notes: vec![],
    })
}

pub fn verify_hitachi_m32r_can(rom: &[u8]) -> Result<OemChecksumReport, String> {
    if rom.len() != 0x80000 {
        return Err("Hitachi M32R CAN requires 512 KiB ROM".to_string());
    }
    verify_hitachi_can_common(
        rom,
        &HitachiCanSpec {
            id: "subaru_hitachi_m32r_can",
            name: "Subaru Hitachi M32R CAN",
            region1: (0x6000, 0x8000),
            ex: (0x10000, 0x10003),
            locs: (0x7FFE8, 0x7FFEC, 0x7FFF0, 0x7FFF4, 0x7FFFA),
            sum3_end: 0x7FFF0,
            balance_is_32bit: false,
        },
    )
}

pub fn repair_hitachi_m32r_can(rom: &[u8]) -> Result<(Vec<u8>, OemChecksumReport), String> {
    if rom.len() != 0x80000 {
        return Err("Hitachi M32R CAN requires 512 KiB ROM".to_string());
    }
    let mut out = rom.to_vec();
    let (s1, x2) = sum32_be_step4_excluding(&out, 0x6000, 0x8000, usize::MAX, 0);
    write_u32_be(&mut out, 0x7FFE8, s1);
    write_u32_be(&mut out, 0x7FFEC, x2);
    // Recompute full-ROM sum/xor (up to 0x7FFF0, which includes 0x7FFE8/0x7FFEC) before balancing @0x7FFFA.
    let (s3, x4) = sum32_be_step4_excluding(&out, 0, 0x7FFF0, 0x10000, 0x10003);
    write_u32_be(&mut out, 0x7FFF0, s3);
    write_u32_be(&mut out, 0x7FFF4, x4);
    let (sum5, _) = sum32_be_step4_excluding(&out, 0x4000, out.len(), 0x10000, 0x10003);
    if (sum5 as u16) != 0x5AA5 {
        let bal = u16_be(&out, 0x7FFFA).wrapping_add(0x5AA5u16.wrapping_sub(sum5 as u16));
        write_u16_be(&mut out, 0x7FFFA, bal);
    }
    // Recompute byte sum/XOR @0x10000 last (excluded from both Checksums 3/4 and 5).
    let (hi, lo) = sum8_xor8_excluding(&out, 0x10000, 0x10003);
    write_u16_be(&mut out, 0x10000, ((hi as u16) << 8) | (lo as u16));
    let rep = verify_hitachi_m32r_can(&out)?;
    Ok((out, rep))
}

pub fn verify_hitachi_sh7058(rom: &[u8]) -> Result<OemChecksumReport, String> {
    if rom.len() != 0x10_0000 {
        return Err("Hitachi SH7058 requires 1 MiB ROM".to_string());
    }
    verify_hitachi_can_common(
        rom,
        &HitachiCanSpec {
            id: "subaru_hitachi_sh7058",
            name: "Subaru Hitachi SH7058 CAN",
            region1: (0x18400, 0x1E000),
            ex: (0xFFFF0, 0xFFFF7),
            locs: (0xFFFE8, 0xFFFEC, 0xFFFF0, 0xFFFF4, 0xFFFF8),
            sum3_end: 0x10_0000,
            balance_is_32bit: true,
        },
    )
}

pub fn repair_hitachi_sh7058(rom: &[u8]) -> Result<(Vec<u8>, OemChecksumReport), String> {
    if rom.len() != 0x10_0000 {
        return Err("Hitachi SH7058 requires 1 MiB ROM".to_string());
    }
    let mut out = rom.to_vec();
    let (s1, x2) = sum32_be_step4_excluding(&out, 0x18400, 0x1E000, usize::MAX, 0);
    write_u32_be(&mut out, 0xFFFE8, s1);
    write_u32_be(&mut out, 0xFFFEC, x2);
    let (sum5, _) = sum32_be_step4_excluding(&out, 0x4000, 0x10_0000, 0xFFFF0, 0xFFFF7);
    if sum5 != DISABLED_MAGIC {
        let bal = u32_be(&out, 0xFFFF8).wrapping_add(DISABLED_MAGIC.wrapping_sub(sum5));
        write_u32_be(&mut out, 0xFFFF8, bal);
    }
    let (s3, x4) = sum32_be_step4_excluding(&out, 0, 0x10_0000, 0xFFFF0, 0xFFFF7);
    write_u32_be(&mut out, 0xFFFF0, s3);
    write_u32_be(&mut out, 0xFFFF4, x4);
    let rep = verify_hitachi_sh7058(&out)?;
    Ok((out, rep))
}

// ===========================================================================
// 3. Hitachi SH72543R (2 MiB) & TCUs (Hitachi M32R TCU, Denso SH7055 TCU, Mitsu MH8104 TCU)
// ===========================================================================

pub fn verify_hitachi_sh72543r(rom: &[u8]) -> Result<OemChecksumReport, String> {
    if rom.len() != 0x20_0000 {
        return Err("Hitachi SH72543R requires 2 MiB ROM".to_string());
    }
    let (chunks, _) = rom[0x6000..0x20_0000].as_chunks::<2>();
    let sum: u16 = chunks
        .iter()
        .fold(0u16, |acc, &c| acc.wrapping_add(u16::from_be_bytes(c)));
    let bal = u16_be(rom, 0x1FFFFE);
    let needed = bal.wrapping_add(0x5AA5u16.wrapping_sub(sum));
    let ok = sum == 0x5AA5;
    Ok(OemChecksumReport {
        algorithm_id: "subaru_hitachi_sh72543r".to_string(),
        algorithm_name: "Subaru Hitachi SH72543R".to_string(),
        family: "Subaru Hitachi".to_string(),
        variant: "2 MiB".to_string(),
        blocks: vec![OemChecksumBlock {
            label: "Balance16 @0x1FFFFE -> 0x5AA5".to_string(),
            start: 0x6000,
            end: 0x1FFFFF,
            store_offset: Some(0x1FFFFE),
            stored: Some(bal as u32),
            computed: Some(needed as u32),
            width_bits: 16,
            correctable: true,
            ok,
            status: if ok { "ok" } else { "stale" }.to_string(),
        }],
        all_ok: ok,
        notes: vec![],
    })
}

pub fn repair_hitachi_sh72543r(rom: &[u8]) -> Result<(Vec<u8>, OemChecksumReport), String> {
    let rep = verify_hitachi_sh72543r(rom)?;
    let mut out = rom.to_vec();
    if let Some(val) = rep.blocks[0].computed {
        write_u16_be(&mut out, 0x1FFFFE, val as u16);
    }
    let after = verify_hitachi_sh72543r(&out)?;
    Ok((out, after))
}

pub fn verify_hitachi_tcu_m32r(rom: &[u8]) -> Result<OemChecksumReport, String> {
    if rom.len() < 0x8024 || !rom.len().is_multiple_of(4) {
        return Err("Hitachi M32R TCU ROM too small or unaligned".to_string());
    }
    let (c1, _) = sum32_be_step4_excluding(rom, 0x8020, rom.len(), usize::MAX, 0);
    let (raw2, _) = sum32_be_step4_excluding(rom, 0, rom.len(), 0x8000, 0x8007);
    let c2 = raw2.wrapping_neg();
    let b1_stored = u32_be(rom, 0x8020);
    let b1_needed = b1_stored.wrapping_add(DISABLED_MAGIC.wrapping_sub(c1));
    let st2 = u32_be(rom, 0x8000);
    let blocks = vec![
        OemChecksumBlock {
            label: "TCU Checksum 1 (@0x8020 -> 0x5AA5A55A)".to_string(),
            start: 0x8020,
            end: rom.len() - 1,
            store_offset: Some(0x8020),
            stored: Some(b1_stored),
            computed: Some(b1_needed),
            width_bits: 32,
            correctable: true,
            ok: c1 == DISABLED_MAGIC,
            status: if c1 == DISABLED_MAGIC { "ok" } else { "stale" }.to_string(),
        },
        OemChecksumBlock {
            label: "TCU Checksum 2 (Two's Compl @0x8000/0x8004)".to_string(),
            start: 0,
            end: rom.len() - 1,
            store_offset: Some(0x8000),
            stored: Some(st2),
            computed: Some(c2),
            width_bits: 32,
            correctable: true,
            ok: st2 == c2 && u32_be(rom, 0x8004) == c2,
            status: if st2 == c2 && u32_be(rom, 0x8004) == c2 { "ok" } else { "stale" }.to_string(),
        },
    ];
    let all_ok = blocks.iter().all(|b| b.ok);
    Ok(OemChecksumReport {
        algorithm_id: "subaru_hitachi_tcu_m32r".to_string(),
        algorithm_name: "Subaru Hitachi M32R CAN TCU".to_string(),
        family: "Subaru TCU".to_string(),
        variant: format!("{} KiB", rom.len() / 1024),
        blocks,
        all_ok,
        notes: vec![],
    })
}

pub fn repair_hitachi_tcu_m32r(rom: &[u8]) -> Result<(Vec<u8>, OemChecksumReport), String> {
    if rom.len() < 0x8024 || !rom.len().is_multiple_of(4) {
        return Err("Hitachi TCU M32R ROM too small or misaligned".to_string());
    }
    let mut out = rom.to_vec();
    let (c1, _) = sum32_be_step4_excluding(&out, 0x8020, out.len(), usize::MAX, 0);
    if c1 != DISABLED_MAGIC {
        let bal = u32_be(&out, 0x8020).wrapping_add(DISABLED_MAGIC.wrapping_sub(c1));
        write_u32_be(&mut out, 0x8020, bal);
    }
    let (raw2, _) = sum32_be_step4_excluding(&out, 0, out.len(), 0x8000, 0x8007);
    let c2 = raw2.wrapping_neg();
    write_u32_be(&mut out, 0x8000, c2);
    write_u32_be(&mut out, 0x8004, c2);
    let rep = verify_hitachi_tcu_m32r(&out)?;
    Ok((out, rep))
}

const DENSO_TCU_AREAS: [(usize, usize); 12] = [
    (0x1000, 0x0800),
    (0x3000, 0x003A),
    (0x3080, 0x4C00),
    (0xC880, 0x83F6),
    (0x1D06C, 0x83F6),
    (0x2D858, 0x83F8),
    (0x3E048, 0x0FDC),
    (0x40000, 0x741C),
    (0x4E838, 0x83F6),
    (0x5F024, 0x83F8),
    (0x6F814, 0x8174),
    (0x7FB80, 0x0240),
];

pub fn verify_subaru_denso_tcu_sh7055(rom: &[u8]) -> Result<OemChecksumReport, String> {
    if rom.len() < 0x80000 {
        return Err("Subaru Denso SH7055 TCU requires 512 KiB ROM".to_string());
    }
    let mut sum = 0u16;
    for (start, word_cnt) in DENSO_TCU_AREAS {
        let end = start + word_cnt * 2;
        let (chunks, _) = rom[start..end].as_chunks::<2>();
        for &c in chunks {
            sum = sum.wrapping_add(u16::from_be_bytes(c));
        }
    }
    let bal = u16_be(rom, 0x7FFF4);
    let needed = bal.wrapping_add(0x5AA5u16.wrapping_sub(sum));
    let ok = sum == 0x5AA5;
    Ok(OemChecksumReport {
        algorithm_id: "subaru_denso_tcu_sh7055".to_string(),
        algorithm_name: "Subaru Denso SH7055 TCU".to_string(),
        family: "Subaru TCU".to_string(),
        variant: "512 KiB".to_string(),
        blocks: vec![OemChecksumBlock {
            label: "12-Region Word Sum Balance @0x7FFF4 -> 0x5AA5".to_string(),
            start: 0x1000,
            end: 0x7FFFF,
            store_offset: Some(0x7FFF4),
            stored: Some(bal as u32),
            computed: Some(needed as u32),
            width_bits: 16,
            correctable: true,
            ok,
            status: if ok { "ok" } else { "stale" }.to_string(),
        }],
        all_ok: ok,
        notes: vec![],
    })
}

pub fn repair_subaru_denso_tcu_sh7055(rom: &[u8]) -> Result<(Vec<u8>, OemChecksumReport), String> {
    let rep = verify_subaru_denso_tcu_sh7055(rom)?;
    let mut out = rom.to_vec();
    if let Some(val) = rep.blocks[0].computed {
        write_u16_be(&mut out, 0x7FFF4, val as u16);
    }
    let after = verify_subaru_denso_tcu_sh7055(&out)?;
    Ok((out, after))
}

pub fn verify_mitsu_mh8104_tcu(rom: &[u8]) -> Result<OemChecksumReport, String> {
    if rom.len() < 0x80000 {
        return Err("Mitsubishi MH8104 TCU requires 512 KiB ROM".to_string());
    }
    let (mut sum, _) = sum32_be_step4_excluding(rom, 0x8000, 0x80000, usize::MAX, 0);
    sum = sum.wrapping_sub(0xFFFF);
    for _ in 0..5 {
        sum = sum.wrapping_sub(0xFFFF_FFFF);
    }
    const TARGET: u32 = 0x5AA4_5AAB;
    let bal = u32_be(rom, 0x81FC);
    let needed = bal.wrapping_add(TARGET.wrapping_sub(sum));
    let ok = sum == TARGET;
    Ok(OemChecksumReport {
        algorithm_id: "mitsu_mh8104_tcu".to_string(),
        algorithm_name: "Mitsubishi MH8104 CAN CVT TCU".to_string(),
        family: "Mitsubishi TCU".to_string(),
        variant: "512 KiB".to_string(),
        blocks: vec![OemChecksumBlock {
            label: "CVT Balance @0x81FC -> 0x5AA45AAB".to_string(),
            start: 0x8000,
            end: 0x7FFFF,
            store_offset: Some(0x81FC),
            stored: Some(bal),
            computed: Some(needed),
            width_bits: 32,
            correctable: true,
            ok,
            status: if ok { "ok" } else { "stale" }.to_string(),
        }],
        all_ok: ok,
        notes: vec![],
    })
}

pub fn repair_mitsu_mh8104_tcu(rom: &[u8]) -> Result<(Vec<u8>, OemChecksumReport), String> {
    let rep = verify_mitsu_mh8104_tcu(rom)?;
    let mut out = rom.to_vec();
    if let Some(val) = rep.blocks[0].computed {
        write_u32_be(&mut out, 0x81FC, val);
    }
    let after = verify_mitsu_mh8104_tcu(&out)?;
    Ok((out, after))
}

pub fn verify_subaru_denso_auto(rom: &[u8]) -> Option<OemChecksumReport> {
    let (offset, entries) = discover_denso_descriptor_table(rom)?;
    let diesel = offset == 0x0FFB80 || offset == 0x17FB80;
    let ok = if diesel {
        verify_diesel_descriptor_checksums(rom, &entries, offset).ok()?
    } else {
        verify_descriptor_checksums(rom, &entries).ok()?
    };
    let blocks = entries
        .iter()
        .enumerate()
        .filter(|(_, e)| !(e.start == 0 && e.end == 0))
        .map(|(idx, e)| {
            let calc = if e.stored == DISABLED_MAGIC {
                DISABLED_MAGIC
            } else if diesel {
                DISABLED_MAGIC.wrapping_sub(sum_words_diesel(rom, e.start as usize, e.end as usize, offset))
            } else {
                crate::denso_descriptor::expected_stored(rom, e)
            };
            OemChecksumBlock {
                label: format!("Denso Descriptor {idx}"),
                start: e.start as usize,
                end: e.end as usize,
                store_offset: Some(offset + idx * 12 + 8),
                stored: Some(e.stored),
                computed: Some(calc),
                width_bits: 32,
                correctable: e.stored != DISABLED_MAGIC,
                ok: e.stored == DISABLED_MAGIC || e.stored == calc,
                status: if e.stored == DISABLED_MAGIC {
                    "disabled"
                } else if e.stored == calc {
                    "ok"
                } else {
                    "stale"
                }
                .to_string(),
            }
        })
        .collect();
    let disabled = all_checksums_disabled(&entries);
    Some(OemChecksumReport {
        algorithm_id: "subaru_denso_sh7xxx".to_string(),
        algorithm_name: "Subaru Denso SH705x/SH7xxx (Inclusive Descriptor Table)".to_string(),
        family: "Subaru Denso".to_string(),
        variant: format!("Table @{offset:#X}"),
        blocks,
        all_ok: disabled || ok,
        notes: vec![],
    })
}

pub fn repair_subaru_denso_auto(rom: &[u8]) -> Result<(Vec<u8>, OemChecksumReport), String> {
    let (offset, entries) = discover_denso_descriptor_table(rom)
        .ok_or_else(|| "No Subaru Denso descriptor table found".to_string())?;
    let diesel = offset == 0x0FFB80 || offset == 0x17FB80;
    let out = correct_descriptor_checksums_with_mode(rom, &entries, offset, diesel)
        .map_err(|e| e.to_string())?;
    let rep = verify_subaru_denso_auto(&out)
        .ok_or_else(|| "Subaru Denso post-repair verify failed".to_string())?;
    Ok((out, rep))
}
