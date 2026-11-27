//! Blind-scan recall measured only on **identity-confirmed** pairs.
//!
//! ## Why this exists
//!
//! Every earlier recall number was measured against whatever definition happened
//! to sit in the same folder. For some pairs that definition describes a
//! different ROM entirely:
//!
//! * `CBR250RR-38770-K64-ND1_BIN.bin` carries `K64J108`; its XDF is titled
//!   `38770-K64-ND1`, and 41 of that XDF's 52 tables are declared 35 rows while
//!   the ROM's axis at that address has only 24 breakpoints.
//!
//! Measuring against such a pair says nothing about the detector. This suite
//! reads `docs/migration/PAIRING_INDEX.md` and keeps only pairs whose part
//! numbers actually match, so the number it reports is about the scanner.
//!
//! ## The index is data, not truth
//!
//! A pair listed as CONFIRMED means both sides carry a shared 6+ character
//! identifier. That makes a mismatch unlikely; it does not prove the definition
//! covers every table in the ROM, and it says nothing about table correctness.
//! Pairs listed as UNVERIFIED are skipped rather than counted as failures,
//! because a ROM with no identifier cannot be judged either way.


const INDEX: &str = "/root/kiviTuner/docs/migration/PAIRING_INDEX.md";
const DB: &str = "/root/ecu_master_database";

/// One row of the confirmed-pairs table.
struct ConfirmedPair {
    binary: String,
    definition: String,
    kind: String,
}

/// Parses the CONFIRMED section of the generated index.
fn load_confirmed() -> Vec<ConfirmedPair> {
    let text = std::fs::read_to_string(INDEX)
        .unwrap_or_else(|e| panic!("pairing index missing at {INDEX}: {e}"));
    let mut out = Vec::new();
    let mut in_confirmed = false;
    for line in text.lines() {
        if line.starts_with("## Identity-confirmed pairs") {
            in_confirmed = true;
            continue;
        }
        if line.starts_with("## ") && in_confirmed {
            break;
        }
        if !in_confirmed || !line.starts_with("| `") {
            continue;
        }
        let cells: Vec<&str> = line.split('|').map(|c| c.trim()).collect();
        if cells.len() < 5 {
            continue;
        }
        let binary = cells[1].trim_matches('`').to_string();
        let definition = cells[2].trim_matches('`').to_string();
        let kind = cells[3].to_string();
        out.push(ConfirmedPair {
            binary,
            definition,
            kind,
        });
    }
    out
}

#[test]
fn the_index_exists_and_lists_confirmed_pairs() {
    let pairs = load_confirmed();
    assert!(
        !pairs.is_empty(),
        "the pairing index must contain at least one confirmed pair"
    );
    eprintln!("confirmed pairs in index: {}", pairs.len());
}

#[test]
fn every_confirmed_pair_exists_on_disk() {
    for p in load_confirmed() {
        assert!(
            std::path::Path::new(&format!("{DB}/{}", p.binary)).exists(),
            "binary missing: {}",
            p.binary
        );
        assert!(
            std::path::Path::new(&format!("{DB}/{}", p.definition)).exists(),
            "definition missing: {}",
            p.definition
        );
    }
}

