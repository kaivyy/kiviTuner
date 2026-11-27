//! Map selection editing, region interpolation/extrapolation, and conversion
//! factor/offset solving.
//!
//! Pure functional helpers: every transformation validates bounds and returns a
//! new matrix rather than mutating caller state.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::interp::catmull_rom_cubic_1d;

/// Errors returned by map editing, interpolation, and conversion solvers.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum MapOpsError {
    #[error("matrix must be non-empty and rectangular")]
    InvalidMatrix,
    #[error("selection ({row_start}..={row_end}, {col_start}..={col_end}) is out of bounds for {rows}x{cols} matrix")]
    InvalidSelection {
        row_start: usize,
        row_end: usize,
        col_start: usize,
        col_end: usize,
        rows: usize,
        cols: usize,
    },
    #[error("original matrix dimensions {orig_rows}x{orig_cols} do not match current {cur_rows}x{cur_cols}")]
    DimensionMismatch {
        cur_rows: usize,
        cur_cols: usize,
        orig_rows: usize,
        orig_cols: usize,
    },
    #[error("invalid clamp range: min ({min}) exceeds max ({max})")]
    InvalidClampRange { min: f64, max: f64 },
    #[error("insufficient unselected border cells around selection for requested interpolation")]
    InsufficientBorder,
    #[error("degenerate or non-finite solver inputs: {0}")]
    DegenerateSolverInput(String),
}

/// Inclusive rectangular selection inside a 2-D calibration matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellSelection {
    pub row_start: usize,
    pub row_end: usize,
    pub col_start: usize,
    pub col_end: usize,
}

/// Editing operation applied uniformly across a [`CellSelection`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SelectionEditOp {
    SetAbsolute(f64),
    AddDelta(f64),
    /// Percentage delta: `+10.0` scales by `1.10`, `-5.0` scales by `0.95`.
    ScalePercent(f64),
    ClampRange { min: f64, max: f64 },
}

/// Interpolation curve kernel for region fill.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegionInterpMode {
    Linear,
    CubicCatmullRom,
}

/// Border axes eligible to drive region interpolation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegionInterpAxes {
    HorizontalOnly,
    VerticalOnly,
    AllFourSides,
}

/// Options controlling [`interpolate_matrix_region`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegionInterpOptions {
    pub mode: RegionInterpMode,
    pub axes: RegionInterpAxes,
    pub allow_extrapolation: bool,
    pub fallback_all_sides: bool,
}

impl Default for RegionInterpOptions {
    fn default() -> Self {
        Self {
            mode: RegionInterpMode::Linear,
            axes: RegionInterpAxes::AllFourSides,
            allow_extrapolation: false,
            fallback_all_sides: true,
        }
    }
}

/// Result returned by [`interpolate_matrix_region`], recording any automatic fallbacks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegionInterpResult {
    pub matrix: Vec<Vec<f64>>,
    pub applied_mode: RegionInterpMode,
    pub applied_axes: RegionInterpAxes,
    pub used_extrapolation: bool,
}

/// Solved raw-to-physical conversion parameters (`y = factor * X + offset` or `y = A / (X + C) + D`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FactorOffsetSolution {
    pub factor: f64,
    pub offset: f64,
    pub reciprocal: bool,
    pub reciprocal_num: f64,
    pub reciprocal_den_offset: f64,
    pub equation: String,
}

fn validate_matrix(matrix: &[Vec<f64>]) -> Result<(usize, usize), MapOpsError> {
    let rows = matrix.len();
    if rows == 0 {
        return Err(MapOpsError::InvalidMatrix);
    }
    let cols = matrix[0].len();
    if cols == 0 || matrix.iter().any(|r| r.len() != cols) {
        return Err(MapOpsError::InvalidMatrix);
    }
    Ok((rows, cols))
}

fn validate_selection(
    rows: usize,
    cols: usize,
    sel: CellSelection,
) -> Result<(), MapOpsError> {
    if sel.row_start <= sel.row_end
        && sel.row_end < rows
        && sel.col_start <= sel.col_end
        && sel.col_end < cols
    {
        Ok(())
    } else {
        Err(MapOpsError::InvalidSelection {
            row_start: sel.row_start,
            row_end: sel.row_end,
            col_start: sel.col_start,
            col_end: sel.col_end,
            rows,
            cols,
        })
    }
}

