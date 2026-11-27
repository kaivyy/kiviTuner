use kivi_protocol::clock::MockClock;
use kivi_protocol::datalogger::{DatalogSession, PlaybackEngine, PlaybackState, SessionMetadata};
use kivi_protocol::model::{TelemetryFrame, TelemetryStatusFlags};
use std::sync::Arc;

fn create_test_session(count: usize, start_ts: u64, interval_ms: u64) -> DatalogSession {
    let mut frames = Vec::with_capacity(count);
    for i in 0..count {
        let ts = start_ts + (i as u64) * interval_ms;
        frames.push(TelemetryFrame {
            timestamp_ms: ts,
            sequence: i as u64,
            is_simulated: true,
            rpm: 1000.0 + (i as f32) * 50.0,
            tps: (i as f32) * 2.0,
            map_kpa: 100.0,
            ect_c: 85.0,
            iat_c: 25.0,
            battery_volts: 14.0,
            ignition_advance_deg: 15.0,
            injector_pulse_width_ms: 3.0,
            afr: 14.7,
            speed_kph: (i as f32) * 5.0,
            gear: 1,
            status_flags: TelemetryStatusFlags::default(),
        });
    }
    let metadata = SessionMetadata::new("TEST-SESSION", "2026-10-24T12:00:00Z");
    DatalogSession::new(metadata, frames)
}

#[tokio::test]
async fn test_step_frame_progression_advancing_mock_clock() {
    let session = Arc::new(create_test_session(5, 1000, 50));
    let clock = Arc::new(MockClock::new(1000));
    let engine = PlaybackEngine::new_with_clock(session.clone(), clock.clone());

    let mut rx = engine.subscribe();
    let initial_status = engine.status();
    assert_eq!(initial_status.state, PlaybackState::Stopped);
    assert_eq!(initial_status.current_index, 0);
    assert_eq!(initial_status.current_time_ms, 1000);
    assert_eq!(initial_status.total_duration_ms, 200);
    assert_eq!(initial_status.total_samples, 5);

    // Step 1: advance mock clock by 50ms, then step_next_frame()
    clock.advance_ms(50);
    let f1 = engine.step_next_frame().expect("step 1");
    assert_eq!(f1.sequence, 1);
    assert_eq!(f1.timestamp_ms, 1050);
    assert_eq!(f1.rpm, 1050.0);
    assert_eq!(f1.sequence, session.frames[1].sequence);
    assert_eq!(f1.timestamp_ms, session.frames[1].timestamp_ms);

    let rx_f1 = rx.try_recv().expect("subscriber received frame 1");
    assert_eq!(rx_f1.sequence, 1);
    assert_eq!(rx_f1.timestamp_ms, 1050);

    let status1 = engine.status();
    assert_eq!(status1.current_index, 1);
    assert_eq!(status1.current_time_ms, 1050);

    // Step 2: advance mock clock by 50ms, then step_next_frame()
    clock.advance_ms(50);
    let f2 = engine.step_next_frame().expect("step 2");
    assert_eq!(f2.sequence, 2);
    assert_eq!(f2.timestamp_ms, 1100);

    let rx_f2 = rx.try_recv().expect("subscriber received frame 2");
    assert_eq!(rx_f2.sequence, 2);
    assert_eq!(rx_f2.timestamp_ms, 1100);

    let status2 = engine.status();
    assert_eq!(status2.current_index, 2);
    assert_eq!(status2.current_time_ms, 1100);
}

#[tokio::test]
async fn test_seeking_forward_and_backward() {
    let session = Arc::new(create_test_session(10, 0, 100));
    let clock = Arc::new(MockClock::new(0));
    let engine = PlaybackEngine::new_with_clock(session.clone(), clock.clone());

    let mut rx = engine.subscribe();

    // Seek forward to 500 ms
    engine.seek_ms(500).expect("seek to 500");
    let status = engine.status();
    assert_eq!(status.current_index, 5);
    assert_eq!(status.current_time_ms, 500);

    let frame = rx.try_recv().expect("immediate frame on seek to 500");
    assert_eq!(frame.sequence, 5);
    assert_eq!(frame.timestamp_ms, 500);

    // Seek forward between frames (e.g. 750 ms -> frame at 700 ms, index 7)
    engine.seek_ms(750).expect("seek to 750");
    let status = engine.status();
    assert_eq!(status.current_index, 7);
    assert_eq!(status.current_time_ms, 750);
    let frame = rx.try_recv().expect("immediate frame on seek to 750");
    assert_eq!(frame.sequence, 7);
    assert_eq!(frame.timestamp_ms, 700);

    // Seek backward to 200 ms
    engine.seek_ms(200).expect("seek backward to 200");
    let status = engine.status();
    assert_eq!(status.current_index, 2);
    assert_eq!(status.current_time_ms, 200);
    let frame = rx.try_recv().expect("immediate frame on backward seek");
    assert_eq!(frame.sequence, 2);
    assert_eq!(frame.timestamp_ms, 200);

    // Seek backward to start (0 ms)
    engine.seek_ms(0).expect("seek to 0");
    let status = engine.status();
    assert_eq!(status.current_index, 0);
    assert_eq!(status.current_time_ms, 0);
    let frame = rx.try_recv().expect("immediate frame on seek to 0");
    assert_eq!(frame.sequence, 0);
    assert_eq!(frame.timestamp_ms, 0);
}