#[test]
fn blind_recall_on_confirmed_xdf_pairs() {
    use kivi_scanner::curve_scan::{scan_curves, CurveScanOptions, Endian};
    use kivi_scanner::xdf_parse::parse_xdf_xml;
    use kivi_scanner::zero_xdf_scan::{scan_16bit_tables_auto_endian, scan_8bit_tables};

    // Bound the run: a full blind scan of a multi-megabyte ROM takes seconds, so
    // 344 pairs would run for hours. A fixed sample keeps the suite usable and is
    // still large enough to be meaningful; the sample is deterministic.
    let all: Vec<ConfirmedPair> = load_confirmed()
        .into_iter()
        .filter(|p| p.kind.eq_ignore_ascii_case("XDF"))
        .collect();
    let pairs: Vec<ConfirmedPair> = all.into_iter().take(40).collect();
    eprintln!("scanning {} of the confirmed XDF pairs", pairs.len());
    if pairs.is_empty() {
        eprintln!("SKIP: no confirmed XDF pairs");
        return;
    }

    let mut total = 0usize;
    let mut matched = 0usize;
    let mut per_pair: Vec<(String, usize, usize)> = Vec::new();

    for (idx, p) in pairs.iter().enumerate() {
        eprintln!("  [{}/{}] {}", idx + 1, pairs.len(), p.binary.rsplit('/').next().unwrap_or(""));
        let Ok(rom) = std::fs::read(format!("{DB}/{}", p.binary)) else { continue };
        let Ok(xdf) = std::fs::read(format!("{DB}/{}", p.definition)) else { continue };
        let Ok(parsed) = parse_xdf_xml(&String::from_utf8_lossy(&xdf)) else { continue };

        // Include 1-D shapes as `(1, n)` so the curve detector is asked for them
        // too. Measured: of 753 declared Hayabusa tables only 364 were found by a
        // 2-D-only scan, and the misses were overwhelmingly `1xN` rows.
        let mut shapes: Vec<(usize, usize)> = parsed
            .tables
            .iter()
            .filter_map(|t| {
                let r = t.y_axis.as_ref().map(|a| a.index_count).unwrap_or(1);
                let c = t.x_axis.as_ref().map(|a| a.index_count).unwrap_or(1);
                if (r >= 2 && c >= 2) || (r == 1 && c >= 2) || (c == 1 && r >= 2) {
                    let (rr, cc) = (r.clamp(1, 48), c.clamp(1, 48));
                    Some((rr, cc))
                } else {
                    None
                }
            })
            .collect();
        shapes.sort_unstable();
        shapes.dedup();

        let mut addrs: Vec<usize> = parsed
            .tables
            .iter()
            .filter_map(|t| {
                let r = t.y_axis.as_ref().map(|a| a.index_count).unwrap_or(1);
                let c = t.x_axis.as_ref().map(|a| a.index_count).unwrap_or(1);
                if r >= 2 || c >= 2 {
                    Some(t.z_axis.address)
                } else {
                    None
                }
            })
            .collect();
        addrs.sort_unstable();
        addrs.dedup();
        if addrs.is_empty() || shapes.is_empty() {
            continue;
        }

        // The rectangular scanner only accepts shapes with both axes >= 2, so it
        // gets the 2-D subset; curves cover the `1xN` rows.
        let shapes_2d: Vec<(usize, usize)> = shapes
            .iter()
            .copied()
            .filter(|(r, c)| *r >= 2 && *c >= 2)
            .collect();
        let found = if shapes_2d.is_empty() {
            Vec::new()
        } else {
            let mut f = scan_16bit_tables_auto_endian(&rom, &shapes_2d, 0, rom.len());
            f.extend(scan_8bit_tables(&rom, &shapes_2d, 0, rom.len()));
            f
        };
        // Curve scanning covers the 1-D rows. The point range comes from the
        // declared 1-D shapes so a `1x6` curve is actually searched for; a fixed
        // 3..40 window missed nothing but wasted time on lengths nobody declared.
        let one_d: Vec<usize> = shapes
            .iter()
            .filter_map(|(r, c)| {
                if *r == 1 && *c >= 2 {
                    Some(*c)
                } else if *c == 1 && *r >= 2 {
                    Some(*r)
                } else {
                    None
                }
            })
            .collect();
        // Measured: narrowing this window to the declared 1-D lengths dropped
        // recall from 13.5% to 9.9%, so the wide window stays. A detector given
        // only the exact lengths has fewer places to anchor a run that is
        // truncated by a neighbouring structure.
        let _ = &one_d;
        let (min_pts, max_pts) = (3usize, 40usize);
        let mut curves = scan_curves(
            &rom,
            &CurveScanOptions {
                min_points: min_pts,
                max_points: max_pts,
                endian: Endian::Big,
                include_8bit: true,
                allow_monotonic: true,
                ..Default::default()
            },
        );
        curves.extend(scan_curves(
            &rom,
            &CurveScanOptions {
                min_points: min_pts,
                max_points: max_pts,
                endian: Endian::Little,
                include_8bit: false,
                allow_monotonic: true,
                ..Default::default()
            },
        ));

        let mut reported: Vec<usize> = found.iter().map(|t| t.z_address).collect();
        reported.extend(curves.iter().map(|c| c.address));
        reported.sort_unstable();

        let hit = addrs
            .iter()
            .filter(|a| {
                reported
                    .binary_search_by(|p| p.cmp(a))
                    .map(|i| {
                        let lo = i.saturating_sub(32);
                        let hi = (i + 32).min(reported.len());
                        reported[lo..hi].iter().any(|r| r.abs_diff(**a) <= 16)
                    })
                    .unwrap_or(false)
            })
            .count();

        total += addrs.len();
        matched += hit;
        per_pair.push((p.binary.clone(), hit, addrs.len()));
    }

    eprintln!("\n=== CONFIRMED PAIRS ONLY ===");
    let _ = &per_pair;
    for (bin, hit, of) in per_pair.iter().take(10) {
        let name = bin.rsplit('/').next().unwrap_or(bin);
        eprintln!("  {name:52} {hit:4}/{of:<4}");
    }
    eprintln!(
        "TOTAL: {matched} of {total} ({:.1}%) across {} confirmed pairs",
        if total > 0 { 100.0 * matched as f64 / total as f64 } else { 0.0 },
        per_pair.len()
    );

    assert!(total > 0, "at least one confirmed pair must yield tables");
}
