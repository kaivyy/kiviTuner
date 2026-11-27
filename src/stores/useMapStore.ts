import { create } from 'zustand';
import type { MapDef, BinaryHashes } from '../types/kivi.ts';
import {
  isDesktopApp,
  bridgeLoadBinData,
  bridgeParseXdf,
  bridgeExtractMapMatrix,
  bridgeExtractOriginalMapMatrix,
  bridgeUpdateMapMatrix,
  bridgeVerifyChecksum,
  bridgeFixChecksum,
  bridgeAutoDetectOemChecksums,
  bridgeRepairOemChecksum,
  bridgeSaveBinData,
} from '../lib/tauriBridge.ts';
import {
  bridgeLoadBinRaw,
  bridgeOpenRomDocumentRaw,
  bridgeOpenRomDocumentFile,
  bridgeSwitchRomDocument,
  bridgeCloseRomDocument,
  bridgeRenameRomDocument,
  bridgeGetRomSessionSummary,
  bridgePatchMapCells,
  bridgePatchRomBytes,
  bridgeUndoRomEdit,
  bridgeRedoRomEdit,
  bridgeGetUndoStatus,
  computeHashesLocal,
  emptyUndoStatus,
  type ByteWriteInput,
  type CellWriteInput,
  type RomDocumentSummary,
  type RomSessionSummary,
  type UndoStatus,
} from '../lib/bridge/windowBridge.ts';
import { extractMatrixFromRom, writeMatrixToRom } from '../lib/matrixRom.ts';

export interface RecentRomEntry {
  id: string;
  label: string;
  filePath: string | null;
  size: number;
  sha256: string;
  loadedAt: number;
}

interface BrowserBytePatch {
  offset: number;
  oldBytes: Uint8Array;
  newBytes: Uint8Array;
}

interface BrowserUndoEntry {
  label: string;
  patches: BrowserBytePatch[];
}

interface BrowserDocSlot {
  id: string;
  label: string;
  filePath: string | null;
  romData: Uint8Array;
  originalRomData: Uint8Array;
  hashes: BinaryHashes;
  maps: MapDef[];
  selectedMapId: string | null;
  baseAddress: number | null;
  checksumAlgorithmId: string | null;
  checksumAlgorithmName: string | null;
  undoStack: BrowserUndoEntry[];
  redoStack: BrowserUndoEntry[];
}

interface DesktopDocMeta {
  maps: MapDef[];
  selectedMapId: string | null;
  checksumAlgorithmId: string | null;
  checksumAlgorithmName: string | null;
}

const MAX_BROWSER_UNDO = 128;
const MAX_RECENT_ROMS = 10;
const MAX_MATRIX_CACHE = 16;

const matrixCache = new Map<string, { active: number[][]; base: number[][] }>();

function buildMatrixCacheKey(docId: string | null, revision: number, mapDef: MapDef): string {
  return `${docId ?? 'none'}:${revision}:${mapDef.id}:${mapDef.address}:${mapDef.rows}x${mapDef.cols}:${mapDef.dtype}:${mapDef.endian}:${mapDef.scaling.equation}`;
}

function getCachedMatrix(key: string): { active: number[][]; base: number[][] } | undefined {
  const hit = matrixCache.get(key);
  if (!hit) return undefined;
  matrixCache.delete(key);
  matrixCache.set(key, hit);
  return hit;
}

function putCachedMatrix(key: string, active: number[][], base: number[][]): void {
  if (matrixCache.has(key)) {
    matrixCache.delete(key);
  } else if (matrixCache.size >= MAX_MATRIX_CACHE) {
    const oldest = matrixCache.keys().next().value;
    if (oldest !== undefined) matrixCache.delete(oldest);
  }
  matrixCache.set(key, { active, base });
}

function clearMatrixCache(): void {
  matrixCache.clear();
}

const EMPTY_ROM_FIELDS = {
  maps: [] as MapDef[],
  selectedMapId: null as string | null,
  romLoaded: false,
  romSize: 0,
  romData: null as Uint8Array | null,
  originalRomData: null as Uint8Array | null,
  hashes: null as BinaryHashes | null,
  activeMatrix: null as number[][] | null,
  baseMatrix: null as number[][] | null,
  checksumStatus: null as 'valid' | 'invalid' | null,
  checksumAlgorithmId: null as string | null,
  checksumAlgorithmName: null as string | null,
  checksumNotice: null as string | null,
  romBaseAddress: null as number | null,
  romRevision: 0,
  undoStatus: emptyUndoStatus(),
  romDocuments: [] as RomDocumentSummary[],
  activeDocId: null as string | null,
};

let browserDocSeq = 0;
let browserDocs: BrowserDocSlot[] = [];
let browserRevision = 0;

const desktopDocMeta = new Map<string, DesktopDocMeta>();

// Monotonic guards and write serialization so concurrent IPC/UI mutations
// cannot commit out of order or overwrite newer active documents.
let docOpSeq = 0;
let extractSeq = 0;
let checksumSeq = 0;
let writeQueue: Promise<unknown> = Promise.resolve();

function enqueueWrite<T>(fn: () => Promise<T>): Promise<T> {
  const next = writeQueue.then(fn, fn);
  writeQueue = next.catch(() => undefined);
  return next;
}

function recordRecentRom(
  prev: RecentRomEntry[],
  entry: Omit<RecentRomEntry, 'id'>
): RecentRomEntry[] {
  const key = entry.filePath ? `path:${entry.filePath}` : `sha:${entry.sha256}`;
  const filtered = prev.filter((item) => {
    const itemKey = item.filePath ? `path:${item.filePath}` : `sha:${item.sha256}`;
    return itemKey !== key;
  });
  return [{ ...entry, id: key }, ...filtered].slice(0, MAX_RECENT_ROMS);
}

function saveDesktopDocMeta(state: MapStoreState): void {
  if (!state.activeDocId) return;
  desktopDocMeta.set(state.activeDocId, {
    maps: state.maps,
    selectedMapId: state.selectedMapId,
    checksumAlgorithmId: state.checksumAlgorithmId,
    checksumAlgorithmName: state.checksumAlgorithmName,
  });
}

function toBrowserUndoStatus(slot: BrowserDocSlot | undefined, revision: number): UndoStatus {
  if (!slot) return emptyUndoStatus();
  const last = slot.undoStack.length > 0 ? slot.undoStack[slot.undoStack.length - 1].label : null;
  return {
    can_undo: slot.undoStack.length > 0,
    can_redo: slot.redoStack.length > 0,
    undo_count: slot.undoStack.length,
    redo_count: slot.redoStack.length,
    last_label: last,
    revision,
  };
}

