use kivi_defs::model::AxisDef;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Available telemetry channels from real-time acquisition, simulation, or datalog playback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TelemetryChannel {
    Rpm,
    Tps,
    MapKpa,
    EctC,
    IatC,
    BatteryVolts,
    IgnitionAdvanceDeg,
    InjectorPulseWidthMs,
    Afr,
    SpeedKph,
    Gear,
}

impl TelemetryChannel {
    /// Canonical engineering unit representation.
    pub fn canonical_unit(self) -> &'static str {
        match self {
            TelemetryChannel::Rpm => "RPM",
            TelemetryChannel::Tps => "%",
            TelemetryChannel::MapKpa => "kPa",
            TelemetryChannel::EctC => "°C",
            TelemetryChannel::IatC => "°C",
            TelemetryChannel::BatteryVolts => "V",
            TelemetryChannel::IgnitionAdvanceDeg => "°BTDC",
            TelemetryChannel::InjectorPulseWidthMs => "ms",
            TelemetryChannel::Afr => "AFR",
            TelemetryChannel::SpeedKph => "km/h",
            TelemetryChannel::Gear => "",
        }
    }

    /// User-facing human readable channel title.
    pub fn display_name(self) -> &'static str {
        match self {
            TelemetryChannel::Rpm => "Engine Speed (RPM)",
            TelemetryChannel::Tps => "Throttle Position (%)",
            TelemetryChannel::MapKpa => "Manifold Absolute Pressure (kPa)",
            TelemetryChannel::EctC => "Engine Coolant Temp (°C)",
            TelemetryChannel::IatC => "Intake Air Temp (°C)",
            TelemetryChannel::BatteryVolts => "Battery Voltage (V)",
            TelemetryChannel::IgnitionAdvanceDeg => "Ignition Advance (°BTDC)",
            TelemetryChannel::InjectorPulseWidthMs => "Injector Pulse Width (ms)",
            TelemetryChannel::Afr => "Air-Fuel Ratio (AFR)",
            TelemetryChannel::SpeedKph => "Vehicle Speed (km/h)",
            TelemetryChannel::Gear => "Gear Position",
        }
    }

    /// Primary standard table axis role.
    pub fn standard_role(self) -> AxisRole {
        match self {
            TelemetryChannel::Rpm => AxisRole::YAxis,
            TelemetryChannel::Tps | TelemetryChannel::MapKpa => AxisRole::XAxis,
            _ => AxisRole::XAxis,
        }
    }
}

/// Axis role in a 2D or 3D calibration table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AxisRole {
    XAxis,
    YAxis,
    ZAxis,
}

/// Association mode for linking a table axis to telemetry.
/// Matches empirical the reference implementation RT XDF specifications:
/// - `NoLink`: Axis is unlinked; tracing will not operate.
/// - `ExplicitLink(target)`: Explicit parameter/channel identifier binding.
/// - `UnitMatch`: Automatic binding based on engineering units and non-string numeric validation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "mode", content = "target")]
pub enum AxisAssociationMode {
    NoLink,
    ExplicitLink(String),
    #[default]
    UnitMatch,
}

/// Error conditions encountered during axis channel resolution.
#[derive(Error, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "error", content = "details")]
pub enum AxisBindingError {
    #[error("No link configured for this axis")]
    NoLink,
    #[error("Missing unit string for UnitMatch mode")]
    MissingUnit,
    #[error("Unit '{unit}' is unsupported or unrecognized")]
    UnsupportedUnit { unit: String },
    #[error("Ambiguous unit '{input}' matches multiple channels: {matched:?}")]
    AmbiguousMatch {
        input: String,
        matched: Vec<TelemetryChannel>,
    },
    #[error("Explicit link target '{target}' is invalid: {reason}")]
    InvalidExplicitReference { target: String, reason: String },
    #[error("Channel {channel:?} is incompatible with axis role {expected_role:?}")]
    IncompatibleRole {
        channel: TelemetryChannel,
        expected_role: AxisRole,
    },
    #[error("Axis contains non-numeric string labels (e.g. '{label_sample}' at index {index})")]
    NonNumericLabels { label_sample: String, index: usize },
    #[error("Invalid axis dimensions: length {length} {reason}")]
    InvalidDimensions { length: usize, reason: String },
}

