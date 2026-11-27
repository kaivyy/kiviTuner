//! Compound-table splitting.
//!
//! ## What a compound table is
//!
//! Some definitions pack two calibrations into one declared rectangle: the
//! scanner sees `rows x cols` worth of cells, but the data actually holds two
//! maps side by side that share row breakpoints. A single rectangular read then
//! returns a surface that is smooth on each half but jumps at a column boundary.
//!
//! Ported from `openremap-core` (`split_compound_tables`, `try_split_compound`).
//!
//! ## The two gates
//!
//! A split at column `k` is accepted only when it is a real **value regime**
//! boundary, measured on the median row so one stray row cannot decide it:
//!
//! 1. **separation** — every value on one side is greater than every value on
//!    the other, or vice versa.
//! 2. **cliff** — the step across the boundary is at least twice the median
//!    step within the halves. A continuous ramp separates trivially but has no
//!    cliff, so it is not split.

use kivi_scanner::table_split::{find_split, split_row};

#[test]
fn a_clean_two_regime_table_splits_at_the_boundary() {
    // 4 rows x 12 cols. Columns 0..6 hold 1000..1300, columns 6..12 hold
    // 5000..5300 — separated by a large cliff.
    let mut grid = Vec::new();
    for r in 0..4u32 {
        let mut row = Vec::new();
        for c in 0..6u32 {
            row.push(1000 + r * 100 + c * 10);
        }
        for c in 0..6u32 {
            row.push(5000 + r * 100 + c * 10);
        }
        grid.push(row);
    }
    assert_eq!(find_split(&grid), Some(6), "split between the two regimes");
}

#[test]
fn a_continuous_ramp_is_not_split() {
    // A single smooth ramp: separated at every point, but no cliff.
    let mut grid = Vec::new();
    for _ in 0..4u32 {
        grid.push((0..12u32).map(|c| 1000 + c * 10).collect::<Vec<_>>());
    }
    assert_eq!(find_split(&grid), None, "no value-regime boundary exists");
}

#[test]
fn a_table_too_narrow_to_hold_two_maps_is_not_split() {
    let grid = vec![vec![1u32, 2, 3, 4]; 4];
    assert_eq!(find_split(&grid), None);
}

#[test]
fn splitting_yields_the_two_halves_in_order() {
    let row: Vec<u32> = (0..12).map(|c| if c < 6 { 1000 + c } else { 5000 + c }).collect();
    let (left, right) = split_row(&row, 6).expect("split");
    assert_eq!(left, vec![1000, 1001, 1002, 1003, 1004, 1005]);
    assert_eq!(right, vec![5006, 5007, 5008, 5009, 5010, 5011]);
}

#[test]
fn a_split_index_outside_the_row_is_refused() {
    let row = vec![1u32, 2, 3];
    assert!(split_row(&row, 0).is_none());
    assert!(split_row(&row, 3).is_none());
    assert!(split_row(&row, 10).is_none());
}