function toBrowserSummaries(activeId: string | null, revision: number): RomDocumentSummary[] {
  return browserDocs.map((doc) => ({
    id: doc.id,
    label: doc.label,
    file_path: doc.filePath,
    loaded: true,
    size: doc.romData.length,
    sha256: doc.hashes.sha256,
    md5: doc.hashes.md5,
    base_address: doc.baseAddress,
    map_count: doc.maps.length,
    undo: toBrowserUndoStatus(doc, revision),
    is_active: doc.id === activeId,
  }));
}

function diffByteSlices(before: Uint8Array, after: Uint8Array): BrowserBytePatch[] {
  const len = Math.min(before.length, after.length);
  const patches: BrowserBytePatch[] = [];
  let i = 0;
  while (i < len) {
    if (before[i] === after[i]) {
      i++;
      continue;
    }
    const start = i;
    while (i < len && before[i] !== after[i]) i++;
    patches.push({
      offset: start,
      oldBytes: before.slice(start, i),
      newBytes: after.slice(start, i),
    });
  }
  return patches;
}

function pushBrowserUndo(slot: BrowserDocSlot, label: string, patches: BrowserBytePatch[]): void {
  if (patches.length === 0) return;
  slot.undoStack.push({ label, patches });
  if (slot.undoStack.length > MAX_BROWSER_UNDO) {
    slot.undoStack.shift();
  }
  slot.redoStack = [];
}

function getActiveBrowserSlot(activeDocId: string | null): BrowserDocSlot | undefined {
  if (activeDocId) {
    const found = browserDocs.find((d) => d.id === activeDocId);
    if (found) return found;
  }
  return browserDocs[0];
}

function syncBrowserDocSlot(
  docId: string | null,
  romData: Uint8Array,
  originalRomData: Uint8Array,
  hashes: BinaryHashes,
  maps: MapDef[],
  selectedMapId: string | null,
  baseAddress: number | null,
  label = 'Primary ROM'
): BrowserDocSlot {
  let slot = docId ? browserDocs.find((d) => d.id === docId) : undefined;
  if (!slot) {
    if (browserDocs.length === 0) {
      browserDocSeq += 1;
      slot = {
        id: `rom-${browserDocSeq}`,
        label,
        filePath: null,
        romData,
        originalRomData,
        hashes,
        maps,
        selectedMapId,
        baseAddress,
        checksumAlgorithmId: null,
        checksumAlgorithmName: null,
        undoStack: [],
        redoStack: [],
      };
      browserDocs.push(slot);
    } else {
      slot = browserDocs[0];
    }
  }
  slot.romData = romData;
  slot.originalRomData = originalRomData;
  slot.hashes = hashes;
  slot.maps = maps;
  slot.selectedMapId = selectedMapId;
  slot.baseAddress = baseAddress;
  return slot;
}

function mergeMapsById(existing: MapDef[], incoming: MapDef[]): MapDef[] {
  const byId = new Map<string, number>();
  const merged = [...existing];
  merged.forEach((m, idx) => byId.set(m.id, idx));
  for (const map of incoming) {
    const idx = byId.get(map.id);
    if (idx !== undefined) {
      merged[idx] = map;
    } else {
      byId.set(map.id, merged.length);
      merged.push(map);
    }
  }
  return merged;
}

function applySessionSummaryState(
  summary: RomSessionSummary,
  currentSelectedMapId: string | null
): Partial<MapStoreState> {
  return {
    activeDocId: summary.active_doc_id,
    romLoaded: summary.loaded,
    romSize: summary.size,
    romData: null,
    originalRomData: null,
    hashes: summary.hashes,
    checksumStatus: null,
    undoStatus: summary.undo,
    romDocuments: summary.documents,
    romBaseAddress: summary.base_address ?? null,
    selectedMapId: currentSelectedMapId,
  };
}

export interface MapStoreState {
  maps: MapDef[];
  selectedMapId: string | null;
  romLoaded: boolean;
  romSize: number;
  romData: Uint8Array | null;
  originalRomData: Uint8Array | null;
  hashes: BinaryHashes | null;
  activeMatrix: number[][] | null;
  baseMatrix: number[][] | null;
  isDesktop: boolean;
  checksumStatus: 'valid' | 'invalid' | null;
  checksumAlgorithmId: string | null;
  checksumAlgorithmName: string | null;
  checksumNotice: string | null;
  /**
   * Lowest logical address of the loaded ROM, or `null` when unknown.
   *
   * `0` is a *known* base of zero; `null` means no address information. This is
   * comparison input, not backend state: `compare_rom_binaries_with_address`
   * receives it as a parameter and never reads `MemoryMap.base_address`.
   */
  romBaseAddress: number | null;
  romRevision: number;
  undoStatus: UndoStatus;
  romDocuments: RomDocumentSummary[];
  activeDocId: string | null;
  recentRoms: RecentRomEntry[];

  setMaps: (maps: MapDef[]) => void;
  addMaps: (newMaps: MapDef[]) => void;
  updateMapDef: (mapDef: MapDef) => Promise<void>;
  deleteMapDef: (mapId: string) => void;
  selectMap: (id: string | null) => void;
  setRomLoaded: (
    loaded: boolean,
    size?: number,
    hashes?: BinaryHashes | null,
    data?: Uint8Array | null
  ) => void;
  setActiveMatrix: (matrix: number[][] | null) => void;
  setBaseMatrix: (matrix: number[][] | null) => void;
  updateCell: (row: number, col: number, value: number) => void;
  setChecksumStatus: (status: 'valid' | 'invalid' | null) => void;
  /** States the loaded ROM's base address. `null` clears it to unknown. */
  setRomBaseAddress: (address: number | null) => void;
  /**
   * Test-only synchronous loader: installs ROM bytes without an async bridge
   * round-trip, so store invariants can be exercised in isolation.
   */
  loadRomBufferSync: (data: Uint8Array) => void;

  // Tauri bridge actions with browser fallback
  loadRomBuffer: (
    data: Uint8Array,
    fallbackHashes?: BinaryHashes,
    meta?: { label?: string; path?: string }
  ) => Promise<BinaryHashes>;
  loadXdfString: (xml: string) => Promise<MapDef[]>;
  extractMatrix: (mapDef: MapDef) => Promise<number[][]>;
  updateMatrix: (mapDef: MapDef, matrix: number[][]) => Promise<void>;
  patchActiveMapCells: (mapDef: MapDef, edits: CellWriteInput[], label?: string) => Promise<void>;
  patchRomBytes: (writes: ByteWriteInput[], label?: string) => Promise<boolean>;
  revertMapToOriginal: (
    mapDef: MapDef,
    selection?: { minR: number; maxR: number; minC: number; maxC: number } | null
  ) => Promise<boolean>;
  undoEdit: () => Promise<boolean>;
  redoEdit: () => Promise<boolean>;
  verifyChecksum: () => Promise<boolean>;
  fixChecksum: () => Promise<boolean>;
  exportRom: () => Promise<Uint8Array | null>;

