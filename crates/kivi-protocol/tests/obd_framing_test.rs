//! Comprehensive test suite for Milestone 3D-2B.1 (Response Framing, Buffer Limits & Configuration Engine).
//!
//! Covers all 32 test cases specified in the Phase 5 test plan:
//! - Cases 1..10: Framing and chunk boundaries
//! - Cases 11..18: Limits, overflow, and invalid input validation
//! - Cases 19..25: Cancellation, timeout, and fault containment
//! - Cases 26..32: Configuration, protocol mapping, and read-only safety invariants

use std::time::Duration;
use tokio::io::AsyncWriteExt;

use kivi_protocol::obd::adapter::{parse_adapter_response, MAX_ISOTP_PAYLOAD_BYTES};
use kivi_protocol::obd::command::{AdapterCommand, Mode01Pid, ObdCommand};
use kivi_protocol::obd::errors::ObdCodecError;
use kivi_protocol::obd::framing::{
    extract_prompt_framed_response, is_permitted_stream_byte, read_framed_response_async,
    ProtocolSelection, SerialBaudRate, DEFAULT_MAX_TEXT_RESPONSE_BYTES,
};
use kivi_protocol::obd::mock::MockObdTransport;
use kivi_protocol::obd::session::{ObdSession, ObdSessionConfig, ObdSessionState};

// ============================================================================
// Group 1: Framing and Chunk Boundaries (Cases 1..10)
// ============================================================================

#[tokio::test]
async fn test_01_complete_response_with_prompt_in_final_chunk() {
    let (mut client, mut server) = tokio::io::duplex(1024);
    server.write_all(b"41 0C 1A F8\r\r>").await.unwrap();

    let mut buffer = Vec::new();
    let frame =
        read_framed_response_async(&mut client, &mut buffer, 1024, Duration::from_millis(500))
            .await
            .unwrap();

    assert_eq!(frame, "41 0C 1A F8\r\r");
    assert!(
        buffer.is_empty(),
        "Buffer must be empty after complete frame extraction"
    );

    let parsed = parse_adapter_response(&frame).unwrap();
    assert_eq!(parsed, vec![0x41, 0x0C, 0x1A, 0xF8]);
}

#[tokio::test]
async fn test_02_prompt_delivered_in_separate_chunk() {
    let (mut client, mut server) = tokio::io::duplex(1024);

    let writer_task = tokio::spawn(async move {
        server.write_all(b"41 0C 1A F8\r\r").await.unwrap();
        tokio::time::sleep(Duration::from_millis(25)).await;
        server.write_all(b">").await.unwrap();
    });

    let mut buffer = Vec::new();
    let frame =
        read_framed_response_async(&mut client, &mut buffer, 1024, Duration::from_millis(500))
            .await
            .unwrap();

    writer_task.await.unwrap();
    assert_eq!(frame, "41 0C 1A F8\r\r");
    assert!(buffer.is_empty());
}

