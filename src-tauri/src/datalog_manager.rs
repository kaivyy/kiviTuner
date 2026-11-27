//! Datalog session recording, export, import, and playback coordinator for Tauri.

use kivi_protocol::datalogger::{
    BoundedDatalogBuffer, DatalogSession, PlaybackEngine, PlaybackState, PlaybackStatus,
    RecordingState, SessionMetadata,
};
use kivi_protocol::model::{EcuIdentity, TelemetryFrame};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

/// Status descriptor emitted over `kivi://datalog-status` representing recorder progress.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatalogRecorderStatus {
    /// Active recording state machine status.
    pub state: RecordingState,
    /// Elapsed duration in milliseconds from first recorded sample.
    pub elapsed_ms: u64,
    /// Number of samples currently held in the recording buffer.
    pub sample_count: usize,
    /// Number of dropped samples due to buffer saturation or channel backpressure.
    pub dropped_count: usize,
}

impl Default for DatalogRecorderStatus {
    fn default() -> Self {
        Self {
            state: RecordingState::Idle,
            elapsed_ms: 0,
            sample_count: 0,
            dropped_count: 0,
        }
    }
}

/// Helper to construct `DatalogRecorderStatus` from the current buffer state.
fn buffer_recorder_status(buf: &BoundedDatalogBuffer) -> DatalogRecorderStatus {
    let elapsed_ms = if buf.is_empty() {
        0
    } else {
        let first_ts = buf.frames().front().map(|f| f.timestamp_ms).unwrap_or(0);
        let last_ts = buf.frames().back().map(|f| f.timestamp_ms).unwrap_or(0);
        last_ts.saturating_sub(first_ts)
    };
    DatalogRecorderStatus {
        state: buf.state().clone(),
        elapsed_ms,
        sample_count: buf.len(),
        dropped_count: buf.dropped_samples_count(),
    }
}

/// Datalog manager coordinating recording buffers, CSV import/export, and playback transport.
pub struct DatalogManager {
    buffer: Arc<tokio::sync::Mutex<BoundedDatalogBuffer>>,
    active_session: Arc<tokio::sync::Mutex<Option<DatalogSession>>>,
    playback: Arc<tokio::sync::Mutex<Option<PlaybackEngine>>>,
    app_handle: Arc<tokio::sync::Mutex<Option<AppHandle>>>,
    recording_task: Arc<Mutex<Option<JoinHandle<()>>>>,
    playback_forward_task: Arc<Mutex<Option<JoinHandle<()>>>>,
    playback_status_task: Arc<Mutex<Option<JoinHandle<()>>>>,
}

impl Default for DatalogManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for DatalogManager {
    fn drop(&mut self) {
        if let Ok(mut g) = self.recording_task.lock() {
            if let Some(h) = g.take() {
                h.abort();
            }
        }
        if let Ok(mut g) = self.playback_forward_task.lock() {
            if let Some(h) = g.take() {
                h.abort();
            }
        }
        if let Ok(mut g) = self.playback_status_task.lock() {
            if let Some(h) = g.take() {
                h.abort();
            }
        }
    }
}

impl DatalogManager {
    /// Creates a new idle `DatalogManager`.
    pub fn new() -> Self {
        Self {
            buffer: Arc::new(tokio::sync::Mutex::new(BoundedDatalogBuffer::new())),
            active_session: Arc::new(tokio::sync::Mutex::new(None)),
            playback: Arc::new(tokio::sync::Mutex::new(None)),
            app_handle: Arc::new(tokio::sync::Mutex::new(None)),
            recording_task: Arc::new(Mutex::new(None)),
            playback_forward_task: Arc::new(Mutex::new(None)),
            playback_status_task: Arc::new(Mutex::new(None)),
        }
    }

    /// Helper to abort recording background task.
    fn abort_recording_task(&self) {
        let mut g = self
            .recording_task
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(h) = g.take() {
            h.abort();
        }
    }

