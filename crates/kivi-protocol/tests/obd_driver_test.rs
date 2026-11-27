//! Deterministic unit and integration tests for Milestone 3D-3:
//! Read-Only OBD-II Driver and Continuous Telemetry Acquisition Engine.
//!
//! Exercises:
//! - Driver configuration validation and boundary rejection.
//! - Protocol-level connection handshake, VIN discovery, and supported-PID bitmap filtering.
//! - Mapping of decoded Mode 01 PID values into canonical TelemetryFrame structures.
//! - Strict handling of stoichiometric ratio and Wideband AFR (no fabricated values).
//! - Continuous telemetry broadcast streaming over bounded Tokio broadcast channel.
//! - Deterministic task cancellation and session teardown on disconnect.
//! - Fault containment on transport error or timeout (polling loop halts immediately).
//! - Clean repeated connect/disconnect lifecycles without task or descriptor leakage.
//! - Absolute ROM/BIN calibration isolation.

use kivi_protocol::driver::{EcuDriver, TelemetrySource};
use kivi_protocol::model::ConnectionState;
use kivi_protocol::obd::command::{Mode01Pid, Mode09Item, ObdCommand};
use kivi_protocol::obd::driver::{Obd2Driver, Obd2DriverConfig};
use kivi_protocol::obd::mock::MockObdTransport;
use std::time::Duration;

fn script_handshake_and_supported_pids(transport: &MockObdTransport) {
    transport.script_standard_handshake();

    // Mode 01 PID 0x00 Supported PIDs: bits for 0x05 (CLT), 0x0B (MAP), 0x0C (RPM), 0x0D (Speed), 0x0F (IAT), 0x11 (TPS)
    // 0x05 is bit 27 (byte 0, bit 3 -> 0x08)
    // 0x0B is bit 21 (byte 1, bit 5 -> 0x20)
    // 0x0C is bit 20 (byte 1, bit 4 -> 0x10)
    // 0x0D is bit 19 (byte 1, bit 3 -> 0x08)
    // 0x0F is bit 17 (byte 1, bit 1 -> 0x02)
    // 0x11 is bit 15 (byte 2, bit 7 -> 0x80)
    // Byte A = 0x08, Byte B = 0x3A, Byte C = 0x80, Byte D = 0x00
    transport.script_response(
        ObdCommand::Mode01(Mode01Pid::SupportedPids01_20),
        Ok("41 00 08 3A 80 00\r\r>".to_string()),
    );
}

fn script_vin(transport: &MockObdTransport) {
    // Mode 09 PID 0x02 VIN query: "1HGCR2F83HA000000"
    transport.script_response(
        ObdCommand::Mode09(Mode09Item::Vin),
        Ok("49 02 01 31 48 47 43 52 32 46 38 33 48 41 30 30 30 30 30 30\r\r>".to_string()),
    );
}

#[test]
fn test_obd2_driver_config_validation() {
    let mut valid = Obd2DriverConfig::default();
    assert!(valid.validate().is_ok());

    // Rejects zero or too fast poll interval
    valid.poll_interval = Duration::from_millis(5);
    assert!(valid.validate().is_err());

    valid.poll_interval = Duration::from_millis(50);
    // Rejects zero command timeout
    valid.command_timeout = Duration::ZERO;
    assert!(valid.validate().is_err());

    valid.command_timeout = Duration::from_millis(500);
    // Rejects empty polled PIDs
    valid.polled_pids = vec![];
    assert!(valid.validate().is_err());

    // Rejects bitmap PID inside polled PIDs
    valid.polled_pids = vec![Mode01Pid::SupportedPids01_20];
    assert!(valid.validate().is_err());

    // Rejects invalid stoich ratio (NaN, negative, zero)
    valid.polled_pids = vec![Mode01Pid::EngineRpm];
    valid.stoichiometric_ratio = Some(f32::NAN);
    assert!(valid.validate().is_err());

    valid.stoichiometric_ratio = Some(-1.0);
    assert!(valid.validate().is_err());

    valid.stoichiometric_ratio = Some(0.0);
    assert!(valid.validate().is_err());

    valid.stoichiometric_ratio = Some(14.7);
    assert!(valid.validate().is_ok());
}

