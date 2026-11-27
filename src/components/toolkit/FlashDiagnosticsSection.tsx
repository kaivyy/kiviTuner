/**
 * Full OBD-II / UDS / Motorcycle / Raw Terminal diagnostic cockpit
 * for ELM327 (USB, Bluetooth SPP, Wi-Fi), Raw K-Line FTDI/CH340,
 * OpenPort 2.0, SAE J2534, DoIP, and SocketCAN adapters.
 */

import React, { useState } from 'react';
import {
  Activity,
  AlertTriangle,
  BatteryCharging,
  CheckCircle2,
  Cpu,
  Gauge,
  Loader2,
  RefreshCw,
  RotateCcw,
  Search,
  Trash2,
} from 'lucide-react';
import {
  runLookupDtc,
  runReadFreezeFrame,
  runReadImReadiness,
  runReadLivePids,
  runReadMode06,
  runScanEcuModules,
  type DiscoveredEcuModule,
  type DtcClearOutcome,
  type DtcDescription,
  type DtcReadOutcome,
  type EcuIdentitySummary,
  type EcuResetMode,
  type EcuResetOutcome,
  type ElmCapabilityReport,
  type FlashState,
  type FreezeFrameSummary,
  type ImReadinessSummary,
  type Mode06TestResult,
  type ObdLiveSnapshot,
  type TransportChoice,
} from './flashController';
import { UdsServiceSubtab } from './diag/UdsServiceSubtab';
import { MotoDiagnosticsSubtab } from './diag/MotoDiagnosticsSubtab';
import { RawTerminalSubtab } from './diag/RawTerminalSubtab';

export interface FlashDiagnosticsSectionProps {
  isBusy: boolean;
  hasDevice: boolean;
  transport: TransportChoice | null;
  ecuInfoState: FlashState<EcuIdentitySummary> | null;
  dtcReadState: FlashState<DtcReadOutcome> | null;
  dtcClearState: FlashState<DtcClearOutcome> | null;
  ecuResetState: FlashState<EcuResetOutcome> | null;
  elmCapabilities?: FlashState<ElmCapabilityReport> | null;
  onReadEcuInfo: (txCanId?: number) => void;
  onReadDtcs: (txCanId?: number) => void;
  onClearDtcs: (txCanId?: number) => void;
  onEcuReset: (mode: EcuResetMode, txCanId?: number) => void;
  onProbeElmCapabilities?: () => void;
  onLog?: (level: 'info' | 'ok' | 'warn' | 'error', message: string) => void;
}

type DiagSubtab = 'overview' | 'live_pids' | 'uds_service' | 'moto_lab' | 'raw_terminal';

const SUBTABS: Array<{ id: DiagSubtab; label: string }> = [
  { id: 'overview', label: 'Overview & DTCs' },
  { id: 'live_pids', label: 'Live OBD PIDs' },
  { id: 'uds_service', label: 'UDS Service & Active Tests' },
  { id: 'moto_lab', label: 'Motorcycle Lab' },
  { id: 'raw_terminal', label: 'Raw Terminal' },
];

const RESET_MODES: Array<{ value: EcuResetMode; label: string; hint: string }> = [
  { value: 'soft', label: 'Soft Reset (0x03)', hint: 'Recommended after clearing codes or quick restart' },
  { value: 'key_off_on', label: 'Key Off/On (0x02)', hint: 'Simulates ignition key cycle' },
  { value: 'hard', label: 'Hard Reset (0x01)', hint: 'Full ECU cold reboot' },
];

const STANDARD_ECU_TARGETS: Array<{ txId: number; label: string }> = [
  { txId: 0x07e0, label: '0x7E0 ECM / Engine (Default)' },
  { txId: 0x07e1, label: '0x7E1 TCM / Transmission' },
  { txId: 0x07e2, label: '0x7E2 ABS / Stability' },
  { txId: 0x07e3, label: '0x7E3 Hybrid / HV Battery' },
  { txId: 0x07e4, label: '0x7E4 BCM / Chassis' },
  { txId: 0x07e5, label: '0x7E5 IPC / Cluster' },
];

