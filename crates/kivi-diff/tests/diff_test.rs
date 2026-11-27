use std::time::Instant;

use kivi_core::types::{DType, Endian};
use kivi_diff::{compare_binaries, diff_region_to_map_def, DiffKind, DiffRegion};

#[test]
fn test_identical_binaries_return_empty() {
    let base = vec![0xEA; 1024];
    let target = vec![0xEA; 1024];

    let diffs = compare_binaries(&base, &target, 4);
    assert!(diffs.is_empty());
}

#[test]
fn test_single_byte_change_scalar() {
    let base = vec![0x00; 512];
    let mut target = base.clone();
    target[42] = 0x7F;

    let diffs = compare_binaries(&base, &target, 4);
    assert_eq!(diffs.len(), 1);

    let region = &diffs[0];
    assert_eq!(region.start_addr, 42);
    assert_eq!(region.end_addr, 43);
    assert_eq!(region.byte_len, 1);
    assert_eq!(region.changed_byte_count, 1);
    assert_eq!(region.max_abs_delta, 127.0);
    assert_eq!(region.mean_delta, 127.0);
    assert_eq!(region.classification, DiffKind::Scalar);
}

#[test]
fn test_cluster_within_max_gap_grouped() {
    let base = vec![0x00; 100];
    let mut target = base.clone();

    // Changes at 10, 12, 16 with max_gap = 4
    // 12 - 10 - 1 = 1 <= 4
    // 16 - 12 - 1 = 3 <= 4
    // All should be merged into one region from 10 to 17 (byte_len = 7)
    target[10] = 0x10;
    target[12] = 0x20;
    target[16] = 0x30;

    let diffs = compare_binaries(&base, &target, 4);
    assert_eq!(diffs.len(), 1);

    let region = &diffs[0];
    assert_eq!(region.start_addr, 10);
    assert_eq!(region.end_addr, 17);
    assert_eq!(region.byte_len, 7);
    assert_eq!(region.changed_byte_count, 3);
    assert_eq!(region.max_abs_delta, 0x30 as f64);
    // (0x10 + 0x20 + 0x30) / 3 = 0x60 / 3 = 0x20 = 32.0
    assert_eq!(region.mean_delta, 32.0);
}

#[test]
fn test_changes_separated_by_more_than_max_gap_separate_regions() {
    let base = vec![0x00; 100];
    let mut target = base.clone();

    // Changed at 10 and 16 with max_gap = 4
    // 16 - 10 - 1 = 5 > 4 (separated by 5 unchanged bytes: 11, 12, 13, 14, 15)
    target[10] = 0x11;
    target[16] = 0x22;

    let diffs = compare_binaries(&base, &target, 4);
    assert_eq!(diffs.len(), 2);

    assert_eq!(diffs[0].start_addr, 10);
    assert_eq!(diffs[0].end_addr, 11);
    assert_eq!(diffs[0].byte_len, 1);
    assert_eq!(diffs[0].classification, DiffKind::Scalar);

    assert_eq!(diffs[1].start_addr, 16);
    assert_eq!(diffs[1].end_addr, 17);
    assert_eq!(diffs[1].byte_len, 1);
    assert_eq!(diffs[1].classification, DiffKind::Scalar);
}

#[test]
fn test_256_byte_table_diff_classified_as_table_2d() {
    let base = vec![0x00; 1024];
    let mut target = base.clone();

    let start = 0x100;
    for i in 0..256 {
        target[start + i] = (i % 250 + 1) as u8;
    }

    let diffs = compare_binaries(&base, &target, 4);
    assert_eq!(diffs.len(), 1);

    let region = &diffs[0];
    assert_eq!(region.start_addr, start);
    assert_eq!(region.end_addr, start + 256);
    assert_eq!(region.byte_len, 256);
    assert_eq!(region.changed_byte_count, 256);
    assert_eq!(region.inferred_dtype, DType::U8);
    assert_eq!(
        region.classification,
        DiffKind::Table2D {
            suggested_rows: 16,
            suggested_cols: 16,
        }
    );
}

#[test]
fn test_axis_1d_diff() {
    let base = vec![0x00; 512];
    let mut target = base.clone();

    // 16-point axis
    let start = 0x40;
    for i in 0..16 {
        target[start + i] = (i + 1) as u8;
    }

    let diffs = compare_binaries(&base, &target, 4);
    assert_eq!(diffs.len(), 1);

    let region = &diffs[0];
    assert_eq!(region.start_addr, start);
    assert_eq!(region.byte_len, 16);
    assert_eq!(region.classification, DiffKind::Axis1D { length: 16 });
}

