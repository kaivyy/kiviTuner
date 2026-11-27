import type { MapDef, DType, Endian } from '../types/kivi';

export function dtypeByteSize(dtype: DType): number {
  if (dtype === 'u16' || dtype === 'i16') return 2;
  if (dtype === 'u32' || dtype === 'i32' || dtype === 'f32') return 4;
  return 1;
}

/**
 * Reads a typed raw value from ROM buffer at specified byte offset.
 */
export function readRawValue(
  rom: Uint8Array,
  offset: number,
  dtype: DType = 'u8',
  endian: Endian = 'little'
): number {
  if (offset < 0 || offset >= rom.length) return 0;
  const le = endian === 'little';
  if (dtype === 'u8') {
    return rom[offset];
  }
  if (dtype === 'i8') {
    const u = rom[offset];
    return u >= 128 ? u - 256 : u;
  }
  if (dtype === 'u16') {
    if (offset + 1 >= rom.length) return rom[offset];
    return le ? rom[offset] | (rom[offset + 1] << 8) : (rom[offset] << 8) | rom[offset + 1];
  }
  if (dtype === 'i16') {
    if (offset + 1 >= rom.length) return rom[offset];
    const u = le ? rom[offset] | (rom[offset + 1] << 8) : (rom[offset] << 8) | rom[offset + 1];
    return u >= 32768 ? u - 65536 : u;
  }
  if (offset + 3 < rom.length) {
    const view = new DataView(rom.buffer, rom.byteOffset + offset, 4);
    if (dtype === 'u32') return view.getUint32(0, le);
    if (dtype === 'i32') return view.getInt32(0, le);
    if (dtype === 'f32') {
      const f = view.getFloat32(0, le);
      return Number.isFinite(f) ? f : 0;
    }
  }
  return rom[offset];
}

/**
 * Writes a typed raw value into ROM buffer at specified byte offset.
 */
export function writeRawValue(
  rom: Uint8Array,
  offset: number,
  value: number,
  dtype: DType = 'u8',
  endian: Endian = 'little'
): void {
  if (offset < 0 || offset >= rom.length || !Number.isFinite(value)) return;
  const le = endian === 'little';
  if (dtype === 'u8') {
    rom[offset] = Math.max(0, Math.min(255, Math.round(value))) & 0xff;
  } else if (dtype === 'i8') {
    const val = Math.max(-128, Math.min(127, Math.round(value)));
    rom[offset] = (val < 0 ? val + 256 : val) & 0xff;
  } else if (dtype === 'u16') {
    if (offset + 1 >= rom.length) return;
    const clamped = Math.max(0, Math.min(65535, Math.round(value)));
    if (le) {
      rom[offset] = clamped & 0xff;
      rom[offset + 1] = (clamped >> 8) & 0xff;
    } else {
      rom[offset] = (clamped >> 8) & 0xff;
      rom[offset + 1] = clamped & 0xff;
    }
  } else if (dtype === 'i16') {
    if (offset + 1 >= rom.length) return;
    const clampedInt = Math.max(-32768, Math.min(32767, Math.round(value)));
    const clamped = clampedInt < 0 ? clampedInt + 65536 : clampedInt;
    if (le) {
      rom[offset] = clamped & 0xff;
      rom[offset + 1] = (clamped >> 8) & 0xff;
    } else {
      rom[offset] = (clamped >> 8) & 0xff;
      rom[offset + 1] = clamped & 0xff;
    }
  } else if (offset + 3 < rom.length) {
    const view = new DataView(rom.buffer, rom.byteOffset + offset, 4);
    if (dtype === 'u32') {
      view.setUint32(0, Math.max(0, Math.min(0xffffffff, Math.round(value))), le);
    } else if (dtype === 'i32') {
      view.setInt32(0, Math.max(-2147483648, Math.min(2147483647, Math.round(value))), le);
    } else if (dtype === 'f32') {
      view.setFloat32(0, value, le);
    }
  }
}

interface LinearScalingCoeff {
  scale: number;
  offset: number;
}

