import { invoke } from '@tauri-apps/api/core';
import { isDesktopApp } from './romBridge.ts';

export interface ChecksumResult {
  algorithm: string;
  width_bits: number;
  value: number;
  input_len: number;
}

/** CRC-32 lookup table, built once. */
const CRC32_TABLE: Uint32Array = (() => {
  const table = new Uint32Array(256);
  for (let i = 0; i < 256; i++) {
    let c = i;
    for (let j = 0; j < 8; j++) {
      c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    }
    table[i] = c >>> 0;
  }
  return table;
})();

function crc32Reflected(data: Uint8Array, poly: number): number {
  let crc = 0xffffffff;
  for (let i = 0; i < data.length; i++) {
    if (poly === 0xedb88320) {
      crc = (crc >>> 8) ^ CRC32_TABLE[(crc ^ data[i]) & 0xff];
    } else {
      crc ^= data[i];
      for (let b = 0; b < 8; b++) {
        const mask = -(crc & 1);
        crc = (crc >>> 1) ^ (poly & mask);
      }
    }
  }
  return (~crc) >>> 0;
}

function crc32Unreflected(data: Uint8Array, poly: number, init: number, xorOut: number): number {
  let crc = init >>> 0;
  for (let i = 0; i < data.length; i++) {
    crc = (crc ^ (data[i] << 24)) >>> 0;
    for (let b = 0; b < 8; b++) {
      crc = crc & 0x80000000 ? ((crc << 1) ^ poly) >>> 0 : (crc << 1) >>> 0;
    }
  }
  return (crc ^ xorOut) >>> 0;
}

function crc16Ccitt(data: Uint8Array): number {
  let crc = 0xffff;
  for (let i = 0; i < data.length; i++) {
    crc ^= data[i] << 8;
    for (let b = 0; b < 8; b++) {
      crc = crc & 0x8000 ? ((crc << 1) ^ 0x1021) & 0xffff : (crc << 1) & 0xffff;
    }
  }
  return crc & 0xffff;
}

/**
 * Computes a checksum over explicitly supplied data.
 *
 * Supported: `crc32_ieee`, `crc32_castagnoli`, `crc32_simos`, `crc16_ccitt`.
 * `crc16_modbus` is rejected: the source engine never implemented it.
 *
 * Pure calculation; no ROM or hardware access. Returns `null` for unsupported
 * algorithms.
 */
export async function bridgeComputeChecksum(
  algorithm: string,
  data?: Uint8Array | null
): Promise<ChecksumResult | null> {
  if (isDesktopApp()) {
    try {
      if (!data || data.length === 0) {
        return await invoke<ChecksumResult>('compute_workspace_checksum', {
          algorithm,
        });
      }
      return await invoke<ChecksumResult>('compute_checksum', {
        algorithm,
        data: Array.from(data),
      });
    } catch (err) {
      console.warn('Tauri compute_checksum IPC error, falling back:', err);
    }
  }

  if (!data) return null;
  switch (algorithm) {
    case 'crc32_ieee':
      return { algorithm, width_bits: 32, value: crc32Reflected(data, 0xedb88320), input_len: data.length };
    case 'crc32_castagnoli':
      return { algorithm, width_bits: 32, value: crc32Reflected(data, 0x82f63b78), input_len: data.length };
    case 'crc32_simos':
      return { algorithm, width_bits: 32, value: crc32Unreflected(data, 0x04c11db7, 0, 0), input_len: data.length };
    case 'crc16_ccitt':
      return { algorithm, width_bits: 16, value: crc16Ccitt(data), input_len: data.length };
    default:
      return null;
  }
}

/**
 * Result of a 2D map interpolation at an arbitrary operating point.
 */
export interface MapInterpolationResult {
  value: number;
  x_low: number;
  x_high: number;
  x_t: number;
  y_low: number;
  y_high: number;
  y_t: number;
  x_clamped: boolean;
  y_clamped: boolean;
}

/** Locates `target` in `axis`, clamping outside the range (source semantics). */
function searchInterval(
  axis: number[],
  target: number
): { low: number; high: number; t: number } | null {
  if (axis.length < 2) return null;
  const last = axis.length - 1;

  if (target <= axis[0]) return { low: 0, high: 1, t: 0 };
  if (target >= axis[last]) return { low: last - 1, high: last, t: 1 };

  let low = 0;
  let high = last;
  while (high - low > 1) {
    const mid = Math.floor((low + high) / 2);
    if (axis[mid] <= target) low = mid;
    else high = mid;
  }
  const span = axis[high] - axis[low];
  const t = Math.abs(span) > 1e-12 ? Math.min(1, Math.max(0, (target - axis[low]) / span)) : 0;
  return { low, high, t };
}

