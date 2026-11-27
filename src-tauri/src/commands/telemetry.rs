use super::AppState;
use crate::datalog_manager::DatalogRecorderStatus;
use kivi_defs::model::MapDef;
use kivi_protocol::autotune::{AutoTuneConfig, AutoTuneEngine, RecommendationReport};
use kivi_protocol::correlation::{correlate_frame, CorrelationResult, TableBindingConfig};
use kivi_protocol::datalogger::{PlaybackStatus, SessionMetadata};
use kivi_protocol::driver::DriverDescriptor;
use kivi_protocol::model::{ConnectionState, EcuIdentity, TelemetryFrame};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use thiserror::Error;

pub fn list_ecu_drivers_inner(state: &AppState) -> Result<Vec<DriverDescriptor>, String> {
    Ok(state.telemetry.list_drivers())
}

pub async fn connect_ecu_inner(
    driver_id: &str,
    app: Option<AppHandle>,
    state: &AppState,
) -> Result<EcuIdentity, String> {
    state.telemetry.connect(driver_id, app).await
}

pub async fn disconnect_ecu_inner(
    app: Option<AppHandle>,
    state: &AppState,
) -> Result<(), String> {
    state.datalog.handle_ecu_disconnect(app.clone()).await;
    state.telemetry.disconnect(app).await
}

pub async fn get_connection_status_inner(
    state: &AppState,
) -> Result<ConnectionState, String> {
    Ok(state.telemetry.get_status().await)
}

pub async fn get_latest_telemetry_inner(
    state: &AppState,
) -> Result<Option<TelemetryFrame>, String> {
    Ok(state.telemetry.get_latest_frame().await)
}

pub async fn start_datalog_recording_inner(
    notes: Option<String>,
    app: Option<AppHandle>,
    state: &AppState,
) -> Result<(), String> {
    let (rx, identity) = state.telemetry.subscribe_with_identity().await?;
    state.datalog.start_recording(rx, identity, notes, app).await
}

pub async fn stop_datalog_recording_inner(
    app: Option<AppHandle>,
    state: &AppState,
) -> Result<SessionMetadata, String> {
    state.datalog.stop_recording(app).await
}

pub async fn get_datalog_status_inner(
    state: &AppState,
) -> Result<DatalogRecorderStatus, String> {
    Ok(state.datalog.get_recorder_status().await)
}

pub async fn export_datalog_csv_inner(
    state: &AppState,
) -> Result<String, String> {
    state.datalog.export_csv().await
}

pub async fn import_datalog_csv_inner(
    csv_content: String,
    app: Option<AppHandle>,
    state: &AppState,
) -> Result<SessionMetadata, String> {
    state.datalog.import_csv(&csv_content, app).await
}

pub async fn load_datalog_playback_inner(
    csv_content: String,
    app: Option<AppHandle>,
    state: &AppState,
) -> Result<SessionMetadata, String> {
    state.datalog.load_playback(&csv_content, app).await
}

pub async fn start_datalog_playback_inner(
    app: Option<AppHandle>,
    state: &AppState,
) -> Result<(), String> {
    state.datalog.start_playback(app).await
}

pub async fn pause_datalog_playback_inner(
    state: &AppState,
) -> Result<(), String> {
    state.datalog.pause_playback().await
}

pub async fn seek_datalog_playback_inner(
    target_ms: u64,
    state: &AppState,
) -> Result<(), String> {
    state.datalog.seek_playback(target_ms).await
}

pub async fn set_playback_speed_inner(
    speed: f32,
    state: &AppState,
) -> Result<(), String> {
    state.datalog.set_playback_speed(speed).await
}

pub async fn set_playback_loop_inner(
    loop_enabled: bool,
    state: &AppState,
) -> Result<(), String> {
    state.datalog.set_playback_loop(loop_enabled).await
}

pub async fn stop_datalog_playback_inner(
    state: &AppState,
) -> Result<(), String> {
    state.datalog.stop_playback().await
}

pub async fn unload_datalog_playback_inner(
    state: &AppState,
) -> Result<(), String> {
    state.datalog.unload_playback().await
}

