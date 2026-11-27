//! Matrix smoothing, Gaussian convolution, Laplacian curvature, and row reduction.
//!
//! Ported from `ecu_engine_rust::math::smoothing` (source of truth). All public
//! functions are **pure**: they take `&[Vec<f64>]` and return a new matrix or a
//! result struct. Nothing mutates its input.
//!
//! Behavior is preserved exactly, including these source quirks:
//!
//! * `spatial_smooth_2d` accepts `alpha = NaN` (both `<= 0.0` and `>= 1.0`
//!   comparisons are false for NaN), producing an all-NaN matrix.
//! * `find_safest_row_to_remove` returns `best_row_index = 1` with infinite
//!   errors when every candidate span is degenerate.
//! * Output width is taken from `matrix[0].len()`, so ragged matrices silently
//!   ignore longer later rows.
//! * `gaussian_smooth_3x3` clamps `sigma` to at least `0.1` and copies border
//!   cells unchanged.
//!
//! ## Not ported
//!
//! `spatial_matrix_smooth_2d` is a one-line deprecated alias of
//! `spatial_smooth_2d`; it is omitted rather than duplicated.
//!
//! ## Safety
//!
//! These functions are analysis-only. They never touch `MemoryMap`, map
//! definitions, or any calibration state, and no caller writes their result
//! back automatically.

use thiserror::Error;

/// Error produced by the smoothing and reduction functions.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SmoothError {
    /// `alpha` must lie strictly inside `(0, 1)`.
    #[error("Alpha smoothing factor must be strictly between 0.0 and 1.0")]
    InvalidAlpha,
    /// Row reduction needs at least three rows.
    #[error("Matrix must have at least 3 rows to evaluate row reduction")]
    TooFewRows,
    /// The axis length must equal the matrix row count.
    #[error("Axis values length must match matrix row count")]
    AxisLengthMismatch,
}

/// 2D spatial blending filter.
///
/// Blends each cell toward the mean of its immediate 4-connected neighbours:
///
/// `Z_new[r,c] = (1 - alpha) * Z[r,c] + alpha * mean(neighbours)`
///
/// Cells with no neighbours (a 1x1 matrix) are left unchanged. Borders use only
/// the neighbours that exist.
///
/// `alpha` must be strictly between 0 and 1. **Note:** `alpha = NaN` passes the
/// range check (source behavior) and yields an all-NaN matrix.
pub fn spatial_smooth_2d(matrix: &[Vec<f64>], alpha: f64) -> Result<Vec<Vec<f64>>, SmoothError> {
    if alpha <= 0.0 || alpha >= 1.0 {
        return Err(SmoothError::InvalidAlpha);
    }
    let rows = matrix.len();
    if rows == 0 {
        return Ok(Vec::new());
    }
    let cols = matrix[0].len();
    if cols == 0 {
        return Ok(matrix.to_vec());
    }
    if matrix.iter().any(|r| r.len() < cols) {
        return Err(SmoothError::InvalidAlpha);
    }
    let mut output = matrix.to_vec();

    for r in 0..rows {
        for c in 0..cols {
            let mut sum_neighbor = 0.0;
            let mut count_neighbor = 0;

            if r > 0 {
                sum_neighbor += matrix[r - 1][c];
                count_neighbor += 1;
            }
            if r + 1 < rows {
                sum_neighbor += matrix[r + 1][c];
                count_neighbor += 1;
            }
            if c > 0 {
                sum_neighbor += matrix[r][c - 1];
                count_neighbor += 1;
            }
            if c + 1 < cols {
                sum_neighbor += matrix[r][c + 1];
                count_neighbor += 1;
            }

            if count_neighbor > 0 {
                let avg_neighbor = sum_neighbor / count_neighbor as f64;
                output[r][c] = (1.0 - alpha) * matrix[r][c] + alpha * avg_neighbor;
            }
        }
    }
    Ok(output)
}

/// 3x3 Gaussian convolution.
///
/// Builds a normalised discrete Gaussian kernel with `sigma` (clamped to at
/// least `0.1`) and convolves the interior. Border cells are copied unchanged.
/// Matrices smaller than 3x3 are returned as-is.
pub fn gaussian_smooth_3x3(matrix: &[Vec<f64>], sigma: f64) -> Vec<Vec<f64>> {
    let rows = matrix.len();
    if rows < 3 {
        return matrix.to_vec();
    }
    let cols = matrix[0].len();
    if cols < 3 || matrix.iter().any(|r| r.len() < cols) {
        return matrix.to_vec();
    }

    let s = sigma.max(0.1);
    let two_s2 = 2.0 * s * s;
    let mut kernel = [[0.0; 3]; 3];
    let mut k_sum = 0.0;

    for i in -1..=1 {
        for j in -1..=1 {
            let r2 = (i * i + j * j) as f64;
            let val = (-r2 / two_s2).exp();
            kernel[(i + 1) as usize][(j + 1) as usize] = val;
            k_sum += val;
        }
    }
    for row in &mut kernel {
        for val in row {
            *val /= k_sum;
        }
    }

    let mut output = matrix.to_vec();
    for r in 1..rows - 1 {
        for c in 1..cols - 1 {
            let mut acc = 0.0;
            for i in -1..=1 {
                for j in -1..=1 {
                    acc += matrix[(r as isize + i) as usize][(c as isize + j) as usize]
                        * kernel[(i + 1) as usize][(j + 1) as usize];
                }
            }
            output[r][c] = acc;
        }
    }
    output
}

