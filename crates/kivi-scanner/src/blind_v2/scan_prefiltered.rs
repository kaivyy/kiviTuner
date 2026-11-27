use rayon::prelude::*;

use super::gates::stripe_penalty_u64;
use super::prefix::{IntKind, IntPrefixIndex};
use super::{BlindV2Candidate, BlindV2Options, DiscoveryStatus};

const I16_CORE_SHAPES: &[(usize, usize)] = &[
    (6, 6),
    (8, 8),
    (6, 12),
    (8, 10),
    (10, 12),
    (12, 12),
    (14, 14),
    (4, 4),
    (6, 8),
    (12, 10),
    (12, 16),
    (16, 12),
    (16, 16),
    (8, 12),
    (12, 8),
    (8, 6),
    (10, 10),
    (14, 10),
    (16, 8),
    (8, 16),
    (16, 20),
    (20, 20),
];

const I8_CORE_SHAPES: &[(usize, usize)] = &[
    (4, 4),
    (4, 6),
    (6, 4),
    (6, 6),
    (8, 8),
    (8, 10),
    (10, 10),
    (10, 14),
    (12, 10),
    (12, 12),
    (12, 16),
    (14, 10),
    (16, 12),
    (16, 16),
    (16, 20),
    (20, 16),
    (20, 20),
];

const U8_CORE_SHAPES: &[(usize, usize)] = &[
    (6, 6),
    (8, 8),
    (4, 16),
    (16, 4),
    (16, 20),
    (20, 20),
    (12, 10),
    (4, 4),
    (12, 12),
    (17, 18),
    (14, 10),
    (16, 16),
    (6, 12),
    (6, 8),
    (8, 6),
    (4, 6),
    (12, 16),
    (14, 14),
    (4, 8),
    (8, 10),
    (10, 10),
    (8, 12),
    (12, 8),
    (16, 12),
    (5, 5),
    (10, 12),
    (6, 10),
    (8, 16),
    (16, 8),
    (20, 18),
    (20, 16),
    (11, 16),
    (10, 14),
    (6, 4),
    (21, 10),
];

const NON_DOM_U16_SHAPES: &[(usize, usize)] = &[
    (4, 4),
    (6, 6),
    (8, 8),
    (8, 12),
    (12, 8),
    (12, 12),
    (12, 16),
    (16, 12),
    (16, 16),
    (16, 20),
    (20, 20),
    (10, 10),
];

