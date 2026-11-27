use kivi_protocol::obd::command::{Mode01Pid, ObdCommand};
use kivi_protocol::obd::errors::ObdCodecError;
use kivi_protocol::obd::mock::MockObdTransport;
use kivi_protocol::obd::session::{ObdSession, ObdSessionConfig, ObdSessionState};
use kivi_protocol::obd::transport::ObdTransport;
use std::sync::Arc;
use std::time::Duration;


// ===========================================================================
// SECTION 2: Timeout, Cancellation, and Stale Data Recovery Tests
// ===========================================================================

#[tokio::test]
async fn test_transaction_timeout_transitions_to_faulted_and_blocks_next_request() {
    let transport = MockObdTransport::new();
    transport.script_standard_handshake();

    let config = ObdSessionConfig {
        command_timeout: Duration::from_millis(100),
        ..Default::default()
    };
    let mut session = ObdSession::new(transport.clone(), config);

    session.open().await.unwrap();
    session.initialize().await.unwrap();

    // Now inject read delay of 300ms, with timeout configured to 100ms
    transport.inject_read_delay(Duration::from_millis(300));

    // Transaction should timeout
    let err = session.query_pid(Mode01Pid::EngineRpm).await.unwrap_err();
    assert!(matches!(err, ObdCodecError::Timeout(100)));

    // State MUST be Faulted
    match session.state() {
        ObdSessionState::Faulted { reason } => {
            assert!(reason.contains("timed out"));
        }
        _ => panic!("Expected Faulted state after transaction timeout"),
    }

    // Subsequent request MUST be rejected immediately to prevent stale data corruption
    let subsequent_err = session
        .query_pid(Mode01Pid::VehicleSpeed)
        .await
        .unwrap_err();
    assert!(matches!(subsequent_err, ObdCodecError::SessionFaulted(_)));
}

#[tokio::test]
async fn test_late_response_after_timeout_cannot_contaminate_subsequent_request() {
    let transport = MockObdTransport::new();
    transport.script_standard_handshake();

    let config = ObdSessionConfig {
        command_timeout: Duration::from_millis(50),
        ..Default::default()
    };
    let mut session = ObdSession::new(transport.clone(), config);
    session.open().await.unwrap();
    session.initialize().await.unwrap();

    // Script 1st command: RPM -> delayed response
    transport.script_response(
        ObdCommand::Mode01(Mode01Pid::EngineRpm),
        Ok("41 0C 1A F8\r\r>".to_string()),
    );
    // Script 2nd command: Speed -> expected response
    transport.script_response(
        ObdCommand::Mode01(Mode01Pid::VehicleSpeed),
        Ok("41 0D 32\r\r>".to_string()),
    );

    // Inject 150ms read delay so the 50ms command_timeout triggers first
    transport.inject_read_delay(Duration::from_millis(150));

    // 1. First transaction times out
    let timeout_err = session.query_pid(Mode01Pid::EngineRpm).await.unwrap_err();
    assert!(matches!(timeout_err, ObdCodecError::Timeout(50)));

    // Session MUST be in Faulted state
    match session.state() {
        ObdSessionState::Faulted { reason } => {
            assert!(reason.contains("timed out"));
        }
        other => panic!("Expected Faulted state, got: {:?}", other),
    }

    // Both scripted responses must still remain unconsumed in the transport queue
    assert_eq!(transport.remaining_scripted_responses(), 2);

    // Clear read delay to simulate late arrival of bytes on the transport
    transport.clear_read_delay();

    // 2. Subsequent request on the same session MUST fail closed immediately with SessionFaulted
    let next_err = session
        .query_pid(Mode01Pid::VehicleSpeed)
        .await
        .unwrap_err();
    match next_err {
        ObdCodecError::SessionFaulted(reason) => {
            assert!(reason.contains("timed out"));
        }
        other => panic!("Expected SessionFaulted error, got: {:?}", other),
    }

    // CRITICAL PROOF: The delayed RPM response was NEVER popped or misattributed to VehicleSpeed!
    assert_eq!(transport.remaining_scripted_responses(), 2);

    // The transport only ever received the first command (EngineRpm); VehicleSpeed was never sent
    let written = transport.written_commands();
    assert_eq!(
        written.last(),
        Some(&ObdCommand::Mode01(Mode01Pid::EngineRpm))
    );
    assert!(!written.contains(&ObdCommand::Mode01(Mode01Pid::VehicleSpeed)));
}

