import { invoke } from '@tauri-apps/api/core';
import type { DiffRegion, DiffKind, DetectedMapCandidate, MapDef } from '../../types/kivi.ts';
import { isDesktopApp } from './romBridge.ts';

function buildCompareHeaders(
  baseAddr: number | null,
  targetAddr: number | null,
  maxGap: number
): Record<string, string> {
  const headers: Record<string, string> = {
    'x-max-gap': String(Math.max(0, Math.floor(maxGap))),
  };
  if (baseAddr !== null) {
    headers['x-base-addr'] = `0x${baseAddr.toString(16)}`;
  }
  if (targetAddr !== null) {
    headers['x-target-addr'] = `0x${targetAddr.toString(16)}`;
  }
  return headers;
}

export async function bridgeCompareLoadedRomRaw(
  baseAddr: number | null,
  targetData: Uint8Array,
  targetAddr: number | null,
  maxGap: number = 4
): Promise<GatedDiffOutcome> {
  if (!isDesktopApp()) {
    return { ok: null, error: 'Requires the kiviTuner desktop application.' };
  }
  try {
    const report = await invoke<GatedDiffReport>('compare_loaded_rom_raw', targetData, {
      headers: buildCompareHeaders(baseAddr, targetAddr, maxGap),
    });
    return { ok: report, error: null };
  } catch (err) {
    return { ok: null, error: String(err) };
  }
}

export interface GatedDiffOutcome {
  ok: GatedDiffReport | null;
  error: string | null;
}

/**
 * Result of a successful address-gated comparison.
 */
export interface GatedDiffReport {
  /** `UnknownBases` or `MatchingBases(0x...)`. */
  compatibility: string;
  regions: DiffRegion[];
}

/**
 * Compares two buffers after checking that their address ranges are compatible.
 *
 * `baseAddr` / `targetAddr` are the lowest physical addresses reported by
 * HEX/SREC ingest, or `null` for raw BIN (unknown). Mixed known/unknown and
 * conflicting known bases are rejected rather than producing a misleading diff.
 *
 * Read-only; performs no hardware access.
 */
export async function bridgeCompareRomBinariesWithAddress(
  baseData: Uint8Array | null,
  baseAddr: number | null,
  targetData: Uint8Array,
  targetAddr: number | null,
  maxGap: number = 4
): Promise<GatedDiffOutcome> {
  if (isDesktopApp()) {
    try {
      if (!baseData || baseData.length === 0) {
        const report = await invoke<GatedDiffReport>('compare_loaded_rom_raw', targetData, {
          headers: buildCompareHeaders(baseAddr, targetAddr, maxGap),
        });
        return { ok: report, error: null };
      }
      const report = await invoke<GatedDiffReport>('compare_rom_binaries_with_address', {
        baseData: Array.from(baseData),
        baseAddr,
        targetData: Array.from(targetData),
        targetAddr,
        maxGap,
      });
      return { ok: report, error: null };
    } catch (err) {
      // Tauri surfaces the Rust rejection message here.
      return { ok: null, error: String(err) };
    }
  }

  // Browser fallback mirrors `kivi_diff::address::diff_with_address_check`.
  if (!baseData || baseData.length === 0) {
    return { ok: null, error: 'Cannot compare: base input is empty' };
  }
  if (targetData.length === 0) {
    return { ok: null, error: 'Cannot compare: target input is empty' };
  }

  let compatibility: string;
  if (baseAddr === null && targetAddr === null) {
    compatibility = 'UnknownBases';
  } else if (baseAddr !== null && targetAddr !== null) {
    if (baseAddr !== targetAddr) {
      return {
        ok: null,
        error:
          `Cannot compare: base addresses differ (base 0x${baseAddr.toString(16).toUpperCase()}, ` +
          `target 0x${targetAddr.toString(16).toUpperCase()}). ` +
          'Re-export both files from the same address range, or compare raw BIN files.',
      };
    }
    compatibility = `MatchingBases(0x${baseAddr.toString(16).toUpperCase()})`;
  } else {
    const fmt = (v: number | null) =>
      v === null ? 'unknown' : `0x${v.toString(16).toUpperCase()}`;
    return {
      ok: null,
      error:
        `Cannot compare: address alignment cannot be verified ` +
        `(base ${fmt(baseAddr)}, target ${fmt(targetAddr)}). ` +
        'Compare files of the same format, or load both as raw BIN.',
    };
  }

  const regions = await bridgeCompareRomBinaries(baseData, targetData, maxGap);
  return { ok: { compatibility, regions }, error: null };
}

