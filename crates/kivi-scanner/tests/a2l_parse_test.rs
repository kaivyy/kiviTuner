//! ASAP2 / ASAM MCD-2 MC (`.a2l`) parsing.
//!
//! ## Format
//!
//! A2L is a plain-text block language. The blocks that matter for calibration:
//!
//! ```text
//! /begin CHARACTERISTIC
//!     <name>
//!     "<description>"
//!     <type>              VALUE | CURVE | MAP | ASCII | VAL_BLK
//!     <address>           hex
//!     <record_layout>
//!     <max_diff>
//!     <conversion>        a COMPU_METHOD name
//!     <lower_limit>
//!     <upper_limit>
//! /end CHARACTERISTIC
//! ```
//!
//! and `COMPU_METHOD` with `RAT_FUNC` + `COEFFS a b c d e f` meaning
//! `y = (a·x² + b·x + c) / (d·x² + e·x + f)`.
//!
//! ## Why this matters
//!
//! A2L is the one definition format with a **published, open specification**
//! (ASAM MCD-2 MC). Supporting it makes kiviTuner usable with OEM-supplied
//! definitions, which XDF alone cannot reach.
//!
//! ## What is verified
//!
//! The fixtures below are shaped exactly like the real files in
//! `/root/ecu_master_database` (6 811 CHARACTERISTIC blocks in one 8.9 MB A2L).

use kivi_scanner::a2l_parse::{parse_a2l, A2lCharacteristicType, A2lError};

const SAMPLE: &str = r#"
/begin COMPU_METHOD
    ident
    ""
    RAT_FUNC
    "%6.2"
    ""
    COEFFS 0 1 0 0 0 1
/end COMPU_METHOD

/begin COMPU_METHOD
    quarter
    "quarter scale"
    RAT_FUNC
    "%6.3"
    "rpm"
    COEFFS 0 1 0 0 0 4
/end COMPU_METHOD

/begin CHARACTERISTIC
    A0
    "Transfer function coefficient"
    VALUE
    0xFC8F1E
    KwSw
    0.1249981
    ident
    -0.06250000
    0.06249809
    FORMAT "%10.8"
/end CHARACTERISTIC

/begin CHARACTERISTIC
    KFZW
    "Ignition angle"
    MAP
    0x1B470C
    KwSw
    0.0
    quarter
    -20.0
    60.0
    FORMAT "%8.2"
/end CHARACTERISTIC
"#;

#[test]
fn two_characteristics_are_parsed() {
    let doc = parse_a2l(SAMPLE).expect("parse");
    assert_eq!(doc.characteristics.len(), 2);
}

#[test]
fn the_name_description_and_type_are_captured() {
    let doc = parse_a2l(SAMPLE).expect("parse");
    let c = &doc.characteristics[0];
    assert_eq!(c.name, "A0");
    assert_eq!(c.description, "Transfer function coefficient");
    assert_eq!(c.kind, A2lCharacteristicType::Value);
}

#[test]
fn the_address_is_decoded_from_hex() {
    let doc = parse_a2l(SAMPLE).expect("parse");
    assert_eq!(doc.characteristics[0].address, 0xFC8F1E);
    assert_eq!(doc.characteristics[1].address, 0x1B470C);
}

#[test]
fn a_map_type_is_recognised() {
    let doc = parse_a2l(SAMPLE).expect("parse");
    assert_eq!(doc.characteristics[1].kind, A2lCharacteristicType::Map);
}

#[test]
fn the_conversion_name_is_captured() {
    let doc = parse_a2l(SAMPLE).expect("parse");
    assert_eq!(doc.characteristics[1].conversion, "quarter");
}

#[test]
fn the_limits_are_captured() {
    let doc = parse_a2l(SAMPLE).expect("parse");
    assert_eq!(doc.characteristics[1].lower_limit, -20.0);
    assert_eq!(doc.characteristics[1].upper_limit, 60.0);
}

// ---------------------------------------------------------------------------
// COMPU_METHOD
// ---------------------------------------------------------------------------

#[test]
fn two_conversions_are_parsed() {
    let doc = parse_a2l(SAMPLE).expect("parse");
    assert_eq!(doc.conversions.len(), 2);
}

