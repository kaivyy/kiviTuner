//! Streaming progress channels and async workspace dump/flash commands.

use crate::commands::{
    load_bin_data_with_meta, lock_or_err, window_io::apply_repaired_slice_if_unchanged, AppState,
};
use crate::flash_commands::{
    dump_ecu_inner_with_progress, dump_ecu_uds23_inner_with_progress,
    flash_ecu_prepared_inner_with_progress, FlashEngineState, FlashResultPayload,
};
use kivi_core::bin_export::validate_dumped_bin_integrity;
use kivi_core::hash::BinaryHashes;
use kivi_flashing::flash_engine::FlashTransferStage;
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FlashProgressEvent {
    pub stage: String,
    pub percent: u8,
    pub bytes_done: usize,
    pub bytes_total: usize,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DumpStreamPayload {
    pub base_address: u32,
    pub size: usize,
    pub blocks_read: usize,
    pub hashes: Option<BinaryHashes>,
    pub loaded_into_workspace: bool,
    pub bytes: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UdsDumpStreamPayload {
    pub base_address: u32,
    pub size: usize,
    pub blocks_read: usize,
    pub pending_retries: usize,
    pub hashes: Option<BinaryHashes>,
    pub loaded_into_workspace: bool,
    pub bytes: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum PreparedFlashPhase {
    Preparing(&'static str),
    Transfer(FlashTransferStage),
}

fn scaled_percent(base: u8, span: u8, done: usize, total: usize) -> u8 {
    let denom = total.max(1) as u64;
    let clamped = (done as u64).min(denom);
    (base as u64 + (clamped * span as u64) / denom).min(100) as u8
}

struct ProgressSample {
    stage: &'static str,
    percent: u8,
    bytes_done: usize,
    bytes_total: usize,
    blocks: usize,
    force: bool,
}

struct FlashProgressThrottler {
    channel: tauri::ipc::Channel<FlashProgressEvent>,
    last_stage: Option<&'static str>,
    last_percent: Option<u8>,
    last_blocks: usize,
}

impl FlashProgressThrottler {
    fn new(channel: tauri::ipc::Channel<FlashProgressEvent>) -> Self {
        Self {
            channel,
            last_stage: None,
            last_percent: None,
            last_blocks: 0,
        }
    }

    fn emit<M>(&mut self, sample: ProgressSample, message: M)
    where
        M: FnOnce() -> String,
    {
        let stage_changed = self.last_stage != Some(sample.stage);
        let pct_advanced = self.last_percent.is_none_or(|prev| sample.percent > prev);
        let step_blocks = sample.blocks.saturating_sub(self.last_blocks) >= 16;
        if !sample.force && !stage_changed && !pct_advanced && !step_blocks {
            return;
        }
        self.last_stage = Some(sample.stage);
        self.last_percent = Some(sample.percent);
        self.last_blocks = sample.blocks;
        let _ = self.channel.send(FlashProgressEvent {
            stage: sample.stage.to_string(),
            percent: sample.percent,
            bytes_done: sample.bytes_done,
            bytes_total: sample.bytes_total,
            message: message(),
        });
    }
}

fn finalize_streamed_dump(
    bytes: &[u8],
    label_prefix: &str,
    address: u32,
    total_requested: usize,
    load_into_workspace: Option<bool>,
    throttler: &mut FlashProgressThrottler,
    app_state: &AppState,
) -> Result<(usize, Option<BinaryHashes>, bool), String> {
    let into_ws = load_into_workspace.unwrap_or(true);
    let size = bytes.len();
    let hashes = if into_ws {
        throttler.emit(
            ProgressSample {
                stage: "ingesting",
                percent: 92,
                bytes_done: size,
                bytes_total: total_requested,
                blocks: 0,
                force: true,
            },
            || "Indexing dumped ROM into active Rust workspace".to_string(),
        );
        Some(load_bin_data_with_meta(
            bytes,
            Some(&format!("{label_prefix} @ 0x{address:06X}")),
            None,
            app_state,
        )?)
    } else {
        None
    };
    Ok((size, hashes, into_ws))
}

#[tauri::command]
pub async fn dump_ecu_stream(
    address: u32,
    length: usize,
    block_size: usize,
    load_into_workspace: Option<bool>,
    on_progress: tauri::ipc::Channel<FlashProgressEvent>,
    flash_state: State<'_, FlashEngineState>,
    app_state: State<'_, AppState>,
) -> Result<DumpStreamPayload, String> {
    let mut throttler = FlashProgressThrottler::new(on_progress.clone());
    throttler.emit(
        ProgressSample {
            stage: "reading",
            percent: 5,
            bytes_done: 0,
            bytes_total: length,
            blocks: 0,
            force: true,
        },
        || format!("Reading {length} bytes from ECU @ 0x{address:06X}"),
    );

    let flash_handle = flash_state.inner().clone_handle();
    let worker_channel = on_progress.clone();
    let dump = tauri::async_runtime::spawn_blocking(move || {
        let mut worker_throttler = FlashProgressThrottler::new(worker_channel);
        dump_ecu_inner_with_progress(
            address,
            length,
            block_size,
            &flash_handle,
            |done, total, blocks| {
                let pct = scaled_percent(5, 85, done, total);
                worker_throttler.emit(
                    ProgressSample {
                        stage: "reading",
                        percent: pct,
                        bytes_done: done,
                        bytes_total: total,
                        blocks,
                        force: done >= total,
                    },
                    || format!("Reading ECU block {blocks} ({done}/{total} bytes)"),
                );
            },
        )
    })
    .await
    .map_err(|e| format!("ECU dump worker panicked: {e}"))??;

    let (size, hashes, into_ws) = finalize_streamed_dump(
        &dump.bytes,
        "ECU Dump",
        address,
        length,
        load_into_workspace,
        &mut throttler,
        &app_state,
    )?;
    throttler.emit(
        ProgressSample {
            stage: "complete",
            percent: 100,
            bytes_done: size,
            bytes_total: length,
            blocks: dump.blocks_read,
            force: true,
        },
        || format!("Dump finished ({} blocks)", dump.blocks_read),
    );
    Ok(DumpStreamPayload {
        base_address: dump.base_address,
        size,
        blocks_read: dump.blocks_read,
        hashes,
        loaded_into_workspace: into_ws,
        bytes: if into_ws { None } else { Some(dump.bytes) },
    })
}

#[tauri::command]
pub async fn dump_ecu_uds23_stream(
    address: u32,
    length: usize,
    block_size: usize,
    load_into_workspace: Option<bool>,
    on_progress: tauri::ipc::Channel<FlashProgressEvent>,
    flash_state: State<'_, FlashEngineState>,
    app_state: State<'_, AppState>,
) -> Result<UdsDumpStreamPayload, String> {
    let mut throttler = FlashProgressThrottler::new(on_progress.clone());
    throttler.emit(
        ProgressSample {
            stage: "reading",
            percent: 5,
            bytes_done: 0,
            bytes_total: length,
            blocks: 0,
            force: true,
        },
        || format!("UDS 0x23 reading {length} bytes @ 0x{address:06X}"),
    );

    let flash_handle = flash_state.inner().clone_handle();
    let worker_channel = on_progress.clone();
    let dump = tauri::async_runtime::spawn_blocking(move || {
        let mut worker_throttler = FlashProgressThrottler::new(worker_channel);
        dump_ecu_uds23_inner_with_progress(
            address,
            length,
            block_size,
            &flash_handle,
            |done, total, blocks, pending| {
                let pct = scaled_percent(5, 85, done, total);
                worker_throttler.emit(
                    ProgressSample {
                        stage: "reading",
                        percent: pct,
                        bytes_done: done,
                        bytes_total: total,
                        blocks,
                        force: done >= total,
                    },
                    || {
                        if pending > 0 {
                            format!(
                                "UDS 0x23 block {blocks} ({done}/{total} bytes, {pending} pending retries)"
                            )
                        } else {
                            format!("UDS 0x23 block {blocks} ({done}/{total} bytes)")
                        }
                    },
                );
            },
        )
    })
    .await
    .map_err(|e| format!("UDS 0x23 dump worker panicked: {e}"))??;

    if dump.bytes.len() >= 1024 {
        validate_dumped_bin_integrity(&dump.bytes)
            .map_err(|e| format!("UDS 0x23 dump integrity check failed: {e}"))?;
    }
    let (size, hashes, into_ws) = finalize_streamed_dump(
        &dump.bytes,
        "ECU UDS 0x23 Dump",
        address,
        length,
        load_into_workspace,
        &mut throttler,
        &app_state,
    )?;
    throttler.emit(
        ProgressSample {
            stage: "complete",
            percent: 100,
            bytes_done: size,
            bytes_total: length,
            blocks: dump.blocks_read,
            force: true,
        },
        || {
            format!(
                "UDS 0x23 dump finished ({} blocks, {} pending retries)",
                dump.blocks_read, dump.pending_retries
            )
        },
    );
    Ok(UdsDumpStreamPayload {
        base_address: dump.base_address,
        size,
        blocks_read: dump.blocks_read,
        pending_retries: dump.pending_retries,
        hashes,
        loaded_into_workspace: into_ws,
        bytes: if into_ws { None } else { Some(dump.bytes) },
    })
}

#[tauri::command]
pub async fn flash_ecu_workspace_stream(
    address: u32,
    block_size: usize,
    on_progress: tauri::ipc::Channel<FlashProgressEvent>,
    flash_state: State<'_, FlashEngineState>,
    app_state: State<'_, AppState>,
) -> Result<FlashResultPayload, String> {
    let (image, active_doc_id, pre_flash_sha256) = {
        let docs_guard = lock_or_err(&app_state.documents, "documents")?;
        let guard = lock_or_err(&app_state.memory, "memory")?;
        let mem = guard.as_ref().ok_or_else(|| "No binary loaded".to_string())?;
        (
            mem.as_slice().to_vec(),
            docs_guard.active_id.clone(),
            mem.hashes().sha256.clone(),
        )
    };
    let total = image.len();
    let flash_handle = flash_state.inner().clone_handle();
    let worker_channel = on_progress.clone();
    let worker_image = image.clone();

    let (outcome, prepared) = tauri::async_runtime::spawn_blocking(move || {
        let mut worker_throttler = FlashProgressThrottler::new(worker_channel);
        let mut prep_step = 0u8;
        flash_ecu_prepared_inner_with_progress(
            address,
            &worker_image,
            block_size,
            &flash_handle,
            |phase, done, needed, blocks| match phase {
                PreparedFlashPhase::Preparing(msg) => {
                    prep_step = if prep_step == 0 { 1 } else { 2 };
                    let pct = if prep_step == 1 { 5 } else { 12 };
                    worker_throttler.emit(
                        ProgressSample {
                            stage: "preparing",
                            percent: pct,
                            bytes_done: 0,
                            bytes_total: needed,
                            blocks: 0,
                            force: true,
                        },
                        || msg.to_string(),
                    );
                }
                PreparedFlashPhase::Transfer(FlashTransferStage::Erasing) => {
                    worker_throttler.emit(
                        ProgressSample {
                            stage: "erasing",
                            percent: 15,
                            bytes_done: done,
                            bytes_total: needed,
                            blocks,
                            force: true,
                        },
                        || format!("Erasing ECU flash sectors from 0x{address:06X}"),
                    );
                }
                PreparedFlashPhase::Transfer(FlashTransferStage::Writing) => {
                    let pct = scaled_percent(20, 65, done, needed);
                    worker_throttler.emit(
                        ProgressSample {
                            stage: "writing",
                            percent: pct,
                            bytes_done: done,
                            bytes_total: needed,
                            blocks,
                            force: done >= needed,
                        },
                        || format!("Writing block {blocks} ({done}/{needed} bytes)"),
                    );
                }
                PreparedFlashPhase::Transfer(FlashTransferStage::Verifying) => {
                    let pct = scaled_percent(88, 10, done, needed);
                    worker_throttler.emit(
                        ProgressSample {
                            stage: "verifying",
                            percent: pct,
                            bytes_done: done,
                            bytes_total: needed,
                            blocks,
                            force: done == 0 || done >= needed,
                        },
                        || format!("Verifying written bytes ({done}/{needed} bytes)"),
                    );
                }
            },
        )
    })
    .await
    .map_err(|e| format!("Flash worker panicked: {e}"))??;

    let mut throttler = FlashProgressThrottler::new(on_progress);
    let mut completion_note = format!(
        "Flash verified ({} bytes, {} blocks)",
        outcome.bytes_written, outcome.blocks_written
    );
    if prepared != image {
        let applied = apply_repaired_slice_if_unchanged(
            &app_state,
            &prepared,
            active_doc_id.as_deref(),
            &pre_flash_sha256,
            "Pre-flash auto-checksum repair",
        )?;
        if applied.is_none() {
            completion_note.push_str(
                " — skipped workspace checksum sync because active ROM changed during flash",
            );
        }
    }
    throttler.emit(
        ProgressSample {
            stage: "complete",
            percent: 100,
            bytes_done: outcome.bytes_written,
            bytes_total: total,
            blocks: outcome.blocks_written,
            force: true,
        },
        || completion_note,
    );
    Ok(outcome)
}
