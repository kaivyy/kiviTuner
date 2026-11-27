//! Blind detection of 1-D calibration curves.
//!
//! ## Why this exists
//!
//! In the 647-pair BIN+XDF catalogue, **68.1%** of the multi-cell tables the
//! XDFs declare are 1-D curves (`250 300` of `367 590` rows), not 2-D maps. The
//! scanner was only ever asked for 2-D shapes, so those rows could never match
//! no matter how good the 2-D detector became. Measuring them together hid that.
//!
//! ## What separates a curve from an axis
//!
//! Both are smooth runs of adjacent words. The difference is monotonicity:
//!
//! | Object | Shape of the run |
//! |---|---|
//! | Axis breakpoints | strictly monotonic, near-uniform steps |
//! | Curve values | smooth, but turns around (a bell, a ramp-and-plateau) |
//!
//! A run is a **curve** when it is smooth *and* has at least one interior
//! turning point. That test is what keeps [`crate::axis_detector`] and this
//! module from claiming the same bytes.
//!
//! ## Gates
//!
//! 1. **Turning point**: at least one sign change between consecutive non-zero
//!    deltas. A monotonic ramp is an axis and is rejected here. Zero deltas
//!    inherit the previous sign, so a curve that plateaus at its peak still
//!    turns exactly once.
//! 2. **Smoothness**: `mean|Δ| / std_dev < 0.48`, the same constant the 2-D
//!    scanner uses. Noise has deltas comparable to its own range, so this
//!    rejects it; a real curve moves a small fraction of its span per step.
//! 3. **Distinct values**: at least a quarter of the points must differ from
//!    their predecessor, which rejects long constant plateaus and two-value
//!    alternating blocks.
//! 4. **Bounded by change**: the cells just outside the window must differ from
//!    the cells just inside it, so a window cannot start inside padding and
//!    swallow the real curve that follows.
//!
//! ## Cost
//!
//! One decode pass per kept range, then O(1) per candidate length via prefix
//! sums. On a 1 MB ROM that is 693 ms for 16-bit plus 8-bit scanning, versus
//! 9 508 ms for the naive re-decode version.
//!
//! ## Scope
//!
//! Detection only. Axis pairing and XDF export are not in this module.

use crate::zero_xdf_prefilter::analyze_rom_blocks;

/// Cell byte order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Endian {
    Little,
    Big,
}

/// A detected 1-D curve.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DiscoveredCurve {
    /// Byte address of the first cell.
    pub address: usize,
    /// Number of cells.
    pub points: usize,
    /// Cell width in bytes (1 or 2).
    pub cell_bytes: usize,
    /// Cell byte order.
    pub endian: Endian,
    /// Smallest cell value.
    pub min_value: f64,
    /// Largest cell value.
    pub max_value: f64,
    /// Number of interior local extrema.
    pub turning_points: usize,
    /// Surface roughness, `mean|Δ| / std_dev`.
    pub smoothness: f64,
    /// Decoded cell values.
    pub values: Vec<f64>,
}

/// Controls for [`scan_curves`].
#[derive(Debug, Clone, PartialEq)]
pub struct CurveScanOptions {
    /// Shortest curve to consider, in cells.
    pub min_points: usize,
    /// Longest curve to consider, in cells.
    pub max_points: usize,
    /// Cell byte order for 16-bit cells.
    pub endian: Endian,
    /// Whether to also look for 8-bit curves.
    pub include_8bit: bool,
    /// Whether a **monotonic** run may be reported as a curve.
    ///
    /// Default `false`, because a monotonic run is what
    /// [`crate::axis_detector`] reports as breakpoints, and reporting the same
    /// bytes as both an axis and a curve would duplicate every axis.
    ///
    /// Set `true` when the caller has already accounted for axes, or when the
    /// goal is table recall rather than axis discovery. Measured: of the 1-D
    /// tables two real definitions declare, **90% (Hayabusa) and 96% (BMW B58)
    /// are monotonic**, so leaving this off makes the majority of real 1-D
    /// calibration curves unreachable.
    pub allow_monotonic: bool,
    /// Whether to snap each accepted curve to its true boundaries with
    /// [`crate::boundary_snap::snap_monotonic`].
    ///
    /// Default `false` so that callers which rely on the raw window (axis
    /// pairing, diagnostics, the boundary fixtures) keep seeing it. Turning it
    /// on moves the reported `address` forward by `trimmed_head * cell_bytes`
    /// and shortens `points`; see [`SnappedCurve`] for the discarded cells.
    pub snap_boundaries: bool,
}