function parseLinearScaling(equation?: string): LinearScalingCoeff | null {
  if (!equation) return null;
  const eq = equation.replace(/\s+/g, '');
  if (eq === '' || /^x$/i.test(eq)) return { scale: 1, offset: 0 };

  let m: RegExpMatchArray | null;
  if ((m = eq.match(/^\(?x\*([+-]?\d*\.?\d+)(?:e([+-]?\d+))?\)?([+-]\d*\.?\d+(?:e[+-]?\d+)?)?$/i))) {
    const scale = parseFloat(m[1] + (m[2] ? `e${m[2]}` : ''));
    const offset = m[3] ? parseFloat(m[3]) : 0;
    if (Number.isFinite(scale) && Number.isFinite(offset)) return { scale, offset };
  }
  if ((m = eq.match(/^([+-]?\d*\.?\d+(?:e[+-]?\d+)?)\*x([+-]\d*\.?\d+(?:e[+-]?\d+)?)?$/i))) {
    const scale = parseFloat(m[1]);
    const offset = m[2] ? parseFloat(m[2]) : 0;
    if (Number.isFinite(scale) && Number.isFinite(offset)) return { scale, offset };
  }
  if ((m = eq.match(/^\(?x\/([+-]?\d*\.?\d+(?:e[+-]?\d+)?)\)?([+-]\d*\.?\d+(?:e[+-]?\d+)?)?$/i))) {
    const denom = parseFloat(m[1]);
    const offset = m[2] ? parseFloat(m[2]) : 0;
    if (Number.isFinite(denom) && denom !== 0 && Number.isFinite(offset)) {
      return { scale: 1 / denom, offset };
    }
  }
  if ((m = eq.match(/^\(x([+-]\d*\.?\d+(?:e[+-]?\d+)?)\)([*/])([+-]?\d*\.?\d+(?:e[+-]?\d+)?)$/i))) {
    const innerOffset = parseFloat(m[1]);
    const op = m[2];
    const factor = parseFloat(m[3]);
    if (Number.isFinite(innerOffset) && Number.isFinite(factor) && !(op === '/' && factor === 0)) {
      const scale = op === '*' ? factor : 1 / factor;
      return { scale, offset: innerOffset * scale };
    }
  }
  if ((m = eq.match(/^x([+-]\d*\.?\d+(?:e[+-]?\d+)?)$/i))) {
    const offset = parseFloat(m[1]);
    if (Number.isFinite(offset)) return { scale: 1, offset };
  }
  return null;
}

/**
 * Evaluates calibration equation from raw value to engineering unit (e.g. X * 0.5 - 40).
 */
export function evaluateScaling(raw: number, equation?: string): number {
  const coeff = parseLinearScaling(equation);
  if (!coeff) return raw;
  const v = raw * coeff.scale + coeff.offset;
  return Number.isFinite(v) ? v : raw;
}

/**
 * Inverts engineering unit back into raw value for writing to ROM.
 */
export function invertScaling(val: number, equation?: string, preserveFloat = false): number {
  const coeff = parseLinearScaling(equation);
  if (!coeff || coeff.scale === 0) return preserveFloat ? val : Math.round(val);
  const raw = (val - coeff.offset) / coeff.scale;
  if (!Number.isFinite(raw)) return preserveFloat ? val : Math.round(val);
  return preserveFloat ? raw : Math.round(raw);
}

function cellByteOffset(map: MapDef, row: number, col: number, bytesPerElem: number): number {
  const elemBits = bytesPerElem * 8;
  const rowBits = !map.major_stride_bits ? map.cols * elemBits : map.major_stride_bits;
  const colBits = !map.minor_stride_bits ? elemBits : map.minor_stride_bits;
  const gridRow = map.column_major ? col : row;
  const gridCol = map.column_major ? row : col;
  return Math.floor((map.address * 8 + gridRow * rowBits + gridCol * colBits) / 8);
}

/**
 * Extracts a 2D matrix of numbers from ROM data for a given MapDef.
 */
export function extractMatrixFromRom(rom: Uint8Array, map: MapDef): number[][] {
  const bytesPerElem = dtypeByteSize(map.dtype);
  const matrix: number[][] = [];
  const decimals = map.scaling.decimals ?? 1;

  for (let r = 0; r < map.rows; r++) {
    const row: number[] = [];
    for (let c = 0; c < map.cols; c++) {
      const offset = cellByteOffset(map, r, c, bytesPerElem);
      const raw = readRawValue(rom, offset, map.dtype, map.endian);
      const scaled = evaluateScaling(raw, map.scaling.equation);
      row.push(Number.isFinite(scaled) ? Number(scaled.toFixed(decimals)) : 0);
    }
    matrix.push(row);
  }
  return matrix;
}

