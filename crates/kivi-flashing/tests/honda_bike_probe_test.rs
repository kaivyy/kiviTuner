//! Tests for the Honda bike Keihin K-Line probe and bootloader entry.
//!
//! ## Source
//!
//! Extracted from `module76.dll` (Honda bike kline, module 76) by reading the
//! `push` immediates that build each frame. Two frames are byte-verified:
//!
//! - probe: `68 82 82 00 09` then `68 00 fc df 04` → `[0x82,0x82,0x00,0x09,...]`
//! - bootloader entry: `68 50 47 4d 94` and `68 7d 08 01 02` → `"PGM"` + `0x94`
//!
//! Neither was present in kiviTuner before this module.

use kivi_flashing::honda_bike_probe::{
    build_bootloader_entry, build_probe_frame, build_read_frame, probe_checksum, McuSize,
};

// ---------------------------------------------------------------------------
// Probe frame
// ---------------------------------------------------------------------------

#[test]
fn the_probe_frame_starts_with_the_documented_header() {
    // module76.dll pushes 0x82, 0x82, 0x00, 0x09 in that order.
    let f = build_probe_frame(0x00DFFC, 4);
    assert_eq!(&f[..4], &[0x82, 0x82, 0x00, 0x09]);
}

#[test]
fn the_probe_frame_is_nine_bytes() {
    assert_eq!(build_probe_frame(0x00DFFC, 4).len(), 9);
}

#[test]
fn the_probe_frame_uses_the_documented_byte_order() {
    // The address bytes appear in the order [bits 16..23, bits 0..7, bits 8..15]
    // — not big-endian, not little-endian. This is the detail that makes the
    // frame work.
    let f = build_probe_frame(0x00DFFC, 4);
    assert_eq!(f[4], 0x00, "bits 16..23");
    assert_eq!(f[5], 0xFC, "bits 0..7");
    assert_eq!(f[6], 0xDF, "bits 8..15");
}

#[test]
fn the_probe_frame_carries_the_requested_length() {
    let f = build_probe_frame(0x00DFFC, 0x10);
    assert_eq!(f[7], 0x10);
}

#[test]
fn the_probe_frame_ends_with_a_twos_complement_checksum() {
    let f = build_probe_frame(0x00DFFC, 4);
    let sum: u8 = f[..8].iter().fold(0u8, |a, &b| a.wrapping_add(b));
    let expected = (!sum).wrapping_add(1);
    assert_eq!(f[8], expected, "last byte must be the two's complement");
}

#[test]
fn the_checksum_helper_agrees_with_the_frame() {
    let f = build_probe_frame(0x123456, 8);
    assert_eq!(f[8], probe_checksum(&f[..8]));
}

// ---------------------------------------------------------------------------
// Read frame
// ---------------------------------------------------------------------------

#[test]
fn a_read_frame_has_the_same_layout_as_a_probe() {
    // The probe and the read use the same shape; only the length differs.
    let read = build_read_frame(0x00DFFC, 4);
    let probe = build_probe_frame(0x00DFFC, 4);
    assert_eq!(read, probe);
}

#[test]
fn a_read_frame_at_a_different_address_differs() {
    assert_ne!(build_read_frame(0x001000, 4), build_read_frame(0x002000, 4));
}

// ---------------------------------------------------------------------------
// Bootloader entry ("PGM")
// ---------------------------------------------------------------------------

#[test]
fn the_bootloader_entry_contains_the_pgm_marker() {
    // module76.dll pushes 0x50,0x47,0x4D = 'P','G','M', then 0x94.
    let f = build_bootloader_entry();
    assert!(
        f.windows(3).any(|w| w == b"PGM"),
        "the entry frame must contain the ASCII marker 'PGM': {f:02x?}"
    );
}

#[test]
fn the_bootloader_entry_ends_with_the_documented_terminator() {
    // 0x94 follows "PGM" in the pushed immediate.
    let f = build_bootloader_entry();
    let idx = f.windows(3).position(|w| w == b"PGM").expect("PGM");
    assert_eq!(f[idx + 3], 0x94);
}

#[test]
fn the_bootloader_entry_starts_with_the_documented_header() {
    // module76.dll pushes 0x7D, 0x08, 0x01, 0x02 before the marker.
    let f = build_bootloader_entry();
    assert_eq!(&f[..4], &[0x7D, 0x08, 0x01, 0x02]);
}

#[test]
fn the_bootloader_entry_is_eight_bytes() {
    assert_eq!(build_bootloader_entry().len(), 8);
}

// ---------------------------------------------------------------------------
// MCU size
// ---------------------------------------------------------------------------

#[test]
fn the_documented_mcu_sizes_are_available() {
    assert_eq!(McuSize::Kb56.bytes(), 56 * 1024);
    assert_eq!(McuSize::Kb128.bytes(), 128 * 1024);
    assert_eq!(McuSize::Kb192.bytes(), 192 * 1024);
    assert_eq!(McuSize::Kb256.bytes(), 256 * 1024);
    assert_eq!(McuSize::Kb384.bytes(), 384 * 1024);
    assert_eq!(McuSize::Kb512.bytes(), 512 * 1024);
    assert_eq!(McuSize::R8c96.bytes(), 96 * 1024);
}

#[test]
fn an_unknown_size_is_represented_explicitly() {
    assert_eq!(McuSize::Unknown.bytes(), 0);
}
