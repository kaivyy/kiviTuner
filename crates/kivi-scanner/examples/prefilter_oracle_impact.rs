//! Attribution experiment: do the CODE-pruned oracle addresses affect recall?
//!
//! Isolated harness. Production code is not modified and not called with
//! different arguments; variant B reproduces the 16-bit scan loop here so it can
//! run over ranges production's prefilter excludes.
//!
//! **No XDF leakage into scanning.** Shapes come from `default_shapes()` (a
//! corpus-wide prior), never from the pair's own XDF. The XDF is read only after
//! scanning, to score the result.
//!
//! Run: `cargo run --release --example prefilter_oracle_impact`

use std::collections::HashSet;

use kivi_scanner::blind_automap::default_shapes;
use kivi_scanner::xdf_parse::parse_xdf_xml;
use kivi_scanner::zero_xdf_scan::{
    apply_memory_interval_nms, calc_ac_and_smoothness_16bit, check_candidate_16bit_integer,
    DiscoveredFullTable, Endianness, MapDataBits, SemanticMapType,
};

const DB: &str = "/root/ecu_master_database";
const INDEX: &str = "/root/kiviTuner/docs/migration/PAIRING_INDEX.md";
const BLOCK: usize = 4096;
const WIN: usize = 256;
const STEP: usize = 64;

// ---------------------------------------------------------------------------
// Prefilter mirror. `should_prune` is private and returns only a bool, so the
// experimental variant cannot ask it *why* a block was dropped. The arithmetic
// below is copied verbatim from `zero_xdf_prefilter::should_prune`.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Rule {
    Blank,
    LowEnt,
    Code,
    Keep,
}

fn classify(chunk: &[u8]) -> Rule {
    let n = chunk.len();
    if n == 0 {
        return Rule::Blank;
    }
    let mut counts = [0u16; 256];
    let mut max_c = 0u16;
    for &b in chunk {
        counts[b as usize] += 1;
        if counts[b as usize] > max_c {
            max_c = counts[b as usize];
        }
    }
    if (max_c as usize) * 100 / n > 85 {
        return Rule::Blank;
    }
    let mut ent = 0.0f64;
    for &c in &counts {
        if c > 0 {
            let p = c as f64 / n as f64;
            ent -= p * p.log2();
        }
    }
    if ent < 1.0 {
        return Rule::LowEnt;
    }
    if ent > 6.95 && n > 1 {
        let mut sum_d = 0u64;
        for j in 0..n - 1 {
            sum_d += (chunk[j + 1] as i32 - chunk[j] as i32).unsigned_abs() as u64;
        }
        if sum_d as f64 / (n - 1) as f64 > 85.0 {
            return Rule::Code;
        }
    }
    Rule::Keep
}

/// Production kept ranges: every block whose rule is not `Keep` is dropped.
fn production_ranges(rom: &[u8]) -> Vec<(usize, usize)> {
    coalesce((0..rom.len()).step_by(BLOCK), rom.len(), |s, e| {
        classify(&rom[s..e]) == Rule::Keep
    })
}

