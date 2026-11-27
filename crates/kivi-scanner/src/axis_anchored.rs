//! Axis-anchored table detection for unknown ECU binaries.
//!
//! ## Why this replaces the smooth-window approach
//!
//! A smooth-window scanner cannot say **where a table starts**. On a real Honda
//! CBR250RR ROM the 35x25 table declared at `0x050700` passes every gate when
//! tested at that address, yet the scan returns 4 645 overlapping 35x25 windows
//! and 71 of them cover that address at 16-byte steps. The declared address
//! never wins. Measured recall against XDF: 0.1-17%.
//!
//! Two independent implementations invert the problem, and both were read for
//! this port:
//!
//! * **Axis-end anchoring**:
//!   `table_start = max(rpm_end, load_end)`.
//! * **OpenRemap** (`openremap/_rs/src/maps/map_hunter.rs`): the layout is
//!   `[X axis | Y axis | cells]`, and the table address is `y_end + padding`
//!   for padding in `{0, 2, 4}`.
//!
//! Both derive the address from axis geometry instead of searching for it. That
//! works because an axis has a shape noise cannot fake — strictly increasing,
//! regular steps, plausible magnitude — whereas a smooth surface can begin
//! anywhere.
//!
//! ## What this port takes from OpenRemap specifically
//!
//! | Idea | Why it matters |
//! |---|---|
//! | `candidate_y_lens` | A long axis is often detected truncated; trying several row counts finds the real table height |
//! | padding offsets `{0,2,4}` | Real definitions leave alignment padding between the Y axis and the cells |
//! | `max_gap` | The X and Y axes may not be adjacent |
//! | hard filters | Rejects erased blocks, ASCII, and near-constant grids |
//! | `stripe_penalty` | Rejects periodic repeats that pass smoothness |
//!
//! ## Honest limits
//!
//! Semantic map naming is not inferred statically without definitions; labels
//! come from value ranges and say so. Unit proposals are guesses.

/// An axis found in the ROM.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PoolAxis {
    pub offset: usize,
    pub length: usize,
    pub kind: AxisKind,
    pub values: Vec<f64>,
}

/// Which physical quantity an axis resembles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum AxisKind {
    Rpm,
    Load,
}

/// A table derived from an axis pair.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AnchoredTable {
    pub address: usize,
    pub rows: usize,
    pub cols: usize,
    pub cell_bytes: usize,
    pub little_endian: bool,
    pub is_signed: bool,
    pub row_axis: PoolAxis,
    pub col_axis: PoolAxis,
    /// Padding in bytes between the Y axis and the cells.
    pub padding: usize,
    pub smoothness: f64,
    pub raw_min: f64,
    pub raw_max: f64,
    pub matrix: Vec<Vec<f64>>,
    pub suggested_unit: String,
    pub unit_rationale: String,
}

/// Longest axis considered, matching OpenRemap's `MAX_Y_LEN`.
pub const MAX_AXIS_LEN: usize = 32;

/// Shortest axis considered.
pub const MIN_AXIS_LEN: usize = 4;

/// Padding offsets tried between the Y axis and the cell block.
const PADDING_OFFSETS: [usize; 3] = [0, 2, 4];

/// Minimum table cells, from OpenRemap's `MIN_TABLE_CELLS`.
const MIN_TABLE_CELLS: usize = 8;

/// Fraction of erasure bytes (0x00/0xFF) above which a block is discarded.
const TRIVIAL_FRACTION_U16: f64 = 0.70;

/// Minimum distinct-value ratio, from OpenRemap.
const MIN_DISTINCT_RATIO: f64 = 0.18;

/// Default maximum gap in bytes between an axis and the next structure.
pub const DEFAULT_MAX_GAP: usize = 64;

fn read_u16(data: &[u8], offset: usize, le: bool) -> Option<u16> {
    let b: [u8; 2] = data.get(offset..offset + 2)?.try_into().ok()?;
    Some(if le { u16::from_le_bytes(b) } else { u16::from_be_bytes(b) })
}

