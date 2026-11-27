//! Application-facing tests for blind auto-mapping.
//!
//! The point of this path is tuning with **no definition file**, so the tests
//! assert the properties that make a result usable: a short list, no overlapping
//! tables, honest scaling proposals, and guards that reject nonsense.

use kivituner::commands::blind_automap_inner;

const CBR: &str = "/root/ecu_master_database/01_MOTORCYCLES/HONDA/SPORT_CBR250RR/CBR250RR-38770-K64-ND1_BIN.bin";
const GROM: &str = "/root/ecu_master_database/01_MOTORCYCLES/HONDA/MINI_MOTO_GROM_MONKEY/38770-K0F-A01.bin";

fn load(p: &str) -> Vec<u8> {
    std::fs::read(p).unwrap_or_else(|e| panic!("fixture {p}: {e}"))
}

#[test]
fn the_command_finds_tables_with_no_definition_file() {
    let rom = load(CBR);
    let tables = blind_automap_inner(&rom, None, false, true).expect("scan");
    assert!(!tables.is_empty());
    assert!(tables.len() <= 400, "list must stay browsable, got {}", tables.len());
    eprintln!("CBR250RR: {} tables", tables.len());
}

#[test]
fn every_table_is_individually_usable() {
    let rom = load(CBR);
    let tables = blind_automap_inner(&rom, None, false, true).expect("scan");
    for t in &tables {
        assert!(t.rows >= 1 && t.cols >= 1);
        assert_eq!(t.matrix.len(), t.rows);
        assert!(t.matrix.iter().all(|r| r.len() == t.cols));
        assert!(!t.label.is_empty());
        assert!(!t.suggested_scaling.rationale.is_empty());
        assert!(
            matches!(t.cell_bits, 8 | 16 | 32),
            "cell width must be a real width, got {}",
            t.cell_bits
        );
    }
}

#[test]
fn no_two_tables_share_a_byte() {
    let rom = load(GROM);
    let tables = blind_automap_inner(&rom, None, false, true).expect("scan");
    let mut spans: Vec<(usize, usize)> = tables
        .iter()
        .map(|t| (t.address, t.address + t.rows * t.cols * (t.cell_bits as usize / 8)))
        .collect();
    spans.sort_unstable();
    for w in spans.windows(2) {
        assert!(w[1].0 >= w[0].1, "overlap between {:?} and {:?}", w[0], w[1]);
    }
}

#[test]
fn an_empty_rom_is_refused() {
    let err = blind_automap_inner(&[], None, false, true).unwrap_err();
    assert!(err.contains("No ROM data"), "unexpected: {err}");
}

#[test]
fn a_tiny_rom_is_refused_with_a_reason() {
    let err = blind_automap_inner(&[0u8; 64], None, false, true).unwrap_err();
    assert!(err.contains("too small"), "unexpected: {err}");
}

#[test]
fn an_out_of_range_threshold_is_refused() {
    let rom = load(GROM);
    assert!(blind_automap_inner(&rom, Some(1.5), false, true).is_err());
    assert!(blind_automap_inner(&rom, Some(-0.1), false, true).is_err());
}

#[test]
fn scanning_does_not_modify_the_rom() {
    let rom = load(GROM);
    let before = rom.clone();
    let _ = blind_automap_inner(&rom, None, false, true).expect("scan");
    assert_eq!(rom, before);
}

#[test]
fn the_same_rom_always_gives_the_same_tables() {
    let rom = load(GROM);
    let a = blind_automap_inner(&rom, None, false, true).expect("scan");
    let b = blind_automap_inner(&rom, None, false, true).expect("scan");
    assert_eq!(a, b);
}
