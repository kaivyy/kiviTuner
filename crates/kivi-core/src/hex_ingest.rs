//! Intel HEX and Motorola S-Record ingest.
//!
//! Ported from `ecu_engine_rust::formats::{intel_hex, srec}`. Parsing semantics
//! follow the source for valid input; the two documented safety deviations
//! below replace source panics with typed errors.
//!
//! ## Intentional safety deviations
//!
//! 1. **Intel HEX oversized byte count.** The source computes
//!    `bytes[4..4 + byte_count]` without validating that the record actually
//!    carries that many bytes, so a truncated record **panics**. This module
//!    validates the length first and returns [`HexIngestError::RecordTooShort`].
//! 2. **S-Record bare `S` line.** The source calls `line.chars().nth(1).unwrap()`,
//!    which **panics** on a line consisting of only `S`. This module validates
//!    the minimum line shape and returns [`HexIngestError::MalformedLine`].
//!
//! Both were confirmed against the unmodified source via a disposable oracle.
//!
//! ## Contiguous reconstruction
//!
//! The returned `bytes` is a **contiguous reconstruction** of the sparse record
//! addresses, starting at `base_address`. Gaps between segments are filled with
//! the caller-supplied padding byte. It is therefore **not** a lossless copy of
//! the original text file.
//!
//! ## Not implemented (deferred)
//!
//! Physical-base rebasing (`to_rebased_full_bin`) and serialization/export are
//! deliberately out of scope. Physical addresses such as `0x80000000` are never
//! silently rebased.

use crate::ingest::MAX_BINARY_SIZE;
use std::collections::BTreeMap;
use thiserror::Error;

/// Upper bound on the reconstructed contiguous span.
///
/// Reuses the application's existing ROM size cap so HEX/SREC input cannot
/// allocate more than a raw binary would be allowed to.
pub const MAX_FIRMWARE_SPAN: usize = MAX_BINARY_SIZE;

/// Detected on-disk firmware container format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirmwareFormat {
    /// Intel HEX (`:`-prefixed records).
    IntelHex,
    /// Motorola S-Record (`S`-prefixed records).
    SRecord,
    /// Raw binary; bytes are passed through unchanged.
    RawBinary,
}

/// Outcome of ingesting a firmware container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HexIngestResult {
    /// Detected container format.
    pub format: FirmwareFormat,
    /// Lowest address present in the records, or `None` when the input carried
    /// no data records at all.
    ///
    /// `Some(0)` is a *known* base of zero — it is not interchangeable with
    /// `None`, which means "no address information". Raw binary always yields
    /// `None`: a bare `.bin` has no inherent absolute address.
    pub base_address: Option<u32>,
    /// Contiguous reconstruction; gaps filled with the requested padding byte.
    pub bytes: Vec<u8>,
    /// Number of parsed data records (0 for raw binary).
    pub record_count: usize,
    /// Entry point, only when a valid start-address record supplied one.
    pub entry_point: Option<u32>,
}

/// Errors produced by the ingest functions.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum HexIngestError {
    /// An Intel HEX record could not be parsed.
    #[error("{0}")]
    Hex(String),
    /// An S-Record could not be parsed.
    #[error("{0}")]
    Srec(String),
    /// A line was too short to carry a record type.
    #[error("Line {line}: record is too short to parse")]
    MalformedLine {
        /// 1-based line number.
        line: usize,
    },
    /// A record declared more payload bytes than it carried.
    ///
    /// This is the intentional deviation from the source, which panics here.
    #[error("Line {line}: declared byte count {declared} exceeds available payload {available}")]
    RecordTooShort {
        /// 1-based line number.
        line: usize,
        /// Byte count declared by the record.
        declared: usize,
        /// Payload bytes actually present.
        available: usize,
    },
    /// The contiguous address span would exceed [`MAX_FIRMWARE_SPAN`].
    #[error("Address span {span} bytes exceeds the {limit} byte limit")]
    AddressRangeTooLarge {
        /// Requested span in bytes.
        span: usize,
        /// Configured limit.
        limit: usize,
    },
}

