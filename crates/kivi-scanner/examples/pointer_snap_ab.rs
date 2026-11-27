//! Pointer-guided snapping measured on CBR250RR and B58.
//!
//! Anchors come from ROM structure only (`pointer_snap::Anchors::from_rom`).
//! XDF is read *only* to score the result, never to build or select anchors.
//!
//! Run: `cargo run --release --example pointer_snap_ab`

use kivi_scanner::curve_scan::{scan_curves, CurveScanOptions, Endian};
use kivi_scanner::pointer_snap::Anchors;
use kivi_scanner::xdf_parse::parse_xdf_xml;
use kivi_scanner::zero_xdf_scan::scan_16bit_tables_auto_endian;

const DB: &str = "/root/ecu_master_database";

struct Rom {
    name: &'static str,
    bin: &'static str,
    xdf: &'static str,
}

const ROMS: &[Rom] = &[
    Rom {
        name: "CBR250RR",
        bin: "01_MOTORCYCLES/HONDA/SPORT_CBR250RR/CBR250RR-38770-K64-ND1_BIN.bin",
        xdf: "01_MOTORCYCLES/HONDA/SPORT_CBR250RR/CBR250RR_38770-K64-ND1_PUBLIC_v1.xdf",
    },
    Rom {
        name: "BMW_B58",
        bin: "02_CARS/BMW/B58_3_0L_TURBO_GEN1_GEN2/00005D55508C05_b58o1_original.bin",
        xdf: "02_CARS/BMW/B58_3_0L_TURBO_GEN1_GEN2/00005D55508C05.xdf",
    },
];

const RADIUS: usize = 16;

fn main() {
    for r in ROMS {
        let rom = std::fs::read(format!("{DB}/{}", r.bin)).expect("rom");
        let xml = String::from_utf8_lossy(&std::fs::read(format!("{DB}/{}", r.xdf)).expect("xdf")).into_owned();
        let parsed = parse_xdf_xml(&xml).expect("parse");

        let mut o1d: Vec<usize> = Vec::new();
        let mut o2d: Vec<usize> = Vec::new();
        let mut shapes: Vec<(usize, usize)> = Vec::new();
        for t in &parsed.tables {
            let rr = t.y_axis.as_ref().map(|a| a.index_count).unwrap_or(1);
            let cc = t.x_axis.as_ref().map(|a| a.index_count).unwrap_or(1);
            if rr < 2 && cc < 2 {
                continue;
            }
            if rr >= 2 && cc >= 2 {
                o2d.push(t.z_axis.address);
                if rr <= 48 && cc <= 48 {
                    shapes.push((rr, cc));
                }
            } else {
                o1d.push(t.z_axis.address);
            }
        }
        o1d.sort_unstable();
        o1d.dedup();
        o2d.sort_unstable();
        o2d.dedup();
        shapes.sort_unstable();
        shapes.dedup();

        // Detector output only — no XDF involvement.
        let tables = if shapes.is_empty() {
            Vec::new()
        } else {
            scan_16bit_tables_auto_endian(&rom, &shapes, 0, rom.len())
        };
        let endian = if r.name == "CBR250RR" { Endian::Big } else { Endian::Little };
        let curves = scan_curves(
            &rom,
            &CurveScanOptions {
                min_points: 3,
                max_points: 40,
                endian,
                include_8bit: true,
                allow_monotonic: true,
                snap_boundaries: true,
            },
        );

        let anchors = Anchors::from_rom(&rom, endian == Endian::Big);
        let before_2d: Vec<usize> = tables.iter().map(|t| t.z_address).collect();
        let before_1d: Vec<usize> = curves.iter().map(|c| c.address).collect();

        // Snap: move an existing candidate onto a nearby anchor, never add one.
        let shifted: Vec<usize> = before_2d
            .iter()
            .map(|&a| anchors.nearest(a, RADIUS).map(|(t, _)| t).unwrap_or(a))
            .collect();
        let shifted_1d: Vec<usize> = before_1d
            .iter()
            .map(|&a| anchors.nearest(a, RADIUS).map(|(t, _)| t).unwrap_or(a))
            .collect();

        let exact = |rep: &[usize], oracle: &[usize]| {
            let mut s = rep.to_vec();
            s.sort_unstable();
            s.dedup();
            oracle.iter().filter(|o| s.binary_search(o).is_ok()).count()
        };

        println!("============================================================");
        println!("{}  anchors={}  radius={RADIUS}B", r.name, anchors.len());
        println!(
            "  candidates 2-D {} -> {}   1-D {} -> {}",
            before_2d.len(),
            shifted.len(),
            before_1d.len(),
            shifted_1d.len()
        );
        println!(
            "  2-D exact   {}/{} ({:.1}%) -> {}/{} ({:.1}%)",
            exact(&before_2d, &o2d), o2d.len(),
            100.0 * exact(&before_2d, &o2d) as f64 / o2d.len().max(1) as f64,
            exact(&shifted, &o2d), o2d.len(),
            100.0 * exact(&shifted, &o2d) as f64 / o2d.len().max(1) as f64
        );
        println!(
            "  1-D exact   {}/{} ({:.1}%) -> {}/{} ({:.1}%)",
            exact(&before_1d, &o1d), o1d.len(),
            100.0 * exact(&before_1d, &o1d) as f64 / o1d.len().max(1) as f64,
            exact(&shifted_1d, &o1d), o1d.len(),
            100.0 * exact(&shifted_1d, &o1d) as f64 / o1d.len().max(1) as f64
        );
        let moved = before_2d
            .iter()
            .zip(&shifted)
            .filter(|(a, b)| a != b)
            .count()
            + before_1d.iter().zip(&shifted_1d).filter(|(a, b)| a != b).count();
        println!("  candidates moved by snapping: {moved}");
    }
}
