//! Live 647-pair corpus audit using the latest `blind_discover_v2` scanner.
//!
//! Splits pairs into Corpus A (`DATA_CONSISTENT`, 312) and Corpus B
//! (`REFERENCE_UNVERIFIED`, 335), deduplicates unique BIN SHA-256 scans in
//! parallel, and reports strict 2-D/1-D attribution (`+-16B` & `>=50%` overlap),
//! precision against XDF oracle, deduplicated unique-oracle metrics, and brand breakdown.

use kivi_scanner::blind_v2::{blind_discover_v2, BlindV2Candidate, BlindV2Options};
use kivi_scanner::xdf_parse::parse_xdf_xml;
use rayon::prelude::*;
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Deserialize)]
struct ManifestPair {
    bin: String,
    bin_sha256: String,
    def: String,
    def_sha256: String,
}

#[derive(Deserialize)]
struct VerifiedManifest {
    corpus_root: String,
    #[serde(rename = "A_primary_verified")]
    corpus_a: Vec<ManifestPair>,
    #[serde(rename = "B_secondary_probable")]
    corpus_b: Vec<ManifestPair>,
}

struct XdfTableSpec {
    address: usize,
    rows: usize,
    cols: usize,
    byte_len: usize,
    is_2d: bool,
}

#[derive(Default, Clone)]
struct BucketStats {
    pairs: usize,
    pairs_with_tables: usize,
    t2d: usize,
    hit_2d_16: usize,
    hit_2d_ov50: usize,
    t1d: usize,
    hit_1d_16: usize,
    hit_1d_ov50: usize,
    cands_sum: usize,
    perfect_pairs: usize,
}

impl BucketStats {
    fn add(&mut self, o: &Self) {
        self.pairs += o.pairs;
        self.pairs_with_tables += o.pairs_with_tables;
        self.t2d += o.t2d;
        self.hit_2d_16 += o.hit_2d_16;
        self.hit_2d_ov50 += o.hit_2d_ov50;
        self.t1d += o.t1d;
        self.hit_1d_16 += o.hit_1d_16;
        self.hit_1d_ov50 += o.hit_1d_ov50;
        self.cands_sum += o.cands_sum;
        self.perfect_pairs += o.perfect_pairs;
    }
}

struct ScannedRom {
    rom_len: usize,
    cands: Vec<BlindV2Candidate>,
    c2d: Vec<(usize, usize)>,
    c1d: Vec<(usize, usize)>,
    checksum_sum: u64,
    elapsed_ms: f64,
}

fn brand_from_rel(rel: &str) -> String {
    let mut parts = rel.split('/');
    let _cat = parts.next();
    parts.next().unwrap_or("OTHER").to_string()
}

fn parse_pair_tables(xdf_path: &Path, rom_len: usize) -> Vec<XdfTableSpec> {
    let Ok(raw) = std::fs::read(xdf_path) else {
        return Vec::new();
    };
    let Ok(parsed) = parse_xdf_xml(&String::from_utf8_lossy(&raw)) else {
        return Vec::new();
    };
    parsed
        .tables
        .iter()
        .filter_map(|t| {
            let rows = t.y_axis.as_ref().map_or(1, |a| a.index_count).max(1);
            let cols = t.x_axis.as_ref().map_or(1, |a| a.index_count).max(1);
            if rows < 2 && cols < 2 {
                return None;
            }
            let bpp = (t.z_axis.element_size_bits / 8).clamp(1, 4);
            let byte_len = rows.saturating_mul(cols).saturating_mul(bpp);
            let addr = t.z_axis.address;
            if addr.saturating_add(byte_len) > rom_len {
                return None;
            }
            Some(XdfTableSpec {
                address: addr,
                rows,
                cols,
                byte_len,
                is_2d: rows >= 2 && cols >= 2,
            })
        })
        .collect()
}

fn matches_interval(addr: usize, byte_len: usize, intervals: &[(usize, usize)]) -> (bool, bool) {
    let end = addr.saturating_add(byte_len);
    let mut hit_16 = false;
    let mut hit_ov50 = false;
    for &(cs, ce) in intervals {
        if cs.abs_diff(addr) <= 16 {
            hit_16 = true;
        }
        let ov = ce.min(end).saturating_sub(cs.max(addr));
        if ov.saturating_mul(2) >= byte_len.max(1) {
            hit_ov50 = true;
        }
        if hit_16 && hit_ov50 {
            break;
        }
    }
    (hit_16, hit_ov50)
}

fn pct(num: usize, den: usize) -> f64 {
    if den == 0 {
        0.0
    } else {
        100.0 * (num as f64) / (den as f64)
    }
}

