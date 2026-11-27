//! Boundary snapping wired into the detector path (`CurveScanOptions`).
//!
//! The unit tests in `boundary_snap_test.rs` exercise the snapper in isolation.
//! These exercise it *through* `scan_curves`, which is what the evaluator
//! actually calls, so a regression in the plumbing is caught here.

use kivi_scanner::curve_scan::{scan_curves, scan_curves_snapped, CurveScanOptions, Endian};

/// A smooth rising run preceded by an interleaved junk prefix, planted in a
/// ROM. The prefix mimics the CBR250RR layout: `big, small, big, small` pairs.
fn rom_with_prefixed_ramp() -> (Vec<u8>, usize) {
    // Non-zero filler: `analyze_rom_blocks` drops long runs of padding, so an
    // all-zero ROM has no kept range and the scan returns nothing at all.
    let mut rom: Vec<u8> = (0..4096).map(|i| ((i * 7) % 251) as u8).collect();
    // Junk prefix: alternating high/low, the shape that bleeds.
    let junk: [u16; 6] = [4000, 33, 3000, 33, 1500, 33];
    let ramp: [u16; 10] = [3, 8, 12, 16, 25, 33, 41, 49, 58, 66];
    let at = 0x200;
    for (i, v) in junk.iter().chain(ramp.iter()).enumerate() {
        let b = v.to_be_bytes();
        rom[at + i * 2] = b[0];
        rom[at + i * 2 + 1] = b[1];
    }
    (rom, at + junk.len() * 2)
}

fn opts(snap: bool) -> CurveScanOptions {
    CurveScanOptions {
        min_points: 3,
        max_points: 20,
        endian: Endian::Big,
        include_8bit: false,
        allow_monotonic: true,
        snap_boundaries: snap,
    }
}

/// With snapping off, the reported start is the raw window start — the
/// behaviour every existing caller depends on.
#[test]
fn snapping_off_reports_the_raw_window_start() {
    let (rom, ramp_at) = rom_with_prefixed_ramp();
    let found = scan_curves(&rom, &opts(false));
    let hit = found
        .iter()
        .find(|c| c.address <= ramp_at && ramp_at < c.address + c.points * 2)
        .expect("a window covering the ramp");
    assert!(hit.address < ramp_at, "raw window should start before the ramp");
}

/// With snapping on, the curve's start must move forward onto the ramp.
#[test]
fn snapping_on_moves_the_start_onto_the_real_run() {
    let (rom, ramp_at) = rom_with_prefixed_ramp();
    let found = scan_curves(&rom, &opts(true));
    let hit = found
        .iter()
        .find(|c| c.address == ramp_at)
        .unwrap_or_else(|| {
            panic!(
                "no curve snapped to {ramp_at:#X}; got {:?}",
                found.iter().map(|c| (c.address, c.points)).collect::<Vec<_>>()
            )
        });
    assert!(hit.points >= 3);
    assert_eq!(hit.values[0], 3.0, "first cell must be the ramp's first");
}

/// The bookkeeping must be exact: `raw_address + trimmed_head * 2` is the new
/// address, and `raw .. raw+points` minus both trims is the new length.
#[test]
fn snapped_bookkeeping_reconstructs_the_raw_window() {
    let (rom, ramp_at) = rom_with_prefixed_ramp();
    let with_snap = scan_curves_snapped(&rom, &opts(true));
    let hit = with_snap.iter().find(|s| s.curve.address == ramp_at).expect("snapped");

    assert_eq!(
        hit.raw_address + hit.trimmed_head * 2,
        hit.curve.address,
        "trimmed_head must explain the shift"
    );
    let raw = scan_curves_snapped(&rom, &opts(false));
    let raw_hit = raw
        .iter()
        .find(|s| s.raw_address == hit.raw_address && s.curve.points > hit.curve.points)
        .or_else(|| raw.iter().find(|s| s.raw_address == hit.raw_address));
    if let Some(r) = raw_hit {
        assert_eq!(
            r.curve.points,
            hit.trimmed_head + hit.curve.points + hit.trimmed_tail,
            "head + body + tail must equal the raw length"
        );
    }
}

/// `scan_curves` with snapping off is byte-identical to the pre-integration
/// behaviour: same addresses, same lengths, same values.
#[test]
fn snapping_off_is_byte_identical_to_the_raw_scan() {
    let (rom, _) = rom_with_prefixed_ramp();
    let a: Vec<_> = scan_curves_snapped(&rom, &opts(false))
        .into_iter()
        .map(|s| (s.curve.address, s.curve.points, s.raw_address, s.trimmed_head, s.trimmed_tail))
        .collect();
    let b: Vec<_> = scan_curves(&rom, &opts(false))
        .into_iter()
        .map(|c| (c.address, c.points, c.address, 0, 0))
        .collect();
    assert_eq!(a, b);
}

/// Duplicate post-snap addresses collapse to one candidate.
///
/// Several raw windows can land on the same snapped start (the CBR axes report
/// 4-6 windows each). The longest is kept.
#[test]
fn post_scan_dedup_keeps_one_candidate_per_snapped_address() {
    let (rom, _) = rom_with_prefixed_ramp();
    let out = scan_curves_snapped(&rom, &opts(true));
    let mut addrs: Vec<usize> = out.iter().map(|s| s.curve.address).collect();
    let before = addrs.len();
    addrs.sort_unstable();
    addrs.dedup();
    assert_eq!(
        addrs.len(),
        before,
        "every snapped address must appear once; {before} entries, {} unique",
        addrs.len()
    );
}

/// A candidate that snaps down to fewer than `min_points` cells is dropped.
///
/// This must not remove any *exact* hit: the guard is asserted on the real
/// fixtures by `real_rom_boundary_test`.
#[test]
fn post_scan_filter_drops_short_fragments() {
    let (rom, _) = rom_with_prefixed_ramp();
    let opts = CurveScanOptions { min_points: 5, ..opts(true) };
    for s in scan_curves_snapped(&rom, &opts) {
        assert!(
            s.curve.points >= opts.min_points,
            "fragment of {} cells survived min_points={}",
            s.curve.points,
            opts.min_points
        );
        let uniq: std::collections::HashSet<u32> =
            s.curve.values.iter().map(|v| *v as u32).collect();
        assert!(uniq.len() >= 3, "two-valued fragment survived");
    }
}
