import React, { useMemo } from 'react';
import { Binary, Crosshair, GitCommit, Info, Pencil, RotateCcw, ShieldAlert, ShieldCheck, X } from 'lucide-react';
import type { MapDef } from '../../types/kivi';
import type { UndoStatus } from '../../lib/bridge/windowBridge';
import { useMapStore } from '../../stores/useMapStore';
import { computeRomDeltaLedger } from './deltaLedgerModel';
import type { RightDockAnimState } from './rightDockModel';

interface InspectorSidebarProps {
  selectedRange: { start: number; end: number } | null;
  waveformPitch: number;
  onExtractMap: () => void;
  checksumStatus: string | null;
  checksumAlgorithmName?: string | null;
  checksumNotice?: string | null;
  onFixChecksum: () => void;
  romLoaded: boolean;
  romSize: number;
  romLabel?: string;
  hashes: { md5: string; sha256: string } | null;
  undoStatus?: UndoStatus;
  selectedMap: MapDef | undefined;
  onEditMapProperties?: (map: MapDef) => void;
  onViewInHex?: (address: number) => void;
  onRevertMapToOriginal?: () => void;
  onClose?: () => void;
  animState?: RightDockAnimState;
  desktopWidth?: number;
  isResizing?: boolean;
  collapsePreview?: boolean;
}

