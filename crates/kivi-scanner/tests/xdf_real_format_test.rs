//! Tests that the XDF parser accepts the format real tools emit.
//!
//! ## Why this exists
//!
//! The parser required `<XDFCATEGORY>`, but real XDF tools write `<CATEGORY>`.
//! Every real XDF therefore failed with "Missing mandatory <XDFCATEGORY> tag",
//! which meant auto-mapping could not read any community definition.
//!
//! Found by parsing a real CBR250RR XDF (1 MB ROM, Keihin SH7058, big-endian).

use kivi_scanner::xdf_parse::parse_xdf_xml;

/// A minimal but structurally complete XDF using standard `<CATEGORY>` tag names.
const REAL_FORMAT: &str = r#"<?xml version="1.0"?>
<XDFFORMAT version="1.70">
  <XDFHEADER>
    <deftitle>Test Definition</deftitle>
    <author>Tester</author>
    <DEFAULTS datasizeinbits="16" sigdigits="2" outputtype="1" signed="0" lsbfirst="0" float="0" />
    <REGION type="0xFFFFFFFF" startaddress="0x0" size="0x1000" regioncolor="0x0" regionflags="0x0" name="Binary File" desc="" />
    <CATEGORY index="0x0" name="Root" />
    <CATEGORY index="0x1" name="Fuel" />
    <CATEGORY index="0x2" name="Ignition" />
  </XDFHEADER>
  <XDFTABLE uniqueid="0x1" flags="0x0">
    <title>Fuel Main</title>
    <description>Main fuel table</description>
    <XDFAXIS id="x" uniqueid="0x0">
      <indexcount>4</indexcount>
      <EMBEDDEDDATA mmedtypeflags="0x2" mmedaddress="0x100" mmedelementsizebits="16" mmedmajorstridebits="-32" mmedminorstridebits="0" />
      <units>RPM</units>
      <MATH equation="X" />
    </XDFAXIS>
    <XDFAXIS id="y" uniqueid="0x0">
      <indexcount>3</indexcount>
      <EMBEDDEDDATA mmedtypeflags="0x2" mmedaddress="0x120" mmedelementsizebits="16" mmedmajorstridebits="-32" mmedminorstridebits="0" />
      <units>kPa</units>
      <MATH equation="X" />
    </XDFAXIS>
    <XDFAXIS id="z">
      <EMBEDDEDDATA mmedtypeflags="0x2" mmedaddress="0x200" mmedelementsizebits="16" mmedmajorstridebits="-64" mmedminorstridebits="16" />
      <MATH equation="X" />
    </XDFAXIS>
  </XDFTABLE>
</XDFFORMAT>
"#;

#[test]
fn a_standard_xdf_parses() {
    let parsed = parse_xdf_xml(REAL_FORMAT).expect("a real-format XDF must parse");
    assert_eq!(parsed.title, "Test Definition");
    assert_eq!(parsed.author, "Tester");
}

#[test]
fn categories_use_the_plain_category_tag() {
    let parsed = parse_xdf_xml(REAL_FORMAT).expect("parse");
    assert_eq!(parsed.categories.len(), 3, "three CATEGORY tags were declared");
    assert!(parsed.categories.iter().any(|(_, name)| name == "Fuel"));
    assert!(parsed.categories.iter().any(|(_, name)| name == "Ignition"));
}

#[test]
fn a_table_is_discovered() {
    let parsed = parse_xdf_xml(REAL_FORMAT).expect("parse");
    assert_eq!(parsed.tables.len(), 1);
    assert_eq!(parsed.tables[0].title, "Fuel Main");
}

#[test]
fn the_table_dimensions_come_from_the_axis_indexcounts() {
    let parsed = parse_xdf_xml(REAL_FORMAT).expect("parse");
    let t = &parsed.tables[0];
    assert_eq!(t.x_axis.as_ref().unwrap().index_count, 4);
    assert_eq!(t.y_axis.as_ref().unwrap().index_count, 3);
}

#[test]
fn the_table_addresses_come_from_embeddeddata() {
    let parsed = parse_xdf_xml(REAL_FORMAT).expect("parse");
    let t = &parsed.tables[0];
    assert_eq!(t.z_axis.address, 0x200);
    assert_eq!(t.x_axis.as_ref().unwrap().address, 0x100);
    assert_eq!(t.y_axis.as_ref().unwrap().address, 0x120);
}

#[test]
fn the_element_size_is_carried_through() {
    let parsed = parse_xdf_xml(REAL_FORMAT).expect("parse");
    assert_eq!(parsed.tables[0].z_axis.element_size_bits, 16);
}

#[test]
fn an_xdf_with_no_categories_still_parses() {
    // `<CATEGORY>` is optional in the XDF format. Real definitions from Suzuki
    // Hayabusa and Toyota Innova ship without one, and requiring it dropped
    // those pairs from every benchmark.
    let no_cat = REAL_FORMAT.replace("<CATEGORY", "<NOTCATEGORY");
    let parsed = parse_xdf_xml(&no_cat).expect("an XDF without categories must parse");
    assert!(parsed.categories.is_empty());
    assert_eq!(parsed.tables.len(), 1, "its tables must survive the missing category");
}

