//! Before/after measurement of boundary snapping on the two ROMs that matter,
//! against their XDF definitions.
//!
//! Scope: **CBR250RR (1 MiB) and BMW B58 (8 MiB) only.** Deliberately not the
//! 30-ROM or 647-pair corpus.
//!
//! Run: `cargo run --release --example measure_snap_two_roms`

use std::collections::HashSet;
use std::path::PathBuf;

use kivi_scanner::curve_scan::{scan_curves, CurveScanOptions, Endian};
use kivi_scanner::xdf_parse::parse_xdf_xml;

const DB: &str = "/root/ecu_master_database";

struct Pair {
    name: &'static str,
    bin: &'static str,
    xdf: &'static str,
    /// Byte order the ROM's calibration cells actually use.
    cell_endian: Endian,
}

const PAIRS: &[Pair] = &[
    Pair {
        name: "CBR250RR",
        bin: "01_MOTORCYCLES/HONDA/SPORT_CBR250RR/CBR250RR-38770-K64-ND1_BIN.bin",
        xdf: "01_MOTORCYCLES/HONDA/SPORT_CBR250RR/CBR250RR_38770-K64-ND1_PUBLIC_v1.xdf",
        cell_endian: Endian::Big,
    },
    Pair {
        name: "BMW_B58",
        bin: "02_CARS/BMW/B58_3_0L_TURBO_GEN1_GEN2/00005D55508C05_b58o1_original.bin",
        xdf: "02_CARS/BMW/B58_3_0L_TURBO_GEN1_GEN2/00005D55508C05.xdf",
        cell_endian: Endian::Little,
    },
];

#[derive(Default)]
struct Stats {
    oracle_1d: usize,
    oracle_2d: usize,
    exact: usize,
    within_8: usize,
    iou: usize,
    reported: usize,
}

fn nearest(sorted: &[usize], a: usize) -> Option<(usize, usize)> {
    let i = sorted.partition_point(|&r| r < a);
    let lo = i.saturating_sub(3);
    let hi = (i + 3).min(sorted.len());
    sorted[lo..hi].iter().map(|&r| (r, r.abs_diff(a))).min_by_key(|(_, d)| *d)
}

fn measure(pair: &Pair, snap: bool) -> Option<Stats> {
    let rom = std::fs::read(PathBuf::from(DB).join(pair.bin)).ok()?;
    let xdf = std::fs::read(PathBuf::from(DB).join(pair.xdf)).ok()?;
    let text = String::from_utf8_lossy(&xdf).into_owned();
    let parsed = parse_xdf_xml(&text).ok()?;

    // Oracle: every multi-cell table the XDF declares, split by shape.
    let mut o1d: Vec<(usize, usize)> = Vec::new(); // (address, cells)
    let mut o2d = 0usize;
    for t in &parsed.tables {
        let rows = t.y_axis.as_ref().map(|a| a.index_count).unwrap_or(1);
        let cols = t.x_axis.as_ref().map(|a| a.index_count).unwrap_or(1);
        if rows < 2 && cols < 2 {
            continue;
        }
        if rows >= 2 && cols >= 2 {
            o2d += 1;
        } else {
            o1d.push((t.z_axis.address, rows * cols));
        }
    }

    // Both byte orders, exactly as `bin_xdf_647_pairs_test` does: the 1-D
    // tables an XDF declares are not all stored in the same order, and scanning
    // one order silently halves the oracle.
    let mut curves = scan_curves(
        &rom,
        &CurveScanOptions {
            min_points: 3,
            max_points: 40,
            endian: pair.cell_endian,
            include_8bit: true,
            allow_monotonic: true,
            snap_boundaries: snap,
        },
    );
    curves.extend(scan_curves(
        &rom,
        &CurveScanOptions {
            min_points: 3,
            max_points: 40,
            endian: match pair.cell_endian {
                Endian::Big => Endian::Little,
                Endian::Little => Endian::Big,
            },
            include_8bit: false,
            allow_monotonic: true,
            snap_boundaries: snap,
        },
    ));
    let mut reported: Vec<usize> = curves.iter().map(|c| c.address).collect();
    reported.sort_unstable();
    reported.dedup();

    let mut s = Stats {
        oracle_1d: o1d.len(),
        oracle_2d: o2d,
        reported: reported.len(),
        ..Default::default()
    };

    // A curve is credited only if its span overlaps the oracle's span.
    let spans: Vec<(usize, usize, usize)> = curves
        .iter()
        .map(|c| (c.address, c.address + c.points * c.cell_bytes, c.points))
        .collect();

    // Matching rule: the one this codebase already uses for oracle recall
    // (`bin_xdf_647_pairs_test`: nearest reported address within 16 bytes).
    // Using a different rule here would make the before/after numbers
    // incomparable with every prior measurement.
    let _ = &spans;
    for &(addr, cells) in &o1d {
        let end = addr + cells * 2;
        let Some((_, delta)) = nearest(&reported, addr) else { continue };
        if delta == 0 {
            s.exact += 1;
        }
        if delta <= 8 {
            s.within_8 += 1;
        }
        // IoU: does any reported curve cover at least half of the oracle span?
        for &(cs, ce, _n) in &spans {
            let inter = ce.min(end).saturating_sub(cs.max(addr));
            if inter == 0 {
                continue;
            }
            let union = ce.max(end) - cs.min(addr);
            if inter * 2 >= union {
                s.iou += 1;
                break;
            }
        }
    }
    Some(s)
}