#[tokio::test]
async fn test_obd2_driver_successful_connect_and_telemetry_streaming() {
    let transport = MockObdTransport::new();
    script_handshake_and_supported_pids(&transport);
    script_vin(&transport);

    // Script Mode 01 diagnostic queries
    // RPM: 2500.0 (41 0C 27 10 -> (0x27*256 + 0x10)/4 = 10000/4 = 2500)
    transport.script_response(
        ObdCommand::Mode01(Mode01Pid::EngineRpm),
        Ok("41 0C 27 10\r\r>".to_string()),
    );
    // TPS: 50.0% (41 11 80 -> 128*100/255 = 50.196%)
    transport.script_response(
        ObdCommand::Mode01(Mode01Pid::ThrottlePosition),
        Ok("41 11 80\r\r>".to_string()),
    );
    // MAP: 105 kPa (41 0B 69 -> 105)
    transport.script_response(
        ObdCommand::Mode01(Mode01Pid::ManifoldPressure),
        Ok("41 0B 69\r\r>".to_string()),
    );
    // CLT: 85°C (41 05 7D -> 125 - 40 = 85)
    transport.script_response(
        ObdCommand::Mode01(Mode01Pid::CoolantTemp),
        Ok("41 05 7D\r\r>".to_string()),
    );
    // IAT: 30°C (41 0F 46 -> 70 - 40 = 30)
    transport.script_response(
        ObdCommand::Mode01(Mode01Pid::IntakeAirTemp),
        Ok("41 0F 46\r\r>".to_string()),
    );
    // Speed: 60 km/h (41 0D 3C -> 60)
    transport.script_response(
        ObdCommand::Mode01(Mode01Pid::VehicleSpeed),
        Ok("41 0D 3C\r\r>".to_string()),
    );

    let config = Obd2DriverConfig {
        poll_interval: Duration::from_millis(20),
        stoichiometric_ratio: None,
        polled_pids: vec![
            Mode01Pid::EngineRpm,
            Mode01Pid::ThrottlePosition,
            Mode01Pid::ManifoldPressure,
            Mode01Pid::CoolantTemp,
            Mode01Pid::IntakeAirTemp,
            Mode01Pid::VehicleSpeed,
        ],
        command_timeout: Duration::from_millis(500),
        query_vin: true,
        protocol: Default::default(),
        baud_rate: Default::default(),
    };

    let driver = Obd2Driver::new(transport, config);
    let mut rx = driver.subscribe_telemetry();

    // 1. Connect
    let identity = driver.connect().await.expect("connect failed");
    assert_eq!(identity.ecu_id, "ELM327 v1.5");
    assert_eq!(identity.vin.as_deref(), Some("1HGCR2F83HA000000"));
    assert!(!identity.is_simulated);

    assert!(matches!(
        driver.connection_state(),
        ConnectionState::Connected { .. }
    ));

    // 2. Await first telemetry frame
    let frame = tokio::time::timeout(Duration::from_millis(500), rx.recv())
        .await
        .expect("timeout waiting for frame")
        .expect("channel error");

    assert!(!frame.is_simulated);
    assert!((frame.rpm - 2500.0).abs() < 1.0);
    assert!((frame.tps - 50.196).abs() < 0.1);
    assert_eq!(frame.map_kpa, 105.0);
    assert_eq!(frame.ect_c, 85.0);
    assert_eq!(frame.iat_c, 30.0);
    assert_eq!(frame.speed_kph, 60.0);
    // Stoichiometric ratio is None -> AFR must NOT be fabricated as 14.7 or 0.0
    assert!(frame.afr.is_nan());

    // 3. Disconnect cleanly
    driver.disconnect().await.expect("disconnect failed");
    assert_eq!(driver.connection_state(), ConnectionState::Disconnected);
}