// ---------------------------------------------------------------------------
// Format detection
// ---------------------------------------------------------------------------

/// Classifies `data` as Intel HEX, S-Record, or raw binary.
///
/// Detection inspects the first non-whitespace byte: `:` selects Intel HEX and
/// `S` selects S-Record. Everything else is treated as raw binary.
///
/// Note that a file beginning with `:` or `S` is classified as a text container
/// even if its body is malformed. That is deliberate: [`ingest_firmware`] then
/// surfaces a parse error instead of silently comparing ASCII bytes as
/// firmware.
pub fn detect_format(data: &[u8]) -> FirmwareFormat {
    match data.iter().find(|b| !b.is_ascii_whitespace()) {
        Some(b':') => FirmwareFormat::IntelHex,
        Some(b'S') => FirmwareFormat::SRecord,
        _ => FirmwareFormat::RawBinary,
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Converts a sparse map into a contiguous vector, enforcing the span limit.
///
/// Returns `None` for the base address when the map is empty: there were no
/// data records, so no address is known.
fn flatten(
    memory: &BTreeMap<u32, u8>,
    pad_byte: u8,
) -> Result<(Option<u32>, Vec<u8>), HexIngestError> {
    if memory.is_empty() {
        return Ok((None, Vec::new()));
    }

    let (&min_addr, _) = memory.iter().next().expect("non-empty");
    let (&max_addr, _) = memory.iter().next_back().expect("non-empty");

    let span = (max_addr as u64)
        .checked_sub(min_addr as u64)
        .and_then(|d| d.checked_add(1))
        .ok_or(HexIngestError::AddressRangeTooLarge {
            span: usize::MAX,
            limit: MAX_FIRMWARE_SPAN,
        })?;

    if span > MAX_FIRMWARE_SPAN as u64 {
        return Err(HexIngestError::AddressRangeTooLarge {
            span: span as usize,
            limit: MAX_FIRMWARE_SPAN,
        });
    }

    let total = span as usize;
    let mut flat = vec![pad_byte; total];
    for (&addr, &b) in memory {
        flat[(addr - min_addr) as usize] = b;
    }
    Ok((Some(min_addr), flat))
}

// ---------------------------------------------------------------------------
// Intel HEX
// ---------------------------------------------------------------------------

/// Parses Intel HEX text into a contiguous buffer.
///
/// Supported record types: data (`0x00`), end-of-file (`0x01`), extended
/// segment address (`0x02`), start segment address (`0x03`), extended linear
/// address (`0x04`), start linear address (`0x05`). Unrecognised types are
/// ignored, matching the source.
///
/// Unlike the source, a record whose declared byte count exceeds the available
/// payload returns [`HexIngestError::RecordTooShort`] instead of panicking.
pub fn ingest_hex_text(text: &str, pad_byte: u8) -> Result<HexIngestResult, HexIngestError> {
    let mut memory: BTreeMap<u32, u8> = BTreeMap::new();
    let mut base_upper_address: u32 = 0;
    let mut entry_point: Option<u32> = None;
    let mut record_count = 0usize;

    for (idx, raw_line) in text.lines().enumerate() {
        let line_num = idx + 1;
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if !line.starts_with(':') {
            return Err(HexIngestError::Hex(format!(
                "Line {}: missing leading colon ':'",
                line_num
            )));
        }
        if !line.is_ascii() {
            return Err(HexIngestError::Hex(format!(
                "Line {}: contains non-ASCII characters",
                line_num
            )));
        }

        let hex_str = &line[1..];
        if hex_str.len() < 10 || hex_str.len() % 2 != 0 {
            return Err(HexIngestError::Hex(format!(
                "Line {}: invalid line length",
                line_num
            )));
        }

        let raw_bytes: Result<Vec<u8>, _> = (0..hex_str.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex_str[i..i + 2], 16))
            .collect();
        let bytes = raw_bytes.map_err(|e| {
            HexIngestError::Hex(format!("Line {}: invalid hex digit: {}", line_num, e))
        })?;

        // Two's complement checksum: sum of all bytes (incl. checksum) == 0.
        let sum: u8 = bytes.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
        if sum != 0 {
            return Err(HexIngestError::Hex(format!(
                "Line {}: checksum mismatch (sum = 0x{:02X})",
                line_num, sum
            )));
        }

        let byte_count = bytes[0] as usize;

        // Every Intel HEX record requires len(1) + addr(2) + type(1) + data[byte_count] + checksum(1).
        if bytes.len() < 5 + byte_count {
            return Err(HexIngestError::RecordTooShort {
                line: line_num,
                declared: byte_count,
                available: bytes.len().saturating_sub(5),
            });
        }
        if bytes.len() != 5 + byte_count {
            return Err(HexIngestError::Hex(format!(
                "Line {}: unexpected trailing bytes (declared {}, record bytes {})",
                line_num,
                byte_count,
                bytes.len()
            )));
        }

        let offset_addr = u16::from_be_bytes([bytes[1], bytes[2]]) as u32;
        let record_type = bytes[3];
        let data = &bytes[4..4 + byte_count];

        match record_type {
            0x00 => {
                let full_addr = base_upper_address.wrapping_add(offset_addr);
                for (i, &b) in data.iter().enumerate() {
                    let addr = full_addr.wrapping_add(i as u32);
                    memory.insert(addr, b);
                }
                record_count += 1;
            }
            0x01 => break,
            0x02 => {
                if data.len() == 2 {
                    let segment = u16::from_be_bytes([data[0], data[1]]) as u32;
                    base_upper_address = segment << 4;
                }
            }
            0x04 => {
                if data.len() == 2 {
                    let upper = u16::from_be_bytes([data[0], data[1]]) as u32;
                    base_upper_address = upper << 16;
                }
            }
            0x05 => {
                if let [a, b, c, d] = data {
                    entry_point = Some(u32::from_be_bytes([*a, *b, *c, *d]));
                }
            }
            _ => {}
        }
    }

    let (base_address, bytes) = flatten(&memory, pad_byte)?;
    Ok(HexIngestResult {
        format: FirmwareFormat::IntelHex,
        base_address,
        bytes,
        record_count,
        entry_point,
    })
}

// ---------------------------------------------------------------------------
// Motorola S-Record
// ---------------------------------------------------------------------------

/// Parses Motorola S-Record text into a contiguous buffer.
///
/// Supported types: `S0` header, `S1` (16-bit), `S2` (24-bit), `S3` (32-bit)
/// data, and `S7`/`S8`/`S9` termination. Other types are ignored, matching the
/// source.
///
/// Unlike the source, a line too short to carry a record type returns
/// [`HexIngestError::MalformedLine`] instead of panicking.
pub fn ingest_srec_text(text: &str, pad_byte: u8) -> Result<HexIngestResult, HexIngestError> {
    let mut memory: BTreeMap<u32, u8> = BTreeMap::new();
    let mut record_count = 0usize;

    for (idx, raw_line) in text.lines().enumerate() {
        let line_num = idx + 1;
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if !line.starts_with('S') {
            return Err(HexIngestError::Srec(format!(
                "Line {}: missing leading 'S'",
                line_num
            )));
        }

        // SAFETY DEVIATION: the source calls `line.chars().nth(1).unwrap()`,
        // which panics on a bare "S". Validate the shape first.
        if !line.is_ascii() || line.len() < 2 {
            return Err(HexIngestError::MalformedLine { line: line_num });
        }
        let record_type = line.as_bytes()[1] as char;

        let hex_payload = &line[2..];
        if hex_payload.len() < 4 || hex_payload.len() % 2 != 0 {
            return Err(HexIngestError::Srec(format!(
                "Line {}: invalid payload length",
                line_num
            )));
        }

        let raw_bytes: Result<Vec<u8>, _> = (0..hex_payload.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex_payload[i..i + 2], 16))
            .collect();
        let bytes = raw_bytes
            .map_err(|e| HexIngestError::Srec(format!("Line {}: invalid hex: {}", line_num, e)))?;

        // One's complement checksum: sum of all bytes == 0xFF.
        let sum: u8 = bytes.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
        if sum != 0xFF {
            return Err(HexIngestError::Srec(format!(
                "Line {}: checksum mismatch (sum = 0x{:02X}, expected 0xFF)",
                line_num, sum
            )));
        }

        let count = bytes[0] as usize;
        if bytes.len() != count + 1 {
            return Err(HexIngestError::Srec(format!(
                "Line {}: byte count field mismatch",
                line_num
            )));
        }

        match record_type {
            '0' => {
                // Header comment; carries no addressable data.
            }
            '1' => {
                if bytes.len() < 4 {
                    return Err(HexIngestError::RecordTooShort {
                        line: line_num,
                        declared: count,
                        available: bytes.len().saturating_sub(1),
                    });
                }
                let addr = u16::from_be_bytes([bytes[1], bytes[2]]) as u32;
                let data = &bytes[3..bytes.len() - 1];
                for (i, &b) in data.iter().enumerate() {
                    memory.insert(addr.wrapping_add(i as u32), b);
                }
                record_count += 1;
            }
            '2' => {
                if bytes.len() < 5 {
                    return Err(HexIngestError::RecordTooShort {
                        line: line_num,
                        declared: count,
                        available: bytes.len().saturating_sub(1),
                    });
                }
                let addr = ((bytes[1] as u32) << 16) | ((bytes[2] as u32) << 8) | (bytes[3] as u32);
                let data = &bytes[4..bytes.len() - 1];
                for (i, &b) in data.iter().enumerate() {
                    memory.insert(addr.wrapping_add(i as u32), b);
                }
                record_count += 1;
            }
            '3' => {
                if bytes.len() < 6 {
                    return Err(HexIngestError::RecordTooShort {
                        line: line_num,
                        declared: count,
                        available: bytes.len().saturating_sub(1),
                    });
                }
                let addr = u32::from_be_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]);
                let data = &bytes[5..bytes.len() - 1];
                for (i, &b) in data.iter().enumerate() {
                    memory.insert(addr.wrapping_add(i as u32), b);
                }
                record_count += 1;
            }
            '7' | '8' | '9' => break,
            _ => {}
        }
    }

    let (base_address, bytes) = flatten(&memory, pad_byte)?;
    Ok(HexIngestResult {
        format: FirmwareFormat::SRecord,
        base_address,
        bytes,
        record_count,
        entry_point: None,
    })
}

