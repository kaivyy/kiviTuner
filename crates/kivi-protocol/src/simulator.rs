//! Deterministic virtual ECU bench simulator and fault injection framework.

use crate::clock::{Clock, SystemMonotonicClock};
use crate::driver::{DriverDescriptor, EcuDriver, TelemetrySource};
use crate::errors::ProtocolError;
use crate::faults::SimFault;
use crate::model::{ConnectionState, EcuIdentity, TelemetryFrame, TelemetryStatusFlags};
use crate::worker::create_telemetry_broadcast_channel;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

/// Linear Congruential Generator (LCG) for deterministic pseudo-random sequence generation.
/// Uses Knuth MMIX constants (multiplier = 6364136223846793005, increment = 1).
#[derive(Debug, Clone)]
pub struct LcgRng {
    state: u64,
}

impl LcgRng {
    /// Creates a new LCG generator initialized with the specified seed.
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Advances the LCG state and returns the next 64-bit integer.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1);
        self.state
    }

    /// Returns a deterministic pseudo-random float in `[0.0, 1.0)`.
    pub fn next_f32(&mut self) -> f32 {
        let v = (self.next_u64() >> 40) as u32;
        v as f32 / 16777216.0
    }

    /// Returns a deterministic pseudo-random float centered at zero in `[-range, range]`.
    pub fn next_range(&mut self, range: f32) -> f32 {
        (self.next_f32() * 2.0 - 1.0) * range
    }
}

/// Operational physics scenario mode for the virtual ECU simulator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SimMode {
    /// Cold start fast idle (1800 rpm) warming to closed-loop operating temp (85°C, 1350 rpm).
    #[default]
    IdleWarmup,
    /// Wide Open Throttle sweep from 2500 to 11000 rpm over 6 seconds with rev limiter cut.
    DynoPull,
    /// Multi-gear progression (1-4) with overrun fuel cut on lift-off deceleration.
    StreetTrack,
    /// Low system voltage condition (10.4V) with diagnostic warning flags asserted.
    LowBatteryFault,
}

struct SimulatorState {
    mode: SimMode,
    fault: SimFault,
    sequence: u64,
    rng: LcgRng,
    last_frame: Option<TelemetryFrame>,
    connection_state: ConnectionState,
    auto_tick: bool,
}

struct SimulatorInner {
    state: Mutex<SimulatorState>,
    clock: Arc<dyn Clock>,
    tx: broadcast::Sender<TelemetryFrame>,
    bg_task: Mutex<Option<JoinHandle<()>>>,
}

/// Default simulated identity parameters for virtual bench ECU.
pub fn default_simulator_identity() -> EcuIdentity {
    EcuIdentity {
        ecu_id: "SIM-BENCH-01".to_string(),
        vin: Some("VIRTUAL0000000001".to_string()),
        software_version: "2.4.0-sim".to_string(),
        hardware_version: "BENCH-REV-3".to_string(),
        protocol_name: "VirtualBench".to_string(),
        is_simulated: true,
    }
}

/// High-fidelity deterministic virtual ECU simulator.
///
/// Implements [`EcuDriver`] and [`TelemetrySource`], broadcasting realistic telemetry frames
/// at nominal 20 Hz (50 ms interval) or stepping synchronously via [`step_tick`](VirtualEcuSimulator::step_tick).
#[derive(Clone)]
pub struct VirtualEcuSimulator {
    inner: Arc<SimulatorInner>,
}

impl Default for VirtualEcuSimulator {
    fn default() -> Self {
        Self::new(SimMode::IdleWarmup)
    }
}

impl VirtualEcuSimulator {
    /// Creates a simulator with the specified mode and default seed (42) and system monotonic clock.
    pub fn new(mode: SimMode) -> Self {
        Self::new_with_seed(42, mode)
    }

    /// Creates a simulator with the specified seed, mode, and system monotonic clock.
    pub fn new_with_seed(seed: u64, mode: SimMode) -> Self {
        Self::new_with_clock(seed, mode, Arc::new(SystemMonotonicClock::new()))
    }

    /// Creates a simulator with the specified seed, mode, and custom clock.
    pub fn new_with_clock(seed: u64, mode: SimMode, clock: Arc<dyn Clock>) -> Self {
        let (tx, _) = create_telemetry_broadcast_channel();
        let state = SimulatorState {
            mode,
            fault: SimFault::None,
            sequence: 0,
            rng: LcgRng::new(seed),
            last_frame: None,
            connection_state: ConnectionState::Disconnected,
            auto_tick: true,
        };

        Self {
            inner: Arc::new(SimulatorInner {
                state: Mutex::new(state),
                clock,
                tx,
                bg_task: Mutex::new(None),
            }),
        }
    }

