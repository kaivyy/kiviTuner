import { test } from 'node:test';
import assert from 'node:assert/strict';
import type { MapDef } from '../../types/kivi.ts';
import {
  decodeBitmask,
  formatRawRepresentations,
  isBitmaskFlagMap,
  isScalarOrFlagMap,
  toggleBitValue,
} from './scalarFlagModel.ts';

function sampleMap(overrides: Partial<MapDef> = {}): MapDef {
  return {
    id: 'scalar_rev',
    name: 'Rev Limit',
    category: 'Limiters',
    address: 0x1200,
    rows: 1,
    cols: 1,
    dtype: 'u16',
    endian: 'big',
    scaling: { equation: 'X', unit: 'RPM', decimals: 0 },
    ...overrides,
  };
}

test('isScalarOrFlagMap and isBitmaskFlagMap classify 1x1 vs multi-cell maps', () => {
  assert.equal(isScalarOrFlagMap(sampleMap()), true);
  assert.equal(isScalarOrFlagMap(sampleMap({ rows: 1, cols: 16 })), false);
  assert.equal(isScalarOrFlagMap(null), false);

  assert.equal(isBitmaskFlagMap(sampleMap()), false);
  assert.equal(
    isBitmaskFlagMap(
      sampleMap({
        category: 'Flags',
        dtype: 'u8',
        scaling: { equation: 'X', unit: 'bitmask', decimals: 0 },
      })
    ),
    true
  );
});

test('decodeBitmask and toggleBitValue decode and toggle MSB-first flags cleanly', () => {
  const bits = decodeBitmask(0x81, 8);
  assert.equal(bits.length, 8);
  assert.equal(bits[0].bit, 7);
  assert.equal(bits[0].maskHex, '0x80');
  assert.equal(bits[0].enabled, true);
  assert.equal(bits[7].bit, 0);
  assert.equal(bits[7].enabled, true);
  assert.equal(bits[3].enabled, false);

  assert.equal(toggleBitValue(0x81, 0, 8), 0x80);
  assert.equal(toggleBitValue(0x80, 3, 8), 0x88);
  assert.equal(toggleBitValue(0x00, 15, 16), 0x8000);
});

test('formatRawRepresentations returns hex, dec, and binary strings', () => {
  const f = formatRawRepresentations(0x0a, 'u8');
  assert.equal(f.hex, '0x0A');
  assert.equal(f.dec, '10');
  assert.equal(f.bin, '00001010');
});
