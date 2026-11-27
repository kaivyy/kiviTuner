import { invoke } from '@tauri-apps/api/core';
import type { BinaryHashes, MapDef } from '../../types/kivi.ts';
import { computeMd5Hex, computeSha1Hex, computeSha256Hex } from '../crypto/hashes.ts';
import { isDesktopApp } from './romBridge.ts';

export interface CellWriteInput {
  row: number;
  col: number;
  value: number;
}

export interface ByteWriteInput {
  offset: number;
  bytes: number[];
}

export interface UndoStatus {
  can_undo: boolean;
  can_redo: boolean;
  undo_count: number;
  redo_count: number;
  last_label: string | null;
  revision: number;
}

export interface PatchCommitOutcome {
  hashes: BinaryHashes;
  applied_cells: number;
  changed_bytes: number;
  undo: UndoStatus;
}

export interface UndoRedoOutcome {
  applied: boolean;
  description: string | null;
  changed_offsets: number[];
  hashes: BinaryHashes;
  undo: UndoStatus;
}

export interface RomSearchResult {
  offsets: number[];
  truncated: boolean;
  total_len: number;
}

export interface ScanProgressEvent {
  stage: string;
  percent: number;
  message: string;
}

export interface RomDocumentSummary {
  id: string;
  label: string;
  file_path: string | null;
  loaded: boolean;
  size: number;
  sha256: string | null;
  md5: string | null;
  base_address: number | null;
  map_count: number;
  undo: UndoStatus;
  is_active: boolean;
}

export interface RomSessionSummary {
  active_doc_id: string | null;
  loaded: boolean;
  size: number;
  hashes: BinaryHashes | null;
  base_address: number | null;
  map_count: number;
  undo: UndoStatus;
  documents: RomDocumentSummary[];
}

export interface WaveformBucket {
  min: number;
  max: number;
  avg: number;
}

export interface WaveformBuckets {
  totalLen: number;
  startOffset: number;
  endOffset: number;
  buckets: WaveformBucket[];
}

export function emptyUndoStatus(): UndoStatus {
  return {
    can_undo: false,
    can_redo: false,
    undo_count: 0,
    redo_count: 0,
    last_label: null,
    revision: 0,
  };
}

function sanitizeAsciiHeader(value: string): string {
  return value.replace(/[^\x20-\x7e]+/g, '_').trim() || 'document.bin';
}

function buildDocHeaders(opts?: {
  docId?: string;
  label?: string;
  path?: string;
}): Record<string, string> {
  const headers: Record<string, string> = {};
  if (opts?.docId) headers['x-doc-id'] = sanitizeAsciiHeader(opts.docId);
  if (opts?.label) headers['x-doc-label'] = sanitizeAsciiHeader(opts.label);
  if (opts?.path) headers['x-doc-path'] = sanitizeAsciiHeader(opts.path);
  return headers;
}

export function computeHashesLocal(data: Uint8Array): BinaryHashes {
  return {
    md5: computeMd5Hex(data),
    sha1: computeSha1Hex(data),
    sha256: computeSha256Hex(data),
  };
}

export function parseWaveformBucketsBinary(buffer: ArrayBuffer): WaveformBuckets {
  if (buffer.byteLength < 16) {
    return { totalLen: 0, startOffset: 0, endOffset: 0, buckets: [] };
  }
  const view = new DataView(buffer);
  const totalLen = view.getUint32(0, true);
  const startOffset = view.getUint32(4, true);
  const endOffset = view.getUint32(8, true);
  const declaredBins = view.getUint32(12, true);
  const payload = new Uint8Array(buffer, 16);
  const availableBins = Math.min(declaredBins, Math.floor(payload.length / 3));
  const buckets: WaveformBucket[] = new Array(availableBins);
  for (let i = 0; i < availableBins; i++) {
    const base = i * 3;
    buckets[i] = {
      min: payload[base],
      max: payload[base + 1],
      avg: payload[base + 2],
    };
  }
  return { totalLen, startOffset, endOffset, buckets };
}

