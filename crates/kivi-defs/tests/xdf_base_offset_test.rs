//! XDF `BASEOFFSET` must be applied to every declared address.
//!
//! ## The bug this pins
//!
//! XDF `BASEOFFSET` converts an address written in the definition into a
//! file offset:
//!
//! ```text
//! file_offset = mmedaddress - offset - subtract
//! ```
//!
//! kiviTuner ignored the tag entirely, so every map in such a definition was
//! read from the wrong place. Measured on
//! `AUDI_RS3_TTRS_MED17_2_5TFSI_DAZA/404J_normal.xdf` (offset 9961472,
//! subtract 1) against its own ROM: **0 of 200** declared addresses land on
//! varying data when the tag is ignored, and **132 of 200** do once it is
//! applied. 42 XDF files in the database carry a non-zero `BASEOFFSET`.
//!
//! ## Why the assertion is on the offset, not just "it parses"
//!
//! A definition that parses but reads the wrong bytes is worse than one that
//! fails to parse: the tuner sees plausible numbers from an unrelated region.

use kivi_defs::xdf::parse_xdf_full;

/// A minimal definition whose only table sits at `mmedaddress`.
fn xdf_with_base_offset(offset: i64, subtract: i64, address: &str) -> String {
    format!(
        r#"<XDFFORMAT version="1.60">
  <XDFHEADER>
    <flags>0x1</flags>
    <deftitle>base offset fixture</deftitle>
    <BASEOFFSET offset="{offset}" subtract="{subtract}" />
    <DEFAULTS datasizeinbits="16" signdata="0" outputtype="1" signedoutput="0" />
  </XDFHEADER>
  <XDFTABLE uniqueid="0x1" flags="0x0">
    <title>t</title>
    <XDFAXIS id="x" uniqueid="0x0">
      <EMBEDDEDDATA mmedtypeflags="0x02" mmedaddress="0x0" mmedelementsizebits="16" />
      <indexcount>2</indexcount><datatype>22</datatype><unittype>0</unittype>
    </XDFAXIS>
    <XDFAXIS id="y" uniqueid="0x0">
      <EMBEDDEDDATA mmedtypeflags="0x02" mmedaddress="0x0" mmedelementsizebits="16" />
      <indexcount>2</indexcount><datatype>22</datatype><unittype>0</unittype>
    </XDFAXIS>
    <XDFAXIS id="z" uniqueid="0x0">
      <EMBEDDEDDATA mmedtypeflags="0x02" mmedaddress="{address}" mmedelementsizebits="16" />
      <MATH equation="X" />
    </XDFAXIS>
  </XDFTABLE>
</XDFFORMAT>"#
    )
}

#[test]
fn a_zero_base_offset_leaves_the_address_unchanged() {
    let defs = parse_xdf_full(&xdf_with_base_offset(0, 0, "0x1000")).expect("parse");
    assert_eq!(defs.maps[0].address, 0x1000);
}

#[test]
fn subtract_zero_adds_the_base_offset() {
    // The XDF subtract flag selects the DIRECTION, not merely whether to subtract:
    //     file_offset = address - base if subtract else address + base
    // 0x3000 + 0x1000 = 0x4000
    let defs = parse_xdf_full(&xdf_with_base_offset(0x1000, 0, "0x3000")).expect("parse");
    assert_eq!(
        defs.maps[0].address, 0x4000,
        "with subtract=0 the base is ADDED"
    );
}

#[test]
fn subtract_one_subtracts_the_base_offset() {
    // 0x3000 - 0x1000 = 0x2000
    let defs = parse_xdf_full(&xdf_with_base_offset(0x1000, 1, "0x3000")).expect("parse");
    assert_eq!(
        defs.maps[0].address, 0x2000,
        "with subtract=1 the base is SUBTRACTED"
    );
}

#[test]
fn a_result_below_zero_is_clamped_not_wrapped() {
    // A malformed definition can produce a negative offset; wrapping would read
    // unrelated bytes, so the value clamps to 0.
    let defs = parse_xdf_full(&xdf_with_base_offset(0x1000, 1, "0x10")).expect("parse");
    assert_eq!(defs.maps[0].address, 0, "must clamp, not wrap");
}

#[test]
fn the_real_med17_definition_lands_inside_its_own_rom() {
    let path = "/root/ecu_master_database/02_CARS/VAG_AUDI_VOLKSWAGEN/\
AUDI_RS3_TTRS_MED17_2_5TFSI_DAZA/404J_normal.xdf";
    let Ok(bytes) = std::fs::read(path) else {
        panic!("fixture missing: {path}");
    };
    let text = String::from_utf8_lossy(&bytes);
    let defs = parse_xdf_full(&text).expect("parse");
    assert!(!defs.maps.is_empty(), "fixture must declare maps");

    // The ROM that ships beside it is 507 904 bytes.
    let rom_len = 507_904usize;
    let inside = defs.maps.iter().filter(|m| m.address < rom_len).count();
    assert!(
        inside * 2 > defs.maps.len(),
        "after applying BASEOFFSET most maps must fall inside the 507 904-byte ROM, \
         got {inside} of {}",
        defs.maps.len()
    );
}

#[test]
fn every_med17_map_lands_inside_the_rom_and_most_on_real_data() {
    let d = "/root/ecu_master_database/02_CARS/VAG_AUDI_VOLKSWAGEN/\
AUDI_RS3_TTRS_MED17_2_5TFSI_DAZA";
    let xdf = std::fs::read(format!("{d}/404J_normal.xdf")).expect("fixture XDF");
    let rom = std::fs::read(format!("{d}/MED17_1_62_STOCK.bin")).expect("fixture BIN");
    let defs = parse_xdf_full(&String::from_utf8_lossy(&xdf)).expect("parse");

    assert!(defs.maps.len() > 3000, "expected thousands of maps");

    // Before BASEOFFSET was applied, none of these addresses fell inside the
    // 507 904-byte ROM; every read was out of bounds or from an unrelated file.
    let inside = defs.maps.iter().filter(|m| m.address < rom.len()).count();
    assert_eq!(
        inside,
        defs.maps.len(),
        "every map must land inside the ROM once BASEOFFSET is applied"
    );

    // And the addresses must point at real calibration data, not padding.
    let varying = defs
        .maps
        .iter()
        .filter(|m| {
            let end = (m.address + 32).min(rom.len());
            m.address + 32 <= rom.len() && rom[m.address..end].iter().any(|&b| b != rom[m.address])
        })
        .count();
    assert!(
        varying * 2 > defs.maps.len(),
        "most maps should sit on varying data, got {varying} of {}",
        defs.maps.len()
    );
}
