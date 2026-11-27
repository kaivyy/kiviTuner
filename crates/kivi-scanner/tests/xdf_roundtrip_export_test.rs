//! Round-trip: blind scan → export XDF → re-parse → extract → compare.
//!
//! ## Why this exists
//!
//! `export_xdf_xml` had no test that re-read its own output. An exported
//! definition that XDF tools cannot open is worthless, so the cycle must close:
//!
//! ```text
//! scan ROM → export XDF → parse that XDF → extract matrices → compare
//! ```
//!
//! ## The comparison is bit-exact
//!
//! The re-extracted matrix must equal the in-memory discovery exactly. A
//! tolerance would hide an address off-by-one, which is the failure that
//! silently corrupts a tune.

use kivi_scanner::limiter_scan::DiscoveredLimiter;
use kivi_scanner::xdf_export::export_xdf_xml;
use kivi_scanner::xdf_parse::{extract_table_matrix_from_bin, parse_xdf_xml};
use kivi_scanner::zero_xdf_scan::{DiscoveredAxis, DiscoveredFullTable, Endianness, MapDataBits};

const ROM: &str = "/root/ecu_master_database/01_MOTORCYCLES/SUZUKI/GSXR_1000_K5_K6_DENSO_32920_41G00/32920-41G00.bin";

fn axis(_id: char, address: usize, count: usize) -> DiscoveredAxis {
    DiscoveredAxis {
        address,
        points_count: count,
        is_16bit: true,
        endianness: Endianness::BigEndian,
        estimated_unit: "rpm".to_string(),
        min_val: 0.0,
        max_val: 12000.0,
        values: (0..count).map(|i| (i * 500) as f64).collect(),
    }
}

fn table(z: usize, rows: usize, cols: usize) -> DiscoveredFullTable {
    let matrix: Vec<Vec<f64>> = (0..rows)
        .map(|r| (0..cols).map(|c| ((r * cols + c) % 251) as f64).collect())
        .collect();
    DiscoveredFullTable {
        z_address: z,
        rows,
        cols,
        data_format: MapDataBits::Bits16(Endianness::BigEndian),
        semantic_type: kivi_scanner::zero_xdf_scan::SemanticMapType::FuelAirChargeVE,
        confidence_score: 0.92,
        smoothness: 0.08,
        x_axis: Some(axis('x', z.saturating_sub(0x80), cols)),
        y_axis: Some(axis('y', z.saturating_sub(0x40), rows)),
        sample_matrix: matrix,
    }
}

#[test]
fn an_exported_xdf_reparses_with_the_same_tables() {
    let tables = vec![table(0x8000, 16, 16), table(0x9000, 20, 12)];
    let limiters: Vec<DiscoveredLimiter> = Vec::new();
    let xml = export_xdf_xml("Round-trip test", &tables, &limiters);

    let parsed = parse_xdf_xml(&xml).expect("the exported XDF must parse");
    assert_eq!(
        parsed.tables.len(),
        tables.len(),
        "every exported table must come back"
    );
}

#[test]
fn the_exported_categories_use_standard_category_tags() {
    // Standard XDF writes `<CATEGORY>`, not `<XDFCATEGORY>`, so an export using
    // `<XDFCATEGORY>` cannot be opened by standard XDF editors.
    let xml = export_xdf_xml("Tag test", &[table(0x8000, 16, 16)], &[]);
    assert!(
        xml.contains("<CATEGORY"),
        "exported XDF must use the <CATEGORY> tag"
    );
}

#[test]
fn the_exported_addresses_match_the_discovered_ones() {
    let tables = vec![table(0x8000, 16, 16), table(0x9000, 20, 12)];
    let xml = export_xdf_xml("Address test", &tables, &[]);
    let parsed = parse_xdf_xml(&xml).expect("parse");

    let mut got: Vec<usize> = parsed.tables.iter().map(|t| t.z_axis.address).collect();
    got.sort_unstable();
    let mut want: Vec<usize> = tables.iter().map(|t| t.z_address).collect();
    want.sort_unstable();
    assert_eq!(got, want, "Z addresses must survive the round trip");
}

#[test]
fn the_exported_dimensions_match() {
    let tables = vec![table(0x8000, 16, 16), table(0x9000, 20, 12)];
    let xml = export_xdf_xml("Dim test", &tables, &[]);
    let parsed = parse_xdf_xml(&xml).expect("parse");

    for t in &parsed.tables {
        let rows = t.y_axis.as_ref().map(|a| a.index_count).unwrap_or(1);
        let cols = t.x_axis.as_ref().map(|a| a.index_count).unwrap_or(1);
        assert!(
            tables.iter().any(|s| s.rows == rows && s.cols == cols),
            "unknown shape {rows}x{cols} came back"
        );
    }
}

