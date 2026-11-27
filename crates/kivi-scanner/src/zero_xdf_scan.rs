//! Zero-XDF Z3: 16-bit table scanning.
//!
//! Ported from `ecu_engine_rust::recognition::zero_xdf_discovery::ZeroXdfEngine`
//! (`check_candidate_16bit_integer`, `calc_ac_and_smoothness_16bit`,
//! `scan_16bit_tables`) plus the `DiscoveredFullTable` / `DiscoveredAxis` /
//! `Endianness` / `MapDataBits` / `SemanticMapType` types.
//!
//! ## Scanning pipeline (preserved)
//!
//! 1. Restrict the scan to the pre-filter's kept ranges, intersected with the
//!    caller's window.
//! 2. For each candidate `(rows, cols)` walk the range in 2-byte steps.
//! 3. Reject with the row-delta pre-check (first 3 rows only, max delta 4500,
//!    sum bounded by `cols · check_rows · 600`).
//! 4. Reject if autocorrelation at lag `cols` is below `0.35`.
//! 5. Reject if smoothness is `>= 0.48`.
//! 6. Confidence is `(1 - min(smooth, 0.9)) · ac`; on acceptance the cursor
//!    advances by 16 bytes instead of 2 to avoid reporting near-duplicates.
//!
//! ## Deviation from the source
//!
//! The source parallelises the outer loop with `rayon::par_iter`. This port is
//! **sequential**. Results are identical because each `(rows, cols)` produces an
//! independent vector that is concatenated in input order; parallelism only
//! reorders the *work*, not the output. Not adding `rayon` keeps determinism
//! trivial to assert and avoids a dependency for a loop that is not the
//! bottleneck here.
//!
//! ## Scope
//!
//! Z3 only. Axis pairing, tier classification and XDF export (Z4–Z5) are **not**
//! in this module. `classify_semantics` and `calculate_quality_score` live in
//! `heuristic_score` (migrated in Wave 4) and are applied by callers.

use crate::zero_xdf_prefilter::analyze_rom_blocks;

/// Byte order of a candidate table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Endianness {
    /// LoHi — Infineon TriCore, Intel, ARM.
    LittleEndian,
    /// HiLo — Renesas SuperH, Motorola.
    BigEndian,
}

/// Element width of a discovered table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MapDataBits {
    /// 8-bit cells.
    Bits8,
    /// 16-bit cells in the given byte order.
    Bits16(Endianness),
    /// 32-bit cells in the given byte order.
    Bits32(Endianness),
}

/// Heuristic semantic guess for a table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SemanticMapType {
    /// Retards under load, advances with RPM.
    IgnitionAdvance,
    /// Rises with load and RPM.
    FuelAirChargeVE,
    /// High plateau with a shallow gradient.
    BoostTorqueLimiter,
    /// Narrow stoichiometric plateau.
    TargetLambdaAFR,
    /// Monotonic calibration curve.
    SensorCalibration,
    /// Not yet classified.
    UnknownTable,
}

/// A discovered axis.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DiscoveredAxis {
    /// Axis address in the ROM.
    pub address: usize,
    /// Breakpoint count.
    pub points_count: usize,
    /// Whether the axis stores 16-bit values.
    pub is_16bit: bool,
    /// Axis byte order.
    pub endianness: Endianness,
    /// Suggested unit.
    pub estimated_unit: String,
    /// Smallest breakpoint.
    pub min_val: f64,
    /// Largest breakpoint.
    pub max_val: f64,
    /// Breakpoint values.
    pub values: Vec<f64>,
}

/// A table found by the scanner.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DiscoveredFullTable {
    /// Base address of the cell data.
    pub z_address: usize,
    /// Row count.
    pub rows: usize,
    /// Column count.
    pub cols: usize,
    /// Element width and byte order.
    pub data_format: MapDataBits,
    /// Heuristic semantic guess.
    pub semantic_type: SemanticMapType,
    /// Confidence in `0.0..=1.0`.
    pub confidence_score: f64,
    /// Surface roughness.
    pub smoothness: f64,
    /// X axis, when paired.
    pub x_axis: Option<DiscoveredAxis>,
    /// Y axis, when paired.
    pub y_axis: Option<DiscoveredAxis>,
    /// Sample values.
    pub sample_matrix: Vec<Vec<f64>>,
}

/// Row-delta pre-check: rejects blocks with abrupt adjacent jumps.
///
/// Only the first `min(rows, 3)` rows are examined. A single delta above `4500`
/// rejects immediately; otherwise the total must stay within
/// `cols · check_rows · 600`.
pub fn check_candidate_16bit_integer(
    slice: &[u8],
    rows: usize,
    cols: usize,
    endianness: Endianness,
) -> bool {
    let check_rows = rows.min(3);
    let is_le = endianness == Endianness::LittleEndian;
    let mut sum_diff = 0i32;

    for r in 0..check_rows {
        let r_off = r * cols * 2;
        let mut prev = read_word(slice, r_off, is_le);
        for c in 1..cols {
            let idx = r_off + c * 2;
            let curr = read_word(slice, idx, is_le);
            let diff = (curr - prev).abs();
            if diff > 4500 {
                return false;
            }
            sum_diff += diff;
            prev = curr;
        }
    }
    sum_diff <= (cols * check_rows * 600) as i32
}