pub async fn get_playback_status_inner(
    state: &AppState,
) -> Result<PlaybackStatus, String> {
    Ok(state.datalog.get_playback_status().await)
}

#[tauri::command]
pub fn list_ecu_drivers(state: State<'_, AppState>) -> Result<Vec<DriverDescriptor>, String> {
    list_ecu_drivers_inner(&state)
}

#[tauri::command]
pub async fn connect_ecu(
    driver_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<EcuIdentity, String> {
    connect_ecu_inner(&driver_id, Some(app), &state).await
}

#[tauri::command]
pub async fn disconnect_ecu(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    disconnect_ecu_inner(Some(app), &state).await
}

#[tauri::command]
pub async fn get_connection_status(
    state: State<'_, AppState>,
) -> Result<ConnectionState, String> {
    get_connection_status_inner(&state).await
}

#[tauri::command]
pub async fn get_latest_telemetry(
    state: State<'_, AppState>,
) -> Result<Option<TelemetryFrame>, String> {
    get_latest_telemetry_inner(&state).await
}

#[tauri::command]
pub async fn start_datalog_recording(
    notes: Option<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    start_datalog_recording_inner(notes, Some(app), &state).await
}

#[tauri::command]
pub async fn stop_datalog_recording(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<SessionMetadata, String> {
    stop_datalog_recording_inner(Some(app), &state).await
}

#[tauri::command]
pub async fn get_datalog_status(
    state: State<'_, AppState>,
) -> Result<DatalogRecorderStatus, String> {
    get_datalog_status_inner(&state).await
}

#[tauri::command]
pub async fn export_datalog_csv(
    state: State<'_, AppState>,
) -> Result<String, String> {
    export_datalog_csv_inner(&state).await
}

#[tauri::command]
pub async fn import_datalog_csv(
    csv_content: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<SessionMetadata, String> {
    import_datalog_csv_inner(csv_content, Some(app), &state).await
}

#[tauri::command]
pub async fn load_datalog_playback(
    csv_content: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<SessionMetadata, String> {
    load_datalog_playback_inner(csv_content, Some(app), &state).await
}

#[tauri::command]
pub async fn start_datalog_playback(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    start_datalog_playback_inner(Some(app), &state).await
}

#[tauri::command]
pub async fn pause_datalog_playback(
    state: State<'_, AppState>,
) -> Result<(), String> {
    pause_datalog_playback_inner(&state).await
}

#[tauri::command]
pub async fn seek_datalog_playback(
    target_ms: u64,
    state: State<'_, AppState>,
) -> Result<(), String> {
    seek_datalog_playback_inner(target_ms, &state).await
}

#[tauri::command]
pub async fn set_playback_speed(
    speed: f32,
    state: State<'_, AppState>,
) -> Result<(), String> {
    set_playback_speed_inner(speed, &state).await
}

#[tauri::command]
pub async fn set_playback_loop(
    loop_enabled: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    set_playback_loop_inner(loop_enabled, &state).await
}

#[tauri::command]
pub async fn stop_datalog_playback(
    state: State<'_, AppState>,
) -> Result<(), String> {
    stop_datalog_playback_inner(&state).await
}

#[tauri::command]
pub async fn unload_datalog_playback(
    state: State<'_, AppState>,
) -> Result<(), String> {
    unload_datalog_playback_inner(&state).await
}

#[tauri::command]
pub async fn get_playback_status(
    state: State<'_, AppState>,
) -> Result<PlaybackStatus, String> {
    get_playback_status_inner(&state).await
}

// ============================================================================
// Milestone 3C, Task 4: Correlation & Auto-Tune Typed IPC Contracts & Handlers
// ============================================================================

/// Machine-readable errors for telemetry-to-map correlation IPC command.
#[derive(Error, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "error", content = "details")]
pub enum CorrelationCommandError {
    #[error("Map with ID '{table_id}' not found")]
    MapNotFound { table_id: String },
    #[error("Invalid table dimensions: rows={rows}, cols={cols}: {reason}")]
    InvalidDimensions {
        rows: usize,
        cols: usize,
        reason: String,
    },
    #[error("Telemetry frame unavailable: {reason}")]
    TelemetryUnavailable { reason: String },
    #[error("ROM binary not loaded into memory")]
    RomNotLoaded,
    #[error("Internal correlation error: {message}")]
    InternalError { message: String },
}

/// Request payload for `correlate_telemetry`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CorrelateTelemetryRequest {
    #[serde(default)]
    pub table_id: Option<String>,
    #[serde(default)]
    pub map_def: Option<MapDef>,
    #[serde(default)]
    pub frame: Option<TelemetryFrame>,
    #[serde(default)]
    pub binding_config: Option<TableBindingConfig>,
    #[serde(default)]
    pub source_epoch: Option<u64>,
    #[serde(default)]
    pub request_id: Option<String>,
}

/// Response payload for `correlate_telemetry`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CorrelateTelemetryResponse {
    pub request_id: Option<String>,
    pub source_epoch: Option<u64>,
    pub table_id: String,
    pub source: String,
    pub result: CorrelationResult,
}

/// Machine-readable errors for Auto-Tune recommendation IPC command.
#[derive(Error, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "error", content = "details")]
pub enum AutoTuneCommandError {
    #[error("Map with ID '{table_id}' not found")]
    MapNotFound { table_id: String },
    #[error("Invalid table dimensions: rows={rows}, cols={cols}: {reason}")]
    InvalidDimensions {
        rows: usize,
        cols: usize,
        reason: String,
    },
    #[error("Invalid auto-tune configuration: {reason}")]
    InvalidConfiguration { reason: String },
    #[error("No telemetry frames available for auto-tune analysis")]
    NoFramesAvailable,
    #[error("ROM binary not loaded into memory")]
    RomNotLoaded,
    #[error("Internal auto-tune error: {message}")]
    InternalError { message: String },
}

/// Request payload for `compute_autotune_recommendations`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AutoTuneRequest {
    #[serde(default)]
    pub table_id: Option<String>,
    #[serde(default)]
    pub map_def: Option<MapDef>,
    #[serde(default)]
    pub config: AutoTuneConfig,
    #[serde(default)]
    pub frames: Option<Vec<TelemetryFrame>>,
    #[serde(default)]
    pub request_id: Option<String>,
}

/// Response payload for `compute_autotune_recommendations`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutoTuneResponse {
    pub request_id: Option<String>,
    pub table_id: String,
    pub source_provenance: String,
    pub report: RecommendationReport,
}

