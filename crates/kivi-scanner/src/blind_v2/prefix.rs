//! O(1) prefix-sum & failure-jump indices for fast multi-width ROM surface scanning.

#[derive(Clone, Copy)]
pub(super) enum IntKind {
    U8,
    I8,
    U16Le,
    U16Be,
    I16Le,
    I16Be,
}

impl IntKind {
    #[inline]
    pub fn elem_bytes(self) -> usize {
        match self {
            IntKind::U8 | IntKind::I8 => 1,
            _ => 2,
        }
    }

    #[inline]
    pub fn little_endian(self) -> bool {
        matches!(self, IntKind::U16Le | IntKind::I16Le)
    }

    #[inline]
    pub fn is_signed(self) -> bool {
        matches!(self, IntKind::I8 | IntKind::I16Le | IntKind::I16Be)
    }

    #[inline]
    pub fn label(self) -> &'static str {
        match self {
            IntKind::U8 => "u8",
            IntKind::I8 => "i8",
            IntKind::U16Le => "u16_le",
            IntKind::U16Be => "u16_be",
            IntKind::I16Le => "i16_le",
            IntKind::I16Be => "i16_be",
        }
    }
}

pub(super) struct IntPrefixIndex {
    pub vals: Vec<i32>,
    p_sum: Vec<i64>,
    p_sq: Vec<u64>,
    p_abs: Vec<u64>,
    p_neg: Vec<u32>,
    last_bad: Vec<i32>,
}

impl IntPrefixIndex {
    pub fn build_from_rom(rom: &[u8], start: usize, end: usize, kind: IntKind, max_step: i32) -> Self {
        let step_b = kind.elem_bytes();
        let cells = end.saturating_sub(start) / step_b;
        let mut vals = Vec::with_capacity(cells);
        let mut p_sum = Vec::with_capacity(cells + 1);
        let mut p_sq = Vec::with_capacity(cells + 1);
        let mut p_abs = Vec::with_capacity(cells + 1);
        let mut p_neg = Vec::with_capacity(cells + 1);
        let mut last_bad = Vec::with_capacity(cells.saturating_sub(1));

        p_sum.push(0);
        p_sq.push(0);
        p_abs.push(0);
        p_neg.push(0);

        let mut acc_sum = 0i64;
        let mut acc_sq = 0u64;
        let mut acc_abs = 0u64;
        let mut acc_neg = 0u32;
        let mut cur_bad = -1i32;

        for i in 0..cells {
            let o = start + i * step_b;
            let v: i32 = match kind {
                IntKind::U8 => rom[o] as i32,
                IntKind::I8 => (rom[o] as i8) as i32,
                IntKind::U16Le => u16::from_le_bytes([rom[o], rom[o + 1]]) as i32,
                IntKind::U16Be => u16::from_be_bytes([rom[o], rom[o + 1]]) as i32,
                IntKind::I16Le => i16::from_le_bytes([rom[o], rom[o + 1]]) as i32,
                IntKind::I16Be => i16::from_be_bytes([rom[o], rom[o + 1]]) as i32,
            };
            vals.push(v);
            acc_sum += v as i64;
            acc_sq += (v as i64 * v as i64) as u64;
            acc_neg += u32::from(v < 0);
            p_sum.push(acc_sum);
            p_sq.push(acc_sq);
            p_neg.push(acc_neg);

            if i > 0 {
                let d = (v - vals[i - 1]).abs();
                if d > max_step {
                    cur_bad = (i - 1) as i32;
                }
                acc_abs += d as u64;
                p_abs.push(acc_abs);
                last_bad.push(cur_bad);
            }
        }
        if cells > 0 {
            // Padding entry so `p_abs[end]` matches cell indexing cleanly when empty.
            p_abs.push(acc_abs);
        }

        Self {
            vals,
            p_sum,
            p_sq,
            p_abs,
            p_neg,
            last_bad,
        }
    }

    /// Returns the smallest next `cell` > current where rows `0..min(rows,3)` no longer hit a step > max_step.
    #[inline]
    pub fn bad_jump_next(&self, cell: usize, rows: usize, cols: usize) -> Option<usize> {
        if cols < 2 {
            return None;
        }
        let check_rows = rows.min(3);
        let mut best: Option<usize> = None;
        for r in 0..check_rows {
            let r_cell = cell + r * cols;
            let lb = *self.last_bad.get(r_cell + cols - 2)?;
            if lb >= r_cell as i32 {
                let next = lb as usize + 1 - r * cols;
                if next > cell {
                    best = Some(best.map_or(next, |b| b.max(next)));
                }
            }
        }
        best
    }

