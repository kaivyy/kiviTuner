use kivi_protocol::datalogger::{
    BoundedDatalogBuffer, DatalogSession, RecordingState, SessionMetadata,
    DEFAULT_BUFFER_CAPACITY,
};
use kivi_protocol::{EcuIdentity, ProtocolError, TelemetryFrame, TelemetryStatusFlags};

fn make_test_frame(seq: u64, timestamp_ms: u64) -> TelemetryFrame {
    TelemetryFrame {
        timestamp_ms,
        sequence: seq,
        is_simulated: true,
        rpm: 3000.0 + (seq as f32 * 10.0),
        tps: 45.0,
        map_kpa: 105.0,
        ect_c: 85.0,
        iat_c: 30.0,
        battery_volts: 13.8,
        ignition_advance_deg: 18.0,
        injector_pulse_width_ms: 4.5,
        afr: 14.1,
        speed_kph: 60.0,
        gear: 3,
        status_flags: TelemetryStatusFlags::default(),
    }
}

#[test]
fn test_default_buffer_capacity_and_initial_state() {
    let buffer = BoundedDatalogBuffer::new();
    assert_eq!(buffer.capacity(), DEFAULT_BUFFER_CAPACITY);
    assert_eq!(buffer.capacity(), 100_000);
    assert_eq!(*buffer.state(), RecordingState::Idle);
    assert_eq!(buffer.len(), 0);
    assert!(buffer.is_empty());
    assert_eq!(buffer.dropped_samples_count(), 0);
    assert_eq!(buffer.total_samples_recorded(), 0);
}

#[test]
fn test_capacity_capping_and_fifo_eviction() {
    let mut buffer = BoundedDatalogBuffer::with_capacity(100);
    assert_eq!(buffer.capacity(), 100);

    // Must be in Recording state
    buffer
        .transition_to(RecordingState::Recording)
        .expect("Idle -> Recording should succeed");
    assert_eq!(*buffer.state(), RecordingState::Recording);

    // Push 100 frames - all should fit without eviction
    for i in 0..100 {
        let stored_without_eviction = buffer.push_frame(make_test_frame(i, i * 50));
        assert!(
            stored_without_eviction,
            "Frame {} should store without eviction",
            i
        );
    }

    assert_eq!(buffer.len(), 100);
    assert_eq!(buffer.total_samples_recorded(), 100);
    assert_eq!(buffer.dropped_samples_count(), 0);

    // Push 25 additional frames - each should trigger eviction of oldest frame
    for i in 100..125 {
        let stored_without_eviction = buffer.push_frame(make_test_frame(i, i * 50));
        assert!(
            !stored_without_eviction,
            "Frame {} should evict oldest frame",
            i
        );
    }

    // Buffer capped at 100 frames
    assert_eq!(buffer.len(), 100);
    assert_eq!(buffer.total_samples_recorded(), 125);
    assert_eq!(buffer.dropped_samples_count(), 25);

    // Verify FIFO circular order: first frame is sequence 25, last is sequence 124
    let session = buffer.build_session();
    assert_eq!(session.frames.len(), 100);
    assert_eq!(session.frames.first().unwrap().sequence, 25);
    assert_eq!(session.frames.last().unwrap().sequence, 124);
}

#[test]
fn test_dropped_samples_accounting_with_backpressure() {
    let mut buffer = BoundedDatalogBuffer::with_capacity(50);
    buffer
        .transition_to(RecordingState::Recording)
        .expect("Transition to Recording");

    // Simulate lag/backpressure drop
    buffer.record_dropped_samples(7);
    assert_eq!(buffer.dropped_samples_count(), 7);
    assert_eq!(buffer.metadata().dropped_samples_count, 7);

    // Push 60 frames (capacity 50 -> 10 evicted)
    for i in 0..60 {
        buffer.push_frame(make_test_frame(i, i * 50));
    }

    // Dropped count tracks both backpressure (7) + eviction (10) = 17
    assert_eq!(buffer.dropped_samples_count(), 17);
    assert_eq!(buffer.metadata().dropped_samples_count, 17);
    assert_eq!(buffer.total_samples_recorded(), 60);
    assert_eq!(buffer.len(), 50);
}

