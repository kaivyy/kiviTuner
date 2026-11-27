/**
 * Quick OBD-II / UDS diagnostic cockpit for ELM327, OpenPort 2.0, J2534,
 * DoIP, and SocketCAN adapters.
 */

import React, { useState } from 'react';
import {
  Activity,
  AlertTriangle,
  BatteryCharging,
  CheckCircle2,
  Loader2,
  RefreshCw,
  RotateCcw,
  Trash2,
} from 'lucide-react';
import type {
  DtcClearOutcome,
  DtcReadOutcome,
  EcuIdentitySummary,
  EcuResetMode,
  EcuResetOutcome,
  FlashState,
  TransportChoice,
} from './flashController';

export interface FlashDiagnosticsSectionProps {
  isBusy: boolean;
  hasDevice: boolean;
  transport: TransportChoice | null;
  ecuInfoState: FlashState<EcuIdentitySummary> | null;
  dtcReadState: FlashState<DtcReadOutcome> | null;
  dtcClearState: FlashState<DtcClearOutcome> | null;
  ecuResetState: FlashState<EcuResetOutcome> | null;
  onReadEcuInfo: () => void;
  onReadDtcs: () => void;
  onClearDtcs: () => void;
  onEcuReset: (mode: EcuResetMode) => void;
}

const RESET_MODES: Array<{ value: EcuResetMode; label: string; hint: string }> = [
  { value: 'soft', label: 'Soft Reset (0x03)', hint: 'Recommended after clearing codes or quick restart' },
  { value: 'key_off_on', label: 'Key Off/On (0x02)', hint: 'Simulates ignition key cycle' },
  { value: 'hard', label: 'Hard Reset (0x01)', hint: 'Full ECU cold reboot' },
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
  onReadEcuInfo,
  onReadDtcs,
  onClearDtcs,
  onEcuReset,
}) => {
  const [confirmClearOpen, setConfirmClearOpen] = useState(false);
  const [resetMode, setResetMode] = useState<EcuResetMode>('soft');
  const [confirmResetOpen, setConfirmResetOpen] = useState(false);

  const infoData = ecuInfoState?.kind === 'ok' ? ecuInfoState.data : null;
  const dtcData = dtcReadState?.kind === 'ok' ? dtcReadState.data : null;
  const battery = formatBatteryVolts(infoData?.battery_mv ?? null);

  return (
    <div className="space-y-3" data-testid="flash-diagnostics-section">
      {/* Adapter Readiness Banner */}
      <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-kiviBorder bg-kiviBg/60 px-3 py-2 font-mono text-[11px]">
        <div className="flex items-center gap-2 text-kiviInk">
          <Activity className="h-3.5 w-3.5 text-kiviAccent" />
          <span>
            {hasDevice
              ? `Ready on ${transport ?? 'hardware'} transport. ELM327 can scan ECU identity, read/clear DTCs, and send ECU reset.`
              : 'No adapter attached yet. Click Auto-Detect & Connect Cable above, or switch to ELM327 Serial.'}
          </span>
        </div>
        {transport === 'serial' && (
          <span className="rounded bg-kiviAccent/15 px-2 py-0.5 text-[10px] text-kiviAccent">
            ELM327 / OBD Mode
          </span>
        )}
      </div>

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
            disabled={isBusy}
            onClick={onReadEcuInfo}
            className="inline-flex min-h-[32px] items-center gap-1.5 rounded border border-kiviAccent/40 bg-kiviAccent/15 px-2.5 py-1 font-mono text-[11px] text-kiviAccent hover:bg-kiviAccent/25 focus-visible:ring-2 focus-visible:ring-kiviAccent disabled:cursor-not-allowed disabled:opacity-50"
          >
            {isBusy ? (
              <Loader2 className="h-3 w-3 animate-spin" />
            ) : (
              <RefreshCw className="h-3 w-3" />
            )}
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

      {/* Card 2: Fault Codes (DTCs) */}
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
              disabled={isBusy}
              onClick={onReadDtcs}
              className="inline-flex min-h-[32px] items-center gap-1.5 rounded bg-kiviAccent/20 px-2.5 py-1 font-mono text-[11px] text-kiviAccent hover:bg-kiviAccent/30 focus-visible:ring-2 focus-visible:ring-kiviAccent disabled:cursor-not-allowed disabled:opacity-50"
            >
              <RefreshCw className="h-3 w-3" />
              <span>Read DTCs</span>
            </button>
            <button
              type="button"
              disabled={isBusy}
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
                disabled={isBusy}
                onClick={() => {
                  setConfirmClearOpen(false);
                  onClearDtcs();
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
                    className="flex flex-wrap items-center justify-between gap-2 rounded border border-kiviBorder bg-kiviSurface px-2.5 py-1.5 font-mono text-[11px]"
                  >
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
      </section>

      {/* Card 3: ECU Reset (UDS 0x11) */}
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
            disabled={isBusy}
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
                disabled={isBusy}
                onClick={() => {
                  setConfirmResetOpen(false);
                  onEcuReset(resetMode);
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
  );
};