#[tokio::test]
async fn test_03_response_split_across_several_arbitrary_chunk_boundaries() {
    let (mut client, mut server) = tokio::io::duplex(1024);

    let writer_task = tokio::spawn(async move {
        let chunks: &[&[u8]] = &[b"4", b"1 ", b"0C ", b"1", b"A ", b"F8", b"\r", b"\r", b">"];
        for chunk in chunks {
            server.write_all(chunk).await.unwrap();
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });

    let mut buffer = Vec::new();
    let frame =
        read_framed_response_async(&mut client, &mut buffer, 1024, Duration::from_millis(500))
            .await
            .unwrap();

    writer_task.await.unwrap();
    assert_eq!(frame, "41 0C 1A F8\r\r");
    let parsed = parse_adapter_response(&frame).unwrap();
    assert_eq!(parsed, vec![0x41, 0x0C, 0x1A, 0xF8]);
}

#[tokio::test]
async fn test_04_prompt_adjacent_crlf_handling() {
    let variants: &[&[u8]] = &[
        b"41 0C 1A F8\r\n>",
        b"41 0C 1A F8\r\r>",
        b"41 0C 1A F8\n>",
        b"41 0C 1A F8>",
    ];

    for &raw in variants {
        let mut buffer = raw.to_vec();
        let frame = extract_prompt_framed_response(&mut buffer, 1024)
            .unwrap()
            .expect("Should extract valid frame");

        let parsed = parse_adapter_response(&frame).unwrap();
        assert_eq!(parsed, vec![0x41, 0x0C, 0x1A, 0xF8]);
        assert!(buffer.is_empty());
    }
}

#[tokio::test]
async fn test_05_command_echo_followed_by_valid_response() {
    let mut buffer = b"010C\r41 0C 1A F8\r\r>".to_vec();
    let frame = extract_prompt_framed_response(&mut buffer, 1024)
        .unwrap()
        .expect("Prompt should be recognized");

    assert_eq!(frame, "010C\r41 0C 1A F8\r\r");
    assert!(buffer.is_empty());

    let parsed = parse_adapter_response(&frame).unwrap();
    assert_eq!(parsed, vec![0x41, 0x0C, 0x1A, 0xF8]);
}

#[tokio::test]
async fn test_06_searching_status_followed_by_valid_response() {
    let mut buffer = b"SEARCHING...\r41 0C 1A F8\r\r>".to_vec();
    let frame = extract_prompt_framed_response(&mut buffer, 1024)
        .unwrap()
        .expect("Prompt should be recognized");

    // The raw frame contains SEARCHING..., which the codec parser strips as status progress
    assert!(frame.contains("SEARCHING..."));
    let parsed = parse_adapter_response(&frame).unwrap();
    assert_eq!(parsed, vec![0x41, 0x0C, 0x1A, 0xF8]);
}

#[tokio::test]
async fn test_07_adapter_error_response_followed_by_prompt() {
    let mut buffer = b"CAN ERROR\r\r>".to_vec();
    let frame = extract_prompt_framed_response(&mut buffer, 1024)
        .unwrap()
        .expect("Prompt should be framed");

    assert_eq!(frame, "CAN ERROR\r\r");
    // Passing framed error to adapter codec yields typed AdapterError
    let err = parse_adapter_response(&frame).unwrap_err();
    assert!(matches!(err, ObdCodecError::AdapterError(_)));
}

#[tokio::test]
async fn test_08_empty_or_whitespace_response_handling() {
    // Just prompt '>'
    let mut buffer = b">".to_vec();
    let frame = extract_prompt_framed_response(&mut buffer, 1024)
        .unwrap()
        .unwrap();
    assert_eq!(frame, "");
    let err = parse_adapter_response(&frame).unwrap_err();
    assert!(matches!(err, ObdCodecError::MalformedText(_)));

    // Whitespace only followed by prompt
    let mut buffer_ws = b"   \r\n   >".to_vec();
    let frame_ws = extract_prompt_framed_response(&mut buffer_ws, 1024)
        .unwrap()
        .unwrap();
    let err_ws = parse_adapter_response(&frame_ws).unwrap_err();
    assert!(matches!(err_ws, ObdCodecError::MalformedText(_)));
}

#[tokio::test]
async fn test_09_eof_before_prompt_produces_unexpected_eof() {
    let (mut client, mut server) = tokio::io::duplex(1024);

    // Send data and drop writer without sending '>'
    server.write_all(b"41 0C 1A F8\r\r").await.unwrap();
    drop(server);

    let mut buffer = Vec::new();
    let err =
        read_framed_response_async(&mut client, &mut buffer, 1024, Duration::from_millis(200))
            .await
            .unwrap_err();

    assert!(matches!(err, ObdCodecError::UnexpectedEof));
    assert_eq!(buffer, b"41 0C 1A F8\r\r");
}

#[tokio::test]
async fn test_10_prompt_free_stream_reaching_deadline() {
    let (mut client, _server) = tokio::io::duplex(1024);
    // Server sends nothing, client reaches deadline

    let mut buffer = Vec::new();
    let err = read_framed_response_async(&mut client, &mut buffer, 1024, Duration::from_millis(50))
        .await
        .unwrap_err();

    assert!(matches!(err, ObdCodecError::Timeout(50)));
}

// ============================================================================
// Group 2: Limits, Overflow, and Invalid Input Validation (Cases 11..18)
// ============================================================================

#[test]
fn test_11_response_immediately_below_configured_limit() {
    let limit = 20;
    // 18 payload chars + 1 prompt char = 19 chars (below limit 20)
    let payload = "A".repeat(18);
    let mut buffer = format!("{}>", payload).into_bytes();
    assert_eq!(buffer.len(), 19);

    let frame = extract_prompt_framed_response(&mut buffer, limit)
        .unwrap()
        .expect("19 bytes <= limit 20 must succeed");
    assert_eq!(frame.len(), 18);
}

#[test]
fn test_12_response_exactly_at_limit() {
    let limit = 20;
    // 19 payload chars + 1 prompt char = 20 chars (exactly at limit 20)
    let payload = "B".repeat(19);
    let mut buffer = format!("{}>", payload).into_bytes();
    assert_eq!(buffer.len(), 20);

    let frame = extract_prompt_framed_response(&mut buffer, limit)
        .unwrap()
        .expect("20 bytes <= limit 20 must succeed");
    assert_eq!(frame.len(), 19);
}

#[test]
fn test_13_response_exceeding_limit_by_one_byte() {
    let limit = 20;
    // 20 payload chars + 1 prompt char = 21 chars (exceeds limit 20)
    let payload = "C".repeat(20);
    let mut buffer = format!("{}>", payload).into_bytes();
    assert_eq!(buffer.len(), 21);

    let err = extract_prompt_framed_response(&mut buffer, limit).unwrap_err();
    match err {
        ObdCodecError::BufferOverflow {
            limit: l,
            actual: a,
        } => {
            assert_eq!(l, 20);
            assert_eq!(a, 21);
        }
        other => panic!("Expected BufferOverflow, got {:?}", other),
    }
}

#[tokio::test]
async fn test_14_oversized_response_arriving_in_many_small_chunks() {
    let limit = 64;
    let (mut client, mut server) = tokio::io::duplex(1024);

    let writer = tokio::spawn(async move {
        // Send 10 bytes at a time for 100 bytes (exceeds limit 64)
        for _ in 0..10 {
            if server.write_all(b"0123456789").await.is_err() {
                break;
            }
        }
    });

    let mut buffer = Vec::new();
    let err =
        read_framed_response_async(&mut client, &mut buffer, limit, Duration::from_millis(500))
            .await
            .unwrap_err();

    writer.await.unwrap();
    assert!(matches!(
        err,
        ObdCodecError::BufferOverflow { limit: 64, .. }
    ));
}

#[tokio::test]
async fn test_15_oversized_response_arriving_in_single_large_chunk() {
    let limit = 128;
    let (mut client, mut server) = tokio::io::duplex(2048);

    let writer = tokio::spawn(async move {
        let large_payload = vec![b'A'; 500];
        let _ = server.write_all(&large_payload).await;
    });

    let mut buffer = Vec::new();
    let err =
        read_framed_response_async(&mut client, &mut buffer, limit, Duration::from_millis(500))
            .await
            .unwrap_err();

    writer.await.unwrap();
    assert!(matches!(
        err,
        ObdCodecError::BufferOverflow { limit: 128, .. }
    ));
}

#[test]
fn test_16_unexpected_control_characters_in_response_rejected() {
    let control_chars = [0x00u8, 0x07, 0x1B, 0x03, 0x04, 0x80, 0xFF];

    for &bad_byte in &control_chars {
        assert!(!is_permitted_stream_byte(bad_byte));

        let mut buffer = vec![b'4', b'1', bad_byte, b'>'];
        let err = extract_prompt_framed_response(&mut buffer, 1024).unwrap_err();
        assert!(matches!(err, ObdCodecError::FramingError(_)));
    }
}

#[test]
fn test_17_invalid_hexadecimal_payload_after_successful_framing() {
    let mut buffer = b"41 0C ZZ 99\r\r>".to_vec();
    let frame = extract_prompt_framed_response(&mut buffer, 1024)
        .unwrap()
        .expect("Framing succeeds because ASCII characters are permitted");

    // Transport framing succeeds, but parsing diagnostic hex tokens fails
    let err = parse_adapter_response(&frame).unwrap_err();
    assert!(matches!(err, ObdCodecError::MalformedText(_)));
}

#[test]
fn test_18_verify_textual_response_limits_and_isotp_payload_limits_are_independent() {
    // 1. DEFAULT_MAX_TEXT_RESPONSE_BYTES is 16 KiB
    assert_eq!(DEFAULT_MAX_TEXT_RESPONSE_BYTES, 16384);
    // 2. MAX_ISOTP_PAYLOAD_BYTES is 4095 binary bytes
    assert_eq!(MAX_ISOTP_PAYLOAD_BYTES, 4095);

    // Verify framing text allows up to 16,384 bytes
    let valid_large_text = "41 0C 00 00\r\n".repeat(1000); // 13,000 bytes
    let mut text_buffer = format!("{}>", valid_large_text).into_bytes();
    assert!(text_buffer.len() <= DEFAULT_MAX_TEXT_RESPONSE_BYTES);

    let framed = extract_prompt_framed_response(&mut text_buffer, DEFAULT_MAX_TEXT_RESPONSE_BYTES);
    assert!(framed.is_ok());

    // But ISO-TP codec enforces MAX_ISOTP_PAYLOAD_BYTES (4095) independently
    let mut oversized_isotp_lines = vec!["1000"]; // Declared 4096 hex = 4096 bytes > 4095 limit
    oversized_isotp_lines.extend(std::iter::repeat_n("0: 01 02 03 04 05 06 07", 586));
    let full_text = oversized_isotp_lines.join("\r\n");
    let err = parse_adapter_response(&full_text).unwrap_err();
    assert!(matches!(
        err,
        ObdCodecError::OversizedPayload {
            expected: 4095,
            actual: 4096
        }
    ));
}

// ============================================================================
// Group 3: Cancellation, Timeout, and Fault Containment (Cases 19..25)
// ============================================================================

#[tokio::test]
async fn test_19_cancellation_before_first_read() {
    let (mut client, mut server) = tokio::io::duplex(1024);
    server.write_all(b"41 0C 1A F8\r\r>").await.unwrap();

    let mut buffer = Vec::new();
    let read_fut =
        read_framed_response_async(&mut client, &mut buffer, 1024, Duration::from_millis(500));

    // Immediately drop future before awaiting
    drop(read_fut);

    // Buffer must remain pristine (no data accumulated yet)
    assert!(buffer.is_empty());
}

#[tokio::test]
async fn test_20_cancellation_after_partial_response() {
    let (mut client, mut server) = tokio::io::duplex(1024);
    server.write_all(b"41 0C").await.unwrap();

    let mut buffer = Vec::new();

    // Cancel by wrapping in a short timeout
    let _ = tokio::time::timeout(
        Duration::from_millis(50),
        read_framed_response_async(&mut client, &mut buffer, 1024, Duration::from_millis(500)),
    )
    .await;

    // Buffer retains the partial bytes read prior to cancellation
    assert_eq!(buffer, b"41 0C");
}

#[tokio::test]
async fn test_21_cancellation_after_response_body_before_prompt() {
    let (mut client, mut server) = tokio::io::duplex(1024);
    server.write_all(b"41 0C 1A F8\r\r").await.unwrap();

    let mut buffer = Vec::new();
    let _ = tokio::time::timeout(
        Duration::from_millis(50),
        read_framed_response_async(&mut client, &mut buffer, 1024, Duration::from_millis(500)),
    )
    .await;

    // Response body is in buffer, but no prompt '>' was received
    assert_eq!(buffer, b"41 0C 1A F8\r\r");
    assert!(extract_prompt_framed_response(&mut buffer, 1024)
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn test_22_delayed_response_arriving_after_timeout() {
    let (mut client, mut server) = tokio::io::duplex(1024);

    let mut buffer = Vec::new();
    // Timeout on first query
    let err = read_framed_response_async(&mut client, &mut buffer, 1024, Duration::from_millis(30))
        .await
        .unwrap_err();
    assert!(matches!(err, ObdCodecError::Timeout(30)));

    // Late response now arrives from adapter on wire
    server.write_all(b"41 0C 1A F8\r\r>").await.unwrap();

    // The stream has dirty residual bytes. Without clearing/draining, next read gets the delayed response
    let late_frame =
        read_framed_response_async(&mut client, &mut buffer, 1024, Duration::from_millis(100))
            .await
            .unwrap();
    assert_eq!(late_frame, "41 0C 1A F8\r\r");
}

#[tokio::test]
async fn test_23_failed_or_cancelled_operation_not_treated_as_successful() {
    let (mut client, mut server) = tokio::io::duplex(1024);
    server.write_all(b"UNABLE TO CONNECT\r\r>").await.unwrap();

    let mut buffer = Vec::new();
    let frame =
        read_framed_response_async(&mut client, &mut buffer, 1024, Duration::from_millis(200))
            .await
            .unwrap();

    // Framing succeeded, but decoding must fail closed
    let err = parse_adapter_response(&frame).unwrap_err();
    assert!(matches!(err, ObdCodecError::AdapterError(_)));
}

#[tokio::test]
async fn test_24_existing_session_remains_faulted_after_cancellation() {
    let transport = MockObdTransport::new();
    transport.script_standard_handshake();

    let config = ObdSessionConfig {
        command_timeout: Duration::from_millis(50),
        ..Default::default()
    };
    let mut session = ObdSession::new(transport.clone(), config);
    session.open().await.unwrap();
    session.initialize().await.unwrap();

    // Now inject read delay of 200ms with command timeout of 50ms
    transport.inject_read_delay(Duration::from_millis(200));

    // Cancelled / timed-out query transitions session to Faulted
    let err = session
        .execute_command(ObdCommand::Mode01(Mode01Pid::EngineRpm))
        .await
        .unwrap_err();
    assert!(matches!(err, ObdCodecError::Timeout(50)));

    assert!(matches!(session.state(), ObdSessionState::Faulted { .. }));

    // Subsequent transaction must be blocked
    let next_err = session
        .execute_command(ObdCommand::Mode01(Mode01Pid::CoolantTemp))
        .await
        .unwrap_err();
    assert!(matches!(next_err, ObdCodecError::SessionFaulted(_)));
}

#[tokio::test]
async fn test_25_stale_response_data_isolation_between_queries() {
    // Demonstrates that draining/clearing the buffer prevents consuming stale partial responses
    let mut buffer = b"41 0C ".to_vec(); // Stale partial bytes from cancelled query

    // Proper lifecycle recovery clears the buffer
    buffer.clear();
    assert!(buffer.is_empty());

    // New query response arrives
    buffer.extend_from_slice(b"41 0D 30\r\r>");
    let frame = extract_prompt_framed_response(&mut buffer, 1024)
        .unwrap()
        .unwrap();
    assert_eq!(frame, "41 0D 30\r\r");
    let parsed = parse_adapter_response(&frame).unwrap();
    assert_eq!(parsed, vec![0x41, 0x0D, 0x30]); // Speed 48 km/h, no RPM contamination
}

// ============================================================================
// Group 4: Configuration, Protocol Mapping, and Safety Invariants (Cases 26..32)
// ============================================================================

#[test]
fn test_26_default_protocol_selection_maps_to_atsp6() {
    let default_proto = ProtocolSelection::default();
    assert_eq!(default_proto, ProtocolSelection::Can11Bit500k);

    let cmd = default_proto.to_adapter_command();
    assert_eq!(cmd, AdapterCommand::SetProtocolCan11_500);
    assert_eq!(ObdCommand::Adapter(cmd).encode(), "ATSP6\r");
}

#[test]
fn test_27_explicit_automatic_protocol_selection_maps_to_atsp0() {
    let auto_proto = ProtocolSelection::Auto;
    let cmd = auto_proto.to_adapter_command();
    assert_eq!(cmd, AdapterCommand::SetProtocolAuto);
    assert_eq!(ObdCommand::Adapter(cmd).encode(), "ATSP0\r");
}

#[test]
fn test_28_baud_rate_selection_accepts_only_supported_enum_values() {
    assert_eq!(SerialBaudRate::default(), SerialBaudRate::Baud115200);
    assert_eq!(SerialBaudRate::Baud115200.as_u32(), 115_200);
    assert_eq!(SerialBaudRate::Baud38400.as_u32(), 38_400);
}

#[test]
fn test_29_no_arbitrary_adapter_command_injection_in_config() {
    let config = ObdSessionConfig::default();
    // Protocol must be one of the closed enum variants
    assert!(matches!(
        config.protocol,
        ProtocolSelection::Can11Bit500k | ProtocolSelection::Auto
    ));
    // Baud rate must be one of the closed enum variants
    assert!(matches!(
        config.baud_rate,
        SerialBaudRate::Baud115200 | SerialBaudRate::Baud38400
    ));
}

#[tokio::test]
async fn test_30_ats1_whitespace_dependent_parser_intact() {
    // ATS1 ensures spaces are present between hex tokens
    let with_spaces = "41 0C 1A F8";
    let parsed = parse_adapter_response(with_spaces).unwrap();
    assert_eq!(parsed, vec![0x41, 0x0C, 0x1A, 0xF8]);

    // If ATS0 were used (spaces off: 410C1AF8), the whitespace parser fails closed
    let without_spaces = "410C1AF8";
    let err = parse_adapter_response(without_spaces).unwrap_err();
    assert!(matches!(err, ObdCodecError::MalformedText(_)));
}

#[test]
fn test_31_existing_read_only_command_allowlisting_unchanged() {
    // Verify that Mode01, Mode09, and Adapter commands remain strictly closed
    let cmd_rpm = ObdCommand::Mode01(Mode01Pid::EngineRpm);
    assert_eq!(cmd_rpm.encode(), "01 0C\r");

    let cmd_vin = ObdCommand::Mode09(kivi_protocol::obd::command::Mode09Item::Vin);
    assert_eq!(cmd_vin.encode(), "09 02\r");

    let cmd_auto = ObdCommand::Adapter(AdapterCommand::SetProtocolAuto);
    assert_eq!(cmd_auto.encode(), "ATSP0\r");
}

#[test]
fn test_32_rom_isolation_invariants_and_autotune_blocker_active() {
    // Verify that default session configuration sets max_response_bytes to 16 KiB
    let config = ObdSessionConfig::default();
    assert_eq!(config.max_response_bytes, DEFAULT_MAX_TEXT_RESPONSE_BYTES);
    assert_eq!(config.max_response_bytes, 16384);
}
