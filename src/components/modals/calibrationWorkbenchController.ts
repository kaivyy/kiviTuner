/**
 * Pure controller logic for CalibrationWorkbenchModal.
 *
 * Keeps IPC and validation separate from React state so node --test can
 * pin empty-ROM, no-backend, base-offset, checksum, and solver paths.
 */

import {
  bitsSignedToDtype,
  bridgeApplyMapSelectionEdit,
  bridgeApplyXdfPatch,
  bridgeAutoDetectChecksums,
  bridgeBlindAutomapV2,
  bridgeClassifyBlindTables,
  bridgeFindMigratedMaps,
  bridgeFindSimilarMaps,
  bridgeInterpolateMapRegion,
  bridgeListChecksumEngines,
  bridgeParseA2lContent,
  bridgeParseDamosContent,
  bridgeParseXdfFullDetails,
  bridgeRepairChecksum,
  bridgeSolveFactorOffset,
  littleBoolToEndian,
  type A2lImportSummary,
  type A2lMapSummary,
  type ApplyXdfPatchResult,
  type BlindTable,
  type CellSelection,
  type ChecksumEngineInfo,
  type ChecksumRepairMode,
  type DamosEntrySummary,
  type DamosImportSummary,
  type DetectedChecksumMatch,
  type FactorOffsetSolution,
  type MapClassification,
  type PatchDef,
  type RegionInterpOptions,
  type RegionInterpResult,
  type RepairChecksumResult,
  type SelectionEditOp,
  type SimilarMapHit,
  type SimilarMapOptions,
  type SimilarMapSpec,
  type XdfFullDefinitions,
} from '../../lib/workbenchBridge.ts';
import type { DType, Endian, MapDef } from '../../types/kivi.ts';

export type WorkbenchState<T> =
  | { kind: 'ok'; data: T }
  | { kind: 'no-backend'; message: string }
  | { kind: 'error'; message: string };

export const NO_BACKEND_MSG =
  'This workbench operation requires the Tauri desktop backend. The browser build does not bundle native calibration engines.';

export const NO_ROM_MSG = 'Load a binary ROM image first.';

export interface ClassifiedBlindEntry {
  table: BlindTable;
  classification: MapClassification | null;
}

export interface BlindV2RunOptions {
  includeCurves: boolean;
  include8bit: boolean;
  include32bit: boolean;
  maxTables: number;
}

export function parseAddressInput(raw: string): number | null {
  const s = raw.trim();
  if (!s) return null;
  if (/^0x[0-9a-fA-F]+$/.test(s)) {
    const val = Number.parseInt(s.slice(2), 16);
    return Number.isSafeInteger(val) && val >= 0 ? val : null;
  }
  if (/^[0-9]+$/.test(s)) {
    const val = Number.parseInt(s, 10);
    return Number.isSafeInteger(val) && val >= 0 ? val : null;
  }
  return null;
}

export async function runBlindV2WithClassification(
  rom: Uint8Array | null,
  options: BlindV2RunOptions,
  blindFn: typeof bridgeBlindAutomapV2 = bridgeBlindAutomapV2,
  classifyFn: typeof bridgeClassifyBlindTables = bridgeClassifyBlindTables,
  workspaceLoaded = false
): Promise<WorkbenchState<ClassifiedBlindEntry[]>> {
  if ((!rom || rom.length === 0) && !workspaceLoaded) {
    return { kind: 'error', message: NO_ROM_MSG };
  }
  if (!Number.isFinite(options.maxTables) || options.maxTables < 1 || options.maxTables > 512) {
    return { kind: 'error', message: 'Max tables must be between 1 and 512.' };
  }
  try {
    const tables = await blindFn(
      rom,
      options.includeCurves,
      options.include8bit,
      options.include32bit,
      options.maxTables
    );
    if (tables === null) return { kind: 'no-backend', message: NO_BACKEND_MSG };
    if (tables.length === 0) return { kind: 'ok', data: [] };

    const classifications = await classifyFn(tables, rom, null);
    const paired: ClassifiedBlindEntry[] = tables.map((table, idx) => ({
      table,
      classification: classifications ? classifications[idx] ?? null : null,
    }));
    return { kind: 'ok', data: paired };
  } catch (err) {
    return { kind: 'error', message: String(err) };
  }
}

