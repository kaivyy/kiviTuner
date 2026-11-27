/**
 * B4a: parsing of a user-entered ROM base address.
 *
 * Empty input means "unknown" — it must not become zero. Anything malformed
 * must be rejected with an actionable message rather than coerced.
 */

import test from 'node:test';
import assert from 'node:assert/strict';
import { parseBaseAddressInput } from './tauriBridge.ts';

test('empty input means unknown, not zero', () => {
  const r = parseBaseAddressInput('');
  assert.deepStrictEqual(r, { ok: true, value: null });
});

test('whitespace-only input means unknown', () => {
  const r = parseBaseAddressInput('   ');
  assert.deepStrictEqual(r, { ok: true, value: null });
});

test('parses a bare hex string', () => {
  const r = parseBaseAddressInput('8000');
  assert.deepStrictEqual(r, { ok: true, value: 0x8000 });
});

test('parses a 0x-prefixed address', () => {
  const r = parseBaseAddressInput('0x10000');
  assert.deepStrictEqual(r, { ok: true, value: 0x10000 });
});

test('parses an uppercase 0X prefix', () => {
  const r = parseBaseAddressInput('0XFF');
  assert.deepStrictEqual(r, { ok: true, value: 255 });
});

test('explicit zero is a known address, distinct from unknown', () => {
  const zero = parseBaseAddressInput('0');
  const unknown = parseBaseAddressInput('');
  assert.deepStrictEqual(zero, { ok: true, value: 0 });
  assert.deepStrictEqual(unknown, { ok: true, value: null });
  assert.notDeepStrictEqual(zero, unknown);
});

test('accepts the u32 maximum', () => {
  const r = parseBaseAddressInput('0xFFFFFFFF');
  assert.deepStrictEqual(r, { ok: true, value: 0xffffffff });
});

test('rejects a value beyond u32', () => {
  const r = parseBaseAddressInput('0x100000000');
  assert.strictEqual(r.ok, false);
});

test('rejects non-hex characters', () => {
  const r = parseBaseAddressInput('0xZZ');
  assert.strictEqual(r.ok, false);
});

test('rejects a bare 0x prefix', () => {
  const r = parseBaseAddressInput('0x');
  assert.strictEqual(r.ok, false);
});

test('rejects a negative value', () => {
  const r = parseBaseAddressInput('-10');
  assert.strictEqual(r.ok, false);
});

test('trims surrounding whitespace', () => {
  const r = parseBaseAddressInput('  0x4000  ');
  assert.deepStrictEqual(r, { ok: true, value: 0x4000 });
});
