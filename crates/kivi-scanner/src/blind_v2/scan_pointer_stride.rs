//! Constant-stride pointer-array 2-D table discovery and shift-snapping.
//!
//! Many ECUs (notably SuperH/Keihin/Denso and Tricore pointer tables) reference 2-D maps via dense
//! arrays of equally spaced ROM pointers `[ptr_0, ptr_1, ..., ptr_{k-1}]` where the stride directly
//! reveals `rows * cols * elem_bytes` regardless of whether `(rows, cols)` appears in any static shape prior.

use rayon::prelude::*;
use std::collections::BTreeMap;

use super::scan_oem_structs::{try_decode_z_surface, ZSurfaceSpec};
use super::{BlindV2Candidate, BlindV2Options, DiscoveryStatus};
use crate::nms_score::{composite_quality_score, ScoreInputs};
use crate::pointer_snap::Anchors;

/// Builds constant-stride pointer-array anchors (both endiannesses) in parallel.
pub(super) fn strong_pointer_anchors_v2(rom: &[u8]) -> Vec<Anchors> {
    let dom_be = rom.len() >= 65_536
        && matches!(
            crate::zero_xdf_scan::detect_global_endianness(rom),
            crate::zero_xdf_scan::Endianness::BigEndian
        );
    let (a_be, a_le) = rayon::join(
        || Anchors::constant_stride_only(rom, true),
        || Anchors::constant_stride_only(rom, false),
    );
    if dom_be {
        vec![a_be, a_le]
    } else {
        vec![a_le, a_be]
    }
}

/// Discovers 2-D tables referenced by constant-stride pointer arrays using exact stride factorizations.
pub(super) fn scan_pointer_stride_tables_with_anchors(
    rom: &[u8],
    anchors: &[Anchors],
    opts: &BlindV2Options,
) -> Vec<BlindV2Candidate> {
    // Dedup target addresses; first anchor slice in `anchors` matches dominant ROM endianness.
    let mut target_map: BTreeMap<usize, (usize, bool)> = BTreeMap::new();
    for anc in anchors {
        let targets = anc.strong_targets();
        if targets.is_empty() {
            continue;
        }
        // Detect anchor endianness via a probe: BE anchors come first when dom_be, LE first otherwise.
        let anc_is_be = std::ptr::eq(anc, &anchors[0])
            && rom.len() >= 65_536
            && matches!(
                crate::zero_xdf_scan::detect_global_endianness(rom),
                crate::zero_xdf_scan::Endianness::BigEndian
            )
            || (!std::ptr::eq(anc, &anchors[0])
                && !(rom.len() >= 65_536
                    && matches!(
                        crate::zero_xdf_scan::detect_global_endianness(rom),
                        crate::zero_xdf_scan::Endianness::BigEndian
                    )));
        let le = !anc_is_be;
        for (addr, stride) in targets {
            target_map.entry(addr).or_insert((stride, le));
        }
    }
    if target_map.is_empty() {
        return Vec::new();
    }

    let jobs: Vec<(usize, usize, bool)> = target_map
        .into_iter()
        .map(|(addr, (stride, le))| (addr, stride, le))
        .collect();

    let eval_target = |&(addr, stride, le): &(usize, usize, bool)| -> Option<BlindV2Candidate> {
        let mut cfgs: Vec<(usize, bool, bool, bool)> = vec![(2, le, false, false), (2, le, true, false)];
        if opts.include_8bit {
            cfgs.push((1, false, false, false));
            cfgs.push((1, false, true, false));
        }
        if opts.include_32bit && stride.is_multiple_of(4) && addr.is_multiple_of(4) {
            cfgs.push((4, le, false, true));
        }

        let mut best: Option<(f64, BlindV2Candidate)> = None;
        for (eb, cfg_le, is_signed, is_float) in cfgs {
            if !stride.is_multiple_of(eb) {
                continue;
            }
            let cells = stride / eb;
            if !(16..=4096).contains(&cells) || addr.saturating_add(stride) > rom.len() {
                continue;
            }

            let max_r = cells.min(64);
            for rows in 4..=max_r {
                if !cells.is_multiple_of(rows) {
                    continue;
                }
                let cols = cells / rows;
                if !(4..=64).contains(&cols) || rows.max(cols) > rows.min(cols) * 10 {
                    continue;
                }
                let Some(decoded) = try_decode_z_surface(
                    rom,
                    ZSurfaceSpec {
                        offset: addr,
                        rows,
                        cols,
                        elem_bytes: eb,
                        le: cfg_le,
                        is_signed,
                        is_float,
                    },
                ) else {
                    continue;
                };
                if decoded.ac < 0.45 || decoded.mdsd >= 0.42 {
                    continue;
                }
                let obj = decoded.ac - 0.6 * decoded.mdsd;
                let replace = match &best {
                    None => true,
                    Some((prev_obj, _)) => obj > *prev_obj,
                };
                if replace {
                    let score = composite_quality_score(&ScoreInputs {
                        rows,
                        cols,
                        z_address: addr,
                        smoothness: decoded.mdsd,
                        confidence_score: 0.96,
                        has_x_axis: false,
                        has_y_axis: false,
                        axes_contiguous: false,
                        first_row_is_flat: false,
                    });
                    best = Some((
                        obj,
                        BlindV2Candidate {
                            address: addr,
                            rows,
                            cols,
                            elem_bytes: eb,
                            little_endian: cfg_le,
                            dtype_label: decoded.label.to_string(),
                            is_curve: false,
                            x_axis: None,
                            y_axis: None,
                            smoothness: decoded.mdsd,
                            p23: None,
                            quality_score: score,
                            status: DiscoveryStatus::StructurallySupported,
                            detection_source: format!("pointer_stride_2d({})", decoded.label),
                        },
                    ));
                }
            }
        }
        best.map(|(_, c)| c)
    };

    if jobs.len() > 64 {
        jobs.par_iter().filter_map(eval_target).collect()
    } else {
        jobs.iter().filter_map(eval_target).collect()
    }
}

