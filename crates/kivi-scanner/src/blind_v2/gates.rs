use std::collections::HashSet;

pub(super) fn passes_hard_filters_f64(values: &[f64], elem_bytes: usize) -> bool {
    let n = values.len();
    if n < 8 {
        return false;
    }
    let erasure_val_hi = if elem_bytes == 1 { 255.0 } else { 65535.0 };
    let erasure_cap = (n as f64 * 0.70) as usize + 1;
    let mut erasures_zero_hi = 0usize;
    let mut neg_ones = 0usize;
    let mut has_neg = false;
    let mut distinct: HashSet<u64> = HashSet::new();
    let (mut lo, mut hi) = (values[0], values[0]);
    for &v in values {
        if v < 0.0 {
            has_neg = true;
            if (v + 1.0).abs() <= 1e-9 {
                neg_ones += 1;
            }
        }
        if v == 0.0 || (elem_bytes <= 2 && v == erasure_val_hi) {
            erasures_zero_hi += 1;
        }
        if v < lo {
            lo = v;
        } else if v > hi {
            hi = v;
        }
        distinct.insert(v.to_bits());
    }
    let erasures = erasures_zero_hi + if has_neg && elem_bytes <= 2 { neg_ones } else { 0 };
    if erasures > erasure_cap || lo == hi {
        return false;
    }
    let d = distinct.len();
    if (d as f64 / n as f64) < 0.18 && d < 6 {
        return false;
    }
    true
}

pub(super) fn stripe_penalty_u64(values: &[u64], cols: usize) -> f64 {
    let n = values.len();
    if cols == 0 || n < cols {
        return 1.0;
    }
    let rows = n / cols;
    let small = n <= 25;
    if rows >= 2 {
        let first = &values[..cols];
        let mut equal = 0;
        for r in 1..rows {
            if &values[r * cols..(r + 1) * cols] == first {
                equal += 1;
            }
        }
        if equal >= rows - 1 {
            return 0.0;
        }
        if equal >= rows / 2 {
            return if small { 0.3 } else { 0.4 };
        }
    }
    let max_period = (n / 3).min(12);
    let hi_thr = if small { 0.85 } else { 0.9 };
    let mid_thr = if small { 0.65 } else { 0.75 };
    for period in 2..=max_period {
        if n < period * 3 {
            break;
        }
        let matches = (period..n).filter(|&i| values[i] == values[i - period]).count();
        let ratio = matches as f64 / (n - period) as f64;
        if ratio > hi_thr {
            return if small { 0.15 } else { 0.2 };
        }
        if ratio > mid_thr {
            return if small { 0.4 } else { 0.5 };
        }
    }
    1.0
}

pub(super) fn mean_delta_over_sd(grid: &[Vec<f64>]) -> f64 {
    let rows = grid.len();
    if rows < 2 {
        return f64::INFINITY;
    }
    let cols = grid[0].len();
    let flat: Vec<f64> = grid.iter().flatten().copied().collect();
    let n = flat.len() as f64;
    let mean = flat.iter().sum::<f64>() / n;
    let var = flat.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / n;
    let sd = var.sqrt();
    if sd < 1e-9 {
        return f64::INFINITY;
    }
    let mut total = 0.0;
    let mut count = 0.0;
    for r in 0..rows {
        for c in 0..cols {
            if c + 1 < cols {
                total += (grid[r][c + 1] - grid[r][c]).abs();
                count += 1.0;
            }
            if r + 1 < rows {
                total += (grid[r + 1][c] - grid[r][c]).abs();
                count += 1.0;
            }
        }
    }
    if count == 0.0 {
        return f64::INFINITY;
    }
    (total / count) / sd
}