    #[inline]
    pub fn check_rows_fast(&self, cell: usize, rows: usize, cols: usize, max_avg_step: usize) -> bool {
        let check_rows = rows.min(3);
        let mut sum_diff = 0u64;
        for r in 0..check_rows {
            let r_cell = cell + r * cols;
            if cols >= 2 {
                if self.last_bad[r_cell + cols - 2] >= r_cell as i32 {
                    return false;
                }
                sum_diff += self.p_abs[r_cell + cols - 1] - self.p_abs[r_cell];
            }
        }
        sum_diff <= (cols * check_rows * max_avg_step) as u64
    }

    #[inline]
    pub fn crosses_zero(&self, cell: usize, total_cells: usize) -> bool {
        let neg = self.p_neg[cell + total_cells] - self.p_neg[cell];
        neg > 0 && neg < total_cells as u32
    }

    #[inline]
    pub fn has_no_sign_mix(&self) -> bool {
        let tot = *self.p_neg.last().unwrap_or(&0);
        tot == 0 || tot == self.vals.len() as u32
    }

    #[inline]
    pub fn window_mean_var(&self, cell: usize, total_cells: usize) -> (f64, f64, f64) {
        let n = total_cells as f64;
        let s = self.p_sum[cell + total_cells] - self.p_sum[cell];
        let sq = self.p_sq[cell + total_cells] - self.p_sq[cell];
        let mean = s as f64 / n;
        let var_num = (sq as i128 * total_cells as i128) - (s as i128 * s as i128);
        let denom = if var_num <= 0 {
            0.0
        } else {
            var_num as f64 / n
        };
        let var = denom / n;
        (mean, var, denom)
    }

    #[inline]
    pub fn sum_dx_u64(&self, cell: usize, rows: usize, cols: usize) -> u64 {
        let mut acc = 0u64;
        for r in 0..rows {
            let r_cell = cell + r * cols;
            acc += self.p_abs[r_cell + cols - 1] - self.p_abs[r_cell];
        }
        acc
    }

    /// Fast zero-float-inner-loop autocorrelation + smoothness via O(1) variance and integer dot product.
    #[inline]
    pub fn ac_and_mdsd(
        &self,
        cell: usize,
        rows: usize,
        cols: usize,
        min_var: f64,
        min_ac: f64,
        max_mdsd: f64,
    ) -> Option<(f64, f64)> {
        let total_cells = rows * cols;
        if cell + total_cells > self.vals.len() || rows < 2 || cols < 2 {
            return None;
        }
        let (mean, var, denom) = self.window_mean_var(cell, total_cells);
        if var < min_var || denom < 1e-12 {
            return None;
        }
        let sd = var.sqrt();
        let delta_count = (rows * (cols - 1) + (rows - 1) * cols) as f64;
        let budget = max_mdsd * sd * delta_count;
        let sum_dx_int = self.sum_dx_u64(cell, rows, cols);
        let sum_dx = sum_dx_int as f64;
        if sum_dx > budget {
            return None;
        }

        let w = &self.vals[cell..cell + total_cells];
        let m = total_cells - cols;
        let mut dot = 0i64;
        for i in 0..m {
            dot += (w[i] as i64) * (w[i + cols] as i64);
        }
        let sum_lo = (self.p_sum[cell + m] - self.p_sum[cell]) as f64;
        let sum_hi = (self.p_sum[cell + total_cells] - self.p_sum[cell + cols]) as f64;
        let num = dot as f64 - mean * (sum_lo + sum_hi) + (m as f64) * mean * mean;
        let ac = num / denom;
        if !ac.is_finite() || ac < min_ac {
            return None;
        }

        let mut acc_dy = 0u64;
        let int_budget = budget.ceil() as u64;
        for r in 0..rows - 1 {
            let base = r * cols;
            let next = base + cols;
            for c in 0..cols {
                acc_dy += w[next + c].abs_diff(w[base + c]) as u64;
            }
            let total_int = sum_dx_int + acc_dy;
            if total_int > int_budget && (total_int as f64) > budget {
                return None;
            }
        }
        let mdsd = ((sum_dx_int + acc_dy) as f64 / delta_count) / sd;
        if mdsd.is_finite() && mdsd < max_mdsd {
            Some((ac, mdsd))
        } else {
            None
        }
    }
}

