//! LAPIS 1.5 — recall of the detector set against XDF oracle, before and after
//! boundary snapping. Two ROMs only.
//!
//! Matching rules are byte-for-byte the ones `l15_stage1.rs` used, so the
//! "before" column reproduces the previously reported baselines (CBR 0.0%
//! exact, B58 1-D 10.4% exact / 29.5% <=8B / 48.1% IoU). Changing them here
//! would make the comparison meaningless.
//!
//! Run: `cargo run --release --example l15_snap_ab`

use kivi_scanner::curve_scan::{scan_curves, CurveScanOptions, Endian};
use kivi_scanner::xdf_parse::parse_xdf_xml;
use kivi_scanner::zero_xdf_scan::scan_16bit_tables_auto_endian;

struct Oracle {
    addr: usize,
    rows: usize,
    cols: usize,
    bits: usize,
}

struct Rom {
    name: &'static str,
    bin: &'static str,
    xdf: &'static str,
}

const ROMS: &[Rom] = &[
    Rom {
        name: "CBR250RR",
        bin: "01_MOTORCYCLES/HONDA/SPORT_CBR250RR/CBR250RR-38770-K64-ND1_BIN.bin",
        xdf: "01_MOTORCYCLES/HONDA/SPORT_CBR250RR/CBR250RR_38770-K64-ND1_PUBLIC_v1.xdf",
    },
    Rom {
        name: "BMW_B58",
        bin: "02_CARS/BMW/B58_3_0L_TURBO_GEN1_GEN2/00005D55508C05_b58o1_original.bin",
        xdf: "02_CARS/BMW/B58_3_0L_TURBO_GEN1_GEN2/00005D55508C05.xdf",
    },
];

#[derive(Default, Clone)]
struct Rec {
    exact: usize,
    off2: usize,
    off4: usize,
    off8: usize,
    off16: usize,
    overlap: usize,
    overlap_own: usize,
    overlap_spurious: usize,
    exact_own: usize,
    n: usize,
}

