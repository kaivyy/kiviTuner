use std::collections::BTreeMap;
use std::path::Path;
use super::corpus::*;
use super::format_markdown_report;
use super::ref_parsers::{parse_reference_file, span_is_varying};

pub(crate) fn spans_overlap(a_start: usize, a_len: usize, b_start: usize, b_len: usize) -> bool {
    let a_end = a_start.saturating_add(a_len.max(1));
    let b_end = b_start.saturating_add(b_len.max(1));
    a_start.max(b_start) < a_end.min(b_end)
}

pub(crate) fn pct(hits: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        (hits as f64 / total as f64) * 100.0
    }
}

pub(crate) fn opt_pct(hits: usize, total: usize) -> Option<f64> {
    if total == 0 {
        None
    } else {
        Some((hits as f64 / total as f64) * 100.0)
    }
}

pub(crate) fn evaluate_scanner_on_refs(
    run: &FrozenScannerRun,
    refs_2d: &[&RefItem],
    refs_1d: &[&RefItem],
    all_varying: &[&RefItem],
) -> ScannerEvalMetrics {
    let mut hits_exact = 0usize;
    let mut hits_c2 = 0usize;
    let mut hits_a8 = 0usize;
    let mut hits_ov = 0usize;
    let mut u8_tot = 0usize;
    let mut u8_a8 = 0usize;
    let mut u8_ov = 0usize;
    let mut u16_tot = 0usize;
    let mut u16_a8 = 0usize;
    let mut u16_ov = 0usize;
    let mut ax_tot = 0usize;
    let mut ax_a8 = 0usize;
    let mut af_tot = 0usize;
    let mut af_a8 = 0usize;

    for r in refs_2d {
        let r_span = r.rows * r.cols * r.elem_bytes;
        let m_exact = run.candidates.iter().any(|c| c.address == r.address);
        let m_c2 = run
            .candidates
            .iter()
            .any(|c| c.address.abs_diff(r.address) <= 2 * r.elem_bytes);
        let m_a8 = run
            .candidates
            .iter()
            .any(|c| c.address.abs_diff(r.address) <= 8);
        let m_ov = run.candidates.iter().any(|c| {
            spans_overlap(
                c.address,
                c.rows * c.cols * c.elem_bytes,
                r.address,
                r_span,
            )
        });
        if m_exact {
            hits_exact += 1;
        }
        if m_c2 {
            hits_c2 += 1;
        }
        if m_a8 {
            hits_a8 += 1;
        }
        if m_ov {
            hits_ov += 1;
        }
        if r.elem_bytes == 1 {
            u8_tot += 1;
            if m_a8 {
                u8_a8 += 1;
            }
            if m_ov {
                u8_ov += 1;
            }
        } else if r.elem_bytes == 2 {
            u16_tot += 1;
            if m_a8 {
                u16_a8 += 1;
            }
            if m_ov {
                u16_ov += 1;
            }
        }
        if r.has_ref_axes {
            ax_tot += 1;
            if m_a8 {
                ax_a8 += 1;
            }
        } else {
            af_tot += 1;
            if m_a8 {
                af_a8 += 1;
            }
        }
    }

    let mut hits_1d_a8 = 0usize;
    let mut hits_1d_ov = 0usize;
    for r in refs_1d {
        let r_span = r.rows * r.cols * r.elem_bytes;
        if run
            .candidates
            .iter()
            .any(|c| c.address.abs_diff(r.address) <= 8)
        {
            hits_1d_a8 += 1;
        }
        if run.candidates.iter().any(|c| {
            spans_overlap(
                c.address,
                c.rows * c.cols * c.elem_bytes,
                r.address,
                r_span,
            )
        }) {
            hits_1d_ov += 1;
        }
    }

    let mut matched_cands_abs8 = 0usize;
    let mut matched_cands_overlap = 0usize;
    for c in &run.candidates {
        let c_span = c.rows * c.cols * c.elem_bytes;
        if all_varying
            .iter()
            .any(|r| c.address.abs_diff(r.address) <= 8)
        {
            matched_cands_abs8 += 1;
        }
        if all_varying.iter().any(|r| {
            spans_overlap(
                c.address,
                c_span,
                r.address,
                r.rows * r.cols * r.elem_bytes,
            )
        }) {
            matched_cands_overlap += 1;
        }
    }

    ScannerEvalMetrics {
        scanner: run.scanner.clone(),
        total_candidates: run.candidate_count,
        candidates_per_mib: run.candidates_per_mib,
        runtime_ms: run.runtime_ms,
        ms_per_mib: run.ms_per_mib,
        recall_2d_exact: pct(hits_exact, refs_2d.len()),
        recall_2d_near_cells2: pct(hits_c2, refs_2d.len()),
        recall_2d_near_abs8: pct(hits_a8, refs_2d.len()),
        recall_2d_overlap: pct(hits_ov, refs_2d.len()),
        hits_2d_exact: hits_exact,
        hits_2d_cells2: hits_c2,
        hits_2d_abs8: hits_a8,
        hits_2d_overlap: hits_ov,
        recall_1d_abs8: pct(hits_1d_a8, refs_1d.len()),
        recall_1d_overlap: pct(hits_1d_ov, refs_1d.len()),
        hits_1d_abs8: hits_1d_a8,
        hits_1d_overlap: hits_1d_ov,
        recall_2d_u8_abs8: opt_pct(u8_a8, u8_tot),
        recall_2d_u16_abs8: opt_pct(u16_a8, u16_tot),
        recall_2d_u8_overlap: opt_pct(u8_ov, u8_tot),
        recall_2d_u16_overlap: opt_pct(u16_ov, u16_tot),
        recall_2d_axis_supported_abs8: opt_pct(ax_a8, ax_tot),
        recall_2d_axis_free_abs8: opt_pct(af_a8, af_tot),
        precision_abs8: pct(matched_cands_abs8, run.candidate_count),
        precision_overlap: pct(matched_cands_overlap, run.candidate_count),
        matched_cands_abs8,
        matched_cands_overlap,
    }
}

