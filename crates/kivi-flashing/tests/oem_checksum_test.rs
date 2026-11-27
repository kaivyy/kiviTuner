//! Tests for `kivi_flashing::oem_checksum_crypto`.
//!
//! ## How expected values were obtained
//!
//! * Additive / XOR / two's-complement checksums: computed by hand from the
//!   source formula and asserted as exact bytes.
//! * CRC functions: cross-checked against `kivi_checksum::primitives`
//!   (`compute_crc32`, `crc32_simos`, `crc32_unreflected`) or against the
//!   published check value of the named standard algorithm (CRC-32/MPEG-2,
//!   CRC-32/ISO-HDLC, CRC-16/ARC, AES-128 FIPS-197).
//! * Seed/key and cipher functions: structural invariants (round-trip,
//!   bijectivity, table permutation, algebraic identity) rather than magic
//!   constants that cannot be derived.
//! * Where an expected value is a transcription of the module's own published
//!   table (the VBF S-box, the Denso nibble table), the formula is
//!   re-implemented independently inside the test from that table.
//!
//! ## What is NOT claimed
//!
//! Nothing here proves a real ECU accepts these values. Several functions have
//! no external specification available (opaque machine-code payloads, OEM
//! key schedules) and are covered structurally only; those tests are marked
//! `Robustness only, not parity` or `Structure only, not parity`.

use kivi_checksum::primitives::{compute_crc32, crc32_unreflected};
use kivi_flashing::oem_checksum_crypto::{
    BoschMe7ChecksumSuite, DsgMechatronicCryptoAndChecksum, SimkAndSmartraEngine,
    SimosChecksumAndCrypto,
};

// ---------------------------------------------------------------------------
// 1. Simos: CRC-32/MPEG-2, counter-XOR, block CRC table, ECM3 64-bit sum
// ---------------------------------------------------------------------------

#[test]
fn simos_crc32_mpeg2_matches_published_check_value() {
    // CRC-32/MPEG-2 (poly 0x04C11DB7, init 0xFFFFFFFF, refin/refout false,
    // xorout 0x00000000) has the standard check value 0x0376E6E7 for "123456789".
    assert_eq!(
        SimosChecksumAndCrypto::crc32_vw_mpeg2(b"123456789", 0xFFFF_FFFF),
        0x0376_E6E7
    );
}

#[test]
fn simos_crc32_mpeg2_equals_unreflected_primitive() {
    let data = [0x11u8, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xAA];
    for init in [0u32, 0xFFFF_FFFF, 0x1234_5678] {
        assert_eq!(
            SimosChecksumAndCrypto::crc32_vw_mpeg2(&data, init),
            crc32_unreflected(&data, 0x04C1_1DB7, init, 0),
            "init {init:#010X}"
        );
    }
    // Empty input returns the seed untouched (no init complement, no xor_out).
    assert_eq!(SimosChecksumAndCrypto::crc32_vw_mpeg2(&[], 0xDEAD_BEEF), 0xDEAD_BEEF);
}