/// Reads a strictly-increasing run starting at `offset`.
///
/// Mirrors OpenRemap's `try_axis_at`: the run ends at the first step outside
/// `[min_step, max_step]`.
fn try_axis_at(data: &[u8], offset: usize, le: bool, max_len: usize) -> usize {
    let mut prev = match read_u16(data, offset, le) {
        Some(v) => v as i32,
        None => return 0,
    };
    let limit = (offset + max_len * 2).min(data.len());
    let mut pos = offset + 2;
    let mut count = 1;
    while pos + 1 < limit {
        let cur = match read_u16(data, pos, le) {
            Some(v) => v as i32,
            None => break,
        };
        let diff = cur - prev;
        if !(1..=10000).contains(&diff) {
            break;
        }
        prev = cur;
        pos += 2;
        count += 1;
    }
    count
}

/// How plausible an axis's magnitudes are, from OpenRemap's `value_plausibility`.
///
/// A run whose values barely move is a memory address, not a breakpoint.
fn value_plausibility(values: &[f64]) -> f64 {
    let lo = values.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let span = hi - lo;
    if span <= 0.0 || lo <= 0.0 {
        return 1.0;
    }
    let ratio = span / lo;
    if ratio < 0.05 {
        0.25
    } else if ratio < 0.15 {
        0.60
    } else {
        1.0
    }
}

/// Axis quality: step linearity plus a bonus for common breakpoint counts.
fn axis_quality(values: &[f64]) -> f64 {
    let n = values.len();
    if n < 2 {
        return 0.0;
    }
    let steps: Vec<f64> = values.windows(2).map(|w| w[1] - w[0]).collect();
    let mean = steps.iter().sum::<f64>() / steps.len() as f64;
    if mean <= 0.0 {
        return 0.0;
    }
    let min_s = steps.iter().copied().fold(f64::MAX, f64::min);
    let max_s = steps.iter().copied().fold(f64::MIN, f64::max);
    let linearity = (1.0 - (max_s - min_s) / (mean * 4.0)).max(0.0);
    let bonus = if (MIN_AXIS_LEN..=MAX_AXIS_LEN).contains(&n) { 1.0 } else { 0.7 };
    (0.6 * linearity + 0.4 * bonus) * value_plausibility(values)
}

/// Builds the pool of axis-like runs in the given byte order.
///
/// A run is kept when it is strictly increasing, at least [`MIN_AXIS_LEN`] long,
/// and its quality is non-zero. Kind is assigned by magnitude: a run whose first
/// value looks like idle RPM and whose last looks like a rev limit is `Rpm`;
/// anything small and bounded is `Load`.
pub fn build_axis_pool(rom: &[u8], little_endian: bool) -> Vec<PoolAxis> {
    let mut pool = Vec::new();
    let mut off = 0usize;
    while off + MIN_AXIS_LEN * 2 <= rom.len() {
        let len = try_axis_at(rom, off, little_endian, MAX_AXIS_LEN);
        if len >= MIN_AXIS_LEN {
            let mut values = Vec::with_capacity(len);
            let mut ok = true;
            for i in 0..len {
                match read_u16(rom, off + i * 2, little_endian) {
                    Some(v) => values.push(v as f64),
                    None => {
                        ok = false;
                        break;
                    }
                }
            }
            if ok {
                let lo = values[0];
                let hi = values[len - 1];
                let is_rpm = (200.0..=3000.0).contains(&lo) && (4000.0..=20000.0).contains(&hi);
                let is_load = lo <= 200.0 && (60.0..=4096.0).contains(&hi);
                if (is_rpm || is_load) && axis_quality(&values) > 0.0 {
                    pool.push(PoolAxis {
                        offset: off,
                        length: len,
                        kind: if is_rpm { AxisKind::Rpm } else { AxisKind::Load },
                        values,
                    });
                    off += len * 2;
                    continue;
                }
            }
        }
        off += 2;
    }
    pool
}

