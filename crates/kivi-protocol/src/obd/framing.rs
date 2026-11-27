use serde::{Deserialize, Serialize};
use std::future::poll_fn;
use std::pin::Pin;
use std::task::Poll;
use std::time::Duration;
use tokio::io::{AsyncRead, ReadBuf};

use crate::obd::command::AdapterCommand;
use crate::obd::errors::ObdCodecError;

/// Named initial candidate ceiling for the adapter textual response layer (16 KiB = 16,384 bytes).
///
/// Derived mathematically from the ISO 15765-2 classical CAN maximum 12-bit diagnostic payload
/// limit (4,095 binary bytes) expanding into up to ~15,250 ASCII characters of multi-frame
/// sequence indices (`0: `, `1: `, etc.), hex byte pairs, space delimiters, CR/LF separators,
/// intermediate status tokens, and prompt character (`>`) with ~1.1 KiB safety margin.
pub const DEFAULT_MAX_TEXT_RESPONSE_BYTES: usize = 16 * 1024;

/// Permitted vehicle bus protocol selection policies.
///
/// This is a strictly closed enum representing explicit, supported protocol configurations.
/// Arbitrary protocol strings or custom command payloads are rejected at compile time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ProtocolSelection {
    /// ISO 15765-4 CAN (11-bit ID, 500 kbaud) via `ATSP6\r` (standard default).
    #[default]
    Can11Bit500k,
    /// ISO 15765-4 CAN (29-bit ID, 500 kbaud) via `ATSP7\r`.
    Can29Bit500k,
    /// ISO 15765-4 CAN (11-bit ID, 250 kbaud) via `ATSP8\r`.
    Can11Bit250k,
    /// ISO 15765-4 CAN (29-bit ID, 250 kbaud) via `ATSP9\r`.
    Can29Bit250k,
    /// ISO 14230-4 KWP2000 Fast Init (10.4 kbaud) via `ATSP5\r`.
    KwpFastInit,
    /// ISO 14230-4 KWP2000 5-Baud Init (10.4 kbaud) via `ATSP4\r`.
    Kwp5BaudInit,
    /// ISO 9141-2 5-Baud Init (10.4 kbaud) via `ATSP3\r`.
    Iso9141,
    /// Explicit automatic protocol search via `ATSP0\r`.
    ///
    /// Note: `Auto` initiates an on-bus protocol search during diagnostic queries.
    /// It does not guarantee compatibility with every vehicle or non-standard ECU.
    Auto,
}

impl ProtocolSelection {
    /// Maps the protocol selection to the approved, allowlisted `AdapterCommand`.
    pub const fn to_adapter_command(&self) -> AdapterCommand {
        match self {
            Self::Can11Bit500k => AdapterCommand::SetProtocolCan11_500,
            Self::Can29Bit500k => AdapterCommand::SetProtocolCan29_500,
            Self::Can11Bit250k => AdapterCommand::SetProtocolCan11_250,
            Self::Can29Bit250k => AdapterCommand::SetProtocolCan29_250,
            Self::KwpFastInit => AdapterCommand::SetProtocolKwpFastInit,
            Self::Kwp5BaudInit => AdapterCommand::SetProtocolKwp5BaudInit,
            Self::Iso9141 => AdapterCommand::SetProtocolIso9141,
            Self::Auto => AdapterCommand::SetProtocolAuto,
        }
    }
}

/// Permitted physical serial communication baud rates.
///
/// Closed enum covering documented factory defaults for supported hardware families.
/// Does not implement or imply automatic baud probing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum SerialBaudRate {
    /// 9,600 baud (legacy serial adapters).
    Baud9600,
    /// 115,200 baud (standard factory default for STN11xx / OBDLink SX).
    #[default]
    Baud115200,
    /// 38,400 baud (standard factory default for genuine ELM327 and generic clones).
    Baud38400,
    /// 230,400 baud (high-speed USB-UART bridges).
    Baud230400,
    /// 500,000 baud (FTDI high-throughput bridges).
    Baud500000,
}

impl SerialBaudRate {
    /// Returns the integer baud rate.
    pub const fn as_u32(&self) -> u32 {
        match self {
            Self::Baud9600 => 9_600,
            Self::Baud115200 => 115_200,
            Self::Baud38400 => 38_400,
            Self::Baud230400 => 230_400,
            Self::Baud500000 => 500_000,
        }
    }
}

/// Validates whether a byte is allowable in the adapter ASCII transport stream.
///
/// Permitted characters:
/// - Printable ASCII characters: `0x20` (space) through `0x7E` (`~`)
/// - Whitespace separators: carriage return (`\r`, `0x0D`), line feed (`\n`, `0x0A`), tab (`\t`, `0x09`)
///
/// Unexpected control characters (e.g. null `0x00`, bells, escapes) or non-ASCII bytes
/// are rejected to prevent UART framing errors from silently corrupting diagnostic responses.
#[inline]
pub const fn is_permitted_stream_byte(b: u8) -> bool {
    (b >= 0x20 && b <= 0x7E) || b == b'\r' || b == b'\n' || b == b'\t'
}

