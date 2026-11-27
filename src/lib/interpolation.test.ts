import test from 'node:test';
import assert from 'node:assert/strict';
import {
  interpolate2D,
  interpolateHorizontal,
  interpolateVertical,
  adjustSelection,
  scaleSelection,
  setSelectionValue,
  smooth3x3,
} from './interpolation.ts';
import { getHeatmapColor, getHeatmapRgb, getPaletteRgb, COLOR_STOPS } from './colorRamp.ts';

test('interpolate2D: smoothly computes bilinear values for a 3x3 matrix', () => {
  const initial = [
    [10, 0, 20],
    [0, 0, 0],
    [30, 0, 40],
  ];

  const result = interpolate2D(initial, 0, 2, 0, 2);

  assert.strictEqual(result[0][0], 10);
  assert.strictEqual(result[0][1], 15);
  assert.strictEqual(result[0][2], 20);

  assert.strictEqual(result[1][0], 20);
  assert.strictEqual(result[1][1], 25);
  assert.strictEqual(result[1][2], 30);

  assert.strictEqual(result[2][0], 30);
  assert.strictEqual(result[2][1], 35);
  assert.strictEqual(result[2][2], 40);

  // Original matrix must not be mutated
  assert.strictEqual(initial[1][1], 0);
});

test('interpolate2D: sub-rectangle only modifies selected region', () => {
  const initial = [
    [100, 100, 100, 100],
    [100,  10,   0, 100],
    [100,   0,  30, 100],
    [100, 100, 100, 100],
  ];

  const result = interpolate2D(initial, 1, 2, 1, 2);

  // Outside borders stay 100
  assert.strictEqual(result[0][0], 100);
  assert.strictEqual(result[3][3], 100);
  assert.strictEqual(result[1][0], 100);
  assert.strictEqual(result[2][3], 100);

  // Inside corners
  assert.strictEqual(result[1][1], 10);
  assert.strictEqual(result[2][2], 30);
});

test('interpolate2D: single cell returns unchanged matrix copy', () => {
  const initial = [
    [1, 2],
    [3, 4],
  ];
  const result = interpolate2D(initial, 1, 1, 1, 1);
  assert.deepStrictEqual(result, initial);
  assert.notStrictEqual(result, initial);
});

test('interpolateHorizontal: interpolates each row independently', () => {
  const initial = [
    [10, 0, 30],
    [50, 0, 90],
  ];

  const result = interpolateHorizontal(initial, 0, 1, 0, 2);

  assert.strictEqual(result[0][0], 10);
  assert.strictEqual(result[0][1], 20);
  assert.strictEqual(result[0][2], 30);

  assert.strictEqual(result[1][0], 50);
  assert.strictEqual(result[1][1], 70);
  assert.strictEqual(result[1][2], 90);
});

test('interpolateVertical: interpolates each column independently', () => {
  const initial = [
    [10, 50],
    [0, 0],
    [30, 90],
  ];

  const result = interpolateVertical(initial, 0, 2, 0, 1);

  assert.strictEqual(result[0][0], 10);
  assert.strictEqual(result[1][0], 20);
  assert.strictEqual(result[2][0], 30);

  assert.strictEqual(result[0][1], 50);
  assert.strictEqual(result[1][1], 70);
  assert.strictEqual(result[2][1], 90);
});

test('adjustSelection: increments or decrements selection range', () => {
  const initial = [
    [10, 20],
    [30, 40],
  ];

  const plusOne = adjustSelection(initial, 0, 1, 0, 0, 1);
  assert.strictEqual(plusOne[0][0], 11);
  assert.strictEqual(plusOne[1][0], 31);
  assert.strictEqual(plusOne[0][1], 20);
  assert.strictEqual(plusOne[1][1], 40);

  const minusTwo = adjustSelection(initial, 0, 0, 0, 1, -2);
  assert.strictEqual(minusTwo[0][0], 8);
  assert.strictEqual(minusTwo[0][1], 18);
});

test('scaleSelection: scales selected cells by percentage', () => {
  const initial = [
    [100, 200],
    [300, 400],
  ];

  const scaled = scaleSelection(initial, 0, 0, 0, 1, 10); // +10%
  assert.strictEqual(scaled[0][0], 110);
  assert.strictEqual(scaled[0][1], 220);
  assert.strictEqual(scaled[1][0], 300); // untouched
});

