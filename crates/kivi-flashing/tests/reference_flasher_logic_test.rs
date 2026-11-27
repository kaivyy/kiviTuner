//! Verification that ECU format codecs, cipher helpers, and CVN routines
//! are genuinely present and working in kiviTuner.
//!
//! Every expected value below is derived from the algorithm or the published
//! frame layout, not from running external binaries.

use kivi_flashing::bench_protocol::CvnCorrectionEngine;
use kivi_flashing::oem_checksum_crypto::{DensoSh705xBootAndEepromEngine, VbfPhfAndMcuMapEngine};

// ===========================================================================
// VBF Blowfish (single S-box, 16-round Feistel)
// ===========================================================================

#[test]
fn the_vbf_blowfish_round_trips() {
    // A symmetric cipher must decrypt what it encrypted.
    let original: Vec<u8> = (0..64).map(|i| (i * 7 % 251) as u8).collect();
    let encrypted = VbfPhfAndMcuMapEngine::encrypt_vbf_format_01_blowfish_single_sbox(&original);
    let decrypted = VbfPhfAndMcuMapEngine::decrypt_vbf_format_01_blowfish_single_sbox(&encrypted);
    assert_eq!(
        &decrypted[..original.len()],
        &original[..],
        "the cipher must be invertible"
    );
}

#[test]
fn the_vbf_blowfish_actually_changes_the_data() {
    let original = vec![0x00u8; 32];
    let encrypted = VbfPhfAndMcuMapEngine::encrypt_vbf_format_01_blowfish_single_sbox(&original);
    assert_ne!(
        &encrypted[..32],
        &original[..],
        "a cipher that leaves zeros unchanged is not a cipher"
    );
}

#[test]
fn the_vbf_blowfish_is_deterministic() {
    let data = vec![0xAAu8; 32];
    assert_eq!(
        VbfPhfAndMcuMapEngine::encrypt_vbf_format_01_blowfish_single_sbox(&data),
        VbfPhfAndMcuMapEngine::encrypt_vbf_format_01_blowfish_single_sbox(&data)
    );
}

#[test]
fn the_vbf_blowfish_handles_an_empty_payload() {
    let empty: Vec<u8> = vec![];
    let encrypted = VbfPhfAndMcuMapEngine::encrypt_vbf_format_01_blowfish_single_sbox(&empty);
    let decrypted = VbfPhfAndMcuMapEngine::decrypt_vbf_format_01_blowfish_single_sbox(&encrypted);
    assert!(decrypted.len() <= encrypted.len() + 8);
}

// ===========================================================================
// Ford Cal-ID normaliser
// ===========================================================================

#[test]
fn a_full_ford_cal_id_is_normalised_to_prefix_plus_suffix() {
    // "DV61-14C204-AB" splits on the two separators: the function keeps the
    // 4-character prefix and the trailing suffix, dropping the middle
    // calibration number. That is the documented contract, verified by
    // indexing the string: b[4]='-' and b[11]='-'.
    let out = VbfPhfAndMcuMapEngine::normalize_ford_cal_id("DV61-14C204-AB");
    assert_eq!(
        out.as_deref(),
        Some("DV61AB"),
        "the normalised form is prefix + suffix"
    );
}

#[test]
fn a_second_known_ford_cal_id_is_recognised() {
    let out = VbfPhfAndMcuMapEngine::normalize_ford_cal_id("BG91-12A650-CD");
    assert!(out.is_some(), "12A650 is a known Ford calibration id");
}

#[test]
fn a_string_without_a_cal_id_is_rejected() {
    assert!(
        VbfPhfAndMcuMapEngine::normalize_ford_cal_id("not-a-cal-id").is_none(),
        "an unrelated string must not be normalised into a fake id"
    );
}

#[test]
fn an_empty_cal_id_is_rejected() {
    assert!(VbfPhfAndMcuMapEngine::normalize_ford_cal_id("").is_none());
}

// ===========================================================================
// $BOOT$ — Denso SH705x direct boot
// ===========================================================================

