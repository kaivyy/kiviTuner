use kivi_core::memory_map::MemoryMap;
use kivi_core::types::{DType, Endian};
use kivi_defs::model::{AxisDef, MapDef, Scaling};
use kivi_protocol::correlation::{
    correlate_frame, match_channel_by_unit, normalize_unit, resolve_axis_channel,
    resolve_axis_coordinate, validate_numeric_labels, AxisAssociationMode, AxisBindingError,
    AxisDescriptor, AxisResolutionError, AxisRole, CorrelationStatus, TableBindingConfig,
    TelemetryChannel,
};
use kivi_protocol::model::{TelemetryFrame, TelemetryStatusFlags};

#[test]
fn test_axis_association_mode_no_link_does_not_fall_back() {
    let desc = AxisDescriptor {
        name: "Engine Speed Axis".to_string(),
        role: AxisRole::YAxis,
        unit: "RPM".to_string(),
        length: 8,
        association_mode: AxisAssociationMode::NoLink,
        labels: vec!["1000".into(), "2000".into()],
        values: vec![1000.0, 2000.0],
    };

    let result = resolve_axis_channel(&desc, None);
    assert_eq!(result, Err(AxisBindingError::NoLink));
}

#[test]
fn test_axis_association_mode_explicit_link_valid() {
    let desc = AxisDescriptor {
        name: "RPM Axis".to_string(),
        role: AxisRole::YAxis,
        unit: "RPM".to_string(),
        length: 8,
        association_mode: AxisAssociationMode::ExplicitLink("rpm".to_string()),
        labels: Vec::new(),
        values: Vec::new(),
    };

    let result = resolve_axis_channel(&desc, None);
    assert_eq!(result, Ok(TelemetryChannel::Rpm));

    // Also verify case-insensitivity on explicit link
    let desc_tps = AxisDescriptor {
        name: "TPS Axis".to_string(),
        role: AxisRole::XAxis,
        unit: "%".to_string(),
        length: 8,
        association_mode: AxisAssociationMode::ExplicitLink("  tPs  ".to_string()),
        labels: Vec::new(),
        values: Vec::new(),
    };
    assert_eq!(resolve_axis_channel(&desc_tps, None), Ok(TelemetryChannel::Tps));

    // MAP kPa
    let desc_map = AxisDescriptor {
        name: "MAP Axis".to_string(),
        role: AxisRole::XAxis,
        unit: "kPa".to_string(),
        length: 8,
        association_mode: AxisAssociationMode::ExplicitLink("map_kpa".to_string()),
        labels: Vec::new(),
        values: Vec::new(),
    };
    assert_eq!(resolve_axis_channel(&desc_map, None), Ok(TelemetryChannel::MapKpa));
}

#[test]
fn test_axis_association_mode_explicit_link_invalid_or_missing() {
    let desc_empty = AxisDescriptor {
        name: "RPM Axis".to_string(),
        role: AxisRole::YAxis,
        unit: "RPM".to_string(),
        length: 8,
        association_mode: AxisAssociationMode::ExplicitLink("   ".to_string()),
        labels: Vec::new(),
        values: Vec::new(),
    };
    assert!(matches!(
        resolve_axis_channel(&desc_empty, None),
        Err(AxisBindingError::InvalidExplicitReference { .. })
    ));

    let desc_unknown = AxisDescriptor {
        name: "Unknown Axis".to_string(),
        role: AxisRole::YAxis,
        unit: "RPM".to_string(),
        length: 8,
        association_mode: AxisAssociationMode::ExplicitLink("non_existent_channel_xyz".to_string()),
        labels: Vec::new(),
        values: Vec::new(),
    };
    assert!(matches!(
        resolve_axis_channel(&desc_unknown, None),
        Err(AxisBindingError::InvalidExplicitReference { .. })
    ));
}

