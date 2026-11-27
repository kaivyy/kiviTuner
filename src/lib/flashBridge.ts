/**
 * Bridge to the hardware ECU dump/flash Tauri commands.
 *
 * Returns `null` (or an explicit desktop-required message) in browser builds
 * instead of substituting any mock transport.
 */

import { Channel, invoke } from '@tauri-apps/api/core';
import { isDesktopApp } from './tauriBridge.ts';

export interface FlashDeviceDescriptor {
  id: string;
  name: string;
  kind: string;
  transport: string;
  isMock: boolean;
}

export interface DumpResultPayload {
  baseAddress: number;
  bytes: number[];
  blocksRead: number;
}

export interface FlashResultPayload {
  verified: boolean;
  bytesWritten: number;
  blocksWritten: number;
}

export interface DiscoveredDriver {
  name: string;
  path: string;
  score: number;
  loadable: boolean;
  via_bridge?: boolean;
  arch_label?: string;
  reason?: string | null;
}

export interface SerialPortRow {
  port_name: string;
  description: string;
  vid: number | null;
  pid: number | null;
  is_openport2: boolean;
  adapter_hint?: string;
}

export interface AutoConnectOutcome {
  connected: boolean;
  transport: TransportChoice | null;
  device_name: string | null;
  target: string | null;
  attempts: string[];
}

export type TransportChoice =
  | 'j2534'
  | 'openport2'
  | 'serial'
  | 'elm_wifi'
  | 'kline_serial'
  | 'doip'
  | 'socketcan';

export type ElmBusProfile =
  | 'auto'
  | 'can_11bit_500k'
  | 'can_29bit_500k'
  | 'can_11bit_250k'
  | 'can_29bit_250k'
  | 'kwp_fast_init'
  | 'kwp_5baud_init'
  | 'iso9141'
  | 'j1850_pwm'
  | 'j1850_vpw'
  | 'kawasaki_kds'
  | 'suzuki_sds'
  | 'honda_kline_experimental';

export interface ElmCapabilityReport {
  adapter_banner: string;
  chip_tier: string;
  active_protocol: string;
  supports_atsh: boolean;
  supports_atcaf0: boolean;
  supports_kline_atsp5: boolean;
  supports_atbi: boolean;
  supports_st_commands: boolean;
  warnings: string[];
}

function normalizeTransportChoice(raw: string | null): TransportChoice | null {
  if (!raw) return null;
  const lowered = raw.toLowerCase();
  if (
    lowered === 'j2534' ||
    lowered === 'openport2' ||
    lowered === 'serial' ||
    lowered === 'elm_wifi' ||
    lowered === 'kline_serial' ||
    lowered === 'doip' ||
    lowered === 'socketcan'
  ) {
    return lowered;
  }
  return null;
}

export async function bridgeListFlashDevices(): Promise<FlashDeviceDescriptor[] | null> {
  if (isDesktopApp()) {
    try {
      const raw = await invoke<Array<Record<string, unknown>>>('list_flash_devices');
      return raw.map((d) => ({
        id: String(d.id ?? ''),
        name: String(d.name ?? ''),
        kind: String(d.kind ?? ''),
        transport: String(d.transport ?? ''),
        isMock: Boolean(d.is_mock ?? d.isMock ?? false),
      }));
    } catch (err) {
      console.warn('Tauri list_flash_devices IPC error:', err);
    }
  }
  return null;
}

export async function bridgeListSerialPorts(): Promise<SerialPortRow[] | null> {
  if (isDesktopApp()) {
    try {
      return await invoke<SerialPortRow[]>('list_serial_ports');
    } catch (err) {
      console.warn('Tauri list_serial_ports IPC error:', err);
    }
  }
  return null;
}

export async function bridgeDumpEcu(
  address: number,
  length: number,
  blockSize: number
): Promise<DumpResultPayload | null> {
  if (isDesktopApp()) {
    try {
      const raw = await invoke<Record<string, unknown>>('dump_ecu', {
        address,
        length,
        blockSize,
      });
      return {
        baseAddress: Number(raw.base_address ?? raw.baseAddress ?? 0),
        bytes: (raw.bytes as number[]) ?? [],
        blocksRead: Number(raw.blocks_read ?? raw.blocksRead ?? 0),
      };
    } catch (err) {
      console.warn('Tauri dump_ecu IPC error:', err);
    }
  }
  return null;
}