#[tokio::test]
async fn test_speed_scaling_proportional_under_mock_clock() {
    let session = Arc::new(create_test_session(100, 0, 100));
    let clock = Arc::new(MockClock::new(1000));
    let engine = PlaybackEngine::new_with_clock(session.clone(), clock.clone());

    engine.play().expect("play");
    assert_eq!(engine.status().state, PlaybackState::Playing);
    assert_eq!(engine.status().speed, 1.0);

    // At 1.0x: advance mock clock by 1000 ms -> virtual time advances 1000 ms
    clock.advance_ms(1000);
    let s1 = engine.status();
    assert_eq!(s1.current_time_ms, 1000);

    // Change speed to 0.5x
    engine.set_speed(0.5).expect("set speed 0.5");
    assert_eq!(engine.status().speed, 0.5);

    // Clock advances by 1000 ms -> virtual time advances 1000 * 0.5 = 500 ms (total: 1500 ms)
    clock.advance_ms(1000);
    let s2 = engine.status();
    assert_eq!(s2.current_time_ms, 1500);

    // Change speed to 2.0x
    engine.set_speed(2.0).expect("set speed 2.0");
    assert_eq!(engine.status().speed, 2.0);

    // Clock advances by 1000 ms -> virtual time advances 1000 * 2.0 = 2000 ms (total: 3500 ms)
    clock.advance_ms(1000);
    let s3 = engine.status();
    assert_eq!(s3.current_time_ms, 3500);

    // Change speed to 4.0x
    engine.set_speed(4.0).expect("set speed 4.0");
    assert_eq!(engine.status().speed, 4.0);

    // Clock advances by 1000 ms -> virtual time advances 1000 * 4.0 = 4000 ms (total: 7500 ms)
    clock.advance_ms(1000);
    let s4 = engine.status();
    assert_eq!(s4.current_time_ms, 7500);

    // Clamping checks: speed < 0.1 clamped to 0.1, speed > 10.0 clamped to 10.0
    engine.set_speed(0.01).expect("clamp low");
    assert_eq!(engine.status().speed, 0.1);

    engine.set_speed(50.0).expect("clamp high");
    assert_eq!(engine.status().speed, 10.0);
}

#[tokio::test]
async fn test_end_of_session_transitions_to_stopped_when_loop_false() {
    let session = Arc::new(create_test_session(3, 0, 100));
    let clock = Arc::new(MockClock::new(0));
    let engine = PlaybackEngine::new_with_clock(session.clone(), clock.clone());

    assert!(!engine.status().loop_playback);
    assert_eq!(engine.status().current_index, 0);

    let f1 = engine.step_next_frame().expect("step to 1");
    assert_eq!(f1.sequence, 1);
    assert_eq!(engine.status().current_index, 1);

    let f2 = engine.step_next_frame().expect("step to 2");
    assert_eq!(f2.sequence, 2);
    assert_eq!(engine.status().current_index, 2);

    // Stepping past the last frame must transition to Stopped and return None
    let f_end = engine.step_next_frame();
    assert!(f_end.is_none());

    let status = engine.status();
    assert_eq!(status.state, PlaybackState::Stopped);
    assert_eq!(status.current_index, 0);
    assert_eq!(status.current_time_ms, 0);

    // Also verify under clock advancement when Playing:
    engine.play().expect("play again");
    assert_eq!(engine.status().state, PlaybackState::Playing);

    // Advance clock past the total duration (200 ms)
    clock.advance_ms(250);
    // status() is strictly read-only: it does not mutate state to Stopped prematurely
    let status_mid = engine.status();
    assert_eq!(status_mid.state, PlaybackState::Playing);
    assert_eq!(status_mid.current_time_ms, 200);

    // State transition to Stopped and tail frame emission belongs strictly to tick()
    let continued = engine.tick();
    assert!(!continued);

    let status_after = engine.status();
    assert_eq!(status_after.state, PlaybackState::Stopped);
    assert_eq!(status_after.current_index, 0);
    assert_eq!(status_after.current_time_ms, 0);
}

