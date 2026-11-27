//! Non-hardware unit tests for Milestone 3D-2B.2A:
//! Physical Serial OBD-II Transport implementation (`SerialObdTransport`).
//!
//! Verifies:
//! - Default configuration and settling-delay customization.
//! - Closed-state write and read guards.
//! - Idempotent close behavior consistent with the `ObdTransport` trait.
//! - OS-appropriate nonexistent-device-path failure mapping.
//! - Disconnected state preservation upon failed open.

use kivi_protocol::obd::command::{AdapterCommand, Mode01Pid, ObdCommand};
use kivi_protocol::obd::errors::ObdCodecError;
use kivi_protocol::obd::framing::SerialBaudRate;
use kivi_protocol::obd::serial::{SerialObdTransport, DEFAULT_SERIAL_SETTLING_DELAY};
use kivi_protocol::obd::transport::ObdTransport;
use std::time::Duration;

#[test]
fn test_serial_transport_default_configuration_and_setters() {
    let transport = SerialObdTransport::new("/dev/ttyUSB0", SerialBaudRate::Baud115200);

    assert_eq!(transport.path(), "/dev/ttyUSB0");
    assert_eq!(transport.baud_rate(), SerialBaudRate::Baud115200);
    assert_eq!(transport.settling_delay(), DEFAULT_SERIAL_SETTLING_DELAY);
    assert!(!transport.is_connected());

    let custom = transport.with_settling_delay(Duration::from_millis(150));
    assert_eq!(custom.settling_delay(), Duration::from_millis(150));
    assert_eq!(custom.path(), "/dev/ttyUSB0");
    assert_eq!(custom.baud_rate(), SerialBaudRate::Baud115200);
}

#[tokio::test]
async fn test_serial_transport_closed_state_write_guard() {
    let mut transport = SerialObdTransport::new("/dev/ttyUSB0", SerialBaudRate::Baud115200);
    assert!(!transport.is_connected());

    let write_res = transport
        .write_command(ObdCommand::Adapter(AdapterCommand::Reset))
        .await;

    match write_res {
        Err(ObdCodecError::InvalidSessionState { current, expected }) => {
            assert_eq!(current, "Closed");
            assert_eq!(expected, "Open");
        }
        other => panic!("Expected InvalidSessionState, got: {:?}", other),
    }

    let write_res_pid = transport
        .write_command(ObdCommand::Mode01(Mode01Pid::EngineRpm))
        .await;

    match write_res_pid {
        Err(ObdCodecError::InvalidSessionState { current, expected }) => {
            assert_eq!(current, "Closed");
            assert_eq!(expected, "Open");
        }
        other => panic!("Expected InvalidSessionState, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_serial_transport_closed_state_read_guard() {
    let mut transport = SerialObdTransport::new("/dev/ttyUSB0", SerialBaudRate::Baud115200);
    assert!(!transport.is_connected());

    let read_res = transport
        .read_response(Duration::from_millis(100), 1024)
        .await;

    match read_res {
        Err(ObdCodecError::InvalidSessionState { current, expected }) => {
            assert_eq!(current, "Closed");
            assert_eq!(expected, "Open");
        }
        other => panic!("Expected InvalidSessionState, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_serial_transport_idempotent_close_when_disconnected() {
    let mut transport = SerialObdTransport::new("/dev/ttyUSB0", SerialBaudRate::Baud115200);
    assert!(!transport.is_connected());

    // Closing when already disconnected must succeed and remain disconnected
    transport
        .close()
        .await
        .expect("Close on disconnected transport failed");
    assert!(!transport.is_connected());

    // Second consecutive close call is idempotent
    transport
        .close()
        .await
        .expect("Second close on disconnected transport failed");
    assert!(!transport.is_connected());
}

#[tokio::test]
async fn test_serial_transport_nonexistent_device_path_fails_open() {
    #[cfg(unix)]
    let nonexistent_path = "/dev/kivituner_nonexistent_serial_device_test_node_xyz";
    #[cfg(windows)]
    let nonexistent_path = "COM999";
    #[cfg(not(any(unix, windows)))]
    let nonexistent_path = "nonexistent_serial_test_port";

    let mut transport = SerialObdTransport::new(nonexistent_path, SerialBaudRate::Baud115200)
        .with_settling_delay(Duration::ZERO);

    assert!(!transport.is_connected());

    let open_res = transport.open().await;

    match open_res {
        Err(ObdCodecError::IoError(err_msg)) => {
            assert!(
                err_msg.contains("failed to open serial port"),
                "Expected open failure error message, got: {}",
                err_msg
            );
            assert!(err_msg.contains(nonexistent_path));
        }
        other => panic!(
            "Expected IoError for nonexistent serial port, got: {:?}",
            other
        ),
    }

    // Transport must remain disconnected on failed open
    assert!(!transport.is_connected());

    // Subsequent write must still fail closed
    let write_res = transport
        .write_command(ObdCommand::Adapter(AdapterCommand::Reset))
        .await;
    assert!(matches!(
        write_res,
        Err(ObdCodecError::InvalidSessionState { .. })
    ));
}