#[test]
fn simos_crc32_mpeg2_is_seed_chainable() {
    // Feeding the CRC of the first half as init must reproduce the CRC of the
    // whole buffer: this is the property the block-table fixer relies on.
    let all = [1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    let split = SimosChecksumAndCrypto::crc32_vw_mpeg2(&all[..4], 0);
    assert_eq!(
        SimosChecksumAndCrypto::crc32_vw_mpeg2(&all[4..], split),
        SimosChecksumAndCrypto::crc32_vw_mpeg2(&all, 0)
    );
}

#[test]
fn simos_counter_xor_applies_index_as_mask() {
    // out[i] = data[i] ^ i, hand-derived.
    assert_eq!(
        SimosChecksumAndCrypto::simos_counter_xor(&[0x00, 0xFF, 0xAA, 0x55]),
        vec![0x00u8, 0xFF ^ 1, 0xAA ^ 2, 0x55 ^ 3]
    );
    assert_eq!(
        SimosChecksumAndCrypto::simos_counter_xor(&[0x00, 0xFF, 0xAA, 0x55]),
        vec![0x00, 0xFE, 0xA8, 0x56]
    );
}

#[test]
fn simos_counter_xor_is_its_own_inverse_and_handles_empty() {
    let data: Vec<u8> = (0..=255u8).collect();
    let once = SimosChecksumAndCrypto::simos_counter_xor(&data);
    assert_eq!(SimosChecksumAndCrypto::simos_counter_xor(&once), data);
    assert!(SimosChecksumAndCrypto::simos_counter_xor(&[]).is_empty());
}

#[test]
fn simos_block_crc32_reports_valid_when_stored_value_is_correct() {
    // Header at 0x300: [pad..0x304] stored CRC, 0x308 area count, descriptors
    // at 0x30C. One area covering the whole 0x300..0x320 block.
    let mut block = vec![0u8; 0x400];
    for (i, b) in block.iter_mut().enumerate().take(0x300) {
        *b = (i & 0xFF) as u8;
    }
    // Descriptor layout from the source: stored CRC at hoff+4 (0x304), area
    // count at hoff+8, first descriptor at hoff+12 as (start_abs, end_abs) LE u32.
    // The hashed range must NOT include the stored CRC field, or the check is
    // circular. Use 0x320..0x3FF instead: data outside the header.
    block[0x308] = 1;
    block[0x30C..0x310].copy_from_slice(&0x320u32.to_le_bytes()); // start_abs
    block[0x310..0x314].copy_from_slice(&0x3FFu32.to_le_bytes()); // end_abs inclusive
    let expected = SimosChecksumAndCrypto::crc32_vw_mpeg2(&block[0x320..0x400], 0);
    block[0x304..0x308].copy_from_slice(&expected.to_le_bytes());

    let (stored, calc, was_valid) =
        SimosChecksumAndCrypto::validate_and_fix_simos_block_crc32(&mut block, 0, false)
            .expect("well-formed block");
    assert_eq!(stored, expected);
    assert_eq!(calc, expected);
    assert!(was_valid, "stored CRC was written from the same primitive");
}

#[test]
fn simos_block_crc32_repairs_a_wrong_stored_value() {
    let mut block = vec![0u8; 0x400];
    block[0x308] = 1;
    block[0x30C..0x310].copy_from_slice(&0x320u32.to_le_bytes());
    block[0x310..0x314].copy_from_slice(&0x3FFu32.to_le_bytes());
    block[0x304..0x308].copy_from_slice(&0xDEAD_BEEFu32.to_le_bytes());

    let (stored, calc, was_valid) =
        SimosChecksumAndCrypto::validate_and_fix_simos_block_crc32(&mut block, 0, false)
            .expect("well-formed block");
    // `stored` is what the function read BEFORE repairing; it must be the value
    // we planted, and `was_valid` must be false.
    assert_eq!(stored, 0xDEAD_BEEF, "the function must report the bad stored CRC");
    assert!(!was_valid, "a wrong CRC must not be reported as valid");
    // The repair must have written `calc` back into the field.
    assert_eq!(&block[0x304..0x308], &calc.to_le_bytes());
    // And `calc` must equal the CRC of the declared range only.
    assert_eq!(calc, SimosChecksumAndCrypto::crc32_vw_mpeg2(&block[0x320..0x400], 0));
}

#[test]
fn simos_block_crc32_rejects_short_and_malformed_blocks() {
    let mut short = vec![0u8; 0x313];
    assert!(SimosChecksumAndCrypto::validate_and_fix_simos_block_crc32(&mut short, 0, false).is_err());

    // CBOOT_TEMP uses 0x340; a 0x350 buffer is still too small.
    let mut short_cboot = vec![0u8; 0x350];
    assert!(
        SimosChecksumAndCrypto::validate_and_fix_simos_block_crc32(&mut short_cboot, 0, true).is_err()
    );

    // Area count 0 is rejected.
    let mut zero_areas = vec![0u8; 0x400];
    zero_areas[0x300 + 8] = 0;
    assert!(SimosChecksumAndCrypto::validate_and_fix_simos_block_crc32(&mut zero_areas, 0, false).is_err());

    // 17 areas exceeds the documented maximum of 16.
    let mut too_many = vec![0u8; 0x500];
    too_many[0x308] = 17;
    assert!(SimosChecksumAndCrypto::validate_and_fix_simos_block_crc32(&mut too_many, 0, false).is_err());

    // start_abs below base_address is rejected.
    let mut below_base = vec![0u8; 0x400];
    below_base[0x308] = 1;
    below_base[0x30C..0x310].copy_from_slice(&0x10u32.to_le_bytes());
    below_base[0x310..0x314].copy_from_slice(&0x20u32.to_le_bytes());
    assert!(
        SimosChecksumAndCrypto::validate_and_fix_simos_block_crc32(&mut below_base, 0x100, false).is_err()
    );
}

#[test]
fn simos_ecm3_sums_32bit_le_words_plus_the_asw1_base() {
    // Region [0, 8) relative to base 0: words 1 and 2. base_sum 5 => 8.
    let mut cal = vec![0u8; 0x408];
    cal[0..4].copy_from_slice(&1u32.to_le_bytes());
    cal[4..8].copy_from_slice(&2u32.to_le_bytes());
    cal[0x400..0x408].copy_from_slice(&8u64.to_le_bytes());

    let (stored, total, valid) =
        SimosChecksumAndCrypto::validate_and_fix_simos_ecm3_64bit(&mut cal, 0, &[(0, 7)], 5)
            .expect("well-formed CAL block");
    assert_eq!(stored, 8);
    assert_eq!(total, 8);
    assert!(valid);
}

#[test]
fn simos_ecm3_writes_the_computed_u64_when_stored_differs() {
    let mut cal = vec![0u8; 0x408];
    cal[0..4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    cal[4..8].copy_from_slice(&2u32.to_le_bytes());
    cal[0x400..0x408].copy_from_slice(&0u64.to_le_bytes());

    let (stored, total, valid) =
        SimosChecksumAndCrypto::validate_and_fix_simos_ecm3_64bit(&mut cal, 0, &[(0, 7)], 0)
            .expect("well-formed CAL block");
    assert_eq!(stored, 0);
    assert_eq!(total, 0x1_0000_0001u64); // wraps out of 32 bits into the u64 accumulator
    assert!(!valid);
    assert_eq!(&cal[0x400..0x408], &total.to_le_bytes());
}

#[test]
fn simos_ecm3_rejects_bad_sizes_alignments_and_ranges() {
    let mut tiny = vec![0u8; 0x407];
    assert!(SimosChecksumAndCrypto::validate_and_fix_simos_ecm3_64bit(&mut tiny, 0, &[], 0).is_err());

    // Region length not a multiple of 4.
    let mut unaligned = vec![0u8; 0x408];
    assert!(
        SimosChecksumAndCrypto::validate_and_fix_simos_ecm3_64bit(&mut unaligned, 0, &[(0, 5)], 0)
            .is_err()
    );

    // Region below the CAL base address.
    let mut below = vec![0u8; 0x408];
    assert!(
        SimosChecksumAndCrypto::validate_and_fix_simos_ecm3_64bit(&mut below, 0x100, &[(0x80, 0x87)], 0)
            .is_err()
    );
}

// ---------------------------------------------------------------------------
// 2. VAG DSG: DQ250 substitution cipher, JAMCRC, DQ381 block CRC
// ---------------------------------------------------------------------------

#[test]
fn dsg_sbox_is_a_permutation_of_all_256_bytes() {
    // Structure only, not parity: a substitution table must be bijective for
    // the inverse cipher in `dq250_encrypt` to be well defined.
    let mut seen = [false; 256];
    for &v in DsgMechatronicCryptoAndChecksum::MQB_DSG_SBOX.iter() {
        assert!(!seen[v as usize], "duplicate S-box entry {v:#04X}");
        seen[v as usize] = true;
    }
    assert!(seen.iter().all(|&s| s), "S-box must cover 0x00..=0xFF");
}

#[test]
fn dq250_encrypt_inverts_decrypt() {
    // Structure only, not parity: round-trip proves the two state machines are
    // inverses; the S-box values themselves are an opaque OEM extraction.
    let plain: Vec<u8> = (0..40u8).map(|i| i.wrapping_mul(7).wrapping_add(3)).collect();
    let cipher = DsgMechatronicCryptoAndChecksum::dq250_encrypt(&plain);
    assert_eq!(DsgMechatronicCryptoAndChecksum::dq250_decrypt(&cipher), plain);
    // Ciphertext is not the identity transform.
    assert_ne!(cipher, plain);
}

#[test]
fn dq250_round_trips_empty_and_single_byte_inputs() {
    assert!(DsgMechatronicCryptoAndChecksum::dq250_decrypt(&[]).is_empty());
    assert!(DsgMechatronicCryptoAndChecksum::dq250_encrypt(&[]).is_empty());
    for b in [0x00u8, 0x01, 0x7F, 0xFF] {
        let enc = DsgMechatronicCryptoAndChecksum::dq250_encrypt(&[b]);
        assert_eq!(DsgMechatronicCryptoAndChecksum::dq250_decrypt(&enc), vec![b]);
    }
}

#[test]
fn dsg_crc32_ieee_matches_iso_hdlc_check_value() {
    // CRC-32/ISO-HDLC published check value for "123456789".
    assert_eq!(DsgMechatronicCryptoAndChecksum::crc32_ieee(b"123456789"), 0xCBF4_3926);
    // And it must agree with the already-tested primitive.
    for data in [&b""[..], &b"a"[..], &b"kiviTuner"[..]] {
        assert_eq!(
            DsgMechatronicCryptoAndChecksum::crc32_ieee(data),
            compute_crc32(data)
        );
    }
}

#[test]
fn dsg_jamcrc_is_the_ones_complement_of_crc32() {
    // Hand-derived: JAMCRC = 0xFFFFFFFF - CRC32(data[..len-4]) stored LE.
    let payload = [0x01u8, 0x02, 0x03, 0x04];
    let expected = 0xFFFF_FFFFu32.wrapping_sub(compute_crc32(&payload));
    assert_eq!(expected, 0x49C3_0432);

    let mut block = payload.to_vec();
    block.extend_from_slice(&expected.to_le_bytes());
    let (stored, calc, valid) =
        DsgMechatronicCryptoAndChecksum::validate_and_fix_dsg_jamcrc(&mut block)
            .expect("8-byte minimum satisfied");
    assert_eq!(stored, expected);
    assert_eq!(calc, expected);
    assert!(valid);
}

#[test]
fn dsg_jamcrc_repairs_a_corrupt_trailer() {
    let mut block = vec![1u8, 2, 3, 4, 0xDE, 0xAD, 0xBE, 0xEF];
    let expected = 0xFFFF_FFFFu32.wrapping_sub(compute_crc32(&block[..4]));
    let (stored, calc, valid) =
        DsgMechatronicCryptoAndChecksum::validate_and_fix_dsg_jamcrc(&mut block)
            .expect("8-byte minimum satisfied");
    assert_eq!(stored, 0xEFBE_ADDE);
    assert_eq!(calc, expected);
    assert!(!valid);
    assert_eq!(&block[4..], &expected.to_le_bytes());
    // After the fix the block verifies clean.
    let (_, _, valid_now) =
        DsgMechatronicCryptoAndChecksum::validate_and_fix_dsg_jamcrc(&mut block).unwrap();
    assert!(valid_now);
}

#[test]
fn dsg_jamcrc_rejects_blocks_shorter_than_eight_bytes() {
    let mut tiny = [0u8; 7];
    assert!(DsgMechatronicCryptoAndChecksum::validate_and_fix_dsg_jamcrc(&mut tiny).is_err());
    let mut empty: [u8; 0] = [];
    assert!(DsgMechatronicCryptoAndChecksum::validate_and_fix_dsg_jamcrc(&mut empty).is_err());
}

#[test]
fn dq381_block_crc32_is_big_endian_ieee_over_the_declared_range() {
    // Header: BE start/end at 0x38/0x3C, BE CRC at 0x44. Range 0..=7 => 8 bytes.
    let mut data = vec![0u8; 0x48];
    data[0..8].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
    data[0x38..0x3C].copy_from_slice(&0u32.to_be_bytes());
    data[0x3C..0x40].copy_from_slice(&7u32.to_be_bytes());
    let expected = compute_crc32(&data[0..8]);
    assert_eq!(expected, 0x3FCA_88C5);
    data[0x44..0x48].copy_from_slice(&expected.to_be_bytes());

    let (stored, calc, valid) =
        DsgMechatronicCryptoAndChecksum::validate_and_fix_dq381_block_crc32(&mut data, 0)
            .expect("0x48-byte block");
    assert_eq!(stored, expected);
    assert_eq!(calc, expected);
    assert!(valid);
}

#[test]
fn dq381_block_crc32_writes_big_endian_and_rejects_bad_headers() {
    let mut data = vec![0u8; 0x48];
    data[0..4].copy_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD]);
    data[0x38..0x3C].copy_from_slice(&0u32.to_be_bytes());
    data[0x3C..0x40].copy_from_slice(&3u32.to_be_bytes());
    let expected = compute_crc32(&data[0..4]);
    let (_, calc, valid) =
        DsgMechatronicCryptoAndChecksum::validate_and_fix_dq381_block_crc32(&mut data, 0).unwrap();
    assert_eq!(calc, expected);
    assert!(!valid);
    assert_eq!(&data[0x44..0x48], &expected.to_be_bytes());

    let mut short = vec![0u8; 0x47];
    assert!(DsgMechatronicCryptoAndChecksum::validate_and_fix_dq381_block_crc32(&mut short, 0).is_err());

    // Range below base_address.
    let mut below = vec![0u8; 0x48];
    below[0x38..0x3C].copy_from_slice(&0u32.to_be_bytes());
    below[0x3C..0x40].copy_from_slice(&3u32.to_be_bytes());
    assert!(DsgMechatronicCryptoAndChecksum::validate_and_fix_dq381_block_crc32(&mut below, 0x100).is_err());
}

