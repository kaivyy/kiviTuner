use async_trait::async_trait;
use kivi_protocol::driver::{DriverDescriptor, EcuDriver, TelemetrySource};
use kivi_protocol::errors::ProtocolError;
use kivi_protocol::model::{ConnectionState, EcuIdentity, TelemetryFrame};
use kivi_protocol::session::ProtocolSessionManager;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;

struct MockDriver {
    connected: AtomicBool,
    tx: broadcast::Sender<TelemetryFrame>,
    fail_on_connect: bool,
}

impl MockDriver {
    fn new() -> Self {
        let (tx, _) = broadcast::channel(16);
        Self {
            connected: AtomicBool::new(false),
            tx,
            fail_on_connect: false,
        }
    }

    fn failing() -> Self {
        let (tx, _) = broadcast::channel(16);
        Self {
            connected: AtomicBool::new(false),
            tx,
            fail_on_connect: true,
        }
    }
}

#[async_trait]
impl EcuDriver for MockDriver {
    fn descriptor(&self) -> DriverDescriptor {
        DriverDescriptor {
            id: "mock".to_string(),
            name: "Mock Driver".to_string(),
            is_simulator: true,
        }
    }

    async fn connect(&self) -> Result<EcuIdentity, ProtocolError> {
        if self.fail_on_connect {
            return Err(ProtocolError::HandshakeFailed(
                "ECU did not acknowledge wake-up pulse".to_string(),
            ));
        }
        self.connected.store(true, Ordering::SeqCst);
        Ok(EcuIdentity {
            ecu_id: "MOCK_ECU_01".to_string(),
            vin: None,
            software_version: "1.0.0".to_string(),
            hardware_version: "REV_A".to_string(),
            protocol_name: "MOCK_PROTOCOL".to_string(),
            is_simulated: true,
        })
    }

    async fn disconnect(&self) -> Result<(), ProtocolError> {
        self.connected.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn connection_state(&self) -> ConnectionState {
        if self.connected.load(Ordering::SeqCst) {
            ConnectionState::Connected {
                identity: EcuIdentity {
                    ecu_id: "MOCK_ECU_01".to_string(),
                    vin: None,
                    software_version: "1.0.0".to_string(),
                    hardware_version: "REV_A".to_string(),
                    protocol_name: "MOCK_PROTOCOL".to_string(),
                    is_simulated: true,
                },
            }
        } else {
            ConnectionState::Disconnected
        }
    }
}

impl TelemetrySource for MockDriver {
    fn subscribe_telemetry(&self) -> broadcast::Receiver<TelemetryFrame> {
        self.tx.subscribe()
    }
}

#[tokio::test]
async fn test_session_manager_connect_and_disconnect() {
    let mut manager = ProtocolSessionManager::new();
    let driver = Arc::new(MockDriver::new());

    assert_eq!(manager.current_state(), ConnectionState::Disconnected);
    assert_eq!(manager.active_descriptor(), None);

    let identity = manager.connect(driver).await.expect("connect failed");
    assert_eq!(identity.ecu_id, "MOCK_ECU_01");
    assert!(matches!(
        manager.current_state(),
        ConnectionState::Connected { .. }
    ));

    let desc = manager.active_descriptor().expect("descriptor should exist");
    assert_eq!(desc.id, "mock");
    assert_eq!(desc.name, "Mock Driver");
    assert!(desc.is_simulator);

    manager.disconnect().await.expect("disconnect failed");
    assert_eq!(manager.current_state(), ConnectionState::Disconnected);
    assert_eq!(manager.active_descriptor(), None);
}

#[tokio::test]
async fn test_cannot_connect_twice_concurrently() {
    let mut manager = ProtocolSessionManager::new();
    let driver1 = Arc::new(MockDriver::new());
    let driver2 = Arc::new(MockDriver::new());

    manager.connect(driver1).await.expect("first connect failed");
    let err = manager
        .connect(driver2)
        .await
        .expect_err("second connect must fail");
    assert!(matches!(err, ProtocolError::PortBusy(_)));
}

#[tokio::test]
async fn test_subscribe_telemetry_flow() {
    let mut manager = ProtocolSessionManager::new();
    let driver = Arc::new(MockDriver::new());

    // Subscribe before connecting should fail with Disconnected
    let sub_err = manager.subscribe().expect_err("should fail when disconnected");
    assert!(matches!(sub_err, ProtocolError::Disconnected(_)));

    manager.connect(driver.clone()).await.expect("connect failed");

    let mut rx = manager.subscribe().expect("subscribe should succeed");
    let frame = TelemetryFrame {
        timestamp_ms: 1000,
        sequence: 42,
        rpm: 3500.0,
        ..Default::default()
    };

    let sent_count = driver.tx.send(frame.clone()).expect("broadcast send");
    assert!(sent_count >= 1);

    let received = rx.recv().await.expect("receive frame");
    assert_eq!(received.timestamp_ms, 1000);
    assert_eq!(received.sequence, 42);
    assert_eq!(received.rpm, 3500.0);
}

#[tokio::test]
async fn test_connect_error_propagation_and_recovery() {
    let mut manager = ProtocolSessionManager::new();
    let failing_driver = Arc::new(MockDriver::failing());

    let err = manager
        .connect(failing_driver)
        .await
        .expect_err("should fail to connect");
    assert!(matches!(err, ProtocolError::HandshakeFailed(_)));
    assert_eq!(manager.current_state(), ConnectionState::Disconnected);
    assert_eq!(manager.active_descriptor(), None);

    // Can subsequently connect with working driver
    let good_driver = Arc::new(MockDriver::new());
    let id = manager.connect(good_driver).await.expect("subsequent connect succeeds");
    assert_eq!(id.ecu_id, "MOCK_ECU_01");
}

#[tokio::test]
async fn test_disconnect_when_idle_is_ok() {
    let mut manager = ProtocolSessionManager::default();
    assert_eq!(manager.current_state(), ConnectionState::Disconnected);
    assert!(manager.disconnect().await.is_ok());
}