pub(crate) fn compute_pu_audit(
    frozen_s4: &FrozenRomDiscovery,
    xdf_2d: &[RefItem],
    xdf_1d: &[RefItem],
    dam_2d: &[RefItem],
    dam_1d: &[RefItem],
) -> PuXdfVsDamosAudit {
    let dam_addrs: std::collections::HashSet<usize> = dam_2d.iter().map(|r| r.address).collect();
    let exact_2d = xdf_2d.iter().filter(|r| dam_addrs.contains(&r.address)).count();
    let span_2d = dam_2d
        .iter()
        .filter(|d| {
            let ds = d.rows * d.cols * d.elem_bytes;
            xdf_2d.iter().any(|x| {
                spans_overlap(d.address, ds, x.address, x.rows * x.cols * x.elem_bytes)
            })
        })
        .count();

    let xdf_all: Vec<&RefItem> = xdf_2d.iter().chain(xdf_1d.iter()).collect();
    let dam_all: Vec<&RefItem> = dam_2d.iter().chain(dam_1d.iter()).collect();
    let v2 = &frozen_s4.scanners["blind_v2_2d"];
    let mut hit_xdf = 0usize;
    let mut hit_dam = 0usize;
    let mut hit_union = 0usize;
    let mut rescued = 0usize;
    for c in &v2.candidates {
        let cs = c.rows * c.cols * c.elem_bytes;
        let mx = xdf_all
            .iter()
            .any(|r| spans_overlap(c.address, cs, r.address, r.rows * r.cols * r.elem_bytes));
        let md = dam_all
            .iter()
            .any(|r| spans_overlap(c.address, cs, r.address, r.rows * r.cols * r.elem_bytes));
        if mx {
            hit_xdf += 1;
        }
        if md {
            hit_dam += 1;
        }
        if mx || md {
            hit_union += 1;
        }
        if md && !mx {
            rescued += 1;
        }
    }

    PuXdfVsDamosAudit {
        rom_id: "AUDI_S4_8D0907551M".to_string(),
        xdf_eval_id: "AUDI_S4_8D0907551M_XDF".to_string(),
        damos_eval_id: "AUDI_S4_8D0907551M_DAMOS".to_string(),
        xdf_2d_varying: xdf_2d.len(),
        xdf_1d_varying: xdf_1d.len(),
        damos_2d_varying: dam_2d.len(),
        damos_1d_varying: dam_1d.len(),
        exact_2d_overlap: exact_2d,
        span_2d_overlap: span_2d,
        xdf_label_freq_c_2d_pct: pct(span_2d, dam_2d.len()),
        blind_v2_2d_candidates: v2.candidate_count,
        v2_matched_xdf_overlap: hit_xdf,
        v2_matched_damos_overlap: hit_dam,
        v2_matched_union_overlap: hit_union,
        v2_xdf_apparent_precision_pct: pct(hit_xdf, v2.candidate_count),
        v2_damos_factory_precision_pct: pct(hit_dam, v2.candidate_count),
        v2_union_precision_pct: pct(hit_union, v2.candidate_count),
        v2_xdf_apparent_fp_rescued_by_damos: rescued,
    }
}

