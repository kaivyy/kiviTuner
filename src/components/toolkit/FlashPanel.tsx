/**
 * ECU dump & flash panel.
 *
 * ## What this is
 *
 * Moves ROM images to and from a connected ECU over J2534, Tactrix OpenPort 2.0,
 * ELM327 serial, DoIP, or SocketCAN. Beginners can pick an ECU or ROM size
 * preset without typing raw hex addresses, while advanced users can expand the
 * manual hex range controls at any time.
 *
 * ## Safety posture
 *
 * - Device risk and hardware warnings are always visible.
 * - Motorcycle ECU presets enforce writable vs protected bootloader ranges.
 * - Flashing requires typing an explicit confirmation word and passing safety checks.
 * - Browser builds have no hardware path; every bridge returns `null` cleanly.
 */

import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  ChevronDown,
  ChevronRight,
  HardDriveDownload,
  HardDriveUpload,
  Loader2,
  Plug,
  ShieldAlert,
  Wrench,
} from 'lucide-react';
import {
  runAttachDoIp,
  runAttachElmWifi,
  runAttachJ2534,
  runAttachKLineSerial,
  runAttachOpenPort2,
  runAttachSerialConfigured,
  runAttachSocketCan,
  runAutoConnectTransport,
  runClearDtcs,
  runDetachTransport,
  runEcuReset,
  runProbeElmCapabilities,
  runReadDtcs,
  runReadEcuInfo,
  runReconfigureTransport,
  runSelectedTransport,
  summariseDtcRead,
  summariseEcuInfo,
  transportCanFlash,
  runDumpEcu,
  runFlashEcu,
  runListFlashDevices,
  runListSerialPorts,
  runDumpUds23,
  runListJ2534Drivers,
  summariseFlashResult,
  summariseUdsDump,
  type DiscoveredDriverView,
  type DtcClearOutcome,
  type DtcReadOutcome,
  type EcuIdentitySummary,
  type EcuResetMode,
  type EcuResetOutcome,
  type ElmBusProfile,
  type ElmCapabilityReport,
  type FlashDeviceView,
  type FlashState,
  type SerialPortView,
  type TransportChoice,
} from './flashController';
import { FlashTransportSection } from './FlashTransportSection';
import { FlashSafetyFlightCard } from './FlashSafetyFlightCard';
import { FlashDiagnosticsSection } from './FlashDiagnosticsSection';
import { FloatingWindow } from '../layout/FloatingWindow';
import { TOOL_WINDOW_SPECS } from '../layout/toolWindowRegistry';
import {
  bridgeConfigureFlashSession,
  bridgeDumpEcuStream,
  bridgeDumpEcuUds23Stream,
  bridgeFlashEcuWorkspaceStream,
  type DumpResultPayload,
  type FlashProgressEvent,
  type FlashResultPayload,
  type SessionProtocolChoice,
  type UdsDumpPayload,
} from '../../lib/flashBridge';
import {
  bridgeCheckMotoWriteBoundaries,
  bridgeListMotoEcuProfiles,
  type MotoEcuProfile,
} from '../../lib/bridge/ecuToolsBridge';
import { isDesktopApp } from '../../lib/tauriBridge';
import { useMapStore } from '../../stores/useMapStore';
import { FlashActivityLog } from './FlashActivityLog';
import {
  appendFlashLog,
  FALLBACK_MOTO_PROFILES,
  formatByteSize,
  formatHexAddr,
  inferRecommendedProtocol,
  parseHexOrDec,
  STANDARD_ROM_PRESETS,
  validateMotoWriteRange,
  type FlashLogEntry,
  type FlashLogLevel,
} from './flashPresets';

export interface FlashPanelProps {
  isOpen: boolean;
  onClose: () => void;
  rom: Uint8Array | null;
  onDumped: (bytes: Uint8Array, baseAddress: number) => void;
  standalone?: boolean;
}

const CONFIRM_WORD = 'FLASH';
const MOTO_PRESET_PREFIX = 'moto:';
type ActiveOperation = 'idle' | 'dump' | 'uds23' | 'flash' | 'diag';
type FlashPanelTab = 'flash' | 'diagnostics';

interface ProgressMilestoneState {
  lastStage: string;
  lastBucket: number;
}

const Field: React.FC<{
  label: string;
  hint?: string;
  value: string;
  onChange: (v: string) => void;
  ariaLabel: string;
}> = ({ label, hint, value, onChange, ariaLabel }) => (
  <label className="flex items-center justify-between gap-2 text-[11px] text-kiviMuted">
    <span className="flex items-center gap-1.5">
      <span>{label}</span>
      {hint && <span className="font-mono text-[10px] text-kiviSecondary">{hint}</span>}
    </span>
    <input
      type="text"
      inputMode="text"
      aria-label={ariaLabel}
      value={value}
      onChange={(e) => onChange(e.target.value)}
      className="w-32 rounded border border-kiviBorder bg-kiviSurface px-1.5 py-0.5 font-mono text-kiviInk"
    />
  </label>
);

function StateLine<T>({
  state,
  render,
}: {
  state: FlashState<T> | null;
  render: (d: T) => React.ReactNode;
}) {
  if (!state) return <p className="text-[11px] text-kiviMuted">No operation run yet.</p>;
  if (state.kind === 'no-backend') return <p className="text-[11px] text-kiviWarn">{state.message}</p>;
  if (state.kind === 'error') return <p className="text-[11px] text-kiviRed">{state.message}</p>;
  return <div className="text-[11px] text-kiviInk">{render(state.data)}</div>;
}

