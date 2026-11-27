use kivi_core::types::{DType, Endian, MemRange, TypeError};

#[test]
fn test_dtype_byte_sizes() {
    assert_eq!(DType::U8.byte_size(), 1);
    assert_eq!(DType::I8.byte_size(), 1);
    assert_eq!(DType::U16.byte_size(), 2);
    assert_eq!(DType::I16.byte_size(), 2);
    assert_eq!(DType::U32.byte_size(), 4);
    assert_eq!(DType::I32.byte_size(), 4);
    assert_eq!(DType::F32.byte_size(), 4);
}

#[test]
fn test_read_value_from_bytes() {
    let bytes = [0x12, 0x34];
    let val_be = DType::U16.read_f64(&bytes, Endian::Big).unwrap();
    let val_le = DType::U16.read_f64(&bytes, Endian::Little).unwrap();
    assert_eq!(val_be, 0x1234 as f64);
    assert_eq!(val_le, 0x3412 as f64);
}

#[test]
fn test_read_signed_and_float() {
    let bytes_i8 = [0xFE]; // -2
    assert_eq!(DType::I8.read_f64(&bytes_i8, Endian::Big).unwrap(), -2.0);

    let bytes_i16 = [0xFF, 0xFE]; // -2 in big endian
    assert_eq!(DType::I16.read_f64(&bytes_i16, Endian::Big).unwrap(), -2.0);

    let bytes_f32 = 42.5f32.to_be_bytes();
    assert_eq!(DType::F32.read_f64(&bytes_f32, Endian::Big).unwrap(), 42.5);
}

#[test]
fn test_write_value_to_bytes() {
    let mut buf = [0u8; 4];
    DType::U16
        .write_f64(0x1234 as f64, &mut buf, Endian::Big)
        .unwrap();
    assert_eq!(&buf[0..2], &[0x12, 0x34]);

    DType::U16
        .write_f64(0x1234 as f64, &mut buf, Endian::Little)
        .unwrap();
    assert_eq!(&buf[0..2], &[0x34, 0x12]);
}

#[test]
fn test_buffer_too_small() {
    let bytes = [0x12];
    let err = DType::U16.read_f64(&bytes, Endian::Big).unwrap_err();
    assert_eq!(
        err,
        TypeError::BufferTooSmall {
            expected: 2,
            got: 1
        }
    );

    let mut buf = [0u8; 1];
    let err_write = DType::U32
        .write_f64(10.0, &mut buf, Endian::Big)
        .unwrap_err();
    assert_eq!(
        err_write,
        TypeError::BufferTooSmall {
            expected: 4,
            got: 1
        }
    );
}

#[test]
fn test_mem_range() {
    let range = MemRange::new(0x8000, 0x1000);
    assert_eq!(range.start, 0x8000);
    assert_eq!(range.length, 0x1000);
    assert_eq!(range.end(), 0x9000);
    assert!(range.contains(0x8000));
    assert!(range.contains(0x8FFF));
    assert!(!range.contains(0x7FFF));
    assert!(!range.contains(0x9000));
}

#[test]
fn test_write_f64_rounds_without_lsb_drift_and_rejects_non_finite() {
    // IEEE-754 inverse-scaling example: 0.57 * 100.0 == 56.99999999999999 (< 57.0)
    let approx = 0.57f64 * 100.0f64;
    assert!(approx < 57.0);

    let mut b1 = [0u8; 1];
    DType::U8.write_f64(approx, &mut b1, Endian::Big).unwrap();
    assert_eq!(b1[0], 57);

    let mut b2 = [0u8; 2];
    DType::U16.write_f64(approx, &mut b2, Endian::Big).unwrap();
    assert_eq!(u16::from_be_bytes(b2), 57);

    DType::I16.write_f64(-approx, &mut b2, Endian::Little).unwrap();
    assert_eq!(i16::from_le_bytes(b2), -57);

    let mut b4 = [0u8; 4];
    DType::U32.write_f64(approx, &mut b4, Endian::Little).unwrap();
    assert_eq!(u32::from_le_bytes(b4), 57);

    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let err = DType::U16.write_f64(bad, &mut b2, Endian::Big).unwrap_err();
        assert!(matches!(err, TypeError::NonFiniteValue(_)));
    }
}
