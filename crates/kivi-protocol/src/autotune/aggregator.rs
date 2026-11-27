/// Accumulator for statistical evidence within a single map cell.
#[derive(Debug, Clone, PartialEq)]
pub struct CellAccumulator {
    pub row: usize,
    pub col: usize,
    pub sample_count: usize,
    pub effective_sample_count: f64,
    pub original_value: f64,
    pub target_afr: f64,
    pub min_afr: f64,
    pub max_afr: f64,
    weighted_afr_sum: f64,
    // Welford's algorithm variables for variance
    mean_afr: f64,
    m2_afr: f64,
}

impl CellAccumulator {
    pub fn new(row: usize, col: usize, original_value: f64, target_afr: f64) -> Self {
        Self {
            row,
            col,
            sample_count: 0,
            effective_sample_count: 0.0,
            original_value,
            target_afr,
            min_afr: f64::INFINITY,
            max_afr: f64::NEG_INFINITY,
            weighted_afr_sum: 0.0,
            mean_afr: 0.0,
            m2_afr: 0.0,
        }
    }

    /// Adds a valid, weighted sample to this cell's statistics.
    pub fn push(&mut self, afr: f64, weight: f64) {
        self.sample_count += 1;
        self.effective_sample_count += weight;
        self.weighted_afr_sum += afr * weight;

        if afr < self.min_afr {
            self.min_afr = afr;
        }
        if afr > self.max_afr {
            self.max_afr = afr;
        }

        // Welford's online algorithm for variance
        let delta = afr - self.mean_afr;
        self.mean_afr += delta / self.sample_count as f64;
        let delta2 = afr - self.mean_afr;
        self.m2_afr += delta * delta2;
    }

    /// Returns the weighted mean measured AFR, or None if no samples.
    pub fn mean_measured_afr(&self) -> Option<f64> {
        if self.effective_sample_count > 0.0 {
            Some(self.weighted_afr_sum / self.effective_sample_count)
        } else {
            None
        }
    }

    /// Returns sample standard deviation of measured AFR.
    pub fn std_dev_afr(&self) -> f64 {
        if self.sample_count >= 2 {
            (self.m2_afr / (self.sample_count - 1) as f64).sqrt()
        } else {
            0.0
        }
    }
}

/// Grid aggregator maintaining accumulators across all cells of a table.
#[derive(Debug)]
pub struct CellAggregator {
    rows: usize,
    cols: usize,
    grid: Vec<CellAccumulator>,
}

impl CellAggregator {
    pub fn new(rows: usize, cols: usize, original_values: &[f64], target_afrs: &[f64]) -> Self {
        let mut grid = Vec::with_capacity(rows * cols);
        for r in 0..rows {
            for c in 0..cols {
                let idx = r * cols + c;
                let orig = original_values.get(idx).copied().unwrap_or(0.0);
                let target = target_afrs.get(idx).copied().unwrap_or(14.7);
                grid.push(CellAccumulator::new(r, c, orig, target));
            }
        }
        Self { rows, cols, grid }
    }

    pub fn record_sample(&mut self, row: usize, col: usize, afr: f64, weight: f64) {
        if row < self.rows && col < self.cols {
            let idx = row * self.cols + col;
            self.grid[idx].push(afr, weight);
        }
    }

    pub fn into_accumulators(self) -> Vec<CellAccumulator> {
        self.grid
    }
}
