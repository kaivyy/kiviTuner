//! 30 verified BIN+XDF pairs across three stages:
//!   A  raw            — `i += points`, no seam, no pointer anchor
//!   B  seam           — `seam_len` advancement on
//!   C  seam+pointer   — B, then every candidate shifted onto a ROM anchor
//!                       within 16 bytes (anchors from ROM bytes only)
//!
//! Matching rule follows `confirmed_pairs_recall_test`: nearest reported
//! address, credited per detector so a curve landing on a 2-D oracle address
//! is not counted as 2-D recall.
//!
//! Run: `cargo run --release --example pairs30_snap_ab`

use kivi_scanner::curve_scan::{scan_curves, CurveScanOptions, Endian};
use kivi_scanner::pointer_snap::Anchors;
use kivi_scanner::xdf_parse::parse_xdf_xml;
use kivi_scanner::zero_xdf_scan::{
    detect_global_endianness, scan_16bit_tables_auto_endian, scan_8bit_tables, Endianness,
};

const INDEX: &str = "/root/kiviTuner/docs/migration/PAIRING_INDEX.md";
const DB: &str = "/root/ecu_master_database";
const SAMPLE: usize = 30;
const RADIUS: usize = 16;

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
    out
}

#[derive(Default, Clone, Copy)]
struct Rec {
    oracle: usize,
    exact_2d: usize,
    exact_1d: usize,
    near_2d: usize,
    near_1d: usize,
    candidates_2d: usize,
    candidates_1d: usize,
}

impl Rec {
    fn add(&mut self, o: &Rec) {
        self.oracle += o.oracle;
        self.exact_2d += o.exact_2d;
        self.exact_1d += o.exact_1d;
        self.near_2d += o.near_2d;
        self.near_1d += o.near_1d;
        self.candidates_2d += o.candidates_2d;
        self.candidates_1d += o.candidates_1d;
    }
}

fn dedup(mut v: Vec<usize>) -> Vec<usize> {
    v.sort_unstable();
    v.dedup();
    v
}

fn score(
    rep_2d: &[usize],
    rep_1d: &[usize],
    o1d: &[usize],
    o2d: &[usize],
) -> Rec {
    let nearest = |sorted: &[usize], a: usize| -> Option<usize> {
        let i = sorted.partition_point(|&r| r < a);
        let lo = i.saturating_sub(3);
        let hi = (i + 3).min(sorted.len());
        sorted[lo..hi].iter().map(|&r| r.abs_diff(a)).min()
    };
    let mut rec = Rec {
        oracle: o1d.len() + o2d.len(),
        candidates_2d: rep_2d.len(),
        candidates_1d: rep_1d.len(),
        ..Default::default()
    };
    for &a in o2d {
        match nearest(rep_2d, a) {
            Some(0) => rec.exact_2d += 1,
            Some(d) if d <= 8 => rec.near_2d += 1,
            _ => {}
        }
    }
    for &a in o1d {
        match nearest(rep_1d, a) {
            Some(0) => rec.exact_1d += 1,
            Some(d) if d <= 8 => rec.near_1d += 1,
            _ => {}
        }
    }
    rec
}

fn shift(addrs: &[usize], anchors: &Anchors, strong_only: bool) -> Vec<usize> {
    addrs
        .iter()
        .map(|&a| {
            let hit = if strong_only {
                anchors.nearest_strong(a, RADIUS)
            } else {
                anchors.nearest(a, RADIUS)
            };
            hit.map(|(t, _)| t).unwrap_or(a)
        })
        .collect()
}

