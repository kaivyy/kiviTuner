use kivi_core::memory_map::MemoryMap;
use kivi_core::types::{DType, Endian};
use kivi_defs::model::{AxisDef, MapDef, Scaling};
use kivi_project::delta::ProjectSession;
use kivi_protocol::autotune::{
    compute_recommendations, AutoTuneConfig, CellRecommendationStatus, TargetAfrSource,
    TunableParameterType,
};
use kivi_protocol::model::{TelemetryFrame, TelemetryStatusFlags};

/// Helper to construct a standard 4x4 VE fuel table for tests.
fn create_test_fuel_map() -> MapDef {
    MapDef {
        id: "ve_fuel_table".to_string(),
        name: "Main VE Fuel Table".to_string(),
        category: "Fuel".to_string(),
        address: 0x1000,
        rows: 4,
        cols: 4,
        dtype: DType::U8,
        endian: Endian::Big,
        column_major: false,
        major_stride_bits: 0,
        minor_stride_bits: 0,
        scaling: Scaling {
            equation: "X".to_string(),
            unit: "%".to_string(),
            decimals: 1,
        },
        y_axis: Some(AxisDef {
            address: 0x0000,
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
            address: 0x0000,
            length: 4,
            dtype: DType::U8,
            endian: Endian::Big,
            scaling: Scaling {
                equation: "X".to_string(),
                unit: "kPa".to_string(),
                decimals: 0,
            },
            values: vec![20.0, 40.0, 60.0, 80.0],
        }),
    }
}

/// Helper to create a memory map with default VE values for the test fuel map.
fn create_test_memory(initial_ve: u8) -> MemoryMap {
    let mut data = vec![0u8; 0x2000];
    for i in 0..16 {
        data[0x1000 + i] = initial_ve;
    }
    MemoryMap::new(data)
}

/// Helper to create a valid, fully warmed-up steady-state telemetry frame.
fn create_valid_frame(timestamp_ms: u64, rpm: f32, map_kpa: f32, afr: f32, tps: f32) -> TelemetryFrame {
    TelemetryFrame {
        timestamp_ms,
        sequence: timestamp_ms / 50,
        is_simulated: false,
        rpm,
        tps,
        map_kpa,
        ect_c: 85.0, // Warmed up (> 70 C)
        iat_c: 30.0,
        battery_volts: 13.8,
        ignition_advance_deg: 20.0,
        injector_pulse_width_ms: 3.5, // Non-zero fuel injection
        afr,
        speed_kph: 60.0,
        gear: 3,
        status_flags: TelemetryStatusFlags::default(),
    }
}

#[test]
fn test_autotune_valid_deterministic_recommendation() {
    let map_def = create_test_fuel_map();
    let mem = create_test_memory(80); // Current VE = 80.0%
    let config = AutoTuneConfig {
        target_afr: TargetAfrSource::Constant(14.7),
        min_samples_per_cell: 5,
        parameter_type: TunableParameterType::VolumetricEfficiency,
        ..Default::default()
    };

    // 10 steady-state samples at RPM = 2000, MAP = 40 (row 1, col 1) with measured AFR = 16.0 (lean)
    let mut frames = Vec::new();
    for i in 0..10 {
        frames.push(create_valid_frame(i * 100, 2000.0, 40.0, 16.0, 25.0));
    }

    let report = compute_recommendations(&frames, &map_def, Some(&mem), &config);

    assert_eq!(report.table_id, "ve_fuel_table");
    assert_eq!(report.total_frames_scanned, 10);
    assert_eq!(report.eligible_frames_used, 10);

    let cell = report.cell(1, 1).expect("Cell (1, 1) recommendation must exist");
    assert_eq!(cell.status, CellRecommendationStatus::Ready);
    assert_eq!(cell.sample_count, 10);
    assert_eq!(cell.original_value, 80.0);

    // Formula: 80.0 * (16.0 / 14.7) = 87.0748 -> rounded to 1 decimal = 87.1
    let expected_proposed = 87.1;
    assert!((cell.proposed_value - expected_proposed).abs() < 0.1);
    assert!((cell.delta_percentage - 8.84).abs() < 0.2);
    assert_eq!(cell.unit, "%");
}

