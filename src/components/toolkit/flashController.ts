/**
 * ECU dump/flash controller.
 *
 * Single auditable layer between the UI and hardware flash bridge commands.
 */

import {
  bridgeAttachDoIpTransport,
  bridgeAttachJ2534Transport,
  bridgeAttachOpenPort2Transport,
  bridgeAttachSerialTransport,
  bridgeAttachSocketCanTransport,
  bridgeAutoConnectFlashTransport,
  bridgeDetachTransport,
  bridgeDiagClearDtcs,
  bridgeDiagEcuReset,
  bridgeDiagReadDtcs,
  bridgeDiagReadEcuInfo,
  bridgeDumpEcu,
  bridgeDumpEcuUds23,
  bridgeFlashEcu,
  bridgeListFlashDevices,
  bridgeListJ2534Drivers,
  bridgeListSerialPorts,
  bridgeSelectedTransport,
  type AutoConnectOutcome,
  type DiscoveredDriver,
  type DtcClearOutcome,
  type DtcReadOutcome,
  type DumpResultPayload,
  type EcuIdentitySummary,
  type EcuResetMode,
  type EcuResetOutcome,
  type FlashDeviceDescriptor,
  type FlashResultPayload,
  type SerialPortRow,
  type TransportChoice,
  type UdsDumpPayload,
} from '../../lib/flashBridge.ts';

export type FlashDeviceView = FlashDeviceDescriptor;
export type DiscoveredDriverView = DiscoveredDriver;
export type SerialPortView = SerialPortRow;
export type {
  DtcClearOutcome,
  DtcReadOutcome,
  EcuIdentitySummary,
  EcuResetMode,
  EcuResetOutcome,
  TransportChoice,
};

export type FlashState<T> =
  | { kind: 'ok'; data: T }
  | { kind: 'no-backend'; message: string }
  | { kind: 'error'; message: string };

const NO_BACKEND =
  'This requires the desktop application. The browser build has no flash engine.';

export interface DeviceRisk {
  level: string;
  message: string;
}

export function describeDeviceRisk(device: FlashDeviceView): DeviceRisk {
  if (device.isMock) {
    return {
      level: 'Test double',
      message:
        'Injected non-hardware test double. Nothing is written to real hardware and no ECU can be damaged.',
    };
  }
  return {
    level: 'Real hardware',
    message:
      'This writes to a physical ECU. A wrong image or an interrupted flash can leave the ECU unusable. Verify the ROM and keep the battery on a charger.',
  };
}

export async function runListFlashDevices(
  listFn: () => Promise<FlashDeviceView[] | null> = bridgeListFlashDevices
): Promise<FlashState<FlashDeviceView[]>> {
  let devices: FlashDeviceView[] | null;
  try {
    devices = await listFn();
  } catch (err) {
    return { kind: 'error', message: String(err) };
  }
  if (devices === null) return { kind: 'no-backend', message: NO_BACKEND };
  return { kind: 'ok', data: devices };
}

export async function runListSerialPorts(
  listFn: () => Promise<SerialPortView[] | null> = bridgeListSerialPorts
): Promise<FlashState<SerialPortView[]>> {
  let ports: SerialPortView[] | null;
  try {
    ports = await listFn();
  } catch (err) {
    return { kind: 'error', message: String(err) };
  }
  if (ports === null) return { kind: 'no-backend', message: NO_BACKEND };
  return { kind: 'ok', data: ports };
}

export async function runDumpEcu(
  dumpFn: (
    address: number,
    length: number,
    blockSize: number
  ) => Promise<DumpResultPayload | null> = bridgeDumpEcu,
  address: number,
  length: number,
  blockSize: number
): Promise<FlashState<DumpResultPayload>> {
  if (!Number.isInteger(length) || length <= 0) {
    return { kind: 'error', message: 'The read length must be a positive whole number.' };
  }
  if (!Number.isInteger(blockSize) || blockSize <= 0) {
    return { kind: 'error', message: 'The block size must be a positive whole number.' };
  }
  if (length > 16 * 1024 * 1024) {
    return { kind: 'error', message: 'The read length exceeds the 16 MiB limit.' };
  }
  let result: DumpResultPayload | null;
  try {
    result = await dumpFn(address, length, blockSize);
  } catch (err) {
    return { kind: 'error', message: String(err) };
  }
  if (result === null) return { kind: 'no-backend', message: NO_BACKEND };
  return { kind: 'ok', data: result };
}

export async function runFlashEcu(
  flashFn: (
    address: number,
    image: Uint8Array,
    blockSize: number
  ) => Promise<FlashResultPayload | null> = bridgeFlashEcu,
  address: number,
  image: Uint8Array,
  blockSize: number
): Promise<FlashState<FlashResultPayload>> {
  if (image.length === 0) {
    return { kind: 'error', message: 'The flash image is empty.' };
  }
  if (!Number.isInteger(blockSize) || blockSize <= 0) {
    return { kind: 'error', message: 'The block size must be a positive whole number.' };
  }
  if (image.length > 16 * 1024 * 1024) {
    return { kind: 'error', message: 'The image exceeds the 16 MiB limit.' };
  }
  let result: FlashResultPayload | null;
  try {
    result = await flashFn(address, image, blockSize);
  } catch (err) {
    return { kind: 'error', message: String(err) };
  }
  if (result === null) return { kind: 'no-backend', message: NO_BACKEND };
  return { kind: 'ok', data: result };
}