/// Pure response-framing function extracting a single prompt-terminated frame from an accumulator.
///
/// # Rules
/// 1. Incremental limit check: rejects if `buffer.len() > max_bytes`.
/// 2. Fail-closed character validation: scans for unexpected control characters or invalid UTF-8.
/// 3. Prompt termination: searches for the prompt character `>` (`0x3E`).
/// 4. If prompt is found:
///    - Verifies `frame_len <= max_bytes` (the prompt byte counts toward the text limit).
///    - Drains the frame bytes (up to and including `>`) from `buffer`.
///    - Returns the payload string (excluding the terminating `>`).
/// 5. If prompt is not yet found:
///    - If `buffer.len() == max_bytes`, fails closed with `BufferOverflow` (buffer cannot fit `>`).
///    - Otherwise returns `Ok(None)` indicating more stream data is required.
pub fn extract_prompt_framed_response(
    buffer: &mut Vec<u8>,
    max_bytes: usize,
) -> Result<Option<String>, ObdCodecError> {
    // 1. Initial length check
    if buffer.len() > max_bytes {
        return Err(ObdCodecError::BufferOverflow {
            limit: max_bytes,
            actual: buffer.len(),
        });
    }

    // 2. Character validation across all accumulated bytes
    for &b in buffer.iter() {
        if !is_permitted_stream_byte(b) {
            return Err(ObdCodecError::FramingError(format!(
                "unexpected control or invalid byte 0x{:02X} in response stream",
                b
            )));
        }
    }

    // 3. Search for terminating prompt '>'
    if let Some(prompt_idx) = buffer.iter().position(|&b| b == b'>') {
        let frame_len = prompt_idx + 1;
        if frame_len > max_bytes {
            return Err(ObdCodecError::BufferOverflow {
                limit: max_bytes,
                actual: frame_len,
            });
        }

        let drained: Vec<u8> = buffer.drain(..=prompt_idx).collect();
        let payload_bytes = &drained[..drained.len() - 1]; // Exclude the '>' prompt
        let text = String::from_utf8(payload_bytes.to_vec()).map_err(|e| {
            ObdCodecError::FramingError(format!("invalid UTF-8 in response frame: {}", e))
        })?;

        Ok(Some(text))
    } else {
        // Prompt absent: if accumulator has reached exact max_bytes, it can never accommodate '>'
        if buffer.len() == max_bytes {
            return Err(ObdCodecError::BufferOverflow {
                limit: max_bytes,
                actual: buffer.len(),
            });
        }
        Ok(None)
    }
}

/// Reads an adapter response from an asynchronous stream up to the terminating prompt `>`
/// enforcing a single absolute deadline, incremental chunk limits, and fail-closed validation.
///
/// # Invariants
/// - **Single Absolute Deadline:** Calculated once upon entry (`now + timeout`). Repeated
///   small chunks cannot keep the operation alive past the deadline.
/// - **Incremental Buffer Enforcement:** Chunk read size is bounded and limit is checked before
///   accumulating data into `buffer`. Oversized streams fail immediately with `BufferOverflow`.
/// - **Cancellation Safety:** If dropped mid-read, bytes read up to that point remain in `buffer`.
///   The caller must drain/clear `buffer` before reusing the transport to prevent stale responses.
pub async fn read_framed_response_async<R: AsyncRead + Unpin>(
    reader: &mut R,
    buffer: &mut Vec<u8>,
    max_bytes: usize,
    timeout: Duration,
) -> Result<String, ObdCodecError> {
    let deadline = tokio::time::Instant::now() + timeout;
    let mut chunk = [0u8; 256];

    loop {
        // 1. Check if a complete frame is already present in the accumulator
        if let Some(frame) = extract_prompt_framed_response(buffer, max_bytes)? {
            return Ok(frame);
        }

        // 2. Enforce single absolute deadline
        let now = tokio::time::Instant::now();
        if now >= deadline {
            return Err(ObdCodecError::Timeout(timeout.as_millis() as u64));
        }
        let remaining = deadline - now;

        // 3. Enforce buffer limit before performing further I/O
        if buffer.len() >= max_bytes {
            return Err(ObdCodecError::BufferOverflow {
                limit: max_bytes,
                actual: buffer.len(),
            });
        }

        // 4. Bound chunk size so we can detect limit violations without unbounded allocations
        let space_available = max_bytes - buffer.len();
        let max_chunk = chunk.len().min(space_available + 1);

        let n = match tokio::time::timeout(
            remaining,
            poll_fn(|cx| {
                let mut read_buf = ReadBuf::new(&mut chunk[..max_chunk]);
                match Pin::new(&mut *reader).poll_read(cx, &mut read_buf) {
                    Poll::Ready(Ok(())) => Poll::Ready(Ok(read_buf.filled().len())),
                    Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
                    Poll::Pending => Poll::Pending,
                }
            }),
        )
        .await
        {
            Ok(read_res) => match read_res {
                Ok(n) => n,
                Err(io_err) => return Err(ObdCodecError::IoError(io_err.to_string())),
            },
            Err(_) => return Err(ObdCodecError::Timeout(timeout.as_millis() as u64)),
        };

        // 5. Handle EOF before prompt
        if n == 0 {
            return Err(ObdCodecError::UnexpectedEof);
        }

        // 6. Incremental limit check: fail closed immediately if this chunk overflows
        if buffer.len() + n > max_bytes {
            let actual = buffer.len() + n;
            return Err(ObdCodecError::BufferOverflow {
                limit: max_bytes,
                actual,
            });
        }

        // 7. Validate character bytes in incoming chunk
        for &b in &chunk[..n] {
            if !is_permitted_stream_byte(b) {
                return Err(ObdCodecError::FramingError(format!(
                    "unexpected control or invalid byte 0x{:02X} in response stream",
                    b
                )));
            }
        }

        // 8. Append valid bytes to accumulator
        buffer.extend_from_slice(&chunk[..n]);
    }
}
