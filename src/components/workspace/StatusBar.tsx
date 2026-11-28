import React, { useEffect, useState } from 'react';
import { Layers, Redo2, Undo2 } from 'lucide-react';
import { useTelemetryStore } from '../../stores/useTelemetryStore';
import type { UndoStatus } from '../../lib/bridge/windowBridge';
import type { LayoutPreset } from '../../App';
import {
  getMinimizedFloatingWindows,
  requestRestoreFloatingWindow,
  subscribeWindowStack,
  type MinimizedWindowEntry,
} from '../layout/floatingWindowModel';
import { useHorizontalStripScroll } from '../../hooks/useHorizontalStripScroll';

export const EcuStatusBarIndicator: React.FC = () => {
  const connectionState = useTelemetryStore((s) => s.connectionState);
  const latestFrame = useTelemetryStore((s) => s.latestFrame);
  const isConnected = connectionState.state === 'Connected';

  return (
    <span className="inline-flex min-w-[140px] shrink-0 items-center gap-1.5 whitespace-nowrap tabular-nums">
      <span
        className={`inline-block h-2 w-2 shrink-0 rounded-full ${
          isConnected ? 'bg-kiviGreen' : 'bg-kiviBorderStrong'
        }`}
      />
      <span>
        ECU:{' '}
        {isConnected
          ? `Live (${latestFrame ? Math.round(latestFrame.rpm) : 0} RPM)`
          : connectionState.state}
      </span>
    </span>
  );
};

interface StatusBarProps {
  selectedMapName: string | undefined;
  isDesktop: boolean;
  layoutPreset: LayoutPreset;
  diffRegionsCount: number;
  waveformPitch: number;
  romLabel?: string;
  romSize?: number;
  sha256Short?: string | null;
  checksumAlgorithmName?: string | null;
  checksumStatus?: 'valid' | 'invalid' | null;
  undoStatus?: UndoStatus;
  onUndo?: () => void;
  onRedo?: () => void;
}

