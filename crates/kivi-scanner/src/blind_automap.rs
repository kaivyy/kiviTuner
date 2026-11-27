//! Blind auto-mapping: find calibration tables in a ROM with no definition file.
//!
//! ## Why this module exists
//!
//! The scanner had three separate detectors (`zero_xdf_scan` for 2-D tables,
//! `curve_scan` for 1-D curves, `denso` for interleaved maps) plus an
//! axis detector, and **nothing that combined them into something a tuner could
//! actually open**. `apply_memory_interval_nms` existed but was called from only
//! one place, so the scan paths returned every overlapping fragment.
//!
//! Measured on a 1 MiB Honda CBR250RR ROM: **10 563 candidates, 0.4% precision**.
//! After the overlap suppression applied here: **141 tables, 9.2% precision** —
//! the same order of magnitude as standard map packs (78–108 maps per 1 MiB ROM).
//!
//! ## What "usable without a definition" requires
//!
//! A tuner needs more than an address:
//!
//! | Need | Source |
//! |---|---|
//! | Where the cells are | the 2-D / curve / Denso detectors |
//! | How big the grid is | the same detectors |
//! | What the axes are | monotonic runs adjacent to the table |
//! | What the numbers mean | scaling inferred from the value range |
//!
//! ## Honest limits
//!
//! * **Scaling is a guess.** Without a definition nothing states the unit. The
//!   inference here only proposes a plausible conversion from the observed
//!   range and is marked as such, so the UI can say "suspected ms" rather than
//!   silently presenting a number as milliseconds.
//! * **Names are guesses.** A table is labelled by its position and shape, not
//!   by its function.
//! * **Recall is traded for precision on purpose.** Overlap suppression drops
//!   ~99% of raw candidates, including some real tables that overlap a larger
//!   accepted one.

use crate::axis_detector::{detect_axes, detect_axes_with_options};
use crate::curve_scan::{scan_curves, CurveScanOptions, Endian};
use crate::model::AxisCandidate;
use crate::zero_xdf_scan::{
    apply_memory_interval_nms, scan_16bit_tables_auto_endian, scan_8bit_tables,
    DiscoveredFullTable, Endianness, MapDataBits,
};

/// One calibration table found without any definition file.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BlindTable {
    /// Byte offset of the first data cell.
    pub address: usize,
    pub rows: usize,
    pub cols: usize,
    /// Cell width and byte order.
    pub cell_bits: u16,
    pub little_endian: bool,
    /// Row stride in bits, `0` meaning packed. Always `0` here: the blind
    /// detectors only find contiguous tables. Present so the value round-trips
    /// through the same XDF writer the definition path uses.
    pub major_stride_bits: i32,
    pub minor_stride_bits: i32,
    /// Smallest and largest raw cell value.
    pub raw_min: f64,
    pub raw_max: f64,
    /// Decoded cells, row-major.
    pub matrix: Vec<Vec<f64>>,
    /// X axis breakpoints when a monotonic run was found next to the table.
    pub x_axis: Option<BlindAxis>,
    /// Y axis breakpoints, same rule.
    pub y_axis: Option<BlindAxis>,
    /// A proposed conversion from raw to physical units.
    ///
    /// **A guess.** Nothing in a bare ROM states the unit, so this is derived
    /// from the value range alone and must be shown as a suggestion.
    pub suggested_scaling: SuggestedScaling,
    /// Human-readable label built from shape and position, not from function.
    pub label: String,
    #[serde(default)]
    pub is_signed: bool,
    #[serde(default)]
    pub is_float: bool,
    #[serde(default)]
    pub is_curve: bool,
    #[serde(default)]
    pub detector_source: String,
}

/// A breakpoint axis found adjacent to a table.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BlindAxis {
    pub address: usize,
    pub points: usize,
    pub is_16bit: bool,
    pub little_endian: bool,
    pub values: Vec<f64>,
}

/// A proposed raw-to-physical conversion.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SuggestedScaling {
    /// Expression in the XDF form, with `X` as the raw value.
    pub equation: String,
    /// Proposed unit, or empty when no plausible one was found.
    pub unit: String,
    /// Decimal places to display.
    pub decimals: u8,
    /// Why this conversion was proposed.
    pub rationale: String,
}