pub(super) struct F32PrefixIndex {
    pub vals: Vec<f32>,
    last_invalid: Vec<i32>,
    p_zero: Vec<u32>,
    p_sum: Vec<f64>,
    p_sq: Vec<f64>,
    p_abs: Vec<f64>,
    max_valid_run: usize,
}

impl F32PrefixIndex {
    #[inline]
    pub fn is_valid_cal_float(fv: f32) -> bool {
        fv.is_finite()
            && !fv.is_subnormal()
            && fv.abs() <= 1_000_000.0
            && (fv == 0.0 || fv.abs() >= 1e-6)
    }

    pub fn build_from_rom(rom: &[u8], start: usize, end: usize, le: bool) -> Self {
        let cells = end.saturating_sub(start) / 4;
        let mut vals = Vec::with_capacity(cells);
        let mut last_invalid = Vec::with_capacity(cells);
        let mut p_zero = Vec::with_capacity(cells + 1);
        let mut p_sum = Vec::with_capacity(cells + 1);
        let mut p_sq = Vec::with_capacity(cells + 1);
        let mut p_abs = Vec::with_capacity(cells + 1);

        p_zero.push(0);
        p_sum.push(0.0);
        p_sq.push(0.0);
        p_abs.push(0.0);

        let mut cur_inv = -1i32;
        let mut acc_zero = 0u32;
        let mut acc_sum = 0.0f64;
        let mut acc_sq = 0.0f64;
        let mut acc_abs = 0.0f64;
        let mut prev_clean = 0.0f64;
        let mut cur_run = 0usize;
        let mut max_valid_run = 0usize;

        for i in 0..cells {
            let o = start + i * 4;
            let b = [rom[o], rom[o + 1], rom[o + 2], rom[o + 3]];
            let fv = if le { f32::from_le_bytes(b) } else { f32::from_be_bytes(b) };
            let valid = Self::is_valid_cal_float(fv);
            if valid {
                cur_run += 1;
                if cur_run > max_valid_run {
                    max_valid_run = cur_run;
                }
            } else {
                cur_inv = i as i32;
                cur_run = 0;
            }
            let clean = if valid { fv as f64 } else { 0.0 };
            vals.push(fv);
            last_invalid.push(cur_inv);
            acc_zero += u32::from(valid && fv == 0.0);
            acc_sum += clean;
            acc_sq += clean * clean;
            p_zero.push(acc_zero);
            p_sum.push(acc_sum);
            p_sq.push(acc_sq);
            if i > 0 {
                acc_abs += (clean - prev_clean).abs();
                p_abs.push(acc_abs);
            }
            prev_clean = clean;
        }
        if cells > 0 {
            p_abs.push(acc_abs);
        }

        Self {
            vals,
            last_invalid,
            p_zero,
            p_sum,
            p_sq,
            p_abs,
            max_valid_run,
        }
    }

    #[inline]
    pub fn max_valid_run(&self) -> usize {
        self.max_valid_run
    }

    #[inline]
    pub fn next_valid_start(&self, cell: usize, cells: usize) -> Option<usize> {
        let end = cell + cells - 1;
        let li = *self.last_invalid.get(end)?;
        if li >= cell as i32 {
            Some(li as usize + 1)
        } else {
            None
        }
    }

    #[inline]
    pub fn zero_count(&self, cell: usize, cells: usize) -> u32 {
        self.p_zero[cell + cells] - self.p_zero[cell]
    }

    #[inline]
    pub fn approx_mean_sd(&self, cell: usize, cells: usize) -> (f64, f64) {
        let n = cells as f64;
        let s = self.p_sum[cell + cells] - self.p_sum[cell];
        let sq = self.p_sq[cell + cells] - self.p_sq[cell];
        let mean = s / n;
        let var = ((sq / n) - mean * mean).max(0.0);
        (mean, var.sqrt())
    }

