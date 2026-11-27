//! Tests for the hex dump view model.

import test from 'node:test';
import assert from 'node:assert/strict';
import {
  BYTES_PER_ROW,
  buildHexRows,
  findAdjacentMark,
  findContainingMark,
  formatHexCellValue,
  formatOffset,
  isMarked,
  parseAddress,
  printable,
} from './hexdumpModel.ts';

function bytes(values: number[]): Uint8Array {
  return new Uint8Array(values);
}

test('a full row shows sixteen hex pairs and sixteen ascii chars', () => {
  const data = bytes(Array.from({ length: 32 }, (_, i) => i));
  const rows = buildHexRows(data, 0, 16);
  assert.equal(rows.length, 1);
  assert.equal(rows[0].hex.split(' ').length, BYTES_PER_ROW);
  assert.equal(rows[0].ascii.length, BYTES_PER_ROW);
});

test('the hex is upper-case and zero padded', () => {
  const rows = buildHexRows(bytes([0x0a, 0xff]), 0, 2);
  assert.ok(rows[0].hex.startsWith('0A FF'));
});

test('a short final row is padded so the ascii column aligns', () => {
  const rows = buildHexRows(bytes([0x41, 0x42, 0x43]), 0, 3);
  // Padding is two spaces per empty slot, so a naive split would over-count.
  // The real invariant is the rendered width: 16 slots, 15 separators.
  const slots = (rows[0].hex.length + 1) / 3;
  assert.equal(slots, BYTES_PER_ROW);
  assert.equal(rows[0].ascii, 'ABC');
});

test('printable bytes render as themselves', () => {
  assert.equal(printable(0x41), 'A');
  assert.equal(printable(0x20), ' ');
});

test('non-printable bytes render as a dot', () => {
  assert.equal(printable(0x00), '.');
  assert.equal(printable(0x1f), '.');
  assert.equal(printable(0x7f), '.');
});

test('the offset is eight upper-case hex digits', () => {
  assert.equal(formatOffset(0), '00000000');
  assert.equal(formatOffset(0x1234), '00001234');
  assert.equal(formatOffset(0xdeadbeef), 'DEADBEEF');
});

test('rows are produced for the requested range only', () => {
  const data = bytes(Array.from({ length: 64 }, (_, i) => i));
  const rows = buildHexRows(data, 16, 32);
  assert.equal(rows.length, 2);
  assert.equal(rows[0].offset, 16);
  assert.equal(rows[1].offset, 32);
});

test('a range past the end is clipped to the data', () => {
  const data = bytes([1, 2, 3, 4]);
  const rows = buildHexRows(data, 0, 100);
  assert.equal(rows.length, 1);
  assert.equal(rows[0].ascii.length, 4);
});

test('a range entirely past the end yields nothing', () => {
  assert.deepEqual(buildHexRows(bytes([1, 2]), 10, 5), []);
});

test('a zero length range yields nothing', () => {
  assert.deepEqual(buildHexRows(bytes([1, 2]), 0, 0), []);
});

test('a marked range marks the rows it touches', () => {
  const data = bytes(Array.from({ length: 48 }, () => 0));
  const rows = buildHexRows(data, 0, 48, [{ start: 20, end: 24 }]);
  assert.equal(rows[0].marked, false, 'row 0..16 is before the mark');
  assert.equal(rows[1].marked, true, 'row 16..32 contains the mark');
  assert.equal(rows[2].marked, false);
});

test('a mark spanning two rows marks both', () => {
  const data = bytes(Array.from({ length: 48 }, () => 0));
  const rows = buildHexRows(data, 0, 48, [{ start: 10, end: 30 }]);
  assert.equal(rows[0].marked, true);
  assert.equal(rows[1].marked, true);
});

test('isMarked reports per-byte membership', () => {
  const marks = [{ start: 10, end: 20 }];
  assert.equal(isMarked(9, marks), false);
  assert.equal(isMarked(10, marks), true);
  assert.equal(isMarked(19, marks), true);
  assert.equal(isMarked(20, marks), false, 'end is exclusive');
});

test('parseAddress accepts 0x hex', () => {
  assert.equal(parseAddress('0x1000'), 0x1000);
  assert.equal(parseAddress('0XFF'), 0xFF);
});

test('parseAddress accepts plain decimal', () => {
  assert.equal(parseAddress('4096'), 4096);
});

test('parseAddress rejects nonsense', () => {
  assert.equal(parseAddress(''), null);
  assert.equal(parseAddress('zzz'), null);
  assert.equal(parseAddress('0x'), null);
});

test('a custom bytes-per-row is honoured', () => {
  const rows = buildHexRows(bytes(Array.from({ length: 16 }, () => 0)), 0, 16, [], 8);
  assert.equal(rows.length, 2);
});

test('formatHexCellValue formats 8-bit and 16-bit BE/LE values cleanly', () => {
  assert.equal(formatHexCellValue([0x0a], '8bit', 'big', 'hex'), '0A');
  assert.equal(formatHexCellValue([10], '8bit', 'big', 'dec'), ' 10');
  assert.equal(formatHexCellValue([0x12, 0x34], '16bit', 'big', 'hex'), '1234');
  assert.equal(formatHexCellValue([0x34, 0x12], '16bit', 'little', 'hex'), '1234');
  assert.equal(formatHexCellValue([0x00, 0xff], '16bit', 'big', 'dec'), '  255');
  assert.equal(formatHexCellValue([null], '8bit', 'big', 'hex'), '..');
});

test('findAdjacentMark and findContainingMark navigate marks with wrap-around', () => {
  const marks = [
    { start: 0x200, end: 0x240, name: 'Spark' },
    { start: 0x100, end: 0x140, name: 'Fuel' },
  ];
  assert.equal(findAdjacentMark(0, marks, 'next')?.name, 'Fuel');
  assert.equal(findAdjacentMark(0x100, marks, 'next')?.name, 'Spark');
  assert.equal(findAdjacentMark(0x300, marks, 'next')?.name, 'Fuel');
  assert.equal(findAdjacentMark(0x300, marks, 'prev')?.name, 'Spark');
  assert.equal(findAdjacentMark(0x50, marks, 'prev')?.name, 'Spark');
  assert.equal(findContainingMark(0x210, marks)?.name, 'Spark');
  assert.equal(findContainingMark(0x240, marks), null);
  assert.equal(findAdjacentMark(0, [], 'next'), null);
});
