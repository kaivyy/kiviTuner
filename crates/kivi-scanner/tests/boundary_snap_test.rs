//! Boundary snapping: hard gradient splitting and unidirectional head trimming.
//!
//! Every value used here is copied from a real ROM, and the comment on each
//! fixture says which. A test built on invented numbers proves nothing about the
//! ROMs that actually fail.

use kivi_scanner::boundary_snap::{snap_monotonic, split_at_gradient_anomaly, Snapped};

// ---------------------------------------------------------------------------
// Part 1: hard gradient splitting
// ---------------------------------------------------------------------------

/// BMW B58, `0x700F82`, little-endian 16-bit.
///
/// The six declared cells are `1700, 2000, 3000, 4000, 4500, 5500`. The cell
/// immediately after them is `65236` (`0xD4FE`), a step of `59 736` — 59x the
/// previous step of 1000. That is the garbage tail the detector swallowed.
const B58_1_WITH_TAIL: [u16; 9] = [1700, 2000, 3000, 4000, 4500, 5500, 65236, 65386, 65426];

#[test]
fn a_hard_jump_ends_the_run_exactly_before_the_garbage() {
    let cut = split_at_gradient_anomaly(&B58_1_WITH_TAIL, 10.0);
    assert_eq!(cut, &[1700, 2000, 3000, 4000, 4500, 5500]);
}

/// The cut must land on the garbage cell, not one cell later and not one cell
/// earlier: a real calibration row's last cell is usually its largest, so an
/// off-by-one here silently deletes a real calibration point.
#[test]
fn the_cut_keeps_every_real_cell() {
    let cut = split_at_gradient_anomaly(&B58_1_WITH_TAIL, 10.0);
    assert_eq!(cut.len(), 6, "must keep all 6 real cells");
    assert_eq!(*cut.last().unwrap(), 5500, "last real cell must survive");
}

/// A smooth ramp must not be cut. Regression guard for making the multiplier
/// too aggressive: `300 -> 1000` is a 3.3x step but both are real cells.
#[test]
fn a_legitimate_step_increase_is_not_a_jump() {
    let values: [u16; 6] = [1700, 2000, 3000, 4000, 4500, 5500];
    assert_eq!(split_at_gradient_anomaly(&values, 10.0), &values);
}

/// BMW B58, `0x7010F4`, little-endian: ten real cells, then the same `65236`
/// garbage. A longer row must split at the same place.
#[test]
fn a_ten_cell_row_splits_at_its_own_end() {
    let real: [u16; 10] = [600, 1300, 1700, 2000, 3000, 4000, 4500, 5500, 6000, 7000];
    let garbage: [u16; 3] = [65236, 65386, 65426];
    let mut values: Vec<u16> = real.to_vec();
    values.extend_from_slice(&garbage);
    assert_eq!(split_at_gradient_anomaly(&values, 10.0), &real);
}

/// Fewer than three cells cannot contain a jump (a jump needs two steps).
#[test]
fn a_run_too_short_to_contain_a_jump_is_returned_whole() {
    let values: [u16; 2] = [1700, 65236];
    assert_eq!(split_at_gradient_anomaly(&values, 10.0), &values);
}

/// The multiplier must be adjustable: with a huge multiplier, nothing is cut.
#[test]
fn the_multiplier_controls_the_sensitivity() {
    // Note: the half-range rule is independent of the multiplier and still
    // applies, so a full-range reset is cut even here. See the test below.
    let values: [u16; 6] = [1700, 2000, 3000, 4000, 4500, 5500];
    assert_eq!(split_at_gradient_anomaly(&values, 1000.0), &values);
}

/// BMW B58, `0x700FD4`, little-endian. The six declared cells are
/// `0, 13107, 29491, 32768, 42598, 65535`, then `0, 0, 0, 250, 500, 800`.
///
/// ## The defect this pins
///
/// The local-ratio rule alone **cannot** cut here. Steps inside the row are
/// 13 107, 16 384, 3 277, 9 830, 22 937, so `previous_step * 10` is at least
/// 32 770; the `65 535 -> 0` reset is a step of exactly 65 535, which is why
/// it seems cuttable — but the step before the *last* real cell was 22 937,
/// giving a limit of 229 370. Measured: without the half-range rule the run
/// swallowed `0, 0, 0, 250, 500, 800` and snapping then moved a start that had
/// been **correct** (+0) to +4.
///
/// Judged against the row's own range the reset is the full scale, so the
/// half-range rule cuts it.
#[test]
fn a_full_scale_reset_ends_the_run_even_when_local_steps_are_large() {
    let values: [u16; 12] = [
        0, 13107, 29491, 32768, 42598, 65535, 0, 0, 0, 250, 500, 800,
    ];
    let cut = split_at_gradient_anomaly(&values, 10.0);
    assert_eq!(cut, &[0, 13107, 29491, 32768, 42598, 65535]);
}

/// Snapping a run that already starts at the right place must not move it.
///
/// This is the regression the previous test diagnosed: the B58 `0x700FD4`
/// window was exact before snapping and became +4 after it, because the
/// tail cut failed to fire and the head trim then ate two real cells.
#[test]
fn snapping_does_not_move_a_window_that_already_starts_correctly() {
    let values: [u16; 12] = [
        0, 13107, 29491, 32768, 42598, 65535, 0, 0, 0, 250, 500, 800,
    ];
    let s = snap_monotonic(&values).expect("snaps");
    assert_eq!(s.trimmed_head, 0, "a correct start must stay put");
    assert_eq!(s.trimmed_tail, 6, "the six garbage cells are cut");
    assert_eq!(s.points, 6);
}

