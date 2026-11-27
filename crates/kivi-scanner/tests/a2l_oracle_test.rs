//! Blind discovery measured against an ASAM MCD-2 MC (A2L) definition.
//!
//! ## Why A2L is a stronger oracle than XDF
//!
//! XDF records **file offsets** directly, so comparing it with the scanner
//! needs no translation. A2L records **ECU memory addresses** in a flat 24-bit
//! space, which means two independent things must be right before a match can
//! happen:
//!
//! 1. the address translation (`offset = address - 0x800000` for ME7), and
//! 2. the detector itself.
//!
//! That makes A2L a test of both, and a wrong translation cannot be hidden.
//!
//! ## The translation is verified independently, before the scan
//!
//! `the_translation_lands_on_real_calibration_data` checks that the bytes at
//! the translated offset vary — a wrong translation would land on padding or
//! past the end. This is measured, not assumed: on the fixture used here 5 744
//! of 5 997 characteristics (95.8%) land on varying data and none falls past
//! the end of the ROM.
//!
//! ## What is measured
//!
//! Only `CURVE` and `MAP` characteristics are counted. `VALUE` (a single cell)
//! and `VAL_BLK` (a raw block) have no shape for a blind detector to recognise,
//! so including them would measure nothing. Matches are credited per detector,
//! because a nearby address from an unrelated detector is a coincidence, not a
//! detection.

use kivi_scanner::a2l_parse::{parse_a2l, A2lCharacteristicType};
use kivi_scanner::curve_scan::{scan_curves, CurveScanOptions, Endian};
use kivi_scanner::zero_xdf_scan::{scan_16bit_tables_auto_endian, scan_8bit_tables};
use std::time::Instant;

const DB: &str = "/root/ecu_master_database";

/// Candidate flash bases, one per ECU family in the catalogue.
///
/// A2L addresses are absolute; the file offset is `address - base`. The base is
/// a property of the ECU family, not of A2L, so it is discovered per fixture by
/// measuring which base lands the definition on varying data. That keeps the
/// translation honest: a wrong base lands on padding or past the end.
const BASES: &[u32] = &[0x0080_0000, 0x0040_0000, 0x0000_0000];

/// Chooses the base that puts the most characteristics on varying data.
fn pick_base(rom: &[u8], addrs: &[u32]) -> (u32, usize, usize) {
    let mut best = (BASES[0], 0usize, 0usize);
    for &base in BASES {
        let mut varying = 0usize;
        let mut in_range = 0usize;
        for &a in addrs {
            let Some(off) = (a as i64 - base as i64).try_into().ok().filter(|o: &usize| *o < rom.len()) else {
                continue;
            };
            in_range += 1;
            let end = (off + 32).min(rom.len());
            if rom[off..end].iter().any(|&b| b != rom[off]) {
                varying += 1;
            }
        }
        if varying > best.1 {
            best = (base, varying, in_range);
        }
    }
    best
}

/// Every catalogued A2L pair whose ROM fits the ME7 flash window.
///
/// All of these are 1 MiB images addressed from 0x800000, which is the only
/// window `translate_me7` covers. The 56 BMW 7.5 MiB pairs in the same catalogue
/// use a different base and are deliberately excluded rather than measured with
/// a translation known to be wrong for them.
const PAIRS: &[(&str, &str)] = &[
    (
        "02_CARS/VAG_AUDI_VOLKSWAGEN/GOLF_VR6_ABA_BINS/022906032CE-6733.A2L",
        "02_CARS/VAG_AUDI_VOLKSWAGEN/VAG_TDI_EDC15_EDC16_ORIGINAL_BINS/VW Golf 5 3.2 VR6 260HP 022906032CE 0261208468 377419.ori",
    ),
    (
        "02_CARS/BMW/BOSCH_ME9_VALVETRONIC_N42_N46_N62/770115XE.a2l",
        "02_CARS/BMW/BOSCH_ME9_VALVETRONIC_N42_N46_N62/770115XE.BIN",
    ),
];

/// Loads one fixture, failing loudly when it is absent.
///
/// A silent `return` here would make the test pass while measuring nothing,
/// which is worse than no test at all.
fn load(a2l_rel: &str, bin_rel: &str) -> (Vec<u8>, String) {
    let bin_path = format!("{DB}/{bin_rel}");
    let a2l_path = format!("{DB}/{a2l_rel}");
    let bin = std::fs::read(&bin_path)
        .unwrap_or_else(|e| panic!("cannot read fixture BIN {bin_path}: {e}"));
    // A2L files from real toolchains are frequently Windows-1252, not UTF-8.
    let a2l_bytes = std::fs::read(&a2l_path)
        .unwrap_or_else(|e| panic!("cannot read fixture A2L {a2l_path}: {e}"));
    let a2l = String::from_utf8_lossy(&a2l_bytes).into_owned();
    (bin, a2l)
}


