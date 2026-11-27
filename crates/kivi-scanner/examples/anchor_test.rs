//! Does the set of 32-bit literals that land inside the ROM act as a hard
//! anchor for table starts?
//!
//! Run: `cargo run --release --example anchor_test`

use std::collections::HashSet;

use kivi_scanner::curve_scan::{scan_curves, CurveScanOptions, Endian};
use kivi_scanner::xdf_parse::parse_xdf_xml;
use kivi_scanner::zero_xdf_scan::scan_16bit_tables_auto_endian;

const DB: &str = "/root/ecu_master_database";
const BIN: &str = "01_MOTORCYCLES/HONDA/SPORT_CBR250RR/CBR250RR-38770-K64-ND1_BIN.bin";
const XDF: &str = "01_MOTORCYCLES/HONDA/SPORT_CBR250RR/CBR250RR_38770-K64-ND1_PUBLIC_v1.xdf";

/// Every 32-bit big-endian word whose value lands inside `[0, len)`.
fn anchor_set(rom: &[u8]) -> HashSet<u32> {
    let mut s = HashSet::new();
    for a in (0..rom.len().saturating_sub(3)).step_by(2) {
        let v = u32::from_be_bytes([rom[a], rom[a + 1], rom[a + 2], rom[a + 3]]);
        if (v as usize) < rom.len() {
            s.insert(v);
        }
    }
    s
}

struct Oracle {
    addr: usize,
    rows: usize,
    cols: usize,
}

fn main() {
    let rom = std::fs::read(format!("{DB}/{BIN}")).expect("rom");
    let xml = String::from_utf8_lossy(&std::fs::read(format!("{DB}/{XDF}")).expect("xdf")).into_owned();
    let parsed = parse_xdf_xml(&xml).expect("parse");

    let mut oracle = Vec::new();
    for t in &parsed.tables {
        let r = t.y_axis.as_ref().map(|a| a.index_count).unwrap_or(1);
        let c = t.x_axis.as_ref().map(|a| a.index_count).unwrap_or(1);
        if r < 2 && c < 2 {
            continue;
        }
        oracle.push(Oracle { addr: t.z_axis.address, rows: r, cols: c });
    }
    oracle.sort_by_key(|o| o.addr);
    oracle.dedup_by_key(|o| o.addr);

    let anchors = anchor_set(&rom);
    println!("oracle tables: {}", oracle.len());
    println!("32-bit literal anchors inside ROM: {}", anchors.len());

    let hit = oracle.iter().filter(|o| anchors.contains(&(o.addr as u32))).count();
    println!(
        "oracle starts that ARE a literal anchor: {hit}/{} ({:.1}%)",
        oracle.len(),
        100.0 * hit as f64 / oracle.len() as f64
    );
    let hit1d = oracle.iter().filter(|o| (o.rows < 2 || o.cols < 2) && anchors.contains(&(o.addr as u32))).count();
    let n1d = oracle.iter().filter(|o| o.rows < 2 || o.cols < 2).count();
    let hit2d = oracle.iter().filter(|o| o.rows >= 2 && o.cols >= 2 && anchors.contains(&(o.addr as u32))).count();
    let n2d = oracle.iter().filter(|o| o.rows >= 2 && o.cols >= 2).count();
    println!("   1-D: {hit1d}/{n1d}   2-D: {hit2d}/{n2d}");

    // Now: does adding the anchors as candidate starts improve recall?
    let mut shapes_2d: Vec<(usize, usize)> = oracle
        .iter()
        .filter(|o| o.rows >= 2 && o.cols >= 2 && o.rows <= 48 && o.cols <= 48)
        .map(|o| (o.rows, o.cols))
        .collect();
    shapes_2d.sort_unstable();
    shapes_2d.dedup();

    let tables = scan_16bit_tables_auto_endian(&rom, &shapes_2d, 0, rom.len());
    let curves = scan_curves(
        &rom,
        &CurveScanOptions {
            min_points: 3,
            max_points: 40,
            endian: Endian::Big,
            include_8bit: true,
            allow_monotonic: true,
            snap_boundaries: true,
        },
    );

    let mut rep_2d: Vec<usize> = tables.iter().map(|t| t.z_address).collect();
    rep_2d.sort_unstable();
    let mut rep_1d: Vec<usize> = curves.iter().map(|c| c.address).collect();
    rep_1d.sort_unstable();

    let count_exact = |rep: &[usize], oracle: &[&Oracle]| -> usize {
        oracle.iter().filter(|o| rep.binary_search(&o.addr).is_ok()).count()
    };
    let o2d: Vec<&Oracle> = oracle.iter().filter(|o| o.rows >= 2 && o.cols >= 2).collect();
    let o1d: Vec<&Oracle> = oracle.iter().filter(|o| o.rows < 2 || o.cols < 2).collect();

    println!();
    println!("without anchors: 2-D exact {}/{}  1-D exact {}/{}",
        count_exact(&rep_2d, &o2d), o2d.len(),
        count_exact(&rep_1d, &o1d), o1d.len());

    // Apply the anchor: any oracle-range address that IS a literal anchor is
    // accepted as a verified start.
    // Union, not sum: an oracle can be both detected and anchored.
    let union2d = o2d.iter().filter(|o| rep_2d.binary_search(&o.addr).is_ok() || anchors.contains(&(o.addr as u32))).count();
    let union1d = o1d.iter().filter(|o| rep_1d.binary_search(&o.addr).is_ok() || anchors.contains(&(o.addr as u32))).count();
    println!("with anchors:    2-D exact {}/{}  1-D exact {}/{}",
        union2d, o2d.len(), union1d, o1d.len());
    println!("(gained 2-D {}, 1-D {})",
        union2d - count_exact(&rep_2d, &o2d), union1d - count_exact(&rep_1d, &o1d));

    // Danger: how many anchors fall OUTSIDE the calibration area? A literal
    // that merely points into the ROM is not evidence of a table.
    let cal = 0x04F000usize..0x053000;
    let in_cal = anchors.iter().filter(|a| cal.contains(&(**a as usize))).count();
    println!();
    println!("anchors inside calibration window: {in_cal} of {}", anchors.len());
    println!("anchors used per oracle hit: {}", anchors.len() as f64 / hit.max(1) as f64);
}
