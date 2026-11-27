use crate::model::{AxisCandidate, DetectedMapCandidate, RawTableCandidate, ScannerSensitivity};
use kivi_core::types::DType;

pub fn score_and_pair(
    tables: Vec<RawTableCandidate>,
    axes: &[AxisCandidate],
    sensitivity: ScannerSensitivity,
) -> Vec<DetectedMapCandidate> {
    score_and_pair_with_min_confidence(tables, axes, sensitivity.min_confidence())
}

pub fn score_and_pair_with_min_confidence(
    tables: Vec<RawTableCandidate>,
    axes: &[AxisCandidate],
    min_confidence: u8,
) -> Vec<DetectedMapCandidate> {
    let mut candidates = Vec::new();

    for raw_table in &tables {
        let mut table = raw_table.clone();
        let row_bytes = table.cols * table.dtype.byte_size();

        // Check if leading rows are actually axes merged into the table
        let mut x_axis: Option<&AxisCandidate> = None;
        let mut y_axis: Option<&AxisCandidate> = None;

        let lead1 = axes.iter().find(|a| {
            a.start_addr == table.start_addr
                && a.length == table.cols
                && is_axis_compatible(a, &table)
        });

        if let Some(a1) = lead1 {
            let lead2 = axes.iter().find(|a| {
                a.start_addr == table.start_addr + row_bytes
                    && a.length == table.cols
                    && is_axis_compatible(a, &table)
            });

            if let Some(a2) = lead2 {
                if table.rows > 2 {
                    table.start_addr += 2 * row_bytes;
                    table.rows -= 2;
                    x_axis = Some(a1);
                    y_axis = Some(a2);
                }
            } else if table.rows > 1 {
                table.start_addr += row_bytes;
                table.rows -= 1;
                x_axis = Some(a1);
            }
        }

        // Search within +/- 1024 bytes of table range
        let min_search = table.start_addr.saturating_sub(1024);
        let max_search = table.end_addr + 1024;

        let mut nearby_axes: Vec<&AxisCandidate> = axes
            .iter()
            .filter(|a| a.start_addr >= min_search && a.start_addr <= max_search)
            .collect();
        nearby_axes.sort_by_key(|a| a.start_addr);

        // Find X axis if not already found from leading rows
        if x_axis.is_none() {
            for &axis in &nearby_axes {
                if axis.length == table.cols && is_axis_compatible(axis, &table) {
                    x_axis = Some(axis);
                    break;
                }
            }
        }

        // Find Y axis if not already found from leading rows
        if y_axis.is_none() {
            for &axis in &nearby_axes {
                if axis.length == table.rows
                    && is_axis_compatible(axis, &table)
                    && x_axis.is_none_or(|x| x.start_addr != axis.start_addr)
                {
                    y_axis = Some(axis);
                    break;
                }
            }
        }

        // Base confidence from row-to-row similarity (30..=50 points)
        let base_confidence = if table.avg_similarity >= 0.80 {
            30.0 + ((table.avg_similarity - 0.80) / 0.20).clamp(0.0, 1.0) * 20.0
        } else {
            (table.avg_similarity / 0.80 * 30.0).clamp(0.0, 30.0)
        };

        let mut axis_bonus = 0.0;
        if x_axis.is_some() {
            axis_bonus += 20.0;
        }
        if y_axis.is_some() {
            axis_bonus += 20.0;
        }

        let total_conf = (base_confidence + axis_bonus).round().clamp(0.0, 100.0) as u8;
        if total_conf < min_confidence {
            continue;
        }

        // Categorize hint
        let (category_hint, name_hint) = categorize_table(&table, x_axis, y_axis);

        let candidate = DetectedMapCandidate {
            id: format!("map_0x{:X}", table.start_addr),
            name_hint,
            category_hint,
            start_addr: table.start_addr,
            end_addr: table.end_addr,
            rows: table.rows,
            cols: table.cols,
            dtype: table.dtype,
            endian: table.endian,
            confidence: total_conf,
            x_axis_addr: x_axis.map(|a| a.start_addr),
            y_axis_addr: y_axis.map(|a| a.start_addr),
        };

        candidates.push(candidate);
    }

    // Deduplicate overlapping detected tables by highest confidence, then size
    candidates.sort_by(|a, b| {
        b.confidence
            .cmp(&a.confidence)
            .then_with(|| (b.rows * b.cols).cmp(&(a.rows * a.cols)))
            .then_with(|| a.start_addr.cmp(&b.start_addr))
    });

    let mut accepted: Vec<DetectedMapCandidate> = Vec::new();
    for cand in candidates {
        let overlaps = accepted
            .iter()
            .any(|acc| cand.start_addr < acc.end_addr && acc.start_addr < cand.end_addr);

        if !overlaps {
            accepted.push(cand);
        }
    }

    accepted.sort_by_key(|c| c.start_addr);
    accepted
}