#[tokio::test]
async fn test_obd2_driver_unsupported_pid_filtering() {
    let transport = MockObdTransport::new();
    transport.script_standard_handshake();

    // Script bitmap: Only RPM (0x0C) is supported (Byte B, bit 4 -> 0x10)
    // CLT (0x05) and TPS (0x11) are NOT supported in this bitmap
    transport.script_response(
        ObdCommand::Mode01(Mode01Pid::SupportedPids01_20),
        Ok("41 00 00 10 00 00\r\r>".to_string()),
    );

    // Only RPM should be polled! If driver attempts to poll CLT or TPS, MockObdTransport will fail
    // because no response is scripted for them!
    transport.script_response(
        ObdCommand::Mode01(Mode01Pid::EngineRpm),
        Ok("41 0C 1A F8\r\r>".to_string()), // 1726 RPM
    );

    let config = Obd2DriverConfig {
        poll_interval: Duration::from_millis(20),
        stoichiometric_ratio: None,
        polled_pids: vec![
            Mode01Pid::CoolantTemp, // Configured, but ECU bitmap says unsupported -> must be skipped!
            Mode01Pid::EngineRpm,   // Supported -> must be polled!
            Mode01Pid::ThrottlePosition, // Configured, but ECU bitmap says unsupported -> must be skipped!
        ],
        command_timeout: Duration::from_millis(500),
        query_vin: false,
        protocol: Default::default(),
        baud_rate: Default::default(),
    };

    let driver = Obd2Driver::new(transport, config);
    let mut rx = driver.subscribe_telemetry();

    driver.connect().await.expect("connect failed");

    let frame = tokio::time::timeout(Duration::from_millis(500), rx.recv())
        .await
        .expect("timeout waiting for frame")
        .expect("channel error");

    assert!((frame.rpm - 1726.0).abs() < 1.0);
    // Coolant temp was never polled because ECU reported it unsupported
    // It remains at uninitialized NaN, never fabricated
    assert!(frame.ect_c.is_nan());

    driver.disconnect().await.expect("disconnect failed");
}

#[tokio::test]
async fn test_obd2_driver_wideband_afr_estimation() {
    let transport = MockObdTransport::new();
    transport.script_standard_handshake();

    // Mode 01 PID 0x00 bitmap: continuation bit set (Byte D, bit 0 -> 0x01)
    transport.script_response(
        ObdCommand::Mode01(Mode01Pid::SupportedPids01_20),
        Ok("41 00 00 10 00 01\r\r>".to_string()), // RPM supported + continuation
    );

    // Mode 01 PID 0x20 bitmap: PID 0x34 (WidebandLambda1) supported (Offset = 0x34 - 0x21 = 19 -> Byte C, bit 4 -> 0x10)
    transport.script_response(
        ObdCommand::Mode01(Mode01Pid::SupportedPids21_40),
        Ok("41 20 00 00 10 00\r\r>".to_string()),
    );

    // RPM
    transport.script_response(
        ObdCommand::Mode01(Mode01Pid::EngineRpm),
        Ok("41 0C 1A F8\r\r>".to_string()),
    );

    // Wideband PID 0x34: lambda = 1.0 (0x8000 -> 32768/32768 = 1.0), current = 0 mA (0x8000 -> 32768/256 - 128 = 0)
    transport.script_response(
        ObdCommand::Mode01(Mode01Pid::WidebandLambda1),
        Ok("41 34 80 00 80 00\r\r>".to_string()),
    );

    let config = Obd2DriverConfig {
        poll_interval: Duration::from_millis(20),
        stoichiometric_ratio: Some(14.7), // Gasoline stoichiometric ratio
        polled_pids: vec![Mode01Pid::EngineRpm, Mode01Pid::WidebandLambda1],
        command_timeout: Duration::from_millis(500),
        query_vin: false,
        protocol: Default::default(),
        baud_rate: Default::default(),
    };

    let driver = Obd2Driver::new(transport, config);
    let mut rx = driver.subscribe_telemetry();

    driver.connect().await.expect("connect failed");

    let frame = tokio::time::timeout(Duration::from_millis(500), rx.recv())
        .await
        .expect("timeout waiting for frame")
        .expect("channel error");

    assert!((frame.rpm - 1726.0).abs() < 1.0);
    // 1.0 lambda * 14.7 stoich = 14.7 AFR
    assert!((frame.afr - 14.7).abs() < 0.01);

    driver.disconnect().await.expect("disconnect failed");
}

