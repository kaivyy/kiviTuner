/**
 * Transient / rescale / resample controller contract tests.
 *
 * The controller is the layer that decides what the UI may show. These test its
 * validation and its honest handling of a missing backend — not the maths, which
 * is tested in Rust.
 */

import test from 'node:test';
import assert from 'node:assert/strict';
import {
  runTransient,
  runInjectorRescale,
  runAxisResample,
} from './transientController.ts';
import type { TransientInput, RescaleInput } from './transientController.ts';

const transient = (over: Partial<TransientInput> = {}): TransientInput => ({
  rpm: 800,
  tpsPct: 0,
  speedKph: 0,
  ectC: 85,
  targetIdleRpm: 800,
  mafVolts: 1.2,
  ...over,
});

const rescale = (over: Partial<RescaleInput> = {}): RescaleInput => ({
  pwOldMs: 4.2,
  deadOldMs: 0.9,
  flowOldCc: 240,
  flowNewCc: 440,
  deadNewMs: 0.7,
  ...over,
});

// ---------------------------------------------------------------------------
// Validation happens before the backend
// ---------------------------------------------------------------------------

test('transient rejects a negative rpm', async () => {
  const s = await runTransient(transient({ rpm: -1 }));
  assert.strictEqual(s.kind, 'error');
  if (s.kind === 'error') assert.match(s.message, /rpm/i);
});

test('transient rejects throttle above 100 percent', async () => {
  const s = await runTransient(transient({ tpsPct: 101 }));
  assert.strictEqual(s.kind, 'error');
  if (s.kind === 'error') assert.match(s.message, /throttle/i);
});

test('transient rejects a negative MAF voltage', async () => {
  const s = await runTransient(transient({ mafVolts: -0.1 }));
  assert.strictEqual(s.kind, 'error');
});

test('rescale rejects a zero old injector flow', async () => {
  const s = await runInjectorRescale(rescale({ flowOldCc: 0 }));
  assert.strictEqual(s.kind, 'error');
  if (s.kind === 'error') assert.match(s.message, /old injector flow/i);
});

test('rescale rejects a zero new injector flow', async () => {
  const s = await runInjectorRescale(rescale({ flowNewCc: 0 }));
  assert.strictEqual(s.kind, 'error');
  if (s.kind === 'error') assert.match(s.message, /new injector flow/i);
});

test('rescale rejects a negative pulse width', async () => {
  const s = await runInjectorRescale(rescale({ pwOldMs: -1 }));
  assert.strictEqual(s.kind, 'error');
});

test('resample rejects mismatched old axis and value lengths', async () => {
  const s = await runAxisResample([0, 1000, 2000], [1, 2], [0, 500, 1000]);
  assert.strictEqual(s.kind, 'error');
  if (s.kind === 'error') assert.match(s.message, /same length/i);
});

test('resample rejects an empty new axis', async () => {
  const s = await runAxisResample([0, 1000], [1, 2], []);
  assert.strictEqual(s.kind, 'error');
  if (s.kind === 'error') assert.match(s.message, /new axis/i);
});

test('resample rejects an empty old axis', async () => {
  const s = await runAxisResample([], [], [0, 1000]);
  assert.strictEqual(s.kind, 'error');
});

test('transient rejects NaN in any numeric field', async () => {
  for (const field of [
    'rpm',
    'tpsPct',
    'speedKph',
    'ectC',
    'targetIdleRpm',
    'mafVolts',
  ] as const) {
    const s = await runTransient(transient({ [field]: NaN }));
    assert.strictEqual(s.kind, 'error', `${field}=NaN must be rejected`);
  }
});

test('rescale rejects NaN in any numeric field', async () => {
  for (const field of [
    'pwOldMs',
    'deadOldMs',
    'flowOldCc',
    'flowNewCc',
    'deadNewMs',
  ] as const) {
    const s = await runInjectorRescale(rescale({ [field]: NaN }));
    assert.strictEqual(s.kind, 'error', `${field}=NaN must be rejected`);
  }
});

test('resample rejects NaN anywhere in any axis', async () => {
  const bad = await runAxisResample([0, 1000], [1, NaN], [0, 500]);
  assert.strictEqual(bad.kind, 'error');
  const badNew = await runAxisResample([0, 1000], [1, 2], [0, NaN]);
  assert.strictEqual(badNew.kind, 'error');
});

// ---------------------------------------------------------------------------
// Missing backend is reported, never faked
// ---------------------------------------------------------------------------

test('valid transient input without a backend reports no-backend', async () => {
  const s = await runTransient(transient());
  assert.strictEqual(s.kind, 'no-backend');
  if (s.kind === 'no-backend') assert.match(s.message, /desktop/i);
});

test('valid rescale input without a backend reports no-backend', async () => {
  const s = await runInjectorRescale(rescale());
  assert.strictEqual(s.kind, 'no-backend');
  if (s.kind === 'no-backend') assert.match(s.message, /desktop/i);
});

test('valid resample input without a backend reports no-backend', async () => {
  const s = await runAxisResample([0, 1000, 2000], [1, 2, 3], [0, 500, 1000, 1500, 2000]);
  assert.strictEqual(s.kind, 'no-backend');
  if (s.kind === 'no-backend') assert.match(s.message, /desktop/i);
});