#[test]
fn test_unit_normalization_and_unit_match() {
    // Normalization helper
    assert_eq!(normalize_unit("  RPM  "), "rpm");
    assert_eq!(normalize_unit("r/min"), "rpm");
    assert_eq!(normalize_unit("R/Min"), "rpm");
    assert_eq!(normalize_unit("  %  "), "%");
    assert_eq!(normalize_unit("pct"), "%");
    assert_eq!(normalize_unit("deg C"), "c");
    assert_eq!(normalize_unit("°C"), "c");
    assert_eq!(normalize_unit("DEGC"), "c");
    assert_eq!(normalize_unit("kPa"), "kpa");
    assert_eq!(normalize_unit("mbar"), "mbar");

    // Unit match resolution
    assert_eq!(match_channel_by_unit("rpm"), Ok(TelemetryChannel::Rpm));
    assert_eq!(match_channel_by_unit("r/min"), Ok(TelemetryChannel::Rpm));
    assert_eq!(match_channel_by_unit("%"), Ok(TelemetryChannel::Tps));
    assert_eq!(match_channel_by_unit("tps"), Ok(TelemetryChannel::Tps));
    assert_eq!(match_channel_by_unit("kpa"), Ok(TelemetryChannel::MapKpa));
    assert_eq!(match_channel_by_unit("mbar"), Ok(TelemetryChannel::MapKpa));
    assert_eq!(match_channel_by_unit("afr"), Ok(TelemetryChannel::Afr));
    assert_eq!(match_channel_by_unit("lambda"), Ok(TelemetryChannel::Afr));
    assert_eq!(match_channel_by_unit("v"), Ok(TelemetryChannel::BatteryVolts));
    assert_eq!(match_channel_by_unit("volts"), Ok(TelemetryChannel::BatteryVolts));
    assert_eq!(match_channel_by_unit("km/h"), Ok(TelemetryChannel::SpeedKph));
    assert_eq!(match_channel_by_unit("kph"), Ok(TelemetryChannel::SpeedKph));
    assert_eq!(match_channel_by_unit("ms"), Ok(TelemetryChannel::InjectorPulseWidthMs));
    assert_eq!(match_channel_by_unit("deg btdc"), Ok(TelemetryChannel::IgnitionAdvanceDeg));
}

#[test]
fn test_unit_match_unsupported_missing_ambiguous() {
    // Missing unit
    assert_eq!(match_channel_by_unit(""), Err(AxisBindingError::MissingUnit));
    assert_eq!(match_channel_by_unit("   "), Err(AxisBindingError::MissingUnit));

    // Unsupported unit
    assert!(matches!(
        match_channel_by_unit("quarks"),
        Err(AxisBindingError::UnsupportedUnit { .. })
    ));
    assert!(matches!(
        match_channel_by_unit("lux"),
        Err(AxisBindingError::UnsupportedUnit { .. })
    ));

    // Ambiguous unit
    assert!(matches!(
        match_channel_by_unit("temp"),
        Err(AxisBindingError::AmbiguousMatch { .. })
    ));
    assert!(matches!(
        match_channel_by_unit("pressure"),
        Err(AxisBindingError::AmbiguousMatch { .. })
    ));
}

#[test]
fn test_numeric_axis_label_validation() {
    // Valid numeric labels
    let valid_labels = vec!["800".to_string(), "1600.5".to_string(), "2400".to_string()];
    let parsed = validate_numeric_labels(&valid_labels);
    assert_eq!(parsed, Ok(vec![800.0, 1600.5, 2400.0]));

    // Empty labels valid (means values might come from ROM)
    let empty_labels: Vec<String> = Vec::new();
    assert_eq!(validate_numeric_labels(&empty_labels), Ok(Vec::new()));

    // Invalid non-numeric string labels (e.g. Park / Neutral / Cruise)
    let non_numeric = vec!["Park".to_string(), "Drive".to_string(), "Low".to_string()];
    assert_eq!(
        validate_numeric_labels(&non_numeric),
        Err(AxisBindingError::NonNumericLabels {
            label_sample: "Park".to_string(),
            index: 0,
        })
    );

    // Mixed numeric and string
    let mixed = vec!["1000".to_string(), "N/A".to_string(), "3000".to_string()];
    assert_eq!(
        validate_numeric_labels(&mixed),
        Err(AxisBindingError::NonNumericLabels {
            label_sample: "N/A".to_string(),
            index: 1,
        })
    );
}

