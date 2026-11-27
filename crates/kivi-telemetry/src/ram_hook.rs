//! Ported from `ecu_engine_rust::telemetry::ram_hook`.
//!
//! Pure parsing/analysis logic. No socket, no serial port, no device access.
//! Verified by unit tests with hand-derived expectations; **not** validated
//! against live hardware.

use kivi_defs::interp::bilinear_2d;

/// 3D Tracer Bubble Location on Active Surface Map.
#[derive(Debug, Clone, Copy)]
pub struct TracerBubble3D {
    pub col_index_left: usize,
    pub col_index_right: usize,
    pub row_index_top: usize,
    pub row_index_bottom: usize,
    /// Normalized horizontal progress within cell (0.0 <= u <= 1.0)
    pub u_ratio: f64,
    /// Normalized vertical progress within cell (0.0 <= v <= 1.0)
    pub v_ratio: f64,
    /// Exact interpolated value under current operating point
    pub current_interpolated_z: f64,
}

/// Monolithic ECU RAM Redirection & Live Surface Tracer.
pub struct RamHookManager;

impl RamHookManager {
    /// Calculate the continuous 3D Tracer Bubble location for live telemetry (RPM, Load).
    pub fn calculate_tracer_bubble(
        x_axis_cols: &[f64],
        y_axis_rows: &[f64],
        table_z: &[Vec<f64>],
        current_x: f64,
        current_y: f64,
    ) -> Result<TracerBubble3D, &'static str> {
        let cols = x_axis_cols.len();
        let rows = y_axis_rows.len();

        if cols < 2 || rows < 2 {
            return Err("Table must have at least 2 columns and 2 rows");
        }
        if !current_x.is_finite() || !current_y.is_finite() {
            return Err("Current operating coordinates must be finite");
        }
        if table_z.len() != rows || table_z.iter().any(|row| row.len() != cols) {
            return Err("Table Z dimensions must match axis lengths");
        }

        // 1. Locate column bounding interval (X)
        let mut c = 0;
        while c < cols - 2 && x_axis_cols[c + 1] < current_x {
            c += 1;
        }
        let x0 = x_axis_cols[c];
        let x1 = x_axis_cols[c + 1];
        let u = if (x1 - x0).abs() < 1e-9 {
            0.0
        } else {
            ((current_x - x0) / (x1 - x0)).clamp(0.0, 1.0)
        };

        // 2. Locate row bounding interval (Y)
        let mut r = 0;
        while r < rows - 2 && y_axis_rows[r + 1] < current_y {
            r += 1;
        }
        let y0 = y_axis_rows[r];
        let y1 = y_axis_rows[r + 1];
        let v = if (y1 - y0).abs() < 1e-9 {
            0.0
        } else {
            ((current_y - y0) / (y1 - y0)).clamp(0.0, 1.0)
        };

        // 3. Bilinear Interpolation for Z (`bilinear_2d` expects q00, q10=(x1,y0), q01=(x0,y1), q11)
        let q_r0_c0 = table_z[r][c];
        let q_r0_c1 = table_z[r][c + 1];
        let q_r1_c0 = table_z[r + 1][c];
        let q_r1_c1 = table_z[r + 1][c + 1];
        let z_interp = bilinear_2d(
            x0,
            x1,
            y0,
            y1,
            q_r0_c0,
            q_r0_c1,
            q_r1_c0,
            q_r1_c1,
            current_x,
            current_y,
        );

        Ok(TracerBubble3D {
            col_index_left: c,
            col_index_right: c + 1,
            row_index_top: r,
            row_index_bottom: r + 1,
            u_ratio: u,
            v_ratio: v,
            current_interpolated_z: z_interp,
        })
    }

    /// Weighted Multi-Cell Live Edit: When tuner presses '+' while engine is running,
    /// distribute the delta increment across the 4 surrounding cells based on bilinear weights (u, v).
    ///
    /// This prevents step-discontinuities and keeps the fuel/ignition surface smooth!
    pub fn apply_weighted_delta_live(
        table_z: &mut [Vec<f64>],
        tracer: &TracerBubble3D,
        delta_amount: f64,
    ) {
        if !delta_amount.is_finite() || !tracer.u_ratio.is_finite() || !tracer.v_ratio.is_finite() {
            return;
        }
        let r0 = tracer.row_index_top;
        let r1 = tracer.row_index_bottom;
        let c0 = tracer.col_index_left;
        let c1 = tracer.col_index_right;
        if r1 >= table_z.len() || r0 >= table_z.len() {
            return;
        }
        if c1 >= table_z[r0].len()
            || c0 >= table_z[r0].len()
            || c1 >= table_z[r1].len()
            || c0 >= table_z[r1].len()
        {
            return;
        }

        let u = tracer.u_ratio;
        let v = tracer.v_ratio;

        let w00 = (1.0 - u) * (1.0 - v);
        let w01 = u * (1.0 - v);
        let w10 = (1.0 - u) * v;
        let w11 = u * v;

        table_z[r0][c0] += delta_amount * w00;
        table_z[r0][c1] += delta_amount * w01;
        table_z[r1][c0] += delta_amount * w10;
        table_z[r1][c1] += delta_amount * w11;
    }
}