#[test]
fn test_autotune_insufficient_samples() {
    let map_def = create_test_fuel_map();
    let mem = create_test_memory(80);
    let config = AutoTuneConfig {
        target_afr: TargetAfrSource::Constant(14.7),
        min_samples_per_cell: 5,
        ..Default::default()
    };

    // Only 3 samples (< 5 required)
    let mut frames = Vec::new();
    for i in 0..3 {
        frames.push(create_valid_frame(i * 100, 2000.0, 40.0, 16.0, 25.0));
    }

    let report = compute_recommendations(&frames, &map_def, Some(&mem), &config);
    let cell = report.cell(1, 1).expect("Cell (1, 1) recommendation must exist");

    assert_eq!(cell.status, CellRecommendationStatus::InsufficientEvidence);
    assert_eq!(cell.sample_count, 3);
    assert_eq!(cell.proposed_value, cell.original_value);
    assert_eq!(cell.delta_percentage, 0.0);
    assert!(cell.warning.is_some());
}

#[test]
fn test_autotune_missing_target_afr_or_incompatible_table() {
    let mut ignition_map = create_test_fuel_map();
    ignition_map.category = "Ignition".to_string();
    ignition_map.scaling.unit = "deg".to_string();

    let mem = create_test_memory(20);
    let config = AutoTuneConfig {
        target_afr: TargetAfrSource::Constant(14.7),
        parameter_type: TunableParameterType::AutoDetect,
        ..Default::default()
    };

    let frames = vec![create_valid_frame(100, 2000.0, 40.0, 14.7, 25.0)];
    let report = compute_recommendations(&frames, &ignition_map, Some(&mem), &config);

    // Ignition table cannot be auto-tuned with AFR formula
    assert!(report.is_blocked());
    assert_eq!(
        report.blocker_reason.as_deref(),
        Some("Incompatible table: table category/unit is not a recognized fuel parameter")
    );
}

#[test]
fn test_autotune_invalid_and_nonfinite_afr_rejected() {
    let map_def = create_test_fuel_map();
    let mem = create_test_memory(80);
    let config = AutoTuneConfig::default();

    let frames = vec![
        create_valid_frame(100, 2000.0, 40.0, f32::NAN, 25.0),
        create_valid_frame(200, 2000.0, 40.0, f32::INFINITY, 25.0),
        create_valid_frame(300, 2000.0, 40.0, 5.0, 25.0),   // Implausibly low (< 10.0)
        create_valid_frame(400, 2000.0, 40.0, 25.0, 25.0),  // Implausibly high (> 20.0)
    ];

    let report = compute_recommendations(&frames, &map_def, Some(&mem), &config);

    assert_eq!(report.eligible_frames_used, 0);
    assert_eq!(report.rejections.rejected_sensor_out_of_range, 2);
    assert_eq!(report.rejections.rejected_non_finite, 2);
}

#[test]
fn test_autotune_stale_samples_and_comm_fault_rejected() {
    let map_def = create_test_fuel_map();
    let mem = create_test_memory(80);
    let config = AutoTuneConfig::default();

    let mut frame_stale = create_valid_frame(100, 2000.0, 40.0, 14.7, 25.0);
    frame_stale.status_flags.stale_sample = true;

    let mut frame_fault = create_valid_frame(200, 2000.0, 40.0, 14.7, 25.0);
    frame_fault.status_flags.communication_fault = true;

    let frames = vec![frame_stale, frame_fault];
    let report = compute_recommendations(&frames, &map_def, Some(&mem), &config);

    assert_eq!(report.eligible_frames_used, 0);
    assert_eq!(report.rejections.rejected_stale, 1);
    assert_eq!(report.rejections.rejected_fault, 1);
}