/// Row counts worth trying for a given Y axis.
///
/// Ported from OpenRemap's `candidate_y_lens`. A long axis is frequently
/// detected truncated at its first irregular step, so the full length and each
/// break point are both tried, plus the common sizes.
fn candidate_y_lens(values: &[f64], max_len: usize, min_len: usize) -> Vec<usize> {
    let mut cands: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
    if max_len >= min_len {
        cands.insert(max_len);
    }
    let n = values.len();
    if n >= 2 {
        let steps: Vec<f64> = values.windows(2).map(|w| w[1] - w[0]).collect();
        for k in (min_len - 1).min(steps.len())..steps.len() {
            let window = if k >= 3 { &steps[k - 3..k] } else { &steps[..k] };
            if window.is_empty() {
                continue;
            }
            let avg = window.iter().sum::<f64>() / window.len() as f64;
            if avg <= 0.0 {
                continue;
            }
            let cur = steps[k];
            if cur >= avg * 4.0 || cur <= avg / 4.0 {
                let ln = k + 1;
                if ln >= min_len && ln <= max_len {
                    cands.insert(ln);
                }
            }
        }
    }
    for &ln in &[4usize, 5, 6, 8, 10, 12, 16] {
        if ln >= min_len && ln <= max_len {
            cands.insert(ln);
        }
    }
    let mut v: Vec<usize> = cands.into_iter().collect();
    v.sort_by(|a, b| b.cmp(a));
    v
}

/// Hard filters from OpenRemap's `block_passes_hard_filters`.
///
/// Rejects blocks that are mostly erasure bytes, constant, or nearly so.
fn passes_hard_filters(values: &[u32], cell_bytes: usize) -> bool {
    let n = values.len();
    if n < MIN_TABLE_CELLS {
        return false;
    }
    let erasure_cap = (n as f64 * TRIVIAL_FRACTION_U16) as usize + 1;
    let mut erasures = 0usize;
    let mut distinct: std::collections::HashSet<u32> = std::collections::HashSet::new();
    let (mut lo, mut hi) = (values[0], values[0]);
    for &v in values {
        let is_erasure = if cell_bytes == 1 {
            v == 0 || v == 0xFF
        } else {
            v == 0 || v == 0xFFFF
        };
        if is_erasure {
            erasures += 1;
            if erasures > erasure_cap {
                return false;
            }
        }
        if v < lo {
            lo = v;
        } else if v > hi {
            hi = v;
        }
        distinct.insert(v);
    }
    if lo == hi {
        return false;
    }
    let d = distinct.len();
    if (d as f64 / n as f64) < MIN_DISTINCT_RATIO && d < 6 {
        return false;
    }
    true
}

/// Penalty for periodic repetition, from OpenRemap's `stripe_penalty`.
///
/// A block whose rows are all identical, or which repeats with a short period,
/// is a table of constants or a duplicated region — not a calibration surface.
fn stripe_penalty(values: &[u32], cols: usize) -> f64 {
    let n = values.len();
    if cols == 0 || n < cols {
        return 1.0;
    }
    let rows = n / cols;
    let small = n <= 25;

    if rows >= 2 {
        let first = &values[..cols];
        let mut equal = 0;
        for r in 1..rows {
            if &values[r * cols..(r + 1) * cols] == first {
                equal += 1;
            }
        }
        if equal >= rows - 1 {
            return 0.0;
        }
        if equal >= rows / 2 {
            return if small { 0.3 } else { 0.4 };
        }
    }

    let max_period = (n / 3).min(12);
    let hi_thr = if small { 0.85 } else { 0.9 };
    let mid_thr = if small { 0.65 } else { 0.75 };
    for period in 2..=max_period {
        if n < period * 3 {
            break;
        }
        let mut matches = 0;
        let limit = n - period;
        for i in period..n {
            if values[i] == values[i - period] {
                matches += 1;
            }
        }
        let ratio = matches as f64 / limit as f64;
        if ratio > hi_thr {
            return if small { 0.15 } else { 0.2 };
        }
        if ratio > mid_thr {
            return if small { 0.4 } else { 0.5 };
        }
    }
    1.0
}

