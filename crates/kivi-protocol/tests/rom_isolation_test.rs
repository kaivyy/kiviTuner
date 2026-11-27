use std::sync::Arc;

use kivi_core::memory_map::MemoryMap;
use kivi_project::ProjectSession as ProjectHistory;
use kivi_protocol::clock::MockClock;
use kivi_protocol::datalogger::{
    export_session_to_csv, parse_session_from_str, BoundedDatalogBuffer, PlaybackEngine,
    PlaybackState, RecordingState,
};
use kivi_protocol::driver::TelemetrySource;
use kivi_protocol::errors::ProtocolError;
use kivi_protocol::faults::SimFault;
use kivi_protocol::model::ConnectionState;
use kivi_protocol::simulator::{SimMode, VirtualEcuSimulator};
use kivi_protocol::validation::validate_telemetry_frame;

#[tokio::test]
async fn test_12_point_rom_calibration_state_isolation_and_workspace_verification() {
    // -------------------------------------------------------------------------
    // Point 1: Ingest a 64 KB ROM (vec![0xAA; 65536]) into MemoryMap
    // -------------------------------------------------------------------------
    let original_bytes = vec![0xAA; 65536];
    let mem = MemoryMap::new(original_bytes.clone());
    assert_eq!(mem.len(), 65536);
    assert_eq!(mem.as_slice(), original_bytes.as_slice());

    // -------------------------------------------------------------------------
    // Point 2: Record baseline SHA-256 and MD5 hashes (mem.hashes())
    // -------------------------------------------------------------------------
    let original_hashes = mem.hashes().clone();
    let original_sha256 = original_hashes.sha256.clone();
    let original_md5 = original_hashes.md5.clone();
    assert!(!original_sha256.is_empty());
    assert!(!original_md5.is_empty());

    // -------------------------------------------------------------------------
    // Point 3: Instantiate ProjectHistory from kivi-project (record commit count)
    // -------------------------------------------------------------------------
    let history = ProjectHistory::from_memory_map(&mem);
    let initial_commit_count = history.history.len();
    assert_eq!(initial_commit_count, 0);
    assert!(history.history.is_empty());

    // -------------------------------------------------------------------------
    // Point 4: Start virtual ECU simulator at 20 Hz, capturing frames into BoundedDatalogBuffer
    // -------------------------------------------------------------------------
    let sim = VirtualEcuSimulator::new_with_seed(1234, SimMode::DynoPull);
    let ecu_id = sim.connect().await.expect("connect to simulator");
    assert!(matches!(sim.connection_state(), ConnectionState::Connected { .. }));

    let mut buffer = BoundedDatalogBuffer::with_capacity(20);
    buffer
        .start_recording(
            "TEST-ROM-ISOLATION-01",
            "2026-10-24T12:00:00Z",
            Some(ecu_id.clone()),
            Some("Milestone 3B ROM isolation test session".to_string()),
        )
        .expect("start recording");
    assert_eq!(*buffer.state(), RecordingState::Recording);

    // -------------------------------------------------------------------------
    // Point 5: Force buffer capacity eviction (fill buffer beyond capacity and verify dropped_samples_count > 0)
    // -------------------------------------------------------------------------
    // Push 35 frames into a buffer with capacity 20 -> 15 frames evicted
    for _ in 0..35 {
        let frame = sim.step_tick();
        buffer.push_frame(frame);
    }

    assert_eq!(buffer.len(), 20, "Buffer len must be capped at capacity (20)");
    assert_eq!(
        buffer.dropped_samples_count(),
        15,
        "Dropped sample count must accurately track 15 evicted frames"
    );
    assert!(
        buffer.dropped_samples_count() > 0,
        "Eviction must have dropped samples"
    );
    assert_eq!(buffer.total_samples_recorded(), 35);

    buffer
        .stop_recording(Some("2026-10-24T12:00:02Z".to_string()))
        .expect("stop recording");
    assert_eq!(*buffer.state(), RecordingState::ReadyToSave);
    let session = buffer.build_session();
    assert_eq!(session.sample_count(), 20);

    // -------------------------------------------------------------------------
    // Point 6: Export session to CSV v1 string (export_session_to_csv)
    // -------------------------------------------------------------------------
    let csv_str = export_session_to_csv(&session).expect("export session to CSV v1");
    assert!(csv_str.starts_with("# kiviTuner Datalog Session v1.0"));
    assert!(csv_str.contains("TEST-ROM-ISOLATION-01"));
    assert!(csv_str.contains("# dropped_samples: 15"));
    assert!(csv_str.contains("# total_samples: 35"));
    assert!(csv_str.contains("timestamp_ms,sequence,rpm,tps"));

    // -------------------------------------------------------------------------
    // Point 7: Parse session from CSV v1 string (parse_session_from_str)
    // -------------------------------------------------------------------------
    let parsed_session = parse_session_from_str(&csv_str).expect("parse session from CSV v1");
    assert_eq!(parsed_session.frames.len(), 20);
    assert_eq!(parsed_session.metadata.session_id, "TEST-ROM-ISOLATION-01");
    assert_eq!(parsed_session.metadata.dropped_samples_count, 15);
    assert_eq!(parsed_session.metadata.total_samples_recorded, 35);

    // -------------------------------------------------------------------------
    // Point 8: Instantiate PlaybackEngine with parsed session under MockClock
    // -------------------------------------------------------------------------
    let start_ts = parsed_session.frames[0].timestamp_ms;
    let clock = Arc::new(MockClock::new(start_ts));
    let playback = PlaybackEngine::new_with_clock(Arc::new(parsed_session.clone()), clock.clone());
    assert_eq!(playback.status().state, PlaybackState::Stopped);
    assert_eq!(playback.status().total_samples, 20);

    // -------------------------------------------------------------------------
    // Point 9: Execute playback controls: play, pause, resume, seek forward/backward, speed scaling, loop toggle, stop
    // -------------------------------------------------------------------------
    // Play
    playback.play().expect("play");
    assert_eq!(playback.status().state, PlaybackState::Playing);

    // Step a frame using clock
    clock.advance_ms(50);
    let f1 = playback.step_next_frame().expect("step frame 1");
    assert_eq!(f1.sequence, parsed_session.frames[1].sequence);

    // Pause
    playback.pause().expect("pause");
    assert_eq!(playback.status().state, PlaybackState::Paused);

    // Resume
    playback.resume().expect("resume");
    assert_eq!(playback.status().state, PlaybackState::Playing);

    // Seek forward
    let max_ts = parsed_session.frames.last().unwrap().timestamp_ms;
    playback.seek_ms(max_ts).expect("seek forward to max_ts");
    assert_eq!(playback.status().current_time_ms, max_ts);

    // Seek backward
    let mid_ts = start_ts + (max_ts - start_ts) / 2;
    playback.seek_ms(mid_ts).expect("seek backward to mid_ts");
    assert_eq!(playback.status().current_time_ms, mid_ts);

    // Speed scaling (0.5x, 2x, 4x)
    playback.set_speed(0.5).expect("set speed 0.5x");
    assert_eq!(playback.status().speed, 0.5);

    playback.set_speed(2.0).expect("set speed 2.0x");
    assert_eq!(playback.status().speed, 2.0);

    playback.set_speed(4.0).expect("set speed 4.0x");
    assert_eq!(playback.status().speed, 4.0);

    // Loop toggle
    playback.set_loop(true);
    assert!(playback.status().loop_playback);
    playback.set_loop(false);
    assert!(!playback.status().loop_playback);

    // Stop
    playback.stop().expect("stop");
    assert_eq!(playback.status().state, PlaybackState::Stopped);

    // -------------------------------------------------------------------------
    // Point 10: Switch telemetry source between live simulator and playback engine
    // -------------------------------------------------------------------------
    let mut active_source: Box<dyn TelemetrySource> = Box::new(sim.clone());
    let mut rx_live = active_source.subscribe_telemetry();
    let tick_frame = sim.step_tick();
    let received_live = rx_live.try_recv().expect("live frame broadcast");
    assert_eq!(received_live.sequence, tick_frame.sequence);

    // Switch to playback source
    active_source = Box::new(playback.clone());
    let mut rx_pb = active_source.subscribe_telemetry();
    playback.play().expect("play after source switch");
    let first_pb = rx_pb.try_recv().expect("first playback frame broadcast on play");
    assert_eq!(first_pb.sequence, parsed_session.frames[0].sequence);

    clock.advance_ms(50);
    let pb_frame = playback.step_next_frame().expect("playback frame step");
    let received_pb = rx_pb.try_recv().expect("playback frame broadcast on step");
    assert_eq!(received_pb.sequence, pb_frame.sequence);
    playback.stop().expect("stop playback after source switch");

    // -------------------------------------------------------------------------
    // Point 11: Inject simulator faults (SimFault::Disconnect, SimFault::Timeout, SimFault::InvalidFrame)
    // -------------------------------------------------------------------------
    // Fault 1: Disconnect
    sim.inject_fault(SimFault::Disconnect);
    sim.step_tick();
    assert!(matches!(sim.connection_state(), ConnectionState::Faulted { .. }));
    sim.inject_fault(SimFault::None);
    assert!(matches!(sim.connection_state(), ConnectionState::Connected { .. }));

    // Fault 2: Timeout
    sim.disconnect().await.expect("disconnect before timeout test");
    sim.inject_fault(SimFault::Timeout);
    let connect_err = sim.connect().await;
    assert!(matches!(connect_err, Err(ProtocolError::Timeout(_))));
    assert!(matches!(sim.connection_state(), ConnectionState::Faulted { .. }));
    sim.inject_fault(SimFault::None);
    sim.connect().await.expect("reconnect after timeout test");

    // Fault 3: InvalidFrame
    sim.inject_fault(SimFault::InvalidFrame);
    let invalid_frame = sim.step_tick();
    assert!(
        !validate_telemetry_frame(&invalid_frame).is_valid(),
        "Injected invalid frame must fail telemetry validation"
    );
    sim.inject_fault(SimFault::None);
    let valid_frame = sim.step_tick();
    assert!(
        validate_telemetry_frame(&valid_frame).is_valid(),
        "Normal frame must pass telemetry validation"
    );

    sim.disconnect().await.expect("clean simulator disconnect");

    // -------------------------------------------------------------------------
    // Point 12: Assertions: ROM memory & ProjectHistory remain completely untouched
    // -------------------------------------------------------------------------
    // Exact 65,536 bytes identical
    assert_eq!(mem.len(), 65536);
    assert_eq!(
        mem.as_slice(),
        original_bytes.as_slice(),
        "ROM calibration memory must remain 100% bit-for-bit identical"
    );

    // Baseline hashes identical
    assert_eq!(
        mem.hashes().sha256,
        original_sha256,
        "SHA-256 hash must not mutate under any logging or playback operation"
    );
    assert_eq!(
        mem.hashes().md5,
        original_md5,
        "MD5 hash must not mutate under any logging or playback operation"
    );

    // Project ledger has zero new commits
    assert_eq!(
        history.history.len(),
        initial_commit_count,
        "Commit ledger count must remain unchanged"
    );
    assert!(
        history.history.is_empty(),
        "Commit ledger must have zero commits"
    );
    assert_eq!(
        history.current_rom, original_bytes,
        "Project current ROM buffer must remain unchanged"
    );
    assert_eq!(
        history.original_rom, original_bytes,
        "Project original ROM baseline must remain unchanged"
    );
}

