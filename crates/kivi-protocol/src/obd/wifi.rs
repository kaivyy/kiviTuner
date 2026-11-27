//! Asynchronous Wi-Fi TCP OBD-II transport for wireless ELM327 / STN dongles
//! (default `192.168.0.10:35000`).

use async_trait::async_trait;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWriteExt, ReadBuf};
use tokio::net::TcpStream;

use crate::obd::command::ObdCommand;
use crate::obd::errors::ObdCodecError;
use crate::obd::framing::read_framed_response_async;
use crate::obd::transport::ObdTransport;

pub const DEFAULT_WIFI_OBD_ENDPOINT: &str = "192.168.0.10:35000";

/// Normalizes Wi-Fi ELM327 endpoint strings, defaulting port to `35000`.
pub fn normalize_wifi_obd_endpoint(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return DEFAULT_WIFI_OBD_ENDPOINT.to_string();
    }
    if trimmed.starts_with('[') || trimmed.matches(':').count() == 1 {
        trimmed.to_string()
    } else if trimmed.matches(':').count() > 1 {
        format!("[{trimmed}]:35000")
    } else {
        format!("{trimmed}:35000")
    }
}

/// Async reader wrapper that drops NUL (`0x00`) bytes emitted during clone UART power-up.
pub(crate) struct NulStrippingAsyncReader<'a, R: AsyncRead + Unpin>(pub &'a mut R);

impl<'a, R: AsyncRead + Unpin> AsyncRead for NulStrippingAsyncReader<'a, R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        loop {
            let max_chunk = buf.remaining().clamp(1, 256);
            let mut scratch = vec![0u8; max_chunk];
            let mut tmp = ReadBuf::new(&mut scratch);
            match Pin::new(&mut *self.0).poll_read(cx, &mut tmp) {
                Poll::Ready(Ok(())) => {
                    let filled = tmp.filled();
                    if filled.is_empty() {
                        return Poll::Ready(Ok(()));
                    }
                    let before = buf.filled().len();
                    for &b in filled {
                        if b != 0x00 {
                            buf.put_slice(&[b]);
                        }
                    }
                    if buf.filled().len() > before {
                        return Poll::Ready(Ok(()));
                    }
                    // Every byte read in this chunk was NUL noise; keep polling cleanly.
                }
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

/// Asynchronous TCP transport speaking AT commands to wireless ELM327 adapters.
pub struct WifiObdTransport {
    endpoint: String,
    stream: Option<TcpStream>,
    buffer: Vec<u8>,
}

impl WifiObdTransport {
    pub fn new(endpoint: impl Into<String>) -> Self {
        let raw = endpoint.into();
        Self {
            endpoint: normalize_wifi_obd_endpoint(&raw),
            stream: None,
            buffer: Vec::new(),
        }
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
}

#[async_trait]
impl ObdTransport for WifiObdTransport {
    async fn open(&mut self) -> Result<(), ObdCodecError> {
        if self.stream.is_some() {
            return Err(ObdCodecError::InvalidSessionState {
                current: "Open".to_string(),
                expected: "Closed".to_string(),
            });
        }
        let stream = tokio::time::timeout(
            Duration::from_secs(3),
            TcpStream::connect(&self.endpoint),
        )
        .await
        .map_err(|_| {
            ObdCodecError::IoError(format!("timed out connecting to Wi-Fi OBD '{}'", self.endpoint))
        })?
        .map_err(|e| {
            ObdCodecError::IoError(format!("failed to connect Wi-Fi OBD '{}': {e}", self.endpoint))
        })?;
        let _ = stream.set_nodelay(true);
        self.buffer.clear();
        self.stream = Some(stream);
        Ok(())
    }

    async fn close(&mut self) -> Result<(), ObdCodecError> {
        self.stream = None;
        self.buffer.clear();
        Ok(())
    }

    async fn write_command(&mut self, cmd: ObdCommand) -> Result<(), ObdCodecError> {
        let stream = self.stream.as_mut().ok_or_else(|| ObdCodecError::InvalidSessionState {
            current: "Closed".to_string(),
            expected: "Open".to_string(),
        })?;
        stream
            .write_all(cmd.encode().as_bytes())
            .await
            .map_err(|e| ObdCodecError::IoError(format!("Wi-Fi OBD write failed: {e}")))?;
        Ok(())
    }

    async fn read_response(
        &mut self,
        timeout: Duration,
        max_bytes: usize,
    ) -> Result<String, ObdCodecError> {
        let stream = self.stream.as_mut().ok_or_else(|| ObdCodecError::InvalidSessionState {
            current: "Closed".to_string(),
            expected: "Open".to_string(),
        })?;
        let mut clean = NulStrippingAsyncReader(stream);
        read_framed_response_async(&mut clean, &mut self.buffer, max_bytes, timeout).await
    }

    fn is_connected(&self) -> bool {
        self.stream.is_some()
    }
}