/// Pure handler for `correlate_telemetry`.
pub async fn correlate_telemetry_inner(
    request: CorrelateTelemetryRequest,
    state: &AppState,
) -> Result<CorrelateTelemetryResponse, CorrelationCommandError> {
    // 1 & 4. Resolve MapDef and snapshot MemoryMap cleanly in canonical lock order (`memory -> maps`)
    let mem_snapshot = if request.map_def.is_none() {
        Some(
            super::lock_or_err(&state.memory, "memory")
                .map_err(|message| CorrelationCommandError::InternalError { message })?
                .clone(),
        )
    } else {
        None
    };

    let map_def = if let Some(map) = request.map_def {
        map
    } else if let Some(ref id) = request.table_id {
        let maps = super::lock_or_err(&state.maps, "maps")
            .map_err(|message| CorrelationCommandError::InternalError { message })?;
        maps.iter()
            .find(|m| m.id == *id)
            .cloned()
            .ok_or_else(|| CorrelationCommandError::MapNotFound {
                table_id: id.clone(),
            })?
    } else {
        return Err(CorrelationCommandError::MapNotFound {
            table_id: "<unspecified>".to_string(),
        });
    };

    // 2. Validate dimensions
    if map_def.rows == 0 || map_def.cols == 0 {
        return Err(CorrelationCommandError::InvalidDimensions {
            rows: map_def.rows,
            cols: map_def.cols,
            reason: "Table rows and columns must be greater than zero".to_string(),
        });
    }

    // 3. Validate telemetry frame
    let (frame, source) = if let Some(f) = request.frame {
        (f, "provided".to_string())
    } else {
        let latest = get_latest_telemetry_inner(state).await.map_err(|e| {
            CorrelationCommandError::TelemetryUnavailable { reason: e }
        })?;
        match latest {
            Some(f) => (f, "live".to_string()),
            None => {
                return Err(CorrelationCommandError::TelemetryUnavailable {
                    reason: "No active live telemetry or playback frame available".to_string(),
                });
            }
        }
    };

    // 4. Access MemoryMap (use snapshot if acquired atomically before `maps`, otherwise lock now)
    let result = if let Some(ref snap) = mem_snapshot {
        correlate_frame(&frame, &map_def, snap.as_ref(), request.binding_config.as_ref())
    } else {
        let mem_guard = super::lock_or_err(&state.memory, "memory")
            .map_err(|message| CorrelationCommandError::InternalError { message })?;
        correlate_frame(
            &frame,
            &map_def,
            mem_guard.as_ref(),
            request.binding_config.as_ref(),
        )
    };

    Ok(CorrelateTelemetryResponse {
        request_id: request.request_id,
        source_epoch: request.source_epoch,
        table_id: map_def.id,
        source,
        result,
    })
}