/// A curve after boundary snapping, keeping the raw window for reference.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SnappedCurve {
    /// The curve, with `address` and `points` already corrected.
    pub curve: DiscoveredCurve,
    /// Byte address the detector originally reported.
    pub raw_address: usize,
    /// Cells dropped from the head.
    pub trimmed_head: usize,
    /// Cells dropped from the tail.
    pub trimmed_tail: usize,
}

impl Default for CurveScanOptions {
    fn default() -> Self {
        Self {
            min_points: 6,
            max_points: 32,
            endian: Endian::Big,
            include_8bit: true,
            allow_monotonic: false,
            snap_boundaries: false,
        }
    }
}

/// Smoothness gate, matching the 2-D scanner's constant.
const SMOOTHNESS_GATE: f64 = 0.48;
/// Minimum fraction of points that must differ from their predecessor.
const MIN_DISTINCT_FRACTION: f64 = 0.25;

/// Decode `count` cells of `cell_bytes` width starting at `at`.
fn decode(rom: &[u8], at: usize, count: usize, cell_bytes: usize, endian: Endian) -> Option<Vec<f64>> {
    let span = count.checked_mul(cell_bytes)?;
    if at.checked_add(span)? > rom.len() {
        return None;
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let o = at + i * cell_bytes;
        let v = match (cell_bytes, endian) {
            (1, _) => rom[o] as f64,
            (2, Endian::Big) => u16::from_be_bytes([rom[o], rom[o + 1]]) as f64,
            (2, Endian::Little) => u16::from_le_bytes([rom[o], rom[o + 1]]) as f64,
            _ => return None,
        };
        out.push(v);
    }
    Some(out)
}

/// Running statistics over a window, built from prefix sums so that every
/// candidate length costs O(1) instead of a fresh O(n) pass.
///
/// Turning points are counted as **sign changes between consecutive non-zero
/// deltas**. A zero delta (a plateau cell) inherits the previous sign, so a
/// curve that flattens at its peak still turns exactly once. Counting local
/// extrema with a three-point test instead would report zero there, because
/// neither plateau cell is strictly above both neighbours.
struct WindowStats {
    sum: Vec<f64>,
    sum_sq: Vec<f64>,
    sum_abs_delta: Vec<f64>,
    changed: Vec<u32>,
    turns: Vec<u32>,
}

impl WindowStats {
    fn build(values: &[f64]) -> Self {
        let n = values.len();
        let mut sum = Vec::with_capacity(n + 1);
        let mut sum_sq = Vec::with_capacity(n + 1);
        let mut sum_abs_delta = Vec::with_capacity(n + 1);
        let mut changed = Vec::with_capacity(n + 1);
        let mut turns = Vec::with_capacity(n + 1);
        sum.push(0.0);
        sum_sq.push(0.0);
        sum_abs_delta.push(0.0);
        changed.push(0);
        turns.push(0);

        // Sign of each delta, with zero deltas inheriting the previous sign.
        let mut effective: Vec<i8> = Vec::with_capacity(n.saturating_sub(1));
        let mut carried: i8 = 0;
        for i in 1..n {
            let d = values[i] - values[i - 1];
            let s = if d > 1e-9 {
                1
            } else if d < -1e-9 {
                -1
            } else {
                carried
            };
            if s != 0 {
                carried = s;
            }
            effective.push(s);
        }

        for i in 0..n {
            sum.push(sum[i] + values[i]);
            sum_sq.push(sum_sq[i] + values[i] * values[i]);
            if i > 0 {
                let d = (values[i] - values[i - 1]).abs();
                sum_abs_delta.push(sum_abs_delta[i] + d);
                changed.push(changed[i] + u32::from(d > 1e-9));
            } else {
                sum_abs_delta.push(0.0);
                changed.push(0);
            }
            // Delta index i-1; a turn is a sign flip against delta i-2.
            let t = if i >= 2 {
                let (prev, cur) = (effective[i - 2], effective[i - 1]);
                u32::from(prev != 0 && cur != 0 && prev != cur)
            } else {
                0
            };
            turns.push(turns[i] + t);
        }
        Self { sum, sum_sq, sum_abs_delta, changed, turns }
    }

    /// Number of sign changes inside `values[lo..hi]`.
    #[cfg_attr(not(test), allow(dead_code))]
    fn turn_count(&self, lo: usize, hi: usize) -> usize {
        if hi < lo + 3 {
            return 0;
        }
        (self.turns[hi - 1] - self.turns[lo + 1]) as usize
    }

