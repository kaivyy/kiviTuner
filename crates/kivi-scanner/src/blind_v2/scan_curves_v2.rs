//! Unified 3-layer 1-D calibration curve discovery for Blind V2:
//! - Layer 1 (Tier 3): OEM length-prefixed (`[len:u16/u8][ax][Z]`) and Denso SuperH pointer 1-D descriptors
//! - Layer 2 (Tier 2): Axis-anchored `[ax][Z]` curves reusing the precomputed 2-D axis pool
//! - Layer 3 (Tier 1): Unoccupied statistical 1-D curve scan (`u16`/`u8`, signed `i16`, `f32`) with seam rescue

use rayon::prelude::*;

use crate::curve_scan::{
    scan_curves_unoccupied_in_ranges, CurveScanOptions, Endian as CurveEndian,
};

use super::axes::{axis_linearity, read_f32, read_i16, read_u16};
use super::gates::{passes_anchored_curve_gates, passes_statistical_curve_gates};
use super::prefix::{F32PrefixIndex, IntKind, IntPrefixIndex};
use super::scan_oem_structs::{
    count_turning_points, read_monotonic_i16_axis, read_monotonic_u16_axis,
    read_monotonic_u8_axis, scan_denso_rh850_le_curves_v2, scan_denso_sh_curves_v2,
    scan_f32_curves_in_ranges,
};
use super::{BlindV2Axis, BlindV2Candidate, DiscoveryStatus};

#[inline]
fn curve_smoothness_and_turns(values: &[f64]) -> Option<(f64, usize)> {
    let n = values.len();
    if n < 3 {
        return None;
    }
    let nf = n as f64;
    let mean = values.iter().sum::<f64>() / nf;
    let var = values
        .iter()
        .map(|&v| {
            let d = v - mean;
            d * d
        })
        .sum::<f64>()
        / nf;
    let sd = var.sqrt();
    if !sd.is_finite() || sd < 1e-9 {
        return None;
    }
    let delta_sum: f64 = values.windows(2).map(|w| (w[1] - w[0]).abs()).sum();
    let smooth = (delta_sum / (n - 1) as f64) / sd;
    if !smooth.is_finite() {
        return None;
    }
    Some((smooth, count_turning_points(values)))
}

#[inline]
fn fast_3_mono_u8_1d(rom: &[u8], off: usize) -> bool {
    let (a, b, c) = (rom[off], rom[off + 1], rom[off + 2]);
    (b > a && c > b && b - a <= 64 && c - b <= 64)
        || (a > b && b > c && a - b <= 64 && b - c <= 64)
}

#[inline]
fn fast_3_mono_16_1d(rom: &[u8], off: usize, le: bool) -> bool {
    let (Some(a), Some(b), Some(c)) = (
        read_u16(rom, off, le),
        read_u16(rom, off + 2, le),
        read_u16(rom, off + 4, le),
    ) else {
        return false;
    };
    let u_ok = (b > a && c > b && (b - a) <= 24_576 && (c - b) <= 24_576)
        || (a > b && b > c && (a - b) <= 24_576 && (b - c) <= 24_576);
    if u_ok {
        return true;
    }
    let (sa, sb, sc) = (a as i16 as i32, b as i16 as i32, c as i16 as i32);
    (sb > sa && sc > sb && (sb - sa) <= 24_576 && (sc - sb) <= 24_576)
        || (sa > sb && sb > sc && (sa - sb) <= 24_576 && (sb - sc) <= 24_576)
}

fn decode_z_1d_slice(
    rom: &[u8],
    offset: usize,
    pts: usize,
    elem_bytes: usize,
    le: bool,
    is_signed: bool,
    is_float: bool,
) -> Option<Vec<f64>> {
    let span = pts.checked_mul(elem_bytes)?;
    if offset.checked_add(span)? > rom.len() {
        return None;
    }
    let mut vals = Vec::with_capacity(pts);
    let mut has_neg = false;
    let mut has_pos = false;
    for k in 0..pts {
        let o = offset + k * elem_bytes;
        let v = if elem_bytes == 1 && is_signed {
            let sv = rom[o] as i8;
            if sv < 0 {
                has_neg = true;
            } else {
                has_pos = true;
            }
            sv as f64
        } else if elem_bytes == 1 {
            rom[o] as f64
        } else if elem_bytes == 2 && is_signed {
            let sv = read_i16(rom, o, le)?;
            if sv < 0 {
                has_neg = true;
            } else {
                has_pos = true;
            }
            sv as f64
        } else if elem_bytes == 2 {
            read_u16(rom, o, le)? as f64
        } else if elem_bytes == 4 && is_float {
            let fv = read_f32(rom, o, le)?;
            if !F32PrefixIndex::is_valid_cal_float(fv) {
                return None;
            }
            fv as f64
        } else {
            return None;
        };
        vals.push(v);
    }
    if is_signed && !(has_neg && has_pos) {
        return None;
    }
    Some(vals)
}

