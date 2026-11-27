use crate::nms_score::{composite_quality_score, ScoreInputs};
use crate::zero_xdf_prefilter::analyze_rom_blocks;
use rayon::prelude::*;

use super::axes::read_u16;
use super::gates::{max_adj_diff_over_range, stripe_penalty_u64};
use super::prefix::{F32PrefixIndex, U32PrefixIndex};
use super::scan_prefiltered::fast_slice_ac_mdsd;
use super::{BlindV2Candidate, DiscoveryStatus};

pub fn scan_32bit_tables(rom: &[u8], shapes: &[(usize, usize)]) -> Vec<BlindV2Candidate> {
    let prefilter = analyze_rom_blocks(rom);
    scan_32bit_tables_in_ranges(rom, &prefilter.kept_ranges, shapes)
}

fn eval_f32_window(
    idx: &F32PrefixIndex,
    range_start: usize,
    cell: usize,
    rows: usize,
    cols: usize,
    le: bool,
) -> Option<BlindV2Candidate> {
    let cells = rows * cols;
    if idx.zero_count(cell, cells) * 2 > cells as u32 {
        return None;
    }
    let (_approx_mean, approx_sd) = idx.approx_mean_sd(cell, cells);
    if !approx_sd.is_finite() || approx_sd < 0.008 {
        return None;
    }
    let delta_count = (rows * (cols - 1) + (rows - 1) * cols) as f64;
    if idx.sum_dx(cell, rows, cols) > 0.48 * (approx_sd + 1e-4) * delta_count * 1.01 {
        return None;
    }

    let fslice = &idx.vals[cell..cell + cells];
    let mut flat = [0.0f64; 1024];
    let mut raw_bits = [0u64; 1024];
    let mut sum_v = 0.0f64;
    for k in 0..cells {
        let fv = fslice[k];
        let v = fv as f64;
        flat[k] = v;
        raw_bits[k] = fv.to_bits() as u64;
        sum_v += v;
    }
    let n = cells as f64;
    let mean = sum_v / n;
    let mut denom = 0.0f64;
    let mut num = 0.0f64;
    for i in 0..cells {
        let diff = flat[i] - mean;
        denom += diff * diff;
        if i + cols < cells {
            num += diff * (flat[i + cols] - mean);
        }
    }
    let sd = (denom / n).sqrt();
    if sd < 0.01 || denom < 1e-12 {
        return None;
    }
    let ac = num / denom;
    if ac < 0.45 {
        return None;
    }

    let budget = 0.48 * sd * delta_count;
    let mut sum_delta = 0.0f64;
    for r in 0..rows {
        let base = r * cols;
        for c in 0..cols - 1 {
            sum_delta += (flat[base + c + 1] - flat[base + c]).abs();
        }
        if sum_delta > budget {
            return None;
        }
    }
    for r in 0..rows - 1 {
        let base = r * cols;
        let next = base + cols;
        for c in 0..cols {
            sum_delta += (flat[next + c] - flat[base + c]).abs();
        }
        if sum_delta > budget {
            return None;
        }
    }
    let mdsd = (sum_delta / delta_count) / sd;
    if !mdsd.is_finite() || mdsd >= 0.48 {
        return None;
    }

    let bits_s = &raw_bits[..cells];
    let mut uniq = bits_s.to_vec();
    uniq.sort_unstable();
    uniq.dedup();
    if uniq.len() < 6 || stripe_penalty_u64(bits_s, cols) < 0.5 {
        return None;
    }
    let mut grid = Vec::with_capacity(rows);
    for r in 0..rows {
        grid.push(flat[r * cols..(r + 1) * cols].to_vec());
    }
    if max_adj_diff_over_range(&grid) > 0.65 {
        return None;
    }
    let off = range_start + cell * 4;
    let score = composite_quality_score(&ScoreInputs {
        rows,
        cols,
        z_address: off,
        smoothness: mdsd,
        confidence_score: (1.0 - (mdsd / 0.5).min(0.9)) * ac,
        has_x_axis: false,
        has_y_axis: false,
        axes_contiguous: false,
        first_row_is_flat: false,
    });
    Some(BlindV2Candidate {
        address: off,
        rows,
        cols,
        elem_bytes: 4,
        little_endian: le,
        dtype_label: if le { "f32_le" } else { "f32_be" }.to_string(),
        is_curve: false,
        x_axis: None,
        y_axis: None,
        smoothness: mdsd,
        p23: None,
        quality_score: score,
        status: if ac >= 0.65 && mdsd < 0.36 {
            DiscoveryStatus::StructurallySupported
        } else {
            DiscoveryStatus::Discovered
        },
        detection_source: "scan_32bit_tables".to_string(),
    })
}

