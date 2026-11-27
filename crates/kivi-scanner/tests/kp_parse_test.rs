//! Reading a plaintext map-pack file.
//!
//! ## What was found
//!
//! A map-pack file (`.kp`) has two distinct states in the real corpus on this
//! machine:
//!
//! | File | Body entropy | Readable |
//! |---|---:|---|
//! | `test8maps.kp` (9 257 B) | **1.67** | **yes** |
//! | `8D0907551M.kp` (43 KB) | 7.05 | no |
//! | `e34.kp` (92 KB) | 7.46 | no |
//!
//! The high-entropy files are compressed or encrypted; nothing inside them can
//! be read without the vendor's codec. The low-entropy file is plaintext and
//! carries everything a calibration editor needs, with the address written
//! directly into the map name:
//!
//! ```text
//! CW_NOROMCHKRESET (AR 11F6B)
//! TVFSREM (AR 1A378)
//! ```
//!
//! `AR` stands for the German *Adresse*; the hex digits are the ROM offset.
//!
//! ## What this parser does
//!
//! It reads the plaintext form and **refuses** the opaque form with a clear
//! reason, rather than returning an empty list that a caller could mistake for
//! "this pack has no maps".

use kivi_scanner::kp_parse::{parse_map_pack, MapPackError};

const PLAINTEXT: &str = r#"WinOLS File
Some header text
Original
Version 1
Codeword: No ROM check reset
CW_NOROMCHKRESET (AR 11F6B)
1 : 1 Umrechnug (dez/hex)
Factor for maximum pressure ratio with auxilliary load signal
FPVMXN2 (RS4 1C00E)
U/min
Motor temperature dependent delay time for drive on (R)
TVFSREM (AR 1A378)
Grad C
Minimum engine speed for misfire recognition fade-out
NMIALU (AR 105C8)
"#;

#[test]
fn the_plaintext_pack_parses() {
    let pack = parse_map_pack(PLAINTEXT.as_bytes()).expect("parse");
    assert_eq!(pack.maps.len(), 3);
}

#[test]
fn the_map_names_are_captured() {
    let pack = parse_map_pack(PLAINTEXT.as_bytes()).expect("parse");
    let names: Vec<&str> = pack.maps.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, vec!["CW_NOROMCHKRESET", "TVFSREM", "NMIALU"]);
}

#[test]
fn the_address_is_decoded_from_the_name_suffix() {
    let pack = parse_map_pack(PLAINTEXT.as_bytes()).expect("parse");
    assert_eq!(pack.maps[0].address, 0x11F6B);
    assert_eq!(pack.maps[1].address, 0x1A378);
    assert_eq!(pack.maps[2].address, 0x105C8);
}

#[test]
fn a_map_without_an_address_suffix_is_skipped() {
    // "FPVMXN2 (RS4 1C00E)" is a different annotation, not a plain AR address.
    let pack = parse_map_pack(PLAINTEXT.as_bytes()).expect("parse");
    assert!(
        !pack.maps.iter().any(|m| m.name == "FPVMXN2"),
        "a map with no AR address must not be reported with a guessed one"
    );
}

#[test]
fn the_units_line_after_a_map_is_captured_when_present() {
    let pack = parse_map_pack(PLAINTEXT.as_bytes()).expect("parse");
    let tv = pack.maps.iter().find(|m| m.name == "TVFSREM").unwrap();
    assert_eq!(tv.unit, "Grad C");
}

#[test]
fn a_map_with_no_following_unit_gets_an_empty_one() {
    let pack = parse_map_pack(PLAINTEXT.as_bytes()).expect("parse");
    let nm = pack.maps.iter().find(|m| m.name == "NMIALU").unwrap();
    assert_eq!(nm.unit, "");
}

// ---------------------------------------------------------------------------
// Opaque packs are refused, not silently empty
// ---------------------------------------------------------------------------

#[test]
fn a_high_entropy_pack_is_reported_as_opaque() {
    // Synthetic: bytes spread evenly across the range, like the real encrypted
    // packs (entropy 7.05-7.46).
    let opaque: Vec<u8> = (0..4096u32).map(|i| (i.wrapping_mul(97) & 0xFF) as u8).collect();
    let err = parse_map_pack(&opaque).unwrap_err();
    assert!(matches!(err, MapPackError::Opaque { .. }));
}

#[test]
fn an_empty_input_is_reported_as_empty() {
    assert!(matches!(
        parse_map_pack(b"").unwrap_err(),
        MapPackError::Empty
    ));
}

#[test]
fn a_plaintext_pack_with_no_maps_is_reported_as_such() {
    let no_maps = b"WinOLS File\nOriginal\nVersion 1\n";
    let err = parse_map_pack(no_maps).unwrap_err();
    assert!(matches!(err, MapPackError::NoMaps));
}

#[test]
fn the_opaque_error_names_the_entropy() {
    let opaque: Vec<u8> = (0..4096u32).map(|i| (i.wrapping_mul(97) & 0xFF) as u8).collect();
    let msg = parse_map_pack(&opaque).unwrap_err().to_string();
    assert!(msg.contains("entropy"), "message was: {msg}");
}

// ---------------------------------------------------------------------------
// Robustness
// ---------------------------------------------------------------------------

#[test]
fn a_lowercase_address_suffix_still_parses() {
    let lower = b"SomeMap (AR 1a378)\n";
    let pack = parse_map_pack(lower).expect("parse");
    assert_eq!(pack.maps[0].address, 0x1A378);
}

#[test]
fn an_out_of_range_address_is_rejected_rather_than_truncated() {
    // 9 hex digits does not fit a u32.
    let bad = b"SomeMap (AR 123456789)\n";
    let err = parse_map_pack(bad).unwrap_err();
    assert!(matches!(err, MapPackError::NoMaps));
}

#[test]
fn the_pack_reports_its_declared_ecu_when_present() {
    let with_ecu = b"WinOLS File\n8D0907551M\nOriginal\nVersion 1\nKFZW (AR 1B470C)\n";
    let pack = parse_map_pack(with_ecu).expect("parse");
    assert!(pack.declared_ecu.is_some(), "the ECU part number should be found");
}
