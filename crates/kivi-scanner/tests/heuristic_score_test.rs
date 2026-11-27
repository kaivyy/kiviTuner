//! Parity tests for heuristic quality scoring and semantic classification.
//!
//! Ported from `ecu_engine_rust::recognition::zero_xdf_discovery::ZeroXdfEngine`
//! (`calculate_quality_score`, `classify_semantics`).
//!
//! ## Scope note
//!
//! The source functions operate on `DiscoveredFullTable`, a Z3 pipeline type that
//! this program has **not** ported (Zero-XDF Z3–Z5 remain blocked on fixture
//! provenance). What is migrated here is the **pure scoring and classification
//! core**, expressed over the inputs those functions actually read:
//! dimensions, addresses, smoothness, confidence, sample matrix.
//!
//! The Z3 container, its scanning stages, and its export are **not** migrated and
//! are not implied by these tests.
//!
//! ## How expected values were obtained
//!
//! Every expectation is hand-derived from the source formulas:
//!
//! ```text
//! score = (axis_bonus × align_bonus × baseline_bonus × area) / (smoothness + 0.05)
//! axis_bonus     : both axes contiguous or conf ≥ 0.95 → 12.0
//!                  both axes present otherwise        →  2.0
//!                  one axis                           →  1.5
//!                  no axes                            →  1.0
//! align_bonus    : z_addr % 16 == 0 → 1.25 ; % 4 == 0 → 1.1 ; else 1.0
//! baseline_bonus : row 0 flat (range < 1e-4) → 1.5 ; else 1.0
//! ```
//!
//! None was produced by calling the implementation.

use kivi_scanner::heuristic_score::{
    classify_semantics, quality_score, AxisRef, ScoreInputs, SemanticType,
};

// ---------------------------------------------------------------------------
// quality_score — axis bonus
// ---------------------------------------------------------------------------

#[test]
fn no_axes_gives_the_base_bonus() {
    // axis 1.0 × align 1.0 × baseline 1.0 × area 4 / (0.95 + 0.05) = 4.0
    let inputs = ScoreInputs {
        rows: 2,
        cols: 2,
        z_address: 1, // not 16- or 4-aligned
        confidence: 0.5,
        smoothness: 0.95,
        x_axis: None,
        y_axis: None,
        row0_is_flat: false,
    };
    assert_eq!(quality_score(&inputs), 4.0);
}

#[test]
fn one_axis_gives_a_1_5_bonus() {
    // 1.5 × 1.0 × 1.0 × 4 / 1.0 = 6.0
    let inputs = ScoreInputs {
        rows: 2,
        cols: 2,
        z_address: 1,
        confidence: 0.5,
        smoothness: 0.95,
        x_axis: Some(AxisRef { address: 0x100, is_16bit: false }),
        y_axis: None,
        row0_is_flat: false,
    };
    assert_eq!(quality_score(&inputs), 6.0);
}

#[test]
fn both_axes_without_contiguity_give_a_2_0_bonus() {
    // 2.0 × 1.25 × 1.0 × 4 / 1.0 = 10.0   (0x5000 is 16-byte aligned)
    let inputs = ScoreInputs {
        rows: 2,
        cols: 2,
        z_address: 0x5000,
        confidence: 0.5,
        smoothness: 0.95,
        x_axis: Some(AxisRef { address: 0x100, is_16bit: false }),
        y_axis: Some(AxisRef { address: 0x200, is_16bit: false }),
        row0_is_flat: false,
    };
    assert_eq!(quality_score(&inputs), 10.0);
}

