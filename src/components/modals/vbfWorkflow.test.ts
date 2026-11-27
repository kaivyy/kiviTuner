/**
 * Wave 5: VBF workflow contract tests.
 *
 * These test the controller's decisions, not the codec — the codec has its own
 * Rust tests. The decisions matter because a wrong one loses data: flattening a
 * multi-segment container into the single-buffer ROM document would silently
 * discard segment addresses.
 */

import test from 'node:test';
import assert from 'node:assert/strict';
import {
  inspectVbf,
  canLoadAsRom,
  bytesForRom,
  exportRomAsVbf,
  describeSegment,
  VBF_LIMITATION_NOTICE,
  type VbfImportState,
} from './vbfWorkflow.ts';

const oneSegment = {
  header: 'vbf_version = 2.4;\nheader {\n sw_part_number = "X";\n num_segments = 1;\n}',
  segments: [{ start_address: 0x80000, data: [0x11, 0x22, 0x33] }],
};

const twoSegments = {
  header: '...',
  segments: [
    { start_address: 0x1000, data: [1, 2] },
    { start_address: 0x2000, data: [3, 4, 5] },
  ],
};

// ---------------------------------------------------------------------------
// Import decisions
// ---------------------------------------------------------------------------

test('empty input is refused before reaching the backend', async () => {
  const s = await inspectVbf(new Uint8Array(0));
  assert.strictEqual(s.kind, 'empty');
});

test('a single-segment container can be loaded as a ROM', () => {
  assert.strictEqual(canLoadAsRom(oneSegment), true);
  const bytes = bytesForRom(oneSegment);
  assert.ok(bytes);
  assert.deepStrictEqual(Array.from(bytes), [0x11, 0x22, 0x33]);
});

test('a multi-segment container is refused for ROM loading', () => {
  // This is the data-loss guard: flattening would drop the 0x1000/0x2000 split.
  assert.strictEqual(canLoadAsRom(twoSegments), false);
  assert.strictEqual(
    bytesForRom(twoSegments),
    null,
    'a multi-segment container must not be silently flattened'
  );
});

test('the browser build reports no-backend rather than pretending to parse', async () => {
  // In the node test environment there is no Tauri, so the bridge returns null.
  const s: VbfImportState = await inspectVbf(new Uint8Array([0x1a, 0x01]));
  assert.strictEqual(s.kind, 'no-backend');
  if (s.kind === 'no-backend') {
    assert.match(s.message, /desktop/i);
  }
});

// ---------------------------------------------------------------------------
// Export decisions
// ---------------------------------------------------------------------------

test('export without a ROM is refused', async () => {
  const s = await exportRomAsVbf(null, 0x80000, 'X');
  assert.strictEqual(s.kind, 'no-rom');
});

test('export with an unknown base address is refused, not defaulted to zero', async () => {
  const s = await exportRomAsVbf(new Uint8Array([1, 2, 3]), null, 'X');
  assert.strictEqual(s.kind, 'unknown-base');
  if (s.kind === 'unknown-base') {
    assert.match(s.message, /unknown/i);
  }
});

test('export with a known base reaches the backend (which is absent here)', async () => {
  const s = await exportRomAsVbf(new Uint8Array([1, 2, 3]), 0x80000, 'X');
  assert.strictEqual(s.kind, 'no-backend');
});

test('a known base of zero is accepted as known', async () => {
  // Some(0) must not be treated like null.
  const s = await exportRomAsVbf(new Uint8Array([1]), 0, 'X');
  assert.notStrictEqual(
    s.kind,
    'unknown-base',
    'address zero is a known address, not an unknown one'
  );
});

// ---------------------------------------------------------------------------
// Presentation
// ---------------------------------------------------------------------------

test('segment description shows a zero-padded address and byte count', () => {
  assert.strictEqual(
    describeSegment({ start_address: 0x80000, data: [1, 2, 3] }),
    '0x00080000 · 3 bytes'
  );
  assert.strictEqual(
    describeSegment({ start_address: 0, data: [1] }),
    '0x00000000 · 1 byte'
  );
});

test('the limitation notice is present and states the OEM caveat', () => {
  assert.match(VBF_LIMITATION_NOTICE, /not validated against OEM/i);
  assert.match(VBF_LIMITATION_NOTICE, /simplified VBF dialect/i);
});