fn run(rom: &[u8], oracle: &[Oracle], shapes_2d: &[(usize, usize)], snap: bool) -> (Rec, Rec, Rec, usize) {
    let tables = scan_16bit_tables_auto_endian(rom, shapes_2d, 0, rom.len());
    let mut curves = scan_curves(
        rom,
        &CurveScanOptions {
            min_points: 3,
            max_points: 40,
            endian: Endian::Big,
            include_8bit: true,
            allow_monotonic: true,
            snap_boundaries: snap,
        },
    );
    curves.extend(scan_curves(
        rom,
        &CurveScanOptions {
            min_points: 3,
            max_points: 40,
            endian: Endian::Little,
            include_8bit: false,
            allow_monotonic: true,
            snap_boundaries: snap,
        },
    ));

    // Attribution matters: a curve whose address happens to coincide with a
    // 2-D oracle address inflates that oracle's score, and moving the curve
    // then looks like a 2-D regression. Keep the source of each window.
    let mut windows: Vec<(usize, usize, char)> = Vec::new();
    for t in &tables {
        windows.push((t.z_address, t.rows * t.cols * 2, 'T'));
    }
    for c in &curves {
        windows.push((c.address, c.points * c.cell_bytes, 'C'));
    }
    let window_count = windows.len();
    {
        let mut addrs: Vec<usize> = curves.iter().map(|c| c.address).collect();
        let total = addrs.len();
        addrs.sort_unstable();
        addrs.dedup();
        let lens: Vec<usize> = curves.iter().map(|c| c.points).collect();
        let short = lens.iter().filter(|&&p| p <= 10).count();
        eprintln!(
            "    [probe snap={snap}] curves={total} unique_addr={} short(<=10)={short} tables={}",
            addrs.len(), tables.len()
        );
    }
    let curve_count = windows.iter().filter(|w| w.2 == 'C').count();
    let _ = curve_count;

    let mut all = Rec::default();
    let mut one_d = Rec::default();
    let mut two_d = Rec::default();

    for o in oracle {
        let s = o.rows * o.cols * (o.bits / 8).max(1);
        let o_end = o.addr + s;
        let mut best_overlap = 0f64;
        // Same, restricted to the detector that is supposed to serve this row:
        // 2-D tables serve 2-D oracle rows, curves serve 1-D ones. Mixing them
        // makes snapping look like a recall loss when it is only a curve
        // shrinking onto its true end.
        let want_kind = if o.rows < 2 || o.cols < 2 { 'C' } else { 'T' };
        let mut best_overlap_own = 0f64;
        // Length of the winning overlap window: a 40-cell window covers most
        // 1-D oracles by accident, so shrink loss may be removal of spurious
        // coverage rather than loss of a real match.
        let mut best_overlap_delta = i64::MAX;
        let mut best_start: Option<(i64, char)> = None;
        for &(ws, wl, k) in &windows {
            let w_end = ws + wl;
            let ov = o_end.min(w_end).saturating_sub(o.addr.max(ws));
            let frac = if s > 0 { ov as f64 / s as f64 } else { 0.0 };
            if frac > best_overlap {
                best_overlap = frac;
                best_overlap_delta = ws as i64 - o.addr as i64;
            }
            if k == want_kind && frac > best_overlap_own {
                best_overlap_own = frac;
            }
            let d = ws as i64 - o.addr as i64;
            if best_start.is_none_or(|(b, _): (i64, char)| d.abs() < b.abs()) {
                best_start = Some((d, k));
            }
        }
        let (bs, _kind) = best_start.unwrap_or((0, '?'));
        let is_1d = o.rows < 2 || o.cols < 2;
        for r in [&mut all, if is_1d { &mut one_d } else { &mut two_d }] {
            r.n += 1;
            if bs == 0 {
                r.exact += 1
            }
            if bs.abs() <= 2 {
                r.off2 += 1
            }
            if bs.abs() <= 4 {
                r.off4 += 1
            }
            if bs.abs() <= 8 {
                r.off8 += 1
            }
            if bs.abs() <= 16 {
                r.off16 += 1
            }
            if best_overlap >= 0.5 {
                r.overlap += 1;
                if best_overlap_delta.abs() > 16 {
                    r.overlap_spurious += 1
                }
            }
            if best_overlap_own >= 0.5 {
                r.overlap_own += 1
            }
            let own_exact = windows
                .iter()
                .any(|&(ws, _, wk)| wk == want_kind && ws == o.addr);
            if own_exact {
                r.exact_own += 1
            }
        }
    }
    (all, one_d, two_d, window_count)
}

fn starts_map(
    rom: &[u8],
    oracle: &[Oracle],
    shapes_2d: &[(usize, usize)],
    snap: bool,
) -> std::collections::HashMap<usize, (i64, char)> {
    let tables = scan_16bit_tables_auto_endian(rom, shapes_2d, 0, rom.len());
    let mut curves = scan_curves(
        rom,
        &CurveScanOptions { min_points: 3, max_points: 40, endian: Endian::Big,
            include_8bit: true, allow_monotonic: true, snap_boundaries: snap },
    );
    curves.extend(scan_curves(
        rom,
        &CurveScanOptions { min_points: 3, max_points: 40, endian: Endian::Little,
            include_8bit: false, allow_monotonic: true, snap_boundaries: snap },
    ));
    let mut windows: Vec<(usize, char)> = Vec::new();
    for t in &tables { windows.push((t.z_address, 'T')); }
    for c in &curves { windows.push((c.address, 'C')); }

    let mut out = std::collections::HashMap::new();
    for o in oracle {
        let mut best: Option<(i64, char)> = None;
        for &(ws, k) in &windows {
            let d = ws as i64 - o.addr as i64;
            if best.is_none_or(|(b, _): (i64, char)| d.abs() < b.abs()) {
                best = Some((d, k));
            }
        }
        if let Some(b) = best { out.insert(o.addr, b); }
    }
    out
}

fn print_rec(label: &str, b: &Rec, a: &Rec) {
    let pct = |v: usize, n: usize| if n == 0 { 0.0 } else { 100.0 * v as f64 / n as f64 };
    let n = b.n.max(1);
    println!(
        "  {label:<20} {:>6.1}% -> {:>6.1}%   ({:+.1})   [{} -> {}]",
        pct(b.exact, n),
        pct(a.exact, n),
        pct(a.exact, n) - pct(b.exact, n),
        b.exact,
        a.exact
    );
    let _ = pct;
}

