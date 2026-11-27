//! Blind auto-mapping must produce a short, usable list — not every fragment.
//!
//! ## The problem this pins
//!
//! Before this module, the scan paths returned raw candidates with no overlap
//! suppression. On a 1 MiB Honda CBR250RR ROM that was **10 563 tables with 0.4%
//! precision**: unusable for tuning without a definition file, because the tuner
//! would have to sift thousands of fragments to find one real map.
//!
//! Reference map packs document **78–108 maps per 1 MiB ROM**. That is the target range.
//!
//! ## What is asserted
//!
//! 1. The result is small enough to browse (same order as reference map packs).
//! 2. Every table has a real grid, axes attached where they exist, and a
//!    scaling proposal that admits when it is guessing.
//! 3. No two returned tables overlap, which is what makes the list usable.
//! 4. Scanning is deterministic and does not touch the ROM.

use kivi_scanner::blind_automap::{blind_automap, BlindOptions};

const CBR: &str = "/root/ecu_master_database/01_MOTORCYCLES/HONDA/SPORT_CBR250RR/CBR250RR-38770-K64-ND1_BIN.bin";
const GROM: &str = "/root/ecu_master_database/01_MOTORCYCLES/HONDA/MINI_MOTO_GROM_MONKEY/38770-K0F-A01.bin";

fn load(path: &str) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("fixture {path}: {e}"))
}

#[test]
fn a_one_megabyte_rom_yields_a_browsable_list() {
    let rom = load(CBR);
    let tables = blind_automap(&rom, &BlindOptions::default());

    assert!(!tables.is_empty(), "no tables found at all");
    // Reference range: 78-108 per MiB. Allow a wide band but not the old 10 563.
    assert!(
        tables.len() <= 400,
        "expected a browsable list in the target order of magnitude, got {}",
        tables.len()
    );
    eprintln!("CBR250RR: {} tables", tables.len());
}

#[test]
fn returned_tables_never_overlap() {
    let rom = load(CBR);
    let tables = blind_automap(&rom, &BlindOptions::default());

    let mut spans: Vec<(usize, usize)> = tables
        .iter()
        .map(|t| {
            let bytes = (t.cell_bits / 8) as usize;
            (t.address, t.address + t.rows * t.cols * bytes)
        })
        .collect();
    spans.sort_unstable();
    for w in spans.windows(2) {
        assert!(
            w[1].0 >= w[0].1,
            "tables overlap: {:?} and {:?}",
            w[0],
            w[1]
        );
    }
}

#[test]
fn every_table_has_a_real_grid_and_honest_metadata() {
    let rom = load(CBR);
    let tables = blind_automap(&rom, &BlindOptions::default());

    for t in &tables {
        assert!(t.rows >= 1 && t.cols >= 1);
        assert_eq!(t.matrix.len(), t.rows, "matrix row count must match");
        assert!(
            t.matrix.iter().all(|r| r.len() == t.cols),
            "every row must have {} cells",
            t.cols
        );
        assert!(t.raw_min <= t.raw_max);
        assert!(
            !t.suggested_scaling.rationale.is_empty(),
            "a scaling proposal must say why it was made"
        );
        assert!(
            !t.label.is_empty(),
            "a table needs a label to be selectable"
        );
        // Cell values must equal the matrix they came from.
        let flat_min = t.matrix.iter().flatten().copied().fold(f64::INFINITY, f64::min);
        assert_eq!(flat_min, t.raw_min);
    }
}

#[test]
fn axes_are_only_attached_when_they_sit_next_to_the_table() {
    let rom = load(CBR);
    let tables = blind_automap(&rom, &BlindOptions::default());
    let with_axis = tables.iter().filter(|t| t.x_axis.is_some() || t.y_axis.is_some()).count();
    eprintln!("{} of {} tables have an adjacent axis", with_axis, tables.len());

    for t in &tables {
        if let Some(x) = &t.x_axis {
            assert_eq!(x.points, t.cols, "x axis must have one point per column");
            assert_eq!(x.values.len(), t.cols);
        }
        if let Some(y) = &t.y_axis {
            assert_eq!(y.points, t.rows, "y axis must have one point per row");
        }
    }
}

#[test]
fn a_small_rom_also_produces_a_usable_list() {
    let rom = load(GROM);
    let tables = blind_automap(&rom, &BlindOptions::default());
    assert!(!tables.is_empty(), "no tables found in the 96 KiB ROM");
    assert!(
        tables.len() <= 60,
        "small ROM should not produce a huge list, got {}",
        tables.len()
    );
    eprintln!("Grom 96KB: {} tables", tables.len());
}

#[test]
fn scanning_is_deterministic_and_read_only() {
    let rom = load(GROM);
    let before = rom.clone();
    let a = blind_automap(&rom, &BlindOptions::default());
    let b = blind_automap(&rom, &BlindOptions::default());
    assert_eq!(a, b, "two runs must agree");
    assert_eq!(rom, before, "the scanner must not mutate the ROM");
}

#[test]
fn an_empty_or_tiny_rom_is_handled_without_panicking() {
    assert!(blind_automap(&[], &BlindOptions::default()).is_empty());
    assert!(blind_automap(&[0u8; 8], &BlindOptions::default()).is_empty());
}

