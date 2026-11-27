import React, { useState } from 'react';
import { Cpu, Loader2, Play, PowerOff, RotateCcw, ShieldAlert } from 'lucide-react';
import {
  runUdsEnterSession,
  runUdsIoControl,
  runUdsReadDids,
  runUdsRoutineControl,
  type FlashState,
  type UdsDidValue,
  type UdsIoControlOutcome,
  type UdsRoutineOutcome,
} from '../flashController';

export interface UdsServiceSubtabProps {
  isBusy: boolean;
  hasDevice: boolean;
  txCanId: number;
  onLog: (level: 'info' | 'ok' | 'warn' | 'error', message: string) => void;
}

const ROUTINE_PRESETS = [
  { idHex: '0xFF00', label: '0xFF00 Erase / Memory Routine (Caution)' },
  { idHex: '0x0202', label: '0x0202 EVAP / Service Reset Routine' },
  { idHex: '0xF001', label: '0xF001 Idle / Throttle Adaptation Reset' },
  { idHex: '0xF002', label: '0xF002 Learned Fuel Trim Reset' },
];

function parseHexBytes(raw: string): number[] {
  return raw
    .trim()
    .split(/[\s,]+/)
    .filter(Boolean)
    .map((tok) => Number.parseInt(tok.replace(/^0x/i, ''), 16))
    .filter((n) => Number.isInteger(n) && n >= 0 && n <= 0xff);
}

function parseHexU16(raw: string): number | null {
  const clean = raw.trim().replace(/^0x/i, '');
  if (!clean) return null;
  const parsed = Number.parseInt(clean, 16);
  return Number.isInteger(parsed) && parsed >= 0 && parsed <= 0xffff ? parsed : null;
}

