use serde::{Deserialize, Serialize};

/// Identifies the physical quantity represented by a calibration map.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "type", content = "details")]
pub enum TunableParameterType {
    /// Inspects `MapDef` category, unit, and name to determine applicability
    #[default]
    AutoDetect,
    /// Speed-density Volumetric Efficiency table (%)
    VolumetricEfficiency,
    /// Raw fuel injector pulse width (ms)
    InjectionTimeMs,
    /// Ignition advance table (degrees) - Incompatible with AFR auto-tune
    IgnitionAdvance,
    /// Target AFR lookup table - Incompatible (this is the reference target, not delivery)
    TargetAfrTable,
    /// Custom unsupported or unknown parameter type
    Unsupported(String),
}

/// Source of Target Air-Fuel Ratio values for recommendation calculation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "source", content = "data")]
pub enum TargetAfrSource {
    /// Global constant target AFR (e.g. 14.7:1 for stoichiometric cruise)
    Constant(f64),
    /// 2D target matrix indexed by [row][col]
    Table(Vec<Vec<f64>>),
}

impl Default for TargetAfrSource {
    fn default() -> Self {
        Self::Constant(14.7)
    }
}

/// Status of recommendation for an individual table cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CellRecommendationStatus {
    /// Valid statistical evidence available, recommended change ready for review
    Ready,
    /// Evidence is sufficient, but measured AFR matches target within tolerance (< 0.2%)
    NoChange,
    /// Sample count is below configured minimum confidence threshold
    InsufficientEvidence,
    /// High sample variance / wide AFR standard deviation indicates noisy conditions
    HighVarianceWarning,
}

/// Structured recommendation for an individual cell in a calibration map.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CellRecommendation {
    pub row: usize,
    pub col: usize,
    pub status: CellRecommendationStatus,
    pub sample_count: usize,
    pub effective_sample_count: f64,
    pub original_value: f64,
    pub proposed_value: f64,
    pub delta_percentage: f64,
    pub unit: String,
    pub mean_measured_afr: Option<f64>,
    pub target_afr: Option<f64>,
    pub std_dev_afr: f64,
    pub min_measured_afr: Option<f64>,
    pub max_measured_afr: Option<f64>,
    pub is_clamped: bool,
    pub warning: Option<String>,
}

/// Breakdown of frames rejected by the eligibility filters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RejectionSummary {
    pub rejected_stale: usize,
    pub rejected_fault: usize,
    pub rejected_out_of_bounds: usize,
    pub rejected_cold_engine: usize,
    pub rejected_fuel_cut: usize,
    pub rejected_transient_throttle: usize,
    pub rejected_sensor_out_of_range: usize,
    pub rejected_non_finite: usize,
}

impl RejectionSummary {
    pub fn total(&self) -> usize {
        self.rejected_stale
            + self.rejected_fault
            + self.rejected_out_of_bounds
            + self.rejected_cold_engine
            + self.rejected_fuel_cut
            + self.rejected_transient_throttle
            + self.rejected_sensor_out_of_range
            + self.rejected_non_finite
    }
}

/// Specific reason why an individual telemetry sample was rejected from auto-tune evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectionReason {
    Stale,
    Fault,
    OutOfBounds,
    ColdEngine,
    FuelCut,
    TransientThrottle,
    SensorOutOfRange,
    NonFinite,
}

/// Complete auto-tune recommendation report across an entire calibration map.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecommendationReport {
    pub table_id: String,
    pub total_frames_scanned: usize,
    pub eligible_frames_used: usize,
    pub cells: Vec<CellRecommendation>,
    pub rejections: RejectionSummary,
    pub blocker_reason: Option<String>,
}

impl RecommendationReport {
    /// Creates a report blocked by a high-level precondition (e.g. incompatible table).
    pub fn blocked(table_id: String, reason: String) -> Self {
        Self {
            table_id,
            total_frames_scanned: 0,
            eligible_frames_used: 0,
            cells: Vec::new(),
            rejections: RejectionSummary::default(),
            blocker_reason: Some(reason),
        }
    }

    /// Returns true if analysis was blocked due to incompatible table or invalid configuration.
    pub fn is_blocked(&self) -> bool {
        self.blocker_reason.is_some()
    }

    /// Looks up recommendation for a specific row and column.
    pub fn cell(&self, row: usize, col: usize) -> Option<&CellRecommendation> {
        self.cells.iter().find(|c| c.row == row && c.col == col)
    }
}