/// Returns `(raw, seam, seam+pointer)`, or `None` with a reason printed.
fn measure(pair: &Pair, strong_only: bool) -> Option<(Rec, Rec, Rec)> {
    let rom = match std::fs::read(format!("{DB}/{}", pair.binary)) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("SKIP read-bin {}: {e}", pair.binary);
            return None;
        }
    };
    let xdf = match std::fs::read(format!("{DB}/{}", pair.definition)) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("SKIP read-xdf {}: {e}", pair.definition);
            return None;
        }
    };
    let parsed = match parse_xdf_xml(&String::from_utf8_lossy(&xdf)) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("SKIP parse-xdf {}: {e}", pair.definition);
            return None;
        }
    };

    let mut o1d: Vec<usize> = Vec::new();
    let mut o2d: Vec<usize> = Vec::new();
    let mut shapes_2d: Vec<(usize, usize)> = Vec::new();
    for t in &parsed.tables {
        let r = t.y_axis.as_ref().map(|a| a.index_count).unwrap_or(1);
        let c = t.x_axis.as_ref().map(|a| a.index_count).unwrap_or(1);
        if r < 2 && c < 2 {
            continue;
        }
        if r >= 2 && c >= 2 {
            o2d.push(t.z_axis.address);
            if r <= 48 && c <= 48 {
                shapes_2d.push((r, c));
            }
        } else {
            o1d.push(t.z_axis.address);
        }
    }
    let o1d = dedup(o1d);
    let o2d = dedup(o2d);
    shapes_2d.sort_unstable();
    shapes_2d.dedup();
    if o1d.is_empty() && o2d.is_empty() {
        eprintln!(
            "SKIP empty-oracle {} (tables={})",
            pair.definition,
            parsed.tables.len()
        );
        return None;
    }

    let mut tables = if shapes_2d.is_empty() {
        Vec::new()
    } else {
        let mut f = scan_16bit_tables_auto_endian(&rom, &shapes_2d, 0, rom.len());
        f.extend(scan_8bit_tables(&rom, &shapes_2d, 0, rom.len()));
        f
    };
    tables.sort_by_key(|t| t.z_address);
    let raw_2d = dedup(tables.iter().map(|t| t.z_address).collect());

    let curves_for = |snap: bool| -> Vec<usize> {
        let mut c = scan_curves(
            &rom,
            &CurveScanOptions {
                min_points: 3,
                max_points: 40,
                endian: Endian::Big,
                include_8bit: true,
                allow_monotonic: true,
                snap_boundaries: snap,
            },
        );
        c.extend(scan_curves(
            &rom,
            &CurveScanOptions {
                min_points: 3,
                max_points: 40,
                endian: Endian::Little,
                include_8bit: false,
                allow_monotonic: true,
                snap_boundaries: snap,
            },
        ));
        dedup(c.iter().map(|c| c.address).collect())
    };

    let raw_1d = curves_for(false);
    let seam_1d = curves_for(true);
    let seam_2d = raw_2d.clone();

    // Anchors are read in the ROM's own byte order. Applying big-endian anchors
    // to a little-endian ROM measures nothing but noise.
    let big_endian = detect_global_endianness(&rom) == Endianness::BigEndian;
    let anchors = Anchors::from_rom(&rom, big_endian);
    let ptr_1d = dedup(shift(&seam_1d, &anchors, strong_only));
    let ptr_2d = dedup(shift(&seam_2d, &anchors, strong_only));

    if std::env::var("TRACE").is_ok() {
        let has = |v: &[usize], a: usize| v.binary_search(&a).is_ok();
        for &a in &o2d {
            if has(&seam_2d, a) && !has(&ptr_2d, a) {
                // Which candidate covered this oracle, and where did it go?
                let from = seam_2d.iter().copied().find(|&c| c == a);
                let moved = seam_2d
                    .iter()
                    .map(|&c| (c, anchors.nearest(c, RADIUS).map(|(t, _)| t).unwrap_or(c)))
                    .find(|(c, _)| *c == a);
                println!(
                    "  LOST 2-D oracle 0x{a:06X}  candidate {from:?} -> {moved:?}"
                );
            }
        }
    }

    Some((
        score(&raw_2d, &raw_1d, &o1d, &o2d),
        score(&seam_2d, &seam_1d, &o1d, &o2d),
        score(&ptr_2d, &ptr_1d, &o1d, &o2d),
    ))
}

