import type { DType, MapDef } from '../../types/kivi.ts';

export interface BitFlagItem {
  bit: number;
  mask: number;
  maskHex: string;
  enabled: boolean;
}

export interface RawValueFormats {
  hex: string;
  dec: string;
  bin: string;
}

/**
 * Returns `true` when `mapDef` represents a 1x1 Scalar or Bitmask Flag parameter rather than a 1D curve or 2D calibration table.
 */
export function isScalarOrFlagMap(mapDef: MapDef | null | undefined): boolean {
  return Boolean(mapDef && mapDef.rows === 1 && mapDef.cols === 1);
}

/**
 * Returns `true` when a 1x1 parameter is configured as a bitmask switch/flag.
 */
export function isBitmaskFlagMap(mapDef: MapDef | null | undefined): boolean {
  if (!isScalarOrFlagMap(mapDef) || !mapDef) return false;
  const unit = (mapDef.scaling.unit || '').toLowerCase();
  const cat = (mapDef.category || '').toLowerCase();
  const eq = (mapDef.scaling.equation || '').toLowerCase();
  return (
    unit.includes('bitmask') ||
    unit.includes('flag') ||
    cat.includes('flag') ||
    cat.includes('switch') ||
    eq.includes('&')
  );
}

/** Returns integer bit width (`8`, `16`, or `32`) for `dtype`. */
export function dtypeBitWidth(dtype: DType): 8 | 16 | 32 {
  if (dtype === 'u8' || dtype === 'i8') return 8;
  if (dtype === 'u16' || dtype === 'i16') return 16;
  return 32;
}

/**
 * Decodes an integer scalar into MSB-first bit entries (`Bit N..0`) for interactive toggles.
 */
export function decodeBitmask(value: number, bitWidth: 8 | 16 = 8): BitFlagItem[] {
  const maskAll = bitWidth === 16 ? 0xffff : 0xff;
  const intVal = (Number.isFinite(value) ? Math.trunc(value) : 0) & maskAll;
  const items: BitFlagItem[] = [];
  for (let bit = bitWidth - 1; bit >= 0; bit--) {
    const mask = 1 << bit;
    items.push({
      bit,
      mask,
      maskHex: `0x${mask.toString(16).toUpperCase().padStart(bitWidth === 16 ? 4 : 2, '0')}`,
      enabled: (intVal & mask) !== 0,
    });
  }
  return items;
}

/** Toggles bit `bitIndex` on `value` within an unsigned 8- or 16-bit bitmask. */
export function toggleBitValue(value: number, bitIndex: number, bitWidth: 8 | 16 = 8): number {
  const maskAll = bitWidth === 16 ? 0xffff : 0xff;
  const maxBit = bitWidth - 1;
  if (!Number.isInteger(bitIndex) || bitIndex < 0 || bitIndex > maxBit) {
    return (Number.isFinite(value) ? Math.trunc(value) : 0) & maskAll;
  }
  const base = (Number.isFinite(value) ? Math.trunc(value) : 0) & maskAll;
  return (base ^ (1 << bitIndex)) & maskAll;
}

/** Formats a numeric value across synchronized Hex, Decimal, and Binary representations. */
export function formatRawRepresentations(value: number, dtype: DType): RawValueFormats {
  if (!Number.isFinite(value)) {
    return { hex: '--', dec: '--', bin: '--' };
  }
  const bits = dtypeBitWidth(dtype);
  const hexDigits = bits / 4;
  if (dtype === 'f32') {
    const buf = new ArrayBuffer(4);
    const dv = new DataView(buf);
    dv.setFloat32(0, value, false);
    const u32 = dv.getUint32(0, false);
    return {
      hex: `0x${u32.toString(16).toUpperCase().padStart(8, '0')}`,
      dec: value.toFixed(4),
      bin: u32.toString(2).padStart(32, '0'),
    };
  }
  const unsignedMask = bits === 8 ? 0xff : bits === 16 ? 0xffff : 0xffffffff;
  const unsigned = (Math.trunc(value) >>> 0) & unsignedMask;
  return {
    hex: `0x${(unsigned >>> 0).toString(16).toUpperCase().padStart(hexDigits, '0')}`,
    dec: String(Math.round(value)),
    bin: (unsigned >>> 0).toString(2).padStart(bits === 32 ? 16 : bits, '0'),
  };
}