    /// Updates the active simulation physics mode.
    pub fn set_mode(&self, mode: SimMode) {
        let mut state = self.inner.state.lock().unwrap_or_else(|e| e.into_inner());
        state.mode = mode;
    }

    /// Returns the currently active simulation physics mode.
    pub fn mode(&self) -> SimMode {
        self.inner.state.lock().unwrap_or_else(|e| e.into_inner()).mode
    }

    /// Injects a fault condition into the simulator.
    pub fn inject_fault(&self, fault: SimFault) {
        let mut state = self.inner.state.lock().unwrap_or_else(|e| e.into_inner());
        match &fault {
            SimFault::None => {
                if matches!(state.connection_state, ConnectionState::Faulted { .. }) {
                    state.connection_state = ConnectionState::Connected {
                        identity: default_simulator_identity(),
                    };
                }
            }
            SimFault::Disconnect => {
                state.connection_state = ConnectionState::Faulted {
                    error: "ECU physical link disconnected".to_string(),
                };
            }
            _ => {}
        }
        state.fault = fault;
    }

    /// Returns the currently active fault injection specification.
    pub fn fault(&self) -> SimFault {
        self.inner.state.lock().unwrap_or_else(|e| e.into_inner()).fault.clone()
    }

    /// Enables or disables automatic 20 Hz ticking in the background Tokio task.
    pub fn set_auto_tick(&self, enabled: bool) {
        let mut state = self.inner.state.lock().unwrap_or_else(|e| e.into_inner());
        state.auto_tick = enabled;
    }

    /// Returns whether background automatic ticking is enabled.
    pub fn is_auto_tick_enabled(&self) -> bool {
        self.inner.state.lock().unwrap_or_else(|e| e.into_inner()).auto_tick
    }

    /// Returns the current sequence counter.
    pub fn sequence(&self) -> u64 {
        self.inner.state.lock().unwrap_or_else(|e| e.into_inner()).sequence
    }

    /// Executes a single deterministic simulation step, broadcasts the frame, and returns it.
    pub fn step_tick(&self) -> TelemetryFrame {
        self.inner.step_tick()
    }

    /// Connects to the virtual ECU, spawning the 20 Hz background streaming task.
    pub async fn connect(&self) -> Result<EcuIdentity, ProtocolError> {
        let mut state = self.inner.state.lock().unwrap_or_else(|e| e.into_inner());

        if let SimFault::Timeout = state.fault {
            let err = "ECU handshake timed out".to_string();
            state.connection_state = ConnectionState::Faulted { error: err };
            return Err(ProtocolError::Timeout(1000));
        }

        let identity = default_simulator_identity();

        state.connection_state = ConnectionState::Connected {
            identity: identity.clone(),
        };

        // Abort previous background task if present
        if let Some(handle) = self.inner.bg_task.lock().unwrap_or_else(|e| e.into_inner()).take() {
            handle.abort();
        }

        // Spawn background 20 Hz (50 ms) streaming loop using Weak reference to break reference cycles on drop
        let inner_weak = Arc::downgrade(&self.inner);
        let handle = tokio::spawn(async move {
            // First tick delayed by 50 ms so synchronous unit test steps do not race
            let mut interval = tokio::time::interval_at(
                tokio::time::Instant::now() + Duration::from_millis(50),
                Duration::from_millis(50),
            );

            loop {
                interval.tick().await;

                let inner = match inner_weak.upgrade() {
                    Some(i) => i,
                    None => break,
                };

                let should_tick = {
                    let st = inner.state.lock().unwrap_or_else(|e| e.into_inner());
                    match st.connection_state {
                        ConnectionState::Connected { .. } => st.auto_tick,
                        ConnectionState::Disconnected => break,
                        _ => false,
                    }
                };

                if should_tick {
                    inner.step_tick();
                }
            }
        });

        *self.inner.bg_task.lock().unwrap_or_else(|e| e.into_inner()) = Some(handle);
        Ok(identity)
    }

    /// Disconnects from the virtual ECU, aborting the background streaming task.
    pub async fn disconnect(&self) -> Result<(), ProtocolError> {
        if let Some(handle) = self.inner.bg_task.lock().unwrap_or_else(|e| e.into_inner()).take() {
            handle.abort();
        }

        let mut state = self.inner.state.lock().unwrap_or_else(|e| e.into_inner());
        state.connection_state = ConnectionState::Disconnected;
        Ok(())
    }