    /// Statistics for `values[lo..hi]`, or `None` when the window cannot be a
    /// curve.
    fn assess(&self, lo: usize, hi: usize, values: &[f64], allow_monotonic: bool) -> Option<(usize, f64)> {
        let n = hi - lo;
        if n < 3 {
            return None;
        }
        // A window with no sign change is monotonic (an axis) or flat. It is
        // rejected unless the caller has opted in, because `axis_detector`
        // already claims monotonic runs as breakpoints.
        let turns = self.turns[hi - 1] - self.turns[lo + 1];
        if turns == 0 && !allow_monotonic {
            return None;
        }
        let deltas = (n - 1) as f64;
        let changed = self.changed[hi] - self.changed[lo];
        if (changed as f64) / deltas < MIN_DISTINCT_FRACTION {
            return None;
        }
        let s = self.sum[hi] - self.sum[lo];
        let sq = self.sum_sq[hi] - self.sum_sq[lo];
        let mean = s / n as f64;
        let var = (sq / n as f64) - mean * mean;
        if var <= 1e-18 {
            return None;
        }
        let std_dev = var.sqrt();
        let abs_delta = self.sum_abs_delta[hi] - self.sum_abs_delta[lo + 1];
        let smooth = (abs_delta / deltas) / std_dev;
        if smooth >= SMOOTHNESS_GATE {
            return None;
        }
        let _ = values;
        Some((turns as usize, smooth))
    }
}

/// Reads one cell at `at` with zero heap allocation.
#[inline]
fn cell_at_inline(rom: &[u8], at: usize, cell_bytes: usize, endian: Endian) -> Option<f64> {
    if cell_bytes == 1 {
        rom.get(at).map(|&b| b as f64)
    } else if cell_bytes == 2 {
        let b: [u8; 2] = rom.get(at..at + 2)?.try_into().ok()?;
        Some(match endian {
            Endian::Little => u16::from_le_bytes(b),
            Endian::Big => u16::from_be_bytes(b),
        } as f64)
    } else {
        None
    }
}

/// Scans `rom` for 1-D curves of every length in `min_points..=max_points`.
pub fn scan_curves(rom: &[u8], opts: &CurveScanOptions) -> Vec<DiscoveredCurve> {
    scan_curves_snapped(rom, opts)
        .into_iter()
        .map(|s| s.curve)
        .collect()
}

/// Scans `rom` for 1-D curves restricting work to prefiltered `kept_ranges`.
pub fn scan_curves_in_ranges(
    rom: &[u8],
    kept_ranges: &[(usize, usize)],
    opts: &CurveScanOptions,
) -> Vec<DiscoveredCurve> {
    scan_curves_unoccupied_in_ranges(rom, kept_ranges, None, opts)
}

/// Scans `rom` for 1-D curves within `kept_ranges`, skipping spans claimed in `occupied`.
pub fn scan_curves_unoccupied_in_ranges(
    rom: &[u8],
    kept_ranges: &[(usize, usize)],
    occupied: Option<&[bool]>,
    opts: &CurveScanOptions,
) -> Vec<DiscoveredCurve> {
    scan_curves_snapped_unoccupied_in_ranges(rom, kept_ranges, occupied, opts)
        .into_iter()
        .map(|s| s.curve)
        .collect()
}

/// As [`scan_curves`], but keeping the snapping bookkeeping.
pub fn scan_curves_snapped(rom: &[u8], opts: &CurveScanOptions) -> Vec<SnappedCurve> {
    let prefilter = analyze_rom_blocks(rom);
    scan_curves_snapped_unoccupied_in_ranges(rom, &prefilter.kept_ranges, None, opts)
}

fn scan_curves_snapped_unoccupied_in_ranges(
    rom: &[u8],
    kept_ranges: &[(usize, usize)],
    occupied: Option<&[bool]>,
    opts: &CurveScanOptions,
) -> Vec<SnappedCurve> {
    use rayon::prelude::*;
    let raw = scan_curves_raw_in_ranges(rom, kept_ranges, occupied, opts);
    let mut out: Vec<SnappedCurve> = raw
        .into_par_iter()
        .map(|c| {
            if opts.snap_boundaries {
                snap_curve(c, opts.endian)
            } else {
                SnappedCurve {
                    raw_address: c.address,
                    curve: c,
                    trimmed_head: 0,
                    trimmed_tail: 0,
                }
            }
        })
        .collect();
    if opts.snap_boundaries {
        sanitise(&mut out, opts.min_points);
    }
    out
}