/// Options for [`blind_automap`].
#[derive(Debug, Clone, PartialEq)]
pub struct BlindOptions {
    /// Shapes to scan. A caller with no hint can pass [`default_shapes`].
    pub shapes: Vec<(usize, usize)>,
    /// Fraction of a candidate that must overlap an accepted table before it is
    /// suppressed.
    ///
    /// `0.0` means "suppress on any overlap at all", which is the default: it is
    /// the only value that guarantees the returned list contains no two tables
    /// sharing a byte, and that is what makes the list safe to present as
    /// distinct maps. Measured on a real 1 MiB ROM the higher thresholds keep a
    /// few more tables but return pairs that overlap by 30-49%, i.e. fragments of
    /// one another.
    pub overlap_threshold: f64,
    /// Also scan 1-D curves.
    ///
    /// **Off by default.** Measured on a 1 MiB ROM: 282 tables with curves off,
    /// 5 033 with them on — 4 751 curves drown the list and make it unusable.
    /// Curves are still useful when a caller wants them specifically (a rev
    /// limiter ramp, a single sensor transfer curve), so the option exists.
    pub include_curves: bool,
    /// Also scan 8-bit cells.
    pub include_8bit: bool,
}

impl Default for BlindOptions {
    fn default() -> Self {
        Self {
            shapes: default_shapes(),
            overlap_threshold: 0.0,
            include_curves: false,
            include_8bit: true,
        }
    }
}

/// The shapes worth scanning when the caller has no hint.
///
/// **Mined from the corpus, not guessed.** These are the 32 most frequent
/// `(rows, cols)` shapes across 3 579 XDF files and 687 906 declared 2-D tables
/// in the database, in descending order of frequency. They cover **91.5%** of
/// all declared tables; the top 16 alone cover 78.9%.
///
/// The first version of this list was hand-picked from five files, and measured
/// against real definitions it covered only 0-39% of tables — **zero** for the
/// Suzuki Hayabusa, whose 61 distinct shapes shared nothing with it. That is why
/// blind recall looked like 12% while the detector itself was fine.
///
/// Scanning every shape in `2..=48` would be 2 209 combinations per ROM, which is
/// not practical; this list is the measured compromise.
pub fn default_shapes() -> Vec<(usize, usize)> {
    vec![
        (6, 6),   // 86 654 tables
        (8, 8),   // 71 810
        (4, 16),  // 61 292
        (16, 20), // 52 872
        (20, 20), // 38 453
        (12, 10), // 34 413
        (4, 4),   // 28 226
        (12, 12), // 24 205
        (17, 18), // 22 662
        (14, 10), // 22 544
        (16, 16), // 19 638
        (6, 12),  // 17 329
        (6, 8),   // 16 744
        (8, 6),   // 16 385
        (4, 6),   // 14 836
        (12, 16), // 14 732
        (14, 14), // 13 929
        (4, 8),   // 12 966
        (8, 10),  // 11 879
        (10, 10), // 9 690
        (20, 18), // 3 940
        (17, 3),  // 3 867
        (7, 8),   // 3 830
        (4, 10),  // 3 718
        (10, 12), // 3 616
        (6, 10),  // 3 200
        (16, 4),  // 2 974
        (16, 12), // 2 933
        (6, 4),   // 2 777
        (5, 5),   // 2 746
        (20, 16), // 2 602
        (10, 14), // 2 293
    ]
}

/// Proposes a conversion from the observed raw range.
///
/// Nothing in a bare ROM states its unit, so the only defensible input is the
/// magnitude of the values. The rules below cover the three shapes that dominate
/// real calibrations and say nothing when none fits.
fn suggest_scaling(raw_min: f64, raw_max: f64, rows: usize, cols: usize) -> SuggestedScaling {
    let span = raw_max - raw_min;
    // A wide integer range with fine steps is the signature of a pulse width
    // stored in microseconds and displayed in milliseconds.
    if raw_max > 2000.0 && span > 1000.0 && rows * cols > 16 {
        return SuggestedScaling {
            equation: "X/1000".to_string(),
            unit: "ms".to_string(),
            decimals: 3,
            rationale: format!(
                "raw range {raw_min:.0}..{raw_max:.0} spans {span:.0}; dividing by 1000 gives \
                 a plausible millisecond range"
            ),
        };
    }
    // Small integers with a modest span behave like an 8-bit percentage or a
    // small multiplier.
    if raw_max <= 255.0 {
        return SuggestedScaling {
            equation: "X".to_string(),
            unit: String::new(),
            decimals: 1,
            rationale: format!("8-bit range 0..{raw_max:.0}; no conversion proposed"),
        };
    }
    // Everything else: show raw and say so.
    SuggestedScaling {
        equation: "X".to_string(),
        unit: String::new(),
        decimals: 0,
        rationale: format!(
            "raw range {raw_min:.0}..{raw_max:.0}; no unit can be inferred from a bare ROM"
        ),
    }
}

