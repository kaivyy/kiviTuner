//! Diagnostic: print the exact windows the detector produces around each
//! real-ROM fixture, so boundary snapping is designed from bytes rather than
//! from a guess.
//!
//! Run: `cargo run --example dump_boundary_windows`

use kivi_scanner::curve_scan::{scan_curves, CurveScanOptions, Endian};

const ROM: &str = "/root/ecu_database/honda/cbr250rr/CBR250RR-38770-K64-ND1_BIN.bin";

/// (name, fixture base address, oracle address, points)
const FIXTURES: &[(&str, usize, usize, usize)] = &[
    ("CBR_TPS_AXIS", 0x04FF00, 0x04FF80, 25),
    ("CBR_MAP_AXIS", 0x04FF32, 0x04FFB2, 25),
    ("CBR_RPM_AXIS", 0x04FF64, 0x04FFE4, 35),
];

fn main() {
    let rom = std::fs::read(ROM).expect("CBR ROM");
    for &(name, base, oracle, points) in FIXTURES {
        let bytes = &rom[base..(base + 320).min(rom.len())];
        let oracle_off = oracle - base;
        let oracle_end = oracle_off + points * 2;

        println!("============================================================");
        println!("{name}  base={base:#X}  oracle {oracle:#X} (fx+{oracle_off})");

        let curves = scan_curves(
            bytes,
            &CurveScanOptions {
                min_points: 3,
                max_points: 40,
                endian: Endian::Big,
                include_8bit: false,
                allow_monotonic: true,
                ..Default::default()
            },
        );

        let mut covering: Vec<_> = curves
            .iter()
            .filter(|c| {
                let e = c.address + c.points * c.cell_bytes;
                e > oracle_off && c.address < oracle_end
            })
            .collect();
        covering.sort_by_key(|c| c.address);

        for c in &covering {
            let delta = c.address as i64 - oracle_off as i64;
            let head: Vec<u16> = c.values.iter().take(10).map(|v| *v as u16).collect();
            let tail: Vec<u16> = c.values.iter().rev().take(6).rev().map(|v| *v as u16).collect();
            println!(
                "  {:#X} ({delta:+}) pts={} smooth={:.4}\n      head={head:?}\n      tail={tail:?}",
                base + c.address,
                c.points,
                c.smoothness
            );
        }
        if covering.is_empty() {
            println!("  NO COVERING CURVE");
        }
    }
}
