//! Interpolation primitives for calibration tables and axis lookups.
//!
//! Ported from `ecu_engine_rust::math::interpolation` (source of truth).
//! Behavior is preserved exactly, including the source's quirks:
//!
//! * **Clamping, not extrapolation.** `bilinear_2d` clamps normalized `u`/`v`
//!   into `[0,1]`; `AxisLookup` clamps the target to the axis boundary. This
//!   differs from `kivi_protocol::correlation::resolver`, which *rejects*
//!   out-of-range coordinates — the two are intentionally distinct.
//! * **Degenerate spans.** When `x1 - x0` is within `1e-12`, `u` becomes `0.0`
//!   (no division). `search_interval` does the same for the interval span.
//! * **Duplicate breakpoints are accepted.** The source does not validate
//!   monotonicity; a duplicate produces a zero span and `t = 0.0`.
//! * **NaN target.** `search_interval` returns `Ok` with `t = NaN` because the
//!   comparisons fall through. Preserved verbatim, not "fixed".
//! * **Catmull-Rom tangents** come from the surrounding points `p_prev`/`p_next`;
//!   `t = 0` returns `p0` and `t = 1` returns `p1` exactly.
//! * **Bicubic** uses Keys' cubic convolution with `a = -0.5`, a row-major 4×4
//!   patch, and a normalized weight sum per pass (so a constant patch returns
//!   that constant). The evaluated cell is between rows 1..2 and cols 1..2.
//!
//! All functions are pure: no I/O, no mutation, deterministic.
//!
//! **Not ported** from the source file: `interpolate_matrix_region_bilinear`
//! (mutates a matrix in place) and `MapComparisonMath` (mutating batch
//! operations). Those are mutation helpers, out of scope for analysis-only
//! interpolation.

use thiserror::Error;

/// Errors produced by the axis lookup functions.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum InterpError {
    /// The axis has fewer than two breakpoints.
    #[error("Axis must contain at least 2 breakpoints")]
    AxisTooShort,
    /// The axis and value arrays have different lengths.
    #[error("Axis and values arrays must have equal length")]
    AxisValuesLengthMismatch,
    /// The map row count does not match the Y axis length.
    #[error("Map rows must match Y axis length")]
    MapRowsMismatch,
    /// The map column count does not match the X axis length.
    #[error("Map columns must match X axis length")]
    MapColsMismatch,
}

/// Result of locating a target value within an axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AxisInterval {
    /// Index of the lower breakpoint.
    pub low: usize,
    /// Index of the upper breakpoint (`low + 1`).
    pub high: usize,
    /// Normalized position between the breakpoints, clamped to `[0,1]`.
    pub t: f64,
}

/// Epsilon used by the source to treat a span as degenerate.
const EPSILON: f64 = 1e-12;

/// 1D linear interpolation between `(x0, y0)` and `(x1, y1)`.
///
/// If `x1 - x0` is within `1e-12`, returns `y0` without dividing.
/// Does **not** clamp `x`; values outside the segment extrapolate linearly.
#[inline]
pub fn lerp_1d(x0: f64, y0: f64, x1: f64, y1: f64, x: f64) -> f64 {
    if (x1 - x0).abs() < EPSILON {
        return y0;
    }
    let t = (x - x0) / (x1 - x0);
    y0 + t * (y1 - y0)
}

/// 1D Catmull-Rom cubic spline interpolation.
///
/// Evaluates a cubic passing through `p0` and `p1` with tangents derived from
/// `p_prev` and `p_next`. `t` is normalized progress from `p0` to `p1`.
#[inline]
pub fn catmull_rom_cubic_1d(p_prev: f64, p0: f64, p1: f64, p_next: f64, t: f64) -> f64 {
    let t2 = t * t;
    let t3 = t2 * t;

    0.5 * ((2.0 * p0)
        + (-p_prev + p1) * t
        + (2.0 * p_prev - 5.0 * p0 + 4.0 * p1 - p_next) * t2
        + (-p_prev + 3.0 * p0 - 3.0 * p1 + p_next) * t3)
}

