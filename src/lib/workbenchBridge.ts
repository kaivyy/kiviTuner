/**
 * Typed bridge for kiviTuner's Calibration Workbench:
 * Blind V2 auto-map, semantic classifier, A2L/DAMOS/full-XDF import & patch,
 * similar/migrated map matching, multi-ECU checksum engines (including MG1 GF(2)
 * keep-CRC), matrix selection edits, interpolation, and conversion solvers.
 */

import { Channel, invoke } from '@tauri-apps/api/core';
import { isDesktopApp } from './tauriBridge.ts';
import type { ScanProgressEvent, UndoStatus } from './bridge/windowBridge.ts';
import type { ConstantDef, DType, Endian, FlagDef, MapDef } from '../types/kivi.ts';

export interface BlindAxis {
  address: number;
  points: number;
  is_16bit: boolean;
  little_endian: boolean;
  values: number[];
}

export interface SuggestedScaling {
  equation: string;
  unit: string;
  decimals: number;
  rationale: string;
}

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
  is_signed?: boolean;
  is_float?: boolean;
  is_curve?: boolean;
  detector_source?: string;
}

export interface MapClassification {
  rule_id: string;
  group_name: string;
  role_name: string;
  category: string;
  unit: string;
  equation: string;
  factor: number;
  offset: number;
  decimals: number;
  min_region: string;
  max_region: string;
  is_proposal: boolean;
  rationale: string;
}

export interface A2lMapSummary {
  name: string;
  description: string;
  kind: string;
  address: number;
  rows: number;
  cols: number;
  record_layout: string;
  conversion: string;
  unit: string;
}

export interface A2lImportSummary {
  ecu_base_suggestions: number[];
  characteristic_count: number;
  map_count: number;
  curve_count: number;
  value_count: number;
  axis_pts_count: number;
  memory_segment_count: number;
  entries: A2lMapSummary[];
  diagnostics: string[];
}

export interface DamosEntrySummary {
  source: string;
  name: string;
  description: string;
  address: number;
  rows: number;
  cols: number;
  elem_bytes: number;
  is_signed: boolean;
  is_2d_map: boolean;
  is_1d_curve: boolean;
}

export interface DamosImportSummary {
  ecu: string | null;
  family: string | null;
  base_address: number | null;
  ump_count: number;
  spz_total_count: number;
  spz_2d_map_count: number;
  spz_1d_curve_count: number;
  characteristic_count: number;
  entries: DamosEntrySummary[];
}

export interface PatchEntryDef {
  name: string;
  address: number;
  patch_data: number[];
  base_data: number[];
}

export interface PatchDef {
  id: string;
  name: string;
  category: string;
  description: string;
  entries: PatchEntryDef[];
}

export interface XdfChecksumDef {
  id: string;
  name: string;
  start_address: number;
  end_address: number;
  store_address: number;
  data_bits: number;
  sum_bits: number;
  little_endian: boolean;
  calculation_method: string;
}

export interface XdfFullDefinitions {
  maps: MapDef[];
  constants: ConstantDef[];
  flags: FlagDef[];
  patches: PatchDef[];
  checksums: XdfChecksumDef[];
}

export type PatchStatus =
  | 'Original'
  | 'Applied'
  | { Conflict: { entry_index: number; address: number } }
  | { OutOfBounds: { entry_index: number; address: number } };

export interface ApplyXdfPatchResult {
  rom: number[];
  status_after: PatchStatus;
}

export interface SimilarMapSpec {
  address: number;
  rows: number;
  cols: number;
  cell_bits: number;
  little_endian: boolean;
  is_signed: boolean;
}

export interface SimilarMapOptions {
  min_pearson_r: number;
  max_nrmse: number;
  max_rel_diff: number | null;
  stride_bytes: number;
  exclude_overlapping_source: boolean;
  max_results: number;
}

export interface SimilarMapHit {
  address: number;
  rows: number;
  cols: number;
  cell_bits: number;
  little_endian: boolean;
  pearson_r: number;
  nrmse: number;
  mean_abs_diff: number;
  max_rel_diff: number;
  is_exact_copy: boolean;
}

export type ChecksumRepairMode = 'UpdateChecksum' | 'CompensateKeepCrc';

export interface ChecksumEngineInfo {
  id: string;
  name: string;
  status: string;
  description: string;
  supports_keep_crc_mode: boolean;
}

export interface ChecksumBlockStatus {
  label: string;
  offset: number;
  start: number;
  end: number;
  stored: number;
  computed: number;
  width_bits: number;
  ok: boolean;
}

export interface DetectedChecksumMatch {
  algorithm_id: string;
  algorithm_name: string;
  status: string;
  notes: string;
  supports_keep_crc_mode: boolean;
  blocks: ChecksumBlockStatus[];
  all_ok: boolean;
}

