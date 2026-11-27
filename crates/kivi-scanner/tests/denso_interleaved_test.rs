//! Denso / Toyota interleaved map detection.
//!
//! ## Why this file exists
//!
//! `DensoMapScanner::scan_interleaved_maps` had **no test and no caller** — a
//! dead port. The format is real and specific: each row stores its Y-axis
//! breakpoint word **inside** the row, before the data:
//!
//! ```text
//! [Y_0] [Z_00 Z_01 ... Z_0(W-1)]
//! [Y_1] [Z_10 Z_11 ... Z_1(W-1)]
//! ```
//!
//! A packed-table scanner cannot find these, because the data words are not
//! contiguous: every row is interrupted by its own breakpoint. That is why
//! Denso needs its own detector.
//!
//! ## Fixture design
//!
//! A 4-row x 4-column map at 0x100 with a rising Y axis (1000, 2000, 3000,
//! 4000) and a data matrix whose cells are all distinct. A packed reader would
//! pick up the breakpoints as data cells, so a correct detection is
//! unambiguous.

use kivi_scanner::denso::DensoMapScanner;

/// Builds a ROM containing one interleaved map at `at`.
fn interleaved_rom(at: usize, y_axis: &[u16], rows: &[Vec<u16>]) -> Vec<u8> {
    let cols = rows[0].len();
    let mut rom = vec![0u8; 4096];
    for (r, y) in y_axis.iter().enumerate() {
        let base = at + r * (cols + 1) * 2;
        rom[base..base + 2].copy_from_slice(&y.to_le_bytes());
        for (c, v) in rows[r].iter().enumerate() {
            let off = base + 2 + c * 2;
            rom[off..off + 2].copy_from_slice(&v.to_le_bytes());
        }
    }
    rom
}

fn sample() -> (Vec<u16>, Vec<Vec<u16>>) {
    let y = vec![1000, 2000, 3000, 4000];
    let rows = vec![
        vec![100, 110, 120, 130],
        vec![200, 210, 220, 230],
        vec![300, 310, 320, 330],
        vec![400, 410, 420, 430],
    ];
    (y, rows)
}

#[test]
fn an_interleaved_map_is_found_at_its_address() {
    let (y, rows) = sample();
    let rom = interleaved_rom(0x100, &y, &rows);

    let found = DensoMapScanner::scan_interleaved_maps(&rom, &[4], 4, 4);
    let hit = found
        .iter()
        .find(|m| m.start_address == 0x100)
        .unwrap_or_else(|| {
            let seen: Vec<usize> = found.iter().map(|m| m.start_address).collect();
            panic!("no interleaved map at 0x100; found {seen:?}")
        });

    assert_eq!(hit.rows, 4);
    assert_eq!(hit.cols, 4);
    assert!(hit.is_16bit);
}

#[test]
fn the_y_axis_is_read_from_the_row_headers() {
    let (y, rows) = sample();
    let rom = interleaved_rom(0x100, &y, &rows);

    let found = DensoMapScanner::scan_interleaved_maps(&rom, &[4], 4, 4);
    let hit = found.iter().find(|m| m.start_address == 0x100).expect("map");
    assert_eq!(hit.y_axis_values, vec![1000.0, 2000.0, 3000.0, 4000.0]);
}

#[test]
fn the_data_matrix_excludes_the_breakpoint_words() {
    let (y, rows) = sample();
    let rom = interleaved_rom(0x100, &y, &rows);

    let found = DensoMapScanner::scan_interleaved_maps(&rom, &[4], 4, 4);
    let hit = found.iter().find(|m| m.start_address == 0x100).expect("map");

    assert_eq!(
        hit.matrix,
        vec![
            vec![100.0, 110.0, 120.0, 130.0],
            vec![200.0, 210.0, 220.0, 230.0],
            vec![300.0, 310.0, 320.0, 330.0],
            vec![400.0, 410.0, 420.0, 430.0],
        ],
        "breakpoint words must never appear as data cells"
    );
}

#[test]
fn a_non_monotonic_y_axis_is_rejected() {
    // Breakpoints that do not rise cannot be a Denso Y axis.
    let y = vec![4000, 3000, 2000, 1000];
    let rows = vec![
        vec![100u16, 110, 120, 130],
        vec![200, 210, 220, 230],
        vec![300, 310, 320, 330],
        vec![400, 410, 420, 430],
    ];
    let rom = interleaved_rom(0x100, &y, &rows);

    let found = DensoMapScanner::scan_interleaved_maps(&rom, &[4], 4, 4);
    assert!(
        found.iter().all(|m| m.start_address != 0x100),
        "a descending breakpoint column is not a Denso interleaved map"
    );
}

#[test]
fn an_implausible_axis_range_is_rejected() {
    // 60000..61000 is neither a plausible RPM axis nor a load axis.
    let y = vec![60000, 60300, 60600, 61000];
    let rows = vec![
        vec![100u16, 110, 120, 130],
        vec![200, 210, 220, 230],
        vec![300, 310, 320, 330],
        vec![400, 410, 420, 430],
    ];
    let rom = interleaved_rom(0x100, &y, &rows);

    let found = DensoMapScanner::scan_interleaved_maps(&rom, &[4], 4, 4);
    assert!(
        found.iter().all(|m| m.start_address != 0x100),
        "an out-of-range axis must be rejected"
    );
}

