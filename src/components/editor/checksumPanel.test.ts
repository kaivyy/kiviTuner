import test from 'node:test';
import assert from 'node:assert/strict';
import { runChecksum, CHECKSUM_ALGORITHMS } from './checksumPanel.ts';

const ASCII_123456789 = new Uint8Array([0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39]);

test('runChecksum reports no-rom for null input', async () => {
  const s = await runChecksum('crc32_ieee', null);
  assert.strictEqual(s.kind, 'no-rom');
});

test('runChecksum reports no-rom for empty input', async () => {
  const s = await runChecksum('crc32_ieee', new Uint8Array(0));
  assert.strictEqual(s.kind, 'no-rom');
});

test('runChecksum returns CRC-32/IEEE for the standard test vector', async () => {
  const s = await runChecksum('crc32_ieee', ASCII_123456789);
  assert.strictEqual(s.kind, 'ok');
  assert.strictEqual(s.value, 0xcbf43926);
  assert.strictEqual(s.widthBits, 32);
  assert.strictEqual(s.inputLen, 9);
});

test('runChecksum returns CRC-32C for the standard test vector', async () => {
  const s = await runChecksum('crc32_castagnoli', ASCII_123456789);
  assert.strictEqual(s.kind, 'ok');
  assert.strictEqual(s.value, 0xe3069283);
});

test('runChecksum returns CRC-16/CCITT with 16-bit width', async () => {
  const s = await runChecksum('crc16_ccitt', ASCII_123456789);
  assert.strictEqual(s.kind, 'ok');
  assert.strictEqual(s.value, 0x29b1);
  assert.strictEqual(s.widthBits, 16);
});

test('runChecksum reports unsupported for crc16_modbus', async () => {
  const s = await runChecksum('crc16_modbus', ASCII_123456789);
  assert.strictEqual(s.kind, 'unsupported');
});

test('runChecksum does not mutate the rom buffer', async () => {
  const rom = new Uint8Array([1, 2, 3, 4]);
  const before = Array.from(rom);
  await runChecksum('crc32_ieee', rom);
  assert.deepStrictEqual(Array.from(rom), before);
});

test('formatChecksumValue pads to the correct width', async () => {
  const { formatChecksumValue } = await import('./checksumPanel.ts');
  assert.strictEqual(formatChecksumValue(0xcbf43926, 32), '0xCBF43926');
  assert.strictEqual(formatChecksumValue(0x29b1, 16), '0x29B1');
  assert.strictEqual(formatChecksumValue(0x1, 32), '0x00000001');
  assert.strictEqual(formatChecksumValue(0x1, 16), '0x0001');
});

test('CHECKSUM_ALGORITHMS lists exactly the backend-supported set', () => {
  const ids = CHECKSUM_ALGORITHMS.map((a) => a.id);
  assert.deepStrictEqual(ids, [
    'crc32_ieee',
    'crc32_castagnoli',
    'crc32_simos',
    'crc16_ccitt',
    'crc16_modbus',
  ]);
});
