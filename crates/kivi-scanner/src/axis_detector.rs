use crate::model::AxisCandidate;
use kivi_core::types::{DType, Endian};

const STANDARD_LENGTHS: &[usize] = &[32, 24, 20, 19, 16, 15, 12, 10, 8];

pub fn detect_axes(rom: &[u8]) -> Vec<AxisCandidate> {
    detect_axes_with_options(rom, 8, 32, 2.0)
}

pub fn detect_axes_with_options(
    rom: &[u8],
    min_len: usize,
    max_len: usize,
    max_step_var_ratio: f64,
) -> Vec<AxisCandidate> {
    if rom.len() < min_len {
        return Vec::new();
    }

    let configs = [
        (DType::U8, Endian::Big),
        (DType::U16, Endian::Big),
        (DType::U16, Endian::Little),
    ];

    let mut raw_candidates: Vec<(AxisCandidate, f64)> = Vec::new();

    for &(dtype, endian) in &configs {
        let elem_size = dtype.byte_size();
        let num_alignments = elem_size;

        for offset_start in 0..num_alignments {
            let total_elems = (rom.len().saturating_sub(offset_start)) / elem_size;
            if total_elems < min_len {
                continue;
            }

            // Extract sequence of values
            let mut values = Vec::with_capacity(total_elems);
            for i in 0..total_elems {
                let addr = offset_start + i * elem_size;
                let val = match (dtype, endian) {
                    (DType::U8, _) => rom[addr] as f64,
                    (DType::U16, Endian::Big) => {
                        u16::from_be_bytes([rom[addr], rom[addr + 1]]) as f64
                    }
                    (DType::U16, Endian::Little) => {
                        u16::from_le_bytes([rom[addr], rom[addr + 1]]) as f64
                    }
                    _ => continue,
                };
                values.push(val);
            }

            // Find contiguous strictly monotonic increasing runs
            let mut run_start = 0;
            while run_start < values.len() {
                let mut run_end = run_start + 1;
                while run_end < values.len() && values[run_end] > values[run_end - 1] {
                    run_end += 1;
                }

                let run_len = run_end - run_start;
                if run_len >= min_len {
                    // Check full run if length is within [min_len, max_len]
                    if run_len <= max_len {
                        if let Some((var, ratio)) =
                            compute_step_variance_and_ratio(&values[run_start..run_end])
                        {
                            if ratio <= max_step_var_ratio {
                                raw_candidates.push((
                                    AxisCandidate {
                                        start_addr: offset_start + run_start * elem_size,
                                        length: run_len,
                                        dtype,
                                        endian,
                                        min_val: values[run_start],
                                        max_val: values[run_end - 1],
                                        step_variance: var,
                                    },
                                    ratio,
                                ));
                            }
                        }
                    }

                    // Also check standard ECU axis sizes within this run
                    for &std_len in STANDARD_LENGTHS {
                        if std_len >= min_len && std_len <= max_len && std_len <= run_len {
                            // Check prefix
                            if let Some((var, ratio)) = compute_step_variance_and_ratio(
                                &values[run_start..run_start + std_len],
                            ) {
                                if ratio <= max_step_var_ratio {
                                    raw_candidates.push((
                                        AxisCandidate {
                                            start_addr: offset_start + run_start * elem_size,
                                            length: std_len,
                                            dtype,
                                            endian,
                                            min_val: values[run_start],
                                            max_val: values[run_start + std_len - 1],
                                            step_variance: var,
                                        },
                                        ratio,
                                    ));
                                }
                            }

                            // Check suffix if different from prefix
                            let suffix_start = run_end - std_len;
                            if suffix_start > run_start {
                                if let Some((var, ratio)) =
                                    compute_step_variance_and_ratio(&values[suffix_start..run_end])
                                {
                                    if ratio <= max_step_var_ratio {
                                        raw_candidates.push((
                                            AxisCandidate {
                                                start_addr: offset_start + suffix_start * elem_size,
                                                length: std_len,
                                                dtype,
                                                endian,
                                                min_val: values[suffix_start],
                                                max_val: values[run_end - 1],
                                                step_variance: var,
                                            },
                                            ratio,
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }

                run_start = run_end;
            }
        }
    }

    // Sort to favor standard ECU axis lengths, natural alignment, longer sequences, then lowest normalized step variance
    raw_candidates.sort_by(|a, b| {
        let is_std_len_a = STANDARD_LENGTHS.contains(&a.0.length);
        let is_std_len_b = STANDARD_LENGTHS.contains(&b.0.length);
        let is_aligned_a = a.0.start_addr % a.0.dtype.byte_size() == 0;
        let is_aligned_b = b.0.start_addr % b.0.dtype.byte_size() == 0;

        is_std_len_b
            .cmp(&is_std_len_a)
            .then_with(|| is_aligned_b.cmp(&is_aligned_a))
            .then_with(|| b.0.length.cmp(&a.0.length))
            .then_with(|| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .then_with(|| a.0.start_addr.cmp(&b.0.start_addr))
    });

    // Deduplicate overlapping candidate ranges in O(rom.len()) via byte occupancy.
    let mut occupied = vec![false; rom.len()];
    let mut accepted: Vec<AxisCandidate> = Vec::new();

    for (cand, _) in raw_candidates {
        let cand_start = cand.start_addr;
        let cand_end = cand.start_addr + cand.length * cand.dtype.byte_size();

        if cand_start >= cand_end || cand_end > occupied.len() {
            continue;
        }
        if !occupied[cand_start..cand_end].iter().any(|&b| b) {
            occupied[cand_start..cand_end].fill(true);
            accepted.push(cand);
        }
    }

    accepted.sort_by_key(|c| c.start_addr);
    accepted
}

fn compute_step_variance_and_ratio(values: &[f64]) -> Option<(f64, f64)> {
    if values.len() < 2 {
        return None;
    }

    let m = (values.len() - 1) as f64;
    let mut steps = Vec::with_capacity(values.len() - 1);
    let mut sum_steps = 0.0;

    for i in 0..(values.len() - 1) {
        let step = values[i + 1] - values[i];
        if step <= 0.0 {
            return None;
        }
        steps.push(step);
        sum_steps += step;
    }

    let mean_step = sum_steps / m;
    let mut sum_sq_diff = 0.0;
    for &step in &steps {
        let diff = step - mean_step;
        sum_sq_diff += diff * diff;
    }

    let variance = sum_sq_diff / m;
    let ratio = variance / (mean_step * mean_step + 1e-6);

    Some((variance, ratio))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_axes_synthetic_u16_be() {
        let mut rom = vec![0u8; 100];
        // RPM Axis: 1000..8500 step 500 (16 values)
        for (i, val) in (1000u16..=8500).step_by(500).enumerate() {
            let offset = 20 + i * 2;
            let bytes = val.to_be_bytes();
            rom[offset] = bytes[0];
            rom[offset + 1] = bytes[1];
        }

        let axes = detect_axes(&rom);
        assert_eq!(axes.len(), 1);
        let rpm = &axes[0];
        assert_eq!(rpm.start_addr, 20);
        assert_eq!(rpm.length, 16);
        assert_eq!(rpm.dtype, DType::U16);
        assert_eq!(rpm.endian, Endian::Big);
        assert_eq!(rpm.min_val, 1000.0);
        assert_eq!(rpm.max_val, 8500.0);
        assert!(rpm.step_variance < 1e-4);
    }
}