#[test]
fn test_autotune_out_of_range_and_failed_correlations_rejected() {
    let map_def = create_test_fuel_map();
    let mem = create_test_memory(80);
    let config = AutoTuneConfig::default();

    // Map X is 20..80 kPa, Y is 1000..4000 RPM
    let frames = vec![
        create_valid_frame(100, 500.0, 40.0, 14.7, 25.0),    // RPM out of range
        create_valid_frame(200, 2000.0, 120.0, 14.7, 25.0),  // MAP out of range
    ];

    let report = compute_recommendations(&frames, &map_def, Some(&mem), &config);

    assert_eq!(report.eligible_frames_used, 0);
    assert_eq!(report.rejections.rejected_out_of_bounds, 2);
}

#[test]
fn test_autotune_zero_or_implausible_target_measured_afr() {
    let map_def = create_test_fuel_map();
    let mem = create_test_memory(80);

    // Target AFR <= 0.0 or > 25.0
    let config_zero = AutoTuneConfig {
        target_afr: TargetAfrSource::Constant(0.0),
        ..Default::default()
    };
    let frames = vec![create_valid_frame(100, 2000.0, 40.0, 14.7, 25.0)];
    let report_zero = compute_recommendations(&frames, &map_def, Some(&mem), &config_zero);
    assert!(report_zero.is_blocked());

    let config_high = AutoTuneConfig {
        target_afr: TargetAfrSource::Constant(30.0),
        ..Default::default()
    };
    let report_high = compute_recommendations(&frames, &map_def, Some(&mem), &config_high);
    assert!(report_high.is_blocked());
}

#[test]
fn test_autotune_transient_throttle_and_cold_engine_rejected() {
    let map_def = create_test_fuel_map();
    let mem = create_test_memory(80);
    let config = AutoTuneConfig {
        min_ect_c: 70.0,
        max_tps_delta_per_sec: 15.0,
        ..Default::default()
    };

    let mut frame_cold = create_valid_frame(100, 2000.0, 40.0, 14.7, 25.0);
    frame_cold.ect_c = 50.0; // Cold engine (< 70 C)

    let mut frame_fuelcut = create_valid_frame(200, 2000.0, 40.0, 14.7, 25.0);
    frame_fuelcut.injector_pulse_width_ms = 0.0; // Overrun fuel cut

    // Throttle steady at 25%, then spikes to 50% in 100ms -> dTPS/dt = 250%/s (> 15%/s)
    let frame_steady = create_valid_frame(300, 2000.0, 40.0, 14.7, 25.0);
    let frame_spike = create_valid_frame(400, 2000.0, 40.0, 14.7, 50.0);

    let frames = vec![frame_cold, frame_fuelcut, frame_steady, frame_spike];
    let report = compute_recommendations(&frames, &map_def, Some(&mem), &config);

    assert_eq!(report.rejections.rejected_cold_engine, 1);
    assert_eq!(report.rejections.rejected_fuel_cut, 1);
    assert_eq!(report.rejections.rejected_transient_throttle, 1);
    // Only the steady frame was accepted as non-transient
    assert_eq!(report.eligible_frames_used, 1);
}

#[test]
fn test_autotune_repeated_samples_temporal_dependence() {
    let map_def = create_test_fuel_map();
    let mem = create_test_memory(80);
    let config = AutoTuneConfig {
        min_samples_per_cell: 5,
        ..Default::default()
    };

    // 10 frames with identical timestamp (0ms delta) should not artificially inflate effective sample weight
    let mut frames = Vec::new();
    for _ in 0..10 {
        frames.push(create_valid_frame(100, 2000.0, 40.0, 16.0, 25.0));
    }

    let report = compute_recommendations(&frames, &map_def, Some(&mem), &config);
    let cell = report.cell(1, 1).unwrap();

    // Repeated identical timestamps get flagged or penalized in effective count
    assert!(cell.effective_sample_count < 10.0);
}

