use super::aggregator::CellAggregator;
use super::config::{AutoTuneConfig, AutoTuneConfigError};
use super::filter::SampleFilter;
use super::types::{
    CellRecommendation, CellRecommendationStatus, RecommendationReport, RejectionReason,
    RejectionSummary, TargetAfrSource, TunableParameterType,
};
use crate::correlation::correlate_frame;
use crate::model::TelemetryFrame;
use kivi_core::memory_map::MemoryMap;
use kivi_defs::expr::evaluate_scaling;
use kivi_defs::model::MapDef;

/// Pure, deterministic recommendation engine for table auto-tuning.
pub struct AutoTuneEngine;

impl AutoTuneEngine {
    /// Computes statistical auto-tune recommendations across a calibration map.
    ///
    /// Invariants:
    /// - Pure analysis: zero ROM mutations, zero project-history edits.
    /// - Deterministic: identical inputs produce identical bit-for-bit results.
    /// - Rejects all stale, faulted, non-finite, out-of-range, and cold engine samples.
    pub fn compute(
        frames: &[TelemetryFrame],
        map_def: &MapDef,
        mem_opt: Option<&MemoryMap>,
        config: &AutoTuneConfig,
    ) -> RecommendationReport {
        // 1. Verify parameter type compatibility
        if let Err(reason) = Self::verify_parameter_compatibility(map_def, &config.parameter_type) {
            return RecommendationReport::blocked(map_def.id.clone(), reason);
        }

        // 2. Validate configuration
        if let Err(err) = config.validate(map_def.rows, map_def.cols) {
            let reason = match err {
                AutoTuneConfigError::InvalidTargetAfr { value } => {
                    format!("Invalid target AFR: {}", value)
                }
                AutoTuneConfigError::TargetAfrDimensionMismatch { .. } => {
                    "Target AFR table dimensions do not match table".to_string()
                }
                AutoTuneConfigError::InvalidMaxDelta { value } => {
                    format!("Invalid max delta: {}", value)
                }
                AutoTuneConfigError::InvalidMinSamples { value } => {
                    format!("Invalid min samples: {}", value)
                }
            };
            return RecommendationReport::blocked(map_def.id.clone(), reason);
        }

        // 3. Extract baseline values and target AFR per cell
        let total_cells = map_def.rows * map_def.cols;
        let mut original_values = Vec::with_capacity(total_cells);
        let mut target_afrs = Vec::with_capacity(total_cells);

        for r in 0..map_def.rows {
            for c in 0..map_def.cols {
                let orig = Self::read_cell_value(map_def, mem_opt, r, c);
                original_values.push(orig);

                let target = match &config.target_afr {
                    TargetAfrSource::Constant(val) => *val,
                    TargetAfrSource::Table(matrix) => matrix[r][c],
                };
                target_afrs.push(target);
            }
        }

        let mut aggregator =
            CellAggregator::new(map_def.rows, map_def.cols, &original_values, &target_afrs);
        let mut filter = SampleFilter::new();
        let mut rejections = RejectionSummary::default();
        let mut eligible_frames_used = 0;

        // 4. Ingest and aggregate eligible evidence
        for frame in frames {
            // First pass: sensor, thermal, throttle, and status eligibility
            let weight = match filter.evaluate_frame(frame, config) {
                Ok(w) => w,
                Err(reason) => {
                    match reason {
                        RejectionReason::Stale => rejections.rejected_stale += 1,
                        RejectionReason::Fault => rejections.rejected_fault += 1,
                        RejectionReason::OutOfBounds => rejections.rejected_out_of_bounds += 1,
                        RejectionReason::ColdEngine => rejections.rejected_cold_engine += 1,
                        RejectionReason::FuelCut => rejections.rejected_fuel_cut += 1,
                        RejectionReason::TransientThrottle => {
                            rejections.rejected_transient_throttle += 1
                        }
                        RejectionReason::SensorOutOfRange => {
                            rejections.rejected_sensor_out_of_range += 1
                        }
                        RejectionReason::NonFinite => rejections.rejected_non_finite += 1,
                    }
                    continue;
                }
            };

            // Second pass: correlate frame to table coordinate
            let corr = correlate_frame(frame, map_def, mem_opt, None);
            if !corr.is_valid {
                rejections.rejected_out_of_bounds += 1;
                continue;
            }

            if let (Some(r), Some(c)) = (corr.selected_row, corr.selected_col) {
                aggregator.record_sample(r, c, frame.afr as f64, weight);
                eligible_frames_used += 1;
            }
        }

        // 5. Generate cell recommendations
        let mult = 10f64.powi(map_def.scaling.decimals as i32);
        let max_delta = config.max_cell_delta_pct / 100.0;
        let accumulators = aggregator.into_accumulators();
        let mut cells = Vec::with_capacity(accumulators.len());

        for acc in accumulators {
            let r = acc.row;
            let c = acc.col;
            let orig = acc.original_value;
            let target = acc.target_afr;
            let samples = acc.sample_count;
            let effective_samples = acc.effective_sample_count;
            let std_dev = acc.std_dev_afr();
            let mean_afr = acc.mean_measured_afr();

            let (status, proposed, delta_pct, is_clamped, warning) = if samples
                < config.min_samples_per_cell
            {
                (
                    CellRecommendationStatus::InsufficientEvidence,
                    orig,
                    0.0,
                    false,
                    Some(format!(
                        "Insufficient samples ({} < min {})",
                        samples, config.min_samples_per_cell
                    )),
                )
            } else if let Some(meas_afr) = mean_afr {
                let factor = meas_afr / target;
                let raw_proposed = orig * factor;
                let raw_delta = (factor - 1.0) * 100.0;

                let b1 = orig * (1.0 - max_delta);
                let b2 = orig * (1.0 + max_delta);
                let lower_bound = b1.min(b2);
                let upper_bound = b1.max(b2);

                let (clamped_proposed, clamped, delta_calc) = if orig == 0.0 {
                    (orig, false, 0.0)
                } else if raw_proposed < lower_bound {
                    (
                        lower_bound,
                        true,
                        ((lower_bound / orig - 1.0) * 100.0 * 100.0).round() / 100.0,
                    )
                } else if raw_proposed > upper_bound {
                    (
                        upper_bound,
                        true,
                        ((upper_bound / orig - 1.0) * 100.0 * 100.0).round() / 100.0,
                    )
                } else {
                    (raw_proposed, false, raw_delta)
                };

                let rounded_proposed = (clamped_proposed * mult).round() / mult;
                let final_delta = if orig.abs() < f64::EPSILON {
                    0.0
                } else {
                    ((rounded_proposed / orig - 1.0) * 100.0 * 100.0).round() / 100.0
                };

                if std_dev > config.max_afr_std_dev {
                    (
                        CellRecommendationStatus::HighVarianceWarning,
                        rounded_proposed,
                        if clamped { delta_calc } else { final_delta },
                        clamped,
                        Some(format!(
                            "High AFR variance (std dev {:.2} > max {:.2})",
                            std_dev, config.max_afr_std_dev
                        )),
                    )
                } else if (!clamped && final_delta.abs() < 0.2) || (orig == 0.0) {
                    (
                        CellRecommendationStatus::NoChange,
                        orig,
                        0.0,
                        false,
                        None,
                    )
                } else {
                    (
                        CellRecommendationStatus::Ready,
                        rounded_proposed,
                        if clamped { delta_calc } else { final_delta },
                        clamped,
                        None,
                    )
                }
            } else {
                (
                    CellRecommendationStatus::InsufficientEvidence,
                    orig,
                    0.0,
                    false,
                    Some("No valid AFR samples".to_string()),
                )
            };

            cells.push(CellRecommendation {
                row: r,
                col: c,
                status,
                sample_count: samples,
                effective_sample_count: effective_samples,
                original_value: orig,
                proposed_value: proposed,
                delta_percentage: delta_pct,
                unit: map_def.scaling.unit.clone(),
                mean_measured_afr: mean_afr,
                target_afr: Some(target),
                std_dev_afr: std_dev,
                min_measured_afr: if samples > 0 {
                    Some(acc.min_afr)
                } else {
                    None
                },
                max_measured_afr: if samples > 0 {
                    Some(acc.max_afr)
                } else {
                    None
                },
                is_clamped,
                warning,
            });
        }

        RecommendationReport {
            table_id: map_def.id.clone(),
            total_frames_scanned: frames.len(),
            eligible_frames_used,
            cells,
            rejections,
            blocker_reason: None,
        }
    }