#[test]
fn test_legal_recording_state_lifecycle() {
    let mut buffer = BoundedDatalogBuffer::with_capacity(100);
    assert_eq!(*buffer.state(), RecordingState::Idle);

    // Idle -> Recording
    buffer
        .transition_to(RecordingState::Recording)
        .expect("Idle -> Recording");
    assert_eq!(*buffer.state(), RecordingState::Recording);

    // Recording -> Stopping
    buffer
        .transition_to(RecordingState::Stopping)
        .expect("Recording -> Stopping");
    assert_eq!(*buffer.state(), RecordingState::Stopping);

    // Stopping -> ReadyToSave
    buffer
        .transition_to(RecordingState::ReadyToSave)
        .expect("Stopping -> ReadyToSave");
    assert_eq!(*buffer.state(), RecordingState::ReadyToSave);

    // ReadyToSave -> Saved
    buffer
        .transition_to(RecordingState::Saved)
        .expect("ReadyToSave -> Saved");
    assert_eq!(*buffer.state(), RecordingState::Saved);

    // Saved -> Idle (reset)
    buffer
        .transition_to(RecordingState::Idle)
        .expect("Saved -> Idle");
    assert_eq!(*buffer.state(), RecordingState::Idle);
}

#[test]
fn test_interrupted_state_preserves_frames() {
    let mut buffer = BoundedDatalogBuffer::with_capacity(100);
    buffer
        .transition_to(RecordingState::Recording)
        .expect("Start recording");

    for i in 0..42 {
        buffer.push_frame(make_test_frame(i, i * 50));
    }
    assert_eq!(buffer.len(), 42);

    // Simulate sudden disconnect during recording
    buffer
        .transition_to(RecordingState::Interrupted {
            reason: "Serial link severed".to_string(),
            recoverable_samples: 0,
        })
        .expect("Recording -> Interrupted");

    match buffer.state() {
        RecordingState::Interrupted {
            reason,
            recoverable_samples,
        } => {
            assert_eq!(reason, "Serial link severed");
            assert_eq!(*recoverable_samples, 42);
        }
        other => panic!("Expected Interrupted state, got {:?}", other),
    }

    // Crucial requirement: frames MUST NOT be cleared upon interruption
    assert_eq!(buffer.len(), 42);
    let session = buffer.build_session();
    assert_eq!(session.frames.len(), 42);
    assert_eq!(session.frames.last().unwrap().sequence, 41);

    // User acknowledges / auto-recovery: Interrupted -> ReadyToSave
    buffer
        .transition_to(RecordingState::ReadyToSave)
        .expect("Interrupted -> ReadyToSave");
    assert_eq!(*buffer.state(), RecordingState::ReadyToSave);
    assert_eq!(buffer.len(), 42);
}

