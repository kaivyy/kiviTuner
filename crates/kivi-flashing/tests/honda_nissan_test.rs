//! Parity tests for Honda K-Line framing and the Shindengen/Nissan seed-key engines.
//!
//! Ported from `ecu_engine_rust::flashing::{honda_kline, nissan_hitachi}`.
//!
//! ## How expected values were obtained
//!
//! The Honda checksum is `0x100 - (sum & 0xFF)`, a two's-complement byte
//! checksum — every expectation below is computed by hand from that formula.
//! Seed-key outputs are asserted as **invariants** (determinism, size, change
//! with input) because the key schedule is table-driven and a magic vector would
//! only prove the table matches itself.

use kivi_flashing::honda_kline::{HondaProtocolFraming, ShindengenSeedKeyEngine};
use kivi_flashing::nissan_hitachi::{
    DifferentialFlashPlanner, DifferentialSectorPlan, NissanHitachiSeedKey,
    RenesasSh705xFtdarProgrammer,
};

// ---------------------------------------------------------------------------
// Honda 8-bit checksum
// ---------------------------------------------------------------------------

#[test]
fn an_empty_checksum_is_zero() {
    // sum = 0 -> 0x100 - 0 = 0x100 -> masked to 0x00.
    assert_eq!(HondaProtocolFraming::checksum_8bit(&[]), 0x00);
}

#[test]
fn a_single_byte_checksum_is_its_twos_complement() {
    // sum = 0x01 -> 0x100 - 0x01 = 0xFF.
    assert_eq!(HondaProtocolFraming::checksum_8bit(&[0x01]), 0xFF);
    // sum = 0xFF -> 0x100 - 0xFF = 0x01.
    assert_eq!(HondaProtocolFraming::checksum_8bit(&[0xFF]), 0x01);
}

#[test]
fn the_checksum_makes_the_total_sum_zero_modulo_256() {
    // Adding the checksum must make the byte sum wrap to 0.
    for data in [
        vec![0x01u8, 0x02, 0x03],
        vec![0xFF; 8],
        vec![0x10, 0x20, 0x30, 0x40],
    ] {
        let cs = HondaProtocolFraming::checksum_8bit(&data);
        let total: u16 = data.iter().map(|&b| b as u16).sum::<u16>() + cs as u16;
        assert_eq!(total & 0xFF, 0, "sum + checksum must be 0 mod 256");
    }
}

#[test]
fn the_checksum_only_depends_on_the_low_byte_of_the_sum() {
    // Two inputs whose sums differ by 256 must produce the same checksum.
    let a = vec![0x01u8];
    let b = vec![0x01u8, 0xFF, 0x01]; // sum = 0x101
    assert_eq!(
        HondaProtocolFraming::checksum_8bit(&a),
        HondaProtocolFraming::checksum_8bit(&b)
    );
}

// ---------------------------------------------------------------------------
// Honda packet framing
// ---------------------------------------------------------------------------

#[test]
fn a_packet_layout_is_header_len_service_subcmd_payload_checksum() {
    let pkt = HondaProtocolFraming::build_packet(0xFE, 0x72, 0x00, &[0xAA, 0xBB]);
    // payload 2 + 5 = 7 total bytes.
    assert_eq!(pkt.len(), 7);
    assert_eq!(pkt[0], 0xFE, "header");
    assert_eq!(pkt[1], 7, "length field counts the whole packet");
    assert_eq!(pkt[2], 0x72, "service");
    assert_eq!(pkt[3], 0x00, "sub-command");
    assert_eq!(&pkt[4..6], &[0xAA, 0xBB], "payload");
}

#[test]
fn a_built_packet_verifies() {
    for payload in [vec![], vec![0x01], vec![0xAA; 32]] {
        let pkt = HondaProtocolFraming::build_packet(0xFE, 0x72, 0x00, &payload);
        assert!(
            HondaProtocolFraming::verify_packet(&pkt),
            "a freshly built packet must verify (payload len {})",
            payload.len()
        );
    }
}

