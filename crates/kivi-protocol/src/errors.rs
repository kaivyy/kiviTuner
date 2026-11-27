use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    #[error("Device not found: {0}")]
    DeviceNotFound(String),
    #[error("Port is busy or locked: {0}")]
    PortBusy(String),
    #[error("Handshake initialization failed: {0}")]
    HandshakeFailed(String),
    #[error("Communication timed out ({0} ms)")]
    Timeout(u64),
    #[error("Malformed or corrupted frame: {0}")]
    MalformedFrame(String),
    #[error("Checksum verification failure")]
    ChecksumMismatch,
    #[error("Connection lost: {0}")]
    Disconnected(String),
    #[error("Invalid driver configuration: {0}")]
    InvalidConfig(String),
    #[error("Internal worker task error: {0}")]
    WorkerError(String),
    #[error("Invalid state transition: {0}")]
    InvalidStateTransition(String),
    #[error("Recording already active")]
    AlreadyRecording,
    #[error("Invalid format: {0}")]
    InvalidFormat(String),
}
