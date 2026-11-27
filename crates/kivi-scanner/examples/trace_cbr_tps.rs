//! Diagnose which offsets the scanner actually visits and what it emits there.
//!
//! Run: `cargo run --example trace_cbr_tps`.

use kivi_scanner::boundary_snap::seam_len;
use kivi_scanner::curve_scan::{scan_curves_snapped, CurveScanOptions, Endian};

const ROM: &str = "/root/ecu_database/honda/cbr250rr/CBR250RR-38770-K64-ND1_BIN.bin";

fn main() {
    let rom = std::fs::read(ROM).unwrap();
    let base = 0x04FF00usize;
    let bytes = &rom[base..base + 320];
    let oracle = 0x04FF80 - base;

    let curves = scan_curves_snapped(
        bytes,
        &CurveScanOptions {
            min_points: 3,
            max_points: 40,
            endian: Endian::Big,
            include_8bit: false,
            allow_monotonic: true,
            snap_boundaries: true,
        },
    );

    println!("oracle fx+{oracle} = 0x{:X}", base + oracle);
    for s in &curves {
        println!(
            "  emitted {:#X} raw={:#X} pts={} head={} tail={}",
            base + s.curve.address,
            base + s.raw_address,
            s.curve.points,
            s.trimmed_head,
            s.trimmed_tail
        );
    }

    let fussy = curves.iter().find(|c| c.raw_address <= oracle && oracle < c.raw_address + c.curve.points*2);
    match fussy {
        Some(s) => println!("covering: raw={:#X} pts={} emitted_at={:#X}", base+s.raw_address, s.curve.points, base+s.curve.address),
        None => println!("no covering curve"),
    }
    // What seam_len reports at each candidate start the loop could visit.
    let vals: Vec<u16> = (0..150)
        .map(|k| u16::from_be_bytes([bytes[k * 2], bytes[k * 2 + 1]]))
        .collect();
    println!("\nseam_len from each start (first 20 cells of window):");
    for k in 55..75 {
        let w = &vals[k..(k + 40).min(vals.len())];
        println!(
            "  f{} {:#X}: seam={:2} head={:?}",
            k, base + k * 2,
            seam_len(w),
            &w[..8.min(w.len())]
        );
    }
}