/**
 * Compares two binary ROM buffers and clusters differences into DiffRegions.
 */
export async function bridgeCompareRomBinaries(
  baseData: Uint8Array,
  targetData: Uint8Array,
  maxGap: number = 4
): Promise<DiffRegion[]> {
  if (isDesktopApp()) {
    try {
      return await invoke<DiffRegion[]>('compare_rom_binaries', {
        baseData: Array.from(baseData),
        targetData: Array.from(targetData),
        maxGap,
      });
    } catch (err) {
      console.warn('Tauri compare_rom_binaries IPC error, falling back:', err);
    }
  }

  // Pure JavaScript fallback implementation matching kivi-diff engine
  const minLen = Math.min(baseData.length, targetData.length);
  const maxLen = Math.max(baseData.length, targetData.length);
  const diffIndices: number[] = [];

  for (let i = 0; i < minLen; i++) {
    if (baseData[i] !== targetData[i]) {
      diffIndices.push(i);
    }
  }
  for (let i = minLen; i < maxLen; i++) {
    diffIndices.push(i);
  }

  if (diffIndices.length === 0) return [];

  interface Cluster {
    start: number;
    last: number;
    count: number;
  }
  const clusters: Cluster[] = [];
  let current: Cluster | null = null;

  for (const idx of diffIndices) {
    if (current && idx - current.last - 1 <= maxGap) {
      current.last = idx;
      current.count++;
    } else {
      if (current) clusters.push(current);
      current = { start: idx, last: idx, count: 1 };
    }
  }
  if (current) clusters.push(current);

  return clusters.map((c, index) => {
    const startAddr = c.start;
    const endAddr = c.last + 1;
    const byteLen = endAddr - startAddr;

    let maxAbsDelta = 0;
    let sumDelta = 0;

    for (let i = startAddr; i < endAddr; i++) {
      const b = i < baseData.length ? baseData[i] : 0;
      const t = i < targetData.length ? targetData[i] : 0;
      const d = t - b;
      if (Math.abs(d) > maxAbsDelta) maxAbsDelta = Math.abs(d);
      sumDelta += d;
    }
    const meanDelta = c.count > 0 ? sumDelta / c.count : 0;

    let classification: DiffKind;
    if (byteLen === 1) {
      classification = { type: 'Scalar' };
    } else if (byteLen <= 24 && [8, 10, 12, 16, 20, 24].includes(byteLen)) {
      classification = { type: 'Axis1D', data: { length: byteLen } };
    } else if (byteLen >= 32) {
      let cols = 16;
      if (byteLen % 16 === 0) cols = 16;
      else if (byteLen % 12 === 0) cols = 12;
      else if (byteLen % 8 === 0) cols = 8;
      const rows = Math.max(1, Math.floor(byteLen / cols));
      classification = { type: 'Table2D', data: { suggested_rows: rows, suggested_cols: cols } };
    } else {
      classification = { type: 'ChecksumOrRawBlock' };
    }

    return {
      id: `diff_${index + 1}`,
      start_addr: startAddr,
      end_addr: endAddr,
      byte_len: byteLen,
      changed_byte_count: c.count,
      max_abs_delta: maxAbsDelta,
      mean_delta: Math.round(meanDelta * 100) / 100,
      inferred_dtype: 'u8',
      inferred_endian: 'little',
      classification,
    };
  });
}

/**
 * Scans ROM data using heuristic periodic table and axis detection algorithms.
 */