export const UdsServiceSubtab: React.FC<UdsServiceSubtabProps> = ({
  isBusy,
  hasDevice,
  txCanId,
  onLog,
}) => {
  const [working, setWorking] = useState(false);
  const [sessionState, setSessionState] = useState<FlashState<string> | null>(null);
  const [customDidInput, setCustomDidInput] = useState('F186, F190, F187, F189, F191, F197');
  const [didsState, setDidsState] = useState<FlashState<UdsDidValue[]> | null>(null);

  const [routineIdInput, setRoutineIdInput] = useState('0xF001');
  const [routineSubfunc, setRoutineSubfunc] = useState(1);
  const [routineOptInput, setRoutineOptInput] = useState('');
  const [confirmRoutine, setConfirmRoutine] = useState(false);
  const [routineState, setRoutineState] = useState<FlashState<UdsRoutineOutcome> | null>(null);

  const [ioDidInput, setIoDidInput] = useState('0x1234');
  const [ioParam, setIoParam] = useState(3);
  const [ioStateHex, setIoStateHex] = useState('64');
  const [confirmIo, setConfirmIo] = useState(false);
  const [ioResultState, setIoResultState] = useState<FlashState<UdsIoControlOutcome> | null>(null);

  const busy = isBusy || working;

  const switchSession = async (subfunc: number, label: string) => {
    setWorking(true);
    try {
      const res = await runUdsEnterSession(subfunc, txCanId);
      setSessionState(res);
      onLog(
        res.kind === 'ok' ? 'ok' : 'warn',
        res.kind === 'ok'
          ? `UDS 0x10 (${label}) on 0x${txCanId.toString(16).toUpperCase()}: ${res.data}`
          : res.message
      );
    } finally {
      setWorking(false);
    }
  };

  const readDids = async () => {
    setWorking(true);
    try {
      const parsed = customDidInput
        .split(/[\s,]+/)
        .map(parseHexU16)
        .filter((n): n is number => n !== null);
      const res = await runUdsReadDids(parsed.length > 0 ? parsed : undefined, txCanId);
      setDidsState(res);
      onLog(
        res.kind === 'ok' ? 'ok' : 'warn',
        res.kind === 'ok' ? `Read ${res.data.length} UDS DID(s) via 0x22.` : res.message
      );
    } finally {
      setWorking(false);
    }
  };

  const runRoutine = async () => {
    const rid = parseHexU16(routineIdInput);
    if (rid === null) {
      setRoutineState({ kind: 'error', message: 'Enter a valid 16-bit hex Routine ID.' });
      return;
    }
    setWorking(true);
    setConfirmRoutine(false);
    try {
      const opt = parseHexBytes(routineOptInput);
      const res = await runUdsRoutineControl(
        routineSubfunc,
        rid,
        opt.length > 0 ? opt : undefined,
        txCanId
      );
      setRoutineState(res);
      onLog(
        res.kind === 'ok' ? 'ok' : 'error',
        res.kind === 'ok' ? `UDS 0x31 reply: ${res.data.response_hex}` : res.message
      );
    } finally {
      setWorking(false);
    }
  };

  const sendIoControl = async (overrideParam?: number) => {
    const did = parseHexU16(ioDidInput);
    if (did === null) {
      setIoResultState({ kind: 'error', message: 'Enter a valid 16-bit hex IO DID.' });
      return;
    }
    const param = overrideParam ?? ioParam;
    setWorking(true);
    setConfirmIo(false);
    try {
      const bytes = parseHexBytes(ioStateHex);
      const res = await runUdsIoControl(
        did,
        param,
        param === 3 && bytes.length > 0 ? bytes : undefined,
        txCanId
      );
      setIoResultState(res);
      onLog(
        res.kind === 'ok' ? 'ok' : 'error',
        res.kind === 'ok' ? `UDS 0x2F reply: ${res.data.response_hex}` : res.message
      );
    } finally {
      setWorking(false);
    }
  };

  return (
    <div className="space-y-3" data-testid="uds-service-subtab">
      {/* 1. UDS DiagnosticSessionControl (0x10) */}
      <section className="space-y-2 rounded-lg border border-kiviBorder bg-kiviBg/50 p-3">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div className="flex items-center gap-2">
            <Cpu className="h-3.5 w-3.5 text-kiviAccent" />
            <h3 className="font-mono text-[11px] font-semibold uppercase tracking-wide text-kiviMuted">
              UDS Session Control (0x10) on 0x{txCanId.toString(16).toUpperCase()}
            </h3>
          </div>
          {working && <Loader2 className="h-3.5 w-3.5 animate-spin text-kiviAccent" />}
        </div>
        <div className="flex flex-wrap gap-2">
          <button
            type="button"
            disabled={busy || !hasDevice}
            onClick={() => void switchSession(0x01, 'Default 0x01')}
            className="min-h-[32px] rounded border border-kiviBorderStrong bg-kiviSurface px-2.5 py-1 font-mono text-[11px] text-kiviInk hover:border-kiviAccent focus-visible:ring-2 focus-visible:ring-kiviAccent disabled:opacity-50"
          >
            Default Session (0x01)
          </button>
          <button
            type="button"
            disabled={busy || !hasDevice}
            onClick={() => void switchSession(0x03, 'Extended 0x03')}
            className="min-h-[32px] rounded border border-kiviAccent/40 bg-kiviAccent/15 px-2.5 py-1 font-mono text-[11px] text-kiviAccent hover:bg-kiviAccent/25 focus-visible:ring-2 focus-visible:ring-kiviAccent disabled:opacity-50"
          >
            Extended Session (0x03)
          </button>
          <button
            type="button"
            disabled={busy || !hasDevice}
            onClick={() => void switchSession(0x02, 'Programming 0x02')}
            className="min-h-[32px] rounded border border-kiviWarn/40 bg-kiviWarn/10 px-2.5 py-1 font-mono text-[11px] text-kiviWarn hover:bg-kiviWarn/20 focus-visible:ring-2 focus-visible:ring-kiviWarn disabled:opacity-50"
          >
            Programming (0x02)
          </button>
        </div>
        {sessionState?.kind === 'ok' && (
          <p className="font-mono text-[11px] text-kiviGreen">Session active: {sessionState.data}</p>
        )}
        {sessionState && sessionState.kind !== 'ok' && (
          <p className="font-mono text-[11px] text-kiviRed">{sessionState.message}</p>
        )}
      </section>

      {/* 2. UDS ReadDataByIdentifier (0x22) */}
      <section className="space-y-2 rounded-lg border border-kiviBorder bg-kiviBg/50 p-3">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <h3 className="font-mono text-[11px] font-semibold uppercase tracking-wide text-kiviMuted">
            ECU Identification &amp; DID Reader (0x22)
          </h3>
          <button
            type="button"
            disabled={busy || !hasDevice}
            onClick={() => void readDids()}
            className="min-h-[32px] rounded bg-kiviAccent/20 px-2.5 py-1 font-mono text-[11px] text-kiviAccent hover:bg-kiviAccent/30 focus-visible:ring-2 focus-visible:ring-kiviAccent disabled:opacity-50"
          >
            Read DIDs
          </button>
        </div>
        <input
          type="text"
          aria-label="UDS DID hex list"
          value={customDidInput}
          onChange={(e) => setCustomDidInput(e.target.value)}
          placeholder="F186, F190, F187, F189"
          className="w-full rounded border border-kiviBorderStrong bg-kiviSurface px-2 py-1 font-mono text-[11px] text-kiviInk focus-visible:ring-2 focus-visible:ring-kiviAccent"
        />
        {didsState?.kind === 'ok' && (
          <div className="space-y-1">
            {didsState.data.length === 0 ? (
              <p className="font-mono text-[11px] text-kiviMuted">No requested DIDs responded.</p>
            ) : (
              didsState.data.map((d) => (
                <div
                  key={d.did_hex}
                  className="flex flex-wrap items-center justify-between gap-2 rounded border border-kiviBorder bg-kiviSurface px-2.5 py-1.5 font-mono text-[11px]"
                >
                  <span className="text-kiviInk">
                    <strong className="text-kiviAccent">{d.did_hex}</strong> ({d.name})
                  </span>
                  <span className="text-kiviSecondary">
                    {d.ascii ? `"${d.ascii}" ` : ''}[{d.raw_hex}]
                  </span>
                </div>
              ))
            )}
          </div>
        )}
        {didsState && didsState.kind !== 'ok' && (
          <p className="font-mono text-[11px] text-kiviRed">{didsState.message}</p>
        )}
      </section>

      {/* 3. UDS RoutineControl (0x31) */}
      <section className="space-y-2 rounded-lg border border-kiviBorder bg-kiviBg/50 p-3">
        <div className="flex items-center gap-2">
          <RotateCcw className="h-3.5 w-3.5 text-kiviWarn" />
          <h3 className="font-mono text-[11px] font-semibold uppercase tracking-wide text-kiviMuted">
            Service Resets &amp; Adaptations (UDS 0x31)
          </h3>
        </div>
        <div className="grid grid-cols-1 gap-2 sm:grid-cols-3">
          <select
            aria-label="UDS routine preset"
            value={routineIdInput}
            onChange={(e) => setRoutineIdInput(e.target.value)}
            className="rounded border border-kiviBorderStrong bg-kiviSurface px-2 py-1 font-mono text-[11px] text-kiviInk focus-visible:ring-2 focus-visible:ring-kiviAccent"
          >
            {ROUTINE_PRESETS.map((p) => (
              <option key={p.idHex} value={p.idHex}>
                {p.label}
              </option>
            ))}
          </select>
          <input
            type="text"
            aria-label="Custom routine ID hex"
            value={routineIdInput}
            onChange={(e) => setRoutineIdInput(e.target.value)}
            placeholder="0xF001"
            className="rounded border border-kiviBorderStrong bg-kiviSurface px-2 py-1 font-mono text-[11px] text-kiviInk focus-visible:ring-2 focus-visible:ring-kiviAccent"
          />
          <select
            aria-label="Routine control subfunction"
            value={routineSubfunc}
            onChange={(e) => setRoutineSubfunc(Number(e.target.value))}
            className="rounded border border-kiviBorderStrong bg-kiviSurface px-2 py-1 font-mono text-[11px] text-kiviInk focus-visible:ring-2 focus-visible:ring-kiviAccent"
          >
            <option value={1}>0x01 Start Routine</option>
            <option value={2}>0x02 Stop Routine</option>
            <option value={3}>0x03 Request Results</option>
          </select>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <input
            type="text"
            aria-label="Routine option hex bytes"
            value={routineOptInput}
            onChange={(e) => setRoutineOptInput(e.target.value)}
            placeholder="Optional hex record bytes (e.g. 00 01)"
            className="flex-1 rounded border border-kiviBorderStrong bg-kiviSurface px-2 py-1 font-mono text-[11px] text-kiviInk focus-visible:ring-2 focus-visible:ring-kiviAccent"
          />
          <button
            type="button"
            disabled={busy || !hasDevice}
            onClick={() => setConfirmRoutine((prev) => !prev)}
            className="inline-flex min-h-[32px] items-center gap-1.5 rounded border border-kiviWarn/40 bg-kiviWarn/10 px-2.5 py-1 font-mono text-[11px] text-kiviWarn hover:bg-kiviWarn/20 focus-visible:ring-2 focus-visible:ring-kiviWarn disabled:opacity-50"
          >
            <Play className="h-3 w-3" />
            <span>Run Routine 0x31</span>
          </button>
        </div>
        {confirmRoutine && (
          <div className="flex flex-wrap items-center justify-between gap-2 rounded border border-kiviWarn/40 bg-kiviWarn/10 px-2.5 py-2 font-mono text-[11px] text-kiviInk">
            <span className="flex items-center gap-1.5">
              <ShieldAlert className="h-3.5 w-3.5 text-kiviWarn" />
              <span>Execute UDS 0x31 ({routineIdInput}) on engine-off ignition ON?</span>
            </span>
            <div className="flex gap-2">
              <button
                type="button"
                disabled={busy}
                onClick={() => void runRoutine()}
                className="rounded bg-kiviWarn px-2.5 py-1 font-semibold text-kiviAccentFg"
              >
                Confirm Routine
              </button>
              <button
                type="button"
                onClick={() => setConfirmRoutine(false)}
                className="rounded border border-kiviBorder bg-kiviSurface px-2 py-1 text-kiviMuted"
              >
                Cancel
              </button>
            </div>
          </div>
        )}
        {routineState?.kind === 'ok' && (
          <p className="font-mono text-[11px] text-kiviGreen">
            {routineState.data.notes.join(' ')} [{routineState.data.response_hex}]
          </p>
        )}
        {routineState && routineState.kind !== 'ok' && (
          <p className="font-mono text-[11px] text-kiviRed">{routineState.message}</p>
        )}
      </section>

      {/* 4. UDS InputOutputControlByIdentifier (0x2F) */}
      <section className="space-y-2 rounded-lg border border-kiviBorder bg-kiviBg/50 p-3">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <h3 className="font-mono text-[11px] font-semibold uppercase tracking-wide text-kiviMuted">
            Active Actuator Tests (UDS 0x2F IOControl)
          </h3>
          <button
            type="button"
            disabled={busy || !hasDevice}
            onClick={() => void sendIoControl(0)}
            className="inline-flex min-h-[32px] items-center gap-1.5 rounded border border-kiviGreen/40 bg-kiviGreen/15 px-2.5 py-1 font-mono text-[11px] font-semibold text-kiviGreen hover:bg-kiviGreen/25 focus-visible:ring-2 focus-visible:ring-kiviGreen disabled:opacity-50"
          >
            <PowerOff className="h-3 w-3" />
            <span>Return Control to ECU (0x00)</span>
          </button>
        </div>
        <div className="grid grid-cols-1 gap-2 sm:grid-cols-3">
          <input
            type="text"
            aria-label="IOControl DID hex"
            value={ioDidInput}
            onChange={(e) => setIoDidInput(e.target.value)}
            placeholder="DID e.g. 0x1234"
            className="rounded border border-kiviBorderStrong bg-kiviSurface px-2 py-1 font-mono text-[11px] text-kiviInk"
          />
          <select
            aria-label="IOControl parameter"
            value={ioParam}
            onChange={(e) => setIoParam(Number(e.target.value))}
            className="rounded border border-kiviBorderStrong bg-kiviSurface px-2 py-1 font-mono text-[11px] text-kiviInk"
          >
            <option value={3}>0x03 Short-Term Adjust</option>
            <option value={2}>0x02 Freeze Current State</option>
            <option value={1}>0x01 Reset to Default</option>
            <option value={0}>0x00 Return Control to ECU</option>
          </select>
          <input
            type="text"
            aria-label="IOControl state hex"
            value={ioStateHex}
            onChange={(e) => setIoStateHex(e.target.value)}
            placeholder="Control state bytes (hex)"
            className="rounded border border-kiviBorderStrong bg-kiviSurface px-2 py-1 font-mono text-[11px] text-kiviInk"
          />
        </div>
        <div className="flex flex-wrap justify-end gap-2">
          <button
            type="button"
            disabled={busy || !hasDevice}
            onClick={() => setConfirmIo((prev) => !prev)}
            className="min-h-[32px] rounded border border-kiviRed/40 bg-kiviRed/10 px-2.5 py-1 font-mono text-[11px] text-kiviRed hover:bg-kiviRed/20 focus-visible:ring-2 focus-visible:ring-kiviRed disabled:opacity-50"
          >
            Send Actuator Command (0x2F)
          </button>
        </div>
        {confirmIo && (
          <div className="flex flex-wrap items-center justify-between gap-2 rounded border border-kiviRed/40 bg-kiviRed/10 px-2.5 py-2 font-mono text-[11px] text-kiviInk">
            <span>Actuate DID {ioDidInput} directly? Click Return Control to ECU (0x00) afterwards.</span>
            <div className="flex gap-2">
              <button
                type="button"
                disabled={busy}
                onClick={() => void sendIoControl()}
                className="rounded bg-kiviRed px-2.5 py-1 font-semibold text-kiviAccentFg"
              >
                Confirm Actuation
              </button>
              <button
                type="button"
                onClick={() => setConfirmIo(false)}
                className="rounded border border-kiviBorder bg-kiviSurface px-2 py-1 text-kiviMuted"
              >
                Cancel
              </button>
            </div>
          </div>
        )}
        {ioResultState?.kind === 'ok' && (
          <p className="font-mono text-[11px] text-kiviGreen">
            {ioResultState.data.notes.join(' ')} [{ioResultState.data.response_hex}]
          </p>
        )}
        {ioResultState && ioResultState.kind !== 'ok' && (
          <p className="font-mono text-[11px] text-kiviRed">{ioResultState.message}</p>
        )}
      </section>
    </div>
  );
};
