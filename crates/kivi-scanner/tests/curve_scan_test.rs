//! Blind detection of 1-D calibration curves.
//!
//! ## Why this exists
//!
//! In the 647-pair BIN+XDF catalogue, **68.1%** of the multi-cell tables the
//! XDFs declare are 1-D curves (`250 300` of `367 590` rows), not 2-D maps. The
//! scanner was only ever asked for 2-D shapes, so those rows could never match
//! no matter how good the 2-D detector became. Measuring them together hid that.
//!
//! ## What separates a curve from an axis
//!
//! Both are smooth runs of adjacent words. The difference is monotonicity:
//!
//! | Object | Shape of the run |
//! |---|---|
//! | Axis breakpoints | strictly monotonic, near-uniform steps |
//! | Curve values | smooth, but turns around (a bell, a ramp-and-plateau) |
//!
//! So a run is a **curve** when it is smooth and has at least one interior
//! turning point, and an **axis** when it never turns. That test is what keeps
//! [`crate::axis_detector`] and this module from claiming the same bytes.
//!
//! ## Scope
//!
//! Detection only. Axis pairing and XDF export are not in this module.

use kivi_scanner::curve_scan::{scan_curves, CurveScanOptions, Endian, DiscoveredCurve};

/// Build a ROM with a 16-bit big-endian curve written at `at`.
fn rom_with_curve_at(at: usize, values: &[u16]) -> Vec<u8> {
    let mut rom = vec![0u8; 1024];
    for (i, v) in values.iter().enumerate() {
        let b = v.to_be_bytes();
        rom[at + i * 2] = b[0];
        rom[at + i * 2 + 1] = b[1];
    }
    rom
}

/// A smooth bell: rises then falls. This is a curve, not an axis.
const BELL: [u16; 16] = [
    100, 140, 190, 250, 320, 400, 480, 560, 560, 480, 400, 320, 250, 190, 140, 100,
];

#[test]
fn finds_a_16bit_bell_curve_at_its_exact_address() {
    let rom = rom_with_curve_at(0x100, &BELL);
    let found = scan_curves(
        &rom,
        &CurveScanOptions {
            min_points: 8,
            max_points: 32,
            endian: Endian::Big,
            ..Default::default()
        },
    );

    let hit = found
        .iter()
        .find(|c| c.address == 0x100)
        .unwrap_or_else(|| panic!("no curve at 0x100; found {:?}", summary(&found)));

    assert_eq!(hit.points, 16);
    assert_eq!(hit.endian, Endian::Big);
    assert_eq!(hit.min_value, 100.0);
    assert_eq!(hit.max_value, 560.0);
}

/// A strictly rising ramp is an axis, not a curve. Claiming it here would
/// duplicate what `axis_detector` already reports.
#[test]
fn rejects_a_strictly_monotonic_ramp_because_that_is_an_axis() {
    let ramp: Vec<u16> = (0..16).map(|i| 1000 + i * 500).collect();
    let rom = rom_with_curve_at(0x100, &ramp);

    let found = scan_curves(
        &rom,
        &CurveScanOptions {
            min_points: 8,
            max_points: 32,
            endian: Endian::Big,
            ..Default::default()
        },
    );

    assert!(
        found.iter().all(|c| c.address != 0x100),
        "a monotonic ramp must not be reported as a curve: {:?}",
        summary(&found)
    );
}

/// A block that alternates between two values has no gradient; the smoothness
/// gate must reject it even though its deltas are tiny.
#[test]
fn rejects_an_alternating_two_value_block() {
    let alternating: Vec<u16> = (0..16).map(|i| if i % 2 == 0 { 100 } else { 900 }).collect();
    let rom = rom_with_curve_at(0x100, &alternating);

    let found = scan_curves(
        &rom,
        &CurveScanOptions {
            min_points: 8,
            max_points: 32,
            endian: Endian::Big,
            ..Default::default()
        },
    );

    assert!(
        found.iter().all(|c| c.address != 0x100),
        "an alternating block is not a curve: {:?}",
        summary(&found)
    );
}

/// Random noise has deltas far larger than its standard deviation, so the
/// smoothness gate rejects it.
#[test]
fn rejects_noise() {
    let mut state = 0x1234_5678u32;
    let noise: Vec<u16> = (0..16)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 16) as u16
        })
        .collect();
    let rom = rom_with_curve_at(0x100, &noise);

    let found = scan_curves(
        &rom,
        &CurveScanOptions {
            min_points: 8,
            max_points: 32,
            endian: Endian::Big,
            ..Default::default()
        },
    );

    assert!(
        found.iter().all(|c| c.address != 0x100),
        "noise is not a curve: {:?}",
        summary(&found)
    );
}

/// Little-endian cells are the common case on TriCore/Infineon parts.
#[test]
fn finds_a_little_endian_curve() {
    let mut rom = vec![0u8; 1024];
    for (i, v) in BELL.iter().enumerate() {
        let b = v.to_le_bytes();
        rom[0x200 + i * 2] = b[0];
        rom[0x200 + i * 2 + 1] = b[1];
    }

    let found = scan_curves(
        &rom,
        &CurveScanOptions {
            min_points: 8,
            max_points: 32,
            endian: Endian::Little,
            ..Default::default()
        },
    );

    let hit = found
        .iter()
        .find(|c| c.address == 0x200)
        .unwrap_or_else(|| panic!("no LE curve at 0x200; found {:?}", summary(&found)));
    assert_eq!(hit.points, 16);
    assert_eq!(hit.endian, Endian::Little);
}

