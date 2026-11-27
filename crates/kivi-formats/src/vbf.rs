//! VBF (Versatile Binary Format) container codec.
//!
//! Ported from `ecu_engine_rust::formats::mod.rs::VbfContainerCodec`
//! (`serialize_to_vbf`, `parse_vbf_segments`) and `FirmwareMemorySegment`.
//!
//! ## Container layout (as implemented by the source)
//!
//! ```text
//! vbf_version = 2.4;
//! header {
//!     sw_part_number = "<id>";
//!     num_segments = <n>;
//! }
//! <per segment>
//!   start_address  u32 big-endian
//!   data_len       u32 big-endian
//!   data           data_len bytes
//!   crc16_ccitt    u16 big-endian
//! ```
//!
//! ## Fidelity note
//!
//! This reproduces the *source's* VBF dialect, which is a simplified subset: an
//! ASCII header followed by length-prefixed segments. Real OEM VBF files may
//! carry additional header keys and block descriptors that this codec neither
//! parses nor preserves. **No claim is made about compatibility with real OEM
//! files** — no authorized fixture has been examined.
//!
//! ## Safety
//!
//! The parser validates every length before slicing, so a truncated or hostile
//! input yields a typed error rather than a panic or an oversized allocation.
//! This is a deliberate improvement over the source, which indexes directly.

use kivi_checksum::primitives::crc16_ccitt;
use thiserror::Error;

/// Errors from the VBF codec.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum VbfError {
    /// The ASCII header has no closing brace.
    #[error("Invalid VBF file: ASCII header closing brace '}}' not found")]
    MissingHeaderTerminator,
    /// Fewer than 8 bytes remain where a segment header is required.
    #[error("Truncated VBF binary segment header")]
    TruncatedSegmentHeader,
    /// The declared payload length exceeds the remaining bytes.
    #[error("Truncated VBF binary segment payload or CRC16")]
    TruncatedPayload,
    /// The stored CRC does not match the payload.
    #[error("VBF segment CRC16-CCITT verification mismatch")]
    CrcMismatch,
}

/// One memory segment inside a VBF container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirmwareMemorySegment {
    /// Load address of the segment.
    pub start_address: u32,
    /// Segment payload.
    pub data: Vec<u8>,
    /// CRC-16/CCITT-FALSE over `data`.
    pub crc16_ccitt: u16,
}

/// Serializes segments into a VBF container.
///
/// The `crc16_ccitt` field of each supplied segment is **recomputed** from its
/// data, so a stale CRC cannot be written out.
pub fn serialize_to_vbf(sw_part_number: &str, segments: &[FirmwareMemorySegment]) -> Vec<u8> {
    let header = format!(
        "vbf_version = 2.4;\nheader {{\n    sw_part_number = \"{}\";\n    num_segments = {};\n}}",
        sw_part_number,
        segments.len()
    );    let mut out = Vec::new();
    out.extend_from_slice(header.as_bytes());

    for seg in segments {
        out.extend_from_slice(&seg.start_address.to_be_bytes());
        out.extend_from_slice(&(seg.data.len() as u32).to_be_bytes());
        out.extend_from_slice(&seg.data);
        let crc = crc16_ccitt(&seg.data);
        out.extend_from_slice(&crc.to_be_bytes());
    }

    out
}

/// Parses a VBF container into its header text and segments.
///
/// Returns the ASCII header as a string and the segments in file order. Every
/// declared length is validated against the remaining bytes before slicing.
pub fn parse_vbf_segments(
    vbf_bytes: &[u8],
) -> Result<(String, Vec<FirmwareMemorySegment>), VbfError> {
    // Locate the closing brace of the ASCII header block.
    let mut brace_depth = 0usize;
    let mut header_end = None;
    for (i, &b) in vbf_bytes.iter().enumerate() {
        if b == b'{' {
            brace_depth += 1;
        } else if b == b'}' {
            if brace_depth == 1 {
                header_end = Some(i + 1);
                break;
            } else if brace_depth > 1 {
                brace_depth -= 1;
            }
        }
    }

    let bin_start = header_end.ok_or(VbfError::MissingHeaderTerminator)?;
    let header_str = String::from_utf8_lossy(&vbf_bytes[..bin_start]).to_string();

    let mut cursor = bin_start;
    let mut segments = Vec::new();

    while cursor < vbf_bytes.len() {
        if cursor + 8 > vbf_bytes.len() {
            return Err(VbfError::TruncatedSegmentHeader);
        }
        let start_address = u32::from_be_bytes([
            vbf_bytes[cursor],
            vbf_bytes[cursor + 1],
            vbf_bytes[cursor + 2],
            vbf_bytes[cursor + 3],
        ]);
        let len = u32::from_be_bytes([
            vbf_bytes[cursor + 4],
            vbf_bytes[cursor + 5],
            vbf_bytes[cursor + 6],
            vbf_bytes[cursor + 7],
        ]) as usize;
        cursor += 8;

        // Validate before slicing: `len` comes from the file and may be hostile.
        let payload_end = cursor
            .checked_add(len)
            .ok_or(VbfError::TruncatedPayload)?;
        if payload_end + 2 > vbf_bytes.len() {
            return Err(VbfError::TruncatedPayload);
        }
        let data = vbf_bytes[cursor..payload_end].to_vec();
        cursor = payload_end;

        let expected_crc = u16::from_be_bytes([vbf_bytes[cursor], vbf_bytes[cursor + 1]]);
        cursor += 2;

        let calc_crc = crc16_ccitt(&data);
        if calc_crc != expected_crc {
            return Err(VbfError::CrcMismatch);
        }

        segments.push(FirmwareMemorySegment {
            start_address,
            data,
            crc16_ccitt: calc_crc,
        });
    }

    Ok((header_str, segments))
}
