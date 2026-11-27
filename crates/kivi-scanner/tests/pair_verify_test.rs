//! Content-based pair verification: does a definition describe this ROM?
//!
//! ## Why this exists
//!
//! Identity matching cannot be the only criterion. Measured across 3 630
//! binaries, a file's own name appears in its ROM for only 36% of BMW, 15% of
//! Honda and **0%** of Nissan, Honda (bike) and Yamaha.
//!
//! So the check here ignores names entirely: if a definition really describes a
//! ROM, the tables it declares must land on calibration-shaped data.

use kivi_scanner::pair_verify::verify_by_content;

/// A ROM with one smooth 8x8 surface at `at`.
fn rom_with_surface(at: usize) -> Vec<u8> {
    let mut rom: Vec<u8> = (0..8192).map(|i| ((i * 7) % 251) as u8).collect();
    for r in 0..8usize {
        for c in 0..8usize {
            let v = (1000 + r * 100 + c * 10) as u16;
            let idx = at + (r * 8 + c) * 2;
            rom[idx..idx + 2].copy_from_slice(&v.to_be_bytes());
        }
    }
    rom
}

/// A ROM whose bytes at `at` are constant padding.
fn rom_with_padding(at: usize) -> Vec<u8> {
    let mut rom: Vec<u8> = (0..8192).map(|i| ((i * 7) % 251) as u8).collect();
    for b in rom[at..at + 128].iter_mut() {
        *b = 0xFF;
    }
    rom
}

fn one_table(addr: usize) -> [(usize, usize, usize, usize, bool, i32); 1] {
    [(addr, 8, 8, 16, false, 0)]
}

#[test]
fn a_real_surface_is_recognised() {
    let rom = rom_with_surface(0x200);
    let v = verify_by_content(&rom, &one_table(0x200));
    assert_eq!(v.tables_in_rom, 1);
    assert_eq!(v.tables_varying, 1);
    assert_eq!(v.tables_smooth, 1);
    assert!((v.smooth_fraction - 1.0).abs() < 1e-9);
}

#[test]
fn padding_is_not_recognised_as_a_surface() {
    let rom = rom_with_padding(0x200);
    let v = verify_by_content(&rom, &one_table(0x200));
    assert_eq!(v.tables_in_rom, 1);
    assert_eq!(v.tables_varying, 0, "constant padding has no variation");
    assert_eq!(v.tables_smooth, 0);
}

#[test]
fn an_address_past_the_end_is_skipped_not_counted() {
    let rom = rom_with_surface(0x200);
    let v = verify_by_content(&rom, &one_table(rom.len() + 0x100));
    assert_eq!(v.tables_in_rom, 0, "out-of-range tables must not be counted");
}

#[test]
fn a_definition_with_too_few_tables_is_inconclusive() {
    let rom = rom_with_surface(0x200);
    let v = verify_by_content(&rom, &one_table(0x200));
    assert!(
        !v.conclusive,
        "one table is not enough evidence to accept or reject a pair"
    );
    assert!(!v.is_consistent(), "inconclusive must not be accepted");
}

#[test]
fn eight_smooth_tables_make_a_conclusive_acceptance() {
    let mut rom: Vec<u8> = (0..16384).map(|i| ((i * 7) % 251) as u8).collect();
    let mut tables = Vec::new();
    for t in 0..8usize {
        let at = 0x200 + t * 0x200;
        for r in 0..8usize {
            for c in 0..8usize {
                let v = (1000 + r * 100 + c * 10) as u16;
                let idx = at + (r * 8 + c) * 2;
                rom[idx..idx + 2].copy_from_slice(&v.to_be_bytes());
            }
        }
        tables.push((at, 8, 8, 16, false, 0));
    }
    let v = verify_by_content(&rom, &tables);
    assert!(v.conclusive);
    assert!(v.is_consistent(), "eight real surfaces must be accepted: {v:?}");
}

#[test]
fn eight_padding_blocks_make_a_conclusive_rejection() {
    let mut rom: Vec<u8> = (0..16384).map(|i| ((i * 7) % 251) as u8).collect();
    let mut tables = Vec::new();
    for t in 0..8usize {
        let at = 0x200 + t * 0x200;
        for b in rom[at..at + 128].iter_mut() {
            *b = 0xFF;
        }
        tables.push((at, 8, 8, 16, false, 0));
    }
    let v = verify_by_content(&rom, &tables);
    assert!(v.conclusive);
    assert!(!v.is_consistent(), "padding must be rejected: {v:?}");
}

#[test]
fn the_real_cbr_pair_is_accepted_by_content() {
    // This pair is a name mismatch (ROM says K64J108, XDF says K64-ND1), yet the
    // XDF's tables really do sit on smooth data in this ROM. Content verification
    // must therefore accept it even though identity matching rejected it — which
    // is exactly why both checks are needed.
    let rom = std::fs::read(
        "/root/ecu_master_database/01_MOTORCYCLES/HONDA/SPORT_CBR250RR/CBR250RR-38770-K64-ND1_BIN.bin",
    )
    .expect("fixture ROM");
    let xdf = std::fs::read(
        "/root/ecu_master_database/01_MOTORCYCLES/HONDA/SPORT_CBR250RR/CBR250RR_38770-K64-ND1_PUBLIC_v1.xdf",
    )
    .expect("fixture XDF");
    let parsed = kivi_scanner::xdf_parse::parse_xdf_xml(&String::from_utf8_lossy(&xdf))
        .expect("parse");

    let tables: Vec<(usize, usize, usize, usize, bool, i32)> = parsed
        .tables
        .iter()
        .filter_map(|t| {
            let r = t.y_axis.as_ref().map(|a| a.index_count).unwrap_or(1);
            let c = t.x_axis.as_ref().map(|a| a.index_count).unwrap_or(1);
            if r < 2 && c < 2 {
                return None;
            }
            Some((
                t.z_axis.address,
                r,
                c,
                t.z_axis.element_size_bits,
                t.z_axis.type_flags & 0x02 != 0,
                t.z_axis.major_stride_bits,
            ))
        })
        .collect();

    let v = verify_by_content(&rom, &tables);
    eprintln!(
        "CBR by content: {} in ROM, {} varying, {} smooth ({:.1}%)",
        v.tables_in_rom,
        v.tables_varying,
        v.tables_smooth,
        100.0 * v.smooth_fraction
    );
    assert!(v.conclusive);
    assert!(
        v.is_consistent(),
        "the CBR definition does describe this ROM's data: {v:?}"
    );
}