#[inline]
fn dtype_label_1d(elem_bytes: usize, le: bool, is_signed: bool, is_float: bool) -> &'static str {
    match (elem_bytes, le, is_signed, is_float) {
        (1, _, true, _) => "i8",
        (1, _, false, _) => "u8",
        (2, true, true, _) => "i16_le",
        (2, false, true, _) => "i16_be",
        (2, true, false, _) => "u16_le",
        (2, false, false, _) => "u16_be",
        (4, true, _, true) => "f32_le",
        _ => "f32_be",
    }
}

#[inline]
fn mark_range_occupied(occupied: &mut [bool], start: usize, end: usize) {
    let s = start.min(occupied.len());
    let e = end.min(occupied.len());
    if s < e {
        occupied[s..e].fill(true);
    }
}

#[inline]
fn header_byte_continues_axis(rom: &[u8], ax_off: usize, ax_eb: usize, le: bool, first: f64, second: f64) -> bool {
    let step = second - first;
    if step.abs() < 1e-9 {
        return false;
    }
    if ax_eb == 1 && ax_off >= 2 {
        let prev = rom[ax_off - 2] as f64;
        let d_prev = first - prev;
        return (d_prev > 0.0) == (step > 0.0)
            && d_prev.abs() <= step.abs() * 4.0
            && d_prev.abs() * 4.0 >= step.abs();
    }
    if ax_eb == 2 {
        for prev_off in [ax_off.checked_sub(2), ax_off.checked_sub(3)]
            .into_iter()
            .flatten()
        {
            if let Some(prev_u) = read_u16(rom, prev_off, le) {
                let prev = if first < 0.0 {
                    (prev_u as i16) as f64
                } else {
                    prev_u as f64
                };
                let d_prev = first - prev;
                if (d_prev > 0.0) == (step > 0.0)
                    && d_prev.abs() <= step.abs() * 4.0
                    && d_prev.abs() * 4.0 >= step.abs()
                {
                    return true;
                }
            }
        }
    }
    false
}

