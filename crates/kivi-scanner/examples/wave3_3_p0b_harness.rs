//! Wave 3.3 P0b: provenance-repaired, CWD-independent blind discovery verification.
//!
//! Strictly blind — zero XDF/A2L/DAMOS imports anywhere in this binary.
//! Fixes D1–D6 from `research/wave-3.3/P0-PROVENANCE-ERRATUM.md` and
//! `research/wave-3.3/BLIND-AUTOMAPPING-VALIDATION-READINESS-AUDIT.md` § 16:
//! - Resolves scanner source paths from `env!("CARGO_MANIFEST_DIR")` (never CWD)
//! - Fails hard on any unreadable source or ROM (never `.unwrap_or_default()`)
//! - Asserts all 4 scanner source hashes are non-empty and pairwise distinct
//! - Records binary SHA-256, git HEAD, dirty status, and `rustc --version`
//!
//! Run:
//!   `cargo run --release -p kivi-scanner --example wave3_3_p0b_harness -- <out.json> [--full-b58-b]`

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

use kivi_core::hash::compute_hashes;
use kivi_scanner::axis_anchored::axis_anchored_scan;
use kivi_scanner::blind_automap::default_shapes;
use kivi_scanner::heuristic_scan::scan_rom_16bit;
use serde::Serialize;

const DB: &str = "/root/ecu_master_database";
const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

const ROMS: &[(&str, &str)] = &[
    (
        "CBR250RR",
        "01_MOTORCYCLES/HONDA/SPORT_CBR250RR/CBR250RR-38770-K64-ND1_BIN.bin",
    ),
    (
        "BMW_B58",
        "02_CARS/BMW/B48_B58_GEN1_GEN2_A2L_XDF/00005D55508C05_b58o1_original.bin",
    ),
    (
        "SUZUKI_5JCZSNC0",
        "01_MOTORCYCLES/SUZUKI/BIG_BIKE_GSXR_HAYABUSA/5JCZSNC0.bin",
    ),
];

const SCANNER_SOURCES: &[&str] = &[
    "src/axis_anchored.rs",
    "src/heuristic_scan.rs",
    "src/blind_automap.rs",
    "src/scan_metrics.rs",
];

#[derive(Serialize)]
struct ScannerSummary {
    executed: bool,
    scope: String,
    candidates: usize,
    runtime_ms: f64,
    by_shape: BTreeMap<String, usize>,
    addresses_sha256: String,
}

#[derive(Serialize)]
struct RomReport {
    name: String,
    path: String,
    bytes: usize,
    sha256: String,
    scanner_a_excluded_reason: String,
    scanner_a_default_shapes: usize,
    scanner_b: ScannerSummary,
    scanner_c: ScannerSummary,
    scanner_c_rows_in_fallback_list: usize,
    b_c_address_overlap: Option<usize>,
}

#[derive(Serialize)]
struct P0bOutput {
    wave: String,
    phase: String,
    note: String,
    git_head: String,
    git_dirty: bool,
    rustc_version: String,
    cargo_manifest_dir: String,
    exe_path: String,
    exe_sha256: String,
    harness_source_sha256: String,
    scanner_source_sha256: BTreeMap<String, String>,
    scanner_source_self_check_passed: bool,
    full_b58_scanner_b: bool,
    totals: BTreeMap<String, usize>,
    roms: Vec<RomReport>,
}

fn sha256_file_strict(path: &std::path::Path) -> (String, usize) {
    let bytes = std::fs::read(path)
        .unwrap_or_else(|e| panic!("FATAL: failed to read '{}': {e}", path.display()));
    assert!(
        !bytes.is_empty(),
        "FATAL: file '{}' is empty",
        path.display()
    );
    let sha = compute_hashes(&bytes).sha256;
    assert_ne!(
        sha, EMPTY_SHA256,
        "FATAL: '{}' hashed to empty-buffer digest",
        path.display()
    );
    (sha, bytes.len())
}

