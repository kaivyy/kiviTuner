pub mod autotune;
pub mod clock;
pub mod correlation;
pub mod datalogger;
pub mod driver;
pub mod errors;
#[cfg(any(test, feature = "test-doubles"))]
pub mod faults;
pub mod model;
pub mod obd;
pub mod ostrich;
pub mod session;
#[cfg(any(test, feature = "test-doubles"))]
pub mod simulator;
pub mod ini_serial;
pub mod validation;
pub mod worker;

pub use autotune::{
    compute_recommendations, AutoTuneConfig, AutoTuneConfigError, AutoTuneEngine, CellAccumulator,
    CellAggregator, CellRecommendation, CellRecommendationStatus, RecommendationReport,
    RejectionReason, RejectionSummary, SampleFilter, TargetAfrSource, TunableParameterType,
};
pub use clock::{Clock, SystemMonotonicClock};
#[cfg(any(test, feature = "test-doubles"))]
pub use clock::MockClock;
pub use datalogger::{
    export_session_to_csv, flags_from_hex, flags_to_hex, parse_session_from_csv,
    parse_session_from_reader, parse_session_from_str, serialize_session_to_string,
    serialize_session_to_writer, BoundedDatalogBuffer, DatalogSession, PlaybackEngine,
    PlaybackState, PlaybackStatus, RecordingState, SessionMetadata, COLUMN_HEADER,
    DEFAULT_BUFFER_CAPACITY, MAGIC_HEADER, MAX_DATA_ROWS, MAX_PAYLOAD_BYTES,
};
pub use driver::{DriverDescriptor, EcuDriver, TelemetrySource};
pub use errors::ProtocolError;
#[cfg(any(test, feature = "test-doubles"))]
pub use faults::SimFault;
pub use model::{ConnectionState, EcuIdentity, TelemetryFrame, TelemetryStatusFlags};
pub use obd::{
    calculate_pid_bitmap_coordinates, classify_command, decode_calibration_id,
    decode_mode01_payload, decode_vin, extract_prompt_framed_response, is_permitted_stream_byte,
    is_pid_supported_in_bitmap, parse_adapter_response, read_framed_response_async, AdapterCommand,
    CommandClassification, DecodedMode01Value, Mode01Pid, Mode09Item, Obd2Driver,
    Obd2DriverConfig, ObdAdapterIdentity, ObdCodecError, ObdCommand, ObdSession, ObdSessionConfig,
    ObdSessionState, ObdTransport, PidBitmapCoordinates, ProtocolSelection, SerialBaudRate,
    SerialObdTransport, SupportedPidsBitmap, WidebandO2Sample, DEFAULT_MAX_TEXT_RESPONSE_BYTES,
    MAX_ISOTP_PAYLOAD_BYTES,
};
#[cfg(any(test, feature = "test-doubles"))]
pub use obd::MockObdTransport;
pub use session::ProtocolSessionManager;
#[cfg(any(test, feature = "test-doubles"))]
pub use simulator::{LcgRng, SimMode, VirtualEcuSimulator};
pub use ini_serial::IniSerialDriver;
pub use validation::{validate_telemetry_frame, SensorRangeError, ValidationReport};
pub use worker::{create_telemetry_broadcast_channel, TELEMETRY_BROADCAST_CAPACITY};
