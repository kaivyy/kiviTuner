//! Heuristic quality scoring and semantic classification.
//!
//! Ported from `ecu_engine_rust::recognition::zero_xdf_discovery::ZeroXdfEngine`
//! (`calculate_quality_score`, `classify_semantics`).
//!
//! ## Scope
//!
//! The source functions take a `DiscoveredFullTable` — a Zero-XDF Z3 pipeline
//! type. Z3–Z5 are **not** migrated (blocked on fixture provenance). What is
//! migrated is the pure scoring and classification core, expressed over the
//! values those functions actually read. The Z3 container, its scanners and its
//! exporter are not present here and are not implied.
//!
//! ## Scoring formula (preserved exactly)
//!
//! ```text
//! score = (axis_bonus × align_bonus × baseline_bonus × area) / (smoothness + 0.05)
//! ```
//!
//! * `axis_bonus` — 12.0 when both axes are contiguous or confidence ≥ 0.95;
//!   2.0 when both axes exist but are not contiguous; 1.5 for one axis; 1.0 for none.
//! * `align_bonus` — 1.25 if `z_address % 16 == 0`, 1.1 if `% 4 == 0`, else 1.0.
//! * `baseline_bonus` — 1.5 when row 0 is flat (range < 1e-4), else 1.0.
//!
//! ## Deviation
//!
//! `classify_semantics` in the source indexes `matrix[0]` unconditionally and
//! would **panic** on an empty matrix. This version returns
//! [`SemanticType::Unknown`] instead. Panicking on empty input is not a contract.
//!
//! Classification is a *heuristic suggestion*, not a verified identification.
//! Synthetic fixtures exercise the rules; no accuracy claim is made for real ECU
//! images.

/// An axis reference used when scoring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AxisRef {
    /// Axis address.
    pub address: usize,
    /// Whether the axis stores 16-bit values.
    pub is_16bit: bool,
}

/// Inputs to the quality score.
#[derive(Debug, Clone, PartialEq)]
pub struct ScoreInputs {
    /// Row count.
    pub rows: usize,
    /// Column count.
    pub cols: usize,
    /// Table base address.
    pub z_address: usize,
    /// Detection confidence, `0.0..=1.0`.
    pub confidence: f64,
    /// Surface roughness of the sample matrix.
    pub smoothness: f64,
    /// X axis, when detected.
    pub x_axis: Option<AxisRef>,
    /// Y axis, when detected.
    pub y_axis: Option<AxisRef>,
    /// Whether the first sample row has effectively no variation.
    pub row0_is_flat: bool,
}

/// Composite quality score; higher is better.
///
/// Deterministic and panic-free, including for zero-area inputs.
pub fn quality_score(inputs: &ScoreInputs) -> f64 {
    let axis_bonus = match (&inputs.x_axis, &inputs.y_axis) {
        (Some(x), Some(y)) => {
            let x_size = inputs.cols * if x.is_16bit { 2 } else { 1 };
            let y_size = inputs.rows * if y.is_16bit { 2 } else { 1 };
            let is_contiguous = x.address + x_size == inputs.z_address
                && y.address + y_size == x.address;
            if is_contiguous || inputs.confidence >= 0.95 {
                12.0
            } else {
                2.0
            }
        }
        (Some(_), None) | (None, Some(_)) => 1.5,
        (None, None) => 1.0,
    };

    let align_bonus = if inputs.z_address.is_multiple_of(16) {
        1.25
    } else if inputs.z_address.is_multiple_of(4) {
        1.1
    } else {
        1.0
    };

    let baseline_bonus = if inputs.row0_is_flat { 1.5 } else { 1.0 };

    let area = (inputs.rows * inputs.cols) as f64;
    let epsilon = 0.05;
    (axis_bonus * align_bonus * baseline_bonus * area) / (inputs.smoothness + epsilon)
}

/// Suggested semantic type for a calibration table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticType {
    /// Retards under load, advances with RPM.
    IgnitionAdvance,
    /// Rises with load and RPM.
    FuelAirChargeVE,
    /// High plateau with a shallow gradient.
    BoostTorqueLimiter,
    /// Narrow mid-range plateau.
    TargetLambdaAFR,
    /// Fallback when no rule matches.
    SensorCalibration,
    /// Empty or unusable input.
    Unknown,
}

/// Classifies a sample matrix by its gradient and value range.
///
/// Returns [`SemanticType::Unknown`] for empty input or an empty first row.
/// This is a heuristic suggestion, not a verified identification.
pub fn classify_semantics(matrix: &[Vec<f64>]) -> SemanticType {
    let rows = matrix.len();
    if rows == 0 {
        return SemanticType::Unknown;
    }
    let cols = matrix[0].len();
    if cols == 0 {
        return SemanticType::Unknown;
    }

    // Gradient across columns and down rows. A single row or column has no
    // gradient in that direction; guard the divisor rather than dividing by zero.
    let mut sum_dx = 0.0;
    for row in matrix.iter() {
        for c in 0..cols.saturating_sub(1) {
            sum_dx += row[c + 1] - row[c];
        }
    }
    let dx_div = (rows * cols.saturating_sub(1)) as f64;
    let grad_x = if dx_div > 0.0 { sum_dx / dx_div } else { 0.0 };

    let mut sum_dy = 0.0;
    if rows > 1 {
        for pair in matrix.windows(2) {
            for (lower, upper) in pair[0].iter().zip(pair[1].iter()) {
                sum_dy += upper - lower;
            }
        }
    }
    let dy_div = (rows.saturating_sub(1) * cols) as f64;
    let grad_y = if dy_div > 0.0 { sum_dy / dy_div } else { 0.0 };

    let total_cells = (rows * cols) as f64;
    let mean: f64 = matrix.iter().flat_map(|r| r.iter()).sum::<f64>() / total_cells;
    let max_val = matrix
        .iter()
        .flat_map(|r| r.iter())
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    let min_val = matrix
        .iter()
        .flat_map(|r| r.iter())
        .copied()
        .fold(f64::INFINITY, f64::min);

    if grad_x < -0.1 && (grad_y > 0.0 || mean < 100.0) {
        SemanticType::IgnitionAdvance
    } else if (grad_x > 0.05 || grad_y > 0.05) && max_val > 500.0 {
        SemanticType::FuelAirChargeVE
    } else if max_val >= 1000.0 && min_val >= 0.0 && grad_x.abs() < 200.0 {
        SemanticType::BoostTorqueLimiter
    } else if max_val <= 255.0 && min_val >= 100.0 && (max_val - min_val) < 50.0 {
        SemanticType::TargetLambdaAFR
    } else {
        SemanticType::SensorCalibration
    }
}