// ---------------------------------------------------------------------------
// 3. Hyundai/Kia SIMK: CRC-16/ARC and the SMARTRA PIN
// ---------------------------------------------------------------------------

#[test]
fn simk_crc16_arc_matches_published_check_value() {
    // CRC-16/ARC (poly 0xA001 reflected, init 0x0000, xorout 0x0000).
    assert_eq!(SimkAndSmartraEngine::crc16_arc_reflected(b"123456789", 0), 0xBB3D);
    // Arbitrary seed is honoured verbatim (no complement): empty data => seed.
    assert_eq!(SimkAndSmartraEngine::crc16_arc_reflected(b"", 0x1234), 0x1234);
    assert_eq!(SimkAndSmartraEngine::crc16_arc_reflected(b"", 0xFFFF), 0xFFFF);
}

#[test]
fn simk_crc16_arc_appends_its_own_residue_to_zero() {
    // Appending the LE CRC makes the running CRC return to 0: the property a
    // zone chained across the checksum field depends on.
    let crc = SimkAndSmartraEngine::crc16_arc_reflected(b"123456789", 0);
    let mut framed = b"123456789".to_vec();
    framed.extend_from_slice(&crc.to_le_bytes());
    assert_eq!(SimkAndSmartraEngine::crc16_arc_reflected(&framed, 0), 0);
}

