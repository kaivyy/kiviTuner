//! Application-path tests for firmware container ingest.
//!
//! Verifies that HEX/SREC files reach a real kiviTuner code path, that raw BIN
//! behaviour is unchanged, and that malformed text is rejected rather than
//! compared as ASCII.
//!
//! The BinDiffModal bug being fixed: the picker accepted `.hex` but the handler
//! read the file as raw bytes and compared ASCII characters as firmware.

use kivituner::commands::{ingest_firmware_inner, load_bin_data_inner, parse_xdf_content_inner};
use kivituner::AppState;

fn hex_line(rec: &[u8]) -> String {
    let mut v = rec.to_vec();
    let sum: u8 = v.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    v.push((!sum).wrapping_add(1));
    format!(
        ":{}\n",
        v.iter().map(|b| format!("{:02X}", b)).collect::<String>()
    )
}

fn srec_line(rtype: char, rec: &[u8]) -> String {
    let mut v = rec.to_vec();
    let sum: u8 = v.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    v.push(!sum);
    format!(
        "S{}{}\n",
        rtype,
        v.iter().map(|b| format!("{:02X}", b)).collect::<String>()
    )
}

#[test]
fn raw_bin_is_passed_through_unchanged() {
    let bin = vec![0x00, 0xFF, 0x12, 0x34];
    let r = ingest_firmware_inner(&bin).expect("ingest failed");
    assert_eq!(r.format, "RawBinary");
    assert_eq!(r.bytes, bin);
}

#[test]
fn hex_file_is_parsed_not_compared_as_ascii() {
    // This is the exact BinDiffModal bug: a .hex file must yield firmware
    // bytes, not its ASCII characters.
    let text = format!(
        "{}:00000001FF\n",
        hex_line(&[0x04, 0x00, 0x00, 0x00, 0xDE, 0xAD, 0xBE, 0xEF])
    );
    let r = ingest_firmware_inner(text.as_bytes()).expect("ingest failed");

    assert_eq!(r.format, "IntelHex");
    assert_eq!(r.bytes, vec![0xDE, 0xAD, 0xBE, 0xEF]);
    // Must NOT equal the ASCII bytes of the text file.
    assert_ne!(r.bytes, text.as_bytes().to_vec());
}

#[test]
fn srec_file_is_parsed() {
    let text = srec_line('1', &[0x05, 0x00, 0x00, 0xCA, 0xFE]);
    let r = ingest_firmware_inner(text.as_bytes()).expect("ingest failed");
    assert_eq!(r.format, "SRecord");
    assert_eq!(r.bytes, vec![0xCA, 0xFE]);
}

#[test]
fn malformed_hex_returns_clear_error() {
    let err = ingest_firmware_inner(b":ZZZZ\n").expect_err("must fail");
    assert!(!err.is_empty());
    assert!(
        err.contains("Line 1") || err.contains("hex"),
        "error must be actionable: {}",
        err
    );
}

#[test]
fn malformed_srec_returns_clear_error() {
    let err = ingest_firmware_inner(b"S\n").expect_err("must fail");
    assert!(!err.is_empty());
}

#[test]
fn hex_oversize_byte_count_does_not_panic() {
    // Source panics here; target must return an error through the app path.
    let err = ingest_firmware_inner(b":1000000001020304E6\n").expect_err("must fail");
    assert!(err.contains("Line 1"), "error: {}", err);
}

#[test]
fn ingest_does_not_touch_app_state() {
    let state = AppState::default();
    load_bin_data_inner(&[0x11; 16], &state).expect("load failed");
    parse_xdf_content_inner(
        r#"<XDFFORMAT version="1.50"><XDFTABLE uniqueid="0x100"><title>T</title>
        <XDFAXIS id="z"><EMBEDDEDDATA mmedaddress="0x0" mmedelementsizebits="8"
        mmedrowcount="1" mmedcolcount="1" /></XDFAXIS></XDFTABLE></XDFFORMAT>"#,
        &state,
    )
    .expect("xdf failed");

    let rom_before = {
        let g = state.memory.lock().unwrap();
        g.as_ref().unwrap().hashes().clone()
    };
    let maps_before = { state.maps.lock().unwrap().clone() };

    let text = format!(
        "{}:00000001FF\n",
        hex_line(&[0x02, 0x00, 0x00, 0x00, 0xAB, 0xCD])
    );
    let _ = ingest_firmware_inner(text.as_bytes()).expect("ingest failed");

    let rom_after = {
        let g = state.memory.lock().unwrap();
        g.as_ref().unwrap().hashes().clone()
    };
    let maps_after = { state.maps.lock().unwrap().clone() };

    assert_eq!(rom_before, rom_after, "ingest must not mutate the ROM");
    assert_eq!(
        maps_before, maps_after,
        "ingest must not mutate definitions"
    );
}

#[test]
fn parsed_hex_feeds_the_diff_path() {
    // End-to-end: parse a .hex file, then feed its bytes to the existing diff
    // engine. Base ROM differs in one byte.
    let text = format!(
        "{}:00000001FF\n",
        hex_line(&[0x04, 0x00, 0x00, 0x00, 0x10, 0x20, 0x30, 0x40])
    );
    let r = ingest_firmware_inner(text.as_bytes()).expect("ingest failed");

    let base = vec![0x10, 0x20, 0x00, 0x40];
    let diffs = kivi_diff::engine::compare_binaries(&base, &r.bytes, 4);
    assert!(!diffs.is_empty(), "expected at least one diff region");
}