#[inline]
#[allow(dead_code)]
pub(super) fn check_16bit_words(slice: &[u16], rows: usize, cols: usize) -> bool {
    let check_rows = rows.min(3);
    let mut sum_diff = 0i32;
    for r in 0..check_rows {
        let r_off = r * cols;
        let mut prev = slice[r_off] as i32;
        for c in 1..cols {
            let curr = slice[r_off + c] as i32;
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

#[inline]
pub(super) fn fast_slice_ac_mdsd<T: Copy>(
    slice: &[T],
    to_f64: fn(T) -> f64,
    rows: usize,
    cols: usize,
    min_var: f64,
    min_ac: f64,
    max_mdsd: f64,
) -> Option<(f64, f64)> {
    let total_cells = rows * cols;
    let end = total_cells;
    if slice.len() < end || rows < 2 || cols < 2 {
        return None;
    }
    let mut sum_val = 0.0f64;
    for &v in &slice[..end] {
        sum_val += to_f64(v);
    }
    let mean = sum_val / total_cells as f64;
    let mut denom = 0.0f64;
    let mut num = 0.0f64;
    for i in 0..total_cells {
        let diff = to_f64(slice[i]) - mean;
        denom += diff * diff;
        if i + cols < total_cells {
            num += diff * (to_f64(slice[i + cols]) - mean);
        }
    }
    let var = denom / total_cells as f64;
    if var < min_var || denom < 1e-12 {
        return None;
    }
    let ac = num / denom;
    if ac < min_ac {
        return None;
    }
    let sd = var.sqrt();
    let delta_count = (rows * (cols - 1) + (rows - 1) * cols) as f64;
    if delta_count <= 0.0 {
        return None;
    }
    let budget = max_mdsd * sd * delta_count;
    let mut sum_delta = 0.0f64;
    for r in 0..rows {
        let base = r * cols;
        for c in 0..cols - 1 {
            sum_delta += (to_f64(slice[base + c + 1]) - to_f64(slice[base + c])).abs();
        }
        if sum_delta > budget {
            return None;
        }
    }
    for r in 0..rows - 1 {
        let base = r * cols;
        let next = base + cols;
        for c in 0..cols {
            sum_delta += (to_f64(slice[next + c]) - to_f64(slice[base + c])).abs();
        }
        if sum_delta > budget {
            return None;
        }
    }
    let mdsd = (sum_delta / delta_count) / sd;
    if mdsd.is_finite() && mdsd < max_mdsd {
        Some((ac, mdsd))
    } else {
        None
    }
}

#[inline]
fn gearys_c_i32(slice: &[i32], rows: usize, cols: usize, mean: f64, denom: f64) -> f64 {
    let n = (rows * cols) as f64;
    if denom <= 1e-12 {
        return 1.0;
    }
    let mut sum_sq = 0i64;
    let mut edges = 0usize;
    for r in 0..rows {
        let base = r * cols;
        for c in 0..cols {
            let v = slice[base + c] as i64;
            if c + 1 < cols {
                let d = slice[base + c + 1] as i64 - v;
                sum_sq += d * d;
                edges += 1;
            }
            if r + 1 < rows {
                let d = slice[base + cols + c] as i64 - v;
                sum_sq += d * d;
                edges += 1;
            }
        }
    }
    let _ = mean;
    if edges == 0 {
        return 1.0;
    }
    ((n - 1.0) * (sum_sq as f64)) / (2.0 * (edges as f64) * denom)
}

#[inline]
fn max_adj_diff_over_range_i32(slice: &[i32], rows: usize, cols: usize) -> f64 {
    let mut lo = i32::MAX;
    let mut hi = i32::MIN;
    let mut max_d = 0i32;
    for r in 0..rows {
        let base = r * cols;
        for c in 0..cols {
            let v = slice[base + c];
            if v < lo {
                lo = v;
            }
            if v > hi {
                hi = v;
            }
            if c + 1 < cols {
                let d = (slice[base + c + 1] - v).abs();
                if d > max_d {
                    max_d = d;
                }
            }
            if r + 1 < rows {
                let d = (slice[base + cols + c] - v).abs();
                if d > max_d {
                    max_d = d;
                }
            }
        }
    }
    let range = (hi as f64) - (lo as f64);
    if range <= 1e-12 {
        0.0
    } else {
        (max_d as f64) / range
    }
}

#[inline]
fn admits_i32_grid(slice: &[i32], is_u8: bool) -> bool {
    let n = slice.len();
    if n == 0 {
        return true;
    }
    if is_u8 {
        let mut counts = [0u16; 256];
        for &v in slice {
            if (0..=255).contains(&v) {
                counts[v as usize] += 1;
            }
        }
        let mut distinct = 0usize;
        let mut max_c = 0usize;
        for &c in &counts {
            if c > 0 {
                distinct += 1;
                if c as usize > max_c {
                    max_c = c as usize;
                }
            }
        }
        return distinct > 2 && ((max_c as f64) / (n as f64)) < 0.85;
    }

    let mut stack = [0i32; 1024];
    let mut heap = Vec::new();
    let buf: &mut [i32] = if n <= stack.len() {
        stack[..n].copy_from_slice(slice);
        &mut stack[..n]
    } else {
        heap.extend_from_slice(slice);
        &mut heap[..n]
    };
    buf.sort_unstable();
    let mut distinct = 1usize;
    let mut max_c = 1usize;
    let mut run = 1usize;
    for i in 1..n {
        if buf[i] == buf[i - 1] {
            run += 1;
            if run > max_c {
                max_c = run;
            }
        } else {
            distinct += 1;
            run = 1;
        }
    }
    distinct > 2 && ((max_c as f64) / (n as f64)) < 0.85
}

#[allow(clippy::too_many_arguments)]
fn build_prefiltered_i32_candidate(
    idx: &IntPrefixIndex,
    cell: usize,
    offset: usize,
    rows: usize,
    cols: usize,
    elem_bytes: usize,
    little_endian: bool,
    is_signed: bool,
    dtype_label: &str,
    ac: f64,
    mdsd: f64,
    _max_mdsd: f64,
) -> Option<(BlindV2Candidate, f64)> {
    let cells = rows * cols;
    let slice = &idx.vals[cell..cell + cells];

    let (mean, _var, denom) = idx.window_mean_var(cell, cells);
    let gc = gearys_c_i32(slice, rows, cols, mean, denom);
    let max_gc = if elem_bytes == 1 { 0.30 } else { 0.32 };
    if !gc.is_finite() || gc >= max_gc {
        return None;
    }
    if rows >= 3 && cols >= 3 && mdsd > 0.32 && max_adj_diff_over_range_i32(slice, rows, cols) > 0.70 {
        return None;
    }

    let mut stack_bits = [0u64; 1024];
    let mut heap_bits = Vec::new();
    let raw_bits: &mut [u64] = if cells <= stack_bits.len() {
        for (dst, &v) in stack_bits[..cells].iter_mut().zip(slice.iter()) {
            *dst = (v as f64).round().to_bits();
        }
        &mut stack_bits[..cells]
    } else {
        heap_bits.extend(slice.iter().map(|&v| (v as f64).round().to_bits()));
        &mut heap_bits[..cells]
    };

    if stripe_penalty_u64(raw_bits, cols) < 0.5 {
        return None;
    }

    if is_signed {
        let mut erasures = 0usize;
        for &v in slice {
            if v == 0 || v == -1 {
                erasures += 1;
            }
        }
        if erasures * 2 > cells {
            return None;
        }
    }
    if !admits_i32_grid(slice, elem_bytes == 1 && !is_signed) {
        return None;
    }

    let align_b = if offset.is_multiple_of(16) {
        1.05
    } else if offset.is_multiple_of(4) {
        1.02
    } else {
        1.0
    };
    let area_w = (cells as f64).powf(0.55);
    let score = align_b * area_w * ac * (1.0 - gc).max(0.1) / (mdsd + 0.05);
    let fine_obj = mdsd + 0.30 * gc - 0.15 * ac;

    Some((
        BlindV2Candidate {
            address: offset,
            rows,
            cols,
            elem_bytes,
            little_endian,
            dtype_label: dtype_label.to_string(),
            is_curve: false,
            x_axis: None,
            y_axis: None,
            smoothness: mdsd,
            p23: None,
            quality_score: score,
            status: if mdsd < 0.36 && ac >= 0.45 {
                DiscoveryStatus::StructurallySupported
            } else {
                DiscoveryStatus::Discovered
            },
            detection_source: "prefiltered_2d_v2".to_string(),
        },
        fine_obj,
    ))
}

struct I32ShapeScanParams {
    range_start: usize,
    max_cells: usize,
    kind: IntKind,
    eb: usize,
    is_signed: bool,
    rows: usize,
    cols: usize,
    max_avg_step: usize,
    min_var: f64,
    min_ac: f64,
    max_mdsd: f64,
}

#[inline]
fn evaluate_i32_cell_at(
    idx: &IntPrefixIndex,
    p: &I32ShapeScanParams,
    cell: usize,
) -> Option<(BlindV2Candidate, f64)> {
    if cell > p.max_cells {
        return None;
    }
    if !idx.check_rows_fast(cell, p.rows, p.cols, p.max_avg_step) {
        return None;
    }
    let total_cells = p.rows * p.cols;
    if p.is_signed && !idx.crosses_zero(cell, total_cells) {
        return None;
    }
    let (ac, mdsd) = idx.ac_and_mdsd(cell, p.rows, p.cols, p.min_var, p.min_ac, p.max_mdsd)?;
    build_prefiltered_i32_candidate(
        idx,
        cell,
        p.range_start + cell * p.eb,
        p.rows,
        p.cols,
        p.eb,
        p.kind.little_endian(),
        p.is_signed,
        p.kind.label(),
        ac,
        mdsd,
        p.max_mdsd,
    )
}

#[inline]
fn median_f64(buf: &mut [f64]) -> f64 {
    if buf.is_empty() {
        return 0.0;
    }
    buf.sort_by(|a, b| a.total_cmp(b));
    buf[buf.len() / 2]
}

/// Snaps a 2-D periodic surface off any mid-row column wrap-around cliff or header-contaminated row 0.
fn snap_accepted_i32_candidate(
    idx: &IntPrefixIndex,
    p: &I32ShapeScanParams,
    cell: usize,
    cand: BlindV2Candidate,
) -> (usize, BlindV2Candidate) {
    let mut cur_cell = cell;
    let mut cur_cand = cand;
    let rows = p.rows;
    let cols = p.cols;

    if rows >= 3 && cols >= 4 {
        let slice = &idx.vals[cur_cell..cur_cell + rows * cols];
        let mut row_diffs = Vec::with_capacity(rows);
        let mut col_jumps = Vec::with_capacity(cols - 1);
        let mut best_k = 1usize;
        let mut best_j = -1.0f64;
        let mut second_max = 0.0f64;

        for k in 1..cols {
            row_diffs.clear();
            for r in 0..rows {
                let d = slice[r * cols + k].abs_diff(slice[r * cols + k - 1]) as f64;
                row_diffs.push(d);
            }
            let mj = median_f64(&mut row_diffs);
            col_jumps.push(mj);
            if mj > best_j {
                second_max = second_max.max(best_j);
                best_j = mj;
                best_k = k;
            } else if mj > second_max {
                second_max = mj;
            }
        }
        let med_jump = median_f64(&mut col_jumps);
        if best_j >= 2.5 * med_jump.max(1.0) && best_j >= 1.8 * second_max.max(1.0) {
            let candidates = [cur_cell.checked_add(best_k), cur_cell.checked_sub(cols - best_k)];
            let mut snapped: Option<(usize, BlindV2Candidate)> = None;
            for next_c in candidates.into_iter().flatten() {
                if next_c <= p.max_cells {
                    if let Some((new_c, _)) = evaluate_i32_cell_at(idx, p, next_c) {
                        if new_c.smoothness <= cur_cand.smoothness * 0.92 {
                            let better = match &snapped {
                                None => true,
                                Some((_, prev)) => new_c.smoothness < prev.smoothness,
                            };
                            if better {
                                snapped = Some((next_c, new_c));
                            }
                        }
                    }
                }
            }
            if let Some((sc, sc_cand)) = snapped {
                cur_cell = sc;
                cur_cand = sc_cand;
            }
        }
    }

    if rows >= 4 && cur_cell + cols <= p.max_cells {
        let slice = &idx.vals[cur_cell..cur_cell + rows * cols];
        let mut row_means = Vec::with_capacity(rows - 1);
        for r in 0..rows - 1 {
            let b0 = r * cols;
            let b1 = (r + 1) * cols;
            let mut s = 0u64;
            for c in 0..cols {
                s += slice[b1 + c].abs_diff(slice[b0 + c]) as u64;
            }
            row_means.push(s as f64 / cols as f64);
        }
        let d01 = row_means[0];
        let d_body = median_f64(&mut row_means[1..]);
        if d01 > 2.5 * d_body.max(1.0) {
            let next_c = cur_cell + cols;
            if let Some((new_c, _)) = evaluate_i32_cell_at(idx, p, next_c) {
                if new_c.smoothness <= cur_cand.smoothness * 0.90 {
                    cur_cell = next_c;
                    cur_cand = new_c;
                }
            }
        }
    }

    (cur_cell, cur_cand)
}

fn scan_int_index_shape(
    idx: &IntPrefixIndex,
    range_start: usize,
    main_end: usize,
    range_end: usize,
    kind: IntKind,
    rows: usize,
    cols: usize,
) -> Vec<BlindV2Candidate> {
    let mut local = Vec::new();
    let total_cells = rows * cols;
    let eb = kind.elem_bytes();
    let byte_len = total_cells * eb;
    let range_bytes = range_end.saturating_sub(range_start);
    let main_cells = main_end.saturating_sub(range_start) / eb;
    if byte_len > range_bytes || main_cells == 0 {
        return local;
    }

    let is_byte = eb == 1;
    let is_signed = kind.is_signed();
    if is_signed && idx.has_no_sign_mix() {
        return local;
    }

    let max_avg_step = if is_byte { 18 } else { 600 };
    let min_var = if is_byte { 16.0 } else { 64.0 };
    let max_mdsd = if is_signed {
        0.36
    } else if is_byte {
        if total_cells <= 24 { 0.30 } else { 0.36 }
    } else if total_cells <= 24 {
        0.34
    } else {
        0.40
    };
    let min_ac = if is_signed { 0.45 } else { 0.35 };
    let last_data_start = idx.vals.len().saturating_sub(total_cells);
    let max_cells = if main_end < range_end {
        last_data_start.min(main_cells.saturating_sub(1))
    } else {
        last_data_start
    };

    let params = I32ShapeScanParams {
        range_start,
        max_cells,
        kind,
        eb,
        is_signed,
        rows,
        cols,
        max_avg_step,
        min_var,
        min_ac,
        max_mdsd,
    };

    let hit_skip = cols.clamp(4, 16);

    let stride = if range_bytes < 16_384 {
        1
    } else if is_byte || total_cells >= 48 {
        4
    } else if total_cells >= 16 {
        2
    } else {
        1
    };

    let mut cell = 0usize;
    while cell <= max_cells {
        if let Some(next_cell) = idx.bad_jump_next(cell, rows, cols) {
            cell = next_cell;
            continue;
        }

        if stride == 1 {
            if let Some((cand, _)) = evaluate_i32_cell_at(idx, &params, cell) {
                let (snapped_cell, snapped_cand) = snap_accepted_i32_candidate(idx, &params, cell, cand);
                local.push(snapped_cand);
                cell = (snapped_cell + hit_skip).max(cell + hit_skip);
                continue;
            }
            cell += 1;
            continue;
        }

        // Strict single coarse probe: only refine neighborhood if this coarse spot resembles a table.
        if !idx.check_rows_fast(cell, rows, cols, max_avg_step)
            || (is_signed && !idx.crosses_zero(cell, total_cells))
            || idx
                .ac_and_mdsd(cell, rows, cols, min_var * 0.75, min_ac * 0.85, max_mdsd * 1.12)
                .is_none()
        {
            cell += stride;
            continue;
        }

        let lo_fine = cell.saturating_sub(stride - 1);
        let hi_fine = (cell + stride - 1).min(max_cells);
        let mut best_hit: Option<(usize, BlindV2Candidate, f64)> = None;
        for fine in lo_fine..=hi_fine {
            if let Some((cand, fine_obj)) = evaluate_i32_cell_at(idx, &params, fine) {
                let replace = match &best_hit {
                    None => true,
                    Some((_, _, prev_obj)) => fine_obj < *prev_obj,
                };
                if replace {
                    best_hit = Some((fine, cand, fine_obj));
                }
            }
        }
        if let Some((best_fine, cand, _)) = best_hit {
            let (snapped_cell, snapped_cand) = snap_accepted_i32_candidate(idx, &params, best_fine, cand);
            local.push(snapped_cand);
            cell = (snapped_cell + hit_skip).max(cell + stride);
        } else {
            cell += stride;
        }
    }
    local
}

pub(super) fn scan_prefiltered_8_and_16_v2(
    rom: &[u8],
    kept_ranges: &[(usize, usize)],
    shapes: &[(usize, usize)],
    opts: &BlindV2Options,
) -> Vec<BlindV2Candidate> {
    let valid_shapes: Vec<(usize, usize)> = shapes
        .iter()
        .copied()
        .filter(|&(r, c)| r >= 2 && c >= 2)
        .collect();
    if valid_shapes.is_empty() {
        return Vec::new();
    }

    let large_rom = rom.len() >= 65_536;
    let skip_nondom_i16 = rom.len() >= 131_072;
    let dom_endian = if large_rom {
        Some(crate::zero_xdf_scan::detect_global_endianness(rom))
    } else {
        None
    };

    let mut non_dom_u16_shapes: Vec<(usize, usize)> = if rom.len() >= 1_048_576 {
        vec![(8, 8), (12, 12), (16, 16), (12, 16)]
    } else {
        NON_DOM_U16_SHAPES.to_vec()
    };
    let mut dom_i16_shapes: Vec<(usize, usize)> = if rom.len() >= 1_048_576 {
        I16_CORE_SHAPES[..16].to_vec()
    } else if large_rom {
        I16_CORE_SHAPES[..18].to_vec()
    } else {
        I16_CORE_SHAPES.to_vec()
    };
    let mut u8_shapes: Vec<(usize, usize)> = if rom.len() < 131_072 {
        valid_shapes.clone()
    } else {
        U8_CORE_SHAPES.to_vec()
    };
    let mut i8_shapes: Vec<(usize, usize)> = I8_CORE_SHAPES.to_vec();
    for &sh in &opts.extra_shapes {
        if sh.0 >= 2 && sh.1 >= 2 {
            if !non_dom_u16_shapes.contains(&sh) {
                non_dom_u16_shapes.push(sh);
            }
            if !dom_i16_shapes.contains(&sh) {
                dom_i16_shapes.push(sh);
            }
            if !u8_shapes.contains(&sh) {
                u8_shapes.push(sh);
            }
            if !i8_shapes.contains(&sh) {
                i8_shapes.push(sh);
            }
        }
    }

    const TILE_BYTES: usize = 256 * 1024;
    let max_shape_bytes = valid_shapes
        .iter()
        .chain(dom_i16_shapes.iter())
        .chain(u8_shapes.iter())
        .chain(i8_shapes.iter())
        .map(|&(r, c)| r * c * 2)
        .max()
        .unwrap_or(1024);
    let halo = max_shape_bytes.max(4096);

    let mut tiles: Vec<(usize, usize, usize)> = Vec::new();
    for &(r_start, r_end) in kept_ranges {
        if r_end.saturating_sub(r_start) <= TILE_BYTES + halo {
            tiles.push((r_start, r_end, r_end));
        } else {
            let mut cur = r_start;
            while cur < r_end {
                let main_end = (cur + TILE_BYTES).min(r_end);
                let scan_end = (main_end + halo).min(r_end);
                tiles.push((cur, main_end, scan_end));
                cur = main_end;
            }
        }
    }

    let mut out = Vec::new();
    for (start, main_end, end) in tiles {
        let range_bytes = end.saturating_sub(start);
        if range_bytes < 16 {
            continue;
        }

        let mut kinds = Vec::with_capacity(6);
        kinds.push(IntKind::U16Le);
        kinds.push(IntKind::U16Be);
        match dom_endian {
            Some(crate::zero_xdf_scan::Endianness::LittleEndian) => {
                kinds.push(IntKind::I16Le);
                if !skip_nondom_i16 {
                    kinds.push(IntKind::I16Be);
                }
            }
            Some(crate::zero_xdf_scan::Endianness::BigEndian) => {
                kinds.push(IntKind::I16Be);
                if !skip_nondom_i16 {
                    kinds.push(IntKind::I16Le);
                }
            }
            None => {
                kinds.push(IntKind::I16Le);
                kinds.push(IntKind::I16Be);
            }
        }
        if opts.include_8bit {
            kinds.push(IntKind::U8);
            kinds.push(IntKind::I8);
        }

        let indexes: Vec<(IntKind, IntPrefixIndex)> = kinds
            .into_par_iter()
            .map(|k| {
                let step = if k.elem_bytes() == 1 { 45 } else { 4500 };
                (k, IntPrefixIndex::build_from_rom(rom, start, end, k, step))
            })
            .collect();

        let mut jobs: Vec<(usize, (usize, usize))> = Vec::new();
        for (ki, (kind, ref idx)) in indexes.iter().enumerate() {
            let is_dom = match dom_endian {
                None => true,
                Some(crate::zero_xdf_scan::Endianness::LittleEndian) => kind.little_endian(),
                Some(crate::zero_xdf_scan::Endianness::BigEndian) => !kind.little_endian(),
            };
            let shape_slice: &[(usize, usize)] = match kind {
                IntKind::U16Le | IntKind::U16Be => {
                    if is_dom {
                        &valid_shapes
                    } else {
                        &non_dom_u16_shapes
                    }
                }
                IntKind::I16Le | IntKind::I16Be => {
                    if idx.has_no_sign_mix() {
                        continue;
                    }
                    &dom_i16_shapes
                }
                IntKind::U8 => &u8_shapes,
                IntKind::I8 => {
                    if idx.has_no_sign_mix() {
                        continue;
                    }
                    &i8_shapes
                }
            };
            for &sh in shape_slice {
                jobs.push((ki, sh));
            }
        }

        let range_hits: Vec<BlindV2Candidate> = jobs
            .par_iter()
            .flat_map_iter(|&(ki, (rows, cols))| {
                let (kind, ref idx) = indexes[ki];
                scan_int_index_shape(idx, start, main_end, end, kind, rows, cols)
            })
            .collect();

        out.extend(range_hits);
    }
    out
}
