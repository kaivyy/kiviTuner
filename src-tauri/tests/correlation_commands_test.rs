use kivi_core::types::{DType, Endian};
use kivi_defs::model::{AxisDef, MapDef, Scaling};
use kivi_protocol::autotune::{
    AutoTuneConfig, CellRecommendationStatus, TargetAfrSource, TunableParameterType,
};
use kivi_protocol::correlation::CorrelationStatus;
use kivi_protocol::model::{TelemetryFrame, TelemetryStatusFlags};
use kivituner::commands::{
    compute_autotune_recommendations_inner, correlate_telemetry_inner, load_bin_data_inner,
    AutoTuneCommandError, AutoTuneRequest, CorrelateTelemetryRequest, CorrelationCommandError,
};
use kivituner::AppState;

/// Helper to create a standard 2x2 VE fuel map.
fn create_test_map(id: &str) -> MapDef {
    MapDef {
        id: id.to_string(),
        name: "Test VE Table".to_string(),
        category: "Fuel".to_string(),
        address: 0x0010,
        rows: 2,
        cols: 2,
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
                unit: "kPa".to_string(),
                decimals: 0,
            },
            values: vec![20.0, 40.0],
        }),
    }
}

/// Helper to create a valid telemetry frame.
fn create_frame(timestamp_ms: u64, rpm: f32, map_kpa: f32, afr: f32) -> TelemetryFrame {
    TelemetryFrame {
        timestamp_ms,
        sequence: timestamp_ms / 50,
        is_simulated: false,
        rpm,
        tps: 25.0,
        map_kpa,
        ect_c: 85.0,
        iat_c: 30.0,
        battery_volts: 13.8,
        ignition_advance_deg: 20.0,
        injector_pulse_width_ms: 3.5,
        afr,
        speed_kph: 60.0,
        gear: 3,
        status_flags: TelemetryStatusFlags::default(),
    }
}

// ============================================================================
// ISO/IEC/IEEE 29119 Test Suite: Correlation & Auto-Tune Tauri IPC
// ============================================================================

#[tokio::test]
async fn test_correlate_telemetry_with_explicit_frame_and_map() {
    let state = AppState::default();
    let map = create_test_map("fuel_01");
    let frame = create_frame(100, 1000.0, 20.0, 14.7);

    let req = CorrelateTelemetryRequest {
        table_id: Some("fuel_01".to_string()),
        map_def: Some(map),
        frame: Some(frame),
        binding_config: None,
        source_epoch: Some(42),
        request_id: Some("req-001".to_string()),
    };

    let resp = correlate_telemetry_inner(req, &state)
        .await
        .expect("Correlation should succeed");

    assert_eq!(resp.request_id.as_deref(), Some("req-001"));
    assert_eq!(resp.source_epoch, Some(42));
    assert_eq!(resp.table_id, "fuel_01");
    assert_eq!(resp.source, "provided");
    assert!(resp.result.is_valid);
    assert_eq!(resp.result.selected_row, Some(0));
    assert_eq!(resp.result.selected_col, Some(0));
    assert_eq!(resp.result.status, CorrelationStatus::Valid);
}

#[tokio::test]
async fn test_correlate_telemetry_lookup_from_state_maps() {
    let state = AppState::default();
    let map = create_test_map("fuel_in_state");
    state.maps.lock().unwrap().push(map);

    let frame = create_frame(100, 2000.0, 40.0, 14.7);
    let req = CorrelateTelemetryRequest {
        table_id: Some("fuel_in_state".to_string()),
        map_def: None,
        frame: Some(frame),
        binding_config: None,
        source_epoch: None,
        request_id: None,
    };

    let resp = correlate_telemetry_inner(req, &state)
        .await
        .expect("Correlation should look up map from AppState");

    assert_eq!(resp.table_id, "fuel_in_state");
    assert!(resp.result.is_valid);
    assert_eq!(resp.result.selected_row, Some(1));
    assert_eq!(resp.result.selected_col, Some(1));
}

