//! Deterministic playback engine for recorded datalog sessions with clock injection.

use std::sync::{Arc, Mutex};
use std::time::Duration;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

use crate::clock::{Clock, SystemMonotonicClock};
use crate::datalogger::session::DatalogSession;
use crate::driver::TelemetrySource;
use crate::errors::ProtocolError;
use crate::model::TelemetryFrame;
use crate::worker::create_telemetry_broadcast_channel;

/// Playback transport state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PlaybackState {
    /// Playback is stopped and reset to start (or session conclusion).
    #[default]
    Stopped,
    /// Playback is actively running, advancing virtual time and broadcasting frames.
    Playing,
    /// Playback is paused at a specific virtual timestamp.
    Paused,
}

impl PlaybackState {
    /// Returns true if playback transport is stopped.
    pub fn is_stopped(&self) -> bool {
        matches!(self, PlaybackState::Stopped)
    }

    /// Returns true if playback transport is actively playing.
    pub fn is_playing(&self) -> bool {
        matches!(self, PlaybackState::Playing)
    }

    /// Returns true if playback transport is paused.
    pub fn is_paused(&self) -> bool {
        matches!(self, PlaybackState::Paused)
    }
}

/// Instantaneous playback transport status and progress metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaybackStatus {
    /// Current transport state (Stopped, Playing, Paused).
    pub state: PlaybackState,
    /// Current virtual playback timestamp in milliseconds.
    pub current_time_ms: u64,
    /// Total duration in milliseconds from first to last recorded frame.
    pub total_duration_ms: u64,
    /// Current zero-based index of the active frame in the session.
    pub current_index: usize,
    /// Total count of frames in the active session.
    pub total_samples: usize,
    /// Playback speed multiplier (0.1x to 10.0x, default 1.0).
    pub speed: f32,
    /// Whether playback wraps around to beginning upon reaching the end (default false).
    pub loop_playback: bool,
}

impl Default for PlaybackStatus {
    fn default() -> Self {
        Self {
            state: PlaybackState::Stopped,
            current_time_ms: 0,
            total_duration_ms: 0,
            current_index: 0,
            total_samples: 0,
            speed: 1.0,
            loop_playback: false,
        }
    }
}

struct PlaybackStateInner {
    state: PlaybackState,
    current_index: usize,
    current_time_ms: u64,
    speed: f32,
    loop_playback: bool,
    clock_anchor_ms: u64,
    virtual_anchor_ms: u64,
}

struct PlaybackInner {
    session: Arc<DatalogSession>,
    clock: Arc<dyn Clock>,
    tx: broadcast::Sender<TelemetryFrame>,
    state: Mutex<PlaybackStateInner>,
    bg_task: Mutex<Option<JoinHandle<()>>>,
}

impl Drop for PlaybackInner {
    fn drop(&mut self) {
        if let Some(handle) = self.bg_task.lock().unwrap_or_else(|e| e.into_inner()).take() {
            handle.abort();
        }
    }
}

/// Deterministic telemetry session playback engine with injectable clock.
#[derive(Clone)]
pub struct PlaybackEngine {
    inner: Arc<PlaybackInner>,
}

impl PlaybackEngine {
    /// Creates a new playback engine using the system monotonic clock.
    pub fn new(session: Arc<DatalogSession>) -> Self {
        Self::new_with_clock(session, Arc::new(SystemMonotonicClock::new()))
    }

    /// Creates a new playback engine using a custom injectable clock for deterministic testing.
    pub fn new_with_clock(session: Arc<DatalogSession>, clock: Arc<dyn Clock>) -> Self {
        let (tx, _) = create_telemetry_broadcast_channel();
        let initial_ts = session.frames.first().map(|f| f.timestamp_ms).unwrap_or(0);
        let now = clock.now_ms();

        let state = PlaybackStateInner {
            state: PlaybackState::Stopped,
            current_index: 0,
            current_time_ms: initial_ts,
            speed: 1.0,
            loop_playback: false,
            clock_anchor_ms: now,
            virtual_anchor_ms: initial_ts,
        };

        Self {
            inner: Arc::new(PlaybackInner {
                session,
                clock,
                tx,
                state: Mutex::new(state),
                bg_task: Mutex::new(None),
            }),
        }
    }

    /// Returns a reference to the active datalog session.
    pub fn session(&self) -> &Arc<DatalogSession> {
        &self.inner.session
    }

    /// Subscribes to the broadcast stream of playback frames.
    pub fn subscribe(&self) -> broadcast::Receiver<TelemetryFrame> {
        self.inner.tx.subscribe()
    }

