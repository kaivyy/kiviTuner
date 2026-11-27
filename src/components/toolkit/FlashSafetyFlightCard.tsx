import React, { useCallback, useEffect, useState } from 'react';
import {
  Activity,
  ChevronDown,
  ChevronRight,
  Download,
  RefreshCw,
  ShieldCheck,
  ShieldAlert,
  Trash2,
} from 'lucide-react';
import {
  bridgeClearBusFlightLog,
  bridgeExportBusFlightLogAsc,
  bridgeExportBusFlightLogJsonl,
  bridgeGetBusFlightLog,
  bridgeGetExperimentalWriteUnlock,
  bridgeListHardwareVerificationMatrix,
  bridgeSetExperimentalWriteUnlock,
  type BusFrameRecord,
  type HardwareTargetCapability,
  type HardwareVerificationTier,
} from '../../lib/flashBridge';

function tierBadgeClass(tier: HardwareVerificationTier): string {
  switch (tier) {
    case 'BenchVerified':
      return 'border-kiviGreen/40 bg-kiviGreen/10 text-kiviGreen';
    case 'ReadOnlyVerified':
      return 'border-kiviAccent/40 bg-kiviAccent/10 text-kiviAccent';
    case 'ExperimentalUntested':
    default:
      return 'border-kiviWarn/40 bg-kiviWarn/10 text-kiviWarn';
  }
}

function tierLabel(tier: HardwareVerificationTier): string {
  switch (tier) {
    case 'BenchVerified':
      return 'Bench Verified';
    case 'ReadOnlyVerified':
      return 'Read-Only Verified';
    case 'ExperimentalUntested':
    default:
      return 'Experimental (Untested)';
  }
}

function downloadTextFile(filename: string, content: string, mime = 'text/plain;charset=utf-8') {
  const blob = new Blob([content], { type: mime });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  URL.revokeObjectURL(url);
}

export interface FlashSafetyFlightCardProps {
  onLog?: (level: 'info' | 'warn' | 'ok' | 'error', text: string) => void;
}

