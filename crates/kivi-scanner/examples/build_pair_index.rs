//! Builds a verified BIN <-> definition pairing index.
//!
//! Two independent checks, because neither alone is enough:
//!
//! | Check | Works for | Fails for |
//! |---|---|---|
//! | **Identity** — shared part number | Volvo 93%, Hyundai 90%, GM 58% | Nissan 0%, Honda-bike 0%, Yamaha 0% |
//! | **Content** — declared tables sit on smooth data | every brand | cannot distinguish two ROMs of one family |
//!
//! A pair is reported with the strongest evidence available: `IDENTITY` when the
//! part numbers match, `CONTENT` when only the data check passes, `REJECTED` when
//! both fail, and `INCONCLUSIVE` when the definition declares too few tables.
use kivi_scanner::pair_verify::verify_by_content;
use kivi_scanner::rom_identity::{compare_identity, IdentityMatch};
use kivi_scanner::xdf_parse::parse_xdf_xml;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

fn collect(dir: &Path, exts: &[&str], out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect(&p, exts, out);
        } else if let Some(ext) = p.extension().and_then(|s| s.to_str()) {
            if exts.contains(&ext.to_ascii_lowercase().as_str()) {
                out.push(p);
            }
        }
    }
}

/// `(address, rows, cols, bits, little_endian, major_stride_bits)`.
type TableSpec = (usize, usize, usize, usize, bool, i32);