/// Computes autocorrelation and smoothness, returning `None` when a gate fails.
///
/// Autocorrelation is measured at lag `cols`; it must reach `0.35`. Smoothness
/// must stay below `0.48`.
pub fn calc_ac_and_smoothness_16bit(
    slice: &[u8],
    rows: usize,
    cols: usize,
    endianness: Endianness,
) -> Option<(f64, f64)> {
    let total_cells = rows * cols;
    let is_le = endianness == Endianness::LittleEndian;

    // Single pass: decode every word once into a stack buffer, then compute all
    // statistics from it. The previous version re-read each word four times
    // (mean, variance, dx, dy), which for a 50x30 table meant 6000 reads per
    // candidate offset. Words are decoded once here.
    //
    // The buffer is fixed-size so this stays allocation-free in the hot loop;
    // the largest shape the scanner considers is bounded by MAX_TABLE_CELLS.
    const MAX_TABLE_CELLS: usize = 64 * 64;
    if total_cells > MAX_TABLE_CELLS {
        return None;
    }
    let mut words = [0u16; MAX_TABLE_CELLS];
    let mut sum_val = 0.0f64;
    for (i, slot) in words.iter_mut().take(total_cells).enumerate() {
        let w = read_word(slice, i * 2, is_le) as u16;
        *slot = w;
        sum_val += w as f64;
    }
    let mean_val = sum_val / total_cells as f64;

    // Autocorrelation gate. Computed in one pass with no re-scan: a per-row
    // early abort would need the remaining energy, and recomputing that per row
    // costs more than it saves (measured: 1.6 s -> 12 s).
    let mut denom = 0.0f64;
    let mut num = 0.0f64;
    for i in 0..total_cells {
        let diff = words[i] as f64 - mean_val;
        denom += diff * diff;
        if i + cols < total_cells {
            num += diff * (words[i + cols] as f64 - mean_val);
        }
    }

    let ac = if denom < 1e-12 { 0.0 } else { num / denom };
    if ac < 0.35 {
        return None;
    }

    // Early abort: smoothness is (mean_dx + mean_dy) / std_dev, and the gate
    // accepts only smooth < 0.48. Since both means are non-negative and
    // std_dev is fixed by now, once the running sum of absolute deltas exceeds
    // `0.48 * std_dev * (number of deltas)` the final value cannot come back
    // under the gate. Checking every row keeps the abort conservative: it can
    // only fire when the result is already decided.
    let std_dev = (denom / total_cells as f64).sqrt() + 1e-5;
    let gate = 0.48;
    let dx_count = (rows * (cols - 1)) as f64;
    let dy_count = ((rows - 1) * cols) as f64;
    let total_deltas = dx_count + dy_count;
    if total_deltas <= 0.0 {
        return None;
    }
    // The bound on the SUM of both means, expressed as one budget.
    let delta_budget = gate * std_dev * total_deltas;

    let mut sum_dx = 0.0f64;
    for r in 0..rows {
        let base = r * cols;
        for c in 0..cols - 1 {
            sum_dx += (words[base + c + 1] as f64 - words[base + c] as f64).abs();
        }
        // Per-row budget: after row r, the dx sum for r+1 rows can only grow,
        // so if it already exceeds the full budget the result is decided.
        if sum_dx > delta_budget {
            return None;
        }
    }
    let mut sum_dy = 0.0f64;
    for r in 0..rows - 1 {
        let base = r * cols;
        let next = base + cols;
        for c in 0..cols {
            sum_dy += (words[next + c] as f64 - words[base + c] as f64).abs();
        }
        if sum_dx + sum_dy > delta_budget {
            return None;
        }
    }

    let mean_dx = sum_dx / dx_count;
    let mean_dy = sum_dy / dy_count;
    let smooth = (mean_dx + mean_dy) / std_dev;

    if smooth < 0.48 {
        Some((ac, smooth))
    } else {
        None
    }
}