/// Ratio of maximum 4-neighbor cell jump to total grid range `(max - min)`.
/// Returns `0.0` for flat grids. Used to reject boundary/filler contamination spikes
/// that inflate `sd` enough to artificially lower `mean_delta_over_sd`.
pub(super) fn max_adj_diff_over_range(grid: &[Vec<f64>]) -> f64 {
    let rows = grid.len();
    if rows == 0 {
        return 0.0;
    }
    let cols = grid[0].len();
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    let mut max_d = 0.0f64;
    for r in 0..rows {
        for c in 0..cols {
            let v = grid[r][c];
            if v < lo {
                lo = v;
            }
            if v > hi {
                hi = v;
            }
            if c + 1 < cols {
                max_d = max_d.max((grid[r][c + 1] - v).abs());
            }
            if r + 1 < rows {
                max_d = max_d.max((grid[r + 1][c] - v).abs());
            }
        }
    }
    let range = hi - lo;
    if range <= 1e-12 {
        0.0
    } else {
        max_d / range
    }
}

/// Geary's C (1954) 2-D spatial autocorrelation over 4-neighbor (Rook) grid edges.
///
/// Counts each undirected edge once:
/// `C = ((N - 1) * sum_{(u,v) in E} (x_u - x_v)^2) / (2 * |E| * sum_i (x_i - mean)^2)`.
/// Equals `~1.0` for spatially uncorrelated noise and `<< 1.0` for smooth surfaces.
pub fn gearys_c_2d(grid: &[Vec<f64>]) -> f64 {
    let rows = grid.len();
    if rows < 2 {
        return 1.0;
    }
    let cols = grid[0].len();
    if cols < 2 {
        return 1.0;
    }
    let n = (rows * cols) as f64;
    let mut sum = 0.0f64;
    for row in grid {
        for &v in row {
            sum += v;
        }
    }
    let mean = sum / n;
    let mut denom = 0.0f64;
    for row in grid {
        for &v in row {
            let d = v - mean;
            denom += d * d;
        }
    }
    if denom <= 1e-12 {
        return 1.0;
    }
    let mut sum_sq = 0.0f64;
    let mut edges = 0usize;
    for r in 0..rows {
        for c in 0..cols {
            let v = grid[r][c];
            if c + 1 < cols {
                let d = grid[r][c + 1] - v;
                sum_sq += d * d;
                edges += 1;
            }
            if r + 1 < rows {
                let d = grid[r + 1][c] - v;
                sum_sq += d * d;
                edges += 1;
            }
        }
    }
    if edges == 0 {
        return 1.0;
    }
    ((n - 1.0) * sum_sq) / (2.0 * (edges as f64) * denom)
}

/// Zero-allocation Rook 4-neighbor Geary's C on a row-major flat grid.
pub(super) fn gearys_c_flat(flat: &[f64], rows: usize, cols: usize) -> f64 {
    let n_cells = rows * cols;
    if rows < 2 || cols < 2 || flat.len() < n_cells {
        return 1.0;
    }
    let s = &flat[..n_cells];
    let n = n_cells as f64;
    let mean = s.iter().sum::<f64>() / n;
    let denom = s
        .iter()
        .map(|&v| {
            let d = v - mean;
            d * d
        })
        .sum::<f64>();
    if denom <= 1e-12 {
        return 1.0;
    }
    let mut sum_sq = 0.0f64;
    let mut edges = 0usize;
    for r in 0..rows {
        let base = r * cols;
        for c in 0..cols {
            let v = s[base + c];
            if c + 1 < cols {
                let d = s[base + c + 1] - v;
                sum_sq += d * d;
                edges += 1;
            }
            if r + 1 < rows {
                let d = s[base + cols + c] - v;
                sum_sq += d * d;
                edges += 1;
            }
        }
    }
    if edges == 0 {
        return 1.0;
    }
    ((n - 1.0) * sum_sq) / (2.0 * (edges as f64) * denom)
}

struct CurveMetrics {
    erasures: usize,
    distinct: usize,
    range: f64,
    max_jump: f64,
    mdsd: f64,
    vn: f64,
    z_vn: f64,
    z_rvn: f64,
}