#[test]
fn contiguous_axes_give_the_full_12_0_bonus() {
    // Layout: y at 0x100 (rows 2, 8-bit -> ends 0x102), x at 0x102
    // (cols 2, 8-bit -> ends 0x104), z at 0x104.
    // contiguity check: x.address + x_size == z && y.address + y_size == x.address
    let inputs = ScoreInputs {
        rows: 2,
        cols: 2,
        z_address: 0x104,
        confidence: 0.5,
        smoothness: 0.95,
        x_axis: Some(AxisRef { address: 0x102, is_16bit: false }),
        y_axis: Some(AxisRef { address: 0x100, is_16bit: false }),
        row0_is_flat: false,
    };
    // 12.0 × 1.0 × 1.0 × 4 / 1.0 = 48.0  (0x104 is 4-aligned -> 1.1, not 1.0)
    // 12.0 × 1.1 × 1.0 × 4 / 1.0 = 52.8
    assert!((quality_score(&inputs) - 52.8).abs() < 1e-9);
}

#[test]
fn high_confidence_alone_earns_the_full_axis_bonus() {
    let inputs = ScoreInputs {
        rows: 2,
        cols: 2,
        z_address: 0x5000,
        confidence: 0.95,
        smoothness: 0.95,
        x_axis: Some(AxisRef { address: 0x100, is_16bit: false }),
        y_axis: Some(AxisRef { address: 0x200, is_16bit: false }),
        row0_is_flat: false,
    };
    // 12.0 × 1.25 × 1.0 × 4 / 1.0 = 60.0   (0x5000 is 16-aligned)
    assert_eq!(quality_score(&inputs), 60.0);
}

// ---------------------------------------------------------------------------
// quality_score — alignment bonus
// ---------------------------------------------------------------------------

#[test]
fn sixteen_byte_alignment_gives_1_25() {
    // 1.0 × 1.25 × 1.0 × 4 / 1.0 = 5.0
    let inputs = ScoreInputs {
        rows: 2,
        cols: 2,
        z_address: 0x1000, // % 16 == 0
        confidence: 0.5,
        smoothness: 0.95,
        x_axis: None,
        y_axis: None,
        row0_is_flat: false,
    };
    assert_eq!(quality_score(&inputs), 5.0);
}

#[test]
fn four_byte_alignment_gives_1_1() {
    // 1.0 × 1.1 × 1.0 × 4 / 1.0 = 4.4
    let inputs = ScoreInputs {
        rows: 2,
        cols: 2,
        z_address: 0x1004, // % 16 != 0, % 4 == 0
        confidence: 0.5,
        smoothness: 0.95,
        x_axis: None,
        y_axis: None,
        row0_is_flat: false,
    };
    assert!((quality_score(&inputs) - 4.4).abs() < 1e-9);
}

// ---------------------------------------------------------------------------
// quality_score — baseline bonus and smoothness
// ---------------------------------------------------------------------------

#[test]
fn a_flat_first_row_gives_the_1_5_baseline_bonus() {
    // 1.0 × 1.0 × 1.5 × 4 / 1.0 = 6.0
    let inputs = ScoreInputs {
        rows: 2,
        cols: 2,
        z_address: 1,
        confidence: 0.5,
        smoothness: 0.95,
        x_axis: None,
        y_axis: None,
        row0_is_flat: true,
    };
    assert_eq!(quality_score(&inputs), 6.0);
}

#[test]
fn a_smoother_table_scores_higher() {
    let base = ScoreInputs {
        rows: 4,
        cols: 4,
        z_address: 0x1000,
        confidence: 0.5,
        smoothness: 0.5,
        x_axis: None,
        y_axis: None,
        row0_is_flat: false,
    };
    let smoother = ScoreInputs { smoothness: 0.1, ..base.clone() };
    assert!(
        quality_score(&smoother) > quality_score(&base),
        "lower smoothness must score higher"
    );
}

#[test]
fn a_larger_area_scores_higher() {
    let small = ScoreInputs {
        rows: 2,
        cols: 2,
        z_address: 0x1000,
        confidence: 0.5,
        smoothness: 0.5,
        x_axis: None,
        y_axis: None,
        row0_is_flat: false,
    };
    let large = ScoreInputs { rows: 8, cols: 8, ..small.clone() };
    assert!(quality_score(&large) > quality_score(&small));
}

