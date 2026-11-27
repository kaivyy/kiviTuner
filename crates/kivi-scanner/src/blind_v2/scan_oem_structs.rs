//! OEM-specific structural & floating-point curve detectors:
//! - Bosch/Siemens length-prefixed `[cols][rows][X][Y][Z]` and Motronic 8-bit `[id][len][axis]` structs
//! - Denso SuperH pointer descriptors (`u16_be`, `u8`, `i16_be`, `f32_be` 2-D/3-D and 1-D curves)
//! - 32-bit `f32` (LE/BE) 1-D calibration curves

use crate::map_quality::{BitWidth, GridFilter};
use crate::nms_score::{composite_quality_score, ScoreInputs};
use crate::scan_metrics::autocorrelation;
use rayon::prelude::*;

use super::axes::{axis_linearity, read_f32, read_i16, read_u16, read_u32};
use super::gates::{
    gearys_c_2d, max_adj_diff_over_range, mean_delta_over_sd, passes_anchored_curve_gates,
    passes_hard_filters_f64, passes_statistical_curve_gates, stripe_penalty_u64,
};
use super::prefix::F32PrefixIndex;
use super::{AxisDirection, BlindV2Axis, BlindV2Candidate, BlindV2Options, DiscoveryStatus};
use crate::axis_anchored::AxisKind;

pub(super) struct ZSurfaceSpec {
    pub(super) offset: usize,
    pub(super) rows: usize,
    pub(super) cols: usize,
    pub(super) elem_bytes: usize,
    pub(super) le: bool,
    pub(super) is_signed: bool,
    pub(super) is_float: bool,
}

pub(super) struct DecodedZSurface {
    pub(super) ac: f64,
    pub(super) mdsd: f64,
    pub(super) label: &'static str,
}

pub(super) fn try_decode_z_surface(rom: &[u8], spec: ZSurfaceSpec) -> Option<DecodedZSurface> {
    let ZSurfaceSpec {
        offset,
        rows,
        cols,
        elem_bytes,
        le,
        is_signed,
        is_float,
    } = spec;
    let cells = rows.checked_mul(cols)?;
    let byte_len = cells.checked_mul(elem_bytes)?;
    if cells < 8 || offset.checked_add(byte_len)? > rom.len() {
        return None;
    }

    let mut flat = Vec::with_capacity(cells);
    let mut raw_bits = Vec::with_capacity(cells);
    let mut has_neg = false;
    let mut has_pos = false;

    for k in 0..cells {
        let o = offset + k * elem_bytes;
        let (v, bits) = if elem_bytes == 1 && is_signed {
            let sv = rom[o] as i8;
            if sv < 0 {
                has_neg = true;
            } else {
                has_pos = true;
            }
            let vf = sv as f64;
            (vf, vf.round().to_bits())
        } else if elem_bytes == 1 {
            let b = rom[o];
            (b as f64, (b as f64).round().to_bits())
        } else if elem_bytes == 2 && is_signed {
            let sv = if le {
                i16::from_le_bytes([rom[o], rom[o + 1]])
            } else {
                i16::from_be_bytes([rom[o], rom[o + 1]])
            };
            if sv < 0 {
                has_neg = true;
            } else {
                has_pos = true;
            }
            (sv as f64, (sv as f64).round().to_bits())
        } else if elem_bytes == 2 {
            let uv = read_u16(rom, o, le)?;
            (uv as f64, (uv as f64).round().to_bits())
        } else if elem_bytes == 4 && is_float {
            let fv = read_f32(rom, o, le)?;
            if !F32PrefixIndex::is_valid_cal_float(fv) {
                return None;
            }
            (fv as f64, fv.to_bits() as u64)
        } else {
            return None;
        };
        flat.push(v);
        raw_bits.push(bits);
    }

    if is_signed && !(has_neg && has_pos) {
        return None;
    }
    if !passes_hard_filters_f64(&flat, elem_bytes) {
        return None;
    }
    if stripe_penalty_u64(&raw_bits, cols) < 0.5 {
        return None;
    }

    let n = cells as f64;
    let mean = flat.iter().sum::<f64>() / n;
    let var = flat.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / n;
    let sd = var.sqrt();
    let min_sd = match elem_bytes {
        1 => 4.0,
        2 => 8.0,
        _ => 0.01,
    };
    if !sd.is_finite() || sd < min_sd || sd > 100_000.0 {
        return None;
    }

    let ac = autocorrelation(&flat, cols);
    if !ac.is_finite() || ac < 0.35 {
        return None;
    }

    let mut grid = Vec::with_capacity(rows);
    for r in 0..rows {
        grid.push(flat[r * cols..(r + 1) * cols].to_vec());
    }
    let mdsd = mean_delta_over_sd(&grid);
    if !mdsd.is_finite() || mdsd >= 0.48 {
        return None;
    }
    if rows >= 3 && cols >= 3 && max_adj_diff_over_range(&grid) > 0.70 {
        return None;
    }
    let gc = gearys_c_2d(&grid);
    let max_gc = if elem_bytes == 1 { 0.30 } else { 0.34 };
    if !gc.is_finite() || gc >= max_gc {
        return None;
    }
    if elem_bytes <= 2 && !is_signed {
        let bw = if elem_bytes == 1 { BitWidth::Bits8 } else { BitWidth::Bits16 };
        if !GridFilter::default().admits(&grid, bw) {
            return None;
        }
    } else if elem_bytes == 4 {
        let mut uniq = raw_bits;
        uniq.sort_unstable();
        uniq.dedup();
        if uniq.len() < 6 {
            return None;
        }
    }

    let label = match (elem_bytes, le, is_signed, is_float) {
        (1, _, true, _) => "i8",
        (1, _, false, _) => "u8",
        (2, true, true, _) => "i16_le",
        (2, false, true, _) => "i16_be",
        (2, true, false, _) => "u16_le",
        (2, false, false, _) => "u16_be",
        (4, true, _, true) => "f32_le",
        _ => "f32_be",
    };
    Some(DecodedZSurface { ac, mdsd, label })
}

