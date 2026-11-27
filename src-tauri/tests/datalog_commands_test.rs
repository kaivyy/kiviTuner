use kivituner::commands::*;
use kivituner::AppState;
use kivi_protocol::datalogger::{PlaybackState, RecordingState};
use std::sync::Arc;
use std::time::Duration;

async fn connect_test_stream(state: &AppState) -> kivi_protocol::model::EcuIdentity {
    let sim = Arc::new(kivi_protocol::simulator::VirtualEcuSimulator::new(
        kivi_protocol::simulator::SimMode::IdleWarmup,
    ));
    state.telemetry.connect_driver(sim, None).await.expect("connect failed")
}

#[tokio::test]
async fn test_full_recording_lifecycle_from_live_stream() {
    let state = AppState::default();

    // 1. Connect to test telemetry stream
    let id = connect_test_stream(&state).await;
    assert_eq!(id.ecu_id, "SIM-BENCH-01");

    // Initial datalog status should be Idle
    let status_initial = get_datalog_status_inner(&state)
        .await
        .expect("get status failed");
    assert_eq!(status_initial.state, RecordingState::Idle);
    assert_eq!(status_initial.sample_count, 0);

    // 2. Start recording with user notes
    start_datalog_recording_inner(Some("Track test lap 1".to_string()), None, &state)
        .await
        .expect("start recording failed");

    // Immediately status is Recording
    let status_rec = get_datalog_status_inner(&state)
        .await
        .expect("get status failed");
    assert_eq!(status_rec.state, RecordingState::Recording);

    // 3. Wait for frames to accumulate from live virtual bench
    tokio::time::sleep(Duration::from_millis(200)).await;

    let status_mid = get_datalog_status_inner(&state)
        .await
        .expect("get status failed");
    assert_eq!(status_mid.state, RecordingState::Recording);
    assert!(
        status_mid.sample_count >= 2,
        "Expected at least 2 samples, got {}",
        status_mid.sample_count
    );
    assert!(status_mid.elapsed_ms > 0);

    // 4. Stop recording
    let metadata = stop_datalog_recording_inner(None, &state)
        .await
        .expect("stop recording failed");
    assert_eq!(metadata.notes, "Track test lap 1");
    assert!(metadata.ended_at_iso.is_some());
    assert_eq!(metadata.ecu_identity.as_ref().map(|i| i.ecu_id.as_str()), Some("SIM-BENCH-01"));
    assert_eq!(metadata.total_samples_recorded, status_mid.sample_count);

    // Status after stop should be ReadyToSave
    let status_stopped = get_datalog_status_inner(&state)
        .await
        .expect("get status failed");
    assert_eq!(status_stopped.state, RecordingState::ReadyToSave);
    assert_eq!(status_stopped.sample_count, metadata.total_samples_recorded);

    // Clean disconnect
    disconnect_ecu_inner(None, &state)
        .await
        .expect("disconnect failed");
}

#[tokio::test]
async fn test_export_csv_and_reimport_into_playback() {
    let state = AppState::default();

    // Connect, record briefly, stop
    connect_test_stream(&state).await;
    start_datalog_recording_inner(Some("Export test".to_string()), None, &state)
        .await
        .expect("start recording failed");
    tokio::time::sleep(Duration::from_millis(150)).await;
    let stop_meta = stop_datalog_recording_inner(None, &state)
        .await
        .expect("stop recording failed");
    disconnect_ecu_inner(None, &state)
        .await
        .expect("disconnect failed");

    // Export CSV
    let csv_content = export_datalog_csv_inner(&state)
        .await
        .expect("export csv failed");
    assert!(csv_content.starts_with("# kiviTuner Datalog Session v1.0"));
    assert!(csv_content.contains("timestamp_ms,sequence,rpm,tps"));
    assert!(csv_content.contains("# notes: Export test"));

    // Status should transition to Saved
    let status_saved = get_datalog_status_inner(&state)
        .await
        .expect("get status failed");
    assert_eq!(status_saved.state, RecordingState::Saved);

    // Re-import CSV into fresh state
    let fresh_state = AppState::default();
    let imported_meta = import_datalog_csv_inner(csv_content.clone(), None, &fresh_state)
        .await
        .expect("import csv failed");
    assert_eq!(imported_meta.session_id, stop_meta.session_id);
    assert_eq!(imported_meta.total_samples_recorded, stop_meta.total_samples_recorded);

    // Load into playback engine
    let playback_meta = load_datalog_playback_inner(csv_content, None, &fresh_state)
        .await
        .expect("load playback failed");
    assert_eq!(playback_meta.session_id, stop_meta.session_id);

    let pb_status = get_playback_status_inner(&fresh_state)
        .await
        .expect("get playback status failed");
    assert_eq!(pb_status.state, PlaybackState::Stopped);
    assert_eq!(pb_status.total_samples, stop_meta.total_samples_recorded);
    assert!(pb_status.total_duration_ms > 0);
}