fn pct(v: usize, n: usize) -> f64 {
    if n == 0 {
        0.0
    } else {
        100.0 * v as f64 / n as f64
    }
}

fn main() {
    let mut all: Vec<Pair> = load_confirmed();
    if let Ok(f) = std::env::var("ONLY") {
        // For tracing a single pair, search the whole index, not just the sample.
        all.retain(|p| p.definition.contains(&f) || p.binary.contains(&f));
    }
    let pairs: Vec<Pair> = all.into_iter().take(SAMPLE).collect();
    println!("verified pairs sampled: {}", pairs.len());

    let mut raw = Rec::default();
    let mut seam = Rec::default();
    let mut ptr = Rec::default();
    let mut used = 0usize;
    let strong_only = std::env::var("STRONG_ONLY").is_ok();
    let verbose = std::env::var("VERBOSE").is_ok();
    for p in &pairs {
        let Some((r, s, t)) = measure(p, strong_only) else { continue };
        used += 1;
        if verbose {
            let d2 = t.exact_2d as i64 - s.exact_2d as i64;
            let d1 = t.exact_1d as i64 - s.exact_1d as i64;
            if d1 != 0 || d2 != 0 {
                println!(
                    "  ptr delta  {}  2-D {:+#}  1-D {:+#}",
                    p.definition.rsplit('/').next().unwrap_or(""),
                    d2,
                    d1
                );
            }
        }
        raw.add(&r);
        seam.add(&s);
        ptr.add(&t);
    }

    let n = raw.oracle.max(1);
    println!("pairs measured: {used}   oracle tables: {}", raw.oracle);
    println!();
    println!("                        raw        +seam    +seam+ptr");
    println!(
        "Exact (all)          {:6.1}%   {:6.1}%   {:6.1}%   ({} -> {} -> {})",
        pct(raw.exact_1d + raw.exact_2d, n),
        pct(seam.exact_1d + seam.exact_2d, n),
        pct(ptr.exact_1d + ptr.exact_2d, n),
        raw.exact_1d + raw.exact_2d,
        seam.exact_1d + seam.exact_2d,
        ptr.exact_1d + ptr.exact_2d
    );
    println!(
        "Exact 1-D            {:6.1}%   {:6.1}%   {:6.1}%   ({} -> {} -> {})",
        pct(raw.exact_1d, n),
        pct(seam.exact_1d, n),
        pct(ptr.exact_1d, n),
        raw.exact_1d,
        seam.exact_1d,
        ptr.exact_1d
    );
    println!(
        "Exact 2-D            {:6.1}%   {:6.1}%   {:6.1}%   ({} -> {} -> {})",
        pct(raw.exact_2d, n),
        pct(seam.exact_2d, n),
        pct(ptr.exact_2d, n),
        raw.exact_2d,
        seam.exact_2d,
        ptr.exact_2d
    );
    let near = |r: &Rec| r.exact_1d + r.exact_2d + r.near_1d + r.near_2d;
    println!(
        "Within 8 bytes       {:6.1}%   {:6.1}%   {:6.1}%   ({} -> {} -> {})",
        pct(near(&raw), n),
        pct(near(&seam), n),
        pct(near(&ptr), n),
        near(&raw),
        near(&seam),
        near(&ptr)
    );
    println!();
    println!(
        "Candidates 1-D   {} -> {} -> {}",
        raw.candidates_1d, seam.candidates_1d, ptr.candidates_1d
    );
    println!(
        "Candidates 2-D   {} -> {} -> {}",
        raw.candidates_2d, seam.candidates_2d, ptr.candidates_2d
    );
    let tot = |r: &Rec| r.candidates_1d + r.candidates_2d;
    println!(
        "Candidates total {} -> {} -> {}",
        tot(&raw),
        tot(&seam),
        tot(&ptr)
    );
}