/**
 * Writes back a modified 2D matrix into a new ROM buffer copy.
 */
export function writeMatrixToRom(
  rom: Uint8Array,
  map: MapDef,
  matrix: number[][]
): Uint8Array {
  const nextRom = new Uint8Array(rom);
  const bytesPerElem = dtypeByteSize(map.dtype);
  const preserveFloat = map.dtype === 'f32';

  for (let r = 0; r < Math.min(map.rows, matrix.length); r++) {
    for (let c = 0; c < Math.min(map.cols, matrix[r].length); c++) {
      const offset = cellByteOffset(map, r, c, bytesPerElem);
      const scaled = matrix[r][c];
      const raw = invertScaling(scaled, map.scaling.equation, preserveFloat);
      writeRawValue(nextRom, offset, raw, map.dtype, map.endian);
    }
  }
  return nextRom;
}

/**
 * Resolves real axis breakpoints from a MapDef when available, falling back to clean indices
 * without inventing misleading synthetic engineering values.
 */
export function resolveDisplayAxisLabels(map: MapDef | undefined | null): {
  xLabels: (string | number)[];
  yLabels: (string | number)[];
  xAxisName: string;
  yAxisName: string;
} {
  if (!map) {
    return { xLabels: [], yLabels: [], xAxisName: 'X', yAxisName: 'Y' };
  }
  const formatVals = (vals: number[] | undefined, count: number, decimals = 1, prefix = ''): string[] => {
    if (vals && vals.length === count) {
      return vals.map((v) =>
        Number.isInteger(v) ? String(v) : Number(v.toFixed(decimals)).toString()
      );
    }
    return Array.from({ length: count }, (_, i) => `${prefix}${i}`);
  };
  const xLabels = formatVals(map.x_axis?.values, map.cols, map.x_axis?.scaling?.decimals ?? 1, 'C');
  const yLabels = formatVals(map.y_axis?.values, map.rows, map.y_axis?.scaling?.decimals ?? 1, 'R');
  const xUnit = map.x_axis?.scaling?.unit?.trim();
  const yUnit = map.y_axis?.scaling?.unit?.trim();
  const xAxisName = map.x_axis?.values?.length === map.cols
    ? (xUnit ? `X (${xUnit})` : 'X Axis')
    : 'Col';
  const yAxisName = map.y_axis?.values?.length === map.rows
    ? (yUnit ? `Y (${yUnit})` : 'Y Axis')
    : 'Row';
  return { xLabels, yLabels, xAxisName, yAxisName };
}

/**
 * Generates synthetic or user-defined axis labels for RPM (Y-axis) and TPS/MAP (X-axis).
 */
export function generateAxisLabels(
  rows: number,
  cols: number,
  mapName: string = ''
): { xLabels: string[]; yLabels: string[]; xAxisName: string; yAxisName: string } {
  const isBoost = mapName.toLowerCase().includes('boost');

  const xAxisName = isBoost ? 'MAP (kPa)' : 'TPS (%)';
  const yAxisName = 'RPM';

  // Realistic RPM breakpoints (Y-axis)
  const yLabels: string[] = [];
  const minRpm = 800;
  const maxRpm = 8000;
  for (let r = 0; r < rows; r++) {
    if (rows === 1) {
      yLabels.push(`${minRpm}`);
    } else {
      const rpm = Math.round((minRpm + (maxRpm - minRpm) * (r / (rows - 1))) / 100) * 100;
      yLabels.push(`${rpm}`);
    }
  }

  // Realistic TPS or MAP breakpoints (X-axis)
  const xLabels: string[] = [];
  if (isBoost) {
    const minKpa = 100;
    const maxKpa = 240;
    for (let c = 0; c < cols; c++) {
      if (cols === 1) {
        xLabels.push(`${minKpa}`);
      } else {
        const kpa = Math.round(minKpa + (maxKpa - minKpa) * (c / (cols - 1)));
        xLabels.push(`${kpa}`);
      }
    }
  } else {
    for (let c = 0; c < cols; c++) {
      if (cols === 1) {
        xLabels.push('0%');
      } else {
        const pct = Math.round((c / (cols - 1)) * 100);
        xLabels.push(`${pct}%`);
      }
    }
  }

  return { xLabels, yLabels, xAxisName, yAxisName };
}
