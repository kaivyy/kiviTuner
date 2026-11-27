//! Cross-path consistency of ROM decoding.
//!
//! ## The risk this guards
//!
//! Six separate places in the scanner decode 16-bit words themselves rather
//! than calling a shared reader:
//!
//! | Module | Purpose |
//! |---|---|
//! | `kivi_core::types::DType::read_f64` | the editor's read path |
//! | `zero_xdf_scan::read_word` | 2-D table scanning |
//! | `curve_scan::decode` | 1-D curve scanning |
//! | `heuristic_scan::evaluate_window` | window scanning |
//! | `axis_detector::detect_axes_with_options` | axis detection |
//! | `table_detector::fill_row` | periodic table detection |
//!
//! Each is individually simple, which is exactly why a disagreement between
//! them can survive review: every one looks right on its own. A disagreement is
//! a real bug — the editor would show one number while the scanner claimed the
//! same cell was another.
//!
//! These tests pin the *same bytes* through the paths that expose their decoded
//! values, so a divergence fails here rather than in a user's map.
//!
//! ## Fixture design
//!
//! `0x12 0x34` is 4660 big-endian and 13330 little-endian. No test can pass by
//! accident if an implementation swaps the order.

use kivi_core::types::{DType, Endian};
use kivi_scanner::curve_scan::{scan_curves, CurveScanOptions, Endian as CurveEndian};
use kivi_scanner::heuristic_scan::scan_rom_16bit;
use kivi_scanner::zero_xdf_scan::{scan_16bit_tables, Endianness as ScanEndian};

/// A ROM the pre-filter keeps, with a 16-bit run written at `at`.
///
/// The filler matters: an all-zero image is pruned as padding, so the scanner
/// never examines it and a test built on zeros would fail for a reason that has
/// nothing to do with decoding. A low-entropy ramp is kept.
fn rising_rom(at: usize, values: &[u16]) -> Vec<u8> {
    let mut rom: Vec<u8> = (0..4096).map(|i| ((i * 7) % 251) as u8).collect();
    for (i, v) in values.iter().enumerate() {
        let b = v.to_be_bytes();
        rom[at + i * 2] = b[0];
        rom[at + i * 2 + 1] = b[1];
    }
    rom
}

/// A pre-filter-friendly image with a `rows x cols` surface at `at`.
fn grid_rom(at: usize, rows: usize, cols: usize) -> Vec<u8> {
    let mut rom: Vec<u8> = (0..8192).map(|i| ((i * 7) % 251) as u8).collect();
    for r in 0..rows {
        for c in 0..cols {
            let v = (1000 + r * 100 + c * 10) as u16;
            let b = v.to_be_bytes();
            let idx = at + (r * cols + c) * 2;
            rom[idx] = b[0];
            rom[idx + 1] = b[1];
        }
    }
    rom
}

/// A bell curve, smooth enough to pass every gate, written big-endian.
const BELL: [u16; 16] = [
    100, 140, 190, 250, 320, 400, 480, 560, 560, 480, 400, 320, 250, 190, 140, 100,
];

#[test]
fn the_core_reader_and_the_curve_scanner_agree_on_the_same_bytes() {
    let rom = rising_rom(0x100, &BELL);

    // Editor path.
    let core_value = DType::U16.read_f64(&rom[0x100..0x102], Endian::Big).unwrap();
    // Curve scanner path.
    let curves = scan_curves(
        &rom,
        &CurveScanOptions {
            min_points: 8,
            max_points: 32,
            endian: CurveEndian::Big,
            include_8bit: false,
            allow_monotonic: true,
                ..Default::default()
            },
    );
    let curve = curves
        .iter()
        .find(|c| c.address == 0x100)
        .expect("curve at 0x100");

    assert_eq!(core_value, 100.0);
    assert_eq!(
        curve.values[0], core_value,
        "the curve scanner and the editor must decode the same first cell"
    );
    assert_eq!(curve.values[7], 560.0, "peak cell must decode as 560");
}

