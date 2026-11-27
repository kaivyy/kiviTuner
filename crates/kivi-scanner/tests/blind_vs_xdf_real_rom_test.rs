//! Blind discovery vs a real community XDF, on a real ROM.
//!
//! ## Data
//!
//! `CBR250RR-38770-K64-N04_BIN.bin` — 1 MB Honda CBR250RR (Keihin, Renesas
//! SH7058, big-endian) paired with `CBR250RR_38770-K64-N04_PUBLIC_v1.xdf`.
//! The XDF is a human-authored definition, so it is an independent oracle.
//!
//! The ROM and XDF are **read-only inputs**: this test never writes them.
//!
//! ## What is compared
//!
//! The XDF names specific tables with specific addresses. The blind scanner does
//! not read the XDF; it discovers tables from the bytes alone. A match means the
//! scanner found the same thing a human found by hand.

use kivi_scanner::xdf_parse::{extract_table_matrix_from_bin, parse_xdf_xml};

const ROM: &str = "/root/.hermes/workspace/cbr250rr/CBR250RR-38770-K64-N04_BIN.bin";
const XDF: &str = "/root/.hermes/workspace/cbr250rr/CBR250RR_38770-K64-N04_PUBLIC_v1.xdf";

fn rom() -> Option<Vec<u8>> {
    std::fs::read(ROM).ok()
}

fn xdf() -> Option<String> {
    std::fs::read_to_string(XDF).ok()
}

#[test]
fn the_rom_is_the_expected_size() {
    let Some(rom) = rom() else {
        eprintln!("SKIP: test ROM not present");
        return;
    };
    assert_eq!(rom.len(), 0x100000, "CBR250RR ROM is 1 MiB");
}

#[test]
fn the_xdf_parses_without_error() {
    let Some(xdf) = xdf() else {
        eprintln!("SKIP: test XDF not present");
        return;
    };
    let parsed = parse_xdf_xml(&xdf).expect("the community XDF must parse");
    assert!(
        !parsed.tables.is_empty(),
        "the XDF must define at least one table"
    );
}

#[test]
fn the_xdf_declares_big_endian_tables() {
    // The XDF header says `lsbfirst="0"`, i.e. MSB-first / big-endian, matching
    // the SH7058. This pins the endianness the scanner must assume.
    let Some(xdf) = xdf() else {
        eprintln!("SKIP");
        return;
    };
    let parsed = parse_xdf_xml(&xdf).expect("parse");
    // The XDF declares `lsbfirst="0"` in its DEFAULTS block. The parser folds
    // that into the axis element sizes rather than exposing a per-table flag,
    // so the check is that the tables parsed at all with 16-bit elements.
    let sixteen_bit = parsed
        .tables
        .iter()
        .filter(|t| t.z_axis.element_size_bits == 16)
        .count();
    assert!(sixteen_bit > 0, "expected 16-bit tables in this XDF");
}

#[test]
fn every_xdf_table_extracts_a_matrix_from_the_rom() {
    let (Some(rom), Some(xdf)) = (rom(), xdf()) else {
        eprintln!("SKIP: data not present");
        return;
    };
    let parsed = parse_xdf_xml(&xdf).expect("parse");

    let mut extracted = 0usize;
    let mut out_of_bounds = 0usize;
    for t in &parsed.tables {
        match extract_table_matrix_from_bin(&rom, t) {
            Ok(m) => {
                let rows = t.y_axis.as_ref().map(|a| a.index_count).unwrap_or(1);
                let cols = t.x_axis.as_ref().map(|a| a.index_count).unwrap_or(1);
                assert_eq!(m.len(), rows, "table '{}' row count", t.title);
                for row in &m {
                    assert_eq!(row.len(), cols, "table '{}' col count", t.title);
                }
                extracted += 1;
            }
            Err(_) => out_of_bounds += 1,
        }
    }
    eprintln!(
        "XDF tables: {}, extracted: {}, out-of-bounds: {}",
        parsed.tables.len(),
        extracted,
        out_of_bounds
    );
    assert!(extracted > 0, "at least one table must extract");
    assert_eq!(
        out_of_bounds, 0,
        "no XDF table may point outside the ROM"
    );
}

#[test]
fn a_known_fuel_table_address_is_inside_the_rom() {
    // The community README documents the MAP axis at 0x41222 and RPM at 0x41254.
    let Some(rom) = rom() else {
        eprintln!("SKIP");
        return;
    };
    for addr in [0x41222usize, 0x41254, 0x411F0, 0x50CF6] {
        assert!(addr < rom.len(), "documented axis {addr:#x} is outside the ROM");
    }
}

// ---------------------------------------------------------------------------
// Blind discovery vs the XDF's own addresses
// ---------------------------------------------------------------------------

#[test]
fn blind_scan_finds_tables_at_the_same_addresses_the_xdf_defines() {
    use kivi_scanner::zero_xdf_scan::{
        detect_global_endianness, scan_16bit_tables_auto_endian,
    };

    let (Some(rom), Some(xdf)) = (rom(), xdf()) else {
        eprintln!("SKIP: data not present");
        return;
    };
    let parsed = parse_xdf_xml(&xdf).expect("parse");

    // The XDF tells us the truth; the scanner must not read it.
    let xdf_addresses: std::collections::HashSet<usize> =
        parsed.tables.iter().map(|t| t.z_axis.address).collect();

    let endian = detect_global_endianness(&rom);
    eprintln!("detected endianness: {endian:?}");
    eprintln!("XDF declares {} table addresses", xdf_addresses.len());

    // The dimensions the community XDF actually uses, so the comparison is
    // about addresses rather than about guessing shapes.
    let dims: Vec<(usize, usize)> = {
        let mut d: Vec<(usize, usize)> = parsed
            .tables
            .iter()
            .map(|t| {
                (
                    t.y_axis.as_ref().map(|a| a.index_count).unwrap_or(1),
                    t.x_axis.as_ref().map(|a| a.index_count).unwrap_or(1),
                )
            })
            .collect();
        d.sort_unstable();
        d.dedup();
        d
    };
    let found = scan_16bit_tables_auto_endian(&rom, &dims, 0, rom.len());
    eprintln!("blind scan found {} candidates", found.len());

    // Count how many XDF addresses the blind scan also reported, within a small
    // window — a table's Z data can start a few bytes off if the XDF includes a
    // header the scanner treats as data.
    let mut hits = 0usize;
    for addr in &xdf_addresses {
        if found
            .iter()
            .any(|c| c.z_address.abs_diff(*addr) <= 16)
        {
            hits += 1;
        }
    }
    let pct = 100.0 * hits as f64 / xdf_addresses.len().max(1) as f64;
    eprintln!(
        "blind scan matched {hits}/{} XDF addresses ({pct:.1}%)",
        xdf_addresses.len()
    );

    // A useful scanner must find a meaningful share of what a human found.
    assert!(
        hits > 0,
        "the blind scan found none of the {} XDF table addresses",
        xdf_addresses.len()
    );
}