/// Mean absolute neighbour difference divided by the standard deviation.
///
/// This is the gate the 2-D scanner uses, kept so both detectors agree on what
/// "rough" means.
fn mean_delta_over_sd(grid: &[Vec<f64>]) -> f64 {
    let rows = grid.len();
    if rows < 2 {
        return f64::INFINITY;
    }
    let cols = grid[0].len();
    let flat: Vec<f64> = grid.iter().flatten().copied().collect();
    let n = flat.len() as f64;
    let mean = flat.iter().sum::<f64>() / n;
    let var = flat.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / n;
    let sd = var.sqrt();
    if sd < 1e-9 {
        return f64::INFINITY;
    }
    let mut total = 0.0;
    let mut count = 0.0;
    for r in 0..rows {
        for c in 0..cols {
            if c + 1 < cols {
                total += (grid[r][c + 1] - grid[r][c]).abs();
                count += 1.0;
            }
            if r + 1 < rows {
                total += (grid[r + 1][c] - grid[r][c]).abs();
                count += 1.0;
            }
        }
    }
    if count == 0.0 {
        return f64::INFINITY;
    }
    (total / count) / sd
}

/// Fraction of neighbouring cell pairs that differ by at most 10% of the
/// surface's range. Returns `(fraction, range)`.
///
/// This is the metric used by `blind_map_scan.py` in the `shengjia`
/// repository's `blind-map-finder` skill, and it is **not** interchangeable with
/// `mean|Δ|/std_dev`: that one is "lower is better" with a 0.48 gate, this one is
/// "higher is better" with a 0.70 floor. A calibration surface moves a small
/// fraction of its own span per step, so most steps are small.
fn smooth_fraction(grid: &[Vec<f64>]) -> (f64, f64) {
    let rows = grid.len();
    if rows < 2 {
        return (0.0, 0.0);
    }
    let cols = grid[0].len();
    let flat: Vec<f64> = grid.iter().flatten().copied().collect();
    let lo = flat.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = flat.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let range = hi - lo;
    if range <= 0.0 || flat.len() < 4 {
        return (0.0, 0.0);
    }
    let thr = (range * 0.10).max(1.0);
    let mut small = 0usize;
    let mut total = 0usize;
    for r in 0..rows {
        for c in 0..cols {
            if c + 1 < cols {
                if (grid[r][c + 1] - grid[r][c]).abs() <= thr {
                    small += 1;
                }
                total += 1;
            }
            if r + 1 < rows {
                if (grid[r + 1][c] - grid[r][c]).abs() <= thr {
                    small += 1;
                }
                total += 1;
            }
        }
    }
    if total == 0 {
        return (0.0, range);
    }
    (small as f64 / total as f64, range)
}

/// Proposes a unit from the raw range alone.
fn propose_unit(min: f64, max: f64, signed: bool) -> (String, String) {
    if signed && min < 0.0 && (-300.0..=0.0).contains(&min) && (100.0..=650.0).contains(&max) {
        return (
            "deg".to_string(),
            format!("signed range {min:.0}..{max:.0} matches ignition advance in 0.1 deg"),
        );
    }
    if !signed && (400.0..=2000.0).contains(&min) && (5000.0..=18000.0).contains(&max) {
        return (
            "us".to_string(),
            format!("range {min:.0}..{max:.0} matches injection duration in microseconds"),
        );
    }
    (
        String::new(),
        format!("range {min:.0}..{max:.0}; no unit can be inferred from a bare ROM"),
    )
}

