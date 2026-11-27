//! Bosch TriCore EDC17/MED17 CRC-32C (Castagnoli) block verification/repair
//! and OpenRemap parameterized native blind checksum sweep engine.

use super::{u16_be, u16_le, u32_be, u32_le, write_u32_be, write_u32_le, OemChecksumBlock, OemChecksumReport, SweepMatch};
use crate::primitives::{compute_crc32_with_initial, crc16_arc, crc16_ccitt, crc32_castagnoli, crc8_maxim};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TriCoreCrcBlockSpec {
    pub label: &'static str,
    pub start: usize,
    pub end_incl: usize,
    pub store_offset: usize,
    pub little_endian: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedTriCoreCrcBlockSpec {
    pub label: String,
    pub start: usize,
    pub end_incl: usize,
    pub store_offset: usize,
    pub little_endian: bool,
}

fn verify_edc17_slices<'a>(
    rom: &[u8],
    specs: impl Iterator<Item = (&'a str, usize, usize, usize, bool)>,
) -> OemChecksumReport {
    let mut blocks = Vec::new();
    for (label, start, end_incl, store_offset, little_endian) in specs {
        if start > end_incl
            || end_incl >= rom.len()
            || store_offset.checked_add(4).is_none_or(|e| e > rom.len())
        {
            blocks.push(OemChecksumBlock {
                label: label.to_string(),
                start,
                end: end_incl,
                store_offset: Some(store_offset),
                stored: None,
                computed: None,
                width_bits: 32,
                correctable: false,
                ok: false,
                status: "out_of_range".to_string(),
            });
            continue;
        }
        let store_end = store_offset.saturating_add(4);
        let payload: Vec<u8> = (start..=end_incl)
            .filter(|addr| *addr < store_offset || *addr >= store_end)
            .map(|addr| rom[addr])
            .collect();
        let computed = crc32_castagnoli(&payload);
        let stored = if little_endian {
            u32_le(rom, store_offset)
        } else {
            u32_be(rom, store_offset)
        };
        let ok = stored == computed;
        blocks.push(OemChecksumBlock {
            label: label.to_string(),
            start,
            end: end_incl,
            store_offset: Some(store_offset),
            stored: Some(stored),
            computed: Some(computed),
            width_bits: 32,
            correctable: true,
            ok,
            status: if ok { "ok" } else { "stale" }.to_string(),
        });
    }
    let all_ok = !blocks.is_empty() && blocks.iter().all(|b| b.ok);
    OemChecksumReport {
        algorithm_id: "bosch_edc17_crc32c".to_string(),
        algorithm_name: "Bosch TriCore EDC17/MED17 (CRC-32C Castagnoli)".to_string(),
        family: "Bosch TriCore".to_string(),
        variant: "Explicit CRC-32C block descriptors".to_string(),
        blocks,
        all_ok,
        notes: vec![
            "Verifies CRC-32C data integrity blocks; OEM RSA/TPROT signatures are outside CRC repair.".to_string(),
        ],
    }
}

/// Verifies caller-supplied TriCore EDC17/MED17 CRC-32C (Castagnoli) block descriptors.
///
/// Honest boundary: verifies and updates CRC-32C integrity blocks only; RSA/TPROT
/// OEM digital signatures require the OEM private key and are not bypassable offline.
pub fn verify_edc17_crc32c_blocks(rom: &[u8], specs: &[TriCoreCrcBlockSpec]) -> OemChecksumReport {
    verify_edc17_slices(
        rom,
        specs
            .iter()
            .map(|s| (s.label, s.start, s.end_incl, s.store_offset, s.little_endian)),
    )
}

pub fn verify_edc17_crc32c_owned(
    rom: &[u8],
    specs: &[OwnedTriCoreCrcBlockSpec],
) -> OemChecksumReport {
    verify_edc17_slices(
        rom,
        specs
            .iter()
            .map(|s| (s.label.as_str(), s.start, s.end_incl, s.store_offset, s.little_endian)),
    )
}

fn repair_edc17_slices<'a, I>(rom: &[u8], specs_fn: impl Fn() -> I) -> Result<(Vec<u8>, OemChecksumReport), String>
where
    I: Iterator<Item = (&'a str, usize, usize, usize, bool)>,
{
    let mut out = rom.to_vec();
    let initial = verify_edc17_slices(&out, specs_fn());
    for (b, (_lbl, _s, _e, store_offset, little_endian)) in initial.blocks.iter().zip(specs_fn()) {
        if !b.correctable {
            return Err(format!("Block '{}' out of range", b.label));
        }
        if let Some(val) = b.computed {
            if little_endian {
                write_u32_le(&mut out, store_offset, val);
            } else {
                write_u32_be(&mut out, store_offset, val);
            }
        }
    }
    let after = verify_edc17_slices(&out, specs_fn());
    Ok((out, after))
}

pub fn repair_edc17_crc32c_blocks(
    rom: &[u8],
    specs: &[TriCoreCrcBlockSpec],
) -> Result<(Vec<u8>, OemChecksumReport), String> {
    repair_edc17_slices(rom, || {
        specs
            .iter()
            .map(|s| (s.label, s.start, s.end_incl, s.store_offset, s.little_endian))
    })
}

pub fn repair_edc17_crc32c_owned(
    rom: &[u8],
    specs: &[OwnedTriCoreCrcBlockSpec],
) -> Result<(Vec<u8>, OemChecksumReport), String> {
    repair_edc17_slices(rom, || {
        specs
            .iter()
            .map(|s| (s.label.as_str(), s.start, s.end_incl, s.store_offset, s.little_endian))
    })
}

// ===========================================================================
// Native Blind Checksum Sweep (ported from OpenRemap checksum.rs + checksum.py)
// ===========================================================================

#[derive(Clone, Copy)]
enum SweepAlgo {
    Sum8,
    Sum16Bytes,
    Sum16Le,
    Sum16Be,
    Xor8,
    Xor16Le,
    Xor16Be,
    Crc8,
    Crc16Ccitt,
    Crc16Arc,
    Crc32Ieee,
    Sum16LeAcc32,
    Sum16BeAcc32,
}

impl SweepAlgo {
    const ALL: [Self; 13] = [
        Self::Sum8,
        Self::Sum16Bytes,
        Self::Sum16Le,
        Self::Sum16Be,
        Self::Xor8,
        Self::Xor16Le,
        Self::Xor16Be,
        Self::Crc8,
        Self::Crc16Ccitt,
        Self::Crc16Arc,
        Self::Crc32Ieee,
        Self::Sum16LeAcc32,
        Self::Sum16BeAcc32,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::Sum8 => "sum8",
            Self::Sum16Bytes => "sum16_bytes",
            Self::Sum16Le => "sum16_le",
            Self::Sum16Be => "sum16_be",
            Self::Xor8 => "xor8",
            Self::Xor16Le => "xor16_le",
            Self::Xor16Be => "xor16_be",
            Self::Crc8 => "crc8_maxim",
            Self::Crc16Ccitt => "crc16_ccitt",
            Self::Crc16Arc => "crc16_arc",
            Self::Crc32Ieee => "crc32_ieee",
            Self::Sum16LeAcc32 => "sum16_le_acc32",
            Self::Sum16BeAcc32 => "sum16_be_acc32",
        }
    }

    fn width_bytes(self) -> usize {
        match self {
            Self::Sum8 | Self::Xor8 | Self::Crc8 => 1,
            Self::Sum16Bytes | Self::Sum16Le | Self::Sum16Be | Self::Xor16Le | Self::Xor16Be
            | Self::Crc16Ccitt | Self::Crc16Arc => 2,
            Self::Crc32Ieee | Self::Sum16LeAcc32 | Self::Sum16BeAcc32 => 4,
        }
    }

    fn inits(self) -> [u32; 2] {
        match self.width_bytes() {
            1 => [0x00, 0xFF],
            2 => [0x0000, 0xFFFF],
            _ => [0x0000_0000, 0xFFFF_FFFF],
        }
    }
}

fn eval_sweep(algo: SweepAlgo, data: &[u8], init: u32) -> u32 {
    match algo {
        SweepAlgo::Sum8 => {
            let mut s = init & 0xFF;
            for &b in data {
                s = s.wrapping_add(b as u32);
            }
            s & 0xFF
        }
        SweepAlgo::Sum16Bytes => {
            let mut s = init & 0xFFFF;
            for &b in data {
                s = s.wrapping_add(b as u32);
            }
            s & 0xFFFF
        }
        SweepAlgo::Sum16Le | SweepAlgo::Sum16Be => {
            let le = matches!(algo, SweepAlgo::Sum16Le);
            let mut s = init & 0xFFFF;
            let (chunks, rem) = data.as_chunks::<2>();
            for &chunk in chunks {
                let w = if le { u16::from_le_bytes(chunk) } else { u16::from_be_bytes(chunk) };
                s = s.wrapping_add(w as u32);
            }
            if let Some(&last) = rem.first() {
                s = s.wrapping_add(last as u32);
            }
            s & 0xFFFF
        }
        SweepAlgo::Xor8 => {
            let mut s = init & 0xFF;
            for &b in data {
                s ^= b as u32;
            }
            s
        }
        SweepAlgo::Xor16Le | SweepAlgo::Xor16Be => {
            let le = matches!(algo, SweepAlgo::Xor16Le);
            let mut s = init & 0xFFFF;
            let (chunks, _) = data.as_chunks::<2>();
            for &chunk in chunks {
                let w = if le { u16::from_le_bytes(chunk) } else { u16::from_be_bytes(chunk) };
                s ^= w as u32;
            }
            s
        }
        SweepAlgo::Crc8 => crc8_maxim(data, init as u8) as u32,
        SweepAlgo::Crc16Ccitt => {
            let mut crc = init as u16;
            for &b in data {
                crc ^= (b as u16) << 8;
                for _ in 0..8 {
                    crc = if (crc & 0x8000) != 0 { (crc << 1) ^ 0x1021 } else { crc << 1 };
                }
            }
            let _ = crc16_ccitt;
            crc as u32
        }
        SweepAlgo::Crc16Arc => crc16_arc(data, init as u16) as u32,
        SweepAlgo::Crc32Ieee => compute_crc32_with_initial(data, init),
        SweepAlgo::Sum16LeAcc32 | SweepAlgo::Sum16BeAcc32 => {
            let le = matches!(algo, SweepAlgo::Sum16LeAcc32);
            let mut s = init;
            let (chunks, _) = data.as_chunks::<2>();
            for &chunk in chunks {
                let w = if le { u16::from_le_bytes(chunk) } else { u16::from_be_bytes(chunk) };
                s = s.wrapping_add(w as u32);
            }
            s
        }
    }
}

fn is_erased_chunk(chunk: &[u8]) -> bool {
    if chunk.is_empty() {
        return true;
    }
    let mut c00 = 0usize;
    let mut cff = 0usize;
    let mut cc3 = 0usize;
    for &b in chunk {
        match b {
            0x00 => c00 += 1,
            0xFF => cff += 1,
            0xC3 => cc3 += 1,
            _ => {}
        }
    }
    let max = c00.max(cff).max(cc3);
    max * 10 >= chunk.len() * 9
}

fn read_width(buf: &[u8], off: usize, w: usize, le: bool) -> Option<u32> {
    if off + w > buf.len() {
        return None;
    }
    Some(match w {
        1 => buf[off] as u32,
        2 => if le { u16_le(buf, off) as u32 } else { u16_be(buf, off) as u32 },
        4 => if le { u32_le(buf, off) } else { u32_be(buf, off) },
        _ => return None,
    })
}

/// Runs a closed parameterized checksum sweep over whole-file and 16/32/64 KiB page schemes.
pub fn blind_checksum_sweep(rom: &[u8]) -> Vec<SweepMatch> {
    let n = rom.len();
    if n < 16 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for algo in SweepAlgo::ALL {
        let w = algo.width_bytes();
        let mask: u32 = match w {
            1 => 0xFF,
            2 => 0xFFFF,
            _ => 0xFFFF_FFFF,
        };
        for init in algo.inits() {
            // Whole-file store at end
            if n > w {
                let val = eval_sweep(algo, &rom[..n - w], init);
                for x in algo.inits() {
                    let direct = (val ^ x) & mask;
                    let compl = direct.wrapping_neg() & mask;
                    for le in if w == 1 { vec![true] } else { vec![true, false] } {
                        if let Some(stored) = read_width(rom, n - w, w, le) {
                            for (is_comp, expected) in [(false, direct), (true, compl)] {
                                if stored == expected {
                                    out.push(SweepMatch {
                                        algo: algo.name().to_string(),
                                        init,
                                        final_xor: x,
                                        region: "whole".to_string(),
                                        exclude_tail: w,
                                        store_location: "file_end".to_string(),
                                        store_le: le,
                                        complement: is_comp,
                                        pages_matched: 1,
                                        pages_total: 1,
                                        rate_bps: 10_000,
                                    });
                                }
                            }
                        }
                    }
                }
            }
            // Per-page variants (16K, 32K, 64K) when at least 3 non-erased pages exist
            for (ps, label) in [(0x4000usize, "page16"), (0x8000, "page32"), (0x10000, "page64")] {
                if n < 2 * ps {
                    continue;
                }
                let pages: Vec<(usize, usize)> = (0..n - (n % ps))
                    .step_by(ps)
                    .map(|s| (s, (s + ps).min(n)))
                    .filter(|&(s, e)| e - s > w && !is_erased_chunk(&rom[s..e]))
                    .collect();
                if pages.len() < 3 {
                    continue;
                }
                for x in algo.inits() {
                    for le in if w == 1 { vec![true] } else { vec![true, false] } {
                        for is_comp in [false, true] {
                            let matched = pages
                                .iter()
                                .filter(|&&(s, e)| {
                                    let val = (eval_sweep(algo, &rom[s..e - w], init) ^ x) & mask;
                                    let exp = if is_comp { val.wrapping_neg() & mask } else { val };
                                    read_width(rom, e - w, w, le) == Some(exp)
                                })
                                .count();
                            if matched * 10 >= pages.len() * 9 {
                                let rate_bps = ((matched * 10_000) / pages.len()) as u16;
                                out.push(SweepMatch {
                                    algo: algo.name().to_string(),
                                    init,
                                    final_xor: x,
                                    region: label.to_string(),
                                    exclude_tail: w,
                                    store_location: "page_end".to_string(),
                                    store_le: le,
                                    complement: is_comp,
                                    pages_matched: matched,
                                    pages_total: pages.len(),
                                    rate_bps,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    out.sort_by(|a, b| b.rate_bps.cmp(&a.rate_bps).then_with(|| a.algo.cmp(&b.algo)));
    out
}