#[tokio::test]
async fn test_correlate_telemetry_fallback_to_live_telemetry() {
    let state = AppState::default();
    let map = create_test_map("fuel_live");

    // Inject deterministic simulator driver for test telemetry streaming
    let sim = std::sync::Arc::new(kivi_protocol::simulator::VirtualEcuSimulator::new(
        kivi_protocol::simulator::SimMode::IdleWarmup,
    ));
    state
        .telemetry
        .connect_driver(sim, None)
        .await
        .expect("bench connect failed");

    // Wait for at least one frame from virtual bench
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    let req = CorrelateTelemetryRequest {
        table_id: None,
        map_def: Some(map),
        frame: None, // Missing frame -> fallback to latest live telemetry
        binding_config: None,
        source_epoch: Some(1),
        request_id: Some("req-live".to_string()),
    };

    let resp = correlate_telemetry_inner(req, &state)
        .await
        .expect("Should resolve latest frame from telemetry manager");

    assert_eq!(resp.source, "live");
    assert!(resp.result.timestamp_ms > 0);

    state.telemetry.disconnect(None).await.expect("disconnect failed");
}

#[tokio::test]
async fn test_correlate_telemetry_unknown_table_id_rejected() {
    let state = AppState::default();
    let frame = create_frame(100, 1000.0, 20.0, 14.7);

    let req = CorrelateTelemetryRequest {
        table_id: Some("non_existent_table".to_string()),
        map_def: None,
        frame: Some(frame),
        binding_config: None,
        source_epoch: None,
        request_id: None,
    };

    let err = correlate_telemetry_inner(req, &state)
        .await
        .expect_err("Should fail with MapNotFound");

    assert_eq!(
        err,
        CorrelationCommandError::MapNotFound {
            table_id: "non_existent_table".to_string()
        }
    );
}

#[tokio::test]
async fn test_correlate_telemetry_no_telemetry_available_rejected() {
    let state = AppState::default();
    let map = create_test_map("fuel_no_telem");

    let req = CorrelateTelemetryRequest {
        table_id: None,
        map_def: Some(map),
        frame: None, // No frame provided and telemetry manager has no latest frame
        binding_config: None,
        source_epoch: None,
        request_id: None,
    };

    let err = correlate_telemetry_inner(req, &state)
        .await
        .expect_err("Should fail when telemetry is unavailable");

    assert!(matches!(
        err,
        CorrelationCommandError::TelemetryUnavailable { .. }
    ));
}

#[tokio::test]
async fn test_correlate_telemetry_invalid_dimensions_rejected() {
    let state = AppState::default();
    let mut map = create_test_map("invalid_dim");
    map.rows = 0; // Invalid dimension

    let frame = create_frame(100, 1000.0, 20.0, 14.7);
    let req = CorrelateTelemetryRequest {
        table_id: None,
        map_def: Some(map),
        frame: Some(frame),
        binding_config: None,
        source_epoch: None,
        request_id: None,
    };

    let err = correlate_telemetry_inner(req, &state)
        .await
        .expect_err("Should reject table with 0 rows");

    assert!(matches!(
        err,
        CorrelationCommandError::InvalidDimensions { rows: 0, .. }
    ));
}

#[tokio::test]
async fn test_correlate_telemetry_stale_faulted_out_of_range_handling() {
    let state = AppState::default();
    let map = create_test_map("fuel_stale");

    // 1. Stale frame
    let mut frame_stale = create_frame(100, 1000.0, 20.0, 14.7);
    frame_stale.status_flags.stale_sample = true;

    let req_stale = CorrelateTelemetryRequest {
        table_id: None,
        map_def: Some(map.clone()),
        frame: Some(frame_stale),
        binding_config: None,
        source_epoch: None,
        request_id: None,
    };
    let resp_stale = correlate_telemetry_inner(req_stale, &state).await.unwrap();
    assert!(!resp_stale.result.is_valid);
    assert_eq!(resp_stale.result.status, CorrelationStatus::StaleSample);

    // 2. Out of range (RPM = 9999 is outside 1000..2000)
    let frame_oor = create_frame(200, 9999.0, 20.0, 14.7);
    let req_oor = CorrelateTelemetryRequest {
        table_id: None,
        map_def: Some(map),
        frame: Some(frame_oor),
        binding_config: None,
        source_epoch: None,
        request_id: None,
    };
    let resp_oor = correlate_telemetry_inner(req_oor, &state).await.unwrap();
    assert!(!resp_oor.result.is_valid);
    assert!(matches!(
        resp_oor.result.status,
        CorrelationStatus::AxisOutOfRange { .. }
    ));
}

