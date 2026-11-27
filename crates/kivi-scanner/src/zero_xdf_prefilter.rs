//! Zero-XDF stage Z2: 4 KiB block pre-filtering and candidate reduction.
//!
//! Ported from `ecu_engine_rust::recognition::zero_xdf_discovery::ZeroXdfEngine::analyze_rom_blocks`
//! (`RomBlockPreFilter`).
//!
//! ## What this does
//!
//! Splits a ROM into 4096-byte blocks and prunes the ones that cannot contain a
//! calibration table, so later scanning stages examine a fraction of the image.
//! Three rules, applied in order:
//!
//! 1. **Blank/padding** — one byte value exceeds 85% of the block.
//! 2. **Low entropy** — Shannon entropy below 1.0 bit.
//! 3. **Machine code** — entropy above 6.95 bits *and* mean adjacent-byte delta
//!    above 85.0.
//!
//! Surviving blocks are coalesced into contiguous `kept_ranges`.
//!
//! ## Scope
//!
//! Z2 only. Scanning, axis pairing, semantic classification and XDF export
//! (Z3–Z5) are **not** implemented here. They remain deferred pending a fixture
//! provenance decision: the source claims bit-exact results on four real ROMs,
//! but no licence for those images has been established, so no real-ROM parity
//! claim is made or implied by this module.
//!
//! ## Determinism
//!
//! Pure and deterministic. No parallelism: the source uses `rayon` for its scan
//! stage, but this stage is a single linear pass and does not need it. Adding a
//! dependency to imitate the source would buy nothing here.

/// Size of one pre-filter block, in bytes.
pub const BLOCK_SIZE: usize = 4096;

/// Result of the Z2 block pre-filter.
#[derive(Debug, Clone, PartialEq)]
pub struct RomBlockPreFilter {
    /// Block size used for the analysis.
    pub block_size: usize,
    /// Total blocks examined.
    pub total_blocks: usize,
    /// Blocks that survived all prune rules.
    pub kept_blocks: usize,
    /// Blocks removed by a prune rule.
    pub pruned_blocks: usize,
    /// Percentage of blocks pruned, `0.0..=100.0`.
    pub pruned_percentage: f64,
    /// Contiguous half-open `[start, end)` ranges that survived.
    pub kept_ranges: Vec<(usize, usize)>,
}

/// Runs the Z2 pre-filter over `rom`.
///
/// Input shorter than one block is kept whole without examination — there is
/// nothing to reduce, and pruning a sub-block image would discard the only data
/// available.
pub fn analyze_rom_blocks(rom: &[u8]) -> RomBlockPreFilter {
    if rom.len() < BLOCK_SIZE {
        return RomBlockPreFilter {
            block_size: BLOCK_SIZE,
            total_blocks: 1,
            kept_blocks: 1,
            pruned_blocks: 0,
            pruned_percentage: 0.0,
            kept_ranges: vec![(0, rom.len())],
        };
    }

    let total_blocks = rom.len().div_ceil(BLOCK_SIZE);
    let mut kept_ranges = Vec::new();
    let mut in_range = false;
    let mut r_start = 0;
    let mut kept_count = 0;

    for i in 0..total_blocks {
        let start = i * BLOCK_SIZE;
        let end = (start + BLOCK_SIZE).min(rom.len());
        let chunk = &rom[start..end];

        if should_prune(chunk) {
            if in_range {
                in_range = false;
                kept_ranges.push((r_start, start));
            }
            continue;
        }

        kept_count += 1;
        if !in_range {
            in_range = true;
            r_start = start;
        }
    }
    if in_range {
        kept_ranges.push((r_start, rom.len()));
    }

    let pruned_count = total_blocks.saturating_sub(kept_count);
    let pruned_pct = if total_blocks > 0 {
        (pruned_count as f64 / total_blocks as f64) * 100.0
    } else {
        0.0
    };

    RomBlockPreFilter {
        block_size: BLOCK_SIZE,
        total_blocks,
        kept_blocks: kept_count,
        pruned_blocks: pruned_count,
        pruned_percentage: pruned_pct,
        kept_ranges,
    }
}

/// Applies the three prune rules to one block.
fn should_prune(chunk: &[u8]) -> bool {
    let chunk_len = chunk.len();
    if chunk_len == 0 {
        return true;
    }
    let _ = chunk_len;

    let mut counts = [0u16; 256];
    let mut max_c = 0u16;
    for &b in chunk {
        counts[b as usize] += 1;
        if counts[b as usize] > max_c {
            max_c = counts[b as usize];
        }
    }

    // 1. Blank / padding: one value dominates.
    if (max_c as usize) * 100 / chunk_len > 85 {
        return true;
    }

    // 2. Shannon entropy.
    let mut ent = 0.0f64;
    let len_f = chunk_len as f64;
    for &c in &counts {
        if c > 0 {
            let p = c as f64 / len_f;
            ent -= p * p.log2();
        }
    }
    if ent < 1.0 {
        return true;
    }

    // 3. Machine code: dense *and* volatile.
    if ent > 6.95 && chunk_len > 1 {
        let mut sum_d = 0u64;
        for j in 0..chunk_len - 1 {
            sum_d += (chunk[j + 1] as i32 - chunk[j] as i32).unsigned_abs() as u64;
        }
        let mean_d = sum_d as f64 / (chunk_len - 1) as f64;
        if mean_d > 85.0 {
            return true;
        }
    }

    false
}