fn eval_pairs(
    corpus_root: &Path,
    pairs: &[ManifestPair],
    scanned: &HashMap<String, ScannedRom>,
    dedup_unique_a: bool,
) -> (BucketStats, BTreeMap<String, BucketStats>, BucketStats) {
    let mut total = BucketStats::default();
    let mut by_brand: BTreeMap<String, BucketStats> = BTreeMap::new();
    let mut unique_stats = BucketStats::default();
    let mut seen_oracle: HashSet<(String, usize, usize, usize)> = HashSet::new();

    for p in pairs {
        total.pairs += 1;
        let brand = brand_from_rel(&p.bin);
        let bstat = by_brand.entry(brand).or_default();
        bstat.pairs += 1;

        let Some(rom_info) = scanned.get(&p.bin_sha256) else {
            continue;
        };
        let specs = parse_pair_tables(&corpus_root.join(&p.def), rom_info.rom_len);
        if specs.is_empty() {
            continue;
        }
        total.pairs_with_tables += 1;
        bstat.pairs_with_tables += 1;
        total.cands_sum += rom_info.cands.len();
        bstat.cands_sum += rom_info.cands.len();

        let mut pair_matched_all = 0usize;
        for t in &specs {
            if t.is_2d {
                let (h16, hov) = matches_interval(t.address, t.byte_len, &rom_info.c2d);
                total.t2d += 1;
                bstat.t2d += 1;
                if h16 {
                    total.hit_2d_16 += 1;
                    bstat.hit_2d_16 += 1;
                }
                if hov {
                    total.hit_2d_ov50 += 1;
                    bstat.hit_2d_ov50 += 1;
                }
                if h16 || hov {
                    pair_matched_all += 1;
                }
                if dedup_unique_a && seen_oracle.insert((p.def_sha256.clone(), t.address, t.rows, t.cols)) {
                    unique_stats.t2d += 1;
                    if h16 {
                        unique_stats.hit_2d_16 += 1;
                    }
                    if hov {
                        unique_stats.hit_2d_ov50 += 1;
                    }
                }
            } else {
                let (h16, hov) = matches_interval(t.address, t.byte_len, &rom_info.c1d);
                total.t1d += 1;
                bstat.t1d += 1;
                if h16 {
                    total.hit_1d_16 += 1;
                    bstat.hit_1d_16 += 1;
                }
                if hov {
                    total.hit_1d_ov50 += 1;
                    bstat.hit_1d_ov50 += 1;
                }
                if h16 || hov {
                    pair_matched_all += 1;
                }
                if dedup_unique_a && seen_oracle.insert((p.def_sha256.clone(), t.address, t.rows, t.cols)) {
                    unique_stats.t1d += 1;
                    if h16 {
                        unique_stats.hit_1d_16 += 1;
                    }
                    if hov {
                        unique_stats.hit_1d_ov50 += 1;
                    }
                }
            }
        }
        if pair_matched_all == specs.len() {
            total.perfect_pairs += 1;
            bstat.perfect_pairs += 1;
        }
    }

    (total, by_brand, unique_stats)
}

fn print_summary_row(label: &str, s: &BucketStats) {
    let tot_tbl = s.t2d + s.t1d;
    let tot_hit16 = s.hit_2d_16 + s.hit_1d_16;
    println!(
        "{:<18} {:>4}/{:<4} {:>8} {:>7}/{:<7}({:>4.1}%) {:>7}/{:<7}({:>4.1}%) {:>7}/{:<7}({:>4.1}%) {:>7}/{:<7}({:>4.1}%) {:>6}",
        label,
        s.pairs_with_tables,
        s.pairs,
        s.cands_sum,
        s.hit_2d_16,
        s.t2d,
        pct(s.hit_2d_16, s.t2d),
        s.hit_2d_ov50,
        s.t2d,
        pct(s.hit_2d_ov50, s.t2d),
        s.hit_1d_16,
        s.t1d,
        pct(s.hit_1d_16, s.t1d),
        tot_hit16,
        tot_tbl,
        pct(tot_hit16, tot_tbl),
        s.perfect_pairs,
    );
}

