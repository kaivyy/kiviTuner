import { invoke, isTauri } from '@tauri-apps/api/core';
import type { BinaryHashes, MapDef } from '../../types/kivi.ts';
import { computeMd5Hex, computeSha1Hex, computeSha256Hex } from '../crypto/hashes.ts';
import { extractMatrixFromRom, writeMatrixToRom } from '../matrixRom.ts';

/**
 * Checks whether the application is running inside a Tauri desktop container.
 */
export function isDesktopApp(): boolean {
  try {
    return isTauri();
  } catch {
    return false;
  }
}

/**
 * Loads binary bytes into engine (Tauri IPC when available, otherwise browser memory).
 */
export async function bridgeLoadBinData(
  data: Uint8Array,
  fallbackHashes?: BinaryHashes
): Promise<BinaryHashes> {
  if (isDesktopApp()) {
    try {
      return await invoke<BinaryHashes>('load_bin_raw', data);
    } catch (err) {
      console.warn('Tauri load_bin_raw IPC error, falling back:', err);
    }
  }

  if (fallbackHashes) return fallbackHashes;

  return {
    md5: computeMd5Hex(data),
    sha1: computeSha1Hex(data),
    sha256: computeSha256Hex(data),
  };
}

/**
 * Parses XDF XML definition content via Tauri IPC or returns provided defs.
 */
export async function bridgeParseXdf(
  xml: string,
  fallbackDefs: MapDef[] = []
): Promise<MapDef[]> {
  if (isDesktopApp()) {
    try {
      return await invoke<MapDef[]>('parse_xdf_content', { xml });
    } catch (err) {
      console.warn('Tauri parse_xdf_content IPC error, falling back:', err);
    }
  }
  return fallbackDefs;
}

/**
 * Extracts 2D matrix for a given map definition from active ROM.
 */
export async function bridgeExtractMapMatrix(
  mapDef: MapDef,
  romData: Uint8Array | null
): Promise<number[][]> {
  if (isDesktopApp()) {
    try {
      return await invoke<number[][]>('extract_map_matrix', { mapDef });
    } catch (err) {
      console.warn('Tauri extract_map_matrix IPC error, falling back:', err);
    }
  }
  return romData ? extractMatrixFromRom(romData, mapDef) : [];
}

/**
 * Extracts original (stock baseline) 2D matrix for delta/baseline comparisons.
 */
export async function bridgeExtractOriginalMapMatrix(
  mapDef: MapDef,
  originalRomData: Uint8Array | null
): Promise<number[][]> {
  if (isDesktopApp()) {
    try {
      return await invoke<number[][]>('extract_original_map_matrix', { mapDef });
    } catch (err) {
      console.warn('Tauri extract_original_map_matrix IPC error, falling back:', err);
    }
  }
  return originalRomData ? extractMatrixFromRom(originalRomData, mapDef) : [];
}

/**
 * Writes updated 2D matrix back into ROM.
 */
export async function bridgeUpdateMapMatrix(
  mapDef: MapDef,
  matrix: number[][],
  romData: Uint8Array | null
): Promise<Uint8Array | null> {
  if (isDesktopApp()) {
    try {
      await invoke('update_map_matrix', { mapDef, matrix });
      return null;
    } catch (err) {
      console.warn('Tauri update_map_matrix IPC error, falling back:', err);
    }
  }
  return romData ? writeMatrixToRom(romData, mapDef, matrix) : null;
}

/**
 * Verifies Honda Keihin checksum.
 */
export async function bridgeVerifyChecksum(romData: Uint8Array | null): Promise<boolean> {
  if (isDesktopApp()) {
    try {
      return await invoke<boolean>('verify_checksum_keihin');
    } catch (err) {
      console.warn('Tauri verify_checksum_keihin IPC error, falling back:', err);
    }
  }

  if (!romData || romData.length === 0) return false;
  let sum = 0;
  for (let i = 0; i < romData.length; i++) {
    sum = (sum + romData[i]) & 0xff;
  }
  return sum === 0;
}

/**
 * Fixes Honda Keihin checksum (last byte 2's complement).
 */