export async function bridgeFlashEcu(
  address: number,
  image: Uint8Array,
  blockSize: number
): Promise<FlashResultPayload | null> {
  if (isDesktopApp()) {
    try {
      const raw = await invoke<Record<string, unknown>>('flash_ecu', {
        address,
        image: Array.from(image),
        blockSize,
      });
      return {
        verified: Boolean(raw.verified),
        bytesWritten: Number(raw.bytes_written ?? raw.bytesWritten ?? 0),
        blocksWritten: Number(raw.blocks_written ?? raw.blocksWritten ?? 0),
      };
    } catch (err) {
      console.warn('Tauri flash_ecu IPC error:', err);
    }
  }
  return null;
}

export async function bridgeListJ2534Drivers(): Promise<DiscoveredDriver[] | null> {
  if (isDesktopApp()) {
    try {
      return await invoke<DiscoveredDriver[]>('list_j2534_drivers');
    } catch (err) {
      console.warn('Tauri list_j2534_drivers IPC error:', err);
    }
  }
  return null;
}

export async function bridgeAttachJ2534Transport(path: string): Promise<string | null> {
  if (isDesktopApp()) {
    try {
      await invoke<void>('attach_j2534_transport', { path });
      return null;
    } catch (err) {
      return String(err);
    }
  }
  return 'No desktop backend available.';
}

export async function bridgeAttachOpenPort2Transport(port: string): Promise<string | null> {
  if (isDesktopApp()) {
    try {
      await invoke<void>('attach_openport2_transport', { port });
      return null;
    } catch (err) {
      return String(err);
    }
  }
  return 'No desktop backend available.';
}

export async function bridgeAttachSerialTransport(
  port: string,
  baudRate?: number,
  busProfile?: ElmBusProfile
): Promise<string | null> {
  if (isDesktopApp()) {
    try {
      await invoke<number>('attach_serial_transport', {
        port,
        baudRate: baudRate ?? null,
        busProfile: busProfile ?? null,
      });
      return null;
    } catch (err) {
      return String(err);
    }
  }
  return 'No desktop backend available.';
}

export async function bridgeAttachElmWifiTransport(
  hostPort: string,
  busProfile?: ElmBusProfile
): Promise<string | null> {
  if (isDesktopApp()) {
    try {
      await invoke<void>('attach_elm_wifi_transport', {
        hostPort,
        busProfile: busProfile ?? null,
      });
      return null;
    } catch (err) {
      return String(err);
    }
  }
  return 'No desktop backend available.';
}

export async function bridgeAttachKLineSerialTransport(
  port: string,
  baudRate?: number
): Promise<string | null> {
  if (isDesktopApp()) {
    try {
      await invoke<void>('attach_kline_serial_transport', {
        port,
        baudRate: baudRate ?? null,
      });
      return null;
    } catch (err) {
      return String(err);
    }
  }
  return 'No desktop backend available.';
}

export async function bridgeReconfigureTransportProtocol(
  protocol: string,
  baudRate: number,
  txCanId?: number,
  rxCanId?: number,
  klineNoChecksum?: boolean
): Promise<string | null> {
  if (isDesktopApp()) {
    try {
      await invoke<void>('reconfigure_transport_protocol', {
        protocol,
        baudRate,
        txCanId: txCanId ?? null,
        rxCanId: rxCanId ?? null,
        klineNoChecksum: klineNoChecksum ?? null,
      });
      return null;
    } catch (err) {
      return String(err);
    }
  }
  return 'No desktop backend available.';
}