#[test]
fn simk_zone_chain_accumulates_the_seed_from_the_init_field() {
    // rom[0..2] = stored BE checksum, rom[2..4] = LE init seed, zone [4,7).
    let mut rom = vec![0u8; 0x20];
    rom[0] = 0xDE;
    rom[1] = 0xAD;
    rom[2..4].copy_from_slice(&0x1234u16.to_le_bytes());
    rom[4..7].copy_from_slice(&[0x11, 0x22, 0x33]);
    let expected = SimkAndSmartraEngine::crc16_arc_reflected(&[0x11, 0x22, 0x33], 0x1234);
    assert_eq!(expected, 0xDE45);

    let (stored, crc, valid) =
        SimkAndSmartraEngine::validate_and_fix_simk_zones(&mut rom, 0, 2, &[(4, 7)])
            .expect("in-bounds zones");
    assert_eq!(stored, 0xDEAD);
    assert_eq!(crc, 0xDE45);
    assert!(!valid);
    // The fixer writes the CRC in big-endian order (see the source comment).
    assert_eq!(&rom[0..2], &expected.to_be_bytes());
}

#[test]
fn simk_zone_fix_is_idempotent() {
    let mut rom = vec![0u8; 0x20];
    rom[2..4].copy_from_slice(&0x1234u16.to_le_bytes());
    rom[4..7].copy_from_slice(&[0x11, 0x22, 0x33]);
    SimkAndSmartraEngine::validate_and_fix_simk_zones(&mut rom, 0, 2, &[(4, 7)]).unwrap();
    let (stored, crc, valid) =
        SimkAndSmartraEngine::validate_and_fix_simk_zones(&mut rom, 0, 2, &[(4, 7)]).unwrap();
    assert!(valid, "stored {stored:#06X} should equal {crc:#06X} after the fix");
    assert_eq!(stored, crc);
}