#[test]
fn a_higher_overlap_threshold_keeps_more_tables() {
    let rom = load(GROM);
    let strict = blind_automap(&rom, &BlindOptions { overlap_threshold: 0.1, include_curves: true, ..Default::default() });
    let loose = blind_automap(&rom, &BlindOptions { overlap_threshold: 0.9, include_curves: true, ..Default::default() });
    assert!(
        loose.len() >= strict.len(),
        "a looser overlap threshold must not suppress more: {} vs {}",
        loose.len(),
        strict.len()
    );
}

/// Axis attachment must stay conservative.
///
/// ## Why the limit is where it is
///
/// Measured against real definitions: of 1 128 declared axis/table pairs, only
/// **37%** sit within 32 bytes of each other; the median gap is 3 096 bytes and
/// the 90th percentile is 71 KB. Definitions commonly place an axis in a
/// separate block and reference it indirectly.
///
/// So a blind scanner can only attach an axis when it happens to be adjacent. A
/// wider tolerance would pair unrelated runs with a table and produce
/// plausible-looking but wrong breakpoints — worse than showing no axis at all.
///
/// This test pins that the module does not over-claim: it may attach axes to a
/// minority of tables, and every attached axis must match the table dimension.
#[test]
fn axis_attachment_is_conservative_and_consistent() {
    let rom = load(CBR);
    let tables = blind_automap(&rom, &BlindOptions::default());
    let with_axis = tables
        .iter()
        .filter(|t| t.x_axis.is_some() || t.y_axis.is_some())
        .count();

    eprintln!(
        "{} of {} tables carry an adjacent axis",
        with_axis,
        tables.len()
    );

    for t in &tables {
        if let Some(x) = &t.x_axis {
            assert_eq!(x.points, t.cols, "an x axis must have one point per column");
            assert!(
                x.values.windows(2).all(|w| w[0] <= w[1]),
                "attached breakpoints must be ordered"
            );
        }
        if let Some(y) = &t.y_axis {
            assert_eq!(y.points, t.rows, "a y axis must have one point per row");
            assert!(y.values.windows(2).all(|w| w[0] <= w[1]));
        }
    }

    // Not a quality bar: a blind scan cannot do better than adjacency. It exists
    // to catch a regression that starts pairing distant runs.
    assert!(
        with_axis * 2 < tables.len(),
        "axis attachment looks over-eager: {with_axis} of {} tables",
        tables.len()
    );
}

/// Scaling proposals must be marked as guesses, never presented as fact.
#[test]
fn scaling_proposals_always_carry_their_reasoning() {
    let rom = load(CBR);
    let tables = blind_automap(&rom, &BlindOptions::default());
    assert!(!tables.is_empty());

    for t in &tables {
        let s = &t.suggested_scaling;
        assert!(
            !s.rationale.is_empty(),
            "table at 0x{:X} proposed '{}' with no rationale",
            t.address,
            s.equation
        );
        // When no unit could be inferred the equation must be the identity.
        if s.unit.is_empty() {
            assert_eq!(s.equation, "X", "no unit inferred, so no conversion either");
        }
    }

    let with_unit = tables.iter().filter(|t| !t.suggested_scaling.unit.is_empty()).count();
    eprintln!("{} of {} tables carry a proposed unit", with_unit, tables.len());
}

/// Blind auto-mapping cannot yet localise a table to its exact start offset.
///
/// ## What was measured
///
/// On the CBR250RR ROM, the XDF declares a 35x25 table at `0x050700`. A direct
/// check of that exact window passes every gate (precheck true, autocorrelation
/// 0.975, smoothness 0.216, degeneracy Healthy, 631 distinct values in 875
/// cells). Yet `blind_automap` does not report it.
///
/// The cause is not the gate. A raw scan with only the 35x25 shape returns
/// **4 645** tables, of which **71 overlap `0x050700`** at offsets stepped by 16
/// bytes (`0x5003A`, `0x5004A`, `0x5005A`, ...). Because the surface is very
/// smooth, a window starting slightly earlier scores just as well, wins the
/// ordering, and overlap suppression then drops the window that happens to start
/// at the declared address.
///
/// So the detector sees the data but cannot say where the table *begins*. That is
/// the single biggest gap between this scanner and a definition file, and it is
/// why blind recall against XDF is 0.1-17% even when the shape list is correct.
///
/// This test records the current behaviour so a future fix is measurable. It
/// asserts the *diagnosis*, not the defect: if localisation is ever fixed, the
/// "many overlapping windows" premise disappears and this test should be
/// rewritten to assert exact-address recall instead.
#[test]
fn overlapping_windows_around_one_table_are_why_localisation_fails() {
    use kivi_scanner::zero_xdf_scan::scan_16bit_tables_auto_endian;

    let rom = load(CBR);
    let raw = scan_16bit_tables_auto_endian(&rom, &[(35, 25)], 0, rom.len());

    let target = 0x050700usize;
    let covering = raw
        .iter()
        .filter(|t| {
            let end = t.z_address + t.rows * t.cols * 2;
            t.z_address <= target && target < end
        })
        .count();

    eprintln!(
        "raw 35x25 windows: {}, overlapping 0x050700: {}, exactly at it: {}",
        raw.len(),
        covering,
        raw.iter().filter(|t| t.z_address == target).count()
    );

    assert!(
        covering > 10,
        "expected many overlapping windows around one real table, got {covering}"
    );
    assert!(
        raw.iter().all(|t| t.z_address != target),
        "if the exact address is now found, localisation was fixed: update this test"
    );
}