    /// Returns the current connection lifecycle state.
    pub fn connection_state(&self) -> ConnectionState {
        self.inner.state.lock().unwrap_or_else(|e| e.into_inner()).connection_state.clone()
    }

    /// Subscribes to the broadcast stream of live telemetry frames.
    pub fn subscribe_telemetry(&self) -> broadcast::Receiver<TelemetryFrame> {
        self.inner.tx.subscribe()
    }
}

impl SimulatorInner {
    fn step_tick(&self) -> TelemetryFrame {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let step = state.sequence;
        state.sequence += 1;

        let timestamp_ms = self.clock.now_ms();

        // Check for stale samples fault
        let is_stale = match &mut state.fault {
            SimFault::StaleSamples(count) => {
                if *count > 1 {
                    *count -= 1;
                    true
                } else if *count == 1 {
                    state.fault = SimFault::None;
                    true
                } else {
                    state.fault = SimFault::None;
                    false
                }
            }
            _ => false,
        };

        let mut frame = if is_stale && state.last_frame.is_some() {
            let mut f = state.last_frame.clone().unwrap();
            f.timestamp_ms = timestamp_ms;
            f.sequence = step;
            f.status_flags.stale_sample = true;
            f
        } else {
            let mode = state.mode;
            let mut f = compute_physics(mode, step, &mut state.rng);
            f.timestamp_ms = timestamp_ms;
            f.sequence = step;
            f.is_simulated = true;
            if is_stale {
                f.status_flags.stale_sample = true;
            }
            f
        };

        // Apply mode-specific overrides (such as LowBatteryFault)
        if matches!(state.mode, SimMode::LowBatteryFault) {
            frame.battery_volts = 10.4 + state.rng.next_range(0.02);
            frame.status_flags.low_voltage = true;
            frame.status_flags.check_engine_light = true;
        }

        // Apply fault injection behaviors
        match &state.fault {
            SimFault::Disconnect => {
                state.connection_state = ConnectionState::Faulted {
                    error: "ECU physical disconnection detected".to_string(),
                };
                frame.status_flags.communication_fault = true;
            }
            SimFault::Timeout => {
                state.connection_state = ConnectionState::Faulted {
                    error: "ECU communication timed out".to_string(),
                };
                frame.status_flags.communication_fault = true;
            }
            SimFault::InvalidFrame => {
                frame.rpm = 25000.0;
                frame.battery_volts = 4.2;
                frame.tps = 110.0;
                frame.status_flags.communication_fault = true;
            }
            SimFault::SensorRange { sensor, value } => {
                apply_sensor_override(&mut frame, sensor, *value);
            }
            _ => {}
        }

        state.last_frame = Some(frame.clone());

        // Broadcast to any active subscribers
        let _ = self.tx.send(frame.clone());

        frame
    }
}

fn apply_sensor_override(frame: &mut TelemetryFrame, sensor: &str, value: f32) {
    match sensor.to_ascii_lowercase().as_str() {
        "rpm" => frame.rpm = value,
        "tps" => frame.tps = value,
        "map_kpa" | "map" => frame.map_kpa = value,
        "ect_c" | "ect" => frame.ect_c = value,
        "iat_c" | "iat" => frame.iat_c = value,
        "battery_volts" | "battery" => frame.battery_volts = value,
        "ignition_advance_deg" | "timing" => frame.ignition_advance_deg = value,
        "injector_pulse_width_ms" | "pw" => frame.injector_pulse_width_ms = value,
        "afr" => frame.afr = value,
        "speed_kph" | "speed" => frame.speed_kph = value,
        "gear" => frame.gear = value as u8,
        _ => {}
    }
}