export function summariseFlashResult(result: FlashResultPayload): string {
  if (!result.verified) {
    return `FAILED verification: wrote ${result.bytesWritten} bytes in ${result.blocksWritten} blocks, but the ECU does not match the image. Do not drive the vehicle.`;
  }
  return `Verified: wrote ${result.bytesWritten} bytes in ${result.blocksWritten} blocks and read them back identically.`;
}

export async function runListJ2534Drivers(
  listFn: () => Promise<DiscoveredDriverView[] | null> = bridgeListJ2534Drivers
): Promise<FlashState<DiscoveredDriverView[]>> {
  let drivers: DiscoveredDriverView[] | null;
  try {
    drivers = await listFn();
  } catch (err) {
    return { kind: 'error', message: String(err) };
  }
  if (drivers === null) return { kind: 'no-backend', message: NO_BACKEND };
  return { kind: 'ok', data: drivers };
}

export function describeDriverUsability(driver: DiscoveredDriverView): string {
  const archPrefix = driver.arch_label ? `[${driver.arch_label}] ` : '';
  if (!driver.loadable) {
    if (driver.reason) {
      return `${archPrefix}${driver.name} cannot be loaded: ${driver.reason}`;
    }
    return `${archPrefix}${driver.name} was found but cannot be loaded on this platform.`;
  }
  if (driver.via_bridge) {
    return `${archPrefix}${driver.name} is loadable via the 32-bit J2534 bridge and ready to open.`;
  }
  return `${archPrefix}${driver.name} is loadable and ready to open.`;
}

export async function runAutoConnectTransport(
  autoFn: () => Promise<AutoConnectOutcome | null> = bridgeAutoConnectFlashTransport
): Promise<FlashState<AutoConnectOutcome>> {
  let outcome: AutoConnectOutcome | null;
  try {
    outcome = await autoFn();
  } catch (err) {
    return { kind: 'error', message: String(err) };
  }
  if (outcome === null) return { kind: 'no-backend', message: NO_BACKEND };
  return { kind: 'ok', data: outcome };
}

export interface TransportStatus {
  choice: TransportChoice | null;
  error: string | null;
}

export async function runAttachJ2534(
  path: string,
  attachFn: (p: string) => Promise<string | null> = bridgeAttachJ2534Transport
): Promise<TransportStatus> {
  if (path.trim() === '') return { choice: null, error: 'A driver path is required.' };
  try {
    const err = await attachFn(path);
    return err !== null ? { choice: null, error: err } : { choice: 'j2534', error: null };
  } catch (e) {
    return { choice: null, error: String(e) };
  }
}

export async function runAttachOpenPort2(
  port: string,
  attachFn: (p: string) => Promise<string | null> = bridgeAttachOpenPort2Transport
): Promise<TransportStatus> {
  if (port.trim() === '') return { choice: null, error: 'An OpenPort 2.0 serial port is required.' };
  try {
    const err = await attachFn(port);
    return err !== null ? { choice: null, error: err } : { choice: 'openport2', error: null };
  } catch (e) {
    return { choice: null, error: String(e) };
  }
}

export async function runAttachSerial(
  port: string,
  attachFn: (p: string) => Promise<string | null> = bridgeAttachSerialTransport
): Promise<TransportStatus> {
  if (port.trim() === '') return { choice: null, error: 'A serial port is required.' };
  try {
    const err = await attachFn(port);
    return err !== null ? { choice: null, error: err } : { choice: 'serial', error: null };
  } catch (e) {
    return { choice: null, error: String(e) };
  }
}

export async function runAttachDoIp(
  hostPort: string,
  attachFn: (hp: string) => Promise<string | null> = bridgeAttachDoIpTransport
): Promise<TransportStatus> {
  if (hostPort.trim() === '') return { choice: null, error: 'A DoIP host:port address is required.' };
  try {
    const err = await attachFn(hostPort);
    return err !== null ? { choice: null, error: err } : { choice: 'doip', error: null };
  } catch (e) {
    return { choice: null, error: String(e) };
  }
}

export async function runAttachSocketCan(
  iface: string,
  attachFn: (i: string) => Promise<string | null> = bridgeAttachSocketCanTransport
): Promise<TransportStatus> {
  if (iface.trim() === '') return { choice: null, error: 'A SocketCAN interface name is required.' };
  try {
    const err = await attachFn(iface);
    return err !== null ? { choice: null, error: err } : { choice: 'socketcan', error: null };
  } catch (e) {
    return { choice: null, error: String(e) };
  }
}