/// Autocorrelation and smoothness over already-decoded words.
///
/// Identical arithmetic to [`calc_ac_and_smoothness_16bit`], but reading a
/// pre-decoded word slice instead of decoding bytes. The scanner calls the byte
/// version ~920 000 times per megabyte ROM with windows that overlap by all but
/// one cell, so decoding is repeated work; measured at 976 ms of a 1 367 ms scan.
///
/// `words` is the whole kept range decoded once, and `start` is the first cell
/// of the window inside it.
fn calc_ac_and_smoothness_words(
    words: &[u16],
    start: usize,
    rows: usize,
    cols: usize,
) -> Option<(f64, f64)> {
    let total_cells = rows * cols;
    let end = start.checked_add(total_cells)?;
    if end > words.len() || total_cells == 0 {
        return None;
    }
    let w = &words[start..end];

    let mut sum_val = 0.0f64;
    for &v in w {
        sum_val += v as f64;
    }
    let mean_val = sum_val / total_cells as f64;

    let mut denom = 0.0f64;
    let mut num = 0.0f64;
    for i in 0..total_cells {
        let diff = w[i] as f64 - mean_val;
        denom += diff * diff;
        if i + cols < total_cells {
            num += diff * (w[i + cols] as f64 - mean_val);
        }
    }

    let ac = if denom < 1e-12 { 0.0 } else { num / denom };
    if ac < 0.35 {
        return None;
    }

    let std_dev = (denom / total_cells as f64).sqrt() + 1e-5;
    let gate = 0.48;
    let dx_count = (rows * (cols - 1)) as f64;
    let dy_count = ((rows - 1) * cols) as f64;
    let total_deltas = dx_count + dy_count;
    if total_deltas <= 0.0 {
        return None;
    }
    let delta_budget = gate * std_dev * total_deltas;

    let mut sum_dx = 0.0f64;
    for r in 0..rows {
        let base = r * cols;
        for c in 0..cols - 1 {
            sum_dx += (w[base + c + 1] as f64 - w[base + c] as f64).abs();
        }
        if sum_dx > delta_budget {
            return None;
        }
    }
    let mut sum_dy = 0.0f64;
    for r in 0..rows - 1 {
        let base = r * cols;
        let next = base + cols;
        for c in 0..cols {
            sum_dy += (w[next + c] as f64 - w[base + c] as f64).abs();
        }
        if sum_dx + sum_dy > delta_budget {
            return None;
        }
    }

    let mean_dx = sum_dx / dx_count;
    let mean_dy = sum_dy / dy_count;
    let smooth = (mean_dx + mean_dy) / std_dev;

    if smooth < 0.48 {
        Some((ac, smooth))
    } else {
        None
    }
}

struct ExactWordPrefix {
    words: Vec<u16>,
    p_sum: Vec<i64>,
    p_sq: Vec<u64>,
    p_abs: Vec<u64>,
    last_bad: Vec<i32>,
}

impl ExactWordPrefix {
    fn build_16(rom: &[u8], start: usize, end: usize, is_le: bool) -> Self {
        let cells = end.saturating_sub(start) / 2;
        let mut words = Vec::with_capacity(cells);
        let mut p_sum = Vec::with_capacity(cells + 1);
        let mut p_sq = Vec::with_capacity(cells + 1);
        let mut p_abs = Vec::with_capacity(cells + 1);
        let mut last_bad = Vec::with_capacity(cells.saturating_sub(1));
        p_sum.push(0);
        p_sq.push(0);
        p_abs.push(0);
        let mut acc_sum = 0i64;
        let mut acc_sq = 0u64;
        let mut acc_abs = 0u64;
        let mut cur_bad = -1i32;

        for i in 0..cells {
            let o = start + i * 2;
            let w = if is_le {
                u16::from_le_bytes([rom[o], rom[o + 1]])
            } else {
                u16::from_be_bytes([rom[o], rom[o + 1]])
            };
            words.push(w);
            let wi = w as i64;
            acc_sum += wi;
            acc_sq += (wi * wi) as u64;
            p_sum.push(acc_sum);
            p_sq.push(acc_sq);
            if i > 0 {
                let d = (w as i32 - words[i - 1] as i32).abs();
                if d > 4500 {
                    cur_bad = (i - 1) as i32;
                }
                acc_abs += d as u64;
                p_abs.push(acc_abs);
                last_bad.push(cur_bad);
            }
        }
        if cells > 0 {
            p_abs.push(acc_abs);
        }
        Self { words, p_sum, p_sq, p_abs, last_bad }
    }

    #[inline]
    fn row0_bad_jump(&self, cell: usize, cols: usize) -> Option<usize> {
        let lb = *self.last_bad.get(cell + cols - 2)?;
        if lb >= cell as i32 {
            Some(lb as usize + 1)
        } else {
            None
        }
    }

    #[inline]
    fn pass_precheck(&self, cell: usize, rows: usize, cols: usize) -> bool {
        let check_rows = rows.min(3);
        let mut sum_diff = 0u64;
        for r in 0..check_rows {
            let r_cell = cell + r * cols;
            if self.last_bad[r_cell + cols - 2] >= r_cell as i32 {
                return false;
            }
            sum_diff += self.p_abs[r_cell + cols - 1] - self.p_abs[r_cell];
        }
        sum_diff <= (cols * check_rows * 600) as u64
    }

    #[inline]
    fn dx_under_budget_conservative(&self, cell: usize, rows: usize, cols: usize) -> bool {
        let n = rows * cols;
        let s = (self.p_sum[cell + n] - self.p_sum[cell]) as i128;
        let sq = self.p_sq[cell + n] - self.p_sq[cell];
        let var_num = (sq as i128 * n as i128) - s * s;
        let denom_f = if var_num <= 0 { 0.0 } else { var_num as f64 / n as f64 };
        if denom_f < 1e-12 {
            return false;
        }
        let std_dev = (denom_f / n as f64).sqrt() + 1e-5;
        let total_deltas = (rows * (cols - 1) + (rows - 1) * cols) as f64;
        let delta_budget = 0.48 * std_dev * total_deltas;
        let mut sum_dx = 0u64;
        for r in 0..rows {
            let r_cell = cell + r * cols;
            sum_dx += self.p_abs[r_cell + cols - 1] - self.p_abs[r_cell];
        }
        (sum_dx as f64) <= delta_budget * 1.01
    }
}