/**
 * Interpolates a 2D map at `(x, y)` using bilinear interpolation.
 *
 * On desktop the call goes through Tauri using `mapId` (the map must already be
 * loaded). Otherwise the supplied `map`/`xAxis`/`yAxis` are interpolated
 * in-process with the same semantics as `kivi_defs::interp::lookup_2d_map`:
 * out-of-range coordinates clamp to the boundary, and axes shorter than two
 * points are rejected.
 *
 * Pure calculation. Returns `null` for malformed input.
 */
export async function bridgeInterpolateMapAt(
  mapId: string,
  map: number[][] | null,
  xAxis: number[] | null,
  yAxis: number[] | null,
  x: number,
  y: number
): Promise<MapInterpolationResult | null> {
  if (isDesktopApp()) {
    try {
      return await invoke<MapInterpolationResult>('interpolate_map_at', {
        mapId,
        x,
        y,
      });
    } catch (err) {
      console.warn('Tauri interpolate_map_at IPC error, falling back:', err);
    }
  }

  if (!map || !xAxis || !yAxis) return null;
  if (map.length !== yAxis.length) return null;
  if (map.length === 0 || map[0].length !== xAxis.length) return null;

  const xi = searchInterval(xAxis, x);
  const yi = searchInterval(yAxis, y);
  if (!xi || !yi) return null;

  const q00 = map[yi.low][xi.low];
  const q10 = map[yi.low][xi.high];
  const q01 = map[yi.high][xi.low];
  const q11 = map[yi.high][xi.high];

  const value =
    (1 - xi.t) * (1 - yi.t) * q00 +
    xi.t * (1 - yi.t) * q10 +
    (1 - xi.t) * yi.t * q01 +
    xi.t * yi.t * q11;

  return {
    value,
    x_low: xi.low,
    x_high: xi.high,
    x_t: xi.t,
    y_low: yi.low,
    y_high: yi.high,
    y_t: yi.t,
    x_clamped: x < xAxis[0] || x > xAxis[xAxis.length - 1],
    y_clamped: y < yAxis[0] || y > yAxis[yAxis.length - 1],
  };
}

/**
 * Read-only smoothing analysis result for a 2D map.
 *
 * `original` is the matrix as read from the ROM; `smoothed` is a preview only
 * and is never persisted; `laplacian` is the curvature map.
 */
export interface MapSmoothingReport {
  original: number[][];
  smoothed: number[][];
  laplacian: number[][];
  max_abs_curvature: number;
  max_curvature_row: number;
  max_curvature_col: number;
  rows: number;
  cols: number;
  alpha: number;
}

/** Discrete 2D Laplacian; borders are zero (matches Rust). */
function laplacianCurvature2d(m: number[][]): number[][] {
  const rows = m.length;
  const cols = rows > 0 ? m[0].length : 0;
  if (rows < 3) {
    return Array.from({ length: rows }, () => new Array(cols).fill(0));
  }
  const out = Array.from({ length: rows }, () => new Array(cols).fill(0));
  for (let r = 1; r < rows - 1; r++) {
    for (let c = 1; c < cols - 1; c++) {
      out[r][c] = m[r + 1][c] + m[r - 1][c] + m[r][c + 1] + m[r][c - 1] - 4 * m[r][c];
    }
  }
  return out;
}

/** 4-connected spatial blend; borders use only existing neighbours. */
function spatialSmooth2d(m: number[][], alpha: number): number[][] {
  const rows = m.length;
  if (rows === 0) return [];
  const cols = m[0].length;
  const out = m.map((row) => [...row]);
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      let sum = 0;
      let count = 0;
      if (r > 0) { sum += m[r - 1][c]; count++; }
      if (r + 1 < rows) { sum += m[r + 1][c]; count++; }
      if (c > 0) { sum += m[r][c - 1]; count++; }
      if (c + 1 < cols) { sum += m[r][c + 1]; count++; }
      if (count > 0) {
        out[r][c] = (1 - alpha) * m[r][c] + alpha * (sum / count);
      }
    }
  }
  return out;
}