fn main() {
    let manifest_path = Path::new("/root/kiviTuner/docs/migration/verified_corpus_2026-10-30.json");
    let raw = std::fs::read_to_string(manifest_path).expect("read verified_corpus_2026-10-30.json");
    let manifest: VerifiedManifest = serde_json::from_str(&raw).expect("parse manifest json");
    let root = PathBuf::from(&manifest.corpus_root);

    let mut unique_bins: HashMap<String, PathBuf> = HashMap::new();
    for p in manifest.corpus_a.iter().chain(manifest.corpus_b.iter()) {
        unique_bins
            .entry(p.bin_sha256.clone())
            .or_insert_with(|| root.join(&p.bin));
    }

    let opts = BlindV2Options {
        include_curves: true,
        ..BlindV2Options::default()
    };

    let jobs: Vec<(String, PathBuf)> = unique_bins.into_iter().collect();
    let t_scan = Instant::now();
    let results: Vec<(String, PathBuf, ScannedRom)> = jobs
        .into_par_iter()
        .filter_map(|(sha, path)| {
            let rom = std::fs::read(&path).ok()?;
            let checksum_sum = rom.iter().map(|&b| b as u64).sum::<u64>();
            let t0 = Instant::now();
            let cands = blind_discover_v2(&rom, &opts);
            let elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
            let c2d = cands
                .iter()
                .filter(|c| !c.is_curve && c.rows >= 2 && c.cols >= 2)
                .map(|c| (c.address, c.address + c.rows * c.cols * c.elem_bytes))
                .collect();
            let c1d = cands
                .iter()
                .filter(|c| c.is_curve)
                .map(|c| (c.address, c.address + c.rows * c.cols * c.elem_bytes))
                .collect();
            Some((
                sha,
                path,
                ScannedRom {
                    rom_len: rom.len(),
                    cands,
                    c2d,
                    c1d,
                    checksum_sum,
                    elapsed_ms,
                },
            ))
        })
        .collect();
    let wall_ms = t_scan.elapsed().as_secs_f64() * 1000.0;

    let mut total_bytes = 0usize;
    let mut cpu_ms = 0.0f64;
    let mut unique_cands = 0usize;
    let mut unique_cands_2d = 0usize;
    let mut unique_cands_1d = 0usize;
    let mut scanned = HashMap::with_capacity(results.len());
    for (sha, path, info) in results {
        let after_sum = std::fs::read(&path)
            .expect("verify intact")
            .iter()
            .map(|&b| b as u64)
            .sum::<u64>();
        assert_eq!(info.checksum_sum, after_sum, "file mutated: {path:?}");
        total_bytes += info.rom_len;
        cpu_ms += info.elapsed_ms;
        unique_cands += info.cands.len();
        unique_cands_2d += info.c2d.len();
        unique_cands_1d += info.c1d.len();
        scanned.insert(sha, info);
    }

    let (stats_a, brands_a, unique_a) =
        eval_pairs(&root, &manifest.corpus_a, &scanned, true);
    let (stats_b, _brands_b, _) =
        eval_pairs(&root, &manifest.corpus_b, &scanned, false);
    let mut stats_all = BucketStats::default();
    stats_all.add(&stats_a);
    stats_all.add(&stats_b);

    let mib = total_bytes as f64 / (1024.0 * 1024.0);
    println!(
        "\n=== BLIND_V2 647-PAIR CORPUS AUDIT ===\nUnique ROMs: {} ({:.2} MiB) | Wall: {:.1} ms ({:.2} MiB/s) | CPU sum: {:.1} ms | Unique cands: {} (2D={}, 1D={})\n",
        scanned.len(),
        mib,
        wall_ms,
        mib / (wall_ms / 1000.0).max(1e-6),
        cpu_ms,
        unique_cands,
        unique_cands_2d,
        unique_cands_1d
    );
    println!(
        "{:<18} {:>9} {:>8} {:>22} {:>22} {:>22} {:>22} {:>6}",
        "Cohort", "Pairs", "Cands", "2D +-16B", "2D Ov>=50%", "1D +-16B", "All +-16B", "Perf"
    );
    println!("{}", "-".repeat(134));
    print_summary_row("Corpus A (Valid)", &stats_a);
    print_summary_row("Corpus B (Unver)", &stats_b);
    print_summary_row("All 647 Pairs", &stats_all);

    let uniq_tot = unique_a.t2d + unique_a.t1d;
    println!(
        "\nCorpus A Deduplicated Unique Oracle ({} unique defs): 2D +-16B={}/{} ({:.1}%), 2D Ov>=50%={}/{} ({:.1}%), 1D +-16B={}/{} ({:.1}%), All +-16B={}/{} ({:.1}%)",
        uniq_tot,
        unique_a.hit_2d_16,
        unique_a.t2d,
        pct(unique_a.hit_2d_16, unique_a.t2d),
        unique_a.hit_2d_ov50,
        unique_a.t2d,
        pct(unique_a.hit_2d_ov50, unique_a.t2d),
        unique_a.hit_1d_16,
        unique_a.t1d,
        pct(unique_a.hit_1d_16, unique_a.t1d),
        unique_a.hit_2d_16 + unique_a.hit_1d_16,
        uniq_tot,
        pct(unique_a.hit_2d_16 + unique_a.hit_1d_16, uniq_tot)
    );

    println!("\n--- Corpus A Brand Breakdown ---");
    for (brand, bs) in &brands_a {
        print_summary_row(brand, bs);
    }
}