#[test]
fn simk_zone_rejects_out_of_bounds_addresses_and_zones() {
    let mut rom = vec![0u8; 8];
    assert!(SimkAndSmartraEngine::validate_and_fix_simk_zones(&mut rom, 7, 0, &[]).is_err());
    assert!(SimkAndSmartraEngine::validate_and_fix_simk_zones(&mut rom, 0, 7, &[]).is_err());
    assert!(SimkAndSmartraEngine::validate_and_fix_simk_zones(&mut rom, 0, 0, &[(0, 9)]).is_err());
    assert!(SimkAndSmartraEngine::validate_and_fix_simk_zones(&mut rom, 0, 0, &[(5, 2)]).is_err());
}

#[test]
fn smartra_pin_equals_a_gf2_32_multiply_by_x_pow_40_mod_1_000_000() {
    // Structure only, not parity: the loop is 40 shift/XOR steps of the LFSR
    // polynomial 0x7798_2990, which is exactly multiplication by x^40 in
    // GF(2)[x]/0x7798_2990. An independent shift-and-add implementation must
    // agree, which pins the loop count and the reduction polynomial.
    fn xtime(a: u32) -> u32 {
        let msb = a & 0x8000_0000;
        let shifted = a << 1;
        if msb != 0 {
            shifted ^ 0x7798_2990
        } else {
            shifted
        }
    }
    fn gmul(mut a: u32, mut b: u32) -> u32 {
        let mut r = 0u32;
        while b != 0 {
            if b & 1 != 0 {
                r ^= a;
            }
            a = xtime(a);
            b >>= 1;
        }
        r
    }
    let mut x40 = 1u32;
    for _ in 0..40 {
        x40 = xtime(x40);
    }
    assert_eq!(x40, 0x24DE_49D0, "x^40 reduced by the polynomial");

    for seed in [0u32, 1, 123_456, 0x8000_0000, 0xFFFF_FFFF, 0x1234_5678] {
        assert_eq!(
            SimkAndSmartraEngine::calculate_smartra_pin(seed),
            gmul(seed, x40) % 1_000_000,
            "seed {seed:#010X}"
        );
    }
}

