//! Denso SH7xxx / SH705x / SH72543 descriptor-table checksum.
//!
//! ## Format
//!
//! Subaru Denso SH705x/SH7xxx/EE20 ROMs store checksums in 12-byte big-endian records:
//! ```text
//! [range_start : 4 B big-endian]
//! [range_end   : 4 B big-endian]
//! [stored_sum  : 4 B big-endian]
//! ```
//!
//! Real Subaru ROMs and SH7xxx XML definitions use an **inclusive** `range_end` (`end % 4 == 3`, e.g.
//! `0x0FFFFF`), summing 32-bit big-endian words across `[start, end + 1)`.
//! Some legacy synthetic fixtures pass an **exclusive** 4-byte-aligned `range_end`
//! (`end % 4 == 0`); [`resolve_effective_end`] supports both while rejecting unaligned ends.
//!
//! Verification passes when `sum.wrapping_add(stored) == 0x5AA5A55A`. A `stored` value of
//! `0x5AA5A55A` marks a disabled record.
//!
//! Also includes Subaru SH705x Diesel word-substitution overrides from FastECU's
//! `checksum_ecu_subaru_denso_sh705x_diesel.cpp` and O(N) table discovery via prefix sums.

/// Length of one descriptor record, in bytes.
pub const DESCRIPTOR_RECORD_LEN: usize = 12;

/// The target value `sum + stored`, and the "disabled" record marker.
pub const DISABLED_MAGIC: u32 = 0x5AA5_A55A;

/// One descriptor record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DescriptorEntry {
    /// First byte of the summed range.
    pub start: u32,
    /// End byte of the summed range (exclusive if `% 4 == 0`, inclusive if `% 4 == 3`).
    pub end: u32,
    /// Stored value at `[record + 8..record + 12]`.
    pub stored: u32,
}

/// Errors returned while validating or updating Denso descriptor tables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DescriptorError {
    /// A record's range extends past the end of the ROM.
    OutOfRange { start: u32, end: u32, rom_len: usize },
    /// A record's range is inverted or empty.
    InvertedRange { start: u32, end: u32 },
    /// Start is not 4-byte aligned, or end is neither `% 4 == 0` nor inclusive `% 4 == 3`.
    UnalignedRange { start: u32, end: u32 },
}

impl std::fmt::Display for DescriptorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DescriptorError::OutOfRange {
                start,
                end,
                rom_len,
            } => write!(
                f,
                "descriptor range {start:#X}..{end:#X} exceeds ROM length {rom_len}"
            ),
            DescriptorError::InvertedRange { start, end } => {
                write!(f, "descriptor range {start:#X}..{end:#X} is empty or inverted")
            }
            DescriptorError::UnalignedRange { start, end } => {
                write!(f, "descriptor range {start:#X}..{end:#X} is not 4-byte aligned")
            }
        }
    }
}

impl std::error::Error for DescriptorError {}

/// Resolves exclusive byte end from either exclusive (`% 4 == 0`) or inclusive (`% 4 == 3`).
#[inline]
pub fn resolve_effective_end(start: u32, end: u32) -> Result<usize, DescriptorError> {
    if !start.is_multiple_of(4) {
        return Err(DescriptorError::UnalignedRange { start, end });
    }
    let excl = if end.is_multiple_of(4) {
        end as usize
    } else if end % 4 == 3 {
        (end as usize).saturating_add(1)
    } else {
        return Err(DescriptorError::UnalignedRange { start, end });
    };
    if excl <= start as usize {
        return Err(DescriptorError::InvertedRange { start, end });
    }
    Ok(excl)
}

/// Sums the 32-bit big-endian words covering `[start, effective_end)`, wrapping at 32 bits.
///
/// Handles both exclusive (`end % 4 == 0`) and real Subaru inclusive (`end % 4 == 3`) bounds.
pub fn sum_words(rom: &[u8], start: usize, end: usize) -> u32 {
    let eff_end = if end % 4 == 3 && end < rom.len() {
        end + 1
    } else {
        end
    };
    let mut sum: u32 = 0;
    let mut i = start;
    while i + 4 <= eff_end && i + 4 <= rom.len() {
        sum = sum.wrapping_add(u32::from_be_bytes([
            rom[i],
            rom[i + 1],
            rom[i + 2],
            rom[i + 3],
        ]));
        i += 4;
    }
    sum
}

/// Sums words with Subaru SH705x Diesel self-reference word overrides applied.
///
/// Mirrors FastECU `ChecksumEcuSubaruDensoSH705xDiesel`:
/// - Table at `0x0FFB80`: word at `0x0FFAFC` contributes `0xFFFF_FFFF`.
/// - Table at `0x17FB80`: word at `0x17FAFC` contributes `0xFFFF_FFFF`.
pub fn sum_words_diesel(rom: &[u8], start: usize, end: usize, table_offset: usize) -> u32 {
    let eff_end = if end % 4 == 3 && end < rom.len() {
        end + 1
    } else {
        end
    };
    let mut sum: u32 = 0;
    let mut i = start;
    while i + 4 <= eff_end && i + 4 <= rom.len() {
        let word = if (table_offset == 0x0FFB80 && i == 0x0FFAFC)
            || (table_offset == 0x17FB80 && i == 0x17FAFC)
        {
            0xFFFF_FFFF
        } else {
            u32::from_be_bytes([rom[i], rom[i + 1], rom[i + 2], rom[i + 3]])
        };
        sum = sum.wrapping_add(word);
        i += 4;
    }
    sum
}