fn compute_physics(mode: SimMode, step: u64, rng: &mut LcgRng) -> TelemetryFrame {
    match mode {
        SimMode::IdleWarmup => {
            let progress = (step as f32 / 400.0).clamp(0.0, 1.0);
            let ect_c = 25.0 + (85.0 - 25.0) * progress + rng.next_range(0.1);
            let rpm = 1800.0 - (1800.0 - 1350.0) * progress + rng.next_range(8.0);
            let tps = 0.0;
            let afr = 13.2 + (14.7 - 13.2) * progress + rng.next_range(0.04);
            let battery_volts = 14.1 + rng.next_range(0.05);
            let map_kpa = 34.0 + (1.0 - progress) * 4.0 + rng.next_range(0.3);
            let iat_c = 25.0 + progress * 5.0 + rng.next_range(0.1);
            let ignition_advance_deg = 12.0 + (16.0 - 12.0) * progress + rng.next_range(0.2);
            let injector_pulse_width_ms = 2.8 - (2.8 - 1.8) * progress + rng.next_range(0.02);

            TelemetryFrame {
                timestamp_ms: 0,
                sequence: step,
                is_simulated: true,
                rpm,
                tps,
                map_kpa,
                ect_c,
                iat_c,
                battery_volts,
                ignition_advance_deg,
                injector_pulse_width_ms,
                afr,
                speed_kph: 0.0,
                gear: 0,
                status_flags: TelemetryStatusFlags::default(),
            }
        }
        SimMode::DynoPull => {
            let cycle_step = step % 160;
            let (rpm, tps, afr, map_kpa, speed_kph, ignition_advance_deg, injector_pulse_width_ms) =
                if cycle_step < 120 {
                    let progress = cycle_step as f32 / 120.0;
                    let r = 2500.0 + (11000.0 - 2500.0) * progress + rng.next_range(10.0);
                    let t = 100.0;
                    let a = 12.6 + rng.next_range(0.04);
                    let m = 101.3 + rng.next_range(0.4);
                    let s = 50.0 + progress * 170.0;
                    let ign = 24.0 + progress * 6.0 + rng.next_range(0.2);
                    let pw = 8.0 + progress * 9.5 + rng.next_range(0.05);
                    (r, t, a, m, s, ign, pw)
                } else if cycle_step < 140 {
                    let cut = if cycle_step.is_multiple_of(2) { -200.0 } else { 50.0 };
                    let r = (11000.0 + cut + rng.next_range(15.0)).clamp(10600.0, 11050.0);
                    let t = 100.0;
                    let a = 13.2 + rng.next_range(0.08);
                    let m = 101.0 + rng.next_range(0.4);
                    let s = 220.0;
                    let ign = 16.0 + rng.next_range(0.2);
                    let pw = 12.0 + rng.next_range(0.1);
                    (r, t, a, m, s, ign, pw)
                } else {
                    let progress = (cycle_step - 140) as f32 / 20.0;
                    let r = 11000.0 - (11000.0 - 2500.0) * progress + rng.next_range(10.0);
                    let t = 0.0;
                    let a = 14.7 + rng.next_range(0.05);
                    let m = 35.0 + rng.next_range(0.5);
                    let s = 220.0 - (220.0 - 50.0) * progress;
                    let ign = 18.0 + rng.next_range(0.2);
                    let pw = 2.5 + rng.next_range(0.05);
                    (r, t, a, m, s, ign, pw)
                };

            let ect_c = 88.0 + rng.next_range(0.1);
            let iat_c = 30.0 + rng.next_range(0.1);
            let battery_volts = 14.1 + rng.next_range(0.05);

            TelemetryFrame {
                timestamp_ms: 0,
                sequence: step,
                is_simulated: true,
                rpm,
                tps,
                map_kpa,
                ect_c,
                iat_c,
                battery_volts,
                ignition_advance_deg,
                injector_pulse_width_ms,
                afr,
                speed_kph,
                gear: 4,
                status_flags: TelemetryStatusFlags::default(),
            }
        }
        SimMode::StreetTrack => {
            let track_step = step % 200;
            let (gear, rpm, speed_kph, tps, map_kpa, afr, ignition_advance_deg, injector_pulse_width_ms) =
                if track_step < 35 {
                    let p = track_step as f32 / 35.0;
                    let r = 1500.0 + (7200.0 - 1500.0) * p + rng.next_range(10.0);
                    let s = 10.0 + 35.0 * p;
                    (1, r, s, 75.0, 85.0 + rng.next_range(0.5), 13.0 + rng.next_range(0.04), 20.0 + p * 8.0, 5.0 + p * 6.0)
                } else if track_step < 45 {
                    let p = (track_step - 35) as f32 / 10.0;
                    let r = 7200.0 - (7200.0 - 4500.0) * p;
                    (2, r, 45.0, 5.0, 35.0, 14.7 + rng.next_range(0.05), 15.0, 2.0)
                } else if track_step < 85 {
                    let p = (track_step - 45) as f32 / 40.0;
                    let r = 4500.0 + (7500.0 - 4500.0) * p + rng.next_range(10.0);
                    let s = 45.0 + 45.0 * p;
                    (2, r, s, 85.0, 92.0 + rng.next_range(0.5), 12.8 + rng.next_range(0.04), 22.0 + p * 7.0, 7.0 + p * 6.0)
                } else if track_step < 95 {
                    let p = (track_step - 85) as f32 / 10.0;
                    let r = 7500.0 - (7500.0 - 5000.0) * p;
                    (3, r, 90.0, 5.0, 35.0, 14.7 + rng.next_range(0.05), 15.0, 2.0)
                } else if track_step < 140 {
                    let p = (track_step - 95) as f32 / 45.0;
                    let r = 5000.0 + (8000.0 - 5000.0) * p + rng.next_range(10.0);
                    let s = 90.0 + 50.0 * p;
                    (3, r, s, 90.0, 96.0 + rng.next_range(0.5), 12.6 + rng.next_range(0.04), 24.0 + p * 6.0, 8.0 + p * 6.0)
                } else if track_step < 150 {
                    let p = (track_step - 140) as f32 / 10.0;
                    let r = 8000.0 - (8000.0 - 5500.0) * p;
                    (4, r, 140.0, 5.0, 35.0, 14.7 + rng.next_range(0.05), 15.0, 2.0)
                } else if track_step < 170 {
                    let p = (track_step - 150) as f32 / 20.0;
                    let r = 5500.0 + (7500.0 - 5500.0) * p + rng.next_range(10.0);
                    let s = 140.0 + 20.0 * p;
                    (4, r, s, 95.0, 98.0 + rng.next_range(0.5), 12.5 + rng.next_range(0.04), 26.0 + p * 5.0, 9.0 + p * 5.0)
                } else {
                    // Overrun deceleration with fuel cut
                    let p = (track_step - 170) as f32 / 30.0;
                    let r = 7500.0 - (7500.0 - 2500.0) * p + rng.next_range(10.0);
                    let s = 160.0 - 100.0 * p;
                    (4, r, s, 0.0, 22.0 + rng.next_range(0.3), 21.5 + rng.next_range(0.2), 10.0, 0.0)
                };

            let ect_c = 88.0 + rng.next_range(0.1);
            let iat_c = 32.0 + rng.next_range(0.1);
            let battery_volts = 14.1 + rng.next_range(0.05);

            TelemetryFrame {
                timestamp_ms: 0,
                sequence: step,
                is_simulated: true,
                rpm,
                tps,
                map_kpa,
                ect_c,
                iat_c,
                battery_volts,
                ignition_advance_deg,
                injector_pulse_width_ms,
                afr,
                speed_kph,
                gear,
                status_flags: TelemetryStatusFlags::default(),
            }
        }
        SimMode::LowBatteryFault => {
            let rpm = 1100.0 + rng.next_range(8.0);
            let ect_c = 80.0 + rng.next_range(0.1);
            let iat_c = 28.0 + rng.next_range(0.1);
            let afr = 14.7 + rng.next_range(0.05);
            let map_kpa = 36.0 + rng.next_range(0.3);
            let ignition_advance_deg = 14.0 + rng.next_range(0.2);
            let injector_pulse_width_ms = 2.4 + rng.next_range(0.02);
            let battery_volts = 10.4 + rng.next_range(0.02);

            let flags = TelemetryStatusFlags {
                low_voltage: true,
                check_engine_light: true,
                ..Default::default()
            };

            TelemetryFrame {
                timestamp_ms: 0,
                sequence: step,
                is_simulated: true,
                rpm,
                tps: 0.0,
                map_kpa,
                ect_c,
                iat_c,
                battery_volts,
                ignition_advance_deg,
                injector_pulse_width_ms,
                afr,
                speed_kph: 0.0,
                gear: 0,
                status_flags: flags,
            }
        }
    }
}

#[async_trait]
impl EcuDriver for VirtualEcuSimulator {
    fn descriptor(&self) -> DriverDescriptor {
        DriverDescriptor {
            id: "virtual_bench_ecu".to_string(),
            name: "Virtual ECU Bench Simulator".to_string(),
            is_simulator: true,
        }
    }

    async fn connect(&self) -> Result<EcuIdentity, ProtocolError> {
        self.connect().await
    }

    async fn disconnect(&self) -> Result<(), ProtocolError> {
        self.disconnect().await
    }

    fn connection_state(&self) -> ConnectionState {
        self.connection_state()
    }
}

impl TelemetrySource for VirtualEcuSimulator {
    fn subscribe_telemetry(&self) -> broadcast::Receiver<TelemetryFrame> {
        self.subscribe_telemetry()
    }
}

impl Drop for SimulatorInner {
    fn drop(&mut self) {
        if let Some(handle) = self.bg_task.lock().unwrap_or_else(|e| e.into_inner()).take() {
            handle.abort();
        }
    }
}
