//! Read-Only OBD-II Driver and Continuous Telemetry Acquisition Engine.
//!
//! Coordinates [`ObdSession`] to execute allowlisted adapter handshakes, discovery
//! of supported PIDs, and continuous read-only polling of Mode 01 diagnostic metrics.
//!
//! Enforces:
//! - Absolute read-only diagnostics boundary (no ECU writing, coding, or mutations).
//! - Single transaction ownership through [`ObdSession`].
//! - Closed typed command domain ([`ObdCommand`], [`Mode01Pid`]).
//! - Discovery and strict filtering against supported-PID bitmaps.
//! - Non-fabrication of unmeasured or unsupported sensor telemetry (leaves values NaN).
//! - Strict validation of stoichiometric ratio for wideband AFR calculation.
//! - Deterministic task cancellation and teardown upon disconnect or failure.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

use crate::clock::{Clock, SystemMonotonicClock};
use crate::driver::{DriverDescriptor, EcuDriver, TelemetrySource};
use crate::errors::ProtocolError;
use crate::model::{ConnectionState, EcuIdentity, TelemetryFrame, TelemetryStatusFlags};
use crate::obd::command::Mode01Pid;
use crate::obd::errors::ObdCodecError;
use crate::obd::framing::{ProtocolSelection, SerialBaudRate};
use crate::obd::pid::DecodedMode01Value;
use crate::obd::session::{ObdSession, ObdSessionConfig, ObdSessionState};
use crate::obd::transport::ObdTransport;
use crate::worker::create_telemetry_broadcast_channel;

/// Operational configuration for the read-only OBD-II driver.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Obd2DriverConfig {
    /// Target polling interval between successive PID acquisition cycles (10 ms..=5000 ms).
    pub poll_interval: Duration,
    /// Explicit fuel stoichiometric ratio for wideband AFR estimation.
    /// If `None`, wideband AFR is not computed from equivalence ratio and remains unestimated.
    pub stoichiometric_ratio: Option<f32>,
    /// Priority-ordered sequence of allowlisted Mode 01 diagnostic PIDs to poll.
    /// Must be non-empty and must not contain bitmap query PIDs (0x00 or 0x20).
    pub polled_pids: Vec<Mode01Pid>,
    /// Per-transaction command timeout for PID queries (1 ms..=5000 ms).
    pub command_timeout: Duration,
    /// Whether to attempt querying the Vehicle Identification Number (VIN) via Mode 09 PID 0x02 upon connect.
    /// Default: false (deferred per Milestone 3D-3 specification to prevent faulting sessions on ECUs lacking Mode 09).
    pub query_vin: bool,
    /// Protocol selection policy for the underlying session.
    pub protocol: ProtocolSelection,
    /// Baud rate for physical serial communication.
    pub baud_rate: SerialBaudRate,
}

impl Default for Obd2DriverConfig {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_millis(50), // 20 Hz nominal
            stoichiometric_ratio: None,
            polled_pids: vec![
                Mode01Pid::EngineRpm,
                Mode01Pid::ThrottlePosition,
                Mode01Pid::ManifoldPressure,
                Mode01Pid::CoolantTemp,
                Mode01Pid::IntakeAirTemp,
                Mode01Pid::VehicleSpeed,
                Mode01Pid::ControlModuleVoltage,
            ],
            command_timeout: Duration::from_millis(500),
            query_vin: false,
            protocol: ProtocolSelection::Can11Bit500k,
            baud_rate: SerialBaudRate::Baud115200,
        }
    }
}

impl Obd2DriverConfig {
    /// Validates configuration parameters against safety boundaries and bounds.
    pub fn validate(&self) -> Result<(), ObdCodecError> {
        if self.poll_interval < Duration::from_millis(10)
            || self.poll_interval > Duration::from_millis(5000)
        {
            return Err(ObdCodecError::InvalidSessionState {
                current: format!("poll_interval: {:?}", self.poll_interval),
                expected: "10ms..=5000ms".to_string(),
            });
        }

        if self.command_timeout.is_zero() || self.command_timeout > Duration::from_millis(5000) {
            return Err(ObdCodecError::InvalidSessionState {
                current: format!("command_timeout: {:?}", self.command_timeout),
                expected: "1ms..=5000ms".to_string(),
            });
        }

        if self.polled_pids.is_empty() {
            return Err(ObdCodecError::InvalidSessionState {
                current: "polled_pids empty".to_string(),
                expected: "non-empty polled_pids".to_string(),
            });
        }

        for pid in &self.polled_pids {
            if matches!(
                pid,
                Mode01Pid::SupportedPids01_20 | Mode01Pid::SupportedPids21_40
            ) {
                return Err(ObdCodecError::UnauthorizedCommand(format!(
                    "Bitmap PID {:?} cannot be polled as a telemetry metric",
                    pid
                )));
            }
        }

        if let Some(stoich) = self.stoichiometric_ratio {
            if stoich.is_nan() || stoich.is_infinite() || stoich <= 0.0 {
                return Err(ObdCodecError::InvalidStoichiometricRatio(format!(
                    "{}",
                    stoich
                )));
            }
        }

        Ok(())
    }
}