export function computeWaveformBucketsFromSlice(
  rom: Uint8Array,
  startOffset?: number,
  endOffset?: number,
  bucketCount = 1024
): WaveformBuckets {
  const totalLen = rom.length;
  const start = Math.max(0, Math.min(startOffset ?? 0, totalLen));
  const end = Math.max(start, Math.min(endOffset ?? totalLen, totalLen));
  const span = end - start;
  const clampedTarget = Math.max(1, Math.min(bucketCount, 4096));
  const actualBins = span === 0 ? 0 : Math.min(clampedTarget, span);
  const buckets: WaveformBucket[] = new Array(actualBins);

  for (let b = 0; b < actualBins; b++) {
    const s = start + Math.floor((b * span) / actualBins);
    const e = start + Math.min(span, Math.max(Math.floor(((b + 1) * span) / actualBins), (s - start) + 1));
    let minV = 255;
    let maxV = 0;
    let sum = 0;
    for (let i = s; i < e; i++) {
      const v = rom[i];
      if (v < minV) minV = v;
      if (v > maxV) maxV = v;
      sum += v;
    }
    buckets[b] = {
      min: minV,
      max: maxV,
      avg: Math.floor(sum / Math.max(1, e - s)),
    };
  }
  return { totalLen, startOffset: start, endOffset: end, buckets };
}

export function searchPatternInSlice(
  rom: Uint8Array,
  pattern: Uint8Array,
  startOffset = 0,
  maxHits = 256
): RomSearchResult {
  if (pattern.length === 0 || pattern.length > 256 || rom.length < pattern.length) {
    return { offsets: [], truncated: false, total_len: rom.length };
  }
  const capped = Math.max(1, Math.min(maxHits, 2000));
  const offsets: number[] = [];
  const limit = rom.length - pattern.length;
  const first = pattern[0];
  let truncated = false;

  for (let i = Math.max(0, startOffset); i <= limit; i++) {
    if (rom[i] !== first) continue;
    let matched = true;
    for (let j = 1; j < pattern.length; j++) {
      if (rom[i + j] !== pattern[j]) {
        matched = false;
        break;
      }
    }
    if (matched) {
      if (offsets.length >= capped) {
        truncated = true;
        break;
      }
      offsets.push(i);
    }
  }
  return { offsets, truncated, total_len: rom.length };
}

export async function bridgeLoadBinRaw(
  data: Uint8Array,
  label?: string,
  path?: string
): Promise<BinaryHashes> {
  if (isDesktopApp()) {
    try {
      return await invoke<BinaryHashes>('load_bin_raw', data, {
        headers: buildDocHeaders({ label, path }),
      });
    } catch (err) {
      console.warn('Tauri load_bin_raw IPC error, falling back:', err);
    }
  }
  return computeHashesLocal(data);
}

export async function bridgeOpenRomDocumentRaw(
  data: Uint8Array,
  opts?: { docId?: string; label?: string; path?: string }
): Promise<RomSessionSummary | null> {
  if (!isDesktopApp()) return null;
  return await invoke<RomSessionSummary>('open_rom_document_raw', data, {
    headers: buildDocHeaders(opts),
  });
}

export async function bridgeOpenRomDocumentFile(
  path: string,
  opts?: { docId?: string; label?: string }
): Promise<RomSessionSummary | null> {
  if (!isDesktopApp()) return null;
  return await invoke<RomSessionSummary>('open_rom_document_file', {
    docId: opts?.docId ?? null,
    label: opts?.label ?? null,
    path,
  });
}

export async function bridgeSwitchRomDocument(docId: string): Promise<RomSessionSummary | null> {
  if (!isDesktopApp()) return null;
  return await invoke<RomSessionSummary>('switch_rom_document', { docId });
}

export async function bridgeCloseRomDocument(docId: string): Promise<RomSessionSummary | null> {
  if (!isDesktopApp()) return null;
  return await invoke<RomSessionSummary>('close_rom_document', { docId });
}

export async function bridgeRenameRomDocument(
  docId: string,
  label: string
): Promise<RomSessionSummary | null> {
  if (!isDesktopApp()) return null;
  return await invoke<RomSessionSummary>('rename_rom_document', { docId, label });
}

export async function bridgeGetRomSessionSummary(): Promise<RomSessionSummary | null> {
  if (!isDesktopApp()) return null;
  return await invoke<RomSessionSummary>('get_rom_session_summary');
}

export async function bridgeSaveBinRaw(fallbackRom?: Uint8Array | null): Promise<Uint8Array | null> {
  if (isDesktopApp()) {
    try {
      const buf = await invoke<ArrayBuffer>('save_bin_raw');
      return new Uint8Array(buf);
    } catch (err) {
      console.warn('Tauri save_bin_raw IPC error, falling back:', err);
    }
  }
  return fallbackRom ?? null;
}