export async function bridgeAttachDoIpTransport(
  hostPort: string,
  sourceAddr?: number,
  targetAddr?: number
): Promise<string | null> {
  if (isDesktopApp()) {
    try {
      await invoke<void>('attach_doip_transport', {
        hostPort,
        sourceAddr: sourceAddr ?? null,
        targetAddr: targetAddr ?? null,
      });
      return null;
    } catch (err) {
      return String(err);
    }
  }
  return 'No desktop backend available.';
}

export async function bridgeAttachSocketCanTransport(iface: string): Promise<string | null> {
  if (isDesktopApp()) {
    try {
      await invoke<void>('attach_socketcan_transport', { interface: iface });
      return null;
    } catch (err) {
      return String(err);
    }
  }
  return 'No desktop backend available.';
}

export async function bridgeDetachTransport(): Promise<string | null> {
  if (isDesktopApp()) {
    try {
      await invoke<void>('detach_transport');
      return null;
    } catch (err) {
      return String(err);
    }
  }
  return 'No desktop backend available.';
}

export async function bridgeSelectedTransport(): Promise<TransportChoice | null> {
  if (isDesktopApp()) {
    try {
      const raw = await invoke<string | null>('selected_transport');
      return normalizeTransportChoice(raw);
    } catch (err) {
      console.warn('Tauri selected_transport IPC error:', err);
    }
  }
  return null;
}

export async function bridgeAutoConnectFlashTransport(): Promise<AutoConnectOutcome | null> {
  if (isDesktopApp()) {
    try {
      const raw = await invoke<Record<string, unknown>>('auto_connect_flash_transport');
      return {
        connected: Boolean(raw.connected),
        transport: normalizeTransportChoice(
          typeof raw.transport === 'string' ? raw.transport : null
        ),
        device_name: typeof raw.device_name === 'string' ? raw.device_name : null,
        target: typeof raw.target === 'string' ? raw.target : null,
        attempts: Array.isArray(raw.attempts) ? raw.attempts.map(String) : [],
      };
    } catch (err) {
      console.warn('Tauri auto_connect_flash_transport IPC error:', err);
    }
  }
  return null;
}

export interface UdsDumpPayload {
  baseAddress: number;
  bytes: number[];
  blocksRead: number;
  pendingRetries: number;
}

export async function bridgeDumpEcuUds23(
  address: number,
  length: number,
  blockSize: number
): Promise<UdsDumpPayload | null> {
  if (isDesktopApp()) {
    try {
      const raw = await invoke<Record<string, unknown>>('dump_ecu_uds23', {
        address,
        length,
        blockSize,
      });
      return {
        baseAddress: Number(raw.base_address ?? raw.baseAddress ?? 0),
        bytes: (raw.bytes as number[]) ?? [],
        blocksRead: Number(raw.blocks_read ?? raw.blocksRead ?? 0),
        pendingRetries: Number(raw.pending_retries ?? raw.pendingRetries ?? 0),
      };
    } catch (err) {
      console.warn('Tauri dump_ecu_uds23 IPC error:', err);
    }
  }
  return null;
}

export type SessionProtocolChoice =
  | {
      kind: 'uds_standard';
      tx_can_id?: number | null;
      seed_key_algo_id?: string | null;
      seed_key_param?: number | null;
      security_level?: number | null;
    }
  | { kind: 'honda_keihin_k_line'; fast_baud?: boolean | null }
  | { kind: 'honda_shindengen_k_line'; fast_baud?: boolean | null }
  | {
      kind: 'ram_kernel';
      arch: string;
      ram_load_address: number;
      kernel_payload: number[];
      tx_can_id?: number | null;
    };

export interface FlashProgressEvent {
  stage: string;
  percent: number;
  bytes_done: number;
  bytes_total: number;
  message: string;
}

export interface DumpStreamPayload {
  base_address: number;
  size: number;
  blocks_read: number;
  hashes: { md5: string; sha1: string; sha256: string } | null;
  loaded_into_workspace: boolean;
  bytes: number[] | null;
}

export interface UdsDumpStreamPayload {
  base_address: number;
  size: number;
  blocks_read: number;
  pending_retries: number;
  hashes: { md5: string; sha1: string; sha256: string } | null;
  loaded_into_workspace: boolean;
  bytes: number[] | null;
}

