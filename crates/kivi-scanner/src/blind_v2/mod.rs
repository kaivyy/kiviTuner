//! Blind V2 multi-width, bidirectional, multi-detector ROM discovery engine.
//!
//! Closes the quantified Wave 3.3 discovery gaps without mutating frozen Scanner C
//! (`crates/kivi-scanner/src/axis_anchored.rs`):
//! - **`B-AX`:** detects `8-bit` AND `16-bit` axes in BOTH increasing and decreasing
//!   directions, using `y_len * y.elem_bytes` for `y_end`.
//! - **`B-NA`:** integrates prefiltered 2-D `16-bit` and `8-bit` table scans so
//!   tables without separate ROM breakpoint runs (`S4.7`) are discovered.
//! - **`B-DT`:** adds 4-byte-aligned `u32` and finite physical-range `f32` 2-D scanning.
//! - **P2.3 descriptors:** attaches disambiguated finite-difference descriptors to
//!   every 2-D candidate without using `Δxy` as an accept/reject gate.

use serde::{Deserialize, Serialize};

use crate::axis_anchored::AxisKind;
use crate::blind_automap::default_shapes;
use crate::p23_descriptors::{compute_p23_descriptors, P23Descriptors};

mod axes;
mod gates;
mod prefix;
mod prefilter;
mod scan_32bit;
mod scan_anchored;
mod scan_curves_v2;
mod scan_oem_structs;
mod scan_pointer_stride;
mod scan_prefiltered;