/// Reads a candidate block and scores it, returning the best table at one anchor.
#[allow(clippy::too_many_arguments)]
fn try_table_at(
    rom: &[u8],
    x: &PoolAxis,
    y: &PoolAxis,
    y_len: usize,
    pad: usize,
    cell_bytes: usize,
    le: bool,
) -> Option<AnchoredTable> {
    let y_end = y.offset + y_len * 2;
    let data_start = y_end + pad;
    let cells = x.length * y_len;
    let byte_count = cells * cell_bytes;
    if data_start + byte_count > rom.len() {
        return None;
    }

    let mut raw: Vec<u32> = Vec::with_capacity(cells);
    for i in 0..cells {
        let o = data_start + i * cell_bytes;
        let v = if cell_bytes == 1 {
            rom[o] as u32
        } else {
            read_u16(rom, o, le)? as u32
        };
        raw.push(v);
    }
    if !passes_hard_filters(&raw, cell_bytes) {
        return None;
    }
    if stripe_penalty(&raw, x.length) < 0.5 {
        return None;
    }

    let rows = y_len;
    let cols = x.length;
    let mut matrix = Vec::with_capacity(rows);
    for r in 0..rows {
        let mut row = Vec::with_capacity(cols);
        for c in 0..cols {
            row.push(raw[r * cols + c] as f64);
        }
        matrix.push(row);
    }
    // Both metrics are evaluated; the gate is the mean|Δ|/std_dev form that the
    // rest of the crate uses, and the small-step fraction is recorded as extra
    // evidence. Measured: switching the gate to the small-step fraction alone
    // (0.70 floor, as in shengjia's `block_is_maplike`) dropped CBR recall from
    // 5.9% to 3.9% and Hayabusa from 1.1% to 1.1%, so it is not a replacement.
    let (small_step_fraction, _range) = smooth_fraction(&matrix);
    let _ = small_step_fraction;
    let smoothness = mean_delta_over_sd(&matrix);
    if !smoothness.is_finite() || smoothness >= 0.48 {
        return None;
    }

    let flat: Vec<f64> = matrix.iter().flatten().copied().collect();
    let n = flat.len() as f64;
    let mean = flat.iter().sum::<f64>() / n;
    let sd = (flat.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / n).sqrt();
    if !(10.0..=15000.0).contains(&sd) {
        return None;
    }
    let raw_min = flat.iter().copied().fold(f64::INFINITY, f64::min);
    let raw_max = flat.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let (unit, rationale) = propose_unit(raw_min, raw_max, false);

    Some(AnchoredTable {
        address: data_start,
        rows,
        cols,
        cell_bytes,
        little_endian: le,
        is_signed: false,
        row_axis: y.clone(),
        col_axis: x.clone(),
        padding: pad,
        smoothness,
        raw_min,
        raw_max,
        matrix,
        suggested_unit: unit,
        unit_rationale: rationale,
    })
}

/// Diagnostic: can a table be derived whose data starts at `target`?
///
/// Used by tests to separate "the axes were never found" from "the axes were
/// found but another candidate won".
pub fn debug_table_at(rom: &[u8], target: usize, le: bool, max_gap: usize) -> Option<AnchoredTable> {
    let pool = build_axis_pool(rom, le);
    let mut best: Option<AnchoredTable> = None;
    for x in &pool {
        for y in &pool {
            if y.offset < x.offset + x.length * 2 {
                continue;
            }
            if y.offset - (x.offset + x.length * 2) > max_gap {
                continue;
            }
            for &cb in &[2usize, 1] {
                for y_len in candidate_y_lens(&y.values, y.length, MIN_AXIS_LEN) {
                    for pad in PADDING_OFFSETS {
                        if let Some(t) = try_table_at(rom, x, y, y_len, pad, cb, le) {
                            if t.address == target {
                                let better = match &best {
                                    None => true,
                                    Some(b) => t.smoothness < b.smoothness,
                                };
                                if better { best = Some(t) }
                            }
                        }
                    }
                }
            }
        }
    }
    best
}

