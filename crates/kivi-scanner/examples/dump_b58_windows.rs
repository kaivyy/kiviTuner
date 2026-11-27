//! Diagnostic: what the curve detector actually does on the real BMW B58 ROM
//! around `0x700F82`, the fixture the summary claimed was "tail-bleeding".
//!
//! Run: `cargo run --example dump_b58_windows`

use kivi_scanner::curve_scan::{scan_curves, CurveScanOptions, Endian};

const ROM: &str =
    "/root/ecu_master_database/02_CARS/BMW/B48_B58_GEN1_GEN2_A2L_XDF/00005D55508C05_b58o1_original.bin";

/// (name, oracle address, points) — all little-endian 16-bit.
const FIXTURES: &[(&str, usize, usize)] = &[
    ("B58_1_0x700F82", 0x700F82, 6),
    ("B58_2_0x700FD4", 0x700FD4, 6),
    ("B58_3_0x7010F4", 0x7010F4, 10),
];

fn main() {
    let rom = std::fs::read(ROM).expect("B58 ROM");

    for &(name, oracle, points) in FIXTURES {
        let base = oracle.saturating_sub(0x100);
        let end = (oracle + 0x200).min(rom.len());
        let bytes = &rom[base..end];
        let oracle_off = oracle - base;
        let oracle_end = oracle_off + points * 2;

        println!("============================================================");
        println!("{name}  oracle={oracle:#X}  points={points}  window={base:#X}..{end:#X}");

        // Oracle cells, to confirm what we are looking for.
        let truth: Vec<u16> = (0..points)
            .map(|i| u16::from_le_bytes([rom[oracle + i * 2], rom[oracle + i * 2 + 1]]))
            .collect();
        println!("  ORACLE cells LE = {truth:?}");

        for el in [Endian::Little, Endian::Big] {
            let curves = scan_curves(
                bytes,
                &CurveScanOptions {
                    min_points: 3,
                    max_points: 64,
                    endian: el,
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
            println!("  --- {el:?}: {} total curves, {} covering", curves.len(), covering.len());
            for c in covering.iter().take(8) {
                let delta = c.address as i64 - oracle_off as i64;
                let head: Vec<u16> = c.values.iter().take(10).map(|v| *v as u16).collect();
                println!(
                    "      {:#X} ({delta:+}) pts={} smooth={:.4} head={head:?}",
                    base + c.address,
                    c.points,
                    c.smoothness
                );
            }
        }
    }
}