fn main() {
    let db = Path::new("/root/ecu_master_database");
    let mut bins = Vec::new();
    let mut defs = Vec::new();
    collect(db, &["bin", "ori", "hex"], &mut bins);
    collect(db, &["xdf", "a2l", "dam", "kp", "ols"], &mut defs);
    eprintln!("{} binaries, {} definitions", bins.len(), defs.len());

    let mut by_dir: BTreeMap<PathBuf, Vec<&PathBuf>> = BTreeMap::new();
    for d in &defs {
        if let Some(parent) = d.parent() {
            by_dir.entry(parent.to_path_buf()).or_default().push(d);
        }
    }

    // Definitions are parsed once and reused. Without this cache the pass reads
    // and parses every XDF again for every binary in the folder: 6 310 binaries
    // times up to 40 definitions is a quarter of a million file parses, which is
    // why the run sat at "0/6310" with the CPU idle.
    let mut def_cache: BTreeMap<PathBuf, Option<Vec<TableSpec>>> = BTreeMap::new();

    let mut rows: Vec<(String, String, String, String, String, String)> = Vec::new();
    let mut counts = [0usize; 4]; // identity, content, rejected, inconclusive

    for (i, bin) in bins.iter().enumerate() {
        if i % 500 == 0 {
            eprintln!("  {i}/{}", bins.len());
        }
        let Ok(rom) = std::fs::read(bin) else { continue };
        let Some(dir) = bin.parent() else { continue };
        let Some(cands) = by_dir.get(dir) else { continue };
        let bin_rel = bin.strip_prefix(db).unwrap_or(bin).to_string_lossy().to_string();

        // Cap the candidates per binary. A folder can hold hundreds of XDFs and
        // running the content check against every one makes the pass quadratic
        // in practice; measured: 0 of 6 310 binaries in several minutes.
        let capped: Vec<&&PathBuf> = cands.iter().take(12).collect();
        for def in capped {
            let def = *def;
            let name = def.file_name().and_then(|s| s.to_str()).unwrap_or("");
            let def_rel = def.strip_prefix(db).unwrap_or(def).to_string_lossy().to_string();
            let kind = def
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_uppercase();

            let ident = compare_identity(&rom, name, name);
            let token = match &ident {
                IdentityMatch::Shared { token } => token.clone(),
                _ => String::new(),
            };

            // Content check only runs for XDF, the only format this crate parses.
            // Content checking is expensive (it decodes every declared table), so
            // it only runs when identity did not already settle the pair.
            let content = if kind == "XDF" && !ident.is_confirmed() {
                let cached = def_cache.entry(def.clone()).or_insert_with(|| {
                    std::fs::read(def).ok().and_then(|bytes| {
                        parse_xdf_xml(&String::from_utf8_lossy(&bytes)).ok().map(|parsed| {
                            parsed
                                .tables
                                .iter()
                                .filter_map(|t| {
                                    let r = t.y_axis.as_ref().map(|a| a.index_count).unwrap_or(1);
                                    let c = t.x_axis.as_ref().map(|a| a.index_count).unwrap_or(1);
                                    if r < 2 && c < 2 {
                                        return None;
                                    }
                                    Some((
                                        t.z_axis.address,
                                        r,
                                        c,
                                        t.z_axis.element_size_bits,
                                        t.z_axis.type_flags & 0x02 != 0,
                                        t.z_axis.major_stride_bits,
                                    ))
                                })
                                .collect::<Vec<_>>()
                        })
                    })
                });
                cached.as_ref().map(|tables| verify_by_content(&rom, tables))
            } else {
                None
            };

            let content_ok = content.as_ref().is_some_and(|v| v.is_consistent());
            let content_conclusive = content.as_ref().is_some_and(|v| v.conclusive);

            let verdict = if ident.is_confirmed() {
                counts[0] += 1;
                "IDENTITY"
            } else if content_ok {
                counts[1] += 1;
                "CONTENT"
            } else if content_conclusive {
                counts[2] += 1;
                "REJECTED"
            } else {
                counts[3] += 1;
                "INCONCLUSIVE"
            };

            let detail = match &content {
                Some(v) => format!(
                    "{}/{} smooth ({:.0}%)",
                    v.tables_smooth,
                    v.tables_in_rom,
                    100.0 * v.smooth_fraction
                ),
                None => String::new(),
            };
            let _ = name;
            rows.push((
                bin_rel.clone(),
                def_rel,
                kind,
                verdict.to_string(),
                token,
                detail,
            ));
        }
    }

    let out = "/root/kiviTuner/docs/migration/PAIRING_INDEX.md";
    let mut f = std::fs::File::create(out).expect("create");
    writeln!(f, "# BIN <-> Definition Pairing Index\n").unwrap();
    writeln!(f, "Generated by `cargo run --release -p kivi-scanner --example build_pair_index`.\n").unwrap();
    writeln!(f, "Two independent checks, because neither alone covers the corpus:\n").unwrap();
    writeln!(f, "| Check | Works for | Fails for |").unwrap();
    writeln!(f, "|---|---|---|").unwrap();
    writeln!(f, "| **IDENTITY** - shared part number | Volvo 93%, Hyundai 90%, GM 58% | Nissan 0%, Honda-bike 0%, Yamaha 0% |").unwrap();
    writeln!(f, "| **CONTENT** - declared tables sit on smooth data | every brand | cannot separate two ROMs of one family |").unwrap();
    writeln!(f, "\n| Verdict | Pairs |").unwrap();
    writeln!(f, "|---|---:|").unwrap();
    for (label, n) in [
        ("IDENTITY", counts[0]),
        ("CONTENT", counts[1]),
        ("REJECTED", counts[2]),
        ("INCONCLUSIVE", counts[3]),
    ] {
        writeln!(f, "| {label} | {n} |").unwrap();
    }

    for (title, want) in [
        ("Identity-confirmed pairs", "IDENTITY"),
        ("Content-confirmed pairs", "CONTENT"),
    ] {
        writeln!(f, "\n## {title}\n").unwrap();
        writeln!(f, "| Binary | Definition | Kind | Evidence |").unwrap();
        writeln!(f, "|---|---|---|---|").unwrap();
        for (bin, def, kind, verdict, token, detail) in &rows {
            if verdict != want {
                continue;
            }
            let ev = if !token.is_empty() {
                format!("token `{token}`")
            } else {
                detail.clone()
            };
            writeln!(f, "| `{bin}` | `{def}` | {kind} | {ev} |").unwrap();
        }
    }

    writeln!(f, "\n## Rejected pairs\n").unwrap();
    writeln!(f, "| Binary | Definition | Kind | Content |").unwrap();
    writeln!(f, "|---|---|---|---|").unwrap();
    for (bin, def, kind, verdict, _, detail) in &rows {
        if verdict == "REJECTED" {
            writeln!(f, "| `{bin}` | `{def}` | {kind} | {detail} |").unwrap();
        }
    }

    eprintln!(
        "wrote {out}: {} identity, {} content, {} rejected, {} inconclusive",
        counts[0], counts[1], counts[2], counts[3]
    );
}
