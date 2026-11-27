use crate::model::{DiffKind, DiffRegion};
use kivi_core::types::{DType, Endian};

pub const DEFAULT_MAX_GAP: usize = 4;

const COMMON_PITCHES: [usize; 9] = [8, 10, 12, 15, 16, 19, 20, 24, 32];

#[derive(Debug, Clone, Copy)]
struct ClusterBuilder {
    start_addr: usize,
    last_changed: usize,
    changed_byte_count: usize,
}

/// Compares two binary buffers and groups differences into clusters of `DiffRegion`s.
/// Differences separated by at most `max_gap` unchanged bytes are merged into the same region.
pub fn compare_binaries(base: &[u8], target: &[u8], max_gap: usize) -> Vec<DiffRegion> {
    let min_len = base.len().min(target.len());
    let max_len = base.len().max(target.len());

    let mut clusters: Vec<ClusterBuilder> = Vec::new();
    let mut current: Option<ClusterBuilder> = None;

    let mut record_diff = |idx: usize| {
        if let Some(c) = current.as_mut() {
            if idx - c.last_changed - 1 <= max_gap {
                c.last_changed = idx;
                c.changed_byte_count += 1;
                return;
            } else {
                clusters.push(*c);
            }
        }
        current = Some(ClusterBuilder {
            start_addr: idx,
            last_changed: idx,
            changed_byte_count: 1,
        });
    };

    // Fast-path scan matching chunks of 64 bytes
    let (chunks_b, _) = base[..min_len].as_chunks::<64>();
    let (chunks_t, _) = target[..min_len].as_chunks::<64>();
    let rem_start = chunks_b.len() * 64;

    for (chunk_idx, (cb, ct)) in chunks_b.iter().zip(chunks_t.iter()).enumerate() {
        if cb != ct {
            let base_idx = chunk_idx * 64;
            for j in 0..64 {
                if cb[j] != ct[j] {
                    record_diff(base_idx + j);
                }
            }
        }
    }

    for i in rem_start..min_len {
        if base[i] != target[i] {
            record_diff(i);
        }
    }

    for i in min_len..max_len {
        record_diff(i);
    }

    if let Some(c) = current {
        clusters.push(c);
    }

    clusters
        .into_iter()
        .map(|c| finalize_cluster(base, target, c))
        .collect()
}

fn finalize_cluster(base: &[u8], target: &[u8], cluster: ClusterBuilder) -> DiffRegion {
    let start_addr = cluster.start_addr;
    let end_addr = cluster.last_changed + 1;
    let byte_len = end_addr - start_addr;

    let mut changed_byte_count = 0;
    let mut max_abs_delta: f64 = 0.0;
    let mut sum_delta: f64 = 0.0;

    for i in start_addr..end_addr {
        let b = base.get(i).copied().unwrap_or(0) as f64;
        let t = target.get(i).copied().unwrap_or(0) as f64;
        let diff = t - b;
        if base.get(i) != target.get(i) {
            changed_byte_count += 1;
        }
        let abs_diff = diff.abs();
        if abs_diff > max_abs_delta {
            max_abs_delta = abs_diff;
        }
        sum_delta += diff;
    }

    let mean_delta = if changed_byte_count > 0 {
        sum_delta / (changed_byte_count as f64)
    } else {
        0.0
    };

    let (inferred_dtype, inferred_endian) =
        infer_dtype_and_endian(base, target, start_addr, end_addr);

    let classification = classify_region(byte_len, inferred_dtype);

    DiffRegion {
        id: format!("diff_0x{:X}", start_addr),
        start_addr,
        end_addr,
        byte_len,
        changed_byte_count,
        max_abs_delta,
        mean_delta,
        inferred_dtype,
        inferred_endian,
        classification,
    }
}