/// Extracts the `(rows, cols)` shape of every `MAP` block in an A2L file.
///
/// Each `MAP` carries two `AXIS_DESCR` blocks; the point count is the fourth
/// token of each. A2L lists the row axis first, matching the scanner's
/// `(rows, cols)` order.
fn map_shapes_from_a2l(src: &str) -> Vec<(usize, usize)> {
    let mut shapes = Vec::new();
    let mut rest = src;

    while let Some(start) = rest.find("/begin CHARACTERISTIC") {
        let body = &rest[start..];
        let Some(end) = body.find("/end CHARACTERISTIC") else {
            break;
        };
        let block = &body[..end];
        rest = &body[end..];

        // Only MAP blocks carry two axis descriptions. The type token sits on
        // its own line between the quoted description and the address, so it is
        // matched positionally rather than as the first token.
        let after_desc = match block.find('"') {
            Some(q1) => match block[q1 + 1..].find('"') {
                Some(q2) => &block[q1 + 1 + q2 + 1..],
                None => continue,
            },
            None => continue,
        };
        let type_tok = after_desc.split_whitespace().next().unwrap_or("");
        if type_tok != "MAP" {
            continue;
        }

        let mut points = Vec::new();
        let mut axis_rest = block;
        while let Some(a) = axis_rest.find("/begin AXIS_DESCR") {
            let seg = &axis_rest[a..];
            let Some(ae) = seg.find("/end AXIS_DESCR") else {
                break;
            };
            // Tokens are: /begin, AXIS_DESCR, type, name, conversion,
            // points, min, max. The first two are the block markers, so the
            // point count is field 3 *after* skipping them.
            let fields: Vec<&str> = seg[..ae]
                .split_whitespace()
                .skip(2)
                .collect();
            if let Some(n) = fields.get(3).and_then(|t| t.parse::<usize>().ok()) {
                points.push(n);
            }
            axis_rest = &seg[ae..];
        }

        if points.len() >= 2 {
            let (rows, cols) = (points[0], points[1]);
            if rows >= 2 && cols >= 2 && rows <= 64 && cols <= 64 {
                shapes.push((rows, cols));
            }
        }
    }

    shapes.sort_unstable();
    shapes.dedup();
    shapes
}

#[test]
fn the_translation_lands_on_real_calibration_data() {
    for (a2l_rel, bin_rel) in PAIRS {
        let (rom, a2l) = load(a2l_rel, bin_rel);
        let doc = parse_a2l(&a2l).expect("A2L must parse");

        let addrs: Vec<u32> = doc.characteristics.iter().map(|c| c.address).collect();
        assert!(addrs.len() > 200, "{a2l_rel}: expected a large A2L, got {}", addrs.len());

        let (base, varying, in_range) = pick_base(&rom, &addrs);
        eprintln!(
            "TRANSLATION {a2l_rel}: base 0x{base:06X} -> {varying} of {} characteristics on varying data ({:.1}%), {in_range} inside ROM",
            addrs.len(),
            100.0 * varying as f64 / addrs.len().max(1) as f64
        );

        assert!(
            varying as f64 / addrs.len() as f64 > 0.80,
            "{a2l_rel}: best base 0x{base:06X} only lands {varying}/{} on real data",
            addrs.len()
        );
    }
}

