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

export type TransportChoice = 'j2534' | 'openport2' | 'serial' | 'doip' | 'socketcan';

function normalizeTransportChoice(raw: string | null): TransportChoice | null {
  if (!raw) return null;
  const lowered = raw.toLowerCase();
  if (
    lowered === 'j2534' ||
    lowered === 'openport2' ||
    lowered === 'serial' ||
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
  baudRate?: number
): Promise<string | null> {
  if (isDesktopApp()) {
    try {
      await invoke<void>('attach_serial_transport', { port, baudRate: baudRate ?? null });
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

export async function bridgeDiagReadEcuInfo(): Promise<EcuIdentitySummary | null> {
  if (!isDesktopApp()) return null;
  const raw = await invoke<Record<string, unknown>>('diag_read_ecu_info');
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

export async function bridgeDiagReadDtcs(): Promise<DtcReadOutcome | null> {
  if (!isDesktopApp()) return null;
  const raw = await invoke<Record<string, unknown>>('diag_read_dtcs');
  const rawDtcs = Array.isArray(raw.dtcs) ? raw.dtcs : [];
  return {
    dtcs: rawDtcs.map((item) => {
      const rec = (item ?? {}) as Record<string, unknown>;
      return {
        code: String(rec.code ?? ''),
        status_mask: Number(rec.status_mask ?? 0),
        status_label: String(rec.status_label ?? ''),
        source: String(rec.source ?? ''),
      };
    }),
    protocol_used: String(raw.protocol_used ?? ''),
    notes: Array.isArray(raw.notes) ? raw.notes.map(String) : [],
  };
}

export async function bridgeDiagClearDtcs(): Promise<DtcClearOutcome | null> {
  if (!isDesktopApp()) return null;
  const raw = await invoke<Record<string, unknown>>('diag_clear_dtcs');
  return {
    cleared: Boolean(raw.cleared ?? false),
    protocol_used: String(raw.protocol_used ?? ''),
    notes: Array.isArray(raw.notes) ? raw.notes.map(String) : [],
  };
}

export async function bridgeDiagEcuReset(
  mode: EcuResetMode
): Promise<EcuResetOutcome | null> {
  if (!isDesktopApp()) return null;
  const raw = await invoke<Record<string, unknown>>('diag_ecu_reset', { mode });
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