export function blindTableToMapDef(
  entry: ClassifiedBlindEntry,
  index: number
): MapDef {
  const { table, classification } = entry;
  const isFloat32 = Boolean(table.is_float && table.cell_bits === 32);
  const dtype: DType = isFloat32
    ? 'f32'
    : bitsSignedToDtype(table.cell_bits, Boolean(table.is_signed));
  const endian: Endian = littleBoolToEndian(table.little_endian);
  const name = classification
    ? `${classification.role_name} @ 0x${table.address.toString(16).toUpperCase()}`
    : `${table.label} (#${index + 1})`;
  const category = classification
    ? classification.category
    : table.is_curve || table.rows <= 1
    ? 'Blind 1D Curves'
    : 'Blind 2D Maps';
  const scaling = classification
    ? {
        equation: classification.equation,
        unit: classification.unit,
        decimals: classification.decimals,
      }
    : {
        equation: table.suggested_scaling.equation || 'X',
        unit: table.suggested_scaling.unit || '',
        decimals: table.suggested_scaling.decimals ?? 2,
      };

  return {
    id: `blind_v2_${table.address.toString(16)}_${table.rows}x${table.cols}`,
    name,
    category,
    address: table.address,
    rows: table.rows,
    cols: table.cols,
    dtype,
    endian,
    scaling,
    x_axis: table.x_axis
      ? {
          address: table.x_axis.address,
          length: table.x_axis.points,
          dtype: table.x_axis.is_16bit ? 'u16' : 'u8',
          endian: littleBoolToEndian(table.x_axis.little_endian),
          scaling: { equation: 'X', unit: '', decimals: 0 },
          values: table.x_axis.values,
        }
      : null,
    y_axis: table.y_axis
      ? {
          address: table.y_axis.address,
          length: table.y_axis.points,
          dtype: table.y_axis.is_16bit ? 'u16' : 'u8',
          endian: littleBoolToEndian(table.y_axis.little_endian),
          scaling: { equation: 'X', unit: '', decimals: 0 },
          values: table.y_axis.values,
        }
      : null,
  };
}

export async function runImportA2l(
  content: string,
  fn: typeof bridgeParseA2lContent = bridgeParseA2lContent
): Promise<WorkbenchState<A2lImportSummary>> {
  if (!content.trim()) return { kind: 'error', message: 'A2L text input is empty.' };
  try {
    const res = await fn(content);
    if (res === null) return { kind: 'no-backend', message: NO_BACKEND_MSG };
    return { kind: 'ok', data: res };
  } catch (err) {
    return { kind: 'error', message: String(err) };
  }
}

export async function runImportDamos(
  content: string,
  fn: typeof bridgeParseDamosContent = bridgeParseDamosContent
): Promise<WorkbenchState<DamosImportSummary>> {
  if (!content.trim()) return { kind: 'error', message: 'DAMOS text input is empty.' };
  try {
    const res = await fn(content);
    if (res === null) return { kind: 'no-backend', message: NO_BACKEND_MSG };
    return { kind: 'ok', data: res };
  } catch (err) {
    return { kind: 'error', message: String(err) };
  }
}

export async function runImportXdfFull(
  xml: string,
  fn: typeof bridgeParseXdfFullDetails = bridgeParseXdfFullDetails
): Promise<WorkbenchState<XdfFullDefinitions>> {
  if (!xml.trim()) return { kind: 'error', message: 'XDF XML input is empty.' };
  try {
    const res = await fn(xml);
    if (res === null) return { kind: 'no-backend', message: NO_BACKEND_MSG };
    return { kind: 'ok', data: res };
  } catch (err) {
    return { kind: 'error', message: String(err) };
  }
}

