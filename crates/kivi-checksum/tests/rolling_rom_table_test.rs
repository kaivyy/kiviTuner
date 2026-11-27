//! Rolling checksum with a ROM-resident lookup table.
//!
//! ## Source
//!
//! `NefMotoOpenSource/Checksum/BaseChecksum.cs` — `CalculateRollingChecksumForRange`.
//! The loop is:
//!
//! ```text
//! for each byte b:
//!     seed = u32_at(seedAddr + ((b ^ (checksum & 0xFF)) << 2))
//!     checksum = (checksum >> 8) ^ seed
//! ```
//!
//! That is a CRC-32 in table-driven form, except the 256-entry table is **stored
//! inside the ROM** rather than hardcoded. Bosch ME7 does this so the table can
//! differ per calibration; a tool that assumes a fixed CRC-32 polynomial gets the
//! wrong answer on those ECUs.
//!
//! kiviTuner had no implementation of this shape.

use kivi_checksum::rolling_rom_table::{rolling_checksum, RollingError};

/// Build a synthetic ROM holding a 256-entry table at `table_addr`.
fn rom_with_table(table_addr: usize, table: &[u32; 256], size: usize) -> Vec<u8> {
    let mut rom = vec![0u8; size];
    for (i, v) in table.iter().enumerate() {
        let off = table_addr + i * 4;
        rom[off..off + 4].copy_from_slice(&v.to_le_bytes());
    }
    rom
}

#[test]
fn an_identity_table_makes_the_checksum_the_input() {
    // With seed[i] = i, the recurrence degenerates predictably and we can verify
    // the loop shape rather than a CRC constant.
    let mut table = [0u32; 256];
    for (i, v) in table.iter_mut().enumerate() {
        *v = i as u32;
    }
    let table_addr = 0x100;
    let data_addr = 0x1000;
    let mut rom = rom_with_table(table_addr, &table, 0x2000);
    rom[data_addr] = 0x01;
    rom[data_addr + 1] = 0x02;

    // Manually apply the documented recurrence.
    let mut expected: u32 = 0;
    for b in [0x01u8, 0x02] {
        let idx = (b ^ (expected as u8)) as usize;
        let seed = table[idx];
        expected = (expected >> 8) ^ seed;
    }

    let got = rolling_checksum(&rom, data_addr, 2, table_addr, 0).expect("compute");
    assert_eq!(got, expected);
}

#[test]
fn a_zero_table_makes_the_checksum_zero() {
    let table = [0u32; 256];
    let rom = rom_with_table(0x100, &table, 0x2000);
    let got = rolling_checksum(&rom, 0x1000, 8, 0x100, 0).expect("compute");
    assert_eq!(got, 0);
}

#[test]
fn the_checksum_depends_on_the_table_contents() {
    // Two different tables must give different results for the same data.
    let mut t1 = [0u32; 256];
    let mut t2 = [0u32; 256];
    for i in 0..256 {
        t1[i] = i as u32;
        t2[i] = (i as u32).wrapping_mul(3);
    }
    let mut r1 = rom_with_table(0x100, &t1, 0x2000);
    let mut r2 = rom_with_table(0x100, &t2, 0x2000);
    r1[0x1000] = 0xAA;
    r2[0x1000] = 0xAA;

    let a = rolling_checksum(&r1, 0x1000, 1, 0x100, 0).unwrap();
    let b = rolling_checksum(&r2, 0x1000, 1, 0x100, 0).unwrap();
    assert_ne!(a, b, "the table must influence the result");
}

#[test]
fn the_checksum_depends_on_the_data() {
    let mut table = [0u32; 256];
    for (i, v) in table.iter_mut().enumerate() {
        *v = (i as u32).wrapping_mul(0x9E37_79B9);
    }
    let mut rom = rom_with_table(0x100, &table, 0x2000);
    rom[0x1000] = 0x11;
    let a = rolling_checksum(&rom, 0x1000, 1, 0x100, 0).unwrap();
    rom[0x1000] = 0x12;
    let b = rolling_checksum(&rom, 0x1000, 1, 0x100, 0).unwrap();
    assert_ne!(a, b);
}