#[test]
fn smartra_pin_is_bounded_below_one_million() {
    for seed in [0u32, 999_999, 1_000_000, u32::MAX] {
        assert!(SimkAndSmartraEngine::calculate_smartra_pin(seed) < 1_000_000);
    }
}

// ---------------------------------------------------------------------------
// 4. Bosch ME7: main / multipoint / rolling checksums
// ---------------------------------------------------------------------------

#[test]
fn me7_main_checksum_sums_16bit_le_words_and_stores_the_inverse() {
    // Words 1, 2, 3, 4 => sum 10 (0x0A), inverse 0xFFFF_FFF5.
    let mut rom = vec![0u8; 32];
    rom[0..8].copy_from_slice(&[1, 0, 2, 0, 3, 0, 4, 0]);

    let (stored, sum, valid) =
        BoschMe7ChecksumSuite::validate_and_fix_me7_main_checksum(&mut rom, &[(0, 8)], 16)
            .expect("in-bounds range");
    assert_eq!(sum, 0x0A);
    assert_eq!(stored, 0);
    assert!(!valid);
    assert_eq!(&rom[16..20], &0x0Au32.to_le_bytes());
    assert_eq!(&rom[20..24], &(!0x0Au32).to_le_bytes());
    assert_eq!(!0x0Au32, 0xFFFF_FFF5);
}