/// 2D bilinear interpolation within a grid cell.
///
/// Corner layout:
/// `(x0, y0) -> q00`, `(x1, y0) -> q10`, `(x0, y1) -> q01`, `(x1, y1) -> q11`.
///
/// `u` and `v` are clamped into `[0,1]`, so out-of-range coordinates saturate
/// at the cell edges rather than extrapolating.
///
/// The ten-argument signature is inherited verbatim from the source engine;
/// collapsing it into a struct would change the ported API for no benefit and
/// make oracle comparison harder to read.
#[allow(clippy::too_many_arguments)]
pub fn bilinear_2d(
    x0: f64,
    x1: f64,
    y0: f64,
    y1: f64,
    q00: f64,
    q10: f64,
    q01: f64,
    q11: f64,
    x: f64,
    y: f64,
) -> f64 {
    let u = if (x1 - x0).abs() > EPSILON {
        ((x - x0) / (x1 - x0)).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let v = if (y1 - y0).abs() > EPSILON {
        ((y - y0) / (y1 - y0)).clamp(0.0, 1.0)
    } else {
        0.0
    };

    (1.0 - u) * (1.0 - v) * q00 + u * (1.0 - v) * q10 + (1.0 - u) * v * q01 + u * v * q11
}

/// Keys' cubic convolution weight with `a = -0.5`.
#[inline]
fn cubic_weight(x: f64) -> f64 {
    let abs_x = x.abs();
    if abs_x <= 1.0 {
        1.5 * abs_x.powi(3) - 2.5 * abs_x.powi(2) + 1.0
    } else if abs_x < 2.0 {
        -0.5 * abs_x.powi(3) + 2.5 * abs_x.powi(2) - 4.0 * abs_x + 2.0
    } else {
        0.0
    }
}

/// 2D 16-point bicubic surface interpolation.
///
/// `patch_16` is a row-major 4×4 neighbourhood. The evaluated cell lies between
/// rows 1..2 and columns 1..2; `u` and `v` are normalized offsets from
/// `patch_16[1][1]`.
///
/// Each pass divides by its weight sum, so a constant patch returns that
/// constant. When a weight sum is within `1e-12`, the centre sample is used.
pub fn bicubic_surface_16point(patch_16: &[[f64; 4]; 4], u: f64, v: f64) -> f64 {
    let mut col_interp = [0.0; 4];

    for (r, patch_row) in patch_16.iter().enumerate() {
        let mut row_sum = 0.0;
        let mut weight_sum = 0.0;
        for (c, &cell) in patch_row.iter().enumerate() {
            let dist = u - (c as f64 - 1.0);
            let w = cubic_weight(dist);
            row_sum += cell * w;
            weight_sum += w;
        }
        col_interp[r] = if weight_sum.abs() > EPSILON {
            row_sum / weight_sum
        } else {
            patch_16[r][1]
        };
    }

    let mut final_val = 0.0;
    let mut weight_sum = 0.0;
    for (r, &row_value) in col_interp.iter().enumerate() {
        let dist = v - (r as f64 - 1.0);
        let w = cubic_weight(dist);
        final_val += row_value * w;
        weight_sum += w;
    }

    if weight_sum.abs() > EPSILON {
        final_val / weight_sum
    } else {
        col_interp[1]
    }
}

/// Locates `target` within `axis`, returning the bracketing indices and
/// normalized position.
///
/// The target is **clamped** to the axis range rather than rejected. A `NaN`
/// target produces `t = NaN` (source behavior), because every comparison falls
/// through. Duplicate or non-monotonic breakpoints are not validated.
pub fn search_interval(axis: &[f64], target: f64) -> Result<AxisInterval, InterpError> {
    if axis.len() < 2 {
        return Err(InterpError::AxisTooShort);
    }

    if target <= axis[0] {
        return Ok(AxisInterval {
            low: 0,
            high: 1,
            t: 0.0,
        });
    }

    let last = axis.len() - 1;
    if target >= axis[last] {
        return Ok(AxisInterval {
            low: last - 1,
            high: last,
            t: 1.0,
        });
    }

    let mut low = 0usize;
    let mut high = last;

    while high - low > 1 {
        let mid = (low + high) / 2;
        if axis[mid] <= target {
            low = mid;
        } else {
            high = mid;
        }
    }

    let span = axis[high] - axis[low];
    let t = if span.abs() > EPSILON {
        ((target - axis[low]) / span).clamp(0.0, 1.0)
    } else {
        0.0
    };

    Ok(AxisInterval { low, high, t })
}

/// 1D curve lookup over non-linear axis breakpoints.
///
/// Requires `axis.len() == values.len()`. Clamps outside the axis range.
pub fn lookup_1d_curve(axis: &[f64], values: &[f64], target: f64) -> Result<f64, InterpError> {
    if axis.len() != values.len() {
        return Err(InterpError::AxisValuesLengthMismatch);
    }
    let iv = search_interval(axis, target)?;
    Ok(lerp_1d(
        axis[iv.low],
        values[iv.low],
        axis[iv.high],
        values[iv.high],
        axis[iv.low] + iv.t * (axis[iv.high] - axis[iv.low]),
    ))
}

/// 2D map lookup with non-linear X and Y axes.
///
/// `map` must have `y_axis.len()` rows, each `x_axis.len()` wide.
/// Bilinear interpolation across the four surrounding cells; out-of-range
/// coordinates clamp to the map boundary.
pub fn lookup_2d_map(
    x_axis: &[f64],
    y_axis: &[f64],
    map: &[Vec<f64>],
    x: f64,
    y: f64,
) -> Result<f64, InterpError> {
    if map.len() != y_axis.len() {
        return Err(InterpError::MapRowsMismatch);
    }
    if map.is_empty() || map[0].len() != x_axis.len() {
        return Err(InterpError::MapColsMismatch);
    }

    let xi = search_interval(x_axis, x)?;
    let yi = search_interval(y_axis, y)?;

    let q00 = map[yi.low][xi.low];
    let q10 = map[yi.low][xi.high];
    let q01 = map[yi.high][xi.low];
    let q11 = map[yi.high][xi.high];

    Ok((1.0 - xi.t) * (1.0 - yi.t) * q00
        + xi.t * (1.0 - yi.t) * q10
        + (1.0 - xi.t) * yi.t * q01
        + xi.t * yi.t * q11)
}

// ---------------------------------------------------------------------------
// Region interpolation and matrix arithmetic (ported from
// `ecu_engine_rust::math::interpolation`)
// ---------------------------------------------------------------------------

/// Bilinearly interpolates the four corners of a rectangular region in place.
///
/// The corners `(r0,c0)`, `(r0,c1)`, `(r1,c0)`, `(r1,c1)` are read first, so
/// overwriting interior cells cannot corrupt the result. Degenerate spans
/// (zero rows or columns) are treated as `t = 0` rather than dividing by zero.
pub fn interpolate_matrix_region_bilinear(
    matrix: &mut [Vec<f64>],
    r0: usize,
    r1: usize,
    c0: usize,
    c1: usize,
) {
    let q00 = matrix[r0][c0];
    let q10 = matrix[r0][c1];
    let q01 = matrix[r1][c0];
    let q11 = matrix[r1][c1];

    let row_span = (r1 - r0) as f64;
    let col_span = (c1 - c0) as f64;

    for r in r0..=r1 {
        let v = if row_span > 0.0 {
            (r - r0) as f64 / row_span
        } else {
            0.0
        };
        for c in c0..=c1 {
            let u = if col_span > 0.0 {
                (c - c0) as f64 / col_span
            } else {
                0.0
            };
            matrix[r][c] = (1.0 - u) * (1.0 - v) * q00
                + u * (1.0 - v) * q10
                + (1.0 - u) * v * q01
                + u * v * q11;
        }
    }
}

/// Cell-by-cell difference: `modified - original`.
pub fn compute_delta(
    original: &[Vec<f64>],
    modified: &[Vec<f64>],
) -> Result<Vec<Vec<f64>>, &'static str> {
    if original.len() != modified.len()
        || original.is_empty()
        || original[0].len() != modified[0].len()
    {
        return Err("Original and modified maps must have identical dimensions");
    }
    let rows = original.len();
    let cols = original[0].len();
    let mut delta = vec![vec![0.0; cols]; rows];
    for r in 0..rows {
        for c in 0..cols {
            delta[r][c] = modified[r][c] - original[r][c];
        }
    }
    Ok(delta)
}

