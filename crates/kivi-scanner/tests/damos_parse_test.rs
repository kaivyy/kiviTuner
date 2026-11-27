//! DAMOS (`.dam`) parsing — the Bosch calibration description format.
//!
//! ## Format
//!
//! DAMOS is a plain-text block language created by `ASAP2DAM`. Header directives
//! start a line with `/TAG,`; calibration objects are numbered entries that begin
//! with an index and a type tag:
//!
//! ```text
//! 225, /SRC, vpsspu_w, {Solldruckverhaeltnis}, $382342, 2, 93, 0, 3;
//! /UMP, {}, abak, {Aufteilungsfaktor Wandfilm}, $38487C, 513, 71, fak_ub_b1, 3, $FF, K;
//! ```
//!
//! `/UMP` is a map. Fields after the description:
//! `address, dimensions, ???, conversion, ???, mask, class`.
//!
//! The dimension word packs **rows in the high byte and columns in the low
//! byte**: `513 = 0x0201` is 2 rows × 1 column, `529 = 0x0211` is 2 × 17.
//!
//! ## Why this matters
//!
//! DAMOS is what Bosch and the OEM tool chain ship. It is plaintext — unlike most
//! `.kp` packs — so it can be read completely.

use kivi_scanner::damos_parse::{parse_damos, DamosError};

const SAMPLE: &str = r#"*** Created by ASAP2DAM Version 6.26 ***
/EPR, {X505R24BO}, {4019.XX}, DAMPAR_03
/EAD, $810005
/PNR, {X505R24BO}
/SGB, {ME7.5}
;
1, /SPZ, A0, {Uebertragungsfunktionskoeffizient}, 1, $81609C, $81609C
/SPW, 57, -2, -0.0625, 0.06249809
/FKX, 0, 0, 0
/ABL, 0;
/UMP, {}, abak, {Aufteilungsfaktor Wandfilm bei BA}, $38487C, 513, 71, fak_ub_b1, 3, $FF, K;
/UMP, {}, adastep, {Adaptionsschritt fuer STADAP-Faktor}, $380A25, 529, 52, fak_sb_b1, 3, $FF, K;
223, /SRC, tumg, {Umgebungstemperatur}, $384885, 1, 408, 0, 3;
"#;

#[test]
fn the_header_identifies_the_ecu() {
    let doc = parse_damos(SAMPLE).expect("parse");
    assert_eq!(doc.ecu.as_deref(), Some("X505R24BO"));
}

#[test]
fn the_ecu_family_is_captured() {
    let doc = parse_damos(SAMPLE).expect("parse");
    assert_eq!(doc.family.as_deref(), Some("ME7.5"));
}

#[test]
fn the_base_address_is_decoded() {
    let doc = parse_damos(SAMPLE).expect("parse");
    assert_eq!(doc.base_address, Some(0x810005));
}

#[test]
fn two_maps_are_found() {
    let doc = parse_damos(SAMPLE).expect("parse");
    assert_eq!(doc.maps.len(), 2);
}

#[test]
fn a_map_name_and_address_are_captured() {
    let doc = parse_damos(SAMPLE).expect("parse");
    let m = &doc.maps[0];
    assert_eq!(m.name, "abak");
    assert_eq!(m.address, 0x38487C);
}

#[test]
fn the_dimension_word_is_split_into_rows_and_columns() {
    // 513 = 0x0201 -> 2 rows, 1 column
    let doc = parse_damos(SAMPLE).expect("parse");
    assert_eq!(doc.maps[0].rows, 2);
    assert_eq!(doc.maps[0].cols, 1);
    // 529 = 0x0211 -> 2 rows, 17 columns
    assert_eq!(doc.maps[1].rows, 2);
    assert_eq!(doc.maps[1].cols, 17);
}

#[test]
fn the_conversion_name_is_captured() {
    let doc = parse_damos(SAMPLE).expect("parse");
    assert_eq!(doc.maps[0].conversion, "fak_ub_b1");
}

#[test]
fn the_description_is_captured() {
    let doc = parse_damos(SAMPLE).expect("parse");
    assert!(doc.maps[0].description.contains("Wandfilm"));
}

#[test]
fn the_mask_is_decoded_from_hex() {
    let doc = parse_damos(SAMPLE).expect("parse");
    assert_eq!(doc.maps[0].mask, 0xFF);
}

// ---------------------------------------------------------------------------
// Rejection
// ---------------------------------------------------------------------------

#[test]
fn an_empty_input_is_rejected() {
    assert!(matches!(parse_damos("").unwrap_err(), DamosError::Empty));
}

#[test]
fn a_document_with_no_maps_is_reported() {
    let no_maps = "/EPR, {X}, {Y}, DAMPAR_03\n";
    assert!(matches!(
        parse_damos(no_maps).unwrap_err(),
        DamosError::NoMaps
    ));
}

#[test]
fn a_malformed_map_line_is_skipped_not_fatal() {
    let broken = SAMPLE.replace("$38487C", "$ZZZ");
    let doc = parse_damos(&broken).expect("parse");
    assert_eq!(doc.maps.len(), 1, "the good map survives");
}

#[test]
fn an_out_of_range_dimension_word_is_skipped() {
    // A word whose low byte is 0 would mean zero columns.
    let zero_cols = SAMPLE.replace(", 513, 71,", ", 512, 71,");
    let doc = parse_damos(&zero_cols).expect("parse");
    assert_eq!(doc.maps.len(), 1);
}

