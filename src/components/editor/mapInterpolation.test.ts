/**
 * Contract tests for the read-only map interpolation controller.
 *
 * The backend resolves `x`/`y` against **engineering axis values**, falling back
 * to cell indices when the XDF supplied no usable breakpoints. The controller
 * must mirror that rule exactly: if the frontend and backend disagree about the
 * coordinate domain, the same query silently returns a value from a different
 * place in the table.
 */

import test from 'node:test';
import assert from 'node:assert/strict';
import {
  resolveAxisBreakpoints,
  computeMapInterpolation,
  describeInterpolation,
  describeAxisDomain,
  type InterpolatableMap,
} from './mapInterpolation.ts';

// Mirrors the fixture used by src-tauri/tests/xdf_axis_interpolation_test.rs
const MAP_WITH_XDF_AXES: InterpolatableMap = {
  rows: 2,
  cols: 3,
  xAxisValues: [1000, 2000, 3000],
  yAxisValues: [1, 2],
};

const MAP_WITHOUT_AXES: InterpolatableMap = {
  rows: 2,
  cols: 3,
  xAxisValues: null,
  yAxisValues: null,
};

const MATRIX = [
  [10, 20, 30],
  [40, 50, 60],
];

// ---------------------------------------------------------------------------
// Breakpoint resolution — must match `axis_breakpoints` in commands.rs
// ---------------------------------------------------------------------------

test('uses XDF values when they match the axis length', () => {
  const r = resolveAxisBreakpoints([1000, 2000, 3000], 3);
  assert.deepStrictEqual(r.values, [1000, 2000, 3000]);
  assert.strictEqual(r.source, 'xdf');
});

test('falls back to indices when values are absent', () => {
  const r = resolveAxisBreakpoints(null, 3);
  assert.deepStrictEqual(r.values, [0, 1, 2]);
  assert.strictEqual(r.source, 'index');
});

test('falls back to indices when the length does not match', () => {
  // A 2-entry axis on a 3-column table is unusable; the backend ignores it.
  const r = resolveAxisBreakpoints([1000, 2000], 3);
  assert.deepStrictEqual(r.values, [0, 1, 2]);
  assert.strictEqual(r.source, 'index');
});

test('falls back to indices for a single-entry axis', () => {
  // The backend requires at least two breakpoints to form an interval.
  const r = resolveAxisBreakpoints([1000], 1);
  assert.deepStrictEqual(r.values, [0]);
  assert.strictEqual(r.source, 'index');
});

test('falls back to indices for an empty axis', () => {
  const r = resolveAxisBreakpoints([], 2);
  assert.deepStrictEqual(r.values, [0, 1]);
  assert.strictEqual(r.source, 'index');
});

// ---------------------------------------------------------------------------
// Interpolation against engineering coordinates
// ---------------------------------------------------------------------------

test('interpolates at an exact XDF breakpoint', async () => {
  const s = await computeMapInterpolation('map_1', MAP_WITH_XDF_AXES, MATRIX, 2000, 2);
  assert.strictEqual(s.kind, 'ok');
  // col 1, row 1
  assert.strictEqual(s.result.value, 50);
  // Interior exact match: the interval is [1,2] with t=0, not [0,1].
  assert.strictEqual(s.result.x_low, 1);
  assert.strictEqual(s.result.x_t, 0);
  // y=2 is the last breakpoint, so the interval is the final one, [0,1], t=1.
  assert.strictEqual(s.result.y_low, 0);
  assert.strictEqual(s.result.y_t, 1);
});

test('interpolates between two XDF breakpoints', async () => {
  const s = await computeMapInterpolation('map_1', MAP_WITH_XDF_AXES, MATRIX, 2500, 1);
  assert.strictEqual(s.kind, 'ok');
  // Halfway between col 1 (20) and col 2 (30) at row 0.
  assert.strictEqual(s.result.value, 25);
  assert.strictEqual(s.result.x_t, 0.5);
});

test('index fallback treats the query as a cell coordinate', async () => {
  const s = await computeMapInterpolation('map_1', MAP_WITHOUT_AXES, MATRIX, 1, 1);
  assert.strictEqual(s.kind, 'ok');
  assert.strictEqual(s.result.value, 50);
  assert.strictEqual(s.axisSource.x, 'index');
  assert.strictEqual(s.axisSource.y, 'index');
});

