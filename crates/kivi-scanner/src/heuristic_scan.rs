//! 16-bit heuristic ROM table scanning.
//!
//! Ported from `ecu_engine_rust::recognition::heuristic::HeuristicMapDetector::scan_rom_16bit`
//! and its `scan_rom_16bit_le` / `scan_rom_16bit_be` wrappers.
//!
//! ## Why this is not a duplicate of `table_detector`
//!
//! [`crate::table_detector::detect_periodic_tables`] finds **periodic runs** using
//! pitch autocorrelation over fixed candidate pitches. This module is a dense
//! exhaustive scan over rectangular `w × h` windows at every 2-byte offset,
//! gated by entropy, surface smoothness and row autocorrelation. The two use
//! different strategies and produce different candidate sets; neither subsumes
//! the other. Both may propose the same region — callers pick one, and this wave
//! does not attempt to merge their scores.
//!
//! ## Gates (read from the source, asserted by tests)
//!
//! * block entropy in `[1.5, 6.8]`,
//! * chosen interpretation's smoothness `< 0.38`,
//! * autocorrelation of the unsigned flat series at lag `w` `> 0.40`.
//!
//! ## Deviation from the source
//!
//! The source sorts with `partial_cmp(..).unwrap()`, which panics when a score is
//! NaN. This module uses a total order so degenerate input cannot crash the
//! scanner. Valid input produces the same ordering.
//!
//! This module performs no I/O and is deterministic.

use crate::scan_metrics::{autocorrelation, entropy, smoothness};

/// A candidate calibration map found by the heuristic scanner.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DiscoveredMap {
    /// Byte offset of the first cell.
    pub offset: usize,
    /// Row count.
    pub rows: usize,
    /// Column count.
    pub cols: usize,
    /// Whether the values are better read as signed 16-bit.
    pub is_signed: bool,
    /// Byte order the scan was performed with.
    pub is_little_endian: bool,
    /// Confidence score; higher is stronger.
    pub score: f64,
    /// Surface roughness of the chosen interpretation.
    pub smoothness: f64,
    /// Smallest cell value.
    pub val_min: f64,
    /// Largest cell value.
    pub val_max: f64,
    /// Suggested display name.
    pub suggested_name: String,
    /// Suggested category.
    pub category: String,
    /// Suggested unit.
    pub unit: String,
}

/// Scans `rom` for rectangular 16-bit tables.
///
/// `candidate_widths` lists column counts to try; rows are tried over
/// `min_rows..=max_rows`. Scanning advances in 2-byte steps.
pub fn scan_rom_16bit(
    rom: &[u8],
    candidate_widths: &[usize],
    min_rows: usize,
    max_rows: usize,
    is_little_endian: bool,
) -> Vec<DiscoveredMap> {
    let mut maps: Vec<DiscoveredMap> = Vec::new();
    let rom_len = rom.len();

    for &w in candidate_widths {
        if w == 0 {
            continue;
        }
        for h in min_rows..=max_rows {
            let size_cells = match w.checked_mul(h) {
                Some(v) => v,
                None => continue,
            };
            let size_bytes = match size_cells.checked_mul(2) {
                Some(v) => v,
                None => continue,
            };
            if size_bytes == 0 {
                continue;
            }

            let mut offset = 0usize;
            while offset + size_bytes <= rom_len {
                let window = &rom[offset..offset + size_bytes];
                let ent = entropy(window);
                if (1.5..=6.8).contains(&ent) {
                    if let Some(found) = evaluate_window(
                        rom,
                        offset,
                        h,
                        w,
                        is_little_endian,
                    ) {
                        maps.push(found);
                    }
                }
                offset += 2;
            }
        }
    }

    // Total order: no panic on NaN, and deterministic for equal scores.
    maps.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Suppress overlapping candidates, keeping the highest-scoring one.
    let mut filtered: Vec<DiscoveredMap> = Vec::new();
    for m in maps {
        let start = m.offset;
        let end = m.offset + m.rows * m.cols * 2;
        let overlaps = filtered.iter().any(|f| {
            let f_start = f.offset;
            let f_end = f.offset + f.rows * f.cols * 2;
            start.max(f_start) < end.min(f_end)
        });
        if !overlaps {
            filtered.push(m);
        }
    }

    filtered.sort_by_key(|m| m.offset);
    filtered
}