#[tokio::test]
async fn test_playback_transport_commands_lifecycle() {
    let state = AppState::default();

    // Create a recording
    connect_test_stream(&state).await;
    start_datalog_recording_inner(None, None, &state)
        .await
        .expect("start recording failed");
    tokio::time::sleep(Duration::from_millis(200)).await;
    let _meta = stop_datalog_recording_inner(None, &state)
        .await
        .expect("stop recording failed");
    disconnect_ecu_inner(None, &state)
        .await
        .expect("disconnect failed");

    let csv_content = export_datalog_csv_inner(&state)
        .await
        .expect("export csv failed");

    // Load into playback
    load_datalog_playback_inner(csv_content, None, &state)
        .await
        .expect("load playback failed");

    // 1. Play
    start_datalog_playback_inner(None, &state)
        .await
        .expect("start playback failed");
    let st1 = get_playback_status_inner(&state).await.expect("status failed");
    assert_eq!(st1.state, PlaybackState::Playing);

    // Let it play briefly
    tokio::time::sleep(Duration::from_millis(60)).await;

    // 2. Pause
    pause_datalog_playback_inner(&state)
        .await
        .expect("pause failed");
    let st2 = get_playback_status_inner(&state).await.expect("status failed");
    assert_eq!(st2.state, PlaybackState::Paused);

    // 3. Seek
    seek_datalog_playback_inner(st2.total_duration_ms / 2, &state)
        .await
        .expect("seek failed");
    let st3 = get_playback_status_inner(&state).await.expect("status failed");
    assert_eq!(st3.state, PlaybackState::Paused);

    // 4. Speed
    set_playback_speed_inner(2.5, &state)
        .await
        .expect("speed failed");
    let st4 = get_playback_status_inner(&state).await.expect("status failed");
    assert!((st4.speed - 2.5).abs() < 1e-4);

    // 5. Loop
    set_playback_loop_inner(true, &state)
        .await
        .expect("loop failed");
    let st5 = get_playback_status_inner(&state).await.expect("status failed");
    assert!(st5.loop_playback);

    // 6. Stop
    stop_datalog_playback_inner(&state)
        .await
        .expect("stop playback failed");
    let st6 = get_playback_status_inner(&state).await.expect("status failed");
    assert_eq!(st6.state, PlaybackState::Stopped);
    assert_eq!(st6.current_index, 0);

    // 7. Unload
    unload_datalog_playback_inner(&state)
        .await
        .expect("unload playback failed");
    let st7 = get_playback_status_inner(&state).await.expect("status failed");
    assert_eq!(st7.state, PlaybackState::Stopped);
    assert_eq!(st7.total_samples, 0);
}

#[tokio::test]
async fn test_concurrency_and_cancellation_safety() {
    let state = Arc::new(AppState::default());

    // Record session
    connect_test_stream(&state).await;
    start_datalog_recording_inner(None, None, &state)
        .await
        .expect("start recording failed");
    tokio::time::sleep(Duration::from_millis(150)).await;
    stop_datalog_recording_inner(None, &state)
        .await
        .expect("stop recording failed");
    disconnect_ecu_inner(None, &state)
        .await
        .expect("disconnect failed");

    let csv_content = export_datalog_csv_inner(&state)
        .await
        .expect("export csv failed");

    load_datalog_playback_inner(csv_content, None, &state)
        .await
        .expect("load playback failed");

    // Spawn 20 concurrent tasks hammering playback controls
    let mut handles = Vec::new();
    for i in 0..20 {
        let s = state.clone();
        handles.push(tokio::spawn(async move {
            match i % 5 {
                0 => { let _ = start_datalog_playback_inner(None, &s).await; }
                1 => { let _ = pause_datalog_playback_inner(&s).await; }
                2 => { let _ = seek_datalog_playback_inner((i * 10) as u64, &s).await; }
                3 => { let _ = set_playback_speed_inner(1.0 + (i as f32 * 0.1), &s).await; }
                4 => { let _ = get_playback_status_inner(&s).await; }
                _ => {}
            }
        }));
    }

    for h in handles {
        h.await.expect("task join failed");
    }

    // Clean stop at the end
    let _ = stop_datalog_playback_inner(&state).await;
    let final_status = get_playback_status_inner(&state).await.expect("status failed");
    assert_eq!(final_status.state, PlaybackState::Stopped);
}