export const FlashPanel: React.FC<FlashPanelProps> = ({
  isOpen,
  onClose,
  rom,
  onDumped,
  standalone = false,
}) => {
  const romLoaded = useMapStore((s) => s.romLoaded);
  const romSize = useMapStore((s) => s.romSize);
  const refreshRomSession = useMapStore((s) => s.refreshRomSession);
  const hasRom = romLoaded || rom !== null;
  const activeRomBytes = rom?.length ?? romSize;

  const [devices, setDevices] = useState<FlashDeviceView[] | null>(null);
  const [deviceState, setDeviceState] = useState<FlashState<FlashDeviceView[]> | null>(null);

  const [motoProfiles, setMotoProfiles] = useState<MotoEcuProfile[]>(() => [
    ...FALLBACK_MOTO_PROFILES,
  ]);
  const [selectedPresetId, setSelectedPresetId] = useState<string>('std_64k');
  const [advancedRangeOpen, setAdvancedRangeOpen] = useState(false);

  const [startAddr, setStartAddr] = useState('0x0000');
  const [readLen, setReadLen] = useState('0x10000');
  const [blockSize, setBlockSize] = useState('0x100');

  const [drivers, setDrivers] = useState<FlashState<DiscoveredDriverView[]> | null>(null);
  const [serialPorts, setSerialPorts] = useState<SerialPortView[]>([]);
  const [transport, setTransport] = useState<TransportChoice | null>(null);
  const [transportError, setTransportError] = useState<string | null>(null);
  const [serialPort, setSerialPort] = useState('/dev/ttyUSB0');
  const serialPortRef = useRef(serialPort);
  serialPortRef.current = serialPort;
  const busyRef = useRef(false);
  const boundarySeqRef = useRef(0);
  const milestoneRef = useRef<ProgressMilestoneState>({ lastStage: '', lastBucket: -1 });

  const [doipHost, setDoipHost] = useState('192.168.0.10:13400');
  const [canIface, setCanIface] = useState('can0');
  const [elmBaudRate, setElmBaudRate] = useState<number | undefined>(undefined);
  const [elmBusProfile, setElmBusProfile] = useState<ElmBusProfile>('auto');
  const [elmWifiEndpoint, setElmWifiEndpoint] = useState('192.168.0.10:35000');
  const [klineBaudRate, setKlineBaudRate] = useState<number>(10400);
  const [elmCapabilities, setElmCapabilities] = useState<FlashState<ElmCapabilityReport> | null>(
    null
  );
  const [dumpState, setDumpState] = useState<FlashState<DumpResultPayload> | null>(null);
  const [udsDumpState, setUdsDumpState] = useState<FlashState<UdsDumpPayload> | null>(null);
  const [flashState, setFlashState] = useState<FlashState<FlashResultPayload> | null>(null);
  const [confirmText, setConfirmText] = useState('');
  const [protocolKind, setProtocolKind] = useState<SessionProtocolChoice['kind']>('uds_standard');
  const [klineFastBaud, setKlineFastBaud] = useState(true);
  const [sessionStatus, setSessionStatus] = useState<string | null>(null);
  const [streamProgress, setStreamProgress] = useState<FlashProgressEvent | null>(null);
  const [activeOp, setActiveOp] = useState<ActiveOperation>('idle');
  const [panelTab, setPanelTab] = useState<FlashPanelTab>('flash');
  const [autoConnecting, setAutoConnecting] = useState(false);
  const [logs, setLogs] = useState<FlashLogEntry[]>([]);
  const [remoteBoundaryError, setRemoteBoundaryError] = useState<string | null>(null);
  const [ecuInfoState, setEcuInfoState] = useState<FlashState<EcuIdentitySummary> | null>(null);
  const [dtcReadState, setDtcReadState] = useState<FlashState<DtcReadOutcome> | null>(null);
  const [dtcClearState, setDtcClearState] = useState<FlashState<DtcClearOutcome> | null>(null);
  const [ecuResetState, setEcuResetState] = useState<FlashState<EcuResetOutcome> | null>(null);

  const pushLog = useCallback((level: FlashLogLevel, message: string) => {
    setLogs((prev) => appendFlashLog(prev, level, message));
  }, []);

  const recordStreamProgress = useCallback(
    (ev: FlashProgressEvent) => {
      setStreamProgress(ev);
      const bucket = Math.floor(Math.max(0, Math.min(100, ev.percent)) / 25);
      const prev = milestoneRef.current;
      if (ev.stage !== prev.lastStage || bucket > prev.lastBucket || ev.percent >= 100) {
        milestoneRef.current = { lastStage: ev.stage, lastBucket: bucket };
        pushLog('info', `[${ev.stage.toUpperCase()} ${ev.percent}%] ${ev.message}`);
      }
    },
    [pushLog]
  );

  const refreshDevices = useCallback(async () => {
    const [state, driverState, ports, selected, profiles] = await Promise.all([
      runListFlashDevices(),
      runListJ2534Drivers(),
      runListSerialPorts(),
      runSelectedTransport(),
      bridgeListMotoEcuProfiles().catch(() => null),
    ]);
    setDeviceState(state);
    setDevices(state.kind === 'ok' ? state.data : null);
    setDrivers(driverState);
    if (ports.kind === 'ok') {
      setSerialPorts(ports.data);
      if (ports.data.length > 0 && serialPortRef.current === '/dev/ttyUSB0') {
        setSerialPort(ports.data[0].port_name);
      }
    }
    setTransport(selected);
    if (profiles && profiles.length > 0) {
      setMotoProfiles(profiles);
    }
  }, []);

  useEffect(() => {
    if (isOpen) void refreshDevices();
  }, [isOpen, refreshDevices]);

  const parsedStart = useMemo(() => parseHexOrDec(startAddr), [startAddr]);
  const parsedLen = useMemo(() => parseHexOrDec(readLen), [readLen]);
  const parsedBlock = useMemo(() => parseHexOrDec(blockSize), [blockSize]);

  const rangeValid =
    Number.isInteger(parsedStart) &&
    parsedStart >= 0 &&
    parsedStart <= 0xffffffff &&
    Number.isInteger(parsedLen) &&
    parsedLen > 0 &&
    Number.isInteger(parsedBlock) &&
    parsedBlock > 0;

  const selectedMotoProfile = useMemo(() => {
    if (!selectedPresetId.startsWith(MOTO_PRESET_PREFIX)) return null;
    const id = selectedPresetId.slice(MOTO_PRESET_PREFIX.length);
    return motoProfiles.find((p) => p.id === id) ?? null;
  }, [selectedPresetId, motoProfiles]);

  useEffect(() => {
    if (!isOpen || !selectedMotoProfile || !hasRom || activeRomBytes <= 0 || !isDesktopApp()) {
      setRemoteBoundaryError(null);
      return;
    }
    if (!Number.isInteger(parsedStart) || parsedStart < 0) {
      setRemoteBoundaryError(null);
      return;
    }
    const seq = ++boundarySeqRef.current;
    void bridgeCheckMotoWriteBoundaries(selectedMotoProfile.id, parsedStart, activeRomBytes).then(
      (err) => {
        if (boundarySeqRef.current === seq) {
          setRemoteBoundaryError(err);
        }
      }
    );
  }, [isOpen, selectedMotoProfile, hasRom, activeRomBytes, parsedStart]);

  const localBoundary = useMemo(() => {
    if (!selectedMotoProfile || !hasRom || activeRomBytes <= 0) return null;
    if (!Number.isInteger(parsedStart) || parsedStart < 0) {
      return { ok: false, reason: 'Start address must be a valid non-negative integer.' };
    }
    return validateMotoWriteRange(selectedMotoProfile, parsedStart, activeRomBytes);
  }, [selectedMotoProfile, hasRom, activeRomBytes, parsedStart]);

  const writeBoundaryError =
    hasRom && selectedMotoProfile
      ? remoteBoundaryError ?? (localBoundary && !localBoundary.ok ? localBoundary.reason : null)
      : null;

  const handleSelectPreset = useCallback(
    (nextId: string) => {
      setSelectedPresetId(nextId);
      if (nextId === 'custom') {
        setAdvancedRangeOpen(true);
        pushLog('info', 'Switched range mode to Custom manual hex entry.');
        return;
      }
      if (nextId === 'active_rom' && activeRomBytes > 0) {
        const lenHex = formatHexAddr(activeRomBytes, 4);
        setStartAddr('0x0000');
        setReadLen(lenHex);
        setBlockSize('0x100');
        pushLog(
          'info',
          `Preset set to Active Editor ROM (${formatByteSize(activeRomBytes)}, range 0x0000..=${formatHexAddr(activeRomBytes - 1, 4)}).`
        );
        return;
      }
      const std = STANDARD_ROM_PRESETS.find((p) => p.id === nextId);
      if (std) {
        setStartAddr(formatHexAddr(std.startAddress, 4));
        setReadLen(formatHexAddr(std.sizeBytes, 4));
        setBlockSize(formatHexAddr(std.blockSize, 3));
        setProtocolKind(std.protocol);
        pushLog('info', `Preset applied: ${std.label}.`);
        return;
      }
      if (nextId.startsWith(MOTO_PRESET_PREFIX)) {
        const profile = motoProfiles.find((p) => p.id === nextId.slice(MOTO_PRESET_PREFIX.length));
        if (profile) {
          const recommendedProto = inferRecommendedProtocol(profile);
          setStartAddr('0x00000');
          setReadLen(formatHexAddr(profile.rom_size_bytes, 5));
          setBlockSize('0x100');
          setProtocolKind(recommendedProto);
          pushLog(
            'info',
            `ECU preset applied: ${profile.make} ${profile.model} (${formatByteSize(profile.rom_size_bytes)}, safe write ${formatHexAddr(profile.writable_start, 5)}..=${formatHexAddr(profile.writable_end_incl, 5)}, protocol ${recommendedProto}).`
          );
        }
      }
    },
    [activeRomBytes, motoProfiles, pushLog]
  );

  const applyManualField = useCallback(
    (setter: (val: string) => void, value: string) => {
      setter(value);
      if (!selectedPresetId.startsWith(MOTO_PRESET_PREFIX)) {
        setSelectedPresetId('custom');
      }
    },
    [selectedPresetId]
  );

  const attachOpenPort2 = useCallback(
    async (overridePort?: string) => {
      const targetPort = overridePort ?? serialPort;
      if (overridePort) setSerialPort(overridePort);
      const s = await runAttachOpenPort2(targetPort);
      setTransport(s.choice);
      setTransportError(s.error);
      pushLog(
        s.error ? 'error' : 'ok',
        s.error ? `OpenPort 2.0 attach failed: ${s.error}` : `Attached OpenPort 2.0 on ${targetPort}.`
      );
      await refreshDevices();
    },
    [serialPort, refreshDevices, pushLog]
  );

  const attachJ2534 = useCallback(
    async (path: string) => {
      const s = await runAttachJ2534(path);
      setTransport(s.choice);
      setTransportError(s.error);
      pushLog(
        s.error ? 'error' : 'ok',
        s.error ? `J2534 attach failed: ${s.error}` : `Attached J2534 driver: ${path}.`
      );
      await refreshDevices();
    },
    [refreshDevices, pushLog]
  );

  const attachSerial = useCallback(
    async (overridePort?: string) => {
      const targetPort = overridePort ?? serialPort;
      if (overridePort) setSerialPort(overridePort);
      setElmCapabilities(null);
      const s = await runAttachSerialConfigured(targetPort, elmBaudRate, elmBusProfile);
      setTransport(s.choice);
      setTransportError(s.error);
      pushLog(
        s.error ? 'error' : 'ok',
        s.error
          ? `Serial attach failed: ${s.error}`
          : `Attached ELM327 serial adapter on ${targetPort} (profile: ${elmBusProfile}, baud: ${elmBaudRate ?? 'auto'}).`
      );
      if (!s.error && s.choice === 'serial') {
        setPanelTab('diagnostics');
        pushLog('info', 'Switched to Diagnostics (ELM327 / OBD) tab for read-only ELM327 mode.');
      }
      await refreshDevices();
    },
    [serialPort, elmBaudRate, elmBusProfile, refreshDevices, pushLog]
  );

  const attachElmWifi = useCallback(async () => {
    setElmCapabilities(null);
    const s = await runAttachElmWifi(elmWifiEndpoint, elmBusProfile);
    setTransport(s.choice);
    setTransportError(s.error);
    pushLog(
      s.error ? 'error' : 'ok',
      s.error
        ? `ELM327 Wi-Fi attach failed: ${s.error}`
        : `Attached ELM327 Wi-Fi adapter at ${elmWifiEndpoint} (profile: ${elmBusProfile}).`
    );
    if (!s.error && s.choice === 'elm_wifi') {
      setPanelTab('diagnostics');
    }
    await refreshDevices();
  }, [elmWifiEndpoint, elmBusProfile, refreshDevices, pushLog]);

  const attachKLineSerial = useCallback(
    async (overridePort?: string) => {
      const targetPort = overridePort ?? serialPort;
      if (overridePort) setSerialPort(overridePort);
      setElmCapabilities(null);
      const s = await runAttachKLineSerial(targetPort, klineBaudRate);
      setTransport(s.choice);
      setTransportError(s.error);
      pushLog(
        s.error ? 'error' : 'ok',
        s.error
          ? `Raw K-Line attach failed: ${s.error}`
          : `Attached Raw K-Line USB cable on ${targetPort} @ ${klineBaudRate} bps.`
      );
      await refreshDevices();
    },
    [serialPort, klineBaudRate, refreshDevices, pushLog]
  );

  const handleProbeElmCapabilities = useCallback(async () => {
    const res = await runProbeElmCapabilities();
    setElmCapabilities(res);
    if (res.kind === 'ok') {
      pushLog(
        'info',
        `ELM probe: ${res.data.adapter_banner} [${res.data.chip_tier}] (ATSH:${res.data.supports_atsh}, CAF0:${res.data.supports_atcaf0}, KWP5:${res.data.supports_kline_atsp5}).`
      );
    } else {
      pushLog('warn', `ELM probe: ${res.message}`);
    }
  }, [pushLog]);

  const handleReconfigureTransport = useCallback(
    async (
      protocol: string,
      baudRate: number,
      txCanIdHex: string,
      rxCanIdHex: string,
      klineNoChecksum: boolean
    ) => {
      const txId = parseHexOrDec(txCanIdHex);
      const rxId = parseHexOrDec(rxCanIdHex);
      const err = await runReconfigureTransport(
        protocol,
        baudRate,
        txId ?? undefined,
        rxId ?? undefined,
        klineNoChecksum
      );
      setTransportError(err);
      pushLog(
        err ? 'error' : 'ok',
        err
          ? `Protocol override failed: ${err}`
          : `Reconfigured channel to ${protocol} @ ${baudRate} bps.`
      );
      await refreshDevices();
    },
    [pushLog, refreshDevices]
  );

  const handleAutoConnect = useCallback(async () => {
    if (busyRef.current || autoConnecting) return;
    setAutoConnecting(true);
    pushLog(
      'info',
      'Starting auto-detection of OpenPort 2.0, J2534 (.dll/.so/.dylib), SocketCAN & ELM327 cables...'
    );
    try {
      const res = await runAutoConnectTransport();
      if (res.kind === 'no-backend') {
        setTransportError(res.message);
        pushLog('warn', res.message);
        return;
      }
      if (res.kind === 'error') {
        setTransportError(res.message);
        pushLog('error', `Auto-detect failed: ${res.message}`);
        return;
      }
      for (const note of res.data.attempts) {
        pushLog(res.data.connected ? 'info' : 'warn', note);
      }
      setTransport(res.data.transport);
      if (res.data.connected) {
        setTransportError(null);
        if (res.data.target && (res.data.transport === 'openport2' || res.data.transport === 'serial')) {
          setSerialPort(res.data.target);
        }
        pushLog(
          'ok',
          `Auto-connected: ${res.data.device_name ?? res.data.transport ?? 'hardware device'}.`
        );
        if (res.data.transport === 'serial') {
          setPanelTab('diagnostics');
          pushLog('info', 'Detected ELM327 serial adapter; opened Diagnostics (ELM327 / OBD) tab.');
        }
      } else {
        setTransportError(
          res.data.attempts[0] ?? 'No compatible hardware cable or J2534 library found.'
        );
      }
      await refreshDevices();
    } finally {
      setAutoConnecting(false);
    }
  }, [autoConnecting, pushLog, refreshDevices]);

  const attachDoIp = useCallback(async () => {
    const s = await runAttachDoIp(doipHost);
    setTransport(s.choice);
    setTransportError(s.error);
    pushLog(
      s.error ? 'error' : 'ok',
      s.error ? `DoIP attach failed: ${s.error}` : `Attached DoIP gateway ${doipHost}.`
    );
    await refreshDevices();
  }, [doipHost, refreshDevices, pushLog]);

  const attachSocketCan = useCallback(async () => {
    const s = await runAttachSocketCan(canIface);
    setTransport(s.choice);
    setTransportError(s.error);
    pushLog(
      s.error ? 'error' : 'ok',
      s.error ? `SocketCAN attach failed: ${s.error}` : `Attached SocketCAN interface ${canIface}.`
    );
    await refreshDevices();
  }, [canIface, refreshDevices, pushLog]);

  const detachTransport = useCallback(async () => {
    const s = await runDetachTransport();
    setTransport(s.choice);
    setTransportError(s.error);
    setEcuInfoState(null);
    setDtcReadState(null);
    setDtcClearState(null);
    setEcuResetState(null);
    pushLog(s.error ? 'warn' : 'info', s.error ? `Detach error: ${s.error}` : 'Transport detached.');
    await refreshDevices();
  }, [refreshDevices, pushLog]);

  const runDiagnosticAction = useCallback(
    async <T,>(
      startMsg: string,
      action: () => Promise<FlashState<T>>,
      setResult: React.Dispatch<React.SetStateAction<FlashState<T> | null>>,
      summariseOk: (data: T) => string
    ) => {
      if (busyRef.current) return;
      busyRef.current = true;
      setActiveOp('diag');
      pushLog('info', startMsg);
      try {
        const res = await action();
        setResult(res);
        if (res.kind === 'ok') {
          pushLog('ok', summariseOk(res.data));
        } else {
          pushLog(res.kind === 'no-backend' ? 'warn' : 'error', res.message);
        }
      } catch (err) {
        const msg = String(err);
        setResult({ kind: 'error', message: msg });
        pushLog('error', msg);
      } finally {
        busyRef.current = false;
        setActiveOp('idle');
      }
    },
    [pushLog]
  );

  const handleReadEcuInfo = useCallback(
    (txCanId?: number) => {
      void runDiagnosticAction(
        'Scanning battery voltage & ECU identity...',
        () => runReadEcuInfo(undefined, txCanId),
        setEcuInfoState,
        summariseEcuInfo
      );
    },
    [runDiagnosticAction]
  );

  const handleReadDtcs = useCallback(
    (txCanId?: number) => {
      void runDiagnosticAction(
        'Reading fault codes (DTCs) via UDS 0x19 / OBD Mode 03...',
        () => runReadDtcs(undefined, txCanId),
        setDtcReadState,
        summariseDtcRead
      );
    },
    [runDiagnosticAction]
  );

  const handleClearDtcs = useCallback(
    (txCanId?: number) => {
      void runDiagnosticAction(
        'Clearing stored/pending DTCs (UDS 0x14 / OBD Mode 04)...',
        () => runClearDtcs(undefined, txCanId),
        setDtcClearState,
        (res) => `DTCs cleared via ${res.protocol_used}.`
      );
    },
    [runDiagnosticAction]
  );

  const handleEcuReset = useCallback(
    (mode: EcuResetMode, txCanId?: number) => {
      void runDiagnosticAction(
        `Sending UDS 0x11 ECU Reset (${mode})...`,
        () => runEcuReset(mode, undefined, txCanId),
        setEcuResetState,
        (res) => `ECU reset (${res.mode}) acknowledged.`
      );
    },
    [runDiagnosticAction]
  );

  const handleConfigureSession = useCallback(async () => {
    const choice: SessionProtocolChoice =
      protocolKind === 'honda_keihin_k_line'
        ? { kind: 'honda_keihin_k_line', fast_baud: klineFastBaud }
        : protocolKind === 'honda_shindengen_k_line'
          ? { kind: 'honda_shindengen_k_line', fast_baud: klineFastBaud }
          : protocolKind === 'ram_kernel'
            ? { kind: 'ram_kernel', arch: 'sh7058', ram_load_address: 0xffff8000, kernel_payload: [] }
            : { kind: 'uds_standard' };
    const err = await bridgeConfigureFlashSession(choice, activeRomBytes || undefined);
    const statusText = err ? `Session config: ${err}` : `Session configured: ${protocolKind}`;
    setSessionStatus(statusText);
    pushLog(err ? 'warn' : 'ok', statusText);
  }, [protocolKind, klineFastBaud, activeRomBytes, pushLog]);

  const handleDump = useCallback(async () => {
    if (busyRef.current) return;
    busyRef.current = true;
    setActiveOp('dump');
    milestoneRef.current = { lastStage: '', lastBucket: -1 };
    const address = parseHexOrDec(startAddr);
    const length = parseHexOrDec(readLen);
    const block = parseHexOrDec(blockSize);
    pushLog(
      'info',
      `Starting kernel ECU read: ${formatByteSize(length)} @ ${formatHexAddr(address, 5)} (block ${formatByteSize(block)}).`
    );
    setStreamProgress({
      stage: 'reading',
      percent: 5,
      bytes_done: 0,
      bytes_total: Number.isFinite(length) && length > 0 ? length : 0,
      message: `Connecting kernel to read @ ${formatHexAddr(address, 5)}...`,
    });
    try {
      const streamed = await bridgeDumpEcuStream(
        address,
        length,
        block,
        recordStreamProgress,
        true
      );
      if (streamed) {
        if (streamed.loaded_into_workspace) {
          await refreshRomSession();
        } else if (streamed.bytes) {
          onDumped(new Uint8Array(streamed.bytes), streamed.base_address);
        }
        setDumpState({
          kind: 'ok',
          data: {
            baseAddress: streamed.base_address,
            bytes: streamed.bytes ?? new Array(streamed.size).fill(0),
            blocksRead: streamed.blocks_read,
          },
        });
        const hashNote = streamed.hashes ? `, SHA-256 ${streamed.hashes.sha256.slice(0, 12)}...` : '';
        pushLog(
          'ok',
          `Read completed: ${formatByteSize(streamed.size)} in ${streamed.blocks_read} blocks${hashNote}.`
        );
        return;
      }
      const state = await runDumpEcu(undefined, address, length, block);
      setDumpState(state);
      if (state.kind === 'ok') {
        setStreamProgress({
          stage: 'complete',
          percent: 100,
          bytes_done: state.data.bytes.length,
          bytes_total: state.data.bytes.length,
          message: `Dump finished (${state.data.blocksRead} blocks)`,
        });
        pushLog(
          'ok',
          `Read completed: ${formatByteSize(state.data.bytes.length)} in ${state.data.blocksRead} blocks.`
        );
        onDumped(new Uint8Array(state.data.bytes), state.data.baseAddress);
      } else {
        setStreamProgress(null);
        pushLog(state.kind === 'no-backend' ? 'warn' : 'error', state.message);
      }
    } catch (err) {
      setStreamProgress(null);
      const msg = String(err);
      setDumpState({ kind: 'error', message: msg });
      pushLog('error', `ECU read failed: ${msg}`);
    } finally {
      busyRef.current = false;
      setActiveOp('idle');
    }
  }, [startAddr, readLen, blockSize, onDumped, refreshRomSession, pushLog, recordStreamProgress]);

  const handleDumpUds23 = useCallback(async () => {
    if (busyRef.current) return;
    busyRef.current = true;
    setActiveOp('uds23');
    milestoneRef.current = { lastStage: '', lastBucket: -1 };
    const address = parseHexOrDec(startAddr);
    const length = parseHexOrDec(readLen);
    const block = parseHexOrDec(blockSize);
    pushLog(
      'info',
      `Starting UDS 0x23 read: ${formatByteSize(length)} @ ${formatHexAddr(address, 5)} (block ${formatByteSize(block)}).`
    );
    setStreamProgress({
      stage: 'reading',
      percent: 5,
      bytes_done: 0,
      bytes_total: Number.isFinite(length) && length > 0 ? length : 0,
      message: `Requesting UDS 0x23 ReadMemoryByAddress @ ${formatHexAddr(address, 5)}...`,
    });
    try {
      const streamed = await bridgeDumpEcuUds23Stream(
        address,
        length,
        block,
        recordStreamProgress,
        true
      );
      if (streamed) {
        if (streamed.loaded_into_workspace) {
          await refreshRomSession();
        } else if (streamed.bytes) {
          onDumped(new Uint8Array(streamed.bytes), streamed.base_address);
        }
        setUdsDumpState({
          kind: 'ok',
          data: {
            baseAddress: streamed.base_address,
            bytes: streamed.bytes ?? new Array(streamed.size).fill(0),
            blocksRead: streamed.blocks_read,
            pendingRetries: streamed.pending_retries,
          },
        });
        const hashNote = streamed.hashes ? `, SHA-256 ${streamed.hashes.sha256.slice(0, 12)}...` : '';
        pushLog(
          'ok',
          `UDS 0x23 read completed: ${formatByteSize(streamed.size)} in ${streamed.blocks_read} blocks (${streamed.pending_retries} retries)${hashNote}.`
        );
        return;
      }
      const state = await runDumpUds23(undefined, address, length, block);
      setUdsDumpState(state);
      if (state.kind === 'ok') {
        setStreamProgress({
          stage: 'complete',
          percent: 100,
          bytes_done: state.data.bytes.length,
          bytes_total: state.data.bytes.length,
          message: summariseUdsDump(state.data),
        });
        pushLog('ok', summariseUdsDump(state.data));
        onDumped(new Uint8Array(state.data.bytes), state.data.baseAddress);
      } else {
        setStreamProgress(null);
        pushLog(state.kind === 'no-backend' ? 'warn' : 'error', state.message);
      }
    } catch (err) {
      setStreamProgress(null);
      const msg = String(err);
      setUdsDumpState({ kind: 'error', message: msg });
      pushLog('error', `UDS 0x23 read failed: ${msg}`);
    } finally {
      busyRef.current = false;
      setActiveOp('idle');
    }
  }, [startAddr, readLen, blockSize, onDumped, refreshRomSession, pushLog, recordStreamProgress]);

  const handleFlash = useCallback(async () => {
    if (!hasRom || busyRef.current || writeBoundaryError) return;
    busyRef.current = true;
    setActiveOp('flash');
    milestoneRef.current = { lastStage: '', lastBucket: -1 };
    const address = parseHexOrDec(startAddr);
    const block = parseHexOrDec(blockSize);
    pushLog(
      'warn',
      `Starting ECU write: ${formatByteSize(activeRomBytes)} @ ${formatHexAddr(address, 5)} (block ${formatByteSize(block)}). Do not interrupt power.`
    );
    try {
      if (!rom && romLoaded) {
        setStreamProgress({
          stage: 'preparing',
          percent: 5,
          bytes_done: 0,
          bytes_total: romSize,
          message: 'Preparing active workspace ROM for flash...',
        });
        const outcome = await bridgeFlashEcuWorkspaceStream(
          address,
          block,
          recordStreamProgress
        );
        if (outcome) {
          setFlashState({ kind: 'ok', data: outcome });
          setConfirmText('');
          pushLog(outcome.verified ? 'ok' : 'error', summariseFlashResult(outcome));
          return;
        }
      }
      if (!rom) {
        const state = await runFlashEcu(undefined, address, new Uint8Array(0), block);
        setFlashState(state);
        setStreamProgress(null);
        pushLog('error', state.kind === 'error' ? state.message : 'No writable ROM buffer.');
        return;
      }
      const state = await runFlashEcu(undefined, address, rom, block);
      setFlashState(state);
      if (state.kind === 'ok') {
        setConfirmText('');
        setStreamProgress({
          stage: 'complete',
          percent: 100,
          bytes_done: state.data.bytesWritten,
          bytes_total: state.data.bytesWritten,
          message: summariseFlashResult(state.data),
        });
        pushLog(state.data.verified ? 'ok' : 'error', summariseFlashResult(state.data));
      } else {
        setStreamProgress(null);
        pushLog(state.kind === 'no-backend' ? 'warn' : 'error', state.message);
      }
    } catch (err) {
      setStreamProgress(null);
      const msg = String(err);
      setFlashState({ kind: 'error', message: msg });
      pushLog('error', `ECU flash failed: ${msg}`);
    } finally {
      busyRef.current = false;
      setActiveOp('idle');
    }
  }, [
    hasRom,
    writeBoundaryError,
    startAddr,
    blockSize,
    activeRomBytes,
    rom,
    romLoaded,
    romSize,
    pushLog,
    recordStreamProgress,
  ]);

  if (!isOpen) return null;

  const isBusy = activeOp !== 'idle';
  const activeDevice = devices && devices.length > 0 ? devices[0] : null;
  const computedEnd =
    Number.isInteger(parsedStart) && Number.isInteger(parsedLen) && parsedLen > 0
      ? parsedStart + parsedLen - 1
      : null;

  const canFlash =
    !isBusy &&
    transportCanFlash(transport) &&
    activeDevice !== null &&
    hasRom &&
    confirmText.trim() === CONFIRM_WORD &&
    Number.isInteger(parsedStart) &&
    parsedStart >= 0 &&
    Number.isInteger(parsedBlock) &&
    parsedBlock > 0 &&
    writeBoundaryError === null;

  return (
    <FloatingWindow
      windowId="flash"
      isOpen={isOpen}
      onClose={onClose}
      title="ECU Read & Flash"
      titleId="flash-panel-title"
      badge="hardware path"
      icon={<Plug className="h-4 w-4" />}
      defaultWidth={TOOL_WINDOW_SPECS.flash.width}
      defaultHeight={TOOL_WINDOW_SPECS.flash.height}
      minWidth={TOOL_WINDOW_SPECS.flash.minWidth}
      minHeight={TOOL_WINDOW_SPECS.flash.minHeight}
      dock={TOOL_WINDOW_SPECS.flash.dock}
      standalone={standalone}
      popoutToolId="flash"
      closeLabel="Close ECU read and flash panel"
    >
      <div className="flex-1 space-y-4 overflow-y-auto p-4">
          <FlashTransportSection
            isBusy={isBusy}
            autoConnecting={autoConnecting}
            transport={transport}
            transportError={transportError}
            deviceState={deviceState}
            activeDevice={activeDevice}
            drivers={drivers}
            serialPorts={serialPorts}
            serialPort={serialPort}
            onChangeSerialPort={setSerialPort}
            elmBaudRate={elmBaudRate}
            onChangeElmBaudRate={setElmBaudRate}
            elmBusProfile={elmBusProfile}
            onChangeElmBusProfile={setElmBusProfile}
            elmWifiEndpoint={elmWifiEndpoint}
            onChangeElmWifiEndpoint={setElmWifiEndpoint}
            klineBaudRate={klineBaudRate}
            onChangeKlineBaudRate={setKlineBaudRate}
            elmCapabilities={elmCapabilities}
            onProbeElmCapabilities={() => void handleProbeElmCapabilities()}
            doipHost={doipHost}
            onChangeDoipHost={setDoipHost}
            canIface={canIface}
            onChangeCanIface={setCanIface}
            onAutoConnect={() => void handleAutoConnect()}
            onRefresh={() => void refreshDevices()}
            onDetach={() => void detachTransport()}
            onAttachOpenPort2={(port) => void attachOpenPort2(port)}
            onAttachSerial={(port) => void attachSerial(port)}
            onAttachElmWifi={() => void attachElmWifi()}
            onAttachKLineSerial={(port) => void attachKLineSerial(port)}
            onReconfigureTransport={(p, b, tx, rx, noCs) =>
              void handleReconfigureTransport(p, b, tx, rx, noCs)
            }
            onAttachJ2534={(path) => void attachJ2534(path)}
            onAttachDoIp={() => void attachDoIp()}
            onAttachSocketCan={() => void attachSocketCan()}
          />

          <FlashSafetyFlightCard onLog={pushLog} />

          {/* Mode Tabs ---------------------------------------------------- */}
          <div
            role="tablist"
            aria-label="ECU Flash and Diagnostics modes"
            className="flex flex-wrap gap-1.5 rounded-lg border border-kiviBorder bg-kiviBg/70 p-1"
          >
            <button
              type="button"
              role="tab"
              aria-selected={panelTab === 'flash'}
              onClick={() => setPanelTab('flash')}
              className={`inline-flex min-h-[34px] flex-1 items-center justify-center gap-1.5 rounded-md px-3 py-1.5 font-mono text-[11px] font-medium transition-colors focus-visible:ring-2 focus-visible:ring-kiviAccent ${
                panelTab === 'flash'
                  ? 'bg-kiviAccent/20 text-kiviAccent'
                  : 'text-kiviMuted hover:bg-kiviElevated hover:text-kiviInk'
              }`}
            >
              <HardDriveDownload className="h-3.5 w-3.5" />
              <span>Read &amp; Flash ROM</span>
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={panelTab === 'diagnostics'}
              onClick={() => setPanelTab('diagnostics')}
              className={`inline-flex min-h-[34px] flex-1 items-center justify-center gap-1.5 rounded-md px-3 py-1.5 font-mono text-[11px] font-medium transition-colors focus-visible:ring-2 focus-visible:ring-kiviAccent ${
                panelTab === 'diagnostics'
                  ? 'bg-kiviAccent/20 text-kiviAccent'
                  : 'text-kiviMuted hover:bg-kiviElevated hover:text-kiviInk'
              }`}
            >
              <Wrench className="h-3.5 w-3.5" />
              <span>Diagnostics (ELM327 / OBD)</span>
            </button>
          </div>

          {panelTab === 'flash' ? (
            <>
              {/* ECU / ROM Target & Range ------------------------------------ */}
              <section className="space-y-2.5 rounded-lg border border-kiviBorder bg-kiviBg/50 p-3">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div>
                <h3 className="text-[11px] font-semibold uppercase tracking-wide text-kiviMuted">
                  ECU / ROM Target &amp; Range
                </h3>
                <p className="text-[10px] text-kiviMuted">
                  Pick a vehicle or size preset to auto-fill flash memory boundaries safely.
                </p>
              </div>
              <select
                aria-label="ECU or ROM size preset"
                value={selectedPresetId}
                onChange={(e) => handleSelectPreset(e.target.value)}
                className="w-full rounded border border-kiviBorder/70 bg-kiviSurface px-2 py-1 text-[11px] text-kiviInk sm:w-72"
              >
                {hasRom && activeRomBytes > 0 && (
                  <optgroup label="Active Editor ROM">
                    <option value="active_rom">
                      Match Loaded ROM ({formatByteSize(activeRomBytes)})
                    </option>
                  </optgroup>
                )}
                <optgroup label="Standard ROM Sizes">
                  {STANDARD_ROM_PRESETS.map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.label}
                    </option>
                  ))}
                </optgroup>
                <optgroup label="Motorcycle ECU Profiles (Bootloader Guarded)">
                  {motoProfiles.map((m) => (
                    <option key={m.id} value={`${MOTO_PRESET_PREFIX}${m.id}`}>
                      {m.make} {m.model} ({formatByteSize(m.rom_size_bytes)}) [{m.part_number}]
                    </option>
                  ))}
                </optgroup>
                <optgroup label="Manual">
                  <option value="custom">Custom (Manual Hex Range)</option>
                </optgroup>
              </select>
            </div>

            <div className="grid grid-cols-2 gap-2 rounded border border-kiviBorder bg-kiviSurface px-2.5 py-1.5 font-mono text-[10px] sm:grid-cols-4">
              <div>
                <span className="block text-kiviMuted">Start</span>
                <span className="text-kiviInk">
                  {Number.isInteger(parsedStart) && parsedStart >= 0
                    ? formatHexAddr(parsedStart, 5)
                    : 'invalid'}
                </span>
              </div>
              <div>
                <span className="block text-kiviMuted">Length</span>
                <span className="text-kiviInk tabular-nums">
                  {Number.isInteger(parsedLen) && parsedLen > 0
                    ? `${formatByteSize(parsedLen)} (${formatHexAddr(parsedLen, 4)})`
                    : 'invalid'}
                </span>
              </div>
              <div>
                <span className="block text-kiviMuted">End</span>
                <span className="text-kiviInk">
                  {computedEnd !== null ? formatHexAddr(computedEnd, 5) : 'invalid'}
                </span>
              </div>
              <div>
                <span className="block text-kiviMuted">Block</span>
                <span className="text-kiviInk tabular-nums">
                  {Number.isInteger(parsedBlock) && parsedBlock > 0
                    ? `${formatByteSize(parsedBlock)} (${formatHexAddr(parsedBlock, 2)})`
                    : 'invalid'}
                </span>
              </div>
            </div>

            {selectedMotoProfile && (
              <div className="space-y-1.5 rounded border border-kiviAccent/30 bg-kiviAccent/5 p-2.5 text-[11px]">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <div className="flex items-center gap-1.5 font-semibold text-kiviInk">
                    <ShieldAlert className="h-3.5 w-3.5 text-kiviAccent" />
                    <span>
                      {selectedMotoProfile.make} {selectedMotoProfile.model}
                    </span>
                  </div>
                  <span className="font-mono text-[10px] text-kiviMuted">
                    {selectedMotoProfile.mcu} | {selectedMotoProfile.part_number}
                  </span>
                </div>
                <div className="grid grid-cols-1 gap-1 font-mono text-[10px] text-kiviSecondary sm:grid-cols-2">
                  <span>
                    Read full ROM: 0x00000..={formatHexAddr(selectedMotoProfile.rom_size_bytes - 1, 5)}
                  </span>
                  <span className="text-kiviGreen">
                    Safe write window: {formatHexAddr(selectedMotoProfile.writable_start, 5)}..=
                    {formatHexAddr(selectedMotoProfile.writable_end_incl, 5)}
                  </span>
                  <span className="text-kiviWarn sm:col-span-2">
                    Protected bootloader:{' '}
                    {selectedMotoProfile.protected_ranges
                      .map(([s, e]) => `${formatHexAddr(s, 5)}..=${formatHexAddr(e, 5)}`)
                      .join(', ')}
                    {selectedMotoProfile.checksum_offset !== null
                      ? ` | Checksum @ ${formatHexAddr(selectedMotoProfile.checksum_offset, 5)}`
                      : ''}
                  </span>
                </div>
                <div className="flex flex-wrap gap-2 pt-0.5">
                  <button
                    type="button"
                    onClick={() => {
                      setStartAddr('0x00000');
                      setReadLen(formatHexAddr(selectedMotoProfile.rom_size_bytes, 5));
                      pushLog('info', 'Range set to full ROM read (including protected bootloader).');
                    }}
                    className="rounded border border-kiviBorder/70 bg-kiviSurface px-2 py-0.5 text-[10px] text-kiviSecondary hover:bg-kiviElevated"
                  >
                    Use Full Read Range
                  </button>
                  <button
                    type="button"
                    onClick={() => {
                      const safeLen =
                        selectedMotoProfile.writable_end_incl - selectedMotoProfile.writable_start + 1;
                      setStartAddr(formatHexAddr(selectedMotoProfile.writable_start, 5));
                      setReadLen(formatHexAddr(safeLen, 5));
                      pushLog(
                        'info',
                        `Range set to writable window ${formatHexAddr(selectedMotoProfile.writable_start, 5)}..=${formatHexAddr(selectedMotoProfile.writable_end_incl, 5)} (${formatByteSize(safeLen)}).`
                      );
                    }}
                    className="rounded border border-kiviAccent/40 bg-kiviSurface px-2 py-0.5 text-[10px] text-kiviAccent hover:bg-kiviAccent/10"
                  >
                    Use Safe Write Window
                  </button>
                </div>
              </div>
            )}

            <div>
              <button
                type="button"
                aria-expanded={advancedRangeOpen || !rangeValid}
                onClick={() => setAdvancedRangeOpen((v) => !v)}
                className="flex items-center gap-1 text-[11px] font-medium text-kiviSecondary hover:text-kiviInk"
              >
                {advancedRangeOpen || !rangeValid ? (
                  <ChevronDown className="h-3.5 w-3.5" />
                ) : (
                  <ChevronRight className="h-3.5 w-3.5" />
                )}
                <span>Advanced: Manual Hex Range</span>
              </button>

              {(advancedRangeOpen || !rangeValid) && (
                <div className="mt-2 space-y-1.5 rounded border border-kiviBorder bg-kiviSurface p-2.5">
                  <Field
                    label="Start address"
                    hint="hex 0x... or dec"
                    value={startAddr}
                    onChange={(v) => applyManualField(setStartAddr, v)}
                    ariaLabel="Flash start address"
                  />
                  <Field
                    label="Length (bytes)"
                    hint={Number.isInteger(parsedLen) && parsedLen > 0 ? `= ${formatByteSize(parsedLen)}` : undefined}
                    value={readLen}
                    onChange={(v) => applyManualField(setReadLen, v)}
                    ariaLabel="Flash read length"
                  />
                  <Field
                    label="Block size"
                    hint={Number.isInteger(parsedBlock) && parsedBlock > 0 ? `= ${formatByteSize(parsedBlock)}` : undefined}
                    value={blockSize}
                    onChange={(v) => applyManualField(setBlockSize, v)}
                    ariaLabel="Flash block size"
                  />
                  {!rangeValid && (
                    <p className="text-[10px] text-kiviRed">
                      Enter valid non-negative Start address and positive Length / Block size.
                    </p>
                  )}
                </div>
              )}
            </div>
          </section>

          {/* Session Protocol --------------------------------------------- */}
          <section className="space-y-2">
            <h3 className="text-[11px] font-semibold uppercase tracking-wide text-kiviMuted">
              Kernel &amp; Session Protocol
            </h3>
            <div className="flex flex-wrap items-center gap-2 text-[11px]">
              <select
                aria-label="ECU kernel session protocol"
                value={protocolKind}
                onChange={(e) => setProtocolKind(e.target.value as SessionProtocolChoice['kind'])}
                className="rounded border border-kiviBorder/70 bg-kiviBg px-2 py-1 text-kiviInk"
              >
                <option value="uds_standard">ISO 14229 UDS Standard</option>
                <option value="honda_keihin_k_line">Honda Keihin K-Line</option>
                <option value="honda_shindengen_k_line">Honda Shindengen K-Line</option>
                <option value="ram_kernel">RAM Resident Kernel</option>
              </select>
              {(protocolKind === 'honda_keihin_k_line' ||
                protocolKind === 'honda_shindengen_k_line') && (
                <label className="flex items-center gap-1 text-kiviSecondary">
                  <input
                    type="checkbox"
                    checked={klineFastBaud}
                    onChange={(e) => setKlineFastBaud(e.target.checked)}
                  />
                  <span>Fast K-Line Baud</span>
                </label>
              )}
              <button
                type="button"
                disabled={isBusy}
                onClick={() => void handleConfigureSession()}
                className="rounded border border-kiviAccent/40 bg-kiviAccent/15 px-2.5 py-1 text-kiviAccent hover:bg-kiviAccent/25 disabled:cursor-not-allowed disabled:opacity-50"
              >
                Configure Session
              </button>
              {sessionStatus && (
                <span className="font-mono text-[10px] text-kiviSecondary">{sessionStatus}</span>
              )}
            </div>
          </section>

          {/* Dump --------------------------------------------------------- */}
          <section className="space-y-2">
            <h3 className="text-[11px] font-semibold uppercase tracking-wide text-kiviMuted">
              Read from ECU
            </h3>
            <button
              type="button"
              disabled={isBusy || !rangeValid}
              onClick={() => void handleDump()}
              className="inline-flex items-center gap-2 rounded bg-kiviAccent/20 px-3 py-1.5 text-[11px] text-kiviAccent hover:bg-kiviAccent/30 disabled:cursor-not-allowed disabled:opacity-50"
            >
              {activeOp === 'dump' ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <HardDriveDownload className="h-3.5 w-3.5" />
              )}
              <span>{activeOp === 'dump' ? 'Reading ECU...' : 'Read into editor'}</span>
            </button>
            <StateLine
              state={dumpState}
              render={(d) => (
                <span>
                  Read {d.bytes.length} bytes from {formatHexAddr(d.baseAddress, 8)} in {d.blocksRead}{' '}
                  blocks. The kernel was shut down afterwards.
                </span>
              )}
            />

            <div className="mt-2 border-t border-kiviBorder pt-2">
              <button
                type="button"
                disabled={isBusy || !rangeValid}
                onClick={() => void handleDumpUds23()}
                className="inline-flex items-center gap-2 rounded bg-kiviGreen/15 px-3 py-1.5 text-[11px] text-kiviGreen hover:bg-kiviGreen/25 disabled:cursor-not-allowed disabled:opacity-50"
              >
                {activeOp === 'uds23' ? (
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                ) : (
                  <HardDriveDownload className="h-3.5 w-3.5" />
                )}
                <span>
                  {activeOp === 'uds23' ? 'Reading UDS 0x23...' : 'Read via UDS 0x23 (no kernel)'}
                </span>
              </button>
              <p className="mt-1 text-[10px] text-kiviMuted">
                Works with a J2534 interface or an ELM327: no vendor kernel, no dongle. Not every
                ECU answers 0x23; if it refuses, use Read into editor above.
              </p>
              <StateLine
                state={udsDumpState}
                render={(d) => <span className="text-kiviGreen">{summariseUdsDump(d)}</span>}
              />
            </div>
          </section>

          {/* Flash -------------------------------------------------------- */}
          <section className="space-y-2">
            <h3 className="text-[11px] font-semibold uppercase tracking-wide text-kiviMuted">
              Write to ECU
            </h3>
            <p className="text-[11px] text-kiviMuted">
              {hasRom
                ? `Active workspace holds ${formatByteSize(activeRomBytes)} (${activeRomBytes.toLocaleString()} bytes). It will be written from ${Number.isInteger(parsedStart) && parsedStart >= 0 ? formatHexAddr(parsedStart, 5) : 'start address'} and verified.`
                : 'Load a ROM in the editor first. Nothing can be written without an image.'}
            </p>
            {writeBoundaryError && (
              <div className="space-y-1.5 rounded border border-kiviRed/40 bg-kiviRed/10 p-2 text-[11px] text-kiviRed">
                <p>{writeBoundaryError}</p>
                {selectedMotoProfile && parsedStart !== selectedMotoProfile.writable_start && (
                  <button
                    type="button"
                    onClick={() => {
                      setStartAddr(formatHexAddr(selectedMotoProfile.writable_start, 5));
                      pushLog(
                        'info',
                        `Adjusted Start address to ${formatHexAddr(selectedMotoProfile.writable_start, 5)} to avoid bootloader.`
                      );
                    }}
                    className="rounded border border-kiviRed/50 bg-kiviSurface px-2 py-0.5 text-[10px] font-medium text-kiviRed hover:bg-kiviElevated"
                  >
                    Set Start to Safe Write ({formatHexAddr(selectedMotoProfile.writable_start, 5)})
                  </button>
                )}
              </div>
            )}
            <label className="flex items-center justify-between gap-2 text-[11px] text-kiviMuted">
              <span>
                Type <code className="rounded bg-kiviElevated px-1 font-mono">{CONFIRM_WORD}</code> to
                enable
              </span>
              <input
                type="text"
                aria-label="Flash confirmation word"
                value={confirmText}
                onChange={(e) => setConfirmText(e.target.value)}
                className="w-28 rounded border border-kiviBorder bg-kiviSurface px-1.5 py-0.5 font-mono text-kiviInk"
              />
            </label>
            <button
              type="button"
              disabled={!canFlash}
              onClick={() => void handleFlash()}
              className={`inline-flex min-w-[164px] shrink-0 items-center justify-center gap-2 whitespace-nowrap rounded px-3 py-1.5 text-[11px] ${
                canFlash
                  ? 'bg-kiviRed/15 text-kiviRed hover:bg-kiviRed/25'
                  : 'cursor-not-allowed bg-kiviElevated text-kiviMuted'
              }`}
            >
              {activeOp === 'flash' ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <HardDriveUpload className="h-3.5 w-3.5" />
              )}
              <span>{activeOp === 'flash' ? 'Flashing & Verifying...' : 'Write and verify'}</span>
            </button>
            <StateLine
              state={flashState}
              render={(d) => (
                <span className={d.verified ? 'text-kiviGreen' : 'text-kiviRed'}>
                  {summariseFlashResult(d)}
                </span>
              )}
            />
          </section>
            </>
          ) : (
            <FlashDiagnosticsSection
              isBusy={isBusy}
              hasDevice={activeDevice !== null}
              transport={transport}
              ecuInfoState={ecuInfoState}
              dtcReadState={dtcReadState}
              dtcClearState={dtcClearState}
              ecuResetState={ecuResetState}
              elmCapabilities={elmCapabilities}
              onReadEcuInfo={handleReadEcuInfo}
              onReadDtcs={handleReadDtcs}
              onClearDtcs={handleClearDtcs}
              onEcuReset={handleEcuReset}
              onProbeElmCapabilities={() => void handleProbeElmCapabilities()}
              onLog={pushLog}
            />
          )}

          {/* Transfer Progress & Activity Log ----------------------------- */}
          <FlashActivityLog
            isBusy={isBusy}
            activeStage={streamProgress?.stage ?? null}
            progress={streamProgress}
            logs={logs}
            onClearLogs={() => setLogs([])}
          />
        </div>
    </FloatingWindow>
  );
};
