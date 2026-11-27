use super::prefix::F32PrefixIndex;
use super::{AxisDirection, BlindV2Axis};
use crate::axis_anchored::AxisKind;
use rayon::prelude::*;

#[inline]
pub(super) fn read_u16(rom: &[u8], off: usize, le: bool) -> Option<u16> {
    let b: [u8; 2] = rom.get(off..off + 2)?.try_into().ok()?;
    Some(if le { u16::from_le_bytes(b) } else { u16::from_be_bytes(b) })
}

#[inline]
pub(super) fn read_i16(rom: &[u8], off: usize, le: bool) -> Option<i16> {
    let b: [u8; 2] = rom.get(off..off + 2)?.try_into().ok()?;
    Some(if le { i16::from_le_bytes(b) } else { i16::from_be_bytes(b) })
}

#[inline]
pub(super) fn read_u32(rom: &[u8], off: usize, le: bool) -> Option<u32> {
    let b: [u8; 4] = rom.get(off..off + 4)?.try_into().ok()?;
    Some(if le { u32::from_le_bytes(b) } else { u32::from_be_bytes(b) })
}

#[inline]
pub(super) fn read_f32(rom: &[u8], off: usize, le: bool) -> Option<f32> {
    let b: [u8; 4] = rom.get(off..off + 4)?.try_into().ok()?;
    Some(if le { f32::from_le_bytes(b) } else { f32::from_be_bytes(b) })
}

#[inline]
pub(super) fn read_axis_val(
    rom: &[u8],
    off: usize,
    elem_bytes: usize,
    le: bool,
    is_signed: bool,
) -> Option<f64> {
    match elem_bytes {
        1 => rom.get(off).map(|&b| b as f64),
        2 if is_signed => read_i16(rom, off, le).map(|v| v as f64),
        2 => read_u16(rom, off, le).map(|v| v as f64),
        4 => {
            let fv = read_f32(rom, off, le)?;
            if F32PrefixIndex::is_valid_cal_float(fv) && fv.abs() <= 100_000.0 {
                Some(fv as f64)
            } else {
                None
            }
        }
        _ => None,
    }
}

#[inline]
pub(super) fn axis_linearity(values: &[f64]) -> (f64, f64) {
    if values.len() < 2 {
        return (0.0, 0.0);
    }
    let mut sum = 0.0f64;
    let mut min_s = f64::MAX;
    let mut max_s = f64::MIN;
    let count = (values.len() - 1) as f64;
    for w in values.windows(2) {
        let s = (w[1] - w[0]).abs();
        sum += s;
        if s < min_s {
            min_s = s;
        }
        if s > max_s {
            max_s = s;
        }
    }
    let mean = sum / count;
    if mean <= 0.0 {
        return (0.0, 0.0);
    }
    let lin = (1.0 - (max_s - min_s) / (mean * 4.0)).max(0.0);
    (lin, mean)
}

/// Multi-width (`u8`, `u16`/`i16` LE/BE), bidirectional (increasing + decreasing) axis pool.
pub fn build_axis_pool_v2(rom: &[u8], include_8bit: bool) -> Vec<BlindV2Axis> {
    build_axis_pool_v2_in_ranges(rom, &[(0, rom.len())], include_8bit, false)
}

type AxisScanTask = ((usize, usize), (usize, bool, bool, AxisDirection));