    #[inline]
    pub fn sum_dx(&self, cell: usize, rows: usize, cols: usize) -> f64 {
        let mut acc = 0.0f64;
        for r in 0..rows {
            let r_cell = cell + r * cols;
            acc += self.p_abs[r_cell + cols - 1] - self.p_abs[r_cell];
        }
        acc
    }
}

pub(super) struct U32PrefixIndex {
    pub vals: Vec<u32>,
    p_zero_or_max: Vec<u32>,
    p_sum: Vec<u64>,
    p_sq: Vec<u128>,
    p_abs: Vec<u64>,
    last_bad: Vec<i32>,
}

impl U32PrefixIndex {
    pub fn build_from_rom(rom: &[u8], start: usize, end: usize, le: bool) -> Self {
        const MAX_U32_STEP: u64 = 150_000_000;
        let cells = end.saturating_sub(start) / 4;
        let mut vals = Vec::with_capacity(cells);
        let mut p_zero_or_max = Vec::with_capacity(cells + 1);
        let mut p_sum = Vec::with_capacity(cells + 1);
        let mut p_sq = Vec::with_capacity(cells + 1);
        let mut p_abs = Vec::with_capacity(cells + 1);
        let mut last_bad = Vec::with_capacity(cells.saturating_sub(1));

        p_zero_or_max.push(0);
        p_sum.push(0);
        p_sq.push(0);
        p_abs.push(0);

        let mut acc_zm = 0u32;
        let mut acc_sum = 0u64;
        let mut acc_sq = 0u128;
        let mut acc_abs = 0u64;
        let mut prev = 0u64;
        let mut cur_bad = -1i32;

        for i in 0..cells {
            let o = start + i * 4;
            let b = [rom[o], rom[o + 1], rom[o + 2], rom[o + 3]];
            let uv = if le { u32::from_le_bytes(b) } else { u32::from_be_bytes(b) };
            vals.push(uv);
            acc_zm += u32::from(uv == 0 || uv == u32::MAX);
            let u64v = uv as u64;
            acc_sum += u64v;
            acc_sq += (uv as u128) * (uv as u128);
            p_zero_or_max.push(acc_zm);
            p_sum.push(acc_sum);
            p_sq.push(acc_sq);
            if i > 0 {
                let d = u64v.abs_diff(prev);
                if d > MAX_U32_STEP {
                    cur_bad = (i - 1) as i32;
                }
                acc_abs += d;
                p_abs.push(acc_abs);
                last_bad.push(cur_bad);
            }
            prev = u64v;
        }
        if cells > 0 {
            p_abs.push(acc_abs);
        }

        Self {
            vals,
            p_zero_or_max,
            p_sum,
            p_sq,
            p_abs,
            last_bad,
        }
    }

    #[inline]
    pub fn bad_jump_next(&self, cell: usize, rows: usize, cols: usize) -> Option<usize> {
        if cols < 2 {
            return None;
        }
        let check_rows = rows.min(3);
        let mut best: Option<usize> = None;
        for r in 0..check_rows {
            let r_cell = cell + r * cols;
            let lb = *self.last_bad.get(r_cell + cols - 2)?;
            if lb >= r_cell as i32 {
                let next = lb as usize + 1 - r * cols;
                if next > cell {
                    best = Some(best.map_or(next, |b| b.max(next)));
                }
            }
        }
        best
    }

    #[inline]
    pub fn zero_or_max_count(&self, cell: usize, cells: usize) -> u32 {
        self.p_zero_or_max[cell + cells] - self.p_zero_or_max[cell]
    }

    #[inline]
    pub fn exact_mean_sd(&self, cell: usize, cells: usize) -> (f64, f64) {
        let n = cells as u128;
        let s = (self.p_sum[cell + cells] - self.p_sum[cell]) as u128;
        let sq = self.p_sq[cell + cells] - self.p_sq[cell];
        let var_num = (sq * n).saturating_sub(s * s);
        let mean = s as f64 / cells as f64;
        let sd = (var_num as f64 / ((cells * cells) as f64)).sqrt();
        (mean, sd)
    }

    #[inline]
    pub fn sum_dx(&self, cell: usize, rows: usize, cols: usize) -> f64 {
        let mut acc = 0u64;
        for r in 0..rows {
            let r_cell = cell + r * cols;
            acc += self.p_abs[r_cell + cols - 1] - self.p_abs[r_cell];
        }
        acc as f64
    }
}