/**
 * Computes a read-only smoothing preview and curvature analysis for a map.
 *
 * Mirrors `kivi_defs::smoothing`. The supplied matrix is never mutated; the
 * result carries `original`, a `smoothed` preview, and the Laplacian map.
 * There is deliberately no apply counterpart.
 *
 * Returns `null` for invalid alpha or missing input.
 */
export async function bridgeAnalyzeMapSmoothing(
  mapId: string,
  map: number[][] | null,
  alpha: number
): Promise<MapSmoothingReport | null> {
  if (isDesktopApp()) {
    try {
      // Send the matrix so the preview analyzes exactly what the editor shows,
      // including edits not yet written back to AppState. The Rust side falls
      // back to its own copy when `matrix` is null.
      return await invoke<MapSmoothingReport>('analyze_map_smoothing', {
        mapId,
        alpha,
        matrix: map ?? null,
      });
    } catch (err) {
      console.warn('Tauri analyze_map_smoothing IPC error, falling back:', err);
    }
  }

  if (!map || map.length === 0) return null;
  // Same range check as Rust: NaN passes (source quirk) but 0/1/negative do not.
  if (alpha <= 0 || alpha >= 1) return null;

  const original = map.map((row) => [...row]);
  const smoothed = spatialSmooth2d(original, alpha);
  const laplacian = laplacianCurvature2d(original);

  let maxAbs = 0;
  let maxRow = 0;
  let maxCol = 0;
  for (let r = 0; r < laplacian.length; r++) {
    for (let c = 0; c < laplacian[r].length; c++) {
      const a = Math.abs(laplacian[r][c]);
      if (a > maxAbs) {
        maxAbs = a;
        maxRow = r;
        maxCol = c;
      }
    }
  }

  return {
    original,
    smoothed,
    laplacian,
    max_abs_curvature: maxAbs,
    max_curvature_row: maxRow,
    max_curvature_col: maxCol,
    rows: original.length,
    cols: original[0].length,
    alpha,
  };
}

/**
 * Result of detecting and parsing a firmware container.
 */
export interface FirmwareIngestReport {
  /** `IntelHex`, `SRecord`, or `RawBinary`. */
  format: string;
  /**
   * Lowest record address, or `null` when unknown.
   *
   * `0` is a *known* base of zero; `null` means no address information. A raw
   * BIN always yields `null` because a bare `.bin` carries no absolute address.
   */
  base_address: number | null;
  bytes: number[];
  record_count: number;
  entry_point: number | null;
}

/** Decodes a hex string to bytes, or null on invalid input. */
function decodeHexPairs(s: string): number[] | null {
  if (s.length % 2 !== 0) return null;
  const out: number[] = [];
  for (let i = 0; i < s.length; i += 2) {
    const v = Number.parseInt(s.slice(i, i + 2), 16);
    if (Number.isNaN(v)) return null;
    out.push(v);
  }
  return out;
}

/** Sparse address -> byte map, flattened to a contiguous buffer. */
function flattenSparse(
  map: Map<number, number>,
  padByte: number
): { base: number | null; bytes: number[] } {
  // An empty map carries no address information at all.
  if (map.size === 0) return { base: null, bytes: [] };
  const addrs = Array.from(map.keys());
  const min = Math.min(...addrs);
  const max = Math.max(...addrs);
  const bytes = new Array<number>(max - min + 1).fill(padByte);
  for (const [addr, b] of map) bytes[addr - min] = b;
  return { base: min, bytes };
}

/**
 * Detects and parses a firmware container (Intel HEX, S-Record, or raw BIN).
 *
 * Mirrors `kivi_core::hex_ingest`. Raw binary is returned unchanged. Text
 * beginning with `:` or `S` is always parsed as HEX/SREC and returns `null` on
 * failure rather than being treated as ASCII firmware.
 *
 * Read-only; performs no hardware access.
 */
