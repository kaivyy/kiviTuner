use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;
use kivi_core::hash::compute_hashes;
use kivi_scanner::axis_anchored::axis_anchored_scan_auto;
use kivi_scanner::blind_automap::{blind_automap, BlindOptions};
use kivi_scanner::blind_v2::{attach_curves_v2, blind_discover_v2, BlindV2Options};
use super::corpus::*;

pub(crate) struct XorShift64(u64);
impl XorShift64 {
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

pub(crate) fn byte_histogram(data: &[u8]) -> [usize; 256] {
    let mut h = [0usize; 256];
    for &b in data {
        h[b as usize] += 1;
    }
    h
}

pub(crate) fn fisher_yates_twin(src: &[u8], seed: u64) -> Vec<u8> {
    let mut out = src.to_vec();
    let mut rng = XorShift64(seed);
    for i in (1..out.len()).rev() {
        let j = (rng.next_u64() as usize) % (i + 1);
        out.swap(i, j);
    }
    out
}

pub(crate) fn run_all_four_scanners(rom_id: &str, rom: &[u8]) -> BTreeMap<String, FrozenScannerRun> {
    let mib = rom.len() as f64 / 1_048_576.0;
    let both_16 = rom.len() <= 2_097_152;
    let mut map = BTreeMap::new();

    // 1. Scanner C (frozen axis_anchored_scan_auto)
    let t0 = Instant::now();
    let (c_res, _be) = axis_anchored_scan_auto(rom);
    let c_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let c_cands: Vec<FrozenCandidate> = c_res
        .into_iter()
        .map(|t| FrozenCandidate {
            address: t.address,
            rows: t.rows,
            cols: t.cols,
            elem_bytes: t.cell_bytes,
            little_endian: t.little_endian,
            is_curve: false,
            source: "scanner_c".to_string(),
        })
        .collect();
    eprintln!(
        "[Phase1-step] {rom_id} scanner_c: n={} ({c_ms:.0}ms)",
        c_cands.len()
    );
    map.insert(
        "scanner_c".to_string(),
        FrozenScannerRun {
            scanner: "scanner_c".to_string(),
            runtime_ms: c_ms,
            ms_per_mib: c_ms / mib,
            candidate_count: c_cands.len(),
            candidates_per_mib: c_cands.len() as f64 / mib,
            candidates: c_cands,
        },
    );

    // 2. Legacy blind_automap
    let t1 = Instant::now();
    let ba_res = blind_automap(
        rom,
        &BlindOptions {
            include_8bit: true,
            include_curves: false,
            ..BlindOptions::default()
        },
    );
    let ba_ms = t1.elapsed().as_secs_f64() * 1000.0;
    let ba_cands: Vec<FrozenCandidate> = ba_res
        .into_iter()
        .map(|t| FrozenCandidate {
            address: t.address,
            rows: t.rows,
            cols: t.cols,
            elem_bytes: (t.cell_bits / 8).max(1) as usize,
            little_endian: t.little_endian,
            is_curve: t.rows == 1 || t.cols == 1,
            source: "blind_automap_legacy".to_string(),
        })
        .collect();
    eprintln!(
        "[Phase1-step] {rom_id} blind_automap_legacy: n={} ({ba_ms:.0}ms)",
        ba_cands.len()
    );
    map.insert(
        "blind_automap_legacy".to_string(),
        FrozenScannerRun {
            scanner: "blind_automap_legacy".to_string(),
            runtime_ms: ba_ms,
            ms_per_mib: ba_ms / mib,
            candidate_count: ba_cands.len(),
            candidates_per_mib: ba_cands.len() as f64 / mib,
            candidates: ba_cands,
        },
    );

    // 3. Blind V2 (2-D default: include_curves = false)
    let to_frozen = |t: &kivi_scanner::blind_v2::BlindV2Candidate| FrozenCandidate {
        address: t.address,
        rows: t.rows,
        cols: t.cols,
        elem_bytes: t.elem_bytes,
        little_endian: t.little_endian,
        is_curve: t.is_curve,
        source: t.detection_source.clone(),
    };

    let t2 = Instant::now();
    let v2_res = blind_discover_v2(
        rom,
        &BlindV2Options {
            include_curves: false,
            scan_both_16bit_endians: both_16,
            ..BlindV2Options::default()
        },
    );
    let v2_ms = t2.elapsed().as_secs_f64() * 1000.0;
    let v2_cands: Vec<FrozenCandidate> = v2_res.iter().map(to_frozen).collect();
    eprintln!(
        "[Phase1-step] {rom_id} blind_v2_2d: n={} ({v2_ms:.0}ms)",
        v2_cands.len()
    );
    map.insert(
        "blind_v2_2d".to_string(),
        FrozenScannerRun {
            scanner: "blind_v2_2d".to_string(),
            runtime_ms: v2_ms,
            ms_per_mib: v2_ms / mib,
            candidate_count: v2_cands.len(),
            candidates_per_mib: v2_cands.len() as f64 / mib,
            candidates: v2_cands,
        },
    );

    // 4. Blind V2 Full (reuses v2_res 2-D spans + incremental curve attachment)
    let t3 = Instant::now();
    let attached = attach_curves_v2(rom, &v2_res, true);
    let curves_ms = t3.elapsed().as_secs_f64() * 1000.0;
    let v2f_ms = v2_ms + curves_ms;
    let mut v2f_combined = v2_res;
    v2f_combined.extend(attached);
    v2f_combined.sort_by_key(|t| t.address);
    let v2f_cands: Vec<FrozenCandidate> = v2f_combined.iter().map(to_frozen).collect();
    eprintln!(
        "[Phase1-step] {rom_id} blind_v2_full: n={} (+{curves_ms:.0}ms curves, {v2f_ms:.0}ms total)",
        v2f_cands.len()
    );
    map.insert(
        "blind_v2_full".to_string(),
        FrozenScannerRun {
            scanner: "blind_v2_full".to_string(),
            runtime_ms: v2f_ms,
            ms_per_mib: v2f_ms / mib,
            candidate_count: v2f_cands.len(),
            candidates_per_mib: v2f_cands.len() as f64 / mib,
            candidates: v2f_cands,
        },
    );

    map
}

/// Phase 1: blind discovery & freeze on raw BINs + Fisher–Yates twins only.
pub(crate) fn run_phase1_freeze(
    out_dir: &Path,
) -> (Vec<FrozenRomDiscovery>, BTreeMap<String, Vec<u8>>, String) {
    let mut rom_buffers: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut entries: Vec<FrozenRomDiscovery> = Vec::new();

    for spec in ROMS {
        let full = PathBuf::from(DB).join(spec.rel_path);
        let rom = std::fs::read(&full)
            .unwrap_or_else(|e| panic!("FATAL: cannot read ROM '{}': {e}", full.display()));
        assert!(!rom.is_empty(), "FATAL: empty ROM '{}'", full.display());
        let sha256 = compute_hashes(&rom).sha256;
        let scanners = run_all_four_scanners(spec.id, &rom);
        eprintln!(
            "[Phase1] {} ({:.2} MiB): C={} BA={} V2_2D={} V2_Full={} ({:.0}ms)",
            spec.id,
            rom.len() as f64 / 1_048_576.0,
            scanners["scanner_c"].candidate_count,
            scanners["blind_automap_legacy"].candidate_count,
            scanners["blind_v2_2d"].candidate_count,
            scanners["blind_v2_full"].candidate_count,
            scanners["blind_v2_2d"].runtime_ms,
        );
        entries.push(FrozenRomDiscovery {
            id: spec.id.to_string(),
            family: spec.family.to_string(),
            path: spec.rel_path.to_string(),
            is_negative_control_twin: false,
            twin_source_rom: None,
            bytes: rom.len(),
            sha256,
            scanners,
        });
        rom_buffers.insert(spec.id.to_string(), rom);
    }

    // Two real-ROM Fisher-Yates negative-control twins (Verify A enforced).
    let twin_specs = [
        ("TWIN_AUDI_RS4", "AUDI_RS4_8D0907551F", 0xA10D_551F_0001_0001u64),
        ("TWIN_BMW_MS43", "BMW_MS43_512K_RHD", 0xB343_0069_0002_0002u64),
    ];
    for (tid, src_id, seed) in twin_specs {
        let src = &rom_buffers[src_id];
        let twin = fisher_yates_twin(src, seed);
        assert_eq!(
            byte_histogram(src),
            byte_histogram(&twin),
            "FATAL: Verify A histogram check failed on twin {tid}"
        );
        let sha256 = compute_hashes(&twin).sha256;
        let scanners = run_all_four_scanners(tid, &twin);
        eprintln!(
            "[Phase1 Twin] {tid}: C={} BA={} V2_2D={} V2_Full={}",
            scanners["scanner_c"].candidate_count,
            scanners["blind_automap_legacy"].candidate_count,
            scanners["blind_v2_2d"].candidate_count,
            scanners["blind_v2_full"].candidate_count,
        );
        entries.push(FrozenRomDiscovery {
            id: tid.to_string(),
            family: "FisherYates_Control".to_string(),
            path: format!("synthetic_twin:{src_id}:0x{seed:X}"),
            is_negative_control_twin: true,
            twin_source_rom: Some(src_id.to_string()),
            bytes: twin.len(),
            sha256,
            scanners,
        });
    }

    let envelope = FrozenFileEnvelope {
        wave: "Wave 3.3".to_string(),
        phase: "Phase 1 Frozen Blind Candidates (Zero-XDF/A2L/DAMOS Contact)".to_string(),
        protocol: "Strict Two-Phase Discovery-then-Freeze: all candidates serialized and SHA-256 frozen before reading reference definitions".to_string(),
        entries: entries.clone(),
    };
    let frozen_path = out_dir.join("BLIND-BENCHMARK-FROZEN-CANDIDATES.json");
    let json_bytes = serde_json::to_vec_pretty(&envelope).expect("serialize frozen envelope");
    let mut f = std::fs::File::create(&frozen_path)
        .unwrap_or_else(|e| panic!("FATAL: create '{}': {e}", frozen_path.display()));
    f.write_all(&json_bytes).expect("write frozen envelope");
    f.flush().expect("flush frozen envelope");
    drop(f);

    let readback = std::fs::read(&frozen_path).expect("read back frozen JSON");
    let frozen_sha256 = compute_hashes(&readback).sha256;
    eprintln!(
        "[Phase1] Frozen candidates written & hashed: {} (sha256={frozen_sha256})",
        frozen_path.display()
    );

    (entries, rom_buffers, frozen_sha256)
}