#[tokio::test]
async fn test_rom_isolation_with_repeated_cycles_and_high_throughput() {
    let original_bytes = vec![0x55; 65536];
    let mem = MemoryMap::new(original_bytes.clone());
    let baseline_hashes = mem.hashes().clone();
    let history = ProjectHistory::from_memory_map(&mem);

    let sim = VirtualEcuSimulator::new_with_seed(999, SimMode::StreetTrack);
    sim.connect().await.expect("connect");

    let mut buffer = BoundedDatalogBuffer::with_capacity(50);
    buffer
        .start_recording(
            "HIGH-LOAD-CYCLE",
            "2026-10-24T12:00:00Z",
            None,
            None,
        )
        .expect("start recording");

    // Push 200 frames to cause multiple buffer wraps
    for _ in 0..200 {
        buffer.push_frame(sim.step_tick());
    }
    assert_eq!(buffer.len(), 50);
    assert_eq!(buffer.dropped_samples_count(), 150);
    buffer.stop_recording(None).expect("stop recording");

    let session = buffer.build_session();
    let csv = export_session_to_csv(&session).expect("export");
    let parsed = parse_session_from_str(&csv).expect("parse");
    let clock = Arc::new(MockClock::new(parsed.frames[0].timestamp_ms));
    let engine = PlaybackEngine::new_with_clock(Arc::new(parsed), clock.clone());

    engine.play().expect("play");
    for _ in 0..50 {
        clock.advance_ms(50);
        let _ = engine.step_next_frame();
    }
    engine.stop().expect("stop");
    sim.disconnect().await.expect("disconnect");

    // Assert ROM calibration integrity remains absolute
    assert_eq!(mem.as_slice(), original_bytes.as_slice());
    assert_eq!(mem.hashes(), &baseline_hashes);
    assert_eq!(history.history.len(), 0);
    assert_eq!(history.current_rom, original_bytes);
}