/// Scans `rom` for 16-bit tables of the given dimensions.
///
/// `scan_start`/`scan_end` further restrict the pre-filter's kept ranges.
/// Returns candidates ordered by dimension then address.
pub fn scan_16bit_tables(
    rom: &[u8],
    candidate_dims: &[(usize, usize)],
    endianness: Endianness,
    scan_start: usize,
    scan_end: usize,
) -> Vec<DiscoveredFullTable> {
    use rayon::prelude::*;

    let prefilter = analyze_rom_blocks(rom);
    let scan_ranges: Vec<(usize, usize)> = prefilter
        .kept_ranges
        .into_iter()
        .filter_map(|(s, e)| {
            let start = s.max(scan_start);
            let end = e.min(scan_end);
            if start < end { Some((start, end)) } else { None }
        })
        .collect();

    let is_le = endianness == Endianness::LittleEndian;
    let range_prefixes: Vec<(usize, usize, ExactWordPrefix)> = scan_ranges
        .iter()
        .filter_map(|&(start, end)| {
            if (end - start) / 2 == 0 {
                return None;
            }
            Some((start, end, ExactWordPrefix::build_16(rom, start, end, is_le)))
        })
        .collect();

    let per_dim: Vec<Vec<DiscoveredFullTable>> = candidate_dims
        .par_iter()
        .map(|&(rows, cols)| {
            let mut local = Vec::new();
            if rows < 2 || cols < 2 || rows * cols > 64 * 64 {
                return local;
            }
            let cells = rows * cols;
            let byte_len = cells * 2;
            for (start, end, pref) in &range_prefixes {
                let limit_cell = end.saturating_sub(byte_len).saturating_sub(*start) / 2;
                if *start + byte_len > *end {
                    continue;
                }
                let mut cell = 0usize;
                while cell <= limit_cell {
                    if let Some(next_c) = pref.row0_bad_jump(cell, cols) {
                        cell = next_c;
                        continue;
                    }
                    if !pref.pass_precheck(cell, rows, cols)
                        || !pref.dx_under_budget_conservative(cell, rows, cols)
                    {
                        cell += 1;
                        continue;
                    }
                    if let Some((ac, smooth)) =
                        calc_ac_and_smoothness_words(&pref.words, cell, rows, cols)
                    {
                        let mut matrix = Vec::with_capacity(rows);
                        for r in 0..rows {
                            let base = cell + r * cols;
                            matrix.push(
                                pref.words[base..base + cols]
                                    .iter()
                                    .map(|&v| v as f64)
                                    .collect(),
                            );
                        }
                        if crate::map_quality::GridFilter::default()
                            .admits(&matrix, crate::map_quality::BitWidth::Bits16)
                        {
                            local.push(DiscoveredFullTable {
                                z_address: *start + cell * 2,
                                rows,
                                cols,
                                data_format: MapDataBits::Bits16(endianness),
                                semantic_type: SemanticMapType::UnknownTable,
                                confidence_score: (1.0 - smooth.min(0.9)) * ac,
                                smoothness: smooth,
                                x_axis: None,
                                y_axis: None,
                                sample_matrix: matrix,
                            });
                        }
                        cell += 8;
                        continue;
                    }
                    cell += 1;
                }
            }
            local
        })
        .collect();

    per_dim.into_iter().flatten().collect()
}

/// Reads one big- or little-endian word as `i32` (the pre-check works on i32).
fn read_word(slice: &[u8], idx: usize, is_le: bool) -> i32 {
    if is_le {
        u16::from_le_bytes([slice[idx], slice[idx + 1]]) as i32
    } else {
        u16::from_be_bytes([slice[idx], slice[idx + 1]]) as i32
    }
}

