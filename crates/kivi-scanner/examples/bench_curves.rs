//! Throughput check for the 1-D curve scanner.
//!
//! Run: `cargo run --release -p kivi-scanner --example bench_curves`
use kivi_scanner::curve_scan::{scan_curves, CurveScanOptions, Endian};
use std::time::Instant;

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        "/root/ecu_master_database/01_MOTORCYCLES/HONDA/SPORT_CBR250RR/CBR250RR-38770-K64-ND1_BIN.bin"
            .to_string()
    });
    let rom = match std::fs::read(&path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("cannot read {path}: {e}");
            return;
        }
    };
    println!("rom = {} bytes ({})", rom.len(), path);

    for (label, opts) in [
        (
            "BE 16-bit",
            CurveScanOptions {
                min_points: 3,
                max_points: 40,
                endian: Endian::Big,
                include_8bit: false,
                allow_monotonic: true,
                ..Default::default()
            },
        ),
        (
            "BE 16+8bit",
            CurveScanOptions {
                min_points: 3,
                max_points: 40,
                endian: Endian::Big,
                include_8bit: true,
                allow_monotonic: true,
                ..Default::default()
            },
        ),
        (
            "LE 16-bit",
            CurveScanOptions {
                min_points: 3,
                max_points: 40,
                endian: Endian::Little,
                include_8bit: false,
                allow_monotonic: true,
                ..Default::default()
            },
        ),
    ] {
        let t = Instant::now();
        let found = scan_curves(&rom, &opts);
        println!(
            "{label:12} -> {:6} curves in {:6} ms",
            found.len(),
            t.elapsed().as_millis()
        );
    }
}