test('setSelectionValue: sets selected cells to constant', () => {
  const initial = [
    [10, 20],
    [30, 40],
  ];

  const fixed = setSelectionValue(initial, 0, 1, 1, 1, 99);
  assert.strictEqual(fixed[0][0], 10);
  assert.strictEqual(fixed[0][1], 99);
  assert.strictEqual(fixed[1][0], 30);
  assert.strictEqual(fixed[1][1], 99);
});

test('colorRamp: correctly blends 3-stop heatmap colors', () => {
  // Min stop (Blue)
  const minRgb = getHeatmapRgb(0, 0, 100);
  assert.deepStrictEqual(minRgb, COLOR_STOPS.MIN_BLUE);
  assert.strictEqual(getHeatmapColor(0, 0, 100), `rgb(30, 64, 175)`);

  // Mid stop (Green)
  const midRgb = getHeatmapRgb(50, 0, 100);
  assert.deepStrictEqual(midRgb, COLOR_STOPS.MID_GREEN);
  assert.strictEqual(getHeatmapColor(50, 0, 100), `rgb(22, 163, 74)`);

  // Max stop (Red)
  const maxRgb = getHeatmapRgb(100, 0, 100);
  assert.deepStrictEqual(maxRgb, COLOR_STOPS.MAX_RED);
  assert.strictEqual(getHeatmapColor(100, 0, 100), `rgb(220, 38, 38)`);

  // Equal min and max returns neutral fallback
  assert.strictEqual(getHeatmapColor(50, 50, 50), '#1e293b');
});

test('smooth3x3: smooths peak across 3x3 neighborhood with boundary normalization', () => {
  const initial = [
    [0, 0, 0],
    [0, 16, 0],
    [0, 0, 0],
  ];

  const smoothed = smooth3x3(initial, 0, 2, 0, 2);
  // Center: (16 * 4) / 16 = 4
  assert.strictEqual(smoothed[1][1], 4);
  // Edge neighbor (0, 1): (16 * 2) / (1+2+1+2+4+2) = 32 / 12 = 2.666667
  assert.strictEqual(Math.round(smoothed[0][1] * 100) / 100, 2.67);
});

test('smooth3x3: sub-rectangle only affects selected region', () => {
  const initial = [
    [10, 20, 30],
    [40, 50, 60],
    [70, 80, 90],
  ];

  const smoothed = smooth3x3(initial, 1, 1, 1, 1);
  assert.strictEqual(smoothed[0][0], 10);
  assert.strictEqual(smoothed[0][1], 20);
  assert.strictEqual(smoothed[2][2], 90);
  // Cell (1,1) was smoothed with its neighbors
  assert.strictEqual(typeof smoothed[1][1], 'number');
});

test('colorRamp: handles thermal, spectrum, and monochrome palettes', () => {
  // Thermal
  const thermalMin = getPaletteRgb(0, 0, 100, 'thermal');
  assert.strictEqual(thermalMin.r, 2);
  assert.strictEqual(thermalMin.g, 6);
  assert.strictEqual(thermalMin.b, 23);

  const thermalMax = getPaletteRgb(100, 0, 100, 'thermal');
  assert.strictEqual(thermalMax.r, 239);
  assert.strictEqual(thermalMax.g, 68);
  assert.strictEqual(thermalMax.b, 68);

  // Spectrum
  const specMin = getPaletteRgb(0, 0, 100, 'spectrum');
  assert.strictEqual(specMin.r, 124);
  assert.strictEqual(specMin.g, 58);
  assert.strictEqual(specMin.b, 237);

  // Monochrome
  const monoMin = getPaletteRgb(0, 0, 100, 'monochrome');
  assert.strictEqual(monoMin.r, 15);
  const monoMax = getPaletteRgb(100, 0, 100, 'monochrome');
  assert.strictEqual(monoMax.r, 248);

  // Heatmap color with palette
  const cssThermal = getHeatmapColor(100, 0, 100, 'thermal');
  assert.strictEqual(cssThermal, 'rgb(239, 68, 68)');
});