export async function bridgeConfigureFlashSession(
  protocol: SessionProtocolChoice,
  imageSize?: number | null
): Promise<string | null> {
  if (!isDesktopApp()) return 'No desktop backend available.';
  try {
    await invoke<void>('configure_flash_session', {
      protocol,
      imageSize: imageSize ?? null,
    });
    return null;
  } catch (err) {
    return String(err);
  }
}

export async function bridgeDumpEcuStream(
  address: number,
  length: number,
  blockSize: number,
  onProgress: (event: FlashProgressEvent) => void,
  loadIntoWorkspace = true
): Promise<DumpStreamPayload | null> {
  if (!isDesktopApp()) return null;
  const channel = new Channel<FlashProgressEvent>(onProgress);
  return await invoke<DumpStreamPayload>('dump_ecu_stream', {
    address,
    length,
    blockSize,
    loadIntoWorkspace,
    onProgress: channel,
  });
}

export async function bridgeDumpEcuUds23Stream(
  address: number,
  length: number,
  blockSize: number,
  onProgress: (event: FlashProgressEvent) => void,
  loadIntoWorkspace = true
): Promise<UdsDumpStreamPayload | null> {
  if (!isDesktopApp()) return null;
  const channel = new Channel<FlashProgressEvent>(onProgress);
  return await invoke<UdsDumpStreamPayload>('dump_ecu_uds23_stream', {
    address,
    length,
    blockSize,
    loadIntoWorkspace,
    onProgress: channel,
  });
}

export async function bridgeFlashEcuWorkspaceStream(
  address: number,
  blockSize: number,
  onProgress: (event: FlashProgressEvent) => void
): Promise<FlashResultPayload | null> {
  if (!isDesktopApp()) return null;
  const channel = new Channel<FlashProgressEvent>(onProgress);
  const raw = await invoke<Record<string, unknown>>('flash_ecu_workspace_stream', {
    address,
    blockSize,
    onProgress: channel,
  });
  return {
    verified: Boolean(raw.verified ?? false),
    bytesWritten: Number(raw.bytes_written ?? raw.bytesWritten ?? 0),
    blocksWritten: Number(raw.blocks_written ?? raw.blocksWritten ?? 0),
  };
}

export interface DtcRecord {
  code: string;
  status_mask: number;
  status_label: string;
  source: string;
  description?: string | null;
}

export interface DtcDescription {
  code: string;
  system: string;
  summary: string;
  probable_causes: string[];
  severity: string;
}

export interface ObdLivePidReading {
  pid: number;
  pid_hex: string;
  name: string;
  category: string;
  value: number;
  display: string;
  unit: string;
  raw_hex: string;
}

export interface ObdLiveSnapshot {
  protocol: string;
  tx_can_id: number;
  supported_pids: number[];
  readings: ObdLivePidReading[];
  notes: string[];
}

export interface ImReadinessMonitor {
  name: string;
  supported: boolean;
  ready: boolean;
}

export interface ImReadinessSummary {
  mil_on: boolean;
  dtc_count: number;
  compression_ignition: boolean;
  monitors: ImReadinessMonitor[];
  permanent_dtcs: DtcRecord[];
  notes: string[];
}

export interface FreezeFrameSummary {
  freeze_dtc: string | null;
  freeze_dtc_description: string | null;
  readings: ObdLivePidReading[];
  notes: string[];
}

export interface Mode06TestResult {
  mid: number;
  tid: number;
  uasid: number;
  value: number;
  min: number;
  max: number;
  passed: boolean;
  label: string;
}

export interface DiscoveredEcuModule {
  tx_can_id: number;
  rx_can_id: number;
  tx_hex: string;
  rx_hex: string;
  label: string;
  is_29bit: boolean;
  vin: string | null;
  part_number: string | null;
  sw_version: string | null;
  active_session: number | null;
  dtc_count: number | null;
}

export interface UdsDidValue {
  did: number;
  did_hex: string;
  name: string;
  raw_hex: string;
  ascii: string | null;
}

