import test from 'node:test';
import assert from 'node:assert/strict';
import { computeRomDeltaLedger } from './deltaLedgerModel.ts';

test('computeRomDeltaLedger returns empty summary for identical or missing buffers', () => {
  const base = new Uint8Array([1, 2, 3, 4]);
  assert.equal(computeRomDeltaLedger(null, base).totalChangedBytes, 0);
  assert.equal(computeRomDeltaLedger(base, new Uint8Array([1, 2, 3, 4])).totalChangedBytes, 0);
});

test('computeRomDeltaLedger groups nearby changes within gap <= 4 bytes and caps clusters', () => {
  const orig = new Uint8Array(64).fill(0x00);
  const cur = new Uint8Array(64).fill(0x00);
  cur[10] = 0x11;
  cur[13] = 0x22; // gap = 3 -> merges into cluster 10..13
  cur[40] = 0xaa; // gap > 4 -> separate cluster 40..40

  const ledger = computeRomDeltaLedger(cur, orig, 12);
  assert.equal(ledger.totalChangedBytes, 3);
  assert.equal(ledger.totalClusterCount, 2);
  assert.equal(ledger.truncated, false);
  assert.deepEqual(ledger.clusters[0], {
    start: 10,
    end: 13,
    changedBytes: 2,
    firstOld: 0x00,
    firstNew: 0x11,
  });
  assert.equal(ledger.clusters[1].start, 40);

  const capped = computeRomDeltaLedger(cur, orig, 1);
  assert.equal(capped.totalClusterCount, 2);
  assert.equal(capped.clusters.length, 1);
  assert.equal(capped.truncated, true);
});