export function a2lEntryToMapDef(entry: A2lMapSummary, baseAddress = 0): MapDef {
  const offset = entry.address >= baseAddress ? entry.address - baseAddress : entry.address;
  const rows = Math.max(1, entry.rows || 1);
  const cols = Math.max(1, entry.cols || 1);
  const dtype: DType = entry.record_layout.toUpperCase().includes('UBYTE') ? 'u8' : 'u16';
  return {
    id: `a2l_${entry.name}_${offset.toString(16)}`,
    name: entry.name,
    category: entry.kind === 'CURVE' ? 'A2L Curves' : 'A2L Maps',
    address: offset,
    rows,
    cols,
    dtype,
    endian: 'little',
    scaling: {
      equation: 'X',
      unit: entry.unit || '',
      decimals: 2,
    },
    x_axis: null,
    y_axis: null,
  };
}

export function damosEntryToMapDef(entry: DamosEntrySummary, baseAddress = 0): MapDef {
  const offset = entry.address >= baseAddress ? entry.address - baseAddress : entry.address;
  const rows = Math.max(1, entry.rows || 1);
  const cols = Math.max(1, entry.cols || 1);
  const bits = entry.elem_bytes === 4 ? 32 : entry.elem_bytes === 2 ? 16 : 8;
  return {
    id: `damos_${entry.name}_${offset.toString(16)}`,
    name: entry.name,
    category: entry.is_1d_curve ? 'DAMOS Curves' : 'DAMOS Maps',
    address: offset,
    rows,
    cols,
    dtype: bitsSignedToDtype(bits, entry.is_signed),
    endian: 'little',
    scaling: { equation: 'X', unit: '', decimals: 2 },
    x_axis: null,
    y_axis: null,
  };
}

export async function runXdfPatchToggle(
  rom: Uint8Array | null,
  patch: PatchDef,
  revert: boolean,
  fn: typeof bridgeApplyXdfPatch = bridgeApplyXdfPatch
): Promise<WorkbenchState<ApplyXdfPatchResult>> {
  if (!rom || rom.length === 0) return { kind: 'error', message: NO_ROM_MSG };
  try {
    const res = await fn(rom, patch, revert);
    if (res === null) return { kind: 'no-backend', message: NO_BACKEND_MSG };
    return { kind: 'ok', data: res };
  } catch (err) {
    return { kind: 'error', message: String(err) };
  }
}

export async function runSimilaritySearch(
  mode: 'in-rom' | 'cross-rom',
  sourceRom: Uint8Array | null,
  targetRom: Uint8Array | null,
  spec: SimilarMapSpec,
  options: SimilarMapOptions,
  inRomFn: typeof bridgeFindSimilarMaps = bridgeFindSimilarMaps,
  crossRomFn: typeof bridgeFindMigratedMaps = bridgeFindMigratedMaps
): Promise<WorkbenchState<SimilarMapHit[]>> {
  if (!sourceRom || sourceRom.length === 0) return { kind: 'error', message: NO_ROM_MSG };
  if (spec.rows < 1 || spec.cols < 1) {
    return { kind: 'error', message: 'Rows and cols must be at least 1.' };
  }
  try {
    if (mode === 'cross-rom') {
      if (!targetRom || targetRom.length === 0) {
        return { kind: 'error', message: 'Load a target ROM binary for cross-ROM map migration.' };
      }
      const res = await crossRomFn(sourceRom, targetRom, spec, options);
      if (res === null) return { kind: 'no-backend', message: NO_BACKEND_MSG };
      return { kind: 'ok', data: res };
    }
    const res = await inRomFn(spec, options);
    if (res === null) return { kind: 'no-backend', message: NO_BACKEND_MSG };
    return { kind: 'ok', data: res };
  } catch (err) {
    return { kind: 'error', message: String(err) };
  }
}

export function similarHitToMapDef(hit: SimilarMapHit, idx: number, prefix: string): MapDef {
  return {
    id: `${prefix}_${hit.address.toString(16)}_${idx}`,
    name: `${prefix.toUpperCase()} @ 0x${hit.address.toString(16).toUpperCase()} (r=${hit.pearson_r.toFixed(3)})`,
    category: prefix === 'migrated' ? 'Migrated Maps' : 'Similar Maps',
    address: hit.address,
    rows: hit.rows,
    cols: hit.cols,
    dtype: bitsSignedToDtype(hit.cell_bits, false),
    endian: littleBoolToEndian(hit.little_endian),
    scaling: { equation: 'X', unit: '', decimals: 2 },
    x_axis: null,
    y_axis: null,
  };
}