export const StatusBar: React.FC<StatusBarProps> = React.memo(({
  selectedMapName,
  isDesktop,
  layoutPreset,
  diffRegionsCount,
  waveformPitch,
  romLabel,
  romSize = 0,
  sha256Short,
  checksumAlgorithmName,
  checksumStatus,
  undoStatus,
  onUndo,
  onRedo,
}) => {
  const [minimizedWindows, setMinimizedWindows] = useState<MinimizedWindowEntry[]>(() =>
    getMinimizedFloatingWindows()
  );
  const statusStrip = useHorizontalStripScroll<HTMLElement>(
    `${selectedMapName ?? ''}:${romLabel ?? ''}:${minimizedWindows.length}:${layoutPreset}:${diffRegionsCount}`
  );

  useEffect(() => {
    setMinimizedWindows(getMinimizedFloatingWindows());
    return subscribeWindowStack(() => {
      setMinimizedWindows(getMinimizedFloatingWindows());
    });
  }, []);

  return (
  <footer
    ref={statusStrip.ref}
    onScroll={statusStrip.refresh}
    className="kivi-tab-scrollbar flex h-7 shrink-0 items-center justify-between gap-2 overflow-x-auto overflow-y-hidden border-t border-kiviBorder bg-kiviSurface px-2.5 text-[11px] text-kiviSecondary tabular-nums md:px-3"
  >
    <div className="flex min-w-0 items-center gap-2 whitespace-nowrap md:gap-3">
      <span className="hidden shrink-0 items-center gap-1.5 whitespace-nowrap md:flex">
        <span className="inline-block h-2 w-2 shrink-0 rounded-full bg-kiviGreen" />
        <span>Engine Ready</span>
      </span>
      <span className="hidden shrink-0 text-kiviBorderStrong md:inline">|</span>
      <span className="max-w-[140px] truncate whitespace-nowrap sm:max-w-xs">
        Map: {selectedMapName ?? 'None'}
      </span>
      <span className="shrink-0 text-kiviBorderStrong">|</span>
      <EcuStatusBarIndicator />
      {minimizedWindows.length > 0 && (
        <>
          <span className="shrink-0 text-kiviBorderStrong">|</span>
          <div className="flex shrink-0 items-center gap-1" role="group" aria-label="Minimized tool windows">
            {minimizedWindows.map((win) => (
              <button
                key={win.id}
                type="button"
                onClick={() => requestRestoreFloatingWindow(win.id)}
                title={`Restore ${win.title}`}
                className="kivi-anim-pop-in inline-flex h-5 max-w-[140px] shrink-0 items-center gap-1 rounded border border-kiviAccent/50 bg-kiviAccent/15 px-1.5 text-[10px] font-medium text-kiviAccent transition-colors hover:bg-kiviAccent/25"
              >
                <Layers className="h-2.5 w-2.5 shrink-0" />
                <span className="truncate">{win.title}</span>
              </button>
            ))}
          </div>
        </>
      )}
      {romLabel && (
        <>
          <span className="hidden shrink-0 text-kiviBorderStrong xl:inline">|</span>
          <span className="hidden truncate whitespace-nowrap font-mono text-[10px] xl:inline">
            {romLabel} ({Math.round(romSize / 1024)} KiB{sha256Short ? ` #${sha256Short}` : ''})
          </span>
        </>
      )}
      {checksumAlgorithmName && (
        <>
          <span className="hidden shrink-0 text-kiviBorderStrong lg:inline">|</span>
          <span
            className={`hidden shrink-0 whitespace-nowrap font-mono text-[10px] lg:inline ${
              checksumStatus === 'valid'
                ? 'text-kiviGreen'
                : checksumStatus === 'invalid'
                  ? 'text-kiviRed'
                  : 'text-kiviMuted'
            }`}
          >
            CKSUM: {checksumAlgorithmName}
          </span>
        </>
      )}
    </div>

    <div className="flex shrink-0 items-center gap-1.5 whitespace-nowrap font-mono text-[10px] md:gap-2">
      {undoStatus && onUndo && onRedo && (
        <div className="flex shrink-0 items-center gap-1">
          <button
            type="button"
            disabled={!undoStatus.can_undo}
            onClick={onUndo}
            title="Undo (Ctrl+Z)"
            aria-label="Undo edit"
            className="flex shrink-0 items-center gap-1 whitespace-nowrap rounded border border-kiviBorder px-1.5 py-0.5 text-kiviSecondary hover:bg-kiviElevated hover:text-kiviInk disabled:opacity-40"
          >
            <Undo2 className="h-3 w-3 shrink-0" />
            <span className="inline-block min-w-[1.5ch] text-right">{undoStatus.undo_count}</span>
          </button>
          <button
            type="button"
            disabled={!undoStatus.can_redo}
            onClick={onRedo}
            title="Redo (Ctrl+Y)"
            aria-label="Redo edit"
            className="flex shrink-0 items-center gap-1 whitespace-nowrap rounded border border-kiviBorder px-1.5 py-0.5 text-kiviSecondary hover:bg-kiviElevated hover:text-kiviInk disabled:opacity-40"
          >
            <Redo2 className="h-3 w-3 shrink-0" />
            <span className="inline-block min-w-[1.5ch] text-right">{undoStatus.redo_count}</span>
          </button>
          <span className="hidden shrink-0 text-kiviBorderStrong sm:inline">|</span>
        </div>
      )}
      <span className="hidden shrink-0 whitespace-nowrap md:inline">
        {isDesktop ? 'Rust Core' : 'Browser'}
      </span>
      <span className="hidden shrink-0 text-kiviBorderStrong lg:inline">|</span>
      <span className="hidden shrink-0 whitespace-nowrap lg:inline">{layoutPreset}</span>
      <span className="hidden shrink-0 text-kiviBorderStrong sm:inline">|</span>
      <span className="hidden min-w-[54px] shrink-0 whitespace-nowrap sm:inline">
        Diffs: {diffRegionsCount}
      </span>
      <span className="hidden shrink-0 text-kiviBorderStrong lg:inline">|</span>
      <span className="hidden min-w-[54px] shrink-0 whitespace-nowrap lg:inline">
        Pitch: {waveformPitch}
      </span>
    </div>
  </footer>
  );
});