/// Scans for 16-bit tables, choosing the byte order that fits best per window.
///
/// Both endiannesses are evaluated; the one with the lower smoothness wins. When
/// only one passes, that one is used. This mirrors the source's
/// `scan_16bit_tables_auto_endian`.
pub fn scan_16bit_tables_auto_endian(
    rom: &[u8],
    candidate_dims: &[(usize, usize)],
    scan_start: usize,
    scan_end: usize,
) -> Vec<DiscoveredFullTable> {
    use rayon::prelude::*;

    let prefilter = analyze_rom_blocks(rom);
    let scan_ranges: Vec<(usize, usize)> = prefilter
        .kept_ranges
        .into_iter()
        .filter_map(|(s, e)| {
            let start = s.max(scan_start);
            let end = e.min(scan_end);
            if start < end { Some((start, end)) } else { None }
        })
        .collect();

    let range_prefixes: Vec<(usize, usize, ExactWordPrefix, ExactWordPrefix)> = scan_ranges
        .iter()
        .filter_map(|&(start, end)| {
            if (end - start) / 2 == 0 {
                return None;
            }
            Some((
                start,
                end,
                ExactWordPrefix::build_16(rom, start, end, true),
                ExactWordPrefix::build_16(rom, start, end, false),
            ))
        })
        .collect();

    let per_dim: Vec<Vec<DiscoveredFullTable>> = candidate_dims
        .par_iter()
        .map(|&(rows, cols)| {
            let mut local = Vec::new();
            if rows < 2 || cols < 2 {
                return local;
            }
            let cells = rows * cols;
            let byte_len = cells * 2;

            for (start, end, pref_le, pref_be) in &range_prefixes {
                if *start + byte_len > *end {
                    continue;
                }
                let limit_cell = end.saturating_sub(byte_len).saturating_sub(*start) / 2;
                let mut cell = 0usize;
                while cell <= limit_cell {
                    if let (Some(j_le), Some(j_be)) = (
                        pref_le.row0_bad_jump(cell, cols),
                        pref_be.row0_bad_jump(cell, cols),
                    ) {
                        cell = j_le.min(j_be);
                        continue;
                    }
                    let pass_le = pref_le.pass_precheck(cell, rows, cols)
                        && pref_le.dx_under_budget_conservative(cell, rows, cols);
                    let pass_be = pref_be.pass_precheck(cell, rows, cols)
                        && pref_be.dx_under_budget_conservative(cell, rows, cols);
                    if !pass_le && !pass_be {
                        cell += 1;
                        continue;
                    }

                    let res_le = if pass_le {
                        calc_ac_and_smoothness_words(&pref_le.words, cell, rows, cols)
                    } else {
                        None
                    };
                    let res_be = if pass_be {
                        calc_ac_and_smoothness_words(&pref_be.words, cell, rows, cols)
                    } else {
                        None
                    };

                    let chosen = match (res_le, res_be) {
                        (Some((ac_le, sm_le)), Some((ac_be, sm_be))) => {
                            if sm_le <= sm_be {
                                Some((Endianness::LittleEndian, ac_le, sm_le))
                            } else {
                                Some((Endianness::BigEndian, ac_be, sm_be))
                            }
                        }
                        (Some((ac, sm)), None) => Some((Endianness::LittleEndian, ac, sm)),
                        (None, Some((ac, sm))) => Some((Endianness::BigEndian, ac, sm)),
                        (None, None) => None,
                    };

                    if let Some((endian, ac, smooth)) = chosen {
                        let words = match endian {
                            Endianness::LittleEndian => &pref_le.words,
                            Endianness::BigEndian => &pref_be.words,
                        };
                        let mut matrix = Vec::with_capacity(rows);
                        for r in 0..rows {
                            let base = cell + r * cols;
                            matrix.push(
                                words[base..base + cols]
                                    .iter()
                                    .map(|&v| v as f64)
                                    .collect(),
                            );
                        }

                        local.push(DiscoveredFullTable {
                            z_address: *start + cell * 2,
                            rows,
                            cols,
                            data_format: MapDataBits::Bits16(endian),
                            semantic_type: SemanticMapType::UnknownTable,
                            confidence_score: (1.0 - smooth.min(0.9)) * ac,
                            smoothness: smooth,
                            x_axis: None,
                            y_axis: None,
                            sample_matrix: matrix,
                        });
                        cell += 8;
                        continue;
                    }
                    cell += 1;
                }
            }
            local
        })
        .collect();

    per_dim.into_iter().flatten().collect()
}

/// Suppresses candidates overlapping a higher-scoring accepted table.
///
/// `overlap_threshold` is the fraction of the candidate's byte span that must
/// overlap an accepted table before it is dropped.
/// Build the composite-score inputs from a discovered table.
fn composite_score_of(t: &DiscoveredFullTable) -> f64 {
    let first_row_is_flat = t
        .sample_matrix
        .first()
        .map(|row| {
            let lo = row.iter().copied().fold(f64::INFINITY, f64::min);
            let hi = row.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            (hi - lo).abs() < 1e-4
        })
        .unwrap_or(false);

    // Contiguous means the Y axis ends where X begins and X ends where Z begins.
    let axes_contiguous = match (&t.x_axis, &t.y_axis) {
        (Some(x), Some(y)) => {
            let x_size = t.cols * if x.is_16bit { 2 } else { 1 };
            let y_size = t.rows * if y.is_16bit { 2 } else { 1 };
            x.address + x_size == t.z_address && y.address + y_size == x.address
        }
        _ => false,
    };

    crate::nms_score::composite_quality_score(&crate::nms_score::ScoreInputs {
        rows: t.rows,
        cols: t.cols,
        z_address: t.z_address,
        smoothness: t.smoothness,
        confidence_score: t.confidence_score,
        has_x_axis: t.x_axis.is_some(),
        has_y_axis: t.y_axis.is_some(),
        axes_contiguous,
        first_row_is_flat,
    })
}

