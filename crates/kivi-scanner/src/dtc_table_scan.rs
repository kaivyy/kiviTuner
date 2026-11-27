//! Offline binary DTC (Diagnostic Trouble Code) array and switch-mask table scanner.
//!
//! Scans Bosch ME7/EDC15/EDC16/EDC17 and Subaru Denso/Hitachi binaries for:
//! - Contiguous SAE J2012 uint16 DTC code tables (`P0xxx`..`P3xxx`, `Cxxxx`, `Bxxxx`, `Uxxxx`)
//! - Parallel 1-byte switch/enable-class tables located nearby (`0x00`/`0x01`/error-class bytes)

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DtcCodeEntry {
    pub index: usize,
    pub raw_code: u16,
    pub formatted_code: String,
    pub switch_value: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DtcTableCandidate {
    pub code_table_offset: usize,
    pub big_endian: bool,
    pub entry_count: usize,
    pub switch_table_offset: Option<usize>,
    pub entries: Vec<DtcCodeEntry>,
}

/// Formats a packed 16-bit SAE J2012 DTC word (`P0420`, `U0100`, etc.).
pub fn format_sae_dtc(word: u16) -> String {
    let prefix = match (word >> 14) & 0x03 {
        0 => 'P',
        1 => 'C',
        2 => 'B',
        _ => 'U',
    };
    let d1 = (word >> 12) & 0x03;
    let d2 = (word >> 8) & 0x0F;
    let d3 = (word >> 4) & 0x0F;
    let d4 = word & 0x0F;
    format!("{prefix}{d1}{d2:X}{d3:X}{d4:X}")
}

fn is_plausible_p_code(word: u16) -> bool {
    if word == 0 || word == 0xFFFF {
        return false;
    }
    let sys = (word >> 14) & 0x03;
    let d1 = (word >> 12) & 0x03;
    let d2 = (word >> 8) & 0x0F;
    // Common SAE OBD-II codes: P0xxx..P3xxx or U0xxx..U1xxx (BCD-like nibbles mostly 0..9 in d2)
    sys == 0 && d1 <= 3 && d2 <= 9
}

fn find_nearby_switch_table(rom: &[u8], code_off: usize, count: usize, code_bytes: usize) -> Option<usize> {
    if count < 6 {
        return None;
    }
    let search_start = code_off.saturating_sub(0x4000);
    let search_end = (code_off + code_bytes + 0x4000).min(rom.len());
    let skip_start = code_off;
    let skip_end = code_off + code_bytes;

    let mut pos = search_start;
    while pos + count <= search_end {
        if pos < skip_end && pos + count > skip_start {
            pos = skip_end;
            continue;
        }
        let window = &rom[pos..pos + count];
        let small_switches = window.iter().filter(|&&b| b <= 0x0F).count();
        let active_cnt = window.iter().filter(|&&b| b != 0).count();
        if small_switches == count && active_cnt >= 2 && active_cnt < count {
            return Some(pos);
        }
        pos += 1;
    }
    None
}

/// Scans `rom` for DTC uint16 code tables (at least `min_entries` long) in both BE and LE.
pub fn scan_dtc_tables(rom: &[u8], min_entries: usize) -> Vec<DtcTableCandidate> {
    let min_len = min_entries.max(8);
    let mut results = Vec::new();

    for big_endian in [true, false] {
        let mut off = 0usize;
        while off + min_len * 2 <= rom.len() {
            let mut count = 0usize;
            while off + (count + 1) * 2 <= rom.len() {
                let p = off + count * 2;
                let w = if big_endian {
                    u16::from_be_bytes([rom[p], rom[p + 1]])
                } else {
                    u16::from_le_bytes([rom[p], rom[p + 1]])
                };
                if !is_plausible_p_code(w) {
                    break;
                }
                count += 1;
            }
            if count >= min_len {
                let mut words = Vec::with_capacity(count);
                let mut distinct_non_zero = Vec::new();
                for i in 0..count {
                    let p = off + i * 2;
                    let w = if big_endian {
                        u16::from_be_bytes([rom[p], rom[p + 1]])
                    } else {
                        u16::from_le_bytes([rom[p], rom[p + 1]])
                    };
                    if !distinct_non_zero.contains(&w) {
                        distinct_non_zero.push(w);
                    }
                    words.push(w);
                }
                // Filter constant fills like 0x0101 repeating
                if distinct_non_zero.len() * 2 >= count {
                    let switch_off = find_nearby_switch_table(rom, off, count, count * 2);
                    let entries = words
                        .into_iter()
                        .enumerate()
                        .map(|(i, raw)| DtcCodeEntry {
                            index: i,
                            raw_code: raw,
                            formatted_code: format_sae_dtc(raw),
                            switch_value: switch_off.map(|so| rom[so + i]),
                        })
                        .collect();
                    results.push(DtcTableCandidate {
                        code_table_offset: off,
                        big_endian,
                        entry_count: count,
                        switch_table_offset: switch_off,
                        entries,
                    });
                    off += count * 2;
                    continue;
                }
            }
            off += 2;
        }
    }

    results
}