export async function bridgeScanRomHeuristics(
  romData: Uint8Array | null,
  minConfidence: number = 70
): Promise<DetectedMapCandidate[]> {
  if (isDesktopApp()) {
    try {
      if (!romData || romData.length === 0) {
        return await invoke<DetectedMapCandidate[]>('scan_rom_heuristics_workspace', {
          minConfidence,
        });
      }
      return await invoke<DetectedMapCandidate[]>('scan_rom_heuristics', {
        romData: Array.from(romData),
        minConfidence,
      });
    } catch (err) {
      console.warn('Tauri scan_rom_heuristics IPC error, falling back:', err);
    }
  }

  // Pure JavaScript fallback for browser and unit testing
  const candidates: DetectedMapCandidate[] = [];
  if (!romData || romData.length < 64) return candidates;

  const pitches = [16, 12, 8];
  for (const cols of pitches) {
    const minBytes = cols * 4;
    for (let addr = 0; addr <= romData.length - minBytes; addr += cols * 2) {
      let nonZeroCount = 0;
      for (let i = 0; i < minBytes; i++) {
        if (romData[addr + i] > 0) nonZeroCount++;
      }
      if (nonZeroCount < minBytes * 0.3) continue;

      const rows = 4;
      const conf = Math.min(95, Math.max(50, 65 + (cols === 16 ? 15 : 5)));
      if (conf >= minConfidence) {
        const id = `scan_${rows}x${cols}_${addr.toString(16)}`;
        candidates.push({
          id,
          name_hint: `Candidate Table ${rows}x${cols} @ 0x${addr.toString(16).toUpperCase()}`,
          category_hint: cols === 16 ? 'Fuel / VE' : 'Ignition',
          start_addr: addr,
          end_addr: addr + rows * cols,
          rows,
          cols,
          dtype: 'u8',
          endian: 'little',
          confidence: conf,
          x_axis_addr: addr >= cols ? addr - cols : null,
          y_axis_addr: null,
        });
        addr += rows * cols;
        if (candidates.length >= 20) break;
      }
    }
    if (candidates.length >= 10) break;
  }

  return candidates;
}

/**
 * Converts a DiffRegion directly into a usable MapDef.
 */
export async function bridgeConvertDiffToMap(
  region: DiffRegion
): Promise<MapDef> {
  if (isDesktopApp()) {
    try {
      return await invoke<MapDef>('convert_diff_to_map', { region });
    } catch (err) {
      console.warn('Tauri convert_diff_to_map IPC error, falling back:', err);
    }
  }

  // Pure JavaScript fallback matching kivi-diff::convert::diff_region_to_map_def
  let rows = 1;
  let cols = 1;
  let name = `Scalar @ 0x${region.start_addr.toString(16).toUpperCase()}`;

  if (region.classification.type === 'Table2D') {
    rows = region.classification.data.suggested_rows;
    cols = region.classification.data.suggested_cols;
    name = `Table ${rows}x${cols} @ 0x${region.start_addr.toString(16).toUpperCase()}`;
  } else if (region.classification.type === 'Axis1D') {
    rows = 1;
    cols = region.classification.data.length;
    name = `Axis ${cols} @ 0x${region.start_addr.toString(16).toUpperCase()}`;
  } else if (region.classification.type === 'ChecksumOrRawBlock') {
    name = `Raw Block @ 0x${region.start_addr.toString(16).toUpperCase()}`;
  }

  return {
    id: `map_diff_${region.id}_${Date.now()}`,
    name,
    category: 'Diff Extract',
    address: region.start_addr,
    rows,
    cols,
    dtype: region.inferred_dtype || 'u8',
    endian: region.inferred_endian || 'little',
    scaling: {
      equation: 'X',
      unit: 'raw',
      decimals: 1,
    },
    x_axis: null,
    y_axis: null,
  };
}


// ---------------------------------------------------------------------------
// Wave 4: VBF container
// ---------------------------------------------------------------------------

/** One VBF segment as supplied to, or returned from, the backend. */
export interface VbfSegmentPayload {
  start_address: number;
  data: number[];
}

/** Result of parsing a VBF container. */
export interface VbfParseReport {
  /** Raw ASCII header text. */
  header: string;
  /** Segments in file order. */
  segments: VbfSegmentPayload[];
}

/**
 * Parses a VBF container.
 *
 * **Limitation:** supports the simplified dialect the backend codec implements
 * (ASCII header + length-prefixed segments). Not validated against real OEM VBF
 * files. A parse failure is returned as `null` with the reason logged, so the
 * caller must treat `null` as "rejected", never as "empty container".
 */
export async function bridgeParseVbf(
  vbfBytes: Uint8Array
): Promise<VbfParseReport | null> {
  if (isDesktopApp()) {
    try {
      return await invoke<VbfParseReport>('parse_vbf', {
        vbfBytes: Array.from(vbfBytes),
      });
    } catch (err) {
      console.warn('Tauri parse_vbf IPC error:', err);
    }
  }
  // No browser fallback: the VBF codec is Rust-only, and reimplementing it here
  // would create a second parser that could drift from the backend.
  return null;
}

/**
 * Serializes segments into a VBF container.
 *
 * Returns `null` when the backend is unavailable; the browser cannot produce a
 * VBF file without duplicating the codec.
 */
export async function bridgeSerializeVbf(
  swPartNumber: string,
  segments: VbfSegmentPayload[]
): Promise<Uint8Array | null> {
  if (isDesktopApp()) {
    try {
      const bytes = await invoke<number[]>('serialize_vbf', {
        swPartNumber,
        segments,
      });
      return new Uint8Array(bytes);
    } catch (err) {
      console.warn('Tauri serialize_vbf IPC error:', err);
    }
  }
  return null;
}