    /// Verifies that the table represents a parameter suitable for AFR auto-tuning.
    fn verify_parameter_compatibility(
        map_def: &MapDef,
        param_type: &TunableParameterType,
    ) -> Result<(), String> {
        match param_type {
            TunableParameterType::VolumetricEfficiency | TunableParameterType::InjectionTimeMs => {
                Ok(())
            }
            TunableParameterType::AutoDetect => {
                let cat = map_def.category.to_lowercase();
                let unit = map_def.scaling.unit.to_lowercase();
                let name = map_def.name.to_lowercase();

                // Explicit non-fuel categories must be rejected
                if cat.contains("ignition")
                    || cat.contains("spark")
                    || cat.contains("timing")
                    || unit == "deg"
                    || unit == "°"
                    || unit == "afr"
                    || unit == "lambda"
                    || (name.contains("target") && name.contains("afr"))
                    || cat.contains("boost")
                    || cat.contains("knock")
                {
                    return Err(
                        "Incompatible table: table category/unit is not a recognized fuel parameter"
                            .to_string(),
                    );
                }

                // Check positive fuel indications
                if cat.contains("fuel")
                    || cat.contains("ve")
                    || unit == "%"
                    || unit == "ve"
                    || unit == "ms"
                    || name.contains("fuel")
                    || name.contains("ve")
                {
                    Ok(())
                } else {
                    Err(
                        "Incompatible table: table category/unit is not a recognized fuel parameter"
                            .to_string(),
                    )
                }
            }
            TunableParameterType::IgnitionAdvance => Err(
                "Incompatible table: Ignition advance cannot be tuned via AFR correction formula"
                    .to_string(),
            ),
            TunableParameterType::TargetAfrTable => Err(
                "Incompatible table: Target AFR table is a reference target, not a tunable delivery map"
                    .to_string(),
            ),
            TunableParameterType::Unsupported(reason) => {
                Err(format!("Incompatible table: {}", reason))
            }
        }
    }

    /// Reads cell value from ROM or defaults to 0.0.
    fn read_cell_value(
        map_def: &MapDef,
        mem_opt: Option<&MemoryMap>,
        row: usize,
        col: usize,
    ) -> f64 {
        if let Some(mem) = mem_opt {
            let bytes_per_elem = map_def.dtype.byte_size();
            let idx = if map_def.column_major {
                col * map_def.rows + row
            } else {
                row * map_def.cols + col
            };
            let offset = map_def.address + idx * bytes_per_elem;
            if let Ok(raw) = mem.read_value(offset, map_def.dtype, map_def.endian) {
                return evaluate_scaling(&map_def.scaling.equation, raw).unwrap_or(raw);
            }
        }
        0.0
    }
}

/// Convenience function to compute recommendations.
pub fn compute_recommendations(
    frames: &[TelemetryFrame],
    map_def: &MapDef,
    mem_opt: Option<&MemoryMap>,
    config: &AutoTuneConfig,
) -> RecommendationReport {
    AutoTuneEngine::compute(frames, map_def, mem_opt, config)
}