export async function bridgeFixChecksum(
  romData: Uint8Array | null
): Promise<{ ok: boolean; data?: Uint8Array }> {
  if (isDesktopApp()) {
    try {
      const ok = await invoke<boolean>('fix_checksum_keihin');
      return { ok };
    } catch (err) {
      console.warn('Tauri fix_checksum_keihin IPC error, falling back:', err);
    }
  }

  if (!romData || romData.length < 1024) {
    return { ok: false };
  }

  const nextRom = new Uint8Array(romData);
  let partialSum = 0;
  for (let i = 0; i < nextRom.length - 1; i++) {
    partialSum = (partialSum + nextRom[i]) & 0xff;
  }
  nextRom[nextRom.length - 1] = (0x100 - partialSum) & 0xff;
  return { ok: true, data: nextRom };
}

/**
 * A single byte-level difference between the active ROM and a reference buffer.
 */
export interface VerifyDifferencePayload {
  offset: number;
  expected_local_byte: number;
  actual_chip_byte: number;
}

/**
 * Result of comparing the active ROM buffer against a reference (e.g. a backup).
 */
export interface RomVerificationReport {
  matches: boolean;
  difference_count: number;
  differences: VerifyDifferencePayload[];
  reference_is_blank: boolean;
  active_len: number;
  reference_len: number;
}

/**
 * Compares the active ROM buffer against a reference buffer.
 *
 * Pure in-memory comparison; no hardware access. Falls back to an equivalent
 * browser-side comparison when the Tauri backend is unavailable.
 */
export async function bridgeVerifyRomAgainstReference(
  referenceData: Uint8Array | null,
  activeRom: Uint8Array | null,
  maxDifferences = 32
): Promise<RomVerificationReport | null> {
  if (isDesktopApp()) {
    try {
      return await invoke<RomVerificationReport>('verify_rom_against_reference', {
        referenceData: referenceData ? Array.from(referenceData) : [],
        maxDifferences,
      });
    } catch (err) {
      console.warn('Tauri verify_rom_against_reference IPC error, falling back:', err);
    }
  }

  if (!activeRom) return null;
  const reference = referenceData ?? new Uint8Array(0);
  const cmpLen = Math.min(activeRom.length, reference.length);
  const differences: VerifyDifferencePayload[] = [];

  for (let offset = 0; offset < cmpLen; offset++) {
    if (activeRom[offset] !== reference[offset]) {
      differences.push({
        offset,
        expected_local_byte: activeRom[offset],
        actual_chip_byte: reference[offset],
      });
      if (differences.length >= maxDifferences) break;
    }
  }

  if (activeRom.length !== reference.length && differences.length === 0) {
    differences.push({
      offset: cmpLen,
      expected_local_byte: activeRom[cmpLen] ?? 0,
      actual_chip_byte: reference[cmpLen] ?? 0,
    });
  }

  return {
    matches: differences.length === 0,
    difference_count: differences.length,
    differences,
    reference_is_blank: reference.every((b) => b === 0xff),
    active_len: activeRom.length,
    reference_len: reference.length,
  };
}

/**
 * A single sector entry in a differential flash plan.
 */
export interface SectorPlanEntry {
  sector_index: number;
  start_address: number;
  length: number;
  is_modified: boolean;
  modified_bytes_count: number;
}

/**
 * Result of planning which sectors differ between two ROM buffers.
 */
export interface DifferentialPlanReport {
  sectors: SectorPlanEntry[];
  total_bytes_to_flash: number;
  rom_len: number;
  modified_sector_count: number;
}

/**
 * Plans which sectors differ between `tunedData` and the active ROM (`stockRom`).
 *
 * Read-only analysis; produces no erase or write command. Falls back to an
 * equivalent browser-side planner when the Tauri backend is unavailable.
 * Returns `null` when the buffers differ in length.
 */