export interface UdsRoutineOutcome {
  routine_id: number;
  subfunction: number;
  acknowledged: boolean;
  response_hex: string;
  notes: string[];
}

export interface UdsIoControlOutcome {
  did: number;
  control_param: number;
  acknowledged: boolean;
  response_hex: string;
  notes: string[];
}

export type MotoProtocolFamily =
  | 'honda_kline'
  | 'yamaha_kline'
  | 'yamaha_euro5_obd'
  | 'kawasaki_kds'
  | 'suzuki_sds';

export interface MotoSensorMetric {
  key: string;
  label: string;
  value: number;
  display: string;
  unit: string;
}

export interface MotoLiveSnapshot {
  family: MotoProtocolFamily;
  protocol: string;
  ecm_id: string | null;
  matched_profile: Record<string, unknown> | null;
  sensors: MotoSensorMetric[];
  raw_hex: string;
  notes: string[];
}

export interface HondaEcmIdentity {
  ecm_id_hex: string;
  matched_profile: Record<string, unknown> | null;
  raw_hex: string;
  notes: string[];
}

export interface MotoCablePinout {
  id: string;
  brand_models: string;
  connector_type: string;
  protocol: string;
  obd16_wiring: string[];
  recommended_adapter: string;
  notes: string;
}

export interface EcuIdentitySummary {
  battery_mv: number | null;
  vin: string | null;
  cal_id: string | null;
  cvn: string | null;
  protocol: string;
  notes: string[];
}

export interface DtcReadOutcome {
  dtcs: DtcRecord[];
  protocol_used: string;
  notes: string[];
}

export interface DtcClearOutcome {
  cleared: boolean;
  protocol_used: string;
  notes: string[];
}

export type EcuResetMode = 'hard' | 'key_off_on' | 'soft';

export interface EcuResetOutcome {
  acknowledged: boolean;
  mode: EcuResetMode;
  power_down_time_secs: number | null;
  notes: string[];
}

export async function bridgeDiagReadEcuInfo(
  txCanId?: number
): Promise<EcuIdentitySummary | null> {
  if (!isDesktopApp()) return null;
  const raw = await invoke<Record<string, unknown>>('diag_read_ecu_info', {
    txCanId: txCanId ?? null,
  });
  return {
    battery_mv:
      typeof raw.battery_mv === 'number' && Number.isFinite(raw.battery_mv)
        ? raw.battery_mv
        : null,
    vin: typeof raw.vin === 'string' ? raw.vin : null,
    cal_id: typeof raw.cal_id === 'string' ? raw.cal_id : null,
    cvn: typeof raw.cvn === 'string' ? raw.cvn : null,
    protocol: String(raw.protocol ?? ''),
    notes: Array.isArray(raw.notes) ? raw.notes.map(String) : [],
  };
}

export async function bridgeDiagReadDtcs(txCanId?: number): Promise<DtcReadOutcome | null> {
  if (!isDesktopApp()) return null;
  const raw = await invoke<Record<string, unknown>>('diag_read_dtcs', {
    txCanId: txCanId ?? null,
  });
  const rawDtcs = Array.isArray(raw.dtcs) ? raw.dtcs : [];
  return {
    dtcs: rawDtcs.map((item) => {
      const rec = (item ?? {}) as Record<string, unknown>;
      return {
        code: String(rec.code ?? ''),
        status_mask: Number(rec.status_mask ?? 0),
        status_label: String(rec.status_label ?? ''),
        source: String(rec.source ?? ''),
        description: typeof rec.description === 'string' ? rec.description : null,
      };
    }),
    protocol_used: String(raw.protocol_used ?? ''),
    notes: Array.isArray(raw.notes) ? raw.notes.map(String) : [],
  };
}

export async function bridgeDiagClearDtcs(txCanId?: number): Promise<DtcClearOutcome | null> {
  if (!isDesktopApp()) return null;
  const raw = await invoke<Record<string, unknown>>('diag_clear_dtcs', {
    txCanId: txCanId ?? null,
  });
  return {
    cleared: Boolean(raw.cleared ?? false),
    protocol_used: String(raw.protocol_used ?? ''),
    notes: Array.isArray(raw.notes) ? raw.notes.map(String) : [],
  };
}