pub(super) fn read_monotonic_u16_axis(
    rom: &[u8],
    offset: usize,
    len: usize,
    le: bool,
) -> Option<BlindV2Axis> {
    if !(3..=64).contains(&len) || offset.checked_add(len * 2)? > rom.len() {
        return None;
    }
    let mut vals = Vec::with_capacity(len);
    for i in 0..len {
        vals.push(read_u16(rom, offset + i * 2, le)? as f64);
    }
    let inc = vals.windows(2).all(|w| w[1] > w[0] && (w[1] - w[0]) <= 16_384.0);
    let dec = vals.windows(2).all(|w| w[0] > w[1] && (w[0] - w[1]) <= 16_384.0);
    if !inc && !dec {
        return None;
    }
    let lo = vals.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = vals.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let span = hi - lo;
    if hi >= 65_535.0 || span < 8.0 {
        return None;
    }
    if lo > 0.0 && span / lo.abs() < 0.05 {
        return None;
    }
    let (lin, mean_step) = axis_linearity(&vals);
    if lin < 0.25 || (lo == 0.0 && mean_step <= 1.01) {
        return None;
    }
    Some(BlindV2Axis {
        offset,
        length: len,
        elem_bytes: 2,
        little_endian: le,
        direction: if inc { AxisDirection::Increasing } else { AxisDirection::Decreasing },
        kind: if (200.0..=3000.0).contains(&lo) && (4000.0..=20000.0).contains(&hi) {
            AxisKind::Rpm
        } else {
            AxisKind::Load
        },
        values: vals,
    })
}

pub(super) fn read_monotonic_i16_axis(
    rom: &[u8],
    offset: usize,
    len: usize,
    le: bool,
) -> Option<BlindV2Axis> {
    if !(3..=64).contains(&len) || offset.checked_add(len * 2)? > rom.len() {
        return None;
    }
    let mut vals = Vec::with_capacity(len);
    for i in 0..len {
        vals.push(read_i16(rom, offset + i * 2, le)? as f64);
    }
    let inc = vals.windows(2).all(|w| w[1] > w[0] && (w[1] - w[0]) <= 16_384.0);
    let dec = vals.windows(2).all(|w| w[0] > w[1] && (w[0] - w[1]) <= 16_384.0);
    if !inc && !dec {
        return None;
    }
    let lo = vals.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = vals.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if !((lo < 0.0 && hi > 0.0) || hi <= 0.0) {
        return None;
    }
    let span = hi - lo;
    if span < 8.0 {
        return None;
    }
    let (lin, mean_step) = axis_linearity(&vals);
    if lin < 0.25 || mean_step < 1.0 {
        return None;
    }
    Some(BlindV2Axis {
        offset,
        length: len,
        elem_bytes: 2,
        little_endian: le,
        direction: if inc { AxisDirection::Increasing } else { AxisDirection::Decreasing },
        kind: AxisKind::Load,
        values: vals,
    })
}

pub(super) fn read_monotonic_u8_axis(
    rom: &[u8],
    offset: usize,
    len: usize,
) -> Option<BlindV2Axis> {
    if !(3..=32).contains(&len) || offset.checked_add(len)? > rom.len() {
        return None;
    }
    let bytes = &rom[offset..offset + len];
    let inc = bytes.windows(2).all(|w| w[1] > w[0] && (w[1] - w[0]) <= 64);
    let dec = bytes.windows(2).all(|w| w[0] > w[1] && (w[0] - w[1]) <= 64);
    if !inc && !dec {
        return None;
    }
    let vals: Vec<f64> = bytes.iter().map(|&b| b as f64).collect();
    let lo = vals.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = vals.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let span = hi - lo;
    let (lin, mean_step) = axis_linearity(&vals);
    let not_counter = (mean_step >= 1.5 || lo >= 8.0) && !(lo <= 1.0 && mean_step <= 1.05);
    if (hi >= 255.0 && lo >= 240.0) || span < 12.0 || lin < 0.45 || !not_counter {
        return None;
    }
    Some(BlindV2Axis {
        offset,
        length: len,
        elem_bytes: 1,
        little_endian: false,
        direction: if inc { AxisDirection::Increasing } else { AxisDirection::Decreasing },
        kind: if hi >= 120.0 && mean_step >= 4.0 {
            AxisKind::Rpm
        } else {
            AxisKind::Load
        },
        values: vals,
    })
}

pub(super) fn read_monotonic_f32_axis(
    rom: &[u8],
    offset: usize,
    len: usize,
    le: bool,
) -> Option<BlindV2Axis> {
    if !(3..=64).contains(&len) || !offset.is_multiple_of(4) || offset.checked_add(len * 4)? > rom.len() {
        return None;
    }
    let mut vals = Vec::with_capacity(len);
    for i in 0..len {
        let fv = read_f32(rom, offset + i * 4, le)?;
        if !fv.is_finite() || fv.is_subnormal() || fv.abs() > 100_000.0 {
            return None;
        }
        vals.push(fv as f64);
    }
    let inc = vals.windows(2).all(|w| w[1] > w[0]);
    let dec = vals.windows(2).all(|w| w[0] > w[1]);
    if !inc && !dec {
        return None;
    }
    let lo = vals.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = vals.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if (hi - lo) < 1e-3 {
        return None;
    }
    let (lin, _) = axis_linearity(&vals);
    if lin <= 0.15 {
        return None;
    }
    Some(BlindV2Axis {
        offset,
        length: len,
        elem_bytes: 4,
        little_endian: le,
        direction: if inc { AxisDirection::Increasing } else { AxisDirection::Decreasing },
        kind: if hi >= 3000.0 { AxisKind::Rpm } else { AxisKind::Load },
        values: vals,
    })
}