/// Phase 2: evaluate ONLY frozen Phase 1 candidates against all 12 reference definitions.
pub(crate) fn run_phase2_evaluate(
    out_dir: &Path,
    entries: &[FrozenRomDiscovery],
    rom_buffers: &BTreeMap<String, Vec<u8>>,
    frozen_sha256: &str,
) {
    let by_rom_id: BTreeMap<&str, &FrozenRomDiscovery> = entries
        .iter()
        .filter(|e| !e.is_negative_control_twin)
        .map(|e| (e.id.as_str(), e))
        .collect();

    let mut twins = Vec::new();
    for t in entries.iter().filter(|e| e.is_negative_control_twin) {
        let mib = t.bytes as f64 / 1_048_576.0;
        let mut fp_counts = BTreeMap::new();
        let mut fp_per_mib = BTreeMap::new();
        for (sname, srun) in &t.scanners {
            fp_counts.insert(sname.clone(), srun.candidate_count);
            fp_per_mib.insert(sname.clone(), srun.candidate_count as f64 / mib);
        }
        twins.push(TwinControlReport {
            twin_id: t.id.clone(),
            source_rom: t.twin_source_rom.clone().unwrap_or_default(),
            bytes: t.bytes,
            verify_a_histogram_equal: true,
            fp_counts,
            fp_per_mib,
        });
    }

    let scanner_names = [
        "scanner_c",
        "blind_automap_legacy",
        "blind_v2_2d",
        "blind_v2_full",
    ];
    let mut evaluations = Vec::new();
    let mut agg_2d_ov: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut agg_2d_a8: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut agg_prec_ov: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut agg_ms: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut pu_xdf_refs: Option<(Vec<RefItem>, Vec<RefItem>)> = None;
    let mut pu_dam_refs: Option<(Vec<RefItem>, Vec<RefItem>)> = None;

    for spec in REFS {
        let rom = &rom_buffers[spec.rom_id];
        let frozen = by_rom_id[spec.rom_id];
        let parsed = parse_reference_file(spec, rom);

        let dedup_2d: Vec<&RefItem> = parsed.dedup_items.iter().filter(|r| r.is_2d).collect();
        let dedup_1d: Vec<&RefItem> = parsed.dedup_items.iter().filter(|r| !r.is_2d).collect();
        let var_2d: Vec<&RefItem> = dedup_2d
            .iter()
            .copied()
            .filter(|r| span_is_varying(rom, r.address, r.rows, r.cols, r.elem_bytes))
            .collect();
        let var_1d: Vec<&RefItem> = dedup_1d
            .iter()
            .copied()
            .filter(|r| span_is_varying(rom, r.address, r.rows, r.cols, r.elem_bytes))
            .collect();
        if spec.eval_id == "AUDI_S4_8D0907551M_XDF" {
            pu_xdf_refs = Some((
                var_2d.iter().map(|&r| r.clone()).collect(),
                var_1d.iter().map(|&r| r.clone()).collect(),
            ));
        } else if spec.eval_id == "AUDI_S4_8D0907551M_DAMOS" {
            pu_dam_refs = Some((
                var_2d.iter().map(|&r| r.clone()).collect(),
                var_1d.iter().map(|&r| r.clone()).collect(),
            ));
        }
        let all_var: Vec<&RefItem> = var_2d.iter().chain(var_1d.iter()).copied().collect();

        let total_dedup = dedup_2d.len() + dedup_1d.len();
        let var_pct = pct(all_var.len(), total_dedup);
        let u8_cnt = var_2d.iter().filter(|r| r.elem_bytes == 1).count();
        let u16_cnt = var_2d.iter().filter(|r| r.elem_bytes == 2).count();
        let ax_cnt = var_2d.iter().filter(|r| r.has_ref_axes).count();
        let af_cnt = var_2d.len() - ax_cnt;

        let mut scanners_eval = BTreeMap::new();
        for sname in scanner_names {
            let run = &frozen.scanners[sname];
            let m = evaluate_scanner_on_refs(run, &var_2d, &var_1d, &all_var);
            agg_2d_ov
                .entry(sname.to_string())
                .or_default()
                .push(m.recall_2d_overlap);
            agg_2d_a8
                .entry(sname.to_string())
                .or_default()
                .push(m.recall_2d_near_abs8);
            agg_prec_ov
                .entry(sname.to_string())
                .or_default()
                .push(m.precision_overlap);
            agg_ms
                .entry(sname.to_string())
                .or_default()
                .push(m.ms_per_mib);
            scanners_eval.insert(sname.to_string(), m);
        }

        eprintln!(
            "[Phase2] {} ({}): var2D={}/{} var1D={}/{} ({:.1}% var) | V2_2D ov={:.1}% (a8={:.1}%) prec={:.1}% vs C ov={:.1}% vs BA ov={:.1}%",
            spec.eval_id,
            parsed.format_name,
            var_2d.len(),
            dedup_2d.len(),
            var_1d.len(),
            dedup_1d.len(),
            var_pct,
            scanners_eval["blind_v2_2d"].recall_2d_overlap,
            scanners_eval["blind_v2_2d"].recall_2d_near_abs8,
            scanners_eval["blind_v2_2d"].precision_overlap,
            scanners_eval["scanner_c"].recall_2d_overlap,
            scanners_eval["blind_automap_legacy"].recall_2d_overlap,
        );

        evaluations.push(RefEvalReport {
            eval_id: spec.eval_id.to_string(),
            rom_id: spec.rom_id.to_string(),
            format: parsed.format_name.to_string(),
            ref_path: spec.rel_path.to_string(),
            disclosure: spec.disclosure.to_string(),
            translator_note: parsed.translator_note,
            rom_bytes: rom.len(),
            raw_ref_entries: parsed.raw_items,
            ram_excluded: parsed.ram_excluded,
            unmapped_excluded: parsed.unmapped_excluded,
            oob_excluded: parsed.oob_excluded,
            dedup_in_bounds_2d: dedup_2d.len(),
            dedup_in_bounds_1d: dedup_1d.len(),
            ref_2d_varying: var_2d.len(),
            ref_1d_varying: var_1d.len(),
            ref_varying_pct: var_pct,
            ref_2d_u8_varying: u8_cnt,
            ref_2d_u16_varying: u16_cnt,
            ref_2d_axis_supported_varying: ax_cnt,
            ref_2d_axis_free_varying: af_cnt,
            scanners: scanners_eval,
        });
    }

    let mean = |xs: &[f64]| -> f64 {
        if xs.is_empty() {
            0.0
        } else {
            xs.iter().sum::<f64>() / xs.len() as f64
        }
    };
    let mut macro_averages = BTreeMap::new();
    for sname in scanner_names {
        let mut inner = BTreeMap::new();
        inner.insert(
            "mean_2d_overlap_recall_pct".to_string(),
            mean(&agg_2d_ov[sname]),
        );
        inner.insert(
            "mean_2d_near_abs8_recall_pct".to_string(),
            mean(&agg_2d_a8[sname]),
        );
        inner.insert(
            "mean_precision_overlap_pct".to_string(),
            mean(&agg_prec_ov[sname]),
        );
        inner.insert("mean_ms_per_mib".to_string(), mean(&agg_ms[sname]));
        macro_averages.insert(sname.to_string(), inner);
    }

    let pu_xdf_vs_damos_audit = match (pu_xdf_refs, pu_dam_refs) {
        (Some((x2, x1)), Some((d2, d1))) => Some(compute_pu_audit(
            by_rom_id["AUDI_S4_8D0907551M"],
            &x2,
            &x1,
            &d2,
            &d1,
        )),
        _ => None,
    };

    let results = BenchmarkResults {
        wave: "Wave 3.3".to_string(),
        phase: "Blind V2 vs Scanner C & Legacy Blind Automap — 10-ROM / 12-Reference Benchmark".to_string(),
        frozen_candidates_path: "research/wave-3.3/BLIND-BENCHMARK-FROZEN-CANDIDATES.json".to_string(),
        frozen_candidates_sha256: frozen_sha256.to_string(),
        twins,
        evaluations,
        macro_averages,
        pu_xdf_vs_damos_audit,
    };

    let res_json_path = out_dir.join("WAVE-3.3-BLIND-BENCHMARK-RESULTS.json");
    let res_bytes = serde_json::to_vec_pretty(&results).expect("serialize results");
    std::fs::write(&res_json_path, &res_bytes).expect("write benchmark results json");

    let md = format_markdown_report(&results);
    let md_path = out_dir.join("WAVE-3.3-BLIND-BENCHMARK-REPORT.md");
    std::fs::write(&md_path, md).expect("write benchmark markdown report");
    eprintln!(
        "[Phase2] Wrote {} & {}",
        res_json_path.display(),
        md_path.display()
    );
}