/// Layer 1: Scans Bosch/Siemens inline 1-D length-prefixed curves `[len:u16/u8][ax][Z]`.
fn scan_bosch_siemens_1d_structs(
    rom: &[u8],
    kept_ranges: &[(usize, usize)],
    include_8bit: bool,
    occupied: &mut [bool],
) -> Vec<BlindV2Candidate> {
    const TILE_BYTES: usize = 64 * 1024;
    let mut tiles: Vec<(usize, usize, usize)> = Vec::new();
    for &(r_start, r_end) in kept_ranges {
        let mut cur = r_start;
        while cur < r_end {
            let main_end = (cur + TILE_BYTES).min(r_end);
            let dec_end = (main_end + 512).min(r_end);
            tiles.push((cur, main_end, dec_end));
            cur = main_end;
        }
    }
    if tiles.is_empty() {
        return Vec::new();
    }

    let endians: &[bool] = if rom.len() >= 131_072 {
        match crate::zero_xdf_scan::detect_global_endianness(rom) {
            crate::zero_xdf_scan::Endianness::LittleEndian => &[true],
            crate::zero_xdf_scan::Endianness::BigEndian => &[false],
        }
    } else {
        &[true, false]
    };
    let z_tries_16: &[(usize, bool)] = if include_8bit {
        &[(2, false), (2, true), (1, false), (1, true)]
    } else {
        &[(2, false), (2, true)]
    };
    let z_tries_8: &[(usize, bool)] = &[(1, false), (1, true), (2, false), (2, true)];

    struct Struct1dHit {
        hdr_start: usize,
        z_end: usize,
        cand: BlindV2Candidate,
    }

    let mut raw_hits: Vec<Struct1dHit> = tiles
        .par_iter()
        .flat_map_iter(|&(t_start, main_end, dec_end)| {
            let mut local = Vec::new();
            // Pattern A: [len:u16][ax:len*2][Z]
            let mut off = (t_start + 1) & !1;
            while off + 16 <= main_end && off + 16 <= dec_end {
                let mut matched = false;
                for &le in endians {
                    let Some(pts) = read_u16(rom, off, le).map(|v| v as usize) else {
                        break;
                    };
                    if !(4..=48).contains(&pts) {
                        continue;
                    }
                    let ax_off = off + 2;
                    let z_off = ax_off + pts * 2;
                    if z_off + pts > dec_end || !fast_3_mono_16_1d(rom, ax_off, le) {
                        continue;
                    }
                    if occupied[off..z_off].iter().any(|&b| b) {
                        continue;
                    }
                    let Some(ax) = read_monotonic_u16_axis(rom, ax_off, pts, le)
                        .or_else(|| read_monotonic_i16_axis(rom, ax_off, pts, le))
                    else {
                        continue;
                    };
                    if axis_linearity(&ax.values).0 < 0.40 {
                        continue;
                    }
                    for &(eb, is_signed) in z_tries_16 {
                        let span = pts * eb;
                        if z_off + span > dec_end || occupied[z_off..z_off + span].iter().any(|&b| b) {
                            continue;
                        }
                        let Some(vals) = decode_z_1d_slice(rom, z_off, pts, eb, le, is_signed, false) else {
                            continue;
                        };
                        let Some((smooth, turns)) = curve_smoothness_and_turns(&vals) else {
                            continue;
                        };
                        if !passes_anchored_curve_gates(&vals, eb, is_signed, turns) {
                            continue;
                        }
                        let label = dtype_label_1d(eb, le && eb > 1, is_signed, false);
                        local.push(Struct1dHit {
                            hdr_start: off,
                            z_end: z_off + span,
                            cand: BlindV2Candidate {
                                address: z_off,
                                rows: 1,
                                cols: pts,
                                elem_bytes: eb,
                                little_endian: le && eb > 1,
                                dtype_label: label.to_string(),
                                is_curve: true,
                                x_axis: Some(ax),
                                y_axis: None,
                                smoothness: smooth,
                                p23: None,
                                quality_score: (pts as f64).sqrt() / (smooth + 0.05) + 12.0,
                                status: DiscoveryStatus::StructurallySupported,
                                detection_source: format!("bosch_siemens_1d_struct({label})"),
                            },
                        });
                        matched = true;
                        break;
                    }
                    if matched {
                        break;
                    }
                }
                off += if matched { 6 } else { 2 };
            }

            // Pattern B: [len:u8][ax][Z]
            let dom_le = endians[0];
            let mut off8 = t_start;
            while off8 + 14 <= main_end && off8 + 14 <= dec_end {
                if occupied[off8] {
                    off8 += 1;
                    continue;
                }
                let pts = rom[off8] as usize;
                let mut matched = false;
                if (6..=48).contains(&pts) {
                    let ax_off = off8 + 1;
                    // B2: 16-bit axis
                    let z16_off = ax_off + pts * 2;
                    if z16_off + pts <= dec_end
                        && fast_3_mono_16_1d(rom, ax_off, dom_le)
                        && !occupied[off8..z16_off].iter().any(|&b| b)
                    {
                        if let Some(ax) = read_monotonic_u16_axis(rom, ax_off, pts, dom_le)
                            .or_else(|| read_monotonic_i16_axis(rom, ax_off, pts, dom_le))
                        {
                            if axis_linearity(&ax.values).0 >= 0.50
                                && !header_byte_continues_axis(rom, ax_off, 2, dom_le, ax.values[0], ax.values[1])
                            {
                                for &(eb, is_signed) in z_tries_16 {
                                    let span = pts * eb;
                                    if z16_off + span > dec_end
                                        || occupied[z16_off..z16_off + span].iter().any(|&b| b)
                                    {
                                        continue;
                                    }
                                    let Some(vals) =
                                        decode_z_1d_slice(rom, z16_off, pts, eb, dom_le, is_signed, false)
                                    else {
                                        continue;
                                    };
                                    let Some((smooth, turns)) = curve_smoothness_and_turns(&vals) else {
                                        continue;
                                    };
                                    if (pts >= 8 && smooth >= 0.38)
                                        || !passes_anchored_curve_gates(&vals, eb, is_signed, turns)
                                    {
                                        continue;
                                    }
                                    let label = dtype_label_1d(eb, dom_le && eb > 1, is_signed, false);
                                    local.push(Struct1dHit {
                                        hdr_start: off8,
                                        z_end: z16_off + span,
                                        cand: BlindV2Candidate {
                                            address: z16_off,
                                            rows: 1,
                                            cols: pts,
                                            elem_bytes: eb,
                                            little_endian: dom_le && eb > 1,
                                            dtype_label: label.to_string(),
                                            is_curve: true,
                                            x_axis: Some(ax),
                                            y_axis: None,
                                            smoothness: smooth,
                                            p23: None,
                                            quality_score: (pts as f64).sqrt() / (smooth + 0.05) + 11.0,
                                            status: DiscoveryStatus::StructurallySupported,
                                            detection_source: format!(
                                                "bosch_siemens_1d_struct(u8hdr_{label})"
                                            ),
                                        },
                                    });
                                    matched = true;
                                    break;
                                }
                            }
                        }
                    }

                    // B1: 8-bit axis
                    let z8_off = ax_off + pts;
                    if !matched
                        && include_8bit
                        && z8_off + pts <= dec_end
                        && fast_3_mono_u8_1d(rom, ax_off)
                        && !occupied[off8..z8_off].iter().any(|&b| b)
                    {
                        if let Some(ax) = read_monotonic_u8_axis(rom, ax_off, pts) {
                            let lo = ax.values.iter().copied().fold(f64::INFINITY, f64::min);
                            let hi = ax.values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                            if axis_linearity(&ax.values).0 >= 0.65
                                && (hi - lo) >= 16.0
                                && !header_byte_continues_axis(rom, ax_off, 1, false, ax.values[0], ax.values[1])
                            {
                                for &(eb, is_signed) in z_tries_8 {
                                    let span = pts * eb;
                                    if z8_off + span > dec_end
                                        || occupied[z8_off..z8_off + span].iter().any(|&b| b)
                                    {
                                        continue;
                                    }
                                    let Some(vals) =
                                        decode_z_1d_slice(rom, z8_off, pts, eb, dom_le, is_signed, false)
                                    else {
                                        continue;
                                    };
                                    let Some((smooth, turns)) = curve_smoothness_and_turns(&vals) else {
                                        continue;
                                    };
                                    if (pts >= 8 && smooth >= 0.38)
                                        || !passes_anchored_curve_gates(&vals, eb, is_signed, turns)
                                    {
                                        continue;
                                    }
                                    let label = dtype_label_1d(eb, dom_le && eb > 1, is_signed, false);
                                    local.push(Struct1dHit {
                                        hdr_start: off8,
                                        z_end: z8_off + span,
                                        cand: BlindV2Candidate {
                                            address: z8_off,
                                            rows: 1,
                                            cols: pts,
                                            elem_bytes: eb,
                                            little_endian: dom_le && eb > 1,
                                            dtype_label: label.to_string(),
                                            is_curve: true,
                                            x_axis: Some(ax),
                                            y_axis: None,
                                            smoothness: smooth,
                                            p23: None,
                                            quality_score: (pts as f64).sqrt() / (smooth + 0.05) + 11.0,
                                            status: DiscoveryStatus::StructurallySupported,
                                            detection_source: format!(
                                                "bosch_siemens_1d_struct(u8hdr_{label})"
                                            ),
                                        },
                                    });
                                    matched = true;
                                    break;
                                }
                            }
                        }
                    }
                }
                off8 += if matched { 6 } else { 1 };
            }
            local
        })
        .collect();

    raw_hits.sort_by(|a, b| {
        b.cand
            .quality_score
            .total_cmp(&a.cand.quality_score)
            .then(a.cand.address.cmp(&b.cand.address))
    });

    let mut out = Vec::new();
    for hit in raw_hits {
        if hit.z_end > occupied.len() || occupied[hit.hdr_start..hit.z_end].iter().any(|&b| b) {
            continue;
        }
        mark_range_occupied(occupied, hit.hdr_start, hit.z_end);
        out.push(hit.cand);
    }
    out
}