#[test]
fn test_unit_match_with_non_numeric_labels_rejected() {
    let desc = AxisDescriptor {
        name: "Gear Status Table".to_string(),
        role: AxisRole::XAxis,
        unit: "gear".to_string(),
        length: 3,
        association_mode: AxisAssociationMode::UnitMatch,
        labels: vec!["Park".into(), "Reverse".into(), "Neutral".into()],
        values: Vec::new(),
    };

    let result = resolve_axis_channel(&desc, None);
    assert_eq!(
        result,
        Err(AxisBindingError::NonNumericLabels {
            label_sample: "Park".to_string(),
            index: 0,
        })
    );
}

#[test]
fn test_axis_descriptor_from_axis_def() {
    let axis_def = AxisDef {
        address: 0x1000,
        length: 12,
        dtype: kivi_core::types::DType::U16,
        endian: kivi_core::types::Endian::Big,
        scaling: Scaling {
            equation: "X * 25.0".to_string(),
            unit: "RPM".to_string(),
            decimals: 0,
        },
        values: vec![800.0, 1200.0, 1600.0],
    };

    let desc = AxisDescriptor::from_axis_def(&axis_def, "RPM Table Axis", AxisRole::YAxis);
    assert_eq!(desc.name, "RPM Table Axis");
    assert_eq!(desc.role, AxisRole::YAxis);
    assert_eq!(desc.unit, "RPM");
    assert_eq!(desc.length, 12);
    assert_eq!(desc.values, vec![800.0, 1200.0, 1600.0]);
    assert_eq!(desc.association_mode, AxisAssociationMode::UnitMatch);

    let resolved = resolve_axis_channel(&desc, None);
    assert_eq!(resolved, Ok(TelemetryChannel::Rpm));
}

#[test]
fn test_table_binding_config_override() {
    let desc_y = AxisDescriptor {
        name: "Y-Axis".to_string(),
        role: AxisRole::YAxis,
        unit: "unknown".to_string(),
        length: 8,
        association_mode: AxisAssociationMode::UnitMatch,
        labels: Vec::new(),
        values: Vec::new(),
    };

    let config = TableBindingConfig {
        x_axis: None,
        y_axis: Some(AxisAssociationMode::ExplicitLink("rpm".to_string())),
    };

    // Without override, unit "unknown" fails
    assert!(resolve_axis_channel(&desc_y, None).is_err());

    // With explicit config override, it resolves
    let resolved = resolve_axis_channel(&desc_y, config.y_axis.as_ref());
    assert_eq!(resolved, Ok(TelemetryChannel::Rpm));
}

#[test]
fn test_deterministic_results() {
    let desc = AxisDescriptor {
        name: "MAP Axis".to_string(),
        role: AxisRole::XAxis,
        unit: "kPa".to_string(),
        length: 8,
        association_mode: AxisAssociationMode::UnitMatch,
        labels: vec!["20".into(), "40".into(), "60".into()],
        values: vec![20.0, 40.0, 60.0],
    };

    for _ in 0..10 {
        assert_eq!(resolve_axis_channel(&desc, None), Ok(TelemetryChannel::MapKpa));
    }
}

// =========================================================================
// Task 2: Breakpoint Coordinate Resolver & Frame Correlation Tests
// =========================================================================

