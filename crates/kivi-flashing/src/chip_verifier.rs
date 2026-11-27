//! Ported from `ecu_engine_rust::flashing::chip_verifier`.
//!
//! Pure byte-level protocol logic: builders, parsers, encoders, state machines.
//! No I/O, no device access, no `unsafe`. Transport is the caller's concern.
//!
//! Verified by unit tests with hand-derived expectations. **Not** validated
//! against a physical ECU; no such claim is made.

/// Result of a Chip or Sector Blank Check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlankCheckResult {
    Blank, // All bytes are 0xFF (erased floating gates)
    NotBlank { first_non_blank_offset: usize, found_byte: u8 },
}

/// Discrepancy Record between Local Calibration Buffer and Physical Chip Mem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyDifference {
    pub offset: usize,
    pub expected_local_byte: u8,
    pub actual_chip_byte: u8,
}

/// Chip Blank Check & Verification Engine.
pub struct ChipVerifier;

impl ChipVerifier {
    /// Perform a high-speed blank check across a memory slice.
    /// Unprogrammed / erased NOR Flash or UV-EPROM must read 0xFF.
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

    /// Compare local edit buffer against data read back from physical chip.
    /// Reconstructs PromIO verification pass.
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

        // If buffers have mismatched length, report that too
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
}
