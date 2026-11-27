//! Wave 3.3 Strict Two-Phase 10-ROM Blind Discovery Benchmark.
//!
//! Enforces strict isolation between discovery and reference verification:
//! - **Phase 1 (`run_phase1_freeze`):** loads ONLY raw `.bin`/`.ori` bytes (plus 2
//!   deterministic Fisher–Yates real-ROM negative-control twins with Verify A
//!   256-bin histogram equality), runs `scanner_c`, `blind_automap_legacy`,
//!   `blind_v2_2d`, and `blind_v2_full`, writes
//!   `research/wave-3.3/BLIND-BENCHMARK-FROZEN-CANDIDATES.json`, and hashes it
//!   with SHA-256 BEFORE opening any XDF, A2L, or DAMOS file.
//! - **Phase 2 (`run_phase2_evaluate`):** evaluates ONLY against the frozen JSON
//!   across all 12 reference definitions (8 XDF, 3 A2L, 1 DAMOS), computes
//!   stratified recall/precision/runtime/FP metrics, and writes
//!   `research/wave-3.3/WAVE-3.3-BLIND-BENCHMARK-RESULTS.json` and
//!   `research/wave-3.3/WAVE-3.3-BLIND-BENCHMARK-REPORT.md`.
//!
//! Run:
//!   `cargo run --release -p kivi-scanner --example wave3_3_blind_benchmark`

use std::path::PathBuf;

mod corpus;
mod phase1_freeze;
mod phase2_eval;
mod ref_parsers;

use corpus::*;
use phase1_freeze::run_phase1_freeze;
use phase2_eval::run_phase2_evaluate;

pub(crate) fn format_opt_pct(v: Option<f64>) -> String {
    match v {
        Some(x) => format!("{x:.1}%"),
        None => "—".to_string(),
    }
}