#[test]
fn test_conversion_to_map_def() {
    let region = DiffRegion {
        id: "diff_0x100".to_string(),
        start_addr: 0x100,
        end_addr: 0x200,
        byte_len: 256,
        changed_byte_count: 256,
        max_abs_delta: 25.0,
        mean_delta: 12.5,
        inferred_dtype: DType::U8,
        inferred_endian: Endian::Big,
        classification: DiffKind::Table2D {
            suggested_rows: 16,
            suggested_cols: 16,
        },
    };

    let map = diff_region_to_map_def(&region);
    assert_eq!(map.id, "map_diff_0x100");
    assert_eq!(map.address, 0x100);
    assert_eq!(map.rows, 16);
    assert_eq!(map.cols, 16);
    assert_eq!(map.dtype, DType::U8);
    assert_eq!(map.endian, Endian::Big);
    assert_eq!(map.scaling.equation, "X");

    let x_axis = map.x_axis.expect("X axis should be present");
    assert_eq!(x_axis.length, 16);
    assert_eq!(x_axis.dtype, DType::U8);

    let y_axis = map.y_axis.expect("Y axis should be present");
    assert_eq!(y_axis.length, 16);
    assert_eq!(y_axis.dtype, DType::U8);
}

#[test]
fn test_performance_1mb_rom() {
    let size = 1024 * 1024; // 1MB slice
    let base = vec![0xAA; size];
    let mut target = base.clone();

    // Introduce several realistic ECU table modifications across the 1MB space
    target[0x1000] = 0x55;

    for i in 0..256 {
        target[0x20000 + i] = (i & 0xFF) as u8;
    }

    for i in 0..16 {
        target[0x40000 + i] = 0x10 + (i as u8);
    }

    for i in 0..144 {
        target[0x80000 + i] = 0x30 + (i as u8);
    }

    let start = Instant::now();
    let diffs = compare_binaries(&base, &target, 4);
    let elapsed = start.elapsed();

    println!("1MB ROM diff execution time: {:?}", elapsed);
    assert!(
        elapsed.as_millis() < 5,
        "Comparison must complete in < 5ms, took {:?}",
        elapsed
    );
    assert_eq!(diffs.len(), 4);
}

#[test]
fn test_performance_4mb_rom() {
    let size = 4 * 1024 * 1024; // 4MB slice
    let base = vec![0xAA; size];
    let mut target = base.clone();

    target[0x1000] = 0x55;

    for i in 0..256 {
        target[0x20000 + i] = (i & 0xFF) as u8;
    }

    for i in 0..16 {
        target[0x80000 + i] = 0x10 + (i as u8);
    }

    for i in 0..144 {
        target[0x200000 + i] = 0x30 + (i as u8);
    }

    let start = Instant::now();
    let diffs = compare_binaries(&base, &target, 4);
    let elapsed = start.elapsed();

    println!("4MB ROM diff execution time: {:?}", elapsed);
    // In release mode this executes in < 1ms; in unoptimized debug mode ensure < 15ms
    #[cfg(not(debug_assertions))]
    assert!(
        elapsed.as_millis() < 5,
        "Comparison must complete in < 5ms, took {:?}",
        elapsed
    );
    #[cfg(debug_assertions)]
    assert!(
        elapsed.as_millis() < 15,
        "Comparison must complete in < 15ms in debug mode, took {:?}",
        elapsed
    );
    assert_eq!(diffs.len(), 4);
}

#[test]
fn test_serde_roundtrip() {
    let region = DiffRegion {
        id: "diff_0x100".to_string(),
        start_addr: 0x100,
        end_addr: 0x200,
        byte_len: 256,
        changed_byte_count: 256,
        max_abs_delta: 25.0,
        mean_delta: 12.5,
        inferred_dtype: DType::U8,
        inferred_endian: Endian::Big,
        classification: DiffKind::Table2D {
            suggested_rows: 16,
            suggested_cols: 16,
        },
    };

    let serialized = serde_json::to_string(&region).unwrap();
    assert!(serialized.contains(r#""type":"Table2D""#));
    assert!(serialized.contains(r#""data":{"suggested_rows":16,"suggested_cols":16}"#));

    let deserialized: DiffRegion = serde_json::from_str(&serialized).unwrap();
    assert_eq!(region, deserialized);
}

#[test]
fn test_16bit_table_inference() {
    let base = vec![0x00; 128];
    let mut target = base.clone();

    // 64-byte region (32 words). Each word changed by 300 in big-endian (0x012C)
    // base: 0x00, 0x00 -> target: 0x01, 0x2C
    // High byte changes by 1, low byte changes by 44 (both bytes changed, starting at byte 0)
    for w in 0..32 {
        target[2 * w] = 1;
        target[2 * w + 1] = 44;
    }

    let diffs = compare_binaries(&base, &target, 4);
    assert_eq!(diffs.len(), 1);

    let region = &diffs[0];
    assert_eq!(region.byte_len, 64);
    assert_eq!(region.inferred_dtype, DType::U16);
    assert_eq!(region.inferred_endian, Endian::Big);
    // 32 words / 8 cols = 4 rows
    assert_eq!(
        region.classification,
        DiffKind::Table2D {
            suggested_rows: 4,
            suggested_cols: 8,
        }
    );
}
