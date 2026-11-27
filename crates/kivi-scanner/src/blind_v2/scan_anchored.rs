use crate::nms_score::{composite_quality_score, ScoreInputs};
use crate::p23_descriptors::compute_p23_descriptors;
use crate::scan_metrics::autocorrelation;
use rayon::prelude::*;

use super::axes::{build_axis_pool_v2_in_ranges, candidate_y_lens_v2, read_f32, read_u16};
use super::gates::{gearys_c_flat, passes_hard_filters_f64, stripe_penalty_u64};
use super::prefix::F32PrefixIndex;
use super::{AxisDirection, BlindV2Axis, BlindV2Candidate, BlindV2Options, DiscoveryStatus};

#[inline]
fn flat_metrics(flat: &[f64], rows: usize, cols: usize) -> Option<(f64, f64, f64, f64)> {
    let n = rows * cols;
    if rows < 2 || cols < 2 || flat.len() < n {
        return None;
    }
    let nf = n as f64;
    let mut sum = 0.0f64;
    let mut lo = flat[0];
    let mut hi = flat[0];
    for &v in &flat[..n] {
        sum += v;
        if v < lo {
            lo = v;
        } else if v > hi {
            hi = v;
        }
    }
    let mean = sum / nf;
    let mut var_sum = 0.0f64;
    for &v in &flat[..n] {
        let d = v - mean;
        var_sum += d * d;
    }
    let sd = (var_sum / nf).sqrt();
    if !sd.is_finite() || sd < 1e-9 {
        return None;
    }

    let mut total_delta = 0.0f64;
    let mut max_d = 0.0f64;
    for r in 0..rows {
        let base = r * cols;
        for c in 0..cols - 1 {
            let d = (flat[base + c + 1] - flat[base + c]).abs();
            total_delta += d;
            if d > max_d {
                max_d = d;
            }
        }
    }
    for r in 0..rows - 1 {
        let base = r * cols;
        let next = base + cols;
        for c in 0..cols {
            let d = (flat[next + c] - flat[base + c]).abs();
            total_delta += d;
            if d > max_d {
                max_d = d;
            }
        }
    }
    let edges = (rows * (cols - 1) + (rows - 1) * cols) as f64;
    let mdsd = (total_delta / edges) / sd;
    let range = hi - lo;
    let spike_ratio = if range <= 1e-12 { 0.0 } else { max_d / range };
    Some((mean, sd, mdsd, spike_ratio))
}

/// Scans axis-anchored candidates across 8-bit, 16-bit (`u16`/`i16`), and optional `f32` LE/BE axes.
pub fn scan_axis_anchored_v2(rom: &[u8], opts: &BlindV2Options) -> Vec<BlindV2Candidate> {
    let mut out = scan_axis_anchored_v2_in_ranges(rom, &[(0, rom.len())], opts);
    out.par_iter_mut().for_each(|cand| {
        if cand.p23.is_some() || cand.rows < 2 || cand.cols < 2 {
            return;
        }
        let span = cand.rows.saturating_mul(cand.cols).saturating_mul(cand.elem_bytes);
        if span == 0 || cand.address.saturating_add(span) > rom.len() {
            return;
        }
        let is_float = cand.dtype_label.starts_with("f32");
        let is_signed = cand.dtype_label.starts_with("i16");
        let mut grid = Vec::with_capacity(cand.rows);
        for r in 0..cand.rows {
            let mut row = Vec::with_capacity(cand.cols);
            for c in 0..cand.cols {
                let off = cand.address + (r * cand.cols + c) * cand.elem_bytes;
                let v = match cand.elem_bytes {
                    1 => rom[off] as f64,
                    2 if is_signed => {
                        let b = [rom[off], rom[off + 1]];
                        (if cand.little_endian { i16::from_le_bytes(b) } else { i16::from_be_bytes(b) }) as f64
                    }
                    2 => read_u16(rom, off, cand.little_endian).unwrap_or(0) as f64,
                    4 if is_float => read_f32(rom, off, cand.little_endian).unwrap_or(0.0) as f64,
                    _ => 0.0,
                };
                row.push(v);
            }
            grid.push(row);
        }
        cand.p23 = compute_p23_descriptors(&grid);
    });
    out
}