/// Applies `op` to every cell inside `sel`, returning an updated copy of `matrix`.
pub fn apply_selection_edit(
    matrix: &[Vec<f64>],
    sel: CellSelection,
    op: SelectionEditOp,
) -> Result<Vec<Vec<f64>>, MapOpsError> {
    let (rows, cols) = validate_matrix(matrix)?;
    validate_selection(rows, cols, sel)?;
    if let SelectionEditOp::ClampRange { min, max } = op {
        if min > max || !min.is_finite() || !max.is_finite() {
            return Err(MapOpsError::InvalidClampRange { min, max });
        }
    }

    let mut out = matrix.to_vec();
    for row in out.iter_mut().take(sel.row_end + 1).skip(sel.row_start) {
        for cell in row.iter_mut().take(sel.col_end + 1).skip(sel.col_start) {
            *cell = match op {
                SelectionEditOp::SetAbsolute(v) => v,
                SelectionEditOp::AddDelta(d) => *cell + d,
                SelectionEditOp::ScalePercent(pct) => *cell * (1.0 + pct / 100.0),
                SelectionEditOp::ClampRange { min, max } => cell.clamp(min, max),
            };
        }
    }
    Ok(out)
}

/// Restores all cells inside `sel` back to their values in `original`.
pub fn revert_selection_to_original(
    current: &[Vec<f64>],
    original: &[Vec<f64>],
    sel: CellSelection,
) -> Result<Vec<Vec<f64>>, MapOpsError> {
    let (cur_rows, cur_cols) = validate_matrix(current)?;
    let (orig_rows, orig_cols) = validate_matrix(original)?;
    if cur_rows != orig_rows || cur_cols != orig_cols {
        return Err(MapOpsError::DimensionMismatch {
            cur_rows,
            cur_cols,
            orig_rows,
            orig_cols,
        });
    }
    validate_selection(cur_rows, cur_cols, sel)?;

    let mut out = current.to_vec();
    for r in sel.row_start..=sel.row_end {
        for c in sel.col_start..=sel.col_end {
            out[r][c] = original[r][c];
        }
    }
    Ok(out)
}

#[derive(Debug, Clone)]
struct AxisFill {
    patch: Vec<Vec<f64>>,
    used_cubic: bool,
    used_extrapolation: bool,
}

fn try_horizontal_fill(
    matrix: &[Vec<f64>],
    sel: CellSelection,
    cols: usize,
    mode: RegionInterpMode,
    allow_extrapolation: bool,
) -> Option<AxisFill> {
    let h = sel.row_end - sel.row_start + 1;
    let w = sel.col_end - sel.col_start + 1;
    let has_left = sel.col_start > 0;
    let has_right = sel.col_end + 1 < cols;

    if has_left && has_right {
        let c_left = sel.col_start - 1;
        let c_right = sel.col_end + 1;
        let span = (c_right - c_left) as f64;
        let can_cubic = mode == RegionInterpMode::CubicCatmullRom
            && sel.col_start >= 2
            && sel.col_end + 2 < cols;
        let mut patch = vec![vec![0.0; w]; h];
        for (dr, r) in (sel.row_start..=sel.row_end).enumerate() {
            let p0 = matrix[r][c_left];
            let p1 = matrix[r][c_right];
            for (dc, c) in (sel.col_start..=sel.col_end).enumerate() {
                let t = (c - c_left) as f64 / span;
                patch[dr][dc] = if can_cubic {
                    let p_prev = matrix[r][sel.col_start - 2];
                    let p_next = matrix[r][sel.col_end + 2];
                    catmull_rom_cubic_1d(p_prev, p0, p1, p_next, t)
                } else {
                    p0 + t * (p1 - p0)
                };
            }
        }
        return Some(AxisFill {
            patch,
            used_cubic: can_cubic,
            used_extrapolation: false,
        });
    }

    if !allow_extrapolation {
        return None;
    }

    if !has_left && sel.col_end + 2 < cols {
        let c_near = sel.col_end + 1;
        let c_far = sel.col_end + 2;
        let mut patch = vec![vec![0.0; w]; h];
        for (dr, r) in (sel.row_start..=sel.row_end).enumerate() {
            let p_near = matrix[r][c_near];
            let p_far = matrix[r][c_far];
            let slope_left = p_near - p_far;
            for (dc, c) in (sel.col_start..=sel.col_end).enumerate() {
                let step = (c_near - c) as f64;
                patch[dr][dc] = p_near + slope_left * step;
            }
        }
        return Some(AxisFill {
            patch,
            used_cubic: false,
            used_extrapolation: true,
        });
    }

    if !has_right && sel.col_start >= 2 {
        let c_far = sel.col_start - 2;
        let c_near = sel.col_start - 1;
        let mut patch = vec![vec![0.0; w]; h];
        for (dr, r) in (sel.row_start..=sel.row_end).enumerate() {
            let p_far = matrix[r][c_far];
            let p_near = matrix[r][c_near];
            let slope_right = p_near - p_far;
            for (dc, c) in (sel.col_start..=sel.col_end).enumerate() {
                let step = (c - c_near) as f64;
                patch[dr][dc] = p_near + slope_right * step;
            }
        }
        return Some(AxisFill {
            patch,
            used_cubic: false,
            used_extrapolation: true,
        });
    }

    None
}

