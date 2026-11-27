use crate::model::{RawTableCandidate, DEFAULT_PITCHES};
use kivi_core::types::{DType, Endian};

pub fn detect_periodic_tables(
    rom: &[u8],
    pitches: &[usize],
    min_similarity: f64,
    min_rows: usize,
) -> Vec<RawTableCandidate> {
    let candidate_pitches = if pitches.is_empty() {
        DEFAULT_PITCHES
    } else {
        pitches
    };

    let configs = [
        (DType::U8, Endian::Little),
        (DType::U16, Endian::Big),
        (DType::U16, Endian::Little),
    ];

    let mut results = Vec::new();

    for &(dtype, endian) in &configs {
        let elem_bytes = dtype.byte_size();
        let step_bytes = elem_bytes;

        for &cols in candidate_pitches {
            if cols < 4 {
                continue;
            }

            let row_bytes = cols * elem_bytes;
            let min_table_bytes = row_bytes * min_rows;
            if rom.len() < min_table_bytes {
                continue;
            }

            let mut row_0 = Vec::with_capacity(cols);
            let mut row_1 = Vec::with_capacity(cols);
            let mut next_row = Vec::with_capacity(cols);

            let mut offset = 0;
            while offset + min_table_bytes <= rom.len() {
                // Fast-skip large blocks of flat padding (0x00 or 0xFF)
                if rom[offset] == 0x00 || rom[offset] == 0xFF {
                    let fill = rom[offset];
                    let mut skip = offset + 1;
                    while skip < rom.len() && rom[skip] == fill {
                        skip += 1;
                    }
                    if skip - offset >= row_bytes {
                        let rem = skip % step_bytes;
                        offset = if rem == 0 {
                            skip
                        } else {
                            skip + (step_bytes - rem)
                        };
                        continue;
                    }
                }

                if !fill_row(rom, offset, cols, dtype, endian, &mut row_0) {
                    offset += step_bytes;
                    continue;
                }

                if row_variance(&row_0) < 1e-4
                    || (dtype == DType::U8 && is_alternating_zero(&row_0))
                {
                    offset += step_bytes;
                    continue;
                }

                if !fill_row(rom, offset + row_bytes, cols, dtype, endian, &mut row_1) {
                    offset += step_bytes;
                    continue;
                }

                if row_variance(&row_1) < 1e-4
                    || (dtype == DType::U8 && is_alternating_zero(&row_1))
                {
                    offset += step_bytes;
                    continue;
                }

                let initial_sim = match rows_compatible(&row_0, &row_1, min_similarity) {
                    Some(s) => s,
                    _ => {
                        offset += step_bytes;
                        continue;
                    }
                };

                // Contiguous rows found; continue checking successive rows
                let mut similarities = vec![initial_sim];
                let mut current_row = row_1.clone();
                let mut num_rows = 2;

                while offset + (num_rows + 1) * row_bytes <= rom.len() {
                    let next_addr = offset + num_rows * row_bytes;
                    if !fill_row(rom, next_addr, cols, dtype, endian, &mut next_row) {
                        break;
                    }

                    if dtype == DType::U8 && is_alternating_zero(&next_row) {
                        break;
                    }

                    match rows_compatible(&current_row, &next_row, min_similarity) {
                        Some(sim) => {
                            similarities.push(sim);
                            std::mem::swap(&mut current_row, &mut next_row);
                            num_rows += 1;
                        }
                        _ => break,
                    }
                }

                if num_rows >= min_rows {
                    let table_len_bytes = num_rows * row_bytes;
                    let (min_val, max_val) =
                        compute_table_min_max(rom, offset, table_len_bytes, dtype, endian);
                    let avg_sim = similarities.iter().sum::<f64>() / similarities.len() as f64;

                    results.push(RawTableCandidate {
                        start_addr: offset,
                        end_addr: offset + table_len_bytes,
                        rows: num_rows,
                        cols,
                        dtype,
                        endian,
                        avg_similarity: avg_sim,
                        min_val,
                        max_val,
                    });

                    // Skip the consumed table region
                    offset += table_len_bytes;
                } else {
                    offset += step_bytes;
                }
            }
        }
    }

    // Group by pitch (cols) and deduplicate overlapping candidates within each pitch,
    // favoring highest similarity (e.g. resolving BE vs LE for the same pitch).
    // Different pitches are preserved so scorer can pair them with matching axes.
    let mut deduplicated: Vec<RawTableCandidate> = Vec::new();
    for &cols in candidate_pitches {
        let mut pitch_candidates: Vec<RawTableCandidate> =
            results.iter().filter(|c| c.cols == cols).cloned().collect();

        pitch_candidates.sort_by(|a, b| {
            b.avg_similarity
                .partial_cmp(&a.avg_similarity)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| (b.rows * b.cols).cmp(&(a.rows * a.cols)))
                .then_with(|| a.start_addr.cmp(&b.start_addr))
        });

        let mut accepted: Vec<RawTableCandidate> = Vec::new();
        for cand in pitch_candidates {
            let overlaps = accepted
                .iter()
                .any(|acc| cand.start_addr < acc.end_addr && acc.start_addr < cand.end_addr);
            if !overlaps {
                accepted.push(cand);
            }
        }
        deduplicated.extend(accepted);
    }

    deduplicated.sort_by_key(|c| c.start_addr);
    deduplicated
}