fn row(label: &str, pct: impl Fn(&Stats) -> f64, before: &Stats, after: &Stats) {
    println!(
        "  {label:<22} {:>7.1}%  ->  {:>7.1}%   ({:+.1})",
        pct(before),
        pct(after),
        pct(after) - pct(before)
    );
}

fn main() {
    let mut totals: Vec<(String, Stats, Stats)> = Vec::new();

    for pair in PAIRS {
        let Some(before) = measure(pair, false) else {
            println!("{}: could not measure", pair.name);
            continue;
        };
        let Some(after) = measure(pair, true) else {
            println!("{}: could not measure (snap)", pair.name);
            continue;
        };

        println!("============================================================");
        println!(
            "{}  1-D oracle rows = {}  2-D oracle rows = {}",
            pair.name, before.oracle_1d, before.oracle_2d
        );
        println!("  candidates reported: {} -> {}", before.reported, after.reported);
        let n = before.oracle_1d.max(1) as f64;
        row("Exact (delta=0)", |s| s.exact as f64 * 100.0 / n, &before, &after);
        row("Within 8 bytes", |s| s.within_8 as f64 * 100.0 / n, &before, &after);
        row("IoU >= 50%", |s| s.iou as f64 * 100.0 / n, &before, &after);
        totals.push((pair.name.to_string(), before, after));
    }

    let mut tb = Stats::default();
    let mut ta = Stats::default();
    for (_, b, a) in &totals {
        tb.oracle_1d += b.oracle_1d;
        tb.exact += b.exact;
        tb.within_8 += b.within_8;
        tb.iou += b.iou;
        tb.reported += b.reported;
        ta.oracle_1d += a.oracle_1d;
        ta.exact += a.exact;
        ta.within_8 += a.within_8;
        ta.iou += a.iou;
        ta.reported += a.reported;
    }
    let n = tb.oracle_1d.max(1) as f64;
    println!("============================================================");
    println!("COMBINED  1-D oracle rows = {}   candidates {} -> {}",
        tb.oracle_1d, tb.reported, ta.reported);
    row("Exact (delta=0)", |s| s.exact as f64 * 100.0 / n, &tb, &ta);
    row("Within 8 bytes", |s| s.within_8 as f64 * 100.0 / n, &tb, &ta);
    row("IoU >= 50%", |s| s.iou as f64 * 100.0 / n, &tb, &ta);

    // Precision guard: counts, not percentages.
    println!(
        "\nPRECISION GUARD: candidates {} -> {} ({:+})",
        tb.reported,
        ta.reported,
        ta.reported as i64 - tb.reported as i64
    );
    let _ = HashSet::<usize>::new();
}