export async function runListChecksumEngines(
  fn: typeof bridgeListChecksumEngines = bridgeListChecksumEngines
): Promise<WorkbenchState<ChecksumEngineInfo[]>> {
  try {
    const res = await fn();
    if (res === null) return { kind: 'no-backend', message: NO_BACKEND_MSG };
    return { kind: 'ok', data: res };
  } catch (err) {
    return { kind: 'error', message: String(err) };
  }
}

export async function runAutoDetectChecksums(
  rom: Uint8Array | null,
  fn: typeof bridgeAutoDetectChecksums = bridgeAutoDetectChecksums,
  workspaceLoaded = false
): Promise<WorkbenchState<DetectedChecksumMatch[]>> {
  if ((!rom || rom.length === 0) && !workspaceLoaded) return { kind: 'error', message: NO_ROM_MSG };
  try {
    const res = await fn(rom);
    if (res === null) return { kind: 'no-backend', message: NO_BACKEND_MSG };
    return { kind: 'ok', data: res };
  } catch (err) {
    return { kind: 'error', message: String(err) };
  }
}

export async function runRepairChecksum(
  rom: Uint8Array | null,
  algorithmId: string,
  mode: ChecksumRepairMode,
  fn: typeof bridgeRepairChecksum = bridgeRepairChecksum
): Promise<WorkbenchState<RepairChecksumResult>> {
  if (!rom || rom.length === 0) return { kind: 'error', message: NO_ROM_MSG };
  try {
    const res = await fn(rom, algorithmId, mode);
    if (res === null) return { kind: 'no-backend', message: NO_BACKEND_MSG };
    return { kind: 'ok', data: res };
  } catch (err) {
    return { kind: 'error', message: String(err) };
  }
}

export async function runSelectionEdit(
  matrix: number[][] | null,
  selection: CellSelection,
  op: SelectionEditOp,
  fn: typeof bridgeApplyMapSelectionEdit = bridgeApplyMapSelectionEdit
): Promise<WorkbenchState<number[][]>> {
  if (!matrix || matrix.length === 0) {
    return { kind: 'error', message: 'Select a map with loaded matrix cells first.' };
  }
  try {
    const res = await fn(matrix, selection, op);
    if (res === null) return { kind: 'no-backend', message: NO_BACKEND_MSG };
    return { kind: 'ok', data: res };
  } catch (err) {
    return { kind: 'error', message: String(err) };
  }
}

export async function runRegionInterpolation(
  matrix: number[][] | null,
  selection: CellSelection,
  options: RegionInterpOptions,
  fn: typeof bridgeInterpolateMapRegion = bridgeInterpolateMapRegion
): Promise<WorkbenchState<RegionInterpResult>> {
  if (!matrix || matrix.length === 0) {
    return { kind: 'error', message: 'Select a map with loaded matrix cells first.' };
  }
  try {
    const res = await fn(matrix, selection, options);
    if (res === null) return { kind: 'no-backend', message: NO_BACKEND_MSG };
    return { kind: 'ok', data: res };
  } catch (err) {
    return { kind: 'error', message: String(err) };
  }
}

export async function runFactorOffsetSolve(
  p1: [number, number],
  p2: [number, number],
  p3: [number, number] | null,
  swapInputOutput: boolean,
  fn: typeof bridgeSolveFactorOffset = bridgeSolveFactorOffset
): Promise<WorkbenchState<FactorOffsetSolution>> {
  const pts = p3 ? [p1, p2, p3] : [p1, p2];
  if (pts.some(([x, y]) => !Number.isFinite(x) || !Number.isFinite(y))) {
    return { kind: 'error', message: 'All solver point coordinates must be finite numbers.' };
  }
  try {
    const res = await fn(p1, p2, p3, swapInputOutput);
    if (res === null) return { kind: 'no-backend', message: NO_BACKEND_MSG };
    return { kind: 'ok', data: res };
  } catch (err) {
    return { kind: 'error', message: String(err) };
  }
}
