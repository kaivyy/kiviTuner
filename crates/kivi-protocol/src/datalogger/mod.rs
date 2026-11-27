//! Datalogger module for live session recording, bounded buffering, and playback.

pub mod csv;
pub mod playback;
pub mod session;

pub use csv::{
    export_session_to_csv, flags_from_hex, flags_to_hex, parse_session_from_csv,
    parse_session_from_reader, parse_session_from_str, serialize_session_to_string,
    serialize_session_to_writer, COLUMN_HEADER, MAGIC_HEADER, MAX_DATA_ROWS, MAX_PAYLOAD_BYTES,
};
pub use playback::{PlaybackEngine, PlaybackState, PlaybackStatus};
pub use session::{
    BoundedDatalogBuffer, DatalogSession, RecordingState, SessionMetadata, DEFAULT_BUFFER_CAPACITY,
};