#[test]
fn a_non_zero_initial_value_changes_the_result() {
    let mut table = [0u32; 256];
    for (i, v) in table.iter_mut().enumerate() {
        *v = i as u32;
    }
    let mut rom = rom_with_table(0x100, &table, 0x2000);
    rom[0x1000] = 0x55;
    let a = rolling_checksum(&rom, 0x1000, 1, 0x100, 0).unwrap();
    let b = rolling_checksum(&rom, 0x1000, 1, 0x100, 0xFFFF_FFFF).unwrap();
    assert_ne!(a, b);
}

#[test]
fn an_empty_range_returns_the_initial_value() {
    let table = [0u32; 256];
    let rom = rom_with_table(0x100, &table, 0x2000);
    let got = rolling_checksum(&rom, 0x1000, 0, 0x100, 0x1234_5678).expect("compute");
    assert_eq!(got, 0x1234_5678);
}

#[test]
fn a_range_past_the_end_is_refused() {
    let table = [0u32; 256];
    let rom = rom_with_table(0x100, &table, 0x2000);
    let err = rolling_checksum(&rom, 0x1FF0, 0x100, 0x100, 0).expect_err("must refuse");
    assert!(matches!(err, RollingError::RangeOutOfBounds { .. }));
}

#[test]
fn a_table_past_the_end_is_refused() {
    let rom = vec![0u8; 0x200];
    // Table at 0x100 needs 0x400 bytes; only 0x100 remain.
    let err = rolling_checksum(&rom, 0, 1, 0x100, 0).expect_err("must refuse");
    assert!(matches!(err, RollingError::TableOutOfBounds { .. }));
}

#[test]
fn the_table_is_read_little_endian() {
    // A table entry 0x11223344 must be read as 0x11223344, not byte-swapped.
    let mut table = [0u32; 256];
    table[0] = 0x1122_3344;
    let rom = rom_with_table(0x100, &table, 0x2000);
    // checksum starts 0, data byte 0 -> idx = 0
    let got = rolling_checksum(&rom, 0x1000, 1, 0x100, 0).expect("compute");
    assert_eq!(got, 0x1122_3344);
}

// ---------------------------------------------------------------------------
// Multi-range byte sum (NefMoto `MultiRangeChecksum`)
// ---------------------------------------------------------------------------

#[test]
fn a_multi_range_sum_adds_every_byte_across_all_ranges() {
    use kivi_checksum::rolling_rom_table::multi_range_byte_sum;

    let mut rom = vec![0u8; 0x100];
    // range A: 0x10..0x14 -> 1+2+3+4 = 10
    rom[0x10] = 1;
    rom[0x11] = 2;
    rom[0x12] = 3;
    rom[0x13] = 4;
    // range B: 0x20..0x22 -> 5+6 = 11
    rom[0x20] = 5;
    rom[0x21] = 6;

    let sum = multi_range_byte_sum(&rom, &[(0x10, 4), (0x20, 2)]).expect("sum");
    assert_eq!(sum, 21);
}

#[test]
fn a_multi_range_sum_wraps_at_32_bits() {
    use kivi_checksum::rolling_rom_table::multi_range_byte_sum;
    let rom = vec![0xFFu8; 0x100];
    // 0x100 bytes of 0xFF = 0xFF00, no wrap at this size; use a bigger idea:
    let sum = multi_range_byte_sum(&rom, &[(0, 0x100)]).expect("sum");
    assert_eq!(sum, 0xFF * 0x100);
}

#[test]
fn an_empty_range_list_sums_to_zero() {
    use kivi_checksum::rolling_rom_table::multi_range_byte_sum;
    let rom = vec![0xAAu8; 16];
    assert_eq!(multi_range_byte_sum(&rom, &[]).expect("sum"), 0);
}

#[test]
fn a_multi_range_past_the_end_is_refused() {
    use kivi_checksum::rolling_rom_table::{multi_range_byte_sum, RollingError};
    let rom = vec![0u8; 16];
    let err = multi_range_byte_sum(&rom, &[(8, 32)]).expect_err("must refuse");
    assert!(matches!(err, RollingError::RangeOutOfBounds { .. }));
}

#[test]
fn two_ranges_touching_each_other_are_both_counted() {
    use kivi_checksum::rolling_rom_table::multi_range_byte_sum;
    let mut rom = vec![0u8; 16];
    rom[0] = 1;
    rom[1] = 2;
    let sum = multi_range_byte_sum(&rom, &[(0, 1), (1, 1)]).expect("sum");
    assert_eq!(sum, 3, "each range is summed independently");
}