fn compute_curve_metrics(
    values: &[f64],
    elem_bytes: usize,
    treat_neg_one_erase: bool,
) -> Option<CurveMetrics> {
    let n = values.len();
    if n < 3 {
        return None;
    }
    let erasures = if elem_bytes >= 4 {
        values.iter().filter(|&&v| v == 0.0).count()
    } else if treat_neg_one_erase {
        values
            .iter()
            .filter(|&&v| v == 0.0 || (v + 1.0).abs() <= 1e-9)
            .count()
    } else {
        let hi_erase = if elem_bytes == 1 { 255.0 } else { 65535.0 };
        values
            .iter()
            .filter(|&&v| v == 0.0 || (v - hi_erase).abs() <= 1e-9)
            .count()
    };

    let mut idx_vals: Vec<(usize, f64)> = values.iter().copied().enumerate().collect();
    if idx_vals.iter().any(|v| !v.1.is_finite()) {
        return None;
    }
    idx_vals.sort_by(|a, b| a.1.total_cmp(&b.1));
    let range = idx_vals[n - 1].1 - idx_vals[0].1;
    if !range.is_finite() || range <= 1e-12 {
        return None;
    }

    let mut distinct = 1usize;
    for k in 1..n {
        if (idx_vals[k].1 - idx_vals[k - 1].1).abs() > 1e-12 {
            distinct += 1;
        }
    }

    let mut max_jump = 0.0f64;
    let mut sum_abs = 0.0f64;
    let mut diff_sq = 0.0f64;
    for w in values.windows(2) {
        let ad = (w[1] - w[0]).abs();
        if ad > max_jump {
            max_jump = ad;
        }
        sum_abs += ad;
        diff_sq += ad * ad;
    }

    let nf = n as f64;
    let mean = values.iter().sum::<f64>() / nf;
    let denom = values
        .iter()
        .map(|&v| {
            let d = v - mean;
            d * d
        })
        .sum::<f64>();
    if denom <= 1e-12 {
        return None;
    }
    let sd = (denom / nf).sqrt();
    let mdsd = (sum_abs / (nf - 1.0)) / sd;
    let vn = diff_sq / denom;
    let var_vn = 4.0 * (nf - 2.0) / ((nf - 1.0) * (nf + 1.0));
    if !var_vn.is_finite() || var_vn <= 1e-12 {
        return None;
    }
    let z_vn = (vn - 2.0) / var_vn.sqrt();

    let mut ranks = vec![0.0f64; n];
    let mut i = 0usize;
    while i < n {
        let mut j = i;
        while j + 1 < n && (idx_vals[j + 1].1 - idx_vals[i].1).abs() <= 1e-12 {
            j += 1;
        }
        let avg_rank = (i + j) as f64 / 2.0 + 1.0;
        for item in &idx_vals[i..=j] {
            ranks[item.0] = avg_rank;
        }
        i = j + 1;
    }
    let r_mean = (nf + 1.0) / 2.0;
    let r_den = ranks
        .iter()
        .map(|&r| {
            let d = r - r_mean;
            d * d
        })
        .sum::<f64>();
    if r_den <= 1e-12 {
        return None;
    }
    let r_num = ranks
        .windows(2)
        .map(|w| {
            let d = w[1] - w[0];
            d * d
        })
        .sum::<f64>();
    let rvn = r_num / r_den;
    let var_rvn = 4.0 * (nf - 2.0) * (5.0 * nf * nf - 2.0 * nf - 9.0)
        / (5.0 * nf * (nf + 1.0) * (nf - 1.0) * (nf - 1.0));
    if var_rvn <= 1e-12 {
        return None;
    }
    let z_rvn = (rvn - 2.0) / var_rvn.sqrt();

    Some(CurveMetrics {
        erasures,
        distinct,
        range,
        max_jump,
        mdsd,
        vn,
        z_vn,
        z_rvn,
    })
}