/// Returns the stored field expected for a descriptor entry.
pub fn expected_stored(rom: &[u8], entry: &DescriptorEntry) -> u32 {
    let excl = resolve_effective_end(entry.start, entry.end).unwrap_or(entry.end as usize);
    DISABLED_MAGIC.wrapping_sub(sum_words(rom, entry.start as usize, excl))
}

fn validate(rom: &[u8], entry: &DescriptorEntry) -> Result<usize, DescriptorError> {
    if entry.end <= entry.start {
        return Err(DescriptorError::InvertedRange {
            start: entry.start,
            end: entry.end,
        });
    }
    let excl = resolve_effective_end(entry.start, entry.end)?;
    if excl > rom.len() {
        return Err(DescriptorError::OutOfRange {
            start: entry.start,
            end: entry.end,
            rom_len: rom.len(),
        });
    }
    Ok(excl)
}

/// True when every enabled record's stored value matches its range.
pub fn verify_descriptor_checksums(
    rom: &[u8],
    entries: &[DescriptorEntry],
) -> Result<bool, DescriptorError> {
    for entry in entries {
        let excl = validate(rom, entry)?;
        if entry.stored == DISABLED_MAGIC {
            continue;
        }
        let expected = DISABLED_MAGIC.wrapping_sub(sum_words(rom, entry.start as usize, excl));
        if entry.stored != expected {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Verifies Subaru SH705x/SH72543 Diesel descriptor table checksums (with word overrides).
pub fn verify_diesel_descriptor_checksums(
    rom: &[u8],
    entries: &[DescriptorEntry],
    table_offset: usize,
) -> Result<bool, DescriptorError> {
    if all_checksums_disabled(entries) {
        return Ok(true);
    }
    for entry in entries {
        if entry.start == 0 && entry.end == 0 && entry.stored == DISABLED_MAGIC {
            continue;
        }
        let excl = validate(rom, entry)?;
        if entry.stored == DISABLED_MAGIC {
            continue;
        }
        let expected =
            DISABLED_MAGIC.wrapping_sub(sum_words_diesel(rom, entry.start as usize, excl, table_offset));
        if entry.stored != expected {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Returns a copy of `rom` with stored fields repaired (assumes table at offset 0).
pub fn correct_descriptor_checksums(
    rom: &[u8],
    entries: &[DescriptorEntry],
) -> Result<Vec<u8>, DescriptorError> {
    correct_descriptor_checksums_at(rom, entries, 0)
}

/// Returns a copy of `rom` with stored fields repaired at explicit `table_offset`.
pub fn correct_descriptor_checksums_at(
    rom: &[u8],
    entries: &[DescriptorEntry],
    table_offset: usize,
) -> Result<Vec<u8>, DescriptorError> {
    correct_descriptor_checksums_with_mode(rom, entries, table_offset, false)
}

/// Repairs either standard Denso descriptors or Subaru SH705x diesel descriptors.
pub fn correct_descriptor_checksums_with_mode(
    rom: &[u8],
    entries: &[DescriptorEntry],
    table_offset: usize,
    diesel_mode: bool,
) -> Result<Vec<u8>, DescriptorError> {
    if all_checksums_disabled(entries) {
        return Ok(rom.to_vec());
    }
    let mut out = rom.to_vec();
    for (i, entry) in entries.iter().enumerate() {
        if entry.start == 0 && entry.end == 0 && entry.stored == DISABLED_MAGIC {
            continue;
        }
        let excl = validate(&out, entry)?;
        if entry.stored == DISABLED_MAGIC {
            continue;
        }
        let sum = if diesel_mode {
            sum_words_diesel(&out, entry.start as usize, excl, table_offset)
        } else {
            sum_words(&out, entry.start as usize, excl)
        };
        let expected = DISABLED_MAGIC.wrapping_sub(sum);
        let field = table_offset
            .checked_add(i.checked_mul(DESCRIPTOR_RECORD_LEN).ok_or(DescriptorError::OutOfRange {
                start: table_offset as u32,
                end: u32::MAX,
                rom_len: out.len(),
            })?)
            .and_then(|o| o.checked_add(8))
            .ok_or(DescriptorError::OutOfRange {
                start: table_offset as u32,
                end: u32::MAX,
                rom_len: out.len(),
            })?;
        if field.checked_add(4).is_none_or(|e| e > out.len()) {
            return Err(DescriptorError::OutOfRange {
                start: field as u32,
                end: field.saturating_add(4) as u32,
                rom_len: out.len(),
            });
        }
        out[field..field + 4].copy_from_slice(&expected.to_be_bytes());
    }
    Ok(out)
}

/// Parses `count` 12-byte descriptor records starting at `offset`.
pub fn parse_descriptor_table(
    rom: &[u8],
    offset: usize,
    count: usize,
) -> Result<Vec<DescriptorEntry>, DescriptorError> {
    let total_bytes = count
        .checked_mul(DESCRIPTOR_RECORD_LEN)
        .ok_or(DescriptorError::OutOfRange {
            start: offset as u32,
            end: u32::MAX,
            rom_len: rom.len(),
        })?;
    let need = offset
        .checked_add(total_bytes)
        .ok_or(DescriptorError::OutOfRange {
            start: offset as u32,
            end: u32::MAX,
            rom_len: rom.len(),
        })?;
    if need > rom.len() {
        return Err(DescriptorError::OutOfRange {
            start: offset as u32,
            end: need as u32,
            rom_len: rom.len(),
        });
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let p = offset + i * DESCRIPTOR_RECORD_LEN;
        let start = u32::from_be_bytes([rom[p], rom[p + 1], rom[p + 2], rom[p + 3]]);
        let end = u32::from_be_bytes([rom[p + 4], rom[p + 5], rom[p + 6], rom[p + 7]]);
        let stored = u32::from_be_bytes([rom[p + 8], rom[p + 9], rom[p + 10], rom[p + 11]]);
        out.push(DescriptorEntry { start, end, stored });
    }
    Ok(out)
}

/// True when the first record is `(0, 0, DISABLED_MAGIC)`.
pub fn all_checksums_disabled(entries: &[DescriptorEntry]) -> bool {
    entries
        .first()
        .is_some_and(|e| e.start == 0 && e.end == 0 && e.stored == DISABLED_MAGIC)
}

/// Discovers a Subaru Denso inclusive-end descriptor table anywhere in `rom` using O(N) prefix sums.
///
/// Mirrors `openremap/_rs/src/checksums/denso.rs`: looks for >= 3 consecutive verifying/valid
/// 12-byte descriptor entries (max 32 entries).
pub fn discover_denso_descriptor_table(rom: &[u8]) -> Option<(usize, Vec<DescriptorEntry>)> {
    if rom.len() < DESCRIPTOR_RECORD_LEN * 3 {
        return None;
    }
    let wcount = rom.len() / 4;
    let mut prefix = vec![0u32; wcount + 1];
    for k in 0..wcount {
        let p = k * 4;
        let w = u32::from_be_bytes([rom[p], rom[p + 1], rom[p + 2], rom[p + 3]]);
        prefix[k + 1] = prefix[k].wrapping_add(w);
    }

    let classify = |off: usize| -> Option<(DescriptorEntry, bool)> {
        if off + DESCRIPTOR_RECORD_LEN > rom.len() {
            return None;
        }
        let start = u32::from_be_bytes([rom[off], rom[off + 1], rom[off + 2], rom[off + 3]]);
        let end = u32::from_be_bytes([rom[off + 4], rom[off + 5], rom[off + 6], rom[off + 7]]);
        let stored = u32::from_be_bytes([rom[off + 8], rom[off + 9], rom[off + 10], rom[off + 11]]);
        if start == 0 && end == 0 && stored == DISABLED_MAGIC {
            return Some((DescriptorEntry { start, end, stored }, true));
        }
        if !start.is_multiple_of(4) || end % 4 != 3 {
            return None;
        }
        let eff_end = (end as usize).checked_add(1)?;
        if eff_end <= start as usize || eff_end > rom.len() || !eff_end.is_multiple_of(4) {
            return None;
        }
        let sum = prefix[eff_end / 4].wrapping_sub(prefix[start as usize / 4]);
        let ok = stored == DISABLED_MAGIC || sum.wrapping_add(stored) == DISABLED_MAGIC;
        Some((DescriptorEntry { start, end, stored }, ok))
    };

    let mut i = 0usize;
    while i + DESCRIPTOR_RECORD_LEN * 3 <= rom.len() {
        if let Some((first, first_ok)) = classify(i) {
            if !(first.start == 0 && first.end == 0) && first_ok {
                let mut entries = vec![first];
                let mut ok_count = 1usize;
                let mut j = i + DESCRIPTOR_RECORD_LEN;
                while entries.len() < 32 {
                    let Some((next, next_ok)) = classify(j) else {
                        break;
                    };
                    if next.start == 0 && next.end == 0 {
                        entries.push(next);
                        break;
                    }
                    entries.push(next);
                    if next_ok {
                        ok_count += 1;
                    }
                    j += DESCRIPTOR_RECORD_LEN;
                }
                if ok_count >= 3 {
                    return Some((i, entries));
                }
            }
        }
        i += 4;
    }
    None
}