#[test]
fn test_ascending_axis_interior_coordinate() {
    let breakpoints = vec![1000.0, 2000.0, 3000.0, 4000.0];

    // Value 2500.0 is halfway between 2000 (idx 1) and 3000 (idx 2)
    let coord = resolve_axis_coordinate(&breakpoints, 2500.0).expect("Should resolve interior");
    assert_eq!(coord.interval_index, 1);
    assert!((coord.fractional_pos - 1.5).abs() < 1e-6);
    assert_eq!(coord.nearest_index, 1); // at 0.5 ties resolved deterministically
    assert!(!coord.is_exact_match);

    // Value 2200.0 is closer to 2000 than 3000
    let coord_near = resolve_axis_coordinate(&breakpoints, 2200.0).expect("Should resolve interior");
    assert_eq!(coord_near.interval_index, 1);
    assert!((coord_near.fractional_pos - 1.2).abs() < 1e-6);
    assert_eq!(coord_near.nearest_index, 1);
    assert!(!coord_near.is_exact_match);

    // Value 2800.0 is closer to 3000 than 2000
    let coord_far = resolve_axis_coordinate(&breakpoints, 2800.0).expect("Should resolve interior");
    assert_eq!(coord_far.interval_index, 1);
    assert!((coord_far.fractional_pos - 1.8).abs() < 1e-6);
    assert_eq!(coord_far.nearest_index, 2);
    assert!(!coord_far.is_exact_match);
}

#[test]
fn test_descending_axis_interior_coordinate() {
    let breakpoints = vec![4000.0, 3000.0, 2000.0, 1000.0];

    // Value 2500.0 is halfway between 3000 (idx 1) and 2000 (idx 2)
    let coord = resolve_axis_coordinate(&breakpoints, 2500.0).expect("Should resolve descending");
    assert_eq!(coord.interval_index, 1);
    assert!((coord.fractional_pos - 1.5).abs() < 1e-6);
    assert_eq!(coord.nearest_index, 1);
    assert!(!coord.is_exact_match);

    // Value 3800.0 is in interval 0 (4000 to 3000)
    let coord_high = resolve_axis_coordinate(&breakpoints, 3800.0).expect("Should resolve descending");
    assert_eq!(coord_high.interval_index, 0);
    assert!((coord_high.fractional_pos - 0.2).abs() < 1e-6);
    assert_eq!(coord_high.nearest_index, 0);
    assert!(!coord_high.is_exact_match);
}

#[test]
fn test_exact_lower_and_upper_boundaries() {
    let ascending = vec![1000.0, 2000.0, 3000.0];

    // Exact lower boundary
    let low = resolve_axis_coordinate(&ascending, 1000.0).expect("Lower bound");
    assert_eq!(low.interval_index, 0);
    assert!((low.fractional_pos - 0.0).abs() < 1e-6);
    assert_eq!(low.nearest_index, 0);
    assert!(low.is_exact_match);

    // Exact upper boundary
    let high = resolve_axis_coordinate(&ascending, 3000.0).expect("Upper bound");
    assert_eq!(high.interval_index, 1);
    assert!((high.fractional_pos - 2.0).abs() < 1e-6);
    assert_eq!(high.nearest_index, 2);
    assert!(high.is_exact_match);

    // Descending exact boundaries
    let descending = vec![3000.0, 2000.0, 1000.0];
    let desc_low = resolve_axis_coordinate(&descending, 3000.0).expect("Desc upper value / index 0");
    assert_eq!(desc_low.interval_index, 0);
    assert!((desc_low.fractional_pos - 0.0).abs() < 1e-6);
    assert_eq!(desc_low.nearest_index, 0);
    assert!(desc_low.is_exact_match);

    let desc_high = resolve_axis_coordinate(&descending, 1000.0).expect("Desc lower value / last index");
    assert_eq!(desc_high.interval_index, 1);
    assert!((desc_high.fractional_pos - 2.0).abs() < 1e-6);
    assert_eq!(desc_high.nearest_index, 2);
    assert!(desc_high.is_exact_match);
}