pub(super) fn scan_axis_anchored_v2_in_ranges(
    rom: &[u8],
    kept_ranges: &[(usize, usize)],
    opts: &BlindV2Options,
) -> Vec<BlindV2Candidate> {
    if rom.len() < 64 {
        return Vec::new();
    }
    let pool = build_axis_pool_v2_in_ranges(rom, kept_ranges, opts.include_8bit, opts.include_32bit);
    scan_axis_anchored_v2_with_pool(rom, &pool, opts)
}

pub(super) fn scan_axis_anchored_v2_with_pool(
    rom: &[u8],
    pool: &[BlindV2Axis],
    opts: &BlindV2Options,
) -> Vec<BlindV2Candidate> {
    if rom.len() < 64 || pool.is_empty() {
        return Vec::new();
    }

    (0..pool.len())
        .into_par_iter()
        .flat_map_iter(|i| {
            let x = &pool[i];
            let x_end = x.offset + x.length * x.elem_bytes;
            let mut local = Vec::new();
            let mut stack_f = [0.0f64; 1024];
            let mut stack_b = [0u64; 1024];
            let mut heap_f: Vec<f64> = Vec::new();
            let mut heap_b: Vec<u64> = Vec::new();

            for y in pool.iter().skip(i + 1) {
                if y.offset < x_end {
                    continue;
                }
                if y.offset - x_end > opts.max_gap {
                    break;
                }
                if x.elem_bytes > 1 && y.elem_bytes == x.elem_bytes && x.little_endian != y.little_endian {
                    continue;
                }
                if x.elem_bytes == y.elem_bytes
                    && x.length == y.length
                    && x.direction == y.direction
                {
                    let dx0 = (x.values[1] - x.values[0]).abs();
                    let dy0 = (y.values[1] - y.values[0]).abs();
                    let step_ratio = if dx0 > 1e-6 { dy0 / dx0 } else { 1.0 };
                    let span_x = (x.values[x.length - 1] - x.values[0]).abs().max(1.0);
                    let row_shift = (y.values[0] - x.values[0]).abs();
                    if (0.70..=1.40).contains(&step_ratio) && row_shift <= span_x * 0.65 {
                        continue;
                    }
                }

                // (cell_bytes, cell_le, is_signed, is_float)
                let mut cell_cfgs = [(0usize, false, false, false); 5];
                let n_cfgs;
                if y.elem_bytes == 4 {
                    cell_cfgs[0] = (4, y.little_endian, false, true);
                    cell_cfgs[1] = (2, y.little_endian, false, false);
                    cell_cfgs[2] = (2, y.little_endian, true, false);
                    n_cfgs = 3;
                } else if y.elem_bytes == 1 {
                    if opts.include_8bit {
                        cell_cfgs[0] = (1, false, false, false);
                        cell_cfgs[1] = (1, false, true, false);
                        cell_cfgs[2] = (2, true, false, false);
                        cell_cfgs[3] = (2, false, false, false);
                        n_cfgs = 4;
                    } else {
                        cell_cfgs[0] = (2, true, false, false);
                        cell_cfgs[1] = (2, false, false, false);
                        n_cfgs = 2;
                    }
                } else if opts.include_8bit {
                    cell_cfgs[0] = (2, y.little_endian, false, false);
                    cell_cfgs[1] = (2, y.little_endian, true, false);
                    cell_cfgs[2] = (1, false, false, false);
                    cell_cfgs[3] = (1, false, true, false);
                    n_cfgs = 4;
                } else {
                    cell_cfgs[0] = (2, y.little_endian, false, false);
                    cell_cfgs[1] = (2, y.little_endian, true, false);
                    n_cfgs = 2;
                }

                let y_lens = candidate_y_lens_v2(&y.values, y.length);
                let contiguous_axes = y.offset.saturating_sub(x_end) <= 8;
                let source_tag = format!(
                    "axis_anchored_v2({}b_{})",
                    y.elem_bytes * 8,
                    if matches!(y.direction, AxisDirection::Increasing) { "inc" } else { "dec" }
                );

                for &(cell_bytes, cell_le, is_signed, is_float) in &cell_cfgs[..n_cfgs] {
                    let dtype_label = match (cell_bytes, cell_le, is_signed, is_float) {
                        (1, _, true, _) => "i8",
                        (1, _, false, _) => "u8",
                        (2, true, true, _) => "i16_le",
                        (2, false, true, _) => "i16_be",
                        (2, true, false, _) => "u16_le",
                        (2, false, false, _) => "u16_be",
                        (4, true, _, true) => "f32_le",
                        _ => "f32_be",
                    };
                    let mut best: Option<(f64, BlindV2Candidate)> = None;
                    for &y_len in &y_lens {
                        let y_end = y.offset + y_len * y.elem_bytes;
                        let cells = y_len * x.length;
                        if cells < 8 {
                            continue;
                        }
                        let byte_count = cells * cell_bytes;

                        for &pad in &opts.padding_offsets {
                            let data_start = y_end + pad;
                            if is_float && !data_start.is_multiple_of(4) {
                                continue;
                            }
                            if data_start + byte_count > rom.len() {
                                continue;
                            }

                            let (raw_f, raw_bits): (&mut [f64], &mut [u64]) = if cells <= stack_f.len() {
                                (&mut stack_f[..cells], &mut stack_b[..cells])
                            } else {
                                heap_f.resize(cells, 0.0);
                                heap_b.resize(cells, 0);
                                (&mut heap_f[..cells], &mut heap_b[..cells])
                            };

                            let mut has_neg = false;
                            let mut has_pos = false;
                            let mut valid_cells = true;

                            for k in 0..cells {
                                let o = data_start + k * cell_bytes;
                                if cell_bytes == 1 {
                                    let v = if is_signed {
                                        let sv = rom[o] as i8;
                                        if sv < 0 {
                                            has_neg = true;
                                        } else {
                                            has_pos = true;
                                        }
                                        sv as f64
                                    } else {
                                        rom[o] as f64
                                    };
                                    raw_f[k] = v;
                                    raw_bits[k] = v.to_bits();
                                } else if is_float {
                                    let Some(fv) = read_f32(rom, o, cell_le) else {
                                        valid_cells = false;
                                        break;
                                    };
                                    if !F32PrefixIndex::is_valid_cal_float(fv) {
                                        valid_cells = false;
                                        break;
                                    }
                                    raw_f[k] = fv as f64;
                                    raw_bits[k] = fv.to_bits() as u64;
                                } else if is_signed {
                                    let sv = if cell_le {
                                        i16::from_le_bytes([rom[o], rom[o + 1]])
                                    } else {
                                        i16::from_be_bytes([rom[o], rom[o + 1]])
                                    };
                                    if sv < 0 {
                                        has_neg = true;
                                    } else {
                                        has_pos = true;
                                    }
                                    let v = sv as f64;
                                    raw_f[k] = v;
                                    raw_bits[k] = v.to_bits();
                                } else {
                                    let v = read_u16(rom, o, cell_le).unwrap_or(0) as f64;
                                    raw_f[k] = v;
                                    raw_bits[k] = v.to_bits();
                                }
                            }
                            if !valid_cells || (is_signed && !(has_neg && has_pos)) {
                                continue;
                            }
                            if !passes_hard_filters_f64(raw_f, cell_bytes) {
                                continue;
                            }
                            if is_signed {
                                let signed_er = raw_f
                                    .iter()
                                    .filter(|&&v| v == 0.0 || (v + 1.0).abs() <= 1e-9)
                                    .count();
                                if signed_er * 2 >= cells {
                                    continue;
                                }
                            }

                            let (min_sd, max_sd) = match cell_bytes {
                                1 => (4.0, 15000.0),
                                2 => (10.0, 15000.0),
                                _ => (0.01, 100_000.0),
                            };
                            let max_smooth = if cell_bytes == 1 && cells >= 16 {
                                0.70
                            } else {
                                0.52
                            };

                            let layout_count = if x.length != y_len { 2 } else { 1 };
                            for li in 0..layout_count {
                                let swapped = li == 1;
                                let (rows, cols) = if swapped {
                                    (x.length, y_len)
                                } else {
                                    (y_len, x.length)
                                };
                                if rows >= 3 && cell_bytes <= 2 {
                                    let last_row = &raw_f[(rows - 1) * cols..rows * cols];
                                    let erase_hi = if is_signed {
                                        -1.0
                                    } else if cell_bytes == 1 {
                                        255.0
                                    } else {
                                        65535.0
                                    };
                                    if last_row.iter().all(|&v| (v - erase_hi).abs() <= 1e-9) {
                                        continue;
                                    }
                                }
                                let Some((_mean, sd, mdsd, spike_ratio)) = flat_metrics(raw_f, rows, cols) else {
                                    continue;
                                };
                                if sd < min_sd || sd > max_sd {
                                    continue;
                                }
                                if !mdsd.is_finite() || mdsd >= max_smooth {
                                    continue;
                                }
                                if gearys_c_flat(raw_f, rows, cols) >= 0.36 {
                                    continue;
                                }
                                if rows >= 3
                                    && cols >= 3
                                    && spike_ratio > 0.70
                                    && autocorrelation(raw_f, cols) < 0.60
                                {
                                    continue;
                                }
                                if cell_bytes == 1 && mdsd >= 0.48 && autocorrelation(raw_f, cols) < 0.35 {
                                    continue;
                                }
                                if cell_bytes >= 2 && mdsd >= 0.45 && autocorrelation(raw_f, cols) < 0.25 {
                                    continue;
                                }
                                if stripe_penalty_u64(raw_bits, cols) < 0.5 {
                                    continue;
                                }

                                let score = composite_quality_score(&ScoreInputs {
                                    rows,
                                    cols,
                                    z_address: data_start,
                                    smoothness: mdsd,
                                    confidence_score: 0.96,
                                    has_x_axis: true,
                                    has_y_axis: true,
                                    axes_contiguous: pad <= 4 && contiguous_axes,
                                    first_row_is_flat: false,
                                });

                                let mut y_trunc = y.clone();
                                y_trunc.length = y_len;
                                y_trunc.values.truncate(y_len);
                                let (x_ax, y_ax) = if swapped {
                                    (y_trunc, x.clone())
                                } else {
                                    (x.clone(), y_trunc)
                                };

                                let rank_score = mdsd + 0.35 * spike_ratio;
                                let cand = BlindV2Candidate {
                                    address: data_start,
                                    rows,
                                    cols,
                                    elem_bytes: cell_bytes,
                                    little_endian: cell_le,
                                    dtype_label: dtype_label.to_string(),
                                    is_curve: false,
                                    x_axis: Some(x_ax),
                                    y_axis: Some(y_ax),
                                    smoothness: mdsd,
                                    p23: None,
                                    quality_score: score,
                                    status: DiscoveryStatus::StructurallySupported,
                                    detection_source: source_tag.clone(),
                                };
                                let replace = match &best {
                                    None => true,
                                    Some((prev_rank, _)) => rank_score < *prev_rank,
                                };
                                if replace {
                                    best = Some((rank_score, cand));
                                }
                            }
                        }
                    }
                    if let Some((_, t)) = best {
                        local.push(t);
                    }
                }
            }
            local
        })
        .collect()
}
