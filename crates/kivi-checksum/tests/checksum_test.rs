use kivi_checksum::plugins::honda_keihin::{
    fix_keihin_checksum, verify_keihin_checksum, KeihinChecksumError,
};
use kivi_checksum::primitives::{
    compute_crc32, sum_modulo_256, sum_modulo_65536, twos_complement_8,
};
use kivi_core::types::Endian;

#[test]
fn test_keihin_two_complement_sum() {
    // 32 KB dummy ECU buffer
    let mut buffer = vec![0x10; 32768];
    // Before fixing, it shouldn't be guaranteed valid
    fix_keihin_checksum(&mut buffer).unwrap();
    assert!(verify_keihin_checksum(&buffer));

    // Modify one byte
    buffer[100] ^= 0xFF;
    assert!(!verify_keihin_checksum(&buffer));

    // Fix again
    fix_keihin_checksum(&mut buffer).unwrap();
    assert!(verify_keihin_checksum(&buffer));
}

#[test]
fn test_keihin_buffer_too_small() {
    let mut small_buffer = vec![0x00; 512];
    let err = fix_keihin_checksum(&mut small_buffer).unwrap_err();
    assert_eq!(err, KeihinChecksumError::BufferTooSmall);

    assert!(!verify_keihin_checksum(&[]));
}

#[test]
fn test_primitives_sum_modulo_256() {
    let data = [0x01, 0x02, 0x03, 0x04];
    assert_eq!(sum_modulo_256(&data), 10);

    let overflow_data = [0xFF, 0x02];
    assert_eq!(sum_modulo_256(&overflow_data), 0x01);
}

#[test]
fn test_primitives_twos_complement_8() {
    assert_eq!(twos_complement_8(0x00), 0x00);
    assert_eq!(twos_complement_8(0x01), 0xFF);
    assert_eq!(twos_complement_8(0x10), 0xF0);
    assert_eq!(twos_complement_8(0xFF), 0x01);
}

#[test]
fn test_primitives_sum_modulo_65536() {
    // Big Endian: 0x0102 + 0x0304 = 0x0406
    let data = [0x01, 0x02, 0x03, 0x04];
    assert_eq!(sum_modulo_65536(&data, Endian::Big), 0x0406);

    // Little Endian: 0x0201 + 0x0403 = 0x0604
    assert_eq!(sum_modulo_65536(&data, Endian::Little), 0x0604);

    // Odd byte count padding test (trailing 0x05 treated as 0x0500 in BE, 0x0005 in LE)
    let odd_data = [0x01, 0x02, 0x05];
    assert_eq!(sum_modulo_65536(&odd_data, Endian::Big), 0x0102 + 0x0500);
    assert_eq!(sum_modulo_65536(&odd_data, Endian::Little), 0x0201 + 0x0005);
}

#[test]
fn test_primitives_crc32() {
    // Standard CRC32 vector for "123456789" is 0xCBF43926
    let vector = b"123456789";
    assert_eq!(compute_crc32(vector), 0xCBF43926);
}