export interface RepairChecksumResult {
  rom: number[];
  match_info: DetectedChecksumMatch;
}

export interface RepairChecksumWorkspaceResult {
  sha256: string;
  match_info: DetectedChecksumMatch;
  undo: UndoStatus;
}

export interface ApplyXdfPatchWorkspaceResult {
  sha256: string;
  status_after: PatchStatus;
  undo: UndoStatus;
}

export interface CellSelection {
  row_start: number;
  row_end: number;
  col_start: number;
  col_end: number;
}

export type SelectionEditOp =
  | { SetAbsolute: number }
  | { AddDelta: number }
  | { ScalePercent: number }
  | { ClampRange: { min: number; max: number } };

export type RegionInterpMode = 'Linear' | 'CubicCatmullRom';
export type RegionInterpAxes = 'HorizontalOnly' | 'VerticalOnly' | 'AllFourSides';

export interface RegionInterpOptions {
  mode: RegionInterpMode;
  axes: RegionInterpAxes;
  allow_extrapolation: boolean;
  fallback_all_sides: boolean;
}

export interface RegionInterpResult {
  matrix: number[][];
  applied_mode: RegionInterpMode;
  applied_axes: RegionInterpAxes;
  used_extrapolation: boolean;
}

export interface FactorOffsetSolution {
  factor: number;
  offset: number;
  reciprocal: boolean;
  reciprocal_num: number;
  reciprocal_den_offset: number;
  equation: string;
}

export async function bridgeBlindAutomapV2(
  romData: Uint8Array | null,
  includeCurves: boolean,
  include8bit: boolean,
  include32bit: boolean,
  maxTables?: number
): Promise<BlindTable[] | null> {
  if (!isDesktopApp()) return null;
  if (!romData || romData.length === 0) {
    return await invoke<BlindTable[]>('blind_automap_v2_workspace', {
      includeCurves,
      include8bit,
      include32bit,
      maxTables: maxTables ?? null,
    });
  }
  return await invoke<BlindTable[]>('blind_automap_v2_rom', {
    romData: Array.from(romData),
    includeCurves,
    include8bit,
    include32bit,
    maxTables: maxTables ?? null,
  });
}

export async function bridgeBlindAutomapV2Stream(
  options: {
    includeCurves: boolean;
    include8bit: boolean;
    include32bit: boolean;
    maxTables?: number;
  },
  onProgress: (event: ScanProgressEvent) => void
): Promise<BlindTable[] | null> {
  if (!isDesktopApp()) return null;
  const channel = new Channel<ScanProgressEvent>(onProgress);
  return await invoke<BlindTable[]>('blind_automap_v2_stream', {
    includeCurves: options.includeCurves,
    include8bit: options.include8bit,
    include32bit: options.include32bit,
    maxTables: options.maxTables ?? null,
    onProgress: channel,
  });
}

export async function bridgeClassifyBlindTables(
  tables: BlindTable[],
  rom?: Uint8Array | null,
  rulesJson?: string | null
): Promise<(MapClassification | null)[] | null> {
  if (!isDesktopApp()) return null;
  if (!rom || rom.length === 0) {
    return await invoke<(MapClassification | null)[]>('classify_blind_tables_workspace', {
      tables,
      useWorkspaceRom: true,
      rulesJson: rulesJson ?? null,
    });
  }
  return await invoke<(MapClassification | null)[]>('classify_blind_tables', {
    tables,
    rom: Array.from(rom),
    rulesJson: rulesJson ?? null,
  });
}

export async function bridgeParseA2lContent(content: string): Promise<A2lImportSummary | null> {
  if (!isDesktopApp()) return null;
  return await invoke<A2lImportSummary>('parse_a2l_content', { content });
}

export async function bridgeParseDamosContent(content: string): Promise<DamosImportSummary | null> {
  if (!isDesktopApp()) return null;
  return await invoke<DamosImportSummary>('parse_damos_content', { content });
}

export async function bridgeParseXdfFullDetails(xml: string): Promise<XdfFullDefinitions | null> {
  if (!isDesktopApp()) return null;
  return await invoke<XdfFullDefinitions>('parse_xdf_full_details', { xml });
}

export async function bridgeApplyXdfPatch(
  rom: Uint8Array | null,
  patch: PatchDef,
  revert: boolean
): Promise<ApplyXdfPatchResult | null> {
  if (!isDesktopApp()) return null;
  return await invoke<ApplyXdfPatchResult>('apply_xdf_patch', {
    rom: rom ? Array.from(rom) : null,
    patch,
    revert,
  });
}

export async function bridgeFindSimilarMaps(
  source: SimilarMapSpec,
  options?: SimilarMapOptions | null
): Promise<SimilarMapHit[] | null> {
  if (!isDesktopApp()) return null;
  return await invoke<SimilarMapHit[]>('find_similar_maps', {
    source,
    options: options ?? null,
  });
}