fn try_vertical_fill(
    matrix: &[Vec<f64>],
    sel: CellSelection,
    rows: usize,
    mode: RegionInterpMode,
    allow_extrapolation: bool,
) -> Option<AxisFill> {
    let h = sel.row_end - sel.row_start + 1;
    let w = sel.col_end - sel.col_start + 1;
    let has_top = sel.row_start > 0;
    let has_bottom = sel.row_end + 1 < rows;

    if has_top && has_bottom {
        let r_top = sel.row_start - 1;
        let r_bottom = sel.row_end + 1;
        let span = (r_bottom - r_top) as f64;
        let can_cubic = mode == RegionInterpMode::CubicCatmullRom
            && sel.row_start >= 2
            && sel.row_end + 2 < rows;
        let mut patch = vec![vec![0.0; w]; h];
        for (dc, c) in (sel.col_start..=sel.col_end).enumerate() {
            let p0 = matrix[r_top][c];
            let p1 = matrix[r_bottom][c];
            for (dr, r) in (sel.row_start..=sel.row_end).enumerate() {
                let t = (r - r_top) as f64 / span;
                patch[dr][dc] = if can_cubic {
                    let p_prev = matrix[sel.row_start - 2][c];
                    let p_next = matrix[sel.row_end + 2][c];
                    catmull_rom_cubic_1d(p_prev, p0, p1, p_next, t)
                } else {
                    p0 + t * (p1 - p0)
                };
            }
        }
        return Some(AxisFill {
            patch,
            used_cubic: can_cubic,
            used_extrapolation: false,
        });
    }

    if !allow_extrapolation {
        return None;
    }

    if !has_top && sel.row_end + 2 < rows {
        let r_near = sel.row_end + 1;
        let r_far = sel.row_end + 2;
        let mut patch = vec![vec![0.0; w]; h];
        for (dc, c) in (sel.col_start..=sel.col_end).enumerate() {
            let p_near = matrix[r_near][c];
            let p_far = matrix[r_far][c];
            let slope_up = p_near - p_far;
            for (dr, r) in (sel.row_start..=sel.row_end).enumerate() {
                let step = (r_near - r) as f64;
                patch[dr][dc] = p_near + slope_up * step;
            }
        }
        return Some(AxisFill {
            patch,
            used_cubic: false,
            used_extrapolation: true,
        });
    }

    if !has_bottom && sel.row_start >= 2 {
        let r_far = sel.row_start - 2;
        let r_near = sel.row_start - 1;
        let mut patch = vec![vec![0.0; w]; h];
        for (dc, c) in (sel.col_start..=sel.col_end).enumerate() {
            let p_far = matrix[r_far][c];
            let p_near = matrix[r_near][c];
            let slope_down = p_near - p_far;
            for (dr, r) in (sel.row_start..=sel.row_end).enumerate() {
                let step = (r - r_near) as f64;
                patch[dr][dc] = p_near + slope_down * step;
            }
        }
        return Some(AxisFill {
            patch,
            used_cubic: false,
            used_extrapolation: true,
        });
    }

    None
}

