//! The scanner's own XDF parser must apply `BASEOFFSET` too.
//!
//! ## Why this is a separate test from the editor's
//!
//! kiviTuner has **two** XDF parsers:
//!
//! | Parser | Used by |
//! |---|---|
//! | `kivi_defs::xdf::parse_xdf_full` | the editor and map matrix read |
//! | `kivi_scanner::xdf_parse::parse_xdf_xml` | auto-mapping and blind tests |
//!
//! Fixing one does not fix the other. The editor fix was verified against a
//! real MED17 definition where **0 of 200** addresses landed correctly before
//! and **132 of 200** did after. This file pins the same behaviour on the
//! scanner's parser, which the 647-pair blind suite depends on.
//!
//! ## Measured scale
//!
//! 42 XDF files in the database carry a non-zero `BASEOFFSET`.

use kivi_scanner::xdf_parse::parse_xdf_xml;

/// A minimal definition whose only table sits at `mmedaddress`.
fn xdf_with_base_offset(offset: i64, subtract: i64, address: &str) -> String {
    format!(
        r#"<XDFFORMAT version="1.60">
  <XDFHEADER>
    <deftitle>scanner base offset fixture</deftitle>
    <DEFAULTS datasizeinbits="16" signdata="0" outputtype="1" signedoutput="0" />
    <BASEOFFSET offset="{offset}" subtract="{subtract}" />
    <CATEGORY index="0x0" name="Root" />
  </XDFHEADER>
  <XDFTABLE uniqueid="0x1" flags="0x0">
    <title>t</title>
    <XDFAXIS id="x" uniqueid="0x0">
      <EMBEDDEDDATA mmedtypeflags="0x0" mmedaddress="0x0" mmedelementsizebits="16" />
      <indexcount>2</indexcount><units>rpm</units><MATH equation="X" />
    </XDFAXIS>
    <XDFAXIS id="y" uniqueid="0x0">
      <EMBEDDEDDATA mmedtypeflags="0x0" mmedaddress="0x0" mmedelementsizebits="16" />
      <indexcount>2</indexcount><units>kpa</units><MATH equation="X" />
    </XDFAXIS>
    <XDFAXIS id="z">
      <EMBEDDEDDATA mmedtypeflags="0x0" mmedaddress="{address}" mmedelementsizebits="16" />
      <MATH equation="X" />
    </XDFAXIS>
  </XDFTABLE>
</XDFFORMAT>"#
    )
}

#[test]
fn a_zero_base_offset_leaves_the_address_unchanged() {
    let p = parse_xdf_xml(&xdf_with_base_offset(0, 0, "0x1000")).expect("parse");
    assert_eq!(p.tables[0].z_axis.address, 0x1000);
}

#[test]
fn subtract_zero_adds_the_table_address() {
    // The XDF subtract flag selects the direction:
    //   file_offset = address - base if subtract else address + base
    // Verified against standard XDF addressing rules and against a real MS43 ROM.
    let p = parse_xdf_xml(&xdf_with_base_offset(0x1000, 0, "0x3000")).expect("parse");
    assert_eq!(
        p.tables[0].z_axis.address, 0x4000,
        "with subtract=0 the base is ADDED"
    );
}

#[test]
fn subtract_one_subtracts_the_table_address() {
    let p = parse_xdf_xml(&xdf_with_base_offset(0x1000, 1, "0x3000")).expect("parse");
    assert_eq!(p.tables[0].z_axis.address, 0x2000);
}

#[test]
fn axis_addresses_are_shifted_by_the_same_base() {
    let xml = xdf_with_base_offset(0x1000, 0, "0x3000")
        .replace(r#"mmedaddress="0x0" mmedelementsizebits="16" />"#,
                 r#"mmedaddress="0x1100" mmedelementsizebits="16" />"#);
    let p = parse_xdf_xml(&xml).expect("parse");
    let x = p.tables[0].x_axis.as_ref().expect("x axis");
    assert_eq!(
        x.address, 0x2100,
        "with subtract=0 the axis base is added too (0x1100 + 0x1000)"
    );
}

#[test]
fn the_real_med17_definition_lands_inside_its_rom_after_base_offset() {
    let d = "/root/ecu_master_database/02_CARS/VAG_AUDI_VOLKSWAGEN/\
AUDI_RS3_TTRS_MED17_2_5TFSI_DAZA";
    let xdf = std::fs::read(format!("{d}/404J_normal.xdf")).expect("fixture XDF");
    let rom = std::fs::read(format!("{d}/MED17_1_62_STOCK.bin")).expect("fixture BIN");
    let parsed = parse_xdf_xml(&String::from_utf8_lossy(&xdf)).expect("parse");

    assert!(parsed.tables.len() > 1000, "expected thousands of tables");
    let inside = parsed
        .tables
        .iter()
        .filter(|t| t.z_axis.address < rom.len())
        .count();
    assert_eq!(
        inside,
        parsed.tables.len(),
        "every table must land inside the ROM once BASEOFFSET is applied"
    );
}
