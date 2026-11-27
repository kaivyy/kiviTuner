//! Application-facing tests for the Denso interleaved scanner command.
//!
//! Proves the detector is reachable from a real kiviTuner path, that its guards
//! reject nonsense without panicking, and that a real Denso ROM no longer
//! produces a flood of overlapping false positives.

use kivituner::commands::scan_rom_denso_interleaved_inner;

/// Builds a ROM with one interleaved map at `at`.
fn interleaved_rom(at: usize, y_axis: &[u16], rows: &[Vec<u16>]) -> Vec<u8> {
    let cols = rows[0].len();
    let mut rom = vec![0u8; 8192];
    for (r, y) in y_axis.iter().enumerate() {
        let base = at + r * (cols + 1) * 2;
        rom[base..base + 2].copy_from_slice(&y.to_le_bytes());
        for (c, v) in rows[r].iter().enumerate() {
            let off = base + 2 + c * 2;
            rom[off..off + 2].copy_from_slice(&v.to_le_bytes());
        }
    }
    rom
}

#[test]
fn the_command_finds_an_interleaved_map() {
    let y = vec![1000u16, 2000, 3000, 4000];
    let rows: Vec<Vec<u16>> = (0..4)
        .map(|r| (0..4).map(|c| (100 + r * 10 + c) as u16).collect())
        .collect();
    let rom = interleaved_rom(0x200, &y, &rows);

    let found = scan_rom_denso_interleaved_inner(&rom, &[4], 4, 4).expect("scan");
    assert!(
        found.iter().any(|m| m.start_address == 0x200),
        "expected a map at 0x200"
    );
}

#[test]
fn an_empty_rom_is_refused() {
    let err = scan_rom_denso_interleaved_inner(&[], &[4], 4, 8).unwrap_err();
    assert!(err.contains("No ROM data"), "unexpected: {err}");
}

#[test]
fn an_empty_width_list_is_refused() {
    let rom = vec![0u8; 1024];
    let err = scan_rom_denso_interleaved_inner(&rom, &[], 4, 8).unwrap_err();
    assert!(err.contains("candidate width"), "unexpected: {err}");
}

#[test]
fn an_inverted_row_range_is_refused() {
    let rom = vec![0u8; 1024];
    let err = scan_rom_denso_interleaved_inner(&rom, &[4], 8, 4).unwrap_err();
    assert!(err.contains("Invalid row range"), "unexpected: {err}");
}

#[test]
fn an_unbounded_combination_is_refused() {
    let rom = vec![0u8; 1024];
    let cols: Vec<usize> = (1..=40).collect();
    let err = scan_rom_denso_interleaved_inner(&rom, &cols, 4, 200).unwrap_err();
    assert!(err.contains("too large"), "unexpected: {err}");
}

#[test]
fn scanning_does_not_modify_the_input() {
    let y = vec![1000u16, 2000, 3000, 4000];
    let rows: Vec<Vec<u16>> = (0..4)
        .map(|r| (0..4).map(|c| (100 + r * 10 + c) as u16).collect())
        .collect();
    let rom = interleaved_rom(0x200, &y, &rows);
    let before = rom.clone();
    let _ = scan_rom_denso_interleaved_inner(&rom, &[4], 4, 4).expect("scan");
    assert_eq!(rom, before, "the scanner must not mutate the ROM");
}

#[test]
fn a_real_denso_rom_yields_far_fewer_detections_than_the_raw_scan() {
    let path = "/root/ecu_master_database/01_MOTORCYCLES/SUZUKI/\
GSXR_1000_K5_K6_DENSO_32920_41G00/32920-41G00.bin";
    let rom = std::fs::read(path).unwrap_or_else(|e| panic!("fixture missing: {e}"));

    let found = scan_rom_denso_interleaved_inner(&rom, &[4, 6, 8, 12, 16], 4, 16)
        .expect("scan");

    // Before the smoothness gate and overlap suppression this returned 1 105
    // detections across only 200 unique addresses. Anything near that number
    // means the false-positive flood is back.
    assert!(
        found.len() < 300,
        "expected fewer than 300 maps after suppression, got {}",
        found.len()
    );

    // Every accepted map must be uniquely addressed and non-overlapping.
    let mut spans: Vec<(usize, usize)> = found
        .iter()
        .map(|m| (m.start_address, m.start_address + m.rows * (m.cols + 1) * 2))
        .collect();
    spans.sort_unstable();
    for w in spans.windows(2) {
        assert!(
            w[1].0 >= w[0].1,
            "accepted maps must not overlap: {:?} and {:?}",
            w[0],
            w[1]
        );
    }
}