/// Layer 2: Axis-anchored 1-D curves `[ax][Z]` reusing the precomputed 2-D axis pool.
fn scan_axis_anchored_1d_v2(
    rom: &[u8],
    axis_pool: &[BlindV2Axis],
    include_8bit: bool,
    occupied_axes: &[bool],
    occupied: &mut [bool],
) -> Vec<BlindV2Candidate> {
    struct Anchored1dHit {
        ax_start: usize,
        z_end: usize,
        cand: BlindV2Candidate,
    }

    let mut raw_hits: Vec<Anchored1dHit> = axis_pool
        .par_iter()
        .filter_map(|ax| {
            let ax_end = ax.offset + ax.length * ax.elem_bytes;
            if ax_end > occupied.len() {
                return None;
            }
            if ax.elem_bytes >= 2 && ax.length < 6 {
                return None;
            }
            if ax.elem_bytes == 1 && ax.length < 8 {
                return None;
            }
            if occupied[ax.offset..ax_end].iter().any(|&b| b)
                || occupied_axes[ax.offset..ax_end].iter().any(|&b| b)
            {
                return None;
            }
            let min_lin = if ax.elem_bytes == 1 { 0.70 } else { 0.45 };
            if axis_linearity(&ax.values).0 < min_lin {
                return None;
            }

            let pads: &[usize] = if ax.elem_bytes == 1 { &[0] } else { &[0, 2] };
            let cfgs: Vec<(usize, bool, bool, bool)> = if ax.elem_bytes == 4 {
                vec![
                    (4, ax.little_endian, false, true),
                    (2, ax.little_endian, false, false),
                    (2, ax.little_endian, true, false),
                ]
            } else if ax.elem_bytes == 2 {
                let mut v = vec![
                    (2, ax.little_endian, false, false),
                    (2, ax.little_endian, true, false),
                ];
                if include_8bit {
                    v.push((1, false, false, false));
                    v.push((1, false, true, false));
                }
                v
            } else if include_8bit {
                vec![(1, false, false, false), (1, false, true, false)]
            } else {
                Vec::new()
            };

            let mut best: Option<(f64, Anchored1dHit)> = None;
            for &pad in pads {
                let data_start = ax_end + pad;
                for &(eb, le, is_signed, is_float) in &cfgs {
                    if is_float && !data_start.is_multiple_of(4) {
                        continue;
                    }
                    let z_end = data_start + ax.length * eb;
                    if z_end > rom.len() || occupied[data_start..z_end].iter().any(|&b| b) {
                        continue;
                    }
                    let Some(vals) = decode_z_1d_slice(rom, data_start, ax.length, eb, le, is_signed, is_float) else {
                        continue;
                    };
                    let Some((smooth, turns)) = curve_smoothness_and_turns(&vals) else {
                        continue;
                    };
                    if (ax.length >= 8 && smooth >= 0.42)
                        || !passes_anchored_curve_gates(&vals, eb, is_signed, turns)
                    {
                        continue;
                    }
                    let label = dtype_label_1d(eb, le && eb > 1, is_signed, is_float);
                    let score = (ax.length as f64).sqrt() / (smooth + 0.05) + 4.0;
                    let obj = smooth;
                    if best.as_ref().is_none_or(|(bm, _)| obj < *bm) {
                        best = Some((
                            obj,
                            Anchored1dHit {
                                ax_start: ax.offset,
                                z_end,
                                cand: BlindV2Candidate {
                                    address: data_start,
                                    rows: 1,
                                    cols: ax.length,
                                    elem_bytes: eb,
                                    little_endian: le && eb > 1,
                                    dtype_label: label.to_string(),
                                    is_curve: true,
                                    x_axis: Some(ax.clone()),
                                    y_axis: None,
                                    smoothness: smooth,
                                    p23: None,
                                    quality_score: score,
                                    status: DiscoveryStatus::StructurallySupported,
                                    detection_source: format!("axis_anchored_1d({label})"),
                                },
                            },
                        ));
                    }
                }
            }
            best.map(|(_, h)| h)
        })
        .collect();

    raw_hits.sort_by(|a, b| {
        b.cand
            .quality_score
            .total_cmp(&a.cand.quality_score)
            .then(a.cand.address.cmp(&b.cand.address))
    });

    let mut out = Vec::new();
    for hit in raw_hits {
        if hit.z_end > occupied.len() || occupied[hit.ax_start..hit.z_end].iter().any(|&b| b) {
            continue;
        }
        mark_range_occupied(occupied, hit.ax_start, hit.z_end);
        out.push(hit.cand);
    }
    out
}

