import test from 'node:test';
import assert from 'node:assert/strict';
import { bridgeCompareRomBinaries, bridgeConvertDiffToMap } from '../../lib/tauriBridge.ts';

test('binary diff: clusters modified calibration table, axis, and scalar regions', async () => {
  const base = new Uint8Array(8192).fill(100);
  const target = new Uint8Array(base);

  // Modified 16x8 fuel table @0x1000
  for (let i = 0; i < 128; i++) target[0x1000 + i] = 112 + (i % 10);
  // Modified 1D axis @0x1400 (16 bytes)
  for (let i = 0; i < 16; i++) target[0x1400 + i] = 140 + i;
  // Modified 2-byte scalar @0x0080
  target[0x0080] = 0x42;
  target[0x0081] = 0x24;

  const diffs = await bridgeCompareRomBinaries(base, target, 4);
  assert.ok(diffs.length >= 3, `Expected at least 3 diff regions, got ${diffs.length}`);

  const tableDiff = diffs.find((d) => d.classification.type === 'Table2D');
  assert.ok(tableDiff, 'Should detect Table2D region');

  const mapDef = await bridgeConvertDiffToMap(tableDiff);
  assert.ok(mapDef.name.includes('Table'));
  assert.strictEqual(mapDef.category, 'Diff Extract');
});
