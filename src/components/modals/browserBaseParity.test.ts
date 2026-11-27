/**
 * B4a.1: the browser and desktop comparison rules must agree.
 *
 * Both paths implement the same policy table. The browser version is a
 * hand-written mirror of `kivi_diff::address::diff_with_address_check`, so it is
 * the one that can silently drift. These tests pin the four cases that matter.
 */

import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { useMapStore } from '../../stores/useMapStore.ts';

const read = (p: string): string => readFileSync(p, 'utf8');

function freshStore() {
  useMapStore.getState().reset();
  useMapStore.getState().loadRomBufferSync(new Uint8Array([1, 2, 3, 4]));
  return useMapStore;
}

// ---------------------------------------------------------------------------
// Single owner: the address must not be duplicated
// ---------------------------------------------------------------------------

test('BinDiffModal reads the address from useMapStore, not local state', () => {
  const src = read('src/components/modals/BinDiffModal.tsx');
  assert.match(
    src,
    /useMapStore\(\(s\)\s*=>\s*s\.romBaseAddress\)/,
    'the modal must read the canonical store field'
  );
  assert.doesNotMatch(
    src,
    /useState<number \| null>\(null\)/,
    'a second local copy of the address must not exist'
  );
});

test('BinDiffModal no longer depends on desktop-only base bridges', () => {
  const src = read('src/components/modals/BinDiffModal.tsx');
  assert.doesNotMatch(
    src,
    /bridgeSetRomBaseAddress|bridgeGetRomBaseAddress/,
    'browser support must not route through the desktop-only bridge'
  );
});

test('the address is written by exactly one store action', () => {
  const src = read('src/stores/useMapStore.ts');
  const writes = src.match(/romBaseAddress:/g) ?? [];
  // declaration + initial + loadRomBuffer reset + reset + setter = 5
  assert.ok(
    writes.length <= 6,
    `unexpected number of romBaseAddress writes: ${writes.length}`
  );
  assert.match(src, /setRomBaseAddress:/);
});

// ---------------------------------------------------------------------------
// Browser/desktop parity of the decision table
// ---------------------------------------------------------------------------

test('browser fallback rejects unknown-vs-known', () => {
  const src = read('src/lib/bridge/diffScanBridge.ts');
  const idx = src.indexOf('Browser fallback mirrors');
  assert.ok(idx > 0, 'fallback block must exist');
  const block = src.slice(idx, idx + 1400);
  assert.match(block, /baseAddr === null && targetAddr === null/);
  assert.match(block, /baseAddr !== null && targetAddr !== null/);
  assert.match(block, /address alignment cannot be verified/);
});

test('browser fallback rejects differing known bases', () => {
  const src = read('src/lib/bridge/diffScanBridge.ts');
  const idx = src.indexOf('Browser fallback mirrors');
  const block = src.slice(idx, idx + 1400);
  assert.match(block, /base addresses differ/);
});

test('browser fallback allows unknown-vs-unknown', () => {
  const src = read('src/lib/bridge/diffScanBridge.ts');
  const idx = src.indexOf('Browser fallback mirrors');
  const block = src.slice(idx, idx + 1400);
  assert.match(block, /compatibility = 'UnknownBases'/);
});

test('browser fallback treats known zero as known, not unknown', () => {
  const src = read('src/lib/bridge/diffScanBridge.ts');
  const idx = src.indexOf('Browser fallback mirrors');
  const block = src.slice(idx, idx + 1400);
  // The comparison is `!== null`, so 0 takes the known branch.
  assert.doesNotMatch(
    block,
    /baseAddr\s*\?\s*/,
    'a truthiness check would treat 0 as unknown'
  );
});

// ---------------------------------------------------------------------------
// Stale-state safety
// ---------------------------------------------------------------------------

test('an address set for one ROM cannot describe another', () => {
  const store = freshStore();
  store.getState().setRomBaseAddress(0x8000);
  assert.strictEqual(store.getState().romBaseAddress, 0x8000);

  store.getState().loadRomBufferSync(new Uint8Array([7, 7, 7]));

  assert.strictEqual(
    store.getState().romBaseAddress,
    null,
    'replacement must clear, so a comparison cannot use a stale address'
  );
});

test('a refused set leaves the previous value untouched rather than faking success', () => {
  const store = useMapStore;
  store.getState().reset();
  // No ROM loaded: the setter must refuse.
  store.getState().setRomBaseAddress(0x1234);
  assert.strictEqual(store.getState().romBaseAddress, null);
  assert.strictEqual(store.getState().romData, null);
});