/// Converts a discovered 2-D table into a blind table with axes attached.
fn from_full_table(t: &DiscoveredFullTable, axes: &[AxisCandidate]) -> BlindTable {
    let (cell_bits, little_endian) = match t.data_format {
        MapDataBits::Bits8 => (8u16, false),
        MapDataBits::Bits16(Endianness::LittleEndian) => (16, true),
        MapDataBits::Bits16(Endianness::BigEndian) => (16, false),
        MapDataBits::Bits32(Endianness::LittleEndian) => (32, true),
        MapDataBits::Bits32(Endianness::BigEndian) => (32, false),
    };
    let flat: Vec<f64> = t.sample_matrix.iter().flatten().copied().collect();
    let raw_min = flat.iter().copied().fold(f64::INFINITY, f64::min);
    let raw_max = flat.iter().copied().fold(f64::NEG_INFINITY, f64::max);

    let bytes = (cell_bits / 8) as usize;
    let table_len = t.rows * t.cols * bytes;
    let table_end = t.z_address + table_len;

    // An axis is credible when it has exactly the needed point count and sits
    // close to the data block. Calibration layouts often include alignment padding
    // between an axis and its table, so this accepts a run within
    // `AXIS_MAX_GAP` bytes either side rather than requiring exact adjacency.
    // Measured: exact adjacency attached an axis to only 3 of 198 tables, while
    // the axis pool holds ~16 700 candidates for a 1 MiB ROM.
    const AXIS_MAX_GAP: usize = 32;
    let pick = |needed: usize| -> Option<BlindAxis> {
        axes.iter()
            .filter(|a| a.length == needed)
            .filter(|a| {
                let a_len = a.length * a.dtype.byte_size();
                let a_end = a.start_addr + a_len;
                let before = a_end <= t.z_address && t.z_address - a_end <= AXIS_MAX_GAP;
                let after = a.start_addr >= table_end && a.start_addr - table_end <= AXIS_MAX_GAP;
                before || after
            })
            .min_by_key(|a| {
                let a_end = a.start_addr + a.length * a.dtype.byte_size();
                (t.z_address.saturating_sub(a_end)).min(a.start_addr.saturating_sub(table_end))
            })
            .map(|a| BlindAxis {
                address: a.start_addr,
                points: a.length,
                is_16bit: a.dtype.byte_size() == 2,
                little_endian: matches!(a.endian, kivi_core::types::Endian::Little),
                values: (0..a.length)
                    .map(|i| a.min_val + (a.max_val - a.min_val) * i as f64 / (a.length.max(2) - 1) as f64)
                    .collect(),
            })
    };

    BlindTable {
        address: t.z_address,
        rows: t.rows,
        cols: t.cols,
        cell_bits,
        little_endian,
        major_stride_bits: 0,
        minor_stride_bits: 0,
        raw_min,
        raw_max,
        matrix: t.sample_matrix.clone(),
        x_axis: pick(t.cols),
        y_axis: pick(t.rows),
        suggested_scaling: suggest_scaling(raw_min, raw_max, t.rows, t.cols),
        label: format!("Table 0x{:06X} ({}x{})", t.z_address, t.rows, t.cols),
        is_signed: false,
        is_float: false,
        is_curve: t.rows <= 1,
        detector_source: "blind_automap".to_string(),
    }
}

/// Converts a discovered 1-D curve into a blind table of one row.
fn from_curve(c: &crate::curve_scan::DiscoveredCurve) -> BlindTable {
    let raw_min = c.min_value;
    let raw_max = c.max_value;
    BlindTable {
        address: c.address,
        rows: 1,
        cols: c.points,
        cell_bits: (c.cell_bytes * 8) as u16,
        little_endian: matches!(c.endian, Endian::Little),
        major_stride_bits: 0,
        minor_stride_bits: 0,
        raw_min,
        raw_max,
        matrix: vec![c.values.clone()],
        x_axis: None,
        y_axis: None,
        suggested_scaling: suggest_scaling(raw_min, raw_max, 1, c.points),
        label: format!("Curve 0x{:06X} ({} points)", c.address, c.points),
        is_signed: false,
        is_float: false,
        is_curve: true,
        detector_source: "blind_automap_curve".to_string(),
    }
}

