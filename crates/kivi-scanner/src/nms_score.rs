//! Composite quality score for Memory Interval NMS.
//!
//! ## Why a composite score
//!
//! NMS suppresses any candidate that overlaps an already-accepted table. Which
//! candidate is accepted first therefore decides what survives. Sorting by
//! `confidence_score` alone is wrong: a 12×12 fragment detected inside a real
//! 39×23 table can carry a higher confidence and would suppress the real table.
//!
//! The composite score weighs what actually distinguishes a real calibration
//! table from a fragment:
//!
//! ```text
//! Score = (axis_bonus × align_bonus × baseline_bonus × area) / (smoothness + ε)
//! ```
//!
//! - **area** in the numerator: a real table covers more bytes than a fragment.
//! - **smoothness** in the denominator: a calibration surface is smooth; code
//!   and padding are not.
//! - **axis_bonus**: a table with both axes paired, especially contiguously, is
//!   far more likely to be real than one guessed from monotonic runs.
//! - **align_bonus**: compilers and OEM tooling align calibration blocks.
//! - **baseline_bonus**: a flat first row is the signature of a limiter plateau.
//!
//! ## Provenance
//!
//! The formula and its constants are taken from the reference implementation in
//! `ecu_engine_rust/src/recognition/zero_xdf_discovery.rs`
//! (`ZeroXdfEngine::calculate_quality_score`). The constants are reproduced
//! exactly so a candidate ranking here matches that engine's ranking.

/// Everything the score needs, so callers do not have to build a whole table.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScoreInputs {
    pub rows: usize,
    pub cols: usize,
    pub z_address: usize,
    /// Dirichlet-style surface roughness; lower is smoother.
    pub smoothness: f64,
    /// 0.0..=1.0 semantic confidence.
    pub confidence_score: f64,
    pub has_x_axis: bool,
    pub has_y_axis: bool,
    /// `true` when the axis data sits immediately before the Z block, which is
    /// how OEM tooling lays a table out.
    pub axes_contiguous: bool,
    /// `true` when the first matrix row is constant — a limiter plateau.
    pub first_row_is_flat: bool,
}

/// Axis-pairing bonus.
///
/// 12.0 for an exact descriptor (contiguous axes, or confidence ≥ 0.95),
/// 2.0 for a blind monotonic guess with both axes, 1.5 for one axis, 1.0 for none.
fn axis_bonus(i: &ScoreInputs) -> f64 {
    match (i.has_x_axis, i.has_y_axis) {
        (true, true) => {
            if i.axes_contiguous || i.confidence_score >= 0.95 {
                12.0
            } else {
                2.0
            }
        }
        (true, false) | (false, true) => 1.5,
        (false, false) => 1.0,
    }
}

/// Address-alignment bonus.
fn align_bonus(z_address: usize) -> f64 {
    if z_address.is_multiple_of(16) {
        1.25
    } else if z_address.is_multiple_of(4) {
        1.1
    } else {
        1.0
    }
}

/// Baseline bonus for a flat first row.
fn baseline_bonus(first_row_is_flat: bool) -> f64 {
    if first_row_is_flat {
        1.5
    } else {
        1.0
    }
}

/// Compute the composite score.
///
/// Always finite: `epsilon` prevents division by zero, and an empty table scores
/// zero rather than `NaN`.
pub fn composite_quality_score(i: &ScoreInputs) -> f64 {
    const EPSILON: f64 = 0.05;
    let area = (i.rows * i.cols) as f64;
    let denominator = i.smoothness + EPSILON;
    if denominator <= 0.0 || !denominator.is_finite() {
        return 0.0;
    }
    let score = axis_bonus(i) * align_bonus(i.z_address) * baseline_bonus(i.first_row_is_flat) * area
        / denominator;
    if score.is_finite() {
        score
    } else {
        0.0
    }
}
