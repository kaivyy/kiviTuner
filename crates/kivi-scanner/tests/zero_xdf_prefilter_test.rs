//! Parity tests for the Zero-XDF Z2 block pre-filter.
//!
//! Ported from `ecu_engine_rust::recognition::zero_xdf_discovery::ZeroXdfEngine::analyze_rom_blocks`
//! (`RomBlockPreFilter`). Z1 metrics live in [`kivi_scanner::scan_metrics`] and
//! are shared with the heuristic detector rather than duplicated.
//!
//! ## How expected values were obtained
//!
//! Block boundaries and kept-range arithmetic are computed from the documented
//! algorithm: 4096-byte blocks, prune when one byte value exceeds 85% of the
//! block, prune when entropy is below 1.0, prune when entropy exceeds 6.95
//! **and** mean adjacent-byte delta exceeds 85.0. Every expectation below is
//! derived from those rules, not from running the implementation.
//!
//! ## Scope
//!
//! Z2 only. Scanning, axis pairing, classification and XDF export (Z3–Z5) are
//! **not** implemented and remain deferred pending fixture provenance.

use kivi_scanner::zero_xdf_prefilter::{analyze_rom_blocks, BLOCK_SIZE};

fn blank_block(len: usize) -> Vec<u8> {
    vec![0xFFu8; len]
}

/// A 4096-byte block with a moderate, non-repeating distribution that survives
/// all three prune rules.
fn calibration_like_block() -> Vec<u8> {
    // 256 distinct values repeated 16 times -> entropy 8.0, but adjacent deltas
    // are small (cycling 0..255 gives |delta| = 1 except at the wrap), so the
    // mean delta stays well under 85 and the block is kept.
    (0..BLOCK_SIZE).map(|i| (i % 256) as u8).collect()
}

// ---------------------------------------------------------------------------
// Undersized input
// ---------------------------------------------------------------------------

#[test]
fn input_smaller_than_one_block_is_kept_whole() {
    let rom = vec![0xAAu8; 100];
    let r = analyze_rom_blocks(&rom);
    assert_eq!(r.block_size, BLOCK_SIZE);
    assert_eq!(r.total_blocks, 1);
    assert_eq!(r.kept_blocks, 1);
    assert_eq!(r.pruned_blocks, 0);
    assert_eq!(r.pruned_percentage, 0.0);
    assert_eq!(r.kept_ranges, vec![(0, 100)]);
}

#[test]
fn empty_input_yields_a_single_empty_range() {
    let r = analyze_rom_blocks(&[]);
    assert_eq!(r.total_blocks, 1);
    assert_eq!(r.kept_ranges, vec![(0, 0)]);
}

// ---------------------------------------------------------------------------
// Pruning rules
// ---------------------------------------------------------------------------

#[test]
fn a_uniform_block_is_pruned_as_blank() {
    // 4096 bytes of 0xFF -> >85% single value -> pruned.
    let rom = blank_block(BLOCK_SIZE);
    let r = analyze_rom_blocks(&rom);
    assert_eq!(r.total_blocks, 1);
    assert_eq!(r.kept_blocks, 0);
    assert_eq!(r.pruned_blocks, 1);
    assert!(r.kept_ranges.is_empty());
    assert_eq!(r.pruned_percentage, 100.0);
}

#[test]
fn a_zero_filled_block_is_pruned_as_blank() {
    let rom = vec![0x00u8; BLOCK_SIZE];
    let r = analyze_rom_blocks(&rom);
    assert_eq!(r.kept_blocks, 0);
}

#[test]
fn a_low_entropy_block_is_pruned() {
    // Two values, heavily skewed: entropy well below 1.0 bit.
    // 4090 zeros + 6 ones -> p(0)=0.9985, entropy ~0.016 bits.
    let mut rom = vec![0x00u8; BLOCK_SIZE];
    for b in rom.iter_mut().take(6) {
        *b = 0x01;
    }
    let r = analyze_rom_blocks(&rom);
    // It also trips the >85% single-value rule first; either way it is pruned.
    assert_eq!(r.kept_blocks, 0);
}

#[test]
fn a_calibration_like_block_is_kept() {
    let rom = calibration_like_block();
    let r = analyze_rom_blocks(&rom);
    assert_eq!(r.kept_blocks, 1, "entropy 8.0 with small deltas must be kept");
    assert_eq!(r.kept_ranges, vec![(0, BLOCK_SIZE)]);
}

