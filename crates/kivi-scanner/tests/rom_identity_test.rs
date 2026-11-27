//! Does a definition describe the ROM it was paired with?
//!
//! ## The failure this exists to catch
//!
//! `CBR250RR-38770-K64-ND1_BIN.bin` carries the identity string **`K64J108`**,
//! while its paired XDF is titled `38770-K64-ND1`. The XDF declares 41 of its 52
//! tables as 35 rows, but the ROM's axis at that address has only 24 breakpoints
//! and falls from 5534 to 645 where a 35th would have to continue.
//!
//! Measuring blind-scan recall against that XDF produces a number that says
//! nothing about the detector. These tests make the mismatch detectable.

use kivi_scanner::rom_identity::{
    compare_identity, extract_definition_tokens, extract_rom_tokens, normalise, IdentityMatch,
};

const CBR_ROM: &str = "/root/ecu_master_database/01_MOTORCYCLES/HONDA/SPORT_CBR250RR/CBR250RR-38770-K64-ND1_BIN.bin";

#[test]
fn separators_do_not_affect_comparison() {
    assert_eq!(normalise("38770-K64-ND1"), normalise("38770_K64_ND1"));
    assert_eq!(normalise("k64j108"), "K64J108");
}

#[test]
fn the_cbr_rom_identity_is_extracted() {
    let rom = std::fs::read(CBR_ROM).expect("fixture");
    let tokens = extract_rom_tokens(&rom);
    assert!(
        tokens.iter().any(|t| t.token == "K64J108"),
        "expected K64J108 among {:?}",
        tokens.iter().map(|t| &t.token).collect::<Vec<_>>()
    );
}

#[test]
fn the_cbr_pair_is_reported_as_mismatched() {
    let rom = std::fs::read(CBR_ROM).expect("fixture");
    // The XDF title names ND1; the ROM says J108.
    let m = compare_identity(
        &rom,
        "CBR250RR_38770-K64-ND1_PUBLIC_v1.xdf",
        "CBR250RR-38770-K64-ND1",
    );
    assert!(
        !m.is_confirmed(),
        "this pair does not share a part number, got {m:?}"
    );
}

#[test]
fn a_matching_pair_is_confirmed() {
    let rom = std::fs::read(CBR_ROM).expect("fixture");
    // A definition naming the ROM's real identity must be accepted.
    let m = compare_identity(&rom, "K64J108.xdf", "CBR250RR K64J108");
    assert!(m.is_confirmed(), "expected a match, got {m:?}");
}

#[test]
fn a_definition_without_a_part_number_is_inconclusive_not_mismatched() {
    let rom = std::fs::read(CBR_ROM).expect("fixture");
    let m = compare_identity(&rom, "maps.xdf", "My Maps");
    assert!(
        matches!(m, IdentityMatch::Inconclusive),
        "an unnamed definition cannot be judged, got {m:?}"
    );
}

#[test]
fn a_rom_without_identity_strings_is_inconclusive() {
    let rom = vec![0u8; 4096];
    let m = compare_identity(&rom, "38770-K64-ND1.xdf", "38770-K64-ND1");
    assert!(matches!(m, IdentityMatch::Inconclusive), "got {m:?}");
}

#[test]
fn a_bare_all_digit_name_is_not_a_part_number() {
    // `12212156` alone is an operating-system number, not a part number. The
    // filename must not contribute a token for it.
    let tokens = extract_definition_tokens("12212156.xdf", "");
    assert!(
        tokens.is_empty(),
        "an all-digit name must not be treated as a part number: {tokens:?}"
    );
}

#[test]
fn a_prefixed_identifier_is_accepted() {
    // `OS 12212156` carries a letter prefix, so it is a usable identifier.
    let tokens = extract_definition_tokens("", "OS 12212156");
    assert!(
        tokens.iter().any(|t| t.token == "OS12212156"),
        "a letter-prefixed identifier should be usable: {tokens:?}"
    );
}

#[test]
fn a_full_part_number_matches_a_suffix_stored_in_the_rom() {
    let rom = std::fs::read(CBR_ROM).expect("fixture");
    // Definition carries the full 38770-K64-J108 form; the ROM stores K64J108.
    let m = compare_identity(&rom, "38770-K64-J108.xdf", "Honda CBR250RR");
    assert!(m.is_confirmed(), "expected suffix match, got {m:?}");
}
