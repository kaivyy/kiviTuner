//! NMS oracle attribution: why valid 2-D oracle tables are missing from the
//! final scanner output.
//!
//! Read-only audit. Production code is not modified. Detection and NMS are
//! called through their public APIs; the NMS *suppression relationship* is
//! reconstructed by replaying the documented acceptance rule over the same
//! candidate set, which is the only part production does not expose.
//!
//! No XDF reaches the scanner: the XDF is read only to build the oracle list
//! and to verify that the pair is legitimate.
//!
//! Run: `cargo run --release --example nms_oracle_attribution`

use std::collections::{HashMap, HashSet};

use kivi_scanner::blind_automap::default_shapes;
use kivi_scanner::nms_score::{composite_quality_score, ScoreInputs};
use kivi_scanner::rom_identity::{compare_identity, IdentityMatch};
use kivi_scanner::xdf_parse::parse_xdf_xml;
use kivi_scanner::zero_xdf_scan::{
    apply_memory_interval_nms, scan_16bit_tables_auto_endian, DiscoveredFullTable, Endianness,
    MapDataBits,
};

const DB: &str = "/root/ecu_master_database";
const INDEX: &str = "/root/kiviTuner/docs/migration/PAIRING_INDEX.md";

// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Trust {
    Verified,
    Probable,
    Unverified,
    Invalid,
}

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

fn is_synthetic(xdf: &str) -> bool {
    xdf.rsplit('/').next().unwrap_or("").starts_with("GENERATIONAL_")
}

/// A table is only meaningful if the ROM actually holds data where the XDF says
/// it does: inside the file, and not a single repeated value.
fn address_plausible(rom: &[u8], z: usize, rows: usize, cols: usize, bits: usize) -> bool {
    let bytes = (rows * cols * bits).div_ceil(8);
    if z + bytes > rom.len() {
        return false;
    }
    let slice = &rom[z..z + bytes];
    slice.iter().any(|&b| b != slice[0])
}

struct Oracle {
    rom_id: String,
    id: usize,
    z: usize,
    rows: usize,
    cols: usize,
    trust: Trust,
    layout_valid: bool,
    shape_scanned: bool,
}