#[test]
fn test_values_immediately_below_and_above_supported_range_never_clamped() {
    let ascending = vec![1000.0, 2000.0, 3000.0];

    // Value 999.0 is below min -> MUST NOT clamp to 0
    assert_eq!(
        resolve_axis_coordinate(&ascending, 999.0),
        Err(AxisResolutionError::OutOfRange {
            value: 999.0,
            min: 1000.0,
            max: 3000.0,
        })
    );

    // Value 3001.0 is above max -> MUST NOT clamp to 2
    assert_eq!(
        resolve_axis_coordinate(&ascending, 3001.0),
        Err(AxisResolutionError::OutOfRange {
            value: 3001.0,
            min: 1000.0,
            max: 3000.0,
        })
    );
}

#[test]
fn test_nonuniform_breakpoint_spacing() {
    let breakpoints = vec![0.0, 10.0, 30.0, 70.0, 100.0];

    // Value 20.0 in interval 1 [10.0, 30.0]: width is 20.0, offset is 10.0 -> frac is 1.5
    let c1 = resolve_axis_coordinate(&breakpoints, 20.0).expect("Nonuniform 20");
    assert_eq!(c1.interval_index, 1);
    assert!((c1.fractional_pos - 1.5).abs() < 1e-6);

    // Value 50.0 in interval 2 [30.0, 70.0]: width is 40.0, offset is 20.0 -> frac is 2.5
    let c2 = resolve_axis_coordinate(&breakpoints, 50.0).expect("Nonuniform 50");
    assert_eq!(c2.interval_index, 2);
    assert!((c2.fractional_pos - 2.5).abs() < 1e-6);
}

#[test]
fn test_empty_single_duplicate_nonmonotonic_nonfinite_axes() {
    // Empty axis
    assert_eq!(
        resolve_axis_coordinate(&[], 100.0),
        Err(AxisResolutionError::EmptyAxis)
    );

    // Single-point axis
    assert_eq!(
        resolve_axis_coordinate(&[100.0], 100.0),
        Err(AxisResolutionError::SinglePointAxis)
    );

    // Duplicate breakpoints
    assert_eq!(
        resolve_axis_coordinate(&[10.0, 20.0, 20.0, 30.0], 15.0),
        Err(AxisResolutionError::DuplicateBreakpoints {
            val: 20.0,
            first_idx: 1,
            second_idx: 2,
        })
    );

    // Non-monotonic
    assert_eq!(
        resolve_axis_coordinate(&[10.0, 30.0, 20.0, 40.0], 15.0),
        Err(AxisResolutionError::NonMonotonic {
            index: 2,
            prev: 30.0,
            curr: 20.0,
        })
    );

    // Non-finite breakpoint
    assert_eq!(
        resolve_axis_coordinate(&[10.0, f64::NAN, 30.0], 15.0),
        Err(AxisResolutionError::NonFiniteBreakpoint { index: 1 })
    );

    // Non-finite input value
    assert!(matches!(
        resolve_axis_coordinate(&[10.0, 20.0, 30.0], f64::NAN),
        Err(AxisResolutionError::NonFiniteValue { .. })
    ));
}

