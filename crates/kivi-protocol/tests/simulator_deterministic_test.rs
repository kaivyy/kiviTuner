use kivi_protocol::clock::MockClock;
use kivi_protocol::driver::{EcuDriver, TelemetrySource};
use kivi_protocol::faults::SimFault;
use kivi_protocol::model::ConnectionState;
use kivi_protocol::simulator::{SimMode, VirtualEcuSimulator};
use kivi_protocol::validation::{validate_telemetry_frame, SensorRangeError};
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn test_simulator_deterministic_repeatability_with_same_seed() {
    let sim1 = VirtualEcuSimulator::new_with_seed(42, SimMode::DynoPull);
    let sim2 = VirtualEcuSimulator::new_with_seed(42, SimMode::DynoPull);

    sim1.connect().await.expect("sim1 connect");
    sim2.connect().await.expect("sim2 connect");

    let mut rx1 = sim1.subscribe_telemetry();
    let mut rx2 = sim2.subscribe_telemetry();

    // Trigger 10 deterministic steps
    for _ in 0..10 {
        sim1.step_tick();
        sim2.step_tick();

        let f1 = rx1.recv().await.expect("rx1 frame");
        let f2 = rx2.recv().await.expect("rx2 frame");

        assert_eq!(f1.rpm, f2.rpm);
        assert_eq!(f1.tps, f2.tps);
        assert_eq!(f1.afr, f2.afr);
        assert_eq!(f1.map_kpa, f2.map_kpa);
        assert_eq!(f1.ignition_advance_deg, f2.ignition_advance_deg);
    }

    sim1.disconnect().await.expect("sim1 disconnect");
    sim2.disconnect().await.expect("sim2 disconnect");
}

#[tokio::test]
async fn test_simulator_fault_injection_disconnect() {
    let sim = VirtualEcuSimulator::new_with_seed(100, SimMode::IdleWarmup);
    sim.connect().await.expect("connect");

    assert!(matches!(sim.connection_state(), ConnectionState::Connected { .. }));

    // Inject abrupt disconnect fault
    sim.inject_fault(SimFault::Disconnect);
    sim.step_tick();

    assert!(matches!(sim.connection_state(), ConnectionState::Faulted { .. }));

    // Clear fault: must restore Connected state
    sim.inject_fault(SimFault::None);
    assert!(matches!(sim.connection_state(), ConnectionState::Connected { .. }));
}

#[tokio::test]
async fn test_simulator_clone_drop_does_not_abort_stream() {
    let sim = VirtualEcuSimulator::new_with_seed(555, SimMode::IdleWarmup);
    sim.connect().await.expect("connect");
    let mut rx = sim.subscribe_telemetry();

    // Create a clone and immediately drop it
    {
        let _clone = sim.clone();
    }

    // Background task must still be running and broadcasting frames
    let frame = tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv())
        .await
        .expect("timed out waiting for frame after clone drop")
        .expect("rx closed prematurely");
    assert!(frame.is_simulated);

    sim.disconnect().await.expect("disconnect");
}

#[tokio::test]
async fn test_simulator_all_physics_modes_produce_valid_telemetry() {
    let modes = [
        SimMode::IdleWarmup,
        SimMode::DynoPull,
        SimMode::StreetTrack,
        SimMode::LowBatteryFault,
    ];

    for mode in modes {
        let sim = VirtualEcuSimulator::new_with_seed(777, mode);
        for _ in 0..50 {
            let frame = sim.step_tick();
            let report = validate_telemetry_frame(&frame);
            assert!(
                report.is_valid(),
                "Mode {:?} produced invalid frame at sequence {}: {:?}",
                mode,
                frame.sequence,
                report.errors
            );
        }
    }
}

#[tokio::test]
async fn test_simulator_low_battery_mode_sets_flag() {
    let sim = VirtualEcuSimulator::new_with_seed(888, SimMode::LowBatteryFault);
    let frame = sim.step_tick();
    assert!(frame.status_flags.low_voltage);
    assert!((frame.battery_volts - 10.4).abs() < 0.5);
}

#[tokio::test]
async fn test_simulator_fault_injection_invalid_frame() {
    let sim = VirtualEcuSimulator::new_with_seed(123, SimMode::IdleWarmup);
    sim.inject_fault(SimFault::InvalidFrame);
    let frame = sim.step_tick();

    let report = validate_telemetry_frame(&frame);
    assert!(!report.is_valid());
    assert!(frame.status_flags.communication_fault);
}

#[tokio::test]
async fn test_simulator_fault_injection_sensor_range() {
    let sim = VirtualEcuSimulator::new_with_seed(123, SimMode::DynoPull);
    sim.inject_fault(SimFault::SensorRange {
        sensor: "rpm".to_string(),
        value: 25000.0,
    });
    let frame = sim.step_tick();

    assert_eq!(frame.rpm, 25000.0);
    let report = validate_telemetry_frame(&frame);
    assert!(!report.is_valid());
    assert!(report
        .errors
        .iter()
        .any(|e| matches!(e, SensorRangeError::RpmOutOfRange(v) if *v == 25000.0)));
}