#[tokio::test]
async fn test_end_of_session_loops_when_loop_true() {
    let session = Arc::new(create_test_session(3, 0, 100));
    let clock = Arc::new(MockClock::new(0));
    let engine = PlaybackEngine::new_with_clock(session.clone(), clock.clone());

    engine.set_loop(true);
    assert!(engine.status().loop_playback);

    engine.step_next_frame().expect("step 1");
    engine.step_next_frame().expect("step 2");
    assert_eq!(engine.status().current_index, 2);

    // Stepping past index 2 must wrap around to index 0
    let f_wrap = engine.step_next_frame().expect("wrap to index 0");
    assert_eq!(f_wrap.sequence, 0);
    assert_eq!(f_wrap.timestamp_ms, 0);
    assert_eq!(engine.status().current_index, 0);

    // Also verify under clock advancement when Playing:
    engine.play().expect("play");
    // Clock at 0; advance by 250 ms (200 ms duration -> wraps by 50 ms)
    clock.advance_ms(250);
    let status = engine.status();
    assert_eq!(status.state, PlaybackState::Playing);
    assert_eq!(status.current_time_ms, 50);
}

#[tokio::test]
async fn test_dropping_playback_engine_terminates_background_tasks_cleanly() {
    let session = Arc::new(create_test_session(100, 0, 50));
    let clock = Arc::new(MockClock::new(0));
    let engine = PlaybackEngine::new_with_clock(session.clone(), clock.clone());

    let mut rx = engine.subscribe();
    engine.play().expect("play");
    assert_eq!(engine.status().state, PlaybackState::Playing);

    // Initial frame emitted when play starts
    let init_frame = rx.try_recv().expect("initial frame on play");
    assert_eq!(init_frame.sequence, 0);

    drop(engine);

    // Channel sender inside engine inner must be dropped, closing rx
    let res = rx.recv().await;
    assert_eq!(
        res,
        Err(tokio::sync::broadcast::error::RecvError::Closed),
        "Expected broadcast channel to close upon engine drop"
    );
}

#[tokio::test]
async fn test_pause_resume_and_stop_lifecycle() {
    let session = Arc::new(create_test_session(20, 1000, 100));
    let clock = Arc::new(MockClock::new(5000));
    let engine = PlaybackEngine::new_with_clock(session.clone(), clock.clone());

    // Initially Stopped
    assert_eq!(engine.status().state, PlaybackState::Stopped);
    assert_eq!(engine.status().current_time_ms, 1000);

    // Start playback
    engine.play().expect("play");
    assert_eq!(engine.status().state, PlaybackState::Playing);

    // Advance 500 ms
    clock.advance_ms(500);
    assert_eq!(engine.status().current_time_ms, 1500);

    // Pause playback
    engine.pause().expect("pause");
    assert_eq!(engine.status().state, PlaybackState::Paused);
    assert_eq!(engine.status().current_time_ms, 1500);

    // Clock advancing while paused should NOT advance playback time
    clock.advance_ms(1000);
    assert_eq!(engine.status().current_time_ms, 1500);

    // Resume playback
    engine.resume().expect("resume");
    assert_eq!(engine.status().state, PlaybackState::Playing);

    // Clock advancing resumes virtual progress from paused timestamp
    clock.advance_ms(300);
    assert_eq!(engine.status().current_time_ms, 1800);

    // Stop playback
    engine.stop().expect("stop");
    assert_eq!(engine.status().state, PlaybackState::Stopped);
    assert_eq!(engine.status().current_index, 0);
    assert_eq!(engine.status().current_time_ms, 1000);
}

#[tokio::test]
async fn test_empty_session_edge_cases() {
    let empty_session = Arc::new(DatalogSession::new(
        SessionMetadata::new("EMPTY", "2026-10-24T12:00:00Z"),
        vec![],
    ));
    let clock = Arc::new(MockClock::new(0));
    let engine = PlaybackEngine::new_with_clock(empty_session, clock);

    let status = engine.status();
    assert_eq!(status.state, PlaybackState::Stopped);
    assert_eq!(status.total_samples, 0);
    assert_eq!(status.total_duration_ms, 0);

    assert!(engine.play().is_err());
    assert!(engine.pause().is_err());
    assert!(engine.resume().is_err());
    assert!(engine.seek_ms(100).is_err());
    assert!(engine.step_next_frame().is_none());
}

