//! Parity with open-source XDF addressing rules.
//!
//! Two rules there differ from what kiviTuner originally assumed, and both are
//! measurable on real files rather than matters of taste.
//!
//! ## 1. `BASEOFFSET` selects a direction, not merely a subtraction
//!
//! ```text
//! file_offset = address - base   if subtract else
//!               address + base
//! ```
//!
//! | File | offset | subtract | always-subtract | repo rule |
//! |---|---:|---:|---:|---:|
//! | MED17 `404J_normal.xdf` | 9 961 472 | 1 | 3 424/3 424 in ROM | same |
//! | MS43 `..._ENG_512K` | 458 752 | 0 | **0** in ROM | **3 736 on varying data** |
//!
//! ## 2. A major stride equal to one element means *packed*
//!
//! ```text
//! row_step = cols*minor if major_stride in (0, size_bits) else major
//! ```
//!
//! Treating `major_stride == size_bits` as a one-element row pitch collapses a
//! multi-column table into one cell per row.

use kivi_scanner::xdf_parse::parse_xdf_xml;

fn xdf(major: i32, minor: i32, rows: usize, cols: usize, addr: &str) -> String {
    format!(
        r#"<XDFFORMAT version="1.60">
  <XDFHEADER>
    <deftitle>parity</deftitle>
    <DEFAULTS datasizeinbits="16" signdata="0" outputtype="1" signedoutput="0" />
    <BASEOFFSET offset="0" subtract="0" />
    <CATEGORY index="0x0" name="Root" />
  </XDFHEADER>
  <XDFTABLE uniqueid="0x1" flags="0x0">
    <title>t</title>
    <XDFAXIS id="x"><EMBEDDEDDATA mmedtypeflags="0x0" mmedaddress="0x0" mmedelementsizebits="16" /><indexcount>{cols}</indexcount><MATH equation="X" /></XDFAXIS>
    <XDFAXIS id="y"><EMBEDDEDDATA mmedtypeflags="0x0" mmedaddress="0x0" mmedelementsizebits="16" /><indexcount>{rows}</indexcount><MATH equation="X" /></XDFAXIS>
    <XDFAXIS id="z"><EMBEDDEDDATA mmedtypeflags="0x0" mmedaddress="{addr}" mmedelementsizebits="16" mmedrowcount="{rows}" mmedcolcount="{cols}" mmedmajorstridebits="{major}" mmedminorstridebits="{minor}" /><MATH equation="X" /></XDFAXIS>
  </XDFTABLE>
</XDFFORMAT>"#
    )
}

/// Same builder with an explicit element width.
fn xdf_bits(bits: usize, major: i32, minor: i32, rows: usize, cols: usize, addr: &str) -> String {
    xdf(major, minor, rows, cols, addr).replace(
        "mmedelementsizebits=\"16\"",
        &format!("mmedelementsizebits=\"{bits}\""),
    )
}

fn rom_with(values: &[u16]) -> Vec<u8> {
    let mut rom = vec![0u8; 0x200];
    for (i, v) in values.iter().enumerate() {
        rom[0x40 + i * 2..0x42 + i * 2].copy_from_slice(&v.to_be_bytes());
    }
    rom
}

#[test]
fn a_major_stride_equal_to_one_element_means_packed() {
    // 3x2 table, major stride = 16 bits = one element. Standard XDF reads this as
    // packed rows, so row 1 starts right after row 0.
    let parsed = parse_xdf_xml(&xdf(16, 0, 2, 3, "0x40")).expect("parse");
    let t = &parsed.tables[0];
    let rom = rom_with(&[10, 11, 12, 20, 21, 22]);

    let m = kivi_scanner::xdf_parse::extract_table_matrix_from_bin(&rom, t).expect("extract");
    assert_eq!(
        m,
        vec![vec![10.0, 11.0, 12.0], vec![20.0, 21.0, 22.0]],
        "major_stride == element size must be treated as packed"
    );
}

#[test]
fn a_major_stride_of_zero_is_packed() {
    let parsed = parse_xdf_xml(&xdf(0, 0, 2, 3, "0x40")).expect("parse");
    let t = &parsed.tables[0];
    let rom = rom_with(&[10, 11, 12, 20, 21, 22]);
    let m = kivi_scanner::xdf_parse::extract_table_matrix_from_bin(&rom, t).expect("extract");
    assert_eq!(m, vec![vec![10.0, 11.0, 12.0], vec![20.0, 21.0, 22.0]]);
}