export async function runDetachTransport(
  detachFn: () => Promise<string | null> = bridgeDetachTransport
): Promise<TransportStatus> {
  try {
    const err = await detachFn();
    return { choice: null, error: err };
  } catch (e) {
    return { choice: null, error: String(e) };
  }
}

export async function runSelectedTransport(
  getFn: () => Promise<TransportChoice | null> = bridgeSelectedTransport
): Promise<TransportChoice | null> {
  try {
    return await getFn();
  } catch {
    return null;
  }
}

export function transportCanFlash(choice: TransportChoice | null): boolean {
  return (
    choice === 'j2534' ||
    choice === 'openport2' ||
    choice === 'doip' ||
    choice === 'socketcan'
  );
}

export function describeTransportCapability(choice: TransportChoice | null): string {
  switch (choice) {
    case 'j2534':
      return 'J2534 pass-thru. Can read and flash a real ECU.';
    case 'openport2':
      return 'Tactrix OpenPort 2.0 native USB-CDC. Can read and flash a real ECU.';
    case 'doip':
      return 'ISO 13400 DoIP Ethernet gateway. Can read and flash a real ECU.';
    case 'socketcan':
      return 'Linux SocketCAN interface. Can read and flash over native CAN/ISO-TP.';
    case 'serial':
      return 'Serial OBD adapter. Can read live data and UDS 0x23 memory, but cannot flash.';
    default:
      return 'No transport attached.';
  }
}

export async function runDumpUds23(
  dumpFn: (
    address: number,
    length: number,
    blockSize: number
  ) => Promise<UdsDumpPayload | null> = bridgeDumpEcuUds23,
  address: number,
  length: number,
  blockSize: number
): Promise<FlashState<UdsDumpPayload>> {
  if (!Number.isInteger(length) || length <= 0) {
    return { kind: 'error', message: 'The read length must be a positive whole number.' };
  }
  if (!Number.isInteger(blockSize) || blockSize <= 0) {
    return { kind: 'error', message: 'The block size must be a positive whole number.' };
  }
  if (blockSize > 0xffff) {
    return { kind: 'error', message: 'A UDS 0x23 block cannot exceed 65535 bytes.' };
  }
  let result: UdsDumpPayload | null;
  try {
    result = await dumpFn(address, length, blockSize);
  } catch (err) {
    return { kind: 'error', message: String(err) };
  }
  if (result === null) return { kind: 'no-backend', message: NO_BACKEND };
  return { kind: 'ok', data: result };
}

export function summariseUdsDump(d: UdsDumpPayload): string {
  const pending = d.pendingRetries > 0 ? `, ${d.pendingRetries} pending retries` : '';
  return `Read ${d.bytes.length} bytes from 0x${d.baseAddress
    .toString(16)
    .padStart(8, '0')} in ${d.blocksRead} blocks${pending}.`;
}

async function runDiagCall<T>(fn: () => Promise<T | null>): Promise<FlashState<T>> {
  let result: T | null;
  try {
    result = await fn();
  } catch (err) {
    return { kind: 'error', message: String(err) };
  }
  if (result === null) return { kind: 'no-backend', message: NO_BACKEND };
  return { kind: 'ok', data: result };
}

export function runReadEcuInfo(
  fn: () => Promise<EcuIdentitySummary | null> = bridgeDiagReadEcuInfo
): Promise<FlashState<EcuIdentitySummary>> {
  return runDiagCall(fn);
}

export function runReadDtcs(
  fn: () => Promise<DtcReadOutcome | null> = bridgeDiagReadDtcs
): Promise<FlashState<DtcReadOutcome>> {
  return runDiagCall(fn);
}

export function runClearDtcs(
  fn: () => Promise<DtcClearOutcome | null> = bridgeDiagClearDtcs
): Promise<FlashState<DtcClearOutcome>> {
  return runDiagCall(fn);
}

export function runEcuReset(
  mode: EcuResetMode,
  fn: (m: EcuResetMode) => Promise<EcuResetOutcome | null> = bridgeDiagEcuReset
): Promise<FlashState<EcuResetOutcome>> {
  return runDiagCall(() => fn(mode));
}

export function summariseEcuInfo(info: EcuIdentitySummary): string {
  const volts =
    info.battery_mv !== null ? `${(info.battery_mv / 1000).toFixed(2)}V` : 'voltage n/a';
  const vin = info.vin ? `VIN ${info.vin}` : 'VIN n/a';
  const cal = info.cal_id ? `CAL ${info.cal_id}` : 'CAL n/a';
  return `${volts} | ${vin} | ${cal} (${info.protocol})`;
}

export function summariseDtcRead(outcome: DtcReadOutcome): string {
  if (outcome.dtcs.length === 0) {
    return `0 trouble codes reported (${outcome.protocol_used}).`;
  }
  const codes = outcome.dtcs.map((d) => d.code).join(', ');
  const suffix = outcome.dtcs.length === 1 ? 'trouble code' : 'trouble codes';
  return `${outcome.dtcs.length} ${suffix} via ${outcome.protocol_used}: ${codes}`;
}
