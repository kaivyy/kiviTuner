//! Datalogger session models, bounded buffer, and recording state machine.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::errors::ProtocolError;
use crate::model::{EcuIdentity, TelemetryFrame};

/// Default capacity for bounded datalog buffer (~6.4 MB in memory, ~83.3 min @ 20 Hz).
pub const DEFAULT_BUFFER_CAPACITY: usize = 100_000;

/// Lifecycle state machine for datalog session recording.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "state", content = "details")]
pub enum RecordingState {
    /// No recording active. Buffer is clear.
    #[default]
    Idle,
    /// Actively capturing incoming frames into the bounded memory buffer.
    Recording,
    /// Flushing final in-flight frames and computing summary statistics.
    Stopping,
    /// Recording has stopped; session data resides in memory ready for CSV export or direct playback.
    ReadyToSave,
    /// Current recording session has been exported to disk.
    Saved,
    /// Live stream was abruptly terminated (e.g. disconnect, timeout, app shutdown).
    /// All buffered frames are preserved and can be saved or recovered.
    Interrupted {
        reason: String,
        recoverable_samples: usize,
    },
}

impl RecordingState {
    /// Returns true if recording is idle.
    pub fn is_idle(&self) -> bool {
        matches!(self, RecordingState::Idle)
    }

    /// Returns true if actively recording incoming frames.
    pub fn is_recording(&self) -> bool {
        matches!(self, RecordingState::Recording)
    }

    /// Returns true if in the process of stopping.
    pub fn is_stopping(&self) -> bool {
        matches!(self, RecordingState::Stopping)
    }

    /// Returns true if recording is stopped and session is ready to save or play.
    pub fn is_ready_to_save(&self) -> bool {
        matches!(self, RecordingState::ReadyToSave)
    }

    /// Returns true if recording session has been saved to disk.
    pub fn is_saved(&self) -> bool {
        matches!(self, RecordingState::Saved)
    }

    /// Returns true if recording was interrupted unexpectedly.
    pub fn is_interrupted(&self) -> bool {
        matches!(self, RecordingState::Interrupted { .. })
    }
}

/// Metadata header describing a recorded datalog session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionMetadata {
    /// Unique identifier for this recording session.
    pub session_id: String,
    /// Schema version for session format (always 1 for v1.0).
    pub schema_version: u32,
    /// ISO-8601 UTC timestamp when recording commenced.
    pub started_at_iso: String,
    /// ISO-8601 UTC timestamp when recording concluded, if stopped.
    pub ended_at_iso: Option<String>,
    /// Connected ECU identity during recording, if available.
    pub ecu_identity: Option<EcuIdentity>,
    /// Nominal acquisition frequency in Hertz (default 20.0).
    pub sample_rate_hz: f32,
    /// Total count of telemetry frames received and processed during this session.
    pub total_samples_recorded: usize,
    /// Cumulative count of dropped or evicted samples (FIFO eviction + channel backpressure).
    pub dropped_samples_count: usize,
    /// Optional user notes or run description.
    pub notes: String,
}

impl SessionMetadata {
    /// Creates a new metadata block with standard v1 schema defaults.
    pub fn new(session_id: impl Into<String>, started_at_iso: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            schema_version: 1,
            started_at_iso: started_at_iso.into(),
            ended_at_iso: None,
            ecu_identity: None,
            sample_rate_hz: 20.0,
            total_samples_recorded: 0,
            dropped_samples_count: 0,
            notes: String::new(),
        }
    }
}

impl Default for SessionMetadata {
    fn default() -> Self {
        Self {
            session_id: String::new(),
            schema_version: 1,
            started_at_iso: String::new(),
            ended_at_iso: None,
            ecu_identity: None,
            sample_rate_hz: 20.0,
            total_samples_recorded: 0,
            dropped_samples_count: 0,
            notes: String::new(),
        }
    }
}

/// Complete in-memory datalog session containing metadata and all logged frames.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatalogSession {
    /// Session metadata header.
    pub metadata: SessionMetadata,
    /// Chronological list of recorded telemetry frames.
    pub frames: Vec<TelemetryFrame>,
}

impl DatalogSession {
    /// Creates a new datalog session.
    pub fn new(metadata: SessionMetadata, frames: Vec<TelemetryFrame>) -> Self {
        Self { metadata, frames }
    }

    /// Computes the total span in milliseconds from first frame to last frame.
    pub fn duration_ms(&self) -> u64 {
        if self.frames.len() < 2 {
            0
        } else {
            let start = self.frames.first().map(|f| f.timestamp_ms).unwrap_or(0);
            let end = self.frames.last().map(|f| f.timestamp_ms).unwrap_or(0);
            end.saturating_sub(start)
        }
    }