  // Multi-ROM document tabs (multi-project switching)
  openRomDocument: (
    data: Uint8Array,
    meta?: { docId?: string; label?: string; path?: string }
  ) => Promise<void>;
  openRomDocumentFile: (path: string, meta?: { docId?: string; label?: string }) => Promise<void>;
  switchRomDocument: (docId: string) => Promise<void>;
  closeRomDocument: (docId: string) => Promise<void>;
  closeActiveRom: () => Promise<void>;
  renameRomDocument: (docId: string, label: string) => Promise<void>;
  refreshRomSession: () => Promise<void>;
  clearRecentRoms: () => void;
  reset: () => void;
}

export const useMapStore = create<MapStoreState>((set, get) => ({
  ...EMPTY_ROM_FIELDS,
  isDesktop: isDesktopApp(),
  recentRoms: [],

  setMaps: (maps) => {
    const activeSlot = getActiveBrowserSlot(get().activeDocId);
    if (activeSlot) activeSlot.maps = maps;
    set({ maps });
    if (isDesktopApp()) saveDesktopDocMeta(get());
  },

  addMaps: (newMaps) => {
    if (newMaps.length === 0) return;
    const state = get();
    const merged = mergeMapsById(state.maps, newMaps);
    const nextSelected = state.selectedMapId ?? newMaps[0]?.id ?? null;
    const activeSlot = getActiveBrowserSlot(state.activeDocId);
    if (activeSlot) {
      activeSlot.maps = merged;
      activeSlot.selectedMapId = nextSelected;
    }
    set({ maps: merged, selectedMapId: nextSelected });
    if (isDesktopApp()) saveDesktopDocMeta(get());
  },

  updateMapDef: async (mapDef) => {
    clearMatrixCache();
    const state = get();
    const exists = state.maps.some((m) => m.id === mapDef.id);
    const nextMaps = exists
      ? state.maps.map((m) => (m.id === mapDef.id ? mapDef : m))
      : [...state.maps, mapDef];
    const activeSlot = getActiveBrowserSlot(state.activeDocId);
    if (activeSlot) activeSlot.maps = nextMaps;
    set({ maps: nextMaps });
    if (isDesktopApp()) saveDesktopDocMeta(get());
    if (get().selectedMapId === mapDef.id && (get().romLoaded || get().romData)) {
      await get().extractMatrix(mapDef);
    }
  },

  deleteMapDef: (mapId) => {
    clearMatrixCache();
    const state = get();
    const nextMaps = state.maps.filter((m) => m.id !== mapId);
    const removingSelected = state.selectedMapId === mapId;
    const nextSelected = removingSelected ? nextMaps[0]?.id ?? null : state.selectedMapId;
    const activeSlot = getActiveBrowserSlot(state.activeDocId);
    if (activeSlot) {
      activeSlot.maps = nextMaps;
      activeSlot.selectedMapId = nextSelected;
    }
    set({
      maps: nextMaps,
      selectedMapId: nextSelected,
      ...(removingSelected ? { activeMatrix: null, baseMatrix: null } : {}),
    });
    if (isDesktopApp()) saveDesktopDocMeta(get());
  },

  selectMap: (id) => {
    const state = get();
    const activeSlot = getActiveBrowserSlot(state.activeDocId);
    if (activeSlot) activeSlot.selectedMapId = id;
    const targetMap = id ? state.maps.find((m) => m.id === id) : undefined;
    if (targetMap && (state.romLoaded || state.romData)) {
      const key = buildMatrixCacheKey(state.activeDocId, state.romRevision, targetMap);
      const cached = getCachedMatrix(key);
      if (cached) {
        set({
          selectedMapId: id,
          activeMatrix: cached.active,
          baseMatrix: cached.base,
        });
        if (isDesktopApp()) saveDesktopDocMeta(get());
        return;
      }
    }
    set({ selectedMapId: id });
    if (isDesktopApp()) saveDesktopDocMeta(get());
  },

  setRomLoaded: (loaded, size = 0, hashes = null, data = null) =>
    set({
      romLoaded: loaded,
      romSize: size,
      hashes,
      romData: data,
      originalRomData: data ? new Uint8Array(data) : null,
      checksumStatus: null,
      checksumNotice: null,
    }),

  setActiveMatrix: (matrix) => set({ activeMatrix: matrix }),
  setBaseMatrix: (matrix) => set({ baseMatrix: matrix }),
  setChecksumStatus: (status) => set({ checksumStatus: status }),

  setRomBaseAddress: (address) =>
    set((state) => {
      // An address describes a specific ROM. Without one there is nothing to
      // attach it to, so refuse rather than hold an orphaned value.
      if (!state.romLoaded && !state.romData) return state;
      const activeSlot = getActiveBrowserSlot(state.activeDocId);
      if (activeSlot) activeSlot.baseAddress = address;
      return { romBaseAddress: address };
    }),

  loadRomBufferSync: (data) => {
    clearMatrixCache();
    docOpSeq += 1;
    extractSeq += 1;
    checksumSeq += 1;
    const hashes = computeHashesLocal(data);
    const originalSnap = new Uint8Array(data);
    browserRevision += 1;
    browserDocs = [];
    const slot = syncBrowserDocSlot(
      null,
      data,
      originalSnap,
      hashes,
      [],
      null,
      null,
      'Primary ROM'
    );
    set({
      ...EMPTY_ROM_FIELDS,
      romLoaded: true,
      romSize: data.length,
      romData: data,
      originalRomData: originalSnap,
      hashes,
      romRevision: browserRevision,
      undoStatus: toBrowserUndoStatus(slot, browserRevision),
      romDocuments: toBrowserSummaries(slot.id, browserRevision),
      activeDocId: slot.id,
    });
  },

  updateCell: (row, col, value) =>
    set((state) => {
      if (!state.activeMatrix) return state;
      const newMatrix = state.activeMatrix.map((r, rIdx) =>
        rIdx === row ? r.map((c, cIdx) => (cIdx === col ? value : c)) : [...r]
      );
      return { activeMatrix: newMatrix };
    }),

  loadRomBuffer: async (data, fallbackHashes, meta) => {
    clearMatrixCache();
    const myDocSeq = ++docOpSeq;
    extractSeq += 1;
    checksumSeq += 1;
    const desktop = isDesktopApp();
    const hashes = desktop
      ? meta?.label || meta?.path
        ? await bridgeLoadBinRaw(data, meta.label, meta.path)
        : await bridgeLoadBinData(data, fallbackHashes)
      : await bridgeLoadBinData(data, fallbackHashes);

    if (desktop) {
      const summary = await bridgeGetRomSessionSummary();
      if (myDocSeq !== docOpSeq) return hashes;
      const activeDoc = summary?.documents.find((d) => d.is_active) ?? null;
      const label = meta?.label ?? activeDoc?.label ?? 'Primary ROM';
      set((state) => ({
        ...EMPTY_ROM_FIELDS,
        romLoaded: true,
        romSize: data.length,
        romData: null,
        originalRomData: null,
        hashes,
        romRevision: state.romRevision + 1,
        undoStatus: summary?.undo ?? emptyUndoStatus(),
        romDocuments: summary?.documents ?? [],
        activeDocId: summary?.active_doc_id ?? activeDoc?.id ?? null,
        recentRoms: recordRecentRom(state.recentRoms, {
          label,
          filePath: meta?.path ?? activeDoc?.file_path ?? null,
          size: data.length,
          sha256: hashes.sha256,
          loadedAt: Date.now(),
        }),
      }));
      return hashes;
    }

    if (myDocSeq !== docOpSeq) return hashes;
    const originalSnap = new Uint8Array(data);
    browserRevision += 1;
    browserDocs = [];
    const label = meta?.label ?? 'Primary ROM';
    const slot = syncBrowserDocSlot(
      null,
      data,
      originalSnap,
      hashes,
      get().maps,
      get().selectedMapId,
      null,
      label
    );
    set((state) => ({
      ...EMPTY_ROM_FIELDS,
      maps: slot.maps,
      selectedMapId: slot.selectedMapId,
      romLoaded: true,
      romSize: data.length,
      romData: data,
      originalRomData: originalSnap,
      hashes,
      romRevision: browserRevision,
      undoStatus: toBrowserUndoStatus(slot, browserRevision),
      romDocuments: toBrowserSummaries(slot.id, browserRevision),
      activeDocId: slot.id,
      recentRoms: recordRecentRom(state.recentRoms, {
        label,
        filePath: meta?.path ?? null,
        size: data.length,
        sha256: hashes.sha256,
        loadedAt: Date.now(),
      }),
    }));
    return hashes;
  },

  loadXdfString: async (xml) => {
    const myDocId = get().activeDocId;
    const myDocSeq = docOpSeq;
    const defs = await bridgeParseXdf(xml, get().maps);
    if (myDocSeq !== docOpSeq || get().activeDocId !== myDocId) {
      return defs;
    }
    const activeSlot = getActiveBrowserSlot(myDocId);
    if (activeSlot) {
      activeSlot.maps = defs;
      if (defs.length > 0) activeSlot.selectedMapId = defs[0].id;
    }
    set({
      maps: defs,
      ...(defs.length > 0 ? { selectedMapId: defs[0].id } : {}),
    });
    if (isDesktopApp()) saveDesktopDocMeta(get());
    return defs;
  },

  extractMatrix: async (mapDef) => {
    const state = get();
    const myDocId = state.activeDocId;
    const rev = state.romRevision;
    const cacheKey = buildMatrixCacheKey(myDocId, rev, mapDef);
    const cached = getCachedMatrix(cacheKey);
    if (cached) {
      if (state.selectedMapId !== mapDef.id || state.activeMatrix !== cached.active || state.baseMatrix !== cached.base) {
        set({
          activeMatrix: cached.active,
          baseMatrix: cached.base,
        });
      }
      return cached.active;
    }

    const seq = ++extractSeq;
    const myDocSeq = docOpSeq;
    const desktop = isDesktopApp();

    if (desktop) {
      const [matrix, stock] = await Promise.all([
        bridgeExtractMapMatrix(mapDef, null),
        bridgeExtractOriginalMapMatrix(mapDef, null),
      ]);
      if (seq !== extractSeq || myDocSeq !== docOpSeq || get().activeDocId !== myDocId) {
        return matrix;
      }
      const base = stock ?? matrix.map((row) => [...row]);
      putCachedMatrix(cacheKey, matrix, base);
      set({
        activeMatrix: matrix,
        baseMatrix: base,
      });
      return matrix;
    }

    const matrix = await bridgeExtractMapMatrix(mapDef, state.romData);
    const stock = state.originalRomData
      ? await bridgeExtractOriginalMapMatrix(mapDef, state.originalRomData)
      : null;
    if (seq !== extractSeq || myDocSeq !== docOpSeq || get().activeDocId !== myDocId) {
      return matrix;
    }
    const base = stock ?? matrix.map((row) => [...row]);
    putCachedMatrix(cacheKey, matrix, base);
    set({
      activeMatrix: matrix,
      baseMatrix: base,
    });
    return matrix;
  },

  updateMatrix: async (mapDef, matrix) => {
    clearMatrixCache();
    const targetDocId = get().activeDocId;
    const targetDocSeq = docOpSeq;
    if (get().selectedMapId === mapDef.id) {
      set({ activeMatrix: matrix });
    }

    await enqueueWrite(async () => {
      if (docOpSeq !== targetDocSeq || get().activeDocId !== targetDocId) return;
      const state = get();
      const prevRom = state.romData;
      const nextRom = await bridgeUpdateMapMatrix(mapDef, matrix, prevRom);
      if (docOpSeq !== targetDocSeq || get().activeDocId !== targetDocId) return;

      if (isDesktopApp()) {
        const undoStatus = await bridgeGetUndoStatus();
        if (docOpSeq !== targetDocSeq || get().activeDocId !== targetDocId) return;
        set((s) => ({
          checksumStatus: null,
          undoStatus,
          romRevision: s.romRevision + 1,
        }));
        return;
      }

      if (nextRom) {
        browserRevision += 1;
        const hashes = computeHashesLocal(nextRom);
        const slot = getActiveBrowserSlot(targetDocId);
        if (slot && prevRom) {
          pushBrowserUndo(slot, `Edit ${mapDef.name}`, diffByteSlices(prevRom, nextRom));
          slot.romData = nextRom;
          slot.hashes = hashes;
        }
        set({
          romData: nextRom,
          hashes,
          checksumStatus: null,
          romRevision: browserRevision,
          undoStatus: toBrowserUndoStatus(slot, browserRevision),
          romDocuments: toBrowserSummaries(targetDocId, browserRevision),
        });
      }
    });
  },

  patchActiveMapCells: async (mapDef, edits, label) => {
    if (edits.length === 0) return;
    clearMatrixCache();
    const targetDocId = get().activeDocId;
    const targetDocSeq = docOpSeq;

    const current = get();
    if (current.activeMatrix && current.selectedMapId === mapDef.id) {
      const patched = current.activeMatrix.map((r) => [...r]);
      for (const edit of edits) {
        if (
          edit.row >= 0 &&
          edit.row < patched.length &&
          edit.col >= 0 &&
          edit.col < patched[edit.row].length
        ) {
          patched[edit.row][edit.col] = edit.value;
        }
      }
      set({ activeMatrix: patched });
    }

    await enqueueWrite(async () => {
      if (docOpSeq !== targetDocSeq || get().activeDocId !== targetDocId) return;
      const state = get();

      if (isDesktopApp()) {
        const outcome = await bridgePatchMapCells(mapDef, edits, label);
        if (docOpSeq !== targetDocSeq || get().activeDocId !== targetDocId) return;
        if (outcome) {
          set((s) => ({
            hashes: outcome.hashes,
            undoStatus: outcome.undo,
            checksumStatus: null,
            romRevision: s.romRevision + 1,
          }));
        }
        return;
      }

      if (!state.romData) return;
      const currentMatrix = extractMatrixFromRom(state.romData, mapDef);
      for (const edit of edits) {
        if (
          edit.row >= 0 &&
          edit.row < currentMatrix.length &&
          edit.col >= 0 &&
          edit.col < currentMatrix[edit.row].length
        ) {
          currentMatrix[edit.row][edit.col] = edit.value;
        }
      }
      const nextRom = writeMatrixToRom(state.romData, mapDef, currentMatrix);
      const hashes = computeHashesLocal(nextRom);
      browserRevision += 1;
      const slot = getActiveBrowserSlot(targetDocId);
      if (slot) {
        pushBrowserUndo(
          slot,
          label ?? `Edit ${mapDef.name}`,
          diffByteSlices(state.romData, nextRom)
        );
        slot.romData = nextRom;
        slot.hashes = hashes;
      }
      set({
        romData: nextRom,
        hashes,
        checksumStatus: null,
        romRevision: browserRevision,
        undoStatus: toBrowserUndoStatus(slot, browserRevision),
        romDocuments: toBrowserSummaries(targetDocId, browserRevision),
        ...(state.selectedMapId === mapDef.id ? { activeMatrix: currentMatrix } : {}),
      });
    });
  },

  patchRomBytes: async (writes, label) => {
    if (writes.length === 0) return false;
    clearMatrixCache();
    const targetDocId = get().activeDocId;
    const targetDocSeq = docOpSeq;

    return await enqueueWrite(async () => {
      if (docOpSeq !== targetDocSeq || get().activeDocId !== targetDocId) return false;
      const state = get();

      if (isDesktopApp()) {
        const outcome = await bridgePatchRomBytes(writes, label);
        if (!outcome || docOpSeq !== targetDocSeq || get().activeDocId !== targetDocId) {
          return false;
        }
        const selectedMap = get().maps.find((m) => m.id === get().selectedMapId);
        const nextMatrix = selectedMap
          ? await bridgeExtractMapMatrix(selectedMap, null)
          : get().activeMatrix;
        if (docOpSeq !== targetDocSeq || get().activeDocId !== targetDocId) return false;
        set((s) => ({
          hashes: outcome.hashes,
          undoStatus: outcome.undo,
          checksumStatus: null,
          romRevision: s.romRevision + 1,
          activeMatrix: nextMatrix,
        }));
        return true;
      }

      if (!state.romData) return false;
      const nextRom = new Uint8Array(state.romData);
      for (const w of writes) {
        if (w.offset < 0 || w.offset + w.bytes.length > nextRom.length) {
          return false;
        }
      }
      for (const w of writes) {
        for (let i = 0; i < w.bytes.length; i++) {
          nextRom[w.offset + i] = w.bytes[i] & 0xff;
        }
      }
      const hashes = computeHashesLocal(nextRom);
      browserRevision += 1;
      const slot = getActiveBrowserSlot(targetDocId);
      if (slot) {
        pushBrowserUndo(slot, label ?? 'Hex Edit', diffByteSlices(state.romData, nextRom));
        slot.romData = nextRom;
        slot.hashes = hashes;
      }
      const selectedMap = state.maps.find((m) => m.id === state.selectedMapId);
      const nextMatrix = selectedMap
        ? extractMatrixFromRom(nextRom, selectedMap)
        : state.activeMatrix;
      set({
        romData: nextRom,
        hashes,
        checksumStatus: null,
        romRevision: browserRevision,
        undoStatus: toBrowserUndoStatus(slot, browserRevision),
        romDocuments: toBrowserSummaries(targetDocId, browserRevision),
        activeMatrix: nextMatrix,
      });
      return true;
    });
  },

  revertMapToOriginal: async (mapDef, selection) => {
    clearMatrixCache();
    const targetDocId = get().activeDocId;
    const targetDocSeq = docOpSeq;

    return await enqueueWrite(async () => {
      if (docOpSeq !== targetDocSeq || get().activeDocId !== targetDocId) return false;
      const state = get();
      const desktop = isDesktopApp();
      const stockMatrix = desktop
        ? await bridgeExtractOriginalMapMatrix(mapDef, null)
        : state.originalRomData
          ? await bridgeExtractOriginalMapMatrix(mapDef, state.originalRomData)
          : null;
      if (!stockMatrix || docOpSeq !== targetDocSeq || get().activeDocId !== targetDocId) {
        return false;
      }

      const liveMatrix = desktop
        ? await bridgeExtractMapMatrix(mapDef, null)
        : state.romData
          ? extractMatrixFromRom(state.romData, mapDef)
          : null;
      if (!liveMatrix || docOpSeq !== targetDocSeq || get().activeDocId !== targetDocId) {
        return false;
      }

      const edits: CellWriteInput[] = [];
      const minR = selection ? Math.max(0, selection.minR) : 0;
      const maxR = selection ? Math.min(stockMatrix.length - 1, selection.maxR) : stockMatrix.length - 1;
      for (let r = minR; r <= maxR; r++) {
        const minC = selection ? Math.max(0, selection.minC) : 0;
        const maxC = selection
          ? Math.min((stockMatrix[r]?.length ?? 0) - 1, selection.maxC)
          : (stockMatrix[r]?.length ?? 0) - 1;
        for (let c = minC; c <= maxC; c++) {
          const stockVal = stockMatrix[r]?.[c];
          const liveVal = liveMatrix[r]?.[c];
          if (stockVal !== undefined && liveVal !== stockVal) {
            edits.push({ row: r, col: c, value: stockVal });
          }
        }
      }

      if (edits.length === 0) return false;
      const label = selection ? `Revert Selection (${mapDef.name})` : `Revert Map (${mapDef.name})`;

      if (desktop) {
        const outcome = await bridgePatchMapCells(mapDef, edits, label);
        if (!outcome || docOpSeq !== targetDocSeq || get().activeDocId !== targetDocId) {
          return false;
        }
        const refreshed = await bridgeExtractMapMatrix(mapDef, null);
        if (docOpSeq !== targetDocSeq || get().activeDocId !== targetDocId) return false;
        set((s) => ({
          hashes: outcome.hashes,
          undoStatus: outcome.undo,
          checksumStatus: null,
          romRevision: s.romRevision + 1,
          ...(s.selectedMapId === mapDef.id ? { activeMatrix: refreshed, baseMatrix: stockMatrix } : {}),
        }));
        return true;
      }

      if (!state.romData) return false;
      const nextMatrix = liveMatrix.map((row) => [...row]);
      for (const edit of edits) {
        nextMatrix[edit.row][edit.col] = edit.value;
      }
      const nextRom = writeMatrixToRom(state.romData, mapDef, nextMatrix);
      const hashes = computeHashesLocal(nextRom);
      browserRevision += 1;
      const slot = getActiveBrowserSlot(targetDocId);
      if (slot) {
        pushBrowserUndo(slot, label, diffByteSlices(state.romData, nextRom));
        slot.romData = nextRom;
        slot.hashes = hashes;
      }
      set({
        romData: nextRom,
        hashes,
        checksumStatus: null,
        romRevision: browserRevision,
        undoStatus: toBrowserUndoStatus(slot, browserRevision),
        romDocuments: toBrowserSummaries(targetDocId, browserRevision),
        ...(state.selectedMapId === mapDef.id ? { activeMatrix: nextMatrix, baseMatrix: stockMatrix } : {}),
      });
      return true;
    });
  },

  undoEdit: async () => {
    clearMatrixCache();
    const targetDocId = get().activeDocId;
    const targetDocSeq = docOpSeq;

    return await enqueueWrite(async () => {
      if (docOpSeq !== targetDocSeq || get().activeDocId !== targetDocId) return false;
      const state = get();

      if (isDesktopApp()) {
        const outcome = await bridgeUndoRomEdit();
        if (!outcome?.applied || docOpSeq !== targetDocSeq || get().activeDocId !== targetDocId) {
          return false;
        }
        const selectedMap = get().maps.find((m) => m.id === get().selectedMapId);
        const nextMatrix = selectedMap
          ? await bridgeExtractMapMatrix(selectedMap, null)
          : get().activeMatrix;
        if (docOpSeq !== targetDocSeq || get().activeDocId !== targetDocId) return false;
        set((s) => ({
          hashes: outcome.hashes,
          undoStatus: outcome.undo,
          checksumStatus: null,
          romRevision: s.romRevision + 1,
          activeMatrix: nextMatrix,
        }));
        return true;
      }

      const slot = getActiveBrowserSlot(targetDocId);
      if (!slot || slot.undoStack.length === 0) return false;
      const tx = slot.undoStack.pop()!;
      const nextRom = new Uint8Array(slot.romData);
      for (let i = tx.patches.length - 1; i >= 0; i--) {
        const p = tx.patches[i];
        nextRom.set(p.oldBytes, p.offset);
      }
      slot.redoStack.push(tx);
      slot.romData = nextRom;
      slot.hashes = computeHashesLocal(nextRom);
      browserRevision += 1;
      const selectedMap = state.maps.find((m) => m.id === state.selectedMapId);
      const nextMatrix = selectedMap
        ? extractMatrixFromRom(nextRom, selectedMap)
        : state.activeMatrix;
      set({
        romData: nextRom,
        hashes: slot.hashes,
        checksumStatus: null,
        romRevision: browserRevision,
        undoStatus: toBrowserUndoStatus(slot, browserRevision),
        romDocuments: toBrowserSummaries(slot.id, browserRevision),
        activeMatrix: nextMatrix,
      });
      return true;
    });
  },

  redoEdit: async () => {
    clearMatrixCache();
    const targetDocId = get().activeDocId;
    const targetDocSeq = docOpSeq;

    return await enqueueWrite(async () => {
      if (docOpSeq !== targetDocSeq || get().activeDocId !== targetDocId) return false;
      const state = get();

      if (isDesktopApp()) {
        const outcome = await bridgeRedoRomEdit();
        if (!outcome?.applied || docOpSeq !== targetDocSeq || get().activeDocId !== targetDocId) {
          return false;
        }
        const selectedMap = get().maps.find((m) => m.id === get().selectedMapId);
        const nextMatrix = selectedMap
          ? await bridgeExtractMapMatrix(selectedMap, null)
          : get().activeMatrix;
        if (docOpSeq !== targetDocSeq || get().activeDocId !== targetDocId) return false;
        set((s) => ({
          hashes: outcome.hashes,
          undoStatus: outcome.undo,
          checksumStatus: null,
          romRevision: s.romRevision + 1,
          activeMatrix: nextMatrix,
        }));
        return true;
      }

      const slot = getActiveBrowserSlot(targetDocId);
      if (!slot || slot.redoStack.length === 0) return false;
      const tx = slot.redoStack.pop()!;
      const nextRom = new Uint8Array(slot.romData);
      for (const p of tx.patches) {
        nextRom.set(p.newBytes, p.offset);
      }
      slot.undoStack.push(tx);
      slot.romData = nextRom;
      slot.hashes = computeHashesLocal(nextRom);
      browserRevision += 1;
      const selectedMap = state.maps.find((m) => m.id === state.selectedMapId);
      const nextMatrix = selectedMap
        ? extractMatrixFromRom(nextRom, selectedMap)
        : state.activeMatrix;
      set({
        romData: nextRom,
        hashes: slot.hashes,
        checksumStatus: null,
        romRevision: browserRevision,
        undoStatus: toBrowserUndoStatus(slot, browserRevision),
        romDocuments: toBrowserSummaries(slot.id, browserRevision),
        activeMatrix: nextMatrix,
      });
      return true;
    });
  },

  verifyChecksum: async () => {
    const state = get();
    if (!state.romLoaded && !state.romData) return false;
    const mySeq = ++checksumSeq;
    const myDocId = state.activeDocId;
    const myDocSeq = docOpSeq;

    if (isDesktopApp()) {
      try {
        const oemReports = await bridgeAutoDetectOemChecksums(state.romData);
        if (mySeq !== checksumSeq || myDocSeq !== docOpSeq || get().activeDocId !== myDocId) {
          return false;
        }
        if (oemReports && oemReports.length > 0) {
          const allOk = oemReports.every((r) => r.all_ok);
          const primary =
            oemReports.find((r) => !r.all_ok) ??
            oemReports.find((r) => r.algorithm_id === state.checksumAlgorithmId) ??
            oemReports[0];
          set({
            checksumStatus: allOk ? 'valid' : 'invalid',
            checksumAlgorithmId: primary.algorithm_id,
            checksumAlgorithmName: primary.algorithm_name,
            checksumNotice: allOk
              ? `${primary.algorithm_name} valid`
              : `${primary.algorithm_name} mismatch`,
          });
          if (isDesktopApp()) saveDesktopDocMeta(get());
          return allOk;
        }
      } catch {
        // Fall back cleanly to standard Keihin check below
      }
    }

    const valid = await bridgeVerifyChecksum(state.romData);
    if (mySeq !== checksumSeq || myDocSeq !== docOpSeq || get().activeDocId !== myDocId) {
      return valid;
    }
    set({
      checksumStatus: valid ? 'valid' : 'invalid',
      checksumAlgorithmId: valid ? 'keihin_8bit' : null,
      checksumAlgorithmName: valid ? 'Keihin 8-bit' : null,
      checksumNotice: valid
        ? 'Keihin 8-bit checksum valid'
        : isDesktopApp()
          ? 'Keihin 8-bit check failed and no OEM rule matched (open Checksums Workbench)'
          : 'Browser mode validates Keihin 8-bit checksums only',
    });
    return valid;
  },

  fixChecksum: async () => {
    const targetDocId = get().activeDocId;
    const targetDocSeq = docOpSeq;
    const mySeq = ++checksumSeq;

    return await enqueueWrite(async () => {
      if (docOpSeq !== targetDocSeq || get().activeDocId !== targetDocId) return false;
      const state = get();
      if (!state.romLoaded && !state.romData) return false;

      if (isDesktopApp()) {
        let algId = state.checksumAlgorithmId;
        let algName = state.checksumAlgorithmName;
        if (!algId || algId === 'keihin_8bit') {
          try {
            const oemReports = await bridgeAutoDetectOemChecksums(state.romData);
            if (oemReports && oemReports.length > 0) {
              const target = oemReports.find((r) => !r.all_ok) ?? oemReports[0];
              algId = target.algorithm_id;
              algName = target.algorithm_name;
            }
          } catch {
            // Fall through to Keihin repair
          }
        }

        if (algId && algId !== 'keihin_8bit') {
          const payload = await bridgeRepairOemChecksum(algId, state.romData, true);
          if (
            mySeq !== checksumSeq ||
            docOpSeq !== targetDocSeq ||
            get().activeDocId !== targetDocId
          ) {
            return false;
          }
          if (payload && payload.report.all_ok) {
            const undoStatus = await bridgeGetUndoStatus();
            const summary = await bridgeGetRomSessionSummary();
            if (docOpSeq !== targetDocSeq || get().activeDocId !== targetDocId) return false;
            set((s) => ({
              checksumStatus: 'valid',
              checksumAlgorithmId: payload.report.algorithm_id,
              checksumAlgorithmName: payload.report.algorithm_name,
              checksumNotice: `${algName ?? payload.report.algorithm_name} repaired`,
              undoStatus,
              hashes: summary?.hashes ?? s.hashes,
              romDocuments: summary?.documents ?? s.romDocuments,
              romRevision: s.romRevision + 1,
            }));
            saveDesktopDocMeta(get());
            return true;
          }
        }
      }

      const prevRom = state.romData;
      const res = await bridgeFixChecksum(prevRom);
      if (
        mySeq !== checksumSeq ||
        docOpSeq !== targetDocSeq ||
        get().activeDocId !== targetDocId
      ) {
        return false;
      }
      if (res.ok) {
        if (isDesktopApp()) {
          const undoStatus = await bridgeGetUndoStatus();
          if (docOpSeq !== targetDocSeq || get().activeDocId !== targetDocId) return false;
          set((s) => ({
            checksumStatus: 'valid',
            checksumAlgorithmId: 'keihin_8bit',
            checksumAlgorithmName: 'Keihin 8-bit',
            checksumNotice: 'Keihin 8-bit checksum repaired',
            undoStatus,
            romRevision: s.romRevision + 1,
          }));
          saveDesktopDocMeta(get());
          return true;
        }

        const nextRom = res.data;
        const slot = getActiveBrowserSlot(targetDocId);
        let hashes = state.hashes;
        if (nextRom) {
          browserRevision += 1;
          hashes = computeHashesLocal(nextRom);
          if (slot && prevRom) {
            pushBrowserUndo(slot, 'Fix Keihin Checksum', diffByteSlices(prevRom, nextRom));
            slot.romData = nextRom;
            slot.hashes = hashes;
          }
        }
        set((s) => ({
          ...(nextRom ? { romData: nextRom, hashes } : {}),
          checksumStatus: 'valid',
          checksumAlgorithmId: 'keihin_8bit',
          checksumAlgorithmName: 'Keihin 8-bit',
          checksumNotice: 'Keihin 8-bit checksum repaired',
          undoStatus: slot ? toBrowserUndoStatus(slot, browserRevision) : s.undoStatus,
          romDocuments: toBrowserSummaries(targetDocId, browserRevision),
          romRevision: browserRevision || s.romRevision + 1,
        }));
        return true;
      }

      set({
        checksumStatus: 'invalid',
        checksumNotice: isDesktopApp()
          ? 'Auto checksum repair did not match a layout (open Checksums Workbench)'
          : 'Browser mode repairs Keihin 8-bit checksums only',
      });
      return false;
    });
  },

  exportRom: async () => {
    const state = get();
    return await bridgeSaveBinData(state.romData);
  },

  openRomDocument: async (data, meta) => {
    clearMatrixCache();
    const myDocSeq = ++docOpSeq;
    extractSeq += 1;
    checksumSeq += 1;
    if (isDesktopApp()) {
      saveDesktopDocMeta(get());
      const summary = await bridgeOpenRomDocumentRaw(data, meta);
      if (myDocSeq !== docOpSeq || !summary) return;
      const activeDoc = summary.documents.find((d) => d.id === summary.active_doc_id) ?? null;
      set((s) => ({
        ...EMPTY_ROM_FIELDS,
        ...applySessionSummaryState(summary, null),
        romRevision: s.romRevision + 1,
        recentRoms: recordRecentRom(s.recentRoms, {
          label: meta?.label ?? activeDoc?.label ?? 'ROM',
          filePath: meta?.path ?? activeDoc?.file_path ?? null,
          size: summary.size,
          sha256: summary.hashes?.sha256 ?? activeDoc?.sha256 ?? '',
          loadedAt: Date.now(),
        }),
      }));
      return;
    }

    const hashes = computeHashesLocal(data);
    const originalSnap = new Uint8Array(data);
    browserRevision += 1;
    browserDocSeq += 1;
    const id = meta?.docId ?? `rom-${browserDocSeq}`;
    const label = meta?.label ?? `ROM ${browserDocSeq}`;
    const slot: BrowserDocSlot = {
      id,
      label,
      filePath: meta?.path ?? null,
      romData: data,
      originalRomData: originalSnap,
      hashes,
      maps: [],
      selectedMapId: null,
      baseAddress: null,
      checksumAlgorithmId: null,
      checksumAlgorithmName: null,
      undoStack: [],
      redoStack: [],
    };
    browserDocs = [...browserDocs.filter((d) => d.id !== id), slot];
    set((s) => ({
      ...EMPTY_ROM_FIELDS,
      romLoaded: true,
      romSize: data.length,
      romData: data,
      originalRomData: originalSnap,
      hashes,
      romRevision: browserRevision,
      undoStatus: toBrowserUndoStatus(slot, browserRevision),
      romDocuments: toBrowserSummaries(slot.id, browserRevision),
      activeDocId: slot.id,
      recentRoms: recordRecentRom(s.recentRoms, {
        label,
        filePath: meta?.path ?? null,
        size: data.length,
        sha256: hashes.sha256,
        loadedAt: Date.now(),
      }),
    }));
  },

  openRomDocumentFile: async (path, meta) => {
    if (!isDesktopApp()) return;
    clearMatrixCache();
    const myDocSeq = ++docOpSeq;
    extractSeq += 1;
    checksumSeq += 1;
    saveDesktopDocMeta(get());
    const summary = await bridgeOpenRomDocumentFile(path, meta);
    if (myDocSeq !== docOpSeq || !summary) return;
    const activeDoc = summary.documents.find((d) => d.id === summary.active_doc_id) ?? null;
    set((s) => ({
      ...EMPTY_ROM_FIELDS,
      ...applySessionSummaryState(summary, null),
      romRevision: s.romRevision + 1,
      recentRoms: recordRecentRom(s.recentRoms, {
        label: meta?.label ?? activeDoc?.label ?? path.split(/[\\/]/).pop() ?? 'ROM',
        filePath: path,
        size: summary.size,
        sha256: summary.hashes?.sha256 ?? activeDoc?.sha256 ?? '',
        loadedAt: Date.now(),
      }),
    }));
  },

  switchRomDocument: async (docId) => {
    clearMatrixCache();
    const myDocSeq = ++docOpSeq;
    extractSeq += 1;
    checksumSeq += 1;
    if (isDesktopApp()) {
      saveDesktopDocMeta(get());
      const summary = await bridgeSwitchRomDocument(docId);
      if (myDocSeq !== docOpSeq || !summary) return;
      const savedMeta = summary.active_doc_id ? desktopDocMeta.get(summary.active_doc_id) : undefined;
      set((s) => ({
        ...EMPTY_ROM_FIELDS,
        ...applySessionSummaryState(summary, savedMeta?.selectedMapId ?? null),
        maps: savedMeta?.maps ?? [],
        checksumAlgorithmId: savedMeta?.checksumAlgorithmId ?? null,
        checksumAlgorithmName: savedMeta?.checksumAlgorithmName ?? null,
        romRevision: s.romRevision + 1,
      }));
      return;
    }
    const slot = browserDocs.find((d) => d.id === docId);
    if (!slot) return;
    browserRevision += 1;
    set({
      ...EMPTY_ROM_FIELDS,
      maps: slot.maps,
      selectedMapId: slot.selectedMapId,
      romLoaded: true,
      romSize: slot.romData.length,
      romData: slot.romData,
      originalRomData: slot.originalRomData,
      hashes: slot.hashes,
      checksumAlgorithmId: slot.checksumAlgorithmId,
      checksumAlgorithmName: slot.checksumAlgorithmName,
      romRevision: browserRevision,
      undoStatus: toBrowserUndoStatus(slot, browserRevision),
      romDocuments: toBrowserSummaries(slot.id, browserRevision),
      activeDocId: slot.id,
    });
    if (slot.baseAddress !== null) {
      get().setRomBaseAddress(slot.baseAddress);
    }
  },

  closeRomDocument: async (docId) => {
    clearMatrixCache();
    const myDocSeq = ++docOpSeq;
    extractSeq += 1;
    checksumSeq += 1;
    if (isDesktopApp()) {
      desktopDocMeta.delete(docId);
      const summary = await bridgeCloseRomDocument(docId);
      if (myDocSeq !== docOpSeq || !summary) return;
      const savedMeta = summary.active_doc_id ? desktopDocMeta.get(summary.active_doc_id) : undefined;
      set((s) => ({
        ...EMPTY_ROM_FIELDS,
        ...applySessionSummaryState(summary, savedMeta?.selectedMapId ?? null),
        maps: savedMeta?.maps ?? [],
        checksumAlgorithmId: savedMeta?.checksumAlgorithmId ?? null,
        checksumAlgorithmName: savedMeta?.checksumAlgorithmName ?? null,
        romRevision: s.romRevision + 1,
      }));
      return;
    }
    browserDocs = browserDocs.filter((d) => d.id !== docId);
    browserRevision += 1;
    if (browserDocs.length === 0) {
      set({ ...EMPTY_ROM_FIELDS, romRevision: browserRevision });
      return;
    }
    const next = browserDocs[browserDocs.length - 1];
    set({
      ...EMPTY_ROM_FIELDS,
      maps: next.maps,
      selectedMapId: next.selectedMapId,
      romLoaded: true,
      romSize: next.romData.length,
      romData: next.romData,
      originalRomData: next.originalRomData,
      hashes: next.hashes,
      romBaseAddress: next.baseAddress,
      checksumAlgorithmId: next.checksumAlgorithmId,
      checksumAlgorithmName: next.checksumAlgorithmName,
      romRevision: browserRevision,
      undoStatus: toBrowserUndoStatus(next, browserRevision),
      romDocuments: toBrowserSummaries(next.id, browserRevision),
      activeDocId: next.id,
    });
  },

  closeActiveRom: async () => {
    clearMatrixCache();
    const activeId = get().activeDocId;
    if (activeId) {
      await get().closeRomDocument(activeId);
      return;
    }
    docOpSeq += 1;
    extractSeq += 1;
    checksumSeq += 1;
    set((s) => ({ ...EMPTY_ROM_FIELDS, romRevision: s.romRevision + 1 }));
  },

  renameRomDocument: async (docId, label) => {
    if (isDesktopApp()) {
      const summary = await bridgeRenameRomDocument(docId, label);
      if (summary) {
        set({
          romDocuments: summary.documents,
        });
      }
      return;
    }
    const slot = browserDocs.find((d) => d.id === docId);
    if (!slot) return;
    slot.label = label.trim() || slot.label;
    set((s) => ({
      romDocuments: toBrowserSummaries(s.activeDocId, s.romRevision),
    }));
  },

  refreshRomSession: async () => {
    if (!isDesktopApp()) return;
    clearMatrixCache();
    const myDocSeq = docOpSeq;
    const summary = await bridgeGetRomSessionSummary();
    if (myDocSeq !== docOpSeq || !summary) return;
    set((s) => ({
      ...applySessionSummaryState(summary, s.selectedMapId),
      romRevision: s.romRevision + 1,
    }));
  },

  clearRecentRoms: () => set({ recentRoms: [] }),

  reset: () => {
    clearMatrixCache();
    docOpSeq += 1;
    extractSeq += 1;
    checksumSeq += 1;
    browserDocs = [];
    browserRevision = 0;
    desktopDocMeta.clear();
    set({ ...EMPTY_ROM_FIELDS, recentRoms: [] });
  },
}));
