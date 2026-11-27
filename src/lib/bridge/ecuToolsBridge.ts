/**
 * Typed IPC bridge for native ECU calibration engines:
 * fingerprinting, Denso SH descriptor & DTC scanners, 22+ OEM checksums,
 * blind checksum sweep, XDF checksums, Seed/Key registry,
 * motorcycle ECU write-boundary DB, and ECU XML / INI / KP / OLS / HEX / E92 / FRF codecs.
 */

import { Channel, invoke } from '@tauri-apps/api/core';
import type { MapDef } from '../../types/kivi.ts';
import { isDesktopApp } from './romBridge.ts';
import type { ScanProgressEvent } from './windowBridge.ts';

export interface EcuFingerprint {
  manufacturer: string;
  family: string;
  variant: string | null;
  cal_ids: string[];
  hw_numbers: string[];
  sw_numbers: string[];
  part_numbers: string[];
  confidence: number;
  evidence: string[];
}

export interface DensoDescriptorMapHit {
  descriptor_offset: number;
  is_3d: boolean;
  cols: number;
  rows: number;
  ptr_x: number;
  ptr_y: number | null;
  ptr_z: number;
  element_bytes: number;
  smoothness: number;
}

export interface DtcCodeEntry {
  index: number;
  raw_code: number;
  formatted_code: string;
  switch_value: number | null;
}

export interface DtcTableCandidate {
  code_table_offset: number;
  big_endian: boolean;
  entry_count: number;
  switch_table_offset: number | null;
  entries: DtcCodeEntry[];
}

export interface OemAlgorithmInfo {
  id: string;
  name: string;
  family: string;
  supports_repair: boolean;
  description: string;
}

export interface OemChecksumBlock {
  label: string;
  start: number;
  end: number;
  store_offset: number | null;
  stored: number | null;
  computed: number | null;
  width_bits: number;
  correctable: boolean;
  ok: boolean;
  status: string;
}

export interface OemChecksumReport {
  algorithm_id: string;
  algorithm_name: string;
  family: string;
  variant: string;
  blocks: OemChecksumBlock[];
  all_ok: boolean;
  notes: string[];
}

export interface OemRepairPayload {
  report: OemChecksumReport;
  repaired_bytes: number[] | null;
  sha256: string | null;
}

export interface SweepMatch {
  algo: string;
  init: number;
  final_xor: number;
  region: string;
  exclude_tail: number;
  store_location: string;
  store_le: boolean;
  complement: boolean;
  pages_matched: number;
  pages_total: number;
  rate_bps: number;
}

export interface XdfChecksumResult {
  id: string;
  name: string;
  start_address: number;
  end_address: number;
  store_address: number;
  stored: number;
  computed: number;
  width_bits: number;
  ok: boolean;
}

export interface XdfRepairPayload {
  results: XdfChecksumResult[];
  repaired_bytes: number[] | null;
  sha256: string | null;
}

export interface SeedKeyEngineInfo {
  id: string;
  name: string;
  family: string;
  key_bits: number;
  verified_for_live_hardware?: boolean;
  notes: string;
}

export interface SeedKeyResult {
  algorithm_id: string;
  key_hex: string;
  key_bytes: number[];
}

export interface MotoEcuProfile {
  id: string;
  make: string;
  model: string;
  years: string;
  part_number: string;
  mcu: string;
  rom_size_bytes: number;
  writable_start: number;
  writable_end_incl: number;
  protected_ranges: [number, number][];
  checksum_offset: number | null;
  ecm_id_hex: string | null;
}

export interface DefinitionImportSummary {
  format: string;
  label: string;
  table_count: number;
  constant_count: number;
  flag_count: number;
  maps: MapDef[];
  warnings: string[];
}

export interface EcuIniSummary {
  signature: string;
  query_command: string;
  endianness: string;
  page_sizes: number[];
  blocking_factor: number;
  table_count: number;
  constant_count: number;
  flag_count: number;
  maps: MapDef[];
}

export interface OlsMapRow {
  name: string;
  kind: string;
  unit: string;
  address: number;
  rows: number;
  cols: number;
  data_size: number;
  scale: number;
  offset: number;
}

