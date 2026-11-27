//! Application-facing tests for the checksum primitives exposed through the
//! Tauri command layer.
//!
//! These verify that the ported CRC algorithms are reachable from a real
//! kiviTuner execution path and that the command is a pure calculation over
//! explicitly supplied data (no ROM mutation, no hardware access).
//!
//! Expected values match the oracle-derived fixtures in
//! `crates/kivi-checksum/tests/crc_parity_test.rs`.

use kivituner::commands::{compute_checksum_inner, load_bin_data_inner};
use kivituner::AppState;

#[test]
fn compute_checksum_castagnoli_known_vector() {
    let r = compute_checksum_inner("crc32_castagnoli", b"123456789").expect("checksum failed");
    assert_eq!(r.algorithm, "crc32_castagnoli");
    assert_eq!(r.width_bits, 32);
    assert_eq!(r.value, 0xE306_9283);
    assert_eq!(r.input_len, 9);
}

#[test]
fn compute_checksum_crc16_ccitt_known_vector() {
    let r = compute_checksum_inner("crc16_ccitt", b"123456789").expect("checksum failed");
    assert_eq!(r.width_bits, 16);
    assert_eq!(r.value, 0x29B1);
}

#[test]
fn compute_checksum_simos_known_vector() {
    let r = compute_checksum_inner("crc32_simos", b"123456789").expect("checksum failed");
    assert_eq!(r.value, 0x89A1_897F);
}

#[test]
fn compute_checksum_ieee_matches_existing_impl() {
    let r = compute_checksum_inner("crc32_ieee", b"123456789").expect("checksum failed");
    assert_eq!(r.value, 0xCBF4_3926);
}

#[test]
fn compute_checksum_empty_input() {
    let castagnoli = compute_checksum_inner("crc32_castagnoli", &[]).expect("checksum failed");
    assert_eq!(castagnoli.value, 0x0000_0000);

    let ccitt = compute_checksum_inner("crc16_ccitt", &[]).expect("checksum failed");
    assert_eq!(ccitt.value, 0xFFFF);

    let simos = compute_checksum_inner("crc32_simos", &[]).expect("checksum failed");
    assert_eq!(simos.value, 0x0000_0000);
}

#[test]
fn compute_checksum_rejects_unknown_algorithm() {
    let err = compute_checksum_inner("crc16_modbus", b"123456789")
        .expect_err("crc16_modbus must be rejected: the source never implements it");
    assert!(
        err.contains("Unsupported checksum algorithm"),
        "unexpected error: {}",
        err
    );
    assert!(
        err.contains("crc16_modbus"),
        "error should name the algorithm: {}",
        err
    );
}

#[test]
fn compute_checksum_does_not_mutate_active_rom() {
    let state = AppState::default();
    load_bin_data_inner(&[0xAA; 32], &state).expect("load failed");

    let before = {
        let guard = state.memory.lock().unwrap();
        guard.as_ref().unwrap().hashes().clone()
    };

    let _ = compute_checksum_inner("crc32_castagnoli", &[0x11; 64]).expect("checksum failed");

    let after = {
        let guard = state.memory.lock().unwrap();
        guard.as_ref().unwrap().hashes().clone()
    };
    assert_eq!(
        before, after,
        "checksum computation must not mutate the ROM"
    );
}

#[test]
fn compute_checksum_is_deterministic() {
    let data: Vec<u8> = (0..=255u8).collect();
    let a = compute_checksum_inner("crc32_castagnoli", &data).expect("checksum failed");
    let b = compute_checksum_inner("crc32_castagnoli", &data).expect("checksum failed");
    assert_eq!(a, b);
    assert_eq!(a.value, 0x9C44_184B);
}