export async function bridgeIngestFirmware(
  data: Uint8Array
): Promise<FirmwareIngestReport | null> {
  if (isDesktopApp()) {
    try {
      return await invoke<FirmwareIngestReport>('ingest_firmware_data', {
        data: Array.from(data),
      });
    } catch (err) {
      console.warn('Tauri ingest_firmware_data IPC error, falling back:', err);
    }
  }

  // Find the first non-whitespace byte to classify.
  let firstNonWs: number | null = null;
  for (let i = 0; i < data.length; i++) {
    const c = data[i];
    if (c !== 0x20 && c !== 0x09 && c !== 0x0a && c !== 0x0d) {
      firstNonWs = c;
      break;
    }
  }

  if (firstNonWs !== 0x3a && firstNonWs !== 0x53) {
    return {
      format: 'RawBinary',
      base_address: null,
      bytes: Array.from(data),
      record_count: 0,
      entry_point: null,
    };
  }

  let text: string;
  try {
    text = new TextDecoder('utf-8', { fatal: true }).decode(data);
  } catch {
    return null;
  }

  const lines = text.split(/\r?\n/);
  const memory = new Map<number, number>();
  let baseUpper = 0;
  let entryPoint: number | null = null;
  let recordCount = 0;

  if (firstNonWs === 0x3a) {
    // ---------------- Intel HEX ----------------
    for (let idx = 0; idx < lines.length; idx++) {
      const line = lines[idx].trim();
      if (line.length === 0) continue;
      if (!line.startsWith(':')) return null;

      const body = line.slice(1);
      if (body.length < 10 || body.length % 2 !== 0) return null;
      const bytes = decodeHexPairs(body);
      if (!bytes) return null;

      const sum = bytes.reduce((a, b) => (a + b) & 0xff, 0);
      if (sum !== 0) return null;

      const byteCount = bytes[0];
      // SAFETY DEVIATION: validate before slicing (source panics here).
      if (bytes.length < 4 + byteCount) return null;

      const offset = (bytes[1] << 8) | bytes[2];
      const recType = bytes[3];
      const payload = bytes.slice(4, 4 + byteCount);

      if (recType === 0x00) {
        const full = (baseUpper + offset) >>> 0;
        payload.forEach((b, i) => memory.set((full + i) >>> 0, b));
        recordCount++;
      } else if (recType === 0x01) {
        break;
      } else if (recType === 0x02 && payload.length === 2) {
        baseUpper = (((payload[0] << 8) | payload[1]) << 4) >>> 0;
      } else if (recType === 0x04 && payload.length === 2) {
        baseUpper = ((((payload[0] << 8) | payload[1]) << 16) >>> 0);
      } else if (recType === 0x05 && payload.length === 4) {
        entryPoint = ((payload[0] << 24) | (payload[1] << 16) | (payload[2] << 8) | payload[3]) >>> 0;
      }
    }
    const { base, bytes } = flattenSparse(memory, 0xff);
    return { format: 'IntelHex', base_address: base, bytes, record_count: recordCount, entry_point: entryPoint };
  }

  // ---------------- S-Record ----------------
  for (let idx = 0; idx < lines.length; idx++) {
    const line = lines[idx].trim();
    if (line.length === 0) continue;
    if (!line.startsWith('S')) return null;
    // SAFETY DEVIATION: bare "S" panics in the source.
    if (line.length < 2) return null;

    const recType = line[1];
    const payloadHex = line.slice(2);
    if (payloadHex.length < 4 || payloadHex.length % 2 !== 0) return null;
    const bytes = decodeHexPairs(payloadHex);
    if (!bytes) return null;

    const sum = bytes.reduce((a, b) => (a + b) & 0xff, 0);
    if (sum !== 0xff) return null;

    const count = bytes[0];
    if (bytes.length !== count + 1) return null;

    if (recType === '0') {
      // header, no data
    } else if (recType === '1') {
      if (bytes.length < 4) return null;
      const addr = (bytes[1] << 8) | bytes[2];
      bytes.slice(3, bytes.length - 1).forEach((b, i) => memory.set(addr + i, b));
      recordCount++;
    } else if (recType === '2') {
      if (bytes.length < 5) return null;
      const addr = (bytes[1] << 16) | (bytes[2] << 8) | bytes[3];
      bytes.slice(4, bytes.length - 1).forEach((b, i) => memory.set(addr + i, b));
      recordCount++;
    } else if (recType === '3') {
      if (bytes.length < 6) return null;
      const addr = ((bytes[1] << 24) | (bytes[2] << 16) | (bytes[3] << 8) | bytes[4]) >>> 0;
      bytes.slice(5, bytes.length - 1).forEach((b, i) => memory.set((addr + i) >>> 0, b));
      recordCount++;
    } else if (recType === '7' || recType === '8' || recType === '9') {
      break;
    }
  }
  const { base, bytes } = flattenSparse(memory, 0xff);
  return { format: 'SRecord', base_address: base, bytes, record_count: recordCount, entry_point: null };
}

/**
 * Result of an address-gated comparison.
 *
 * Exactly one of `ok` / `error` is populated.
 */