export interface OlsVersionRow {
  index: number;
  name: string;
  base_address: number;
  byte_len: number;
}

export interface OlsProjectSummary {
  format_version: number;
  make: string;
  model: string;
  ecu_name: string;
  hw_number: string;
  sw_number: string;
  versions: OlsVersionRow[];
  maps: OlsMapRow[];
  warnings: string[];
}

export interface HexDocumentSummary {
  format: string;
  metadata: Record<string, string>;
  entry_point: number | null;
  segments: Array<{ address: number; data: number[] }>;
}

export interface FrfUnpackPayload {
  entry_name: string;
  odx_xml: string;
  raw_len: number;
}

export async function bridgeFingerprintEcuRom(
  data?: Uint8Array | null
): Promise<EcuFingerprint | null> {
  if (!isDesktopApp()) return null;
  return await invoke<EcuFingerprint>('fingerprint_ecu_rom', {
    data: data ? Array.from(data) : null,
  });
}

export async function bridgeScanDensoShDescriptors(
  data?: Uint8Array | null
): Promise<DensoDescriptorMapHit[] | null> {
  if (!isDesktopApp()) return null;
  return await invoke<DensoDescriptorMapHit[]>('scan_denso_sh_descriptors', {
    data: data ? Array.from(data) : null,
  });
}

export async function bridgeScanDtcTables(
  minEntries = 6,
  data?: Uint8Array | null
): Promise<DtcTableCandidate[] | null> {
  if (!isDesktopApp()) return null;
  return await invoke<DtcTableCandidate[]>('scan_dtc_tables_cmd', {
    minEntries,
    data: data ? Array.from(data) : null,
  });
}

export async function bridgeListOemChecksumEngines(): Promise<OemAlgorithmInfo[] | null> {
  if (!isDesktopApp()) return null;
  return await invoke<OemAlgorithmInfo[]>('list_oem_checksum_engines');
}

export async function bridgeAutoDetectOemChecksums(
  data?: Uint8Array | null
): Promise<OemChecksumReport[] | null> {
  if (!isDesktopApp()) return null;
  return await invoke<OemChecksumReport[]>('auto_detect_oem_checksums_cmd', {
    data: data ? Array.from(data) : null,
  });
}

export async function bridgeVerifyOemChecksum(
  algorithmId: string,
  data?: Uint8Array | null
): Promise<OemChecksumReport | null> {
  if (!isDesktopApp()) return null;
  return await invoke<OemChecksumReport>('verify_oem_checksum_cmd', {
    algorithmId,
    data: data ? Array.from(data) : null,
  });
}

export async function bridgeRepairOemChecksum(
  algorithmId: string,
  data?: Uint8Array | null,
  applyToLoaded = true
): Promise<OemRepairPayload | null> {
  if (!isDesktopApp()) return null;
  return await invoke<OemRepairPayload>('repair_oem_checksum_cmd', {
    algorithmId,
    data: data ? Array.from(data) : null,
    applyToLoaded,
  });
}

export async function bridgeSweepBlindChecksums(
  data?: Uint8Array | null
): Promise<SweepMatch[] | null> {
  if (!isDesktopApp()) return null;
  return await invoke<SweepMatch[]>('sweep_blind_checksums_cmd', {
    data: data ? Array.from(data) : null,
  });
}

export async function bridgeVerifyXdfChecksums(
  xml: string,
  data?: Uint8Array | null
): Promise<XdfChecksumResult[] | null> {
  if (!isDesktopApp()) return null;
  return await invoke<XdfChecksumResult[]>('verify_xdf_checksums_cmd', {
    xml,
    data: data ? Array.from(data) : null,
  });
}

export async function bridgeRepairXdfChecksums(
  xml: string,
  data?: Uint8Array | null,
  applyToLoaded = true
): Promise<XdfRepairPayload | null> {
  if (!isDesktopApp()) return null;
  return await invoke<XdfRepairPayload>('repair_xdf_checksums_cmd', {
    xml,
    data: data ? Array.from(data) : null,
    applyToLoaded,
  });
}

export async function bridgeListSeedKeyAlgorithms(): Promise<SeedKeyEngineInfo[] | null> {
  if (!isDesktopApp()) return null;
  return await invoke<SeedKeyEngineInfo[]>('list_seed_key_algorithms_cmd');
}