test('reports which axis domain was used so the UI can label its inputs', async () => {
  const withAxes = await computeMapInterpolation('map_1', MAP_WITH_XDF_AXES, MATRIX, 2000, 2);
  assert.strictEqual(withAxes.kind, 'ok');
  assert.strictEqual(withAxes.axisSource.x, 'xdf');
  assert.strictEqual(withAxes.axisSource.y, 'xdf');
  assert.deepStrictEqual(withAxes.axisValues.x, [1000, 2000, 3000]);
  assert.deepStrictEqual(withAxes.axisValues.y, [1, 2]);
});

test('clamping is reported rather than silently applied', async () => {
  const s = await computeMapInterpolation('map_1', MAP_WITH_XDF_AXES, MATRIX, 100, 2);
  assert.strictEqual(s.kind, 'ok');
  assert.strictEqual(s.result.x_clamped, true);
});

// ---------------------------------------------------------------------------
// Refusals — the controller must not invent a query
// ---------------------------------------------------------------------------

test('rejects a non-finite X coordinate', async () => {
  const s = await computeMapInterpolation('map_1', MAP_WITH_XDF_AXES, MATRIX, NaN, 2);
  assert.strictEqual(s.kind, 'invalid-coordinate');
});

test('rejects a non-finite Y coordinate', async () => {
  const s = await computeMapInterpolation('map_1', MAP_WITH_XDF_AXES, MATRIX, 2000, Infinity);
  assert.strictEqual(s.kind, 'invalid-coordinate');
});

test('rejects a 1xN map, which has no interval to interpolate over', async () => {
  const flat: InterpolatableMap = { rows: 1, cols: 3, xAxisValues: null, yAxisValues: null };
  const s = await computeMapInterpolation('map_1', flat, [[10, 20, 30]], 1, 0);
  assert.strictEqual(s.kind, 'degenerate');
});

test('rejects a matrix whose shape disagrees with the map definition', async () => {
  const wrong = [[1, 2], [3, 4]];
  const s = await computeMapInterpolation('map_1', MAP_WITH_XDF_AXES, wrong, 2000, 2);
  assert.strictEqual(s.kind, 'dimension-mismatch');
});

test('rejects a missing matrix', async () => {
  const s = await computeMapInterpolation('map_1', MAP_WITH_XDF_AXES, null, 2000, 2);
  assert.strictEqual(s.kind, 'no-map');
});

test('rejects a missing map id instead of falling back silently', async () => {
  // The desktop path resolves the map by id in Rust. An empty id makes the
  // invoke fail, and the bridge would then quietly compute in JS — so the
  // backend command would never run. Refusing up front keeps that visible.
  const s = await computeMapInterpolation('', MAP_WITH_XDF_AXES, MATRIX, 2000, 2);
  assert.strictEqual(s.kind, 'no-map');
});

// ---------------------------------------------------------------------------
// Summary text
// ---------------------------------------------------------------------------

test('summary states the value and the bracketing cells', async () => {
  const s = await computeMapInterpolation('map_1', MAP_WITH_XDF_AXES, MATRIX, 2500, 1);
  assert.strictEqual(s.kind, 'ok');
  const text = describeInterpolation(s.result);
  assert.match(text, /25\.0/);
  assert.match(text, /col 1.*2/);
});

test('summary flags a clamped coordinate', async () => {
  const s = await computeMapInterpolation('map_1', MAP_WITH_XDF_AXES, MATRIX, 100, 2);
  assert.strictEqual(s.kind, 'ok');
  assert.match(describeInterpolation(s.result), /clamp/i);
});

// ---------------------------------------------------------------------------
// Axis domain label — the user cannot guess which domain their input is in
// ---------------------------------------------------------------------------

test('labels an XDF domain by its endpoint range', () => {
  const axis = resolveAxisBreakpoints([1000, 2000, 3000], 3);
  assert.strictEqual(describeAxisDomain(axis), '1000–3000');
});

test('labels an index domain as indices', () => {
  const axis = resolveAxisBreakpoints(null, 3);
  assert.strictEqual(describeAxisDomain(axis), 'index 0–2');
});

test('never renders a negative index for an empty axis', () => {
  const axis = resolveAxisBreakpoints(null, 0);
  assert.strictEqual(describeAxisDomain(axis), 'empty');
});
