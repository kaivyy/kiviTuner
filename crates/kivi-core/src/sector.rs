//! Differential sector planning for ROM comparison.
//!
//! Ported from `ecu_engine_rust::flashing::nissan_hitachi`
//! (`DifferentialSectorPlan`, `DifferentialFlashPlanner`). Only the planner
//! logic is reproduced; the source file's cryptographic constants
//! (`NissanHitachiSeedKey::KEYTABLE`) are deliberately **not** included.
//!
//! Behavior is preserved exactly, including:
//!
//! * Iteration stops as soon as `current_offset >= tuned_rom.len()`; a sector
//!   list longer than the ROM simply produces fewer entries.
//! * Sector sizes that do not cover the whole ROM leave the tail unplanned —
//!   there is no validation that the sector list is complete.
//! * A zero-length sector yields a zero-length plan entry at the current offset.
//! * `start_address` is the running byte offset cast to `u32` (truncating on
//!   ROMs larger than 4 GiB, matching the source).
//!
//! This module performs no I/O and is deterministic.

use thiserror::Error;

/// Error produced by [`plan_differential_flash`].
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SectorPlanError {
    /// The stock and tuned buffers have different lengths.
    #[error("Stock and Tuned ROM sizes must match for differential planning")]
    LengthMismatch,
}

/// Per-sector outcome of a differential plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DifferentialSectorPlan {
    /// Index of the sector in the caller-supplied size list.
    pub sector_index: usize,
    /// Byte offset where this sector starts in the ROM.
    pub start_address: u32,
    /// Number of bytes covered by this sector.
    pub length: usize,
    /// Whether any byte differs between stock and tuned.
    pub is_modified: bool,
    /// Number of differing bytes within the sector.
    pub modified_bytes_count: usize,
}

/// Compares `stock_rom` against `tuned_rom` across the supplied sector layout.
///
/// Returns one entry per sector that starts inside the ROM. See the module
/// documentation for the exact boundary semantics inherited from the source.
pub fn plan_differential_flash(
    stock_rom: &[u8],
    tuned_rom: &[u8],
    sector_sizes: &[usize],
) -> Result<Vec<DifferentialSectorPlan>, SectorPlanError> {
    if stock_rom.len() != tuned_rom.len() {
        return Err(SectorPlanError::LengthMismatch);
    }

    let mut plan = Vec::new();
    let mut current_offset = 0usize;

    for (idx, &sec_len) in sector_sizes.iter().enumerate() {
        if current_offset >= tuned_rom.len() {
            break;
        }

        let end_offset = current_offset.saturating_add(sec_len).min(tuned_rom.len());
        let stock_slice = &stock_rom[current_offset..end_offset];
        let tuned_slice = &tuned_rom[current_offset..end_offset];

        let mut diff_count = 0usize;
        for i in 0..stock_slice.len() {
            if stock_slice[i] != tuned_slice[i] {
                diff_count += 1;
            }
        }

        plan.push(DifferentialSectorPlan {
            sector_index: idx,
            start_address: current_offset as u32,
            length: end_offset - current_offset,
            is_modified: diff_count > 0,
            modified_bytes_count: diff_count,
        });

        current_offset = end_offset;
    }

    Ok(plan)
}

/// Sum of `length` across all modified sectors.
pub fn total_bytes_to_flash(plan: &[DifferentialSectorPlan]) -> usize {
    plan.iter()
        .filter(|p| p.is_modified)
        .map(|p| p.length)
        .sum()
}