// ---------------------------------------------------------------------------
// Unified entry point
// ---------------------------------------------------------------------------

/// Detects the container format and parses accordingly.
///
/// Raw binary input is returned byte-for-byte unchanged with
/// [`FirmwareFormat::RawBinary`]. Text that begins with `:` or `S` is always
/// treated as HEX/SREC and will surface a parse error rather than being
/// compared as ASCII.
pub fn ingest_firmware(data: &[u8], pad_byte: u8) -> Result<HexIngestResult, HexIngestError> {
    match detect_format(data) {
        FirmwareFormat::IntelHex => {
            let text = std::str::from_utf8(data)
                .map_err(|e| HexIngestError::Hex(format!("input is not valid UTF-8: {}", e)))?;
            ingest_hex_text(text, pad_byte)
        }
        FirmwareFormat::SRecord => {
            let text = std::str::from_utf8(data)
                .map_err(|e| HexIngestError::Srec(format!("input is not valid UTF-8: {}", e)))?;
            ingest_srec_text(text, pad_byte)
        }
        FirmwareFormat::RawBinary => Ok(HexIngestResult {
            format: FirmwareFormat::RawBinary,
            // A raw BIN has no inherent absolute address.
            base_address: None,
            bytes: data.to_vec(),
            record_count: 0,
            entry_point: None,
        }),
    }
}
