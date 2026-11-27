import test from 'node:test';
import assert from 'node:assert/strict';
import { useMapStore } from './useMapStore.ts';
import type { MapDef } from '../types/kivi.ts';

function sampleMap(overrides: Partial<MapDef> = {}): MapDef {
  return {
    id: 'm1',
    name: 'Main Fuel',
    category: 'Fuel',
    address: 0x10,
    rows: 2,
    cols: 2,
    dtype: 'u8',
    endian: 'big',
    scaling: { equation: 'X', unit: 'raw', decimals: 0 },
    ...overrides,
  };
}

test('useMapStore keeps stock originalRomData and baseMatrix immutable across edits and reverts cleanly', async () => {
  const store = useMapStore.getState();
  store.reset();

  const initial = new Uint8Array(64);
  initial[0x10] = 10;
  initial[0x11] = 20;
  initial[0x12] = 30;
  initial[0x13] = 40;

  await store.loadRomBuffer(initial, undefined, { label: 'stock_base.bin' });
  const m = sampleMap();
  store.setMaps([m]);
  store.selectMap(m.id);

  await useMapStore.getState().extractMatrix(m);
  assert.deepStrictEqual(useMapStore.getState().baseMatrix, [
    [10, 20],
    [30, 40],
  ]);

  // Edit cells via serialized patchActiveMapCells and updateMatrix
  await useMapStore.getState().patchActiveMapCells(m, [{ row: 0, col: 0, value: 99 }], 'Boost edit');
  await useMapStore.getState().extractMatrix(m);

  // Live matrix changed, but stock baseMatrix stays 10/20/30/40!
  assert.deepStrictEqual(useMapStore.getState().activeMatrix, [
    [99, 20],
    [30, 40],
  ]);
  assert.deepStrictEqual(useMapStore.getState().baseMatrix, [
    [10, 20],
    [30, 40],
  ]);
  assert.strictEqual(useMapStore.getState().originalRomData?.[0x10], 10);

  // Revert single selection cell back to stock
  const didRevert = await useMapStore.getState().revertMapToOriginal(m, {
    minR: 0,
    maxR: 0,
    minC: 0,
    maxC: 0,
  });
  assert.strictEqual(didRevert, true);
  assert.deepStrictEqual(useMapStore.getState().activeMatrix, [
    [10, 20],
    [30, 40],
  ]);

  // Verify Undo reverts the revert itself
  await useMapStore.getState().undoEdit();
  assert.deepStrictEqual(useMapStore.getState().activeMatrix, [
    [99, 20],
    [30, 40],
  ]);
});

test('useMapStore serializes concurrent patchActiveMapCells writes without dropping mutations', async () => {
  const store = useMapStore.getState();
  store.reset();

  const initial = new Uint8Array(64);
  initial[0x10] = 1;
  initial[0x11] = 2;
  initial[0x12] = 3;
  initial[0x13] = 4;

  await store.loadRomBuffer(initial, undefined, { label: 'concurrent.bin' });
  const m = sampleMap();
  store.setMaps([m]);
  store.selectMap(m.id);
  await useMapStore.getState().extractMatrix(m);

  await Promise.all([
    useMapStore.getState().patchActiveMapCells(m, [{ row: 0, col: 0, value: 11 }], 'Edit 0,0'),
    useMapStore.getState().patchActiveMapCells(m, [{ row: 0, col: 1, value: 22 }], 'Edit 0,1'),
    useMapStore.getState().patchActiveMapCells(m, [{ row: 1, col: 1, value: 44 }], 'Edit 1,1'),
  ]);

  const rom = useMapStore.getState().romData!;
  assert.strictEqual(rom[0x10], 11);
  assert.strictEqual(rom[0x11], 22);
  assert.strictEqual(rom[0x12], 3);
  assert.strictEqual(rom[0x13], 44);
  assert.strictEqual(useMapStore.getState().undoStatus.undo_count, 3);
});

test('useMapStore tracks recentRoms, addMaps, updateMapDef, deleteMapDef, and closeActiveRom cleanly', async () => {
  const store = useMapStore.getState();
  store.reset();

  const romA = new Uint8Array(32).fill(5);
  await store.loadRomBuffer(romA, undefined, { label: 'alpha.bin' });
  assert.strictEqual(useMapStore.getState().recentRoms.length, 1);
  assert.strictEqual(useMapStore.getState().recentRoms[0].label, 'alpha.bin');

  const m1 = sampleMap({ id: 'a1', address: 0 });
  const m2 = sampleMap({ id: 'a2', address: 4, name: 'Spark' });
  useMapStore.getState().addMaps([m1, m2]);
  assert.strictEqual(useMapStore.getState().maps.length, 2);

  useMapStore.getState().selectMap('a1');
  await useMapStore.getState().updateMapDef({ ...m1, scaling: { equation: 'X * 2', unit: 'deg', decimals: 1 } });
  assert.deepStrictEqual(useMapStore.getState().activeMatrix, [
    [10, 10],
    [10, 10],
  ]);

  useMapStore.getState().deleteMapDef('a1');
  assert.strictEqual(useMapStore.getState().maps.length, 1);
  assert.strictEqual(useMapStore.getState().selectedMapId, 'a2');

  await useMapStore.getState().closeActiveRom();
  assert.strictEqual(useMapStore.getState().romLoaded, false);
  // Recent ROM history persists across closing active document
  assert.strictEqual(useMapStore.getState().recentRoms.length, 1);
});