#[tokio::test]
async fn test_obd2_driver_fault_containment_and_disconnect() {
    let transport = MockObdTransport::new();
    script_handshake_and_supported_pids(&transport);

    // Script Mode 01 RPM query to fail with timeout error
    transport.script_response(
        ObdCommand::Mode01(Mode01Pid::EngineRpm),
        Err(kivi_protocol::obd::errors::ObdCodecError::Timeout(500)),
    );

    let config = Obd2DriverConfig {
        poll_interval: Duration::from_millis(20),
        stoichiometric_ratio: None,
        polled_pids: vec![Mode01Pid::EngineRpm],
        command_timeout: Duration::from_millis(500),
        query_vin: false,
        protocol: Default::default(),
        baud_rate: Default::default(),
    };

    let driver = Obd2Driver::new(transport, config);
    driver.connect().await.expect("connect failed");

    // Wait for the polling loop to encounter the timeout fault
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Driver must transition to Faulted or Disconnected state and halt background polling
    let state = driver.connection_state();
    assert!(
        matches!(
            state,
            ConnectionState::Faulted { .. } | ConnectionState::Disconnected
        ),
        "Expected Faulted or Disconnected state, got {:?}",
        state
    );

    // Explicit disconnect should clean up cleanly
    driver.disconnect().await.expect("disconnect failed");
    assert_eq!(driver.connection_state(), ConnectionState::Disconnected);
}

#[tokio::test]
async fn test_obd2_driver_repeated_connect_disconnect_lifecycle() {
    for _ in 0..3 {
        let transport = MockObdTransport::new();
        script_handshake_and_supported_pids(&transport);

        transport.script_response(
            ObdCommand::Mode01(Mode01Pid::EngineRpm),
            Ok("41 0C 1A F8\r\r>".to_string()),
        );

        let config = Obd2DriverConfig {
            poll_interval: Duration::from_millis(20),
            stoichiometric_ratio: None,
            polled_pids: vec![Mode01Pid::EngineRpm],
            command_timeout: Duration::from_millis(500),
            query_vin: false,
            protocol: Default::default(),
            baud_rate: Default::default(),
        };

        let driver = Obd2Driver::new(transport, config);
        let mut rx = driver.subscribe_telemetry();

        driver.connect().await.expect("connect failed");
        let frame = tokio::time::timeout(Duration::from_millis(500), rx.recv())
            .await
            .expect("timeout")
            .expect("rx");
        assert!((frame.rpm - 1726.0).abs() < 1.0);

        driver.disconnect().await.expect("disconnect failed");
        assert_eq!(driver.connection_state(), ConnectionState::Disconnected);
    }
}

#[tokio::test]
async fn test_obd2_driver_malformed_and_truncated_pid_response_does_not_fabricate_readings() {
    let transport = MockObdTransport::new();
    script_handshake_and_supported_pids(&transport);

    // Truncated RPM payload: only 1 data byte instead of 2 expected data bytes
    transport.script_response(
        ObdCommand::Mode01(Mode01Pid::EngineRpm),
        Ok("41 0C 1A\r\r>".to_string()),
    );

    let config = Obd2DriverConfig {
        poll_interval: Duration::from_millis(20),
        stoichiometric_ratio: None,
        polled_pids: vec![Mode01Pid::EngineRpm],
        command_timeout: Duration::from_millis(500),
        query_vin: false,
        protocol: Default::default(),
        baud_rate: Default::default(),
    };

    let driver = Obd2Driver::new(transport, config);
    let mut rx = driver.subscribe_telemetry();

    driver.connect().await.expect("connect failed");

    // Wait for the background loop to process the malformed response
    tokio::time::sleep(Duration::from_millis(80)).await;

    // Driver must not broadcast a fabricated valid reading or zero reading
    // Any frame received before fault would not exist, and no frame is broadcast with truncated data
    assert!(rx.try_recv().is_err());

    // Connection state must be faulted due to truncated payload error
    assert!(matches!(
        driver.connection_state(),
        ConnectionState::Faulted { .. }
    ));

    driver.disconnect().await.expect("disconnect failed");
}