/// Discovers constant-stride pointer-array 2-D tables on `rom`.
#[allow(dead_code)]
pub(super) fn scan_pointer_stride_tables_v2(rom: &[u8], opts: &BlindV2Options) -> Vec<BlindV2Candidate> {
    let anchors = strong_pointer_anchors_v2(rom);
    scan_pointer_stride_tables_with_anchors(rom, &anchors, opts)
}

/// Snaps shifted prefiltered 2-D candidates onto strong constant-stride pointer targets within `+-32 B`
/// whenever decoding at the pointer target maintains or improves surface smoothness.
pub(super) fn snap_candidates_to_strong_anchors(
    rom: &[u8],
    cands: &mut [BlindV2Candidate],
    anchors: &[Anchors],
) {
    if anchors.is_empty() {
        return;
    }
    for cand in cands.iter_mut() {
        if cand.is_curve || cand.detection_source != "prefiltered_2d_v2" {
            continue;
        }
        let mut best_snap: Option<(usize, f64)> = None;
        for anc in anchors {
            let Some((target, _stride)) = anc.nearest_strong(cand.address, 32) else {
                continue;
            };
            if target == cand.address {
                best_snap = None;
                break;
            }
            let is_float = cand.dtype_label.starts_with("f32");
            let is_signed = cand.dtype_label.starts_with("i16") || cand.dtype_label == "i8";
            let Some(decoded) = try_decode_z_surface(
                rom,
                ZSurfaceSpec {
                    offset: target,
                    rows: cand.rows,
                    cols: cand.cols,
                    elem_bytes: cand.elem_bytes,
                    le: cand.little_endian,
                    is_signed,
                    is_float,
                },
            ) else {
                continue;
            };
            if decoded.mdsd <= cand.smoothness * 1.02 {
                let d_new = target.abs_diff(cand.address);
                let replace = match best_snap {
                    None => true,
                    Some((prev_t, prev_m)) => {
                        decoded.mdsd < prev_m - 1e-4
                            || ((decoded.mdsd - prev_m).abs() <= 1e-4 && d_new < prev_t.abs_diff(cand.address))
                    }
                };
                if replace {
                    best_snap = Some((target, decoded.mdsd));
                }
            }
        }
        if let Some((target, mdsd)) = best_snap {
            cand.address = target;
            cand.smoothness = mdsd;
        }
    }
}
