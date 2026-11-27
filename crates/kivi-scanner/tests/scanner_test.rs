use kivi_core::types::{DType, Endian};
use kivi_scanner::{
    detect_axes, detect_periodic_tables, scan_rom, scan_rom_with_options, AxisCandidate,
    DetectedMapCandidate, ScannerSensitivity,
};

#[test]
fn test_detect_axis_rpm_synthetic_16bit_be() {
    let mut rom = vec![0u8; 1024];
    let start = 0x100;

    // RPM 1000..8500 step 500 (16 points)
    for (i, val) in (1000u16..=8500).step_by(500).enumerate() {
        let addr = start + i * 2;
        let bytes = val.to_be_bytes();
        rom[addr] = bytes[0];
        rom[addr + 1] = bytes[1];
    }

    let axes = detect_axes(&rom);
    assert_eq!(axes.len(), 1);

    let rpm = &axes[0];
    assert_eq!(rpm.start_addr, start);
    assert_eq!(rpm.length, 16);
    assert_eq!(rpm.dtype, DType::U16);
    assert_eq!(rpm.endian, Endian::Big);
    assert_eq!(rpm.min_val, 1000.0);
    assert_eq!(rpm.max_val, 8500.0);
    assert!(rpm.step_variance < 1e-4);
}

#[test]
fn test_detect_axis_tps_synthetic_8bit() {
    let mut rom = vec![0u8; 512];
    let start = 0x80;
    let tps_points: [u8; 16] = [
        0, 5, 10, 15, 20, 25, 30, 40, 50, 60, 70, 80, 85, 90, 95, 100,
    ];

    for (i, &val) in tps_points.iter().enumerate() {
        rom[start + i] = val;
    }

    let axes = detect_axes(&rom);
    assert_eq!(axes.len(), 1);

    let tps = &axes[0];
    assert_eq!(tps.start_addr, start);
    assert_eq!(tps.length, 16);
    assert_eq!(tps.dtype, DType::U8);
    assert_eq!(tps.min_val, 0.0);
    assert_eq!(tps.max_val, 100.0);
}

#[test]
fn test_detect_2d_table_synthetic_fuel_bell_curve() {
    let rows = 16;
    let cols = 16;
    let mut rom = vec![0u8; 4096];
    let table_start = 0x200;

    for r in 0..rows {
        for c in 0..cols {
            let addr = table_start + (r * cols + c) * 2;
            let val = (1500.0
                + 4500.0 * (-(((r as f64 - 8.0).powi(2) + (c as f64 - 8.0).powi(2)) / 32.0)).exp())
                as u16;
            let bytes = val.to_be_bytes();
            rom[addr] = bytes[0];
            rom[addr + 1] = bytes[1];
        }
    }

    let tables = detect_periodic_tables(&rom, &[16], 0.80, 6);
    assert_eq!(tables.len(), 1);

    let table = &tables[0];
    assert_eq!(table.start_addr, table_start);
    assert_eq!(table.rows, 16);
    assert_eq!(table.cols, 16);
    assert_eq!(table.dtype, DType::U16);
    assert_eq!(table.endian, Endian::Big);
    assert!(table.avg_similarity > 0.95);
}