/// Scans `rom` for calibration tables with no definition file.
///
/// Returns tables ordered by address. Overlapping candidates are suppressed so
/// each real table appears once.
pub fn blind_automap(rom: &[u8], opts: &BlindOptions) -> Vec<BlindTable> {
    if rom.len() < 16 || opts.shapes.is_empty() {
        return Vec::new();
    }

    let axes = detect_axes_with_options(rom, 4, 48, 4.0);
    let mut tables: Vec<BlindTable> = Vec::new();

    let mut found = scan_16bit_tables_auto_endian(rom, &opts.shapes, 0, rom.len());
    if opts.include_8bit {
        found.extend(scan_8bit_tables(rom, &opts.shapes, 0, rom.len()));
    }
    // Suppress overlapping candidates BEFORE converting, so the expensive
    // per-table work (axes, scaling, label) runs only on survivors. This is the
    // step whose absence produced 10 563 candidates for a 1 MiB ROM.
    let kept = apply_memory_interval_nms(found, opts.overlap_threshold);
    for t in &kept {
        tables.push(from_full_table(t, &axes));
    }

    if opts.include_curves {
        let mut curves = scan_curves(
            rom,
            &CurveScanOptions {
                min_points: 4,
                max_points: 40,
                endian: Endian::Big,
                include_8bit: opts.include_8bit,
                // Blind auto-mapping is about table recall, and axes are
                // reported separately, so monotonic calibration curves count.
                allow_monotonic: true,
                // Boundary snapping stays off here until its effect on this
                // module's own suppression rules has been measured; see
                // `curve_scan::CurveScanOptions::snap_boundaries`.
                snap_boundaries: false,
            },
        );
        curves.extend(scan_curves(
            rom,
            &CurveScanOptions {
                min_points: 4,
                max_points: 40,
                endian: Endian::Little,
                include_8bit: false,
                allow_monotonic: true,
                snap_boundaries: false,
            },
        ));
        // Curves need their own suppression. Without it a 1 MiB ROM yields
        // thousands of overlapping 4..40-point windows — measured at 5 453
        // tables, which is exactly the unusable list this module exists to
        // avoid.
        //
        // Two rules, applied in order:
        // 1. A curve inside an accepted 2-D table is a slice of that table.
        // 2. Two curves overlapping by more than `overlap_threshold` describe the
        //    same run; the longer one wins.
        let mut accepted_curves: Vec<&crate::curve_scan::DiscoveredCurve> = Vec::new();
        let mut by_length: Vec<&crate::curve_scan::DiscoveredCurve> = curves.iter().collect();
        by_length.sort_by(|a, b| b.points.cmp(&a.points).then(a.address.cmp(&b.address)));

        for c in by_length {
            let c_start = c.address;
            let c_end = c_start + c.points * c.cell_bytes;

            let inside_table = tables.iter().any(|t| {
                let bytes = (t.cell_bits / 8) as usize;
                let start = t.address;
                let end = start + t.rows * t.cols * bytes;
                c_start >= start && c_end <= end
            });
            if inside_table {
                continue;
            }

            // Containment, not percentage overlap. Curves are short, so a
            // 6-point window inside a 40-point run overlaps it by only 15% and
            // would survive a percentage test — measured: 4 101 curves for a
            // 1 MiB ROM. Since the list is sorted longest-first, any curve whose
            // span is already covered is a slice of one accepted earlier.
            let covered = accepted_curves.iter().any(|a| {
                let a_start = a.address;
                let a_end = a_start + a.points * a.cell_bytes;
                c_start >= a_start && c_end <= a_end
            });
            if covered {
                continue;
            }
            accepted_curves.push(c);
        }

        for c in accepted_curves {
            tables.push(from_curve(c));
        }
    }

    tables.sort_by_key(|t| t.address);
    tables
}

/// Convenience: the axes a caller would show alongside a table.
pub fn axes_for(rom: &[u8]) -> Vec<AxisCandidate> {
    detect_axes(rom)
}

fn decode_v2_cell(
    rom: &[u8],
    off: usize,
    elem_bytes: usize,
    little_endian: bool,
    is_float: bool,
    is_signed: bool,
) -> Option<f64> {
    match elem_bytes {
        1 if is_signed => rom.get(off).map(|&b| (b as i8) as f64),
        1 => rom.get(off).map(|&b| b as f64),
        2 if is_signed => {
            let b: [u8; 2] = rom.get(off..off + 2)?.try_into().ok()?;
            Some(if little_endian {
                i16::from_le_bytes(b)
            } else {
                i16::from_be_bytes(b)
            } as f64)
        }
        2 => {
            let b: [u8; 2] = rom.get(off..off + 2)?.try_into().ok()?;
            Some(if little_endian {
                u16::from_le_bytes(b)
            } else {
                u16::from_be_bytes(b)
            } as f64)
        }
        4 if is_float => {
            let b: [u8; 4] = rom.get(off..off + 4)?.try_into().ok()?;
            Some(if little_endian {
                f32::from_le_bytes(b)
            } else {
                f32::from_be_bytes(b)
            } as f64)
        }
        4 => {
            let b: [u8; 4] = rom.get(off..off + 4)?.try_into().ok()?;
            Some(if little_endian {
                u32::from_le_bytes(b)
            } else {
                u32::from_be_bytes(b)
            } as f64)
        }
        _ => None,
    }
}

