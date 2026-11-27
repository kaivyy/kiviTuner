use thiserror::Error;

/// Errors arising from OBD-II protocol encoding, adapter response parsing,
/// or diagnostic payload decoding.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ObdCodecError {
    /// Payload length was shorter than the strictly required byte length.
    #[error("truncated OBD payload: expected {expected} bytes, received {actual} bytes")]
    TruncatedPayload { expected: usize, actual: usize },

    /// Payload length exceeded the strictly allowed byte length.
    #[error("oversized OBD payload: expected {expected} bytes, received {actual} bytes")]
    OversizedPayload { expected: usize, actual: usize },

    /// Adapter textual response exceeded the configured buffer limit.
    #[error("adapter textual response exceeded buffer limit: received {actual} bytes, limit is {limit} bytes")]
    BufferOverflow { limit: usize, actual: usize },

    /// Response stream contained unexpected control characters, invalid UTF-8, or framing corruption.
    #[error("framing error in response stream: {0}")]
    FramingError(String),

    /// Response stream reached end-of-file before receiving the completion prompt '>'.
    #[error("unexpected end of response stream before prompt '>'")]
    UnexpectedEof,

    /// Service identifier in response does not match the expected positive echo.
    #[error("unexpected service in response: expected 0x{expected:02X}, received 0x{actual:02X}")]
    UnexpectedService { expected: u8, actual: u8 },

    /// Parameter identifier (PID) in response does not match the requested PID.
    #[error("unexpected PID in response: expected 0x{expected:02X}, received 0x{actual:02X}")]
    UnexpectedPid { expected: u8, actual: u8 },

    /// Requested PID is outside the allowable range for the given bitmap base.
    #[error("PID 0x{pid:02X} is out of range for bitmap base 0x{base:02X}")]
    PidOutOfRange { pid: u8, base: u8 },

    /// Adapter returned "NO DATA" (no response from ECU within timeout).
    #[error("adapter reported NO DATA")]
    NoData,

    /// Adapter returned a hardware or protocol error condition (e.g. BUS BUSY, CAN ERROR).
    #[error("adapter error: {0}")]
    AdapterError(String),

    /// Response text could not be parsed into valid diagnostic bytes.
    #[error("malformed adapter response text: {0}")]
    MalformedText(String),

    /// Decoded metadata contains invalid non-ASCII characters.
    #[error("non-ASCII metadata: {0}")]
    NonAsciiMetadata(String),

    /// Stoichiometric ratio for AFR calculation was invalid (non-positive, NaN, or infinite).
    #[error("invalid stoichiometric ratio: {0}")]
    InvalidStoichiometricRatio(String),

    /// Session state does not permit the requested operation.
    #[error("invalid session state: currently {current}, expected {expected}")]
    InvalidSessionState { current: String, expected: String },

    /// Session is currently in a faulted state and cannot execute transactions.
    #[error("session faulted: {0}")]
    SessionFaulted(String),

    /// A transaction is already in progress on this session (concurrent access rejected).
    #[error("session is busy with an active transaction")]
    SessionBusy,

    /// Transport-level I/O error occurred.
    #[error("transport I/O error: {0}")]
    IoError(String),

    /// Communication timed out after specified milliseconds.
    #[error("communication timed out after {0} ms")]
    Timeout(u64),

    /// Command is not permitted in the current session state or lifecycle allowlist.
    #[error("unauthorized command for current lifecycle: {0}")]
    UnauthorizedCommand(String),
}
