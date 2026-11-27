//! Flash-region overlap detection.
//!
//! ## Why overlap is dangerous
//!
//! Allocating intersecting flash memory areas across loadable code/calibration modules
//! makes the final ECU image depend on write order — which is not guaranteed across
//! independent plans. That produces a corrupted image matching neither plan, often with
//! **no write error at all** because each individual block write succeeded.
//!
//! The only safe moment to catch intersecting regions is before the first byte is written.
//!
//! ## Half-open intervals
//!
//! A region `{ address: 0x1000, length: 0x100 }` covers `[0x1000, 0x1100)`.
//! Two regions that merely touch — `[0x1000, 0x1100)` and `[0x1100, 0x1200)` —
//! do **not** overlap. Treating them as an overlap would reject the normal case
//! of a flash split into consecutive sectors.

/// One contiguous span of flash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlashRegion {
    pub address: u32,
    pub length: usize,
}

impl FlashRegion {
    /// The address one past the last covered byte, saturating at `u32::MAX`.
    ///
    /// A region ending at the top of the address space would otherwise wrap to
    /// zero and appear to overlap everything.
    pub fn end_exclusive(&self) -> u64 {
        (self.address as u64).saturating_add(self.length as u64)
    }

    /// Whether this region covers no byte.
    pub fn is_empty(&self) -> bool {
        self.length == 0
    }

    /// Whether two regions share at least one byte.
    pub fn overlaps(&self, other: &FlashRegion) -> bool {
        if self.is_empty() || other.is_empty() {
            return false;
        }
        (self.address as u64) < other.end_exclusive()
            && (other.address as u64) < self.end_exclusive()
    }
}

/// Why a region list was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegionOverlapError {
    /// Two regions share at least one byte.
    Overlap {
        first: FlashRegion,
        second: FlashRegion,
    },
}

impl std::fmt::Display for RegionOverlapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegionOverlapError::Overlap { first, second } => write!(
                f,
                "flash regions overlap: [0x{:08X}, +{}) and [0x{:08X}, +{}); \
                 writing both would make the result depend on write order",
                first.address,
                first.length,
                second.address,
                second.length
            ),
        }
    }
}

impl std::error::Error for RegionOverlapError {}

/// Verify that no two regions share a byte.
///
/// Returns the **first** collision found, in list order, so the message points
/// at a specific pair rather than "somewhere in this list".
pub fn check_no_overlap(regions: &[FlashRegion]) -> Result<(), RegionOverlapError> {
    for i in 0..regions.len() {
        for j in (i + 1)..regions.len() {
            if regions[i].overlaps(&regions[j]) {
                return Err(RegionOverlapError::Overlap {
                    first: regions[i],
                    second: regions[j],
                });
            }
        }
    }
    Ok(())
}
