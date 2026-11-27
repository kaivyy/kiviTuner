//! Tests for the composite quality score used by Memory Interval NMS.
//!
//! ## Why this exists
//!
//! kiviTuner's NMS previously sorted candidates by `confidence_score` alone.
//! The reference implementation sorts by a **composite** score:
//!
//! ```text
//! Score = (axis_bonus × align_bonus × baseline_bonus × area) / (smoothness + ε)
//! ```
//!
//! Sorting by confidence alone keeps the wrong candidate when two detections
//! overlap: a large smooth table with a good axis pairing can lose to a small
//! high-confidence fragment, and the fragment then suppresses the real table.
//!
//! ## What each factor means
//!
//! - `axis_bonus` — 12.0 for an exact OEM descriptor or a contiguous layout,
//!   2.0 for a blind monotonic guess, 1.5 for one axis, 1.0 for none.
//! - `align_bonus` — 1.25 at 16-byte alignment, 1.1 at 4-byte, else 1.0.
//! - `baseline_bonus` — 1.5 when the first row is flat (a limiter plateau).
//! - `area` — `rows × cols`, so a bigger real table outranks a fragment.
//! - `smoothness` — in the denominator: smoother is better.

use kivi_scanner::nms_score::{composite_quality_score, ScoreInputs};

fn base() -> ScoreInputs {
    ScoreInputs {
        rows: 16,
        cols: 16,
        z_address: 0x1000,
        smoothness: 0.2,
        confidence_score: 0.8,
        has_x_axis: true,
        has_y_axis: true,
        axes_contiguous: false,
        first_row_is_flat: false,
    }
}

// ---------------------------------------------------------------------------
// Axis bonus
// ---------------------------------------------------------------------------

#[test]
fn a_contiguous_axis_pair_scores_much_higher_than_no_axes() {
    let with = ScoreInputs {
        axes_contiguous: true,
        ..base()
    };
    let without = ScoreInputs {
        has_x_axis: false,
        has_y_axis: false,
        ..base()
    };
    assert!(composite_quality_score(&with) > composite_quality_score(&without) * 5.0);
}

#[test]
fn a_high_confidence_pair_gets_the_descriptor_bonus_without_contiguity() {
    // confidence >= 0.95 is treated as an exact OEM descriptor.
    let high = ScoreInputs {
        confidence_score: 0.97,
        ..base()
    };
    let low = ScoreInputs {
        confidence_score: 0.50,
        ..base()
    };
    assert!(composite_quality_score(&high) > composite_quality_score(&low));
}

#[test]
fn one_axis_scores_between_none_and_both() {
    let none = ScoreInputs {
        has_x_axis: false,
        has_y_axis: false,
        ..base()
    };
    let one = ScoreInputs {
        has_x_axis: true,
        has_y_axis: false,
        ..base()
    };
    let both = ScoreInputs {
        axes_contiguous: true,
        ..base()
    };
    let (n, o, b) = (
        composite_quality_score(&none),
        composite_quality_score(&one),
        composite_quality_score(&both),
    );
    assert!(n < o && o < b, "expected none<{n} one<{o} both<{b}");
}

// ---------------------------------------------------------------------------
// Alignment bonus
// ---------------------------------------------------------------------------

#[test]
fn a_16_byte_aligned_table_outranks_an_unaligned_one() {
    let aligned = ScoreInputs {
        z_address: 0x1000,
        ..base()
    };
    let unaligned = ScoreInputs {
        z_address: 0x1002,
        ..base()
    };
    assert!(composite_quality_score(&aligned) > composite_quality_score(&unaligned));
}

#[test]
fn a_4_byte_aligned_table_outranks_a_2_byte_one() {
    let four = ScoreInputs {
        z_address: 0x1004,
        ..base()
    };
    let two = ScoreInputs {
        z_address: 0x1002,
        ..base()
    };
    assert!(composite_quality_score(&four) > composite_quality_score(&two));
}

// ---------------------------------------------------------------------------
// Baseline bonus
// ---------------------------------------------------------------------------

#[test]
fn a_flat_first_row_scores_higher() {
    // A flat first row is the signature of a limiter plateau.
    let flat = ScoreInputs {
        first_row_is_flat: true,
        ..base()
    };
    let sloped = base();
    assert!(composite_quality_score(&flat) > composite_quality_score(&sloped));
}

// ---------------------------------------------------------------------------
// Area and smoothness
// ---------------------------------------------------------------------------

#[test]
fn a_bigger_table_outranks_a_smaller_one_with_the_same_shape() {
    let big = ScoreInputs {
        rows: 32,
        cols: 32,
        ..base()
    };
    let small = ScoreInputs {
        rows: 8,
        cols: 8,
        ..base()
    };
    assert!(composite_quality_score(&big) > composite_quality_score(&small));
}

