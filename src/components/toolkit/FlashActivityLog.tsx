import React, { useEffect, useRef } from 'react';
import { Check, Copy, Loader2, Trash2 } from 'lucide-react';
import type { FlashProgressEvent } from '../../lib/flashBridge';
import { formatByteSize, type FlashLogEntry, type FlashLogLevel } from './flashPresets';

export interface FlashActivityLogProps {
  isBusy: boolean;
  activeStage: string | null;
  progress: FlashProgressEvent | null;
  logs: FlashLogEntry[];
  onClearLogs: () => void;
}

const LOG_LEVEL_CLASS: Record<FlashLogLevel, string> = {
  info: 'text-kiviSecondary',
  ok: 'text-kiviGreen',
  warn: 'text-kiviWarn',
  error: 'text-kiviRed',
};

function resolveStageBadge(
  stage: string | null,
  isBusy: boolean,
  logs: FlashLogEntry[]
): { label: string; className: string } {
  const normalized = stage?.toLowerCase() ?? '';
  if (normalized === 'complete') {
    return { label: 'COMPLETE', className: 'border-kiviGreen/40 bg-kiviGreen/15 text-kiviGreen' };
  }
  if (normalized === 'erasing' || normalized === 'writing') {
    return {
      label: normalized.toUpperCase(),
      className: 'border-kiviWarn/40 bg-kiviWarn/15 text-kiviWarn',
    };
  }
  if (isBusy) {
    return {
      label: normalized ? normalized.toUpperCase() : 'WORKING',
      className: 'border-kiviAccent/40 bg-kiviAccent/15 text-kiviAccent',
    };
  }
  const latest = logs[logs.length - 1];
  if (latest?.level === 'error') {
    return { label: 'ERROR', className: 'border-kiviRed/40 bg-kiviRed/15 text-kiviRed' };
  }
  return { label: 'IDLE', className: 'border-kiviBorder bg-kiviElevated text-kiviMuted' };
}

export const FlashActivityLog: React.FC<FlashActivityLogProps> = ({
  isBusy,
  activeStage,
  progress,
  logs,
  onClearLogs,
}) => {
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const [copied, setCopied] = React.useState(false);
  const copyTimerRef = useRef<number | null>(null);

  useEffect(() => {
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [logs]);

  useEffect(
    () => () => {
      if (copyTimerRef.current !== null) {
        window.clearTimeout(copyTimerRef.current);
      }
    },
    []
  );

  const handleCopy = () => {
    const text = logs
      .map((l) => `[${l.time}] [${l.level.toUpperCase()}] ${l.message}`)
      .join('\n');
    void navigator.clipboard?.writeText(text).then(() => {
      setCopied(true);
      if (copyTimerRef.current !== null) window.clearTimeout(copyTimerRef.current);
      copyTimerRef.current = window.setTimeout(() => setCopied(false), 1500);
    });
  };

  const badge = resolveStageBadge(activeStage ?? progress?.stage ?? null, isBusy, logs);
  const percent = progress ? Math.min(100, Math.max(0, progress.percent)) : isBusy ? 0 : 0;
  const barColor =
    badge.label === 'COMPLETE'
      ? 'bg-kiviGreen'
      : badge.label === 'ERROR'
        ? 'bg-kiviRed'
        : badge.label === 'ERASING' || badge.label === 'WRITING'
          ? 'bg-kiviWarn'
          : 'bg-kiviAccent';

  return (
    <section className="space-y-2 rounded-lg border border-kiviBorder bg-kiviBg/80 p-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex items-center gap-2">
          {isBusy && <Loader2 className="h-3.5 w-3.5 animate-spin text-kiviAccent" />}
          <h3 className="text-[11px] font-semibold uppercase tracking-wide text-kiviMuted">
            Transfer Progress &amp; Activity Log
          </h3>
          <span
            className={`rounded border px-1.5 py-0.5 font-mono text-[10px] font-semibold ${badge.className}`}
          >
            {badge.label}
          </span>
        </div>
        <div className="flex items-center gap-1.5">
          {progress && (
            <span className="font-mono text-[10px] text-kiviSecondary tabular-nums">
              {formatByteSize(progress.bytes_done)} / {formatByteSize(progress.bytes_total)} ({percent}%)
            </span>
          )}
          <button
            type="button"
            aria-label="Copy flash activity log"
            onClick={handleCopy}
            disabled={logs.length === 0}
            className="inline-flex items-center gap-1 rounded border border-kiviBorder px-1.5 py-0.5 text-[10px] text-kiviMuted hover:bg-kiviElevated hover:text-kiviInk disabled:cursor-not-allowed disabled:opacity-50"
          >
            {copied ? <Check className="h-3 w-3 text-kiviGreen" /> : <Copy className="h-3 w-3" />}
            <span>{copied ? 'Copied' : 'Copy'}</span>
          </button>
          <button
            type="button"
            aria-label="Clear flash activity log"
            onClick={onClearLogs}
            disabled={logs.length === 0}
            className="inline-flex items-center gap-1 rounded border border-kiviBorder px-1.5 py-0.5 text-[10px] text-kiviMuted hover:bg-kiviElevated hover:text-kiviInk disabled:cursor-not-allowed disabled:opacity-50"
          >
            <Trash2 className="h-3 w-3" />
            <span>Clear</span>
          </button>
        </div>
      </div>

      <div
        role="progressbar"
        aria-label="ECU transfer progress"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={progress ? percent : isBusy ? undefined : 0}
        className="h-1.5 w-full overflow-hidden rounded bg-kiviBorder"
      >
        {isBusy && !progress ? (
          <div className="h-full w-1/2 animate-pulse rounded bg-kiviAccent" />
        ) : (
          <div
            className={`h-full transition-[width] duration-150 ${barColor}`}
            style={{ width: `${progress ? percent : 0}%` }}
          />
        )}
      </div>

      {progress?.message && (
        <p className="truncate font-mono text-[10px] text-kiviSecondary">{progress.message}</p>
      )}

      <div
        ref={scrollRef}
        aria-label="ECU activity log output"
        className="max-h-36 min-h-[4.5rem] space-y-0.5 overflow-y-auto rounded border border-kiviBorder bg-kiviSurface px-2 py-1.5 font-mono text-[10px]"
      >
        {logs.length === 0 ? (
          <p className="text-kiviMuted">
            Ready. Choose an ECU/ROM preset and transport, then start Read or Write.
          </p>
        ) : (
          logs.map((entry) => (
            <div key={entry.id} className={`flex items-start gap-2 leading-4 ${LOG_LEVEL_CLASS[entry.level]}`}>
              <span className="shrink-0 text-kiviMuted tabular-nums">[{entry.time}]</span>
              <span className="break-words">{entry.message}</span>
            </div>
          ))
        )}
      </div>
    </section>
  );
};
