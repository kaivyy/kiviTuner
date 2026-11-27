//! Masked-signature scanner and binary difference counter.
//!
//! Ported from `ecu_engine_rust::recognition::pattern_search`
//! (`MaskedSignatureScanner`). Locates calibration tables, limiter switches and
//! DTC masks across ECU software revisions where addresses move but byte
//! signatures stay the same.
//!
//! ## Deviation from the source
//!
//! The source exposes `replace_all(&mut [u8], ...)`, which mutates the caller's
//! buffer. This crate is analysis-oriented and read-only by convention, so the
//! operation is exposed as [`replace_all_copy`], which returns a new buffer.
//! Matching and counting semantics are unchanged.
//!
//! Everything else preserves source behaviour exactly, including:
//!
//! * an empty pattern never matches (it does not match at every offset),
//! * a buffer shorter than the pattern yields no matches,
//! * `count_different_bytes` adds the absolute length difference to the
//!   in-prefix difference count.
//!
//! This module performs no I/O and is deterministic.

/// Wildcard and bitmasked byte-pattern scanning.
///
/// A pattern is a sequence of `Option<u8>`: `Some(b)` must match exactly,
/// `None` matches any byte.
pub struct MaskedSignatureScanner;

impl MaskedSignatureScanner {
    /// Parses a hex pattern with wildcards into optional bytes.
    ///
    /// `"48 89 5C 24 ?? 57"` — `??` and `?` are wildcards. Tokens are parsed as
    /// hexadecimal; a token that is not a valid `u8` is rejected.
    pub fn parse_pattern(pattern_str: &str) -> Result<Vec<Option<u8>>, &'static str> {
        let mut tokens = Vec::new();
        for part in pattern_str.split_whitespace() {
            if part == "??" || part == "?" {
                tokens.push(None);
            } else {
                let byte_val = u8::from_str_radix(part, 16)
                    .map_err(|_| "Invalid hexadecimal byte in pattern")?;
                tokens.push(Some(byte_val));
            }
        }
        if tokens.is_empty() {
            return Err("Pattern cannot be empty");
        }
        Ok(tokens)
    }

    /// Returns every offset where the pattern matches.
    pub fn find_all(buffer: &[u8], pattern: &[Option<u8>]) -> Vec<usize> {
        let mut matches = Vec::new();
        if buffer.len() < pattern.len() || pattern.is_empty() {
            return matches;
        }

        let max_pos = buffer.len() - pattern.len();
        for i in 0..=max_pos {
            if Self::matches_at(buffer, pattern, i) {
                matches.push(i);
            }
        }

        matches
    }

    /// Returns the lowest matching offset, if any.
    pub fn find_first(buffer: &[u8], pattern: &[Option<u8>]) -> Option<usize> {
        if buffer.len() < pattern.len() || pattern.is_empty() {
            return None;
        }

        let max_pos = buffer.len() - pattern.len();
        (0..=max_pos).find(|&i| Self::matches_at(buffer, pattern, i))
    }

    /// Replaces every match and returns the new buffer plus the match count.
    ///
    /// Pure: the supplied buffer is never mutated.
    pub fn replace_all_copy(
        buffer: &[u8],
        pattern: &[Option<u8>],
        replacement: &[u8],
    ) -> Result<(Vec<u8>, usize), &'static str> {
        if pattern.len() != replacement.len() {
            return Err("Replacement length must match pattern length for in-place replacement");
        }

        let offsets = Self::find_all(buffer, pattern);
        let count = offsets.len();

        let mut out = buffer.to_vec();
        for off in offsets {
            out[off..off + replacement.len()].copy_from_slice(replacement);
        }

        Ok((out, count))
    }

    /// Counts differing bytes, including any length difference.
    pub fn count_different_bytes(a: &[u8], b: &[u8]) -> usize {
        let min_len = a.len().min(b.len());
        let mut diffs = 0;

        for i in 0..min_len {
            if a[i] != b[i] {
                diffs += 1;
            }
        }

        diffs + a.len().abs_diff(b.len())
    }

    /// True when the pattern matches `buffer` starting at `start`.
    fn matches_at(buffer: &[u8], pattern: &[Option<u8>], start: usize) -> bool {
        pattern.iter().enumerate().all(|(p_idx, &expected)| {
            match expected {
                Some(exp_byte) => buffer[start + p_idx] == exp_byte,
                None => true,
            }
        })
    }
}

// ---------------------------------------------------------------------------
// Free functions
// ---------------------------------------------------------------------------

/// Parses a hex pattern with wildcards. See [`MaskedSignatureScanner::parse_pattern`].
pub fn parse_pattern(pattern_str: &str) -> Result<Vec<Option<u8>>, &'static str> {
    MaskedSignatureScanner::parse_pattern(pattern_str)
}

/// Returns every matching offset. See [`MaskedSignatureScanner::find_all`].
pub fn find_all(buffer: &[u8], pattern: &[Option<u8>]) -> Vec<usize> {
    MaskedSignatureScanner::find_all(buffer, pattern)
}

/// Returns the lowest matching offset. See [`MaskedSignatureScanner::find_first`].
pub fn find_first(buffer: &[u8], pattern: &[Option<u8>]) -> Option<usize> {
    MaskedSignatureScanner::find_first(buffer, pattern)
}

/// Replaces every match, returning a new buffer and the match count.
pub fn replace_all_copy(
    buffer: &[u8],
    pattern: &[Option<u8>],
    replacement: &[u8],
) -> Result<(Vec<u8>, usize), &'static str> {
    MaskedSignatureScanner::replace_all_copy(buffer, pattern, replacement)
}

/// Counts differing bytes between two buffers, including length difference.
pub fn count_different_bytes(a: &[u8], b: &[u8]) -> usize {
    MaskedSignatureScanner::count_different_bytes(a, b)
}
