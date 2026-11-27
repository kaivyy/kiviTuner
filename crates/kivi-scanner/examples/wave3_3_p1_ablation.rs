//! Wave 3.3 P1.2 Ablation (`E_base`, `E0–E3`) + `P1.5` Prior Attribution.
//!
//! Strictly blind: zero reference definitions (`xdf`/`a2l`/`damos`) imported or read.
//! Verifies exact count/address/smoothness parity (`E_base == axis_anchored_scan`)
//! against the frozen production Scanner C before evaluating E0–E3.

#[path = "../../../research/wave-3.3/variants/axis_anchored_ablation.rs"]
mod axis_anchored_ablation;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

use axis_anchored_ablation::{axis_anchored_ablation_scan, AblationConfig, YSource};
use kivi_core::hash::compute_hashes;
use kivi_scanner::axis_anchored::axis_anchored_scan;
use serde::Serialize;

const DB: &str = "/root/ecu_master_database";

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

#[derive(Serialize)]
struct ExpRunResult {
    exp_id: String,
    rom_name: String,
    rom_sha256: String,
    config: AblationConfig,
    candidates: usize,
    runtime_ms: f64,
    by_y_source: BTreeMap<String, usize>,
    by_padding: BTreeMap<usize, usize>,
    by_cell_bytes: BTreeMap<usize, usize>,
}

#[derive(Serialize)]
struct ExpAggregate {
    exp_id: String,
    total_candidates: usize,
    ratio_to_ebase: f64,
    by_y_source_total: BTreeMap<String, usize>,
}

#[derive(Serialize)]
struct P12Report {
    wave: String,
    phase: String,
    production_axis_anchored_sha256: String,
    variant_source_sha256: String,
    ebase_exact_parity_verified: bool,
    ebase_total: usize,
    e0_over_ebase_ratio: f64,
    p15_attribution_summary: String,
    aggregates: Vec<ExpAggregate>,
    runs: Vec<ExpRunResult>,
}

fn summarize_tags(
    tables: &[axis_anchored_ablation::AblationTable],
) -> (
    BTreeMap<String, usize>,
    BTreeMap<usize, usize>,
    BTreeMap<usize, usize>,
) {
    let mut y_src: BTreeMap<String, usize> = BTreeMap::new();
    let mut pad_map: BTreeMap<usize, usize> = BTreeMap::new();
    let mut width_map: BTreeMap<usize, usize> = BTreeMap::new();
    for t in tables {
        let key = match t.y_source {
            YSource::FullAxis => "FullAxis",
            YSource::StepBreak => "StepBreak",
            YSource::FallbackOnly => "FallbackOnly",
            YSource::CoincidentBoth => "CoincidentBoth",
        };
        *y_src.entry(key.to_string()).or_default() += 1;
        *pad_map.entry(t.padding).or_default() += 1;
        *width_map.entry(t.cell_bytes).or_default() += 1;
    }
    (y_src, pad_map, width_map)
}

