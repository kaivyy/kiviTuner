pub mod binding;
pub mod resolver;
pub mod types;

pub use binding::{
    match_channel_by_unit, normalize_unit, resolve_axis_channel, validate_numeric_labels,
};
pub use resolver::{
    correlate_frame, extract_axis_breakpoints, extract_channel_value, resolve_axis_coordinate,
};
pub use types::{
    AxisAssociationMode, AxisBindingError, AxisCoordinate, AxisDescriptor, AxisResolutionError,
    AxisRole, CorrelationResult, CorrelationStatus, TableBindingConfig, TelemetryChannel,
};
