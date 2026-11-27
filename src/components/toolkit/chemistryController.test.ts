/**
 * Chemistry / airflow controller contract tests.
 *
 * The controller is the layer that decides what the UI may show. These test its
 * validation and its honest handling of a missing backend — not the maths, which
 * is tested in Rust.
 *
 * There is no Tauri in this environment, so every valid call must resolve to
 * `no-backend` rather than a fabricated `ok`.
 */

import test from 'node:test';
import assert from 'node:assert/strict';
import {
  runBlend,
  runCompressor,
  runThrottleFlow,
} from './chemistryController.ts';
import type { CompressorInput } from './chemistryController.ts';

const validCompressor: CompressorInput = {
  tInC: 25,
  pressureRatio: 2.0,
  isentropicEfficiency: 0.72,
  intercoolerEfficiency: 0.8,
  ambientC: 25,
  lambda: 0.85,
  sparkRetardDeg: 2,
};

// ---------------------------------------------------------------------------
// Blend validation
// ---------------------------------------------------------------------------

test('blend rejects ethanol above 100 percent', async () => {
  const s = await runBlend(100.1);
  assert.strictEqual(s.kind, 'error');
  if (s.kind === 'error') assert.match(s.message, /ethanol/i);
});

test('blend rejects negative ethanol', async () => {
  const s = await runBlend(-1);
  assert.strictEqual(s.kind, 'error');
  if (s.kind === 'error') assert.match(s.message, /ethanol/i);
});

test('blend rejects NaN ethanol', async () => {
  const s = await runBlend(NaN);
  assert.strictEqual(s.kind, 'error');
  if (s.kind === 'error') assert.match(s.message, /finite/i);
});

// ---------------------------------------------------------------------------
// Compressor validation
// ---------------------------------------------------------------------------

test('compressor rejects a pressure ratio below 1', async () => {
  const s = await runCompressor({ ...validCompressor, pressureRatio: 0.99 });
  assert.strictEqual(s.kind, 'error');
  if (s.kind === 'error') assert.match(s.message, /pressure ratio/i);
});

test('compressor rejects isentropic efficiency outside 0.40..0.95', async () => {
  const low = await runCompressor({ ...validCompressor, isentropicEfficiency: 0.39 });
  assert.strictEqual(low.kind, 'error');
  const high = await runCompressor({ ...validCompressor, isentropicEfficiency: 0.96 });
  assert.strictEqual(high.kind, 'error');
  if (high.kind === 'error') assert.match(high.message, /isentropic/i);
});

test('compressor rejects intercooler efficiency above 1', async () => {
  const s = await runCompressor({ ...validCompressor, intercoolerEfficiency: 1.01 });
  assert.strictEqual(s.kind, 'error');
  if (s.kind === 'error') assert.match(s.message, /intercooler/i);
});

// ---------------------------------------------------------------------------
// Valid input with no desktop backend
// ---------------------------------------------------------------------------

test('a valid blend resolves to no-backend, not a fake ok', async () => {
  const s = await runBlend(85);
  assert.strictEqual(s.kind, 'no-backend');
  if (s.kind === 'no-backend') assert.ok(s.message.length > 0);
});

test('a valid compressor input resolves to no-backend', async () => {
  const s = await runCompressor(validCompressor);
  assert.strictEqual(s.kind, 'no-backend');
  if (s.kind === 'no-backend') assert.ok(s.message.length > 0);
});

// ---------------------------------------------------------------------------
// Throttle flow has no backend command at all
// ---------------------------------------------------------------------------

test('throttle flow validates the angle before the backend', async () => {
  const s = await runThrottleFlow({ throttleDeg: 120, rpm: 3000, mapKpa: 50, iatC: 30 });
  assert.strictEqual(s.kind, 'error');
  if (s.kind === 'error') assert.match(s.message, /between 0 and 90/);
});

test('throttle flow rejects a negative MAP', async () => {
  const s = await runThrottleFlow({ throttleDeg: 30, rpm: 3000, mapKpa: -1, iatC: 30 });
  assert.strictEqual(s.kind, 'error');
});

test('throttle flow rejects a non-finite input', async () => {
  const s = await runThrottleFlow({ throttleDeg: NaN, rpm: 3000, mapKpa: 50, iatC: 30 });
  assert.strictEqual(s.kind, 'error');
});

test('a valid throttle flow request reaches the bridge (absent here)', async () => {
  const s = await runThrottleFlow({ throttleDeg: 30, rpm: 3000, mapKpa: 50, iatC: 30 });
  assert.strictEqual(s.kind, 'no-backend');
});
