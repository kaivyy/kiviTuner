//! `BASEOFFSET` is not one rule: the tag appears with two incompatible meanings.
//!
//! ## The two cases, both measured on real files
//!
//! **Absolute addresses (MED17).** `404J_normal.xdf` declares
//! `offset=9961472 subtract=1` with addresses around `0x9B3123`. The ROM is
//! 507 904 bytes, so the raw address cannot be a file offset. Subtracting the
//! base puts 3 424 of 3 424 tables inside the ROM.
//!
//! **Already-relative addresses (MS43).** `Siemens_MS43_MS430069_ENG_512K_v1.16.xdf`
//! declares `offset=0x70000` (458 752) with addresses from `0x42` to `0xEE0A`.
//! The ROM is 524 288 bytes, so **all 3 782 addresses already fit**. Subtracting
//! the base makes every one of them negative, and clamping piles them all onto
//! offset 0 — which silently destroys the definition.
//!
//! ## The rule
//!
//! Apply the base only when the raw addresses do **not** fit the ROM. That is a
//! measurement of the file in front of us, not an assumption about the tool that
//! wrote it. The decision is recorded so callers can explain it.

use kivi_defs::xdf::parse_xdf_full;

const MED17: &str = "/root/ecu_master_database/02_CARS/VAG_AUDI_VOLKSWAGEN/\
AUDI_RS3_TTRS_MED17_2_5TFSI_DAZA/404J_normal.xdf";
const MED17_ROM: &str = "/root/ecu_master_database/02_CARS/VAG_AUDI_VOLKSWAGEN/\
AUDI_RS3_TTRS_MED17_2_5TFSI_DAZA/MED17_1_62_STOCK.bin";

const MS43: &str = "/root/ecu_master_database/02_CARS/BMW/SIEMENS_MS43/\
Siemens_MS43_MS430069_ENG_512K_v1.16.xdf";
const MS43_ROM: &str = "/root/ecu_master_database/02_CARS/BMW/SIEMENS_MS43/\
Siemens_MS43_MS430069_E46_M54B22_EU4_RHD.bin";

fn read(path: &str) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("fixture {path}: {e}"))
}

#[test]
fn absolute_addresses_are_shifted_down_by_the_base() {
    let xdf = String::from_utf8_lossy(&read(MED17)).into_owned();
    let rom = read(MED17_ROM);
    let defs = parse_xdf_full(&xdf).expect("parse");

    let inside = defs.maps.iter().filter(|m| m.address < rom.len()).count();
    assert_eq!(
        inside,
        defs.maps.len(),
        "MED17 addresses are absolute and must all land inside the ROM after shifting"
    );
}

#[test]
fn already_relative_addresses_are_left_alone() {
    let xdf = String::from_utf8_lossy(&read(MS43)).into_owned();
    let rom = read(MS43_ROM);
    let defs = parse_xdf_full(&xdf).expect("parse");

    let inside = defs.maps.iter().filter(|m| m.address < rom.len()).count();
    assert_eq!(
        inside,
        defs.maps.len(),
        "MS43 addresses already fit the ROM; shifting them destroys the definition"
    );

    // And they must not all collapse onto offset 0, which is what clamping does.
    let distinct: std::collections::HashSet<usize> = defs.maps.iter().map(|m| m.address).collect();
    assert!(
        distinct.len() > 100,
        "addresses must stay spread out, got {} distinct values",
        distinct.len()
    );
}

#[test]
fn the_ms43_first_table_still_points_at_real_data() {
    let xdf = String::from_utf8_lossy(&read(MS43)).into_owned();
    let rom = read(MS43_ROM);
    let defs = parse_xdf_full(&xdf).expect("parse");

    let first = defs.maps.iter().min_by_key(|m| m.address).expect("a map");
    assert!(
        first.address < rom.len(),
        "the lowest address must be inside the ROM, got {}",
        first.address
    );
}