/// Peer-reviewed statistical randomness and jump guards for 1-D calibration curves:
/// - Erasure saturation cap (`0` / `0xFF` / `0xFFFF` / `-1` <= 30% or <= 28% if monotonic)
/// - Minimum distinct value floor and max adjacent jump / range bound
/// - Bienaymé–Kendall turning-point rate (`turns / (n - 2) <= 0.25`)
/// - Exact von Neumann Ratio finite-sample `z`-score (1941)
/// - Bartels Rank Ratio `RVN` & exact finite-sample `z`-score (1982, midrank ties)
pub fn passes_statistical_curve_gates(values: &[f64], elem_bytes: usize, turning_points: usize) -> bool {
    let n = values.len();
    if n < 8 || (elem_bytes == 1 && n < 10) {
        return false;
    }
    let is_signed_hint = elem_bytes <= 2 && values.iter().any(|&v| v < 0.0);
    let Some(m) = compute_curve_metrics(values, elem_bytes, is_signed_hint) else {
        return false;
    };

    let max_erase_frac = if turning_points == 0 { 0.20 } else { 0.25 };
    if (m.erasures as f64) / (n as f64) > max_erase_frac {
        return false;
    }

    if elem_bytes == 1 && (m.distinct < 6 || m.range < 24.0) {
        return false;
    }

    let min_distinct = if turning_points == 0 || n <= 12 {
        (n / 2).max(5)
    } else {
        (n / 3).max(5)
    };
    if m.distinct < min_distinct {
        return false;
    }

    let max_jump_frac = if turning_points == 0 { 0.35 } else { 0.45 };
    if (m.max_jump / m.range) > max_jump_frac {
        return false;
    }

    let turn_rate = (turning_points as f64) / ((n.saturating_sub(2)).max(1) as f64);
    if turn_rate > 0.25 {
        return false;
    }

    let max_vn = if turning_points == 0 { 0.20 } else { 0.30 };
    if !m.vn.is_finite() || m.vn >= max_vn {
        return false;
    }

    let max_z_vn = if n <= 10 { -2.55 } else { -2.90 };
    if !m.z_vn.is_finite() || m.z_vn > max_z_vn {
        return false;
    }

    let max_z_rvn = if n <= 10 { -2.75 } else { -3.10 };
    m.z_rvn.is_finite() && m.z_rvn <= max_z_rvn
}

/// Statistical curve gate for structurally anchored (`[len][ax][Z]` or `[ax][Z]`) 1-D curves (`n >= 4`).
pub(super) fn passes_anchored_curve_gates(
    values: &[f64],
    elem_bytes: usize,
    is_signed: bool,
    turning_points: usize,
) -> bool {
    let n = values.len();
    if n < 4 {
        return false;
    }
    let Some(m) = compute_curve_metrics(values, elem_bytes, is_signed) else {
        return false;
    };
    if (m.erasures as f64) / (n as f64) > 0.40 {
        return false;
    }
    let min_distinct = if n >= 8 { 4 } else { 3 };
    if m.distinct < min_distinct || (m.max_jump / m.range) > 0.55 {
        return false;
    }
    if n <= 7 {
        let line_ref = (12.0_f64 / ((n * n - 1) as f64)).sqrt();
        let max_turns = if n == 4 { 0 } else { 1 };
        let max_vn = if n <= 5 { 0.85 } else { 0.70 };
        m.mdsd.is_finite()
            && m.mdsd <= 1.20 * line_ref
            && turning_points <= max_turns
            && m.vn.is_finite()
            && m.vn <= max_vn
    } else {
        let turn_rate = (turning_points as f64) / ((n - 2) as f64);
        m.mdsd.is_finite()
            && m.mdsd < 0.50
            && turn_rate <= 0.35
            && m.vn.is_finite()
            && m.vn < 0.45
            && m.z_vn.is_finite()
            && m.z_vn <= -2.30
            && m.z_rvn.is_finite()
            && m.z_rvn <= -2.35
    }
}