fn infer_dtype_and_endian(
    base: &[u8],
    target: &[u8],
    start_addr: usize,
    end_addr: usize,
) -> (DType, Endian) {
    let byte_len = end_addr - start_addr;
    if byte_len <= 1 {
        return (DType::U8, Endian::Big);
    }
    if byte_len == 2 {
        return (DType::U16, Endian::Big);
    }
    if !byte_len.is_multiple_of(2) {
        return (DType::U8, Endian::Big);
    }

    let mut deltas_8 = Vec::with_capacity(byte_len);
    for i in 0..byte_len {
        let b = base.get(start_addr + i).copied().unwrap_or(0) as f64;
        let t = target.get(start_addr + i).copied().unwrap_or(0) as f64;
        deltas_8.push(t - b);
    }

    let alternate_even_zero = deltas_8.iter().step_by(2).all(|&d| d == 0.0);
    let alternate_odd_zero = deltas_8.iter().skip(1).step_by(2).all(|&d| d == 0.0);

    if alternate_even_zero && deltas_8.iter().skip(1).step_by(2).any(|&d| d != 0.0) {
        return (DType::U16, Endian::Big);
    }
    if alternate_odd_zero && deltas_8.iter().step_by(2).any(|&d| d != 0.0) {
        return (DType::U16, Endian::Little);
    }

    let words = byte_len / 2;
    let mut deltas_16_be = Vec::with_capacity(words);
    let mut deltas_16_le = Vec::with_capacity(words);

    for w in 0..words {
        let b_hi = base.get(start_addr + 2 * w).copied().unwrap_or(0);
        let b_lo = base.get(start_addr + 2 * w + 1).copied().unwrap_or(0);
        let t_hi = target.get(start_addr + 2 * w).copied().unwrap_or(0);
        let t_lo = target.get(start_addr + 2 * w + 1).copied().unwrap_or(0);

        let b_be = u16::from_be_bytes([b_hi, b_lo]) as f64;
        let t_be = u16::from_be_bytes([t_hi, t_lo]) as f64;
        deltas_16_be.push(t_be - b_be);

        let b_le = u16::from_le_bytes([b_hi, b_lo]) as f64;
        let t_le = u16::from_le_bytes([t_hi, t_lo]) as f64;
        deltas_16_le.push(t_le - b_le);
    }

    let norm_var_8 = normalized_variance(&deltas_8);
    let norm_var_16_be = normalized_variance(&deltas_16_be);
    let norm_var_16_le = normalized_variance(&deltas_16_le);

    // If 8-bit variance is low, deltas are cleanly 8-bit
    if norm_var_8 < 0.05 {
        return (DType::U8, Endian::Big);
    }

    let min_16_var = norm_var_16_be.min(norm_var_16_le);
    if min_16_var < norm_var_8 * 0.5 {
        let endian = if norm_var_16_le < norm_var_16_be {
            Endian::Little
        } else {
            Endian::Big
        };
        (DType::U16, endian)
    } else {
        (DType::U8, Endian::Big)
    }
}

fn normalized_variance(values: &[f64]) -> f64 {
    if values.len() <= 1 {
        return 0.0;
    }
    let mean_abs = values.iter().map(|v| v.abs()).sum::<f64>() / values.len() as f64;
    if mean_abs < 1e-6 {
        return 0.0;
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let var = values.iter().map(|&v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64;
    var.sqrt() / mean_abs
}

fn classify_region(byte_len: usize, dtype: DType) -> DiffKind {
    if (1..=2).contains(&byte_len) {
        return DiffKind::Scalar;
    }

    let elem_size = dtype.byte_size();
    if !byte_len.is_multiple_of(elem_size) {
        return DiffKind::ChecksumOrRawBlock;
    }

    let element_count = byte_len / elem_size;

    struct Candidate {
        rows: usize,
        cols: usize,
        aspect_ratio: f64,
    }

    let mut candidates = Vec::new();
    for &c in &COMMON_PITCHES {
        if element_count.is_multiple_of(c) {
            let rows = element_count / c;
            if (4..=64).contains(&rows) {
                let aspect_ratio = (rows as f64).max(c as f64) / (rows as f64).min(c as f64);
                candidates.push(Candidate {
                    rows,
                    cols: c,
                    aspect_ratio,
                });
            }
        }
    }

    if !candidates.is_empty() {
        // Prioritize pitch that gives reasonable aspect ratio (square-like, closest to 1.0)
        // Tie-break preferring column pitch closest to 16
        candidates.sort_by(|a, b| {
            a.aspect_ratio
                .partial_cmp(&b.aspect_ratio)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    let dist_a = (a.cols as isize - 16).abs();
                    let dist_b = (b.cols as isize - 16).abs();
                    dist_a.cmp(&dist_b)
                })
        });

        let best = &candidates[0];
        DiffKind::Table2D {
            suggested_rows: best.rows,
            suggested_cols: best.cols,
        }
    } else if (4..=32).contains(&element_count) {
        DiffKind::Axis1D {
            length: element_count,
        }
    } else {
        DiffKind::ChecksumOrRawBlock
    }
}