pub fn apply_memory_interval_nms(
    tables: Vec<DiscoveredFullTable>,
    overlap_threshold: f64,
) -> Vec<DiscoveredFullTable> {
    if tables.is_empty() {
        return tables;
    }
    // Precompute the COMPOSITE score once per table instead of inside sort_by
    // comparisons; sorting order preserves ties just like slice::sort_by.
    let mut scored: Vec<(f64, DiscoveredFullTable)> = tables
        .into_iter()
        .map(|t| (composite_score_of(&t), t))
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    let bytes_per_elem = |t: &DiscoveredFullTable| match t.data_format {
        MapDataBits::Bits8 => 1usize,
        MapDataBits::Bits16(_) => 2,
        MapDataBits::Bits32(_) => 4,
    };

    if overlap_threshold == 0.0 {
        let max_end = scored
            .iter()
            .map(|(_, t)| t.z_address.saturating_add(t.rows * t.cols * bytes_per_elem(t)))
            .max()
            .unwrap_or(0);
        let mut occupied = vec![false; max_end];
        let mut accepted: Vec<DiscoveredFullTable> = Vec::with_capacity(scored.len());
        for (_, cand) in scored {
            let cand_len = cand.rows * cand.cols * bytes_per_elem(&cand);
            if cand_len == 0 {
                accepted.push(cand);
                continue;
            }
            let cand_start = cand.z_address;
            let cand_end = cand_start + cand_len;
            if !occupied[cand_start..cand_end].iter().any(|&b| b) {
                occupied[cand_start..cand_end].fill(true);
                accepted.push(cand);
            }
        }
        return accepted;
    }

    let mut accepted: Vec<DiscoveredFullTable> = Vec::with_capacity(scored.len());
    for (_, cand) in scored {
        let cand_len = cand.rows * cand.cols * bytes_per_elem(&cand);
        let cand_start = cand.z_address;
        let cand_end = cand_start + cand_len;

        let suppressed = accepted.iter().any(|acc| {
            let acc_len = acc.rows * acc.cols * bytes_per_elem(acc);
            let acc_start = acc.z_address;
            let acc_end = acc_start + acc_len;
            let overlap = cand_end.min(acc_end).saturating_sub(cand_start.max(acc_start));
            cand_len > 0 && (overlap as f64 / cand_len as f64) > overlap_threshold
        });

        if !suppressed {
            accepted.push(cand);
        }
    }
    accepted
}

// ---------------------------------------------------------------------------
// Monotonicity and section detection
// ---------------------------------------------------------------------------

/// True when a series is ordered and has enough distinct steps to be a real
/// breakpoint axis rather than a repeated constant.
///
/// Requires at least 4 points, an ordered direction (ascending or descending
/// overall), and at least 60% of steps being non-trivial.
pub fn is_monotonic(slice: &[f64]) -> bool {
    if slice.len() < 4 {
        return false;
    }
    let mut inc = 0;
    let mut dec = 0;
    let mut unique_steps = 0;
    for i in 0..slice.len() - 1 {
        let diff = (slice[i + 1] - slice[i]).abs();
        if diff > 1e-4 {
            unique_steps += 1;
        }
        if slice[i + 1] >= slice[i] {
            inc += 1;
        }
        if slice[i + 1] <= slice[i] {
            dec += 1;
        }
    }
    let is_ordered = (inc >= slice.len() - 2
        && slice.last().unwrap() > slice.first().unwrap())
        || (dec >= slice.len() - 2 && slice.first().unwrap() > slice.last().unwrap());
    is_ordered && unique_steps >= (slice.len() * 6) / 10
}

/// Alias of [`is_monotonic`], matching the source's second entry point.
pub fn is_monotonic_increasing(slice: &[f64]) -> bool {
    is_monotonic(slice)
}

/// Returns the calibration sections that survive the block pre-filter.
pub fn detect_calibration_sections(rom: &[u8]) -> Vec<(usize, usize)> {
    analyze_rom_blocks(rom).kept_ranges
}

// ---------------------------------------------------------------------------
// 8-bit scanning (ported from the same source module)
// ---------------------------------------------------------------------------

/// Row-delta pre-check for 8-bit cells: max adjacent delta 45, sum bound
/// `cols · check_rows · 18`, first three rows only.
pub fn check_candidate_8bit_integer(slice: &[u8], rows: usize, cols: usize) -> bool {
    let check_rows = rows.min(3);
    let mut sum_diff = 0i32;
    for r in 0..check_rows {
        let r_off = r * cols;
        let mut prev = slice[r_off] as i32;
        for c in 1..cols {
            let curr = slice[r_off + c] as i32;
            let diff = (curr - prev).abs();
            if diff > 45 {
                return false;
            }
            sum_diff += diff;
            prev = curr;
        }
    }
    sum_diff <= (cols * check_rows * 18) as i32
}

/// Autocorrelation and smoothness for 8-bit cells.
///
/// Gates: `ac >= 0.30` and `smooth < 0.48`.
pub fn calc_ac_and_smoothness_8bit(
    slice: &[u8],
    rows: usize,
    cols: usize,
) -> Option<(f64, f64)> {
    let total_cells = rows * cols;
    let mut sum_val = 0.0f64;
    for &b in slice {
        sum_val += b as f64;
    }
    let mean_val = sum_val / total_cells as f64;

    let mut denom = 0.0f64;
    let mut num = 0.0f64;
    for i in 0..total_cells {
        let diff = slice[i] as f64 - mean_val;
        denom += diff * diff;
        if i + cols < total_cells {
            num += diff * (slice[i + cols] as f64 - mean_val);
        }
    }

    let ac = if denom < 1e-12 { 0.0 } else { num / denom };
    if ac < 0.30 {
        return None;
    }

    let mut sum_dx = 0.0f64;
    let mut sum_dy = 0.0f64;
    for r in 0..rows {
        let r_off = r * cols;
        for c in 0..cols - 1 {
            sum_dx += (slice[r_off + c + 1] as f64 - slice[r_off + c] as f64).abs();
        }
    }
    for r in 0..rows - 1 {
        let r_off1 = r * cols;
        let r_off2 = (r + 1) * cols;
        for c in 0..cols {
            sum_dy += (slice[r_off2 + c] as f64 - slice[r_off1 + c] as f64).abs();
        }
    }

    let std_dev = (denom / total_cells as f64).sqrt() + 1e-5;
    let mean_dx = sum_dx / (rows * (cols - 1)) as f64;
    let mean_dy = sum_dy / ((rows - 1) * cols) as f64;
    let smooth = (mean_dx + mean_dy) / std_dev;

    if smooth < 0.48 {
        Some((ac, smooth))
    } else {
        None
    }
}