fn find_u8_seam_len(values: &[f64]) -> usize {
    if values.len() < 4 {
        return values.len();
    }
    let mut diffs: Vec<f64> = values.windows(2).map(|w| (w[1] - w[0]).abs()).collect();
    diffs.sort_by(|a, b| a.total_cmp(b));
    let med = diffs[diffs.len() / 2].max(1.0);
    let mut dir = 0i8;
    for j in 0..values.len() - 1 {
        let step = values[j + 1] - values[j];
        let sign = if step > 1e-9 { 1 } else if step < -1e-9 { -1 } else { 0 };
        if j >= 4 && ((dir != 0 && sign != 0 && sign != dir) || step.abs() > med * 2.5) {
            return j + 1;
        }
        if sign != 0 {
            dir = sign;
        }
    }
    values.len()
}

fn rescue_seam_split_curves(
    c: &crate::curve_scan::DiscoveredCurve,
    out: &mut Vec<BlindV2Candidate>,
) {
    let mut pos = 0usize;
    let mut pieces = 0usize;
    let label = match (c.cell_bytes, c.endian) {
        (1, _) => "u8",
        (2, CurveEndian::Little) => "u16_le",
        _ => "u16_be",
    };
    while pieces < 3 && pos + 8 <= c.values.len() {
        let raw_seg_len = if c.cell_bytes == 2 {
            let words: Vec<u16> = c.values[pos..]
                .iter()
                .map(|&v| v.clamp(0.0, 65535.0) as u16)
                .collect();
            crate::boundary_snap::seam_len(&words)
        } else {
            find_u8_seam_len(&c.values[pos..])
        };
        let seg_len = raw_seg_len.min(c.values.len() - pos);
        if seg_len >= 8 {
            let sub = &c.values[pos..pos + seg_len];
            if let Some((smooth, turns)) = curve_smoothness_and_turns(sub) {
                if smooth < 0.48 && passes_statistical_curve_gates(sub, c.cell_bytes, turns) {
                    out.push(BlindV2Candidate {
                        address: c.address + pos * c.cell_bytes,
                        rows: 1,
                        cols: seg_len,
                        elem_bytes: c.cell_bytes,
                        little_endian: matches!(c.endian, CurveEndian::Little),
                        dtype_label: label.to_string(),
                        is_curve: true,
                        x_axis: None,
                        y_axis: None,
                        smoothness: smooth,
                        p23: None,
                        quality_score: (seg_len as f64).sqrt() / (smooth + 0.05),
                        status: DiscoveryStatus::Discovered,
                        detection_source: "curve_scan_v2_seam".to_string(),
                    });
                    pieces += 1;
                }
            }
        }
        pos += seg_len.max(1);
    }
}