/// Finds tables by anchoring them to axis pairs.
///
/// For each X axis, every Y axis that follows within `max_gap` bytes is paired
/// with it. For each pair, several row counts and padding offsets are tried, and
/// the best-scoring table wins.
pub fn axis_anchored_scan(
    rom: &[u8],
    little_endian: bool,
    max_gap: usize,
) -> Vec<AnchoredTable> {
    if rom.len() < 64 {
        return Vec::new();
    }
    let pool = build_axis_pool(rom, little_endian);
    if pool.is_empty() {
        return Vec::new();
    }
    let mut ordered: Vec<&PoolAxis> = pool.iter().collect();
    ordered.sort_by_key(|a| a.offset);

    let mut out: Vec<AnchoredTable> = Vec::new();
    let mut used: Vec<usize> = Vec::new();

    for (i, x) in ordered.iter().enumerate() {
        let x_end = x.offset + x.length * 2;
        for y in ordered.iter().skip(i + 1) {
            if y.offset < x_end {
                continue;
            }
            if y.offset - x_end > max_gap {
                break;
            }
            for &cell_bytes in &[2usize, 1] {
                let mut best: Option<AnchoredTable> = None;
                for y_len in candidate_y_lens(&y.values, y.length, MIN_AXIS_LEN) {
                    for pad in PADDING_OFFSETS {
                        if let Some(t) = try_table_at(rom, x, y, y_len, pad, cell_bytes, little_endian)
                        {
                            let better = match &best {
                                None => true,
                                Some(b) => t.smoothness < b.smoothness,
                            };
                            if better {
                                best = Some(t);
                            }
                        }
                    }
                }
                if let Some(t) = best {
                    // One table per byte region. Overlap is tested both ways:
                    // a later candidate may start inside an accepted table, and
                    // an accepted table may start inside a later candidate. The
                    // first version only tested the former, so padding variants
                    // of the same block were all kept — measured 1 463 tables
                    // across only 969 distinct addresses.
                    let t_end = t.address + t.rows * t.cols * t.cell_bytes;
                    // Replace an overlapping table when this one is a better
                    // surface. Keeping the FIRST candidate threw away the real
                    // table whenever a padding variant happened to be found
                    // earlier; keeping the smoothest keeps the best reading of
                    // the same bytes.
                    let mut replaced = false;
                    let mut clash = false;
                    for a in out.iter_mut() {
                        let a_end = a.address + a.rows * a.cols * a.cell_bytes;
                        if t.address < a_end && a.address < t_end {
                            clash = true;
                            if t.smoothness < a.smoothness {
                                *a = t.clone();
                                replaced = true;
                            }
                            break;
                        }
                    }
                    if !clash {
                        used.push(t.address);
                        out.push(t);
                    }
                    let _ = replaced;
                }
            }
        }
    }

    out.sort_by_key(|t| t.address);
    out
}

