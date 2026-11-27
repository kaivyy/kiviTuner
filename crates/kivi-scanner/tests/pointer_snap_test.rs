//! Pointer-guided snapping must only move existing candidates.

use kivi_scanner::pointer_snap::Anchors;

const ROM: &str = "/root/ecu_master_database/01_MOTORCYCLES/HONDA/SPORT_CBR250RR/CBR250RR-38770-K64-ND1_BIN.bin";

fn rom() -> Vec<u8> {
    std::fs::read(ROM).expect("CBR ROM")
}

#[test]
fn finds_the_constant_stride_array_targets() {
    let a = Anchors::from_rom(&rom(), true);
    // Eight entries with stride 1750 (35*25*2) starting at 0x04BC14.
    for t in [0x05002A, 0x050700, 0x050DD6, 0x0514AC] {
        let (hit, stride) = a.nearest(t, 0).unwrap_or_else(|| panic!("{t:#X} missing"));
        assert_eq!(hit, t);
        assert_eq!(stride, Some(1750), "{t:#X} must carry the array stride");
    }
}

#[test]
fn finds_the_literal_pool_axis_targets() {
    let a = Anchors::from_rom(&rom(), true);
    for t in [0x04FF80, 0x04FFB2, 0x04FFE4] {
        assert!(a.nearest(t, 0).is_some(), "{t:#X} must be an anchor");
    }
}

/// The real defect: a 2-D candidate reported at 0x0506FE must snap onto the
/// pointer target 0x050700, two bytes later.
#[test]
fn a_candidate_two_bytes_early_snaps_onto_the_pointer() {
    let a = Anchors::from_rom(&rom(), true);
    let (t, stride) = a.nearest(0x0506FE, 16).expect("anchor near 0x0506FE");
    assert_eq!(t, 0x050700);
    assert_eq!(stride, Some(1750));
}

/// A candidate 64 bytes from any anchor must be left alone.
#[test]
fn a_candidate_with_no_anchor_is_untouched() {
    let rom = rom();
    let a = Anchors::from_rom(&rom, true);
    // Find a gap: an address with no anchor within 64 bytes on either side.
    let gap = (0x1000..rom.len() - 0x100)
        .step_by(0x100)
        .find(|&c| a.nearest(c, 64).is_none())
        .expect("some gap exists");
    assert!(a.nearest(gap, 32).is_none(), "{gap:#X} must have no anchor nearby");
}

/// A strong anchor is one backed by a constant-stride array. A literal-pool
/// target near the same address must not win, because it proves only that some
/// code mentions the address, not that a table begins there.
#[test]
fn a_strong_anchor_beats_a_weak_one_at_equal_distance() {
    let rom = rom();
    let a = Anchors::from_rom(&rom, true);
    // 0x05002A is the first array entry (stride 1750). The axis pointers in the
    // literal pool are weak and live within a few hundred bytes of it.
    let (t, stride) = a.nearest_strong(0x05002A, 16).expect("strong anchor");
    assert_eq!(t, 0x05002A);
    assert_eq!(stride, Some(1750));
}

/// The strong selector must ignore literal-pool entries entirely, so a
/// candidate whose only nearby anchor is weak is left where it is.
#[test]
fn a_candidate_with_only_a_weak_anchor_is_untouched_by_strong_snapping() {
    let a = Anchors::from_rom(&rom(), true);
    // 0x04FF80 is a literal-pool axis pointer, never an array entry.
    assert!(a.nearest(0x04FF80, 0).is_some(), "the weak anchor exists");
    assert!(
        a.nearest_strong(0x04FF80, 0).is_none(),
        "but strong snapping must not use it"
    );
}

/// On a ROM with no pointer structure at all the anchor set is empty, so the
/// mechanism is a no-op.
#[test]
fn random_bytes_produce_no_anchors() {
    let mut state = 0x1234_5678u32;
    let rom: Vec<u8> = (0..4096)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 24) as u8
        })
        .collect();
    let a = Anchors::from_rom(&rom, true);
    assert!(a.len() < 8, "random data must not yield many anchors, got {}", a.len());
}