#[test]
fn scoring_is_deterministic() {
    let inputs = ScoreInputs {
        rows: 3,
        cols: 5,
        z_address: 0x2000,
        confidence: 0.7,
        smoothness: 0.33,
        x_axis: Some(AxisRef { address: 0x100, is_16bit: true }),
        y_axis: None,
        row0_is_flat: true,
    };
    assert_eq!(quality_score(&inputs), quality_score(&inputs));
}

#[test]
fn degenerate_dimensions_do_not_panic() {
    let inputs = ScoreInputs {
        rows: 0,
        cols: 0,
        z_address: 0,
        confidence: 0.0,
        smoothness: 0.0,
        x_axis: None,
        y_axis: None,
        row0_is_flat: false,
    };
    // area 0 -> score 0, and the epsilon prevents division by zero.
    assert_eq!(quality_score(&inputs), 0.0);
}

// ---------------------------------------------------------------------------
// classify_semantics
// ---------------------------------------------------------------------------

#[test]
fn an_empty_matrix_classifies_as_unknown_without_panicking() {
    // Deviation: the source indexes matrix[0] and would panic.
    assert_eq!(classify_semantics(&[]), SemanticType::Unknown);
}

#[test]
fn a_matrix_with_an_empty_first_row_is_unknown() {
    assert_eq!(classify_semantics(&[vec![]]), SemanticType::Unknown);
}

#[test]
fn a_descending_row_gradient_with_positive_y_gradient_is_ignition() {
    // grad_x < -0.1 and grad_y > 0.0
    // rows: 0,1,2 down the column; cols: 10,8,6 across
    let m = vec![
        vec![100.0, 90.0, 80.0],
        vec![101.0, 91.0, 81.0],
        vec![102.0, 92.0, 82.0],
    ];
    assert_eq!(classify_semantics(&m), SemanticType::IgnitionAdvance);
}

#[test]
fn a_rising_matrix_with_large_values_is_fuel() {
    // grad_x > 0.05 and max > 500
    let m = vec![
        vec![600.0, 700.0, 800.0],
        vec![610.0, 710.0, 810.0],
        vec![620.0, 720.0, 820.0],
    ];
    assert_eq!(classify_semantics(&m), SemanticType::FuelAirChargeVE);
}

#[test]
fn a_high_plateau_with_small_gradient_is_boost() {
    // max >= 1000, min >= 0, |grad_x| < 200, and no rising gradient —
    // a rising gradient would trip the Fuel rule first.
    let m = vec![
        vec![1200.0, 1200.0, 1200.0],
        vec![1200.0, 1200.0, 1200.0],
        vec![1200.0, 1200.0, 1200.0],
    ];
    assert_eq!(classify_semantics(&m), SemanticType::BoostTorqueLimiter);
}

#[test]
fn a_narrow_mid_range_matrix_is_lambda() {
    // max <= 255, min >= 100, (max - min) < 50
    let m = vec![
        vec![120.0, 121.0, 122.0],
        vec![121.0, 122.0, 123.0],
        vec![122.0, 123.0, 124.0],
    ];
    assert_eq!(classify_semantics(&m), SemanticType::TargetLambdaAFR);
}

#[test]
fn an_unmatched_matrix_falls_back_to_sensor_calibration() {
    // Low values, flat: no rule matches.
    let m = vec![
        vec![10.0, 11.0, 12.0],
        vec![10.0, 11.0, 12.0],
        vec![10.0, 11.0, 12.0],
    ];
    assert_eq!(classify_semantics(&m), SemanticType::SensorCalibration);
}

#[test]
fn classification_is_deterministic() {
    let m = vec![
        vec![600.0, 700.0],
        vec![610.0, 710.0],
    ];
    assert_eq!(classify_semantics(&m), classify_semantics(&m));
}

#[test]
fn a_single_row_matrix_is_handled_without_panicking() {
    // rows == 1 means grad_y divides by zero; the implementation must guard it.
    let m = vec![vec![100.0, 90.0, 80.0]];
    let _ = classify_semantics(&m);
}

#[test]
fn a_single_column_matrix_is_handled_without_panicking() {
    let m = vec![vec![100.0], vec![90.0], vec![80.0]];
    let _ = classify_semantics(&m);
}
