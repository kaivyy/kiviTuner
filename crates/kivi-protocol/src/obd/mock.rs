use crate::obd::command::ObdCommand;
use crate::obd::errors::ObdCodecError;
use crate::obd::transport::ObdTransport;
use async_trait::async_trait;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Deterministic mock transport for simulating adapter communications, timing, and fault conditions.
#[derive(Debug, Clone)]
pub struct MockObdTransport {
    inner: Arc<Mutex<MockInner>>,
}

#[derive(Debug, Default)]
struct MockInner {
    connected: bool,
    scripted_responses: VecDeque<(ObdCommand, Result<String, ObdCodecError>)>,
    written_commands: Vec<ObdCommand>,
    fail_open: Option<ObdCodecError>,
    fail_close: Option<ObdCodecError>,
    fail_write: Option<ObdCodecError>,
    fail_read: Option<ObdCodecError>,
    disconnect_on_read: bool,
    read_delay: Option<Duration>,
    write_delay: Option<Duration>,
    open_delay: Option<Duration>,
    close_delay: Option<Duration>,
}

impl Default for MockObdTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl MockObdTransport {
    /// Creates a new mock transport with disconnected initial state.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(MockInner::default())),
        }
    }

    /// Appends a scripted command expectation and corresponding response.
    pub fn script_response(&self, cmd: ObdCommand, resp: Result<String, ObdCodecError>) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.scripted_responses.push_back((cmd, resp));
    }

    /// Sets up standard successful adapter handshake responses.
    pub fn script_standard_handshake(&self) {
        use crate::obd::command::AdapterCommand;
        self.script_response(ObdCommand::Adapter(AdapterCommand::Reset), Ok("ELM327 v1.5\r\r>".to_string()));
        self.script_response(ObdCommand::Adapter(AdapterCommand::EchoOff), Ok("OK\r\r>".to_string()));
        self.script_response(ObdCommand::Adapter(AdapterCommand::LinefeedsOff), Ok("OK\r\r>".to_string()));
        self.script_response(ObdCommand::Adapter(AdapterCommand::HeadersOff), Ok("OK\r\r>".to_string()));
        self.script_response(ObdCommand::Adapter(AdapterCommand::SpacesOn), Ok("OK\r\r>".to_string()));
        self.script_response(ObdCommand::Adapter(AdapterCommand::SetProtocolCan11_500), Ok("OK\r\r>".to_string()));
        self.script_response(ObdCommand::Adapter(AdapterCommand::QueryElmVersion), Ok("ELM327 v1.5\r\r>".to_string()));
        self.script_response(ObdCommand::Adapter(AdapterCommand::QueryStnDeviceId), Ok("STN1130 v3.3.0\r\r>".to_string()));
        self.script_response(ObdCommand::Adapter(AdapterCommand::QueryStnFirmwareVersion), Ok("STN1130 FW 4.2.1\r\r>".to_string()));
        self.script_response(ObdCommand::Adapter(AdapterCommand::QueryStnSerialNumber), Ok("1130-123456\r\r>".to_string()));
    }

    /// Injects a failure when `open()` is called.
    pub fn inject_fail_open(&self, err: ObdCodecError) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).fail_open = Some(err);
    }

    /// Injects a failure when `close()` is called.
    pub fn inject_fail_close(&self, err: ObdCodecError) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).fail_close = Some(err);
    }

    /// Injects an artificial delay on open (used to simulate timeouts or in-flight cancellation).
    pub fn inject_open_delay(&self, delay: Duration) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).open_delay = Some(delay);
    }

    /// Clears any injected open delay.
    pub fn clear_open_delay(&self) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).open_delay = None;
    }

    /// Injects an artificial delay on close (used to simulate timeouts or in-flight cancellation).
    pub fn inject_close_delay(&self, delay: Duration) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).close_delay = Some(delay);
    }

    /// Clears any injected close delay.
    pub fn clear_close_delay(&self) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).close_delay = None;
    }

    /// Injects a failure when `write_command()` is called.
    pub fn inject_fail_write(&self, err: ObdCodecError) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).fail_write = Some(err);
    }

    /// Injects a failure when `read_response()` is called.
    pub fn inject_fail_read(&self, err: ObdCodecError) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).fail_read = Some(err);
    }

    /// Injects an artificial delay on reads (used to simulate timeouts).
    pub fn inject_read_delay(&self, delay: Duration) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).read_delay = Some(delay);
    }

    /// Injects an artificial delay on writes (used to simulate timeouts or in-flight cancellation).
    pub fn inject_write_delay(&self, delay: Duration) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).write_delay = Some(delay);
    }

    /// Clears any injected write delay.
    pub fn clear_write_delay(&self) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).write_delay = None;
    }

    /// Injects a transport disconnect during read.
    pub fn inject_disconnect_on_read(&self) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).disconnect_on_read = true;
    }

    /// Returns a list of all commands written to this transport.
    pub fn written_commands(&self) -> Vec<ObdCommand> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).written_commands.clone()
    }

    /// Returns the number of unconsumed scripted responses in the queue.
    pub fn remaining_scripted_responses(&self) -> usize {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).scripted_responses.len()
    }

    /// Clears any injected read delay.
    pub fn clear_read_delay(&self) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).read_delay = None;
    }
}

