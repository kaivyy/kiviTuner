use super::types::{TargetAfrSource, TunableParameterType};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Error conditions in Auto-Tune configuration.
#[derive(Error, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AutoTuneConfigError {
    #[error("Target AFR {value} is outside plausible combustion limits [8.0, 22.0]")]
    InvalidTargetAfr { value: f64 },
    #[error("Target AFR table dimensions ({rows}x{cols}) do not match table ({expected_rows}x{expected_cols})")]
    TargetAfrDimensionMismatch {
        rows: usize,
        cols: usize,
        expected_rows: usize,
        expected_cols: usize,
    },
    #[error("Maximum delta percentage must be positive, got {value}%")]
    InvalidMaxDelta { value: f64 },
    #[error("Minimum samples per cell must be at least 1, got {value}")]
    InvalidMinSamples { value: usize },
}

/// Operational configuration for statistical auto-tune analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutoTuneConfig {
    /// Source of target AFR values
    pub target_afr: TargetAfrSource,
    /// Parameter type semantics of the table being tuned
    pub parameter_type: TunableParameterType,
    /// Minimum engine coolant temperature (°C) to reject cold-start warmup enrichment
    pub min_ect_c: f32,
    /// Maximum throttle rate-of-change (%/s) to reject transient accel/decel enrichment
    pub max_tps_delta_per_sec: f32,
    /// Minimum engine speed (RPM) for valid tuning zone
    pub min_rpm: f32,
    /// Maximum engine speed (RPM) for valid tuning zone
    pub max_rpm: f32,
    /// Minimum wideband AFR sensor reading
    pub min_afr: f32,
    /// Maximum wideband AFR sensor reading
    pub max_afr: f32,
    /// Minimum eligible samples required in a cell before recommending a change
    pub min_samples_per_cell: usize,
    /// Maximum allowable single-iteration correction clamp (± percentage, e.g. 15.0%)
    pub max_cell_delta_pct: f64,
    /// Maximum acceptable standard deviation of AFR samples within a cell before warning
    pub max_afr_std_dev: f64,
}

impl Default for AutoTuneConfig {
    fn default() -> Self {
        Self {
            target_afr: TargetAfrSource::Constant(14.7),
            parameter_type: TunableParameterType::AutoDetect,
            min_ect_c: 70.0,
            max_tps_delta_per_sec: 15.0,
            min_rpm: 800.0,
            max_rpm: 9000.0,
            min_afr: 10.0,
            max_afr: 20.0,
            min_samples_per_cell: 5,
            max_cell_delta_pct: 15.0,
            max_afr_std_dev: 1.5,
        }
    }
}

impl AutoTuneConfig {
    /// Validates the configuration parameters.
    pub fn validate(&self, expected_rows: usize, expected_cols: usize) -> Result<(), AutoTuneConfigError> {
        match &self.target_afr {
            TargetAfrSource::Constant(val) => {
                if !val.is_finite() || *val < 8.0 || *val > 22.0 {
                    return Err(AutoTuneConfigError::InvalidTargetAfr { value: *val });
                }
            }
            TargetAfrSource::Table(matrix) => {
                if matrix.len() != expected_rows {
                    return Err(AutoTuneConfigError::TargetAfrDimensionMismatch {
                        rows: matrix.len(),
                        cols: matrix.first().map(|r| r.len()).unwrap_or(0),
                        expected_rows,
                        expected_cols,
                    });
                }
                for row in matrix {
                    if row.len() != expected_cols {
                        return Err(AutoTuneConfigError::TargetAfrDimensionMismatch {
                            rows: matrix.len(),
                            cols: row.len(),
                            expected_rows,
                            expected_cols,
                        });
                    }
                    for &val in row {
                        if !val.is_finite() || val < 8.0 || val > 22.0 {
                            return Err(AutoTuneConfigError::InvalidTargetAfr { value: val });
                        }
                    }
                }
            }
        }

        if !self.max_cell_delta_pct.is_finite() || self.max_cell_delta_pct <= 0.0 {
            return Err(AutoTuneConfigError::InvalidMaxDelta {
                value: self.max_cell_delta_pct,
            });
        }

        if self.min_samples_per_cell == 0 {
            return Err(AutoTuneConfigError::InvalidMinSamples {
                value: self.min_samples_per_cell,
            });
        }

        Ok(())
    }
}
