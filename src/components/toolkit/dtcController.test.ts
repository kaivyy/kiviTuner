/**
 * DTC controller contract tests.
 *
 * Validation happens before the backend, and a patch never mutates the caller's
 * buffer.
 */

import test from 'node:test';
import assert from 'node:assert/strict';
import { runDtcParse, runDtcPatch, DTC_DISABLED_BYTE } from './dtcController.ts';

// ---------------------------------------------------------------------------
// Parse validation
// ---------------------------------------------------------------------------

test('rejects a code that is not 5 characters', async () => {
  const short = await runDtcParse('P042');
  const long = await runDtcParse('P04201');
  assert.strictEqual(short.kind, 'error');
  assert.strictEqual(long.kind, 'error');
});

test('rejects an invalid prefix before the backend', async () => {
  const s = await runDtcParse('X0420');
  assert.strictEqual(s.kind, 'error');
  if (s.kind === 'error') assert.match(s.message, /P, C, B, or U/);
});

test('a well-formed code reaches the bridge (absent here)', async () => {
  const s = await runDtcParse('P0420');
  assert.strictEqual(s.kind, 'no-backend');
});

test('the prefix check is case-insensitive', async () => {
  const s = await runDtcParse('p0420');
  assert.strictEqual(s.kind, 'no-backend', 'lowercase prefix must pass validation');
});

// ---------------------------------------------------------------------------
// Patch validation
// ---------------------------------------------------------------------------

test('patching without a ROM is refused', async () => {
  const s = await runDtcPatch(null, 0);
  assert.strictEqual(s.kind, 'error');
  if (s.kind === 'error') assert.match(s.message, /Load a ROM/);
});

test('patching an empty ROM is refused', async () => {
  const s = await runDtcPatch(new Uint8Array(0), 0);
  assert.strictEqual(s.kind, 'error');
});

test('a negative switch address is refused', async () => {
  const s = await runDtcPatch(new Uint8Array([1, 2, 3]), -1);
  assert.strictEqual(s.kind, 'error');
});

test('a non-integer switch address is refused', async () => {
  const s = await runDtcPatch(new Uint8Array([1, 2, 3]), 1.5);
  assert.strictEqual(s.kind, 'error');
});

test('an out-of-bounds switch address is refused with the size in the message', async () => {
  const s = await runDtcPatch(new Uint8Array(4), 10);
  assert.strictEqual(s.kind, 'error');
  if (s.kind === 'error') {
    assert.match(s.message, /outside the ROM/);
    assert.match(s.message, /4 bytes/);
  }
});

test('a valid patch request reaches the bridge (absent here)', async () => {
  const s = await runDtcPatch(new Uint8Array([1, 2, 3, 4]), 2);
  assert.strictEqual(s.kind, 'no-backend');
});

test('patching does not mutate the supplied buffer', async () => {
  const rom = new Uint8Array([1, 2, 3, 4]);
  const copy = Uint8Array.from(rom);
  await runDtcPatch(rom, 2);
  assert.deepStrictEqual(rom, copy, 'the caller buffer must be untouched');
});

test('the disabled byte is documented as 0x00', () => {
  assert.strictEqual(DTC_DISABLED_BYTE, 0x00);
});
