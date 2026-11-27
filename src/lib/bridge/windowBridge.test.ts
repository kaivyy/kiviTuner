import test from 'node:test';
import assert from 'node:assert/strict';
import {
  computeWaveformBucketsFromSlice,
  parseWaveformBucketsBinary,
  searchPatternInSlice,
} from './windowBridge.ts';
import {
  buildWindowHexRows,
  parseHexSearchQuery,
} from '../../components/editor/hexdumpModel.ts';
import { useMapStore } from '../../stores/useMapStore.ts';

test('parseWaveformBucketsBinary decodes little-endian header and RGB triplets', () => {
  const buf = new ArrayBuffer(16 + 2 * 3);
  const dv = new DataView(buf);
  dv.setUint32(0, 65536, true);
  dv.setUint32(4, 1024, true);
  dv.setUint32(8, 2048, true);
  dv.setUint32(12, 2, true);
  const body = new Uint8Array(buf, 16);
  body.set([10, 240, 120, 0, 255, 128]);

  const parsed = parseWaveformBucketsBinary(buf);
  assert.strictEqual(parsed.totalLen, 65536);
  assert.strictEqual(parsed.startOffset, 1024);
  assert.strictEqual(parsed.endOffset, 2048);
  assert.deepStrictEqual(parsed.buckets, [
    { min: 10, max: 240, avg: 120 },
    { min: 0, max: 255, avg: 128 },
  ]);
});

test('computeWaveformBucketsFromSlice and searchPatternInSlice behave consistently', () => {
  const rom = new Uint8Array(512).fill(0x20);
  rom[64] = 0xde;
  rom[65] = 0xad;
  rom[250] = 0xde;
  rom[251] = 0xad;
  rom[400] = 0xde;
  rom[401] = 0xad;

  const hits = searchPatternInSlice(rom, new Uint8Array([0xde, 0xad]), 0, 2);
  assert.deepStrictEqual(hits.offsets, [64, 250]);
  assert.strictEqual(hits.truncated, true);

  const buckets = computeWaveformBucketsFromSlice(rom, 64, 128, 8);
  assert.strictEqual(buckets.totalLen, 512);
  assert.strictEqual(buckets.startOffset, 64);
  assert.strictEqual(buckets.endOffset, 128);
  assert.strictEqual(buckets.buckets.length, 8);
  assert.strictEqual(buckets.buckets[0].min, 0x20);
  assert.strictEqual(buckets.buckets[0].max, 0xde);
});

test('hexdumpModel windowed rows and hex/ASCII query parsing work', () => {
  const page = new Uint8Array([0x4b, 0x45, 0x49, 0x48, 0x00, 0xff]);
  const rows = buildWindowHexRows(page, 0x1000, 0x1000, 16);
  assert.strictEqual(rows.length, 1);
  assert.strictEqual(rows[0].offset, 0x1000);
  assert.ok(rows[0].ascii.startsWith('KEIH.'));

  assert.deepStrictEqual(Array.from(parseHexSearchQuery('DE AD BE EF') ?? []), [0xde, 0xad, 0xbe, 0xef]);
  assert.deepStrictEqual(Array.from(parseHexSearchQuery('0xCAFE') ?? []), [0xca, 0xfe]);
  assert.deepStrictEqual(Array.from(parseHexSearchQuery('"KEIH"') ?? []), [0x4b, 0x45, 0x49, 0x48]);
  assert.strictEqual(parseHexSearchQuery('not hex'), null);
});

test('useMapStore browser delta patching, undo/redo, and multi-ROM document switching work', async () => {
  const store = useMapStore.getState();
  store.reset();

  const docA = new Uint8Array(64).fill(0x11);
  await store.openRomDocument(docA, { docId: 'a', label: 'Stock_A.bin' });
  assert.strictEqual(useMapStore.getState().romDocuments.length, 1);
  assert.strictEqual(useMapStore.getState().activeDocId, 'a');

  const ok = await useMapStore.getState().patchRomBytes([{ offset: 4, bytes: [0x99, 0x88] }], 'Patch A');
  assert.strictEqual(ok, true);
  assert.strictEqual(useMapStore.getState().romData?.[4], 0x99);
  assert.strictEqual(useMapStore.getState().undoStatus.can_undo, true);

  const undone = await useMapStore.getState().undoEdit();
  assert.strictEqual(undone, true);
  assert.strictEqual(useMapStore.getState().romData?.[4], 0x11);
  assert.strictEqual(useMapStore.getState().undoStatus.can_redo, true);

  const redone = await useMapStore.getState().redoEdit();
  assert.strictEqual(redone, true);
  assert.strictEqual(useMapStore.getState().romData?.[4], 0x99);

  const docB = new Uint8Array(128).fill(0x22);
  await useMapStore.getState().openRomDocument(docB, { docId: 'b', label: 'Tuned_B.bin' });
  assert.strictEqual(useMapStore.getState().romDocuments.length, 2);
  assert.strictEqual(useMapStore.getState().romData?.length, 128);

  await useMapStore.getState().switchRomDocument('a');
  assert.strictEqual(useMapStore.getState().romData?.length, 64);
  assert.strictEqual(useMapStore.getState().romData?.[4], 0x99);

  await useMapStore.getState().closeRomDocument('a');
  assert.strictEqual(useMapStore.getState().activeDocId, 'b');
  assert.strictEqual(useMapStore.getState().romDocuments.length, 1);
  useMapStore.getState().reset();
});