/// Descriptor of an axis ready for telemetry correlation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AxisDescriptor {
    pub name: String,
    pub role: AxisRole,
    pub unit: String,
    pub length: usize,
    pub association_mode: AxisAssociationMode,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub values: Vec<f64>,
}

impl AxisDescriptor {
    /// Creates an `AxisDescriptor` from canonical `AxisDef` metadata.
    pub fn from_axis_def(def: &AxisDef, name: &str, role: AxisRole) -> Self {
        Self {
            name: name.to_string(),
            role,
            unit: def.scaling.unit.clone(),
            length: def.length,
            association_mode: AxisAssociationMode::UnitMatch,
            labels: Vec::new(),
            values: def.values.clone(),
        }
    }
}

/// Explicit configuration overrides for table axis bindings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TableBindingConfig {
    #[serde(default)]
    pub x_axis: Option<AxisAssociationMode>,
    #[serde(default)]
    pub y_axis: Option<AxisAssociationMode>,
}

/// Continuous coordinate and discrete index position along an axis.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AxisCoordinate {
    /// 0-based index of the interval's bounding breakpoint
    pub interval_index: usize,
    /// Continuous fractional position along the axis in range [0.0, N - 1.0]
    pub fractional_pos: f64,
    /// Nearest breakpoint index (discrete integer index in 0..N)
    pub nearest_index: usize,
    /// True if the telemetry value fell exactly on a breakpoint within floating-point tolerance
    pub is_exact_match: bool,
}

/// Error conditions during breakpoint coordinate resolution.
#[derive(Error, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "error", content = "details")]
pub enum AxisResolutionError {
    #[error("Axis has no breakpoints (empty)")]
    EmptyAxis,
    #[error("Axis has only 1 breakpoint, minimum 2 required for intervals")]
    SinglePointAxis,
    #[error("Axis contains non-finite breakpoint value at index {index}")]
    NonFiniteBreakpoint { index: usize },
    #[error("Axis has duplicate breakpoints: value {val} at indices {first_idx} and {second_idx}")]
    DuplicateBreakpoints {
        val: f64,
        first_idx: usize,
        second_idx: usize,
    },
    #[error("Axis is non-monotonic at index {index}: value {curr} vs prior {prev}")]
    NonMonotonic {
        index: usize,
        prev: f64,
        curr: f64,
    },
    #[error("Telemetry value {value} is out of axis range [{min}, {max}]")]
    OutOfRange { value: f64, min: f64, max: f64 },
    #[error("Telemetry channel value is non-finite: {value}")]
    NonFiniteValue { value: f32 },
}

/// Status of a telemetry-to-map correlation attempt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", content = "details")]
pub enum CorrelationStatus {
    /// Fully resolved operating cell
    Valid,
    /// Stale sample detected (status_flags.stale_sample == true)
    StaleSample,
    /// Communication fault (status_flags.communication_fault == true)
    CommunicationFault,
    /// Channel binding failed (e.g. NoLink, UnsupportedUnit, AmbiguousMatch)
    UnboundChannel {
        axis_role: AxisRole,
        error: AxisBindingError,
    },
    /// Telemetry value out of axis bounds or non-finite
    AxisOutOfRange {
        axis_role: AxisRole,
        error: AxisResolutionError,
    },
    /// Table dimensions invalid or mismatch with axis lengths
    DimensionMismatch {
        reason: String,
    },
}

/// Result of correlating a telemetry frame with a calibration table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CorrelationResult {
    pub is_valid: bool,
    pub status: CorrelationStatus,
    pub timestamp_ms: u64,
    pub sequence: u64,
    pub is_simulated: bool,
    /// Selected row index (Y-axis, discrete in 0..rows)
    pub selected_row: Option<usize>,
    /// Selected column index (X-axis, discrete in 0..cols)
    pub selected_col: Option<usize>,
    /// Y-axis continuous coordinate [0.0, rows - 1.0]
    pub row_coordinate: Option<AxisCoordinate>,
    /// X-axis continuous coordinate [0.0, cols - 1.0]
    pub col_coordinate: Option<AxisCoordinate>,
    /// Raw integer/float cell value from ROM (if MemoryMap provided and valid)
    pub cell_raw: Option<f64>,
    /// Scaled engineering cell value (if scaling and MemoryMap provided and valid)
    pub cell_eng: Option<f64>,
    pub table_id: String,
}