export const FlashSafetyFlightCard: React.FC<FlashSafetyFlightCardProps> = ({ onLog }) => {
  const [matrixOpen, setMatrixOpen] = useState(false);
  const [matrix, setMatrix] = useState<HardwareTargetCapability[]>([]);
  const [writeUnlocked, setWriteUnlocked] = useState<boolean>(false);
  const [frames, setFrames] = useState<BusFrameRecord[]>([]);
  const [backendAvailable, setBackendAvailable] = useState<boolean>(true);
  const [busy, setBusy] = useState<boolean>(false);

  const refreshStatus = useCallback(async () => {
    setBusy(true);
    try {
      const [m, unlocked, log] = await Promise.all([
        bridgeListHardwareVerificationMatrix(),
        bridgeGetExperimentalWriteUnlock(),
        bridgeGetBusFlightLog(200),
      ]);
      if (m === null && unlocked === null && log === null) {
        setBackendAvailable(false);
        return;
      }
      setBackendAvailable(true);
      if (m) setMatrix(m);
      if (unlocked !== null) setWriteUnlocked(unlocked);
      if (log) setFrames(log);
    } finally {
      setBusy(false);
    }
  }, []);

  useEffect(() => {
    void refreshStatus();
  }, [refreshStatus]);

  const handleToggleUnlock = async (next: boolean) => {
    const res = await bridgeSetExperimentalWriteUnlock(next);
    if (res === null) {
      onLog?.('warn', 'Expert hardware write unlock requires the kiviTuner desktop backend.');
      return;
    }
    setWriteUnlocked(res);
    onLog?.(
      res ? 'warn' : 'info',
      res
        ? 'Expert / Bench Write Unlock enabled for ExperimentalUntested real hardware. Ensure stabilized >=12.5V power and stock backup.'
        : 'Live hardware write lock restored (ExperimentalUntested writes blocked).'
    );
  };

  const handleExportAsc = async () => {
    const asc = await bridgeExportBusFlightLogAsc();
    if (!asc) return;
    downloadTextFile(`kivituner-bus-trace-${Date.now()}.asc`, asc);
    onLog?.('ok', `Exported ${frames.length} bus trace records as CAN .ASC.`);
  };

  const handleExportJsonl = async () => {
    const jsonl = await bridgeExportBusFlightLogJsonl();
    if (!jsonl) return;
    downloadTextFile(
      `kivituner-bus-trace-${Date.now()}.jsonl`,
      jsonl,
      'application/x-ndjson;charset=utf-8'
    );
    onLog?.('ok', `Exported ${frames.length} bus trace records as JSONL.`);
  };

  const handleClearLog = async () => {
    const cleared = await bridgeClearBusFlightLog();
    if (cleared !== null) {
      setFrames([]);
      onLog?.('info', `Cleared ${cleared} bus flight recorder frame(s).`);
    }
  };

  return (
    <section className="space-y-2.5 rounded-lg border border-kiviBorder bg-kiviBg/50 p-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <button
          type="button"
          aria-expanded={matrixOpen}
          onClick={() => setMatrixOpen((v) => !v)}
          className="inline-flex items-center gap-1.5 text-[11px] font-semibold uppercase tracking-wide text-kiviSecondary hover:text-kiviInk focus-visible:outline-2 focus-visible:outline-kiviAccent"
        >
          {matrixOpen ? (
            <ChevronDown className="h-3.5 w-3.5 text-kiviAccent" />
          ) : (
            <ChevronRight className="h-3.5 w-3.5 text-kiviAccent" />
          )}
          {writeUnlocked ? (
            <ShieldAlert className="h-3.5 w-3.5 text-kiviWarn" />
          ) : (
            <ShieldCheck className="h-3.5 w-3.5 text-kiviGreen" />
          )}
          <span>Hardware Safety Matrix &amp; Bus Flight Recorder</span>
        </button>

        <div className="flex flex-wrap items-center gap-1.5 text-[10px]">
          <span
            className={`rounded border px-1.5 py-0.5 font-mono tabular-nums ${
              writeUnlocked
                ? 'border-kiviWarn/40 bg-kiviWarn/10 text-kiviWarn'
                : 'border-kiviGreen/40 bg-kiviGreen/10 text-kiviGreen'
            }`}
          >
            {writeUnlocked ? 'Write Unlocked (Expert)' : 'Live Write Locked'}
          </span>
          <span className="inline-flex items-center gap-1 rounded border border-kiviBorder bg-kiviSurface px-1.5 py-0.5 font-mono text-kiviSecondary tabular-nums">
            <Activity className="h-3 w-3 text-kiviAccent" />
            <span>{frames.length} frames</span>
          </span>
        </div>
      </div>

      <div className="flex flex-wrap items-center justify-between gap-2 rounded border border-kiviBorder bg-kiviSurface px-2.5 py-2 text-[11px]">
        <label className="flex cursor-pointer items-center gap-2 text-kiviInk">
          <input
            type="checkbox"
            checked={writeUnlocked}
            disabled={!backendAvailable}
            onChange={(e) => void handleToggleUnlock(e.target.checked)}
            className="h-3.5 w-3.5 accent-kiviWarn"
          />
          <span>
            Bench / Expert Write Unlock (permit writes on ExperimentalUntested hardware)
          </span>
        </label>

        <div className="flex flex-wrap items-center gap-1">
          <button
            type="button"
            disabled={busy || !backendAvailable}
            onClick={() => void refreshStatus()}
            className="inline-flex items-center gap-1 rounded border border-kiviBorderStrong bg-kiviBg px-2 py-0.5 text-[10px] text-kiviSecondary hover:text-kiviInk disabled:opacity-40"
          >
            <RefreshCw className="h-3 w-3" />
            <span>Refresh</span>
          </button>
          <button
            type="button"
            disabled={!backendAvailable || frames.length === 0}
            onClick={() => void handleExportAsc()}
            className="inline-flex items-center gap-1 rounded border border-kiviBorderStrong bg-kiviBg px-2 py-0.5 text-[10px] text-kiviInk hover:bg-kiviElevated disabled:opacity-40"
          >
            <Download className="h-3 w-3" />
            <span>Export .ASC</span>
          </button>
          <button
            type="button"
            disabled={!backendAvailable || frames.length === 0}
            onClick={() => void handleExportJsonl()}
            className="inline-flex items-center gap-1 rounded border border-kiviBorderStrong bg-kiviBg px-2 py-0.5 text-[10px] text-kiviInk hover:bg-kiviElevated disabled:opacity-40"
          >
            <Download className="h-3 w-3" />
            <span>JSONL</span>
          </button>
          <button
            type="button"
            disabled={!backendAvailable || frames.length === 0}
            onClick={() => void handleClearLog()}
            aria-label="Clear bus flight recorder"
            className="inline-flex items-center gap-1 rounded border border-kiviRed/30 bg-kiviRed/10 px-2 py-0.5 text-[10px] text-kiviRed hover:bg-kiviRed/20 disabled:opacity-40"
          >
            <Trash2 className="h-3 w-3" />
            <span>Clear</span>
          </button>
        </div>
      </div>

      {!backendAvailable && (
        <p className="text-[10px] text-kiviMuted">
          Hardware matrix and bus flight capture run inside the kiviTuner desktop binary.
        </p>
      )}

      {matrixOpen && (
        <div className="space-y-2">
          {matrix.length > 0 && (
            <div className="max-h-52 overflow-auto rounded border border-kiviBorder bg-kiviSurface">
              <table className="w-full text-left font-mono text-[10px]">
                <thead className="sticky top-0 bg-kiviElevated text-kiviMuted">
                  <tr>
                    <th className="px-2 py-1">Target</th>
                    <th className="px-2 py-1">Tier</th>
                    <th className="px-2 py-1">R / W</th>
                    <th className="px-2 py-1">Verification Evidence</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-kiviBorder text-kiviSecondary">
                  {matrix.map((row) => (
                    <tr key={row.id}>
                      <td className="px-2 py-1 font-semibold text-kiviInk">{row.name}</td>
                      <td className="px-2 py-1">
                        <span
                          className={`inline-block rounded border px-1.5 py-0.5 ${tierBadgeClass(
                            row.tier
                          )}`}
                        >
                          {tierLabel(row.tier)}
                        </span>
                      </td>
                      <td className="px-2 py-1 tabular-nums">
                        {row.supports_read ? 'R' : '-'} / {row.supports_write ? 'W' : '-'}
                      </td>
                      <td className="px-2 py-1">{row.evidence_note}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}

          {frames.length > 0 && (
            <div className="max-h-36 overflow-auto rounded border border-kiviBorder bg-kiviBg p-2 font-mono text-[10px] text-kiviSecondary tabular-nums">
              {frames.slice(-20).map((f) => (
                <div key={f.seq} className="flex gap-2 truncate">
                  <span className="text-kiviMuted">#{f.seq}</span>
                  <span
                    className={
                      f.direction === 'Tx'
                        ? 'text-kiviAccent'
                        : f.direction === 'Rx'
                        ? 'text-kiviGreen'
                        : f.direction === 'Error'
                        ? 'text-kiviRed'
                        : 'text-kiviWarn'
                    }
                  >
                    {f.direction}
                  </span>
                  <span>{f.protocol}</span>
                  <span className="truncate text-kiviInk">{f.data_hex || f.note || ''}</span>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </section>
  );
};