export async function bridgeFindMigratedMaps(
  sourceRom: Uint8Array,
  targetRom: Uint8Array,
  source: SimilarMapSpec,
  options?: SimilarMapOptions | null
): Promise<SimilarMapHit[] | null> {
  if (!isDesktopApp()) return null;
  return await invoke<SimilarMapHit[]>('find_migrated_maps', {
    sourceRom: Array.from(sourceRom),
    targetRom: Array.from(targetRom),
    source,
    options: options ?? null,
  });
}

export async function bridgeListChecksumEngines(): Promise<ChecksumEngineInfo[] | null> {
  if (!isDesktopApp()) return null;
  return await invoke<ChecksumEngineInfo[]>('list_checksum_engines');
}

export async function bridgeAutoDetectChecksums(
  rom?: Uint8Array | null
): Promise<DetectedChecksumMatch[] | null> {
  if (!isDesktopApp()) return null;
  return await invoke<DetectedChecksumMatch[]>('auto_detect_checksums', {
    rom: rom ? Array.from(rom) : null,
  });
}

export async function bridgeRepairChecksum(
  rom: Uint8Array | null,
  algorithmId: string,
  mode: ChecksumRepairMode
): Promise<RepairChecksumResult | null> {
  if (!isDesktopApp()) return null;
  return await invoke<RepairChecksumResult>('repair_checksum', {
    rom: rom ? Array.from(rom) : null,
    algorithmId,
    mode,
  });
}

export async function bridgeApplyMapSelectionEdit(
  matrix: number[][],
  selection: CellSelection,
  op: SelectionEditOp
): Promise<number[][] | null> {
  if (!isDesktopApp()) return null;
  return await invoke<number[][]>('apply_map_selection_edit', {
    matrix,
    selection,
    op,
  });
}

export async function bridgeInterpolateMapRegion(
  matrix: number[][],
  selection: CellSelection,
  options: RegionInterpOptions
): Promise<RegionInterpResult | null> {
  if (!isDesktopApp()) return null;
  return await invoke<RegionInterpResult>('interpolate_map_region', {
    matrix,
    selection,
    options,
  });
}

export async function bridgeSolveFactorOffset(
  p1: [number, number],
  p2: [number, number],
  p3?: [number, number] | null,
  swapInputOutput = false
): Promise<FactorOffsetSolution | null> {
  if (!isDesktopApp()) return null;
  return await invoke<FactorOffsetSolution>('solve_factor_offset', {
    p1,
    p2,
    p3: p3 ?? null,
    swapInputOutput,
  });
}

export async function bridgeApplyXdfPatchWorkspace(
  patch: PatchDef,
  revert: boolean
): Promise<ApplyXdfPatchWorkspaceResult | null> {
  if (!isDesktopApp()) return null;
  return await invoke<ApplyXdfPatchWorkspaceResult>('apply_xdf_patch_workspace', {
    patch,
    revert,
  });
}

export async function bridgeFindMigratedMapsWorkspaceRaw(
  targetRom: Uint8Array,
  source: SimilarMapSpec,
  options?: SimilarMapOptions | null
): Promise<SimilarMapHit[] | null> {
  if (!isDesktopApp()) return null;
  return await invoke<SimilarMapHit[]>('find_migrated_maps_workspace_raw', targetRom, {
    headers: {
      'x-spec-json': JSON.stringify({
        source,
        options: options ?? null,
      }),
    },
  });
}

export async function bridgeRepairChecksumWorkspace(
  algorithmId: string,
  mode: ChecksumRepairMode
): Promise<RepairChecksumWorkspaceResult | null> {
  if (!isDesktopApp()) return null;
  return await invoke<RepairChecksumWorkspaceResult>('repair_checksum_workspace', {
    algorithmId,
    mode,
  });
}

export function mapDtypeToBitsSigned(dtype: DType): { bits: number; signed: boolean } {
  switch (dtype) {
    case 'u8':
      return { bits: 8, signed: false };
    case 'i8':
      return { bits: 8, signed: true };
    case 'u16':
      return { bits: 16, signed: false };
    case 'i16':
      return { bits: 16, signed: true };
    case 'u32':
    case 'f32':
      return { bits: 32, signed: false };
    case 'i32':
      return { bits: 32, signed: true };
  }
}

export function bitsSignedToDtype(bits: number, signed: boolean): DType {
  if (bits === 8) return signed ? 'i8' : 'u8';
  if (bits === 32) return signed ? 'i32' : 'u32';
  return signed ? 'i16' : 'u16';
}

export function littleBoolToEndian(little: boolean): Endian {
  return little ? 'little' : 'big';
}
