/**
 * Batch B2 integration guard.
 *
 * Asserts that the two backend commands this batch integrates are reachable
 * from the application surface, and — critically — that interpolation is fed
 * real XDF axis values rather than synthesised labels.
 *
 * These are source assertions: this repository has no DOM renderer in its test
 * setup. That limitation is stated rather than worked around by adding a
 * dependency. Behavioural coverage of the controllers lives in
 * `mapInterpolation.test.ts` and `romVerification.test.ts`.
 */

import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const read = (p: string): string => readFileSync(p, 'utf8');
const readHeatmap = (): string =>
  [
    'src/components/editor/HeatmapTable.tsx',
    'src/components/editor/heatmap/HeatmapAnalysisBar.tsx',
  ]
    .map(read)
    .join('\n');
const readWorkspace = (): string =>
  ['src/App.tsx', 'src/components/workspace/WorkspaceViewport.tsx'].map(read).join('\n');

test('interpolation controller uses the real interpolation bridge', () => {
  const src = read('src/components/editor/mapInterpolation.ts');
  assert.match(src, /bridgeInterpolateMapAt/);
});

test('verification controller uses the real verification bridge', () => {
  const src = read('src/components/modals/romVerification.ts');
  assert.match(src, /bridgeVerifyRomAgainstReference/);
});

test('interpolation controller never reads synthetic axis labels', () => {
  // generateAxisLabels invents RPM/TPS ranges that are not in the XDF. Using
  // them would query a coordinate domain the ROM does not have.
  // Match an import or call, not the explanatory comment.
  const src = read('src/components/editor/mapInterpolation.ts');
  assert.doesNotMatch(src, /import[^;]*generateAxisLabels/);
  assert.doesNotMatch(src, /generateAxisLabels\s*\(/);
});

test('HeatmapTable wires the interpolation controller', () => {
  const src = readHeatmap();
  assert.match(src, /mapInterpolation/);
  assert.match(src, /aria-label="Interpolation X"/);
  assert.match(src, /aria-label="Interpolation Y"/);
});

test('BinDiffModal wires the verification controller', () => {
  const src = read('src/components/modals/BinDiffModal.tsx');
  assert.match(src, /romVerification/);
  assert.match(src, /aria-label="Verify against reference"/);
});

test('App passes XDF axis values to every HeatmapTable site', () => {
  const src = read('src/components/workspace/WorkspaceViewport.tsx');
  const sites = src.match(/<HeatmapTable/g)?.length ?? 0;
  const wired = src.match(/interpolationAxes=/g)?.length ?? 0;
  assert.ok(sites >= 3, `expected at least 3 HeatmapTable sites, found ${sites}`);
  assert.strictEqual(wired, sites, 'every HeatmapTable site must receive axis values');
});

test('App sources interpolation axes from the map definition, not the labels', () => {
  const src = readWorkspace();
  // The axes handed to the editor must come from selectedMap.x_axis/y_axis.
  assert.match(src, /selectedMap\.x_axis/);
  assert.match(src, /selectedMap\.y_axis/);
});