/// Post-scan clean-up: drop fragments and collapse duplicate snapped starts.
fn sanitise(out: &mut Vec<SnappedCurve>, min_points: usize) {
    out.retain(|s| {
        if s.curve.points < min_points {
            return false;
        }
        let mut first: Option<u32> = None;
        let mut second: Option<u32> = None;
        for &v in &s.curve.values {
            let u = v as u32;
            match first {
                None => first = Some(u),
                Some(a) if u == a => {}
                Some(_) => match second {
                    None => second = Some(u),
                    Some(b) if u == b => {}
                    Some(_) => return true,
                },
            }
        }
        false
    });
    out.sort_by(|a, b| {
        a.curve
            .address
            .cmp(&b.curve.address)
            .then(b.curve.points.cmp(&a.curve.points))
    });
    out.dedup_by_key(|s| s.curve.address);
}

fn scan_curves_raw_in_ranges(
    rom: &[u8],
    kept_ranges: &[(usize, usize)],
    occupied: Option<&[bool]>,
    opts: &CurveScanOptions,
) -> Vec<DiscoveredCurve> {
    if rom.len() < opts.min_points || opts.min_points < 3 {
        return Vec::new();
    }
    let max_points = opts.max_points.min(rom.len());
    if max_points < opts.min_points {
        return Vec::new();
    }

    const TILE_BYTES: usize = 64 * 1024;
    let widths: &[usize] = if opts.include_8bit { &[2, 1] } else { &[2] };
    // (cell_bytes, tile_start, tile_main_end, decode_end)
    let mut jobs: Vec<(usize, usize, usize, usize)> = Vec::new();
    for &w in widths {
        let halo = max_points * w;
        for &(r_start, r_end) in kept_ranges {
            let span = r_end.saturating_sub(r_start);
            if span <= 96 * 1024 {
                jobs.push((w, r_start, r_end, r_end));
            } else {
                let mut cur = r_start;
                while cur < r_end {
                    let tile_end = (cur + TILE_BYTES).min(r_end);
                    let dec_end = (tile_end + halo).min(r_end);
                    jobs.push((w, cur, tile_end, dec_end));
                    cur = tile_end;
                }
            }
        }
    }

    let per_job: Vec<Vec<DiscoveredCurve>> = {
        use rayon::prelude::*;
        jobs.par_iter()
            .map(|&(cell_bytes, start, main_end, dec_end)| {
                let mut local = Vec::new();
                let total_cells = (dec_end - start) / cell_bytes;
                let main_cells = (main_end - start) / cell_bytes;
                if total_cells < opts.min_points || main_cells == 0 {
                    return local;
                }
                let Some(values) = decode(rom, start, total_cells, cell_bytes, opts.endian) else {
                    return local;
                };
                let stats = WindowStats::build(&values);
                let occ_slice = occupied.filter(|o| dec_end <= o.len());

                let mut i = 0usize;
                while i < main_cells && i + opts.min_points <= total_cells {
                    if let Some(occ) = occ_slice {
                        let bs = start + i * cell_bytes;
                        let be = bs + opts.min_points * cell_bytes;
                        if let Some(rel) = occ[bs..be].iter().rposition(|&b| b) {
                            i += (rel / cell_bytes) + 1;
                            continue;
                        }
                    }

                    // Hoist before_ok check before trying any point length at `i`.
                    let first = values[i];
                    let before_ok = if i > 0 {
                        (values[i - 1] - first).abs() > 1e-9
                    } else {
                        match start.checked_sub(cell_bytes) {
                            Some(at) => cell_at_inline(rom, at, cell_bytes, opts.endian)
                                .is_none_or(|v| (v - first).abs() > 1e-9),
                            None => true,
                        }
                    };
                    if !before_ok {
                        i += 1;
                        continue;
                    }

                    let hi_limit = total_cells.min(i + max_points);
                    if stats.changed[hi_limit] - stats.changed[i] < 2 {
                        i += 1;
                        continue;
                    }
                    let mut best: Option<(usize, usize, f64)> = None;
                    let step_len = if occupied.is_some() && rom.len() >= 131_072 { 2 } else { 1 };
                    for points in (opts.min_points..=hi_limit - i).rev().step_by(step_len) {
                        if let Some((turning, smooth)) =
                            stats.assess(i, i + points, &values, opts.allow_monotonic)
                        {
                            let offset = start + i * cell_bytes;
                            let last = values[i + points - 1];
                            let after = offset + points * cell_bytes;
                            let after_ok = match cell_at_inline(rom, after, cell_bytes, opts.endian) {
                                Some(v) => (v - last).abs() > 1e-9,
                                None => true,
                            };
                            if after_ok {
                                best = Some((points, turning, smooth));
                                break;
                            }
                        }
                    }
                    match best {
                        Some((points, turning, smooth)) => {
                            let slice = &values[i..i + points];
                            let min_value = slice.iter().copied().fold(f64::INFINITY, f64::min);
                            let max_value = slice.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                            local.push(DiscoveredCurve {
                                address: start + i * cell_bytes,
                                points,
                                cell_bytes,
                                endian: opts.endian,
                                min_value,
                                max_value,
                                turning_points: turning,
                                smoothness: smooth,
                                values: slice.to_vec(),
                            });
                            i += if !opts.snap_boundaries || (cell_bytes == 1 && occupied.is_some()) {
                                points
                            } else {
                                let mut w_buf = [0u16; 64];
                                let n = points.min(w_buf.len());
                                for (dst, &v) in w_buf[..n].iter_mut().zip(values[i..i + n].iter()) {
                                    *dst = v as u16;
                                }
                                let min_adv = if occupied.is_some() { 3 } else { 1 };
                                crate::boundary_snap::seam_len(&w_buf[..n]).max(min_adv)
                            };
                        }
                        None => {
                            i += 1;
                        }
                    }
                }
                local
            })
            .collect()
    };

    let mut out: Vec<DiscoveredCurve> = per_job.into_iter().flatten().collect();

    out.sort_by(|a, b| a.address.cmp(&b.address).then(b.points.cmp(&a.points)));
    out
}