#[test]
fn test_rejection_of_invalid_state_transitions() {
    let mut buffer = BoundedDatalogBuffer::with_capacity(100);

    // From Idle: cannot jump to Stopping, ReadyToSave, Saved, Interrupted
    let err = buffer
        .transition_to(RecordingState::Stopping)
        .expect_err("Idle -> Stopping must be rejected");
    assert!(matches!(err, ProtocolError::InvalidStateTransition(_)));

    let err = buffer
        .transition_to(RecordingState::ReadyToSave)
        .expect_err("Idle -> ReadyToSave must be rejected");
    assert!(matches!(err, ProtocolError::InvalidStateTransition(_)));

    let err = buffer
        .transition_to(RecordingState::Saved)
        .expect_err("Idle -> Saved must be rejected");
    assert!(matches!(err, ProtocolError::InvalidStateTransition(_)));

    let err = buffer
        .transition_to(RecordingState::Interrupted {
            reason: "test".to_string(),
            recoverable_samples: 0,
        })
        .expect_err("Idle -> Interrupted must be rejected");
    assert!(matches!(err, ProtocolError::InvalidStateTransition(_)));

    // Transition Idle -> Recording
    buffer.transition_to(RecordingState::Recording).unwrap();

    // From Recording: cannot start recording again (concurrent recording guard)
    let err = buffer
        .transition_to(RecordingState::Recording)
        .expect_err("Recording -> Recording must be rejected");
    assert!(matches!(err, ProtocolError::InvalidStateTransition(_)));

    // From Recording: cannot jump straight to Saved or Idle
    let err = buffer
        .transition_to(RecordingState::Saved)
        .expect_err("Recording -> Saved must be rejected");
    assert!(matches!(err, ProtocolError::InvalidStateTransition(_)));

    let err = buffer
        .transition_to(RecordingState::Idle)
        .expect_err("Recording -> Idle must be rejected");
    assert!(matches!(err, ProtocolError::InvalidStateTransition(_)));

    // Transition Recording -> Interrupted
    buffer
        .transition_to(RecordingState::Interrupted {
            reason: "timeout".to_string(),
            recoverable_samples: 0,
        })
        .unwrap();

    // From Interrupted: cannot transition to Recording directly without saving/discarding
    let err = buffer
        .transition_to(RecordingState::Recording)
        .expect_err("Interrupted -> Recording must be rejected");
    assert!(matches!(err, ProtocolError::InvalidStateTransition(_)));

    let err = buffer
        .transition_to(RecordingState::Stopping)
        .expect_err("Interrupted -> Stopping must be rejected");
    assert!(matches!(err, ProtocolError::InvalidStateTransition(_)));
}

#[test]
fn test_session_metadata_and_datalog_session_serde() {
    let ecu = EcuIdentity {
        ecu_id: "SIM-BENCH-01".to_string(),
        vin: Some("1HGCR2F8XHA000000".to_string()),
        software_version: "v1.2.3".to_string(),
        hardware_version: "v4.0".to_string(),
        protocol_name: "VirtualBench".to_string(),
        is_simulated: true,
    };

    let mut metadata = SessionMetadata::new("ses-20261024-001", "2026-10-24T18:00:00Z");
    metadata.ended_at_iso = Some("2026-10-24T18:01:30Z".to_string());
    metadata.ecu_identity = Some(ecu);
    metadata.notes = "Dyno Pull 3rd Gear WOT".to_string();
    metadata.total_samples_recorded = 100;
    metadata.dropped_samples_count = 2;

    assert_eq!(metadata.schema_version, 1);
    assert_eq!(metadata.sample_rate_hz, 20.0);

    let frames = vec![
        make_test_frame(0, 1000),
        make_test_frame(1, 1050),
        make_test_frame(2, 1100),
    ];

    let session = DatalogSession::new(metadata.clone(), frames);
    assert_eq!(session.sample_count(), 3);
    assert_eq!(session.duration_ms(), 100);
    assert!(!session.is_empty());

    // Serde roundtrip for SessionMetadata
    let json_meta = serde_json::to_string(&metadata).expect("Serialize metadata");
    let deserialized_meta: SessionMetadata =
        serde_json::from_str(&json_meta).expect("Deserialize metadata");
    assert_eq!(metadata, deserialized_meta);

    // Serde roundtrip for DatalogSession
    let json_session = serde_json::to_string(&session).expect("Serialize session");
    let deserialized_session: DatalogSession =
        serde_json::from_str(&json_session).expect("Deserialize session");
    assert_eq!(session, deserialized_session);

    // Serde roundtrip for RecordingState
    let states = vec![
        RecordingState::Idle,
        RecordingState::Recording,
        RecordingState::Stopping,
        RecordingState::ReadyToSave,
        RecordingState::Saved,
        RecordingState::Interrupted {
            reason: "Cable disconnect".to_string(),
            recoverable_samples: 42,
        },
    ];

    for state in states {
        let json_state = serde_json::to_string(&state).expect("Serialize state");
        let deserialized_state: RecordingState =
            serde_json::from_str(&json_state).expect("Deserialize state");
        assert_eq!(state, deserialized_state);
    }
}