#[test]
fn detection_is_deterministic() {
    let (y, rows) = sample();
    let rom = interleaved_rom(0x100, &y, &rows);
    let a = DensoMapScanner::scan_interleaved_maps(&rom, &[4], 4, 4);
    let b = DensoMapScanner::scan_interleaved_maps(&rom, &[4], 4, 4);
    assert_eq!(
        a.iter().map(|m| m.start_address).collect::<Vec<_>>(),
        b.iter().map(|m| m.start_address).collect::<Vec<_>>()
    );
}

/// A detected map must contain smooth data, not noise.
///
/// ## The bug this pins
///
/// The original detector checked only that the per-row breakpoint words rose.
/// On a real 256 KiB Denso ROM that returned **1 105** "maps", and inspection of
/// the first two showed garbage:
///
/// ```text
/// @0x3820  4x4  y = [1272, 3832, 14840, 16561]
///   [1784, 2296, 2808, 3320]
///   [4344, 4856, 5368, 5880]
///   [65280, 65535, 898, 0]        <- 65535 next to 898
///   [65535, 1410, 65535, 32897]
/// ```
///
/// The axis rose, so every gate passed, but the "table" is unrelated bytes. A
/// calibration surface changes smoothly; a random window does not.
#[test]
fn noise_that_happens_to_have_a_rising_header_is_rejected() {
    // Row headers rise plausibly, but the data is unrelated noise.
    let y = vec![1000u16, 2000, 3000, 4000];
    let rows = vec![
        vec![1784u16, 2296, 2808, 3320],
        vec![4344, 4856, 5368, 5880],
        vec![65280, 65535, 898, 0],
        vec![65535, 1410, 65535, 32897],
    ];
    let rom = interleaved_rom(0x100, &y, &rows);

    let found = DensoMapScanner::scan_interleaved_maps(&rom, &[4], 4, 4);
    assert!(
        found.iter().all(|m| m.start_address != 0x100),
        "a rising header around noise is not a calibration map"
    );
}

/// A real calibration surface must still be accepted after the smoothness gate.
#[test]
fn a_smooth_surface_survives_the_smoothness_gate() {
    let y = vec![1000u16, 2000, 3000, 4000];
    let rows = vec![
        vec![100u16, 110, 120, 130],
        vec![200, 210, 220, 230],
        vec![300, 310, 320, 330],
        vec![400, 410, 420, 430],
    ];
    let rom = interleaved_rom(0x100, &y, &rows);
    let found = DensoMapScanner::scan_interleaved_maps(&rom, &[4], 4, 4);
    assert!(
        found.iter().any(|m| m.start_address == 0x100),
        "a smooth, monotonic surface must still be detected"
    );
}

/// Overlapping detections must be suppressed so one map is reported once.
///
/// ## The bug this pins
///
/// The scanner steps in 16-byte increments and tries 16 shapes, so a single real
/// map is found at many overlapping offsets. On a real 256 KiB Denso ROM that
/// produced **922 detections across only 200 unique start addresses**, with 370
/// adjacent overlapping pairs.
///
/// The fix keeps the longest shape at each address and drops detections that
/// overlap a longer accepted one.
#[test]
fn overlapping_detections_are_suppressed() {
    // A 10-row map will also be detected as 4..9-row prefixes at nearby
    // offsets; only the longest should survive at that address.
    let y: Vec<u16> = (0..10).map(|i| 1000 + i as u16 * 500).collect();
    let rows: Vec<Vec<u16>> = (0..10)
        .map(|r| (0..4).map(|c| (100 + r * 10 + c) as u16).collect())
        .collect();
    let rom = interleaved_rom(0x100, &y, &rows);

    let found = DensoMapScanner::scan_interleaved_maps(&rom, &[4], 4, 12);

    // No two accepted maps may overlap in bytes.
    let mut spans: Vec<(usize, usize)> = found
        .iter()
        .map(|m| (m.start_address, m.start_address + m.rows * (m.cols + 1) * 2))
        .collect();
    spans.sort_unstable();
    for w in spans.windows(2) {
        assert!(
            w[1].0 >= w[0].1,
            "accepted maps overlap: {:?} and {:?}",
            w[0],
            w[1]
        );
    }
}

/// The reported map at an address must be the longest shape found there.
#[test]
fn the_longest_shape_wins_at_a_given_address() {
    let y: Vec<u16> = (0..10).map(|i| 1000 + i as u16 * 500).collect();
    let rows: Vec<Vec<u16>> = (0..10)
        .map(|r| (0..4).map(|c| (100 + r * 10 + c) as u16).collect())
        .collect();
    let rom = interleaved_rom(0x100, &y, &rows);

    let found = DensoMapScanner::scan_interleaved_maps(&rom, &[4], 4, 12);
    let at = found
        .iter()
        .find(|m| m.start_address == 0x100)
        .expect("map at 0x100");
    assert_eq!(
        at.rows, 10,
        "the 10-row map must be reported, not one of its 4..9-row prefixes"
    );
}