pub use axes::build_axis_pool_v2;
pub use gates::{gearys_c_2d, passes_statistical_curve_gates};
use axes::build_axis_pool_v2_in_ranges;
use prefilter::calibration_ranges_v2;
pub use scan_32bit::scan_32bit_tables;
use scan_32bit::scan_32bit_tables_in_ranges;
pub use scan_anchored::scan_axis_anchored_v2;
use scan_anchored::scan_axis_anchored_v2_with_pool;
use scan_curves_v2::attach_curves_v2_in_ranges;
use scan_oem_structs::{scan_bosch_siemens_structs, scan_denso_sh_descriptors_v2};
use scan_pointer_stride::{
    scan_pointer_stride_tables_with_anchors, snap_candidates_to_strong_anchors,
    strong_pointer_anchors_v2,
};
use scan_prefiltered::scan_prefiltered_8_and_16_v2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiscoveryStatus {
    StructurallySupported,
    Discovered,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AxisDirection {
    Increasing,
    Decreasing,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlindV2Axis {
    pub offset: usize,
    pub length: usize,
    pub elem_bytes: usize,
    pub little_endian: bool,
    pub direction: AxisDirection,
    pub kind: AxisKind,
    pub values: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlindV2Candidate {
    pub address: usize,
    pub rows: usize,
    pub cols: usize,
    pub elem_bytes: usize,
    pub little_endian: bool,
    pub dtype_label: String,
    pub is_curve: bool,
    pub x_axis: Option<BlindV2Axis>,
    pub y_axis: Option<BlindV2Axis>,
    pub smoothness: f64,
    pub p23: Option<P23Descriptors>,
    pub quality_score: f64,
    pub status: DiscoveryStatus,
    pub detection_source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlindV2Options {
    pub include_8bit: bool,
    pub include_32bit: bool,
    pub include_curves: bool,
    pub include_prefiltered_2d: bool,
    pub scan_both_16bit_endians: bool,
    pub max_gap: usize,
    pub padding_offsets: Vec<usize>,
    pub extra_shapes: Vec<(usize, usize)>,
}

impl Default for BlindV2Options {
    fn default() -> Self {
        Self {
            include_8bit: true,
            include_32bit: true,
            include_curves: false,
            include_prefiltered_2d: true,
            scan_both_16bit_endians: true,
            max_gap: 64,
            padding_offsets: vec![0, 2, 4],
            extra_shapes: Vec::new(),
        }
    }
}


fn non_blank_ranges(rom: &[u8]) -> Vec<(usize, usize)> {
    const BLK: usize = 4096;
    if rom.len() < BLK {
        return vec![(0, rom.len())];
    }
    let mut out = Vec::new();
    let mut in_run = false;
    let mut run_start = 0usize;
    let blocks = rom.len().div_ceil(BLK);
    for b in 0..blocks {
        let s = b * BLK;
        let e = (s + BLK).min(rom.len());
        let chunk = &rom[s..e];
        let mut counts = [0u16; 256];
        let mut max_c = 0u16;
        for &byte in chunk {
            counts[byte as usize] += 1;
            if counts[byte as usize] > max_c {
                max_c = counts[byte as usize];
            }
        }
        let is_blank = (max_c as usize) * 100 >= chunk.len() * 98;
        if is_blank {
            if in_run {
                in_run = false;
                out.push((run_start, s));
            }
        } else if !in_run {
            in_run = true;
            run_start = s;
        }
    }
    if in_run {
        out.push((run_start, rom.len()));
    }
    out
}

#[inline]
fn detector_tier(src: &str) -> u8 {
    if src.starts_with("denso_sh_descriptor")
        || src.starts_with("bosch_siemens_struct")
        || src.starts_with("motronic_8bit_struct")
        || src.starts_with("pointer_stride_2d")
    {
        3
    } else if src.starts_with("axis_anchored_v2") {
        2
    } else {
        1
    }
}

pub fn attach_curves_v2(
    rom: &[u8],
    accepted_2d: &[BlindV2Candidate],
    include_8bit: bool,
) -> Vec<BlindV2Candidate> {
    let kept_ranges = calibration_ranges_v2(rom);
    let axis_pool = build_axis_pool_v2_in_ranges(rom, &kept_ranges, include_8bit, false);
    attach_curves_v2_in_ranges(rom, &kept_ranges, accepted_2d, &axis_pool, include_8bit, false)
}

/// Runs the complete Blind V2 multi-detector discovery pipeline on a raw ROM buffer.
pub fn blind_discover_v2(rom: &[u8], opts: &BlindV2Options) -> Vec<BlindV2Candidate> {
    if rom.len() < 32 {
        return Vec::new();
    }

    let kept_ranges = calibration_ranges_v2(rom);
    let nb_ranges;
    let anchored_ranges: &[(usize, usize)] = if rom.len() < 524_288 {
        nb_ranges = non_blank_ranges(rom);
        &nb_ranges
    } else {
        &kept_ranges
    };
    let axis_pool =
        build_axis_pool_v2_in_ranges(rom, anchored_ranges, opts.include_8bit, opts.include_32bit);
    let strong_anchors = strong_pointer_anchors_v2(rom);

    let mut all_2d: Vec<BlindV2Candidate> = scan_axis_anchored_v2_with_pool(rom, &axis_pool, opts);

    all_2d.extend(scan_bosch_siemens_structs(rom, &kept_ranges, opts));
    all_2d.extend(scan_denso_sh_descriptors_v2(rom, opts));
    all_2d.extend(scan_pointer_stride_tables_with_anchors(
        rom,
        &strong_anchors,
        opts,
    ));

    let mut shapes = default_shapes();
    for &(r, c) in &[(8, 12), (12, 8), (8, 16), (16, 8), (12, 16), (16, 12)] {
        if !shapes.contains(&(r, c)) {
            shapes.push((r, c));
        }
    }
    for &(r, c) in &opts.extra_shapes {
        if r >= 2 && c >= 2 && !shapes.contains(&(r, c)) {
            shapes.push((r, c));
        }
    }

    if opts.include_prefiltered_2d {
        let mut pre_2d = scan_prefiltered_8_and_16_v2(rom, &kept_ranges, &shapes, opts);
        snap_candidates_to_strong_anchors(rom, &mut pre_2d, &strong_anchors);
        all_2d.extend(pre_2d);
    }

    if opts.include_32bit {
        let mut shapes32 = vec![
            (4, 4),
            (6, 6),
            (8, 8),
            (10, 10),
            (12, 12),
            (16, 16),
            (4, 8),
            (8, 4),
            (6, 8),
            (8, 6),
            (8, 12),
            (12, 8),
            (8, 16),
            (16, 8),
            (12, 16),
            (16, 12),
        ];
        for &s in &opts.extra_shapes {
            if !shapes32.contains(&s) {
                shapes32.push(s);
            }
        }
        all_2d.extend(scan_32bit_tables_in_ranges(
            rom,
            &kept_ranges,
            &shapes32,
        ));
    }

    // Strict 3-tier non-overlapping NMS on 2-D candidates, deterministic under rayon.
    all_2d.sort_by(|a, b| {
        detector_tier(&b.detection_source)
            .cmp(&detector_tier(&a.detection_source))
            .then_with(|| b.quality_score.total_cmp(&a.quality_score))
            .then_with(|| a.address.cmp(&b.address))
            .then_with(|| b.rows.cmp(&a.rows))
            .then_with(|| b.cols.cmp(&a.cols))
            .then_with(|| a.elem_bytes.cmp(&b.elem_bytes))
            .then_with(|| a.dtype_label.cmp(&b.dtype_label))
    });

    let mut occupied_2d = vec![false; rom.len()];
    let mut kept: Vec<BlindV2Candidate> = Vec::new();
    for cand in all_2d {
        let c_start = cand.address;
        let c_end = c_start.saturating_add(cand.rows * cand.cols * cand.elem_bytes);
        if c_start >= c_end || c_end > occupied_2d.len() {
            continue;
        }
        if occupied_2d[c_start..c_end].iter().any(|&b| b) {
            continue;
        }
        occupied_2d[c_start..c_end].fill(true);
        kept.push(cand);
    }

    // Populate P2.3 descriptors ONLY on post-NMS surviving 2-D tables.
    {
        use rayon::prelude::*;
        kept.par_iter_mut().for_each(|cand| {
            if cand.is_curve || cand.p23.is_some() || cand.rows < 2 || cand.cols < 2 {
                return;
            }
            let span = cand.rows.saturating_mul(cand.cols).saturating_mul(cand.elem_bytes);
            if span == 0 || cand.address.saturating_add(span) > rom.len() {
                return;
            }
            let is_float = cand.dtype_label.starts_with("f32");
            let is_signed = cand.dtype_label.starts_with("i16") || cand.dtype_label == "i8";
            let mut grid = Vec::with_capacity(cand.rows);
            for r in 0..cand.rows {
                let mut row = Vec::with_capacity(cand.cols);
                for c in 0..cand.cols {
                    let off = cand.address + (r * cand.cols + c) * cand.elem_bytes;
                    let v = match cand.elem_bytes {
                        1 if is_signed => (rom[off] as i8) as f64,
                        1 => rom[off] as f64,
                        2 if is_signed => {
                            let b = [rom[off], rom[off + 1]];
                            (if cand.little_endian { i16::from_le_bytes(b) } else { i16::from_be_bytes(b) }) as f64
                        }
                        2 => axes::read_u16(rom, off, cand.little_endian).unwrap_or(0) as f64,
                        4 if is_float => axes::read_f32(rom, off, cand.little_endian).unwrap_or(0.0) as f64,
                        4 => axes::read_u32(rom, off, cand.little_endian).unwrap_or(0) as f64,
                        _ => 0.0,
                    };
                    row.push(v);
                }
                grid.push(row);
            }
            cand.p23 = compute_p23_descriptors(&grid);
        });
    }

    if opts.include_curves {
        let curves = attach_curves_v2_in_ranges(
            rom,
            &kept_ranges,
            &kept,
            &axis_pool,
            opts.include_8bit,
            opts.include_32bit,
        );
        kept.extend(curves);
    }

    kept.sort_by_key(|t| t.address);
    kept
}


#[cfg(test)]
mod tests;
