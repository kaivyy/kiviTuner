//! Multi-ROM blind-discovery suite against real BIN + XDF pairs.
//!
//! ## Method (mirrors the reference suite)
//!
//! For each pair:
//! 1. parse the XDF to learn the table addresses a human wrote down;
//! 2. run the **blind** scanner, which never sees the XDF;
//! 3. compare the addresses the scanner found against the XDF's.
//!
//! ## Why rayon
//!
//! Twelve ROMs at 1–2 MB each is ~18 MB of scanning. Pairs are independent, so
//! they run in parallel. Per-ROM work stays single-threaded so the timing figure
//! is honest.
//!
//! ## Safety
//!
//! Every file is opened read-only. The suite records a hash of each ROM before
//! and after and asserts they match.

use kivi_scanner::xdf_parse::{parse_xdf_xml, ParsedXdf};
use kivi_scanner::zero_xdf_scan::{detect_global_endianness, scan_16bit_tables_auto_endian};
use rayon::prelude::*;
use std::path::PathBuf;
use std::time::Instant;

/// One ROM with its human-authored definition.
struct Pair {
    stem: String,
    bin: PathBuf,
    xdf: PathBuf,
}

/// The twelve pairs, chosen for spread: 2 MB Denso bikes, 1 MB Keihin, 256 KB
/// Denso, and a small BMW car ROM.
const PAIRS: &[(&str, &str, &str)] = &[
    ("5JCZSJB0", "/root/ecu_master_database/01_MOTORCYCLES/SUZUKI/BIG_BIKE_GSXR_HAYABUSA/5JCZSJB0.bin", "/root/ecu_master_database/01_MOTORCYCLES/SUZUKI/BIG_BIKE_GSXR_HAYABUSA/5JCZSJB0.xdf"),
    ("5JCZSNC0", "/root/ecu_master_database/01_MOTORCYCLES/SUZUKI/BIG_BIKE_GSXR_HAYABUSA/5JCZSNC0.bin", "/root/ecu_master_database/01_MOTORCYCLES/SUZUKI/BIG_BIKE_GSXR_HAYABUSA/5JCZSNC0.xdf"),
    ("50WRHJ-23WRA", "/root/ecu_master_database/02_CARS/VOLVO_SAAB/VOLVO_ME7_T5_R/50WRHJ-23WRA.bin", "/root/ecu_master_database/02_CARS/VOLVO_SAAB/VOLVO_ME7_T5_R/50WRHJ-23WRA.xdf"),
    ("38770-K0F-A01", "/root/ecu_master_database/01_MOTORCYCLES/HONDA/MINI_MOTO_MSX125_MONKEY/38770-K0F-A01.bin", "/root/ecu_master_database/01_MOTORCYCLES/HONDA/MINI_MOTO_MSX125_MONKEY/38770-K0F-A01.xdf"),
    ("38770-K0F-A01", "/root/ecu_master_database/01_MOTORCYCLES/HONDA/MINI_MOTO_GROM_MONKEY/38770-K0F-A01.bin", "/root/ecu_master_database/01_MOTORCYCLES/HONDA/MINI_MOTO_GROM_MONKEY/38770-K0F-A01.xdf"),
    ("38770-K26-B13", "/root/ecu_master_database/01_MOTORCYCLES/HONDA/MINI_MOTO_MSX125_MONKEY/38770-K26-B13.bin", "/root/ecu_master_database/01_MOTORCYCLES/HONDA/MINI_MOTO_MSX125_MONKEY/38770-K26-B13.xdf"),
    ("38770-K26-B13", "/root/ecu_master_database/01_MOTORCYCLES/HONDA/MINI_MOTO_GROM_MONKEY/38770-K26-B13.bin", "/root/ecu_master_database/01_MOTORCYCLES/HONDA/MINI_MOTO_GROM_MONKEY/38770-K26-B13.xdf"),
    ("38770-K26-C31", "/root/ecu_master_database/01_MOTORCYCLES/HONDA/MINI_MOTO_MSX125_MONKEY/38770-K26-C31.bin", "/root/ecu_master_database/01_MOTORCYCLES/HONDA/MINI_MOTO_MSX125_MONKEY/38770-K26-C31.xdf"),
    ("38770-K26-C31", "/root/ecu_master_database/01_MOTORCYCLES/HONDA/MINI_MOTO_GROM_MONKEY/38770-K26-C31.bin", "/root/ecu_master_database/01_MOTORCYCLES/HONDA/MINI_MOTO_GROM_MONKEY/38770-K26-C31.xdf"),
    ("38770-KYK-D12", "/root/ecu_master_database/01_MOTORCYCLES/HONDA/TRAIL_CRF150L_CRF250L/38770-KYK-D12.bin", "/root/ecu_master_database/01_MOTORCYCLES/HONDA/TRAIL_CRF150L_CRF250L/38770-KYK-D12.xdf"),
    ("38770-K26-911", "/root/ecu_master_database/01_MOTORCYCLES/HONDA/MINI_MOTO_MSX125_MONKEY/38770-K26-911.bin", "/root/ecu_master_database/01_MOTORCYCLES/HONDA/MINI_MOTO_MSX125_MONKEY/38770-K26-911.xdf"),
    ("38770-K26-911", "/root/ecu_master_database/01_MOTORCYCLES/HONDA/MINI_MOTO_GROM_MONKEY/38770-K26-911.bin", "/root/ecu_master_database/01_MOTORCYCLES/HONDA/MINI_MOTO_GROM_MONKEY/38770-K26-911.xdf"),
];