#[tokio::test]
async fn test_transport_disconnect_during_read_transitions_to_faulted() {
    let transport = MockObdTransport::new();
    transport.script_standard_handshake();

    let mut session = ObdSession::new(transport.clone(), ObdSessionConfig::default());
    session.open().await.unwrap();
    session.initialize().await.unwrap();

    // Now inject transport disconnect
    transport.inject_disconnect_on_read();

    let err = session.query_pid(Mode01Pid::EngineRpm).await.unwrap_err();
    assert!(matches!(err, ObdCodecError::IoError(_)));

    assert!(matches!(session.state(), ObdSessionState::Faulted { .. }));
}

#[tokio::test]
async fn test_oversized_adapter_response_rejected() {
    let transport = MockObdTransport::new();
    transport.script_standard_handshake();

    // Script response exceeding the maximum configured 128 bytes
    let huge_resp = "41 0C ".to_string() + &"1A ".repeat(200) + "\r\r>";
    transport.script_response(ObdCommand::Mode01(Mode01Pid::EngineRpm), Ok(huge_resp));

    let config = ObdSessionConfig {
        max_response_bytes: 128,
        ..Default::default()
    };
    let mut session = ObdSession::new(transport, config);
    session.open().await.unwrap();
    session.initialize().await.unwrap();

    let err = session.query_pid(Mode01Pid::EngineRpm).await.unwrap_err();
    assert!(matches!(err, ObdCodecError::OversizedPayload { .. }));
}

#[tokio::test]
async fn test_cancellation_during_write_command_transitions_to_faulted() {
    let transport = MockObdTransport::new();
    transport.script_standard_handshake();

    let mut session = ObdSession::new(
        transport.clone(),
        ObdSessionConfig {
            command_timeout: Duration::from_millis(1000),
            ..Default::default()
        },
    );

    session.open().await.unwrap();
    session.initialize().await.unwrap();

    // Inject write delay of 500ms
    transport.inject_write_delay(Duration::from_millis(500));
    transport.script_response(
        ObdCommand::Mode01(Mode01Pid::EngineRpm),
        Ok("41 0C 1A F8\r\r>".to_string()),
    );

    // Call query_pid with a timeout of 20ms to cancel during write_command
    let timeout_res = tokio::time::timeout(
        Duration::from_millis(20),
        session.query_pid(Mode01Pid::EngineRpm),
    )
    .await;

    assert!(
        timeout_res.is_err(),
        "Expected caller timeout to drop future during write"
    );

    // PROOF: TransactionGuard::drop executed, clearing in_transaction and transitioning state to Faulted
    match session.state() {
        ObdSessionState::Faulted { reason } => {
            assert!(
                reason.contains("cancelled or dropped"),
                "Expected cancellation reason, got: {}",
                reason
            );
        }
        other => panic!(
            "Expected Faulted state after cancellation during write, got: {:?}",
            other
        ),
    }

    // Subsequent command must return SessionFaulted, NOT SessionBusy
    let next_err = session
        .query_pid(Mode01Pid::VehicleSpeed)
        .await
        .unwrap_err();
    assert!(matches!(next_err, ObdCodecError::SessionFaulted(_)));
}

#[tokio::test]
async fn test_write_failure_transitions_to_faulted_and_blocks_session_reuse() {
    let transport = MockObdTransport::new();
    transport.script_standard_handshake();

    let mut session = ObdSession::new(transport.clone(), ObdSessionConfig::default());
    session.open().await.unwrap();
    session.initialize().await.unwrap();

    // Inject write failure
    transport.inject_fail_write(ObdCodecError::IoError(
        "hardware port transmit failed".to_string(),
    ));

    let err = session.query_pid(Mode01Pid::EngineRpm).await.unwrap_err();
    assert!(matches!(err, ObdCodecError::IoError(_)));

    // Session must be in Faulted state
    match session.state() {
        ObdSessionState::Faulted { reason } => {
            assert!(reason.contains("write command failed"));
        }
        other => panic!(
            "Expected Faulted state after write failure, got: {:?}",
            other
        ),
    }

    // Subsequent query must fail closed with SessionFaulted
    let next_err = session
        .query_pid(Mode01Pid::VehicleSpeed)
        .await
        .unwrap_err();
    assert!(matches!(next_err, ObdCodecError::SessionFaulted(_)));
}

