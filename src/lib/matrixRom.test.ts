import test from 'node:test';
import assert from 'node:assert/strict';
import {
  readRawValue,
  writeRawValue,
  evaluateScaling,
  invertScaling,
  extractMatrixFromRom,
  writeMatrixToRom,
  generateAxisLabels,
  resolveDisplayAxisLabels,
} from './matrixRom.ts';
import type { MapDef } from '../types/kivi';

test('readRawValue and writeRawValue: round-trips u8 and u16', () => {
  const buf = new Uint8Array(8);

  // u8
  writeRawValue(buf, 0, 200, 'u8');
  assert.strictEqual(readRawValue(buf, 0, 'u8'), 200);

  // u16 little endian
  writeRawValue(buf, 2, 45000, 'u16', 'little');
  assert.strictEqual(readRawValue(buf, 2, 'u16', 'little'), 45000);

  // u16 big endian
  writeRawValue(buf, 4, 32100, 'u16', 'big');
  assert.strictEqual(readRawValue(buf, 4, 'u16', 'big'), 32100);
});

test('evaluateScaling and invertScaling: handles linear equations', () => {
  // X * 0.5
  assert.strictEqual(evaluateScaling(200, 'X * 0.5'), 100);
  assert.strictEqual(invertScaling(100, 'X * 0.5'), 200);

  // X * 0.25
  assert.strictEqual(evaluateScaling(80, 'X * 0.25'), 20);
  assert.strictEqual(invertScaling(20, 'X * 0.25'), 80);

  // Raw X
  assert.strictEqual(evaluateScaling(123, 'X'), 123);
  assert.strictEqual(invertScaling(123, 'X'), 123);
});

test('extractMatrixFromRom and writeMatrixToRom: round-trips 2D maps', () => {
  const rom = new Uint8Array(256);
  const map: MapDef = {
    id: 'test_map',
    name: 'Test Map',
    category: 'Fuel',
    address: 0,
    rows: 2,
    cols: 2,
    dtype: 'u8',
    endian: 'little',
    scaling: { equation: 'X * 0.5', unit: '%', decimals: 1 },
  };

  // Pre-fill rom bytes with 20, 40, 60, 80
  rom[0] = 20; // scaled: 10
  rom[1] = 40; // scaled: 20
  rom[2] = 60; // scaled: 30
  rom[3] = 80; // scaled: 40

  const matrix = extractMatrixFromRom(rom, map);
  assert.deepStrictEqual(matrix, [
    [10, 20],
    [30, 40],
  ]);

  // Modify cell
  matrix[0][1] = 25; // raw should become 50
  const updatedRom = writeMatrixToRom(rom, map, matrix);
  assert.strictEqual(updatedRom[1], 50);
});

test('generateAxisLabels: creates realistic RPM and TPS labels', () => {
  const { xLabels, yLabels, xAxisName, yAxisName } = generateAxisLabels(4, 4, 'Main Fuel');
  assert.strictEqual(xAxisName, 'TPS (%)');
  assert.strictEqual(yAxisName, 'RPM');
  assert.strictEqual(xLabels.length, 4);
  assert.strictEqual(yLabels.length, 4);
  assert.strictEqual(xLabels[0], '0%');
  assert.strictEqual(xLabels[3], '100%');
  assert.strictEqual(yLabels[0], '800');
  assert.strictEqual(yLabels[3], '8000');
});

test('readRawValue, writeRawValue, and stride decoding handle u32, i32, f32 LE/BE and affine equations', () => {
  const buf = new Uint8Array(32);

  writeRawValue(buf, 0, 305419896, 'u32', 'big');
  assert.strictEqual(readRawValue(buf, 0, 'u32', 'big'), 305419896);

  writeRawValue(buf, 4, -123456, 'i32', 'little');
  assert.strictEqual(readRawValue(buf, 4, 'i32', 'little'), -123456);

  writeRawValue(buf, 8, 12.5, 'f32', 'little');
  assert.ok(Math.abs(readRawValue(buf, 8, 'f32', 'little') - 12.5) < 1e-5);

  writeRawValue(buf, 12, -3.25, 'f32', 'big');
  assert.ok(Math.abs(readRawValue(buf, 12, 'f32', 'big') + 3.25) < 1e-5);

  // Affine scaling: X * 0.5 + 10
  assert.strictEqual(evaluateScaling(20, 'X * 0.5 + 10'), 20);
  assert.strictEqual(invertScaling(20, 'X * 0.5 + 10'), 20);

  // Stride-aware extraction (minor_stride_bits=16 for u8 every 2 bytes)
  const stridedRom = new Uint8Array([10, 0xff, 20, 0xff, 30, 0xff, 40, 0xff]);
  const stridedMap: MapDef = {
    id: 'strided',
    name: 'Strided Table',
    category: 'Fuel',
    address: 0,
    rows: 2,
    cols: 2,
    dtype: 'u8',
    endian: 'big',
    minor_stride_bits: 16,
    major_stride_bits: 32,
    scaling: { equation: 'X', unit: '%', decimals: 0 },
    x_axis: {
      address: 0,
      length: 2,
      dtype: 'u16',
      endian: 'big',
      scaling: { equation: 'X', unit: 'TPS', decimals: 0 },
      values: [10, 90],
    },
    y_axis: {
      address: 0,
      length: 2,
      dtype: 'u16',
      endian: 'big',
      scaling: { equation: 'X', unit: 'RPM', decimals: 0 },
      values: [1000, 6000],
    },
  };
  assert.deepStrictEqual(extractMatrixFromRom(stridedRom, stridedMap), [
    [10, 20],
    [30, 40],
  ]);

  const labels = resolveDisplayAxisLabels(stridedMap);
  assert.deepStrictEqual(labels.xLabels, ['10', '90']);
  assert.deepStrictEqual(labels.yLabels, ['1000', '6000']);
  assert.ok(labels.xAxisName.includes('TPS'));
  assert.ok(labels.yAxisName.includes('RPM'));
});