fn run_cmd_trimmed(cmd: &str, args: &[&str], cwd: &std::path::Path) -> String {
    let out = Command::new(cmd)
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap_or_else(|e| panic!("FATAL: failed to run `{cmd}`: {e}"));
    assert!(
        out.status.success(),
        "FATAL: `{cmd} {:?}` exited with {:?}",
        args,
        out.status
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn hash_sorted_addresses(addrs: &[usize]) -> String {
    let joined: String = addrs.iter().map(|a| format!("{a:#x},")).collect();
    compute_hashes(joined.as_bytes()).sha256
}

fn main() {
    let mut out_path: Option<String> = None;
    let mut full_b58_b = false;
    for arg in std::env::args().skip(1) {
        if arg == "--full-b58-b" {
            full_b58_b = true;
        } else if out_path.is_none() {
            out_path = Some(arg);
        } else {
            panic!("Unexpected CLI argument: {arg}");
        }
    }
    let out_path = out_path.unwrap_or_else(|| "P0b-RAW-OUTPUT.json".to_string());

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("CARGO_MANIFEST_DIR must be crates/kivi-scanner")
        .to_path_buf();

    // 1. Source provenance verification (fixes D1).
    let mut scanner_source_sha256 = BTreeMap::new();
    let mut unique_hashes = BTreeSet::new();
    for rel in SCANNER_SOURCES {
        let p = manifest_dir.join(rel);
        let (sha, _) = sha256_file_strict(&p);
        assert!(
            unique_hashes.insert(sha.clone()),
            "FATAL: duplicate source hash {sha} for {rel}"
        );
        scanner_source_sha256.insert((*rel).to_string(), sha);
    }
    assert_eq!(unique_hashes.len(), SCANNER_SOURCES.len());

    let (harness_source_sha256, _) =
        sha256_file_strict(&manifest_dir.join("examples/wave3_3_p0b_harness.rs"));
    let exe_path = std::env::current_exe().expect("resolve current_exe");
    let (exe_sha256, _) = sha256_file_strict(&exe_path);

    let git_head = run_cmd_trimmed("git", &["rev-parse", "HEAD"], &repo_root);
    let git_dirty = !run_cmd_trimmed("git", &["status", "--porcelain"], &repo_root).is_empty();
    let rustc_version = run_cmd_trimmed("rustc", &["--version"], &repo_root);

    let mut rom_reports = Vec::new();
    let mut scanner_b_total = 0usize;
    let mut scanner_c_total = 0usize;

    for &(name, rel) in ROMS {
        let rom_path = PathBuf::from(DB).join(rel);
        let rom = std::fs::read(&rom_path)
            .unwrap_or_else(|e| panic!("FATAL: cannot read ROM '{}': {e}", rom_path.display()));
        assert!(!rom.is_empty(), "FATAL: ROM '{}' is empty", rom_path.display());
        let rom_sha = compute_hashes(&rom).sha256;

        // Scanner B (skip 8 MiB B58 unless --full-b58-b is supplied).
        let run_b = full_b58_b || name != "BMW_B58";
        let (b_list, b_ms, b_scope) = if run_b {
            let t = Instant::now();
            let b = scan_rom_16bit(&rom, &[16, 12, 8], 6, 16, true);
            let ms = t.elapsed().as_secs_f64() * 1000.0;
            (b, ms, "widths=[16,12,8] rows=6..=16 le=true".to_string())
        } else {
            (
                Vec::new(),
                0.0,
                "SKIPPED_ON_8MIB_B58_WITHOUT_FULL_FLAG".to_string(),
            )
        };
        scanner_b_total += b_list.len();

        let mut b_by_shape: BTreeMap<String, usize> = BTreeMap::new();
        let mut b_addrs = Vec::with_capacity(b_list.len());
        for m in &b_list {
            *b_by_shape.entry(format!("{}x{}", m.rows, m.cols)).or_default() += 1;
            b_addrs.push(m.offset);
        }
        b_addrs.sort_unstable();

        // Scanner C (all 3 P0 ROMs unconditionally).
        let t = Instant::now();
        let c_list = axis_anchored_scan(&rom, true, 64);
        let c_ms = t.elapsed().as_secs_f64() * 1000.0;
        scanner_c_total += c_list.len();

        let mut c_by_shape: BTreeMap<String, usize> = BTreeMap::new();
        let mut c_addrs = Vec::with_capacity(c_list.len());
        let mut c_fallback_rows = 0usize;
        for tbl in &c_list {
            *c_by_shape.entry(format!("{}x{}", tbl.rows, tbl.cols)).or_default() += 1;
            c_addrs.push(tbl.address);
            if [4usize, 5, 6, 8, 10, 12, 16].contains(&tbl.rows) {
                c_fallback_rows += 1;
            }
        }
        c_addrs.sort_unstable();

        let b_c_overlap = if run_b {
            let b_spans: Vec<(usize, usize)> = b_list
                .iter()
                .map(|m| (m.offset, m.offset + m.rows * m.cols * 2))
                .collect();
            let c_spans: Vec<(usize, usize)> = c_list
                .iter()
                .map(|t| (t.address, t.address + t.rows * t.cols * t.cell_bytes))
                .collect();
            Some(
                c_spans
                    .iter()
                    .filter(|(cs, ce)| b_spans.iter().any(|(bs, be)| cs.max(bs) < ce.min(be)))
                    .count(),
            )
        } else {
            None
        };

        eprintln!(
            "[P0b] {name}: C={} ({c_ms:.0} ms)  B={} (executed={run_b}, {b_ms:.0} ms)",
            c_list.len(),
            b_list.len()
        );

        rom_reports.push(RomReport {
            name: name.to_string(),
            path: rel.to_string(),
            bytes: rom.len(),
            sha256: rom_sha,
            scanner_a_excluded_reason:
                "excluded from P0/P0b: consumes default_shapes() corpus prior".to_string(),
            scanner_a_default_shapes: default_shapes().len(),
            scanner_b: ScannerSummary {
                executed: run_b,
                scope: b_scope,
                candidates: b_list.len(),
                runtime_ms: b_ms,
                by_shape: b_by_shape,
                addresses_sha256: hash_sorted_addresses(&b_addrs),
            },
            scanner_c: ScannerSummary {
                executed: true,
                scope: "le=true max_gap=64".to_string(),
                candidates: c_list.len(),
                runtime_ms: c_ms,
                by_shape: c_by_shape,
                addresses_sha256: hash_sorted_addresses(&c_addrs),
            },
            scanner_c_rows_in_fallback_list: c_fallback_rows,
            b_c_address_overlap: b_c_overlap,
        });
    }

    let mut totals = BTreeMap::new();
    totals.insert("scanner_b_total".to_string(), scanner_b_total);
    totals.insert("scanner_c_total".to_string(), scanner_c_total);

    let report = P0bOutput {
        wave: "3.3".to_string(),
        phase: "P0b".to_string(),
        note: "discovery only; no reference definition read; CWD-independent provenance checks passed"
            .to_string(),
        git_head,
        git_dirty,
        rustc_version,
        cargo_manifest_dir: manifest_dir.display().to_string(),
        exe_path: exe_path.display().to_string(),
        exe_sha256,
        harness_source_sha256,
        scanner_source_sha256,
        scanner_source_self_check_passed: true,
        full_b58_scanner_b: full_b58_b,
        totals,
        roms: rom_reports,
    };

    let json = serde_json::to_string_pretty(&report).expect("serialize P0b JSON");
    std::fs::write(&out_path, json)
        .unwrap_or_else(|e| panic!("FATAL: failed to write '{out_path}': {e}"));
    println!("[P0b] wrote {out_path} (scanner_c_total={scanner_c_total})");
}
