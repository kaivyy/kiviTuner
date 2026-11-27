//! Quality gate that rejects degenerate candidate grids.
//!
//! ## Why a gate is needed
//!
//! A block of one repeated value passes every smoothness test trivially: the
//! gradient is zero, the variance is zero, autocorrelation is undefined. Without
//! a degeneracy gate the scanner reports every padded region as a map, and on a
//! 2 MB ROM that is tens of thousands of useless candidates.
//!
//! ## Degeneracy rejection rules
//!
//! To eliminate padded regions and trivial false positives during blind discovery,
//! the quality gate filters out constant grids (single distinct value),
//! mostly-constant grids, and two-value alternating grids. Each rule can be
//! toggled independently, and an optional bit-width filter (8/16/32) narrows the search.
//!
//! ## What this module does not do
//!
//! It does not read bytes. It judges a decoded grid, so it is equally usable by
//! the 16-bit scanner, the 8-bit scanner, and a future 32-bit one.

/// How degenerate a candidate grid is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DegeneracyClass {
    /// Exactly one distinct value.
    Singular,
    /// Two distinct values in a checkerboard pattern.
    Alternating,
    /// Exactly two distinct values, arranged any other way.
    TwoValued,
    /// One value covers at least `NEAR_SINGULAR_FRACTION` of the cells.
    NearSingular,
    /// Enough distinct values with enough structure to be a real calibration.
    Healthy,
}

/// Share of identical cells at which a grid counts as near-singular.
pub const NEAR_SINGULAR_FRACTION: f64 = 0.85;

/// Element width, for the width filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitWidth {
    Bits8,
    Bits16,
    Bits32,
}

impl BitWidth {
    pub fn bytes(self) -> usize {
        match self {
            BitWidth::Bits8 => 1,
            BitWidth::Bits16 => 2,
            BitWidth::Bits32 => 4,
        }
    }
}

/// Which degeneracy classes to reject, and an optional width restriction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GridFilter {
    pub allow_singular: bool,
    pub allow_near_singular: bool,
    pub allow_alternating: bool,
    pub allow_two_valued: bool,
    /// `None` accepts every width.
    pub bit_width: Option<BitWidth>,
}

// `Default` is derived: every `allow_*` is `false` and `bit_width` is `None`,
// which is the safe default — a false candidate costs the user attention, a
// missed real map costs a tune.

impl GridFilter {
    /// Whether this class should be discarded.
    pub fn rejects(&self, class: DegeneracyClass) -> bool {
        match class {
            DegeneracyClass::Singular => !self.allow_singular,
            DegeneracyClass::NearSingular => !self.allow_near_singular,
            DegeneracyClass::Alternating => !self.allow_alternating,
            DegeneracyClass::TwoValued => !self.allow_two_valued,
            DegeneracyClass::Healthy => false,
        }
    }

    /// Whether a width passes the filter.
    pub fn accepts_width(&self, width: BitWidth) -> bool {
        match self.bit_width {
            None => true,
            Some(w) => w == width,
        }
    }

    /// Both rules together: the grid must be healthy enough *and* the width must
    /// be allowed.
    pub fn admits(&self, grid: &[Vec<f64>], width: BitWidth) -> bool {
        self.accepts_width(width) && !self.rejects(assess_grid_degeneracy(grid))
    }
}

/// Classify a decoded grid.
///
/// Comparison is exact on `f64`. Calibration cells are decoded from integers, so
/// exact equality is the right test — a tolerance would merge genuinely distinct
/// neighbouring cells into one bucket.
pub fn assess_grid_degeneracy(grid: &[Vec<f64>]) -> DegeneracyClass {
    let rows = grid.len();
    if rows == 0 {
        // Nothing to judge. Emptiness is the caller's problem, not degeneracy.
        return DegeneracyClass::Healthy;
    }
    let cols = grid[0].len();
    if cols == 0 {
        return DegeneracyClass::Healthy;
    }
    let total = rows * cols;

    // Count occurrences of each distinct value.
    //
    // A linear `position()` scan here is O(cells x distinct): for a 29x29 table
    // with hundreds of distinct values that is ~350 000 comparisons per
    // candidate, and the scanner evaluates ~12 000 candidates per ROM — measured
    // at 611 ms of a 1 766 ms scan. A hash map makes it O(cells).
    //
    // The classification only ever uses the number of distinct values and the
    // largest count, never their order, so this is an exact substitution.
    // `f64` is not `Hash`, so cells are keyed by their bit pattern; calibration
    // cells are decoded from integers and `-0.0`/`0.0` cannot both occur, so
    // bitwise equality matches the `==` comparison it replaces.
    let mut histogram: std::collections::HashMap<u64, usize> =
        std::collections::HashMap::with_capacity(total.min(4096));
    for row in grid.iter() {
        for &v in row.iter() {
            *histogram.entry(v.to_bits()).or_insert(0) += 1;
        }
    }
    let distinct = histogram.len();

    if distinct == 1 {
        return DegeneracyClass::Singular;
    }

    if distinct == 2 {
        // Checkerboard: the value alternates with (row + col) parity.
        let a = grid[0][0];
        let b = if cols > 1 { grid[0][1] } else { grid[1][0] };
        if a != b {
            let mut alternating = true;
            'outer: for (r, row) in grid.iter().enumerate() {
                for (c, &v) in row.iter().enumerate() {
                    let expected = if (r + c) % 2 == 0 { a } else { b };
                    if v != expected {
                        alternating = false;
                        break 'outer;
                    }
                }
            }
            if alternating {
                return DegeneracyClass::Alternating;
            }
        }
        return DegeneracyClass::TwoValued;
    }

    // Near-singular: one value dominates.
    let max_count = histogram.values().copied().max().unwrap_or(0);
    if (max_count as f64) / (total as f64) >= NEAR_SINGULAR_FRACTION {
        return DegeneracyClass::NearSingular;
    }

    DegeneracyClass::Healthy
}
