/**
 * Wave 8 UI wiring tests.
 *
 * A panel that exists but is never rendered is not integration. These assert the
 * toolkit is reachable from the app shell and that its controls are labelled.
 */

import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const read = (p: string): string => readFileSync(p, 'utf8');

test('the toolkit panel is imported by the app shell', () => {
  const src = read('src/App.tsx');
  assert.match(src, /import \{ EngineeringToolkit \} from '\.\/components\/toolkit\/EngineeringToolkit'/);
});

test('the toolkit panel is rendered with open/close state', () => {
  const src = read('src/App.tsx');
  assert.match(src, /<EngineeringToolkit\s+isOpen=\{isToolkitOpen\}/);
  assert.match(src, /setIsToolkitOpen\(false\)/);
});

test('there is a discoverable toolbar button', () => {
  const src = ['src/App.tsx', 'src/components/workspace/WorkspaceViewport.tsx']
    .map(read)
    .join('\n');
  assert.match(src, /aria-label="Open engineering toolkit"/);
  assert.match(src, /setIsToolkitOpen\(true\)/);
});

test('the panel exposes all six tabs', () => {
  const src = read('src/components/toolkit/EngineeringToolkit.tsx');
  for (const label of [
    'Fuel pulse',
    'Air mass',
    'Spark timing',
    'Torque',
    'Sensor',
    'Flash chip',
  ]) {
    assert.match(src, new RegExp(`label: '${label}'`), `${label} tab must exist`);
  }
});

test('every input is labelled for accessibility', () => {
  const src = read('src/components/toolkit/EngineeringToolkit.tsx');
  const labels = src.match(/ariaLabel="/g) ?? [];
  assert.ok(labels.length >= 15, `expected many labelled inputs, found ${labels.length}`);
});

test('the panel states it is calculation-only', () => {
  const src = read('src/components/toolkit/EngineeringToolkit.tsx');
  assert.match(src, /calculation only/i);
  assert.match(src, /Nothing here writes to a ROM, an ECU, or a file/);
});

test('the panel surfaces the chip size mismatch rather than hiding it', () => {
  const src = read('src/components/toolkit/EngineeringToolkit.tsx');
  assert.match(src, /hasBlockSizeMismatch/);
  assert.match(src, /do not assume the gap is usable flash/);
});

test('the panel marks write-blocked blocks', () => {
  const src = read('src/components/toolkit/EngineeringToolkit.tsx');
  assert.match(src, /write-blocked/);
  assert.match(src, /!b\.write_safe/);
});

test('the panel does not import any Tauri invoke directly', () => {
  const src = read('src/components/toolkit/EngineeringToolkit.tsx');
  assert.doesNotMatch(src, /\binvoke\(/, 'the panel must go through the controller');
  assert.doesNotMatch(src, /@tauri-apps\/api/);
});