#[tokio::test]
async fn test_close_resets_faulted_session_to_disconnected() {
    let transport = MockObdTransport::new();
    transport.script_standard_handshake();

    let mut session = ObdSession::new(transport.clone(), ObdSessionConfig::default());
    session.open().await.unwrap();
    session.initialize().await.unwrap();

    // Fault the session by injecting write failure
    transport.inject_fail_write(ObdCodecError::IoError("hardware write error".to_string()));
    let _ = session.query_pid(Mode01Pid::EngineRpm).await;
    assert!(matches!(session.state(), ObdSessionState::Faulted { .. }));

    // Closing the faulted session must release transport resources and cleanly transition to Disconnected
    session
        .close()
        .await
        .expect("Failed to close faulted session");
    assert_eq!(*session.state(), ObdSessionState::Disconnected);
    assert!(!transport.is_connected());

    // Session can be cleanly reopened after graceful close
    transport.clear_write_delay();
    session.open().await.expect("Failed to reopen after close");
    assert_eq!(*session.state(), ObdSessionState::Initializing);
}


#[tokio::test]
async fn test_async_cancellation_during_read_invalidates_session() {
    let transport = MockObdTransport::new();
    transport.script_standard_handshake();

    let mut session = ObdSession::new(
        transport.clone(),
        ObdSessionConfig {
            command_timeout: Duration::from_millis(1000),
            ..Default::default()
        },
    );

    session.open().await.unwrap();
    session.initialize().await.unwrap();

    // Script Mode 01 RPM query response and inject delay AFTER handshake
    transport.script_response(
        ObdCommand::Mode01(Mode01Pid::EngineRpm),
        Ok("41 0C 1A F8\r\r>".to_string()),
    );
    transport.inject_read_delay(Duration::from_millis(500));

    // Spawn transaction and cancel it early (after 20ms)
    let session_arc = Arc::new(tokio::sync::Mutex::new(session));
    let s_clone = session_arc.clone();

    let handle = tokio::spawn(async move {
        let mut guard = s_clone.lock().await;
        guard.query_pid(Mode01Pid::EngineRpm).await
    });

    tokio::time::sleep(Duration::from_millis(20)).await;
    handle.abort(); // Cancel the transaction future

    // Let abort propagate
    let _ = handle.await;

    // Check session lock: transaction was aborted mid-stream.
    let mut guard = session_arc.lock().await;

    // PROOF 2: State must be Faulted with reason containing cancellation (not stuck in TransactionInProgress)
    match guard.state() {
        ObdSessionState::Faulted { reason } => {
            assert!(
                reason.contains("cancelled or dropped"),
                "Expected cancellation reason, got: {}",
                reason
            );
        }
        other => panic!(
            "Expected Faulted state after cancellation, got: {:?}",
            other
        ),
    }

    // Subsequent command must return SessionFaulted, NOT SessionBusy
    let next_res = guard.query_pid(Mode01Pid::VehicleSpeed).await;
    match next_res {
        Err(ObdCodecError::SessionFaulted(reason)) => {
            assert!(reason.contains("cancelled or dropped"));
        }
        other => panic!("Expected SessionFaulted error, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_cancellation_via_tokio_timeout_transitions_to_faulted() {
    let transport = MockObdTransport::new();
    transport.script_standard_handshake();

    let mut session = ObdSession::new(
        transport.clone(),
        ObdSessionConfig {
            command_timeout: Duration::from_millis(1000),
            ..Default::default()
        },
    );

    session.open().await.unwrap();
    session.initialize().await.unwrap();

    transport.script_response(
        ObdCommand::Mode01(Mode01Pid::EngineRpm),
        Ok("41 0C 1A F8\r\r>".to_string()),
    );
    transport.inject_read_delay(Duration::from_millis(200));

    // Wrap query in tokio::time::timeout of 20ms, dropping the future on timeout
    let timeout_res = tokio::time::timeout(
        Duration::from_millis(20),
        session.query_pid(Mode01Pid::EngineRpm),
    )
    .await;

    assert!(
        timeout_res.is_err(),
        "Expected caller timeout to drop future"
    );

    // RAII guard MUST have transitioned session to Faulted upon drop
    match session.state() {
        ObdSessionState::Faulted { reason } => {
            assert!(
                reason.contains("cancelled or dropped"),
                "Expected cancellation reason, got: {}",
                reason
            );
        }
        other => panic!(
            "Expected Faulted state after timeout drop, got: {:?}",
            other
        ),
    }

    // Subsequent command MUST return SessionFaulted, NOT SessionBusy
    let next_err = session
        .query_pid(Mode01Pid::VehicleSpeed)
        .await
        .unwrap_err();
    assert!(matches!(next_err, ObdCodecError::SessionFaulted(_)));
}


#[tokio::test]
async fn test_cancellation_during_open_transitions_to_faulted() {
    let transport = MockObdTransport::new();
    transport.inject_open_delay(Duration::from_millis(200));

    let mut session = ObdSession::new(transport.clone(), ObdSessionConfig::default());
    assert_eq!(*session.state(), ObdSessionState::Disconnected);

    // Cancel open future via timeout before transport completes open
    let timeout_res = tokio::time::timeout(Duration::from_millis(20), session.open()).await;
    assert!(
        timeout_res.is_err(),
        "Expected timeout dropping open future"
    );

    // OpenGuard MUST have moved session to Faulted
    match session.state() {
        ObdSessionState::Faulted { reason } => {
            assert!(
                reason.contains("cancelled or dropped"),
                "Expected cancellation reason, got: {}",
                reason
            );
        }
        other => panic!(
            "Expected Faulted state after open cancellation, got: {:?}",
            other
        ),
    }

    // Subsequent open() must be rejected because session is Faulted, not Disconnected
    let open_err = session.open().await.unwrap_err();
    assert!(matches!(
        open_err,
        ObdCodecError::InvalidSessionState { .. }
    ));

    // Subsequent initialize() must be rejected because session is Faulted, not Initializing
    let init_err = session.initialize().await.unwrap_err();
    assert!(matches!(
        init_err,
        ObdCodecError::InvalidSessionState { .. }
    ));

    // Diagnostic query must be rejected with SessionFaulted
    let diag_err = session.query_pid(Mode01Pid::EngineRpm).await.unwrap_err();
    assert!(matches!(diag_err, ObdCodecError::SessionFaulted(_)));

    // Calling close() must reset session to Disconnected
    session.close().await.expect("Session close failed");
    assert_eq!(*session.state(), ObdSessionState::Disconnected);

    // Clear open delay and verify clean reconnect
    transport.clear_open_delay();
    transport.script_standard_handshake();
    session.open().await.expect("Session reopen failed");
    assert_eq!(*session.state(), ObdSessionState::Initializing);
    let id = session
        .initialize()
        .await
        .expect("Session handshake failed");
    assert_eq!(id.elm_version, "ELM327 v1.5");
    assert_eq!(*session.state(), ObdSessionState::Ready);
}

#[tokio::test]
async fn test_open_failure_transitions_to_faulted_and_requires_close() {
    let transport = MockObdTransport::new();
    transport.inject_fail_open(ObdCodecError::IoError(
        "hardware port inaccessible".to_string(),
    ));

    let mut session = ObdSession::new(transport, ObdSessionConfig::default());
    assert_eq!(*session.state(), ObdSessionState::Disconnected);

    let err = session.open().await.unwrap_err();
    assert!(matches!(err, ObdCodecError::IoError(_)));

    // OpenGuard / error handler MUST transition session to Faulted
    match session.state() {
        ObdSessionState::Faulted { reason } => {
            assert!(
                reason.contains("hardware port inaccessible"),
                "Expected open failure reason, got: {}",
                reason
            );
        }
        other => panic!("Expected Faulted state after open error, got: {:?}", other),
    }

    // Reopen without close must be rejected
    let open_err = session.open().await.unwrap_err();
    assert!(matches!(
        open_err,
        ObdCodecError::InvalidSessionState { .. }
    ));

    // Calling close resets to Disconnected
    session.close().await.expect("Session close failed");
    assert_eq!(*session.state(), ObdSessionState::Disconnected);
}

#[tokio::test]
async fn test_close_failure_transitions_to_faulted_and_blocks_reopen() {
    let transport = MockObdTransport::new();
    transport.script_standard_handshake();

    let mut session = ObdSession::new(transport.clone(), ObdSessionConfig::default());
    session.open().await.expect("Session open failed");
    session
        .initialize()
        .await
        .expect("Session initialize failed");
    assert_eq!(*session.state(), ObdSessionState::Ready);

    // Inject transport close failure
    transport.inject_fail_close(ObdCodecError::IoError(
        "hardware port lock release failed".to_string(),
    ));

    let close_err = session.close().await.unwrap_err();
    assert!(matches!(close_err, ObdCodecError::IoError(_)));

    // Must transition to Faulted, NOT Disconnected!
    match session.state() {
        ObdSessionState::Faulted { reason } => {
            assert!(
                reason.contains("hardware port lock release failed"),
                "Expected close failure in state reason, got: {}",
                reason
            );
        }
        other => panic!(
            "Expected Faulted state after close failure, got: {:?}",
            other
        ),
    }

    // While Faulted from close failure:
    // 1. Reopening must be blocked
    let open_err = session.open().await.unwrap_err();
    assert!(matches!(
        open_err,
        ObdCodecError::InvalidSessionState { .. }
    ));

    // 2. Initializing must be blocked
    let init_err = session.initialize().await.unwrap_err();
    assert!(matches!(
        init_err,
        ObdCodecError::InvalidSessionState { .. }
    ));

    // 3. Diagnostics must be blocked with SessionFaulted
    let diag_err = session.query_pid(Mode01Pid::EngineRpm).await.unwrap_err();
    assert!(matches!(diag_err, ObdCodecError::SessionFaulted(_)));

    // Retrying close without injected failure succeeds and transitions to Disconnected
    session.close().await.expect("Close retry failed");
    assert_eq!(*session.state(), ObdSessionState::Disconnected);

    // Now session can be reopened and used cleanly
    transport.script_standard_handshake();
    session.open().await.expect("Reopen failed");
    assert_eq!(*session.state(), ObdSessionState::Initializing);
    session.initialize().await.expect("Re-initialize failed");
    assert_eq!(*session.state(), ObdSessionState::Ready);
}

#[tokio::test]
async fn test_cancellation_during_close_transitions_to_faulted() {
    let transport = MockObdTransport::new();
    transport.script_standard_handshake();

    let mut session = ObdSession::new(transport.clone(), ObdSessionConfig::default());
    session.open().await.expect("Session open failed");
    session
        .initialize()
        .await
        .expect("Session initialize failed");
    assert_eq!(*session.state(), ObdSessionState::Ready);

    // Inject artificial delay on close
    transport.inject_close_delay(Duration::from_millis(200));

    // Cancel close future via timeout while transport.close() is pending
    let timeout_res = tokio::time::timeout(Duration::from_millis(20), session.close()).await;
    assert!(
        timeout_res.is_err(),
        "Expected timeout dropping close future"
    );

    // CloseGuard MUST have moved session to Faulted
    match session.state() {
        ObdSessionState::Faulted { reason } => {
            assert!(
                reason.contains("cancelled or dropped"),
                "Expected cancellation reason, got: {}",
                reason
            );
        }
        other => panic!(
            "Expected Faulted state after close cancellation, got: {:?}",
            other
        ),
    }

    // Subsequent open() must be rejected because session is Faulted, not Disconnected
    let open_err = session.open().await.unwrap_err();
    assert!(matches!(
        open_err,
        ObdCodecError::InvalidSessionState { .. }
    ));

    // Clear close delay and call close again to cleanly teardown
    transport.clear_close_delay();
    session.close().await.expect("Close recovery failed");
    assert_eq!(*session.state(), ObdSessionState::Disconnected);
}