function formatBatteryVolts(mv: number | null): { text: string; toneClass: string } {
  if (mv === null || !Number.isFinite(mv)) {
    return { text: 'Not reported', toneClass: 'text-kiviMuted' };
  }
  const volts = mv / 1000;
  const text = `${volts.toFixed(2)} V`;
  if (mv < 11_800) return { text: `${text} (low)`, toneClass: 'text-kiviRed' };
  if (mv < 12_200) return { text: `${text} (weak)`, toneClass: 'text-kiviWarn' };
  return { text: `${text} (healthy)`, toneClass: 'text-kiviGreen' };
}

function renderBannerError<T>(state: FlashState<T> | null): React.ReactNode {
  if (!state) return null;
  if (state.kind === 'no-backend') {
    return <p className="font-mono text-[11px] text-kiviWarn">{state.message}</p>;
  }
  if (state.kind === 'error') {
    return <p className="font-mono text-[11px] text-kiviRed">{state.message}</p>;
  }
  return null;
}

export const FlashDiagnosticsSection: React.FC<FlashDiagnosticsSectionProps> = ({
  isBusy,
  hasDevice,
  transport,
  ecuInfoState,
  dtcReadState,
  dtcClearState,
  ecuResetState,
  elmCapabilities = null,
  onReadEcuInfo,
  onReadDtcs,
  onClearDtcs,
  onEcuReset,
  onProbeElmCapabilities = () => {},
  onLog = () => {},
}) => {
  const [subtab, setSubtab] = useState<DiagSubtab>('overview');
  const [txCanId, setTxCanId] = useState<number>(0x07e0);
  const [include29bitScan, setInclude29bitScan] = useState(false);

  const [confirmClearOpen, setConfirmClearOpen] = useState(false);
  const [resetMode, setResetMode] = useState<EcuResetMode>('soft');
  const [confirmResetOpen, setConfirmResetOpen] = useState(false);

  const [subWorking, setSubWorking] = useState(false);
  const [modulesState, setModulesState] = useState<FlashState<DiscoveredEcuModule[]> | null>(null);
  const [imReadinessState, setImReadinessState] = useState<FlashState<ImReadinessSummary> | null>(null);
  const [freezeFrameState, setFreezeFrameState] = useState<FlashState<FreezeFrameSummary> | null>(null);
  const [dtcLookupQuery, setDtcLookupQuery] = useState('');
  const [dtcLookupState, setDtcLookupState] = useState<FlashState<DtcDescription> | null>(null);
  const [livePidsState, setLivePidsState] = useState<FlashState<ObdLiveSnapshot> | null>(null);
  const [mode06State, setMode06State] = useState<FlashState<Mode06TestResult[]> | null>(null);

  const busy = isBusy || subWorking;
  const infoData = ecuInfoState?.kind === 'ok' ? ecuInfoState.data : null;
  const dtcData = dtcReadState?.kind === 'ok' ? dtcReadState.data : null;
  const battery = formatBatteryVolts(infoData?.battery_mv ?? null);

  const scanAllEcus = async () => {
    setSubWorking(true);
    try {
      const res = await runScanEcuModules(include29bitScan);
      setModulesState(res);
      if (res.kind === 'ok') {
        onLog('ok', `Multi-ECU scan discovered ${res.data.length} module(s).`);
        if (res.data.length > 0) {
          setTxCanId(res.data[0].tx_can_id);
        }
      } else {
        onLog('warn', res.message);
      }
    } finally {
      setSubWorking(false);
    }
  };

  const loadImAndFreeze = async () => {
    setSubWorking(true);
    try {
      const [im, ff] = await Promise.all([runReadImReadiness(txCanId), runReadFreezeFrame(txCanId)]);
      setImReadinessState(im);
      setFreezeFrameState(ff);
      onLog(
        im.kind === 'ok' ? 'ok' : 'warn',
        im.kind === 'ok'
          ? `Readiness: MIL ${im.data.mil_on ? 'ON' : 'OFF'}, ${im.data.monitors.length} monitor(s), ${im.data.permanent_dtcs.length} permanent DTC(s).`
          : im.message
      );
    } finally {
      setSubWorking(false);
    }
  };

  const lookupDtcCode = async (e: React.FormEvent) => {
    e.preventDefault();
    const res = await runLookupDtc(dtcLookupQuery);
    setDtcLookupState(res);
  };

  const pollLivePids = async () => {
    setSubWorking(true);
    try {
      const [live, m06] = await Promise.all([
        runReadLivePids(txCanId),
        runReadMode06(txCanId),
      ]);
      setLivePidsState(live);
      setMode06State(m06);
      onLog(
        live.kind === 'ok' ? 'ok' : 'warn',
        live.kind === 'ok'
          ? `Live PID scan on 0x${txCanId.toString(16).toUpperCase()}: ${live.data.readings.length} sensor(s).`
          : live.message
      );
    } finally {
      setSubWorking(false);
    }
  };

  return (
    <div className="space-y-3" data-testid="flash-diagnostics-section">
      {/* Adapter Readiness Banner */}
      <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-kiviBorder bg-kiviBg/60 px-3 py-2 font-mono text-[11px]">
        <div className="flex items-center gap-2 text-kiviInk">
          <Activity className="h-3.5 w-3.5 text-kiviAccent" />
          <span>
            {hasDevice
              ? `Ready on ${transport ?? 'hardware'} transport (Target 0x${txCanId.toString(16).toUpperCase()}).`
              : 'No adapter attached yet. Click Auto-Detect Cable above, or connect ELM327 / Raw K-Line.'}
          </span>
        </div>
        {(transport === 'serial' || transport === 'elm_wifi' || transport === 'kline_serial') && (
          <span className="rounded bg-kiviAccent/15 px-2 py-0.5 text-[10px] text-kiviAccent">
            {transport === 'kline_serial' ? 'Raw K-Line Mode' : 'ELM327 / OBD Mode'}
          </span>
        )}
      </div>

      {/* Diagnostic Sub-tabs */}
      <div
        role="tablist"
        aria-label="Diagnostic workspace sub-tabs"
        className="flex flex-wrap gap-1 rounded-lg border border-kiviBorder bg-kiviBg/70 p-1"
      >
        {SUBTABS.map((t) => (
          <button
            key={t.id}
            type="button"
            role="tab"
            aria-selected={subtab === t.id}
            onClick={() => setSubtab(t.id)}
            className={`min-h-[30px] flex-1 whitespace-nowrap rounded px-2 py-1 font-mono text-[10px] font-medium transition-colors focus-visible:ring-2 focus-visible:ring-kiviAccent ${
              subtab === t.id
                ? 'bg-kiviAccent/20 text-kiviAccent'
                : 'text-kiviMuted hover:bg-kiviElevated hover:text-kiviInk'
            }`}
          >
            {t.label}
          </button>
        ))}
      </div>

      {subtab === 'overview' && (
        <div className="space-y-3">
          {/* Multi-ECU Scanner & Target CAN ID */}
          <section className="space-y-2 rounded-lg border border-kiviBorder bg-kiviBg/50 p-3">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div className="flex items-center gap-2">
                <Cpu className="h-3.5 w-3.5 text-kiviAccent" />
                <h3 className="font-mono text-[11px] font-semibold uppercase tracking-wide text-kiviMuted">
                  Target ECU Module &amp; Multi-ECU Scanner
                </h3>
              </div>
              <div className="flex flex-wrap items-center gap-2">
                <label className="flex items-center gap-1 font-mono text-[10px] text-kiviMuted">
                  <input
                    type="checkbox"
                    checked={include29bitScan}
                    onChange={(e) => setInclude29bitScan(e.target.checked)}
                  />
                  <span>Include 29-bit CAN</span>
                </label>
                <button
                  type="button"
                  disabled={busy || !hasDevice}
                  onClick={() => void scanAllEcus()}
                  className="inline-flex min-h-[30px] items-center gap-1.5 rounded border border-kiviAccent/40 bg-kiviAccent/15 px-2.5 py-1 font-mono text-[10px] text-kiviAccent hover:bg-kiviAccent/25 disabled:opacity-50"
                >
                  {subWorking ? <Loader2 className="h-3 w-3 animate-spin" /> : <Search className="h-3 w-3" />}
                  <span>Scan All ECUs (0x7E0-0x7E7)</span>
                </button>
              </div>
            </div>

            <select
              aria-label="Target ECU CAN ID"
              value={txCanId}
              onChange={(e) => setTxCanId(Number(e.target.value))}
              className="w-full rounded border border-kiviBorderStrong bg-kiviSurface px-2 py-1 font-mono text-[11px] text-kiviInk"
            >
              {STANDARD_ECU_TARGETS.map((m) => (
                <option key={m.txId} value={m.txId}>
                  {m.label}
                </option>
              ))}
              {modulesState?.kind === 'ok' &&
                modulesState.data
                  .filter((d) => !STANDARD_ECU_TARGETS.some((s) => s.txId === d.tx_can_id))
                  .map((d) => (
                    <option key={d.tx_hex} value={d.tx_can_id}>
                      {d.tx_hex} {d.label}
                    </option>
                  ))}
            </select>

            {modulesState?.kind === 'ok' && (
              <div className="space-y-1">
                {modulesState.data.length === 0 ? (
                  <p className="font-mono text-[10px] text-kiviWarn">No responding CAN ECU modules found.</p>
                ) : (
                  modulesState.data.map((m) => (
                    <button
                      key={m.tx_hex}
                      type="button"
                      onClick={() => setTxCanId(m.tx_can_id)}
                      className={`flex w-full flex-wrap items-center justify-between gap-2 rounded border px-2 py-1 text-left font-mono text-[10px] ${
                        txCanId === m.tx_can_id
                          ? 'border-kiviAccent bg-kiviAccent/10 text-kiviInk'
                          : 'border-kiviBorder bg-kiviSurface text-kiviSecondary hover:border-kiviAccent/50'
                      }`}
                    >
                      <span>
                        <strong>{m.tx_hex}</strong> ({m.label})
                      </span>
                      <span>
                        {m.part_number ? `P/N ${m.part_number} ` : ''}
                        {m.dtc_count !== null ? `DTCs: ${m.dtc_count}` : ''}
                      </span>
                    </button>
                  ))
                )}
              </div>
            )}
            {renderBannerError(modulesState)}
          </section>

          {/* Card 1: Battery Voltage & ECU Identity */}
          <section className="space-y-2.5 rounded-lg border border-kiviBorder bg-kiviBg/50 p-3">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div className="flex items-center gap-2">
                <BatteryCharging className="h-3.5 w-3.5 text-kiviAccent" />
                <h3 className="font-mono text-[11px] font-semibold uppercase tracking-wide text-kiviMuted">
                  Battery &amp; ECU Identity
                </h3>
              </div>
              <button
                type="button"
                disabled={busy}
                onClick={() => onReadEcuInfo(txCanId)}
                className="inline-flex min-h-[32px] items-center gap-1.5 rounded border border-kiviAccent/40 bg-kiviAccent/15 px-2.5 py-1 font-mono text-[11px] text-kiviAccent hover:bg-kiviAccent/25 focus-visible:ring-2 focus-visible:ring-kiviAccent disabled:cursor-not-allowed disabled:opacity-50"
              >
                {busy ? <Loader2 className="h-3 w-3 animate-spin" /> : <RefreshCw className="h-3 w-3" />}
                <span>Scan ECU &amp; Battery</span>
              </button>
            </div>

            <div className="grid grid-cols-[repeat(auto-fit,minmax(185px,1fr))] gap-2">
              <div className="rounded border border-kiviBorder bg-kiviSurface px-2.5 py-1.5 font-mono text-[11px]">
                <div className="text-[10px] text-kiviMuted">Battery Voltage (ATRV)</div>
                <div className={`font-semibold tabular-nums ${battery.toneClass}`}>
                  {infoData ? battery.text : 'Scan to read'}
                </div>
              </div>
              <div className="rounded border border-kiviBorder bg-kiviSurface px-2.5 py-1.5 font-mono text-[11px]">
                <div className="text-[10px] text-kiviMuted">Protocol</div>
                <div className="font-semibold text-kiviInk">
                  {infoData ? infoData.protocol : 'Auto / ISO15765'}
                </div>
              </div>
              <div className="rounded border border-kiviBorder bg-kiviSurface px-2.5 py-1.5 font-mono text-[11px]">
                <div className="text-[10px] text-kiviMuted">VIN (UDS 0xF190 / Mode 09)</div>
                <div className="truncate font-semibold text-kiviInk">
                  {infoData?.vin ?? 'Not scanned'}
                </div>
              </div>
              <div className="rounded border border-kiviBorder bg-kiviSurface px-2.5 py-1.5 font-mono text-[11px]">
                <div className="text-[10px] text-kiviMuted">Calibration ID / CVN</div>
                <div className="truncate font-semibold text-kiviInk">
                  {infoData?.cal_id
                    ? `${infoData.cal_id}${infoData.cvn ? ` (${infoData.cvn})` : ''}`
                    : infoData?.cvn ?? 'Not scanned'}
                </div>
              </div>
            </div>

            {renderBannerError(ecuInfoState)}
            {infoData && infoData.notes.length > 0 && (
              <p className="font-mono text-[10px] text-kiviMuted">{infoData.notes.join(' | ')}</p>
            )}
          </section>

          {/* Card 2: Fault Codes (DTCs) + Offline Lookup */}
          <section className="space-y-2.5 rounded-lg border border-kiviBorder bg-kiviBg/50 p-3">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div className="flex items-center gap-2">
                <AlertTriangle className="h-3.5 w-3.5 text-kiviWarn" />
                <h3 className="font-mono text-[11px] font-semibold uppercase tracking-wide text-kiviMuted">
                  Fault Codes (DTCs)
                </h3>
              </div>
              <div className="flex flex-wrap items-center gap-2">
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => onReadDtcs(txCanId)}
                  className="inline-flex min-h-[32px] items-center gap-1.5 rounded bg-kiviAccent/20 px-2.5 py-1 font-mono text-[11px] text-kiviAccent hover:bg-kiviAccent/30 focus-visible:ring-2 focus-visible:ring-kiviAccent disabled:cursor-not-allowed disabled:opacity-50"
                >
                  <RefreshCw className="h-3 w-3" />
                  <span>Read DTCs</span>
                </button>
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => setConfirmClearOpen((prev) => !prev)}
                  className="inline-flex min-h-[32px] items-center gap-1.5 rounded border border-kiviWarn/40 bg-kiviWarn/10 px-2.5 py-1 font-mono text-[11px] text-kiviWarn hover:bg-kiviWarn/20 focus-visible:ring-2 focus-visible:ring-kiviWarn disabled:cursor-not-allowed disabled:opacity-50"
                >
                  <Trash2 className="h-3 w-3" />
                  <span>Clear / Reset DTCs</span>
                </button>
              </div>
            </div>

            {confirmClearOpen && (
              <div className="flex flex-wrap items-center justify-between gap-2 rounded border border-kiviWarn/40 bg-kiviWarn/10 px-2.5 py-2 font-mono text-[11px] text-kiviInk">
                <span>Clear stored &amp; pending fault codes (UDS 0x14 / OBD Mode 04)?</span>
                <div className="flex items-center gap-2">
                  <button
                    type="button"
                    disabled={busy}
                    onClick={() => {
                      setConfirmClearOpen(false);
                      onClearDtcs(txCanId);
                    }}
                    className="rounded bg-kiviWarn px-2.5 py-1 font-mono text-[11px] font-semibold text-kiviAccentFg hover:opacity-90 focus-visible:ring-2 focus-visible:ring-kiviWarn"
                  >
                    Confirm Clear DTCs
                  </button>
                  <button
                    type="button"
                    onClick={() => setConfirmClearOpen(false)}
                    className="rounded border border-kiviBorder bg-kiviSurface px-2 py-1 font-mono text-[11px] text-kiviMuted hover:text-kiviInk focus-visible:ring-2 focus-visible:ring-kiviAccent"
                  >
                    Cancel
                  </button>
                </div>
              </div>
            )}

            {dtcData && (
              <div className="space-y-1.5">
                <div className="flex items-center justify-between font-mono text-[10px] text-kiviSecondary">
                  <span>Protocol: {dtcData.protocol_used}</span>
                  <span className="tabular-nums">{dtcData.dtcs.length} code(s)</span>
                </div>
                {dtcData.dtcs.length === 0 ? (
                  <div className="flex items-center gap-2 rounded border border-kiviGreen/30 bg-kiviGreen/10 px-2.5 py-2 font-mono text-[11px] text-kiviGreen">
                    <CheckCircle2 className="h-3.5 w-3.5" />
                    <span>No fault codes stored on this ECU.</span>
                  </div>
                ) : (
                  <ul className="space-y-1">
                    {dtcData.dtcs.map((dtc) => (
                      <li
                        key={`${dtc.source}-${dtc.code}`}
                        className="space-y-0.5 rounded border border-kiviBorder bg-kiviSurface px-2.5 py-1.5 font-mono text-[11px]"
                      >
                        <div className="flex flex-wrap items-center justify-between gap-2">
                          <span className="font-semibold text-kiviInk">{dtc.code}</span>
                          <div className="flex items-center gap-2 text-[10px]">
                            <span
                              className={`rounded px-1.5 py-0.5 ${
                                dtc.status_label === 'confirmed'
                                  ? 'bg-kiviRed/15 text-kiviRed'
                                  : 'bg-kiviWarn/15 text-kiviWarn'
                              }`}
                            >
                              {dtc.status_label}
                            </span>
                            <span className="text-kiviMuted">{dtc.source}</span>
                          </div>
                        </div>
                        {dtc.description && (
                          <p className="text-[10px] text-kiviSecondary">{dtc.description}</p>
                        )}
                      </li>
                    ))}
                  </ul>
                )}
              </div>
            )}

            {dtcClearState?.kind === 'ok' && (
              <p className="font-mono text-[11px] text-kiviGreen">
                DTCs cleared via {dtcClearState.data.protocol_used}.
              </p>
            )}
            {renderBannerError(dtcReadState)}
            {renderBannerError(dtcClearState)}

            <form onSubmit={(e) => void lookupDtcCode(e)} className="flex gap-2 border-t border-kiviBorder pt-2">
              <input
                type="text"
                aria-label="Lookup DTC code offline"
                value={dtcLookupQuery}
                onChange={(e) => setDtcLookupQuery(e.target.value)}
                placeholder="Offline DTC dictionary lookup (e.g. P0171, U0100)"
                className="flex-1 rounded border border-kiviBorderStrong bg-kiviSurface px-2 py-1 font-mono text-[10px] text-kiviInk"
              />
              <button
                type="submit"
                className="rounded border border-kiviBorder bg-kiviSurface px-2.5 py-1 font-mono text-[10px] text-kiviInk hover:border-kiviAccent"
              >
                Lookup DTC
              </button>
            </form>
            {dtcLookupState?.kind === 'ok' && (
              <div className="rounded border border-kiviBorder bg-kiviSurface px-2.5 py-1.5 font-mono text-[10px] text-kiviInk">
                <div className="font-semibold">
                  {dtcLookupState.data.code} [{dtcLookupState.data.system}] ({dtcLookupState.data.severity})
                </div>
                <div className="text-kiviSecondary">{dtcLookupState.data.summary}</div>
                <div className="text-kiviMuted">
                  Checks: {dtcLookupState.data.probable_causes.join(' | ')}
                </div>
              </div>
            )}
          </section>

          {/* Card 3: I/M Readiness & Mode 02 Freeze Frame */}
          <section className="space-y-2 rounded-lg border border-kiviBorder bg-kiviBg/50 p-3">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <h3 className="font-mono text-[11px] font-semibold uppercase tracking-wide text-kiviMuted">
                I/M Readiness (01 01), Permanent DTC (0A) &amp; Freeze Frame (Mode 02)
              </h3>
              <button
                type="button"
                disabled={busy || !hasDevice}
                onClick={() => void loadImAndFreeze()}
                className="min-h-[30px] rounded border border-kiviBorderStrong bg-kiviSurface px-2.5 py-1 font-mono text-[10px] text-kiviInk hover:border-kiviAccent disabled:opacity-50"
              >
                Scan Readiness &amp; Freeze Frame
              </button>
            </div>
            {imReadinessState?.kind === 'ok' && (
              <div className="space-y-1.5 font-mono text-[10px]">
                <div className="text-kiviInk">
                  MIL: <strong>{imReadinessState.data.mil_on ? 'ON' : 'OFF'}</strong> | Stored Count:{' '}
                  {imReadinessState.data.dtc_count} | Permanent Mode 0A:{' '}
                  {imReadinessState.data.permanent_dtcs.length}
                </div>
                <div className="flex flex-wrap gap-1.5">
                  {imReadinessState.data.monitors
                    .filter((m) => m.supported)
                    .map((m) => (
                      <span
                        key={m.name}
                        className={`rounded px-1.5 py-0.5 ${
                          m.ready
                            ? 'bg-kiviGreen/15 text-kiviGreen'
                            : 'bg-kiviWarn/15 text-kiviWarn'
                        }`}
                      >
                        {m.name}: {m.ready ? 'Ready' : 'Incomplete'}
                      </span>
                    ))}
                </div>
              </div>
            )}
            {freezeFrameState?.kind === 'ok' && (
              <div className="rounded border border-kiviBorder bg-kiviSurface px-2.5 py-1.5 font-mono text-[10px]">
                <div className="text-kiviInk">
                  Trigger DTC: {freezeFrameState.data.freeze_dtc ?? 'None'}{' '}
                  {freezeFrameState.data.freeze_dtc_description
                    ? `(${freezeFrameState.data.freeze_dtc_description})`
                    : ''}
                </div>
                {freezeFrameState.data.readings.length > 0 && (
                  <div className="mt-1 flex flex-wrap gap-2 text-kiviSecondary">
                    {freezeFrameState.data.readings.map((r) => (
                      <span key={r.pid_hex}>
                        {r.name}: {r.display}
                      </span>
                    ))}
                  </div>
                )}
              </div>
            )}
          </section>

          {/* Card 4: ECU Reset (UDS 0x11) */}
          <section className="space-y-2.5 rounded-lg border border-kiviBorder bg-kiviBg/50 p-3">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div className="flex items-center gap-2">
                <RotateCcw className="h-3.5 w-3.5 text-kiviRed" />
                <h3 className="font-mono text-[11px] font-semibold uppercase tracking-wide text-kiviMuted">
                  ECU Reset (UDS 0x11)
                </h3>
              </div>
              <span className="font-mono text-[10px] text-kiviMuted">Engine off, ignition ON</span>
            </div>

            <div className="flex flex-wrap items-center gap-2">
              <select
                aria-label="ECU reset mode"
                value={resetMode}
                onChange={(e) => setResetMode(e.target.value as EcuResetMode)}
                className="min-h-[32px] flex-1 rounded border border-kiviBorderStrong bg-kiviSurface px-2 py-1 font-mono text-[11px] text-kiviInk focus-visible:ring-2 focus-visible:ring-kiviAccent"
              >
                {RESET_MODES.map((m) => (
                  <option key={m.value} value={m.value}>
                    {m.label} ({m.hint})
                  </option>
                ))}
              </select>
              <button
                type="button"
                disabled={busy}
                onClick={() => setConfirmResetOpen((prev) => !prev)}
                className="inline-flex min-h-[32px] items-center gap-1.5 rounded border border-kiviRed/40 bg-kiviRed/10 px-2.5 py-1 font-mono text-[11px] text-kiviRed hover:bg-kiviRed/20 focus-visible:ring-2 focus-visible:ring-kiviRed disabled:cursor-not-allowed disabled:opacity-50"
              >
                <RotateCcw className="h-3 w-3" />
                <span>Request ECU Reset</span>
              </button>
            </div>

            {confirmResetOpen && (
              <div className="flex flex-wrap items-center justify-between gap-2 rounded border border-kiviRed/40 bg-kiviRed/10 px-2.5 py-2 font-mono text-[11px] text-kiviInk">
                <span>Reboot ECU using UDS 0x11 ({resetMode}) now? Keep key ON.</span>
                <div className="flex items-center gap-2">
                  <button
                    type="button"
                    disabled={busy}
                    onClick={() => {
                      setConfirmResetOpen(false);
                      onEcuReset(resetMode, txCanId);
                    }}
                    className="rounded bg-kiviRed px-2.5 py-1 font-mono text-[11px] font-semibold text-kiviAccentFg hover:opacity-90 focus-visible:ring-2 focus-visible:ring-kiviRed"
                  >
                    Confirm ECU Reset
                  </button>
                  <button
                    type="button"
                    onClick={() => setConfirmResetOpen(false)}
                    className="rounded border border-kiviBorder bg-kiviSurface px-2 py-1 font-mono text-[11px] text-kiviMuted hover:text-kiviInk focus-visible:ring-2 focus-visible:ring-kiviAccent"
                  >
                    Cancel
                  </button>
                </div>
              </div>
            )}

            {ecuResetState?.kind === 'ok' && (
              <p className="font-mono text-[11px] text-kiviGreen">
                ECU reset acknowledged ({ecuResetState.data.mode}).
              </p>
            )}
            {renderBannerError(ecuResetState)}
          </section>
        </div>
      )}

      {subtab === 'live_pids' && (
        <div className="space-y-3" data-testid="live-pids-subtab">
          <section className="space-y-2.5 rounded-lg border border-kiviBorder bg-kiviBg/50 p-3">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div className="flex items-center gap-2">
                <Gauge className="h-3.5 w-3.5 text-kiviAccent" />
                <h3 className="font-mono text-[11px] font-semibold uppercase tracking-wide text-kiviMuted">
                  Live OBD-II Mode 01 PIDs &amp; Mode 06 Monitors
                </h3>
              </div>
              <button
                type="button"
                disabled={busy || !hasDevice}
                onClick={() => void pollLivePids()}
                className="inline-flex min-h-[32px] items-center gap-1.5 rounded bg-kiviAccent/20 px-2.5 py-1 font-mono text-[11px] text-kiviAccent hover:bg-kiviAccent/30 disabled:opacity-50"
              >
                {subWorking ? <Loader2 className="h-3 w-3 animate-spin" /> : <RefreshCw className="h-3 w-3" />}
                <span>Scan &amp; Read Live PIDs</span>
              </button>
            </div>

            {livePidsState?.kind === 'ok' && (
              <div className="space-y-2">
                <div className="font-mono text-[10px] text-kiviSecondary">
                  Supported PID bitmap count: {livePidsState.data.supported_pids.length} | Returned:{' '}
                  {livePidsState.data.readings.length} reading(s)
                </div>
                <div className="grid grid-cols-[repeat(auto-fit,minmax(175px,1fr))] gap-2">
                  {livePidsState.data.readings.map((r) => (
                    <div
                      key={r.pid_hex}
                      className="rounded border border-kiviBorder bg-kiviSurface px-2.5 py-1.5 font-mono text-[11px]"
                    >
                      <div className="flex items-center justify-between text-[10px] text-kiviMuted">
                        <span className="truncate">{r.name}</span>
                        <span>{r.pid_hex}</span>
                      </div>
                      <div className="font-semibold tabular-nums text-kiviInk">{r.display}</div>
                      <div className="text-[9px] text-kiviMuted">
                        {r.category} | raw {r.raw_hex}
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            )}
            {renderBannerError(livePidsState)}

            {mode06State?.kind === 'ok' && mode06State.data.length > 0 && (
              <div className="space-y-1 border-t border-kiviBorder pt-2">
                <h4 className="font-mono text-[10px] font-semibold uppercase text-kiviMuted">
                  Mode 06 On-Board Monitor Results
                </h4>
                {mode06State.data.map((m, idx) => (
                  <div
                    key={`${m.mid}-${m.tid}-${idx}`}
                    className="flex flex-wrap items-center justify-between gap-2 rounded border border-kiviBorder bg-kiviSurface px-2 py-1 font-mono text-[10px]"
                  >
                    <span className="text-kiviInk">
                      MID 0x{m.mid.toString(16).toUpperCase()} ({m.label})
                    </span>
                    <span className={m.passed ? 'text-kiviGreen' : 'text-kiviRed'}>
                      val {m.value} [{m.min}..{m.max}] ({m.passed ? 'PASS' : 'FAIL'})
                    </span>
                  </div>
                ))}
              </div>
            )}
          </section>
        </div>
      )}

      {subtab === 'uds_service' && (
        <UdsServiceSubtab isBusy={busy} hasDevice={hasDevice} txCanId={txCanId} onLog={onLog} />
      )}

      {subtab === 'moto_lab' && (
        <MotoDiagnosticsSubtab isBusy={busy} hasDevice={hasDevice} onLog={onLog} />
      )}

      {subtab === 'raw_terminal' && (
        <RawTerminalSubtab
          isBusy={busy}
          hasDevice={hasDevice}
          elmCapabilities={elmCapabilities}
          onProbeElmCapabilities={onProbeElmCapabilities}
          onLog={onLog}
        />
      )}
    </div>
  );
};