#[test]
fn a_high_entropy_high_volatility_block_is_pruned_as_code() {
    // Alternating 0x00/0xFF gives entropy 1.0 and delta 255, but entropy is not
    // > 6.95 so the code rule does not fire. Build a block with entropy > 6.95
    // AND mean delta > 85: cycle through 256 values in a shuffled-looking order
    // with large jumps.
    let mut rom = Vec::with_capacity(BLOCK_SIZE);
    for i in 0..BLOCK_SIZE {
        // Large jumps: alternate between low and high halves.
        rom.push(if i % 2 == 0 {
            (i % 128) as u8
        } else {
            128 + (i % 128) as u8
        });
    }
    let r = analyze_rom_blocks(&rom);
    // Mean delta is 128 (> 85) and entropy is 8.0 (> 6.95) -> pruned as code.
    assert_eq!(r.kept_blocks, 0, "volatile high-entropy block must be pruned");
}

// ---------------------------------------------------------------------------
// Multi-block behaviour
// ---------------------------------------------------------------------------

#[test]
fn adjacent_kept_blocks_merge_into_one_range() {
    // Two consecutive calibration-like blocks -> one contiguous range.
    let mut rom = calibration_like_block();
    rom.extend(calibration_like_block());
    let r = analyze_rom_blocks(&rom);
    assert_eq!(r.total_blocks, 2);
    assert_eq!(r.kept_blocks, 2);
    assert_eq!(
        r.kept_ranges,
        vec![(0, BLOCK_SIZE * 2)],
        "contiguous kept blocks must coalesce"
    );
}

#[test]
fn a_pruned_block_splits_the_ranges() {
    // kept | pruned | kept -> two separate ranges.
    let mut rom = calibration_like_block();
    rom.extend(blank_block(BLOCK_SIZE));
    rom.extend(calibration_like_block());
    let r = analyze_rom_blocks(&rom);
    assert_eq!(r.total_blocks, 3);
    assert_eq!(r.kept_blocks, 2);
    assert_eq!(
        r.kept_ranges,
        vec![(0, BLOCK_SIZE), (BLOCK_SIZE * 2, BLOCK_SIZE * 3)]
    );
}

#[test]
fn a_trailing_partial_block_is_still_examined() {
    // One full calibration block plus 100 bytes of calibration-like data.
    let mut rom = calibration_like_block();
    rom.extend((0..100).map(|i| (i % 256) as u8));
    let r = analyze_rom_blocks(&rom);
    assert_eq!(r.total_blocks, 2);
    assert_eq!(r.kept_blocks, 2);
    assert_eq!(r.kept_ranges, vec![(0, rom.len())]);
}

#[test]
fn pruned_percentage_is_consistent_with_the_counts() {
    let mut rom = calibration_like_block();
    rom.extend(blank_block(BLOCK_SIZE));
    let r = analyze_rom_blocks(&rom);
    assert_eq!(r.total_blocks, 2);
    assert_eq!(r.pruned_blocks, 1);
    assert!((r.pruned_percentage - 50.0).abs() < 1e-9);
}

// ---------------------------------------------------------------------------
// Determinism
// ---------------------------------------------------------------------------

#[test]
fn output_is_deterministic_across_runs() {
    let mut rom = calibration_like_block();
    rom.extend(blank_block(BLOCK_SIZE));
    rom.extend(calibration_like_block());

    let a = analyze_rom_blocks(&rom);
    let b = analyze_rom_blocks(&rom);
    assert_eq!(a, b, "pre-filter output must be stable");
}

#[test]
fn kept_ranges_are_sorted_and_non_overlapping() {
    let mut rom = calibration_like_block();
    rom.extend(blank_block(BLOCK_SIZE));
    rom.extend(calibration_like_block());
    rom.extend(blank_block(BLOCK_SIZE));
    rom.extend(calibration_like_block());

    let r = analyze_rom_blocks(&rom);
    let mut prev_end = 0;
    for (start, end) in &r.kept_ranges {
        assert!(start <= end, "range must be well formed");
        assert!(*start >= prev_end, "ranges must not overlap");
        prev_end = *end;
    }
}

#[test]
fn total_blocks_covers_the_whole_input() {
    let rom = vec![0x5Au8; BLOCK_SIZE * 2 + 1];
    let r = analyze_rom_blocks(&rom);
    assert_eq!(r.total_blocks, 3, "ceil(len / block_size)");
    assert_eq!(
        r.kept_blocks + r.pruned_blocks,
        r.total_blocks,
        "every block is accounted for exactly once"
    );
}