fn is_axis_compatible(axis: &AxisCandidate, table: &RawTableCandidate) -> bool {
    if axis.dtype != table.dtype {
        return false;
    }
    if axis.dtype == DType::U8 {
        true
    } else {
        axis.endian == table.endian
    }
}

fn categorize_table(
    table: &RawTableCandidate,
    x_axis: Option<&AxisCandidate>,
    y_axis: Option<&AxisCandidate>,
) -> (String, String) {
    let has_rpm_axis = [x_axis, y_axis].into_iter().flatten().any(is_rpm_axis);
    let has_load_axis = [x_axis, y_axis].into_iter().flatten().any(is_load_axis);

    // Ignition timing check: values typically 5-55 deg, accompanied by RPM or load
    let looks_like_timing = table.min_val >= 0.0
        && table.max_val <= 65.0
        && table.min_val <= 25.0
        && table.max_val >= 15.0;

    if looks_like_timing && (has_rpm_axis || has_load_axis) {
        return (
            "Ignition".to_string(),
            format!("Ignition Timing ({}x{})", table.rows, table.cols),
        );
    }

    // Fuel VE check: 0-255 (U8) or 0-100 or standard 16-bit VE
    let looks_like_fuel_u8 = table.dtype == DType::U8 && table.max_val > 30.0;
    let looks_like_fuel_u16 = table.dtype == DType::U16
        && ((table.min_val >= 0.0 && table.max_val <= 255.0 && table.max_val > 40.0)
            || (table.min_val >= 100.0 && table.max_val <= 15000.0));

    if looks_like_fuel_u8 || looks_like_fuel_u16 {
        return (
            "Fuel".to_string(),
            format!("Fuel VE Table ({}x{})", table.rows, table.cols),
        );
    }

    (
        "Unknown".to_string(),
        format!("Map {}x{}", table.rows, table.cols),
    )
}

fn is_rpm_axis(axis: &AxisCandidate) -> bool {
    axis.max_val >= 3000.0 && axis.max_val <= 18000.0
}

fn is_load_axis(axis: &AxisCandidate) -> bool {
    (axis.min_val >= 0.0 && axis.max_val <= 100.0)
        || (axis.min_val >= 10.0 && axis.max_val <= 400.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kivi_core::types::Endian;

    #[test]
    fn test_score_and_pair_strict_and_balanced() {
        let raw = RawTableCandidate {
            start_addr: 0x1040,
            end_addr: 0x1240,
            rows: 16,
            cols: 16,
            dtype: DType::U16,
            endian: Endian::Big,
            avg_similarity: 0.98,
            min_val: 1000.0,
            max_val: 6000.0,
        };

        let x_axis = AxisCandidate {
            start_addr: 0x1000,
            length: 16,
            dtype: DType::U16,
            endian: Endian::Big,
            min_val: 1000.0,
            max_val: 8000.0,
            step_variance: 0.0,
        };

        let y_axis = AxisCandidate {
            start_addr: 0x1020,
            length: 16,
            dtype: DType::U16,
            endian: Endian::Big,
            min_val: 0.0,
            max_val: 100.0,
            step_variance: 0.0,
        };

        let strict_results = score_and_pair(
            vec![raw.clone()],
            &[x_axis.clone(), y_axis.clone()],
            ScannerSensitivity::Strict,
        );
        assert_eq!(strict_results.len(), 1);
        assert!(strict_results[0].confidence >= 85);
        assert_eq!(strict_results[0].category_hint, "Fuel");
        assert_eq!(strict_results[0].x_axis_addr, Some(0x1000));
        assert_eq!(strict_results[0].y_axis_addr, Some(0x1020));
    }
}
