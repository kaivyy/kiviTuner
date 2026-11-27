/**
 * Wave 8 toolkit controller contract tests.
 *
 * The controller is the layer that decides what the UI may show. These test its
 * validation and its honest handling of a missing backend — not the maths, which
 * is tested in Rust.
 */

import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import {
  runFuelPulse,
  runAirMass,
  runSparkTiming,
  runTorque,
  runSensorDecode,
  runDescribeFlashTarget,
  runHistogram,
  hasBlockSizeMismatch,
  describeFlashTarget,
} from './toolkitController.ts';
import type { FlashTargetView } from '../../lib/tauriBridge.ts';

const read = (p: string): string => readFileSync(p, 'utf8');

// ---------------------------------------------------------------------------
// Validation happens before the backend
// ---------------------------------------------------------------------------

test('fuel pulse rejects a non-positive lambda before calling the backend', async () => {
  const s = await runFuelPulse({
    air_mass_g: 0.15,
    target_lambda: 0,
    battery_volts: 13.8,
    rail_diff_kpa: 300,
    cycle_dt_sec: 0.017,
    fuel: 'e10',
  });
  assert.strictEqual(s.kind, 'error');
  if (s.kind === 'error') assert.match(s.message, /lambda/i);
});

test('fuel pulse rejects negative air mass', async () => {
  const s = await runFuelPulse({
    air_mass_g: -1,
    target_lambda: 1,
    battery_volts: 13.8,
    rail_diff_kpa: 300,
    cycle_dt_sec: 0.017,
    fuel: 'e10',
  });
  assert.strictEqual(s.kind, 'error');
});

test('fuel pulse rejects a NaN input', async () => {
  const s = await runFuelPulse({
    air_mass_g: NaN,
    target_lambda: 1,
    battery_volts: 13.8,
    rail_diff_kpa: 300,
    cycle_dt_sec: 0.017,
    fuel: 'e10',
  });
  assert.strictEqual(s.kind, 'error');
  if (s.kind === 'error') assert.match(s.message, /finite/i);
});

test('air mass rejects zero cylinders', async () => {
  const s = await runAirMass(155, 0, 100, 0.85, 30, 85, 7000);
  assert.strictEqual(s.kind, 'error');
});

test('air mass rejects zero rpm', async () => {
  const s = await runAirMass(155, 1, 100, 0.85, 30, 85, 0);
  assert.strictEqual(s.kind, 'error');
});

test('spark timing rejects negative knock retard', async () => {
  const s = await runSparkTiming(20, 0, 0, -5);
  assert.strictEqual(s.kind, 'error');
});

test('torque rejects a pedal outside 0..100', async () => {
  assert.strictEqual((await runTorque(150, 5000, 100, 85, 0, 100)).kind, 'error');
  assert.strictEqual((await runTorque(-10, 5000, 100, 85, 0, 100)).kind, 'error');
});

test('sensor decode rejects a non-finite raw value', async () => {
  const s = await runSensorDecode('map', Infinity);
  assert.strictEqual(s.kind, 'error');
});

test('flash target rejects a negative chip id', async () => {
  const s = await runDescribeFlashTarget(-1);
  assert.strictEqual(s.kind, 'error');
});

// ---------------------------------------------------------------------------
// Missing backend is reported, never faked
// ---------------------------------------------------------------------------

test('valid input without a backend reports no-backend, not success', async () => {
  const s = await runAirMass(155, 1, 100, 0.85, 30, 85, 7000);
  assert.strictEqual(s.kind, 'no-backend');
  if (s.kind === 'no-backend') assert.match(s.message, /desktop/i);
});

test('sensor decode without a backend reports no-backend', async () => {
  const s = await runSensorDecode('map', 4.5);
  assert.strictEqual(s.kind, 'no-backend');
});

// ---------------------------------------------------------------------------
// Flash target mismatch surfacing
// ---------------------------------------------------------------------------

