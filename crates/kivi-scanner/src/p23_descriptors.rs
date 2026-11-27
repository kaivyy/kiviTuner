//! Wave 3.3 P2.3: Finite-difference surface descriptors (`regularity` family).
//!
//! Strictly implements the disambiguated operators from
//! `research/wave-3.3/WAVE-3.3-P2.3-SYNTHETIC-DESIGN-SPEC.md`:
//! - `canonical_smoothness`: matches [`crate::scan_metrics::smoothness`] (no factor of 2).
//! - `grad_*`: normalized `L2` gradient magnitude `||(Δx, Δy)||`.
//! - `laplacian_energy`: 5-point discrete Laplacian curvature energy.
//! - `delta_xy_ratio`: mixed second difference `Δxy` — **descriptor only, never a gate**.

use crate::scan_metrics::smoothness as canonical_smoothness_fn;
use serde::{Deserialize, Serialize};

/// Regularity and separability descriptors for a 2-D matrix (`rows >= 2, cols >= 2`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct P23Descriptors {
    /// `(mean|Δx| + mean|Δy|) / (σ + 1e-5)` — identical to `scan_metrics::smoothness`.
    pub canonical_smoothness: f64,
    /// Mean normalized gradient magnitude `||(Δx, Δy)||_2 / (σ + 1e-5)`.
    pub grad_mean: f64,
    /// 95th-percentile normalized gradient magnitude.
    pub grad_p95: f64,
    /// Max normalized gradient magnitude.
    pub grad_max: f64,
    /// Normalized 5-point discrete Laplacian curvature (`0.0` when `< 3x3` or affine plane).
    pub laplacian_energy: f64,
    /// Normalized mixed second difference `|Δxy| / (σ + 1e-5)`. Descriptor only, never a gate.
    pub delta_xy_ratio: f64,
}

/// Computes all P2.3 finite-difference descriptors on a rectangular 2-D matrix.
///
/// Returns `None` if `rows < 2`, `cols < 2`, or any row length mismatches `cols`.
pub fn compute_p23_descriptors(grid: &[Vec<f64>]) -> Option<P23Descriptors> {
    let rows = grid.len();
    if rows < 2 {
        return None;
    }
    let cols = grid[0].len();
    if cols < 2 || grid.iter().any(|r| r.len() != cols) {
        return None;
    }

    let n = (rows * cols) as f64;
    let sum: f64 = grid.iter().flat_map(|r| r.iter()).sum();
    let mean = sum / n;
    let var: f64 = grid
        .iter()
        .flat_map(|r| r.iter())
        .map(|v| (v - mean) * (v - mean))
        .sum::<f64>()
        / n;
    let denom = var.sqrt() + 1e-5;

    let canonical_smoothness = canonical_smoothness_fn(grid);

    // Gradient magnitude ||(Δx, Δy)||_2 on overlapping pairs [0..rows-1] x [0..cols-1].
    let mut grads = Vec::with_capacity((rows - 1) * (cols - 1));
    let mut dxy_sum = 0.0;
    for r in 0..rows - 1 {
        for c in 0..cols - 1 {
            let dx = grid[r][c + 1] - grid[r][c];
            let dy = grid[r + 1][c] - grid[r][c];
            grads.push((dx * dx + dy * dy).sqrt() / denom);
            let dxy = (grid[r + 1][c + 1] - grid[r + 1][c]) - (grid[r][c + 1] - grid[r][c]);
            dxy_sum += dxy.abs();
        }
    }
    let pair_count = grads.len() as f64;
    let grad_mean = grads.iter().sum::<f64>() / pair_count;
    let delta_xy_ratio = (dxy_sum / pair_count) / denom;

    grads.sort_by(|a, b| a.total_cmp(b));
    let grad_max = *grads.last().unwrap_or(&0.0);
    let p95_idx = ((grads.len() as f64 * 0.95).ceil() as usize)
        .saturating_sub(1)
        .min(grads.len() - 1);
    let grad_p95 = grads[p95_idx];

    // 5-point discrete Laplacian L[r,c] = M[r,c+1] + M[r,c-1] + M[r+1,c] + M[r-1,c] - 4*M[r,c].
    let laplacian_energy = if rows >= 3 && cols >= 3 {
        let mut lap_sum = 0.0;
        let interior = ((rows - 2) * (cols - 2)) as f64;
        for r in 1..rows - 1 {
            for c in 1..cols - 1 {
                let lap = grid[r][c + 1]
                    + grid[r][c - 1]
                    + grid[r + 1][c]
                    + grid[r - 1][c]
                    - 4.0 * grid[r][c];
                lap_sum += lap.abs();
            }
        }
        (lap_sum / interior) / denom
    } else {
        0.0
    };

    Some(P23Descriptors {
        canonical_smoothness,
        grad_mean,
        grad_p95,
        grad_max,
        laplacian_energy,
        delta_xy_ratio,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_smoothness_matches_scan_metrics_without_factor_of_two() {
        let grid: Vec<Vec<f64>> = (0..8)
            .map(|r| (0..8).map(|c| 100.0 + (r * 30 + c * 15) as f64).collect())
            .collect();
        let d = compute_p23_descriptors(&grid).expect("descriptors");
        assert!((d.canonical_smoothness - crate::scan_metrics::smoothness(&grid)).abs() < 1e-12);
        // Planar additive surface has zero discrete Laplacian and zero Δxy.
        assert!(d.laplacian_energy < 1e-9);
        assert!(d.delta_xy_ratio < 1e-9);
    }

    #[test]
    fn delta_xy_distinguishes_additive_vs_multiplicative_as_descriptor() {
        let f = [3.0, 7.0, 11.0, 19.0, 23.0];
        let g = [100.0, 140.0, 190.0, 250.0, 320.0];
        let additive: Vec<Vec<f64>> = f
            .iter()
            .map(|&fi| g.iter().map(|&gj| fi + gj).collect())
            .collect();
        let multiplicative: Vec<Vec<f64>> = f
            .iter()
            .map(|&fi| g.iter().map(|&gj| fi * gj).collect())
            .collect();

        let da = compute_p23_descriptors(&additive).unwrap();
        let dm = compute_p23_descriptors(&multiplicative).unwrap();
        assert!(da.delta_xy_ratio < 1e-9);
        assert!(dm.delta_xy_ratio > 0.01);
        // Multiplicative real table still has finite bounded canonical smoothness
        // (even though Δxy != 0 and no-factor-of-2 doubles the sum vs the mis-transcribed formula).
        assert!(dm.canonical_smoothness.is_finite() && dm.canonical_smoothness < 1.2);
    }
}