// ---------------------------------------------------------------------------
// Wave 4: address translation
// ---------------------------------------------------------------------------

/** Direction of an address translation. */
export type TranslationDirection = 'cpu_to_raw' | 'raw_to_cpu';

/** Result of an address translation. */
export interface AddressTranslationReport {
  value: number;
  /** True when no rule matched and the input came back unchanged. */
  is_identity_fallback: boolean;
  /**
   * Name of the rule that matched, or `null`.
   *
   * This is what exposes ambiguity: the RH850 raw range overlaps the TriCore
   * cached window, so a raw offset there resolves through the *cached* rule.
   */
  matched_rule: string | null;
}

/**
 * Translates an address between the CPU view and the raw file view.
 *
 * **Analysis only.** This is not a write-address validator and confers no
 * permission to write to a device.
 */
export async function bridgeTranslateAddress(
  direction: TranslationDirection,
  address: number
): Promise<AddressTranslationReport | null> {
  if (isDesktopApp()) {
    try {
      return await invoke<AddressTranslationReport>('translate_address', {
        direction,
        address,
      });
    } catch (err) {
      console.warn('Tauri translate_address IPC error:', err);
    }
  }
  return null;
}

// ---------------------------------------------------------------------------
// Wave 4: heuristic window scanner
// ---------------------------------------------------------------------------

/**
 * A candidate found by the exhaustive 16-bit window scanner.
 *
 * **Not comparable** with `DetectedMapCandidate` from `bridgeScanRomHeuristics`:
 * different detector, different scoring scale. Do not merge or cross-dedupe.
 */
export interface DiscoveredWindowMap {
  offset: number;
  rows: number;
  cols: number;
  is_signed: boolean;
  is_little_endian: boolean;
  score: number;
  smoothness: number;
  val_min: number;
  val_max: number;
  suggested_name: string;
  category: string;
  unit: string;
}

/**
 * Runs the exhaustive 16-bit window scanner.
 *
 * Separate from `bridgeScanRomHeuristics` (periodic-run detection). Scores are
 * on different scales and must not be combined.
 */
export async function bridgeScanRomHeuristicWindows(
  romData: Uint8Array | null,
  candidateWidths: number[],
  minRows: number,
  maxRows: number,
  littleEndian: boolean
): Promise<DiscoveredWindowMap[] | null> {
  if (isDesktopApp()) {
    try {
      if (!romData || romData.length === 0) {
        return await invoke<DiscoveredWindowMap[]>('scan_rom_heuristic_windows_workspace', {
          candidateWidths,
          minRows,
          maxRows,
          littleEndian,
        });
      }
      return await invoke<DiscoveredWindowMap[]>('scan_rom_heuristic_windows', {
        romData: Array.from(romData),
        candidateWidths,
        minRows,
        maxRows,
        littleEndian,
      });
    } catch (err) {
      console.warn('Tauri scan_rom_heuristic_windows IPC error:', err);
    }
  }
  return null;
}

// ---------------------------------------------------------------------------
// Blind auto-mapping
// ---------------------------------------------------------------------------

/** A breakpoint axis attached to a blind-mapped table. */
export interface BlindAxis {
  address: number;
  points: number;
  is_16bit: boolean;
  little_endian: boolean;
  values: number[];
}

/**
 * A proposed raw-to-physical conversion.
 *
 * A bare ROM states no unit, so this is a **suggestion** derived from the value
 * range. `rationale` says why it was proposed and must be shown to the user.
 */
export interface SuggestedScaling {
  equation: string;
  unit: string;
  decimals: number;
  rationale: string;
}

/** A calibration table found without any definition file. */
export interface BlindTable {
  address: number;
  rows: number;
  cols: number;
  cell_bits: number;
  little_endian: boolean;
  major_stride_bits: number;
  minor_stride_bits: number;
  raw_min: number;
  raw_max: number;
  matrix: number[][];
  x_axis: BlindAxis | null;
  y_axis: BlindAxis | null;
  suggested_scaling: SuggestedScaling;
  label: string;
}

/**
 * Finds calibration tables in a ROM with **no XDF / DAMOS / A2L / map pack**.
 *
 * Returns a browsable list: overlapping candidates are suppressed so each table
 * appears once. On a 1 MiB Honda ROM that is ~198 tables, versus 10 563 raw
 * fragments before suppression.
 *
 * `includeCurves` is off by default because 1-D curves outnumber 2-D tables 18:1
 * and drown the list.
 */