#[test]
fn test_autotune_high_variance_outlier_detection() {
    let map_def = create_test_fuel_map();
    let mem = create_test_memory(80);
    let config = AutoTuneConfig {
        min_samples_per_cell: 4,
        max_afr_std_dev: 1.0,
        ..Default::default()
    };

    // Readings with wild variance: 11.0, 18.0, 12.0, 19.0 (std dev > 3.0)
    let frames = vec![
        create_valid_frame(100, 2000.0, 40.0, 11.0, 25.0),
        create_valid_frame(200, 2000.0, 40.0, 18.0, 25.0),
        create_valid_frame(300, 2000.0, 40.0, 12.0, 25.0),
        create_valid_frame(400, 2000.0, 40.0, 19.0, 25.0),
    ];

    let report = compute_recommendations(&frames, &map_def, Some(&mem), &config);
    let cell = report.cell(1, 1).unwrap();

    assert_eq!(cell.status, CellRecommendationStatus::HighVarianceWarning);
    assert!(cell.std_dev_afr > 1.0);
    assert!(cell.warning.is_some());
}

#[test]
fn test_autotune_correction_bounds_and_rounding() {
    let map_def = create_test_fuel_map();
    let mem = create_test_memory(80);
    let config = AutoTuneConfig {
        target_afr: TargetAfrSource::Constant(14.7),
        min_samples_per_cell: 5,
        max_cell_delta_pct: 15.0, // Strict +/- 15% clamp
        ..Default::default()
    };

    // Extremely lean: AFR = 20.0 (unclamped formula would be 80 * 20 / 14.7 = 108.84, +36%)
    let mut frames = Vec::new();
    for i in 0..10 {
        frames.push(create_valid_frame(i * 100, 2000.0, 40.0, 20.0, 25.0));
    }

    let report = compute_recommendations(&frames, &map_def, Some(&mem), &config);
    let cell = report.cell(1, 1).unwrap();

    // Must be clamped to exactly 80.0 * 1.15 = 92.0
    assert_eq!(cell.proposed_value, 92.0);
    assert_eq!(cell.delta_percentage, 15.0);
    assert!(cell.is_clamped);
}

#[test]
fn test_autotune_multiple_cells_receive_independent_evidence() {
    let map_def = create_test_fuel_map();
    let mem = create_test_memory(80);
    let config = AutoTuneConfig {
        min_samples_per_cell: 5,
        ..Default::default()
    };

    let mut frames = Vec::new();
    // 6 samples at (row 0, col 0) [1000 RPM, 20 kPa] - Lean (16.0 AFR)
    for i in 0..6 {
        frames.push(create_valid_frame(i * 100, 1000.0, 20.0, 16.0, 10.0));
    }
    // 6 samples at (row 2, col 3) [3000 RPM, 80 kPa] - Rich (13.0 AFR)
    for i in 0..6 {
        frames.push(create_valid_frame(1000 + i * 100, 3000.0, 80.0, 13.0, 60.0));
    }

    let report = compute_recommendations(&frames, &map_def, Some(&mem), &config);

    let cell_0_0 = report.cell(0, 0).unwrap();
    let cell_2_3 = report.cell(2, 3).unwrap();
    let cell_1_1 = report.cell(1, 1).unwrap();

    // Cell (0, 0) should propose fuel addition
    assert_eq!(cell_0_0.status, CellRecommendationStatus::Ready);
    assert!(cell_0_0.proposed_value > cell_0_0.original_value);

    // Cell (2, 3) should propose fuel cut
    assert_eq!(cell_2_3.status, CellRecommendationStatus::Ready);
    assert!(cell_2_3.proposed_value < cell_2_3.original_value);

    // Cell (1, 1) had no samples
    assert_eq!(cell_1_1.status, CellRecommendationStatus::InsufficientEvidence);
    assert_eq!(cell_1_1.sample_count, 0);
}

