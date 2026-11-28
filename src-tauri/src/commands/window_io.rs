use super::rom::{cell_offset, load_bin_data_with_meta};
use super::AppState;
use kivi_checksum::plugins::honda_keihin::compute_keihin_checksum_byte;
use kivi_core::hash::BinaryHashes;
use kivi_core::ingest::ingest_binary;
use kivi_core::memory_map::MemoryMap;
use kivi_core::undo_history::{BytePatch, UndoHistory, UndoStatus};
use kivi_defs::expr::invert_scaling;
use kivi_defs::model::MapDef;
use serde::{Deserialize, Serialize};
use tauri::State;

pub(crate) fn now_epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

const MAX_WINDOW_READ_BYTES: usize = 1024 * 1024;
const MAX_WAVEFORM_BUCKETS: usize = 4096;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CellWriteInput {
    pub row: usize,
    pub col: usize,
    pub value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ByteWriteInput {
    pub offset: usize,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PatchCommitOutcome {
    pub hashes: BinaryHashes,
    pub applied_cells: usize,
    pub changed_bytes: usize,
    pub undo: UndoStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UndoRedoOutcome {
    pub applied: bool,
    pub description: Option<String>,
    pub changed_offsets: Vec<usize>,
    pub hashes: BinaryHashes,
    pub undo: UndoStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RomSearchResult {
    pub offsets: Vec<usize>,
    pub truncated: bool,
    pub total_len: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScanProgressEvent {
    pub stage: String,
    pub percent: u8,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RomDocumentSummary {
    pub id: String,
    pub label: String,
    pub file_path: Option<String>,
    pub loaded: bool,
    pub size: usize,
    pub sha256: Option<String>,
    pub md5: Option<String>,
    pub base_address: Option<u32>,
    pub map_count: usize,
    pub undo: UndoStatus,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RomSessionSummary {
    pub active_doc_id: Option<String>,
    pub loaded: bool,
    pub size: usize,
    pub hashes: Option<BinaryHashes>,
    pub base_address: Option<u32>,
    pub map_count: usize,
    pub undo: UndoStatus,
    pub documents: Vec<RomDocumentSummary>,
}

pub struct RomDocumentEntry {
    pub id: String,
    pub label: String,
    pub file_path: Option<String>,
    pub memory: Option<MemoryMap>,
    pub original_memory: Option<MemoryMap>,
    pub maps: Vec<MapDef>,
    pub undo: UndoHistory,
}

#[derive(Default)]
pub struct RomDocumentStore {
    pub docs: Vec<RomDocumentEntry>,
    pub active_id: Option<String>,
    pub next_seq: usize,
}

impl RomDocumentStore {
    fn alloc_doc_id(&mut self) -> String {
        self.next_seq += 1;
        format!("rom-{}", self.next_seq)
    }

    pub fn ensure_active_metadata(&mut self, label: Option<&str>, file_path: Option<&str>) -> String {
        if let Some(active_id) = self.active_id.clone() {
            if let Some(entry) = self.docs.iter_mut().find(|d| d.id == active_id) {
                if let Some(lbl) = label {
                    entry.label = lbl.to_string();
                }
                if file_path.is_some() {
                    entry.file_path = file_path.map(ToString::to_string);
                }
                return active_id;
            }
        }
        let id = self.alloc_doc_id();
        self.docs.push(RomDocumentEntry {
            id: id.clone(),
            label: label.unwrap_or("Primary ROM").to_string(),
            file_path: file_path.map(ToString::to_string),
            memory: None,
            original_memory: None,
            maps: Vec::new(),
            undo: UndoHistory::default(),
        });
        self.active_id = Some(id.clone());
        id
    }

    fn park_active_into_slot(
        &mut self,
        memory: &mut Option<MemoryMap>,
        original_memory: &mut Option<MemoryMap>,
        maps: &mut Vec<MapDef>,
        undo: &mut UndoHistory,
    ) {
        if let Some(active_id) = self.active_id.as_ref() {
            if let Some(entry) = self.docs.iter_mut().find(|d| &d.id == active_id) {
                entry.memory = memory.take();
                entry.original_memory = original_memory.take();
                entry.maps = std::mem::take(maps);
                entry.undo = std::mem::take(undo);
            }
        }
    }
}

/// Coalesces contiguous differing regions between two equal-length buffers.
pub(crate) fn compute_diff_patches(before: &[u8], after: &[u8]) -> Vec<BytePatch> {
    let len = before.len().min(after.len());
    let mut patches = Vec::new();
    let mut i = 0;
    while i < len {
        if before[i] == after[i] {
            i += 1;
            continue;
        }
        let start = i;
        while i < len && before[i] != after[i] {
            i += 1;
        }
        patches.push(BytePatch {
            offset: start,
            old_bytes: before[start..i].to_vec(),
            new_bytes: after[start..i].to_vec(),
        });
    }
    patches
}

/// Atomically reads, computes a repaired image, diffs, writes back, and pushes an undo entry
/// under a single `(memory, undo)` lock acquisition.
pub(crate) fn mutate_active_rom_with_undo<T>(
    state: &AppState,
    label: &str,
    compute: impl FnOnce(&[u8]) -> Result<(Vec<u8>, T), String>,
) -> Result<(Vec<u8>, BinaryHashes, UndoStatus, T), String> {
    let mut mem_guard = super::lock_or_err(&state.memory, "memory")?;
    let mut undo_guard = super::lock_or_err(&state.undo, "undo")?;
    let mem = mem_guard
        .as_mut()
        .ok_or_else(|| "No binary loaded".to_string())?;
    let (after, meta) = compute(mem.as_slice())?;
    if after.len() != mem.len() {
        return Err(format!(
            "Repaired image length ({}) does not match active ROM length ({})",
            after.len(),
            mem.len()
        ));
    }
    let patches = compute_diff_patches(mem.as_slice(), &after);
    let hashes = if !patches.is_empty() {
        for patch in &patches {
            mem.write_slice_deferred_hash(patch.offset, &patch.new_bytes)
                .map_err(|e| e.to_string())?;
        }
        undo_guard.push(label.to_string(), now_epoch_ms(), patches);
        mem.recompute_hashes().clone()
    } else {
        mem.hashes().clone()
    };
    let undo = undo_guard.status();
    Ok((after, hashes, undo, meta))
}

/// Applies `after` only if the active ROM document ID and pre-flash SHA-256 still match.
/// Returns `Ok(None)` when the workspace switched documents or changed while hardware was flashing.
pub(crate) fn apply_repaired_slice_if_unchanged(
    state: &AppState,
    after: &[u8],
    expected_doc_id: Option<&str>,
    expected_sha256: &str,
    label: &str,
) -> Result<Option<BinaryHashes>, String> {
    let docs_guard = super::lock_or_err(&state.documents, "documents")?;
    let mut mem_guard = super::lock_or_err(&state.memory, "memory")?;
    let mut undo_guard = super::lock_or_err(&state.undo, "undo")?;
    if docs_guard.active_id.as_deref() != expected_doc_id {
        return Ok(None);
    }
    let Some(mem) = mem_guard.as_mut() else {
        return Ok(None);
    };
    if mem.hashes().sha256 != expected_sha256 || after.len() != mem.len() {
        return Ok(None);
    }
    let patches = compute_diff_patches(mem.as_slice(), after);
    if !patches.is_empty() {
        for patch in &patches {
            mem.write_slice_deferred_hash(patch.offset, &patch.new_bytes)
                .map_err(|e| e.to_string())?;
        }
        undo_guard.push(label.to_string(), now_epoch_ms(), patches);
        return Ok(Some(mem.recompute_hashes().clone()));
    }
    Ok(Some(mem.hashes().clone()))
}

pub fn read_rom_window_inner(
    offset: usize,
    length: usize,
    state: &AppState,
) -> Result<Vec<u8>, String> {
    let guard = super::lock_or_err(&state.memory, "memory")?;
    let mem = guard.as_ref().ok_or_else(|| "No binary loaded".to_string())?;
    let capped = length.min(MAX_WINDOW_READ_BYTES);
    Ok(mem.get_window(offset, capped).to_vec())
}

pub fn build_waveform_buckets_bytes(
    start_offset: Option<usize>,
    end_offset: Option<usize>,
    bucket_count: usize,
    state: &AppState,
) -> Result<Vec<u8>, String> {
    let guard = super::lock_or_err(&state.memory, "memory")?;
    let mem = guard.as_ref().ok_or_else(|| "No binary loaded".to_string())?;
    let data = mem.as_slice();
    let total_len = data.len();
    let start = start_offset.unwrap_or(0).min(total_len);
    let end = end_offset.unwrap_or(total_len).clamp(start, total_len);
    let view = &data[start..end];
    let target_bins = bucket_count.clamp(1, MAX_WAVEFORM_BUCKETS);

    let actual_bins = if view.is_empty() {
        0
    } else {
        target_bins.min(view.len())
    };
    let mut out = Vec::with_capacity(16 + actual_bins * 3);
    out.extend_from_slice(&(total_len as u32).to_le_bytes());
    out.extend_from_slice(&(start as u32).to_le_bytes());
    out.extend_from_slice(&(end as u32).to_le_bytes());
    out.extend_from_slice(&(actual_bins as u32).to_le_bytes());

    if actual_bins > 0 {
        let span = view.len();
        for b in 0..actual_bins {
            let s = (b * span) / actual_bins;
            let e = (((b + 1) * span) / actual_bins).max(s + 1).min(span);
            let mut min_v = u8::MAX;
            let mut max_v = 0u8;
            let mut sum: u64 = 0;
            for &byte in &view[s..e] {
                if byte < min_v {
                    min_v = byte;
                }
                if byte > max_v {
                    max_v = byte;
                }
                sum += byte as u64;
            }
            let avg = (sum / ((e - s) as u64)) as u8;
            out.push(min_v);
            out.push(max_v);
            out.push(avg);
        }
    }
    Ok(out)
}

pub fn search_rom_pattern_inner(
    pattern: &[u8],
    start_offset: usize,
    max_hits: usize,
    state: &AppState,
) -> Result<RomSearchResult, String> {
    if pattern.is_empty() {
        return Err("Search pattern cannot be empty".to_string());
    }
    if pattern.len() > 256 {
        return Err("Search pattern is limited to 256 bytes".to_string());
    }
    let guard = super::lock_or_err(&state.memory, "memory")?;
    let mem = guard.as_ref().ok_or_else(|| "No binary loaded".to_string())?;
    let capped = max_hits.clamp(1, 2000);
    let mut offsets = mem.search_pattern(pattern, start_offset, capped + 1);
    let truncated = offsets.len() > capped;
    if truncated {
        offsets.truncate(capped);
    }
    Ok(RomSearchResult {
        offsets,
        truncated,
        total_len: mem.len(),
    })
}

pub fn patch_map_cells_inner(
    map_def: &MapDef,
    edits: &[CellWriteInput],
    label: Option<&str>,
    state: &AppState,
) -> Result<PatchCommitOutcome, String> {
    let mut mem_guard = super::lock_or_err(&state.memory, "memory")?;
    let mut undo_guard = super::lock_or_err(&state.undo, "undo")?;
    let mem = mem_guard
        .as_mut()
        .ok_or_else(|| "No binary loaded".to_string())?;

    if edits.is_empty() {
        return Ok(PatchCommitOutcome {
            hashes: mem.hashes().clone(),
            applied_cells: 0,
            changed_bytes: 0,
            undo: undo_guard.status(),
        });
    }

    let elem_bytes = map_def.dtype.byte_size();
    let rom_len = mem.len();
    let mut planned: Vec<(usize, f64)> = Vec::with_capacity(edits.len());

    for edit in edits {
        if edit.row >= map_def.rows || edit.col >= map_def.cols {
            return Err(format!(
                "Cell ({},{}) is outside map bounds ({}x{})",
                edit.row, edit.col, map_def.rows, map_def.cols
            ));
        }
        let (grid_row, grid_col) = if map_def.column_major {
            (edit.col, edit.row)
        } else {
            (edit.row, edit.col)
        };
        let offset = cell_offset(map_def, grid_row, grid_col, elem_bytes);
        if offset < 0 {
            return Err(format!(
                "Map cell ({},{}) resolves to negative offset",
                edit.row, edit.col
            ));
        }
        let off = offset as usize;
        if off.saturating_add(elem_bytes) > rom_len {
            return Err(format!(
                "Cell ({},{}) at 0x{:X} exceeds ROM length 0x{:X}",
                edit.row, edit.col, off, rom_len
            ));
        }
        let raw = invert_scaling(&map_def.scaling.equation, edit.value).map_err(|e| e.to_string())?;
        planned.push((off, raw));
    }

    let mut patches = Vec::new();
    let mut changed_bytes = 0usize;
    for &(off, raw) in &planned {
        let (written_off, old_bytes, new_bytes) = mem
            .write_value_deferred_hash(off, raw, map_def.dtype, map_def.endian)
            .map_err(|e| e.to_string())?;
        if old_bytes != new_bytes {
            changed_bytes += new_bytes.len();
            patches.push(BytePatch {
                offset: written_off,
                old_bytes,
                new_bytes,
            });
        }
    }

    let hashes = if patches.is_empty() {
        mem.hashes().clone()
    } else {
        let tx_label = label
            .filter(|s| !s.trim().is_empty())
            .map(ToString::to_string)
            .unwrap_or_else(|| format!("Edit {} ({} cell(s))", map_def.name, edits.len()));
        undo_guard.push(tx_label, now_epoch_ms(), patches);
        mem.recompute_hashes().clone()
    };

    Ok(PatchCommitOutcome {
        hashes,
        applied_cells: edits.len(),
        changed_bytes,
        undo: undo_guard.status(),
    })
}

pub fn patch_rom_bytes_inner(
    writes: &[ByteWriteInput],
    label: Option<&str>,
    state: &AppState,
) -> Result<PatchCommitOutcome, String> {
    let mut mem_guard = super::lock_or_err(&state.memory, "memory")?;
    let mut undo_guard = super::lock_or_err(&state.undo, "undo")?;
    let mem = mem_guard
        .as_mut()
        .ok_or_else(|| "No binary loaded".to_string())?;

    if writes.is_empty() {
        return Ok(PatchCommitOutcome {
            hashes: mem.hashes().clone(),
            applied_cells: 0,
            changed_bytes: 0,
            undo: undo_guard.status(),
        });
    }

    let rom_len = mem.len();
    for w in writes {
        if w.offset.saturating_add(w.bytes.len()) > rom_len {
            return Err(format!(
                "Byte write at 0x{:X}..0x{:X} exceeds ROM length 0x{:X}",
                w.offset,
                w.offset.saturating_add(w.bytes.len()),
                rom_len
            ));
        }
    }

    let mut patches = Vec::new();
    let mut changed_bytes = 0usize;
    for w in writes {
        if w.bytes.is_empty() {
            continue;
        }
        let (old_bytes, new_bytes) = mem
            .write_slice_deferred_hash(w.offset, &w.bytes)
            .map_err(|e| e.to_string())?;
        if old_bytes != new_bytes {
            changed_bytes += new_bytes.len();
            patches.push(BytePatch {
                offset: w.offset,
                old_bytes,
                new_bytes,
            });
        }
    }

    let hashes = if patches.is_empty() {
        mem.hashes().clone()
    } else {
        let tx_label = label
            .filter(|s| !s.trim().is_empty())
            .map(ToString::to_string)
            .unwrap_or_else(|| format!("Hex Edit ({} byte(s))", changed_bytes));
        undo_guard.push(tx_label, now_epoch_ms(), patches);
        mem.recompute_hashes().clone()
    };

    Ok(PatchCommitOutcome {
        hashes,
        applied_cells: writes.len(),
        changed_bytes,
        undo: undo_guard.status(),
    })
}

pub fn undo_rom_edit_inner(state: &AppState) -> Result<UndoRedoOutcome, String> {
    let mut mem_guard = super::lock_or_err(&state.memory, "memory")?;
    let mut undo_guard = super::lock_or_err(&state.undo, "undo")?;
    let mem = mem_guard
        .as_mut()
        .ok_or_else(|| "No binary loaded".to_string())?;

    if !undo_guard.status().can_undo {
        return Ok(UndoRedoOutcome {
            applied: false,
            description: None,
            changed_offsets: Vec::new(),
            hashes: mem.hashes().clone(),
            undo: undo_guard.status(),
        });
    }

    let outcome = undo_guard.undo(mem).map_err(|e| e.to_string())?;
    Ok(UndoRedoOutcome {
        applied: true,
        description: Some(outcome.applied_label),
        changed_offsets: outcome.changed_offsets,
        hashes: outcome.hashes,
        undo: outcome.undo,
    })
}

pub fn redo_rom_edit_inner(state: &AppState) -> Result<UndoRedoOutcome, String> {
    let mut mem_guard = super::lock_or_err(&state.memory, "memory")?;
    let mut undo_guard = super::lock_or_err(&state.undo, "undo")?;
    let mem = mem_guard
        .as_mut()
        .ok_or_else(|| "No binary loaded".to_string())?;

    if !undo_guard.status().can_redo {
        return Ok(UndoRedoOutcome {
            applied: false,
            description: None,
            changed_offsets: Vec::new(),
            hashes: mem.hashes().clone(),
            undo: undo_guard.status(),
        });
    }

    let outcome = undo_guard.redo(mem).map_err(|e| e.to_string())?;
    Ok(UndoRedoOutcome {
        applied: true,
        description: Some(outcome.applied_label),
        changed_offsets: outcome.changed_offsets,
        hashes: outcome.hashes,
        undo: outcome.undo,
    })
}

pub fn get_undo_status_inner(state: &AppState) -> Result<UndoStatus, String> {
    Ok(super::lock_or_err(&state.undo, "undo")?.status())
}

pub fn save_bin_file_inner(
    path: &str,
    fix_keihin: bool,
    state: &AppState,
) -> Result<BinaryHashes, String> {
    let (active_doc_id, pre_sha256, keihin_patch, mut out_bytes, pre_hashes) = {
        let docs_guard = super::lock_or_err(&state.documents, "documents")?;
        let mem_guard = super::lock_or_err(&state.memory, "memory")?;
        let mem = mem_guard
            .as_ref()
            .ok_or_else(|| "No binary loaded".to_string())?;
        let mut bytes = mem.as_slice().to_vec();
        let patch = if fix_keihin {
            let len = bytes.len();
            if len == 0 {
                return Err("Cannot fix Keihin checksum on empty ROM".to_string());
            }
            let target = compute_keihin_checksum_byte(&bytes).map_err(|e| e.to_string())?;
            let last_idx = len - 1;
            let old = bytes[last_idx];
            if old != target {
                bytes[last_idx] = target;
                Some((last_idx, vec![old], vec![target]))
            } else {
                None
            }
        } else {
            None
        };
        (
            docs_guard.active_id.clone(),
            mem.hashes().sha256.clone(),
            patch,
            bytes,
            mem.hashes().clone(),
        )
    };

    super::write_file_atomic(path, &out_bytes)?;
    let _ = &mut out_bytes;

    let mut docs_guard = super::lock_or_err(&state.documents, "documents")?;
    let mut mem_guard = super::lock_or_err(&state.memory, "memory")?;
    let mut undo_guard = super::lock_or_err(&state.undo, "undo")?;

    let saved_hashes = if let Some((last_idx, _old_bytes, ref new_bytes)) = keihin_patch {
        if docs_guard.active_id == active_doc_id {
            if let Some(mem) = mem_guard.as_mut() {
                if mem.hashes().sha256 == pre_sha256 {
                    let (old_actual, new_actual) = mem
                        .write_slice_deferred_hash(last_idx, new_bytes)
                        .map_err(|e| e.to_string())?;
                    if old_actual != new_actual {
                        undo_guard.push(
                            "Fix Keihin Checksum (Save)".to_string(),
                            now_epoch_ms(),
                            vec![BytePatch {
                                offset: last_idx,
                                old_bytes: old_actual,
                                new_bytes: new_actual,
                            }],
                        );
                        mem.recompute_hashes().clone()
                    } else {
                        mem.hashes().clone()
                    }
                } else {
                    kivi_core::hash::compute_hashes(&out_bytes)
                }
            } else {
                kivi_core::hash::compute_hashes(&out_bytes)
            }
        } else {
            kivi_core::hash::compute_hashes(&out_bytes)
        }
    } else {
        pre_hashes
    };

    if let Some(ref saved_id) = active_doc_id {
        if let Some(entry) = docs_guard.docs.iter_mut().find(|d| &d.id == saved_id) {
            entry.file_path = Some(path.to_string());
        }
    }
    Ok(saved_hashes)
}

fn summarize_docs_locked(
    docs_guard: &RomDocumentStore,
    mem_guard: &Option<MemoryMap>,
    maps_guard: &[MapDef],
    undo_guard: &UndoHistory,
) -> Vec<RomDocumentSummary> {
    docs_guard
        .docs
        .iter()
        .map(|entry| {
            let is_active = docs_guard.active_id.as_deref() == Some(entry.id.as_str());
            if is_active {
                let hashes = mem_guard.as_ref().map(|m| m.hashes());
                RomDocumentSummary {
                    id: entry.id.clone(),
                    label: entry.label.clone(),
                    file_path: entry.file_path.clone(),
                    loaded: mem_guard.is_some(),
                    size: mem_guard.as_ref().map_or(0, |m| m.len()),
                    sha256: hashes.map(|h| h.sha256.clone()),
                    md5: hashes.map(|h| h.md5.clone()),
                    base_address: mem_guard.as_ref().and_then(|m| m.base_address()),
                    map_count: maps_guard.len(),
                    undo: undo_guard.status(),
                    is_active: true,
                }
            } else {
                let hashes = entry.memory.as_ref().map(|m| m.hashes());
                RomDocumentSummary {
                    id: entry.id.clone(),
                    label: entry.label.clone(),
                    file_path: entry.file_path.clone(),
                    loaded: entry.memory.is_some(),
                    size: entry.memory.as_ref().map_or(0, |m| m.len()),
                    sha256: hashes.map(|h| h.sha256.clone()),
                    md5: hashes.map(|h| h.md5.clone()),
                    base_address: entry.memory.as_ref().and_then(|m| m.base_address()),
                    map_count: entry.maps.len(),
                    undo: entry.undo.status(),
                    is_active: false,
                }
            }
        })
        .collect()
}

fn build_session_summary_locked(
    docs_guard: &RomDocumentStore,
    mem_guard: &Option<MemoryMap>,
    maps_guard: &[MapDef],
    undo_guard: &UndoHistory,
) -> RomSessionSummary {
    let documents = summarize_docs_locked(docs_guard, mem_guard, maps_guard, undo_guard);
    RomSessionSummary {
        active_doc_id: docs_guard.active_id.clone(),
        loaded: mem_guard.is_some(),
        size: mem_guard.as_ref().map_or(0, |m| m.len()),
        hashes: mem_guard.as_ref().map(|m| m.hashes().clone()),
        base_address: mem_guard.as_ref().and_then(|m| m.base_address()),
        map_count: maps_guard.len(),
        undo: undo_guard.status(),
        documents,
    }
}

pub fn get_rom_session_summary_inner(state: &AppState) -> Result<RomSessionSummary, String> {
    let docs_guard = super::lock_or_err(&state.documents, "documents")?;
    let mem_guard = super::lock_or_err(&state.memory, "memory")?;
    let maps_guard = super::lock_or_err(&state.maps, "maps")?;
    let undo_guard = super::lock_or_err(&state.undo, "undo")?;
    Ok(build_session_summary_locked(
        &docs_guard,
        &mem_guard,
        &maps_guard,
        &undo_guard,
    ))
}

pub fn open_rom_document_bytes_inner(
    doc_id: Option<&str>,
    label: Option<&str>,
    file_path: Option<&str>,
    data: &[u8],
    state: &AppState,
) -> Result<RomSessionSummary, String> {
    let mem = ingest_binary(data).map_err(|e| e.to_string())?;
    let mut docs_guard = super::lock_or_err(&state.documents, "documents")?;
    let mut mem_guard = super::lock_or_err(&state.memory, "memory")?;
    let mut orig_guard = super::lock_or_err(&state.original_memory, "original_memory")?;
    let mut maps_guard = super::lock_or_err(&state.maps, "maps")?;
    let mut undo_guard = super::lock_or_err(&state.undo, "undo")?;

    docs_guard.park_active_into_slot(
        &mut mem_guard,
        &mut orig_guard,
        &mut maps_guard,
        &mut undo_guard,
    );

    let target_id = doc_id
        .filter(|s| !s.trim().is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| docs_guard.alloc_doc_id());
    let resolved_label = label
        .filter(|s| !s.trim().is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| format!("ROM {}", target_id));

    if let Some(pos) = docs_guard.docs.iter().position(|d| d.id == target_id) {
        let entry = &mut docs_guard.docs[pos];
        entry.label = resolved_label;
        entry.file_path = file_path.map(ToString::to_string);
        entry.memory = None;
        entry.original_memory = None;
        entry.maps.clear();
        entry.undo.clear();
        *orig_guard = Some(mem.clone());
        *mem_guard = Some(mem);
        maps_guard.clear();
        undo_guard.clear();
    } else {
        docs_guard.docs.push(RomDocumentEntry {
            id: target_id.clone(),
            label: resolved_label,
            file_path: file_path.map(ToString::to_string),
            memory: None,
            original_memory: None,
            maps: Vec::new(),
            undo: UndoHistory::default(),
        });
        *orig_guard = Some(mem.clone());
        *mem_guard = Some(mem);
        maps_guard.clear();
        undo_guard.clear();
    }
    docs_guard.active_id = Some(target_id);
    Ok(build_session_summary_locked(
        &docs_guard,
        &mem_guard,
        &maps_guard,
        &undo_guard,
    ))
}

pub fn switch_rom_document_inner(doc_id: &str, state: &AppState) -> Result<RomSessionSummary, String> {
    let mut docs_guard = super::lock_or_err(&state.documents, "documents")?;
    let mut mem_guard = super::lock_or_err(&state.memory, "memory")?;
    let mut orig_guard = super::lock_or_err(&state.original_memory, "original_memory")?;
    let mut maps_guard = super::lock_or_err(&state.maps, "maps")?;
    let mut undo_guard = super::lock_or_err(&state.undo, "undo")?;

    if docs_guard.active_id.as_deref() != Some(doc_id) {
        let target_idx = docs_guard
            .docs
            .iter()
            .position(|d| d.id == doc_id)
            .ok_or_else(|| format!("ROM document not found: {doc_id}"))?;
        docs_guard.park_active_into_slot(
            &mut mem_guard,
            &mut orig_guard,
            &mut maps_guard,
            &mut undo_guard,
        );
        let target = &mut docs_guard.docs[target_idx];
        *mem_guard = target.memory.take();
        *orig_guard = target.original_memory.take();
        *maps_guard = std::mem::take(&mut target.maps);
        *undo_guard = std::mem::take(&mut target.undo);
        docs_guard.active_id = Some(doc_id.to_string());
    }
    Ok(build_session_summary_locked(
        &docs_guard,
        &mem_guard,
        &maps_guard,
        &undo_guard,
    ))
}

pub fn close_rom_document_inner(doc_id: &str, state: &AppState) -> Result<RomSessionSummary, String> {
    let mut docs_guard = super::lock_or_err(&state.documents, "documents")?;
    let mut mem_guard = super::lock_or_err(&state.memory, "memory")?;
    let mut orig_guard = super::lock_or_err(&state.original_memory, "original_memory")?;
    let mut maps_guard = super::lock_or_err(&state.maps, "maps")?;
    let mut undo_guard = super::lock_or_err(&state.undo, "undo")?;

    let pos = docs_guard
        .docs
        .iter()
        .position(|d| d.id == doc_id)
        .ok_or_else(|| format!("ROM document not found: {doc_id}"))?;
    let was_active = docs_guard.active_id.as_deref() == Some(doc_id);
    docs_guard.docs.remove(pos);

    if was_active {
        if let Some(next_idx) = if docs_guard.docs.is_empty() {
            None
        } else {
            Some(pos.min(docs_guard.docs.len() - 1))
        } {
            let next_doc = &mut docs_guard.docs[next_idx];
            let next_id = next_doc.id.clone();
            *mem_guard = next_doc.memory.take();
            *orig_guard = next_doc.original_memory.take();
            *maps_guard = std::mem::take(&mut next_doc.maps);
            *undo_guard = std::mem::take(&mut next_doc.undo);
            docs_guard.active_id = Some(next_id);
        } else {
            *mem_guard = None;
            *orig_guard = None;
            maps_guard.clear();
            undo_guard.clear();
            docs_guard.active_id = None;
        }
    }
    Ok(build_session_summary_locked(
        &docs_guard,
        &mem_guard,
        &maps_guard,
        &undo_guard,
    ))
}

pub fn rename_rom_document_inner(
    doc_id: &str,
    label: &str,
    state: &AppState,
) -> Result<RomSessionSummary, String> {
    let trimmed = label.trim();
    if trimmed.is_empty() {
        return Err("Document label cannot be empty".to_string());
    }
    let mut docs_guard = super::lock_or_err(&state.documents, "documents")?;
    let mem_guard = super::lock_or_err(&state.memory, "memory")?;
    let maps_guard = super::lock_or_err(&state.maps, "maps")?;
    let undo_guard = super::lock_or_err(&state.undo, "undo")?;
    let entry = docs_guard
        .docs
        .iter_mut()
        .find(|d| d.id == doc_id)
        .ok_or_else(|| format!("ROM document not found: {doc_id}"))?;
    entry.label = trimmed.to_string();
    Ok(build_session_summary_locked(
        &docs_guard,
        &mem_guard,
        &maps_guard,
        &undo_guard,
    ))
}

// ---------------------------------------------------------------------------
// Tauri IPC commands (raw binary transfer, windowed reads, delta patches)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn load_bin_raw(
    request: tauri::ipc::Request<'_>,
    state: State<'_, AppState>,
) -> Result<BinaryHashes, String> {
    let label = request
        .headers()
        .get("x-doc-label")
        .and_then(|v| v.to_str().ok())
        .map(ToString::to_string);
    let file_path = request
        .headers()
        .get("x-doc-path")
        .and_then(|v| v.to_str().ok())
        .map(ToString::to_string);
    let bytes = match request.body() {
        tauri::ipc::InvokeBody::Raw(data) => data.as_slice(),
        tauri::ipc::InvokeBody::Json(_) => {
            return Err("load_bin_raw expects raw binary body".to_string())
        }
    };
    load_bin_data_with_meta(bytes, label.as_deref(), file_path.as_deref(), &state)
}

#[tauri::command]
pub fn open_rom_document_raw(
    request: tauri::ipc::Request<'_>,
    state: State<'_, AppState>,
) -> Result<RomSessionSummary, String> {
    let doc_id = request
        .headers()
        .get("x-doc-id")
        .and_then(|v| v.to_str().ok())
        .map(ToString::to_string);
    let label = request
        .headers()
        .get("x-doc-label")
        .and_then(|v| v.to_str().ok())
        .map(ToString::to_string);
    let file_path = request
        .headers()
        .get("x-doc-path")
        .and_then(|v| v.to_str().ok())
        .map(ToString::to_string);
    let bytes = match request.body() {
        tauri::ipc::InvokeBody::Raw(data) => data.as_slice(),
        tauri::ipc::InvokeBody::Json(_) => {
            return Err("open_rom_document_raw expects raw binary body".to_string())
        }
    };
    open_rom_document_bytes_inner(
        doc_id.as_deref(),
        label.as_deref(),
        file_path.as_deref(),
        bytes,
        &state,
    )
}

#[tauri::command]
pub fn open_rom_document_file(
    doc_id: Option<String>,
    label: Option<String>,
    path: String,
    state: State<'_, AppState>,
) -> Result<RomSessionSummary, String> {
    let data = super::read_bounded_file_bytes(&path, super::MAX_FILE_READ_BYTES)?;
    let inferred_label = label.or_else(|| {
        std::path::Path::new(&path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
    });
    open_rom_document_bytes_inner(
        doc_id.as_deref(),
        inferred_label.as_deref(),
        Some(&path),
        &data,
        &state,
    )
}

#[tauri::command]
pub fn switch_rom_document(
    doc_id: String,
    state: State<'_, AppState>,
) -> Result<RomSessionSummary, String> {
    switch_rom_document_inner(&doc_id, &state)
}

#[tauri::command]
pub fn close_rom_document(
    doc_id: String,
    state: State<'_, AppState>,
) -> Result<RomSessionSummary, String> {
    close_rom_document_inner(&doc_id, &state)
}

#[tauri::command]
pub fn rename_rom_document(
    doc_id: String,
    label: String,
    state: State<'_, AppState>,
) -> Result<RomSessionSummary, String> {
    rename_rom_document_inner(&doc_id, &label, &state)
}

#[tauri::command]
pub fn get_rom_session_summary(state: State<'_, AppState>) -> Result<RomSessionSummary, String> {
    get_rom_session_summary_inner(&state)
}

pub fn sync_workspace_maps_inner(maps: Vec<MapDef>, state: &AppState) -> Result<usize, String> {
    let mut maps_guard = super::lock_or_err(&state.maps, "maps")?;
    let count = maps.len();
    *maps_guard = maps;
    Ok(count)
}

pub fn get_workspace_maps_inner(state: &AppState) -> Result<Vec<MapDef>, String> {
    let maps_guard = super::lock_or_err(&state.maps, "maps")?;
    Ok(maps_guard.clone())
}

#[tauri::command]
pub fn sync_workspace_maps(maps: Vec<MapDef>, state: State<'_, AppState>) -> Result<usize, String> {
    sync_workspace_maps_inner(maps, &state)
}

#[tauri::command]
pub fn get_workspace_maps(state: State<'_, AppState>) -> Result<Vec<MapDef>, String> {
    get_workspace_maps_inner(&state)
}

#[tauri::command]
pub fn save_bin_raw(state: State<'_, AppState>) -> Result<tauri::ipc::Response, String> {
    let bytes = super::rom::save_bin_data_inner(&state)?;
    Ok(tauri::ipc::Response::new(bytes))
}

#[tauri::command]
pub fn save_bin_file(
    path: String,
    fix_keihin: Option<bool>,
    state: State<'_, AppState>,
) -> Result<BinaryHashes, String> {
    save_bin_file_inner(&path, fix_keihin.unwrap_or(false), &state)
}

#[tauri::command]
pub fn read_rom_window(
    offset: usize,
    length: usize,
    state: State<'_, AppState>,
) -> Result<tauri::ipc::Response, String> {
    let bytes = read_rom_window_inner(offset, length, &state)?;
    Ok(tauri::ipc::Response::new(bytes))
}

#[tauri::command]
pub fn get_waveform_buckets(
    start_offset: Option<usize>,
    end_offset: Option<usize>,
    bucket_count: Option<usize>,
    state: State<'_, AppState>,
) -> Result<tauri::ipc::Response, String> {
    let bytes = build_waveform_buckets_bytes(
        start_offset,
        end_offset,
        bucket_count.unwrap_or(1024),
        &state,
    )?;
    Ok(tauri::ipc::Response::new(bytes))
}

#[tauri::command]
pub fn search_rom_pattern(
    pattern: Vec<u8>,
    start_offset: Option<usize>,
    max_hits: Option<usize>,
    state: State<'_, AppState>,
) -> Result<RomSearchResult, String> {
    search_rom_pattern_inner(
        &pattern,
        start_offset.unwrap_or(0),
        max_hits.unwrap_or(256),
        &state,
    )
}

#[tauri::command]
pub fn patch_map_cells(
    map_def: MapDef,
    edits: Vec<CellWriteInput>,
    label: Option<String>,
    state: State<'_, AppState>,
) -> Result<PatchCommitOutcome, String> {
    patch_map_cells_inner(&map_def, &edits, label.as_deref(), &state)
}

#[tauri::command]
pub fn patch_rom_bytes(
    writes: Vec<ByteWriteInput>,
    label: Option<String>,
    state: State<'_, AppState>,
) -> Result<PatchCommitOutcome, String> {
    patch_rom_bytes_inner(&writes, label.as_deref(), &state)
}

#[tauri::command]
pub fn undo_rom_edit(state: State<'_, AppState>) -> Result<UndoRedoOutcome, String> {
    undo_rom_edit_inner(&state)
}

#[tauri::command]
pub fn redo_rom_edit(state: State<'_, AppState>) -> Result<UndoRedoOutcome, String> {
    redo_rom_edit_inner(&state)
}

#[tauri::command]
pub fn get_undo_status(state: State<'_, AppState>) -> Result<UndoStatus, String> {
    get_undo_status_inner(&state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kivi_core::types::{DType, Endian};
    use kivi_defs::model::Scaling;

    fn sample_map() -> MapDef {
        MapDef {
            id: "fuel".to_string(),
            name: "Fuel Main".to_string(),
            category: "Fuel".to_string(),
            address: 0x10,
            rows: 2,
            cols: 2,
            dtype: DType::U16,
            endian: Endian::Little,
            major_stride_bits: 0,
            minor_stride_bits: 0,
            column_major: false,
            scaling: Scaling {
                equation: "X * 0.5".to_string(),
                unit: "ms".to_string(),
                decimals: 1,
            },
            x_axis: None,
            y_axis: None,
        }
    }

    #[test]
    fn patch_map_cells_and_undo_redo_restore_hashes() {
        let state = AppState::default();
        let rom = vec![0x00u8; 256];
        let initial = super::super::rom::load_bin_data_inner(&rom, &state).unwrap();
        let map = sample_map();

        let outcome = patch_map_cells_inner(
            &map,
            &[CellWriteInput {
                row: 1,
                col: 1,
                value: 20.0,
            }],
            Some("Bump cell (1,1)"),
            &state,
        )
        .unwrap();
        assert_eq!(outcome.applied_cells, 1);
        assert_eq!(outcome.changed_bytes, 2);
        assert_ne!(outcome.hashes.sha256, initial.sha256);
        assert!(outcome.undo.can_undo);
        assert!(!outcome.undo.can_redo);

        let window = read_rom_window_inner(0x16, 2, &state).unwrap();
        assert_eq!(window, vec![40, 0]);

        let undone = undo_rom_edit_inner(&state).unwrap();
        assert!(undone.applied);
        assert_eq!(undone.hashes.sha256, initial.sha256);
        assert_eq!(read_rom_window_inner(0x16, 2, &state).unwrap(), vec![0, 0]);

        let redone = redo_rom_edit_inner(&state).unwrap();
        assert!(redone.applied);
        assert_eq!(redone.hashes.sha256, outcome.hashes.sha256);
    }

    #[test]
    fn patch_rom_bytes_validates_all_writes_before_mutating() {
        let state = AppState::default();
        let initial = super::super::rom::load_bin_data_inner(&[0xAAu8; 64], &state).unwrap();
        let err = patch_rom_bytes_inner(
            &[
                ByteWriteInput {
                    offset: 4,
                    bytes: vec![0x11, 0x22],
                },
                ByteWriteInput {
                    offset: 60,
                    bytes: vec![0; 10],
                },
            ],
            None,
            &state,
        )
        .unwrap_err();
        assert!(err.contains("exceeds ROM length"));
        assert_eq!(read_rom_window_inner(4, 2, &state).unwrap(), vec![0xAA, 0xAA]);
        assert_eq!(
            state.memory.lock().unwrap().as_ref().unwrap().hashes().sha256,
            initial.sha256
        );
    }

    #[test]
    fn multi_rom_documents_switch_preserve_independent_rom_and_undo() {
        let state = AppState::default();
        let s1 = open_rom_document_bytes_inner(
            Some("doc-a"),
            Some("Stock.bin"),
            None,
            &[0x11u8; 128],
            &state,
        )
        .unwrap();
        assert_eq!(s1.active_doc_id.as_deref(), Some("doc-a"));

        patch_rom_bytes_inner(
            &[ByteWriteInput {
                offset: 0,
                bytes: vec![0x99],
            }],
            Some("Mark doc-a"),
            &state,
        )
        .unwrap();

        let s2 = open_rom_document_bytes_inner(
            Some("doc-b"),
            Some("Tuned.bin"),
            None,
            &[0x22u8; 256],
            &state,
        )
        .unwrap();
        assert_eq!(s2.active_doc_id.as_deref(), Some("doc-b"));
        assert_eq!(s2.documents.len(), 2);
        assert_eq!(read_rom_window_inner(0, 1, &state).unwrap(), vec![0x22]);
        assert!(!get_undo_status_inner(&state).unwrap().can_undo);

        let back = switch_rom_document_inner("doc-a", &state).unwrap();
        assert_eq!(back.size, 128);
        assert_eq!(read_rom_window_inner(0, 1, &state).unwrap(), vec![0x99]);
        assert!(get_undo_status_inner(&state).unwrap().can_undo);

        let after_close = close_rom_document_inner("doc-a", &state).unwrap();
        assert_eq!(after_close.active_doc_id.as_deref(), Some("doc-b"));
        assert_eq!(after_close.documents.len(), 1);
        assert_eq!(after_close.size, 256);
    }

    #[test]
    fn waveform_buckets_and_search_pattern_work() {
        let state = AppState::default();
        let mut rom = vec![0x10u8; 512];
        rom[100] = 0xDE;
        rom[101] = 0xAD;
        rom[200] = 0xDE;
        rom[201] = 0xAD;
        rom[300] = 0xDE;
        rom[301] = 0xAD;
        super::super::rom::load_bin_data_inner(&rom, &state).unwrap();

        let hits = search_rom_pattern_inner(&[0xDE, 0xAD], 0, 2, &state).unwrap();
        assert_eq!(hits.offsets, vec![100, 200]);
        assert!(hits.truncated);

        let packed = build_waveform_buckets_bytes(None, None, 64, &state).unwrap();
        let total = u32::from_le_bytes(packed[0..4].try_into().unwrap());
        let bins = u32::from_le_bytes(packed[12..16].try_into().unwrap());
        assert_eq!(total, 512);
        assert_eq!(bins, 64);
        assert_eq!(packed.len(), 16 + 64 * 3);
    }

    #[test]
    fn sync_workspace_maps_updates_summary_and_survives_document_switch() {
        let state = AppState::default();
        let s1 = open_rom_document_bytes_inner(
            Some("doc-1"),
            Some("Stock.bin"),
            None,
            &[0x11u8; 128],
            &state,
        )
        .unwrap();
        assert_eq!(s1.map_count, 0);

        let count = sync_workspace_maps_inner(vec![sample_map()], &state).unwrap();
        assert_eq!(count, 1);
        assert_eq!(get_workspace_maps_inner(&state).unwrap().len(), 1);
        assert_eq!(get_rom_session_summary_inner(&state).unwrap().map_count, 1);

        let s2 = open_rom_document_bytes_inner(
            Some("doc-2"),
            Some("Alt.bin"),
            None,
            &[0x22u8; 128],
            &state,
        )
        .unwrap();
        assert_eq!(s2.map_count, 0);
        assert_eq!(s2.documents.iter().find(|d| d.id == "doc-1").unwrap().map_count, 1);

        let back = switch_rom_document_inner("doc-1", &state).unwrap();
        assert_eq!(back.map_count, 1);
        assert_eq!(get_workspace_maps_inner(&state).unwrap()[0].id, "fuel");
    }
}