/// Variant B: a block dropped *specifically by the CODE rule* is re-examined at
/// 256-byte granularity, and only its non-CODE windows are restored.
fn experimental_ranges(rom: &[u8]) -> Vec<(usize, usize)> {
    let n = rom.len();
    let slots = n.div_ceil(STEP);
    let mut keep = vec![false; slots];
    let mark = |keep: &mut Vec<bool>, from: usize, to: usize| {
        let mut k = from / STEP;
        while k * STEP < to && k < slots {
            keep[k] = true;
            k += 1;
        }
    };
    for i in 0..n.div_ceil(BLOCK) {
        let s = i * BLOCK;
        let e = (s + BLOCK).min(n);
        match classify(&rom[s..e]) {
            Rule::Keep => mark(&mut keep, s, e),
            Rule::Code => {
                // Restore only the parts of the block that look like data.
                for w in (s..e).step_by(STEP) {
                    let we = (w + WIN).min(n);
                    if classify(&rom[w..we]) == Rule::Keep {
                        mark(&mut keep, w, w + STEP);
                    }
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    let mut run: Option<usize> = None;
    for (k, &k_kept) in keep.iter().enumerate() {
        let s = (k * STEP).min(n);
        match (k_kept, run) {
            (true, None) => run = Some(s),
            (false, Some(a)) => {
                out.push((a, s));
                run = None;
            }
            _ => {}
        }
    }
    if let Some(a) = run {
        out.push((a, n));
    }
    out
}

fn coalesce(
    steps: impl Iterator<Item = usize>,
    n: usize,
    keep: impl Fn(usize, usize) -> bool,
) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut run: Option<usize> = None;
    for s in steps {
        let e = (s + BLOCK).min(n);
        if keep(s, e) {
            run.get_or_insert(s);
        } else if let Some(a) = run.take() {
            out.push((a, s));
        }
    }
    if let Some(a) = run {
        out.push((a, n));
    }
    out
}

// ---------------------------------------------------------------------------
// 16-bit scan over an explicit range set. Same acceptance predicates and the
// same `offset += 16` advance-on-accept as `scan_16bit_tables_auto_endian`.
// ---------------------------------------------------------------------------

fn scan_ranges(rom: &[u8], shapes: &[(usize, usize)], ranges: &[(usize, usize)]) -> Vec<DiscoveredFullTable> {
    let mut out = Vec::new();
    for &(rows, cols) in shapes {
        if rows < 2 || cols < 2 {
            continue;
        }
        let byte_len = rows * cols * 2;
        for &(start, end) in ranges {
            if end - start < byte_len {
                continue;
            }
            let mut offset = start;
            let limit = end - byte_len;
            while offset <= limit {
                let slice = &rom[offset..offset + byte_len];
                let le = check_candidate_16bit_integer(slice, rows, cols, Endianness::LittleEndian)
                    .then(|| calc_ac_and_smoothness_16bit(slice, rows, cols, Endianness::LittleEndian))
                    .flatten();
                let be = check_candidate_16bit_integer(slice, rows, cols, Endianness::BigEndian)
                    .then(|| calc_ac_and_smoothness_16bit(slice, rows, cols, Endianness::BigEndian))
                    .flatten();
                let chosen = match (le, be) {
                    (Some((acl, sml)), Some((acb, smb))) => {
                        if sml <= smb {
                            Some((Endianness::LittleEndian, acl, sml))
                        } else {
                            Some((Endianness::BigEndian, acb, smb))
                        }
                    }
                    (Some((ac, sm)), None) => Some((Endianness::LittleEndian, ac, sm)),
                    (None, Some((ac, sm))) => Some((Endianness::BigEndian, ac, sm)),
                    (None, None) => None,
                };
                match chosen {
                    Some((endian, ac, smooth)) => {
                        let is_le = endian == Endianness::LittleEndian;
                        let matrix: Vec<Vec<f64>> = (0..rows)
                            .map(|r| {
                                (0..cols)
                                    .map(|c| {
                                        let i = (r * cols + c) * 2;
                                        if is_le {
                                            u16::from_le_bytes([slice[i], slice[i + 1]]) as f64
                                        } else {
                                            u16::from_be_bytes([slice[i], slice[i + 1]]) as f64
                                        }
                                    })
                                    .collect()
                            })
                            .collect();
                        out.push(DiscoveredFullTable {
                            z_address: offset,
                            rows,
                            cols,
                            data_format: MapDataBits::Bits16(endian),
                            semantic_type: SemanticMapType::UnknownTable,
                            confidence_score: (1.0 - smooth.min(0.9)) * ac,
                            smoothness: smooth,
                            x_axis: None,
                            y_axis: None,
                            sample_matrix: matrix,
                        });
                        offset += 16;
                    }
                    None => offset += 2,
                }
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------

struct Pair {
    binary: String,
    definition: String,
}

fn load_confirmed() -> Vec<Pair> {
    let text = std::fs::read_to_string(INDEX).expect("pairing index");
    let mut out = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        if line.starts_with("## Identity-confirmed pairs") {
            inside = true;
            continue;
        }
        if line.starts_with("## ") && inside {
            break;
        }
        if !inside || !line.starts_with("| `") {
            continue;
        }
        let cells: Vec<&str> = line.split('|').map(|c| c.trim()).collect();
        if cells.len() < 5 || !cells[3].eq_ignore_ascii_case("XDF") {
            continue;
        }
        out.push(Pair {
            binary: cells[1].trim_matches('`').to_string(),
            definition: cells[2].trim_matches('`').to_string(),
        });
    }
    out.into_iter().take(30).collect()
}

/// Synthetic `GENERATIONAL_*` definitions declare addresses in a ROM layout that
/// does not match the binary they are paired with. Their oracle addresses are not
/// valid ground truth.
fn is_synthetic(xdf: &str) -> bool {
    xdf.rsplit('/').next().unwrap_or("").starts_with("GENERATIONAL_")
}

#[derive(Default)]
struct Stats {
    oracle_2d: usize,
    exact_2d: usize,
    near_2d: usize,
    cand_2d: usize,
    nms_2d: usize,
    nms_exact_2d: usize,
    fp: usize,
}

fn nearest(sorted: &[usize], a: usize) -> Option<usize> {
    let i = sorted.partition_point(|&r| r < a);
    let lo = i.saturating_sub(3);
    let hi = (i + 3).min(sorted.len());
    sorted[lo..hi].iter().map(|&r| r.abs_diff(a)).min()
}

fn main() {
    let shapes = default_shapes();
    let pairs = load_confirmed();
    let mut rows: Vec<(String, String, usize, usize, usize, bool)> = Vec::new();
    let mut base = Stats::default();
    let mut exp = Stats::default();
    let mut base_raw = 0usize;
    let mut exp_raw = 0usize;
    let mut recovered: Vec<(String, usize)> = Vec::new();

    for p in &pairs {
        let Ok(rom) = std::fs::read(format!("{DB}/{}", p.binary)) else { continue };
        let Ok(xdf) = std::fs::read(format!("{DB}/{}", p.definition)) else { continue };
        let Ok(parsed) = parse_xdf_xml(&String::from_utf8_lossy(&xdf)) else { continue };

        let mut o2d: Vec<usize> = Vec::new();
        let mut o1d: Vec<usize> = Vec::new();
        for t in &parsed.tables {
            let r = t.y_axis.as_ref().map(|a| a.index_count).unwrap_or(1);
            let c = t.x_axis.as_ref().map(|a| a.index_count).unwrap_or(1);
            if r < 2 && c < 2 {
                continue;
            }
            if r >= 2 && c >= 2 {
                o2d.push(t.z_axis.address);
            } else {
                o1d.push(t.z_axis.address);
            }
        }
        o2d.sort_unstable();
        o2d.dedup();
        o1d.sort_unstable();
        o1d.dedup();

        // --- scanning: ROM and default shapes only ---
        let ranges_a = production_ranges(&rom);
        let ranges_b = experimental_ranges(&rom);
        let a_raw = scan_ranges(&rom, &shapes, &ranges_a);
        let b_raw = scan_ranges(&rom, &shapes, &ranges_b);

        let mut a2d: Vec<usize> = a_raw.iter().map(|t| t.z_address).collect();
        a2d.sort_unstable();
        a2d.dedup();
        let mut b2d: Vec<usize> = b_raw.iter().map(|t| t.z_address).collect();
        b2d.sort_unstable();
        b2d.dedup();

        base_raw += a_raw.len();
        exp_raw += b_raw.len();
        base.cand_2d += a2d.len();
        exp.cand_2d += b2d.len();
        let a_nms: Vec<usize> = apply_memory_interval_nms(a_raw.clone(), 0.0)
            .iter()
            .map(|t| t.z_address)
            .collect();
        let b_nms: Vec<usize> = apply_memory_interval_nms(b_raw.clone(), 0.0)
            .iter()
            .map(|t| t.z_address)
            .collect();
        base.nms_2d += a_nms.len();
        exp.nms_2d += b_nms.len();
        let _ = &o1d;
        let mut a_nms_s = a_nms.clone();
        a_nms_s.sort_unstable();
        let mut b_nms_s = b_nms.clone();
        b_nms_s.sort_unstable();
        for &a in &o2d {
            if nearest(&a_nms_s, a) == Some(0) {
                base.nms_exact_2d += 1;
            }
            if nearest(&b_nms_s, a) == Some(0) {
                exp.nms_exact_2d += 1;
            }
        }
        // False positives: candidates that land within 8 bytes of no oracle.
        let all_oracle: HashSet<usize> = o2d.iter().chain(o1d.iter()).copied().collect();
        let mut all_s: Vec<usize> = all_oracle.iter().copied().collect();
        all_s.sort_unstable();
        let fp = |c: &[usize]| {
            c.iter()
                .filter(|&&x| nearest(&all_s, x).is_none_or(|d| d > 8))
                .count()
        };
        base.fp += fp(&a_nms_s);
        exp.fp += fp(&b_nms_s);

        let oracle_set: HashSet<usize> = o2d.iter().copied().collect();
        for &a in &o2d {
            match nearest(&a2d, a) {
                Some(0) => base.exact_2d += 1,
                Some(d) if d <= 8 => base.near_2d += 1,
                _ => {}
            }
            let d = nearest(&b2d, a).unwrap_or(usize::MAX);
            match d {
                0 => exp.exact_2d += 1,
                d if d <= 8 => exp.near_2d += 1,
                _ => {}
            }
            if nearest(&a2d, a).is_none_or(|d| d > 8) && d <= 8 {
                recovered.push((p.binary.clone(), a));
            }
        }
        // Per-address attribution for the addresses of interest.
        for &a in &o2d {
            if a >= rom.len() {
                continue;
            }
            let blk = (a / BLOCK) * BLOCK;
            if classify(&rom[blk..(blk + BLOCK).min(rom.len())]) != Rule::Code {
                continue;
            }
            let w = ((a / STEP) * STEP).min(rom.len());
            let wr = classify(&rom[w..(w + WIN).min(rom.len())]);
            rows.push((
                p.binary.rsplit('/').next().unwrap_or("").to_string(),
                format!("{wr:?}"),
                a,
                blk,
                nearest(&a2d, a).unwrap_or(usize::MAX),
                is_synthetic(&p.definition),
            ));
        }
        let _ = oracle_set;
        base.oracle_2d += o2d.len();
        exp.oracle_2d += o2d.len();
    }

    println!("shapes used: {} (default_shapes, no XDF leakage)", shapes.len());
    println!("\n--- addresses of interest: 2-D oracle inside a CODE-pruned 4KiB block ---");
    println!("{:<34} {:>6} {:>8} {:>8} {:>7}", "binary", "win", "addr", "dist_base", "synth");
    for r in &rows {
        println!(
            "{:<34} {:>6} 0x{:06X} {:>8} {:>7}",
            r.0.chars().take(34).collect::<String>(),
            r.1,
            r.2,
            if r.4 == usize::MAX { "none".to_string() } else { r.4.to_string() },
            r.5
        );
    }
    let recovered_in = |a: usize| recovered.iter().any(|(_, x)| *x == a);
    println!("\nof the addresses above, recovered by variant B: {}",
        rows.iter().filter(|r| recovered_in(r.2) && !r.5).count());

    let pct = |v: usize, n: usize| if n == 0 { 0.0 } else { 100.0 * v as f64 / n as f64 };
    println!("\n--- aggregate (2-D; 1-D not varied — see limitations) ---");
    println!("{:<26} {:>10} {:>10}", "metric", "baseline", "variant B");
    println!("{:<26} {:>10} {:>10}", "oracle 2-D", base.oracle_2d, exp.oracle_2d);
    println!("{:<26} {:>9.1}% {:>9.1}%", "exact 2-D", pct(base.exact_2d, base.oracle_2d), pct(exp.exact_2d, exp.oracle_2d));
    println!("{:<26} {:>10} {:>10}", "exact 2-D count", base.exact_2d, exp.exact_2d);
    println!("{:<26} {:>10} {:>10}", "exact 2-D count (all)", base.exact_2d + base.near_2d, exp.exact_2d + exp.near_2d);
    println!("{:<26} {:>10} {:>10}", "within-8 count", base.exact_2d + base.near_2d, exp.exact_2d + exp.near_2d);
    println!("{:<26} {:>10} {:>10}", "raw 2-D candidates", base_raw, exp_raw);
    println!("{:<26} {:>10} {:>10}", "distinct 2-D addrs", base.cand_2d, exp.cand_2d);
    println!("{:<26} {:>10} {:>10}", "after NMS", base.nms_2d, exp.nms_2d);
    println!("{:<26} {:>9.1}% {:>9.1}%", "exact 2-D after NMS", pct(base.nms_exact_2d, base.oracle_2d), pct(exp.nms_exact_2d, exp.oracle_2d));
    println!("{:<26} {:>10} {:>10}", "exact 2-D after NMS count", base.nms_exact_2d, exp.nms_exact_2d);
    println!("{:<26} {:>10} {:>10}", "false positives (NMS)", base.fp, exp.fp);
    println!(
        "\ncandidate growth after NMS: {:+.2}%",
        100.0 * (exp.nms_2d as f64 - base.nms_2d as f64) / base.nms_2d.max(1) as f64
    );
    println!(
        "false-positive growth      : {:+.2}%",
        100.0 * (exp.fp as f64 - base.fp as f64) / base.fp.max(1) as f64
    );
}
