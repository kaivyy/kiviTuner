//! Compound-table splitting.
//!
//! ## What a compound table is
//!
//! Some definitions pack two calibrations into one declared rectangle: the
//! scanner sees `rows x cols` cells, but the data holds two maps side by side
//! that share row breakpoints. Reading it as one rectangle returns a surface that
//! is smooth on each half but jumps at a column boundary, and both real tables
//! come out with the wrong width.
//!
//! Ported from `openremap-core` (`split_compound_tables`, `try_split_compound`).
//!
//! ## The two gates
//!
//! A split at column `k` is accepted only when it is a real **value regime**
//! boundary, measured on the median row so a single stray row cannot decide it:
//!
//! 1. **separation** — one side's values are entirely above the other's.
//! 2. **cliff** — the step across the boundary is at least twice the median step
//!    within the halves.
//!
//! A continuous ramp satisfies separation at every point but has no cliff, so it
//! is never split. That distinction is what makes this safe to run on every
//! candidate.
//!
//! ## Limits
//!
//! Only a single split per table is considered, and only tables at least three
//! rows by six columns. More than two regimes, or a vertical split, are not
//! detected.

/// Smallest table that can hold two maps side by side.
const MIN_ROWS: usize = 3;
/// Minimum columns, and each half must keep at least this many.
const MIN_COLS: usize = 12;
/// Minimum width of each half.
const MIN_HALF: usize = 4;

/// Finds the column index where a table splits into two value regimes.
///
/// Returns `None` when no split passes both gates.
pub fn find_split(grid: &[Vec<u32>]) -> Option<usize> {
    let rows = grid.len();
    if rows < MIN_ROWS {
        return None;
    }
    let cols = grid.first().map(|r| r.len()).unwrap_or(0);
    if cols < MIN_COLS || grid.iter().any(|r| r.len() != cols) {
        return None;
    }
    if cols < MIN_HALF * 2 {
        return None;
    }

    let mut best: Option<(usize, f64)> = None;
    for k in MIN_HALF..=(cols - MIN_HALF) {
        let mut separations: Vec<i64> = Vec::with_capacity(rows);
        let mut crossings: Vec<i64> = Vec::with_capacity(rows);
        let mut within: Vec<i64> = Vec::with_capacity(rows * (cols - 1));

        for row in grid {
            let (mut lmin, mut lmax) = (u32::MAX, 0u32);
            let (mut rmin, mut rmax) = (u32::MAX, 0u32);
            for &v in &row[..k] {
                lmin = lmin.min(v);
                lmax = lmax.max(v);
            }
            for &v in &row[k..] {
                rmin = rmin.min(v);
                rmax = rmax.max(v);
            }
            // Positive when one side is entirely above the other.
            let sl = lmin as i64 - rmax as i64;
            let sr = rmin as i64 - lmax as i64;
            separations.push(sl.max(sr));
            crossings.push((row[k] as i64 - row[k - 1] as i64).abs());
            for w in row.windows(2) {
                within.push((w[1] as i64 - w[0] as i64).abs());
            }
        }

        separations.sort_unstable();
        crossings.sort_unstable();
        within.sort_unstable();
        let med_sep = separations[separations.len() / 2];
        let med_cross = crossings[crossings.len() / 2];
        let med_within = within[within.len() / 2].max(1);

        if med_sep <= 0 {
            continue;
        }
        if med_cross <= 2 * med_within {
            continue;
        }
        let ratio = med_cross as f64 / med_within as f64;
        if best.is_none_or(|(_, br)| ratio > br) {
            best = Some((k, ratio));
        }
    }
    best.map(|(k, _)| k)
}

/// Splits one row at `k`, returning the two halves.
///
/// `k` is the first index of the right half, so both halves are non-empty.
pub fn split_row(row: &[u32], k: usize) -> Option<(Vec<u32>, Vec<u32>)> {
    if k == 0 || k >= row.len() {
        return None;
    }
    Some((row[..k].to_vec(), row[k..].to_vec()))
}