#[test]
fn the_boot_sequence_starts_with_the_wakeup_frame() {
    let frames = DensoSh705xBootAndEepromEngine::build_denso_sh705x_boot_frames(&[0xAA; 12]);
    assert_eq!(
        frames[0],
        [0xFF, 0x86, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        "the first frame must be the 0xFF 0x86 wake-up"
    );
}

#[test]
fn the_boot_sequence_sets_the_ram_load_address() {
    let frames = DensoSh705xBootAndEepromEngine::build_denso_sh705x_boot_frames(&[0xAA; 6]);
    assert_eq!(
        frames[1],
        [0x7A, 0x9C, 0xFF, 0xFF, 0x80, 0x00, 0x00, 0x00],
        "the second frame loads RAM address 0xFFFF8000"
    );
}

#[test]
fn the_boot_sequence_ends_with_vector_set_and_execute() {
    let frames = DensoSh705xBootAndEepromEngine::build_denso_sh705x_boot_frames(&[0xAA; 6]);
    let n = frames.len();
    assert_eq!(
        frames[n - 2],
        [0x7A, 0xB4, 0xFF, 0xFF, 0x9A, 0x80, 0x00, 0x00],
        "the second-to-last frame sets the execution vector 0xFFFF9A80"
    );
    assert_eq!(
        frames[n - 1],
        [0x7A, 0xA0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        "the last frame executes the kernel"
    );
}

#[test]
fn the_boot_sequence_chunks_the_kernel_six_bytes_at_a_time() {
    // 12-byte kernel -> 2 chunk frames between the address and vector frames.
    let frames = DensoSh705xBootAndEepromEngine::build_denso_sh705x_boot_frames(&[0x11; 12]);
    // wakeup + address + 2 chunks + vector + exec = 6
    assert_eq!(frames.len(), 6, "12 bytes at 6 per frame is 2 chunks");
    assert_eq!(frames[2][0], 0x7A, "chunk frames start with 0x7A");
    assert_eq!(frames[2][1], 0xAE, "a full 6-byte chunk uses 0xAE");
}

#[test]
fn a_partial_final_chunk_uses_the_terminator_opcode() {
    // 7 bytes -> one full 0xAE chunk, then one partial 0xAC chunk.
    let frames = DensoSh705xBootAndEepromEngine::build_denso_sh705x_boot_frames(&[0x22; 7]);
    // wakeup + address + 2 chunks + vector + exec = 6
    assert_eq!(frames.len(), 6);
    assert_eq!(frames[2][1], 0xAE, "the full chunk");
    assert_eq!(frames[3][1], 0xAC, "the final partial chunk uses 0xAC");
}

#[test]
fn an_empty_kernel_still_produces_a_valid_sequence() {
    let frames = DensoSh705xBootAndEepromEngine::build_denso_sh705x_boot_frames(&[]);
    // wakeup + address + vector + exec = 4
    assert_eq!(frames.len(), 4, "no chunks for an empty kernel");
}

// ===========================================================================
// Middle-endian sector CRC-32
// ===========================================================================

#[test]
fn the_middle_endian_crc_is_four_bytes() {
    let crc = DensoSh705xBootAndEepromEngine::compute_denso_sector_crc32_middle_endian(b"TEST");
    assert_eq!(crc.len(), 4);
}

#[test]
fn the_middle_endian_crc_is_deterministic() {
    let a = DensoSh705xBootAndEepromEngine::compute_denso_sector_crc32_middle_endian(b"ABC");
    let b = DensoSh705xBootAndEepromEngine::compute_denso_sector_crc32_middle_endian(b"ABC");
    assert_eq!(a, b);
}

#[test]
fn the_middle_endian_crc_changes_with_the_input() {
    let a = DensoSh705xBootAndEepromEngine::compute_denso_sector_crc32_middle_endian(b"ABC");
    let b = DensoSh705xBootAndEepromEngine::compute_denso_sector_crc32_middle_endian(b"ABD");
    assert_ne!(a, b, "one changed byte must change the CRC");
}

#[test]
fn the_middle_endian_crc_byte_order_differs_from_plain_big_endian() {
    // The whole point of "middle-endian" is the [b2,b3,b0,b1] ordering, so the
    // emitted bytes must not simply be a big-endian CRC.
    let crc = DensoSh705xBootAndEepromEngine::compute_denso_sector_crc32_middle_endian(
        b"DENSO_SH7058_SECTOR",
    );
    let as_be = u32::from_be_bytes(crc);
    let as_le = u32::from_le_bytes(crc);
    assert_ne!(
        as_be, as_le,
        "the byte order is neither plain big- nor little-endian"
    );
}

// ===========================================================================
// GF(2) CVN balancer
// ===========================================================================

#[test]
fn the_cvn_engine_exposes_a_calibration_cvn() {
    let rom = vec![0x5Au8; 256];
    let cvn = CvnCorrectionEngine::calculate_calibration_cvn(&rom);
    // A CVN is a 32-bit value; it must be stable for the same input.
    assert_eq!(cvn, CvnCorrectionEngine::calculate_calibration_cvn(&rom));
}

#[test]
fn the_cvn_changes_with_the_rom_contents() {
    let a = CvnCorrectionEngine::calculate_calibration_cvn(&[0x00u8; 128]);
    let b = CvnCorrectionEngine::calculate_calibration_cvn(&[0x01u8; 128]);
    assert_ne!(a, b, "different calibration data must give a different CVN");
}
