//! Parity tests for the ported arithmetic and page checksums.
//!
//! Ported from `ecu_engine_rust::math::checksum`
//! (`ChecksumEngine::calculate_arithmetic_checksum`, `ChecksumEngine::inject_checksum`,
//! `BoschMe7EepromChecksum`).
//!
//! ## How expected values were obtained
//!
//! Every expected value below is **hand-computed from the source algorithm**
//! (summation with a mask, two's complement, one's complement) or taken from the
//! source module's own assertions. None was produced by running the new
//! implementation.
//!
//! ## Documented deviation
//!
//! `inject_checksum` mutates the caller's buffer in the source. Here it is
//! exposed as `inject_checksum_into`, which returns a new buffer, matching this
//! crate's read-only convention. Byte layout is identical.

use kivi_checksum::arithmetic::{
    calculate_arithmetic_checksum, inject_checksum_into, ArithmeticChecksumError,
    ChecksumAlgorithm, ChecksumBits,
};
use kivi_checksum::eeprom_me7::{
    calculate_page_checksum, fix_all_page_checksums, is_page_exempt, EepromChecksumError,
    PAGE_COUNT, PAGE_DESCRIPTORS, PAGE_SIZE,
};

// ---------------------------------------------------------------------------
// calculate_arithmetic_checksum
// ---------------------------------------------------------------------------

#[test]
fn sum_8bit_single_bytes() {
    // 0x10 + 0x20 + 0x30 = 0x60
    let rom = [0x10, 0x20, 0x30];
    let r = calculate_arithmetic_checksum(
        &rom,
        ChecksumAlgorithm::Sum,
        0,
        2,
        ChecksumBits::Bits8,
        ChecksumBits::Bits8,
        true,
    )
    .expect("checksum");
    assert_eq!(r, 0x60);
}

#[test]
fn sum_wraps_modulo_the_checksum_width() {
    // 0xFF + 0x02 = 0x101 -> masked to 8 bits = 0x01
    let rom = [0xFF, 0x02];
    let r = calculate_arithmetic_checksum(
        &rom,
        ChecksumAlgorithm::Sum,
        0,
        1,
        ChecksumBits::Bits8,
        ChecksumBits::Bits8,
        true,
    )
    .expect("checksum");
    assert_eq!(r, 0x01);
}

#[test]
fn twos_complement_is_negation_of_the_masked_sum() {
    // sum = 0x10; two's complement = (0x100 - 0x10) & 0xFF = 0xF0
    let rom = [0x10];
    let r = calculate_arithmetic_checksum(
        &rom,
        ChecksumAlgorithm::TwosComplement,
        0,
        0,
        ChecksumBits::Bits8,
        ChecksumBits::Bits8,
        true,
    )
    .expect("checksum");
    assert_eq!(r, 0xF0);
}

#[test]
fn ones_complement_is_bitwise_not_of_the_masked_sum() {
    // sum = 0x10; ~0x10 & 0xFF = 0xEF
    let rom = [0x10];
    let r = calculate_arithmetic_checksum(
        &rom,
        ChecksumAlgorithm::OnesComplement,
        0,
        0,
        ChecksumBits::Bits8,
        ChecksumBits::Bits8,
        true,
    )
    .expect("checksum");
    assert_eq!(r, 0xEF);
}

#[test]
fn sum_16bit_big_endian_reads_high_byte_first() {
    // bytes [0x01, 0x02] as BE u16 = 0x0102
    let rom = [0x01, 0x02];
    let r = calculate_arithmetic_checksum(
        &rom,
        ChecksumAlgorithm::Sum,
        0,
        1,
        ChecksumBits::Bits16,
        ChecksumBits::Bits16,
        false,
    )
    .expect("checksum");
    assert_eq!(r, 0x0102);
}

#[test]
fn sum_16bit_little_endian_reads_low_byte_first() {
    // bytes [0x01, 0x02] as LE u16 = 0x0201
    let rom = [0x01, 0x02];
    let r = calculate_arithmetic_checksum(
        &rom,
        ChecksumAlgorithm::Sum,
        0,
        1,
        ChecksumBits::Bits16,
        ChecksumBits::Bits16,
        true,
    )
    .expect("checksum");
    assert_eq!(r, 0x0201);
}

#[test]
fn sum_16bit_adds_multiple_elements() {
    // [0x01,0x02] BE = 0x0102, [0x03,0x04] BE = 0x0304; total 0x0406
    let rom = [0x01, 0x02, 0x03, 0x04];
    let r = calculate_arithmetic_checksum(
        &rom,
        ChecksumAlgorithm::Sum,
        0,
        3,
        ChecksumBits::Bits16,
        ChecksumBits::Bits16,
        false,
    )
    .expect("checksum");
    assert_eq!(r, 0x0406);
}