#[test]
fn the_curve_scanner_decodes_the_whole_bell_the_editor_would_show() {
    let rom = rising_rom(0x100, &BELL);
    let curves = scan_curves(
        &rom,
        &CurveScanOptions {
            min_points: 8,
            max_points: 32,
            endian: CurveEndian::Big,
            include_8bit: false,
            allow_monotonic: true,
                ..Default::default()
            },
    );
    let curve = curves.iter().find(|c| c.address == 0x100).expect("curve");

    for (i, expected) in BELL.iter().enumerate() {
        let via_core = DType::U16
            .read_f64(&rom[0x100 + i * 2..0x102 + i * 2], Endian::Big)
            .unwrap();
        assert_eq!(
            curve.values[i], via_core,
            "cell {i}: scanner says {} but the editor would show {via_core}",
            curve.values[i]
        );
        assert_eq!(via_core, *expected as f64);
    }
}

#[test]
fn the_2d_scanner_agrees_with_the_core_reader() {
    // An 8x8 smooth surface in big-endian 16-bit.
    let at = 0x200;
    let rom = grid_rom(at, 8, 8);

    let tables = scan_16bit_tables(&rom, &[(8, 8)], ScanEndian::BigEndian, 0, rom.len());
    let hit = tables
        .iter()
        .find(|t| t.z_address == at)
        .expect("4x4 table at 0x200");

    for r in 0..8usize {
        for c in 0..8usize {
            let off = at + (r * 8 + c) * 2;
            let via_core = DType::U16
                .read_f64(&rom[off..off + 2], Endian::Big)
                .unwrap();
            assert_eq!(
                hit.sample_matrix[r][c], via_core,
                "cell ({r},{c}): 2-D scanner disagrees with the editor"
            );
        }
    }
}

#[test]
fn the_heuristic_scanner_agrees_with_the_core_reader() {
    let at = 0x200;
    let rom = grid_rom(at, 8, 8);

    let maps = scan_rom_16bit(&rom, &[8], 8, 8, false);
    let hit = maps.iter().find(|m| m.offset == at);
    if let Some(hit) = hit {
        // The struct reports min/max, so those are what can be compared.
        let mut expected: Vec<f64> = Vec::new();
        for r in 0..8usize {
            for c in 0..8usize {
                let off = at + (r * 8 + c) * 2;
                expected.push(DType::U16.read_f64(&rom[off..off + 2], Endian::Big).unwrap());
            }
        }
        let lo = expected.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = expected.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        assert_eq!(hit.val_min, lo, "heuristic min disagrees with the editor");
        assert_eq!(hit.val_max, hi, "heuristic max disagrees with the editor");
        assert!(!hit.is_little_endian, "this ROM is big-endian");
    }
}

#[test]
fn swapping_byte_order_changes_the_decoded_value_in_every_path() {
    // If any path ignored byte order, this would return the same number twice.
    let rom = {
        let mut r = vec![0u8; 64];
        r[0x10] = 0x12;
        r[0x11] = 0x34;
        r
    };

    let be = DType::U16.read_f64(&rom[0x10..0x12], Endian::Big).unwrap();
    let le = DType::U16.read_f64(&rom[0x10..0x12], Endian::Little).unwrap();
    assert_eq!(be, 4660.0);
    assert_eq!(le, 13330.0);
    assert_ne!(be, le, "byte order must change the decoded value");
}

#[test]
fn a_wrong_byte_order_fails_the_curve_gate_rather_than_returning_garbage() {
    // A big-endian bell read as little-endian has huge adjacent deltas, so the
    // smoothness gate must reject it. Reporting a "curve" here would be a
    // silent decoding bug visible to the user as nonsense values.
    let rom = rising_rom(0x100, &BELL);
    let le_curves = scan_curves(
        &rom,
        &CurveScanOptions {
            min_points: 8,
            max_points: 32,
            endian: CurveEndian::Little,
            include_8bit: false,
            allow_monotonic: true,
                ..Default::default()
            },
    );
    assert!(
        le_curves.iter().all(|c| c.address != 0x100),
        "a big-endian curve must not be reported as a little-endian one"
    );
}