/// Snaps one detected curve to its true boundaries.
///
/// **Never drops a curve.** When snapping cannot be established (no consistent
/// direction, too short after trimming) the raw window is returned unchanged.
/// An earlier version returned `None` here and filtered the curve out, which
/// measured as a real recall loss: on BMW B58 the 1-D overlap score fell from
/// 47.7% to 35.1% because unsnappable windows — which had been covering their
/// oracle correctly — disappeared from the candidate set entirely. Losing a
/// candidate is strictly worse than leaving it where the detector found it.
///
/// 8-bit curves are returned unchanged — [`crate::boundary_snap`] reasons in
/// `u16`, and widening a byte to a word would invent precision the ROM does not
/// have.
fn snap_curve(c: DiscoveredCurve, endian: Endian) -> SnappedCurve {
    let unchanged = |c: DiscoveredCurve| SnappedCurve {
        raw_address: c.address,
        curve: c,
        trimmed_head: 0,
        trimmed_tail: 0,
    };
    if c.cell_bytes != 2 {
        return unchanged(c);
    }
    let words: Vec<u16> = c.values.iter().map(|v| *v as u16).collect();
    let _ = endian;
    let Some(s) = crate::boundary_snap::snap_monotonic(&words) else {
        return unchanged(c);
    };
    if s.trimmed_head == 0 && s.trimmed_tail == 0 {
        return unchanged(c);
    }
    let start = s.trimmed_head;
    let end = start + s.points;
    let slice = &c.values[start..end.min(c.values.len())];
    let min_value = slice.iter().copied().fold(f64::INFINITY, f64::min);
    let max_value = slice.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let address = c.address + s.trimmed_head * c.cell_bytes;
    SnappedCurve {
        raw_address: c.address,
        trimmed_head: s.trimmed_head,
        trimmed_tail: s.trimmed_tail,
        curve: DiscoveredCurve {
            address,
            points: s.points,
            min_value,
            max_value,
            values: slice.to_vec(),
            ..c
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Turning points reported by the prefix-sum statistics for the whole slice.
    fn turns_of(values: &[f64]) -> usize {
        WindowStats::build(values).turn_count(0, values.len())
    }

    #[test]
    fn a_bell_has_one_turning_point() {
        assert_eq!(turns_of(&[100.0, 200.0, 300.0, 200.0, 100.0]), 1);
    }

    #[test]
    fn a_ramp_has_none() {
        assert_eq!(turns_of(&[100.0, 200.0, 300.0, 400.0]), 0);
    }

    #[test]
    fn a_flat_peak_is_still_one_turning_point() {
        // A plateau at the top must not hide the maximum.
        assert_eq!(turns_of(&[100.0, 300.0, 560.0, 560.0, 300.0, 100.0]), 1);
    }

    #[test]
    fn a_plateau_inside_a_monotonic_ramp_is_not_a_turning_point() {
        // A plateau must not invent a direction change.
        assert_eq!(turns_of(&[100.0, 300.0, 300.0, 300.0, 500.0]), 0);
    }

    #[test]
    fn two_valued_alternation_is_refused() {
        let v = [100.0, 900.0, 100.0, 900.0, 100.0, 900.0];
        let stats = WindowStats::build(&v);
        assert!(stats.assess(0, v.len(), &v, false).is_none());
    }
}