export async function bridgeDiagEcuReset(
  mode: EcuResetMode,
  txCanId?: number
): Promise<EcuResetOutcome | null> {
  if (!isDesktopApp()) return null;
  const raw = await invoke<Record<string, unknown>>('diag_ecu_reset', {
    mode,
    txCanId: txCanId ?? null,
  });
  return {
    acknowledged: Boolean(raw.acknowledged ?? false),
    mode: (raw.mode as EcuResetMode) ?? mode,
    power_down_time_secs:
      typeof raw.power_down_time_secs === 'number' && Number.isFinite(raw.power_down_time_secs)
        ? raw.power_down_time_secs
        : null,
    notes: Array.isArray(raw.notes) ? raw.notes.map(String) : [],
  };
}

export async function bridgeDiagProbeElmCapabilities(): Promise<ElmCapabilityReport | null> {
  if (!isDesktopApp()) return null;
  return await invoke<ElmCapabilityReport>('diag_probe_elm_capabilities');
}

export async function bridgeDiagSendRawCommand(command: string): Promise<string | null> {
  if (!isDesktopApp()) return null;
  return await invoke<string>('diag_send_raw_command', { command });
}

export async function bridgeDiagLookupDtc(code: string): Promise<DtcDescription | null> {
  if (!isDesktopApp()) return null;
  return await invoke<DtcDescription>('diag_lookup_dtc', { code });
}

export async function bridgeDiagReadLivePids(
  txCanId?: number,
  pids?: number[]
): Promise<ObdLiveSnapshot | null> {
  if (!isDesktopApp()) return null;
  return await invoke<ObdLiveSnapshot>('diag_read_live_pids', {
    txCanId: txCanId ?? null,
    pids: pids ?? null,
  });
}

export async function bridgeDiagReadImReadiness(
  txCanId?: number
): Promise<ImReadinessSummary | null> {
  if (!isDesktopApp()) return null;
  return await invoke<ImReadinessSummary>('diag_read_im_readiness', {
    txCanId: txCanId ?? null,
  });
}

export async function bridgeDiagReadFreezeFrame(
  txCanId?: number
): Promise<FreezeFrameSummary | null> {
  if (!isDesktopApp()) return null;
  return await invoke<FreezeFrameSummary>('diag_read_freeze_frame', {
    txCanId: txCanId ?? null,
  });
}

export async function bridgeDiagReadMode06Monitors(
  txCanId?: number,
  mids?: number[]
): Promise<Mode06TestResult[] | null> {
  if (!isDesktopApp()) return null;
  return await invoke<Mode06TestResult[]>('diag_read_mode06_monitors', {
    txCanId: txCanId ?? null,
    mids: mids ?? null,
  });
}

export async function bridgeDiagScanEcuModules(
  include29bit = false
): Promise<DiscoveredEcuModule[] | null> {
  if (!isDesktopApp()) return null;
  return await invoke<DiscoveredEcuModule[]>('diag_scan_ecu_modules', { include29bit });
}

export async function bridgeDiagUdsEnterSession(
  sessionSubfunction: number,
  txCanId?: number
): Promise<string | null> {
  if (!isDesktopApp()) return null;
  return await invoke<string>('diag_uds_enter_session', {
    txCanId: txCanId ?? null,
    sessionSubfunction,
  });
}

export async function bridgeDiagUdsReadDids(
  dids?: number[],
  txCanId?: number
): Promise<UdsDidValue[] | null> {
  if (!isDesktopApp()) return null;
  return await invoke<UdsDidValue[]>('diag_uds_read_dids', {
    txCanId: txCanId ?? null,
    dids: dids ?? null,
  });
}

export async function bridgeDiagUdsRoutineControl(
  subfunction: number,
  routineId: number,
  optionRecord?: number[],
  txCanId?: number
): Promise<UdsRoutineOutcome | null> {
  if (!isDesktopApp()) return null;
  return await invoke<UdsRoutineOutcome>('diag_uds_routine_control', {
    txCanId: txCanId ?? null,
    subfunction,
    routineId,
    optionRecord: optionRecord ?? null,
  });
}