#[test]
fn a_smoother_table_outranks_a_rougher_one() {
    let smooth = ScoreInputs {
        smoothness: 0.05,
        ..base()
    };
    let rough = ScoreInputs {
        smoothness: 0.9,
        ..base()
    };
    assert!(composite_quality_score(&smooth) > composite_quality_score(&rough));
}

#[test]
fn the_score_is_finite_even_at_zero_smoothness() {
    // epsilon keeps this from dividing by zero.
    let s = ScoreInputs {
        smoothness: 0.0,
        ..base()
    };
    assert!(composite_quality_score(&s).is_finite());
}

#[test]
fn the_score_is_finite_for_an_empty_table() {
    let s = ScoreInputs {
        rows: 0,
        cols: 0,
        ..base()
    };
    assert!(composite_quality_score(&s).is_finite());
}

// ---------------------------------------------------------------------------
// The case that motivated this
// ---------------------------------------------------------------------------

#[test]
fn a_large_smooth_table_beats_a_small_confident_fragment() {
    // This is the exact regression: the fragment has higher confidence but
    // covers a fraction of the bytes, and sorting by confidence alone kept it.
    let real_table = ScoreInputs {
        rows: 39,
        cols: 23,
        z_address: 0x15750A,
        smoothness: 0.08,
        confidence_score: 0.72,
        has_x_axis: true,
        has_y_axis: true,
        axes_contiguous: true,
        first_row_is_flat: false,
    };
    let fragment = ScoreInputs {
        rows: 12,
        cols: 12,
        z_address: 0x15750A,
        smoothness: 0.30,
        confidence_score: 0.91,
        has_x_axis: true,
        has_y_axis: true,
        axes_contiguous: false,
        first_row_is_flat: false,
    };
    assert!(
        composite_quality_score(&real_table) > composite_quality_score(&fragment),
        "the real 39x23 table must outrank a 12x12 fragment at the same address"
    );
}

// ---------------------------------------------------------------------------
// The fix must reach the NMS itself, not only the score function
// ---------------------------------------------------------------------------

#[test]
fn nms_keeps_the_large_table_when_a_fragment_overlaps_it() {
    // End-to-end: a 12x12 fragment with higher confidence sits inside a 39x23
    // table. Sorting by confidence alone would keep the fragment and discard the
    // real table. The composite score must keep the table.
    use kivi_scanner::zero_xdf_scan::{
        apply_memory_interval_nms, DiscoveredAxis, DiscoveredFullTable, Endianness, MapDataBits,
    };

    fn axis(address: usize, is_16bit: bool) -> DiscoveredAxis {
        DiscoveredAxis {
            address,
            points_count: 16,
            is_16bit,
            endianness: Endianness::BigEndian,
            estimated_unit: "rpm".to_string(),
            min_val: 0.0,
            max_val: 7000.0,
            values: (0..16).map(|i| (i * 500) as f64).collect(),
        }
    }

    fn table(rows: usize, cols: usize, z: usize, smoothness: f64, conf: f64) -> DiscoveredFullTable {
        let matrix: Vec<Vec<f64>> = (0..rows)
            .map(|r| (0..cols).map(|c| ((r * cols + c) % 17) as f64).collect())
            .collect();
        DiscoveredFullTable {
            z_address: z,
            rows,
            cols,
            data_format: MapDataBits::Bits16(Endianness::BigEndian),
            semantic_type: kivi_scanner::zero_xdf_scan::SemanticMapType::UnknownTable,
            confidence_score: conf,
            smoothness,
            x_axis: Some(axis(z.saturating_sub(64), true)),
            y_axis: Some(axis(z.saturating_sub(128), true)),
            sample_matrix: matrix,
        }
    }

    // Fragment is inside the real table's byte range and has higher confidence.
    let real = table(39, 23, 0x15750A, 0.08, 0.72);
    let fragment = table(12, 12, 0x15750A, 0.30, 0.91);

    let kept = apply_memory_interval_nms(vec![fragment, real.clone()], 0.10);
    assert_eq!(kept.len(), 1, "only one table should survive");
    assert_eq!(kept[0].rows, 39, "the 39-row table must win");
    assert_eq!(kept[0].cols, 23);
}

#[test]
fn nms_keeps_tables_at_different_addresses() {
    use kivi_scanner::zero_xdf_scan::{
        apply_memory_interval_nms, DiscoveredFullTable, Endianness, MapDataBits,
    };

    fn t(z: usize) -> DiscoveredFullTable {
        DiscoveredFullTable {
            z_address: z,
            rows: 8,
            cols: 8,
            data_format: MapDataBits::Bits16(Endianness::BigEndian),
            semantic_type: kivi_scanner::zero_xdf_scan::SemanticMapType::UnknownTable,
            confidence_score: 0.8,
            smoothness: 0.2,
            x_axis: None,
            y_axis: None,
            sample_matrix: vec![vec![1.0; 8]; 8],
        }
    }
    let kept = apply_memory_interval_nms(vec![t(0x1000), t(0x8000)], 0.10);
    assert_eq!(kept.len(), 2, "tables at different addresses must both survive");
}