#[async_trait]
impl ObdTransport for MockObdTransport {
    async fn open(&mut self) -> Result<(), ObdCodecError> {
        let delay = { self.inner.lock().unwrap_or_else(|e| e.into_inner()).open_delay };
        if let Some(d) = delay {
            tokio::time::sleep(d).await;
        }

        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(err) = inner.fail_open.take() {
            return Err(err);
        }
        inner.connected = true;
        Ok(())
    }

    async fn close(&mut self) -> Result<(), ObdCodecError> {
        let delay = { self.inner.lock().unwrap_or_else(|e| e.into_inner()).close_delay };
        if let Some(d) = delay {
            tokio::time::sleep(d).await;
        }

        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(err) = inner.fail_close.take() {
            return Err(err);
        }
        inner.connected = false;
        Ok(())
    }

    async fn write_command(&mut self, cmd: ObdCommand) -> Result<(), ObdCodecError> {
        let delay = { self.inner.lock().unwrap_or_else(|e| e.into_inner()).write_delay };
        if let Some(d) = delay {
            tokio::time::sleep(d).await;
        }

        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if !inner.connected {
            return Err(ObdCodecError::IoError("transport is not open".to_string()));
        }
        if let Some(err) = inner.fail_write.take() {
            return Err(err);
        }
        inner.written_commands.push(cmd);
        Ok(())
    }

    async fn read_response(
        &mut self,
        timeout: Duration,
        max_bytes: usize,
    ) -> Result<String, ObdCodecError> {
        let delay = { self.inner.lock().unwrap_or_else(|e| e.into_inner()).read_delay };
        if let Some(d) = delay {
            if d >= timeout {
                tokio::time::sleep(timeout).await;
                return Err(ObdCodecError::Timeout(timeout.as_millis() as u64));
            }
            tokio::time::sleep(d).await;
        }

        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if !inner.connected || inner.disconnect_on_read {
            inner.connected = false;
            return Err(ObdCodecError::IoError("connection reset or disconnected".to_string()));
        }

        if let Some(err) = inner.fail_read.take() {
            return Err(err);
        }

        if let Some((_expected_cmd, resp_res)) = inner.scripted_responses.pop_front() {
            match resp_res {
                Ok(resp) => {
                    if resp.len() > max_bytes {
                        return Err(ObdCodecError::OversizedPayload {
                            expected: max_bytes,
                            actual: resp.len(),
                        });
                    }
                    Ok(resp)
                }
                Err(e) => Err(e),
            }
        } else {
            Err(ObdCodecError::NoData)
        }
    }

    fn is_connected(&self) -> bool {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).connected
    }
}