#[test]
fn test_correlate_frame_valid_2d_table() {
    let map_def = MapDef {
        id: "fuel_table".to_string(),
        name: "Base Fuel VE".to_string(),
        category: "Fuel".to_string(),
        address: 0x0000,
        rows: 4,
        cols: 4,
        dtype: DType::U8,
        endian: Endian::Big,
        column_major: false,
        major_stride_bits: 0,
        minor_stride_bits: 0,
        scaling: Scaling {
            equation: "X * 0.5".to_string(),
            unit: "%".to_string(),
            decimals: 1,
        },
        y_axis: Some(AxisDef {
            address: 0,
            length: 4,
            dtype: DType::U16,
            endian: Endian::Big,
            scaling: Scaling {
                equation: "X".to_string(),
                unit: "RPM".to_string(),
                decimals: 0,
            },
            values: vec![1000.0, 2000.0, 3000.0, 4000.0],
        }),
        x_axis: Some(AxisDef {
            address: 0,
            length: 4,
            dtype: DType::U8,
            endian: Endian::Big,
            scaling: Scaling {
                equation: "X".to_string(),
                unit: "%".to_string(),
                decimals: 0,
            },
            values: vec![0.0, 30.0, 60.0, 100.0],
        }),
    };

    // 4x4 matrix bytes: raw values = 100 + r*10 + c
    let mut rom_bytes = vec![0u8; 16];
    for r in 0..4 {
        for c in 0..4 {
            rom_bytes[r * 4 + c] = (100 + r * 10 + c) as u8;
        }
    }
    let mem = MemoryMap::new(rom_bytes);

    let frame = TelemetryFrame {
        timestamp_ms: 12345,
        sequence: 42,
        is_simulated: true,
        rpm: 2500.0, // halfway between 2000 (row 1) and 3000 (row 2) -> row 1
        tps: 60.0,   // exact match at col 2
        ..Default::default()
    };

    let result = correlate_frame(&frame, &map_def, Some(&mem), None);
    assert!(result.is_valid);
    assert_eq!(result.status, CorrelationStatus::Valid);
    assert_eq!(result.timestamp_ms, 12345);
    assert_eq!(result.sequence, 42);
    assert!(result.is_simulated);
    assert_eq!(result.selected_row, Some(1)); // nearest row
    assert_eq!(result.selected_col, Some(2)); // exact col 2
    assert_eq!(result.cell_raw, Some(112.0)); // 100 + 10*1 + 2 = 112
    assert_eq!(result.cell_eng, Some(56.0));  // 112 * 0.5 = 56.0
}

#[test]
fn test_correlate_frame_column_major_addressing() {
    let map_def = MapDef {
        id: "col_major_table".to_string(),
        name: "Column Major Table".to_string(),
        category: "Test".to_string(),
        address: 0x0100,
        rows: 3,
        cols: 3,
        dtype: DType::U8,
        endian: Endian::Big,
        column_major: true,
        major_stride_bits: 0,
        minor_stride_bits: 0, // Column-major filling
        scaling: Scaling::default(),
        y_axis: Some(AxisDef {
            address: 0,
            length: 3,
            dtype: DType::U16,
            endian: Endian::Big,
            scaling: Scaling {
                equation: "X".to_string(),
                unit: "RPM".to_string(),
                decimals: 0,
            },
            values: vec![1000.0, 2000.0, 3000.0],
        }),
        x_axis: Some(AxisDef {
            address: 0,
            length: 3,
            dtype: DType::U8,
            endian: Endian::Big,
            scaling: Scaling {
                equation: "X".to_string(),
                unit: "%".to_string(),
                decimals: 0,
            },
            values: vec![0.0, 50.0, 100.0],
        }),
    };

    // Column-major: offset = 0x0100 + (c * rows + r) * 1
    // Let's test cell (r=1, c=2): idx = 2 * 3 + 1 = 7.
    let mut rom_bytes = vec![0u8; 512];
    rom_bytes[0x0100 + 7] = 0xAB;
    let mem = MemoryMap::new(rom_bytes);

    let frame = TelemetryFrame {
        timestamp_ms: 1000,
        sequence: 1,
        is_simulated: false,
        rpm: 2000.0, // exact row 1
        tps: 100.0,  // exact col 2
        ..Default::default()
    };

    let result = correlate_frame(&frame, &map_def, Some(&mem), None);
    assert!(result.is_valid);
    assert_eq!(result.selected_row, Some(1));
    assert_eq!(result.selected_col, Some(2));
    assert_eq!(result.cell_raw, Some(0xAB as f64));
}

