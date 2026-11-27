import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

/**
 * Batch B1 integration guard.
 *
 * These assert that the two previously-unreachable bridges are now referenced
 * by the editor surface and that the app wires the controls. They are source
 * assertions because this repository has no DOM renderer in its test setup —
 * that limitation is stated rather than worked around with a new dependency.
 */

const read = (p: string): string => readFileSync(p, 'utf8');
const readHeatmap = (): string =>
  [
    'src/components/editor/HeatmapTable.tsx',
    'src/components/editor/heatmap/HeatmapAnalysisBar.tsx',
  ]
    .map(read)
    .join('\n');

test('checksum controller uses the real checksum bridge', () => {
  const src = read('src/components/editor/checksumPanel.ts');
  assert.match(src, /bridgeComputeChecksum/);
});

test('smoothing controller uses the real smoothing bridge', () => {
  const src = read('src/components/editor/smoothingPreview.ts');
  assert.match(src, /bridgeAnalyzeMapSmoothing/);
});

test('HeatmapTable renders the checksum and preview controls', () => {
  const src = readHeatmap();
  assert.match(src, /checksumPanel/);
  assert.match(src, /smoothingPreview/);
  assert.match(src, /aria-label="Checksum algorithm"/);
  assert.match(src, /aria-label="Smoothing alpha"/);
});

test('App wires the controls into every editable HeatmapTable site', () => {
  const src = read('src/components/workspace/WorkspaceViewport.tsx');
  const sites = src.match(/<HeatmapTable/g)?.length ?? 0;
  const wired = src.match(/enableSmoothingPreview/g)?.length ?? 0;
  assert.ok(sites >= 3, `expected at least 3 HeatmapTable sites, found ${sites}`);
  assert.strictEqual(wired, sites, 'every HeatmapTable site must be wired');
});

test('smoothing preview never invokes onChangeMatrix', () => {
  // The controller may *document* the prohibition; what matters is that it
  // never calls the callback.
  const src = read('src/components/editor/smoothingPreview.ts');
  assert.doesNotMatch(src, /onChangeMatrix\s*\(/, 'must not invoke onChangeMatrix');
  assert.doesNotMatch(src, /onChangeMatrix\s*[,}]/, 'must not pass onChangeMatrix');
});

test('preview matrix is not passed through baseMatrix', () => {
  // baseMatrix is the delta-mode baseline; overloading it would corrupt diffs.
  const src = readHeatmap();
  const previewAssignments = src.match(/baseMatrix=\{previewMatrix\}/g) ?? [];
  assert.strictEqual(previewAssignments.length, 0, 'preview must not reuse baseMatrix');
});

test('alpha slider is bounded away from the rejected endpoints', () => {
  const src = readHeatmap();
  assert.match(src, /min=\{0\.05\}/);
  assert.match(src, /max=\{0\.95\}/);
});
