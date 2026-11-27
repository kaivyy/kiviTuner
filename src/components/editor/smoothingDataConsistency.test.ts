/**
 * Data-consistency audit for the smoothing preview (Batch B1 follow-up).
 *
 * ## Question
 *
 * The desktop preview sends only `{ mapId, alpha }` to Rust, which reads the
 * matrix from `AppState.memory`. The editor displays `activeMatrix`. Can these
 * two diverge during a normal edit?
 *
 * ## Verified data flow (traced in source, not inferred)
 *
 * 1. `HeatmapTable` cell edit -> `onChangeMatrix(newMatrix)`
 * 2. `App.handleChangeMatrix` -> `useMapStore.updateMatrix(mapDef, matrix)`
 * 3. `updateMatrix`:
 *      a. `set({ activeMatrix: matrix })`            <-- frontend first
 *      b. `await bridgeUpdateMapMatrix(...)`         <-- backend second
 *      c. `if (nextRom) set({ romData: nextRom })`
 * 4. `bridgeUpdateMapMatrix` (desktop) invokes `update_map_matrix`, which writes
 *    every cell into `AppState.memory` via `MemoryMap::write_value`.
 *
 * So in the **happy path** the two are kept in step, and the preview is
 * consistent. The only window is step (a) before step (b) completes — and if
 * step (b) throws, `activeMatrix` keeps the edit while `AppState` does not.
 *
 * These tests pin that ordering so a future refactor cannot silently invert it.
 */

import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const read = (p: string): string => readFileSync(p, 'utf8');

test('updateMatrix writes activeMatrix before awaiting the backend', () => {
  const src = read('src/stores/useMapStore.ts');
  const body = src.slice(src.indexOf('updateMatrix: async'));
  const setIdx = body.indexOf('set({ activeMatrix: matrix })');
  const awaitIdx = body.indexOf('await bridgeUpdateMapMatrix');
  assert.ok(setIdx >= 0, 'activeMatrix must be set');
  assert.ok(awaitIdx >= 0, 'backend call must exist');
  assert.ok(
    setIdx < awaitIdx,
    'activeMatrix is set BEFORE the backend write completes — this is the divergence window'
  );
});

test('desktop edit path writes the matrix into AppState memory', () => {
  const bridge = read('src/lib/bridge/romBridge.ts');
  assert.match(bridge, /invoke\('update_map_matrix'/);

  const cmd = read('src-tauri/src/commands/rom.rs');
  const inner = cmd.slice(cmd.indexOf('pub fn update_map_matrix_inner'));
  assert.match(
    inner.slice(0, 1500),
    /mem\.write_value/,
    'the Rust command must write cells into MemoryMap'
  );
});

test('smoothing command reads from AppState, not from a supplied matrix', () => {
  const cmd = read('src-tauri/src/commands/analysis.rs');
  const inner = cmd.slice(cmd.indexOf('pub fn analyze_map_smoothing_inner'));
  assert.match(
    inner.slice(0, 2000),
    /extract_matrix_from_mem|extract_map_matrix_inner/,
    'the command must read the matrix from application state'
  );
});

test('preview controller never receives a mapId/matrix mismatch by construction', () => {
  // HeatmapTable passes its own `matrix` and `tableId` to the same call, so the
  // pair always describes one table.
  const src = read('src/components/editor/HeatmapTable.tsx');
  const call = src.slice(src.indexOf('computeSmoothingPreview('));
  assert.match(call.slice(0, 120), /tableId \?\? ''/);
  assert.match(call.slice(0, 200), /matrix/);
});

test('preview invalidates when the table changes', () => {
  const src = read('src/components/editor/HeatmapTable.tsx');
  const idx = src.indexOf('useEffect(');
  const guard = src.slice(idx, idx + 400);
  assert.match(guard, /\[tableId\]/, 'preview must reset when tableId changes');
  assert.match(guard, /setPreviewState\(null\)/);
});

test('bridge fallback path also writes the matrix into the ROM copy', () => {
  const bridge = read('src/lib/bridge/romBridge.ts');
  assert.match(
    bridge,
    /return romData \? writeMatrixToRom\(romData, mapDef, matrix\) : null/,
    'browser fallback must produce an updated ROM containing the edit'
  );
});