export async function bridgeSaveBinFile(
  path: string,
  fixKeihin = false
): Promise<BinaryHashes | null> {
  if (!isDesktopApp()) return null;
  return await invoke<BinaryHashes>('save_bin_file', { path, fixKeihin });
}

export async function bridgeReadRomWindow(
  offset: number,
  length: number,
  fallbackRom?: Uint8Array | null
): Promise<Uint8Array> {
  const safeOffset = Math.max(0, Math.floor(offset));
  const safeLength = Math.max(0, Math.floor(length));
  if (safeLength === 0) return new Uint8Array(0);

  if (isDesktopApp()) {
    try {
      const buf = await invoke<ArrayBuffer>('read_rom_window', {
        offset: safeOffset,
        length: safeLength,
      });
      return new Uint8Array(buf);
    } catch (err) {
      console.warn('Tauri read_rom_window IPC error, falling back:', err);
    }
  }

  if (!fallbackRom || safeOffset >= fallbackRom.length) {
    return new Uint8Array(0);
  }
  const end = Math.min(fallbackRom.length, safeOffset + safeLength);
  return fallbackRom.slice(safeOffset, end);
}

export async function bridgeGetWaveformBuckets(
  startOffset?: number,
  endOffset?: number,
  bucketCount = 1024,
  fallbackRom?: Uint8Array | null
): Promise<WaveformBuckets | null> {
  if (isDesktopApp()) {
    try {
      const buf = await invoke<ArrayBuffer>('get_waveform_buckets', {
        startOffset: startOffset ?? null,
        endOffset: endOffset ?? null,
        bucketCount,
      });
      return parseWaveformBucketsBinary(buf);
    } catch (err) {
      console.warn('Tauri get_waveform_buckets IPC error, falling back:', err);
    }
  }

  if (!fallbackRom) return null;
  return computeWaveformBucketsFromSlice(fallbackRom, startOffset, endOffset, bucketCount);
}

export async function bridgeSearchRomPattern(
  pattern: Uint8Array,
  startOffset = 0,
  maxHits = 256,
  fallbackRom?: Uint8Array | null
): Promise<RomSearchResult> {
  if (pattern.length === 0) {
    return { offsets: [], truncated: false, total_len: fallbackRom?.length ?? 0 };
  }
  if (isDesktopApp()) {
    try {
      return await invoke<RomSearchResult>('search_rom_pattern', {
        pattern: Array.from(pattern.slice(0, 256)),
        startOffset,
        maxHits,
      });
    } catch (err) {
      console.warn('Tauri search_rom_pattern IPC error, falling back:', err);
    }
  }
  if (!fallbackRom) return { offsets: [], truncated: false, total_len: 0 };
  return searchPatternInSlice(fallbackRom, pattern, startOffset, maxHits);
}

export async function bridgePatchMapCells(
  mapDef: MapDef,
  edits: CellWriteInput[],
  label?: string
): Promise<PatchCommitOutcome | null> {
  if (!isDesktopApp()) return null;
  return await invoke<PatchCommitOutcome>('patch_map_cells', {
    mapDef,
    edits,
    label: label ?? null,
  });
}

export async function bridgePatchRomBytes(
  writes: ByteWriteInput[],
  label?: string
): Promise<PatchCommitOutcome | null> {
  if (!isDesktopApp()) return null;
  return await invoke<PatchCommitOutcome>('patch_rom_bytes', {
    writes,
    label: label ?? null,
  });
}

export async function bridgeUndoRomEdit(): Promise<UndoRedoOutcome | null> {
  if (!isDesktopApp()) return null;
  return await invoke<UndoRedoOutcome>('undo_rom_edit');
}

export async function bridgeRedoRomEdit(): Promise<UndoRedoOutcome | null> {
  if (!isDesktopApp()) return null;
  return await invoke<UndoRedoOutcome>('redo_rom_edit');
}

export async function bridgeGetUndoStatus(): Promise<UndoStatus> {
  if (!isDesktopApp()) return emptyUndoStatus();
  try {
    return await invoke<UndoStatus>('get_undo_status');
  } catch (err) {
    console.warn('Tauri get_undo_status IPC error:', err);
    return emptyUndoStatus();
  }
}