#[tokio::test]
async fn test_correlate_telemetry_memory_map_value_extraction() {
    let state = AppState::default();
    let map = create_test_map("fuel_mem");

    // Load ROM into state memory with 85 at offset 0x0010 (cell 0,0)
    let mut rom = vec![0u8; 128];
    rom[0x0010] = 85;
    load_bin_data_inner(&rom, &state).unwrap();

    let frame = create_frame(100, 1000.0, 20.0, 14.7);
    let req = CorrelateTelemetryRequest {
        table_id: None,
        map_def: Some(map),
        frame: Some(frame),
        binding_config: None,
        source_epoch: None,
        request_id: None,
    };

    let resp = correlate_telemetry_inner(req, &state).await.unwrap();
    assert!(resp.result.is_valid);
    assert_eq!(resp.result.cell_raw, Some(85.0));
    assert_eq!(resp.result.cell_eng, Some(85.0));
}

#[tokio::test]
async fn test_compute_autotune_recommendations_with_explicit_frames() {
    let state = AppState::default();
    let map = create_test_map("ve_fuel");

    let mut rom = vec![0u8; 128];
    rom[0x0010] = 80; // VE = 80.0
    load_bin_data_inner(&rom, &state).unwrap();

    let mut frames = Vec::new();
    for i in 0..10 {
        frames.push(create_frame(i * 100, 1000.0, 20.0, 16.0)); // Lean (16.0 vs 14.7)
    }

    let req = AutoTuneRequest {
        table_id: None,
        map_def: Some(map),
        config: AutoTuneConfig {
            target_afr: TargetAfrSource::Constant(14.7),
            min_samples_per_cell: 5,
            parameter_type: TunableParameterType::VolumetricEfficiency,
            ..Default::default()
        },
        frames: Some(frames),
        request_id: Some("tune-001".to_string()),
    };

    let resp = compute_autotune_recommendations_inner(req, &state)
        .await
        .expect("AutoTune should succeed");

    assert_eq!(resp.request_id.as_deref(), Some("tune-001"));
    assert_eq!(resp.source_provenance, "provided");
    assert_eq!(resp.report.eligible_frames_used, 10);

    let cell = resp.report.cell(0, 0).expect("Cell (0, 0) exists");
    assert_eq!(cell.status, CellRecommendationStatus::Ready);
    assert!((cell.proposed_value - 87.1).abs() < 0.1);
}

#[tokio::test]
async fn test_compute_autotune_recommendations_from_datalog_snapshot() {
    let state = AppState::default();
    let map = create_test_map("ve_fuel_datalog");

    let mut rom = vec![0u8; 128];
    rom[0x0010] = 80;
    load_bin_data_inner(&rom, &state).unwrap();

    // Start datalog recording on test telemetry stream
    let sim = std::sync::Arc::new(kivi_protocol::simulator::VirtualEcuSimulator::new(
        kivi_protocol::simulator::SimMode::IdleWarmup,
    ));
    state.telemetry.connect_driver(sim, None).await.unwrap();
    let rx = state.telemetry.subscribe().await.unwrap();
    state
        .datalog
        .start_recording(rx, None, Some("Bench Session".into()), None)
        .await
        .unwrap();

    // Collect frames for 250ms
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    state.datalog.stop_recording(None).await.unwrap();

    let req = AutoTuneRequest {
        table_id: None,
        map_def: Some(map),
        config: AutoTuneConfig {
            min_samples_per_cell: 1, // Lower threshold for short test
            ..Default::default()
        },
        frames: None, // Missing explicit frames -> snapshots from datalog manager
        request_id: Some("tune-snap".to_string()),
    };

    let resp = compute_autotune_recommendations_inner(req, &state)
        .await
        .expect("Should snapshot from datalog session");

    assert!(resp.source_provenance == "active_session" || resp.source_provenance == "recording_buffer");
    assert!(resp.report.total_frames_scanned > 0);

    state.telemetry.disconnect(None).await.unwrap();
}

#[tokio::test]
async fn test_compute_autotune_blocked_for_ignition_table() {
    let state = AppState::default();
    let mut map = create_test_map("spark_advance");
    map.category = "Ignition".to_string();
    map.scaling.unit = "deg".to_string();

    let req = AutoTuneRequest {
        table_id: None,
        map_def: Some(map),
        config: AutoTuneConfig::default(),
        frames: Some(vec![create_frame(100, 1000.0, 20.0, 14.7)]),
        request_id: None,
    };

    let resp = compute_autotune_recommendations_inner(req, &state)
        .await
        .expect("Should return blocked report");

    assert!(resp.report.is_blocked());
    assert!(resp.report.blocker_reason.is_some());
}