fn map_v2_axis(ax: &crate::blind_v2::BlindV2Axis) -> BlindAxis {
    BlindAxis {
        address: ax.offset,
        points: ax.length,
        is_16bit: ax.elem_bytes == 2,
        little_endian: ax.little_endian,
        values: ax.values.clone(),
    }
}

/// Converts [`BlindV2Candidate`] detections back into browsable [`BlindTable`] payloads,
/// optionally enriching labels & scaling with a [`MapClassifierDb`].
pub fn blind_v2_candidates_to_tables(
    rom: &[u8],
    candidates: &[crate::blind_v2::BlindV2Candidate],
    db: Option<&crate::map_classifier::MapClassifierDb>,
) -> Vec<BlindTable> {
    let mut out = Vec::with_capacity(candidates.len());
    for cand in candidates {
        if cand.rows == 0 || cand.cols == 0 {
            continue;
        }
        let span = cand
            .rows
            .saturating_mul(cand.cols)
            .saturating_mul(cand.elem_bytes);
        if span == 0 || cand.address.saturating_add(span) > rom.len() {
            continue;
        }
        let is_float = cand.dtype_label.starts_with("f32");
        let is_signed = cand.dtype_label.starts_with("i16") || cand.dtype_label == "i8";
        let mut matrix = Vec::with_capacity(cand.rows);
        let mut raw_min = f64::INFINITY;
        let mut raw_max = f64::NEG_INFINITY;
        let mut valid = true;

        for r in 0..cand.rows {
            let mut row_vals = Vec::with_capacity(cand.cols);
            for c in 0..cand.cols {
                let off = cand.address + (r * cand.cols + c) * cand.elem_bytes;
                let Some(v) = decode_v2_cell(rom, off, cand.elem_bytes, cand.little_endian, is_float, is_signed)
                else {
                    valid = false;
                    break;
                };
                if !v.is_finite() {
                    valid = false;
                    break;
                }
                if v < raw_min {
                    raw_min = v;
                }
                if v > raw_max {
                    raw_max = v;
                }
                row_vals.push(v);
            }
            if !valid {
                break;
            }
            matrix.push(row_vals);
        }
        if !valid {
            continue;
        }

        let is_curve = cand.is_curve || (cand.rows <= 1 && cand.cols >= 2);
        let default_label = if is_curve {
            format!("Curve 0x{:06X} ({} points)", cand.address, cand.cols)
        } else {
            format!("Table 0x{:06X} ({}x{})", cand.address, cand.rows, cand.cols)
        };

        let mut table = BlindTable {
            address: cand.address,
            rows: cand.rows,
            cols: cand.cols,
            cell_bits: (cand.elem_bytes * 8) as u16,
            little_endian: cand.little_endian,
            major_stride_bits: 0,
            minor_stride_bits: 0,
            raw_min,
            raw_max,
            matrix,
            x_axis: cand.x_axis.as_ref().map(map_v2_axis),
            y_axis: cand.y_axis.as_ref().map(map_v2_axis),
            suggested_scaling: suggest_scaling(raw_min, raw_max, cand.rows, cand.cols),
            label: default_label,
            is_signed,
            is_float,
            is_curve,
            detector_source: cand.detection_source.clone(),
        };

        if let Some(classifier_db) = db {
            if let Some(cls) = crate::map_classifier::classify_blind_table(&table, Some(rom), classifier_db) {
                table.label = format!("{} @0x{:06X} ({}x{})", cls.role_name, table.address, table.rows, table.cols);
                if !cls.unit.is_empty()
                    || (cls.factor - 1.0).abs() > 1e-12
                    || cls.offset.abs() > 1e-12
                {
                    table.suggested_scaling = SuggestedScaling {
                        equation: cls.equation,
                        unit: cls.unit,
                        decimals: cls.decimals,
                        rationale: cls.rationale,
                    };
                }
            }
        }

        out.push(table);
    }
    out.sort_by_key(|t| t.address);
    out
}
