import React, { useEffect, useState } from 'react';
import { Activity, AlertTriangle, Bike, Cable, Cpu, RefreshCw, Trash2 } from 'lucide-react';
import {
  runMotoClearDtcs,
  runMotoHondaEcmId,
  runMotoListPinouts,
  runMotoReadDtcs,
  runMotoReadLiveSensors,
  type DtcClearOutcome,
  type DtcReadOutcome,
  type FlashState,
  type HondaEcmIdentity,
  type MotoCablePinout,
  type MotoLiveSnapshot,
  type MotoProtocolFamily,
} from '../flashController';

export interface MotoDiagnosticsSubtabProps {
  isBusy: boolean;
  hasDevice: boolean;
  onLog: (level: 'info' | 'ok' | 'warn' | 'error', message: string) => void;
}

const MOTO_PROTOCOLS: Array<{ value: MotoProtocolFamily; label: string }> = [
  { value: 'honda_kline', label: 'Honda PGM-FI K-Line (4-Pin Red DLC)' },
  { value: 'yamaha_euro5_obd', label: 'Honda / Yamaha / Kawasaki Euro 5 CAN (6-Pin)' },
  { value: 'yamaha_kline', label: 'Yamaha K-Line (3-Pin / 4-Pin Pre-Euro5)' },
  { value: 'kawasaki_kds', label: 'Kawasaki KDS K-Line (4-Pin / 6-Pin)' },
  { value: 'suzuki_sds', label: 'Suzuki SDS K-Line (6-Pin White DLC)' },
];

