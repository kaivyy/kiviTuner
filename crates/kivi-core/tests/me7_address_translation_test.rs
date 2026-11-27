//! ME7 ECU address → file-offset translation.
//!
//! ## Why this is needed
//!
//! A DAMOS file describes an ME7 ECU using **ECU memory addresses**, not file
//! offsets:
//!
//! ```text
//! /EAD, $810007            base of the flash window
//! /UMP, {}, abak, {...}, $38487C, 513, ...   <- internal RAM, not flash
//! /UMP, {}, kfzw, {...}, $810C4A, 513, ...   <- flash
//! ```
//!
//! A BIN dump is a flat file starting at offset 0. Comparing a DAMOS address
//! against a BIN offset without translating is meaningless — which is exactly
//! why the 100-pair catalog test reported 0% for DAMOS.
//!
//! ## The ME7 map
//!
//! | ECU window | Contents | File offset |
//! |---|---|---|
//! | `0x800000..0x87FFFF` | flash, 512 KiB | `addr - 0x800000` |
//! | `0x880000..0x8FFFFF` | flash, second 512 KiB on 1 MiB parts | `addr - 0x800000` |
//! | `0x380000..0x3FFFFF` | internal RAM | **no file offset** |
//!
//! The RAM window is the important part: a map that lives there is a runtime
//! variable, and a ROM dump cannot contain it. The translator must say so
//! rather than silently producing a bogus offset.

use kivi_core::address_translation::{
    translate_me7, Me7Translation, ME7_FLASH_BASE, ME7_RAM_END, ME7_RAM_START,
};

#[test]
fn the_flash_window_constants_match_the_me7_map() {
    assert_eq!(ME7_FLASH_BASE, 0x80_0000);
    assert_eq!(ME7_RAM_START, 0x38_0000);
    assert_eq!(ME7_RAM_END, 0x3F_FFFF);
}

#[test]
fn a_flash_address_translates_to_a_file_offset() {
    let t = translate_me7(0x81_0C4A);
    assert_eq!(t, Me7Translation::FileOffset(0x01_0C4A));
}

#[test]
fn the_first_flash_byte_maps_to_offset_zero() {
    assert_eq!(translate_me7(0x80_0000), Me7Translation::FileOffset(0));
}

#[test]
fn a_second_bank_address_maps_into_the_same_linear_file() {
    // On a 1 MiB part the window continues: 0x880000 -> offset 0x80000.
    assert_eq!(translate_me7(0x88_0000), Me7Translation::FileOffset(0x08_0000));
}

#[test]
fn a_ram_address_is_reported_as_ram_not_as_an_offset() {
    // 0x38487C is internal RAM. A ROM dump cannot contain it.
    assert_eq!(translate_me7(0x38_487C), Me7Translation::InternalRam);
}

#[test]
fn the_ram_window_boundaries_are_exact() {
    assert_eq!(translate_me7(ME7_RAM_START), Me7Translation::InternalRam);
    assert_eq!(translate_me7(ME7_RAM_END), Me7Translation::InternalRam);
    // One past the end is no longer RAM.
    assert_ne!(translate_me7(ME7_RAM_END + 1), Me7Translation::InternalRam);
}

#[test]
fn an_unmapped_address_is_reported_as_such() {
    assert_eq!(translate_me7(0x00_1000), Me7Translation::Unmapped);
}

#[test]
fn an_offset_past_the_largest_me7_flash_is_still_a_file_offset() {
    // 0x8F_FFFF is inside the second bank on a 1 MiB part.
    assert_eq!(translate_me7(0x8F_FFFF), Me7Translation::FileOffset(0x0F_FFFF));
}

#[test]
fn a_flash_address_beyond_one_megabyte_is_unmapped() {
    // 0x900000 is past both banks.
    assert_eq!(translate_me7(0x90_0000), Me7Translation::Unmapped);
}

#[test]
fn a_map_address_can_be_checked_against_a_rom_length() {
    // Convenience: does this DAMOS address land inside a ROM of this size?
    use kivi_core::address_translation::me7_offset_within;
    assert!(me7_offset_within(0x81_0C4A, 0x10_0000));
    assert!(!me7_offset_within(0x81_0C4A, 0x01_0000));
    assert!(!me7_offset_within(0x38_487C, 0x10_0000));
}