/// Interpolates (or extrapolates along borders when enabled) cells inside `sel`
/// from surrounding unselected reference cells, with automatic mode/axis fallbacks.
pub fn interpolate_matrix_region(
    matrix: &[Vec<f64>],
    sel: CellSelection,
    opts: RegionInterpOptions,
) -> Result<RegionInterpResult, MapOpsError> {
    let (rows, cols) = validate_matrix(matrix)?;
    validate_selection(rows, cols, sel)?;

    let h_fill = || try_horizontal_fill(matrix, sel, cols, opts.mode, opts.allow_extrapolation);
    let v_fill = || try_vertical_fill(matrix, sel, rows, opts.mode, opts.allow_extrapolation);

    let (patch, applied_axes, used_cubic, used_extrapolation) = match opts.axes {
        RegionInterpAxes::HorizontalOnly => {
            if let Some(h) = h_fill() {
                (h.patch, RegionInterpAxes::HorizontalOnly, h.used_cubic, h.used_extrapolation)
            } else if opts.fallback_all_sides {
                let v = v_fill().ok_or(MapOpsError::InsufficientBorder)?;
                (v.patch, RegionInterpAxes::VerticalOnly, v.used_cubic, v.used_extrapolation)
            } else {
                return Err(MapOpsError::InsufficientBorder);
            }
        }
        RegionInterpAxes::VerticalOnly => {
            if let Some(v) = v_fill() {
                (v.patch, RegionInterpAxes::VerticalOnly, v.used_cubic, v.used_extrapolation)
            } else if opts.fallback_all_sides {
                let h = h_fill().ok_or(MapOpsError::InsufficientBorder)?;
                (h.patch, RegionInterpAxes::HorizontalOnly, h.used_cubic, h.used_extrapolation)
            } else {
                return Err(MapOpsError::InsufficientBorder);
            }
        }
        RegionInterpAxes::AllFourSides => match (h_fill(), v_fill()) {
            (Some(h), Some(v)) => {
                let ph = h.patch.len();
                let pw = h.patch[0].len();
                let mut blended = vec![vec![0.0; pw]; ph];
                for dr in 0..ph {
                    for dc in 0..pw {
                        blended[dr][dc] = 0.5 * (h.patch[dr][dc] + v.patch[dr][dc]);
                    }
                }
                (
                    blended,
                    RegionInterpAxes::AllFourSides,
                    h.used_cubic || v.used_cubic,
                    h.used_extrapolation || v.used_extrapolation,
                )
            }
            (Some(h), None) => (
                h.patch,
                RegionInterpAxes::HorizontalOnly,
                h.used_cubic,
                h.used_extrapolation,
            ),
            (None, Some(v)) => (
                v.patch,
                RegionInterpAxes::VerticalOnly,
                v.used_cubic,
                v.used_extrapolation,
            ),
            (None, None) => return Err(MapOpsError::InsufficientBorder),
        },
    };

    let mut out = matrix.to_vec();
    for (dr, r) in (sel.row_start..=sel.row_end).enumerate() {
        for (dc, c) in (sel.col_start..=sel.col_end).enumerate() {
            out[r][c] = patch[dr][dc];
        }
    }

    Ok(RegionInterpResult {
        matrix: out,
        applied_mode: if used_cubic {
            RegionInterpMode::CubicCatmullRom
        } else {
            RegionInterpMode::Linear
        },
        applied_axes,
        used_extrapolation,
    })
}

fn format_linear_equation(factor: f64, offset: f64) -> String {
    if offset.abs() < 1e-12 {
        format!("{factor} * X")
    } else if offset > 0.0 {
        format!("{factor} * X + {offset}")
    } else {
        format!("{factor} * X - {}", -offset)
    }
}

