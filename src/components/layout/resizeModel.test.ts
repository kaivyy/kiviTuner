import test from 'node:test';
import assert from 'node:assert/strict';
import { computePanelResize, computeSplitRatio } from './resizeModel.ts';

test('computePanelResize clamps bounds and previews auto-collapse below threshold', () => {
  assert.deepEqual(
    computePanelResize({ rawWidth: 320, minWidth: 180, maxWidth: 440, collapseThreshold: 140 }),
    { width: 320, willCollapse: false }
  );
  assert.deepEqual(
    computePanelResize({ rawWidth: 600, minWidth: 180, maxWidth: 440, collapseThreshold: 140 }),
    { width: 440, willCollapse: false }
  );
  assert.deepEqual(
    computePanelResize({ rawWidth: 155, minWidth: 180, maxWidth: 440, collapseThreshold: 140 }),
    { width: 180, willCollapse: false }
  );
  assert.deepEqual(
    computePanelResize({
      rawWidth: 120,
      minWidth: 180,
      maxWidth: 440,
      collapseThreshold: 140,
      minPreviewWidth: 112,
    }),
    { width: 120, willCollapse: true }
  );
  assert.deepEqual(
    computePanelResize({
      rawWidth: 20,
      minWidth: 180,
      maxWidth: 440,
      collapseThreshold: 140,
      minPreviewWidth: 112,
    }),
    { width: 112, willCollapse: true }
  );
});

test('computeSplitRatio clamps within bounds and handles invalid geometry safely', () => {
  assert.strictEqual(
    computeSplitRatio({ pointerPos: 500, containerStart: 100, containerSize: 1000 }),
    0.4
  );
  assert.strictEqual(
    computeSplitRatio({ pointerPos: 120, containerStart: 100, containerSize: 1000 }),
    0.25
  );
  assert.strictEqual(
    computeSplitRatio({ pointerPos: 990, containerStart: 100, containerSize: 1000 }),
    0.75
  );
  assert.strictEqual(
    computeSplitRatio({ pointerPos: 200, containerStart: 100, containerSize: 0 }),
    0.5
  );
});