/// Discrete 2D Laplacian (surface curvature).
///
/// `Lap[r,c] = Z[r+1,c] + Z[r-1,c] + Z[r,c+1] + Z[r,c-1] - 4 * Z[r,c]`
///
/// A large magnitude marks a spike or discontinuity. Border cells are zero.
/// Matrices with fewer than three rows return an all-zero matrix of the same
/// shape. Output width follows `matrix[0].len()`.
pub fn laplacian_curvature_2d(matrix: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let rows = matrix.len();
    if rows < 3 {
        return vec![vec![0.0; matrix.first().map_or(0, |r| r.len())]; rows];
    }
    let cols = matrix[0].len();
    if cols < 3 || matrix.iter().any(|r| r.len() < cols) {
        return vec![vec![0.0; cols]; rows];
    }
    let mut output = vec![vec![0.0; cols]; rows];

    for r in 1..rows - 1 {
        for c in 1..cols - 1 {
            let val = matrix[r + 1][c] + matrix[r - 1][c] + matrix[r][c + 1] + matrix[r][c - 1]
                - 4.0 * matrix[r][c];
            output[r][c] = val;
        }
    }
    output
}

/// Outcome of evaluating which interior row is safest to delete.
#[derive(Debug, Clone, PartialEq)]
pub struct RowReductionResult {
    /// Index of the interior row with the smallest maximum interpolation error.
    pub best_row_index: usize,
    /// Largest per-cell error for that row.
    pub min_max_error: f64,
    /// Mean per-cell error for that row.
    pub min_avg_error: f64,
}

/// Finds the interior row whose removal introduces the least error.
///
/// For each interior row `r`, the row is predicted by linear interpolation
/// between rows `r-1` and `r+1` using the axis positions, and the error is
/// compared against the actual row. The row with the smallest maximum error
/// wins.
///
/// Requires at least three rows and `axis_values.len() == matrix.len()`.
///
/// **Source quirk:** if every candidate span is degenerate (`|x_next - x_prev|`
/// below `1e-12`), no row is evaluated and the function returns
/// `best_row_index = 1` with infinite errors.
///
/// Unlike `interp::search_interval`, this uses the axis only to compute a ratio,
/// so a descending axis is supported.
pub fn find_safest_row_to_remove(
    matrix: &[Vec<f64>],
    axis_values: &[f64],
) -> Result<RowReductionResult, SmoothError> {
    let rows = matrix.len();
    if rows < 3 {
        return Err(SmoothError::TooFewRows);
    }
    if axis_values.len() != rows {
        return Err(SmoothError::AxisLengthMismatch);
    }
    let cols = matrix[0].len();

    let mut best_row = 1usize;
    let mut lowest_max_err = f64::INFINITY;
    let mut lowest_avg_err = f64::INFINITY;

    for r in 1..rows - 1 {
        let x_prev = axis_values[r - 1];
        let x_curr = axis_values[r];
        let x_next = axis_values[r + 1];

        let span = x_next - x_prev;
        if span.abs() < 1e-12 {
            continue;
        }
        let ratio = (x_curr - x_prev) / span;

        let mut row_max_err = 0.0f64;
        let mut row_err_sum = 0.0f64;

        // Iterate the three rows by zipped slices so the accumulation order
        // matches the source exactly (left to right over `cols`).
        let prev_row = &matrix[r - 1][..cols];
        let curr_row = &matrix[r][..cols];
        let next_row = &matrix[r + 1][..cols];

        for ((&p, &n), &cur) in prev_row.iter().zip(next_row.iter()).zip(curr_row.iter()) {
            let interp_val = p + ratio * (n - p);
            let err = (interp_val - cur).abs();
            row_err_sum += err;
            if err > row_max_err {
                row_max_err = err;
            }
        }

        let row_avg_err = row_err_sum / cols as f64;

        if row_max_err < lowest_max_err {
            lowest_max_err = row_max_err;
            lowest_avg_err = row_avg_err;
            best_row = r;
        }
    }

    Ok(RowReductionResult {
        best_row_index: best_row,
        min_max_error: lowest_max_err,
        min_avg_error: lowest_avg_err,
    })
}
