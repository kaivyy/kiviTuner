//! Quality gate for candidate map shapes.
//!
//! ## Degenerate shape rejection rules
//!
//! To eliminate padded ROM regions and false positives, the quality gate ignores:
//!
//! - a grid that holds only **one** distinct value
//! - a grid that is **mostly** one value
//! - a grid of **two alternating** values
//!
//! Each check can be enabled individually alongside a bit-width filter (8/16/32).
//!
//! ## Why these matter
//!
//! A constant block passes every smoothness test trivially: zero gradient, zero
//! variance. Without a degeneracy gate the scanner reports every padded region as
//! a map. On a 2 MB ROM that is tens of thousands of useless candidates.
//!
//! ## Naming
//!
//! The vocabulary here is this project's own (`DegeneracyClass`,
//! `assess_grid_degeneracy`) rather than any vendor's term.

use kivi_scanner::map_quality::{assess_grid_degeneracy, DegeneracyClass, GridFilter, BitWidth};

fn grid(rows: usize, cols: usize, fill: impl Fn(usize, usize) -> f64) -> Vec<Vec<f64>> {
    (0..rows)
        .map(|r| (0..cols).map(|c| fill(r, c)).collect())
        .collect()
}

// ---------------------------------------------------------------------------
// Classification
// ---------------------------------------------------------------------------

#[test]
fn a_grid_of_one_value_is_singular() {
    let g = grid(8, 8, |_, _| 42.0);
    assert_eq!(assess_grid_degeneracy(&g), DegeneracyClass::Singular);
}

#[test]
fn a_grid_of_two_alternating_values_is_alternating() {
    let g = grid(8, 8, |r, c| if (r + c) % 2 == 0 { 10.0 } else { 20.0 });
    assert_eq!(assess_grid_degeneracy(&g), DegeneracyClass::Alternating);
}

#[test]
fn a_grid_of_two_non_alternating_values_is_two_valued() {
    // Two distinct values, but not in a checkerboard: half the grid is 10, the
    // other half is 20, arranged by row.
    let g = grid(8, 8, |r, _| if r < 4 { 10.0 } else { 20.0 });
    assert_eq!(assess_grid_degeneracy(&g), DegeneracyClass::TwoValued);
}

#[test]
fn a_mostly_one_value_grid_is_near_singular() {
    // Needs three or more distinct values, otherwise it classifies as
    // TwoValued first. 98 of 100 cells are 5; two stragglers make it three-valued
    // while one value still dominates above the 85% threshold.
    let g = grid(10, 10, |r, c| match (r, c) {
        (0, 0) => 99.0,
        (0, 1) => 77.0,
        _ => 5.0,
    });
    assert_eq!(assess_grid_degeneracy(&g), DegeneracyClass::NearSingular);
}

#[test]
fn a_real_gradient_is_healthy() {
    let g = grid(8, 8, |r, c| (r * 8 + c) as f64);
    assert_eq!(assess_grid_degeneracy(&g), DegeneracyClass::Healthy);
}

#[test]
fn an_empty_grid_is_healthy_rather_than_panicking() {
    // Nothing to judge; the caller's size checks handle emptiness.
    assert_eq!(assess_grid_degeneracy(&[]), DegeneracyClass::Healthy);
}

#[test]
fn a_single_cell_grid_is_singular() {
    assert_eq!(assess_grid_degeneracy(&[vec![7.0]]), DegeneracyClass::Singular);
}

// ---------------------------------------------------------------------------
// Filter policy
// ---------------------------------------------------------------------------

#[test]
fn the_default_filter_rejects_every_degenerate_class() {
    let f = GridFilter::default();
    assert!(f.rejects(DegeneracyClass::Singular));
    assert!(f.rejects(DegeneracyClass::NearSingular));
    assert!(f.rejects(DegeneracyClass::Alternating));
    assert!(f.rejects(DegeneracyClass::TwoValued));
}

#[test]
fn the_default_filter_accepts_a_healthy_grid() {
    assert!(!GridFilter::default().rejects(DegeneracyClass::Healthy));
}

#[test]
fn every_rejection_can_be_switched_off() {
    // A user may deliberately want a constant-valued map (a limiter plateau).
    let f = GridFilter {
        allow_singular: true,
        allow_near_singular: true,
        allow_alternating: true,
        allow_two_valued: true,
        ..GridFilter::default()
    };
    for c in [
        DegeneracyClass::Singular,
        DegeneracyClass::NearSingular,
        DegeneracyClass::Alternating,
        DegeneracyClass::TwoValued,
        DegeneracyClass::Healthy,
    ] {
        assert!(!f.rejects(c), "{c:?} should be allowed");
    }
}