export async function bridgeComputeOemSeedKey(
  algorithmId: string,
  seedHex: string,
  algoIndex = 0,
  extraHex?: string
): Promise<SeedKeyResult | null> {
  if (!isDesktopApp()) return null;
  return await invoke<SeedKeyResult>('compute_oem_seed_key_cmd', {
    algorithmId,
    seedHex,
    algoIndex,
    extraHex: extraHex?.trim() ? extraHex.trim() : null,
  });
}

export async function bridgeListMotoEcuProfiles(query?: string): Promise<MotoEcuProfile[] | null> {
  if (!isDesktopApp()) return null;
  return await invoke<MotoEcuProfile[]>('list_moto_ecu_profiles_cmd', {
    query: query ?? null,
  });
}

export async function bridgeCheckMotoWriteBoundaries(
  profileId: string,
  writeStart: number,
  writeLen: number
): Promise<string | null> {
  if (!isDesktopApp()) return 'Desktop backend required.';
  try {
    await invoke<void>('check_moto_write_boundaries_cmd', {
      profileId,
      writeStart,
      writeLen,
    });
    return null;
  } catch (err) {
    return String(err);
  }
}

export async function bridgeParseEcuXml(
  xml: string,
  loadIntoWorkspace = false
): Promise<DefinitionImportSummary | null> {
  if (!isDesktopApp()) return null;
  return await invoke<DefinitionImportSummary>('parse_ecu_xml_cmd', {
    xml,
    loadIntoWorkspace,
  });
}

export async function bridgeParseEcuIni(
  ini: string,
  activeDefines: string[] = [],
  loadIntoWorkspace = false
): Promise<EcuIniSummary | null> {
  if (!isDesktopApp()) return null;
  return await invoke<EcuIniSummary>('parse_ecu_ini_cmd', {
    ini,
    activeDefines,
    loadIntoWorkspace,
  });
}

export async function bridgeParseKpFile(
  data: Uint8Array,
  loadIntoWorkspace = false
): Promise<DefinitionImportSummary | null> {
  if (!isDesktopApp()) return null;
  return await invoke<DefinitionImportSummary>('parse_kp_file_cmd', {
    data: Array.from(data),
    loadIntoWorkspace,
  });
}

export async function bridgeParseOlsProject(data: Uint8Array): Promise<OlsProjectSummary | null> {
  if (!isDesktopApp()) return null;
  return await invoke<OlsProjectSummary>('parse_ols_project_cmd', {
    data: Array.from(data),
  });
}

export async function bridgeParseHexOrSrec(text: string): Promise<HexDocumentSummary | null> {
  if (!isDesktopApp()) return null;
  return await invoke<HexDocumentSummary>('parse_hex_or_srec_cmd', { text });
}

export async function bridgeExportIntelHex(
  baseAddress: number,
  data?: Uint8Array | null
): Promise<string | null> {
  if (!isDesktopApp()) return null;
  return await invoke<string>('export_intel_hex_cmd', {
    baseAddress,
    data: data ? Array.from(data) : null,
  });
}

export async function bridgeExportSrec(
  baseAddress: number,
  header?: string | null,
  data?: Uint8Array | null
): Promise<string | null> {
  if (!isDesktopApp()) return null;
  return await invoke<string>('export_srec_cmd', {
    baseAddress,
    header: header ?? null,
    data: data ? Array.from(data) : null,
  });
}

export async function bridgeUnpackFrfOdx(
  frfBytes: Uint8Array,
  keyHex: string
): Promise<FrfUnpackPayload | null> {
  if (!isDesktopApp()) return null;
  return await invoke<FrfUnpackPayload>('unpack_frf_odx_cmd', {
    frfBytes: Array.from(frfBytes),
    keyHex,
  });
}

export interface OwnedTriCoreCrcBlockSpec {
  label: string;
  start: number;
  end_incl: number;
  store_offset: number;
  little_endian: boolean;
}