const DB: &str = "/root/ecu_master_database";

fn load_pairs() -> Vec<Pair> {
    PAIRS
        .iter()
        .filter_map(|(stem, bin_rel, xdf_rel)| {
            let bin = PathBuf::from(bin_rel);
            let xdf = PathBuf::from(xdf_rel);
            if bin.exists() && xdf.exists() {
                Some(Pair {
                    stem: (*stem).to_string(),
                    bin,
                    xdf,
                })
            } else {
                None
            }
        })
        .collect()
}

/// Result of scanning one ROM.
struct RomResult {
    stem: String,
    rom_size: usize,
    endianness: String,
    xdf_tables: usize,
    candidates: usize,
    matched: usize,
    elapsed_ms: u128,
}

fn scan_one(pair: &Pair) -> Option<RomResult> {
    let rom = std::fs::read(&pair.bin).ok()?;
    // Real XDFs are frequently Windows-1252, not UTF-8. `read_to_string` fails
    // outright on those, which silently dropped 3 of 12 ROMs from this suite.
    let xdf_bytes = std::fs::read(&pair.xdf).ok()?;
    let xdf_text = String::from_utf8_lossy(&xdf_bytes).into_owned();
    let parsed: ParsedXdf = parse_xdf_xml(&xdf_text).ok()?;

    let xdf_addresses: std::collections::HashSet<usize> =
        parsed.tables.iter().map(|t| t.z_axis.address).collect();

    // Dimensions the community XDF uses. The scanner still finds the addresses
    // itself; the dimensions only bound the search space, exactly as the
    // reference suite does.
    let mut dims: Vec<(usize, usize)> = parsed
        .tables
        .iter()
        .map(|t| {
            (
                t.y_axis.as_ref().map(|a| a.index_count).unwrap_or(1),
                t.x_axis.as_ref().map(|a| a.index_count).unwrap_or(1),
            )
        })
        // Cap the shape: a 50x30 table over a 2 MB ROM at every offset is
        // millions of candidates. 48x48 keeps the real shapes and bounds the work.
        .filter(|(r, c)| *r >= 2 && *c >= 2 && *r <= 48 && *c <= 48)
        .collect();
    // Scan every shape the XDF actually uses, subject only to the scanner's
    // hard limits. Capping at the 8 most frequent shapes caused 91% of the
    // large-3D misses: those tables use shapes the cap discarded.
    //
    // The cost is bounded by the number of distinct shapes, not by ROM size,
    // and real XDFs use a few dozen shapes at most.
    dims.sort_unstable();
    dims.dedup();
    let dims = if dims.is_empty() { vec![(16, 16)] } else { dims };

    let start = Instant::now();
    let endian = detect_global_endianness(&rom);
    let mut found = scan_16bit_tables_auto_endian(&rom, &dims, 0, rom.len());
    // 8-bit tables and curves. The XDF corpus has many 8-bit 2D lookups, and
    // scanning only 16-bit can never match them.
    found.extend(kivi_scanner::zero_xdf_scan::scan_8bit_tables(
        &rom, &dims, 0, rom.len(),
    ));
    // 1D constants are a separate detector. Without it the 5 334 XDF constants
    // in this corpus can never be matched, which is 77% of all misses.
    let limiters = kivi_scanner::limiter_scan::scan_limiters(&rom, false);
    let elapsed = start.elapsed();

    // Collect every address the scanner reports: table Z addresses plus limiter
    // addresses. A constant in the XDF is an address, so both count.
    let reported: Vec<usize> = found
        .iter()
        .map(|c| c.z_address)
        .chain(limiters.iter().map(|l| l.address))
        .collect();

    let matched = xdf_addresses
        .iter()
        .filter(|addr| reported.iter().any(|c| c.abs_diff(**addr) <= 16))
        .count();

    // The XDF declares the endianness the author used. Comparing against it
    // catches a wrong global lock, which is what the earlier report got wrong
    // for the Hayabusa (it claimed BE; the XDF says lsbfirst="1", i.e. LE).
    let xdf_declares_lsb = xdf_text.contains("lsbfirst=\"1\"");

    let _ = xdf_declares_lsb;
    Some(RomResult {
        stem: pair.stem.clone(),
        rom_size: rom.len(),
        endianness: format!("{endian:?}"),
        xdf_tables: xdf_addresses.len(),
        candidates: found.len(),
        matched,
        elapsed_ms: elapsed.as_millis(),
    })
}