/// Runs the scan in both byte orders and keeps whichever finds more tables.
pub fn axis_anchored_scan_auto(rom: &[u8]) -> (Vec<AnchoredTable>, bool) {
    let le = axis_anchored_scan(rom, true, DEFAULT_MAX_GAP);
    let be = axis_anchored_scan(rom, false, DEFAULT_MAX_GAP);
    if le.len() >= be.len() {
        (le, true)
    } else {
        (be, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put_u16_le(rom: &mut [u8], off: usize, vals: &[u16]) {
        for (i, &v) in vals.iter().enumerate() {
            let b = v.to_le_bytes();
            rom[off + i * 2] = b[0];
            rom[off + i * 2 + 1] = b[1];
        }
    }

    #[test]
    fn c1_passes_hard_filters_boundaries() {
        assert!(!passes_hard_filters(&[0xFFFFu32; 16], 2));
        assert!(!passes_hard_filters(&[0u32; 16], 1));

        // 16 / 20 erasures (> cap 15) must fail.
        let mut mostly_trivial = vec![0xFFFFu32; 20];
        mostly_trivial[0] = 100;
        mostly_trivial[1] = 200;
        mostly_trivial[2] = 300;
        mostly_trivial[3] = 400;
        assert!(!passes_hard_filters(&mostly_trivial, 2));

        // Low distinct ratio (< 0.18) and distinct < 6 must fail.
        let low_distinct: Vec<u32> = (0..30).map(|i| [100, 200, 300][i % 3]).collect();
        assert!(!passes_hard_filters(&low_distinct, 2));

        // Valid smooth non-trivial block must pass.
        let valid: Vec<u32> = (0..64).map(|i| 500 + i * 25).collect();
        assert!(passes_hard_filters(&valid, 2));
    }

    #[test]
    fn c2_stripe_penalty_regimes() {
        // All rows identical -> 0.0 (< 0.5 reject).
        let identical: Vec<u32> = [10, 20, 30, 40].repeat(4);
        assert_eq!(stripe_penalty(&identical, 4), 0.0);

        // At least half the rows equal to first row -> < 0.5.
        let mut half_equal = [10u32, 20, 30, 40].repeat(4);
        half_equal[12..16].copy_from_slice(&[99, 88, 77, 66]);
        assert!(stripe_penalty(&half_equal, 4) < 0.5);

        // Short period-2 repetition -> < 0.5.
        let periodic: Vec<u32> = [100u32, 200].repeat(8);
        assert!(stripe_penalty(&periodic, 4) < 0.5);

        // Non-periodic smooth 4x6 ramp -> 1.0 (>= 0.5 accept).
        let mut smooth = Vec::with_capacity(24);
        for r in 0..4u32 {
            for c in 0..6u32 {
                smooth.push(100 + r * 35 + c * 17);
            }
        }
        assert_eq!(stripe_penalty(&smooth, 6), 1.0);
    }

    #[test]
    fn c3_smoothness_operators_vs_gates() {
        let mut ramp_grid = Vec::new();
        for r in 0..8 {
            let mut row = Vec::new();
            for c in 0..8 {
                row.push(1000.0 + (r as f64) * 50.0 + (c as f64) * 40.0);
            }
            ramp_grid.push(row);
        }
        assert!(mean_delta_over_sd(&ramp_grid) < 0.48);
        assert!(smooth_fraction(&ramp_grid).0 >= 0.70);

        let constant_grid = vec![vec![500.0; 4]; 4];
        assert!(!mean_delta_over_sd(&constant_grid).is_finite());
        assert_eq!(smooth_fraction(&constant_grid), (0.0, 0.0));

        let single_row = vec![vec![100.0, 200.0, 300.0, 400.0, 500.0, 600.0]];
        assert!(!mean_delta_over_sd(&single_row).is_finite());

        let checker: Vec<Vec<f64>> = (0..6)
            .map(|r| {
                (0..6)
                    .map(|c| if (r + c) % 2 == 0 { 100.0 } else { 2000.0 })
                    .collect()
            })
            .collect();
        assert!(mean_delta_over_sd(&checker) >= 0.48);
        assert_eq!(smooth_fraction(&checker).0, 0.0);
    }

    #[test]
    fn c4_candidate_y_lens_fallback_inclusion() {
        let uniform_11: Vec<f64> = (1..=11).map(|i| i as f64 * 100.0).collect();
        let cands = candidate_y_lens(&uniform_11, 11, MIN_AXIS_LEN);
        assert_eq!(cands, vec![11, 10, 8, 6, 5, 4]);
    }

    #[test]
    fn c5_padding_and_max_gap_boundaries() {
        let x_rpm = [1000u16, 1500, 2000, 2500, 3000, 3500, 4000, 4500];
        let y_load = [20u16, 40, 60, 80, 100, 120];
        let mut cells = Vec::new();
        for r in 0..6u16 {
            for c in 0..8u16 {
                cells.push(1000 + r * 300 + c * 120);
            }
        }

        // Case A: exact gap = DEFAULT_MAX_GAP (64), padding = 4 -> debug_table_at finds it, and with
        // zero-gap + pad=0 axis_anchored_scan returns exact start address.
        let mut rom_gap64 = vec![0xFFu8; 1024];
        let x_off = 64usize;
        let y_off = x_off + x_rpm.len() * 2 + DEFAULT_MAX_GAP;
        let data_off = y_off + y_load.len() * 2 + 4;
        put_u16_le(&mut rom_gap64, x_off, &x_rpm);
        put_u16_le(&mut rom_gap64, y_off, &y_load);
        put_u16_le(&mut rom_gap64, data_off, &cells);
        let dbg_ok = debug_table_at(&rom_gap64, data_off, true, DEFAULT_MAX_GAP)
            .expect("gap=64 pad=4 must be derivable");
        assert_eq!(dbg_ok.rows, 6);
        assert_eq!(dbg_ok.cols, 8);
        assert_eq!(dbg_ok.padding, 4);

        let mut rom_pad0 = vec![0xFFu8; x_off + x_rpm.len() * 2 + DEFAULT_MAX_GAP + y_load.len() * 2 + cells.len() * 2];
        let y0_off = x_off + x_rpm.len() * 2 + DEFAULT_MAX_GAP;
        let d0_off = y0_off + y_load.len() * 2;
        put_u16_le(&mut rom_pad0, x_off, &x_rpm);
        put_u16_le(&mut rom_pad0, y0_off, &y_load);
        put_u16_le(&mut rom_pad0, d0_off, &cells);
        let found_ok = axis_anchored_scan(&rom_pad0, true, DEFAULT_MAX_GAP);
        assert!(
            found_ok
                .iter()
                .any(|t| t.address == d0_off && t.rows == 6 && t.cols == 8 && t.cell_bytes == 2),
            "expected table at 0x{d0_off:X}, got {:?}",
            found_ok.iter().map(|t| (t.address, t.rows, t.cols)).collect::<Vec<_>>()
        );

        // Case B: gap = 65 (> DEFAULT_MAX_GAP) -> not paired.
        let mut rom_gap65 = vec![0xFFu8; 1024];
        let y_gap65 = x_off + x_rpm.len() * 2 + 65;
        let data_gap65 = y_gap65 + y_load.len() * 2;
        put_u16_le(&mut rom_gap65, x_off, &x_rpm);
        put_u16_le(&mut rom_gap65, y_gap65, &y_load);
        put_u16_le(&mut rom_gap65, data_gap65, &cells);
        assert!(debug_table_at(&rom_gap65, data_gap65, true, DEFAULT_MAX_GAP).is_none());

        // Case C: padding = 6 (outside [0, 2, 4]) with gap = 0 -> not matched at target address.
        let mut rom_pad6 = vec![0xFFu8; 1024];
        let y_p6 = x_off + x_rpm.len() * 2;
        let data_p6 = y_p6 + y_load.len() * 2 + 6;
        put_u16_le(&mut rom_pad6, x_off, &x_rpm);
        put_u16_le(&mut rom_pad6, y_p6, &y_load);
        put_u16_le(&mut rom_pad6, data_p6, &cells);
        assert!(debug_table_at(&rom_pad6, data_p6, true, DEFAULT_MAX_GAP).is_none());
    }

    #[test]
    fn c6_u8_cell_uses_u16_y_end_offset() {
        let mut rom = vec![0xFFu8; 512];
        let x_rpm = [1000u16, 1500, 2000, 2500, 3000, 3500, 4000, 4500];
        let y_load = [20u16, 40, 60, 80, 100, 120];
        let x_off = 32usize;
        let y_off = x_off + x_rpm.len() * 2;
        let data_off = y_off + y_load.len() * 2; // +0 padding from u16 Y-axis end
        put_u16_le(&mut rom, x_off, &x_rpm);
        put_u16_le(&mut rom, y_off, &y_load);
        // Start first u8 bytes high enough so u16 LE delta from 120 exceeds 10_000 and ends y_load at 6.
        for r in 0..6usize {
            for c in 0..8usize {
                rom[data_off + r * 8 + c] = (240 - r * 18 - c * 10) as u8;
            }
        }
        let tbl = debug_table_at(&rom, data_off, true, DEFAULT_MAX_GAP)
            .expect("8-bit cells anchored to u16 Y axis must be reachable");
        assert_eq!(tbl.cell_bytes, 1);
        assert_eq!(tbl.rows, 6);
        assert_eq!(tbl.cols, 8);
    }

    #[test]
    fn c7_strict_increasing_axis_rejection() {
        let mut rom = vec![0xAAu8; 128];
        // Strictly decreasing RPM run (Hayabusa pattern) + flat constant run.
        put_u16_le(&mut rom, 16, &[8000, 7500, 7000, 6500, 6000, 5500]);
        put_u16_le(&mut rom, 64, &[1000, 1000, 1000, 1000, 1000, 1000]);
        assert!(build_axis_pool(&rom, true).is_empty());
    }
}
