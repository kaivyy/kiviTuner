/**
 * Contract tests for the read-only ROM verification controller.
 *
 * The backend command `verify_rom_against_reference` compares byte offsets and
 * returns a report. This controller decides what the UI may show, and — more
 * importantly — what it must refuse to show.
 */

import test from 'node:test';
import assert from 'node:assert/strict';
import { runRomVerification, describeVerification } from './romVerification.ts';

const ACTIVE = new Uint8Array([1, 2, 3, 4]);
const SAME = new Uint8Array([1, 2, 3, 4]);
const DIFFERENT = new Uint8Array([1, 9, 3, 4]);

test('identical buffers report a match', async () => {
  const s = await runRomVerification(ACTIVE, SAME);
  assert.strictEqual(s.kind, 'ok');
  assert.strictEqual(s.report.matches, true);
  assert.strictEqual(s.report.difference_count, 0);
  assert.deepStrictEqual(s.report.differences, []);
});

test('a single differing byte is reported with its offset', async () => {
  const s = await runRomVerification(ACTIVE, DIFFERENT);
  assert.strictEqual(s.kind, 'ok');
  assert.strictEqual(s.report.matches, false);
  assert.strictEqual(s.report.difference_count, 1);
  assert.strictEqual(s.report.differences[0].offset, 1);
  assert.strictEqual(s.report.differences[0].expected_local_byte, 2);
  assert.strictEqual(s.report.differences[0].actual_chip_byte, 9);
});

test('missing active ROM is refused rather than reported as a match', async () => {
  const s = await runRomVerification(null, SAME);
  assert.strictEqual(s.kind, 'no-active-rom');
});

test('missing reference is refused rather than compared against nothing', async () => {
  // Comparing against an empty buffer would report every byte as different,
  // which reads as "your ROM is wrong" when nothing was supplied.
  const s = await runRomVerification(ACTIVE, null);
  assert.strictEqual(s.kind, 'no-reference');
});

test('empty reference is refused as no-reference', async () => {
  const s = await runRomVerification(ACTIVE, new Uint8Array(0));
  assert.strictEqual(s.kind, 'no-reference');
});

test('length mismatch is surfaced, not hidden behind a diff count', async () => {
  const s = await runRomVerification(ACTIVE, new Uint8Array([1, 2, 3]));
  assert.strictEqual(s.kind, 'ok');
  assert.notStrictEqual(s.report.active_len, s.report.reference_len);
  // The mismatch must be visible in the summary the UI renders.
  const text = describeVerification(s.report);
  assert.match(text, /length/i);
});

test('a blank reference is called out explicitly', async () => {
  const s = await runRomVerification(ACTIVE, new Uint8Array([0xff, 0xff, 0xff, 0xff]));
  assert.strictEqual(s.kind, 'ok');
  assert.strictEqual(s.report.reference_is_blank, true);
  assert.match(describeVerification(s.report), /blank/i);
});

test('describeVerification summarises a clean match', async () => {
  const s = await runRomVerification(ACTIVE, SAME);
  assert.strictEqual(s.kind, 'ok');
  assert.match(describeVerification(s.report), /match/i);
});

test('describeVerification reports the difference count', async () => {
  const s = await runRomVerification(ACTIVE, DIFFERENT);
  assert.strictEqual(s.kind, 'ok');
  assert.match(describeVerification(s.report), /1 differen/i);
});

test('verification does not mutate the buffers it is given', async () => {
  const active = new Uint8Array([1, 2, 3, 4]);
  const reference = new Uint8Array([1, 9, 3, 4]);
  const activeCopy = Uint8Array.from(active);
  const referenceCopy = Uint8Array.from(reference);

  await runRomVerification(active, reference);

  assert.deepStrictEqual(active, activeCopy);
  assert.deepStrictEqual(reference, referenceCopy);
});