#[test]
fn test_end_to_end_scan_rom_with_axis_pairing() {
    let mut rom = vec![0u8; 8192];

    // RPM axis (16 values U16 BE)
    let rpm_start = 0x1000;
    for (i, val) in (1000u16..=8500).step_by(500).enumerate() {
        let bytes = val.to_be_bytes();
        rom[rpm_start + i * 2] = bytes[0];
        rom[rpm_start + i * 2 + 1] = bytes[1];
    }

    // TPS axis (16 values U16 BE)
    let tps_start = 0x1020;
    let tps_vals: [u16; 16] = [
        0, 5, 10, 15, 20, 25, 30, 40, 50, 60, 70, 80, 85, 90, 95, 100,
    ];
    for (i, &val) in tps_vals.iter().enumerate() {
        let bytes = val.to_be_bytes();
        rom[tps_start + i * 2] = bytes[0];
        rom[tps_start + i * 2 + 1] = bytes[1];
    }

    // 16x16 Fuel VE table
    let table_start = 0x1040;
    for r in 0..16 {
        for c in 0..16 {
            let addr = table_start + (r * 16 + c) * 2;
            let val = (2000.0
                + 6000.0 * (-(((r as f64 - 8.0).powi(2) + (c as f64 - 8.0).powi(2)) / 32.0)).exp())
                as u16;
            let bytes = val.to_be_bytes();
            rom[addr] = bytes[0];
            rom[addr + 1] = bytes[1];
        }
    }

    let candidates = scan_rom(&rom, ScannerSensitivity::Strict);
    assert_eq!(candidates.len(), 1);

    let map = &candidates[0];
    assert_eq!(map.start_addr, table_start);
    assert_eq!(map.rows, 16);
    assert_eq!(map.cols, 16);
    assert_eq!(map.x_axis_addr, Some(rpm_start));
    assert_eq!(map.y_axis_addr, Some(tps_start));
    assert_eq!(map.category_hint, "Fuel");
    assert!(map.confidence >= 85);
}

#[test]
fn test_sensitivity_filtering() {
    let mut rom = vec![0u8; 8192];

    // High quality: table with 2 paired axes (Confidence ~90)
    let rpm_start = 0x1000;
    for (i, val) in (1000u16..=8500).step_by(500).enumerate() {
        let bytes = val.to_be_bytes();
        rom[rpm_start + i * 2] = bytes[0];
        rom[rpm_start + i * 2 + 1] = bytes[1];
    }

    let tps_start = 0x1020;
    let tps_vals: [u16; 16] = [
        0, 5, 10, 15, 20, 25, 30, 40, 50, 60, 70, 80, 85, 90, 95, 100,
    ];
    for (i, &val) in tps_vals.iter().enumerate() {
        let bytes = val.to_be_bytes();
        rom[tps_start + i * 2] = bytes[0];
        rom[tps_start + i * 2 + 1] = bytes[1];
    }

    let table1_start = 0x1040;
    for r in 0..16 {
        for c in 0..16 {
            let addr = table1_start + (r * 16 + c) * 2;
            let val = (2000.0
                + 6000.0 * (-(((r as f64 - 8.0).powi(2) + (c as f64 - 8.0).powi(2)) / 32.0)).exp())
                as u16;
            let bytes = val.to_be_bytes();
            rom[addr] = bytes[0];
            rom[addr + 1] = bytes[1];
        }
    }

    // Medium quality: table with only 1 paired axis (at 0x1500)
    // Put an isolated axis far away from table 1, close to table 2
    let axis3_start = 0x1500;
    for (i, val) in (2000u16..=5000).step_by(200).enumerate() {
        let bytes = val.to_be_bytes();
        rom[axis3_start + i * 2] = bytes[0];
        rom[axis3_start + i * 2 + 1] = bytes[1];
    }

    let table2_start = 0x1540;
    for r in 0..16 {
        for c in 0..16 {
            let addr = table2_start + (r * 16 + c) * 2;
            let val = (1500.0
                + 3000.0 * (-(((r as f64 - 8.0).powi(2) + (c as f64 - 8.0).powi(2)) / 32.0)).exp())
                as u16;
            let bytes = val.to_be_bytes();
            rom[addr] = bytes[0];
            rom[addr + 1] = bytes[1];
        }
    }

    let strict = scan_rom(&rom, ScannerSensitivity::Strict);
    let balanced = scan_rom(&rom, ScannerSensitivity::Balanced);
    let aggressive = scan_rom(&rom, ScannerSensitivity::Aggressive);

    // Strict requires >= 85, so only table1 (paired with 2 axes) passes
    assert_eq!(strict.len(), 1);
    assert_eq!(strict[0].start_addr, table1_start);

    // Balanced requires >= 70, so table1 (both axes) and table2 (1 axis) pass
    assert!(balanced.len() >= 2);

    // Aggressive requires >= 50, includes all candidates
    assert!(aggressive.len() >= balanced.len());
}

