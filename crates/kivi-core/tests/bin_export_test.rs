//! Parity tests for BinExportEngine.
//!
//! Ported from `ecu_engine_rust::formats::mod.rs::BinExportEngine`:
//! `swap_16bit_words_in_place`, `split_odd_even_eprom`, `merge_odd_even_eprom`,
//! `merge_partial_read_into_full_bin`, `validate_dumped_bin_integrity`.
//!
//! ## How expected values were obtained
//!
//! All expected byte sequences are hand-derived from the documented operation
//! (pairwise swap, de-interleave, interleave, offset placement). Integrity
//! thresholds are taken from the source constants, and the boundary cases are
//! computed arithmetically.
//!
//! ## Documented deviation
//!
//! `swap_16bit_words_in_place` mutates in the source. A pure
//! [`swap_16bit_words`] is provided for analysis use; the mutating form is also
//! kept for callers that need in-place behaviour, matching the source.

use kivi_core::bin_export::{
    merge_odd_even_eprom, merge_partial_read_into_full_bin, split_odd_even_eprom, swap_16bit_words,
    swap_16bit_words_in_place, validate_dumped_bin_integrity, BinExportError,
};

// ---------------------------------------------------------------------------
// 16-bit word swap
// ---------------------------------------------------------------------------

#[test]
fn swaps_each_byte_pair() {
    let out = swap_16bit_words(&[0x11, 0x22, 0x33, 0x44]).expect("swap");
    assert_eq!(out, vec![0x22, 0x11, 0x44, 0x33]);
}

#[test]
fn swapping_twice_restores_the_original() {
    let original = vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06];
    let once = swap_16bit_words(&original).expect("swap");
    let twice = swap_16bit_words(&once).expect("swap");
    assert_eq!(twice, original);
}

#[test]
fn swap_of_empty_input_is_empty() {
    assert_eq!(swap_16bit_words(&[]).expect("swap"), Vec::<u8>::new());
}

#[test]
fn swap_rejects_an_odd_length() {
    let err = swap_16bit_words(&[0x01, 0x02, 0x03]).expect_err("must reject");
    assert_eq!(err, BinExportError::OddLengthForWordSwap);
}

#[test]
fn swap_does_not_mutate_its_input() {
    let input = vec![0x11, 0x22];
    let copy = input.clone();
    let _ = swap_16bit_words(&input).expect("swap");
    assert_eq!(input, copy);
}

#[test]
fn in_place_swap_matches_the_pure_variant() {
    let mut buf = vec![0xAA, 0xBB, 0xCC, 0xDD];
    swap_16bit_words_in_place(&mut buf).expect("swap");
    assert_eq!(buf, swap_16bit_words(&[0xAA, 0xBB, 0xCC, 0xDD]).expect("pure"));
}

#[test]
fn in_place_swap_rejects_an_odd_length() {
    let mut buf = vec![1, 2, 3];
    let err = swap_16bit_words_in_place(&mut buf).expect_err("must reject");
    assert_eq!(err, BinExportError::OddLengthForWordSwap);
}

// ---------------------------------------------------------------------------
// EPROM split / merge
// ---------------------------------------------------------------------------

#[test]
fn splits_interleaved_bytes_into_even_and_odd_banks() {
    let (lo, hi) = split_odd_even_eprom(&[0x00, 0x01, 0x02, 0x03]).expect("split");
    assert_eq!(lo, vec![0x00, 0x02], "even bank takes indices 0,2");
    assert_eq!(hi, vec![0x01, 0x03], "odd bank takes indices 1,3");
}

#[test]
fn split_rejects_an_odd_length() {
    let err = split_odd_even_eprom(&[0x00, 0x01, 0x02]).expect_err("must reject");
    assert_eq!(err, BinExportError::OddLengthForEpromSplit);
}

#[test]
fn split_of_empty_input_yields_two_empty_banks() {
    let (lo, hi) = split_odd_even_eprom(&[]).expect("split");
    assert!(lo.is_empty());
    assert!(hi.is_empty());
}

#[test]
fn merge_interleaves_the_two_banks() {
    let merged = merge_odd_even_eprom(&[0x00, 0x02], &[0x01, 0x03]).expect("merge");
    assert_eq!(merged, vec![0x00, 0x01, 0x02, 0x03]);
}

#[test]
fn split_then_merge_round_trips() {
    let original: Vec<u8> = (0u8..32).collect();
    let (lo, hi) = split_odd_even_eprom(&original).expect("split");
    let merged = merge_odd_even_eprom(&lo, &hi).expect("merge");
    assert_eq!(merged, original);
}

#[test]
fn merge_rejects_mismatched_bank_sizes() {
    let err = merge_odd_even_eprom(&[0x00, 0x01], &[0x02]).expect_err("must reject");
    assert_eq!(err, BinExportError::BankSizeMismatch);
}

#[test]
fn merge_of_empty_banks_yields_empty() {
    assert_eq!(
        merge_odd_even_eprom(&[], &[]).expect("merge"),
        Vec::<u8>::new()
    );
}

// ---------------------------------------------------------------------------
// Partial read merge
// ---------------------------------------------------------------------------