/// Pure handler for `compute_autotune_recommendations`.
pub async fn compute_autotune_recommendations_inner(
    request: AutoTuneRequest,
    state: &AppState,
) -> Result<AutoTuneResponse, AutoTuneCommandError> {
    // 1 & 5. Snapshot MemoryMap before locking `maps` (canonical lock order: memory -> maps)
    let mem_snapshot = super::lock_or_err(&state.memory, "memory")
        .map_err(|message| AutoTuneCommandError::InternalError { message })?
        .clone();

    let map_def = if let Some(map) = request.map_def {
        map
    } else if let Some(ref id) = request.table_id {
        let maps = super::lock_or_err(&state.maps, "maps")
            .map_err(|message| AutoTuneCommandError::InternalError { message })?;
        maps.iter()
            .find(|m| m.id == *id)
            .cloned()
            .ok_or_else(|| AutoTuneCommandError::MapNotFound {
                table_id: id.clone(),
            })?
    } else {
        return Err(AutoTuneCommandError::MapNotFound {
            table_id: "<unspecified>".to_string(),
        });
    };

    // 2. Validate dimensions
    if map_def.rows == 0 || map_def.cols == 0 {
        return Err(AutoTuneCommandError::InvalidDimensions {
            rows: map_def.rows,
            cols: map_def.cols,
            reason: "Table rows and columns must be greater than zero".to_string(),
        });
    }

    // 3. Validate configuration at the boundary
    if let Err(e) = request.config.validate(map_def.rows, map_def.cols) {
        return Err(AutoTuneCommandError::InvalidConfiguration {
            reason: e.to_string(),
        });
    }

    // 4. Resolve telemetry frames
    let (frames, source_provenance) = if let Some(f) = request.frames {
        (f, "provided".to_string())
    } else {
        state.datalog.get_frames_snapshot().await
    };

    // 6. Compute recommendations
    let report = AutoTuneEngine::compute(
        &frames,
        &map_def,
        mem_snapshot.as_ref(),
        &request.config,
    );

    Ok(AutoTuneResponse {
        request_id: request.request_id,
        table_id: map_def.id,
        source_provenance,
        report,
    })
}

#[tauri::command]
pub async fn correlate_telemetry(
    request: CorrelateTelemetryRequest,
    state: State<'_, AppState>,
) -> Result<CorrelateTelemetryResponse, CorrelationCommandError> {
    correlate_telemetry_inner(request, &state).await
}

#[tauri::command]
pub async fn compute_autotune_recommendations(
    request: AutoTuneRequest,
    state: State<'_, AppState>,
) -> Result<AutoTuneResponse, AutoTuneCommandError> {
    compute_autotune_recommendations_inner(request, &state).await
}