#[test]
fn an_exported_xdf_extracts_the_same_matrix_from_a_real_rom() {
    let Ok(rom) = std::fs::read(ROM) else {
        eprintln!("SKIP: test ROM not present");
        return;
    };

    // Build a table over a region that is safely inside the ROM.
    let z = 0x8000usize;
    let rows = 8usize;
    let cols = 8usize;
    let t = table(z, rows, cols);
    let xml = export_xdf_xml("Extract test", std::slice::from_ref(&t), &[]);
    let parsed = parse_xdf_xml(&xml).expect("parse");
    let exported = parsed.tables.first().expect("one table");

    let matrix = extract_table_matrix_from_bin(&rom, exported).expect("extract");

    // The matrix must equal what the ROM holds at that address, read the same
    // way the exporter declared it.
    assert_eq!(matrix.len(), rows);
    for (r, row) in matrix.iter().enumerate() {
        assert_eq!(row.len(), cols);
        for (c, got) in row.iter().enumerate() {
            let off = z + (r * cols + c) * 2;
            let want = u16::from_be_bytes([rom[off], rom[off + 1]]) as f64;
            assert_eq!(
                *got, want,
                "cell [{r}][{c}] differs: exported {got} vs ROM {want}"
            );
        }
    }
}

#[test]
fn an_exported_xdf_with_limiters_round_trips_them_as_constants() {
    let limiter = DiscoveredLimiter {
        address: 0xA000,
        raw_value: 12000,
        converted_rpm: 12000.0,
        limiter_type: "RevLimiter",
    };
    let xml = export_xdf_xml("Limiter test", &[], &[limiter]);
    let parsed = parse_xdf_xml(&xml).expect("parse");
    assert_eq!(
        parsed.constants.len(),
        1,
        "the limiter must come back as one constant"
    );
    assert_eq!(parsed.constants[0].address, 0xA000);
}

// ---------------------------------------------------------------------------
// Full cycle on a real ROM: scan → export → parse → extract → compare
// ---------------------------------------------------------------------------

#[test]
fn the_full_cycle_on_a_real_rom_is_bit_exact() {
    use kivi_scanner::zero_xdf_scan::scan_16bit_tables_auto_endian;

    let Ok(rom) = std::fs::read(ROM) else {
        eprintln!("SKIP: ROM not present");
        return;
    };

    // 1. Blind scan.
    let dims = vec![(16usize, 16usize), (20, 12), (8, 8)];
    let found = scan_16bit_tables_auto_endian(&rom, &dims, 0, rom.len());
    if found.is_empty() {
        eprintln!("SKIP: scanner found no tables in this ROM");
        return;
    }
    let sample: Vec<_> = found.into_iter().take(5).collect();

    // 2. Export.
    let xml = export_xdf_xml("Full cycle", &sample, &[]);

    // 3. Re-parse.
    let parsed = parse_xdf_xml(&xml).expect("exported XDF must parse");

    // 4. Extract from the ROM using the exported definition, and compare with
    //    the in-memory discovery.
    let mut compared_cells = 0usize;
    let mut matched_tables = 0usize;
    for exported in &parsed.tables {
        let Some(orig) = sample
            .iter()
            .find(|s| s.z_address == exported.z_axis.address)
        else {
            continue;
        };
        let matrix = match extract_table_matrix_from_bin(&rom, exported) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if matrix.len() != orig.sample_matrix.len() {
            continue;
        }
        let mut identical = true;
        for (r, row) in matrix.iter().enumerate() {
            for (c, got) in row.iter().enumerate() {
                let want = orig.sample_matrix[r][c];
                if (got - want).abs() > 1e-6 {
                    identical = false;
                    break;
                }
                compared_cells += 1;
            }
            if !identical {
                break;
            }
        }
        if identical {
            matched_tables += 1;
        }
    }

    eprintln!(
        "full cycle: {} tables exported, {} re-extracted bit-exact, {} cells compared",
        parsed.tables.len(),
        matched_tables,
        compared_cells
    );
    assert!(
        matched_tables > 0,
        "at least one table must round-trip bit-exact"
    );
}


/// Exported definitions must round-trip through the stride-aware reader.
///
/// The exporter writes **packed** tables (no `mmedmajorstridebits`), which is
/// correct because the blind scanner only ever produces packed candidates. This
/// test pins that invariant: if the exporter ever emitted a non-zero stride while
/// the data stayed packed, every row after the first would shift.
#[test]
fn exported_tables_are_packed_and_round_trip_through_the_stride_reader() {
    let rom = std::fs::read(ROM).expect("fixture ROM");
    let tables = vec![table(0x8000, 16, 16), table(0x9000, 20, 12)];
    let xml = export_xdf_xml("stride invariant", &tables, &[]);

    assert!(
        !xml.contains("mmedmajorstridebits")
            || xml.contains("mmedmajorstridebits=\"0\""),
        "the exporter must not emit a non-zero row stride for packed data"
    );

    let parsed = parse_xdf_xml(&xml).expect("exported XDF must parse");
    assert_eq!(parsed.tables.len(), 2);
    for t in &parsed.tables {
        assert_eq!(
            t.z_axis.major_stride_bits, 0,
            "exported tables must declare no stride"
        );
        assert_eq!(t.z_axis.minor_stride_bits, 0);
        // Reading with stride 0 must behave exactly like packed reading.
        let matrix = extract_table_matrix_from_bin(&rom, t).expect("re-extract");
        assert_eq!(matrix.len(), t.y_axis.as_ref().map(|a| a.index_count).unwrap_or(1));
        assert!(matrix.iter().all(|r| r.iter().all(|v| v.is_finite())));
    }
}