#[test]
fn sum_32bit_big_endian() {
    let rom = [0x00, 0x00, 0x01, 0x00];
    let r = calculate_arithmetic_checksum(
        &rom,
        ChecksumAlgorithm::Sum,
        0,
        3,
        ChecksumBits::Bits32,
        ChecksumBits::Bits32,
        false,
    )
    .expect("checksum");
    assert_eq!(r, 0x0000_0100);
}

#[test]
fn rejects_an_out_of_bounds_end_address() {
    let rom = [1, 2, 3];
    let err = calculate_arithmetic_checksum(
        &rom,
        ChecksumAlgorithm::Sum,
        0,
        3,
        ChecksumBits::Bits8,
        ChecksumBits::Bits8,
        true,
    )
    .expect_err("must reject");
    assert_eq!(err, ArithmeticChecksumError::OutOfBounds);
}

#[test]
fn rejects_start_greater_than_end() {
    let rom = [1, 2, 3];
    let err = calculate_arithmetic_checksum(
        &rom,
        ChecksumAlgorithm::Sum,
        2,
        1,
        ChecksumBits::Bits8,
        ChecksumBits::Bits8,
        true,
    )
    .expect_err("must reject");
    assert_eq!(err, ArithmeticChecksumError::OutOfBounds);
}

#[test]
fn rejects_a_crc_algorithm_in_the_arithmetic_engine() {
    let rom = [1, 2, 3];
    let err = calculate_arithmetic_checksum(
        &rom,
        ChecksumAlgorithm::Crc32Ieee,
        0,
        2,
        ChecksumBits::Bits8,
        ChecksumBits::Bits8,
        true,
    )
    .expect_err("must reject");
    assert_eq!(err, ArithmeticChecksumError::UnsupportedAlgorithm);
}

#[test]
fn respects_a_nonzero_start_address() {
    // Only the trailing byte is summed.
    let rom = [0xFF, 0xFF, 0x05];
    let r = calculate_arithmetic_checksum(
        &rom,
        ChecksumAlgorithm::Sum,
        2,
        2,
        ChecksumBits::Bits8,
        ChecksumBits::Bits8,
        true,
    )
    .expect("checksum");
    assert_eq!(r, 0x05);
}

// ---------------------------------------------------------------------------
// inject_checksum_into
// ---------------------------------------------------------------------------

#[test]
fn injects_8bit_at_the_store_address() {
    let rom = [0, 0, 0, 0];
    let out = inject_checksum_into(&rom, 1, 0xAB, ChecksumBits::Bits8, true).expect("inject");
    assert_eq!(out, vec![0x00, 0xAB, 0x00, 0x00]);
}

#[test]
fn injects_16bit_little_endian() {
    let rom = [0, 0, 0, 0];
    let out = inject_checksum_into(&rom, 0, 0x1234, ChecksumBits::Bits16, true).expect("inject");
    assert_eq!(out, vec![0x34, 0x12, 0x00, 0x00]);
}

#[test]
fn injects_16bit_big_endian() {
    let rom = [0, 0, 0, 0];
    let out = inject_checksum_into(&rom, 0, 0x1234, ChecksumBits::Bits16, false).expect("inject");
    assert_eq!(out, vec![0x12, 0x34, 0x00, 0x00]);
}

#[test]
fn injects_32bit_big_endian() {
    let rom = [0, 0, 0, 0];
    let out = inject_checksum_into(&rom, 0, 0x1234_5678, ChecksumBits::Bits32, false).expect("inject");
    assert_eq!(out, vec![0x12, 0x34, 0x56, 0x78]);
}

#[test]
fn inject_leaves_the_input_buffer_untouched() {
    let rom = vec![1, 2, 3, 4];
    let original = rom.clone();
    let _ = inject_checksum_into(&rom, 0, 0xFF, ChecksumBits::Bits8, true).expect("inject");
    assert_eq!(rom, original);
}

#[test]
fn inject_rejects_a_store_address_that_overflows() {
    let rom = [0, 0, 0];
    let err = inject_checksum_into(&rom, 2, 0x1234, ChecksumBits::Bits16, true)
        .expect_err("must reject");
    assert_eq!(err, ArithmeticChecksumError::StoreOverflow);
}

// ---------------------------------------------------------------------------
// Bosch ME7 EEPROM page checksums
// ---------------------------------------------------------------------------

