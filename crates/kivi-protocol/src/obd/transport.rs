use crate::obd::command::ObdCommand;
use crate::obd::errors::ObdCodecError;
use async_trait::async_trait;
use std::time::Duration;

/// Asynchronous transport boundary for communicating with an OBD-II adapter.
///
/// Ensures strict type safety: commands can ONLY be transmitted via typed `ObdCommand`.
/// No public API allows transmitting arbitrary strings or caller-provided byte sequences.
#[async_trait]
pub trait ObdTransport: Send + Sync {
    /// Opens the communication transport channel.
    async fn open(&mut self) -> Result<(), ObdCodecError>;

    /// Closes the transport channel and releases associated system resources.
    async fn close(&mut self) -> Result<(), ObdCodecError>;

    /// Writes a strictly typed `ObdCommand` to the transport.
    async fn write_command(&mut self, cmd: ObdCommand) -> Result<(), ObdCodecError>;

    /// Reads an ASCII response from the adapter up to the prompt character `>` or until deadline.
    ///
    /// Must bound total bytes to `max_bytes` to prevent buffer overflow or memory exhaustion.
    async fn read_response(
        &mut self,
        timeout: Duration,
        max_bytes: usize,
    ) -> Result<String, ObdCodecError>;

    /// Reports whether the transport is currently connected.
    fn is_connected(&self) -> bool;
}