export async function bridgeBlindAutomap(
  romData: Uint8Array | null,
  overlapThreshold?: number,
  includeCurves = false,
  include8Bit = true
): Promise<BlindTable[] | null> {
  if (isDesktopApp()) {
    try {
      if (!romData || romData.length === 0) {
        return await invoke<BlindTable[]>('blind_automap_workspace', {
          overlapThreshold: overlapThreshold ?? null,
          includeCurves,
          include8bit: include8Bit,
        });
      }
      return await invoke<BlindTable[]>('blind_automap_rom', {
        romData: Array.from(romData),
        overlapThreshold: overlapThreshold ?? null,
        includeCurves,
        include8bit: include8Bit,
      });
    } catch (err) {
      console.warn('Tauri blind_automap_rom IPC error:', err);
    }
  }
  return null;
}

// ---------------------------------------------------------------------------
// Denso / Toyota interleaved map scanner
// ---------------------------------------------------------------------------

/**
 * A Denso/Toyota interleaved map.
 *
 * These store the Y-axis breakpoint **inside** each row, before the data, so the
 * data words are not contiguous. No rectangular scanner can find them, which is
 * why this is a separate detector.
 */
export interface DensoInterleavedMap {
  start_address: number;
  rows: number;
  cols: number;
  is_16bit: boolean;
  y_axis_values: number[];
  matrix: number[][];
}

/**
 * Runs the Denso interleaved-map scanner over a ROM buffer.
 *
 * Returns `null` outside the desktop app, like the other bridges.
 */
export async function bridgeScanDensoInterleaved(
  romData: Uint8Array | null,
  candidateCols: number[],
  minRows: number,
  maxRows: number
): Promise<DensoInterleavedMap[] | null> {
  if (isDesktopApp()) {
    try {
      if (!romData || romData.length === 0) {
        return await invoke<DensoInterleavedMap[]>('scan_rom_denso_interleaved_workspace', {
          candidateCols,
          minRows,
          maxRows,
        });
      }
      return await invoke<DensoInterleavedMap[]>('scan_rom_denso_interleaved', {
        romData: Array.from(romData),
        candidateCols,
        minRows,
        maxRows,
      });
    } catch (err) {
      console.warn('Tauri scan_rom_denso_interleaved IPC error:', err);
    }
  }
  return null;
}

// ---------------------------------------------------------------------------
// 1-D curve scanner
// ---------------------------------------------------------------------------

/**
 * A calibration curve found by the blind 1-D scanner.
 *
 * A curve has one axis of length one, so neither `bridgeScanRomHeuristics`
 * (periodic runs) nor `bridgeScanRomHeuristicWindows` (rectangular windows)
 * can report it. In the catalogued BIN+XDF corpus 68% of declared multi-cell
 * tables are curves.
 */
export interface DiscoveredCurve {
  /** Byte address of the first cell. */
  address: number;
  /** Number of cells. */
  points: number;
  /** Cell width in bytes: 1 or 2. */
  cell_bytes: number;
  /** 'Little' or 'Big'. */
  endian: 'Little' | 'Big';
  min_value: number;
  max_value: number;
  /** Interior local extrema; a monotonic ramp is an axis and is never returned. */
  turning_points: number;
  /** Mean absolute delta divided by standard deviation; lower is smoother. */
  smoothness: number;
  /** Decoded cell values. */
  values: number[];
}

/**
 * Runs the blind 1-D curve scanner over a ROM buffer.
 *
 * Both byte orders are scanned on the Rust side. Returns `null` when running
 * outside the desktop app, like the other bridges.
 */
export async function bridgeScanRomCurves(
  romData: Uint8Array | null,
  minPoints: number,
  maxPoints: number,
  include8Bit: boolean
): Promise<DiscoveredCurve[] | null> {
  if (isDesktopApp()) {
    try {
      if (!romData || romData.length === 0) {
        return await invoke<DiscoveredCurve[]>('scan_rom_curves_workspace', {
          minPoints,
          maxPoints,
          include8bit: include8Bit,
        });
      }
      return await invoke<DiscoveredCurve[]>('scan_rom_curves', {
        romData: Array.from(romData),
        minPoints,
        maxPoints,
        include8bit: include8Bit,
      });
    } catch (err) {
      console.warn('Tauri scan_rom_curves IPC error:', err);
    }
  }
  return null;
}

