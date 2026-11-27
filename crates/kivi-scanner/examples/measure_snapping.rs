//! Measures what boundary snapping achieves on real ROM windows, for all six
//! 1-D fixtures, and probes whether any single gradient multiplier could
//! separate the boundaries that are still missed.
//!
//! Run: `cargo run --example measure_snapping`

use kivi_scanner::boundary_snap::snap_monotonic;
use kivi_scanner::curve_scan::{scan_curves, CurveScanOptions, Endian};

const CBR: &str = "/root/ecu_database/honda/cbr250rr/CBR250RR-38770-K64-ND1_BIN.bin";
const B58: &str =
    "/root/ecu_master_database/02_CARS/BMW/B48_B58_GEN1_GEN2_A2L_XDF/00005D55508C05_b58o1_original.bin";

struct Fixture {
    name: &'static str,
    rom: &'static str,
    oracle: usize,
    points: usize,
    endian: Endian,
}

const FIXTURES: &[Fixture] = &[
    Fixture { name: "CBR_TPS_AXIS", rom: CBR, oracle: 0x04FF80, points: 25, endian: Endian::Big },
    Fixture { name: "CBR_MAP_AXIS", rom: CBR, oracle: 0x04FFB2, points: 25, endian: Endian::Big },
    Fixture { name: "CBR_RPM_AXIS", rom: CBR, oracle: 0x04FFE4, points: 35, endian: Endian::Big },
    Fixture { name: "B58_1_0x700F82", rom: B58, oracle: 0x700F82, points: 6, endian: Endian::Little },
    Fixture { name: "B58_2_0x700FD4", rom: B58, oracle: 0x700FD4, points: 6, endian: Endian::Little },
    Fixture { name: "B58_3_0x7010F4", rom: B58, oracle: 0x7010F4, points: 10, endian: Endian::Little },
];

fn decode(bytes: &[u8], at: usize, n: usize, endian: Endian) -> Vec<u16> {
    (0..n)
        .map(|i| {
            let o = at + i * 2;
            match endian {
                Endian::Big => u16::from_be_bytes([bytes[o], bytes[o + 1]]),
                Endian::Little => u16::from_le_bytes([bytes[o], bytes[o + 1]]),
            }
        })
        .collect()
}

fn main() {
    let cbr = std::fs::read(CBR).expect("CBR ROM");
    let b58 = std::fs::read(B58).expect("B58 ROM");

    let mut exact_before = 0;
    let mut exact_after = 0;
    let mut considered = 0;
    let mut any_exact_count = 0;

    for f in FIXTURES {
        let rom: &Vec<u8> = if f.rom == CBR { &cbr } else { &b58 };
        let base = f.oracle.saturating_sub(0x100);
        let end = (f.oracle + 0x200).min(rom.len());
        let bytes = &rom[base..end];
        let oracle_off = f.oracle - base;
        let oracle_end = oracle_off + f.points * 2;

        let curves = scan_curves(
            bytes,
            &CurveScanOptions {
                min_points: 3,
                max_points: 64,
                endian: f.endian,
                include_8bit: false,
                allow_monotonic: true,
                ..Default::default()
            },
        );

        let mut covering: Vec<_> = curves
            .iter()
            .filter(|c| c.address + c.points * c.cell_bytes > oracle_off && c.address < oracle_end)
            .collect();
        covering.sort_by_key(|c| std::cmp::Reverse(c.points));

        let Some(curve) = covering.first() else {
            println!("{}: no covering curve", f.name);
            continue;
        };
        considered += 1;

        let delta_before = curve.address as i64 - oracle_off as i64;
        if delta_before == 0 {
            exact_before += 1;
        }

        let vals = decode(bytes, curve.address, curve.points, f.endian);
        let Some(s) = snap_monotonic(&vals) else {
            println!("{}: snap returned None", f.name);
            continue;
        };
        let new_start = curve.address + s.trimmed_head * 2;
        let delta_after = new_start as i64 - oracle_off as i64;
        if delta_after == 0 {
            exact_after += 1;
        }

        println!(
            "{}: window {:#X} pts={} head={:?}\n    before {:+}  after {:+} (head-{} tail-{}) => {}",
            f.name,
            base + curve.address,
            curve.points,
            vals.iter().take(8).collect::<Vec<_>>(),
            delta_before,
            delta_after,
            s.trimmed_head,
            s.trimmed_tail,
            if delta_after == 0 { "EXACT" } else { "still off" }
        );

        // Would the snapped window's own cells look like the declared row?
        let trimmed: Vec<u16> = vals.iter().skip(s.trimmed_head).take(s.points).copied().collect();
        println!("    snapped cells = {:?}", &trimmed[..trimmed.len().min(12)]);

        // Recall-at-exact-boundary: does ANY covering window, once snapped,
        // land on the oracle? This is the metric that matters, because a single
        // 1-D axis is often covered by several partial windows and no one of
        // them is privileged.
        let mut any_exact = false;
        let mut best: Option<(usize, i64)> = None;
        for c in &covering {
            let v = decode(bytes, c.address, c.points, f.endian);
            let Some(sc) = snap_monotonic(&v) else { continue };
            let start = c.address + sc.trimmed_head * 2;
            let d = start as i64 - oracle_off as i64;
            if d == 0 {
                any_exact = true;
            }
            if best.is_none_or(|(_, bd)| d.abs() < bd.abs()) {
                best = Some((base + start, d));
            }
        }
        let (addr, d) = best.unwrap_or((0, i64::MIN));
        println!(
            "    any-exact = {any_exact}; best snapped start {addr:#X} ({d:+})"
        );
        if any_exact {
            any_exact_count += 1;
        }
    }

    println!("\nEXACT (primary window) before {exact_before}/{considered}  after {exact_after}/{considered}");
    println!("RECALL any-window-exact after snapping: {any_exact_count}/{considered}");
}