/// Solves a linear conversion `y = factor * X + offset` from two `(raw, phys)` pairs.
///
/// When `swap_input_output` is `true`, the pair coordinates are swapped before solving.
pub fn solve_linear_factor_offset(
    p1: (f64, f64),
    p2: (f64, f64),
    swap_input_output: bool,
) -> Result<FactorOffsetSolution, MapOpsError> {
    let (x1, y1) = if swap_input_output { (p1.1, p1.0) } else { p1 };
    let (x2, y2) = if swap_input_output { (p2.1, p2.0) } else { p2 };
    if ![x1, y1, x2, y2].iter().all(|v| v.is_finite()) {
        return Err(MapOpsError::DegenerateSolverInput(
            "non-finite input coordinate".to_string(),
        ));
    }
    let dx = x2 - x1;
    if dx.abs() < 1e-12 {
        return Err(MapOpsError::DegenerateSolverInput(
            "linear solver requires distinct input coordinates".to_string(),
        ));
    }
    let factor = (y2 - y1) / dx;
    let offset = y1 - factor * x1;
    Ok(FactorOffsetSolution {
        factor,
        offset,
        reciprocal: false,
        reciprocal_num: 0.0,
        reciprocal_den_offset: 0.0,
        equation: format_linear_equation(factor, offset),
    })
}

/// Solves a reciprocal conversion `y = A / (X + C) + D` from three distinct `(raw, phys)` pairs.
pub fn solve_reciprocal_conversion(
    p1: (f64, f64),
    p2: (f64, f64),
    p3: (f64, f64),
    swap_input_output: bool,
) -> Result<FactorOffsetSolution, MapOpsError> {
    let (x1, y1) = if swap_input_output { (p1.1, p1.0) } else { p1 };
    let (x2, y2) = if swap_input_output { (p2.1, p2.0) } else { p2 };
    let (x3, y3) = if swap_input_output { (p3.1, p3.0) } else { p3 };

    if ![x1, y1, x2, y2, x3, y3].iter().all(|v| v.is_finite()) {
        return Err(MapOpsError::DegenerateSolverInput(
            "non-finite input coordinate".to_string(),
        ));
    }
    if (x2 - x1).abs() < 1e-12 || (x3 - x2).abs() < 1e-12 || (x3 - x1).abs() < 1e-12 {
        return Err(MapOpsError::DegenerateSolverInput(
            "reciprocal solver requires three distinct X coordinates".to_string(),
        ));
    }
    if (y2 - y1).abs() < 1e-12 || (y3 - y2).abs() < 1e-12 || (y3 - y1).abs() < 1e-12 {
        return Err(MapOpsError::DegenerateSolverInput(
            "reciprocal solver requires three distinct Y coordinates".to_string(),
        ));
    }

    let lhs = (y1 - y2) * (x3 - x2);
    let rhs = (y2 - y3) * (x2 - x1);
    let denom_c = lhs - rhs;
    if denom_c.abs() < 1e-12 {
        return Err(MapOpsError::DegenerateSolverInput(
            "points are collinear (no reciprocal curve)".to_string(),
        ));
    }

    let c = (rhs * x3 - lhs * x1) / denom_c;
    if (x1 + c).abs() < 1e-12 || (x2 + c).abs() < 1e-12 || (x3 + c).abs() < 1e-12 {
        return Err(MapOpsError::DegenerateSolverInput(
            "vertical asymptote coincides with input X coordinate".to_string(),
        ));
    }

    let a = (y1 - y2) * (x1 + c) * (x2 + c) / (x2 - x1);
    if !a.is_finite() || a.abs() < 1e-12 {
        return Err(MapOpsError::DegenerateSolverInput(
            "solved reciprocal numerator is degenerate".to_string(),
        ));
    }
    let d = y1 - a / (x1 + c);
    if !d.is_finite() {
        return Err(MapOpsError::DegenerateSolverInput(
            "solved reciprocal offset is non-finite".to_string(),
        ));
    }

    let eq = if c.abs() < 1e-12 && d.abs() < 1e-12 {
        format!("{a} / X")
    } else {
        format!("({a}) / (X + ({c})) + ({d})")
    };

    Ok(FactorOffsetSolution {
        factor: 1.0,
        offset: d,
        reciprocal: true,
        reciprocal_num: a,
        reciprocal_den_offset: c,
        equation: eq,
    })
}