/// Fast signed `i16` 1-D zero-crossing curve scanner over unoccupied calibration bytes.
fn scan_signed_i16_curves_v2(
    rom: &[u8],
    kept_ranges: &[(usize, usize)],
    occupied: &[bool],
) -> Vec<BlindV2Candidate> {
    const TILE_BYTES: usize = 64 * 1024;
    const MIN_PTS: usize = 8;
    const MAX_PTS: usize = 40;
    const HALO_BYTES: usize = MAX_PTS * 2;

    let kinds: &[IntKind] = if rom.len() >= 65_536 {
        match crate::zero_xdf_scan::detect_global_endianness(rom) {
            crate::zero_xdf_scan::Endianness::LittleEndian => &[IntKind::I16Le],
            crate::zero_xdf_scan::Endianness::BigEndian => &[IntKind::I16Be],
        }
    } else {
        &[IntKind::I16Le, IntKind::I16Be]
    };

    let mut jobs = Vec::new();
    for &kind in kinds {
        for &(r_start, r_end) in kept_ranges {
            let mut cur = r_start;
            while cur < r_end {
                let main_end = (cur + TILE_BYTES).min(r_end);
                let dec_end = (main_end + HALO_BYTES).min(r_end);
                jobs.push((kind, cur, main_end, dec_end));
                cur = main_end;
            }
        }
    }

    jobs.par_iter()
        .flat_map_iter(|&(kind, start, main_end, dec_end)| {
            let mut local = Vec::new();
            let align_start = (start + 1) & !1;
            let total_cells = dec_end.saturating_sub(align_start) / 2;
            let main_cells = main_end.saturating_sub(align_start) / 2;
            if total_cells < MIN_PTS || main_cells == 0 {
                return local;
            }
            let idx = IntPrefixIndex::build_from_rom(rom, align_start, dec_end, kind, 4500);
            if idx.has_no_sign_mix() {
                return local;
            }
            let occ_ok = dec_end <= occupied.len();
            let mut i = 0usize;
            while i < main_cells && i + MIN_PTS <= total_cells {
                if let Some(next_i) = idx.bad_jump_next(i, 1, MIN_PTS) {
                    i = next_i;
                    continue;
                }
                if occ_ok {
                    let bs = align_start + i * 2;
                    let be = bs + MIN_PTS * 2;
                    if let Some(rel) = occupied[bs..be].iter().rposition(|&b| b) {
                        i += (rel / 2) + 1;
                        continue;
                    }
                }
                if i > 0 && idx.vals[i - 1] == idx.vals[i] {
                    i += 1;
                    continue;
                }

                let hi_limit = total_cells.min(i + MAX_PTS);
                let mut best: Option<(usize, f64)> = None;
                let mut has_neg = false;
                let mut has_pos = false;
                let mut sum = 0.0f64;
                let mut sum_sq = 0.0f64;
                let mut delta_sum = 0.0f64;

                for end in (i + 1)..=hi_limit {
                    let k = end - 1;
                    let byte_off = align_start + k * 2;
                    if occ_ok && (occupied[byte_off] || occupied[byte_off + 1]) {
                        break;
                    }
                    let v = idx.vals[k];
                    if v < 0 {
                        has_neg = true;
                    } else if v >= 0 {
                        has_pos = true;
                    }
                    let vf = v as f64;
                    sum += vf;
                    sum_sq += vf * vf;
                    if k > i {
                        let d = v.abs_diff(idx.vals[k - 1]);
                        if d > 4500 {
                            break;
                        }
                        delta_sum += d as f64;
                    }
                    let pts = end - i;
                    if pts >= MIN_PTS && has_neg && has_pos {
                        let after_ok = end >= total_cells || idx.vals[end] != idx.vals[end - 1];
                        if after_ok {
                            let nf = pts as f64;
                            let mean = sum / nf;
                            let var = ((sum_sq / nf) - mean * mean).max(0.0);
                            let sd = var.sqrt();
                            if sd >= 5.0 {
                                let smooth = (delta_sum / (pts - 1) as f64) / sd;
                                if smooth.is_finite() && smooth < 0.48 {
                                    best = Some((pts, smooth));
                                }
                            }
                        }
                    }
                }

                if let Some((pts, smooth)) = best {
                    let vals: Vec<f64> = idx.vals[i..i + pts].iter().map(|&v| v as f64).collect();
                    let turns = count_turning_points(&vals);
                    if passes_statistical_curve_gates(&vals, 2, turns) {
                        local.push(BlindV2Candidate {
                            address: align_start + i * 2,
                            rows: 1,
                            cols: pts,
                            elem_bytes: 2,
                            little_endian: kind.little_endian(),
                            dtype_label: kind.label().to_string(),
                            is_curve: true,
                            x_axis: None,
                            y_axis: None,
                            smoothness: smooth,
                            p23: None,
                            quality_score: (pts as f64).sqrt() / (smooth + 0.05),
                            status: DiscoveryStatus::Discovered,
                            detection_source: format!("curve_scan_i16_v2({})", kind.label()),
                        });
                        i += pts;
                        continue;
                    }
                }
                i += 1;
            }
            local
        })
        .collect()
}

