//! Strictly read-only SAE J1979 / ISO 15765-4 OBD-II codec.
//!
//! Provides:
//! - Closed typed command domain (`ObdCommand`, `Mode01Pid`, `Mode09Item`, `AdapterCommand`).
//! - Hardware adapter ASCII response parser and multi-line ISO-TP reassembler.
//! - Supported-PIDs bitmap mapping and range coordinate calculator.
//! - Mode 01 diagnostic payload decoders (RPM, TPS, MAP, ECT, IAT, Battery Voltage, Speed, Wideband Lambda).
//! - Mode 09 vehicle metadata decoders (VIN, Calibration ID).
//! - Strict validation against truncated, oversized, or unexpected responses.

pub mod adapter;
pub mod bitmap;
pub mod classification;
pub mod command;
pub mod driver;
pub mod errors;
pub mod framing;
pub mod metadata;
#[cfg(any(test, feature = "test-doubles"))]
pub mod mock;
pub mod pid;
pub mod serial;
pub mod session;
pub mod transport;

pub use adapter::{parse_adapter_response, MAX_ISOTP_PAYLOAD_BYTES};
pub use bitmap::{
    calculate_pid_bitmap_coordinates, is_pid_supported_in_bitmap, PidBitmapCoordinates,
    SupportedPidsBitmap,
};
pub use classification::{classify_command, CommandClassification};
pub use command::{AdapterCommand, Mode01Pid, Mode09Item, ObdCommand};
pub use driver::{create_empty_frame, update_frame_from_decoded_pid, Obd2Driver, Obd2DriverConfig};
pub use errors::ObdCodecError;
pub use framing::{
    extract_prompt_framed_response, is_permitted_stream_byte, read_framed_response_async,
    ProtocolSelection, SerialBaudRate, DEFAULT_MAX_TEXT_RESPONSE_BYTES,
};
pub use metadata::{decode_calibration_id, decode_vin};
#[cfg(any(test, feature = "test-doubles"))]
pub use mock::MockObdTransport;
pub use pid::{decode_mode01_payload, DecodedMode01Value, WidebandO2Sample};
pub use serial::SerialObdTransport;
pub use session::{
    is_recognized_adapter_banner, ObdAdapterIdentity, ObdSession, ObdSessionConfig, ObdSessionState,
};
pub use transport::ObdTransport;