// ---------------------------------------------------------------------------
// Bit-width filter
// ---------------------------------------------------------------------------

#[test]
fn the_bit_width_filter_accepts_only_the_selected_width() {
    let f = GridFilter {
        bit_width: Some(BitWidth::Bits16),
        ..GridFilter::default()
    };
    assert!(f.accepts_width(BitWidth::Bits16));
    assert!(!f.accepts_width(BitWidth::Bits8));
    assert!(!f.accepts_width(BitWidth::Bits32));
}

#[test]
fn no_bit_width_filter_accepts_every_width() {
    let f = GridFilter::default();
    for w in [BitWidth::Bits8, BitWidth::Bits16, BitWidth::Bits32] {
        assert!(f.accepts_width(w));
    }
}

#[test]
fn bit_width_reports_its_size_in_bytes() {
    assert_eq!(BitWidth::Bits8.bytes(), 1);
    assert_eq!(BitWidth::Bits16.bytes(), 2);
    assert_eq!(BitWidth::Bits32.bytes(), 4);
}

// ---------------------------------------------------------------------------
// Combined decision
// ---------------------------------------------------------------------------

#[test]
fn a_candidate_is_rejected_when_either_rule_fires() {
    let f = GridFilter::default();
    let flat = grid(4, 4, |_, _| 1.0);
    assert!(!f.admits(&flat, BitWidth::Bits16));

    let healthy = grid(4, 4, |r, c| (r * 4 + c) as f64);
    assert!(f.admits(&healthy, BitWidth::Bits16));

    // Healthy grid, wrong width.
    let only16 = GridFilter {
        bit_width: Some(BitWidth::Bits16),
        ..GridFilter::default()
    };
    assert!(!only16.admits(&healthy, BitWidth::Bits8));
}

/// The classifier must stay correct for large grids, where a quadratic
/// distinct-value count becomes the dominant cost.
///
/// ## Why this is a correctness test, not a speed test
///
/// `assess_grid_degeneracy` originally counted distinct values with
/// `values.iter().position(..)`, a linear scan inside the loop — O(cells x
/// distinct). For a 29x29 table that is ~350 000 comparisons per candidate, and
/// the scanner evaluates ~12 000 candidates per ROM. Rewriting it must not
/// change a single classification, so these cases pin the answer for grids big
/// enough that the old code was slow.
#[test]
fn a_large_healthy_grid_is_still_healthy() {
    let grid: Vec<Vec<f64>> = (0..29)
        .map(|r| (0..29).map(|c| (1000 + r * 37 + c * 11) as f64).collect())
        .collect();
    assert_eq!(
        assess_grid_degeneracy(&grid),
        DegeneracyClass::Healthy,
        "a grid with hundreds of distinct values is healthy"
    );
}

#[test]
fn a_large_singular_grid_is_still_singular() {
    let grid: Vec<Vec<f64>> = (0..29).map(|_| vec![0x40 as f64; 29]).collect();
    assert_eq!(assess_grid_degeneracy(&grid), DegeneracyClass::Singular);
}

#[test]
fn a_large_two_valued_grid_is_still_two_valued() {
    // Values chosen so the grid is not a clean checkerboard.
    let grid: Vec<Vec<f64>> = (0..29)
        .map(|r| (0..29).map(|c| if (r * 7 + c * 3) % 5 == 0 { 1.0 } else { 2.0 }).collect())
        .collect();
    assert_eq!(assess_grid_degeneracy(&grid), DegeneracyClass::TwoValued);
}

#[test]
fn a_large_checkerboard_is_still_alternating() {
    let grid: Vec<Vec<f64>> = (0..29)
        .map(|r| (0..29).map(|c| if (r + c) % 2 == 0 { 10.0 } else { 20.0 }).collect())
        .collect();
    assert_eq!(assess_grid_degeneracy(&grid), DegeneracyClass::Alternating);
}

#[test]
fn a_large_near_singular_grid_is_still_near_singular() {
    // 90% one value, the rest distinct: above the 0.85 threshold.
    let mut grid: Vec<Vec<f64>> = (0..29).map(|_| vec![500.0; 29]).collect();
    let mut n = 0;
    for row in grid.iter_mut() {
        for cell in row.iter_mut() {
            if n % 10 == 0 {
                *cell = (1000 + n) as f64;
            }
            n += 1;
        }
    }
    assert_eq!(
        assess_grid_degeneracy(&grid),
        DegeneracyClass::NearSingular
    );
}