pub(super) fn attach_curves_v2_in_ranges(
    rom: &[u8],
    kept_ranges: &[(usize, usize)],
    accepted_2d: &[BlindV2Candidate],
    axis_pool: &[BlindV2Axis],
    include_8bit: bool,
    include_f32: bool,
) -> Vec<BlindV2Candidate> {
    let mut occupied = vec![false; rom.len()];
    let mut occupied_axes = vec![false; rom.len()];
    for t in accepted_2d {
        mark_range_occupied(&mut occupied, t.address, t.address.saturating_add(t.rows * t.cols * t.elem_bytes));
        if let Some(ax) = &t.x_axis {
            mark_range_occupied(&mut occupied_axes, ax.offset, ax.offset.saturating_add(ax.length * ax.elem_bytes));
        }
        if let Some(ax) = &t.y_axis {
            mark_range_occupied(&mut occupied_axes, ax.offset, ax.offset.saturating_add(ax.length * ax.elem_bytes));
        }
    }

    let mut out = Vec::new();

    // Layer 1 (Tier 3): OEM structural 1-D descriptors
    if include_f32 && (65_536..=8_500_000).contains(&rom.len()) {
        out.extend(scan_denso_sh_curves_v2(rom, include_8bit, &mut occupied));
    }
    out.extend(scan_denso_rh850_le_curves_v2(
        rom,
        include_8bit,
        &mut occupied,
    ));
    out.extend(scan_bosch_siemens_1d_structs(
        rom,
        kept_ranges,
        include_8bit,
        &mut occupied,
    ));

    // Layer 2 (Tier 2): Axis-anchored [ax][Z] 1-D curves
    out.extend(scan_axis_anchored_1d_v2(
        rom,
        axis_pool,
        include_8bit,
        &occupied_axes,
        &mut occupied,
    ));

    // Layer 3 (Tier 1): Standalone blind 1-D curves (never overlapping 2-D tables or 2-D axes)
    for (o, &ax_b) in occupied.iter_mut().zip(occupied_axes.iter()) {
        *o |= ax_b;
    }

    let raw_curves = if rom.len() >= 65_536 {
        let endian = match crate::zero_xdf_scan::detect_global_endianness(rom) {
            crate::zero_xdf_scan::Endianness::LittleEndian => CurveEndian::Little,
            crate::zero_xdf_scan::Endianness::BigEndian => CurveEndian::Big,
        };
        scan_curves_unoccupied_in_ranges(
            rom,
            kept_ranges,
            Some(&occupied),
            &CurveScanOptions {
                min_points: 8,
                max_points: 40,
                endian,
                include_8bit,
                allow_monotonic: true,
                snap_boundaries: true,
            },
        )
    } else {
        let (mut big, little) = rayon::join(
            || {
                scan_curves_unoccupied_in_ranges(
                    rom,
                    kept_ranges,
                    Some(&occupied),
                    &CurveScanOptions {
                        min_points: 8,
                        max_points: 40,
                        endian: CurveEndian::Big,
                        include_8bit,
                        allow_monotonic: true,
                        snap_boundaries: true,
                    },
                )
            },
            || {
                scan_curves_unoccupied_in_ranges(
                    rom,
                    kept_ranges,
                    Some(&occupied),
                    &CurveScanOptions {
                        min_points: 8,
                        max_points: 40,
                        endian: CurveEndian::Little,
                        include_8bit: false,
                        allow_monotonic: true,
                        snap_boundaries: true,
                    },
                )
            },
        );
        big.extend(little);
        big
    };

    let mut layer3_raw = Vec::new();
    for c in &raw_curves {
        let c_start = c.address;
        let c_end = c_start.saturating_add(c.points * c.cell_bytes);
        if c_start >= c_end || c_end > occupied.len() || occupied[c_start..c_end].iter().any(|&b| b) {
            continue;
        }
        if passes_statistical_curve_gates(&c.values, c.cell_bytes, c.turning_points) {
            layer3_raw.push(BlindV2Candidate {
                address: c.address,
                rows: 1,
                cols: c.points,
                elem_bytes: c.cell_bytes,
                little_endian: matches!(c.endian, CurveEndian::Little),
                dtype_label: match (c.cell_bytes, c.endian) {
                    (1, _) => "u8".to_string(),
                    (2, CurveEndian::Little) => "u16_le".to_string(),
                    _ => "u16_be".to_string(),
                },
                is_curve: true,
                x_axis: None,
                y_axis: None,
                smoothness: c.smoothness,
                p23: None,
                quality_score: (c.points as f64).sqrt() / (c.smoothness + 0.05),
                status: DiscoveryStatus::Discovered,
                detection_source: "curve_scan_v2".to_string(),
            });
        } else if c.points >= 14 {
            rescue_seam_split_curves(c, &mut layer3_raw);
        }
    }

    layer3_raw.extend(scan_signed_i16_curves_v2(rom, kept_ranges, &occupied));

    layer3_raw.sort_by(|a, b| {
        let sa = (a.cols as f64).sqrt() / (a.smoothness + 0.05);
        let sb = (b.cols as f64).sqrt() / (b.smoothness + 0.05);
        sb.total_cmp(&sa)
            .then_with(|| a.address.cmp(&b.address))
            .then_with(|| b.cols.cmp(&a.cols))
    });

    for cand in layer3_raw {
        let s = cand.address;
        let e = s.saturating_add(cand.cols * cand.elem_bytes);
        if s < e && e <= occupied.len() && !occupied[s..e].iter().any(|&b| b) {
            mark_range_occupied(&mut occupied, s, e);
            out.push(cand);
        }
    }

    if include_f32 {
        out.extend(scan_f32_curves_in_ranges(rom, kept_ranges, &mut occupied));
    }
    out
}