#[test]
fn test_robustness_on_random_noise() {
    let mut rom = vec![0u8; 65536];

    // Populate with pseudo-random bytes using linear congruential generator
    let mut seed: u64 = 0xDEAD_BEEF;
    for byte in &mut rom {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        *byte = (seed >> 32) as u8;
    }

    let balanced = scan_rom(&rom, ScannerSensitivity::Balanced);
    assert!(
        balanced.is_empty(),
        "Random noise must not produce balanced confidence candidates"
    );

    let strict = scan_rom(&rom, ScannerSensitivity::Strict);
    assert!(
        strict.is_empty(),
        "Random noise must not produce strict confidence candidates"
    );
}

#[test]
fn test_detect_ignition_table_category() {
    let mut rom = vec![0u8; 4096];

    // RPM axis (16 values U16 BE)
    let rpm_start = 0x200;
    for (i, val) in (1000u16..=8500).step_by(500).enumerate() {
        let bytes = val.to_be_bytes();
        rom[rpm_start + i * 2] = bytes[0];
        rom[rpm_start + i * 2 + 1] = bytes[1];
    }

    // MAP load axis (16 values U16 BE: 20..100 kPa)
    let map_start = 0x220;
    for (i, val) in (20u16..=95).step_by(5).enumerate() {
        let bytes = val.to_be_bytes();
        rom[map_start + i * 2] = bytes[0];
        rom[map_start + i * 2 + 1] = bytes[1];
    }

    // 16x16 Ignition timing table: degrees advance 10..42
    let table_start = 0x240;
    for r in 0..16 {
        for c in 0..16 {
            let addr = table_start + (r * 16 + c) * 2;
            let val = (12.0 + (r as f64 * 1.5) + (c as f64 * 0.5)) as u16;
            let bytes = val.to_be_bytes();
            rom[addr] = bytes[0];
            rom[addr + 1] = bytes[1];
        }
    }

    let candidates = scan_rom(&rom, ScannerSensitivity::Balanced);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].category_hint, "Ignition");
    assert_eq!(candidates[0].start_addr, table_start);
}

#[test]
fn test_scan_rom_with_options_and_serde() {
    let mut rom = vec![0u8; 2048];
    let start = 0x100;
    for (i, val) in (1000u16..=8500).step_by(500).enumerate() {
        let bytes = val.to_be_bytes();
        rom[start + i * 2] = bytes[0];
        rom[start + i * 2 + 1] = bytes[1];
    }

    let candidates = scan_rom_with_options(&rom, 70, Some(&[16]));
    assert!(candidates.is_empty()); // No table, only an axis

    let cand = DetectedMapCandidate {
        id: "map_0x100".to_string(),
        name_hint: "Test Map".to_string(),
        category_hint: "Fuel".to_string(),
        start_addr: 0x100,
        end_addr: 0x300,
        rows: 16,
        cols: 16,
        dtype: DType::U16,
        endian: Endian::Big,
        confidence: 88,
        x_axis_addr: Some(0x80),
        y_axis_addr: None,
    };

    let serialized = serde_json::to_string(&cand).expect("serialize candidate");
    let deserialized: DetectedMapCandidate =
        serde_json::from_str(&serialized).expect("deserialize candidate");
    assert_eq!(cand, deserialized);

    let axis = AxisCandidate {
        start_addr: 0x80,
        length: 16,
        dtype: DType::U16,
        endian: Endian::Big,
        min_val: 1000.0,
        max_val: 8500.0,
        step_variance: 0.0,
    };
    let axis_json = serde_json::to_string(&axis).expect("serialize axis");
    let axis_deser: AxisCandidate = serde_json::from_str(&axis_json).expect("deserialize axis");
    assert_eq!(axis, axis_deser);

    assert_eq!(ScannerSensitivity::Strict.min_confidence(), 85);
    assert_eq!(ScannerSensitivity::Balanced.min_confidence(), 70);
    assert_eq!(ScannerSensitivity::Aggressive.min_confidence(), 50);
}