#[test]
fn places_a_partial_read_at_its_file_offset() {
    // physical base 0x80040000, partial at 0x80040000 -> offset 0.
    let out = merge_partial_read_into_full_bin(None, 16, 0x8004_0000, 0x8004_0000, &[0xAA, 0xBB])
        .expect("merge");
    assert_eq!(out.len(), 16);
    assert_eq!(&out[0..2], &[0xAA, 0xBB]);
    assert_eq!(out[2], 0xFF, "unwritten area stays blank");
}

#[test]
fn places_a_partial_read_at_a_nonzero_offset() {
    // base 0x80040000, partial at 0x80040004 -> offset 4.
    let out = merge_partial_read_into_full_bin(None, 16, 0x8004_0000, 0x8004_0004, &[0x11, 0x22])
        .expect("merge");
    assert_eq!(&out[4..6], &[0x11, 0x22]);
    assert_eq!(out[0], 0xFF);
}

#[test]
fn merges_onto_an_existing_base_rom() {
    let base = vec![0x00u8; 8];
    let out = merge_partial_read_into_full_bin(Some(&base), 8, 0x1000, 0x1002, &[0xAB])
        .expect("merge");
    assert_eq!(out[2], 0xAB);
    assert_eq!(out[0], 0x00, "untouched base bytes survive");
}

#[test]
fn rejects_a_partial_address_below_the_physical_base() {
    let err = merge_partial_read_into_full_bin(None, 16, 0x8004_0000, 0x8003_0000, &[0xAA])
        .expect_err("must reject");
    assert_eq!(err, BinExportError::PartialBelowBase);
}

#[test]
fn rejects_a_partial_read_that_overruns_the_target() {
    let err = merge_partial_read_into_full_bin(None, 4, 0, 2, &[0xAA, 0xBB, 0xCC])
        .expect_err("must reject");
    assert_eq!(err, BinExportError::PartialExceedsTarget);
}

#[test]
fn rejects_a_zero_target_size() {
    let err = merge_partial_read_into_full_bin(None, 0, 0, 0, &[0xAA]).expect_err("must reject");
    assert_eq!(err, BinExportError::ZeroTargetSize);
}

#[test]
fn rejects_a_base_rom_of_the_wrong_size() {
    let base = vec![0x00u8; 4];
    let err = merge_partial_read_into_full_bin(Some(&base), 8, 0, 0, &[0xAA])
        .expect_err("must reject");
    assert_eq!(err, BinExportError::BaseSizeMismatch);
}

// ---------------------------------------------------------------------------
// Integrity validation
// ---------------------------------------------------------------------------

fn varied_dump(len: usize) -> Vec<u8> {
    // Non-blank, non-mirrored, pseudo-varied content.
    (0..len).map(|i| ((i * 37 + 11) % 251) as u8).collect()
}

#[test]
fn accepts_a_plausible_dump() {
    assert!(validate_dumped_bin_integrity(&varied_dump(4096)).is_ok());
}

#[test]
fn rejects_a_dump_smaller_than_1024_bytes() {
    let err = validate_dumped_bin_integrity(&vec![0x42u8; 1023]).expect_err("must reject");
    assert_eq!(err, BinExportError::DumpTooSmall);
}

#[test]
fn rejects_a_dump_that_is_almost_entirely_ff() {
    let mut rom = varied_dump(4096);
    // Make >= 98% 0xFF.
    let keep = 4096 * 2 / 100 - 1;
    for b in rom.iter_mut().skip(keep) {
        *b = 0xFF;
    }
    let err = validate_dumped_bin_integrity(&rom).expect_err("must reject");
    assert_eq!(err, BinExportError::BlankFlashDump);
}

#[test]
fn rejects_a_dump_that_is_almost_entirely_zero() {
    let mut rom = varied_dump(4096);
    let keep = 4096 * 2 / 100 - 1;
    for b in rom.iter_mut().skip(keep) {
        *b = 0x00;
    }
    let err = validate_dumped_bin_integrity(&rom).expect_err("must reject");
    assert_eq!(err, BinExportError::LockedZeroDump);
}

#[test]
fn rejects_a_repeating_256_byte_mirror_dump() {
    // 16 identical non-trivial pages -> the SecurityAccess-locked trap.
    let mut page = [0u8; 256];
    for (i, b) in page.iter_mut().enumerate() {
        *b = ((i * 7 + 3) % 200) as u8 + 1; // non-zero, non-FF
    }
    let rom: Vec<u8> = page.iter().copied().cycle().take(256 * 16).collect();
    let err = validate_dumped_bin_integrity(&rom).expect_err("must reject");
    assert_eq!(err, BinExportError::FakeMirrorReadout);
}

#[test]
fn accepts_a_dump_whose_pages_differ() {
    // Same first page, but later pages differ -> not a mirror trap.
    let mut rom = varied_dump(256 * 16);
    let mut page = [0u8; 256];
    for (i, b) in page.iter_mut().enumerate() {
        *b = ((i * 7 + 3) % 200) as u8 + 1;
    }
    rom[0..256].copy_from_slice(&page);
    // Ensure the second page differs from the first.
    rom[256] = page[0].wrapping_add(1);
    assert!(validate_dumped_bin_integrity(&rom).is_ok());
}

#[test]
fn integrity_check_does_not_mutate_its_input() {
    let rom = varied_dump(2048);
    let copy = rom.clone();
    let _ = validate_dumped_bin_integrity(&rom);
    assert_eq!(rom, copy);
}