/// Cell-by-cell percentage change.
///
/// A near-zero base yields `0.0` rather than infinity, matching the source.
pub fn compute_percentage_difference(
    original: &[Vec<f64>],
    modified: &[Vec<f64>],
) -> Result<Vec<Vec<f64>>, &'static str> {
    if original.len() != modified.len()
        || original.is_empty()
        || original[0].len() != modified[0].len()
    {
        return Err("Original and modified maps must have identical dimensions");
    }
    let rows = original.len();
    let cols = original[0].len();
    let mut percent = vec![vec![0.0; cols]; rows];
    for r in 0..rows {
        for c in 0..cols {
            let base = original[r][c];
            percent[r][c] = if base.abs() > 1e-12 {
                ((modified[r][c] - base) / base) * 100.0
            } else {
                0.0
            };
        }
    }
    Ok(percent)
}

/// Adds a scalar offset to every cell in place.
pub fn apply_scalar_offset(map: &mut [Vec<f64>], offset: f64) {
    for row in map.iter_mut() {
        for cell in row.iter_mut() {
            *cell += offset;
        }
    }
}

/// Scales every cell by `1 + percentage/100` in place.
pub fn apply_percentage_scale(map: &mut [Vec<f64>], percentage: f64) {
    let factor = 1.0 + percentage / 100.0;
    for row in map.iter_mut() {
        for cell in row.iter_mut() {
            *cell *= factor;
        }
    }
}

/// Clamps every cell into `[min_val, max_val]` in place.
pub fn clamp_matrix(map: &mut [Vec<f64>], min_val: f64, max_val: f64) {
    for row in map.iter_mut() {
        for cell in row.iter_mut() {
            *cell = cell.clamp(min_val, max_val);
        }
    }
}