#[test]
fn test_correlate_frame_stale_sample_rejected() {
    let map_def = MapDef {
        id: "test".to_string(),
        name: "Test".to_string(),
        category: "Test".to_string(),
        address: 0,
        rows: 2,
        cols: 2,
        dtype: DType::U8,
        endian: Endian::Big,
        column_major: false,
        major_stride_bits: 0,
        minor_stride_bits: 0,
        scaling: Scaling::default(),
        y_axis: Some(AxisDef {
            address: 0,
            length: 2,
            dtype: DType::U16,
            endian: Endian::Big,
            scaling: Scaling {
                equation: "X".to_string(),
                unit: "RPM".to_string(),
                decimals: 0,
            },
            values: vec![1000.0, 2000.0],
        }),
        x_axis: Some(AxisDef {
            address: 0,
            length: 2,
            dtype: DType::U8,
            endian: Endian::Big,
            scaling: Scaling {
                equation: "X".to_string(),
                unit: "%".to_string(),
                decimals: 0,
            },
            values: vec![0.0, 100.0],
        }),
    };

    let frame = TelemetryFrame {
        rpm: 1500.0,
        tps: 50.0,
        status_flags: TelemetryStatusFlags {
            stale_sample: true,
            ..Default::default()
        },
        ..Default::default()
    };

    let result = correlate_frame(&frame, &map_def, None, None);
    assert!(!result.is_valid);
    assert_eq!(result.status, CorrelationStatus::StaleSample);
    assert_eq!(result.cell_raw, None);
    assert_eq!(result.cell_eng, None);
}

#[test]
fn test_correlate_frame_communication_fault_rejected() {
    let map_def = MapDef {
        id: "test".to_string(),
        name: "Test".to_string(),
        category: "Test".to_string(),
        address: 0,
        rows: 2,
        cols: 2,
        dtype: DType::U8,
        endian: Endian::Big,
        column_major: false,
        major_stride_bits: 0,
        minor_stride_bits: 0,
        scaling: Scaling::default(),
        y_axis: Some(AxisDef {
            address: 0,
            length: 2,
            dtype: DType::U16,
            endian: Endian::Big,
            scaling: Scaling {
                equation: "X".to_string(),
                unit: "RPM".to_string(),
                decimals: 0,
            },
            values: vec![1000.0, 2000.0],
        }),
        x_axis: Some(AxisDef {
            address: 0,
            length: 2,
            dtype: DType::U8,
            endian: Endian::Big,
            scaling: Scaling {
                equation: "X".to_string(),
                unit: "%".to_string(),
                decimals: 0,
            },
            values: vec![0.0, 100.0],
        }),
    };

    let frame = TelemetryFrame {
        rpm: 1500.0,
        tps: 50.0,
        status_flags: TelemetryStatusFlags {
            communication_fault: true,
            ..Default::default()
        },
        ..Default::default()
    };

    let result = correlate_frame(&frame, &map_def, None, None);
    assert!(!result.is_valid);
    assert_eq!(result.status, CorrelationStatus::CommunicationFault);
}

#[test]
fn test_correlate_frame_nan_or_infinite_channel_rejected() {
    let map_def = MapDef {
        id: "test".to_string(),
        name: "Test".to_string(),
        category: "Test".to_string(),
        address: 0,
        rows: 2,
        cols: 2,
        dtype: DType::U8,
        endian: Endian::Big,
        column_major: false,
        major_stride_bits: 0,
        minor_stride_bits: 0,
        scaling: Scaling::default(),
        y_axis: Some(AxisDef {
            address: 0,
            length: 2,
            dtype: DType::U16,
            endian: Endian::Big,
            scaling: Scaling {
                equation: "X".to_string(),
                unit: "RPM".to_string(),
                decimals: 0,
            },
            values: vec![1000.0, 2000.0],
        }),
        x_axis: Some(AxisDef {
            address: 0,
            length: 2,
            dtype: DType::U8,
            endian: Endian::Big,
            scaling: Scaling {
                equation: "X".to_string(),
                unit: "%".to_string(),
                decimals: 0,
            },
            values: vec![0.0, 100.0],
        }),
    };

    let frame_nan = TelemetryFrame {
        rpm: f32::NAN,
        tps: 50.0,
        ..Default::default()
    };

    let result = correlate_frame(&frame_nan, &map_def, None, None);
    assert!(!result.is_valid);
    assert!(matches!(result.status, CorrelationStatus::AxisOutOfRange { .. }));
}