/// Scans `rom` for 8-bit tables of the given dimensions.
struct ExactBytePrefix {
    p_sum: Vec<i64>,
    p_sq: Vec<u64>,
    p_abs: Vec<u64>,
    last_bad: Vec<i32>,
}

impl ExactBytePrefix {
    fn build(bytes: &[u8]) -> Self {
        let n = bytes.len();
        let mut p_sum = Vec::with_capacity(n + 1);
        let mut p_sq = Vec::with_capacity(n + 1);
        let mut p_abs = Vec::with_capacity(n + 1);
        let mut last_bad = Vec::with_capacity(n.saturating_sub(1));
        p_sum.push(0);
        p_sq.push(0);
        p_abs.push(0);
        let mut acc_sum = 0i64;
        let mut acc_sq = 0u64;
        let mut acc_abs = 0u64;
        let mut cur_bad = -1i32;

        for i in 0..n {
            let v = bytes[i] as i64;
            acc_sum += v;
            acc_sq += (v * v) as u64;
            p_sum.push(acc_sum);
            p_sq.push(acc_sq);
            if i > 0 {
                let d = (bytes[i] as i32 - bytes[i - 1] as i32).abs();
                if d > 45 {
                    cur_bad = (i - 1) as i32;
                }
                acc_abs += d as u64;
                p_abs.push(acc_abs);
                last_bad.push(cur_bad);
            }
        }
        if n > 0 {
            p_abs.push(acc_abs);
        }
        Self { p_sum, p_sq, p_abs, last_bad }
    }

    #[inline]
    fn row0_bad_jump(&self, cell: usize, cols: usize) -> Option<usize> {
        let lb = *self.last_bad.get(cell + cols - 2)?;
        if lb >= cell as i32 { Some(lb as usize + 1) } else { None }
    }

    #[inline]
    fn pass_precheck(&self, cell: usize, rows: usize, cols: usize) -> bool {
        let check_rows = rows.min(3);
        let mut sum_diff = 0u64;
        for r in 0..check_rows {
            let r_cell = cell + r * cols;
            if self.last_bad[r_cell + cols - 2] >= r_cell as i32 {
                return false;
            }
            sum_diff += self.p_abs[r_cell + cols - 1] - self.p_abs[r_cell];
        }
        sum_diff <= (cols * check_rows * 18) as u64
    }

    #[inline]
    fn dx_under_budget_conservative(&self, cell: usize, rows: usize, cols: usize) -> bool {
        let n = rows * cols;
        let s = (self.p_sum[cell + n] - self.p_sum[cell]) as i128;
        let sq = self.p_sq[cell + n] - self.p_sq[cell];
        let var_num = (sq as i128 * n as i128) - s * s;
        let denom_f = if var_num <= 0 { 0.0 } else { var_num as f64 / n as f64 };
        if denom_f < 1e-12 {
            return false;
        }
        let std_dev = (denom_f / n as f64).sqrt() + 1e-5;
        let dx_count = (rows * (cols - 1)) as f64;
        let mut sum_dx = 0u64;
        for r in 0..rows {
            let r_cell = cell + r * cols;
            sum_dx += self.p_abs[r_cell + cols - 1] - self.p_abs[r_cell];
        }
        (sum_dx as f64) <= 0.48 * std_dev * dx_count * 1.01
    }
}