#[tokio::test]
async fn test_ecu_disconnect_interrupts_recording_and_preserves_buffer() {
    let state = AppState::default();

    connect_test_stream(&state).await;

    start_datalog_recording_inner(Some("Disconnect recovery test".to_string()), None, &state)
        .await
        .expect("start recording failed");

    // Wait for frames
    tokio::time::sleep(Duration::from_millis(150)).await;

    let pre_disconnect_status = get_datalog_status_inner(&state)
        .await
        .expect("get status failed");
    let sample_count = pre_disconnect_status.sample_count;
    assert!(sample_count >= 2);

    // Disconnect ECU while recording is active
    disconnect_ecu_inner(None, &state)
        .await
        .expect("disconnect failed");

    // Datalog recorder state must transition to Interrupted
    let post_disconnect_status = get_datalog_status_inner(&state)
        .await
        .expect("get status failed");
    match post_disconnect_status.state {
        RecordingState::Interrupted { reason, recoverable_samples } => {
            assert!(reason.contains("ECU disconnected during recording"));
            assert_eq!(recoverable_samples, sample_count);
        }
        other => panic!("Expected Interrupted state, got: {:?}", other),
    }

    // Buffer is preserved and can still be exported to CSV
    let csv = export_datalog_csv_inner(&state)
        .await
        .expect("export after interrupt failed");
    assert!(csv.contains("# kiviTuner Datalog Session v1.0"));
    assert!(csv.contains("Disconnect recovery test"));

    // Stop recording can also finalize the session
    let recovered_meta = stop_datalog_recording_inner(None, &state)
        .await
        .expect("stop after interrupt failed");
    assert_eq!(recovered_meta.total_samples_recorded, sample_count);
}

#[tokio::test]
async fn test_state_memory_rom_is_100_percent_untouched() {
    let state = AppState::default();

    // 1. Ingest dummy ROM into state.memory
    let original_rom = vec![0xAB; 4096];
    let original_hashes = load_bin_data_inner(&original_rom, &state).expect("load rom failed");

    // 2. Perform full recording lifecycle
    connect_test_stream(&state).await;
    start_datalog_recording_inner(Some("ROM isolation check".to_string()), None, &state)
        .await
        .expect("start failed");
    tokio::time::sleep(Duration::from_millis(100)).await;
    stop_datalog_recording_inner(None, &state).await.expect("stop failed");
    disconnect_ecu_inner(None, &state).await.expect("disconnect failed");

    let csv = export_datalog_csv_inner(&state).await.expect("export failed");
    load_datalog_playback_inner(csv, None, &state).await.expect("load playback failed");
    start_datalog_playback_inner(None, &state).await.expect("play failed");
    seek_datalog_playback_inner(50, &state).await.expect("seek failed");
    pause_datalog_playback_inner(&state).await.expect("pause failed");
    stop_datalog_playback_inner(&state).await.expect("stop playback failed");
    unload_datalog_playback_inner(&state).await.expect("unload failed");

    // 3. Verify state.memory is 100% untouched
    let current_rom = save_bin_data_inner(&state).expect("read memory failed");
    assert_eq!(current_rom, original_rom);
    let guard = state.memory.lock().unwrap();
    let current_mem = guard.as_ref().expect("memory missing");
    assert_eq!(current_mem.hashes().sha256, original_hashes.sha256);
    assert_eq!(current_mem.hashes().md5, original_hashes.md5);
}
