//! Ported from `ecu_engine_rust::telemetry::histogram`.
//!
//! Pure parsing/analysis logic. No socket, no serial port, no device access.
//! Verified by unit tests with hand-derived expectations; **not** validated
//! against live hardware.

/// Statistics accumulated per histogram cell.
#[derive(Debug, Clone, Copy)]
pub struct HistogramCell {
    pub hit_count: u32,
    pub sum_val: f64,
    pub min_val: f64,
    pub max_val: f64,
    pub running_average: f64,
}

impl Default for HistogramCell {
    fn default() -> Self {
        Self {
            hit_count: 0,
            sum_val: 0.0,
            min_val: f64::INFINITY,
            max_val: f64::NEG_INFINITY,
            running_average: 0.0,
        }
    }
}

/// 3D Histogram Grid for Real-Time Cell Tracking (e.g. AFR Wideband or Knock per RPM x Load).
pub struct HistogramTracer {
    pub rows: usize,
    pub cols: usize,
    pub row_axis_breakpoints: Vec<f64>,
    pub col_axis_breakpoints: Vec<f64>,
    pub cells: Vec<Vec<HistogramCell>>,
    pub last_hit_row: Option<usize>,
    pub last_hit_col: Option<usize>,
}

impl HistogramTracer {
    pub fn new(row_axis: Vec<f64>, col_axis: Vec<f64>) -> Self {
        let rows = row_axis.len();
        let cols = col_axis.len();
        let cells = vec![vec![HistogramCell::default(); cols]; rows];
        Self {
            rows,
            cols,
            row_axis_breakpoints: row_axis,
            col_axis_breakpoints: col_axis,
            cells,
            last_hit_row: None,
            last_hit_col: None,
        }
    }

    /// Locate cell indices [r, c] via nearest-neighbor or lower-bound search.
    pub fn locate_cell(&self, row_val: f64, col_val: f64) -> (usize, usize) {
        let r = Self::find_axis_index(&self.row_axis_breakpoints, row_val);
        let c = Self::find_axis_index(&self.col_axis_breakpoints, col_val);
        (r, c)
    }

    fn find_axis_index(axis: &[f64], val: f64) -> usize {
        if axis.is_empty() {
            return 0;
        }
        if val <= axis[0] {
            return 0;
        }
        let n = axis.len();
        if val >= axis[n - 1] {
            return n - 1;
        }
        for i in 0..n - 1 {
            if val >= axis[i] && val < axis[i + 1] {
                // Return closer breakpoint index
                let dist_left = (val - axis[i]).abs();
                let dist_right = (axis[i + 1] - val).abs();
                return if dist_left <= dist_right { i } else { i + 1 };
            }
        }
        n - 1
    }

    /// Ingest a live sensor sample and update the corresponding cell statistics.
    /// Returns the active (row, col) cell coordinates for real-time GUI highlighting.
    pub fn ingest_sample(&mut self, row_val: f64, col_val: f64, feedback_val: f64) -> (usize, usize) {
        if self.rows == 0
            || self.cols == 0
            || !row_val.is_finite()
            || !col_val.is_finite()
            || !feedback_val.is_finite()
        {
            return (0, 0);
        }
        let (r, c) = self.locate_cell(row_val, col_val);

        let cell = &mut self.cells[r][c];
        cell.hit_count = cell.hit_count.saturating_add(1);
        cell.sum_val += feedback_val;
        cell.running_average = cell.sum_val / cell.hit_count as f64;
        if feedback_val < cell.min_val {
            cell.min_val = feedback_val;
        }
        if feedback_val > cell.max_val {
            cell.max_val = feedback_val;
        }

        self.last_hit_row = Some(r);
        self.last_hit_col = Some(c);
        (r, c)
    }

    /// Calculate difference matrix between live histogram average and a target calibration table.
    /// Delta = LiveAverage - Target.
    pub fn calculate_delta_matrix(&self, target_table: &[Vec<f64>]) -> Vec<Vec<Option<f64>>> {
        let mut delta_matrix = vec![vec![None; self.cols]; self.rows];
        for r in 0..self.rows.min(target_table.len()) {
            for c in 0..self.cols.min(target_table[r].len()) {
                if self.cells[r][c].hit_count > 0 {
                    let diff = self.cells[r][c].running_average - target_table[r][c];
                    delta_matrix[r][c] = Some(diff);
                }
            }
        }
        delta_matrix
    }

    /// Clear all accumulated statistics.
    pub fn reset(&mut self) {
        for row in &mut self.cells {
            for cell in row {
                *cell = HistogramCell::default();
            }
        }
        self.last_hit_row = None;
        self.last_hit_col = None;
    }
}