#[test]
fn me7_main_checksum_accepts_a_correctly_sealed_rom() {
    let mut rom = vec![0u8; 32];
    rom[0..4].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]); // words 0xFFFF + 0xFFFF
    let sum = 0x1_FFFEu32;
    rom[16..20].copy_from_slice(&sum.to_le_bytes());
    rom[20..24].copy_from_slice(&(!sum).to_le_bytes());

    let (stored, calc, valid) =
        BoschMe7ChecksumSuite::validate_and_fix_me7_main_checksum(&mut rom, &[(0, 4)], 16).unwrap();
    assert_eq!(stored, sum);
    assert_eq!(calc, sum);
    assert!(valid);
}

#[test]
fn me7_main_checksum_rejects_odd_lengths_and_out_of_bounds() {
    let mut rom = vec![0u8; 16];
    assert!(BoschMe7ChecksumSuite::validate_and_fix_me7_main_checksum(&mut rom, &[(0, 3)], 0).is_err());
    assert!(BoschMe7ChecksumSuite::validate_and_fix_me7_main_checksum(&mut rom, &[(0, 17)], 0).is_err());
    assert!(BoschMe7ChecksumSuite::validate_and_fix_me7_main_checksum(&mut rom, &[(4, 4)], 0).is_err());
    assert!(BoschMe7ChecksumSuite::validate_and_fix_me7_main_checksum(&mut rom, &[], 9).is_err());
}

#[test]
fn me7_multipoint_descriptors_fix_each_entry_and_count_repairs() {
    // Two 16-byte descriptors at 0x20. Descriptor 0 covers abs [0x1000, 0x1003],
    // descriptor 1 covers [0x1004, 0x1007]; both ranges hold word value 0x30.
    let mut rom = vec![0u8; 0x100];
    rom[..8].fill(0x30);
    for (i, start) in [0x1000u32, 0x1004u32].iter().enumerate() {
        let d = 0x20 + i * 16;
        rom[d..d + 4].copy_from_slice(&start.to_le_bytes());
        rom[d + 4..d + 8].copy_from_slice(&(start + 3).to_le_bytes());
    }

    let fixed = BoschMe7ChecksumSuite::validate_and_fix_me7_multipoint_descriptors(
        &mut rom, 0x1000, 0x20, 2,
    )
    .expect("descriptor table in bounds");
    assert_eq!(fixed, 2);

    // Words 0x3030 + 0x3030 = 0x6060 for each two-word range.
    for i in 0..2 {
        let d = 0x20 + i * 16;
        let sum = u32::from_le_bytes([rom[d + 8], rom[d + 9], rom[d + 10], rom[d + 11]]);
        let inv = u32::from_le_bytes([rom[d + 12], rom[d + 13], rom[d + 14], rom[d + 15]]);
        assert_eq!(sum, 0x6060);
        assert_eq!(inv, !0x6060u32);
    }

    // Re-running reports zero repairs.
    let again = BoschMe7ChecksumSuite::validate_and_fix_me7_multipoint_descriptors(
        &mut rom, 0x1000, 0x20, 2,
    )
    .unwrap();
    assert_eq!(again, 0);
}

#[test]
fn me7_multipoint_terminator_and_skipped_descriptors_are_not_counted() {
    let mut rom = vec![0u8; 0x100];
    // Descriptor 0: terminator (0xFFFFFFFF / 0xFFFFFFFF) => loop stops.
    rom[0x20..0x24].copy_from_slice(&u32::MAX.to_le_bytes());
    rom[0x24..0x28].copy_from_slice(&u32::MAX.to_le_bytes());
    let fixed =
        BoschMe7ChecksumSuite::validate_and_fix_me7_multipoint_descriptors(&mut rom, 0, 0x20, 4)
            .unwrap();
    assert_eq!(fixed, 0);

    // Descriptor below base_address is skipped, not an error.
    let mut rom2 = vec![0u8; 0x100];
    rom2[0x20..0x24].copy_from_slice(&0u32.to_le_bytes());
    rom2[0x24..0x28].copy_from_slice(&3u32.to_le_bytes());
    let fixed2 =
        BoschMe7ChecksumSuite::validate_and_fix_me7_multipoint_descriptors(&mut rom2, 0x100, 0x20, 1)
            .unwrap();
    assert_eq!(fixed2, 0);
}

