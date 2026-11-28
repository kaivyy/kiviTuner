import { test } from 'node:test';
import assert from 'node:assert/strict';
import { computeHorizontalScrollState } from './useHorizontalStripScroll.ts';

test('computeHorizontalScrollState reports no overflow within 2px tolerance', () => {
  assert.deepEqual(computeHorizontalScrollState(0, 320, 320), {
    hasOverflow: false,
    canLeft: false,
    canRight: false,
  });
  assert.deepEqual(computeHorizontalScrollState(0, 320, 322), {
    hasOverflow: false,
    canLeft: false,
    canRight: false,
  });
});

test('computeHorizontalScrollState tracks start, middle, and end scroll positions', () => {
  assert.deepEqual(computeHorizontalScrollState(0, 240, 520), {
    hasOverflow: true,
    canLeft: false,
    canRight: true,
  });

  assert.deepEqual(computeHorizontalScrollState(80, 240, 520), {
    hasOverflow: true,
    canLeft: true,
    canRight: true,
  });

  assert.deepEqual(computeHorizontalScrollState(280, 240, 520), {
    hasOverflow: true,
    canLeft: true,
    canRight: false,
  });

  // Subpixel tolerance near the right edge
  assert.deepEqual(computeHorizontalScrollState(279, 240, 520), {
    hasOverflow: true,
    canLeft: true,
    canRight: false,
  });
});