#[test]
fn a_corrupted_packet_fails_verification() {
    let mut pkt = HondaProtocolFraming::build_packet(0xFE, 0x72, 0x00, &[0x01, 0x02]);
    let last = pkt.len() - 1;
    pkt[last] ^= 0xFF;
    assert!(!HondaProtocolFraming::verify_packet(&pkt));
}

#[test]
fn a_packet_shorter_than_four_bytes_fails_verification() {
    for len in 0..4usize {
        assert!(!HondaProtocolFraming::verify_packet(&vec![0u8; len]));
    }
}

// ---------------------------------------------------------------------------
// Shindengen seed-key
// ---------------------------------------------------------------------------

#[test]
fn a_rom_without_the_shindengen_signature_is_rejected() {
    // The engine locates a signature table inside the ROM. A buffer of uniform
    // bytes cannot contain it, so the call must fail with an explicit message
    // rather than returning a fabricated key.
    let bin = vec![0x5Au8; 512];
    let err = ShindengenSeedKeyEngine::compute_write_key(&bin, [0x12, 0x34])
        .expect_err("a ROM without the signature must be refused");
    assert!(
        err.contains("signature not found"),
        "error must name the missing signature: {err}"
    );
}

#[test]
fn an_empty_rom_is_rejected_by_the_seed_key_engine() {
    assert!(ShindengenSeedKeyEngine::compute_write_key(&[], [0, 0]).is_err());
}

#[test]
fn the_recovery_engine_also_refuses_a_rom_without_the_signature() {
    let bin = vec![0x00u8; 256];
    assert!(ShindengenSeedKeyEngine::compute_recovery_key(&bin, [0, 0]).is_err());
}

#[test]
fn the_shindengen_checksum32_is_deterministic() {
    let bin: Vec<u8> = (0..1024).map(|i| (i % 251) as u8).collect();
    let a = ShindengenSeedKeyEngine::calculate_shindengen_checksum32(&bin, 0x10);
    let b = ShindengenSeedKeyEngine::calculate_shindengen_checksum32(&bin, 0x10);
    assert_eq!(a, b);
}

// ---------------------------------------------------------------------------
// Nissan Hitachi seed-key
// ---------------------------------------------------------------------------

#[test]
fn the_nissan_key_is_four_bytes_and_deterministic() {
    let seed = [0x11u8, 0x22, 0x33, 0x44];
    let a = NissanHitachiSeedKey::compute_key2(seed);
    let b = NissanHitachiSeedKey::compute_key2(seed);
    assert_eq!(a, b);
    assert_eq!(a.len(), 4);
}

#[test]
fn different_nissan_seeds_produce_different_keys() {
    let a = NissanHitachiSeedKey::compute_key2([0x00, 0x00, 0x00, 0x00]);
    let b = NissanHitachiSeedKey::compute_key2([0x00, 0x00, 0x00, 0x01]);
    assert_ne!(a, b);
}

// ---------------------------------------------------------------------------
// Differential flash planner
// ---------------------------------------------------------------------------

#[test]
fn an_identical_rom_plans_no_work() {
    let rom = vec![0xAAu8; 4096];
    let plan = DifferentialFlashPlanner::create_plan(&rom, &rom, &[1024, 1024, 1024, 1024])
        .expect("plan");
    assert_eq!(
        DifferentialFlashPlanner::total_bytes_to_flash(&plan),
        0,
        "an unchanged ROM must not schedule any sector"
    );
}

#[test]
fn a_single_changed_sector_is_the_only_one_planned() {
    let original = vec![0x00u8; 4096];
    let mut tuned = original.clone();
    tuned[2048] = 0xFF; // inside the third 1024-byte sector
    let plan = DifferentialFlashPlanner::create_plan(&original, &tuned, &[1024, 1024, 1024, 1024])
        .expect("plan");
    assert_eq!(
        DifferentialFlashPlanner::total_bytes_to_flash(&plan),
        1024,
        "only the third sector changed"
    );
}