export async function bridgeSweepBlindChecksumsStream(
  onProgress: (event: ScanProgressEvent) => void
): Promise<SweepMatch[] | null> {
  if (!isDesktopApp()) return null;
  const channel = new Channel<ScanProgressEvent>(onProgress);
  return await invoke<SweepMatch[]>('sweep_blind_checksums_stream', {
    onProgress: channel,
  });
}

export async function bridgeVerifyEdc17Crc32c(
  specs: OwnedTriCoreCrcBlockSpec[],
  data?: Uint8Array | null
): Promise<OemChecksumReport | null> {
  if (!isDesktopApp()) return null;
  return await invoke<OemChecksumReport>('verify_edc17_crc32c_cmd', {
    specs,
    data: data ? Array.from(data) : null,
  });
}

export async function bridgeRepairEdc17Crc32c(
  specs: OwnedTriCoreCrcBlockSpec[],
  data?: Uint8Array | null,
  applyToLoaded = true
): Promise<OemRepairPayload | null> {
  if (!isDesktopApp()) return null;
  return await invoke<OemRepairPayload>('repair_edc17_crc32c_cmd', {
    specs,
    data: data ? Array.from(data) : null,
    applyToLoaded,
  });
}

export async function bridgeExportXdfXml(
  title?: string,
  maps?: MapDef[] | null
): Promise<string | null> {
  if (!isDesktopApp()) return null;
  return await invoke<string>('export_xdf_xml_cmd', {
    title: title ?? null,
    maps: maps ?? null,
  });
}

export async function bridgeExportEcuIni(
  signature?: string,
  pageSize?: number,
  maps?: MapDef[] | null
): Promise<string | null> {
  if (!isDesktopApp()) return null;
  return await invoke<string>('export_ecu_ini_cmd', {
    signature: signature ?? null,
    pageSize: pageSize ?? null,
    maps: maps ?? null,
  });
}

export async function bridgeExportKpAr(
  ecuId?: string,
  maps?: MapDef[] | null
): Promise<string | null> {
  if (!isDesktopApp()) return null;
  return await invoke<string>('export_kp_ar_cmd', {
    ecuId: ecuId ?? null,
    maps: maps ?? null,
  });
}

export interface CustomChecksumBlockResult {
  name: string;
  start: number;
  end_inclusive: number;
  store_offset: number;
  width_bits: number;
  stored_hex: string;
  computed_hex: string;
  valid: boolean;
}

export interface CustomChecksumReport {
  plugin_id: string;
  plugin_name: string;
  family: string;
  blocks: CustomChecksumBlockResult[];
  all_valid: boolean;
}

export interface CustomChecksumRepairPayload {
  report: CustomChecksumReport;
  repaired_bytes: number[] | null;
  sha256: string | null;
}

export type RuleDryRunStatus =
  | 'Ready'
  | 'NotFound'
  | 'Ambiguous'
  | 'ExpectedMismatch'
  | 'OutOfBounds'
  | 'InvalidRule';

export interface RuleDryRunOutcome {
  rule_name: string;
  status: RuleDryRunStatus;
  match_count: number;
  matched_offsets: number[];
  target_offset: number | null;
  bytes_changed: number;
  before_hex: string;
  after_hex: string;
  detail: string;
}

export interface CalibrationScriptDryRun {
  script_name: string;
  can_apply: boolean;
  total_bytes_changed: number;
  rules: RuleDryRunOutcome[];
}

export interface CalibrationScriptApplyPayload {
  dry_run: CalibrationScriptDryRun;
  sha256: string;
  can_undo: boolean;
  can_redo: boolean;
}

export type StockIntegrityStatus =
  | 'ExactStockMatch'
  | 'FamilyMatchModifiedOrUnknown'
  | 'Unrecognized';

export interface StockRomEntry {
  id: string;
  label: string;
  manufacturer: string;
  ecu_family: string;
  sw_version: string;
  size_bytes: number;
  sha256: string | null;
  identity_tokens: string[];
}

export interface AnchoredMapDiscovery {
  name: string;
  category: string;
  address: number;
  rows: number;
  cols: number;
  cell_bits: number;
  little_endian: boolean;
  unit: string;
  match_count: number;
}

