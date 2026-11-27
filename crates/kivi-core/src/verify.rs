//! Chip / sector verification primitives.
//!
//! Ported from `ecu_engine_rust::flashing::chip_verifier` (source of truth).
//! Behavior is preserved exactly, including two non-obvious edge cases that are
//! covered by parity tests against the original implementation:
//!
//! 1. `verify_buffer` compares only `min(local.len(), chip.len())` bytes.
//!    When the buffers differ in length *and* the compared prefix matches, a
//!    single synthetic `VerifyDifference` is emitted at `offset = cmp_len`
//!    with `unwrap_or(0)` bytes for the missing side.
//! 2. `max_differences_to_record == 0` still records one difference, because
//!    the limit check runs *after* the push. This mirrors the source exactly
//!    and must not be "fixed" without a deliberate, documented decision.
//!
//! No I/O, no allocation beyond the returned `Vec`, fully deterministic.

/// Result of a blank check over a memory slice.
///
/// Erased NOR flash and UV-EPROM read as `0xFF`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlankCheckResult {
    /// Every byte is `0xFF`.
    Blank,
    /// The first byte that is not `0xFF`.
    NotBlank {
        /// Byte offset of the first non-`0xFF` value.
        first_non_blank_offset: usize,
        /// The offending byte.
        found_byte: u8,
    },
}

/// A single byte-level discrepancy between a local buffer and chip contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyDifference {
    /// Offset of the difference (or the comparison length for a length mismatch).
    pub offset: usize,
    /// Byte value in the local (expected) buffer.
    pub expected_local_byte: u8,
    /// Byte value read back from the chip.
    pub actual_chip_byte: u8,
}

/// Scans `data` for the first byte that is not `0xFF`.
///
/// Returns [`BlankCheckResult::Blank`] for an empty slice.
pub fn check_blank(data: &[u8]) -> BlankCheckResult {
    for (offset, &b) in data.iter().enumerate() {
        if b != 0xFF {
            return BlankCheckResult::NotBlank {
                first_non_blank_offset: offset,
                found_byte: b,
            };
        }
    }
    BlankCheckResult::Blank
}

/// Compares a local edit buffer against data read back from the chip.
///
/// Returns `Ok(())` when the buffers are byte-identical over the compared range
/// and have equal length. Otherwise returns the recorded differences.
///
/// `max_differences_to_record` caps the number of recorded differences; see the
/// module documentation for the exact semantics inherited from the source.
pub fn verify_buffer(
    local_buffer: &[u8],
    chip_buffer: &[u8],
    max_differences_to_record: usize,
) -> Result<(), Vec<VerifyDifference>> {
    let cmp_len = local_buffer.len().min(chip_buffer.len());
    let mut differences = Vec::new();

    for offset in 0..cmp_len {
        let exp = local_buffer[offset];
        let act = chip_buffer[offset];
        if exp != act {
            differences.push(VerifyDifference {
                offset,
                expected_local_byte: exp,
                actual_chip_byte: act,
            });
            if differences.len() >= max_differences_to_record {
                break;
            }
        }
    }

    // Length mismatch with an identical compared prefix: report a synthetic
    // difference at the comparison boundary, matching the source behavior.
    if local_buffer.len() != chip_buffer.len() && differences.is_empty() {
        differences.push(VerifyDifference {
            offset: cmp_len,
            expected_local_byte: local_buffer.get(cmp_len).copied().unwrap_or(0),
            actual_chip_byte: chip_buffer.get(cmp_len).copied().unwrap_or(0),
        });
    }

    if differences.is_empty() {
        Ok(())
    } else {
        Err(differences)
    }
}