#[test]
fn an_identity_conversion_evaluates_to_the_input() {
    let doc = parse_a2l(SAMPLE).expect("parse");
    let c = doc.conversions.iter().find(|c| c.name == "ident").unwrap();
    assert!((c.apply(42.0) - 42.0).abs() < 1e-9);
}

#[test]
fn a_quarter_conversion_divides_by_four() {
    // COEFFS 0 1 0 0 0 4  ->  y = (0x² + 1x + 0) / (0x² + 0x + 4) = x / 4
    let doc = parse_a2l(SAMPLE).expect("parse");
    let c = doc.conversions.iter().find(|c| c.name == "quarter").unwrap();
    assert!((c.apply(100.0) - 25.0).abs() < 1e-9);
}

#[test]
fn the_units_are_captured() {
    let doc = parse_a2l(SAMPLE).expect("parse");
    let c = doc.conversions.iter().find(|c| c.name == "quarter").unwrap();
    assert_eq!(c.unit, "rpm");
}

// ---------------------------------------------------------------------------
// Rejection
// ---------------------------------------------------------------------------

#[test]
fn an_empty_document_yields_nothing() {
    let doc = parse_a2l("").expect("parse");
    assert!(doc.characteristics.is_empty());
    assert!(doc.conversions.is_empty());
}

#[test]
fn an_unterminated_characteristic_is_rejected() {
    let broken = SAMPLE.replace("/end CHARACTERISTIC", "");
    assert!(matches!(
        parse_a2l(&broken).unwrap_err(),
        A2lError::UnterminatedBlock(_)
    ));
}

#[test]
fn a_characteristic_with_too_few_fields_is_skipped_not_fatal() {
    // Real A2L files contain occasional malformed entries; one bad block must
    // not discard the other 6 810.
    let broken = SAMPLE.replace("    VALUE\n    0xFC8F1E\n", "    VALUE\n");
    let doc = parse_a2l(&broken).expect("parse");
    assert_eq!(doc.characteristics.len(), 1, "the good block survives");
}

#[test]
fn a_bad_address_is_skipped_not_fatal() {
    let broken = SAMPLE.replace("0xFC8F1E", "NOT_HEX");
    let doc = parse_a2l(&broken).expect("parse");
    assert_eq!(doc.characteristics.len(), 1);
}

#[test]
fn an_unknown_characteristic_type_falls_back_to_unknown() {
    let odd = SAMPLE.replace("    VALUE\n", "    WEIRD\n");
    let doc = parse_a2l(&odd).expect("parse");
    assert_eq!(doc.characteristics[0].kind, A2lCharacteristicType::Unknown);
}

// ---------------------------------------------------------------------------
// Comment handling
// ---------------------------------------------------------------------------

#[test]
fn c_style_comments_are_ignored() {
    let with_comment = SAMPLE.replace(
        "/begin CHARACTERISTIC\n    A0",
        "/* a comment\n   spanning lines */\n/begin CHARACTERISTIC\n    A0",
    );
    let doc = parse_a2l(&with_comment).expect("parse");
    assert_eq!(doc.characteristics.len(), 2);
}