fn main() {
    let pairs = load_confirmed();
    let shapes = default_shapes();

    let mut oracles: Vec<Oracle> = Vec::new();
    let mut matrix: Vec<(String, String, Trust, String)> = Vec::new();
    let mut next_id = 0usize;

    // ---- Phase 1 + 2 -----------------------------------------------------
    for p in &pairs {
        let Ok(rom) = std::fs::read(format!("{DB}/{}", p.binary)) else { continue };
        let Ok(xdf) = std::fs::read(format!("{DB}/{}", p.definition)) else { continue };
        let xml = String::from_utf8_lossy(&xdf);
        let Ok(parsed) = parse_xdf_xml(&xml) else { continue };

        let rom_name = p.binary.rsplit('/').next().unwrap_or("");
        let xdf_name = p.definition.rsplit('/').next().unwrap_or("");

        let (trust, reason) = if is_synthetic(&p.definition) {
            (
                Trust::Invalid,
                "GENERATIONAL_* definition: declared addresses do not belong to this ROM".to_string(),
            )
        } else {
            match compare_identity(&rom, xdf_name, &parsed.title) {
                IdentityMatch::Shared { token } => (Trust::Verified, format!("shared token {token}")),
                IdentityMatch::Mismatched { .. } => {
                    (Trust::Unverified, "no shared identity token".to_string())
                }
                IdentityMatch::Inconclusive => {
                    (Trust::Probable, "no identity token on one side".to_string())
                }
            }
        };
        matrix.push((rom_name.to_string(), xdf_name.to_string(), trust, reason.clone()));

        for t in &parsed.tables {
            let r = t.y_axis.as_ref().map(|a| a.index_count).unwrap_or(1);
            let c = t.x_axis.as_ref().map(|a| a.index_count).unwrap_or(1);
            if r < 2 || c < 2 {
                continue;
            }
            let bits = t.z_axis.element_size_bits.max(8);
            let mut tr = trust;
            let mut why = reason.clone();
            if !address_plausible(&rom, t.z_axis.address, r, c, bits) {
                tr = Trust::Invalid;
                why = format!("{why}; declared region is uniform or out of range");
            }
            oracles.push(Oracle {
                rom_id: rom_name.to_string(),
                id: next_id,
                z: t.z_axis.address,
                rows: r,
                cols: c,
                trust: tr,
                layout_valid: address_plausible(&rom, t.z_axis.address, r, c, bits),
                shape_scanned: shapes.contains(&(r.min(48), c.min(48))),
            });
            next_id += 1;
        }
    }

    println!("=== Phase 1: pair verification ===");
    println!("{:<44} {:<40} {:>10}  reason", "binary", "xdf", "status");
    for (b, x, t, r) in &matrix {
        println!(
            "{:<44} {:<40} {:>10}  {}",
            b.chars().take(44).collect::<String>(),
            x.chars().take(40).collect::<String>(),
            format!("{t:?}"),
            r
        );
    }
    let counts = |v: &[Oracle], t: Trust| v.iter().filter(|o| o.trust == t).count();
    let lv = |v: &[Oracle]| {
        v.iter()
            .filter(|o| o.layout_valid && matches!(o.trust, Trust::Verified | Trust::Probable))
            .count()
    };
    println!(
        "\n2-D oracles: {} total | VERIFIED {} | PROBABLE {} | UNVERIFIED {} | INVALID {} | VERIFIED+layout-valid {}",
        oracles.len(),
        counts(&oracles, Trust::Verified),
        counts(&oracles, Trust::Probable),
        counts(&oracles, Trust::Unverified),
        counts(&oracles, Trust::Invalid),
        lv(&oracles)
    );

    // ---- Phase 3: raw attribution ---------------------------------------
    // Scan each distinct ROM once, keyed by binary path.
    let mut rom_cache: HashMap<String, (Vec<DiscoveredFullTable>, Vec<usize>)> = HashMap::new();
    let mut by_rom: HashMap<String, Vec<&Oracle>> = HashMap::new();
    for o in &oracles {
        by_rom.entry(o.rom_id.clone()).or_default().push(o);
    }

    for p in &pairs {
        let rom_name = p.binary.rsplit('/').next().unwrap_or("").to_string();
        if rom_cache.contains_key(&rom_name) || !by_rom.contains_key(&rom_name) {
            continue;
        }
        let Ok(rom) = std::fs::read(format!("{DB}/{}", p.binary)) else { continue };
        let raw = scan_16bit_tables_auto_endian(&rom, &shapes, 0, rom.len());
        let mut starts: Vec<usize> = raw.iter().map(|t| t.z_address).collect();
        starts.sort_unstable();
        starts.dedup();
        rom_cache.insert(rom_name, (raw, starts));
    }

    // Score every raw candidate exactly as production NMS does.
    let score_of = |t: &DiscoveredFullTable| -> f64 {
        let flat = t
            .sample_matrix
            .first()
            .map(|row| {
                let lo = row.iter().copied().fold(f64::INFINITY, f64::min);
                let hi = row.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                (hi - lo).abs() < 1e-4
            })
            .unwrap_or(false);
        #[allow(unused)]
        let axes_contiguous = match (&t.x_axis, &t.y_axis) {
            (Some(x), Some(y)) => {
                let xs = t.cols * if x.is_16bit { 2 } else { 1 };
                let ys = t.rows * if y.is_16bit { 2 } else { 1 };
                x.address + xs == t.z_address && y.address + ys == x.address
            }
            _ => false,
        };
        composite_quality_score(&ScoreInputs {
            rows: t.rows,
            cols: t.cols,
            z_address: t.z_address,
            smoothness: t.smoothness,
            confidence_score: t.confidence_score,
            has_x_axis: t.x_axis.is_some(),
            has_y_axis: t.y_axis.is_some(),
            axes_contiguous,
            first_row_is_flat: flat,
        })
    };

    let bytes_of = |t: &DiscoveredFullTable| match t.data_format {
        MapDataBits::Bits8 => 1usize,
        MapDataBits::Bits16(_) => 2,
        MapDataBits::Bits32(_) => 4,
    };

    let mut stage: HashMap<usize, (Option<usize>, String)> = HashMap::new();
    let mut nms_survived = 0usize;
    let mut nms_suppressed = 0usize;
    let mut suppressed_reason: HashMap<&'static str, usize> = HashMap::new();
    let mut sup_is_oracle: (usize, usize) = (0, 0);

    for o in &oracles {
        let Some((raw, starts)) = rom_cache.get(&o.rom_id) else {
            stage.insert(o.id, (None, "not detected: ROM not scanned".into()));
            continue;
        };
        // Nearest candidate start, up to 8 bytes.
        let i = starts.partition_point(|&s| s < o.z);
        let mut best: Option<(usize, usize)> = None;
        for &s in &starts[i.saturating_sub(2)..(i + 2).min(starts.len())] {
            let d = s.abs_diff(o.z);
            if best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, s));
            }
        }
        let Some((dist, cand_addr)) = best else {
            stage.insert(o.id, (None, "not detected: no candidate".into()));
            continue;
        };
        if dist > 8 {
            stage.insert(o.id, (None, format!("not detected: nearest candidate {dist} B away")));
            continue;
        }

        // All candidates at that start address.
        let idxs: Vec<usize> = raw
            .iter()
            .enumerate()
            .filter(|(_, t)| t.z_address == cand_addr)
            .map(|(i, _)| i)
            .collect();

        // Choose the candidate that best matches the oracle's dimensions.
        let best_idx = idxs
            .iter()
            .copied()
            .min_by_key(|&i| {
                let t = &raw[i];
                t.rows.abs_diff(o.rows) + t.cols.abs_diff(o.cols) + t.z_address.abs_diff(o.z)
            })
            .unwrap();

        // Replay production NMS acceptance over the same set.
        let mut order: Vec<usize> = (0..raw.len()).collect();
        order.sort_by(|&a, &b| {
            score_of(&raw[b])
                .partial_cmp(&score_of(&raw[a]))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut accepted: Vec<usize> = Vec::with_capacity(order.len());
        let mut suppressor: HashMap<usize, (usize, usize, f64)> = HashMap::new();
        for &c in &order {
            let clen = raw[c].rows * raw[c].cols * bytes_of(&raw[c]);
            let cs = raw[c].z_address;
            let ce = cs + clen;
            let mut found: Option<(usize, usize, f64)> = None;
            for &a in &accepted {
                let alen = raw[a].rows * raw[a].cols * bytes_of(&raw[a]);
                let as_ = raw[a].z_address;
                let ae = as_ + alen;
                let ov = ce.min(ae).saturating_sub(cs.max(as_));
                let ratio = if clen > 0 { ov as f64 / clen as f64 } else { 0.0 };
                if ratio > 0.0 {
                    found = Some((a, ov, ratio));
                    break;
                }
            }
            match found {
                Some(f) => {
                    suppressor.insert(c, f);
                }
                None => accepted.push(c),
            }
        }

        let survivors: HashSet<usize> = accepted.iter().copied().collect();
        if survivors.contains(&best_idx) {
            nms_survived += 1;
            stage.insert(
                o.id,
                (
                    Some(cand_addr),
                    format!("NMS survived (dist {dist} B, {}x{})", raw[best_idx].rows, raw[best_idx].cols),
                ),
            );
        } else {
            nms_suppressed += 1;
            let (sup, ov, ratio) = suppressor[&best_idx];
            let kind = if raw[sup].z_address <= cand_addr
                && raw[sup].z_address + raw[sup].rows * raw[sup].cols * bytes_of(&raw[sup])
                    >= cand_addr + raw[best_idx].rows * raw[best_idx].cols * bytes_of(&raw[best_idx])
            {
                "contained by"
            } else {
                "overlapped by"
            };
            *suppressed_reason.entry(if kind == "contained by" { "containment" } else { "partial overlap" }).or_default() += 1;
            if matches!(o.trust, Trust::Verified | Trust::Probable) {
                let sup_addr = raw[sup].z_address;
                let same_rom = by_rom[&o.rom_id].iter();
                let hit = same_rom
                    .filter(|x| x.id != o.id)
                    .any(|x| x.z.abs_diff(sup_addr) <= 8);
                if hit {
                    sup_is_oracle.0 += 1;
                } else {
                    sup_is_oracle.1 += 1;
                }
            }
            stage.insert(
                o.id,
                (
                    Some(cand_addr),
                    format!(
                        "NMS suppressed: {kind} 0x{:06X} ({}x{}), overlap {ov} B = {:.0}%",
                        raw[sup].z_address, raw[sup].rows, raw[sup].cols, ratio * 100.0
                    ),
                ),
            );
        }
    }

    // ---- Phase 6: metrics ------------------------------------------------
    let eligible = |t: Trust| matches!(t, Trust::Verified | Trust::Probable);
    let report = |label: &str, filter: &dyn Fn(&Oracle) -> bool| {
        let sel: Vec<&Oracle> = oracles.iter().filter(|o| filter(o)).collect();
        let n = sel.len();
        let not_detected = sel.iter().filter(|o| stage[&o.id].0.is_none()).count();
        let detected = n - not_detected;
        let exact = sel
            .iter()
            .filter(|o| stage[&o.id].0 == Some(o.z))
            .count();
        let survived = sel.iter().filter(|o| stage[&o.id].1.starts_with("NMS survived")).count();
        let suppressed = sel
            .iter()
            .filter(|o| stage[&o.id].1.starts_with("NMS suppressed"))
            .count();
        let exact_post = sel
            .iter()
            .filter(|o| stage[&o.id].1.starts_with("NMS survived") && stage[&o.id].0 == Some(o.z))
            .count();
        println!("\n--- {label} ---");
        println!("eligible oracles      : {n}");
        println!("  not detected        : {not_detected} ({:.1}%)", 100.0 * not_detected as f64 / n.max(1) as f64);
        println!("  detected            : {detected} ({:.1}%)", 100.0 * detected as f64 / n.max(1) as f64);
        println!("    exact address     : {exact} ({:.1}%)", 100.0 * exact as f64 / n.max(1) as f64);
        println!("  NMS survived        : {survived} ({:.1}%)", 100.0 * survived as f64 / n.max(1) as f64);
        println!("  NMS suppressed      : {suppressed} ({:.1}%)", 100.0 * suppressed as f64 / detected.max(1) as f64);
        println!("  exact after NMS     : {exact_post} ({:.1}%)", 100.0 * exact_post as f64 / n.max(1) as f64);
    };

    println!("\n=== Phase 6: attribution ===");
    report("VERIFIED + layout valid", &|o| eligible(o.trust) && o.layout_valid);
    report("VERIFIED, shape IS scanned", &|o| eligible(o.trust) && o.shape_scanned);
    report("VERIFIED, shape NOT scanned", &|o| eligible(o.trust) && !o.shape_scanned);
    report("VERIFIED + PROBABLE pairs (any layout)", &|o| eligible(o.trust));
    report("ALL pairs (incl. INVALID, any layout)", &|_| true);
    report("INVALID only", &|o| o.trust == Trust::Invalid);

    println!("\nsuppression cause: {suppressed_reason:?}");
    println!(
        "suppressor address is another declared oracle: {} yes, {} no",
        sup_is_oracle.0, sup_is_oracle.1
    );
    println!("(nms_survived={nms_survived}, nms_suppressed={nms_suppressed})");

    // ---- Phase 7: family breakdown --------------------------------------
    println!("\n=== Phase 7: by ROM ===");
    let mut roms: Vec<&String> = by_rom.keys().collect();
    roms.sort();
    println!("{:<44} {:>6} {:>8} {:>8} {:>8}", "rom", "n", "nodet", "nmssup", "postNMS");
    for r in roms {
        let sel = &by_rom[r];
        let n = sel.len();
        let nd = sel.iter().filter(|o| stage[&o.id].0.is_none()).count();
        let sp = sel.iter().filter(|o| stage[&o.id].1.starts_with("NMS suppressed")).count();
        let sv = sel.iter().filter(|o| stage[&o.id].1.starts_with("NMS survived")).count();
        println!("{:<44} {:>6} {:>8} {:>8} {:>8}", r.chars().take(44).collect::<String>(), n, nd, sp, sv);
    }

    let _ = apply_memory_interval_nms(Vec::new(), 0.0);
    let _ = Endianness::BigEndian;
}