#[test]
fn a_fully_changed_rom_plans_every_sector() {
    let original = vec![0x00u8; 4096];
    let tuned = vec![0xFFu8; 4096];
    let plan = DifferentialFlashPlanner::create_plan(&original, &tuned, &[1024, 1024, 1024, 1024])
        .expect("plan");
    assert_eq!(
        DifferentialFlashPlanner::total_bytes_to_flash(&plan),
        4096,
        "every sector must be scheduled"
    );
}

#[test]
fn a_short_sector_list_only_covers_the_bytes_it_declares() {
    // The source stops when the sector list runs out; it does not require the
    // sectors to cover the whole ROM. Bytes past the last sector are simply not
    // planned. This is documented rather than treated as an error.
    let original = vec![0x00u8; 4096];
    let tuned = vec![0xFFu8; 4096];
    let plan = DifferentialFlashPlanner::create_plan(&original, &tuned, &[1024, 1024])
        .expect("a short sector list is accepted");
    assert_eq!(
        DifferentialFlashPlanner::total_bytes_to_flash(&plan),
        2048,
        "only the two declared sectors are planned"
    );
}

#[test]
fn mismatched_rom_lengths_are_rejected() {
    let a = vec![0u8; 4096];
    let b = vec![0u8; 2048];
    assert!(
        DifferentialFlashPlanner::create_plan(&a, &b, &[1024, 1024]).is_err(),
        "the two ROMs must be the same length"
    );
}

#[test]
fn a_plan_for_an_empty_sector_list_is_empty() {
    let plan: Vec<DifferentialSectorPlan> = vec![];
    assert_eq!(DifferentialFlashPlanner::total_bytes_to_flash(&plan), 0);
}

// ---------------------------------------------------------------------------
// Renesas SH705x FTDAR addressing
// ---------------------------------------------------------------------------

#[test]
fn a_24_bit_address_in_the_ram_window_sign_extends_to_0xffff() {
    // Only addresses with bit 23 set AND landing in 0xFFFF0000..=0xFFFFDFFF are
    // valid; that is the SH705x on-chip RAM window.
    let addr = RenesasSh705xFtdarProgrammer::sign_extend_ram_address_24_to_32(0xFF_8000)
        .expect("0xFF8000 is inside the RAM window");
    assert_eq!(addr, 0xFFFF_8000);
}

#[test]
fn a_24_bit_address_without_bit_23_set_is_rejected() {
    let err = RenesasSh705xFtdarProgrammer::sign_extend_ram_address_24_to_32(0x00_1234)
        .expect_err("0x001234 is not in the RAM window");
    assert!(err.contains("bit 23"), "error must explain the rule: {err}");
}

#[test]
fn an_address_outside_the_ram_window_is_rejected() {
    // Bit 23 set but the result would be 0xFFFFE000, past the window end.
    let err = RenesasSh705xFtdarProgrammer::sign_extend_ram_address_24_to_32(0xFF_E000)
        .expect_err("0xFFFFE000 is past 0xFFFFDFFF");
    assert!(err.contains("outside"), "error: {err}");
}

#[test]
fn an_address_above_24_bits_is_rejected() {
    assert!(RenesasSh705xFtdarProgrammer::sign_extend_ram_address_24_to_32(0x0100_0000).is_err());
}

#[test]
fn the_24_bit_diagnostic_bytes_are_big_endian_and_ordered() {
    // The function emits the three address bytes of the 0xFFFFxxxx value:
    // (addr >> 16) & 0xFF, (addr >> 8) & 0xFF, addr & 0xFF.
    // For 0xFFFF_ABCD that is FF, AB, CD.
    let bytes = RenesasSh705xFtdarProgrammer::to_24bit_diagnostic_bytes(0xFFFF_ABCD)
        .expect("convert");
    assert_eq!(bytes, [0xFF, 0xAB, 0xCD], "address bytes, most-significant first");
}

#[test]
fn an_address_outside_the_ffff_segment_cannot_be_encoded() {
    let err = RenesasSh705xFtdarProgrammer::to_24bit_diagnostic_bytes(0x0100_0000)
        .expect_err("must reject");
    assert!(err.contains("0xFFFF"), "error: {err}");
}