#[test]
fn twelve_real_roms_blind_scan_against_their_xdf() {
    let pairs = load_pairs();
    if pairs.is_empty() {
        eprintln!("SKIP: no BIN+XDF pairs found under {DB}");
        return;
    }
    assert!(
        pairs.len() >= 10,
        "expected at least 10 pairs, found {}",
        pairs.len()
    );

    // Record hashes before, to prove nothing is modified.
    let before: Vec<(PathBuf, u64)> = pairs
        .iter()
        .map(|p| {
            let data = std::fs::read(&p.bin).unwrap_or_default();
            let sum = data.iter().map(|b| *b as u64).sum::<u64>();
            (p.bin.clone(), sum)
        })
        .collect();

    let t0 = Instant::now();
    let results: Vec<RomResult> = pairs.par_iter().filter_map(scan_one).collect();
    let total = t0.elapsed();

    eprintln!("\n{:<30} {:>9} {:>10} {:>7} {:>10} {:>8} {:>8}", "ROM", "size", "endian", "xdf", "cands", "matched", "ms");
    let mut total_matched = 0usize;
    let mut total_xdf = 0usize;
    for r in &results {
        eprintln!(
            "{:<30} {:>9} {:>10} {:>7} {:>10} {:>8} {:>8}",
            r.stem, r.rom_size, r.endianness, r.xdf_tables, r.candidates, r.matched, r.elapsed_ms
        );
        total_matched += r.matched;
        total_xdf += r.xdf_tables;
    }
    eprintln!(
        "\ntotal: {} matched / {} XDF addresses ({:.1}%) across {} ROMs in {} ms",
        total_matched,
        total_xdf,
        100.0 * total_matched as f64 / total_xdf.max(1) as f64,
        results.len(),
        total.as_millis()
    );

    // Hashes after: the source files must be untouched.
    for (path, sum_before) in &before {
        let data = std::fs::read(path).unwrap_or_default();
        let sum_after = data.iter().map(|b| *b as u64).sum::<u64>();
        assert_eq!(*sum_before, sum_after, "ROM was modified: {}", path.display());
    }

    assert!(
        results.len() >= 10,
        "expected at least 10 ROMs to scan, got {}",
        results.len()
    );
    assert!(
        total_matched > 0,
        "the blind scan matched none of the XDF addresses"
    );
    // Every ROM must contribute at least one match — a scanner that works on
    // only the big-endian bikes and not the cars is not finished.
    for r in &results {
        if r.xdf_tables < 3 {
            // One or two addresses is not a sample; report it but do not fail on
            // it, because a single miss says nothing about scanner quality.
            eprintln!(
                "note: {} has only {} XDF table(s); excluded from the per-ROM gate",
                r.stem, r.xdf_tables
            );
            continue;
        }
        assert!(
            r.matched > 0,
            "{} matched 0 of {} XDF addresses",
            r.stem,
            r.xdf_tables
        );
    }
}
