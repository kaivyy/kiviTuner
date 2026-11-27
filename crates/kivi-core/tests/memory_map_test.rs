use kivi_core::ingest::{ingest_binary, IngestError};
use kivi_core::memory_map::MemoryError;
use kivi_core::types::{DType, Endian};

#[test]
fn test_memory_map_read_write() {
    let dummy_rom = vec![0x00; 1024];
    let mut mem = ingest_binary(&dummy_rom).expect("Failed to ingest binary");
    assert_eq!(mem.len(), 1024);
    assert!(!mem.is_empty());
    assert_eq!(mem.hashes().md5.len(), 32);
    assert_eq!(mem.hashes().sha1.len(), 40);
    assert_eq!(mem.hashes().sha256.len(), 64);

    let initial_hash = mem.hashes().clone();

    // Write a 16-bit word at offset 0x100
    mem.write_value(0x100, 4200.0, DType::U16, Endian::Big)
        .unwrap();
    let val = mem.read_value(0x100, DType::U16, Endian::Big).unwrap();
    assert_eq!(val, 4200.0);

    // Hashes should have updated after write
    assert_ne!(mem.hashes(), &initial_hash);
}

#[test]
fn test_memory_map_out_of_bounds() {
    let dummy_rom = vec![0x00; 10];
    let mut mem = ingest_binary(&dummy_rom).expect("Failed to ingest binary");

    // Reading 4 bytes at offset 8 (8+4=12 > 10)
    let read_err = mem.read_value(8, DType::U32, Endian::Little);
    assert!(matches!(
        read_err,
        Err(MemoryError::OutOfBounds { addr: 8, size: 10 })
    ));

    // Writing 2 bytes at offset 10 (10+2=12 > 10)
    let write_err = mem.write_value(10, 1.0, DType::U16, Endian::Big);
    assert!(matches!(
        write_err,
        Err(MemoryError::OutOfBounds { addr: 10, size: 10 })
    ));
}

#[test]
fn test_memory_map_get_window() {
    let dummy_rom: Vec<u8> = (0..20).collect();
    let mem = ingest_binary(&dummy_rom).expect("Failed to ingest binary");

    // Normal window inside buffer
    let w1 = mem.get_window(5, 5);
    assert_eq!(w1, &[5, 6, 7, 8, 9]);

    // Window overlapping end of buffer
    let w2 = mem.get_window(18, 5);
    assert_eq!(w2, &[18, 19]);

    // Window starting at or past end of buffer
    let w3 = mem.get_window(20, 5);
    assert!(w3.is_empty());
    let w4 = mem.get_window(30, 5);
    assert!(w4.is_empty());
}

#[test]
fn test_ingest_validation() {
    // Empty file
    let empty: [u8; 0] = [];
    let err = ingest_binary(&empty).unwrap_err();
    assert!(matches!(err, IngestError::EmptyFile));

    // File too large (> 64 MB)
    let too_large = vec![0u8; 64 * 1024 * 1024 + 1];
    let err = ingest_binary(&too_large).unwrap_err();
    assert!(matches!(err, IngestError::FileTooLarge));
}
