use kivi_protocol::driver::EcuDriver;
use kivi_protocol::faults::SimFault;
use kivi_protocol::model::ConnectionState;
use kivi_protocol::simulator::{SimMode, VirtualEcuSimulator};
use kivituner::commands::{
    connect_ecu_inner, disconnect_ecu_inner, get_connection_status_inner, get_latest_telemetry_inner,
    list_ecu_drivers_inner,
};
use kivituner::{AppState, TelemetryManager};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

#[test]
fn test_list_ecu_drivers_has_no_virtual_simulators() {
    let state = AppState::default();
    let drivers = list_ecu_drivers_inner(&state).expect("Failed to list drivers");
    assert!(drivers.iter().all(|d| d.id != "virtual_bench" && !d.is_simulator));
}

#[tokio::test]
async fn test_initial_status_and_telemetry_empty() {
    let state = AppState::default();
    let status = get_connection_status_inner(&state).await.expect("Status failed");
    assert_eq!(status, ConnectionState::Disconnected);

    let latest = get_latest_telemetry_inner(&state).await.expect("Telemetry query failed");
    assert!(latest.is_none());
}

#[tokio::test]
async fn test_connect_virtual_or_unknown_driver_fails() {
    let state = AppState::default();
    let err_missing = connect_ecu_inner("non_existent_serial_port", None, &state)
        .await
        .unwrap_err();
    assert!(!err_missing.is_empty());

    let err_sim = connect_ecu_inner("virtual_bench", None, &state)
        .await
        .unwrap_err();
    assert!(err_sim.contains("Unknown hardware telemetry driver"));
}

#[tokio::test]
async fn test_connect_and_disconnect_lifecycle_via_trait_injection() {
    let state = AppState::default();
    let sim = Arc::new(VirtualEcuSimulator::new(SimMode::IdleWarmup));

    let identity = state
        .telemetry
        .connect_driver(sim.clone(), None)
        .await
        .expect("Connect failed");
    assert_eq!(identity.ecu_id, "SIM-BENCH-01");

    let status = get_connection_status_inner(&state).await.expect("Status query failed");
    match status {
        ConnectionState::Connected { identity: id } => assert_eq!(id.ecu_id, "SIM-BENCH-01"),
        _ => panic!("Expected Connected state, got {:?}", status),
    }

    let second_connect = state.telemetry.connect_driver(sim.clone(), None).await;
    assert!(second_connect.is_err());

    disconnect_ecu_inner(None, &state).await.expect("Disconnect failed");
    assert_eq!(
        get_connection_status_inner(&state).await.expect("Status query failed"),
        ConnectionState::Disconnected
    );
    disconnect_ecu_inner(None, &state).await.expect("Idempotent disconnect failed");
}

#[tokio::test]
async fn test_telemetry_streaming_modes_and_fault_injection() {
    let state = AppState::default();
    let sim = Arc::new(VirtualEcuSimulator::new(SimMode::IdleWarmup));
    state.telemetry.connect_driver(sim.clone(), None).await.expect("Connect failed");

    sim.set_mode(SimMode::DynoPull);
    sim.inject_fault(SimFault::SensorRange {
        sensor: "rpm".to_string(),
        value: 12500.0,
    });
    sim.inject_fault(SimFault::None);

    sleep(Duration::from_millis(150)).await;
    let frame = get_latest_telemetry_inner(&state)
        .await
        .expect("Get telemetry failed")
        .expect("Expected cached frame after 150ms");
    assert!(frame.rpm >= 500.0 && frame.rpm <= 13000.0);
    assert!(frame.battery_volts > 9.0 && frame.battery_volts < 16.0);

    disconnect_ecu_inner(None, &state).await.expect("Disconnect failed");
}

#[tokio::test]
async fn test_telemetry_manager_direct_lifecycle() {
    let manager = TelemetryManager::new();
    assert!(manager.list_drivers().iter().all(|d| !d.is_simulator));
    assert_eq!(manager.get_status().await, ConnectionState::Disconnected);

    let sim = Arc::new(VirtualEcuSimulator::new(SimMode::IdleWarmup));
    let identity = manager.connect_driver(sim.clone(), None).await.expect("Connect failed");
    assert_eq!(identity.ecu_id, "SIM-BENCH-01");
    assert!(matches!(manager.get_status().await, ConnectionState::Connected { .. }));

    sim.set_mode(SimMode::DynoPull);
    sleep(Duration::from_millis(120)).await;
    assert!(manager.get_latest_frame().await.is_some());

    manager.disconnect(None).await.expect("Disconnect failed");
    assert_eq!(manager.get_status().await, ConnectionState::Disconnected);
    let _ = sim.descriptor();
}
