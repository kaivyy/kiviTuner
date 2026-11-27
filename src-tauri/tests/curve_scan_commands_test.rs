//! Application-facing tests for the 1-D curve scanner exposed through the Tauri
//! command layer.
//!
//! These prove the detector is reachable from a real kiviTuner execution path,
//! that its guards reject nonsense without panicking, and that it is a pure
//! calculation over the supplied buffer.

use kivituner::commands::scan_rom_curves_inner;

/// Build a ROM with a 16-bit big-endian bell written at `at`.
fn rom_with_bell(at: usize, values: &[u16]) -> Vec<u8> {
    let mut rom = vec![0u8; 2048];
    for (i, v) in values.iter().enumerate() {
        let b = v.to_be_bytes();
        rom[at + i * 2] = b[0];
        rom[at + i * 2 + 1] = b[1];
    }
    rom
}

const BELL: [u16; 16] = [
    100, 140, 190, 250, 320, 400, 480, 560, 560, 480, 400, 320, 250, 190, 140, 100,
];

#[test]
fn command_finds_a_curve_at_the_address_it_was_written_to() {
    let rom = rom_with_bell(0x200, &BELL);
    let found = scan_rom_curves_inner(&rom, 8, 32, false).expect("scan failed");

    let hit = found
        .iter()
        .find(|c| c.address == 0x200)
        .unwrap_or_else(|| {
            let addrs: Vec<usize> = found.iter().map(|c| c.address).collect();
            panic!("no curve at 0x200; reported {addrs:?}")
        });
    assert_eq!(hit.points, 16);
    assert_eq!(hit.min_value, 100.0);
    assert_eq!(hit.max_value, 560.0);
}

#[test]
fn an_empty_rom_is_refused() {
    let err = scan_rom_curves_inner(&[], 8, 32, false).unwrap_err();
    assert!(err.contains("No ROM data"), "unexpected message: {err}");
}

#[test]
fn min_points_below_three_is_refused() {
    let rom = rom_with_bell(0x100, &BELL);
    let err = scan_rom_curves_inner(&rom, 2, 32, false).unwrap_err();
    assert!(err.contains("at least 3"), "unexpected message: {err}");
}

#[test]
fn an_inverted_point_range_is_refused() {
    let rom = rom_with_bell(0x100, &BELL);
    let err = scan_rom_curves_inner(&rom, 32, 8, false).unwrap_err();
    assert!(err.contains("Invalid point range"), "unexpected message: {err}");
}

#[test]
fn an_unbounded_max_points_is_refused() {
    let rom = rom_with_bell(0x100, &BELL);
    let err = scan_rom_curves_inner(&rom, 8, 100_000, false).unwrap_err();
    assert!(err.contains("4096"), "unexpected message: {err}");
}

#[test]
fn scanning_does_not_modify_the_input_buffer() {
    let rom = rom_with_bell(0x200, &BELL);
    let before = rom.clone();
    let _ = scan_rom_curves_inner(&rom, 8, 32, true).expect("scan failed");
    assert_eq!(rom, before, "the scanner must not mutate the ROM");
}

#[test]
fn results_are_ordered_by_address() {
    let mut rom = rom_with_bell(0x400, &BELL);
    let second = rom_with_bell(0x100, &BELL);
    rom[0x100..0x120].copy_from_slice(&second[0x100..0x120]);

    let found = scan_rom_curves_inner(&rom, 8, 32, false).expect("scan failed");
    let addresses: Vec<usize> = found.iter().map(|c| c.address).collect();
    let mut sorted = addresses.clone();
    sorted.sort_unstable();
    assert_eq!(addresses, sorted, "curves must come back in address order");
}