/// Creates a new uninitialized `TelemetryFrame` with NaN for all unmeasured sensor fields,
/// ensuring that unsupported or unpolled channels are never fabricated as zeroes.
pub fn create_empty_frame() -> TelemetryFrame {
    TelemetryFrame {
        timestamp_ms: 0,
        sequence: 0,
        is_simulated: false,
        rpm: f32::NAN,
        tps: f32::NAN,
        map_kpa: f32::NAN,
        ect_c: f32::NAN,
        iat_c: f32::NAN,
        battery_volts: f32::NAN,
        ignition_advance_deg: f32::NAN,
        injector_pulse_width_ms: f32::NAN,
        afr: f32::NAN,
        speed_kph: f32::NAN,
        gear: 0,
        status_flags: TelemetryStatusFlags::default(),
    }
}

/// Updates a `TelemetryFrame` from a strongly-typed decoded Mode 01 diagnostic value.
pub fn update_frame_from_decoded_pid(
    frame: &mut TelemetryFrame,
    value: &DecodedMode01Value,
    stoichiometric_ratio: Option<f32>,
) {
    match value {
        DecodedMode01Value::EngineRpm(rpm) => frame.rpm = *rpm,
        DecodedMode01Value::ThrottlePosition(tps) => frame.tps = *tps,
        DecodedMode01Value::ManifoldPressure(kpa) => frame.map_kpa = *kpa as f32,
        DecodedMode01Value::CoolantTemp(clt) => frame.ect_c = *clt,
        DecodedMode01Value::IntakeAirTemp(iat) => frame.iat_c = *iat,
        DecodedMode01Value::ControlModuleVoltage(volts) => frame.battery_volts = *volts,
        DecodedMode01Value::VehicleSpeed(kmh) => frame.speed_kph = *kmh,
        DecodedMode01Value::WidebandLambda1(sample) => {
            if let Some(stoich) = stoichiometric_ratio {
                if let Ok(afr) = sample.estimated_afr(stoich) {
                    frame.afr = afr;
                }
            }
        }
    }
}

struct DriverInner<T: ObdTransport> {
    session: tokio::sync::Mutex<ObdSession<T>>,
    config: Obd2DriverConfig,
    state: Mutex<ConnectionState>,
    tx: broadcast::Sender<TelemetryFrame>,
    bg_task: Mutex<Option<JoinHandle<()>>>,
    sequence: AtomicU64,
    clock: Arc<dyn Clock>,
}

impl<T: ObdTransport> Drop for DriverInner<T> {
    fn drop(&mut self) {
        if let Some(handle) = self.bg_task.lock().unwrap_or_else(|e| e.into_inner()).take() {
            handle.abort();
        }
    }
}

/// Read-only OBD-II telemetry driver implementing [`EcuDriver`] and [`TelemetrySource`].
///
/// Wraps an underlying [`ObdTransport`] and coordinates continuous background Mode 01 PID polling.
#[derive(Clone)]
pub struct Obd2Driver<T: ObdTransport> {
    inner: Arc<DriverInner<T>>,
}

impl<T: ObdTransport + 'static> Obd2Driver<T> {
    /// Creates a new disconnected OBD-II driver with the provided transport and config.
    pub fn new(transport: T, config: Obd2DriverConfig) -> Self {
        let (tx, _) = create_telemetry_broadcast_channel();
        let session_cfg = ObdSessionConfig {
            command_timeout: config.command_timeout,
            protocol: config.protocol,
            baud_rate: config.baud_rate,
            ..Default::default()
        };

        Self {
            inner: Arc::new(DriverInner {
                session: tokio::sync::Mutex::new(ObdSession::new(transport, session_cfg)),
                config,
                state: Mutex::new(ConnectionState::Disconnected),
                tx,
                bg_task: Mutex::new(None),
                sequence: AtomicU64::new(0),
                clock: Arc::new(SystemMonotonicClock::new()),
            }),
        }
    }

    /// Creates a new driver with a custom clock (used for deterministic testing).
    pub fn with_clock(transport: T, config: Obd2DriverConfig, clock: Arc<dyn Clock>) -> Self {
        let mut driver = Self::new(transport, config);
        if let Some(inner) = Arc::get_mut(&mut driver.inner) {
            inner.clock = clock;
        }
        driver
    }

    /// Helper to convert internal codec errors into public [`ProtocolError`] variants.
    fn map_codec_error(err: ObdCodecError) -> ProtocolError {
        match err {
            ObdCodecError::IoError(s) => ProtocolError::DeviceNotFound(s),
            ObdCodecError::Timeout(ms) => ProtocolError::Timeout(ms),
            ObdCodecError::AdapterError(s) => ProtocolError::HandshakeFailed(s),
            ObdCodecError::UnauthorizedCommand(s) => ProtocolError::InvalidFormat(s),
            ObdCodecError::SessionFaulted(s) => ProtocolError::Disconnected(s),
            ObdCodecError::SessionBusy => ProtocolError::PortBusy("Session is busy".to_string()),
            other => ProtocolError::InvalidFormat(other.to_string()),
        }
    }
}