#[test]
fn axis_pts_memory_segment_record_layout_and_dimensions_parse_cleanly() {
    let extended = r#"
    /begin MOD_COMMON ""
        BYTE_ORDER MSB_LAST
    /end MOD_COMMON
    /begin MEMORY_SEGMENT
        Pst80080000 "" CODE FLASH INTERN 0x80080000 0x77F800 -1 -1 -1 -1 -1
    /end MEMORY_SEGMENT
    /begin RECORD_LAYOUT KfMap
        NO_AXIS_PTS_X 1 UBYTE
        FNC_VALUES 5 UWORD COLUMN_DIR DIRECT
        AXIS_PTS_X 3 SWORD INDEX_INCR DIRECT
    /end RECORD_LAYOUT
    /begin AXIS_PTS
        N_AXIS "RPM" 0x80100000 rpm RefAxis 10.0 ident 16 0.0 8000.0
    /end AXIS_PTS
    /begin CHARACTERISTIC
        MAIN_MAP "Main Fuel" MAP 0x80100100 KfMap 0.0 ident 0.0 100.0
        /begin AXIS_DESCR COM_AXIS rpm ident 16 0.0 8000.0 AXIS_PTS_REF N_AXIS /end AXIS_DESCR
        /begin AXIS_DESCR STD_AXIS load ident 12 0.0 200.0 /end AXIS_DESCR
    /end CHARACTERISTIC
    "#;
    let doc = parse_a2l(extended).expect("parse extended");
    assert_eq!(doc.axis_pts.len(), 1);
    assert_eq!(doc.axis_pts[0].name, "N_AXIS");
    assert_eq!(doc.axis_pts[0].address, 0x8010_0000);
    assert_eq!(doc.memory_segments.len(), 1);
    assert_eq!(doc.memory_segments[0].address, 0x8008_0000);
    assert_eq!(doc.record_layouts.len(), 1);
    assert_eq!(doc.record_layouts[0].fnc_element_bytes(), Some(2));
    assert!(doc.record_layouts[0].is_column_dir());
    assert_eq!(doc.characteristics[0].dimensions(), Some((16, 12)));
    assert_eq!(
        doc.characteristics[0].axes[0].axis_pts_ref.as_deref(),
        Some("N_AXIS")
    );

    let tr = doc
        .build_translator_for_rom(8_388_608)
        .expect("8 MiB translator");
    let mapped = kivi_core::address_translation::cpu_to_raw(&tr, 0x8010_0100).expect("translate");
    assert!(!mapped.is_identity_fallback);
    assert_eq!(mapped.value, 0x10_0100);
}

#[test]
fn real_corpus_a2ls_parse_segments_axes_and_record_layouts() {
    use kivi_core::address_translation::cpu_to_raw;

    let cases = [
        (
            "/root/ecu_master_database/02_CARS/VAG_AUDI_VOLKSWAGEN/GOLF_VR6_ABA_BINS/022906032CE-6733.A2L",
            1_048_576usize,
            Some(0x8EBBE0u32),
            Some(0x0EBBE0u32),
        ),
        (
            "/root/ecu_master_database/02_CARS/BMW/BOSCH_ME9_VALVETRONIC_N42_N46_N62/770115XE.a2l",
            1_048_576usize,
            Some(0x4EAA0Au32),
            Some(0x0EAA0Au32),
        ),
        (
            "/root/ecu_master_database/02_CARS/BMW/B48_B58_GEN1_GEN2_A2L_XDF/R4C9L8N5B_5d55-080_060_005_v7.2.3.a2l",
            8_388_608usize,
            Some(0x8078D032u32),
            Some(0x0078D032u32),
        ),
    ];

    for (path, rom_len, sample_cpu, expected_off) in cases {
        let bytes = std::fs::read(path).expect(path);
        let text = String::from_utf8_lossy(&bytes);
        let doc = parse_a2l(&text).expect("real A2L parse");
        assert!(!doc.memory_segments.is_empty(), "{path}: expected segments");
        assert!(!doc.record_layouts.is_empty(), "{path}: expected record layouts");
        let tr = doc
            .build_translator_for_rom(rom_len)
            .unwrap_or_else(|| panic!("{path}: expected translator for {rom_len}"));
        if let (Some(cpu), Some(off)) = (sample_cpu, expected_off) {
            let t = cpu_to_raw(&tr, cpu).expect("translate");
            assert!(!t.is_identity_fallback, "{path}: fallback on 0x{cpu:X}");
            assert_eq!(t.value, off, "{path}: offset mismatch");
        }
    }

    // Siemens MS43 has 0 MEMORY_SEGMENTs, 596 AXIS_PTS, 3731 CHARACTERISTICs.
    let ms43_bytes = std::fs::read("/root/ecu_master_database/02_CARS/BMW/SIEMENS_MS43/4323T110.A2L").unwrap();
    let ms43_doc = parse_a2l(&String::from_utf8_lossy(&ms43_bytes)).expect("parse MS43 A2L");
    assert!(ms43_doc.axis_pts.len() >= 500);
    assert!(ms43_doc.characteristics.len() >= 3000);
    assert!(ms43_doc.memory_segments.is_empty());
    assert!(ms43_doc.build_translator_for_rom(524_288).is_none());
}