#[tokio::test]
async fn test_compute_autotune_invalid_configuration_rejected() {
    let state = AppState::default();
    let map = create_test_map("fuel_bad_cfg");

    let req = AutoTuneRequest {
        table_id: None,
        map_def: Some(map),
        config: AutoTuneConfig {
            target_afr: TargetAfrSource::Constant(50.0), // Implausible AFR (> 22.0)
            ..Default::default()
        },
        frames: Some(vec![create_frame(100, 1000.0, 20.0, 14.7)]),
        request_id: None,
    };

    let err = compute_autotune_recommendations_inner(req, &state)
        .await
        .expect_err("Should reject invalid configuration");

    assert!(matches!(
        err,
        AutoTuneCommandError::InvalidConfiguration { .. }
    ));
}

#[tokio::test]
async fn test_compute_autotune_no_frames_available() {
    let state = AppState::default();
    let map = create_test_map("fuel_empty");

    let req = AutoTuneRequest {
        table_id: None,
        map_def: Some(map),
        config: AutoTuneConfig::default(),
        frames: None, // No frames in datalog and none provided
        request_id: None,
    };

    let resp = compute_autotune_recommendations_inner(req, &state)
        .await
        .expect("Should return report with 0 frames scanned");

    assert_eq!(resp.report.total_frames_scanned, 0);
    assert_eq!(resp.report.eligible_frames_used, 0);
}

#[tokio::test]
async fn test_ipc_request_id_and_source_epoch_preservation() {
    let state = AppState::default();
    let map = create_test_map("epoch_test");
    let frame = create_frame(100, 1000.0, 20.0, 14.7);

    let req = CorrelateTelemetryRequest {
        table_id: None,
        map_def: Some(map),
        frame: Some(frame),
        binding_config: None,
        source_epoch: Some(999),
        request_id: Some("client-req-999".to_string()),
    };

    let resp = correlate_telemetry_inner(req, &state).await.unwrap();
    assert_eq!(resp.source_epoch, Some(999));
    assert_eq!(resp.request_id.as_deref(), Some("client-req-999"));
}

#[tokio::test]
async fn test_ipc_commands_strict_rom_and_history_isolation() {
    let state = AppState::default();
    let map = create_test_map("iso_test");

    let initial_rom = vec![0x33u8; 256];
    load_bin_data_inner(&initial_rom, &state).unwrap();

    let frame = create_frame(100, 1000.0, 20.0, 16.0);

    // 1. Run correlation
    let _ = correlate_telemetry_inner(
        CorrelateTelemetryRequest {
            table_id: None,
            map_def: Some(map.clone()),
            frame: Some(frame.clone()),
            binding_config: None,
            source_epoch: None,
            request_id: None,
        },
        &state,
    )
    .await
    .unwrap();

    // 2. Run AutoTune
    let _ = compute_autotune_recommendations_inner(
        AutoTuneRequest {
            table_id: None,
            map_def: Some(map),
            config: AutoTuneConfig::default(),
            frames: Some(vec![frame]),
            request_id: None,
        },
        &state,
    )
    .await
    .unwrap();

    // Verify memory map byte-for-byte identical
    let mem_lock = state.memory.lock().unwrap();
    assert_eq!(mem_lock.as_ref().unwrap().as_slice(), initial_rom.as_slice());
}

#[test]
fn test_ipc_contracts_serde_roundtrip() {
    let req = CorrelateTelemetryRequest {
        table_id: Some("map_1".to_string()),
        map_def: None,
        frame: Some(create_frame(100, 1000.0, 20.0, 14.7)),
        binding_config: None,
        source_epoch: Some(10),
        request_id: Some("req-test".to_string()),
    };

    let json = serde_json::to_string(&req).expect("serialize req failed");
    let req_deser: CorrelateTelemetryRequest =
        serde_json::from_str(&json).expect("deserialize req failed");
    assert_eq!(req, req_deser);

    let err = CorrelationCommandError::MapNotFound {
        table_id: "xyz".to_string(),
    };
    let json_err = serde_json::to_string(&err).expect("serialize err failed");
    let err_deser: CorrelationCommandError =
        serde_json::from_str(&json_err).expect("deserialize err failed");
    assert_eq!(err, err_deser);
}
