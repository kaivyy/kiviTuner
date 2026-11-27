import test from 'node:test';
import assert from 'node:assert/strict';
import { computeSmoothingPreview, isPreviewRenderable } from './smoothingPreview.ts';

const SPIKE = [
  [10, 10, 10],
  [10, 50, 10],
  [10, 10, 10],
];

test('rejects alpha = 0', async () => {
  const s = await computeSmoothingPreview('map_1', SPIKE, 0);
  assert.strictEqual(s.kind, 'invalid-alpha');
});

test('rejects alpha = 1', async () => {
  const s = await computeSmoothingPreview('map_1', SPIKE, 1);
  assert.strictEqual(s.kind, 'invalid-alpha');
});

test('rejects NaN alpha instead of returning a NaN matrix', async () => {
  const s = await computeSmoothingPreview('map_1', SPIKE, NaN);
  assert.strictEqual(s.kind, 'invalid-alpha');
});

test('rejects positive and negative infinity', async () => {
  assert.strictEqual((await computeSmoothingPreview('map_1', SPIKE, Infinity)).kind, 'invalid-alpha');
  assert.strictEqual((await computeSmoothingPreview('map_1', SPIKE, -Infinity)).kind, 'invalid-alpha');
});

test('rejects null matrix as no-map', async () => {
  const s = await computeSmoothingPreview('map_1', null, 0.5);
  assert.strictEqual(s.kind, 'no-map');
});

test('rejects empty matrix as no-map', async () => {
  const s = await computeSmoothingPreview('map_1', [], 0.5);
  assert.strictEqual(s.kind, 'no-map');
});

test('returns verified smoothed value and curvature', async () => {
  const s = await computeSmoothingPreview('map_1', SPIKE, 0.5);
  assert.strictEqual(s.kind, 'ok');
  // Derived from the Rust algorithm: (1-0.5)*50 + 0.5*mean(10,10,10,10) = 30
  assert.strictEqual(s.report!.smoothed[1][1], 30);
  // Laplacian: 10+10+10+10 - 4*50 = -160
  assert.strictEqual(s.report!.laplacian[1][1], -160);
  assert.strictEqual(s.report!.max_abs_curvature, 160);
  assert.strictEqual(s.report!.max_curvature_row, 1);
  assert.strictEqual(s.report!.max_curvature_col, 1);
});

test('preserves the original matrix in the report', async () => {
  const s = await computeSmoothingPreview('map_1', SPIKE, 0.5);
  assert.strictEqual(s.kind, 'ok');
  assert.deepStrictEqual(s.report!.original, SPIKE);
});

test('does not mutate the input matrix', async () => {
  const snapshot = JSON.stringify(SPIKE);
  await computeSmoothingPreview('map_1', SPIKE, 0.5);
  assert.strictEqual(JSON.stringify(SPIKE), snapshot);
});

test('returned smoothed matrix matches input dimensions', async () => {
  const s = await computeSmoothingPreview('map_1', SPIKE, 0.5);
  assert.strictEqual(s.kind, 'ok');
  assert.strictEqual(s.report!.smoothed.length, SPIKE.length);
  assert.strictEqual(s.report!.smoothed[0].length, SPIKE[0].length);
  assert.strictEqual(s.report!.rows, 3);
  assert.strictEqual(s.report!.cols, 3);
});

test('rejects a report containing non-finite values', async () => {
  // A NaN-producing alpha would land here if validation were bypassed.
  const s = await computeSmoothingPreview('map_1', [[NaN, 1], [2, 3]], 0.5);
  assert.strictEqual(s.kind, 'error');
  assert.match(s.message!, /finite/i);
});

test('isPreviewRenderable rejects non-finite matrices', () => {
  assert.strictEqual(isPreviewRenderable([[1, 2], [3, 4]]), true);
  assert.strictEqual(isPreviewRenderable([[1, NaN]]), false);
  assert.strictEqual(isPreviewRenderable([[Infinity]]), false);
  assert.strictEqual(isPreviewRenderable([]), false);
});