#[test]
fn page_28_and_29_are_exempt() {
    assert!(is_page_exempt(28));
    assert!(is_page_exempt(29));
    assert!(!is_page_exempt(0));
    assert!(!is_page_exempt(27));
    assert!(!is_page_exempt(30));
}

#[test]
fn descriptor_table_shape_matches_the_source() {
    assert_eq!(PAGE_COUNT, 32);
    assert_eq!(PAGE_SIZE, 16);
    assert_eq!(PAGE_DESCRIPTORS.len(), 32);
    // Spot-check the first and last entries against the source table.
    assert_eq!(PAGE_DESCRIPTORS[0], 0xFF18);
    assert_eq!(PAGE_DESCRIPTORS[31], 0x13F7);
}

#[test]
fn page_checksum_is_twos_complement_of_sum_plus_page_index() {
    // Page 1: bytes 16..=29 all zero -> sum 0; + page index 1 = 1.
    // desc 0x0017: bit0 set (has checksum), bit6 (0x40) NOT set -> no adjustment.
    // two's complement of 1 = 0xFFFF.
    // (Page 0 has descriptor 0xFF18 with bit0 clear, so it carries no checksum.)
    let data = vec![0u8; 512];
    let r = calculate_page_checksum(&data, 1).expect("page");
    assert_eq!(r, 0xFFFF);
}

#[test]
fn page_checksum_rejects_a_page_out_of_range() {
    let data = vec![0u8; 512];
    let err = calculate_page_checksum(&data, 32).expect_err("must reject");
    assert_eq!(err, EepromChecksumError::PageOutOfBounds);
}

#[test]
fn page_checksum_rejects_a_short_buffer() {
    let data = vec![0u8; 16];
    let err = calculate_page_checksum(&data, 1).expect_err("must reject");
    assert_eq!(err, EepromChecksumError::BufferTooSmall);
}

#[test]
fn page_checksum_rejects_a_page_without_a_descriptor() {
    // Verified against the source table: pages 0, 17, 18, 19, 20 have bit0
    // clear and therefore carry no page checksum.
    let data = vec![0u8; 512];
    for p in [0usize, 17, 18, 19, 20] {
        assert_eq!((PAGE_DESCRIPTORS[p] & 1), 0, "page {} must lack bit0", p);
        let err = calculate_page_checksum(&data, p).expect_err("must reject");
        assert_eq!(err, EepromChecksumError::NoDescriptor);
    }
}

#[test]
fn fix_all_page_checksums_writes_only_changed_pages() {
    let mut eeprom = vec![0u8; 512];
    // Page 1 currently stores 0x0000 at offset 30..32; expected is 0xFFFF.
    let fixed = fix_all_page_checksums(&mut eeprom).expect("fix");
    assert!(fixed > 0, "at least one page must be corrected");
    assert_eq!(
        u16::from_le_bytes([eeprom[16 + 14], eeprom[16 + 15]]),
        0xFFFF,
        "page 1 checksum must be written little-endian"
    );
    // Page 0 has no descriptor, so its slot must be left alone.
    assert_eq!(
        u16::from_le_bytes([eeprom[14], eeprom[15]]),
        0x0000,
        "page 0 carries no checksum and must not be written"
    );
}

#[test]
fn fix_all_page_checksums_is_idempotent() {
    let mut eeprom = vec![0u8; 512];
    fix_all_page_checksums(&mut eeprom).expect("first pass");
    let second = fix_all_page_checksums(&mut eeprom).expect("second pass");
    assert_eq!(second, 0, "a corrected EEPROM needs no further fixes");
}

#[test]
fn fix_all_page_checksums_leaves_exempt_pages_alone() {
    let mut eeprom = vec![0u8; 512];
    // Put sentinel bytes in the exempt pages' checksum slots.
    eeprom[28 * PAGE_SIZE + 14] = 0xDE;
    eeprom[28 * PAGE_SIZE + 15] = 0xAD;
    eeprom[29 * PAGE_SIZE + 14] = 0xBE;
    eeprom[29 * PAGE_SIZE + 15] = 0xEF;

    fix_all_page_checksums(&mut eeprom).expect("fix");

    assert_eq!(eeprom[28 * PAGE_SIZE + 14], 0xDE, "page 28 is exempt");
    assert_eq!(eeprom[29 * PAGE_SIZE + 14], 0xBE, "page 29 is exempt");
}

#[test]
fn fix_all_page_checksums_rejects_a_short_buffer() {
    let mut short = vec![0u8; 511];
    let err = fix_all_page_checksums(&mut short).expect_err("must reject");
    assert_eq!(err, EepromChecksumError::EepromTooSmall);
}