export async function bridgeDiagUdsIoControl(
  did: number,
  controlParam: number,
  controlState?: number[],
  txCanId?: number
): Promise<UdsIoControlOutcome | null> {
  if (!isDesktopApp()) return null;
  return await invoke<UdsIoControlOutcome>('diag_uds_io_control', {
    txCanId: txCanId ?? null,
    did,
    controlParam,
    controlState: controlState ?? null,
  });
}

export async function bridgeDiagMotoListPinouts(): Promise<MotoCablePinout[] | null> {
  if (!isDesktopApp()) return null;
  return await invoke<MotoCablePinout[]>('diag_moto_list_pinouts');
}

export async function bridgeDiagMotoReadLiveSensors(
  family: MotoProtocolFamily
): Promise<MotoLiveSnapshot | null> {
  if (!isDesktopApp()) return null;
  return await invoke<MotoLiveSnapshot>('diag_moto_read_live_sensors', { family });
}

export async function bridgeDiagMotoReadDtcs(
  family: MotoProtocolFamily
): Promise<DtcReadOutcome | null> {
  if (!isDesktopApp()) return null;
  return await invoke<DtcReadOutcome>('diag_moto_read_dtcs', { family });
}

export async function bridgeDiagMotoClearDtcs(
  family: MotoProtocolFamily
): Promise<DtcClearOutcome | null> {
  if (!isDesktopApp()) return null;
  return await invoke<DtcClearOutcome>('diag_moto_clear_dtcs', { family });
}

export async function bridgeDiagMotoHondaEcmId(): Promise<HondaEcmIdentity | null> {
  if (!isDesktopApp()) return null;
  return await invoke<HondaEcmIdentity>('diag_moto_honda_ecm_id');
}

export type HardwareVerificationTier =
  | 'BenchVerified'
  | 'ReadOnlyVerified'
  | 'ExperimentalUntested';

export interface HardwareTargetCapability {
  id: string;
  name: string;
  category: string;
  tier: HardwareVerificationTier;
  supports_read: boolean;
  supports_write: boolean;
  requires_write_unlock: boolean;
  evidence_note: string;
}

export type BusDirection = 'Tx' | 'Rx' | 'Control' | 'Error';

export interface BusFrameRecord {
  seq: number;
  timestamp_ms: number;
  protocol: string;
  baud_rate: number;
  direction: BusDirection;
  data_hex: string;
  note: string | null;
}

export async function bridgeListHardwareVerificationMatrix(): Promise<
  HardwareTargetCapability[] | null
> {
  if (!isDesktopApp()) return null;
  return await invoke<HardwareTargetCapability[]>('list_hardware_verification_matrix');
}

export async function bridgeGetExperimentalWriteUnlock(): Promise<boolean | null> {
  if (!isDesktopApp()) return null;
  return await invoke<boolean>('get_experimental_write_unlock');
}

export async function bridgeSetExperimentalWriteUnlock(
  enabled: boolean
): Promise<boolean | null> {
  if (!isDesktopApp()) return null;
  return await invoke<boolean>('set_experimental_write_unlock', { enabled });
}

export async function bridgeGetBusFlightLog(
  limit?: number
): Promise<BusFrameRecord[] | null> {
  if (!isDesktopApp()) return null;
  return await invoke<BusFrameRecord[]>('get_bus_flight_log', {
    limit: limit ?? null,
  });
}

export async function bridgeExportBusFlightLogAsc(): Promise<string | null> {
  if (!isDesktopApp()) return null;
  return await invoke<string>('export_bus_flight_log_asc');
}

export async function bridgeExportBusFlightLogJsonl(): Promise<string | null> {
  if (!isDesktopApp()) return null;
  return await invoke<string>('export_bus_flight_log_jsonl');
}

export async function bridgeClearBusFlightLog(): Promise<number | null> {
  if (!isDesktopApp()) return null;
  return await invoke<number>('clear_bus_flight_log');
}
