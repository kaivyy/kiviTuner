//! Shared recognition metrics: entropy, autocorrelation, surface smoothness,
//! and 16-bit word extraction.
//!
//! These are the primitives shared by the heuristic detector and the Zero-XDF
//! pre-filter. They are **one** implementation serving both, because the source
//! modules duplicate the same maths under different names:
//!
//! | Heuristic | Zero-XDF | Identical? |
//! |---|---|---|
//! | `calculate_entropy` | `shannon_entropy` | yes, byte-for-byte |
//! | `autocorrelation_lag_w` | `autocorr_w` | yes, byte-for-byte |
//! | `calculate_surface_smoothness` | `calc_smoothness` | yes for rectangular matrices |
//!
//! ## The one real difference
//!
//! `heuristic::calculate_surface_smoothness` counts neighbour differences with an
//! explicit counter; `zero_xdf::calc_smoothness` derives the counts from
//! `rows * (cols - 1)` and `(rows - 1) * cols`. For a rectangular matrix these
//! are equal. For a **ragged** matrix they diverge, because the row length used
//! for `cols` is that of the first row. This module uses the dimension-derived
//! form (the Zero-XDF behaviour) and documents it; callers are expected to pass
//! rectangular matrices, which is what every source caller does.
//!
//! All functions are pure, deterministic, and perform no I/O.

/// Shannon entropy of a byte slice, in bits per symbol.
///
/// Returns `0.0` for empty input. The result is bounded to `[0.0, 8.0]`.
pub fn entropy(slice: &[u8]) -> f64 {
    if slice.is_empty() {
        return 0.0;
    }
    let mut counts = [0u32; 256];
    for &b in slice {
        counts[b as usize] += 1;
    }
    let len = slice.len() as f64;
    let mut ent = 0.0;
    for &c in &counts {
        if c > 0 {
            let p = c as f64 / len;
            ent -= p * p.log2();
        }
    }
    ent
}

/// Normalised autocorrelation at lag `w`.
///
/// Returns `0.0` when `data.len() <= w` or when the signal has effectively no
/// variance (denominator below `1e-12`). Lag `0` of a non-constant signal is
/// `1.0` by construction.
pub fn autocorrelation(data: &[f64], w: usize) -> f64 {
    let n = data.len();
    if n <= w {
        return 0.0;
    }
    let mean = data.iter().sum::<f64>() / n as f64;
    let mut num = 0.0;
    let mut denom = 0.0;

    for i in 0..n {
        let diff = data[i] - mean;
        denom += diff * diff;
        if i + w < n {
            num += diff * (data[i + w] - mean);
        }
    }

    if denom < 1e-12 {
        0.0
    } else {
        num / denom
    }
}

/// Total-variation surface roughness ratio.
///
/// Lower is smoother. Returns `1.0` for a matrix with fewer than two rows or
/// columns (matching the source's "cannot assess" value). The standard deviation
/// carries a `1e-5` floor so a perfectly constant matrix yields approximately
/// zero rather than dividing by zero.
pub fn smoothness(matrix: &[Vec<f64>]) -> f64 {
    let rows = matrix.len();
    if rows < 2 {
        return 1.0;
    }
    let cols = matrix[0].len();
    if cols < 2 {
        return 1.0;
    }

    let mut sum_dx = 0.0;
    for row in matrix.iter() {
        for pair in row.windows(2) {
            sum_dx += (pair[1] - pair[0]).abs();
        }
    }
    let mut sum_dy = 0.0;
    for rows_pair in matrix.windows(2) {
        for (lower, upper) in rows_pair[0].iter().zip(rows_pair[1].iter()) {
            sum_dy += (upper - lower).abs();
        }
    }

    let total_cells = (rows * cols) as f64;
    let sum_val: f64 = matrix.iter().flat_map(|r| r.iter()).sum();
    let mean_val = sum_val / total_cells;
    let var_val: f64 = matrix
        .iter()
        .flat_map(|r| r.iter())
        .map(|v| (v - mean_val).powi(2))
        .sum();
    let std_dev = (var_val / total_cells).sqrt() + 1e-5;

    let mean_dx = sum_dx / (rows * (cols - 1)) as f64;
    let mean_dy = sum_dy / ((rows - 1) * cols) as f64;
    (mean_dx + mean_dy) / std_dev
}

/// Extracts 16-bit little-endian words from a byte slice.
///
/// A trailing odd byte is ignored rather than padded, matching the source.
pub fn scan_16bit_le(rom: &[u8]) -> Vec<u16> {
    rom.as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .collect()
}

/// Extracts 16-bit big-endian words from a byte slice.
///
/// A trailing odd byte is ignored rather than padded, matching the source.
pub fn scan_16bit_be(rom: &[u8]) -> Vec<u16> {
    rom.as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_be_bytes(*c))
        .collect()
}