export async function bridgePlanDifferentialFlash(
  tunedData: Uint8Array,
  stockRom: Uint8Array | null,
  sectorSizes: number[]
): Promise<DifferentialPlanReport | null> {
  if (isDesktopApp()) {
    try {
      return await invoke<DifferentialPlanReport>('plan_differential_flash_raw', tunedData, {
        headers: {
          'x-sector-sizes': sectorSizes.join(','),
        },
      });
    } catch (err) {
      console.warn('Tauri plan_differential_flash_raw IPC error, falling back:', err);
    }
  }

  if (!stockRom || stockRom.length !== tunedData.length) return null;

  const sectors: SectorPlanEntry[] = [];
  let currentOffset = 0;

  for (let idx = 0; idx < sectorSizes.length; idx++) {
    if (currentOffset >= tunedData.length) break;

    const endOffset = Math.min(currentOffset + sectorSizes[idx], tunedData.length);
    let diffCount = 0;
    for (let i = currentOffset; i < endOffset; i++) {
      if (stockRom[i] !== tunedData[i]) diffCount++;
    }

    sectors.push({
      sector_index: idx,
      start_address: currentOffset,
      length: endOffset - currentOffset,
      is_modified: diffCount > 0,
      modified_bytes_count: diffCount,
    });

    currentOffset = endOffset;
  }

  const totalBytesToFlash = sectors
    .filter((s) => s.is_modified)
    .reduce((sum, s) => sum + s.length, 0);

  return {
    sectors,
    total_bytes_to_flash: totalBytesToFlash,
    rom_len: stockRom.length,
    modified_sector_count: sectors.filter((s) => s.is_modified).length,
  };
}

/**
 * A single flash block in a target description.
 */
export interface FlashBlockEntry {
  start_address: number;
  length: number;
  block_type: string;
  write_safe: boolean;
}

/**
 * Description of a NOR flash target resolved from a JEDEC id.
 */
export interface FlashTargetReport {
  jedec_id: number;
  model_name: string;
  total_size_bytes: number;
  blocks: FlashBlockEntry[];
  boot_block_count: number;
  write_safe_block_count: number;
}

/** Known-chip table mirroring `kivi_core::flash_target`. */
const FLASH_TARGETS: Record<number, { name: string; size: number; blocks: [number, number, string][] }> = {
  0x00892274: {
    name: 'Intel 28F200BX 256KiB',
    size: 262144,
    blocks: [
      [0x00000, 0x04000, 'Boot'],
      [0x04000, 0x02000, 'Parameter'],
      [0x06000, 0x02000, 'Parameter'],
      [0x08000, 0x18000, 'Calibration'],
      [0x20000, 0x20000, 'OperatingSystem'],
    ],
  },
  0x00892275: {
    name: 'Intel 28F200BX 256KiB',
    size: 262144,
    blocks: [
      [0x00000, 0x04000, 'Boot'],
      [0x04000, 0x02000, 'Parameter'],
      [0x06000, 0x02000, 'Parameter'],
      [0x08000, 0x18000, 'Calibration'],
      [0x20000, 0x20000, 'OperatingSystem'],
    ],
  },
  0x00894471: {
    name: 'Intel 28F400B 512KiB',
    size: 524288,
    blocks: [
      [0x00000, 0x04000, 'Boot'],
      [0x04000, 0x02000, 'Parameter'],
      [0x06000, 0x02000, 'Parameter'],
      [0x08000, 0x18000, 'Calibration'],
      [0x20000, 0x20000, 'OperatingSystem'],
      [0x40000, 0x20000, 'OperatingSystem'],
      [0x60000, 0x20000, 'OperatingSystem'],
    ],
  },
  0x00012258: {
    name: 'AMD Am29F400BT 512KiB Top-Boot',
    size: 524288,
    blocks: [
      [0x00000, 0x10000, 'OperatingSystem'],
      [0x10000, 0x10000, 'OperatingSystem'],
      [0x20000, 0x10000, 'OperatingSystem'],
      [0x30000, 0x10000, 'Calibration'],
      [0x70000, 0x08000, 'Parameter'],
      [0x78000, 0x04000, 'Parameter'],
      [0x7c000, 0x04000, 'Boot'],
    ],
  },
  0x000122d7: {
    name: 'AMD Am29F800BB 1024KiB Bottom-Boot',
    size: 1048576,
    blocks: [
      [0x00000, 0x04000, 'Boot'],
      [0x04000, 0x02000, 'Parameter'],
      [0x06000, 0x02000, 'Parameter'],
      [0x08000, 0x18000, 'Calibration'],
      [0x20000, 0x20000, 'OperatingSystem'],
      [0x40000, 0x40000, 'OperatingSystem'],
      [0x80000, 0x80000, 'OperatingSystem'],
    ],
  },
};

