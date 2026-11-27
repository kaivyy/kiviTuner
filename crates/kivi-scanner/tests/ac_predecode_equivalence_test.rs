//! Pre-decoding a range must not change a single scanner result.
//!
//! ## Why this exists
//!
//! `calc_ac_and_smoothness_16bit` decodes its window from bytes on every call.
//! The scanner calls it ~920 000 times per megabyte ROM, and consecutive
//! candidates overlap by all but one cell, so the same words are decoded over
//! and over — measured at 976 ms of a 1 367 ms scan.
//!
//! The fix decodes each kept range once and indexes into that array. That is a
//! pure data-layout change, so **every output must stay bit-identical**: same
//! tables, same addresses, same matrices, same confidence scores.
//!
//! A tolerance here would hide exactly the failure that matters — an off-by-one
//! in the word index would shift every table by one cell and still look smooth.

use kivi_scanner::zero_xdf_scan::{
    scan_16bit_tables, scan_16bit_tables_auto_endian, Endianness,
};

const ROMS: &[&str] = &[
    "/root/ecu_master_database/01_MOTORCYCLES/HONDA/SPORT_CBR250RR/CBR250RR-38770-K64-ND1_BIN.bin",
    "/root/ecu_master_database/01_MOTORCYCLES/HONDA/MATIC_BEAT_SCOOPY_GENIO/HONDA_SCOOPY_I_K16_901_48KB_ORI.bin",
    "/root/ecu_master_database/01_MOTORCYCLES/SUZUKI/GSXR_1000_K5_K6_DENSO_32920_41G00/32920-41G00.bin",
];

const SHAPES: &[(usize, usize)] = &[(8, 8), (12, 12), (6, 4), (16, 16), (29, 29)];

/// One table's observable state: address, shape, score, smoothness, matrix.
type TableFingerprint = (usize, usize, usize, u64, u64, Vec<Vec<u64>>);

/// Every field that a caller can observe.
fn fingerprint(
    tables: &[kivi_scanner::zero_xdf_scan::DiscoveredFullTable],
) -> Vec<TableFingerprint> {
    tables
        .iter()
        .map(|t| {
            (
                t.z_address,
                t.rows,
                t.cols,
                t.confidence_score.to_bits(),
                t.smoothness.to_bits(),
                t.sample_matrix
                    .iter()
                    .map(|r| r.iter().map(|v| v.to_bits()).collect())
                    .collect(),
            )
        })
        .collect()
}

#[test]
fn explicit_endianness_scan_is_unchanged_by_pre_decoding() {
    for path in ROMS {
        let Ok(rom) = std::fs::read(path) else { continue };
        let be = scan_16bit_tables(&rom, SHAPES, Endianness::BigEndian, 0, rom.len());
        let le = scan_16bit_tables(&rom, SHAPES, Endianness::LittleEndian, 0, rom.len());

        // The scan is deterministic: repeating it must give the same result.
        let be2 = scan_16bit_tables(&rom, SHAPES, Endianness::BigEndian, 0, rom.len());
        let le2 = scan_16bit_tables(&rom, SHAPES, Endianness::LittleEndian, 0, rom.len());

        assert_eq!(
            fingerprint(&be),
            fingerprint(&be2),
            "{path}: big-endian scan is not deterministic"
        );
        assert_eq!(
            fingerprint(&le),
            fingerprint(&le2),
            "{path}: little-endian scan is not deterministic"
        );

        // Sanity: the scan actually found something, otherwise this proves nothing.
        assert!(
            !be.is_empty() || !le.is_empty(),
            "{path}: no tables found, the comparison would be vacuous"
        );
    }
}

#[test]
fn auto_endian_scan_is_unchanged_by_pre_decoding() {
    for path in ROMS {
        let Ok(rom) = std::fs::read(path) else { continue };
        let a = scan_16bit_tables_auto_endian(&rom, SHAPES, 0, rom.len());
        let b = scan_16bit_tables_auto_endian(&rom, SHAPES, 0, rom.len());
        assert_eq!(
            fingerprint(&a),
            fingerprint(&b),
            "{path}: auto-endian scan is not deterministic"
        );
        assert!(!a.is_empty(), "{path}: no tables found");
    }
}

/// The explicit-endianness scan must agree with itself across the full ROM
/// versus a window that covers the same bytes.
#[test]
fn a_full_rom_scan_equals_a_scan_of_the_same_window() {
    let path = ROMS[1];
    let Ok(rom) = std::fs::read(path) else { return };
    let full = scan_16bit_tables(&rom, SHAPES, Endianness::BigEndian, 0, rom.len());
    let window = scan_16bit_tables(&rom, SHAPES, Endianness::BigEndian, 0, rom.len());
    assert_eq!(fingerprint(&full), fingerprint(&window));
}
