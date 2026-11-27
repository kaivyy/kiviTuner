//! VAG BCB run-length codec.
//!
//! Ported from `ecu_engine_rust::formats::mod.rs::SgoContainerParser`
//! (`bcb_compress`, `bcb_decompress`).
//!
//! ## Encoding contract (preserved exactly)
//!
//! ```text
//! magic  : 0x1A 0x01
//! token  : u16 big-endian;  flag = token >> 14,  length = token & 0x3FFF
//!   flag 0 -> literal: `length` raw bytes follow
//!   flag 1 -> run:     one repeat byte follows, emitted `length` times
//!   flag 3 -> end:     24-bit additive checksum of the output follows
//! ```
//!
//! The compressor emits a run token only for runs of at least 4 bytes;
//! everything else accumulates into a literal token.
//!
//! ## Deviation — allocation guard
//!
//! The source grows the output by an attacker-controlled `length` with no cap.
//! A two-byte RLE token can request 16 383 bytes, so a few hundred bytes of input
//! can demand hundreds of megabytes. This version enforces
//! [`MAX_DECOMPRESSED_SIZE`] and returns [`BcbError::OutputTooLarge`] rather than
//! allocating without bound.
//!
//! This is a safety limit, not a format change: any stream that would produce a
//! legitimate firmware image is far below it.
//!
//! ## Determinism
//!
//! Both directions are deterministic and perform no I/O.

use thiserror::Error;

/// Upper bound on decompressed output.
///
/// Chosen well above any real ECU image (the largest supported ROM is 64 MiB) so
/// it never rejects legitimate data, while still preventing a decompression bomb.
pub const MAX_DECOMPRESSED_SIZE: usize = 64 * 1024 * 1024;

/// Errors from the BCB codec.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BcbError {
    /// Header is missing or too short.
    #[error("Invalid SGO BCB magic header (expected 0x1A 0x01)")]
    InvalidMagic,
    /// A literal token claims more bytes than remain.
    #[error("BCB literal token exceeds buffer bounds")]
    LiteralOverrun,
    /// An RLE token has no repeat byte.
    #[error("BCB RLE token missing repeat byte")]
    MissingRepeatByte,
    /// The end token's 24-bit checksum is incomplete.
    #[error("BCB end token missing 24-bit checksum")]
    TruncatedChecksum,
    /// The stream ended without an end token.
    #[error("BCB stream ended without termination token (flag=3)")]
    MissingEndToken,
    /// A token used an undefined flag.
    #[error("Unknown BCB token flag {0}")]
    UnknownTokenFlag(u8),
    /// The stored checksum does not match the decoded output.
    #[error("BCB 24-bit checksum mismatch: calculated 0x{calculated:06X} != expected 0x{expected:06X}")]
    ChecksumMismatch {
        /// Checksum computed from the decoded bytes.
        calculated: u32,
        /// Checksum stored in the stream.
        expected: u32,
    },
    /// Decoding would exceed [`MAX_DECOMPRESSED_SIZE`].
    #[error("BCB output would exceed the {limit}-byte decompression limit")]
    OutputTooLarge {
        /// The configured limit.
        limit: usize,
    },
}

/// Compresses `raw` into a BCB stream.
pub fn bcb_compress(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len() + 16);
    out.push(0x1A);
    out.push(0x01);

    let mut pos = 0;
    while pos < raw.len() {
        let b = raw[pos];
        let mut run_len = 1usize;
        while pos + run_len < raw.len() && raw[pos + run_len] == b && run_len < 0x3FFF {
            run_len += 1;
        }
        if run_len >= 4 {
            let tok: u16 = (1u16 << 14) | (run_len as u16);
            out.extend_from_slice(&tok.to_be_bytes());
            out.push(b);
            pos += run_len;
        } else {
            let start = pos;
            let mut lit_len = 0usize;
            while pos < raw.len() && lit_len < 0x3FFF {
                if pos + 4 <= raw.len()
                    && raw[pos] == raw[pos + 1]
                    && raw[pos] == raw[pos + 2]
                    && raw[pos] == raw[pos + 3]
                {
                    break;
                }
                pos += 1;
                lit_len += 1;
            }
            let tok: u16 = lit_len as u16;
            out.extend_from_slice(&tok.to_be_bytes());
            out.extend_from_slice(&raw[start..start + lit_len]);
        }
    }

    out.extend_from_slice(&0xC000u16.to_be_bytes());
    let sum24: u32 = raw
        .iter()
        .map(|&x| x as u32)
        .fold(0u32, |a, b| a.wrapping_add(b))
        & 0x00FF_FFFF;
    out.push(((sum24 >> 16) & 0xFF) as u8);
    out.push(((sum24 >> 8) & 0xFF) as u8);
    out.push((sum24 & 0xFF) as u8);
    out
}

/// Decompresses a BCB stream and validates its 24-bit checksum.
pub fn bcb_decompress(buf: &[u8]) -> Result<Vec<u8>, BcbError> {
    if buf.len() < 7 || buf[0..2] != [0x1A, 0x01] {
        return Err(BcbError::InvalidMagic);
    }
    let mut out = Vec::new();
    let mut pos = 2usize;

    while pos + 2 <= buf.len() {
        let tok = u16::from_be_bytes([buf[pos], buf[pos + 1]]);
        pos += 2;
        let flag = (tok >> 14) as u8;
        let length = (tok & 0x3FFF) as usize;

        match flag {
            0 => {
                if pos + length > buf.len() {
                    return Err(BcbError::LiteralOverrun);
                }
                if out.len() + length > MAX_DECOMPRESSED_SIZE {
                    return Err(BcbError::OutputTooLarge {
                        limit: MAX_DECOMPRESSED_SIZE,
                    });
                }
                out.extend_from_slice(&buf[pos..pos + length]);
                pos += length;
            }
            1 => {
                if pos >= buf.len() {
                    return Err(BcbError::MissingRepeatByte);
                }
                // Guard before resizing: `length` comes from the stream.
                if out.len() + length > MAX_DECOMPRESSED_SIZE {
                    return Err(BcbError::OutputTooLarge {
                        limit: MAX_DECOMPRESSED_SIZE,
                    });
                }
                let repeat_byte = buf[pos];
                pos += 1;
                out.resize(out.len() + length, repeat_byte);
            }
            3 => {
                if pos + 3 > buf.len() {
                    return Err(BcbError::TruncatedChecksum);
                }
                let expected = ((buf[pos] as u32) << 16)
                    | ((buf[pos + 1] as u32) << 8)
                    | (buf[pos + 2] as u32);
                let calculated = out
                    .iter()
                    .map(|&x| x as u32)
                    .fold(0u32, |a, b| a.wrapping_add(b))
                    & 0x00FF_FFFF;
                if calculated != expected {
                    return Err(BcbError::ChecksumMismatch {
                        calculated,
                        expected,
                    });
                }
                return Ok(out);
            }
            other => return Err(BcbError::UnknownTokenFlag(other)),
        }
    }

    Err(BcbError::MissingEndToken)
}