#[allow(clippy::too_many_arguments)]
fn eval_u32_window(
    rom: &[u8],
    idx: &U32PrefixIndex,
    f32_idx: &F32PrefixIndex,
    range_start: usize,
    cell: usize,
    rows: usize,
    cols: usize,
    le: bool,
) -> Option<BlindV2Candidate> {
    let cells = rows * cols;
    if idx.zero_or_max_count(cell, cells) * 2 > cells as u32 {
        return None;
    }
    let (mean, sd) = idx.exact_mean_sd(cell, cells);
    if !sd.is_finite() || sd < 5.0 {
        return None;
    }
    let delta_count = (rows * (cols - 1) + (rows - 1) * cols) as f64;
    let budget = 0.48 * sd * delta_count;
    let sum_dx = idx.sum_dx(cell, rows, cols);
    if sum_dx > budget * 1.000_001 {
        return None;
    }

    // Valid physical IEEE-754 float surfaces take strict precedence over u32 bit reinterpretations.
    if f32_idx.next_valid_start(cell, cells).is_none()
        && eval_f32_window(f32_idx, range_start, cell, rows, cols, le).is_some()
    {
        return None;
    }

    let uslice = &idx.vals[cell..cell + cells];
    let mut flat = [0.0f64; 1024];
    let mut raw_bits = [0u64; 1024];
    for k in 0..cells {
        let uv = uslice[k];
        flat[k] = uv as f64;
        raw_bits[k] = uv as u64;
    }
    let mut denom = 0.0f64;
    let mut num = 0.0f64;
    for i in 0..cells {
        let diff = flat[i] - mean;
        denom += diff * diff;
        if i + cols < cells {
            num += diff * (flat[i + cols] - mean);
        }
    }
    if denom < 1e-12 {
        return None;
    }
    let ac = num / denom;
    if ac < 0.45 {
        return None;
    }

    let mut sum_delta = sum_dx;
    for r in 0..rows - 1 {
        let base = r * cols;
        let next = base + cols;
        for c in 0..cols {
            sum_delta += (flat[next + c] - flat[base + c]).abs();
        }
        if sum_delta > budget {
            return None;
        }
    }
    let mdsd = (sum_delta / delta_count) / sd;
    if !mdsd.is_finite() || mdsd >= 0.48 {
        return None;
    }

    let flat_s = &flat[..cells];
    let bits_s = &raw_bits[..cells];
    let mut uniq = bits_s.to_vec();
    uniq.sort_unstable();
    uniq.dedup();
    if uniq.len() < 6 || stripe_penalty_u64(bits_s, cols) < 0.5 {
        return None;
    }
    let all_inc = flat_s.windows(2).all(|w| w[1] >= w[0]);
    let all_dec = flat_s.windows(2).all(|w| w[1] <= w[0]);
    let max_v = flat_s.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if (all_inc || all_dec) && max_v > 65536.0 {
        return None;
    }

    let off = range_start + cell * 4;
    let mut u16_words = [0u16; 2048];
    for k in 0..cells {
        let o = off + k * 4;
        u16_words[k * 2] = read_u16(rom, o, le).unwrap_or(0);
        u16_words[k * 2 + 1] = read_u16(rom, o + 2, le).unwrap_or(0);
    }
    #[inline]
    fn u16_as_f64(v: u16) -> f64 {
        v as f64
    }
    if fast_slice_ac_mdsd(&u16_words[..cells * 2], u16_as_f64, rows, cols * 2, 64.0, 0.35, 0.38).is_some() {
        return None;
    }

    let mut grid = Vec::with_capacity(rows);
    for r in 0..rows {
        grid.push(flat_s[r * cols..(r + 1) * cols].to_vec());
    }
    if max_adj_diff_over_range(&grid) > 0.65 {
        return None;
    }
    let score = composite_quality_score(&ScoreInputs {
        rows,
        cols,
        z_address: off,
        smoothness: mdsd,
        confidence_score: (1.0 - (mdsd / 0.5).min(0.9)) * ac,
        has_x_axis: false,
        has_y_axis: false,
        axes_contiguous: false,
        first_row_is_flat: false,
    });
    Some(BlindV2Candidate {
        address: off,
        rows,
        cols,
        elem_bytes: 4,
        little_endian: le,
        dtype_label: if le { "u32_le" } else { "u32_be" }.to_string(),
        is_curve: false,
        x_axis: None,
        y_axis: None,
        smoothness: mdsd,
        p23: None,
        quality_score: score,
        status: if ac >= 0.65 && mdsd < 0.36 {
            DiscoveryStatus::StructurallySupported
        } else {
            DiscoveryStatus::Discovered
        },
        detection_source: "scan_32bit_tables".to_string(),
    })
}

