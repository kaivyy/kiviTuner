//! Physical serial OBD-II transport implementation backed by `tokio-serial`.
//!
//! Provides asynchronous serial communication with ELM327 / STN-compatible OBD-II
//! adapters over physical USB-to-serial bridges or native UART character devices.

use async_trait::async_trait;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio_serial::{ClearBuffer, SerialPort, SerialPortBuilderExt, SerialStream};

use crate::obd::command::ObdCommand;
use crate::obd::errors::ObdCodecError;
use crate::obd::framing::{read_framed_response_async, SerialBaudRate};
use crate::obd::transport::ObdTransport;

/// Default provisional settling delay after opening physical serial port (75 ms).
///
/// Allows USB-to-serial bridge DTR/RTS lines and adapter microcontroller power/clock
/// to stabilize before issuing commands. Configurable via `with_settling_delay`.
pub const DEFAULT_SERIAL_SETTLING_DELAY: Duration = Duration::from_millis(75);

/// Asynchronous physical serial transport for OBD-II adapters.
///
/// Enforces:
/// - Explicit 8N1 raw serial framing without flow control.
/// - Fail-closed error handling during port opening, queue purging, and transactions.
/// - Single absolute monotonic deadline and bounded byte limits during response reading.
/// - Strict type-safety: only allowlisted typed `ObdCommand` values can be transmitted.
pub struct SerialObdTransport {
    path: String,
    baud_rate: SerialBaudRate,
    settling_delay: Duration,
    port: Option<SerialStream>,
    buffer: Vec<u8>,
}

impl SerialObdTransport {
    /// Creates a new disconnected `SerialObdTransport` with the specified port path and baud rate.
    pub fn new(path: impl Into<String>, baud_rate: SerialBaudRate) -> Self {
        Self {
            path: path.into(),
            baud_rate,
            settling_delay: DEFAULT_SERIAL_SETTLING_DELAY,
            port: None,
            buffer: Vec::new(),
        }
    }

    /// Sets the provisional settling delay to wait after opening the port before clearing the queue.
    pub fn with_settling_delay(mut self, delay: Duration) -> Self {
        self.settling_delay = delay;
        self
    }

    /// Returns the configured serial device path.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns the configured serial baud rate.
    pub fn baud_rate(&self) -> SerialBaudRate {
        self.baud_rate
    }

    /// Returns the configured provisional settling delay.
    pub fn settling_delay(&self) -> Duration {
        self.settling_delay
    }
}

#[async_trait]
impl ObdTransport for SerialObdTransport {
    /// Opens the physical serial port and executes the fail-closed initialization lifecycle:
    ///
    /// 1. Verifies the transport is not already open.
    /// 2. Opens the asynchronous serial stream with 8N1 raw attributes and no flow control.
    /// 3. Awaits the provisional settling delay to allow USB line transients to settle.
    /// 4. Flushes the OS kernel receive queue via `stream.clear(ClearBuffer::Input)`.
    /// 5. Clears the local user-space receive buffer.
    /// 6. Stores the stream handle only after every step succeeds.
    ///
    /// If any step fails or the future is cancelled, the local stream is dropped,
    /// closing the OS descriptor, and `self.port` remains `None`.
    async fn open(&mut self) -> Result<(), ObdCodecError> {
        if self.port.is_some() {
            return Err(ObdCodecError::InvalidSessionState {
                current: "Open".to_string(),
                expected: "Closed".to_string(),
            });
        }

        let builder = tokio_serial::new(&self.path, self.baud_rate.as_u32())
            .data_bits(tokio_serial::DataBits::Eight)
            .parity(tokio_serial::Parity::None)
            .stop_bits(tokio_serial::StopBits::One)
            .flow_control(tokio_serial::FlowControl::None);

        let stream = builder.open_native_async().map_err(|e| {
            ObdCodecError::IoError(format!("failed to open serial port '{}': {}", self.path, e))
        })?;

        if self.settling_delay > Duration::ZERO {
            tokio::time::sleep(self.settling_delay).await;
        }

        stream.clear(ClearBuffer::Input).map_err(|e| {
            ObdCodecError::IoError(format!(
                "failed to clear serial receive queue on '{}': {}",
                self.path, e
            ))
        })?;

        self.buffer.clear();
        self.port = Some(stream);
        Ok(())
    }

    /// Closes the physical serial transport and releases the OS file descriptor/handle.
    async fn close(&mut self) -> Result<(), ObdCodecError> {
        self.port = None;
        self.buffer.clear();
        Ok(())
    }

    /// Transmits a strongly-typed `ObdCommand` across the serial stream using `write_all`.
    async fn write_command(&mut self, cmd: ObdCommand) -> Result<(), ObdCodecError> {
        let stream = self
            .port
            .as_mut()
            .ok_or_else(|| ObdCodecError::InvalidSessionState {
                current: "Closed".to_string(),
                expected: "Open".to_string(),
            })?;

        let encoded = cmd.encode();
        stream.write_all(encoded.as_bytes()).await.map_err(|e| {
            ObdCodecError::IoError(format!("serial write failed on '{}': {}", self.path, e))
        })?;

        Ok(())
    }

    /// Reads an ASCII response up to the prompt character `>` using the shared bounded framing engine.
    async fn read_response(
        &mut self,
        timeout: Duration,
        max_bytes: usize,
    ) -> Result<String, ObdCodecError> {
        let stream = self
            .port
            .as_mut()
            .ok_or_else(|| ObdCodecError::InvalidSessionState {
                current: "Closed".to_string(),
                expected: "Open".to_string(),
            })?;

        read_framed_response_async(stream, &mut self.buffer, max_bytes, timeout).await
    }

    /// Reports whether the transport currently holds an open serial stream handle.
    fn is_connected(&self) -> bool {
        self.port.is_some()
    }
}