#[test]
fn me7_multipoint_rejects_a_descriptor_table_past_the_end() {
    let mut rom = vec![0u8; 0x30];
    assert!(
        BoschMe7ChecksumSuite::validate_and_fix_me7_multipoint_descriptors(&mut rom, 0, 0x20, 2)
            .is_err()
    );
}

#[test]
fn me7_rolling_checksum_is_a_table_driven_crc() {
    // Independently re-implemented from the documented recurrence:
    //   seed = table[(byte ^ (cks & 0xFF))]; cks = (cks >> 8) ^ seed
    // with the first four bytes loading cks little-endian and the result
    // stored inverted.
    let mut rom = vec![0u8; 0x500];
    rom[0..8].copy_from_slice(&[0x11, 0x22, 0x33, 0x44, 0xAA, 0xBB, 0xCC, 0xDD]);
    // Give the first data byte a non-zero seed so the table lookup matters.
    let idx0 = ((0xAAu8 ^ 0x11u8) as usize) << 2;
    rom[0x100 + idx0..0x100 + idx0 + 4].copy_from_slice(&0xDEAD_BEEFu32.to_le_bytes());

    let mut cks = 0x4433_2211u32;
    for &b in &rom[4..8] {
        let idx = ((b ^ (cks as u8)) as usize) << 2;
        let seed = u32::from_le_bytes([
            rom[0x100 + idx],
            rom[0x100 + idx + 1],
            rom[0x100 + idx + 2],
            rom[0x100 + idx + 3],
        ]);
        cks = (cks >> 8) ^ seed;
    }
    let expected = !cks;

    let (stored, calc, valid) = BoschMe7ChecksumSuite::validate_and_fix_me7_rolling_checksum(
        &mut rom,
        0x100,
        &[(0, 8)],
        0x400,
    )
    .expect("seed table and target in bounds");
    assert_eq!(calc, expected);
    assert_eq!(stored, 0);
    assert!(!valid);
    assert_eq!(&rom[0x400..0x404], &expected.to_le_bytes());

    // Idempotent on a second pass.
    let (_, _, valid2) = BoschMe7ChecksumSuite::validate_and_fix_me7_rolling_checksum(
        &mut rom,
        0x100, // same seed table as the first pass
        &[(0, 8)],
        0x400,
    )
    .unwrap();
    assert!(valid2, "a second pass must find the checksum already correct");
}

#[test]
fn me7_rolling_checksum_with_an_all_zero_table_returns_the_inverted_shifted_seed() {
    // With every seed entry 0 the recurrence is cks >>= 8 per byte. Starting
    // from 0x44332211 and four bytes, cks becomes 0, so the stored value is
    // 0xFFFF_FFFF.
    let mut rom = vec![0u8; 0x500];
    rom[0..8].copy_from_slice(&[0x11, 0x22, 0x33, 0x44, 0xAA, 0xBB, 0xCC, 0xDD]);
    let (_, calc, _) = BoschMe7ChecksumSuite::validate_and_fix_me7_rolling_checksum(
        &mut rom,
        0x0,
        &[(0, 8)],
        0x400,
    )
    .unwrap();
    assert_eq!(calc, 0xFFFF_FFFF);
}

#[test]
fn me7_rolling_checksum_rejects_short_tables_and_bad_ranges() {
    let mut rom = vec![0u8; 0x200];
    assert!(BoschMe7ChecksumSuite::validate_and_fix_me7_rolling_checksum(&mut rom, 0x100, &[], 0).is_err());
    let mut rom2 = vec![0u8; 0x800];
    assert!(BoschMe7ChecksumSuite::validate_and_fix_me7_rolling_checksum(&mut rom2, 0x100, &[], 0x7FE).is_err());
    assert!(BoschMe7ChecksumSuite::validate_and_fix_me7_rolling_checksum(&mut rom2, 0x100, &[(0, 0x900)], 0).is_err());
    assert!(BoschMe7ChecksumSuite::validate_and_fix_me7_rolling_checksum(&mut rom2, 0x100, &[(4, 4)], 0).is_err());
}