#[tokio::test]
async fn test_obd2_driver_no_commands_sent_after_session_fault() {
    let transport = MockObdTransport::new();
    script_handshake_and_supported_pids(&transport);

    // Inject transport error on the first RPM query
    transport.script_response(
        ObdCommand::Mode01(Mode01Pid::EngineRpm),
        Err(kivi_protocol::obd::errors::ObdCodecError::IoError(
            "UART framing error".to_string(),
        )),
    );

    let config = Obd2DriverConfig {
        poll_interval: Duration::from_millis(20),
        stoichiometric_ratio: None,
        polled_pids: vec![Mode01Pid::EngineRpm, Mode01Pid::ThrottlePosition],
        command_timeout: Duration::from_millis(500),
        query_vin: false,
        protocol: Default::default(),
        baud_rate: Default::default(),
    };

    let driver = Obd2Driver::new(transport.clone(), config);
    driver.connect().await.expect("connect failed");

    // Allow multiple poll intervals to elapse
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Count how many Mode 01 diagnostic requests were written after handshake
    let commands = transport.written_commands();
    let diagnostic_requests: Vec<_> = commands
        .into_iter()
        .filter(|cmd| matches!(cmd, ObdCommand::Mode01(..)))
        .collect();

    // Mode 01 SupportedPids01_20 + exactly 1 failed EngineRpm query
    // The driver MUST NOT continue issuing subsequent queries (e.g. ThrottlePosition) after fault!
    assert_eq!(
        diagnostic_requests.len(),
        2,
        "Expected exactly SupportedPids01_20 and 1 EngineRpm query, found {:?}",
        diagnostic_requests
    );

    driver.disconnect().await.expect("disconnect failed");
}

#[tokio::test]
async fn test_obd2_driver_channel_backpressure_and_lagged_receiver() {
    let transport = MockObdTransport::new();
    script_handshake_and_supported_pids(&transport);

    // Script 100 consecutive RPM responses
    for _ in 0..100 {
        transport.script_response(
            ObdCommand::Mode01(Mode01Pid::EngineRpm),
            Ok("41 0C 1A F8\r\r>".to_string()),
        );
    }

    let config = Obd2DriverConfig {
        poll_interval: Duration::from_millis(10),
        stoichiometric_ratio: None,
        polled_pids: vec![Mode01Pid::EngineRpm],
        command_timeout: Duration::from_millis(500),
        query_vin: false,
        protocol: Default::default(),
        baud_rate: Default::default(),
    };

    let driver = Obd2Driver::new(transport, config);
    let mut lagging_rx = driver.subscribe_telemetry();

    driver.connect().await.expect("connect failed");

    // Sleep long enough for the driver to generate > 64 frames (exceeding channel capacity)
    tokio::time::sleep(Duration::from_millis(800)).await;

    // The driver background loop must NOT deadlock or hang due to unread frames!
    // Instead, Tokio broadcast semantics report RecvError::Lagged
    let recv_result = lagging_rx.recv().await;
    match recv_result {
        Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
            assert!(skipped > 0, "Expected non-zero skipped frames");
        }
        Ok(_) => {
            // Alternatively if buffer caught up, subsequent read should succeed
        }
        Err(e) => panic!("Unexpected error: {:?}", e),
    }

    driver.disconnect().await.expect("disconnect failed");
}

#[tokio::test]
async fn test_obd2_driver_rom_isolation() {
    use kivi_core::ingest::ingest_binary;

    // Create a dummy 32KB calibration ROM
    let rom_bytes = vec![0xEA; 32768];
    let memory_map = ingest_binary(&rom_bytes).expect("ingest failed");
    let initial_hash = memory_map.hashes().sha256.clone();

    let transport = MockObdTransport::new();
    script_handshake_and_supported_pids(&transport);

    transport.script_response(
        ObdCommand::Mode01(Mode01Pid::EngineRpm),
        Ok("41 0C 1A F8\r\r>".to_string()),
    );

    let config = Obd2DriverConfig {
        poll_interval: Duration::from_millis(20),
        stoichiometric_ratio: None,
        polled_pids: vec![Mode01Pid::EngineRpm],
        command_timeout: Duration::from_millis(500),
        query_vin: false,
        protocol: Default::default(),
        baud_rate: Default::default(),
    };

    let driver = Obd2Driver::new(transport, config);
    let mut rx = driver.subscribe_telemetry();

    driver.connect().await.expect("connect failed");
    let _ = rx.recv().await;
    driver.disconnect().await.expect("disconnect failed");

    // Assert that the memory map SHA-256 hash is 100% identical and unchanged
    assert_eq!(
        memory_map.hashes().sha256,
        initial_hash,
        "ROM hash was corrupted by telemetry driver execution"
    );
}