const AM29F400BT: FlashTargetView = {
  model_name: 'AMD Am29F400BT 512KiB Top-Boot',
  total_size_bytes: 524288,
  blocks_total_bytes: 327680,
  blocks: [
    { start_address: 0, length: 65536, block_type: 'OperatingSystem', write_safe: true },
    { start_address: 0x7C000, length: 0x4000, block_type: 'Boot', write_safe: false },
  ],
};

const CLEAN_CHIP: FlashTargetView = {
  model_name: 'Clean chip',
  total_size_bytes: 1024,
  blocks_total_bytes: 1024,
  blocks: [{ start_address: 0, length: 1024, block_type: 'Calibration', write_safe: true }],
};

test('a size mismatch is detected', () => {
  assert.strictEqual(hasBlockSizeMismatch(AM29F400BT), true);
  assert.strictEqual(hasBlockSizeMismatch(CLEAN_CHIP), false);
});

test('the mismatch warning names the unaccounted byte count', () => {
  const text = describeFlashTarget(AM29F400BT);
  assert.match(text, /WARNING/);
  assert.match(text, /196608/, 'the 0x30000 gap must be stated in bytes');
});

test('a clean chip produces no warning', () => {
  assert.doesNotMatch(describeFlashTarget(CLEAN_CHIP), /WARNING/);
});

// ---------------------------------------------------------------------------
// Wiring: commands registered, bridges typed, no `any`
// ---------------------------------------------------------------------------

test('all seven toolkit commands are registered', () => {
  const lib = read('src-tauri/src/lib.rs');
  for (const cmd of [
    'compute_fuel_pulse',
    'compute_air_mass',
    'compute_spark_timing',
    'describe_flash_target_view',
    'compute_histogram',
    'compute_torque',
    'decode_sensor',
  ]) {
    assert.match(lib, new RegExp(`toolkit::${cmd}\\b`), `${cmd} must be registered`);
  }
});

test('the toolkit bridges are typed and invoke the registered names', () => {
  const src = read('src/lib/bridge/toolkitBridge.ts');
  const pairs: Array<[string, string]> = [
    ['bridgeComputeFuelPulse', 'compute_fuel_pulse'],
    ['bridgeComputeAirMass', 'compute_air_mass'],
    ['bridgeComputeSparkTiming', 'compute_spark_timing'],
    ['bridgeDescribeFlashTargetView', 'describe_flash_target_view'],
    ['bridgeComputeTorque', 'compute_torque'],
    ['bridgeDecodeSensor', 'decode_sensor'],
  ];
  for (const [bridge, cmd] of pairs) {
    assert.match(src, new RegExp(`function ${bridge}`), `${bridge} must exist`);
    assert.match(src, new RegExp(`invoke<[^>]*>\\('${cmd}'`), `${bridge} must invoke ${cmd}`);
  }
});

test('no toolkit bridge uses a bare any type', () => {
  const src = read('src/lib/bridge/toolkitBridge.ts');
  const start = src.indexOf('Wave 8: engineering toolkit');
  assert.ok(start > 0);
  assert.doesNotMatch(src.slice(start), /:\s*any\b/);
  assert.doesNotMatch(src.slice(start), /\brequire\(/);
});

// ---------------------------------------------------------------------------
// Histogram
// ---------------------------------------------------------------------------

test('histogram rejects a degenerate axis before the backend', async () => {
  const s = await runHistogram([0], [0, 1], [[0, 0]]);
  assert.strictEqual(s.kind, 'error');
  if (s.kind === 'error') assert.match(s.message, /two breakpoints/i);
});

test('histogram rejects an empty point set', async () => {
  const s = await runHistogram([0, 1], [0, 1], []);
  assert.strictEqual(s.kind, 'error');
  if (s.kind === 'error') assert.match(s.message, /at least one/i);
});

test('histogram without a backend reports no-backend', async () => {
  const s = await runHistogram([0, 1000], [0, 100], [[500, 50]]);
  assert.strictEqual(s.kind, 'no-backend');
});