fn main() {
    let db = "/root/ecu_master_database";
    for rom_meta in ROMS {
        let rom = std::fs::read(format!("{db}/{}", rom_meta.bin)).expect("rom");
        let xml = String::from_utf8_lossy(&std::fs::read(format!("{db}/{}", rom_meta.xdf)).expect("xdf"))
            .into_owned();
        let parsed = parse_xdf_xml(&xml).expect("parse");

        let mut oracle: Vec<Oracle> = Vec::new();
        for t in &parsed.tables {
            let r = t.y_axis.as_ref().map(|a| a.index_count).unwrap_or(1);
            let c = t.x_axis.as_ref().map(|a| a.index_count).unwrap_or(1);
            if r < 2 && c < 2 {
                continue;
            }
            oracle.push(Oracle {
                addr: t.z_axis.address,
                rows: r,
                cols: c,
                bits: t.z_axis.element_size_bits,
            });
        }
        oracle.sort_by_key(|o| o.addr);
        oracle.dedup_by_key(|o| o.addr);

        let mut shapes_2d: Vec<(usize, usize)> = oracle
            .iter()
            .filter(|o| o.rows >= 2 && o.cols >= 2 && o.rows <= 48 && o.cols <= 48)
            .map(|o| (o.rows, o.cols))
            .collect();
        shapes_2d.sort_unstable();
        shapes_2d.dedup();

        let (a_all, a_1d, a_2d, wc_off) = run(&rom, &oracle, &shapes_2d, false);
        let (b_all, b_1d, b_2d, wc_on) = run(&rom, &oracle, &shapes_2d, true);

        // Regressions: oracle rows whose exact match existed before and not
        // after. Identify which detector supplied it.
        let before_starts = starts_map(&rom, &oracle, &shapes_2d, false);
        let after_starts = starts_map(&rom, &oracle, &shapes_2d, true);
        let mut reg = Vec::new();
        for o in &oracle {
            let b = before_starts.get(&o.addr).copied().unwrap_or((i64::MAX, '?'));
            let a = after_starts.get(&o.addr).copied().unwrap_or((i64::MAX, '?'));
            if b.0 == 0 && a.0 != 0 {
                reg.push((o.addr, o.rows, o.cols, a.0, a.1));
            }
        }
        if !reg.is_empty() {
            println!("  REGRESSED exact starts ({}):", reg.len());
            for (addr, r, c, d, k) in reg.iter().take(12) {
                println!("     {addr:#08X} {r}x{c}: now delta {d:+} via {k}");
            }
        }

        println!("============================================================");
        println!(
            "{}  ROM {} bytes | oracle {} tables | windows {} -> {}",
            rom_meta.name,
            rom.len(),
            oracle.len(),
            wc_off,
            wc_on
        );

        for (kind, b, a) in [("all", &a_all, &b_all), ("1D", &a_1d, &b_1d), ("2D", &a_2d, &b_2d)] {
            let n = b.n.max(1);
            println!("  --- {kind} (n={}) ---", b.n);
            print_rec("Exact (delta=0)", b, a);
            let pct = |v: usize| 100.0 * v as f64 / n as f64;
            println!(
                "  {:<20} {:>6.1}% -> {:>6.1}%   ({:+.1})",
                "|delta| <= 8",
                pct(b.off8),
                pct(a.off8),
                pct(a.off8) - pct(b.off8)
            );
            println!(
                "  {:<20} {:>6.1}% -> {:>6.1}%   ({:+.1})   [any detector]",
                "IoU >= 50%",
                pct(b.overlap),
                pct(a.overlap),
                pct(a.overlap) - pct(b.overlap)
            );
            println!(
                "  {:<20} {:>6.1}% -> {:>6.1}%   ({:+.1})   [own detector]",
                "IoU >= 50%",
                pct(b.overlap_own),
                pct(a.overlap_own),
                pct(a.overlap_own) - pct(b.overlap_own)
            );
            println!(
                "  {:<20}         {} -> {}        [own detector]",
                "Exact (delta=0)", b.exact_own, a.exact_own
            );
            println!(
                "  {:<20}         {} -> {}        [overlap won by a window >16B away]",
                "IoU spurious", b.overlap_spurious, a.overlap_spurious
            );
        }
    }
}