pub fn scan_8bit_tables(
    rom: &[u8],
    candidate_dims: &[(usize, usize)],
    scan_start: usize,
    scan_end: usize,
) -> Vec<DiscoveredFullTable> {
    use rayon::prelude::*;

    let prefilter = analyze_rom_blocks(rom);
    let scan_ranges: Vec<(usize, usize)> = prefilter
        .kept_ranges
        .into_iter()
        .filter_map(|(s, e)| {
            let start = s.max(scan_start);
            let end = e.min(scan_end);
            if start < end { Some((start, end)) } else { None }
        })
        .collect();

    let range_prefixes: Vec<(usize, usize, ExactBytePrefix)> = scan_ranges
        .iter()
        .map(|&(start, end)| (start, end, ExactBytePrefix::build(&rom[start..end])))
        .collect();

    let per_dim: Vec<Vec<DiscoveredFullTable>> = candidate_dims
        .par_iter()
        .map(|&(rows, cols)| {
            let mut local = Vec::new();
            if rows < 2 || cols < 2 {
                return local;
            }
            let byte_len = rows * cols;
            for (start, end, pref) in &range_prefixes {
                if *start + byte_len > *end {
                    continue;
                }
                let limit_cell = end.saturating_sub(byte_len).saturating_sub(*start);
                let mut cell = 0usize;
                while cell <= limit_cell {
                    if let Some(next_c) = pref.row0_bad_jump(cell, cols) {
                        cell = next_c;
                        continue;
                    }
                    if !pref.pass_precheck(cell, rows, cols)
                        || !pref.dx_under_budget_conservative(cell, rows, cols)
                    {
                        cell += 1;
                        continue;
                    }
                    let offset = *start + cell;
                    let slice = &rom[offset..offset + byte_len];
                    if let Some((ac, smooth)) = calc_ac_and_smoothness_8bit(slice, rows, cols) {
                        let mut matrix = Vec::with_capacity(rows);
                        for r in 0..rows {
                            let mut row_vals = Vec::with_capacity(cols);
                            for c in 0..cols {
                                row_vals.push(slice[r * cols + c] as f64);
                            }
                            matrix.push(row_vals);
                        }
                        local.push(DiscoveredFullTable {
                            z_address: offset,
                            rows,
                            cols,
                            data_format: MapDataBits::Bits8,
                            semantic_type: SemanticMapType::UnknownTable,
                            confidence_score: (1.0 - smooth.min(0.9)) * ac,
                            smoothness: smooth,
                            x_axis: None,
                            y_axis: None,
                            sample_matrix: matrix,
                        });
                        cell += 8;
                        continue;
                    }
                    cell += 1;
                }
            }
            local
        })
        .collect();

    per_dim.into_iter().flatten().collect()
}

/// Splits tables into Tier-1 (high confidence, both axes, core semantic type)
/// and Tier-2 (everything else that survived the scan).
pub fn categorize_into_tiers(
    tables: &[DiscoveredFullTable],
) -> (Vec<DiscoveredFullTable>, Vec<DiscoveredFullTable>) {
    let mut tier1 = Vec::new();
    let mut tier2 = Vec::new();

    for tbl in tables {
        let is_high_confidence = tbl.confidence_score >= 0.85;
        let has_both_axes = tbl.x_axis.is_some() && tbl.y_axis.is_some();
        let is_core_type = matches!(
            tbl.semantic_type,
            SemanticMapType::IgnitionAdvance
                | SemanticMapType::FuelAirChargeVE
                | SemanticMapType::BoostTorqueLimiter
                | SemanticMapType::TargetLambdaAFR
        );

        if is_high_confidence && has_both_axes && is_core_type {
            tier1.push(tbl.clone());
        } else {
            tier2.push(tbl.clone());
        }
    }
    (tier1, tier2)
}

/// Votes on the ROM's global byte order from vector-table and smoothness cues.
///
/// Falls back to little-endian when the ROM is too short or no cue fires,
/// matching the source.
pub fn detect_global_endianness(rom: &[u8]) -> Endianness {
    if rom.len() < 16 {
        return Endianness::LittleEndian;
    }

    // Architecture vector table: e.g. Denso SH7052.
    if rom[0] == 0x00 && rom[1] == 0x00 && (0x01..=0x40).contains(&rom[2]) && rom[3] == 0x00 {
        let be_sp = u32::from_be_bytes([rom[4], rom[5], rom[6], rom[7]]);
        if be_sp >= 0xFF00_0000 || (0x0000_1000..=0x0200_0000).contains(&be_sp) {
            return Endianness::BigEndian;
        }
    }

    // Keihin 38770 header.
    if rom.len() >= 8 && rom[0] == 0x00 && rom[1] == 0x5A && rom[2] == 0x00 && rom[3] == 0x5A {
        return Endianness::BigEndian;
    }

    // Smoothness vote over sampled windows.
    let sample_rows = 8usize;
    let sample_cols = 16usize;
    let byte_len = sample_rows * sample_cols * 2;
    let mut le_votes = 0usize;
    let mut be_votes = 0usize;

    let mut offset = 0usize;
    while offset + byte_len <= rom.len() {
        let slice = &rom[offset..offset + byte_len];
        let pass_le = check_candidate_16bit_integer(slice, sample_rows, sample_cols, Endianness::LittleEndian);
        let pass_be = check_candidate_16bit_integer(slice, sample_rows, sample_cols, Endianness::BigEndian);
        if pass_le || pass_be {
            let res_le = if pass_le {
                calc_ac_and_smoothness_16bit(slice, sample_rows, sample_cols, Endianness::LittleEndian)
            } else {
                None
            };
            let res_be = if pass_be {
                calc_ac_and_smoothness_16bit(slice, sample_rows, sample_cols, Endianness::BigEndian)
            } else {
                None
            };
            match (res_le, res_be) {
                (Some((_, sm_le)), Some((_, sm_be))) => {
                    if sm_le < sm_be * 0.75 {
                        le_votes += 1;
                    } else if sm_be < sm_le * 0.75 {
                        be_votes += 1;
                    }
                }
                (Some(_), None) => le_votes += 1,
                (None, Some(_)) => be_votes += 1,
                (None, None) => {}
            }
        }
        offset += 512;
        if (le_votes + be_votes) >= 30 {
            break;
        }
    }

    if be_votes > le_votes {
        Endianness::BigEndian
    } else {
        Endianness::LittleEndian
    }
}