// ---------------------------------------------------------------------------
// Robustness
// ---------------------------------------------------------------------------

#[test]
fn a_map_with_a_single_cell_is_allowed() {
    // 257 = 0x0101 -> 1 row, 1 column.
    let single = "/UMP, {}, x, {d}, $1000, 257, 1, dez, 3, $FF, K;\n";
    let doc = parse_damos(single).expect("parse");
    assert_eq!(doc.maps[0].rows, 1);
    assert_eq!(doc.maps[0].cols, 1);
}

#[test]
fn the_characteristic_entries_are_counted_separately() {
    // /SPZ is a scalar characteristic, not a map.
    let doc = parse_damos(SAMPLE).expect("parse");
    assert_eq!(doc.characteristics.len(), 1);
    assert_eq!(doc.characteristics[0].name, "A0");
}

#[test]
fn a_characteristic_carries_its_address() {
    let doc = parse_damos(SAMPLE).expect("parse");
    assert_eq!(doc.characteristics[0].address, 0x81609C);
}

#[test]
fn parses_spz_flash_maps_curves_and_resolves_type7_and_type8_group_axes() {
    let sample = r#"
1, /SPZ, A0, {Scalar}, 1, $81588E, $81588E
/SPW, 48, -2, -0.0625, 0.06249809
/FKX, 0, 0, 0
/FKY, 0, 0, 0
/ABL, 0;
3032, /SPZ, SST_RPM, {RPM axis}, 6, $81005A, $81005A
/SPW, 0, 0, 0, 0
/FKX, 16, 0, 0
/FKY, 0, 0, 0
/ABL, 2;
3079, /SPZ, SST_LOAD, {Load axis}, 6, $810100, $810100
/SPW, 0, 0, 0, 0
/FKX, 12, 0, 0
/FKY, 0, 0, 0
/ABL, 2;
101, /SPZ, BRABEVI, {1D curve}, 2, $81B6B0, $81B6B0
/SPW, 7, 2, 0, 7110.5
/FKX, 8, 0, 0
/FKY, 0, 0, 0
/ABL, 2;
1775, /SPZ, FKKVS, {2D map with own axes}, 3, $81C05C, $81C05C
/SPW, 77, 2, 0, 1.999969
/FKX, 16, 0, 0
/FKY, 16, 0, 0
/ABL, 2;
30, /SPZ, AINTKAN, {Group curve}, 7, $818426, $818426
/SPW, 15, 1, 0, 255
/SPX, 165, 1, 0, 10200, 3032
/SPY, 0, 0, 0, 0, 0
/FKX, 0, 0, 0
/FKY, 0, 0, 0
/ABL, 2;
2072, /SPZ, KFAMAL, {Group 2D map}, 8, $810450, $810450
/SPW, 58, 1, 0, 0.9961
/SPX, 256, 1, 0, 191.3, 3079
/SPY, 165, 1, 0, 10200, 3032
/FKX, 0, 0, 0
/FKY, 0, 0, 0
/ABL, 2;
"#;
    let doc = parse_damos(sample).expect("parse spz sample");
    assert_eq!(doc.spz_objects.len(), 7);

    let fkkvs = doc.spz_objects.iter().find(|o| o.name == "FKKVS").unwrap();
    assert!(fkkvs.is_2d_map());
    assert_eq!((fkkvs.rows, fkkvs.cols, fkkvs.elem_bytes), (16, 16, 2));

    let kfamal = doc.spz_objects.iter().find(|o| o.name == "KFAMAL").unwrap();
    assert!(kfamal.is_2d_map());
    assert_eq!((kfamal.rows, kfamal.cols, kfamal.elem_bytes), (16, 12, 1));

    let brabevi = doc.spz_objects.iter().find(|o| o.name == "BRABEVI").unwrap();
    assert!(brabevi.is_1d_curve());
    assert_eq!((brabevi.rows, brabevi.cols, brabevi.elem_bytes), (0, 8, 2));

    let aintkan = doc.spz_objects.iter().find(|o| o.name == "AINTKAN").unwrap();
    assert!(aintkan.is_1d_curve());
    assert_eq!(aintkan.cols, 16);
}

#[test]
fn parses_real_audi_s4_damos_flash_maps_and_curves() {
    let path = "/root/ecu_master_database/02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-0002.dam";
    let Ok(bytes) = std::fs::read(path) else {
        return;
    };
    let txt = String::from_utf8_lossy(&bytes);
    let doc = parse_damos(&txt).expect("parse real S4 damos");
    let flash_2d: Vec<_> = doc
        .spz_objects
        .iter()
        .filter(|o| (0x800000..=0x8FFFFF).contains(&o.address) && o.is_2d_map())
        .collect();
    let flash_1d: Vec<_> = doc
        .spz_objects
        .iter()
        .filter(|o| (0x800000..=0x8FFFFF).contains(&o.address) && o.is_1d_curve())
        .collect();
    assert!(
        flash_2d.len() >= 240,
        "expected >=240 flash 2D maps, got {}",
        flash_2d.len()
    );
    assert!(
        flash_1d.len() >= 800,
        "expected >=800 flash 1D curves, got {}",
        flash_1d.len()
    );
    let unresolved = doc
        .spz_objects
        .iter()
        .filter(|o| {
            (o.spz_kind == 7 && o.cols == 0) || (o.spz_kind == 8 && (o.rows == 0 || o.cols == 0))
        })
        .count();
    assert_eq!(unresolved, 0, "expected 0 unresolved Type 7/8 group axes");
}