fn parse_motronic_u8_axis(rom: &[u8], offset: usize, len: usize) -> Option<BlindV2Axis> {
    if !(4..=24).contains(&len) || offset.checked_add(len)? > rom.len() {
        return None;
    }
    let bytes = &rom[offset..offset + len];
    // Form 1: directly monotonic u8 breakpoints
    let mono_inc = bytes.windows(2).all(|w| w[1] > w[0] && (w[1] - w[0]) <= 64);
    let mono_dec = bytes.windows(2).all(|w| w[0] > w[1] && (w[0] - w[1]) <= 64);
    if mono_inc || mono_dec {
        let vals: Vec<f64> = bytes.iter().map(|&b| b as f64).collect();
        let lo = vals.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = vals.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let (lin, mean_step) = axis_linearity(&vals);
        if lin >= 0.55 && hi - lo >= 12.0 && (mean_step >= 2.0 || lo >= 8.0) {
            return Some(BlindV2Axis {
                offset,
                length: len,
                elem_bytes: 1,
                little_endian: false,
                direction: if mono_inc { AxisDirection::Increasing } else { AxisDirection::Decreasing },
                kind: if hi >= 120.0 { AxisKind::Rpm } else { AxisKind::Load },
                values: vals,
            });
        }
    }
    // Form 2: Bosch Motronic reverse-delta encoding (each step >= 1, total <= 255)
    if bytes.iter().all(|&b| (1..=80).contains(&b)) {
        let sum: usize = bytes.iter().map(|&b| b as usize).sum();
        if (16..=255).contains(&sum) && bytes[..len - 1].iter().any(|&b| b >= 2) {
            let mut vals = Vec::with_capacity(len);
            let mut cur = 256.0 - sum as f64;
            for &b in bytes.iter().rev() {
                vals.push(cur);
                cur += b as f64;
            }
            return Some(BlindV2Axis {
                offset,
                length: len,
                elem_bytes: 1,
                little_endian: false,
                direction: AxisDirection::Increasing,
                kind: AxisKind::Load,
                values: vals,
            });
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn try_push_2d_struct_layouts(
    rom: &[u8],
    r_end: usize,
    z_off: usize,
    d1: usize,
    d2: usize,
    ax1: &BlindV2Axis,
    ax2: &BlindV2Axis,
    le: bool,
    tries: &[(usize, bool)],
    min_ac: f64,
    max_mdsd: f64,
    source_prefix: &str,
    local: &mut Vec<BlindV2Candidate>,
) -> bool {
    let layouts = [(d2, d1, ax2, ax1), (d1, d2, ax1, ax2)];
    for &(rows, cols, y_ax, x_ax) in &layouts {
        for &(eb, is_signed) in tries {
            let Some(span) = rows.checked_mul(cols).and_then(|c| c.checked_mul(eb)) else {
                continue;
            };
            if z_off.saturating_add(span) > r_end {
                continue;
            }
            if let Some(decoded) = try_decode_z_surface(
                rom,
                ZSurfaceSpec {
                    offset: z_off,
                    rows,
                    cols,
                    elem_bytes: eb,
                    le: le && eb > 1,
                    is_signed,
                    is_float: false,
                },
            ) {
                let DecodedZSurface { ac, mdsd, label } = decoded;
                if ac < min_ac || mdsd >= max_mdsd || (eb == 2 && mdsd >= 0.45 && ac < 0.45) {
                    continue;
                }
                let score = composite_quality_score(&ScoreInputs {
                    rows,
                    cols,
                    z_address: z_off,
                    smoothness: mdsd,
                    confidence_score: (1.0 - mdsd.min(0.9)) * ac.max(0.5),
                    has_x_axis: true,
                    has_y_axis: true,
                    axes_contiguous: true,
                    first_row_is_flat: false,
                }) + 0.04;
                local.push(BlindV2Candidate {
                    address: z_off,
                    rows,
                    cols,
                    elem_bytes: eb,
                    little_endian: le && eb > 1,
                    dtype_label: label.to_string(),
                    is_curve: false,
                    x_axis: Some(x_ax.clone()),
                    y_axis: Some(y_ax.clone()),
                    smoothness: mdsd,
                    p23: None,
                    quality_score: score,
                    status: DiscoveryStatus::StructurallySupported,
                    detection_source: format!("{source_prefix}({label})"),
                });
                return true;
            }
        }
    }
    false
}

#[inline]
fn fast_3_mono_u8(rom: &[u8], off: usize) -> bool {
    let (a, b, c) = (rom[off], rom[off + 1], rom[off + 2]);
    (b > a && c > b && b - a <= 64 && c - b <= 64)
        || (a > b && b > c && a - b <= 64 && b - c <= 64)
}

#[inline]
fn fast_3_mono_16(rom: &[u8], off: usize, le: bool) -> bool {
    let (Some(a), Some(b), Some(c)) = (
        read_u16(rom, off, le),
        read_u16(rom, off + 2, le),
        read_u16(rom, off + 4, le),
    ) else {
        return false;
    };
    let u_ok = (b > a && c > b && (b - a) <= 16_384 && (c - b) <= 16_384)
        || (a > b && b > c && (a - b) <= 16_384 && (b - c) <= 16_384);
    if u_ok {
        return true;
    }
    let (sa, sb, sc) = (a as i16 as i32, b as i16 as i32, c as i16 as i32);
    (sb > sa && sc > sb && (sb - sa) <= 16_384 && (sc - sb) <= 16_384)
        || (sa > sb && sb > sc && (sa - sb) <= 16_384 && (sb - sc) <= 16_384)
}

/// Scans kept ranges for inline OEM length-prefixed structs:
/// - Bosch ME7/ME9 & Siemens `[cols:u16][rows:u16][X:cols][Y:rows][Z:rows*cols]`
/// - Bosch/Siemens `[d1:u8][d2:u8][ax1][ax2][Z]` (`u8` or `u16`/`i16` axes)
/// - Classic Bosch Motronic 8-bit `[id_y:u8][len_y:u8][axis_y][id_x:u8][len_x:u8][axis_x][Z]`
pub(super) fn scan_bosch_siemens_structs(
    rom: &[u8],
    kept_ranges: &[(usize, usize)],
    opts: &BlindV2Options,
) -> Vec<BlindV2Candidate> {
    const TILE_BYTES: usize = 64 * 1024;
    let mut tiles: Vec<(usize, usize)> = Vec::new();
    for &(r_start, r_end) in kept_ranges {
        let mut cur = r_start;
        while cur < r_end {
            let tile_end = (cur + TILE_BYTES).min(r_end);
            let dec_end = (tile_end + 1024).min(r_end);
            tiles.push((cur, dec_end));
            cur = tile_end;
        }
    }

    let endians = if !opts.scan_both_16bit_endians {
        [true].as_slice()
    } else if rom.len() >= 131_072 {
        match crate::zero_xdf_scan::detect_global_endianness(rom) {
            crate::zero_xdf_scan::Endianness::LittleEndian => [true].as_slice(),
            crate::zero_xdf_scan::Endianness::BigEndian => [false].as_slice(),
        }
    } else {
        [true, false].as_slice()
    };

    let per_tile: Vec<Vec<BlindV2Candidate>> = tiles
        .par_iter()
        .map(|&(r_start, r_end)| {
            let mut local = Vec::new();
            let mut off = (r_start + 1) & !1;
            let main_end = (r_start + TILE_BYTES).min(r_end);
            let tries_16_dom = if opts.include_8bit {
                [(2usize, false), (2, true), (1, false), (1, true)].as_slice()
            } else {
                [(2usize, false), (2, true)].as_slice()
            };
            let tries_8_dom = [(1usize, false), (1, true), (2, false), (2, true)].as_slice();

            while off + 16 <= main_end {
                let mut matched_16 = false;
                for &le in endians {
                    let Some(d1) = read_u16(rom, off, le).map(|v| v as usize) else {
                        break;
                    };
                    if !(3..=32).contains(&d1) {
                        continue;
                    }
                    let Some(d2) = read_u16(rom, off + 2, le).map(|v| v as usize) else {
                        break;
                    };
                    if !(3..=32).contains(&d2) || d1 * d2 < 12 {
                        continue;
                    }
                    let ax1_off = off + 4;
                    let ax2_off = ax1_off + d1 * 2;
                    let z_off = ax2_off + d2 * 2;
                    if z_off + d1 * d2 > r_end {
                        continue;
                    }
                    let Some(ax1) = read_monotonic_u16_axis(rom, ax1_off, d1, le)
                        .or_else(|| read_monotonic_i16_axis(rom, ax1_off, d1, le))
                    else {
                        continue;
                    };
                    let Some(ax2) = read_monotonic_u16_axis(rom, ax2_off, d2, le)
                        .or_else(|| read_monotonic_i16_axis(rom, ax2_off, d2, le))
                    else {
                        continue;
                    };
                    if try_push_2d_struct_layouts(
                        rom,
                        r_end,
                        z_off,
                        d1,
                        d2,
                        &ax1,
                        &ax2,
                        le,
                        tries_16_dom,
                        0.35,
                        0.48,
                        "bosch_siemens_struct",
                        &mut local,
                    ) {
                        matched_16 = true;
                        break;
                    }
                }
                off += if matched_16 { 8 } else { 2 };
            }

            if r_end.saturating_sub(r_start) >= 16 {
                let allow_motronic = opts.include_8bit && rom.len() <= 2_097_152;
                let mut off8 = r_start;
                while off8 + 16 <= main_end && off8 + 16 <= r_end {
                    let mut accepted = false;
                    let d1 = rom[off8] as usize;
                    let d2 = rom[off8 + 1] as usize;
                    if (3..=32).contains(&d1) && (3..=32).contains(&d2) && d1 * d2 >= 12 {
                        let ax1_off = off8 + 2;
                        for &le in endians {
                            if ax1_off + (d1 + d2) * 2 + d1 * d2 <= r_end
                                && fast_3_mono_16(rom, ax1_off, le)
                            {
                                let ax2_off = ax1_off + d1 * 2;
                                let z_off = ax2_off + d2 * 2;
                                if let (Some(ax1), Some(ax2)) = (
                                    read_monotonic_u16_axis(rom, ax1_off, d1, le)
                                        .or_else(|| read_monotonic_i16_axis(rom, ax1_off, d1, le)),
                                    read_monotonic_u16_axis(rom, ax2_off, d2, le)
                                        .or_else(|| read_monotonic_i16_axis(rom, ax2_off, d2, le)),
                                ) {
                                    // Reject when the 2 header bytes [d1, d2] are actually an axis word continuing smoothly into ax1.
                                    let hdr_is_prev_word = read_u16(rom, off8, le).is_some_and(|pu| {
                                        let prev = if ax1.values[0] < 0.0 { (pu as i16) as f64 } else { pu as f64 };
                                        let step = ax1.values[1] - ax1.values[0];
                                        let dp = ax1.values[0] - prev;
                                        step.abs() > 1e-9
                                            && (dp > 0.0) == (step > 0.0)
                                            && dp.abs() <= step.abs() * 4.0
                                            && dp.abs() * 4.0 >= step.abs()
                                    });
                                    if hdr_is_prev_word {
                                        continue;
                                    }
                                    if try_push_2d_struct_layouts(
                                        rom,
                                        r_end,
                                        z_off,
                                        d1,
                                        d2,
                                        &ax1,
                                        &ax2,
                                        le,
                                        tries_16_dom,
                                        0.40,
                                        0.44,
                                        "bosch_siemens_struct_u8hdr",
                                        &mut local,
                                    ) {
                                        accepted = true;
                                        break;
                                    }
                                }
                            }
                        }

                        if !accepted
                            && opts.include_8bit
                            && ax1_off + d1 + d2 + d1 * d2 <= r_end
                            && fast_3_mono_u8(rom, ax1_off)
                        {
                            let ax2_off = ax1_off + d1;
                            let z_off = ax2_off + d2;
                            if let (Some(ax1), Some(ax2)) = (
                                read_monotonic_u8_axis(rom, ax1_off, d1),
                                read_monotonic_u8_axis(rom, ax2_off, d2),
                            ) {
                                if try_push_2d_struct_layouts(
                                    rom,
                                    r_end,
                                    z_off,
                                    d1,
                                    d2,
                                    &ax1,
                                    &ax2,
                                    endians[0],
                                    tries_8_dom,
                                    0.40,
                                    0.44,
                                    "bosch_siemens_struct_u8hdr",
                                    &mut local,
                                ) {
                                    accepted = true;
                                }
                            }
                        }
                    }

                    if !accepted && allow_motronic && off8 + 28 <= main_end && off8 + 28 <= r_end {
                        let id_y = d1 as u8;
                        let len_y = d2;
                        if (0x20..=0x7F).contains(&id_y)
                            && (4..=24).contains(&len_y)
                            && rom[off8 + 2] != 0
                            && rom[off8 + 2] != rom[off8 + 3]
                            && rom[off8 + 2].abs_diff(rom[off8 + 3]) <= 80
                        {
                            let x_hdr = off8 + 2 + len_y;
                            if x_hdr + 4 <= r_end {
                                let id_x = rom[x_hdr];
                                let len_x = rom[x_hdr + 1] as usize;
                                if (0x20..=0x7F).contains(&id_x)
                                    && id_x != id_y
                                    && (4..=24).contains(&len_x)
                                    && len_y * len_x >= 24
                                {
                                    let z_off = x_hdr + 2 + len_x;
                                    if z_off + len_y * len_x <= r_end {
                                        if let (Some(y_ax), Some(x_ax)) = (
                                            parse_motronic_u8_axis(rom, off8 + 2, len_y),
                                            parse_motronic_u8_axis(rom, x_hdr + 2, len_x),
                                        ) {
                                            if let Some(decoded) = try_decode_z_surface(
                                                rom,
                                                ZSurfaceSpec {
                                                    offset: z_off,
                                                    rows: len_y,
                                                    cols: len_x,
                                                    elem_bytes: 1,
                                                    le: false,
                                                    is_signed: false,
                                                    is_float: false,
                                                },
                                            ) {
                                                let DecodedZSurface { ac, mdsd, label } = decoded;
                                                if ac >= 0.40 && mdsd < 0.42 {
                                                    let score = composite_quality_score(&ScoreInputs {
                                                        rows: len_y,
                                                        cols: len_x,
                                                        z_address: z_off,
                                                        smoothness: mdsd,
                                                        confidence_score: (1.0 - mdsd.min(0.9)) * ac,
                                                        has_x_axis: true,
                                                        has_y_axis: true,
                                                        axes_contiguous: true,
                                                        first_row_is_flat: false,
                                                    }) + 0.04;
                                                    local.push(BlindV2Candidate {
                                                        address: z_off,
                                                        rows: len_y,
                                                        cols: len_x,
                                                        elem_bytes: 1,
                                                        little_endian: false,
                                                        dtype_label: label.to_string(),
                                                        is_curve: false,
                                                        x_axis: Some(x_ax),
                                                        y_axis: Some(y_ax),
                                                        smoothness: mdsd,
                                                        p23: None,
                                                        quality_score: score,
                                                        status: DiscoveryStatus::StructurallySupported,
                                                        detection_source: "motronic_8bit_struct".to_string(),
                                                    });
                                                    accepted = true;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    off8 += if accepted { 6 } else { 1 };
                }
            }
            local
        })
        .collect();

    per_tile.into_iter().flatten().collect()
}

/// Bridges SuperH Denso pointer descriptors (`u16_be`, `u8`, `i16_be`, and `f32_be` tables).
pub(super) fn scan_denso_sh_descriptors_v2(
    rom: &[u8],
    opts: &BlindV2Options,
) -> Vec<BlindV2Candidate> {
    let len = rom.len();
    if !(65_536..=2_100_000).contains(&len) {
        return Vec::new();
    }

    const DESC_TILE: usize = 128 * 1024;
    let max_off = len.saturating_sub(15);
    let desc_tiles: Vec<(usize, usize)> = (0..max_off)
        .step_by(DESC_TILE)
        .map(|s| (s, (s + DESC_TILE).min(max_off)))
        .collect();

    let mut out: Vec<BlindV2Candidate> = desc_tiles
        .par_iter()
        .flat_map_iter(|&(t_start, t_end)| {
            let mut local = Vec::new();
            let mut off = (t_start + 3) & !3;
            while off < t_end {
                let sx = u16::from_be_bytes([rom[off], rom[off + 1]]) as usize;
                let sy = u16::from_be_bytes([rom[off + 2], rom[off + 3]]) as usize;
                if (3..=64).contains(&sx) && (3..=64).contains(&sy) {
                    let px = read_u32(rom, off + 4, false).unwrap_or(0) as usize;
                    let py = read_u32(rom, off + 8, false).unwrap_or(0) as usize;
                    let pz = read_u32(rom, off + 12, false).unwrap_or(0) as usize;
                    if px != 0
                        && py != 0
                        && px != py
                        && py != pz
                        && px != pz
                        && px < len
                        && py < len
                        && pz < len
                    {
                        let mut matched_u16 = false;
                        if pz > px && pz > py {
                            if let (Some(x_ax), Some(y_ax)) = (
                                read_monotonic_u16_axis(rom, px, sx, false),
                                read_monotonic_u16_axis(rom, py, sy, false),
                            ) {
                                let tries = if opts.include_8bit {
                                    [(2usize, false), (2, true), (1, false)].as_slice()
                                } else {
                                    [(2usize, false), (2, true)].as_slice()
                                };
                                for &(eb, is_signed) in tries {
                                    if let Some(decoded) = try_decode_z_surface(
                                        rom,
                                        ZSurfaceSpec {
                                            offset: pz,
                                            rows: sy,
                                            cols: sx,
                                            elem_bytes: eb,
                                            le: false,
                                            is_signed,
                                            is_float: false,
                                        },
                                    ) {
                                        let DecodedZSurface { ac, mdsd, label } = decoded;
                                        let score = composite_quality_score(&ScoreInputs {
                                            rows: sy,
                                            cols: sx,
                                            z_address: pz,
                                            smoothness: mdsd,
                                            confidence_score: (1.0 - mdsd.min(0.9)) * ac,
                                            has_x_axis: true,
                                            has_y_axis: true,
                                            axes_contiguous: false,
                                            first_row_is_flat: false,
                                        }) + 0.06;
                                        local.push(BlindV2Candidate {
                                            address: pz,
                                            rows: sy,
                                            cols: sx,
                                            elem_bytes: eb,
                                            little_endian: false,
                                            dtype_label: label.to_string(),
                                            is_curve: false,
                                            x_axis: Some(x_ax),
                                            y_axis: Some(y_ax),
                                            smoothness: mdsd,
                                            p23: None,
                                            quality_score: score,
                                            status: DiscoveryStatus::StructurallySupported,
                                            detection_source: format!("denso_sh_descriptor({label})"),
                                        });
                                        matched_u16 = true;
                                        break;
                                    }
                                }
                            }
                        }

                        if !matched_u16
                            && opts.include_32bit
                            && (3..=48).contains(&sx)
                            && sx * sy >= 12
                            && px.is_multiple_of(4)
                            && py.is_multiple_of(4)
                            && pz.is_multiple_of(4)
                        {
                            match rom.get(off + 16).copied() {
                                Some(t @ (0x00 | 0x04 | 0x08 | 0x10)) => {
                                    let type_info = match t {
                                        0x04 => opts.include_8bit.then_some((1usize, false, false, "u8")),
                                        0x08 => Some((2usize, false, false, "u16_be")),
                                        0x10 => Some((2usize, true, false, "i16_be")),
                                        _ => Some((4usize, false, true, "f32_be")),
                                    };
                                    if let Some((eb, is_signed, is_float, label)) = type_info {
                                        let cells = sy * sx;
                                        let byte_len = cells * eb;
                                        if pz.checked_add(byte_len).is_some_and(|e| e <= len)
                                            && !rom[pz..pz + byte_len].iter().all(|&b| b == 0xFF)
                                        {
                                            if let (Some(x_ax), Some(y_ax)) = (
                                                read_monotonic_f32_axis(rom, px, sx, false),
                                                read_monotonic_f32_axis(rom, py, sy, false),
                                            ) {
                                                let mut flat = Vec::with_capacity(cells);
                                                let mut valid = true;
                                                for k in 0..cells {
                                                    let o = pz + k * eb;
                                                    let v = if is_float {
                                                        let Some(fv) = read_f32(rom, o, false) else {
                                                            valid = false;
                                                            break;
                                                        };
                                                        if !F32PrefixIndex::is_valid_cal_float(fv) {
                                                            valid = false;
                                                            break;
                                                        }
                                                        fv as f64
                                                    } else if eb == 2 && is_signed {
                                                        i16::from_be_bytes([rom[o], rom[o + 1]]) as f64
                                                    } else if eb == 2 {
                                                        read_u16(rom, o, false).unwrap_or(0) as f64
                                                    } else {
                                                        rom[o] as f64
                                                    };
                                                    flat.push(v);
                                                }
                                                if valid {
                                                    let n = cells as f64;
                                                    let mean = flat.iter().sum::<f64>() / n;
                                                    let sd = (flat
                                                        .iter()
                                                        .map(|v| (v - mean) * (v - mean))
                                                        .sum::<f64>()
                                                        / n)
                                                        .sqrt();
                                                    let mdsd = if !sd.is_finite() || sd < 1e-9 {
                                                        0.0
                                                    } else {
                                                        let mut tot_d = 0.0f64;
                                                        for r in 0..sy {
                                                            let b = r * sx;
                                                            for c in 0..sx - 1 {
                                                                tot_d += (flat[b + c + 1] - flat[b + c]).abs();
                                                            }
                                                        }
                                                        for r in 0..sy - 1 {
                                                            let b = r * sx;
                                                            let nxt = b + sx;
                                                            for c in 0..sx {
                                                                tot_d += (flat[nxt + c] - flat[b + c]).abs();
                                                            }
                                                        }
                                                        let edges = (sy * (sx - 1) + (sy - 1) * sx) as f64;
                                                        ((tot_d / edges) / sd).min(1.0)
                                                    };
                                                    let score = composite_quality_score(&ScoreInputs {
                                                        rows: sy,
                                                        cols: sx,
                                                        z_address: pz,
                                                        smoothness: mdsd,
                                                        confidence_score: (1.0 - mdsd.min(0.9)) * 0.95,
                                                        has_x_axis: true,
                                                        has_y_axis: true,
                                                        axes_contiguous: false,
                                                        first_row_is_flat: false,
                                                    }) + 0.25;
                                                    local.push(BlindV2Candidate {
                                                        address: pz,
                                                        rows: sy,
                                                        cols: sx,
                                                        elem_bytes: eb,
                                                        little_endian: false,
                                                        dtype_label: label.to_string(),
                                                        is_curve: false,
                                                        x_axis: Some(x_ax),
                                                        y_axis: Some(y_ax),
                                                        smoothness: mdsd,
                                                        p23: None,
                                                        quality_score: score,
                                                        status: DiscoveryStatus::StructurallySupported,
                                                        detection_source: format!("denso_sh_descriptor({label})"),
                                                    });
                                                }
                                            }
                                        }
                                    }
                                }
                                _ => {
                                    if let (Some(x_ax), Some(y_ax)) = (
                                        read_monotonic_f32_axis(rom, px, sx, false),
                                        read_monotonic_f32_axis(rom, py, sy, false),
                                    ) {
                                        if let Some(decoded) = try_decode_z_surface(
                                            rom,
                                            ZSurfaceSpec {
                                                offset: pz,
                                                rows: sy,
                                                cols: sx,
                                                elem_bytes: 4,
                                                le: false,
                                                is_signed: false,
                                                is_float: true,
                                            },
                                        ) {
                                            let DecodedZSurface { ac, mdsd, label } = decoded;
                                            let score = composite_quality_score(&ScoreInputs {
                                                rows: sy,
                                                cols: sx,
                                                z_address: pz,
                                                smoothness: mdsd,
                                                confidence_score: (1.0 - mdsd.min(0.9)) * ac,
                                                has_x_axis: true,
                                                has_y_axis: true,
                                                axes_contiguous: false,
                                                first_row_is_flat: false,
                                            }) + 0.06;
                                            local.push(BlindV2Candidate {
                                                address: pz,
                                                rows: sy,
                                                cols: sx,
                                                elem_bytes: 4,
                                                little_endian: false,
                                                dtype_label: label.to_string(),
                                                is_curve: false,
                                                x_axis: Some(x_ax),
                                                y_axis: Some(y_ax),
                                                smoothness: mdsd,
                                                p23: None,
                                                quality_score: score,
                                                status: DiscoveryStatus::StructurallySupported,
                                                detection_source: format!("denso_sh_descriptor({label})"),
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                off += 4;
            }
            local
        })
        .collect();

    out.sort_by(|a, b| {
        a.address
            .cmp(&b.address)
            .then(b.rows.cmp(&a.rows))
            .then(b.cols.cmp(&a.cols))
            .then(a.elem_bytes.cmp(&b.elem_bytes))
            .then_with(|| a.dtype_label.cmp(&b.dtype_label))
    });
    out
}

/// Scans SuperH Denso 1-D curve pointer descriptors `[len:u16_be][flags:u16_be][px:u32_be][pz:u32_be]`
/// where `px` references a monotonic `f32_be` axis and `pz` stores unoccupied `f32_be`, `u8`, or `u16_be` values.
pub(super) fn scan_denso_sh_curves_v2(
    rom: &[u8],
    include_8bit: bool,
    occupied: &mut [bool],
) -> Vec<BlindV2Candidate> {
    let len = rom.len();
    if !(65_536..=2_100_000).contains(&len) {
        return Vec::new();
    }

    const DESC_TILE: usize = 128 * 1024;
    let max_off = len.saturating_sub(11);
    let desc_tiles: Vec<(usize, usize)> = (0..max_off)
        .step_by(DESC_TILE)
        .map(|s| (s, (s + DESC_TILE).min(max_off)))
        .collect();

    let mut raw_hits: Vec<BlindV2Candidate> = desc_tiles
        .par_iter()
        .flat_map_iter(|&(t_start, t_end)| {
            let mut local = Vec::new();
            let mut off = (t_start + 3) & !3;
            while off < t_end {
                let pts = u16::from_be_bytes([rom[off], rom[off + 1]]) as usize;
                let flags = u16::from_be_bytes([rom[off + 2], rom[off + 3]]);
                if (4..=64).contains(&pts) && (flags == 0 || flags == 0x0400 || flags == 0x0800 || flags <= 2) {
                    let px = read_u32(rom, off + 4, false).unwrap_or(0) as usize;
                    let pz = read_u32(rom, off + 8, false).unwrap_or(0) as usize;
                    let mut matched_f32_ax = false;

                    if (flags & 0xFF) == 0 && px != 0 && pz != 0 && px != pz && px.is_multiple_of(4) && px < len {
                        let type_spec = match flags >> 8 {
                            0x00 => Some((4usize, true, "f32_be")),
                            0x04 if include_8bit => Some((1usize, false, "u8")),
                            0x08 => Some((2usize, false, "u16_be")),
                            _ => None,
                        };
                        if let Some((eb, is_float, label)) = type_spec {
                            let span = pts * eb;
                            if (eb == 1 || pz.is_multiple_of(eb))
                                && pz.checked_add(span).is_some_and(|e| e <= occupied.len() && e <= len)
                                && !occupied[pz..pz + span].iter().any(|&b| b)
                                && !rom[pz..pz + span].iter().all(|&b| b == 0xFF)
                            {
                                if let Some(x_ax) = read_monotonic_f32_axis(rom, px, pts, false) {
                                    let mut vals = Vec::with_capacity(pts);
                                    let mut ok = true;
                                    for k in 0..pts {
                                        let o = pz + k * eb;
                                        let v = if is_float {
                                            let Some(fv) = read_f32(rom, o, false) else {
                                                ok = false;
                                                break;
                                            };
                                            if !F32PrefixIndex::is_valid_cal_float(fv) {
                                                ok = false;
                                                break;
                                            }
                                            fv as f64
                                        } else if eb == 2 {
                                            read_u16(rom, o, false).unwrap_or(0) as f64
                                        } else {
                                            rom[o] as f64
                                        };
                                        vals.push(v);
                                    }
                                    if ok {
                                        let n = pts as f64;
                                        let mean = vals.iter().sum::<f64>() / n;
                                        let var = vals.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / n;
                                        let sd = var.sqrt();
                                        let smooth = if !sd.is_finite() || sd < 1e-9 {
                                            0.0
                                        } else {
                                            let delta_sum: f64 = vals.windows(2).map(|w| (w[1] - w[0]).abs()).sum();
                                            (delta_sum / (pts - 1) as f64) / sd
                                        };
                                        local.push(BlindV2Candidate {
                                            address: pz,
                                            rows: 1,
                                            cols: pts,
                                            elem_bytes: eb,
                                            little_endian: false,
                                            dtype_label: label.to_string(),
                                            is_curve: true,
                                            x_axis: Some(x_ax),
                                            y_axis: None,
                                            smoothness: smooth,
                                            p23: None,
                                            quality_score: (pts as f64) / (smooth + 0.05) + 10.0,
                                            status: DiscoveryStatus::StructurallySupported,
                                            detection_source: format!("denso_sh_curve_descriptor({label})"),
                                        });
                                        matched_f32_ax = true;
                                    }
                                }
                            }
                        }
                    }

                    if !matched_f32_ax && px != 0 && pz != 0 && px != pz && px.is_multiple_of(2) && px < len {
                        if let Some(x_ax) = read_monotonic_u16_axis(rom, px, pts, false) {
                            let z_tries: &[(usize, &str)] = match flags {
                                0x0400 if include_8bit => &[(1, "u8")],
                                0x0400 => &[],
                                0x0800 => &[(2, "u16_be")],
                                _ if include_8bit => &[(2, "u16_be"), (1, "u8")],
                                _ => &[(2, "u16_be")],
                            };
                            for &(eb, label) in z_tries {
                                let span = pts * eb;
                                if (eb == 1 || pz.is_multiple_of(eb))
                                    && pz.checked_add(span).is_some_and(|e| e <= occupied.len() && e <= len)
                                    && !occupied[pz..pz + span].iter().any(|&b| b)
                                {
                                    let mut vals = Vec::with_capacity(pts);
                                    for k in 0..pts {
                                        let o = pz + k * eb;
                                        vals.push(if eb == 2 {
                                            read_u16(rom, o, false).unwrap_or(0) as f64
                                        } else {
                                            rom[o] as f64
                                        });
                                    }
                                    let turns = count_turning_points(&vals);
                                    if passes_anchored_curve_gates(&vals, eb, false, turns) {
                                        let n = pts as f64;
                                        let mean = vals.iter().sum::<f64>() / n;
                                        let sd = (vals.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / n).sqrt();
                                        let smooth = if sd > 1e-9 {
                                            let ds: f64 = vals.windows(2).map(|w| (w[1] - w[0]).abs()).sum();
                                            (ds / (pts - 1) as f64) / sd
                                        } else {
                                            0.0
                                        };
                                        local.push(BlindV2Candidate {
                                            address: pz,
                                            rows: 1,
                                            cols: pts,
                                            elem_bytes: eb,
                                            little_endian: false,
                                            dtype_label: label.to_string(),
                                            is_curve: true,
                                            x_axis: Some(x_ax),
                                            y_axis: None,
                                            smoothness: smooth,
                                            p23: None,
                                            quality_score: (pts as f64) / (smooth + 0.05) + 10.0,
                                            status: DiscoveryStatus::StructurallySupported,
                                            detection_source: format!("denso_sh_curve_descriptor({label})"),
                                        });
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
                off += 4;
            }
            local
        })
        .collect();

    raw_hits.sort_by(|a, b| {
        b.quality_score
            .total_cmp(&a.quality_score)
            .then(a.address.cmp(&b.address))
            .then(a.elem_bytes.cmp(&b.elem_bytes))
    });

    let mut accepted = Vec::new();
    for cand in raw_hits {
        let s = cand.address;
        let e = s + cand.cols * cand.elem_bytes;
        if !occupied[s..e].iter().any(|&b| b) {
            occupied[s..e].fill(true);
            accepted.push(cand);
        }
    }
    accepted
}

/// Scans `f32_le` and `f32_be` 1-D calibration curves within kept ranges without hitting `occupied`.
pub(super) fn scan_f32_curves_in_ranges(
    rom: &[u8],
    kept_ranges: &[(usize, usize)],
    occupied: &mut [bool],
) -> Vec<BlindV2Candidate> {
    const MIN_PTS: usize = 8;
    const MAX_PTS: usize = 36;
    const TILE_BYTES: usize = 256 * 1024;
    const HALO_BYTES: usize = MAX_PTS * 4;

    let mut jobs: Vec<(bool, usize, usize, usize)> = Vec::new();
    for le in [true, false] {
        for &(r_start, r_end) in kept_ranges {
            if r_end.saturating_sub(r_start) <= TILE_BYTES + HALO_BYTES {
                jobs.push((le, r_start, r_end, r_end));
            } else {
                let mut cur = r_start;
                while cur < r_end {
                    let main_end = (cur + TILE_BYTES).min(r_end);
                    let scan_end = (main_end + HALO_BYTES).min(r_end);
                    jobs.push((le, cur, main_end, scan_end));
                    cur = main_end;
                }
            }
        }
    }

    let mut raw_hits: Vec<BlindV2Candidate> = jobs
        .par_iter()
        .flat_map_iter(|&(le, r_start, main_end, r_end)| {
            let mut local = Vec::new();
            let start = (r_start + 3) & !3;
            let cells = r_end.saturating_sub(start) / 4;
            let main_cells = main_end.saturating_sub(start) / 4;
            if cells < MIN_PTS || main_cells == 0 {
                return local;
            }
            let idx = F32PrefixIndex::build_from_rom(rom, start, r_end, le);
            if idx.max_valid_run() < MIN_PTS {
                return local;
            }
            let label = if le { "f32_le" } else { "f32_be" };
            let last_start = cells.saturating_sub(MIN_PTS);
            let max_start = if main_end < r_end {
                last_start.min(main_cells.saturating_sub(1))
            } else {
                last_start
            };

            let mut c = 0usize;
            while c <= max_start {
                if let Some(jump) = idx.next_valid_start(c, MIN_PTS) {
                    c = jump;
                    continue;
                }
                let byte_min_start = start + c * 4;
                if occupied[byte_min_start..byte_min_start + MIN_PTS * 4].iter().any(|&b| b) {
                    c += 1;
                    continue;
                }

                // Extend maximal contiguous smooth run first (zero allocations), then run Bartels/VN gates once.
                let max_end = cells.min(c + MAX_PTS);
                let mut best_smooth_end = 0usize;
                let mut best_smooth = f64::INFINITY;
                let mut prev_v = idx.vals[c] as f64;
                let mut delta_sum = 0.0f64;

                let mut end = c + 1;
                while end <= max_end {
                    let cur = idx.vals[end - 1];
                    if !F32PrefixIndex::is_valid_cal_float(cur)
                        || occupied[start + (end - 1) * 4..start + end * 4].iter().any(|&b| b)
                    {
                        break;
                    }
                    let cf = cur as f64;
                    if end > c + 1 {
                        delta_sum += (cf - prev_v).abs();
                        prev_v = cf;
                    }
                    let pts = end - c;
                    if pts >= MIN_PTS {
                        let (_mean, sd) = idx.approx_mean_sd(c, pts);
                        if sd >= 0.01 {
                            let smooth = (delta_sum / (pts - 1) as f64) / sd;
                            if smooth.is_finite() && smooth < 0.35 {
                                best_smooth_end = end;
                                best_smooth = smooth;
                            }
                        }
                    }
                    end += 1;
                }

                let mut best_pts = 0usize;
                if best_smooth_end >= c + MIN_PTS {
                    let vals: Vec<f64> = idx.vals[c..best_smooth_end]
                        .iter()
                        .map(|&v| v as f64)
                        .collect();
                    let turns = count_turning_points(&vals);
                    if passes_statistical_curve_gates(&vals, 4, turns) {
                        best_pts = best_smooth_end - c;
                    }
                }

                if best_pts >= MIN_PTS {
                    let c_start = start + c * 4;
                    local.push(BlindV2Candidate {
                        address: c_start,
                        rows: 1,
                        cols: best_pts,
                        elem_bytes: 4,
                        little_endian: le,
                        dtype_label: label.to_string(),
                        is_curve: true,
                        x_axis: None,
                        y_axis: None,
                        smoothness: best_smooth,
                        p23: None,
                        quality_score: (best_pts as f64) / (best_smooth + 0.05),
                        status: DiscoveryStatus::Discovered,
                        detection_source: format!("curve_scan_f32_v2({label})"),
                    });
                    c += best_pts;
                } else {
                    c += 1;
                }
            }
            local
        })
        .collect();

    raw_hits.sort_by(|a, b| {
        (!a.little_endian)
            .cmp(&(!b.little_endian))
            .then(a.address.cmp(&b.address))
            .then(b.cols.cmp(&a.cols))
    });

    let mut out = Vec::new();
    for cand in raw_hits {
        let s = cand.address;
        let e = s + cand.cols * 4;
        if e <= occupied.len() && !occupied[s..e].iter().any(|&b| b) {
            occupied[s..e].fill(true);
            out.push(cand);
        }
    }
    out
}

pub(super) fn count_turning_points(values: &[f64]) -> usize {
    let mut prev_sign = 0i8;
    let mut turns = 0usize;
    for w in values.windows(2) {
        let d = w[1] - w[0];
        let s = if d > 1e-9 {
            1
        } else if d < -1e-9 {
            -1
        } else {
            prev_sign
        };
        if prev_sign != 0 && s != 0 && s != prev_sign {
            turns += 1;
        }
        if s != 0 {
            prev_sign = s;
        }
    }
    turns
}