#[tokio::test]
async fn test_seeking_edge_cases_out_of_bounds() {
    // Frames at 1000, 2000, 3000
    let session = Arc::new(create_test_session(3, 1000, 1000));
    let clock = Arc::new(MockClock::new(0));
    let engine = PlaybackEngine::new_with_clock(session.clone(), clock);

    let mut rx = engine.subscribe();

    // Seek before first frame (target = 500 ms) -> clamps to first frame (1000 ms, idx 0)
    engine.seek_ms(500).expect("seek before first");
    let status = engine.status();
    assert_eq!(status.current_index, 0);
    assert_eq!(status.current_time_ms, 1000);
    let f = rx.try_recv().expect("rx frame");
    assert_eq!(f.timestamp_ms, 1000);

    // Seek after last frame (target = 9999 ms) -> clamps to last frame (3000 ms, idx 2)
    engine.seek_ms(9999).expect("seek after last");
    let status = engine.status();
    assert_eq!(status.current_index, 2);
    assert_eq!(status.current_time_ms, 3000);
    let f = rx.try_recv().expect("rx frame");
    assert_eq!(f.timestamp_ms, 3000);
}

#[tokio::test]
async fn test_speed_validation_and_clamping() {
    let session = Arc::new(create_test_session(5, 0, 100));
    let clock = Arc::new(MockClock::new(0));
    let engine = PlaybackEngine::new_with_clock(session, clock);

    // Negative and zero speeds are rejected
    assert!(engine.set_speed(0.0).is_err());
    assert!(engine.set_speed(-1.5).is_err());
    assert!(engine.set_speed(f32::NAN).is_err());

    // Below 0.1 clamped to 0.1
    engine.set_speed(0.05).expect("valid clamp");
    assert_eq!(engine.status().speed, 0.1);

    // Above 10.0 clamped to 10.0
    engine.set_speed(15.0).expect("valid clamp");
    assert_eq!(engine.status().speed, 10.0);
}

#[tokio::test]
async fn test_playback_telemetry_source_trait() {
    use kivi_protocol::driver::TelemetrySource;

    let session = Arc::new(create_test_session(5, 0, 100));
    let engine = PlaybackEngine::new(session.clone());
    let source: Arc<dyn TelemetrySource> = Arc::new(engine.clone());

    let mut rx = source.subscribe_telemetry();
    let stepped = engine.step_next_frame().expect("step frame");
    let received = rx.recv().await.expect("recv via TelemetrySource trait");
    assert_eq!(stepped.sequence, received.sequence);
    assert_eq!(stepped.timestamp_ms, received.timestamp_ms);
}

#[test]
fn test_playback_state_and_status_serde() {
    let states = [PlaybackState::Stopped, PlaybackState::Playing, PlaybackState::Paused];
    for state in states {
        let json = serde_json::to_string(&state).expect("serialize state");
        let de: PlaybackState = serde_json::from_str(&json).expect("deserialize state");
        assert_eq!(state, de);
    }

    let status = kivi_protocol::datalogger::PlaybackStatus {
        state: PlaybackState::Playing,
        current_time_ms: 1250,
        total_duration_ms: 5000,
        current_index: 25,
        total_samples: 100,
        speed: 2.0,
        loop_playback: true,
    };
    let json = serde_json::to_string(&status).expect("serialize status");
    let de: kivi_protocol::datalogger::PlaybackStatus = serde_json::from_str(&json).expect("deserialize status");
    assert_eq!(status, de);
}