export const MotoDiagnosticsSubtab: React.FC<MotoDiagnosticsSubtabProps> = ({
  isBusy,
  hasDevice,
  onLog,
}) => {
  const [family, setFamily] = useState<MotoProtocolFamily>('honda_kline');
  const [working, setWorking] = useState(false);
  const [ecmState, setEcmState] = useState<FlashState<HondaEcmIdentity> | null>(null);
  const [liveState, setLiveState] = useState<FlashState<MotoLiveSnapshot> | null>(null);
  const [dtcState, setDtcState] = useState<FlashState<DtcReadOutcome> | null>(null);
  const [clearState, setClearState] = useState<FlashState<DtcClearOutcome> | null>(null);
  const [confirmClear, setConfirmClear] = useState(false);
  const [pinouts, setPinouts] = useState<MotoCablePinout[]>([]);
  const [showPinouts, setShowPinouts] = useState(true);

  useEffect(() => {
    void runMotoListPinouts().then((res) => {
      if (res.kind === 'ok') {
        setPinouts(res.data);
      }
    });
  }, []);

  const busy = isBusy || working;

  const readHondaEcm = async () => {
    setWorking(true);
    try {
      const res = await runMotoHondaEcmId();
      setEcmState(res);
      onLog(
        res.kind === 'ok' ? 'ok' : 'warn',
        res.kind === 'ok' ? `Honda ECM ID: ${res.data.ecm_id_hex}` : res.message
      );
    } finally {
      setWorking(false);
    }
  };

  const readLive = async () => {
    setWorking(true);
    try {
      const res = await runMotoReadLiveSensors(family);
      setLiveState(res);
      onLog(
        res.kind === 'ok' ? 'ok' : 'warn',
        res.kind === 'ok'
          ? `Motorcycle telemetry (${res.data.family}): ${res.data.sensors.length} sensor(s).`
          : res.message
      );
    } finally {
      setWorking(false);
    }
  };

  const readDtcs = async () => {
    setWorking(true);
    try {
      const res = await runMotoReadDtcs(family);
      setDtcState(res);
      onLog(
        res.kind === 'ok' ? 'ok' : 'warn',
        res.kind === 'ok'
          ? `Motorcycle DTC scan (${res.data.protocol_used}): ${res.data.dtcs.length} code(s).`
          : res.message
      );
    } finally {
      setWorking(false);
    }
  };

  const clearDtcs = async () => {
    setWorking(true);
    setConfirmClear(false);
    try {
      const res = await runMotoClearDtcs(family);
      setClearState(res);
      onLog(
        res.kind === 'ok' ? 'ok' : 'error',
        res.kind === 'ok'
          ? `Motorcycle clear DTCs (${res.data.protocol_used}): ${res.data.cleared ? 'OK' : 'unconfirmed'}`
          : res.message
      );
    } finally {
      setWorking(false);
    }
  };

  return (
    <div className="space-y-3" data-testid="moto-diagnostics-subtab">
      <section className="space-y-2.5 rounded-lg border border-kiviBorder bg-kiviBg/50 p-3">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div className="flex items-center gap-2">
            <Bike className="h-3.5 w-3.5 text-kiviAccent" />
            <h3 className="font-mono text-[11px] font-semibold uppercase tracking-wide text-kiviMuted">
              Motorcycle Diagnostic Protocol
            </h3>
          </div>
          <select
            aria-label="Motorcycle protocol family"
            value={family}
            onChange={(e) => setFamily(e.target.value as MotoProtocolFamily)}
            className="rounded border border-kiviBorder/70 bg-kiviSurface px-2 py-1 font-mono text-[11px] text-kiviInk focus-visible:ring-2 focus-visible:ring-kiviAccent"
          >
            {MOTO_PROTOCOLS.map((p) => (
              <option key={p.value} value={p.value}>
                {p.label}
              </option>
            ))}
          </select>
        </div>

        <div className="flex flex-wrap gap-2">
          {family === 'honda_kline' && (
            <button
              type="button"
              disabled={busy || !hasDevice}
              onClick={() => void readHondaEcm()}
              className="inline-flex min-h-[32px] items-center gap-1.5 rounded border border-kiviAccent/40 bg-kiviAccent/15 px-2.5 py-1 font-mono text-[11px] text-kiviAccent hover:bg-kiviAccent/25 focus-visible:ring-2 focus-visible:ring-kiviAccent disabled:opacity-50"
            >
              <Cpu className="h-3 w-3" />
              <span>Probe Honda ECM ID</span>
            </button>
          )}
          <button
            type="button"
            disabled={busy || !hasDevice}
            onClick={() => void readLive()}
            className="inline-flex min-h-[32px] items-center gap-1.5 rounded bg-kiviAccent/20 px-2.5 py-1 font-mono text-[11px] text-kiviAccent hover:bg-kiviAccent/30 focus-visible:ring-2 focus-visible:ring-kiviAccent disabled:opacity-50"
          >
            <Activity className="h-3 w-3" />
            <span>Read Live Bike Sensors</span>
          </button>
          <button
            type="button"
            disabled={busy || !hasDevice}
            onClick={() => void readDtcs()}
            className="inline-flex min-h-[32px] items-center gap-1.5 rounded border border-kiviBorder/70 bg-kiviSurface px-2.5 py-1 font-mono text-[11px] text-kiviInk hover:border-kiviAccent focus-visible:ring-2 focus-visible:ring-kiviAccent disabled:opacity-50"
          >
            <RefreshCw className="h-3 w-3" />
            <span>Read Bike DTCs / Blink Codes</span>
          </button>
          <button
            type="button"
            disabled={busy || !hasDevice}
            onClick={() => setConfirmClear((p) => !p)}
            className="inline-flex min-h-[32px] items-center gap-1.5 rounded border border-kiviWarn/40 bg-kiviWarn/10 px-2.5 py-1 font-mono text-[11px] text-kiviWarn hover:bg-kiviWarn/20 focus-visible:ring-2 focus-visible:ring-kiviWarn disabled:opacity-50"
          >
            <Trash2 className="h-3 w-3" />
            <span>Clear Bike DTCs</span>
          </button>
        </div>

        {confirmClear && (
          <div className="flex flex-wrap items-center justify-between gap-2 rounded border border-kiviWarn/40 bg-kiviWarn/10 px-2.5 py-2 font-mono text-[11px] text-kiviInk">
            <span>Clear motorcycle fault memory now? Ensure ignition is ON and kill-switch is RUN.</span>
            <div className="flex gap-2">
              <button
                type="button"
                disabled={busy}
                onClick={() => void clearDtcs()}
                className="rounded bg-kiviWarn px-2.5 py-1 font-semibold text-kiviAccentFg"
              >
                Confirm Clear
              </button>
              <button
                type="button"
                onClick={() => setConfirmClear(false)}
                className="rounded border border-kiviBorder bg-kiviSurface px-2 py-1 text-kiviMuted"
              >
                Cancel
              </button>
            </div>
          </div>
        )}

        {ecmState?.kind === 'ok' && (
          <div className="rounded border border-kiviBorder bg-kiviSurface px-2.5 py-1.5 font-mono text-[11px] text-kiviInk">
            <div>
              ECM ID: <strong className="text-kiviAccent">{ecmState.data.ecm_id_hex}</strong> | Raw:{' '}
              {ecmState.data.raw_hex}
            </div>
            {ecmState.data.matched_profile && (
              <div className="text-[10px] text-kiviGreen">
                Matched Profile: {String(ecmState.data.matched_profile.make ?? '')}{' '}
                {String(ecmState.data.matched_profile.model ?? '')} (
                {String(ecmState.data.matched_profile.part_number ?? '')})
              </div>
            )}
          </div>
        )}
        {ecmState && ecmState.kind !== 'ok' && (
          <p className="font-mono text-[11px] text-kiviRed">{ecmState.message}</p>
        )}

        {liveState?.kind === 'ok' && (
          <div className="space-y-2">
            <div className="grid grid-cols-[repeat(auto-fit,minmax(160px,1fr))] gap-2">
              {liveState.data.sensors.map((s) => (
                <div
                  key={s.key}
                  className="rounded border border-kiviBorder bg-kiviSurface px-2.5 py-1.5 font-mono text-[11px]"
                >
                  <div className="text-[10px] text-kiviMuted">{s.label}</div>
                  <div className="font-semibold tabular-nums text-kiviInk">{s.display}</div>
                </div>
              ))}
            </div>
            {liveState.data.raw_hex && (
              <p className="break-all font-mono text-[10px] text-kiviMuted">
                Raw payload: {liveState.data.raw_hex}
              </p>
            )}
          </div>
        )}
        {liveState && liveState.kind !== 'ok' && (
          <p className="font-mono text-[11px] text-kiviRed">{liveState.message}</p>
        )}

        {dtcState?.kind === 'ok' && (
          <div className="space-y-1">
            {dtcState.data.dtcs.length === 0 ? (
              <p className="font-mono text-[11px] text-kiviGreen">
                0 motorcycle DTCs stored ({dtcState.data.protocol_used}).
              </p>
            ) : (
              dtcState.data.dtcs.map((d) => (
                <div
                  key={`${d.source}-${d.code}`}
                  className="rounded border border-kiviBorder bg-kiviSurface px-2.5 py-1.5 font-mono text-[11px]"
                >
                  <div className="flex items-center justify-between">
                    <span className="font-semibold text-kiviInk">{d.code}</span>
                    <span className="text-[10px] text-kiviWarn">{d.status_label}</span>
                  </div>
                  {d.description && <p className="text-[10px] text-kiviSecondary">{d.description}</p>}
                </div>
              ))
            )}
          </div>
        )}
        {dtcState && dtcState.kind !== 'ok' && (
          <p className="font-mono text-[11px] text-kiviRed">{dtcState.message}</p>
        )}

        {clearState?.kind === 'ok' && (
          <div className="rounded border border-kiviGreen/30 bg-kiviGreen/10 px-2.5 py-1.5 font-mono text-[11px] text-kiviGreen">
            <div>Clear status ({clearState.data.protocol_used}): {clearState.data.cleared ? 'Cleared' : 'Check notes'}</div>
            {clearState.data.notes.length > 0 && (
              <div className="text-[10px] text-kiviSecondary">{clearState.data.notes.join(' | ')}</div>
            )}
          </div>
        )}
        {clearState && clearState.kind !== 'ok' && (
          <p className="font-mono text-[11px] text-kiviRed">{clearState.message}</p>
        )}
      </section>

      {/* Modified Motorcycle DLC-to-OBD16 Pinout Reference */}
      <section className="space-y-2 rounded-lg border border-kiviBorder bg-kiviBg/50 p-3">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Cable className="h-3.5 w-3.5 text-kiviAccent" />
            <h3 className="font-mono text-[11px] font-semibold uppercase tracking-wide text-kiviMuted">
              Modified DLC to OBD-II (16-Pin) Pinout Guide
            </h3>
          </div>
          <button
            type="button"
            onClick={() => setShowPinouts((v) => !v)}
            className="font-mono text-[10px] text-kiviAccent hover:underline"
          >
            {showPinouts ? 'Hide Guide' : 'Show Guide'}
          </button>
        </div>

        {showPinouts && (
          <div className="space-y-2">
            {pinouts.length === 0 ? (
              <p className="flex items-center gap-1.5 font-mono text-[11px] text-kiviMuted">
                <AlertTriangle className="h-3.5 w-3.5 text-kiviWarn" />
                <span>Pinout table loads in desktop mode, or match OBD Pin 4/5 GND, Pin 6 CAN-H, Pin 7 K-Line, Pin 14 CAN-L, Pin 16 +12V.</span>
              </p>
            ) : (
              pinouts.map((p) => (
                <div
                  key={p.id}
                  className="space-y-1 rounded border border-kiviBorder bg-kiviSurface px-2.5 py-2 font-mono text-[10px]"
                >
                  <div className="flex flex-wrap items-center justify-between gap-1 text-[11px] font-semibold text-kiviInk">
                    <span>{p.brand_models}</span>
                    <span className="text-kiviAccent">{p.connector_type}</span>
                  </div>
                  <div className="text-kiviSecondary">Protocol: {p.protocol}</div>
                  <ul className="list-disc pl-4 text-kiviInk">
                    {p.obd16_wiring.map((w) => (
                      <li key={w}>{w}</li>
                    ))}
                  </ul>
                  <div className="text-kiviGreen">Recommended: {p.recommended_adapter}</div>
                  <div className="text-kiviMuted">{p.notes}</div>
                </div>
              ))
            )}
          </div>
        )}
      </section>
    </div>
  );
};
