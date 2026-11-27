//! Tests for flash-region overlap detection.
//!
//! ## Why overlap checks matter
//!
//! Intersecting flash memory areas across loadable modules make the final programmed
//! result depend on write order — which is not guaranteed across independent plans.
//! That produces an image matching neither plan, often with no write error at all.
//! Detecting intersections before the first byte is written prevents partial overwrites.

use kivi_flashing::region_overlap::{check_no_overlap, RegionOverlapError, FlashRegion};

fn region(address: u32, length: usize) -> FlashRegion {
    FlashRegion { address, length }
}

// ---------------------------------------------------------------------------
// Non-overlapping cases
// ---------------------------------------------------------------------------

#[test]
fn a_single_region_never_overlaps() {
    assert!(check_no_overlap(&[region(0x0000, 0x100)]).is_ok());
}

#[test]
fn two_regions_that_touch_but_do_not_overlap_are_allowed() {
    // [0x0000, 0x0100) and [0x0100, 0x0200) share a boundary but no byte.
    let r = check_no_overlap(&[region(0x0000, 0x100), region(0x0100, 0x100)]);
    assert!(r.is_ok(), "abutting regions are not an overlap");
}

#[test]
fn two_separated_regions_are_allowed() {
    assert!(check_no_overlap(&[region(0x0000, 0x100), region(0x1000, 0x100)]).is_ok());
}

#[test]
fn an_empty_list_is_allowed() {
    assert!(check_no_overlap(&[]).is_ok());
}

#[test]
fn zero_length_regions_never_overlap() {
    // A zero-length region covers no byte, so it cannot collide.
    let r = check_no_overlap(&[region(0x0000, 0x100), region(0x0010, 0)]);
    assert!(r.is_ok());
}

// ---------------------------------------------------------------------------
// Overlapping cases
// ---------------------------------------------------------------------------

#[test]
fn two_identical_regions_overlap() {
    let err = check_no_overlap(&[region(0x0000, 0x100), region(0x0000, 0x100)])
        .expect_err("identical regions must be rejected");
    assert!(matches!(err, RegionOverlapError::Overlap { .. }));
}

#[test]
fn a_partially_overlapping_region_is_rejected() {
    // [0x0000, 0x0100) and [0x0080, 0x0180) share 0x80 bytes.
    let err = check_no_overlap(&[region(0x0000, 0x100), region(0x0080, 0x100)])
        .expect_err("partial overlap must be rejected");
    assert!(matches!(err, RegionOverlapError::Overlap { .. }));
}

#[test]
fn a_region_fully_inside_another_is_rejected() {
    let err = check_no_overlap(&[region(0x0000, 0x1000), region(0x0100, 0x100)])
        .expect_err("contained region must be rejected");
    assert!(matches!(err, RegionOverlapError::Overlap { .. }));
}

#[test]
fn the_error_names_both_offending_regions() {
    let err = check_no_overlap(&[region(0x0000, 0x100), region(0x0080, 0x100)]).unwrap_err();
    let text = err.to_string();
    assert!(text.contains("0x00000000") || text.contains("0x0"), "was: {text}");
    assert!(text.contains("0x00000080") || text.contains("0x80"), "was: {text}");
}

#[test]
fn overlap_is_detected_regardless_of_order() {
    let a = region(0x0080, 0x100);
    let b = region(0x0000, 0x100);
    assert!(check_no_overlap(&[a, b]).is_err());
    assert!(check_no_overlap(&[b, a]).is_err());
}

#[test]
fn a_three_way_overlap_is_detected() {
    let r = check_no_overlap(&[
        region(0x0000, 0x100),
        region(0x2000, 0x100),
        region(0x0040, 0x100),
    ]);
    assert!(r.is_err(), "the first and third regions overlap");
}

#[test]
fn a_region_ending_at_the_address_limit_does_not_overflow() {
    // u32::MAX is the last byte; a length that would wrap must not panic.
    let r = check_no_overlap(&[region(u32::MAX - 1, 2)]);
    assert!(r.is_ok());
}

// ---------------------------------------------------------------------------
// Ordering
// ---------------------------------------------------------------------------

#[test]
fn the_reported_pair_is_the_first_collision_found() {
    let err = check_no_overlap(&[
        region(0x0000, 0x100),
        region(0x2000, 0x100),
        region(0x0040, 0x100), // collides with the first
    ])
    .unwrap_err();
    match err {
        RegionOverlapError::Overlap { first, second } => {
            assert_eq!(first.address, 0x0000);
            assert_eq!(second.address, 0x0040);
        }
    }
}