/// Scans with little-endian 16-bit words.
pub fn scan_rom_16bit_le(
    rom: &[u8],
    candidate_widths: &[usize],
    min_rows: usize,
    max_rows: usize,
) -> Vec<DiscoveredMap> {
    scan_rom_16bit(rom, candidate_widths, min_rows, max_rows, true)
}

/// Scans with big-endian 16-bit words.
pub fn scan_rom_16bit_be(
    rom: &[u8],
    candidate_widths: &[usize],
    min_rows: usize,
    max_rows: usize,
) -> Vec<DiscoveredMap> {
    scan_rom_16bit(rom, candidate_widths, min_rows, max_rows, false)
}

/// Evaluates one candidate window, returning a map when it passes the gates.
fn evaluate_window(
    rom: &[u8],
    offset: usize,
    h: usize,
    w: usize,
    is_little_endian: bool,
) -> Option<DiscoveredMap> {
    let mut u_mat = vec![vec![0.0f64; w]; h];
    let mut i_mat = vec![vec![0.0f64; w]; h];
    let mut flat_u = Vec::with_capacity(w * h);

    for (r, u_row) in u_mat.iter_mut().enumerate() {
        for (c, u_cell) in u_row.iter_mut().enumerate() {
            let idx = offset + (r * w + c) * 2;
            let pair = [rom[idx], rom[idx + 1]];
            let u_val = if is_little_endian {
                u16::from_le_bytes(pair) as f64
            } else {
                u16::from_be_bytes(pair) as f64
            };
            let i_val = if is_little_endian {
                i16::from_le_bytes([pair[0], pair[1]]) as f64
            } else {
                i16::from_be_bytes([pair[0], pair[1]]) as f64
            };
            *u_cell = u_val;
            i_mat[r][c] = i_val;
            flat_u.push(u_val);
        }
    }

    let u_smooth = smoothness(&u_mat);
    let i_smooth = smoothness(&i_mat);

    let (is_signed, best_smooth, chosen) = if i_smooth < u_smooth * 0.75
        && flat_u.iter().any(|&v| v > 60000.0)
    {
        (true, i_smooth, &i_mat)
    } else {
        (false, u_smooth, &u_mat)
    };

    if best_smooth >= 0.38 {
        return None;
    }

    let r_w = autocorrelation(&flat_u, w);
    if r_w <= 0.40 {
        return None;
    }

    let score = (r_w * 100.0) / (best_smooth + 0.05);

    let vmin = chosen
        .iter()
        .flat_map(|r| r.iter())
        .copied()
        .fold(f64::INFINITY, f64::min);
    let vmax = chosen
        .iter()
        .flat_map(|r| r.iter())
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);

    let (name, category, unit) = if is_signed && vmin < 0.0 && vmax <= 650.0 {
        (
            "Ignition Timing Advance Map".to_string(),
            "Ignition".to_string(),
            "Degrees BTDC".to_string(),
        )
    } else if !is_signed && vmin >= 400.0 && vmax >= 6000.0 {
        (
            "Base Fuel Injection Table".to_string(),
            "Fuel".to_string(),
            "Microseconds (us)".to_string(),
        )
    } else {
        (
            format!("Calibration Matrix {}x{}", h, w),
            "Generic Map".to_string(),
            "RAW".to_string(),
        )
    };

    Some(DiscoveredMap {
        offset,
        rows: h,
        cols: w,
        is_signed,
        is_little_endian,
        score,
        smoothness: best_smooth,
        val_min: vmin,
        val_max: vmax,
        suggested_name: name,
        category,
        unit,
    })
}