#[async_trait]
impl<T: ObdTransport + 'static> EcuDriver for Obd2Driver<T> {
    fn descriptor(&self) -> DriverDescriptor {
        DriverDescriptor {
            id: "obd2_serial".to_string(),
            name: "OBD-II Serial Diagnostic Driver".to_string(),
            is_simulator: false,
        }
    }

    async fn connect(&self) -> Result<EcuIdentity, ProtocolError> {
        self.inner
            .config
            .validate()
            .map_err(Self::map_codec_error)?;

        // Acquire session lock before checking/updating state so concurrent connect() calls serialize.
        let mut session = self.inner.session.lock().await;

        {
            let mut st = self.inner.state.lock().unwrap_or_else(|e| e.into_inner());
            if matches!(
                *st,
                ConnectionState::Connected { .. }
                    | ConnectionState::Connecting { .. }
                    | ConnectionState::Reconnecting { .. }
            ) {
                return Err(ProtocolError::PortBusy(
                    "OBD-II driver is already connected or connecting".to_string(),
                ));
            }
            *st = ConnectionState::Connecting { attempt: 1 };
        }

        // Abort any leftover background task before starting a new connection.
        if let Some(prev) = self.inner.bg_task.lock().unwrap_or_else(|e| e.into_inner()).take() {
            prev.abort();
        }

        // 1. Open transport if disconnected
        if matches!(session.state(), ObdSessionState::Disconnected) {
            session.open().await.map_err(|e| {
                *self.inner.state.lock().unwrap_or_else(|e| e.into_inner()) = ConnectionState::Faulted {
                    error: e.to_string(),
                };
                Self::map_codec_error(e)
            })?;
        }

        // 2. Initialize adapter (ATZ, ATE0, ATL0, ATH0, ATS1, ATSP, ATI)
        let adapter_identity = session.initialize().await.map_err(|e| {
            *self.inner.state.lock().unwrap_or_else(|e| e.into_inner()) = ConnectionState::Faulted {
                error: e.to_string(),
            };
            Self::map_codec_error(e)
        })?;

        // 3. Discover supported PIDs via bitmap queries
        let mut active_pids = Vec::new();
        let bitmap_01_20 = session
            .query_supported_pids(Mode01Pid::SupportedPids01_20)
            .await
            .ok();

        let bitmap_21_40 = if let Some(ref bm) = bitmap_01_20 {
            if bm.has_next_bitmap_range() {
                session
                    .query_supported_pids(Mode01Pid::SupportedPids21_40)
                    .await
                    .ok()
            } else {
                None
            }
        } else {
            None
        };

        let bitmap_41_60 = if let Some(ref bm) = bitmap_21_40 {
            if bm.has_next_bitmap_range() {
                session
                    .query_supported_pids(Mode01Pid::SupportedPids41_60)
                    .await
                    .ok()
            } else {
                None
            }
        } else {
            None
        };

        let no_21_40_range = bitmap_01_20
            .as_ref()
            .is_some_and(|b| !b.has_next_bitmap_range());
        let no_41_60_range = no_21_40_range
            || bitmap_21_40
                .as_ref()
                .is_some_and(|b| !b.has_next_bitmap_range());

        for pid in &self.inner.config.polled_pids {
            let pid_num = pid.pid();
            let is_supported = match pid_num {
                0x01..=0x20 => bitmap_01_20.as_ref().map(|b| b.is_supported(pid_num)),
                0x21..=0x40 => {
                    if no_21_40_range {
                        Some(false)
                    } else {
                        bitmap_21_40.as_ref().map(|b| b.is_supported(pid_num))
                    }
                }
                0x41..=0x60 => {
                    if no_41_60_range {
                        Some(false)
                    } else {
                        bitmap_41_60.as_ref().map(|b| b.is_supported(pid_num))
                    }
                }
                _ => {
                    if no_21_40_range {
                        Some(false)
                    } else {
                        None
                    }
                }
            };

            // If bitmap query succeeded and reported support, or if bitmap queries were unavailable,
            // retain the PID. If bitmap explicitly says unsupported (false), skip it.
            if is_supported != Some(false) {
                active_pids.push(*pid);
            }
        }

        // 4. Safe VIN query (Mode 09 PID 0x02)
        // Per Milestone 3D-3 constraints, VIN acquisition is deferred by default (query_vin: false)
        // to avoid faulting the session on ECUs or test harnesses lacking Mode 09 support.
        let vin = if self.inner.config.query_vin {
            session.query_vin().await.ok()
        } else {
            None
        };

        let identity = EcuIdentity {
            ecu_id: adapter_identity.elm_version.clone(),
            vin,
            software_version: adapter_identity
                .stn_firmware_version
                .unwrap_or_else(|| adapter_identity.elm_version.clone()),
            hardware_version: adapter_identity
                .stn_device_id
                .unwrap_or_else(|| "ELM327-Compatible".to_string()),
            protocol_name: "ISO 15765-4 (CAN 11/500k)".to_string(),
            is_simulated: false,
        };

        // Transition state to Connected
        *self.inner.state.lock().unwrap_or_else(|e| e.into_inner()) = ConnectionState::Connected {
            identity: identity.clone(),
        };

        // 5. Spawn background polling task using Weak reference to prevent Arc leak on Drop.
        let inner_weak = Arc::downgrade(&self.inner);
        let poll_interval = self.inner.config.poll_interval;
        let stoich = self.inner.config.stoichiometric_ratio;

        let handle = tokio::spawn(async move {
            let mut current_frame = create_empty_frame();
            let mut active_pids = active_pids;

            loop {
                let Some(inner) = inner_weak.upgrade() else {
                    break;
                };

                // Check if connection state is still connected
                {
                    let state = inner.state.lock().unwrap_or_else(|e| e.into_inner());
                    if !matches!(*state, ConnectionState::Connected { .. }) {
                        break;
                    }
                }

                let mut cycle_error: Option<String> = None;
                let mut unsupported_pids = Vec::new();

                // Acquire session lock and poll each active PID sequentially
                {
                    let mut session_guard = inner.session.lock().await;

                    if !matches!(session_guard.state(), ObdSessionState::Ready) {
                        cycle_error = Some(format!(
                            "session entered non-ready state: {:?}",
                            session_guard.state()
                        ));
                    } else {
                        for pid in &active_pids {
                            match session_guard.query_pid(*pid).await {
                                Ok(value) => {
                                    update_frame_from_decoded_pid(
                                        &mut current_frame,
                                        &value,
                                        stoich,
                                    );
                                }
                                Err(ObdCodecError::NoData) => {
                                    // ECU has no data for this optional PID; stop polling it without faulting session.
                                    unsupported_pids.push(*pid);
                                }
                                Err(e) => {
                                    // Bounded fault handling: if session becomes faulted or disconnected,
                                    // halt polling immediately to preserve fault containment.
                                    cycle_error = Some(e.to_string());
                                    break;
                                }
                            }
                        }
                    }
                }

                if !unsupported_pids.is_empty() {
                    active_pids.retain(|p| !unsupported_pids.contains(p));
                }

                // If a fatal communication error or session fault occurred, halt polling immediately
                if let Some(err) = cycle_error {
                    *inner.state.lock().unwrap_or_else(|e| e.into_inner()) = ConnectionState::Faulted { error: err };
                    break;
                }

                // Update frame header and broadcast
                current_frame.timestamp_ms = inner.clock.now_ms();
                current_frame.sequence = inner.sequence.fetch_add(1, Ordering::Relaxed);
                let _ = inner.tx.send(current_frame.clone());

                tokio::time::sleep(poll_interval).await;
            }
        });

        let mut bg = self.inner.bg_task.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(prev) = bg.take() {
            prev.abort();
        }
        *bg = Some(handle);

        Ok(identity)
    }

    async fn disconnect(&self) -> Result<(), ProtocolError> {
        // Set state to Disconnected
        *self.inner.state.lock().unwrap_or_else(|e| e.into_inner()) = ConnectionState::Disconnected;

        // Abort background polling task
        if let Some(handle) = self.inner.bg_task.lock().unwrap_or_else(|e| e.into_inner()).take() {
            handle.abort();
        }

        // Close session
        let mut session = self.inner.session.lock().await;
        if !matches!(session.state(), ObdSessionState::Disconnected) {
            let _ = session.close().await;
        }

        Ok(())
    }

    fn connection_state(&self) -> ConnectionState {
        self.inner.state.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl<T: ObdTransport + 'static> TelemetrySource for Obd2Driver<T> {
    fn subscribe_telemetry(&self) -> broadcast::Receiver<TelemetryFrame> {
        self.inner.tx.subscribe()
    }
}