/// 8-bit curves are the other half of the catalogue.
#[test]
fn finds_an_8bit_curve() {
    let bell8: [u8; 16] = [10, 30, 60, 100, 140, 180, 210, 230, 230, 210, 180, 140, 100, 60, 30, 10];
    let mut rom = vec![0u8; 1024];
    rom[0x300..0x300 + bell8.len()].copy_from_slice(&bell8);

    let found = scan_curves(
        &rom,
        &CurveScanOptions {
            min_points: 8,
            max_points: 32,
            endian: Endian::Big,
            include_8bit: true,
            ..Default::default()
        },
    );

    let hit = found
        .iter()
        .find(|c| c.address == 0x300)
        .unwrap_or_else(|| panic!("no 8-bit curve at 0x300; found {:?}", summary(&found)));
    assert_eq!(hit.points, 16);
}

/// The reported turning point count must reflect the real shape: a bell turns
/// once (up then down).
#[test]
fn reports_the_turning_point_count() {
    let rom = rom_with_curve_at(0x100, &BELL);
    let found = scan_curves(
        &rom,
        &CurveScanOptions {
            min_points: 8,
            max_points: 32,
            endian: Endian::Big,
            ..Default::default()
        },
    );

    let hit = found.iter().find(|c| c.address == 0x100).expect("curve at 0x100");
    assert_eq!(hit.turning_points, 1, "a bell turns exactly once");
}

fn summary(found: &[DiscoveredCurve]) -> Vec<(usize, usize)> {
    found.iter().map(|c| (c.address, c.points)).collect()
}

/// A **monotonic** 1-D calibration curve must be findable.
///
/// ## The measurement that forced this
///
/// Of the 1-D tables two real definitions declare:
///
/// | ROM | 1-D tables | monotonic |
/// |---|---:|---:|
/// | Hayabusa | 392 | **351 (90%)** |
/// | BMW B58 | 864 | **827 (96%)** |
///
/// `scan_curves` rejected every monotonic run as "an axis". That gate is right
/// for *breakpoint* runs, which `axis_detector` reports separately, but it also
/// threw away the majority of real 1-D calibration curves — a sensor transfer
/// function, an injector dead-time table, and a limiter ramp are all monotonic.
///
/// The fixture below is a real shape taken from a B58 ROM at `0x700F82`:
/// `1700, 2000, 3000, 4000, 4500, 5500`.
#[test]
fn a_monotonic_calibration_curve_is_found_when_asked_for() {
    let values: [u16; 6] = [1700, 2000, 3000, 4000, 4500, 5500];
    let mut rom: Vec<u8> = (0..4096).map(|i| ((i * 7) % 251) as u8).collect();
    for (i, v) in values.iter().enumerate() {
        let b = v.to_be_bytes();
        rom[0x400 + i * 2] = b[0];
        rom[0x400 + i * 2 + 1] = b[1];
    }

    let found = scan_curves(
        &rom,
        &CurveScanOptions {
            min_points: 4,
            max_points: 12,
            endian: Endian::Big,
            include_8bit: false,
            allow_monotonic: true,
                ..Default::default()
            },
    );
    let hit = found
        .iter()
        .find(|c| c.address == 0x400)
        .unwrap_or_else(|| panic!("monotonic curve at 0x400 not found; got {:?}", summary(&found)));
    // A monotonic run has no value boundary, so the reported length may extend
    // into neighbouring rising data. What matters is that the six declared cells
    // are the start of the reported run and appear in order.
    assert!(hit.points >= 6, "expected at least the 6 declared cells, got {}", hit.points);
    assert_eq!(hit.values[0], 1700.0);
    assert_eq!(&hit.values[1..6], &[2000.0, 3000.0, 4000.0, 4500.0, 5500.0]);
}

/// The default must stay monotonic-rejecting, so axis detection is not
/// duplicated by callers that do not opt in.
#[test]
fn monotonic_curves_stay_rejected_by_default() {
    let values: [u16; 6] = [1700, 2000, 3000, 4000, 4500, 5500];
    let mut rom: Vec<u8> = (0..4096).map(|i| ((i * 7) % 251) as u8).collect();
    for (i, v) in values.iter().enumerate() {
        let b = v.to_be_bytes();
        rom[0x400 + i * 2] = b[0];
        rom[0x400 + i * 2 + 1] = b[1];
    }
    let found = scan_curves(
        &rom,
        &CurveScanOptions {
            min_points: 4,
            max_points: 12,
            endian: Endian::Big,
            include_8bit: false,
            allow_monotonic: false,
                ..Default::default()
            },
    );
    assert!(
        found.iter().all(|c| c.address != 0x400),
        "default behaviour must not report monotonic runs as curves"
    );
}