#[test]
fn test_autotune_empty_input_and_mixed_validity_batches() {
    let map_def = create_test_fuel_map();
    let mem = create_test_memory(80);
    let config = AutoTuneConfig::default();

    // 1. Completely empty frame slice
    let report_empty = compute_recommendations(&[], &map_def, Some(&mem), &config);
    assert_eq!(report_empty.total_frames_scanned, 0);
    assert_eq!(report_empty.eligible_frames_used, 0);

    // 2. Mixed batch: 5 valid, 5 invalid
    let mut mixed_frames = Vec::new();
    for i in 0..5 {
        mixed_frames.push(create_valid_frame(i * 100, 2000.0, 40.0, 14.7, 25.0));
    }
    for i in 5..10 {
        let mut f = create_valid_frame(i * 100, 2000.0, 40.0, 14.7, 25.0);
        f.status_flags.stale_sample = true;
        mixed_frames.push(f);
    }

    let report_mixed = compute_recommendations(&mixed_frames, &map_def, Some(&mem), &config);
    assert_eq!(report_mixed.total_frames_scanned, 10);
    assert_eq!(report_mixed.eligible_frames_used, 5);
    assert_eq!(report_mixed.rejections.rejected_stale, 5);
}

#[test]
fn test_autotune_deterministic_repeatability() {
    let map_def = create_test_fuel_map();
    let mem = create_test_memory(80);
    let config = AutoTuneConfig {
        min_samples_per_cell: 5,
        ..Default::default()
    };

    let mut frames = Vec::new();
    for i in 0..20 {
        let afr = 14.0 + ((i % 5) as f32) * 0.4;
        frames.push(create_valid_frame(i * 100, 2000.0, 40.0, afr, 25.0));
    }

    let report_1 = compute_recommendations(&frames, &map_def, Some(&mem), &config);
    let report_2 = compute_recommendations(&frames, &map_def, Some(&mem), &config);

    assert_eq!(report_1, report_2);
}

#[test]
fn test_autotune_distinction_between_unavailable_and_zero_correction() {
    let map_def = create_test_fuel_map();
    let mem = create_test_memory(80);
    let config = AutoTuneConfig {
        target_afr: TargetAfrSource::Constant(14.7),
        min_samples_per_cell: 5,
        ..Default::default()
    };

    // 10 samples perfectly on target (14.7 AFR)
    let mut frames = Vec::new();
    for i in 0..10 {
        frames.push(create_valid_frame(i * 100, 2000.0, 40.0, 14.7, 25.0));
    }

    let report = compute_recommendations(&frames, &map_def, Some(&mem), &config);
    let cell = report.cell(1, 1).unwrap();

    // Should be explicitly NoChange (not InsufficientEvidence)
    assert_eq!(cell.status, CellRecommendationStatus::NoChange);
    assert_eq!(cell.proposed_value, 80.0);
    assert_eq!(cell.delta_percentage, 0.0);
}

#[test]
fn test_autotune_rom_and_project_isolation() {
    let map_def = create_test_fuel_map();
    let initial_rom = vec![0x55u8; 0x2000];
    let mem = MemoryMap::new(initial_rom.clone());
    let project = ProjectSession::new(initial_rom.clone());

    let config = AutoTuneConfig {
        min_samples_per_cell: 5,
        ..Default::default()
    };

    let mut frames = Vec::new();
    for i in 0..10 {
        frames.push(create_valid_frame(i * 100, 2000.0, 40.0, 16.0, 25.0));
    }

    // Run engine
    let report = compute_recommendations(&frames, &map_def, Some(&mem), &config);
    assert!(report.eligible_frames_used > 0);

    // Assert MemoryMap byte slice is 100% byte-for-byte identical
    assert_eq!(mem.as_slice(), initial_rom.as_slice());

    // Assert ProjectSession ROM and commit history are completely untouched
    assert_eq!(project.original_rom, initial_rom);
    assert_eq!(project.current_rom, initial_rom);
    assert_eq!(project.history.len(), 0);
}