fn main() {
    let out_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "research/wave-3.3/P1.2-ABLATION-RESULTS.json".to_string());

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let prod_bytes =
        std::fs::read(manifest_dir.join("src/axis_anchored.rs")).expect("read axis_anchored.rs");
    let prod_sha = compute_hashes(&prod_bytes).sha256;
    let var_bytes = std::fs::read(
        manifest_dir.join("../../research/wave-3.3/variants/axis_anchored_ablation.rs"),
    )
    .expect("read axis_anchored_ablation.rs");
    let var_sha = compute_hashes(&var_bytes).sha256;

    let mut exps: Vec<(&str, AblationConfig)> = Vec::new();
    exps.push(("E_base", AblationConfig::baseline()));

    let mut e0 = AblationConfig::baseline();
    e0.use_y_fallback = false;
    exps.push(("E0_no_fallback", e0));

    let mut e1 = AblationConfig::baseline();
    e1.padding_offsets = vec![0];
    exps.push(("E1_no_padding", e1));

    let mut e2 = AblationConfig::baseline();
    e2.use_y_fallback = false;
    e2.padding_offsets = vec![0];
    exps.push(("E2_pure_axis_only", e2));

    for gap in [0usize, 16, 64, 128] {
        let mut with_fb = AblationConfig::baseline();
        with_fb.max_gap = gap;
        exps.push((
            match gap {
                0 => "E3_gap0_fb",
                16 => "E3_gap16_fb",
                64 => "E3_gap64_fb",
                _ => "E3_gap128_fb",
            },
            with_fb,
        ));
        let mut no_fb = AblationConfig::baseline();
        no_fb.max_gap = gap;
        no_fb.use_y_fallback = false;
        exps.push((
            match gap {
                0 => "E3_gap0_nofb",
                16 => "E3_gap16_nofb",
                64 => "E3_gap64_nofb",
                _ => "E3_gap128_nofb",
            },
            no_fb,
        ));
    }

    let mut roms_data = Vec::new();
    let mut prod_total = 0usize;
    for &(name, rel) in ROMS {
        let path = PathBuf::from(DB).join(rel);
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|e| panic!("cannot read '{}': {e}", path.display()));
        let sha = compute_hashes(&bytes).sha256;
        let prod = axis_anchored_scan(&bytes, true, 64);
        prod_total += prod.len();
        roms_data.push((name, bytes, sha, prod));
    }
    assert_eq!(
        prod_total, 1033,
        "Expected historical Scanner C baseline total 1033, got {prod_total}"
    );

    let mut runs = Vec::new();
    let mut ebase_total = 0usize;
    let mut e0_total = 0usize;
    let mut ebase_y_sources: BTreeMap<String, usize> = BTreeMap::new();
    let mut aggregates = Vec::new();

    for (exp_id, cfg) in &exps {
        let mut exp_total = 0usize;
        let mut exp_y_src: BTreeMap<String, usize> = BTreeMap::new();

        for (name, bytes, sha, prod) in &roms_data {
            let t = Instant::now();
            let found = axis_anchored_ablation_scan(bytes, cfg);
            let ms = t.elapsed().as_secs_f64() * 1000.0;

            if *exp_id == "E_base" {
                assert_eq!(
                    found.len(),
                    prod.len(),
                    "PARITY FAILURE on {name}: ablation={} prod={}",
                    found.len(),
                    prod.len()
                );
                for (a, b) in found.iter().zip(prod.iter()) {
                    assert_eq!(a.address, b.address, "{name} addr mismatch");
                    assert_eq!(a.rows, b.rows, "{name} rows mismatch");
                    assert_eq!(a.cols, b.cols, "{name} cols mismatch");
                    assert_eq!(a.cell_bytes, b.cell_bytes, "{name} width mismatch");
                    assert!((a.smoothness - b.smoothness).abs() < 1e-12);
                }
            }

            let (ys, pm, wm) = summarize_tags(&found);
            for (k, v) in &ys {
                *exp_y_src.entry(k.clone()).or_default() += v;
            }
            exp_total += found.len();

            runs.push(ExpRunResult {
                exp_id: (*exp_id).to_string(),
                rom_name: (*name).to_string(),
                rom_sha256: sha.clone(),
                config: cfg.clone(),
                candidates: found.len(),
                runtime_ms: ms,
                by_y_source: ys,
                by_padding: pm,
                by_cell_bytes: wm,
            });
        }

        if *exp_id == "E_base" {
            ebase_total = exp_total;
            ebase_y_sources = exp_y_src.clone();
        }
        if *exp_id == "E0_no_fallback" {
            e0_total = exp_total;
        }

        eprintln!(
            "[P1.2] {exp_id}: total={exp_total} ratio_to_ebase={:.4} y_sources={exp_y_src:?}",
            exp_total as f64 / ebase_total.max(1) as f64
        );

        aggregates.push(ExpAggregate {
            exp_id: (*exp_id).to_string(),
            total_candidates: exp_total,
            ratio_to_ebase: exp_total as f64 / ebase_total.max(1) as f64,
            by_y_source_total: exp_y_src,
        });
    }

    let ratio = e0_total as f64 / ebase_total as f64;
    let fb_only = *ebase_y_sources.get("FallbackOnly").unwrap_or(&0);
    let coinc = *ebase_y_sources.get("CoincidentBoth").unwrap_or(&0);
    let full_or_step =
        *ebase_y_sources.get("FullAxis").unwrap_or(&0) + *ebase_y_sources.get("StepBreak").unwrap_or(&0);
    let summary = format!(
        "E_base={ebase_total} (exact parity), E0(no_fallback)={e0_total} ({:.1}% retained); \
         exact E_base attribution: FallbackOnly={fb_only}, CoincidentBoth={coinc}, PureFullOrStep={full_or_step}",
        ratio * 100.0
    );

    let report = P12Report {
        wave: "3.3".to_string(),
        phase: "P1.2-P1.5".to_string(),
        production_axis_anchored_sha256: prod_sha,
        variant_source_sha256: var_sha,
        ebase_exact_parity_verified: true,
        ebase_total,
        e0_over_ebase_ratio: ratio,
        p15_attribution_summary: summary,
        aggregates,
        runs,
    };

    let json = serde_json::to_string_pretty(&report).expect("serialize P1.2 JSON");
    std::fs::write(&out_path, json).expect("write P1.2 JSON");
    println!("[P1.2] wrote {out_path}");
}