#[test]
fn a_larger_major_stride_pads_the_rows() {
    // 2x2 table with 8-byte row pitch: row 1 starts at 0x48, not 0x44.
    let parsed = parse_xdf_xml(&xdf(64, 0, 2, 2, "0x40")).expect("parse");
    let t = &parsed.tables[0];
    let mut rom = vec![0u8; 0x200];
    for (at, v) in [(0x40usize, 10u16), (0x42, 11), (0x48, 20), (0x4A, 21)] {
        rom[at..at + 2].copy_from_slice(&v.to_be_bytes());
    }
    let m = kivi_scanner::xdf_parse::extract_table_matrix_from_bin(&rom, t).expect("extract");
    assert_eq!(m, vec![vec![10.0, 11.0], vec![20.0, 21.0]]);
}

#[test]
fn a_minor_stride_of_one_element_is_packed_columns() {
    let parsed = parse_xdf_xml(&xdf(0, 16, 2, 3, "0x40")).expect("parse");
    let t = &parsed.tables[0];
    let rom = rom_with(&[10, 11, 12, 20, 21, 22]);
    let m = kivi_scanner::xdf_parse::extract_table_matrix_from_bin(&rom, t).expect("extract");
    assert_eq!(m, vec![vec![10.0, 11.0, 12.0], vec![20.0, 21.0, 22.0]]);
}

#[test]
fn the_ms43_definition_now_reaches_real_data() {
    let d = "/root/ecu_master_database/02_CARS/BMW/SIEMENS_MS43";
    let xdf = std::fs::read(format!("{d}/Siemens_MS43_MS430069_ENG_512K_v1.16.xdf"))
        .expect("fixture XDF");
    let rom = std::fs::read(format!("{d}/Siemens_MS43_MS430069_E46_M54B22_EU4_RHD.bin"))
        .expect("fixture BIN");
    let p = parse_xdf_xml(&String::from_utf8_lossy(&xdf)).expect("parse");

    // The definition declares offset 0x70000 with subtract 0, so the base is
    // ADDED. Before this was corrected every address became negative, clamped to
    // zero, and the whole definition collapsed onto one offset.
    let inside = p.tables.iter().filter(|t| t.z_axis.address < rom.len()).count();
    assert_eq!(inside, p.tables.len(), "every table must land inside the ROM");

    let distinct: std::collections::HashSet<usize> =
        p.tables.iter().map(|t| t.z_axis.address).collect();
    assert!(
        distinct.len() > 1000,
        "addresses must stay spread out, got {} distinct",
        distinct.len()
    );

    let varying = p
        .tables
        .iter()
        .filter(|t| {
            let a = t.z_axis.address;
            a + 32 <= rom.len() && rom[a..a + 32].iter().any(|&b| b != rom[a])
        })
        .count();
    assert!(
        varying > 300,
        "expected hundreds of tables on varying data, got {varying}"
    );
}

/// The two reference implementations disagree on `major_stride == size_bits`.
///
/// | Source | Rule when `major_stride == size_bits` |
/// |---|---|
/// | `XDF addressing reference` | packed rows |
/// | `ArtifactTuner` (`src/codec.js`) | one-element row pitch |
///
/// This module follows the Exporter. The reason is structural, and it is stated
/// as such because **no binary could be found to settle it empirically**: every
/// XDF in the database with this shape lacks a matching BIN (the only nearby
/// case is a Mazda BPL9 definition beside unrelated GM binaries).
///
/// The structural argument: a 16-row, 13-column, 8-bit table with a one-byte
/// row pitch would start every row inside the previous one, overlapping by 12 of
/// 13 bytes. That is not a table, so the packed reading is the only coherent one.
///
/// This test pins the choice so a future change is deliberate rather than
/// accidental, and records that it is unverified against firmware.
#[test]
fn a_major_stride_equal_to_one_element_is_read_as_packed_here() {
    // 8-bit cells with major_stride == 8 bits: this is the disputed case.
    let parsed = parse_xdf_xml(&xdf_bits(8, 8, 0, 16, 13, "0x40")).expect("parse");
    let t = &parsed.tables[0];

    let mut rom = vec![0u8; 0x400];
    // Fill 16x13 = 208 bytes with a ramp so a wrong layout is visible.
    for i in 0..208 {
        rom[0x40 + i] = (i % 200) as u8;
    }
    let m = kivi_scanner::xdf_parse::extract_table_matrix_from_bin(&rom, t).expect("extract");

    assert_eq!(m.len(), 16);
    assert_eq!(m[0].len(), 13);
    // Packed: row 1 starts 13 bytes after row 0.
    assert_eq!(m[0][0], rom[0x40] as f64);
    assert_eq!(m[1][0], rom[0x40 + 13] as f64);
    // A one-element pitch would have put row 1 at 0x41 instead.
    assert_ne!(
        m[1][0], rom[0x41] as f64,
        "row pitch must be cols*element, not one element"
    );
}