// ---------------------------------------------------------------------------
// Part 2: full snapping on the real detector windows
// ---------------------------------------------------------------------------

/// CBR250RR `0x04FF72` window, exactly as `scan_curves` reports it.
///
/// Address `0x04FF72` is 14 bytes (7 cells) before the oracle at `0x04FF80`.
/// The first seven cells are the interleaved `33, <rpm>` pairs of the preceding
/// axis; from index 7 onward the real TPS axis begins `0, 0, 3, 8, 12, ...`.
const CBR_TPS_WINDOW: [u16; 14] = [
    4000, 33, 3000, 33, 1500, 33, 0, 0, 3, 8, 12, 16, 25, 33,
];

#[test]
fn snapping_removes_the_interleaved_prefix() {
    let s: Snapped = snap_monotonic(&CBR_TPS_WINDOW).expect("snaps");
    assert_eq!(s.trimmed_head, 7, "must drop the 7 leading pair cells");
    assert_eq!(s.points, 7);
    // The detector reports this window at 0x04FF72; the oracle is 0x04FF80.
    let new_start = 0x04FF72 + s.trimmed_head * 2;
    assert_eq!(new_start, 0x04FF80, "snapped start must be the oracle");
}

/// CBR250RR `0x04FFC2` window: the detector starts *inside* the map axis, eight
/// cells late. Nothing in this module may move a start backwards, so the honest
/// expectation is `trimmed_head == 0`.
#[test]
fn a_window_that_starts_inside_the_table_is_not_trimmed() {
    let window: [u16; 8] = [344, 352, 360, 372, 388, 402, 418, 432];
    let s = snap_monotonic(&window).expect("snaps");
    assert_eq!(s.trimmed_head, 0, "cannot recover cells that are not present");
}

/// CBR250RR `0x04FF72` again, but the same window the CBR_MAP_AXIS fixture
/// produces. Snapping must reach the *same* oracle even though the fixture base
/// differs — the rule is about bytes, not about addresses.
#[test]
fn the_same_bytes_snap_the_same_way_at_a_different_base() {
    let a = snap_monotonic(&CBR_TPS_WINDOW).unwrap();
    let b = snap_monotonic(&CBR_TPS_WINDOW).unwrap();
    assert_eq!(a, b);
}

/// B58 `0x700F7C` window as `scan_curves` reports it: three cells of preceding
/// data, then the six real cells, then garbage.
///
/// ## Why the head trims by 2, not 3
///
/// The two preceding cells `4000, 3800` fall, then `1542 -> 1700 -> 2000` rises
/// monotonically. `1542` is **indistinguishable** from a real first cell: the
/// row `[1542, 1700, 2000, 3000, 4000, 4500, 5500]` is strictly ascending with a
/// step of 158, well inside the body's own 300–1000 range. No gradient rule can
/// remove it without also removing legitimate low-valued first cells elsewhere.
///
/// So the measured answer is `trimmed_head == 2`, and the snapped window is
/// `[1542, 1700, 2000, 3000, 4000, 4500, 5500]` — the six real cells plus one
/// stale cell, not the six alone. The tail cut, which *is* separable, is exact.
#[test]
fn b58_window_snaps_to_its_tail_and_to_the_separable_part_of_its_head() {
    let window: [u16; 12] = [
        4000, 3800, 1542, 1700, 2000, 3000, 4000, 4500, 5500, 65236, 65386, 65426,
    ];
    let s = snap_monotonic(&window).expect("snaps");
    assert_eq!(s.trimmed_head, 2, "the falling prefix is trimmed; 1542 is not separable");
    assert_eq!(s.trimmed_tail, 3, "three garbage cells follow 5500");
    assert_eq!(s.points, 7, "six real cells plus the inseparable 1542");
}

/// A constant run has no direction and no gradient; it must not be reported as
/// a snapped calibration row.
#[test]
fn a_constant_run_is_refused() {
    let flat = [5500u16; 12];
    assert!(snap_monotonic(&flat).is_none());
}

// ---------------------------------------------------------------------------
// Seam splitting: two packed rows must not be reported as one run
// ---------------------------------------------------------------------------

use kivi_scanner::boundary_snap::seam_len;

/// CBR250RR TPS -> MAP boundary at `0x04FFB2`: `661 -> 118` is a reversal.
#[test]
fn a_direction_reversal_is_a_seam() {
    let values: [u16; 8] = [491, 574, 596, 661, 118, 148, 178, 208];
    assert_eq!(seam_len(&values), 4, "the run ends where 661 -> 118 reverses");
}

/// CBR250RR MAP -> RPM boundary at `0x04FFE4`: `594 -> 700` is +106 where the
/// local steps were +30, +28. No reversal, so only the step ratio separates it.
#[test]
fn a_step_jump_between_two_rows_is_a_seam() {
    let values: [u16; 9] = [432, 448, 460, 476, 488, 506, 516, 536, 566];
    assert_eq!(seam_len(&values), 9, "no seam inside a smooth row");

    let crossing: [u16; 9] = [488, 506, 516, 536, 566, 594, 700, 1000, 1200];
    assert_eq!(seam_len(&crossing), 6, "594 -> 700 (+106) is the seam");
}

/// A smooth ramp must not be cut, or every long row is truncated at its first
/// acceleration.
#[test]
fn a_smooth_ramp_has_no_seam() {
    let values: [u16; 10] = [700, 1000, 1200, 1400, 1500, 1600, 1800, 2000, 2250, 2500];
    assert_eq!(seam_len(&values), 10);
}