/**
 * Describes a NOR flash target from its JEDEC id.
 *
 * Read-only metadata lookup. Returns `null` for unknown ids.
 */
export async function bridgeDescribeFlashTarget(
  chipId: number
): Promise<FlashTargetReport | null> {
  if (isDesktopApp()) {
    try {
      return await invoke<FlashTargetReport>('describe_flash_target', { chipId });
    } catch (err) {
      console.warn('Tauri describe_flash_target IPC error, falling back:', err);
    }
  }

  const entry = FLASH_TARGETS[chipId];
  if (!entry) return null;

  const blocks: FlashBlockEntry[] = entry.blocks.map(([start, len, type]) => ({
    start_address: start,
    length: len,
    block_type: type,
    write_safe: type !== 'Boot',
  }));

  return {
    jedec_id: chipId,
    model_name: entry.name,
    total_size_bytes: entry.size,
    blocks,
    boot_block_count: blocks.filter((b) => b.block_type === 'Boot').length,
    write_safe_block_count: blocks.filter((b) => b.write_safe).length,
  };
}

/**
 * Result of a pure checksum computation.
 */

/**
 * Saves binary ROM data.
 */
export async function bridgeSaveBinData(
  romData: Uint8Array | null
): Promise<Uint8Array | null> {
  if (isDesktopApp()) {
    try {
      const buf = await invoke<ArrayBuffer>('save_bin_raw');
      return new Uint8Array(buf);
    } catch (err) {
      console.warn('Tauri save_bin_raw IPC error, falling back:', err);
    }
  }
  return romData;
}

/**
 * Reads the loaded ROM's base address, or `null` when unknown.
 *
 * The browser fallback returns `null`: there is no authoritative store outside
 * the desktop backend, and inventing an address would defeat the purpose.
 */
export async function bridgeGetRomBaseAddress(): Promise<number | null> {
  if (isDesktopApp()) {
    try {
      return await invoke<number | null>('get_rom_base_address');
    } catch (err) {
      console.warn('Tauri get_rom_base_address IPC error, falling back:', err);
    }
  }
  return null;
}

/**
 * States the loaded ROM's base address. `null` clears it back to unknown.
 *
 * Returns whether the backend accepted the value. The browser fallback cannot
 * persist it and reports `false` rather than pretending it succeeded.
 */
export async function bridgeSetRomBaseAddress(base: number | null): Promise<boolean> {
  if (isDesktopApp()) {
    try {
      await invoke('set_rom_base_address', { base });
      return true;
    } catch (err) {
      console.warn('Tauri set_rom_base_address IPC error, falling back:', err);
    }
  }
  return false;
}

/**
 * Parses a user-entered base address.
 *
 * Accepts an optional `0x` prefix and surrounding whitespace. Empty input means
 * "unknown" and yields `null`. Anything that is not a valid `u32` is rejected
 * with an actionable message rather than silently coerced.
 */
export function parseBaseAddressInput(
  raw: string
): { ok: true; value: number | null } | { ok: false; error: string } {
  const text = raw.trim();
  if (text === '') return { ok: true, value: null };

  const body = /^0x/i.test(text) ? text.slice(2) : text;
  if (body === '' || !/^[0-9a-fA-F]+$/.test(body)) {
    return { ok: false, error: 'Enter a hexadecimal address, or leave empty for unknown.' };
  }

  const value = parseInt(body, 16);
  if (!Number.isFinite(value) || value < 0 || value > 0xffffffff) {
    return { ok: false, error: 'Address must fit in 32 bits (0x00000000-0xFFFFFFFF).' };
  }
  return { ok: true, value };
}