#[tokio::test]
async fn test_simulator_fault_injection_stale_samples() {
    let sim = VirtualEcuSimulator::new_with_seed(555, SimMode::DynoPull);
    sim.inject_fault(SimFault::StaleSamples(3));

    let f1 = sim.step_tick();
    assert!(f1.status_flags.stale_sample);

    let f2 = sim.step_tick();
    assert!(f2.status_flags.stale_sample);
    assert_eq!(f1.rpm, f2.rpm);

    let f3 = sim.step_tick();
    assert!(f3.status_flags.stale_sample);
    assert_eq!(f1.rpm, f3.rpm);

    let f4 = sim.step_tick();
    assert!(!f4.status_flags.stale_sample);
}

#[tokio::test]
async fn test_simulator_mock_clock_advancement() {
    let clock = Arc::new(MockClock::new(5000));
    let sim = VirtualEcuSimulator::new_with_clock(999, SimMode::IdleWarmup, clock.clone());

    let f1 = sim.step_tick();
    assert_eq!(f1.timestamp_ms, 5000);

    clock.advance_ms(50);
    let f2 = sim.step_tick();
    assert_eq!(f2.timestamp_ms, 5050);
}

#[tokio::test]
async fn test_street_track_overrun_fuel_cut() {
    let sim = VirtualEcuSimulator::new_with_seed(333, SimMode::StreetTrack);
    let mut saw_overrun_cut = false;

    for _ in 0..200 {
        let frame = sim.step_tick();
        if frame.afr > 20.0 && frame.injector_pulse_width_ms == 0.0 && frame.tps == 0.0 {
            saw_overrun_cut = true;
            break;
        }
    }

    assert!(saw_overrun_cut, "Expected overrun fuel cut in StreetTrack mode");
}

#[tokio::test]
async fn test_dyno_pull_soft_rev_limiter() {
    let sim = VirtualEcuSimulator::new_with_seed(444, SimMode::DynoPull);

    // Step 120 ticks to reach rev cut region (6 seconds sweep at 20 Hz)
    for _ in 0..120 {
        sim.step_tick();
    }

    let cut_frame = sim.step_tick();
    assert!(
        cut_frame.rpm >= 10500.0 && cut_frame.rpm <= 11100.0,
        "Expected RPM in limiter region, got {}",
        cut_frame.rpm
    );
}

#[tokio::test]
async fn test_simulator_background_streaming_loop() {
    let sim = VirtualEcuSimulator::new_with_seed(666, SimMode::IdleWarmup);
    let mut rx = sim.subscribe_telemetry();

    sim.connect().await.expect("connect");

    // Wait for at least 2 frames streamed automatically by background 20 Hz task
    let f1 = tokio::time::timeout(Duration::from_millis(300), rx.recv())
        .await
        .expect("timeout waiting for frame 1")
        .expect("recv frame 1");

    let f2 = tokio::time::timeout(Duration::from_millis(300), rx.recv())
        .await
        .expect("timeout waiting for frame 2")
        .expect("recv frame 2");

    assert!(f2.sequence > f1.sequence);

    sim.disconnect().await.expect("disconnect");
}

#[tokio::test]
async fn test_simulator_ecu_driver_trait_implementation() {
    let sim = VirtualEcuSimulator::new(SimMode::IdleWarmup);
    let driver: Arc<dyn EcuDriver> = Arc::new(sim);
    let desc = driver.descriptor();
    assert_eq!(desc.id, "virtual_bench_ecu");
    assert!(desc.is_simulator);

    let identity = driver.connect().await.expect("connect via trait");
    assert_eq!(identity.ecu_id, "SIM-BENCH-01");
    assert!(matches!(driver.connection_state(), ConnectionState::Connected { .. }));

    driver.disconnect().await.expect("disconnect via trait");
    assert_eq!(driver.connection_state(), ConnectionState::Disconnected);
}

#[tokio::test]
async fn test_simulator_telemetry_source_trait_implementation() {
    let sim = VirtualEcuSimulator::new(SimMode::IdleWarmup);
    let source: Arc<dyn TelemetrySource> = Arc::new(sim.clone());
    let mut rx = source.subscribe_telemetry();
    let sent_frame = sim.step_tick();
    let received = rx.recv().await.expect("recv via trait source");
    assert_eq!(sent_frame.sequence, received.sequence);
}

#[test]
fn test_sim_fault_and_mode_serde_roundtrip() {
    let faults = [
        SimFault::None,
        SimFault::Timeout,
        SimFault::InvalidFrame,
        SimFault::Disconnect,
        SimFault::StaleSamples(5),
        SimFault::SensorRange {
            sensor: "tps".to_string(),
            value: 99.5,
        },
    ];

    for fault in faults {
        let json = serde_json::to_string(&fault).expect("serialize SimFault");
        let deserialized: SimFault = serde_json::from_str(&json).expect("deserialize SimFault");
        assert_eq!(fault, deserialized);
    }

    let modes = [
        SimMode::IdleWarmup,
        SimMode::DynoPull,
        SimMode::StreetTrack,
        SimMode::LowBatteryFault,
    ];

    for mode in modes {
        let json = serde_json::to_string(&mode).expect("serialize SimMode");
        let deserialized: SimMode = serde_json::from_str(&json).expect("deserialize SimMode");
        assert_eq!(mode, deserialized);
    }
}