/// A second dialect, found in the Suzuki Hayabusa and Toyota Innova definitions:
/// `<EMBEDDED type="1" count="24" address="0x40000" />` instead of
/// `<EMBEDDEDDATA mmedaddress="..." mmedelementsizebits="16" />`, and no
/// `<indexcount>` — the count rides on the tag. Both were rejected outright.
const COMPACT_FORMAT: &str = r#"<?xml version="1.0"?>
<XDFFORMAT version="1.60">
  <XDFHEADER>
    <flags>0x1</flags>
    <description>Compact dialect</description>
    <BASEOFFSET offset="0" subtract="0" />
    <DEFAULTS datasizeinbits="16" outputtype="1" signed="0" />
  </XDFHEADER>
  <XDFTABLE uniqueid="0x1000">
    <title>Base Fuel Injection Map (TPS vs RPM)</title>
    <description>Primary Fueling matrix [24x28]</description>
    <categorymem index="0" category="Fuel Calibration" />
    <XDFAXIS id="x" uniqueid="0x0">
      <EMBEDDED type="1" count="24" address="0x40000" />
      <units>RPM</units>
    </XDFAXIS>
    <XDFAXIS id="y" uniqueid="0x0">
      <EMBEDDED type="1" count="28" address="0x40040" />
      <units>TPS %</units>
    </XDFAXIS>
    <XDFAXIS id="z" uniqueid="0x0">
      <EMBEDDED type="1" count="672" address="0x40100" />
      <datasizeinbits>16</datasizeinbits>
      <MATH equation="X*0.005" />
    </XDFAXIS>
  </XDFTABLE>
  <XDFCONSTANT uniqueid="0x1002">
    <title>Hard Rev Limiter (Engine RPM Cutoff)</title>
    <categorymem index="0" category="Rev &amp; Speed Limiters" />
    <EMBEDDED address="0x41200" type="1" datasizeinbits="16" />
    <MATH equation="X" />
  </XDFCONSTANT>
</XDFFORMAT>
"#;

#[test]
fn a_compact_embedded_xdf_parses() {
    let parsed = parse_xdf_xml(COMPACT_FORMAT).expect("the compact <EMBEDDED> dialect must parse");
    assert_eq!(parsed.tables.len(), 1);
    assert_eq!(parsed.constants.len(), 1);
}

#[test]
fn the_compact_axis_addresses_and_counts_are_read() {
    let parsed = parse_xdf_xml(COMPACT_FORMAT).expect("parse");
    let t = &parsed.tables[0];
    assert_eq!(t.z_axis.address, 0x40100, "z address comes from the `address` attribute");
    assert_eq!(t.x_axis.as_ref().unwrap().address, 0x40000);
    assert_eq!(t.y_axis.as_ref().unwrap().address, 0x40040);
    assert_eq!(t.x_axis.as_ref().unwrap().index_count, 24, "count comes from the `count` attribute");
    assert_eq!(t.y_axis.as_ref().unwrap().index_count, 28);
    assert_eq!(t.z_axis.element_size_bits, 16, "bits come from the <datasizeinbits> child");
}

/// `Anchors::from_rom` scans 32-bit big-endian words. Applying it to a
/// little-endian ROM would read every address backwards, so the anchor set has
/// to know which byte order it is reading.
#[test]
fn anchors_refuse_to_read_a_little_endian_rom_as_big_endian() {
    use kivi_scanner::pointer_snap::Anchors;

    // One big-endian pointer array: 0x1000, 0x1100, 0x1200, 0x1300 (stride 0x100).
    let mut rom = vec![0u8; 0x2000];
    for (i, target) in [0x1000u32, 0x1100, 0x1200, 0x1300].iter().enumerate() {
        rom[0x200 + i * 4..0x204 + i * 4].copy_from_slice(&target.to_be_bytes());
    }
    // The same words stored little-endian are not big-endian pointers.
    let mut le = rom.clone();
    for (i, target) in [0x1000u32, 0x1100, 0x1200, 0x1300].iter().enumerate() {
        le[0x200 + i * 4..0x204 + i * 4].copy_from_slice(&target.to_le_bytes());
    }

    let be = Anchors::from_rom(&rom, true);
    assert!(be.by_target.contains_key(&0x1100), "big-endian array must be anchored");

    let le_as_be = Anchors::from_rom(&le, true);
    assert!(
        !le_as_be.by_target.contains_key(&0x1100),
        "a little-endian table read as big-endian must yield no anchor"
    );
}

#[test]
fn an_xdf_with_no_table_is_rejected() {
    let no_table = REAL_FORMAT.replace("<XDFTABLE", "<NOTATABLE");
    assert!(parse_xdf_xml(&no_table).is_err());
}

#[test]
fn an_xdf_with_no_header_is_rejected() {
    let no_header = REAL_FORMAT.replace("<XDFHEADER>", "<NOTHEADER>");
    assert!(parse_xdf_xml(&no_header).is_err());
}