    /// Helper to abort playback background tasks.
    fn abort_playback_tasks(&self) {
        {
            let mut g = self
                .playback_forward_task
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some(h) = g.take() {
                h.abort();
            }
        }
        {
            let mut g = self
                .playback_status_task
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some(h) = g.take() {
                h.abort();
            }
        }
    }

    /// Starts capturing telemetry frames from a broadcast receiver into the datalog buffer.
    pub async fn start_recording(
        &self,
        mut rx: broadcast::Receiver<TelemetryFrame>,
        identity: Option<EcuIdentity>,
        notes: Option<String>,
        app: Option<AppHandle>,
    ) -> Result<(), String> {
        if let Some(ref a) = app {
            *self.app_handle.lock().await = Some(a.clone());
        }

        let mut buf = self.buffer.lock().await;
        if buf.state().is_recording() {
            return Err("Recording already in progress".to_string());
        }

        // Abort previous recording task if still alive
        self.abort_recording_task();

        // If previously interrupted or saved, recover or transition to Idle first
        if buf.state().is_interrupted() {
            let _ = buf.recover_interrupted();
            let _ = buf.transition_to(RecordingState::Idle);
        } else if buf.state().is_saved() || buf.state().is_ready_to_save() {
            let _ = buf.transition_to(RecordingState::Idle);
        }

        let now_utc = chrono::Utc::now();
        let session_id = format!("ses-{}", now_utc.timestamp_millis());
        let started_at_iso = now_utc.to_rfc3339();

        buf.start_recording(session_id, started_at_iso, identity, notes)
            .map_err(|e| e.to_string())?;

        let initial_status = DatalogRecorderStatus {
            state: RecordingState::Recording,
            elapsed_ms: 0,
            sample_count: 0,
            dropped_count: 0,
        };

        let app_handle = self.app_handle.lock().await.clone().or(app);
        if let Some(ref a) = app_handle {
            let _ = a.emit("kivi://datalog-status", &initial_status);
        }

        // Spawn background task to consume telemetry frames
        let buffer_clone = self.buffer.clone();
        let app_clone = app_handle;

        let handle = tokio::spawn(async move {
            let mut frames_since_emit = 0usize;
            let mut last_emit = tokio::time::Instant::now();

            loop {
                match rx.recv().await {
                    Ok(frame) => {
                        let mut buf = buffer_clone.lock().await;
                        if !buf.state().is_recording() {
                            break;
                        }
                        buf.push_frame(frame);
                        frames_since_emit += 1;

                        let now = tokio::time::Instant::now();
                        let interval_elapsed = now.duration_since(last_emit) >= Duration::from_millis(250);

                        if frames_since_emit >= 10 || interval_elapsed {
                            frames_since_emit = 0;
                            last_emit = now;

                            let status = buffer_recorder_status(&buf);
                            drop(buf);

                            if let Some(ref a) = app_clone {
                                let _ = a.emit("kivi://datalog-status", &status);
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(dropped)) => {
                        let mut buf = buffer_clone.lock().await;
                        buf.record_dropped_samples(dropped as usize);
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        let mut buf = buffer_clone.lock().await;
                        if buf.state().is_recording() {
                            let _ = buf.interrupt("ECU disconnected during recording");
                            let status = buffer_recorder_status(&buf);
                            drop(buf);
                            if let Some(ref a) = app_clone {
                                let _ = a.emit("kivi://datalog-status", &status);
                            }
                        }
                        break;
                    }
                }
            }
        });

        if let Ok(mut g) = self.recording_task.lock() {
            *g = Some(handle);
        }

        Ok(())
    }

    /// Stops the active recording session, finalizing metadata and caching the session.
    pub async fn stop_recording(&self, app: Option<AppHandle>) -> Result<SessionMetadata, String> {
        if let Some(ref a) = app {
            *self.app_handle.lock().await = Some(a.clone());
        }

        let mut buf = self.buffer.lock().await;
        self.abort_recording_task();
        let ended_at_iso = chrono::Utc::now().to_rfc3339();

        if buf.state().is_recording() {
            buf.stop_recording(Some(ended_at_iso))
                .map_err(|e| e.to_string())?;
        } else if buf.state().is_interrupted() {
            buf.recover_interrupted().map_err(|e| e.to_string())?;
            buf.metadata_mut().ended_at_iso = Some(ended_at_iso);
        } else if buf.state().is_ready_to_save() || buf.state().is_saved() {
            // Already finalized
        } else {
            return Err("No active or stopped recording session to finalize".to_string());
        }

        let session = buf.build_session();
        let metadata = session.metadata.clone();
        let status = buffer_recorder_status(&buf);
        drop(buf);

        *self.active_session.lock().await = Some(session);

        let app_handle = self.app_handle.lock().await.clone().or(app);
        if let Some(ref a) = app_handle {
            let _ = a.emit("kivi://datalog-status", &status);
        }

        Ok(metadata)
    }

    /// Queries current recording buffer status.
    pub async fn get_recorder_status(&self) -> DatalogRecorderStatus {
        let buf = self.buffer.lock().await;
        if *buf.state() != RecordingState::Idle {
            return buffer_recorder_status(&buf);
        }
        drop(buf);

        if let Some(ref s) = *self.active_session.lock().await {
            DatalogRecorderStatus {
                state: RecordingState::ReadyToSave,
                elapsed_ms: s.duration_ms(),
                sample_count: s.sample_count(),
                dropped_count: s.metadata.dropped_samples_count,
            }
        } else {
            DatalogRecorderStatus::default()
        }
    }

    /// Exports active datalog session to CSV string format.
    pub async fn export_csv(&self) -> Result<String, String> {
        let mut buf = self.buffer.lock().await;
        if buf.state().is_recording() {
            return Err("Cannot export CSV while recording is in progress. Stop recording first.".to_string());
        }

        if buf.state().is_ready_to_save() || buf.state().is_saved() || buf.state().is_interrupted() {
            let session = buf.build_session();
            let csv = session.to_csv().map_err(|e| e.to_string())?;
            if buf.state().is_ready_to_save() {
                let _ = buf.mark_saved();
            }
            let status = buffer_recorder_status(&buf);
            drop(buf);

            *self.active_session.lock().await = Some(session);

            let app_handle = self.app_handle.lock().await.clone();
            if let Some(ref a) = app_handle {
                let _ = a.emit("kivi://datalog-status", &status);
            }

            Ok(csv)
        } else if let Some(ref session) = *self.active_session.lock().await {
            session.to_csv().map_err(|e| e.to_string())
        } else {
            Err("No datalog session available to export".to_string())
        }
    }

    /// Imports a datalog session from CSV string into memory.
    pub async fn import_csv(&self, csv_content: &str, app: Option<AppHandle>) -> Result<SessionMetadata, String> {
        if let Some(ref a) = app {
            *self.app_handle.lock().await = Some(a.clone());
        }

        let session = DatalogSession::from_csv_str(csv_content).map_err(|e| e.to_string())?;
        let meta = session.metadata.clone();

        let mut buf = self.buffer.lock().await;
        if buf.state().is_recording() {
            return Err("Cannot import CSV while recording is in progress. Stop recording first.".to_string());
        }
        *buf = BoundedDatalogBuffer::new();
        *self.active_session.lock().await = Some(session);
        Ok(meta)
    }

    /// Loads a datalog session into the playback engine.
    pub async fn load_playback(&self, csv_content: &str, app: Option<AppHandle>) -> Result<SessionMetadata, String> {
        if let Some(ref a) = app {
            *self.app_handle.lock().await = Some(a.clone());
        }

        let session = if !csv_content.trim().is_empty() {
            let s = DatalogSession::from_csv_str(csv_content).map_err(|e| e.to_string())?;
            *self.active_session.lock().await = Some(s.clone());
            s
        } else if let Some(ref s) = *self.active_session.lock().await {
            s.clone()
        } else {
            return Err("No datalog session provided or loaded in memory".to_string());
        };

        if session.is_empty() {
            return Err("Cannot load playback for empty datalog session".to_string());
        }

        let engine = PlaybackEngine::new(Arc::new(session.clone()));
        let status = engine.status();
        {
            let mut pb_guard = self.playback.lock().await;
            if let Some(old_engine) = pb_guard.take() {
                let _ = old_engine.stop();
            }
            self.abort_playback_tasks();
            *pb_guard = Some(engine);
        }

        let app_handle = self.app_handle.lock().await.clone().or(app);
        if let Some(ref a) = app_handle {
            let _ = a.emit("kivi://playback-status", &status);
        }

        Ok(session.metadata)
    }

    /// Starts or resumes datalog playback streaming.
    pub async fn start_playback(&self, app: Option<AppHandle>) -> Result<(), String> {
        if let Some(ref a) = app {
            *self.app_handle.lock().await = Some(a.clone());
        }

        let playback_guard = self.playback.lock().await;
        let engine = playback_guard
            .as_ref()
            .ok_or_else(|| "No datalog playback session loaded".to_string())?;

        if engine.status().state.is_playing() {
            return Ok(());
        }

        self.abort_playback_tasks();

        engine.play().map_err(|e| e.to_string())?;

        let app_handle = self.app_handle.lock().await.clone().or(app);
        let mut rx = engine.subscribe();

        // Spawn task to forward playback telemetry frames to Tauri event emitter
        let app_forward = app_handle.clone();
        let forward_handle = tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(frame) => {
                        if let Some(ref a) = app_forward {
                            let _ = a.emit("kivi://telemetry", &frame);
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        if let Ok(mut g) = self.playback_forward_task.lock() {
            *g = Some(forward_handle);
        }

        // Spawn periodic playback status reporting task
        let app_status = app_handle.clone();
        let engine_clone = engine.clone();
        let status_handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(50));
            loop {
                interval.tick().await;
                let status = engine_clone.status();
                if let Some(ref a) = app_status {
                    let _ = a.emit("kivi://playback-status", &status);
                }
                if status.state == PlaybackState::Stopped {
                    break;
                }
            }
        });
        if let Ok(mut g) = self.playback_status_task.lock() {
            *g = Some(status_handle);
        }

        let initial_status = engine.status();
        if let Some(ref a) = app_handle {
            let _ = a.emit("kivi://playback-status", &initial_status);
        }

        Ok(())
    }

    /// Pauses playback streaming.
    pub async fn pause_playback(&self) -> Result<(), String> {
        let playback_guard = self.playback.lock().await;
        let engine = playback_guard
            .as_ref()
            .ok_or_else(|| "No datalog playback session loaded".to_string())?;

        // Abort periodic status task so it stops ticking while paused
        if let Ok(mut g) = self.playback_status_task.lock() {
            if let Some(h) = g.take() {
                h.abort();
            }
        }

        engine.pause().map_err(|e| e.to_string())?;

        let status = engine.status();
        let app_handle = self.app_handle.lock().await.clone();
        if let Some(ref a) = app_handle {
            let _ = a.emit("kivi://playback-status", &status);
        }

        Ok(())
    }

    /// Seeks playback cursor to a specific timestamp in milliseconds.
    pub async fn seek_playback(&self, target_ms: u64) -> Result<(), String> {
        let playback_guard = self.playback.lock().await;
        let engine = playback_guard
            .as_ref()
            .ok_or_else(|| "No datalog playback session loaded".to_string())?;

        engine.seek_ms(target_ms).map_err(|e| e.to_string())?;

        let status = engine.status();
        let app_handle = self.app_handle.lock().await.clone();
        if let Some(ref a) = app_handle {
            let _ = a.emit("kivi://playback-status", &status);
        }

        Ok(())
    }

    /// Sets playback speed multiplier.
    pub async fn set_playback_speed(&self, speed: f32) -> Result<(), String> {
        let playback_guard = self.playback.lock().await;
        let engine = playback_guard
            .as_ref()
            .ok_or_else(|| "No datalog playback session loaded".to_string())?;

        engine.set_speed(speed).map_err(|e| e.to_string())?;

        let status = engine.status();
        let app_handle = self.app_handle.lock().await.clone();
        if let Some(ref a) = app_handle {
            let _ = a.emit("kivi://playback-status", &status);
        }

        Ok(())
    }

    /// Sets whether playback loops continuously.
    pub async fn set_playback_loop(&self, loop_enabled: bool) -> Result<(), String> {
        let playback_guard = self.playback.lock().await;
        let engine = playback_guard
            .as_ref()
            .ok_or_else(|| "No datalog playback session loaded".to_string())?;

        engine.set_loop(loop_enabled);

        let status = engine.status();
        let app_handle = self.app_handle.lock().await.clone();
        if let Some(ref a) = app_handle {
            let _ = a.emit("kivi://playback-status", &status);
        }

        Ok(())
    }

    /// Stops playback and resets cursor to start.
    pub async fn stop_playback(&self) -> Result<(), String> {
        let playback_guard = self.playback.lock().await;
        let engine = playback_guard
            .as_ref()
            .ok_or_else(|| "No datalog playback session loaded".to_string())?;

        self.abort_playback_tasks();

        engine.stop().map_err(|e| e.to_string())?;

        let status = engine.status();
        let app_handle = self.app_handle.lock().await.clone();
        if let Some(ref a) = app_handle {
            let _ = a.emit("kivi://playback-status", &status);
        }

        Ok(())
    }

    /// Unloads playback engine from memory.
    pub async fn unload_playback(&self) -> Result<(), String> {
        {
            let mut playback_guard = self.playback.lock().await;
            if let Some(engine) = playback_guard.take() {
                let _ = engine.stop();
            }
            self.abort_playback_tasks();
        }

        let status = PlaybackStatus::default();
        let app_handle = self.app_handle.lock().await.clone();
        if let Some(ref a) = app_handle {
            let _ = a.emit("kivi://playback-status", &status);
        }

        Ok(())
    }

    /// Queries current playback status.
    pub async fn get_playback_status(&self) -> PlaybackStatus {
        let playback_guard = self.playback.lock().await;
        if let Some(ref engine) = *playback_guard {
            engine.status()
        } else {
            PlaybackStatus::default()
        }
    }

    /// Retrieves a point-in-time snapshot of telemetry frames for analysis.
    ///
    /// Priority:
    /// 1. Playback session frames (if playback engine is loaded).
    /// 2. Active finalized session frames (if present).
    /// 3. Recording buffer frames (if present).
    pub async fn get_frames_snapshot(&self) -> (Vec<TelemetryFrame>, String) {
        let playback_guard = self.playback.lock().await;
        if let Some(ref engine) = *playback_guard {
            return (engine.session().frames.clone(), "playback".to_string());
        }
        drop(playback_guard);

        let active_guard = self.active_session.lock().await;
        if let Some(ref session) = *active_guard {
            return (session.frames.clone(), "active_session".to_string());
        }
        drop(active_guard);

        let buf_guard = self.buffer.lock().await;
        let frames: Vec<TelemetryFrame> = buf_guard.frames().iter().cloned().collect();
        let prov = if frames.is_empty() {
            "empty".to_string()
        } else {
            "recording_buffer".to_string()
        };
        (frames, prov)
    }

    /// Handles unexpected or explicit ECU disconnection during recording.
    pub async fn handle_ecu_disconnect(&self, app: Option<AppHandle>) {
        if let Some(ref a) = app {
            *self.app_handle.lock().await = Some(a.clone());
        }

        let mut buf = self.buffer.lock().await;
        self.abort_recording_task();
        if buf.state().is_recording() {
            let _ = buf.interrupt("ECU disconnected during recording");
            let status = buffer_recorder_status(&buf);
            drop(buf);
            let app_handle = self.app_handle.lock().await.clone().or(app);
            if let Some(ref a) = app_handle {
                let _ = a.emit("kivi://datalog-status", &status);
            }
        }
    }
}