    /// Starts or resumes playback of the recorded session.
    pub fn play(&self) -> Result<(), ProtocolError> {
        if self.inner.session.is_empty() {
            return Err(ProtocolError::InvalidStateTransition(
                "Cannot start playback on an empty datalog session".to_string(),
            ));
        }

        // Canonical lock order: bg_task -> state (matches pause() / stop()).
        let mut bg_task = self.inner.bg_task.lock().unwrap_or_else(|e| e.into_inner());
        {
            let mut st = self.inner.state.lock().unwrap_or_else(|e| e.into_inner());
            match st.state {
                PlaybackState::Playing => return Ok(()),
                PlaybackState::Paused => {
                    let now = self.inner.clock.now_ms();
                    st.clock_anchor_ms = now;
                    st.virtual_anchor_ms = st.current_time_ms;
                    st.state = PlaybackState::Playing;
                }
                PlaybackState::Stopped => {
                    let now = self.inner.clock.now_ms();
                    let first_ts = self.inner.session.frames.first().map(|f| f.timestamp_ms).unwrap_or(0);

                    if st.current_index == 0 {
                        st.current_time_ms = first_ts;
                        let frame = self.inner.session.frames[0].clone();
                        let _ = self.inner.tx.send(frame);
                    }
                    st.clock_anchor_ms = now;
                    st.virtual_anchor_ms = st.current_time_ms;
                    st.state = PlaybackState::Playing;
                }
            }
        }

        // Abort existing background task if present before spawning replacement
        if let Some(handle) = bg_task.take() {
            handle.abort();
        }

        // Spawn Tokio background streaming task
        let inner_weak = Arc::downgrade(&self.inner);
        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(10));
            loop {
                interval.tick().await;
                let inner = match inner_weak.upgrade() {
                    Some(i) => i,
                    None => break,
                };

                let should_continue = inner.tick();
                if !should_continue {
                    break;
                }
            }
        });

        *bg_task = Some(handle);
        Ok(())
    }

    /// Pauses playback, freezing virtual time and halting the background streaming task.
    pub fn pause(&self) -> Result<(), ProtocolError> {
        if let Some(handle) = self.inner.bg_task.lock().unwrap_or_else(|e| e.into_inner()).take() {
            handle.abort();
        }

        let mut st = self.inner.state.lock().unwrap_or_else(|e| e.into_inner());
        match st.state {
            PlaybackState::Paused => Ok(()),
            PlaybackState::Stopped => Err(ProtocolError::InvalidStateTransition(
                "Cannot pause playback when stopped".to_string(),
            )),
            PlaybackState::Playing => {
                let now = self.inner.clock.now_ms();
                if !self.inner.session.is_empty() {
                    let pos = self.inner.resolve_position(&st, now);
                    st.current_time_ms = pos.virtual_time_ms;
                    st.current_index = pos.index;
                }
                st.state = PlaybackState::Paused;
                st.virtual_anchor_ms = st.current_time_ms;
                st.clock_anchor_ms = now;
                Ok(())
            }
        }
    }

    /// Resumes playback from a paused state.
    pub fn resume(&self) -> Result<(), ProtocolError> {
        let state = self.inner.state.lock().unwrap_or_else(|e| e.into_inner()).state;
        match state {
            PlaybackState::Playing => Ok(()),
            PlaybackState::Paused => self.play(),
            PlaybackState::Stopped => Err(ProtocolError::InvalidStateTransition(
                "Cannot resume playback when stopped. Call play() instead.".to_string(),
            )),
        }
    }

    /// Seeks to a specific timestamp in milliseconds using binary search.
    /// Emits the target frame immediately to all subscribers.
    pub fn seek_ms(&self, target_ms: u64) -> Result<(), ProtocolError> {
        if self.inner.session.is_empty() {
            return Err(ProtocolError::InvalidStateTransition(
                "Cannot seek in an empty session".to_string(),
            ));
        }

        let mut st = self.inner.state.lock().unwrap_or_else(|e| e.into_inner());
        let frames = &self.inner.session.frames;
        let first_ts = frames.first().map(|f| f.timestamp_ms).unwrap_or(0);
        let last_ts = frames.last().map(|f| f.timestamp_ms).unwrap_or(0);

        let pp = frames.partition_point(|f| f.timestamp_ms <= target_ms);
        let index = if pp == 0 {
            0
        } else {
            (pp - 1).min(frames.len() - 1)
        };

        let target_clamped = target_ms.clamp(first_ts, last_ts);
        st.current_index = index;
        st.current_time_ms = target_clamped;
        st.virtual_anchor_ms = target_clamped;
        st.clock_anchor_ms = self.inner.clock.now_ms();

        let frame = frames[index].clone();
        let _ = self.inner.tx.send(frame);
        Ok(())
    }

    /// Adjusts playback speed multiplier, clamped between 0.1x and 10.0x.
    /// Re-anchors the virtual clock smoothly to prevent discontinuous time jumping.
    pub fn set_speed(&self, speed: f32) -> Result<(), ProtocolError> {
        if speed.is_nan() || speed <= 0.0 {
            return Err(ProtocolError::InvalidConfig(format!(
                "Invalid playback speed: {}",
                speed
            )));
        }
        let clamped_speed = speed.clamp(0.1, 10.0);
        let mut st = self.inner.state.lock().unwrap_or_else(|e| e.into_inner());

        if st.state == PlaybackState::Playing {
            let now = self.inner.clock.now_ms();
            let elapsed_real = now.saturating_sub(st.clock_anchor_ms);
            let elapsed_virtual = (elapsed_real as f64 * st.speed as f64).round() as u64;
            st.virtual_anchor_ms = st.virtual_anchor_ms.saturating_add(elapsed_virtual);
            st.clock_anchor_ms = now;
            st.current_time_ms = st.virtual_anchor_ms;
        } else {
            st.virtual_anchor_ms = st.current_time_ms;
            st.clock_anchor_ms = self.inner.clock.now_ms();
        }

        st.speed = clamped_speed;
        Ok(())
    }

    /// Sets whether playback wraps around continuously upon reaching the end of the session.
    pub fn set_loop(&self, loop_enabled: bool) {
        let mut st = self.inner.state.lock().unwrap_or_else(|e| e.into_inner());
        st.loop_playback = loop_enabled;
    }

    /// Stops playback, resetting the cursor to 0 and current_time_ms to initial frame timestamp.
    pub fn stop(&self) -> Result<(), ProtocolError> {
        if let Some(handle) = self.inner.bg_task.lock().unwrap_or_else(|e| e.into_inner()).take() {
            handle.abort();
        }

        let mut st = self.inner.state.lock().unwrap_or_else(|e| e.into_inner());
        let first_ts = self.inner.session.frames.first().map(|f| f.timestamp_ms).unwrap_or(0);
        st.state = PlaybackState::Stopped;
        st.current_index = 0;
        st.current_time_ms = first_ts;
        st.clock_anchor_ms = self.inner.clock.now_ms();
        st.virtual_anchor_ms = first_ts;
        Ok(())
    }

    /// Queries current playback transport status and progress.
    pub fn status(&self) -> PlaybackStatus {
        self.inner.get_status()
    }

    /// Deterministically advances playback to the next frame and returns it.
    /// Emits the frame to subscribers.
    pub fn step_next_frame(&self) -> Option<TelemetryFrame> {
        if self.inner.session.is_empty() {
            return None;
        }

        let mut st = self.inner.state.lock().unwrap_or_else(|e| e.into_inner());
        let frames = &self.inner.session.frames;
        let total_samples = frames.len();
        let first_ts = frames.first().map(|f| f.timestamp_ms).unwrap_or(0);
        let now = self.inner.clock.now_ms();

        let next_index = st.current_index + 1;
        if next_index >= total_samples {
            if st.loop_playback {
                st.current_index = 0;
                let frame = frames[0].clone();
                st.current_time_ms = frame.timestamp_ms;
                st.virtual_anchor_ms = frame.timestamp_ms;
                st.clock_anchor_ms = now;
                let _ = self.inner.tx.send(frame.clone());
                Some(frame)
            } else {
                st.state = PlaybackState::Stopped;
                st.current_index = 0;
                st.current_time_ms = first_ts;
                st.virtual_anchor_ms = first_ts;
                st.clock_anchor_ms = now;
                None
            }
        } else {
            st.current_index = next_index;
            let frame = frames[next_index].clone();
            st.current_time_ms = frame.timestamp_ms;
            st.virtual_anchor_ms = frame.timestamp_ms;
            st.clock_anchor_ms = now;
            let _ = self.inner.tx.send(frame.clone());
            Some(frame)
        }
    }

    /// Synchronously executes a single tick step against the clock, advancing frames as necessary.
    pub fn tick(&self) -> bool {
        self.inner.tick()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ResolvedPosition {
    /// Instantaneous virtual timestamp in milliseconds.
    virtual_time_ms: u64,
    /// Corresponding frame index in the session.
    index: usize,
    /// True if virtual time reached or exceeded session duration without loop.
    has_ended: bool,
    /// True if virtual time reached or exceeded session duration and wrapped around.
    has_wrapped: bool,
}

impl PlaybackInner {
    /// Computes instantaneous virtual timestamp and index from clock anchors without mutating state.
    fn resolve_position(&self, st: &PlaybackStateInner, now_ms: u64) -> ResolvedPosition {
        if self.session.is_empty() {
            return ResolvedPosition {
                virtual_time_ms: 0,
                index: 0,
                has_ended: true,
                has_wrapped: false,
            };
        }

        let first_ts = self.session.frames.first().map(|f| f.timestamp_ms).unwrap_or(0);
        let last_ts = self.session.frames.last().map(|f| f.timestamp_ms).unwrap_or(0);
        let total_duration = self.session.duration_ms();
        let total_samples = self.session.frames.len();

        let elapsed_real = now_ms.saturating_sub(st.clock_anchor_ms);
        let elapsed_virtual = (elapsed_real as f64 * st.speed as f64).round() as u64;
        let raw_virt_time = st.virtual_anchor_ms.saturating_add(elapsed_virtual);

        if raw_virt_time >= last_ts {
            if st.loop_playback {
                let virt_time = if total_duration > 0 {
                    let offset = (raw_virt_time.saturating_sub(first_ts)) % total_duration;
                    first_ts + offset
                } else {
                    first_ts
                };
                let pp = self.session.frames.partition_point(|f| f.timestamp_ms <= virt_time);
                let index = if pp == 0 {
                    0
                } else {
                    (pp - 1).min(total_samples - 1)
                };
                ResolvedPosition {
                    virtual_time_ms: virt_time,
                    index,
                    has_ended: false,
                    has_wrapped: true,
                }
            } else {
                ResolvedPosition {
                    virtual_time_ms: last_ts,
                    index: total_samples.saturating_sub(1),
                    has_ended: true,
                    has_wrapped: false,
                }
            }
        } else {
            let virt_time = raw_virt_time;
            let pp = self.session.frames.partition_point(|f| f.timestamp_ms <= virt_time);
            let index = if pp == 0 {
                0
            } else {
                (pp - 1).min(total_samples - 1)
            };
            ResolvedPosition {
                virtual_time_ms: virt_time,
                index,
                has_ended: false,
                has_wrapped: false,
            }
        }
    }

    /// Queries transport status strictly in a read-only manner without mutating internal state.
    fn get_status(&self) -> PlaybackStatus {
        let st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let total_duration_ms = self.session.duration_ms();
        let total_samples = self.session.sample_count();

        if self.session.is_empty() {
            return PlaybackStatus {
                state: st.state,
                current_time_ms: 0,
                total_duration_ms: 0,
                current_index: 0,
                total_samples: 0,
                speed: st.speed,
                loop_playback: st.loop_playback,
            };
        }

        let (current_time_ms, current_index) = if st.state == PlaybackState::Playing {
            let now = self.clock.now_ms();
            let pos = self.resolve_position(&st, now);
            (pos.virtual_time_ms, pos.index)
        } else {
            (st.current_time_ms, st.current_index)
        };

        PlaybackStatus {
            state: st.state,
            current_time_ms,
            total_duration_ms,
            current_index,
            total_samples,
            speed: st.speed,
            loop_playback: st.loop_playback,
        }
    }

    /// Executes a single playback tick step against the clock.
    /// Advances cursor, performs state transitions, and emits frames to broadcast subscribers.
    fn tick(&self) -> bool {
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if st.state != PlaybackState::Playing {
            return false;
        }
        if self.session.is_empty() {
            st.state = PlaybackState::Stopped;
            return false;
        }

        let first_ts = self.session.frames.first().map(|f| f.timestamp_ms).unwrap_or(0);
        let total_samples = self.session.frames.len();
        let now = self.clock.now_ms();
        let pos = self.resolve_position(&st, now);

        if pos.has_ended {
            // Session reached or exceeded end duration without looping:
            // emit all remaining frames up to end of session.
            for idx in (st.current_index + 1)..total_samples {
                let frame = self.session.frames[idx].clone();
                let _ = self.tx.send(frame);
            }
            st.state = PlaybackState::Stopped;
            st.current_index = 0;
            st.current_time_ms = first_ts;
            st.clock_anchor_ms = now;
            st.virtual_anchor_ms = first_ts;
            false
        } else if pos.has_wrapped {
            st.clock_anchor_ms = now;
            st.virtual_anchor_ms = pos.virtual_time_ms;
            st.current_time_ms = pos.virtual_time_ms;

            for idx in (st.current_index + 1)..total_samples {
                let frame = self.session.frames[idx].clone();
                let _ = self.tx.send(frame);
            }
            for idx in 0..=pos.index {
                let frame = self.session.frames[idx].clone();
                let _ = self.tx.send(frame);
            }
            st.current_index = pos.index;
            true
        } else {
            st.current_time_ms = pos.virtual_time_ms;
            if pos.index > st.current_index {
                for idx in (st.current_index + 1)..=pos.index {
                    let frame = self.session.frames[idx].clone();
                    let _ = self.tx.send(frame);
                }
                st.current_index = pos.index;
            }
            true
        }
    }
}

impl TelemetrySource for PlaybackEngine {
    fn subscribe_telemetry(&self) -> broadcast::Receiver<TelemetryFrame> {
        self.subscribe()
    }
}