pub(crate) fn format_markdown_report(r: &BenchmarkResults) -> String {
    let mut s = String::new();
    s.push_str("# Wave 3.3 Blind Benchmark Report (10 ROMs, 12 XDF/A2L/DAMOS References)\n\n");
    s.push_str(&format!(
        "- **Protocol:** Strict Two-Phase Discovery-then-Freeze\n- **Frozen Candidates File:** `{}`\n- **Frozen Candidates SHA-256:** `{}`\n\n",
        r.frozen_candidates_path, r.frozen_candidates_sha256
    ));

    s.push_str("## 1. Macro Averages Across 12 Reference Evaluations\n\n");
    s.push_str("| Scanner | Mean 2D Span-Overlap Recall | Mean 2D Abs<=8B Recall | Mean Span-Overlap Precision | Mean Runtime (ms/MiB) |\n");
    s.push_str("|---|---:|---:|---:|---:|\n");
    for sname in ["scanner_c", "blind_automap_legacy", "blind_v2_2d", "blind_v2_full"] {
        let m = &r.macro_averages[sname];
        s.push_str(&format!(
            "| `{sname}` | {:.1}% | {:.1}% | {:.1}% | {:.0} |\n",
            m["mean_2d_overlap_recall_pct"],
            m["mean_2d_near_abs8_recall_pct"],
            m["mean_precision_overlap_pct"],
            m["mean_ms_per_mib"],
        ));
    }

    s.push_str("\n## 2. Per-Reference 2-D Discovery Comparison\n\n");
    s.push_str("| Eval ID | Format | 2D Var / InB | 1D Var | Var % | C Ov / <=8B | BA Ov / <=8B | V2-2D Ov / <=8B / Cells<=2 / Exact | V2-2D Prec (Ov / <=8B) | V2-Full 1D Ov / Prec |\n");
    s.push_str("|---|---|---:|---:|---:|---|---|---|---:|---|\n");
    for e in &r.evaluations {
        let c = &e.scanners["scanner_c"];
        let ba = &e.scanners["blind_automap_legacy"];
        let v2 = &e.scanners["blind_v2_2d"];
        let v2f = &e.scanners["blind_v2_full"];
        s.push_str(&format!(
            "| `{}` | {} | {}/{} | {} | {:.1}% | {:.1}% / {:.1}% | {:.1}% / {:.1}% | {:.1}% / {:.1}% / {:.1}% / {:.1}% | {:.1}% / {:.1}% | {:.1}% / {:.1}% |\n",
            e.eval_id,
            e.format,
            e.ref_2d_varying,
            e.dedup_in_bounds_2d,
            e.ref_1d_varying,
            e.ref_varying_pct,
            c.recall_2d_overlap,
            c.recall_2d_near_abs8,
            ba.recall_2d_overlap,
            ba.recall_2d_near_abs8,
            v2.recall_2d_overlap,
            v2.recall_2d_near_abs8,
            v2.recall_2d_near_cells2,
            v2.recall_2d_exact,
            v2.precision_overlap,
            v2.precision_abs8,
            v2f.recall_1d_overlap,
            v2f.precision_overlap,
        ));
    }

    s.push_str("\n## 3. Stratified Breakdown for `blind_v2_2d` (u8 vs u16, Axis-Supported vs Axis-Free)\n\n");
    s.push_str("| Eval ID | u8 Var | u16 Var | V2 u8 Ov / <=8B | V2 u16 Ov / <=8B | Ax-Supp Var | Ax-Free Var | V2 Ax-Supp <=8B | V2 Ax-Free <=8B | Cands / MiB | ms / MiB |\n");
    s.push_str("|---|---:|---:|---|---|---:|---:|---|---|---:|---:|\n");
    for e in &r.evaluations {
        let v2 = &e.scanners["blind_v2_2d"];
        s.push_str(&format!(
            "| `{}` | {} | {} | {} / {} | {} / {} | {} | {} | {} | {} | {:.1} | {:.0} |\n",
            e.eval_id,
            e.ref_2d_u8_varying,
            e.ref_2d_u16_varying,
            format_opt_pct(v2.recall_2d_u8_overlap),
            format_opt_pct(v2.recall_2d_u8_abs8),
            format_opt_pct(v2.recall_2d_u16_overlap),
            format_opt_pct(v2.recall_2d_u16_abs8),
            e.ref_2d_axis_supported_varying,
            e.ref_2d_axis_free_varying,
            format_opt_pct(v2.recall_2d_axis_supported_abs8),
            format_opt_pct(v2.recall_2d_axis_free_abs8),
            v2.candidates_per_mib,
            v2.ms_per_mib,
        ));
    }

    s.push_str("\n## 4. Real-ROM Fisher–Yates Negative-Control Twins (Verify A Passed)\n\n");
    s.push_str("| Twin ID | Source ROM | Bytes | Verify A | `scanner_c` FP (/MiB) | `blind_automap_legacy` FP (/MiB) | `blind_v2_2d` FP (/MiB) | `blind_v2_full` FP (/MiB) |\n");
    s.push_str("|---|---|---:|---|---|---|---|---|\n");
    for t in &r.twins {
        s.push_str(&format!(
            "| `{}` | `{}` | {} | {} | {} ({:.2}) | {} ({:.2}) | {} ({:.2}) | {} ({:.2}) |\n",
            t.twin_id,
            t.source_rom,
            t.bytes,
            t.verify_a_histogram_equal,
            t.fp_counts["scanner_c"],
            t.fp_per_mib["scanner_c"],
            t.fp_counts["blind_automap_legacy"],
            t.fp_per_mib["blind_automap_legacy"],
            t.fp_counts["blind_v2_2d"],
            t.fp_per_mib["blind_v2_2d"],
            t.fp_counts["blind_v2_full"],
            t.fp_per_mib["blind_v2_full"],
        ));
    }

    s.push_str("\n## 5. Reference Translation & Compatibility Disclosures\n\n");
    s.push_str("| Eval ID | Raw Entries | RAM Excl | Unmapped Excl | OOB Excl | Translator / Disclosure |\n");
    s.push_str("|---|---:|---:|---:|---:|---|\n");
    for e in &r.evaluations {
        s.push_str(&format!(
            "| `{}` | {} | {} | {} | {} | {} {} |\n",
            e.eval_id,
            e.raw_ref_entries,
            e.ram_excluded,
            e.unmapped_excluded,
            e.oob_excluded,
            e.translator_note,
            if e.disclosure.is_empty() {
                String::new()
            } else {
                format!("— {}", e.disclosure)
            }
        ));
    }

    if let Some(pu) = &r.pu_xdf_vs_damos_audit {
        s.push_str("\n## 6. Empirical Positive-Unlabeled (PU) XDF Completeness Audit (`AUDI_S4_8D0907551M`)\n\n");
        s.push_str(&format!(
            "Comparing community XDF (`{}`) against factory Bosch DAMOS flash `/SPZ` records (`{}`) on the exact same frozen `blind_v2_2d` candidates of `{}` (`{}` candidates):\n\n",
            pu.xdf_eval_id, pu.damos_eval_id, pu.rom_id, pu.blind_v2_2d_candidates
        ));
        s.push_str("| Metric | Value |\n|---|---:|\n");
        s.push_str(&format!("| Community XDF Varying (`2D` / `1D`) | {} / {} |\n", pu.xdf_2d_varying, pu.xdf_1d_varying));
        s.push_str(&format!("| Factory DAMOS Flash Varying (`2D` / `1D`) | {} / {} |\n", pu.damos_2d_varying, pu.damos_1d_varying));
        s.push_str(&format!("| Exact 2-D Address Overlap (`XDF ∩ DAMOS`) | {} |\n", pu.exact_2d_overlap));
        s.push_str(&format!("| Span-Overlap 2-D Coverage (`c_2D = |XDF∩DAMOS|_span / |DAMOS|`) | {}/{} ({:.1}%) |\n", pu.span_2d_overlap, pu.damos_2d_varying, pu.xdf_label_freq_c_2d_pct));
        s.push_str(&format!("| `blind_v2_2d` Matched by XDF (`apparent precision`) | {}/{} ({:.1}%) |\n", pu.v2_matched_xdf_overlap, pu.blind_v2_2d_candidates, pu.v2_xdf_apparent_precision_pct));
        s.push_str(&format!("| `blind_v2_2d` Matched by DAMOS (`factory precision`) | {}/{} ({:.1}%) |\n", pu.v2_matched_damos_overlap, pu.blind_v2_2d_candidates, pu.v2_damos_factory_precision_pct));
        s.push_str(&format!("| `blind_v2_2d` Matched by `XDF ∪ DAMOS` | {}/{} ({:.1}%) |\n", pu.v2_matched_union_overlap, pu.blind_v2_2d_candidates, pu.v2_union_precision_pct));
        s.push_str(&format!("| Apparent XDF False Positives Confirmed Real in DAMOS | {} |\n", pu.v2_xdf_apparent_fp_rescued_by_damos));
    }
    s
}

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("CARGO_MANIFEST_DIR must be crates/kivi-scanner");
    let out_dir = repo_root.join("research/wave-3.3");
    std::fs::create_dir_all(&out_dir).expect("create research/wave-3.3 dir");

    let (entries, rom_buffers, frozen_sha256) = run_phase1_freeze(&out_dir);
    run_phase2_evaluate(&out_dir, &entries, &rom_buffers, &frozen_sha256);
}
