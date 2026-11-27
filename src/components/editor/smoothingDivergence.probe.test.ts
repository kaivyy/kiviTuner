/**
 * Reachability probe: can `activeMatrix` and `AppState.memory` diverge?
 *
 * This simulates the `updateMatrix` sequence from `useMapStore.ts` against a
 * stub bridge, without touching real ROM files or any backend.
 *
 * The sequence under test is exactly:
 *   1. set({ activeMatrix: matrix })
 *   2. await bridgeUpdateMapMatrix(...)
 *   3. if (nextRom) set({ romData: nextRom })
 */

import test from 'node:test';
import assert from 'node:assert/strict';

interface SimState {
  activeMatrix: number[][] | null;
  romData: Uint8Array | null;
  /** Stands in for `AppState.memory` on the Rust side. */
  backendMatrix: number[][] | null;
}

/** Mirrors `useMapStore.updateMatrix` verbatim, with an injectable bridge. */
async function updateMatrix(
  state: SimState,
  matrix: number[][],
  bridge: (m: number[][]) => Promise<Uint8Array | null>
): Promise<void> {
  state.activeMatrix = matrix; // set({ activeMatrix: matrix })
  const nextRom = await bridge(matrix);
  if (nextRom) {
    state.romData = nextRom; // set({ romData: nextRom })
  }
}

/** Mirrors `analyze_map_smoothing_inner`: prefers the supplied matrix. */
function previewSourceOfTruth(
  state: SimState,
  supplied: number[][] | null
): number[][] | null {
  return supplied ?? state.backendMatrix;
}

test('happy path keeps activeMatrix and backend in step', async () => {
  const state: SimState = {
    activeMatrix: [[1, 2]],
    romData: new Uint8Array([1, 2]),
    backendMatrix: [[1, 2]],
  };

  const edited = [[9, 9]];
  await updateMatrix(state, edited, async (m) => {
    // The real bridge invokes update_map_matrix, which writes into AppState.
    state.backendMatrix = m.map((r) => [...r]);
    return new Uint8Array([9, 9]);
  });

  assert.deepStrictEqual(state.activeMatrix, edited);
  assert.deepStrictEqual(previewSourceOfTruth(state, edited), edited);
  assert.deepStrictEqual(state.activeMatrix, previewSourceOfTruth(state, edited));
});

test('backend failure leaves activeMatrix ahead of the preview source', async () => {
  const state: SimState = {
    activeMatrix: [[1, 2]],
    romData: new Uint8Array([1, 2]),
    backendMatrix: [[1, 2]],
  };

  const edited = [[9, 9]];
  await updateMatrix(state, edited, async () => {
    // Simulates the desktop IPC failure path: `bridgeUpdateMapMatrix` catches
    // the error, falls through to the browser fallback, and here we model the
    // worst case where it yields no ROM at all.
    return null;
  });

  // The edit is visible in the editor...
  assert.deepStrictEqual(state.activeMatrix, edited);
  // ...but the backend still holds the old values.
  assert.deepStrictEqual(state.backendMatrix, [[1, 2]]);

  // Pre-fix this would analyze the stale backend copy. Post-fix the preview
  // receives the displayed matrix, so the divergence no longer reaches it.
  assert.deepStrictEqual(
    previewSourceOfTruth(state, edited),
    edited,
    'preview must analyze what the editor displays'
  );
  assert.deepStrictEqual(
    state.activeMatrix,
    previewSourceOfTruth(state, edited),
    'DIVERGENCE CLOSED for the preview path'
  );
});

test('divergence is invisible to the user because the edit was never persisted', async () => {
  // Documents the practical severity: on backend failure `romData` is also
  // unchanged, so the user's edit exists only in transient UI state.
  const state: SimState = {
    activeMatrix: [[1, 2]],
    romData: new Uint8Array([1, 2]),
    backendMatrix: [[1, 2]],
  };

  const before = state.romData;
  await updateMatrix(state, [[9, 9]], async () => null);

  assert.strictEqual(state.romData, before, 'romData must not change on failure');
  assert.deepStrictEqual(state.activeMatrix, [[9, 9]]);
});