#[tokio::test]
async fn test_interleaving_status_queries_with_tick_does_not_drop_frames() {
    // 5 frames at timestamps 1000, 1100, 1200, 1300, 1400 (intervals of 100ms)
    let session = Arc::new(create_test_session(5, 1000, 100));
    let clock = Arc::new(MockClock::new(1000));
    let engine = PlaybackEngine::new_with_clock(session.clone(), clock.clone());

    let mut rx = engine.subscribe();

    // Start playback (emits initial frame 0 at ts = 1000)
    engine.play().expect("play");
    assert_eq!(engine.status().state, PlaybackState::Playing);

    let init_frame = rx.try_recv().expect("initial frame 0 received on play");
    assert_eq!(init_frame.sequence, 0);
    assert_eq!(init_frame.timestamp_ms, 1000);

    let mut received_sequences = vec![init_frame.sequence];

    // For intervals leading to frames 1, 2, 3:
    // advance clock incrementally and query engine.status() multiple times
    for expected_seq in 1..=3 {
        // Interleaved sub-advancements: advance 30ms, query status
        clock.advance_ms(30);
        let s1 = engine.status();
        assert_eq!(s1.state, PlaybackState::Playing);
        assert!(rx.try_recv().is_err(), "no frame emitted just by status() call");

        // Advance another 30ms, query status again
        clock.advance_ms(30);
        let s2 = engine.status();
        assert_eq!(s2.state, PlaybackState::Playing);
        assert!(rx.try_recv().is_err(), "no frame emitted just by status() call");

        // Advance remaining 40ms to complete the 100ms interval
        clock.advance_ms(40);
        let s3 = engine.status();
        assert_eq!(s3.state, PlaybackState::Playing);
        assert!(rx.try_recv().is_err(), "no frame emitted before tick()");

        // Execute tick()
        let should_continue = engine.tick();
        assert!(should_continue);

        // Frame must be received
        let frame = rx
            .try_recv()
            .unwrap_or_else(|_| panic!("expected frame sequence {}", expected_seq));
        assert_eq!(frame.sequence, expected_seq);
        received_sequences.push(frame.sequence);

        // No spurious duplicate frames
        assert!(rx.try_recv().is_err(), "no extra frame emitted");

        // Status reflects the newly ticked index
        let s4 = engine.status();
        assert_eq!(s4.current_index, expected_seq as usize);
    }

    // Now test reaching and exceeding session end (frame 4 at 1400ms):
    // Advance 100ms (to 1400ms) + another 50ms past duration (to 1450ms)
    clock.advance_ms(150);

    // Query status() multiple times while past duration
    let s_past1 = engine.status();
    assert_eq!(
        s_past1.state,
        PlaybackState::Playing,
        "status() must not transition to Stopped prematurely"
    );
    assert_eq!(s_past1.current_time_ms, 1400);
    assert_eq!(s_past1.current_index, 4);
    assert!(rx.try_recv().is_err(), "status() must not emit tail frames");

    let s_past2 = engine.status();
    assert_eq!(s_past2.state, PlaybackState::Playing);

    // Now tick(): tick() must emit the tail frame (frame 4) and transition to Stopped
    let should_continue = engine.tick();
    assert!(!should_continue);

    let tail_frame = rx.try_recv().expect("tail frame received on tick at end");
    assert_eq!(tail_frame.sequence, 4);
    assert_eq!(tail_frame.timestamp_ms, 1400);
    received_sequences.push(tail_frame.sequence);

    // No extra frames
    assert!(rx.try_recv().is_err());

    // After tick, status is Stopped
    let s_stopped = engine.status();
    assert_eq!(s_stopped.state, PlaybackState::Stopped);
    assert_eq!(s_stopped.current_index, 0);

    // Verify all frames 0..=4 were received with none dropped or skipped
    assert_eq!(received_sequences, vec![0, 1, 2, 3, 4]);
}

#[tokio::test]
async fn test_interleaving_status_queries_with_tick_loop_wrap_around() {
    // 3 frames: timestamps 0, 100, 200
    let session = Arc::new(create_test_session(3, 0, 100));
    let clock = Arc::new(MockClock::new(0));
    let engine = PlaybackEngine::new_with_clock(session.clone(), clock.clone());
    engine.set_loop(true);

    let mut rx = engine.subscribe();
    engine.play().expect("play");
    let init_frame = rx.try_recv().expect("frame 0");
    assert_eq!(init_frame.sequence, 0);

    let mut received = vec![init_frame.sequence];

    // Tick to frame 1 (100ms) with interleaved status queries
    clock.advance_ms(50);
    let _ = engine.status();
    clock.advance_ms(50);
    let _ = engine.status();
    assert!(engine.tick());
    let f1 = rx.try_recv().expect("frame 1");
    assert_eq!(f1.sequence, 1);
    received.push(f1.sequence);

    // Tick across wrap boundary (advance by 150ms -> 250ms, wrapping to 50ms)
    clock.advance_ms(50);
    let _ = engine.status();
    clock.advance_ms(100);
    // status() query across boundary
    let s_wrapped = engine.status();
    assert_eq!(s_wrapped.state, PlaybackState::Playing);
    assert_eq!(s_wrapped.current_time_ms, 50);

    // tick() wraps and emits frame 2 (end of cycle 1) and frame 0 (start of cycle 2)
    assert!(engine.tick());
    let f2 = rx.try_recv().expect("frame 2 tail");
    assert_eq!(f2.sequence, 2);
    received.push(f2.sequence);

    let f0_loop = rx.try_recv().expect("frame 0 wrapped");
    assert_eq!(f0_loop.sequence, 0);
    received.push(f0_loop.sequence);

    assert_eq!(received, vec![0, 1, 2, 0]);
}