pub fn cosine_similarity(a: &[f64], b: &[f64]) -> Option<f64> {
    if a.len() != b.len() || a.is_empty() {
        return None;
    }

    let mut dot = 0.0;
    let mut norm_a_sq = 0.0;
    let mut norm_b_sq = 0.0;

    for (&x, &y) in a.iter().zip(b.iter()) {
        dot += x * y;
        norm_a_sq += x * x;
        norm_b_sq += y * y;
    }

    if norm_a_sq < 1e-9 || norm_b_sq < 1e-9 {
        return None;
    }

    let denom = (norm_a_sq * norm_b_sq).sqrt();
    if denom < 1e-9 {
        return None;
    }

    Some((dot / denom).clamp(-1.0, 1.0))
}

pub fn pearson_correlation(a: &[f64], b: &[f64]) -> Option<f64> {
    if a.len() != b.len() || a.is_empty() {
        return None;
    }

    let mean_a = a.iter().sum::<f64>() / a.len() as f64;
    let mean_b = b.iter().sum::<f64>() / b.len() as f64;

    let mut num = 0.0;
    let mut var_a = 0.0;
    let mut var_b = 0.0;

    for (&x, &y) in a.iter().zip(b.iter()) {
        let da = x - mean_a;
        let db = y - mean_b;
        num += da * db;
        var_a += da * da;
        var_b += db * db;
    }

    let denom = (var_a * var_b).sqrt();
    if denom < 1e-9 {
        return None;
    }

    Some((num / denom).clamp(-1.0, 1.0))
}

fn rows_compatible(a: &[f64], b: &[f64], min_similarity: f64) -> Option<f64> {
    if row_variance(a) < 1e-4 || row_variance(b) < 1e-4 {
        return None;
    }
    let mean_a = a.iter().sum::<f64>() / a.len() as f64;
    let mean_b = b.iter().sum::<f64>() / b.len() as f64;
    let max_mean = mean_a.abs().max(mean_b.abs());
    let min_mean = mean_a.abs().min(mean_b.abs());
    if max_mean > 1e-6 && (min_mean / max_mean) < 0.40 {
        return None;
    }
    pearson_correlation(a, b).filter(|&sim| sim >= min_similarity)
}

pub fn row_variance(row: &[f64]) -> f64 {
    if row.len() < 2 {
        return 0.0;
    }
    let mean = row.iter().sum::<f64>() / row.len() as f64;
    let mut sum_sq = 0.0;
    for &x in row {
        let diff = x - mean;
        sum_sq += diff * diff;
    }
    sum_sq / row.len() as f64
}