pub(super) fn build_axis_pool_v2_in_ranges(
    rom: &[u8],
    kept_ranges: &[(usize, usize)],
    include_8bit: bool,
    include_f32: bool,
) -> Vec<BlindV2Axis> {
    const MIN_LEN: usize = 4;
    const MAX_LEN: usize = 48;

    let mut configs: Vec<(usize, bool, bool, AxisDirection)> = vec![
        (2, true, false, AxisDirection::Increasing),
        (2, true, false, AxisDirection::Decreasing),
        (2, false, false, AxisDirection::Increasing),
        (2, false, false, AxisDirection::Decreasing),
    ];
    if rom.len() >= 131_072 {
        let dom_le = matches!(
            crate::zero_xdf_scan::detect_global_endianness(rom),
            crate::zero_xdf_scan::Endianness::LittleEndian
        );
        configs.push((2, dom_le, true, AxisDirection::Increasing));
        configs.push((2, dom_le, true, AxisDirection::Decreasing));
    } else {
        configs.push((2, true, true, AxisDirection::Increasing));
        configs.push((2, true, true, AxisDirection::Decreasing));
        configs.push((2, false, true, AxisDirection::Increasing));
        configs.push((2, false, true, AxisDirection::Decreasing));
    }
    if include_8bit {
        configs.push((1, false, false, AxisDirection::Increasing));
        configs.push((1, false, false, AxisDirection::Decreasing));
    }
    if include_f32 {
        configs.push((4, true, false, AxisDirection::Increasing));
        configs.push((4, true, false, AxisDirection::Decreasing));
        configs.push((4, false, false, AxisDirection::Increasing));
        configs.push((4, false, false, AxisDirection::Decreasing));
    }

    let tasks: Vec<AxisScanTask> = kept_ranges
        .iter()
        .copied()
        .filter(|&(r_start, r_end)| r_start < r_end.min(rom.len()))
        .flat_map(|range| configs.iter().copied().map(move |cfg| (range, cfg)))
        .collect();

    let mut pool: Vec<BlindV2Axis> = tasks
        .par_iter()
        .flat_map_iter(|&((r_start, r_end), (elem_bytes, le, is_signed, dir))| {
            let range_end = r_end.min(rom.len());
            let mut local = Vec::new();
            let align_mask = elem_bytes - 1;
            let mut off = (r_start + align_mask) & !align_mask;
            let min_step = if elem_bytes == 4 { 1e-4 } else { 1.0 };
            let max_step = if elem_bytes == 1 {
                64.0
            } else if elem_bytes == 2 {
                16_384.0
            } else {
                5000.0
            };
            let mut buf = [0.0f64; MAX_LEN];

            while off + MIN_LEN * elem_bytes <= range_end {
                let Some(first) = read_axis_val(rom, off, elem_bytes, le, is_signed) else {
                    off += elem_bytes;
                    continue;
                };
                let Some(second) = read_axis_val(rom, off + elem_bytes, elem_bytes, le, is_signed) else {
                    off += elem_bytes;
                    continue;
                };
                let d0 = match dir {
                    AxisDirection::Increasing => second - first,
                    AxisDirection::Decreasing => first - second,
                };
                if d0 < min_step || d0 > max_step {
                    off += elem_bytes;
                    continue;
                }

                buf[0] = first;
                buf[1] = second;
                let mut len = 2usize;
                let mut prev = second;
                let mut pos = off + elem_bytes * 2;

                while len < MAX_LEN && pos + elem_bytes <= range_end {
                    let Some(cur) = read_axis_val(rom, pos, elem_bytes, le, is_signed) else {
                        break;
                    };
                    let diff = match dir {
                        AxisDirection::Increasing => cur - prev,
                        AxisDirection::Decreasing => prev - cur,
                    };
                    if diff < min_step || diff > max_step {
                        break;
                    }
                    // Prevent merging an immediately adjacent [X | Y] axis pair into one run
                    // when step regime shifts abruptly (e.g. X step ~800 followed by Y step ~30).
                    if len >= MIN_LEN {
                        let prev_mean = (buf[len - 1] - buf[0]).abs() / (len - 1) as f64;
                        if prev_mean > 0.0 && (diff >= prev_mean * 2.5 || diff * 4.0 <= prev_mean) {
                            if let Some(next_val) = read_axis_val(rom, pos + elem_bytes, elem_bytes, le, is_signed) {
                                let next_diff = match dir {
                                    AxisDirection::Increasing => next_val - cur,
                                    AxisDirection::Decreasing => cur - next_val,
                                };
                                if (min_step..=max_step).contains(&next_diff)
                                    && (next_diff * 4.0 <= diff || next_diff >= diff * 4.0)
                                {
                                    break;
                                }
                            }
                        }
                    }
                    buf[len] = cur;
                    len += 1;
                    prev = cur;
                    pos += elem_bytes;
                }

                if len >= MIN_LEN {
                    let values = &buf[..len];
                    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
                    for &v in values {
                        if v < lo {
                            lo = v;
                        }
                        if v > hi {
                            hi = v;
                        }
                    }
                    // Signed i16 axes must strictly cross zero (positive & negative breakpoints).
                    if is_signed && !(lo < 0.0 && hi > 0.0) {
                        off += elem_bytes;
                        continue;
                    }
                    let span = hi - lo;
                    let (lin, mean_step) = axis_linearity(values);

                    let (valid_mag, kind) = if elem_bytes == 4 {
                        let is_rpm = (0.0..=3000.0).contains(&lo) && (2000.0..=20000.0).contains(&hi);
                        let is_load = lo >= -50.0 && hi <= 1000.0 && span >= 5.0;
                        let ok = lin >= 0.35 && (is_rpm || is_load);
                        (ok, if is_rpm { AxisKind::Rpm } else { AxisKind::Load })
                    } else if elem_bytes == 2 {
                        let is_rpm = (200.0..=3000.0).contains(&lo) && (4000.0..=20000.0).contains(&hi);
                        let is_load = lo <= 200.0 && (60.0..=4096.0).contains(&hi);
                        let rel_span = if lo <= 0.0 || span / lo.abs() >= 0.05 { 1.0 } else { 0.0 };
                        let is_general_16 = len >= 5
                            && span >= 20.0
                            && (is_signed || hi < 65535.0)
                            && rel_span > 0.0
                            && lin >= 0.50
                            && mean_step >= 2.0
                            && !(lo == 0.0 && mean_step <= 1.01);
                        (
                            ((is_rpm || is_load) && lin > 0.0 && rel_span > 0.0) || is_general_16,
                            if is_rpm { AxisKind::Rpm } else { AxisKind::Load },
                        )
                    } else {
                        // Guard against sequential 0,1,2,3.. byte index tables.
                        let not_counter = mean_step >= 2.0 || (lo >= 8.0 && span >= 12.0);
                        let ok = lo <= 160.0 && hi >= 32.0 && span >= 15.0 && lin >= 0.65 && not_counter;
                        let kind = if hi >= 120.0 && mean_step >= 4.0 {
                            AxisKind::Rpm
                        } else {
                            AxisKind::Load
                        };
                        (ok, kind)
                    };

                    if valid_mag {
                        local.push(BlindV2Axis {
                            offset: off,
                            length: len,
                            elem_bytes,
                            little_endian: le,
                            direction: dir,
                            kind,
                            values: values.to_vec(),
                        });
                        off += len * elem_bytes;
                        continue;
                    }
                }
                off += elem_bytes;
            }
            local
        })
        .collect();

    pool.sort_by_key(|a| (a.offset, a.elem_bytes, a.little_endian, a.length));
    pool
}

pub(super) fn candidate_y_lens_v2(values: &[f64], max_len: usize) -> Vec<usize> {
    let min_len = 4usize;
    let mut set = std::collections::BTreeSet::new();
    if max_len >= min_len {
        set.insert(max_len);
    }
    if values.len() >= 2 {
        let steps: Vec<f64> = values.windows(2).map(|w| (w[1] - w[0]).abs()).collect();
        for k in (min_len - 1).min(steps.len())..steps.len() {
            let window = if k >= 3 { &steps[k - 3..k] } else { &steps[..k] };
            if window.is_empty() {
                continue;
            }
            let avg = window.iter().sum::<f64>() / window.len() as f64;
            if avg > 0.0 && (steps[k] >= avg * 4.0 || steps[k] <= avg / 4.0) {
                let ln = k + 1;
                if ln >= min_len && ln <= max_len {
                    set.insert(ln);
                }
            }
        }
    }
    for &ln in &[4usize, 5, 6, 8, 10, 12, 16] {
        if ln >= min_len && ln <= max_len {
            set.insert(ln);
        }
    }
    let mut out: Vec<usize> = set.into_iter().collect();
    out.sort_by(|a, b| b.cmp(a));
    out
}
