/**
 * B4a.1: browser support for user-specified ROM base addresses.
 *
 * ## Why the address lives in `useMapStore`
 *
 * The address is **not** backend state. `compare_rom_binaries_with_address`
 * takes `base_addr` as a parameter and never reads `MemoryMap.base_address`, so
 * storing it in the canonical frontend ROM state duplicates nothing — it makes
 * the desktop and browser paths symmetric instead of asymmetric.
 *
 * ## Why `romData` alone is enough
 *
 * `loadRomBuffer` is the single writer of `romData`, and it overwrites it on
 * every load. That is exactly the invalidation point: an address set for ROM A
 * cannot survive ROM B, because the store rewrites both fields together.
 */

import test from 'node:test';
import assert from 'node:assert/strict';
import { useMapStore } from './useMapStore.ts';

function freshStore() {
  useMapStore.getState().reset();
  return useMapStore;
}

test('a freshly loaded ROM has an unknown base address', () => {
  const store = freshStore();
  store.getState().loadRomBufferSync(new Uint8Array([1, 2, 3]));
  assert.strictEqual(store.getState().romBaseAddress, null);
});

test('a known zero address is distinct from unknown', () => {
  const store = freshStore();
  store.getState().loadRomBufferSync(new Uint8Array([1, 2, 3]));

  assert.strictEqual(store.getState().romBaseAddress, null);
  store.getState().setRomBaseAddress(0);
  assert.strictEqual(
    store.getState().romBaseAddress,
    0,
    'explicit zero must be stored as 0, not collapsed to null'
  );
});

test('a known nonzero address round-trips', () => {
  const store = freshStore();
  store.getState().loadRomBufferSync(new Uint8Array([1, 2, 3]));
  store.getState().setRomBaseAddress(0x8000);
  assert.strictEqual(store.getState().romBaseAddress, 0x8000);
});

test('the address can be cleared back to unknown', () => {
  const store = freshStore();
  store.getState().loadRomBufferSync(new Uint8Array([1, 2, 3]));
  store.getState().setRomBaseAddress(0x4000);
  store.getState().setRomBaseAddress(null);
  assert.strictEqual(store.getState().romBaseAddress, null);
});

test('loading a different ROM clears the previous address', () => {
  const store = freshStore();
  store.getState().loadRomBufferSync(new Uint8Array([1, 2, 3]));
  store.getState().setRomBaseAddress(0x8000);

  store.getState().loadRomBufferSync(new Uint8Array([9, 9, 9, 9]));

  assert.strictEqual(
    store.getState().romBaseAddress,
    null,
    'an address stated for one ROM must never describe another'
  );
});

test('reset clears the address', () => {
  const store = freshStore();
  store.getState().loadRomBufferSync(new Uint8Array([1]));
  store.getState().setRomBaseAddress(0x1000);

  store.getState().reset();

  assert.strictEqual(store.getState().romBaseAddress, null);
  assert.strictEqual(store.getState().romData, null);
});

test('setRomBaseAddress without a loaded ROM is refused', () => {
  const store = freshStore();
  store.getState().setRomBaseAddress(0x1000);
  assert.strictEqual(
    store.getState().romBaseAddress,
    null,
    'there is no ROM to attach an address to'
  );
});
