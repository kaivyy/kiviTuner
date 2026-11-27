//! Denso SH7xxx descriptor-table checksum.
//!
//! ## Format
//!
//! Subaru Denso SH705x/SH7xxx ROMs store their checksums in a descriptor table
//! of 12-byte records:
//!
//! ```text
//! [range_start : 4 B big-endian]
//! [range_end   : 4 B big-endian]
//! [stored_sum  : 4 B big-endian]
//! ```
//!
//! For each record the engine sums every 32-bit big-endian word in
//! `[start, end)` and compares it with `0x5AA5A55A - sum`. The magic value
//! `0x5AA5A55A` appears in the stored field of records that are disabled, and a
//! first record of `(0, 0, 0x5AA5A55A)` means every checksum in the ROM is off.
//!
//! Ported from `modules/checksum/checksum_ecu_subaru_denso_sh7xxx.cpp` in the
//! FastECU repository. The oracle below is derived from the format definition,
//! not from running that code.

use kivi_checksum::denso_descriptor::{
    correct_descriptor_checksums, parse_descriptor_table, verify_descriptor_checksums,
    DescriptorEntry, DESCRIPTOR_RECORD_LEN, DISABLED_MAGIC,
};

/// Builds a 12-byte descriptor record.
fn record(start: u32, end: u32, stored: u32) -> [u8; DESCRIPTOR_RECORD_LEN] {
    let mut r = [0u8; DESCRIPTOR_RECORD_LEN];
    r[0..4].copy_from_slice(&start.to_be_bytes());
    r[4..8].copy_from_slice(&end.to_be_bytes());
    r[8..12].copy_from_slice(&stored.to_be_bytes());
    r
}

/// Sums 32-bit big-endian words in `[start, end)`, matching the format.
fn expected_sum(rom: &[u8], start: usize, end: usize) -> u32 {
    let mut sum: u32 = 0;
    let mut i = start;
    while i + 4 <= end {
        let w = u32::from_be_bytes([rom[i], rom[i + 1], rom[i + 2], rom[i + 3]]);
        sum = sum.wrapping_add(w);
        i += 4;
    }
    sum
}

#[test]
fn a_single_correct_descriptor_verifies() {
    // ROM layout: [descriptor @0x00][data @0x20..0x30]
    let mut rom = vec![0u8; 0x40];
    rom[0x20..0x24].copy_from_slice(&0x0000_0001u32.to_be_bytes());
    rom[0x24..0x28].copy_from_slice(&0x0000_0002u32.to_be_bytes());

    let sum = expected_sum(&rom, 0x20, 0x30);
    let stored = DISABLED_MAGIC.wrapping_sub(sum);
    rom[0..12].copy_from_slice(&record(0x20, 0x30, stored));

    let entries = [DescriptorEntry { start: 0x20, end: 0x30, stored }];
    assert!(verify_descriptor_checksums(&rom, &entries).unwrap());
}

#[test]
fn a_wrong_stored_value_fails_verification() {
    let mut rom = vec![0u8; 0x40];
    rom[0x20..0x24].copy_from_slice(&0x0000_0001u32.to_be_bytes());
    // A value that cannot be confused with the correct one or with the
    // disabled marker.
    let stored = 0x1111_2222u32;
    let entries = [DescriptorEntry { start: 0x20, end: 0x30, stored }];
    assert!(!verify_descriptor_checksums(&rom, &entries).unwrap());
}

#[test]
fn correction_writes_the_expected_value_back() {
    let mut rom = vec![0u8; 0x40];
    rom[0x20..0x24].copy_from_slice(&0xDEAD_BEEFu32.to_be_bytes());
    rom[0x24..0x28].copy_from_slice(&0x0000_0002u32.to_be_bytes());
    // Deliberately wrong stored value.
    rom[0..12].copy_from_slice(&record(0x20, 0x30, 0x1234_5678));

    let entries = [DescriptorEntry { start: 0x20, end: 0x30, stored: 0x1234_5678 }];
    let fixed = correct_descriptor_checksums(&rom, &entries).unwrap();
    assert!(!verify_descriptor_checksums(&rom, &entries).unwrap());

    // The corrected value must equal the format's definition.
    let sum = expected_sum(&fixed, 0x20, 0x30);
    assert_eq!(fixed[8..12], DISABLED_MAGIC.wrapping_sub(sum).to_be_bytes());

    // Re-parsing the corrected ROM must yield a verifying table. Verification
    // reads the stored value from the entries it is given, so the caller has to
    // re-parse after a correction — that is the real workflow.
    let reparsed = parse_descriptor_table(&fixed, 0, 1).unwrap();
    assert!(
        verify_descriptor_checksums(&fixed, &reparsed).unwrap(),
        "after correction the checksum must verify"
    );
}

#[test]
fn a_disabled_record_is_skipped() {
    // Stored == magic means "this block's checksum is off".
    let rom = vec![0u8; 0x40];
    let entries = [DescriptorEntry {
        start: 0x20,
        end: 0x30,
        stored: DISABLED_MAGIC,
    }];
    assert!(
        verify_descriptor_checksums(&rom, &entries).unwrap(),
        "a disabled record must not be reported as a failure"
    );
}

#[test]
fn correction_leaves_disabled_records_alone() {
    let mut rom = vec![0u8; 0x40];
    rom[0..12].copy_from_slice(&record(0x20, 0x30, DISABLED_MAGIC));
    let entries = [DescriptorEntry {
        start: 0x20,
        end: 0x30,
        stored: DISABLED_MAGIC,
    }];
    let fixed = correct_descriptor_checksums(&rom, &entries).unwrap();
    assert_eq!(&fixed[8..12], &DISABLED_MAGIC.to_be_bytes());
}

#[test]
fn an_out_of_range_descriptor_is_refused() {
    let rom = vec![0u8; 0x40];
    let entries = [DescriptorEntry {
        start: 0x20,
        end: 0x1000,
        stored: 0,
    }];
    assert!(
        correct_descriptor_checksums(&rom, &entries).is_err(),
        "a range past the end of the ROM must be refused, not silently clamped"
    );
}

#[test]
fn an_inverted_range_is_refused() {
    let rom = vec![0u8; 0x40];
    let entries = [DescriptorEntry {
        start: 0x30,
        end: 0x20,
        stored: 0,
    }];
    assert!(correct_descriptor_checksums(&rom, &entries).is_err());
}

#[test]
fn correction_does_not_touch_bytes_outside_the_stored_field() {
    let mut rom = vec![0xAAu8; 0x40];
    rom[0..12].copy_from_slice(&record(0x20, 0x30, 0));
    let entries = [DescriptorEntry { start: 0x20, end: 0x30, stored: 0 }];
    let fixed = correct_descriptor_checksums(&rom, &entries).unwrap();
    // Only bytes 8..12 of the record may change.
    assert_eq!(&fixed[0..8], &rom[0..8], "start/end fields must be untouched");
    assert_eq!(&fixed[12..], &rom[12..], "the rest of the ROM must be untouched");
}