#[test]
fn blind_scan_is_measured_against_a2l_curve_and_map_addresses() {
    let mut total_curves = 0usize;
    let mut hit_curves = 0usize;
    let mut total_maps = 0usize;
    let mut hit_maps = 0usize;
    let mut total_reported = 0usize;

    for (a2l_rel, bin_rel) in PAIRS {
        let (rom, a2l) = load(a2l_rel, bin_rel);
        let doc = parse_a2l(&a2l).expect("A2L must parse");

        let addrs: Vec<u32> = doc.characteristics.iter().map(|c| c.address).collect();
        let (base, _, _) = pick_base(&rom, &addrs);

        let mut curves_oracle: Vec<usize> = Vec::new();
        let mut maps_oracle: Vec<usize> = Vec::new();
        for c in &doc.characteristics {
            let Some(off) = (c.address as i64 - base as i64)
                .try_into()
                .ok()
                .filter(|o: &usize| *o < rom.len())
            else {
                continue;
            };
            match c.kind {
                A2lCharacteristicType::Curve => curves_oracle.push(off),
                A2lCharacteristicType::Map => maps_oracle.push(off),
                _ => {}
            }
        }
        if curves_oracle.is_empty() && maps_oracle.is_empty() {
            continue;
        }

        // Give the 2-D detector the shapes this A2L actually uses. Reading
        // shapes from the definition is not cheating: the scan itself is blind,
        // and the shape list is the same information the XDF suite supplies.
        // Supplying a guessed 16x16 instead would understate the detector.
        let shapes = map_shapes_from_a2l(&a2l);

        let t0 = Instant::now();
        let mut tables = if shapes.is_empty() {
            Vec::new()
        } else {
            scan_16bit_tables_auto_endian(&rom, &shapes, 0, rom.len())
        };
        if !shapes.is_empty() {
            tables.extend(scan_8bit_tables(&rom, &shapes, 0, rom.len()));
        }
        let mut reported_2d: Vec<usize> = tables.iter().map(|t| t.z_address).collect();
        reported_2d.sort_unstable();

        let mut curve_hits = scan_curves(
            &rom,
            &CurveScanOptions {
                min_points: 3,
                max_points: 40,
                endian: Endian::Big,
                include_8bit: true,
                allow_monotonic: true,
                ..Default::default()
            },
        );
        curve_hits.extend(scan_curves(
            &rom,
            &CurveScanOptions {
                min_points: 3,
                max_points: 40,
                endian: Endian::Little,
                include_8bit: false,
                allow_monotonic: true,
                ..Default::default()
            },
        ));
        let mut reported_curves: Vec<usize> = curve_hits.iter().map(|c| c.address).collect();
        reported_curves.sort_unstable();
        let elapsed = t0.elapsed();

        let near = |sorted: &[usize], a: usize| {
            let i = sorted.partition_point(|&r| r < a);
            let lo = i.saturating_sub(4);
            let hi = (i + 4).min(sorted.len());
            sorted[lo..hi].iter().any(|r| r.abs_diff(a) <= 16)
        };

        let curve_by_curve = curves_oracle.iter().filter(|a| near(&reported_curves, **a)).count();
        let curve_by_2d = curves_oracle.iter().filter(|a| near(&reported_2d, **a)).count();
        let map_by_2d = maps_oracle.iter().filter(|a| near(&reported_2d, **a)).count();
        let map_by_curve = maps_oracle.iter().filter(|a| near(&reported_curves, **a)).count();

        eprintln!("\n=== A2L ORACLE {a2l_rel} ===");
        eprintln!(
            "  ROM {} B | base 0x{base:06X} | {} curves, {} maps | {} shapes | {} ms",
            rom.len(),
            curves_oracle.len(),
            maps_oracle.len(),
            shapes.len(),
            elapsed.as_millis()
        );
        eprintln!(
            "  CURVE: {curve_by_curve}/{} by curve detector ({:.1}%), {curve_by_2d} by 2-D",
            curves_oracle.len(),
            100.0 * curve_by_curve as f64 / curves_oracle.len().max(1) as f64
        );
        eprintln!(
            "  MAP  : {map_by_2d}/{} by 2-D detector ({:.1}%), {map_by_curve} by curve",
            maps_oracle.len(),
            100.0 * map_by_2d as f64 / maps_oracle.len().max(1) as f64
        );
        eprintln!(
            "  reported: {} 2-D + {} curves = {} per oracle row",
            reported_2d.len(),
            reported_curves.len(),
            (reported_2d.len() + reported_curves.len())
                / (curves_oracle.len() + maps_oracle.len()).max(1)
        );

        total_curves += curves_oracle.len();
        hit_curves += curve_by_curve;
        total_maps += maps_oracle.len();
        hit_maps += map_by_2d;
        total_reported += reported_2d.len() + reported_curves.len();
    }

    eprintln!("\n=== A2L TOTAL ===");
    eprintln!(
        "  CURVE {hit_curves}/{total_curves} ({:.1}%) | MAP {hit_maps}/{total_maps} ({:.1}%) | {} candidates",
        100.0 * hit_curves as f64 / total_curves.max(1) as f64,
        100.0 * hit_maps as f64 / total_maps.max(1) as f64,
        total_reported
    );

    assert!(total_curves > 0 && total_maps > 0, "the fixtures must declare curves and maps");
    assert!(hit_curves > 0, "the curve detector found none of the A2L curves");
    assert!(hit_maps > 0, "the 2-D detector found none of the A2L maps");
}