fn fill_row(
    rom: &[u8],
    addr: usize,
    cols: usize,
    dtype: DType,
    endian: Endian,
    buf: &mut Vec<f64>,
) -> bool {
    let elem_size = dtype.byte_size();
    let row_bytes = cols * elem_size;
    if addr + row_bytes > rom.len() {
        return false;
    }

    let slice = &rom[addr..addr + row_bytes];
    buf.clear();

    match (dtype, endian) {
        (DType::U8, _) => {
            for &b in slice {
                buf.push(b as f64);
            }
        }
        (DType::U16, Endian::Big) => {
            for chunk in slice.as_chunks::<2>().0 {
                buf.push(u16::from_be_bytes(*chunk) as f64);
            }
        }
        (DType::U16, Endian::Little) => {
            for chunk in slice.as_chunks::<2>().0 {
                buf.push(u16::from_le_bytes(*chunk) as f64);
            }
        }
        _ => return false,
    }

    true
}

fn compute_table_min_max(
    rom: &[u8],
    start: usize,
    len_bytes: usize,
    dtype: DType,
    endian: Endian,
) -> (f64, f64) {
    let slice = &rom[start..start + len_bytes];
    let mut min_val = f64::INFINITY;
    let mut max_val = f64::NEG_INFINITY;

    match (dtype, endian) {
        (DType::U8, _) => {
            for &b in slice {
                let v = b as f64;
                if v < min_val {
                    min_val = v;
                }
                if v > max_val {
                    max_val = v;
                }
            }
        }
        (DType::U16, Endian::Big) => {
            for chunk in slice.as_chunks::<2>().0 {
                let v = u16::from_be_bytes(*chunk) as f64;
                if v < min_val {
                    min_val = v;
                }
                if v > max_val {
                    max_val = v;
                }
            }
        }
        (DType::U16, Endian::Little) => {
            for chunk in slice.as_chunks::<2>().0 {
                let v = u16::from_le_bytes(*chunk) as f64;
                if v < min_val {
                    min_val = v;
                }
                if v > max_val {
                    max_val = v;
                }
            }
        }
        _ => {}
    }

    if min_val.is_infinite() {
        (0.0, 0.0)
    } else {
        (min_val, max_val)
    }
}

fn is_alternating_zero(row: &[f64]) -> bool {
    if row.len() < 4 {
        return false;
    }
    let even_zeros = row.iter().step_by(2).filter(|&&x| x == 0.0).count();
    let odd_zeros = row.iter().skip(1).step_by(2).filter(|&&x| x == 0.0).count();
    let half = row.len() / 2;
    even_zeros >= half.saturating_sub(1) || odd_zeros >= half.saturating_sub(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_periodic_table_synthetic() {
        let rows = 16;
        let cols = 16;
        let mut rom = vec![0u8; 100 + rows * cols * 2 + 100];
        let table_start = 100;

        for r in 0..rows {
            for c in 0..cols {
                let offset = table_start + (r * cols + c) * 2;
                let val = (1000.0
                    + 5000.0
                        * (-(((r as f64 - 8.0).powi(2) + (c as f64 - 8.0).powi(2)) / 32.0)).exp())
                    as u16;
                let bytes = val.to_be_bytes();
                rom[offset] = bytes[0];
                rom[offset + 1] = bytes[1];
            }
        }

        let tables = detect_periodic_tables(&rom, &[16], 0.80, 6);
        assert!(!tables.is_empty());
        let table = &tables[0];
        assert_eq!(table.start_addr, table_start);
        assert_eq!(table.rows, 16);
        assert_eq!(table.cols, 16);
        assert_eq!(table.dtype, DType::U16);
        assert_eq!(table.endian, Endian::Big);
        assert!(table.avg_similarity > 0.90);
    }
}