export interface StockRomMatchReport {
  sha256: string;
  rom_size: number;
  pack_name: string;
  integrity_status: StockIntegrityStatus;
  matched_entry: StockRomEntry | null;
  identity_tokens: string[];
  fingerprint: EcuFingerprint;
  anchored_maps: AnchoredMapDiscovery[];
}

export interface OstrichVersion {
  hardware_type: number;
  firmware_major: number;
  firmware_minor: number;
  hardware_id_char: string;
  display: string;
}

export interface OstrichTransferSummary {
  bank: number;
  start_addr: number;
  bytes_written: number;
  chunks: number;
}

export interface OstrichVerifyReport {
  bank: number;
  start_addr: number;
  bytes_checked: number;
  verified: boolean;
  first_mismatch_offset: number | null;
}

export async function bridgeSampleCustomChecksumPlugin(): Promise<string | null> {
  if (!isDesktopApp()) return null;
  return await invoke<string>('sample_custom_checksum_plugin_cmd');
}

export async function bridgeVerifyCustomChecksumPlugin(
  pluginJson: string,
  data?: Uint8Array | null
): Promise<CustomChecksumReport | null> {
  if (!isDesktopApp()) return null;
  return await invoke<CustomChecksumReport>('verify_custom_checksum_plugin_cmd', {
    pluginJson,
    data: data ? Array.from(data) : null,
  });
}

export async function bridgeRepairCustomChecksumPlugin(
  pluginJson: string,
  data?: Uint8Array | null,
  applyToLoaded = true
): Promise<CustomChecksumRepairPayload | null> {
  if (!isDesktopApp()) return null;
  return await invoke<CustomChecksumRepairPayload>('repair_custom_checksum_plugin_workspace_cmd', {
    pluginJson,
    applyToLoaded,
    data: data ? Array.from(data) : null,
  });
}

export async function bridgeSampleCalibrationScript(): Promise<string | null> {
  if (!isDesktopApp()) return null;
  return await invoke<string>('sample_calibration_script_cmd');
}

export async function bridgeDryRunCalibrationScript(
  scriptJson: string,
  romData?: Uint8Array | null
): Promise<CalibrationScriptDryRun | null> {
  if (!isDesktopApp()) return null;
  return await invoke<CalibrationScriptDryRun>('dry_run_calibration_script_cmd', {
    scriptJson,
    romData: romData ? Array.from(romData) : null,
  });
}

export async function bridgeApplyCalibrationScriptWorkspace(
  scriptJson: string
): Promise<CalibrationScriptApplyPayload | null> {
  if (!isDesktopApp()) return null;
  return await invoke<CalibrationScriptApplyPayload>('apply_calibration_script_workspace_cmd', {
    scriptJson,
  });
}

export async function bridgeMatchStockRomRegistry(
  customPackJson?: string | null,
  romData?: Uint8Array | null
): Promise<StockRomMatchReport | null> {
  if (!isDesktopApp()) return null;
  return await invoke<StockRomMatchReport>('match_stock_rom_registry_cmd', {
    customPackJson: customPackJson?.trim() ? customPackJson : null,
    romData: romData ? Array.from(romData) : null,
  });
}

export async function bridgeOstrichProbe(
  port: string,
  baudRate?: number
): Promise<OstrichVersion | null> {
  if (!isDesktopApp()) return null;
  return await invoke<OstrichVersion>('ostrich_probe_cmd', {
    port,
    baudRate: baudRate ?? null,
  });
}

export async function bridgeOstrichUploadActiveRom(
  port: string,
  bank: number,
  baseAddr = 0,
  baudRate?: number
): Promise<OstrichTransferSummary | null> {
  if (!isDesktopApp()) return null;
  return await invoke<OstrichTransferSummary>('ostrich_upload_active_rom_cmd', {
    port,
    bank,
    baseAddr,
    baudRate: baudRate ?? null,
  });
}

export async function bridgeOstrichVerifyActiveRom(
  port: string,
  bank: number,
  baseAddr = 0,
  baudRate?: number
): Promise<OstrichVerifyReport | null> {
  if (!isDesktopApp()) return null;
  return await invoke<OstrichVerifyReport>('ostrich_verify_active_rom_cmd', {
    port,
    bank,
    baseAddr,
    baudRate: baudRate ?? null,
  });
}
