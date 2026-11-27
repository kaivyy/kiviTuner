//! Address-compatibility gate for binary comparison.
//!
//! ## The problem
//!
//! [`crate::engine::compare_binaries`] compares **vector offsets**. It has no
//! concept of physical address, so two buffers covering different physical
//! ranges can be diffed as if their offset 0 were the same ECU address.
//!
//! Intel HEX and S-Record ingestion (Slice 6A) reports a `base_address`, but a
//! raw `.bin` carries no address metadata at all. Treating a BIN's offset zero
//! as a proven physical address would be an assumption, not evidence.
//!
//! ## Policy
//!
//! This module gates comparison on what the application can actually establish:
//!
//! | Base | Target | Outcome |
//! |---|---|---|
//! | unknown | unknown | allowed — existing BIN-to-BIN behaviour |
//! | known A | known A | allowed |
//! | known A | known B, A != B | rejected: [`DiffRejection::ConflictingBases`] |
//! | unknown | known | rejected: [`DiffRejection::UnverifiableAlignment`] |
//! | known | unknown | rejected: [`DiffRejection::UnverifiableAlignment`] |
//!
//! ## What "allowed" does *not* mean
//!
//! Equal base addresses are **necessary but not sufficient** to prove the two
//! buffers describe the same address space. A matching base says nothing about:
//!
//! * whether both buffers were read from the same ECU or software revision,
//! * whether the sparse reconstruction padded a different set of gaps,
//! * whether one buffer covers a region the other does not.
//!
//! This gate prevents the specific, demonstrable error of comparing inputs whose
//! known bases conflict or where one side's base is unknown. It does **not**
//! establish full address correctness, and it deliberately performs no rebasing.
//!
//! ## Safety
//!
//! Pure: no I/O, no mutation of the supplied slices, deterministic.

use crate::engine::compare_binaries;
use crate::model::DiffRegion;
use thiserror::Error;

/// A half-open physical address range `[start, end)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AddressRange {
    /// First address included.
    pub start: u32,
    /// Number of bytes covered.
    pub len: u32,
}

impl AddressRange {
    /// Builds a range, returning `None` when `start + len` would overflow `u32`.
    pub fn new(start: u32, len: u32) -> Option<Self> {
        start.checked_add(len)?;
        Some(Self { start, len })
    }

    /// Exclusive end address.
    pub fn end_exclusive(&self) -> u32 {
        self.start + self.len
    }

    /// Number of bytes covered.
    pub fn len(&self) -> u32 {
        self.len
    }

    /// Always `false`: a range is constructed with an explicit length, which may
    /// legitimately be zero for an empty input.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

/// How the two inputs' address ranges related.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressCompatibility {
    /// Neither input carried address metadata (raw BIN to raw BIN).
    UnknownBases,
    /// Both inputs declared the same base address.
    MatchingBases(u32),
}

/// Why a comparison was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum DiffRejection {
    /// Both inputs declared a base address, and they differ.
    #[error(
        "Cannot compare: base addresses differ (base 0x{base:X}, target 0x{target:X}). \
         Re-export both files from the same address range, or compare raw BIN files."
    )]
    ConflictingBases {
        /// Base address declared by the base input.
        base: u32,
        /// Base address declared by the target input.
        target: u32,
    },
    /// One side declared a base address and the other did not.
    #[error(
        "Cannot compare: address alignment cannot be verified (base {}, target {}). \
         Compare files of the same format, or load both as raw BIN.",
        fmt_opt(*base), fmt_opt(*target)
    )]
    UnverifiableAlignment {
        /// Base address of the base input, if known.
        base: Option<u32>,
        /// Base address of the target input, if known.
        target: Option<u32>,
    },
    /// One of the inputs was empty.
    #[error("Cannot compare: {which} input is empty")]
    EmptyInput {
        /// Which side was empty: `"base"` or `"target"`.
        which: &'static str,
    },
}

/// Renders an optional address for error messages: `0x8000` or `unknown`.
fn fmt_opt(v: Option<u32>) -> String {
    match v {
        Some(a) => format!("0x{:X}", a),
        None => "unknown".to_string(),
    }
}

/// Result of a successful address-gated comparison.
#[derive(Debug, Clone, PartialEq)]
pub struct GatedDiff {
    /// How the inputs' addresses related.
    pub compatibility: AddressCompatibility,
    /// Difference regions from the unchanged diff engine.
    pub regions: Vec<DiffRegion>,
}

/// Compares two buffers after checking that their address ranges are compatible.
///
/// `base_addr` / `target_addr` are the lowest physical addresses reported by
/// HEX/SREC ingestion, or `None` for raw BIN (unknown).
///
/// Returns [`DiffRejection`] when alignment cannot be established. On success
/// the comparison is delegated to [`compare_binaries`] unchanged, so existing
/// diff semantics (including tail handling for unequal lengths) are preserved.
pub fn diff_with_address_check(
    base: &[u8],
    base_addr: Option<u32>,
    target: &[u8],
    target_addr: Option<u32>,
    max_gap: usize,
) -> Result<GatedDiff, DiffRejection> {
    if base.is_empty() {
        return Err(DiffRejection::EmptyInput { which: "base" });
    }
    if target.is_empty() {
        return Err(DiffRejection::EmptyInput { which: "target" });
    }

    let compatibility = match (base_addr, target_addr) {
        (None, None) => AddressCompatibility::UnknownBases,
        (Some(a), Some(b)) if a == b => AddressCompatibility::MatchingBases(a),
        (Some(a), Some(b)) => return Err(DiffRejection::ConflictingBases { base: a, target: b }),
        (base, target) => return Err(DiffRejection::UnverifiableAlignment { base, target }),
    };

    Ok(GatedDiff {
        compatibility,
        regions: compare_binaries(base, target, max_gap),
    })
}
