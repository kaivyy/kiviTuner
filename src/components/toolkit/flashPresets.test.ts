import test from 'node:test';
import assert from 'node:assert/strict';
import {
  appendFlashLog,
  FALLBACK_MOTO_PROFILES,
  formatByteSize,
  formatHexAddr,
  inferRecommendedProtocol,
  parseHexOrDec,
  STANDARD_ROM_PRESETS,
  validateMotoWriteRange,
} from './flashPresets.ts';

test('parseHexOrDec parses hex and decimal strings cleanly and rejects invalid text', () => {
  assert.equal(parseHexOrDec('0x08000'), 0x8000);
  assert.equal(parseHexOrDec(' 524288 '), 524288);
  assert.ok(Number.isNaN(parseHexOrDec('')));
  assert.ok(Number.isNaN(parseHexOrDec('0xZZ')));
});

test('formatHexAddr and formatByteSize produce beginner-friendly readouts', () => {
  assert.equal(formatHexAddr(0x8000, 5), '0x08000');
  assert.equal(formatByteSize(256), '256 B');
  assert.equal(formatByteSize(512 * 1024), '512 KiB');
  assert.equal(formatByteSize(1024 * 1024), '1 MiB');
  assert.equal(formatByteSize(56 * 1024), '56 KiB');
});

test('standard presets and motorcycle profiles cover key sizes and K-Line defaults', () => {
  assert.ok(STANDARD_ROM_PRESETS.some((p) => p.sizeBytes === 512 * 1024));
  const cbr250rr = FALLBACK_MOTO_PROFILES.find((p) => p.id === 'honda_cbr250rr_k64');
  assert.ok(cbr250rr);
  assert.equal(inferRecommendedProtocol(cbr250rr), 'honda_keihin_k_line');

  const yamaha128 = FALLBACK_MOTO_PROFILES.find((p) => p.id === 'yamaha_128k_kline');
  assert.ok(yamaha128);
  assert.equal(inferRecommendedProtocol(yamaha128), 'uds_standard');
});

test('validateMotoWriteRange blocks protected bootloader overlaps and allows valid writable windows', () => {
  const cbr250rr = FALLBACK_MOTO_PROFILES.find((p) => p.id === 'honda_cbr250rr_k64')!;
  const blocked = validateMotoWriteRange(cbr250rr, 0x0000, 512 * 1024);
  assert.equal(blocked.ok, false);
  assert.match(blocked.reason ?? '', /protected bootloader/i);

  const valid = validateMotoWriteRange(cbr250rr, 0x08000, 0x80000 - 0x08000);
  assert.equal(valid.ok, true);
  assert.equal(valid.reason, null);

  const yamaha64 = FALLBACK_MOTO_PROFILES.find((p) => p.id === 'yamaha_64k_kline')!;
  assert.equal(validateMotoWriteRange(yamaha64, 0x0000, 0x8000).ok, false);
  assert.equal(validateMotoWriteRange(yamaha64, 0x1800, 0x7fff - 0x1800 + 1).ok, true);
});

test('appendFlashLog formats timestamps and enforces max entry cap', () => {
  const fixedDate = new Date('2026-11-27T09:05:03');
  let logs = appendFlashLog([], 'info', 'Preset applied', fixedDate, 3);
  logs = appendFlashLog(logs, 'ok', 'Second entry', fixedDate, 3);
  logs = appendFlashLog(logs, 'warn', 'Third entry', fixedDate, 3);
  logs = appendFlashLog(logs, 'error', 'Fourth entry', fixedDate, 3);
  assert.equal(logs.length, 3);
  assert.equal(logs[0].message, 'Second entry');
  assert.equal(logs[2].time, '09:05:03');
  assert.equal(logs[2].id, 4);
});