export const InspectorSidebar: React.FC<InspectorSidebarProps> = ({
  selectedRange,
  waveformPitch,
  onExtractMap,
  checksumStatus,
  checksumAlgorithmName,
  checksumNotice,
  onFixChecksum,
  romLoaded,
  romSize,
  romLabel,
  hashes,
  undoStatus,
  selectedMap,
  onEditMapProperties,
  onViewInHex,
  onRevertMapToOriginal,
  onClose,
  animState = 'open',
  desktopWidth = 272,
  isResizing = false,
  collapsePreview = false,
}) => {
  const romData = useMapStore((s) => s.romData);
  const originalRomData = useMapStore((s) => s.originalRomData);
  const deltaLedger = useMemo(
    () => computeRomDeltaLedger(romData, originalRomData, 12),
    [romData, originalRomData]
  );

  const animClass =
    animState === 'closing'
      ? 'kivi-anim-slide-right-out'
      : animState === 'swap'
        ? 'kivi-anim-fade-in'
        : 'kivi-anim-slide-right-in';

  return (
  <aside
    data-testid="drawer-inspector"
    style={{ '--kivi-panel-w': `${desktopWidth}px` } as React.CSSProperties}
    className={`absolute inset-y-0 right-0 z-40 flex w-[min(85vw,16rem)] flex-col overflow-hidden rounded-none kivi-panel-surface bg-kiviSurface lg:static lg:inset-y-auto lg:right-auto lg:z-auto lg:w-[var(--kivi-panel-w)] lg:shrink-0 lg:shadow-none lg:border-l lg:border-kiviBorder/60 ${animClass} ${
      collapsePreview ? 'opacity-55' : 'opacity-100'
    } ${isResizing ? 'transition-none' : 'transition-opacity duration-150'}`}
  >
    <div
      data-testid="panel-titlebar-inspector"
      className="flex h-9 shrink-0 items-center justify-between gap-2 bg-kiviSurface px-3 text-xs font-semibold uppercase tracking-wider text-kiviSecondary"
    >
      <div className="flex min-w-0 items-center gap-1.5">
        <Info className="h-3.5 w-3.5 shrink-0 text-kiviAccent" />
        <span className="truncate whitespace-nowrap">Inspector</span>
      </div>
      {onClose && (
        <button
          type="button"
          aria-label="Close inspector panel"
          onClick={onClose}
          className="flex h-6 w-6 shrink-0 items-center justify-center rounded text-kiviMuted transition-colors hover:bg-kiviElevated hover:text-kiviInk"
        >
          <X className="h-3.5 w-3.5" />
        </button>
      )}
    </div>

    <div className="flex-1 space-y-3 overflow-y-auto p-3 text-xs">
      <div className="rounded-md border border-kiviBorder/60 bg-kiviBg p-2.5">
        <h3 className="mb-1.5 text-[11px] font-semibold uppercase tracking-wide text-kiviSecondary">
          ROM Project
        </h3>
        <div className="space-y-1 text-[11px]">
          <div className="flex justify-between gap-2">
            <span className="text-kiviMuted">Document:</span>
            <span className="truncate font-medium text-kiviInk">{romLabel ?? 'None'}</span>
          </div>
          <div className="flex justify-between gap-2 tabular-nums">
            <span className="text-kiviMuted">Size:</span>
            <span className="font-mono text-kiviInk">
              {romLoaded ? `${romSize.toLocaleString()} B` : '-'}
            </span>
          </div>
          {hashes && (
            <div className="flex justify-between gap-2 tabular-nums">
              <span className="text-kiviMuted">SHA-256:</span>
              <span className="font-mono text-kiviSecondary" title={hashes.sha256}>
                {hashes.sha256.slice(0, 12)}...
              </span>
            </div>
          )}
          {undoStatus && (
            <div className="flex justify-between gap-2 tabular-nums">
              <span className="text-kiviMuted">Undo / Redo:</span>
              <span className="font-mono text-kiviInk">
                {undoStatus.undo_count} / {undoStatus.redo_count}
              </span>
            </div>
          )}
        </div>
      </div>

      <div className="rounded-md border border-kiviBorder/60 bg-kiviBg p-2.5">
        <h3 className="mb-1.5 flex items-center gap-1 text-[11px] font-semibold uppercase tracking-wide text-kiviSecondary">
          <Crosshair className="h-3 w-3 text-kiviAccent" />
          <span>Waveform Selection</span>
        </h3>
        {selectedRange ? (
          <div className="space-y-1 font-mono text-[11px] tabular-nums">
            <div className="flex justify-between">
              <span className="text-kiviMuted">Start:</span>
              <span className="text-kiviAccent">0x{selectedRange.start.toString(16).toUpperCase()}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-kiviMuted">End:</span>
              <span className="text-kiviAccent">0x{selectedRange.end.toString(16).toUpperCase()}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-kiviMuted">Bytes:</span>
              <span className="text-kiviInk">{selectedRange.end - selectedRange.start}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-kiviMuted">Pitch:</span>
              <span className="text-kiviInk">{waveformPitch}</span>
            </div>
            <button
              type="button"
              onClick={onExtractMap}
              className="mt-2 w-full rounded border border-kiviAccent/40 bg-kiviAccent/15 py-1 text-[11px] font-semibold text-kiviAccent transition-colors hover:bg-kiviAccent/25"
            >
              Extract to Table Map
            </button>
          </div>
        ) : (
          <p className="text-[11px] text-kiviMuted">
            Drag across the 2D waveform to mark a map block.
          </p>
        )}
      </div>

      <div className="rounded-md border border-kiviBorder/60 bg-kiviBg p-2.5">
        <h3 className="mb-1.5 flex items-center gap-1 text-[11px] font-semibold uppercase tracking-wide text-kiviSecondary">
          {checksumStatus === 'valid' ? (
            <ShieldCheck className="h-3.5 w-3.5 text-kiviGreen" />
          ) : (
            <ShieldAlert className="h-3.5 w-3.5 text-kiviWarn" />
          )}
          <span>Checksum Health</span>
        </h3>
        <div className="space-y-1 text-[11px]">
          <div className="flex items-center justify-between gap-2">
            <span className="text-kiviMuted">Algorithm:</span>
            <span className="truncate font-mono text-[10px] text-kiviInk">
              {checksumAlgorithmName ?? 'Auto'}
            </span>
          </div>
          <div className="flex items-center justify-between">
            <span className="text-kiviMuted">Status:</span>
            <span
              className={`font-semibold ${
                checksumStatus === 'valid'
                  ? 'text-kiviGreen'
                  : checksumStatus === 'invalid'
                    ? 'text-kiviRed'
                    : 'text-kiviMuted'
              }`}
            >
              {checksumStatus ? checksumStatus.toUpperCase() : 'UNCHECKED'}
            </span>
          </div>
          {checksumNotice && (
            <p className="text-[10px] leading-snug text-kiviMuted">{checksumNotice}</p>
          )}
          {romLoaded && (
            <button
              type="button"
              onClick={onFixChecksum}
              className="mt-1.5 w-full rounded border border-kiviBorder/70 bg-kiviElevated py-1 text-[11px] text-kiviInk transition-colors hover:border-kiviAccent"
            >
              Recalculate &amp; Repair
            </button>
          )}
        </div>
      </div>

      <div
        data-testid="inspector-delta-ledger"
        className="rounded-md border border-kiviBorder/60 bg-kiviBg p-2.5"
      >
        <div className="mb-1.5 flex items-center justify-between gap-2">
          <h3 className="flex items-center gap-1 text-[11px] font-semibold uppercase tracking-wide text-kiviSecondary">
            <GitCommit className="h-3.5 w-3.5 text-kiviAccent" />
            <span>Change Ledger</span>
          </h3>
          <span
            className={`font-mono text-[10px] font-semibold tabular-nums ${
              deltaLedger.totalChangedBytes > 0 ? 'text-kiviGreen' : 'text-kiviMuted'
            }`}
          >
            {romLoaded ? `${deltaLedger.totalChangedBytes} B` : '--'}
          </span>
        </div>

        {!romLoaded ? (
          <p className="text-[10px] text-kiviMuted">Load a ROM to track edits vs original baseline.</p>
        ) : deltaLedger.totalChangedBytes === 0 ? (
          <p className="text-[10px] text-kiviMuted">Matches Stock baseline (0 B modified).</p>
        ) : (
          <div className="space-y-1">
            <p className="text-[10px] text-kiviSecondary tabular-nums">
              {deltaLedger.totalClusterCount} modified region(s) vs original:
            </p>
            <div className="max-h-36 space-y-1 overflow-y-auto pr-0.5 font-mono text-[10px] tabular-nums">
              {deltaLedger.clusters.map((cluster) => (
                <button
                  key={cluster.start}
                  type="button"
                  onClick={() => onViewInHex?.(cluster.start)}
                  disabled={!onViewInHex}
                  title="Jump to modified offset in Hex viewer"
                  className="flex w-full items-center justify-between gap-1.5 rounded border border-kiviBorder bg-kiviSurface px-2 py-1 text-left transition-colors hover:border-kiviAccent/60 hover:bg-kiviElevated"
                >
                  <span className="truncate text-kiviAccent">
                    0x{cluster.start.toString(16).toUpperCase().padStart(4, '0')}
                    {cluster.end > cluster.start
                      ? `..0x${cluster.end.toString(16).toUpperCase().padStart(4, '0')}`
                      : ''}
                  </span>
                  <span className="shrink-0 text-kiviGreen">
                    {cluster.changedBytes}B ({cluster.firstOld.toString(16).toUpperCase().padStart(2, '0')}
                    &rarr;
                    {cluster.firstNew.toString(16).toUpperCase().padStart(2, '0')})
                  </span>
                </button>
              ))}
            </div>
            {deltaLedger.truncated && (
              <p className="text-[9px] text-kiviMuted">Showing first 12 modified regions.</p>
            )}
          </div>
        )}
      </div>

      {selectedMap && (
        <div className="rounded-md border border-kiviBorder/60 bg-kiviBg p-2.5">
          <h3 className="mb-1.5 text-[11px] font-semibold uppercase tracking-wide text-kiviSecondary">
            Selected Map
          </h3>
          <div className="space-y-1 font-mono text-[11px] tabular-nums">
            <div className="flex justify-between">
              <span className="text-kiviMuted">Offset:</span>
              <span className="text-kiviInk">0x{selectedMap.address.toString(16).toUpperCase()}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-kiviMuted">Dims:</span>
              <span className="text-kiviInk">
                {selectedMap.rows} &times; {selectedMap.cols}
              </span>
            </div>
            <div className="flex justify-between">
              <span className="text-kiviMuted">Format:</span>
              <span className="text-kiviInk">
                {selectedMap.dtype.toUpperCase()} / {selectedMap.endian}
              </span>
            </div>
            <div className="flex justify-between gap-2">
              <span className="text-kiviMuted">Scaling:</span>
              <span className="truncate text-kiviInk">{selectedMap.scaling.equation || 'X'}</span>
            </div>
          </div>

          <div className="mt-2.5 grid grid-cols-1 gap-1.5 border-t border-kiviBorder pt-2">
            {onEditMapProperties && (
              <button
                type="button"
                onClick={() => onEditMapProperties(selectedMap)}
                className="flex items-center justify-center gap-1 rounded border border-kiviBorder/70 bg-kiviElevated py-1 text-[11px] text-kiviInk hover:border-kiviAccent"
              >
                <Pencil className="h-3 w-3 text-kiviAccent" />
                <span>Edit Map Properties</span>
              </button>
            )}
            {onViewInHex && (
              <button
                type="button"
                onClick={() => onViewInHex(selectedMap.address)}
                className="flex items-center justify-center gap-1 rounded border border-kiviBorder/70 bg-kiviElevated py-1 text-[11px] text-kiviInk hover:border-kiviAccent"
              >
                <Binary className="h-3 w-3 text-kiviAccent" />
                <span>View in Hex</span>
              </button>
            )}
            {onRevertMapToOriginal && (
              <button
                type="button"
                onClick={onRevertMapToOriginal}
                className="flex items-center justify-center gap-1 rounded border border-kiviWarn/40 bg-kiviWarn/10 py-1 text-[11px] text-kiviWarn hover:bg-kiviWarn/20"
              >
                <RotateCcw className="h-3 w-3" />
                <span>Revert Map to Stock</span>
              </button>
            )}
          </div>
        </div>
      )}
    </div>
  </aside>
  );
};