    /// Returns the number of frames present in this session.
    pub fn sample_count(&self) -> usize {
        self.frames.len()
    }

    /// Returns true if this session has no frames.
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Exports this datalog session to versioned CSV v1.0 format.
    pub fn to_csv(&self) -> Result<String, ProtocolError> {
        super::csv::serialize_session_to_string(self)
    }

    /// Parses a datalog session from versioned CSV v1.0 format string.
    pub fn from_csv_str(csv_str: &str) -> Result<Self, ProtocolError> {
        super::csv::parse_session_from_str(csv_str)
    }
}

/// Bounded in-memory circular buffer for high-frequency telemetry recording.
/// Capped at a configurable capacity (default 100,000 frames) with FIFO eviction.
#[derive(Debug, Clone)]
pub struct BoundedDatalogBuffer {
    capacity: usize,
    state: RecordingState,
    metadata: SessionMetadata,
    frames: VecDeque<TelemetryFrame>,
}

impl BoundedDatalogBuffer {
    /// Creates a buffer with default capacity (100,000 frames).
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_BUFFER_CAPACITY)
    }

    /// Creates a buffer with a custom capacity, clamped to at least 1.
    pub fn with_capacity(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            capacity,
            state: RecordingState::Idle,
            metadata: SessionMetadata::default(),
            frames: VecDeque::with_capacity(capacity.min(DEFAULT_BUFFER_CAPACITY)),
        }
    }

    /// Maximum number of frames this buffer can retain.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Current recording state machine status.
    pub fn state(&self) -> &RecordingState {
        &self.state
    }

    /// Read-only reference to session metadata.
    pub fn metadata(&self) -> &SessionMetadata {
        &self.metadata
    }

    /// Mutable reference to session metadata.
    pub fn metadata_mut(&mut self) -> &mut SessionMetadata {
        &mut self.metadata
    }

    /// Current number of frames stored in memory.
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// Returns true if no frames are currently buffered.
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Cumulative count of dropped or evicted samples.
    pub fn dropped_samples_count(&self) -> usize {
        self.metadata.dropped_samples_count
    }

    /// Total count of samples processed by this buffer.
    pub fn total_samples_recorded(&self) -> usize {
        self.metadata.total_samples_recorded
    }

    /// Increments dropped samples counter by the given count (e.g. broadcast backpressure lag).
    pub fn record_dropped_samples(&mut self, count: usize) {
        self.metadata.dropped_samples_count += count;
    }

    /// Read-only reference to the underlying frames queue.
    pub fn frames(&self) -> &VecDeque<TelemetryFrame> {
        &self.frames
    }

    /// Pushes a telemetry frame into the buffer.
    ///
    /// If capacity has been reached, the oldest frame is evicted (FIFO circular overwrite),
    /// `dropped_samples_count` is incremented, and `false` is returned.
    /// If frame is stored without eviction, `true` is returned.
    pub fn push_frame(&mut self, frame: TelemetryFrame) -> bool {
        self.metadata.total_samples_recorded += 1;
        let evicted = if self.frames.len() >= self.capacity {
            self.frames.pop_front();
            self.metadata.dropped_samples_count += 1;
            true
        } else {
            false
        };
        self.frames.push_back(frame);
        !evicted
    }

    /// Transitions the recording state machine to the target state.
    ///
    /// Validates state legality according to the lifecycle model:
    /// - `Idle` -> `Recording`
    /// - `Recording` -> `Stopping` | `ReadyToSave` | `Interrupted`
    /// - `Stopping` -> `ReadyToSave` | `Interrupted`
    /// - `ReadyToSave` -> `Saved` | `Idle` | `Recording`
    /// - `Saved` -> `Idle` | `Recording`
    /// - `Interrupted` -> `ReadyToSave` | `Idle`
    ///
    /// Returns `Err(ProtocolError::InvalidStateTransition)` if the transition is prohibited.
    pub fn transition_to(&mut self, next: RecordingState) -> Result<(), ProtocolError> {
        match (&self.state, &next) {
            // Idle transitions
            (RecordingState::Idle, RecordingState::Recording) => {
                self.frames.clear();
                self.metadata.total_samples_recorded = 0;
                self.metadata.dropped_samples_count = 0;
                self.state = next;
                Ok(())
            }

            // Recording transitions
            (RecordingState::Recording, RecordingState::Stopping) => {
                self.state = next;
                Ok(())
            }
            (RecordingState::Recording, RecordingState::ReadyToSave) => {
                self.state = next;
                Ok(())
            }
            (RecordingState::Recording, RecordingState::Interrupted { reason, .. }) => {
                self.state = RecordingState::Interrupted {
                    reason: reason.clone(),
                    recoverable_samples: self.frames.len(),
                };
                Ok(())
            }

            // Stopping transitions
            (RecordingState::Stopping, RecordingState::ReadyToSave) => {
                self.state = next;
                Ok(())
            }
            (RecordingState::Stopping, RecordingState::Interrupted { reason, .. }) => {
                self.state = RecordingState::Interrupted {
                    reason: reason.clone(),
                    recoverable_samples: self.frames.len(),
                };
                Ok(())
            }

            // ReadyToSave transitions
            (RecordingState::ReadyToSave, RecordingState::Saved) => {
                self.state = next;
                Ok(())
            }
            (RecordingState::ReadyToSave, RecordingState::Idle) => {
                self.frames.clear();
                self.metadata = SessionMetadata::default();
                self.state = RecordingState::Idle;
                Ok(())
            }
            (RecordingState::ReadyToSave, RecordingState::Recording) => {
                self.frames.clear();
                self.metadata.total_samples_recorded = 0;
                self.metadata.dropped_samples_count = 0;
                self.state = next;
                Ok(())
            }

            // Saved transitions
            (RecordingState::Saved, RecordingState::Idle) => {
                self.frames.clear();
                self.metadata = SessionMetadata::default();
                self.state = RecordingState::Idle;
                Ok(())
            }
            (RecordingState::Saved, RecordingState::Recording) => {
                self.frames.clear();
                self.metadata.total_samples_recorded = 0;
                self.metadata.dropped_samples_count = 0;
                self.state = next;
                Ok(())
            }

            // Interrupted transitions
            (RecordingState::Interrupted { .. }, RecordingState::ReadyToSave) => {
                self.state = RecordingState::ReadyToSave;
                Ok(())
            }
            (RecordingState::Interrupted { .. }, RecordingState::Idle) => {
                self.frames.clear();
                self.metadata = SessionMetadata::default();
                self.state = RecordingState::Idle;
                Ok(())
            }

            // Prohibited transitions
            (current, invalid_target) => Err(ProtocolError::InvalidStateTransition(format!(
                "Cannot transition recording state from {:?} to {:?}",
                current, invalid_target
            ))),
        }
    }

    /// Convenience helper to initialize metadata and transition into `Recording` state.
    pub fn start_recording(
        &mut self,
        session_id: impl Into<String>,
        started_at_iso: impl Into<String>,
        ecu_identity: Option<EcuIdentity>,
        notes: Option<String>,
    ) -> Result<(), ProtocolError> {
        self.transition_to(RecordingState::Recording)?;
        self.metadata = SessionMetadata::new(session_id, started_at_iso);
        self.metadata.ecu_identity = ecu_identity;
        if let Some(n) = notes {
            self.metadata.notes = n;
        }
        Ok(())
    }

    /// Convenience helper to transition through `Stopping` into `ReadyToSave` state.
    pub fn stop_recording(&mut self, ended_at_iso: Option<String>) -> Result<(), ProtocolError> {
        self.transition_to(RecordingState::Stopping)?;
        self.metadata.ended_at_iso = ended_at_iso;
        self.transition_to(RecordingState::ReadyToSave)?;
        Ok(())
    }

    /// Convenience helper to record an unexpected interruption with preserved frames.
    pub fn interrupt(&mut self, reason: impl Into<String>) -> Result<(), ProtocolError> {
        let reason_str = reason.into();
        self.transition_to(RecordingState::Interrupted {
            reason: reason_str,
            recoverable_samples: self.frames.len(),
        })
    }

    /// Convenience helper to mark session as saved to persistent storage.
    pub fn mark_saved(&mut self) -> Result<(), ProtocolError> {
        self.transition_to(RecordingState::Saved)
    }

    /// Convenience helper to acknowledge/recover from an interrupted state.
    pub fn recover_interrupted(&mut self) -> Result<(), ProtocolError> {
        self.transition_to(RecordingState::ReadyToSave)
    }

    /// Constructs an immutable `DatalogSession` from current buffer contents and metadata.
    pub fn build_session(&self) -> DatalogSession {
        DatalogSession {
            metadata: self.metadata.clone(),
            frames: self.frames.iter().cloned().collect(),
        }
    }
}

impl Default for BoundedDatalogBuffer {
    fn default() -> Self {
        Self::new()
    }
}