#[inline]
fn u32_coarse_ok(idx: &U32PrefixIndex, cell: usize, rows: usize, cols: usize) -> bool {
    let cells = rows * cols;
    if idx.zero_or_max_count(cell, cells) * 2 > cells as u32 {
        return false;
    }
    let (_mean, sd) = idx.exact_mean_sd(cell, cells);
    if !sd.is_finite() || sd < 5.0 {
        return false;
    }
    let delta_count = (rows * (cols - 1) + (rows - 1) * cols) as f64;
    let budget = 0.48 * sd * delta_count;
    idx.sum_dx(cell, rows, cols) <= budget * 1.1
}

pub(super) fn scan_32bit_tables_in_ranges(
    rom: &[u8],
    kept_ranges: &[(usize, usize)],
    shapes: &[(usize, usize)],
) -> Vec<BlindV2Candidate> {
    let valid_shapes: Vec<(usize, usize)> = shapes
        .iter()
        .copied()
        .filter(|&(r, c)| r >= 3 && c >= 3 && r * c <= 1024)
        .collect();
    if valid_shapes.is_empty() {
        return Vec::new();
    }

    let u32_kinds: &[u8] = if rom.len() >= 65_536 {
        match crate::zero_xdf_scan::detect_global_endianness(rom) {
            crate::zero_xdf_scan::Endianness::LittleEndian => &[0, 1, 2],
            crate::zero_xdf_scan::Endianness::BigEndian => &[0, 1, 3],
        }
    } else {
        &[0, 1, 2, 3]
    };

    const TILE_BYTES: usize = 256 * 1024;
    let max_shape_bytes = valid_shapes
        .iter()
        .map(|&(r, c)| r * c * 4)
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

    let need_u32_le = u32_kinds.contains(&2);
    let need_u32_be = u32_kinds.contains(&3);

    let mut out = Vec::new();
    for (r_start, main_end, r_end) in tiles {
        let align_start = (r_start + 3) & !3;
        let total_4 = r_end.saturating_sub(align_start) / 4;
        let main_cells = main_end.saturating_sub(align_start) / 4;
        if total_4 < 9 || main_cells == 0 {
            continue;
        }
        let ((f32_le, f32_be), (u32_le, u32_be)) = rayon::join(
            || {
                rayon::join(
                    || F32PrefixIndex::build_from_rom(rom, align_start, r_end, true),
                    || F32PrefixIndex::build_from_rom(rom, align_start, r_end, false),
                )
            },
            || {
                rayon::join(
                    || need_u32_le.then(|| U32PrefixIndex::build_from_rom(rom, align_start, r_end, true)),
                    || need_u32_be.then(|| U32PrefixIndex::build_from_rom(rom, align_start, r_end, false)),
                )
            },
        );

        const U32_LARGE_ROM_SHAPES: &[(usize, usize)] = &[
            (4, 4),
            (6, 6),
            (8, 8),
            (8, 12),
            (12, 8),
            (12, 12),
            (12, 16),
            (16, 16),
        ];
        let mut jobs: Vec<(u8, (usize, usize))> = Vec::new();
        for &kind in u32_kinds {
            let max_run = match kind {
                0 => f32_le.max_valid_run(),
                1 => f32_be.max_valid_run(),
                _ => usize::MAX,
            };
            let base_shapes: &[(usize, usize)] = if (kind == 2 || kind == 3) && rom.len() >= 1_048_576 {
                U32_LARGE_ROM_SHAPES
            } else {
                &valid_shapes
            };
            for &(r, c) in base_shapes {
                if r * c <= max_run && valid_shapes.contains(&(r, c)) {
                    jobs.push((kind, (r, c)));
                }
            }
        }

        let range_hits: Vec<BlindV2Candidate> = jobs
            .par_iter()
            .flat_map_iter(|&(kind, (rows, cols))| {
                let mut local = Vec::new();
                let cells = rows * cols;
                if cells > total_4 {
                    return local;
                }
                let hit_skip = if total_4 >= 4096 { (cells / 4).clamp(4, 32) } else { 4 };
                let last_data_start = total_4.saturating_sub(cells);
                let max_cell = if main_end < r_end {
                    last_data_start.min(main_cells.saturating_sub(1))
                } else {
                    last_data_start
                };
                match kind {
                    0 | 1 => {
                        let le = kind == 0;
                        let idx = if le { &f32_le } else { &f32_be };
                        let mut c = 0usize;
                        while c <= max_cell {
                            if let Some(jump) = idx.next_valid_start(c, cells) {
                                c = jump;
                                continue;
                            }
                            if let Some(cand) = eval_f32_window(idx, align_start, c, rows, cols, le) {
                                local.push(cand);
                                c += hit_skip;
                            } else {
                                c += 1;
                            }
                        }
                    }
                    _ => {
                        let le = kind == 2;
                        let idx = if le {
                            u32_le.as_ref().expect("u32_le built")
                        } else {
                            u32_be.as_ref().expect("u32_be built")
                        };
                        let f_idx = if le { &f32_le } else { &f32_be };
                        let stride = if total_4 >= 4096 { 2usize } else { 1usize };
                        let mut c = 0usize;
                        while c <= max_cell {
                            if let Some(jump) = idx.bad_jump_next(c, rows, cols) {
                                c = jump;
                                continue;
                            }
                            if stride == 1 {
                                if let Some(cand) = eval_u32_window(rom, idx, f_idx, align_start, c, rows, cols, le) {
                                    local.push(cand);
                                    c += hit_skip;
                                } else {
                                    c += 1;
                                }
                            } else if u32_coarse_ok(idx, c, rows, cols) {
                                let lo = c.saturating_sub(1);
                                let hi = (c + 1).min(max_cell);
                                let mut hit = None;
                                for fine in lo..=hi {
                                    if let Some(cand) = eval_u32_window(rom, idx, f_idx, align_start, fine, rows, cols, le) {
                                        hit = Some((fine, cand));
                                        break;
                                    }
                                }
                                if let Some((best_c, cand)) = hit {
                                    local.push(cand);
                                    c = (best_c + hit_skip).max(c + 2);
                                } else {
                                    c += 2;
                                }
                            } else {
                                c += 2;
                            }
                        }
                    }
                }
                local
            })
            .collect();

        out.extend(range_hits);
    }
    out
}
