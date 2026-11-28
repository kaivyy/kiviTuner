import React, { useEffect, useRef, useState } from 'react';
import { Layers, Plus, Radar, Search, X } from 'lucide-react';
import type {
  DetectedMapCandidate,
  DiscoveredWindowMap,
  ScanProgressEvent,
} from '../../lib/tauriBridge';
import {
  bridgeBlindAutomapV2Stream,
  bridgeClassifyBlindTables,
  type BlindTable,
  type MapClassification,
} from '../../lib/workbenchBridge';
import { blindTableToMapDef } from '../modals/calibrationWorkbenchController';
import type { MapDef } from '../../types/kivi';
import type { RightDockAnimState } from './rightDockModel';

type ScannerTab = 'blind_v2' | 'heuristics' | 'window';

interface ClassifiedBlindResult {
  table: BlindTable;
  classification: MapClassification | null;
}

interface ScannerDrawerProps {
  isOpen?: boolean;
  animState?: RightDockAnimState;
  onClose: () => void;
  isDesktop: boolean;
  romData: Uint8Array | null;
  scannerSensitivity: 'strict' | 'balanced' | 'aggressive';
  onChangeSensitivity: (sens: 'strict' | 'balanced' | 'aggressive') => void;
  isScanning: boolean;
  onRunScanner: () => void;
  scanProgress?: ScanProgressEvent | null;
  hasRomData: boolean;
  isWindowScanning: boolean;
  onRunWindowScanner: () => void;
  windowScanError: string | null;
  windowCandidates: DiscoveredWindowMap[];
  detectedCandidates: DetectedMapCandidate[];
  onLoadCandidate: (candidate: DetectedMapCandidate) => void;
  onAddMaps?: (maps: MapDef[], selectFirst?: boolean) => void;
  onJumpToWaveform: (startAddr: number, endAddr: number) => void;
  desktopWidth?: number;
  isResizing?: boolean;
  collapsePreview?: boolean;
}

export const ScannerDrawer: React.FC<ScannerDrawerProps> = ({
  isOpen = true,
  animState = 'open',
  onClose,
  isDesktop,
  romData,
  scannerSensitivity,
  onChangeSensitivity,
  isScanning,
  onRunScanner,
  scanProgress = null,
  hasRomData,
  isWindowScanning,
  onRunWindowScanner,
  windowScanError,
  windowCandidates,
  detectedCandidates,
  onLoadCandidate,
  onAddMaps,
  onJumpToWaveform,
  desktopWidth = 312,
  isResizing = false,
  collapsePreview = false,
}) => {
  const [activeTab, setActiveTab] = useState<ScannerTab>(isDesktop ? 'blind_v2' : 'heuristics');
  const [isBlindRunning, setIsBlindRunning] = useState(false);
  const [blindProgress, setBlindProgress] = useState<ScanProgressEvent | null>(null);
  const [blindResults, setBlindResults] = useState<ClassifiedBlindResult[]>([]);
  const [blindError, setBlindError] = useState<string | null>(null);
  const blindSeqRef = useRef(0);

  useEffect(() => {
    if (!isOpen || animState === 'closing') return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        onClose();
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [isOpen, animState, onClose]);

  const handleRunBlindV2 = async () => {
    if (!hasRomData) return;
    if (!isDesktop) {
      setBlindError('Blind V2 multi-brand scanner runs in the desktop Rust backend.');
      return;
    }
    const seq = ++blindSeqRef.current;
    setIsBlindRunning(true);
    setBlindError(null);
    setBlindProgress({ stage: 'starting', percent: 5, message: 'Scanning ECU binary...' });

    try {
      const maxTables =
        scannerSensitivity === 'strict' ? 60 : scannerSensitivity === 'aggressive' ? 200 : 120;
      const tables = await bridgeBlindAutomapV2Stream(
        { includeCurves: true, include8bit: true, include32bit: true, maxTables },
        (ev) => {
          if (seq === blindSeqRef.current) {
            setBlindProgress(ev);
          }
        }
      );
      if (seq !== blindSeqRef.current) return;
      if (!tables) {
        setBlindError('Blind V2 returned no result.');
        setBlindResults([]);
        return;
      }
      const classes = await bridgeClassifyBlindTables(tables, romData, null);
      if (seq !== blindSeqRef.current) return;
      const merged: ClassifiedBlindResult[] = tables.map((table, i) => ({
        table,
        classification: classes?.[i] ?? null,
      }));
      setBlindResults(merged);
      setBlindProgress({ stage: 'complete', percent: 100, message: `Detected ${merged.length} table(s)` });
    } catch (err) {
      if (seq === blindSeqRef.current) {
        setBlindError(err instanceof Error ? err.message : String(err));
      }
    } finally {
      if (seq === blindSeqRef.current) setIsBlindRunning(false);
    }
  };

  if (!isOpen && animState !== 'closing') return null;

  const animClass =
    animState === 'closing'
      ? 'kivi-anim-slide-right-out'
      : animState === 'swap'
        ? 'kivi-anim-fade-in'
        : 'kivi-anim-slide-right-in';

  return (
    <aside
      data-testid="drawer-scanner"
      aria-label="ROM Scanner Drawer"
      style={{ '--kivi-panel-w': `${desktopWidth}px` } as React.CSSProperties}
      className={`absolute inset-y-0 right-0 z-40 flex w-[min(92vw,21rem)] flex-col overflow-hidden rounded-none kivi-panel-surface bg-kiviSurface lg:static lg:inset-y-auto lg:right-auto lg:z-auto lg:w-[var(--kivi-panel-w)] lg:shrink-0 lg:shadow-none lg:border-l lg:border-kiviBorder/60 ${animClass} ${
        collapsePreview ? 'opacity-55' : 'opacity-100'
      } ${isResizing ? 'transition-none' : 'transition-opacity duration-150'}`}
    >
      <div
        data-testid="panel-titlebar-scanner"
        className="flex h-9 shrink-0 items-center justify-between gap-2 bg-kiviSurface px-3 text-xs font-semibold uppercase tracking-wider text-kiviSecondary"
      >
        <div className="flex min-w-0 items-center gap-1.5">
          <Radar className="h-3.5 w-3.5 shrink-0 text-kiviAccent" />
          <h2 className="truncate whitespace-nowrap">ROM Discovery Scanner</h2>
        </div>
        <button
          type="button"
          onClick={onClose}
          aria-label="Close scanner"
          className="flex h-6 w-6 shrink-0 items-center justify-center rounded text-kiviMuted transition-colors hover:bg-kiviElevated hover:text-kiviInk"
        >
          <X className="h-3.5 w-3.5" />
        </button>
      </div>

      <div className="space-y-2 border-b border-kiviBorder/50 bg-kiviBg/40 px-3 py-2">
        <div className="grid grid-cols-3 gap-1 rounded border border-kiviBorder bg-kiviBg p-0.5 text-[10px]">
          <button
            type="button"
            onClick={() => setActiveTab('blind_v2')}
            className={`rounded py-1 font-semibold transition-colors ${
              activeTab === 'blind_v2'
                ? 'bg-kiviAccent text-kiviAccentFg'
                : 'text-kiviSecondary hover:text-kiviInk'
            }`}
          >
            Auto-Scan
          </button>
          <button
            type="button"
            onClick={() => setActiveTab('heuristics')}
            className={`rounded py-1 font-semibold transition-colors ${
              activeTab === 'heuristics'
                ? 'bg-kiviAccent text-kiviAccentFg'
                : 'text-kiviSecondary hover:text-kiviInk'
            }`}
          >
            Heuristics
          </button>
          <button
            type="button"
            onClick={() => setActiveTab('window')}
            className={`rounded py-1 font-semibold transition-colors ${
              activeTab === 'window'
                ? 'bg-kiviAccent text-kiviAccentFg'
                : 'text-kiviSecondary hover:text-kiviInk'
            }`}
          >
            16-bit Win
          </button>
        </div>

        <div className="flex items-center justify-between text-xs">
          <label htmlFor="scanner-sensitivity" className="text-[11px] text-kiviSecondary">
            Sensitivity:
          </label>
          <select
            id="scanner-sensitivity"
            value={scannerSensitivity}
            onChange={(e) =>
              onChangeSensitivity(e.target.value as 'strict' | 'balanced' | 'aggressive')
            }
            className="rounded border border-kiviBorder/70 bg-kiviSurface px-2 py-0.5 text-[11px] text-kiviInk"
          >
            <option value="strict">Strict (High Conf)</option>
            <option value="balanced">Balanced</option>
            <option value="aggressive">Aggressive (Low Threshold)</option>
          </select>
        </div>

        {activeTab === 'blind_v2' && (
          <div className="space-y-2">
            <button
              type="button"
              disabled={!hasRomData || isBlindRunning}
              onClick={() => void handleRunBlindV2()}
              className="flex w-full items-center justify-center gap-1.5 rounded bg-kiviAccent py-1.5 text-xs font-semibold text-kiviAccentFg transition-opacity hover:opacity-90 disabled:opacity-50"
            >
              <Layers className="h-3.5 w-3.5" />
              <span>{isBlindRunning ? 'Scanning ROM...' : 'Run Multi-Brand Auto-Scan'}</span>
            </button>
            {blindProgress && (
              <div className="space-y-1 text-[10px]">
                <div className="flex justify-between text-kiviSecondary">
                  <span className="truncate">{blindProgress.message}</span>
                  <span className="font-mono tabular-nums">{blindProgress.percent}%</span>
                </div>
                <div className="h-1.5 w-full overflow-hidden rounded bg-kiviBg">
                  <div
                    className="h-full bg-kiviAccent transition-all"
                    style={{ width: `${Math.min(100, Math.max(0, blindProgress.percent))}%` }}
                  />
                </div>
              </div>
            )}
            {blindError && <p className="text-[10px] text-kiviRed">{blindError}</p>}
            {blindResults.length > 0 && onAddMaps && (
              <button
                type="button"
                onClick={() => {
                  const mapped = blindResults.map((r, i) => blindTableToMapDef(r, i));
                  onAddMaps(mapped, true);
                }}
                className="flex w-full items-center justify-center gap-1 rounded border border-kiviAccent/50 bg-kiviAccent/15 py-1 text-[11px] font-semibold text-kiviAccent hover:bg-kiviAccent/25"
              >
                <Plus className="h-3 w-3" />
                <span>Import All {blindResults.length} Tables to Explorer</span>
              </button>
            )}
          </div>
        )}

        {activeTab === 'heuristics' && (
          <div className="space-y-2">
            <button
              type="button"
              disabled={isScanning || !hasRomData}
              onClick={onRunScanner}
              className="flex w-full items-center justify-center gap-1.5 rounded bg-kiviAccent py-1.5 text-xs font-semibold text-kiviAccentFg transition-opacity hover:opacity-90 disabled:opacity-50"
            >
              <Search className="h-3.5 w-3.5" />
              <span>{isScanning ? 'Scanning Binary...' : 'Run Heuristic Auto-Scan'}</span>
            </button>
            {scanProgress && (
              <div className="space-y-1 text-[10px]">
                <div className="flex justify-between text-kiviSecondary">
                  <span className="truncate">{scanProgress.message}</span>
                  <span className="font-mono tabular-nums">{scanProgress.percent}%</span>
                </div>
                <div className="h-1.5 w-full overflow-hidden rounded bg-kiviBg">
                  <div
                    className="h-full bg-kiviAccent transition-all"
                    style={{ width: `${scanProgress.percent}%` }}
                  />
                </div>
              </div>
            )}
          </div>
        )}

        {activeTab === 'window' && (
          <div className="space-y-1.5">
            <button
              type="button"
              disabled={isWindowScanning || !hasRomData}
              onClick={onRunWindowScanner}
              className="flex w-full items-center justify-center gap-1.5 rounded border border-kiviBorder/70 bg-kiviElevated py-1.5 text-xs font-medium text-kiviInk transition-colors hover:border-kiviAccent/60 disabled:opacity-50"
            >
              <Search className="h-3.5 w-3.5 text-kiviAccent" />
              <span>{isWindowScanning ? 'Scanning windows...' : 'Scan 16-bit windows'}</span>
            </button>
            <p className="text-[10px] leading-tight text-kiviMuted">
              Separate detector - scores are on a different scale and are not comparable with the
              auto-scan results above.
            </p>
            {windowScanError && <p className="text-[10px] text-kiviRed">{windowScanError}</p>}
          </div>
        )}
      </div>

      <div className="flex-1 space-y-2 overflow-y-auto p-3" aria-live="polite">
        {activeTab === 'blind_v2' ? (
          blindResults.length === 0 ? (
            <div className="flex flex-col items-center justify-center p-6 text-center text-xs text-kiviMuted">
              <Layers className="mb-2 h-8 w-8 opacity-40" />
              <p>Run Multi-Brand Auto-Scan to discover 2D maps and 1D curves without an XDF.</p>
            </div>
          ) : (
            blindResults.map((entry, idx) => {
              const t = entry.table;
              const dtypeBadge = t.is_float
                ? 'f32'
                : `${t.is_signed ? 'i' : 'u'}${t.cell_bits}`;
              const addrHex = t.address.toString(16).toUpperCase();
              const label =
                entry.classification?.role_name ??
                (t.is_curve || t.rows <= 1
                  ? `${t.cols}-pt ${dtypeBadge.toUpperCase()} Curve (0x${addrHex})`
                  : `${t.rows}x${t.cols} ${dtypeBadge.toUpperCase()} Map (0x${addrHex})`);
              return (
                <div
                  key={`blind-${t.address}-${idx}`}
                  className="rounded-md border border-kiviBorder bg-kiviBg p-2.5 text-xs hover:border-kiviAccent/50"
                >
                  <div className="mb-1 flex items-start justify-between gap-1">
                    <span className="truncate text-[11px] font-semibold text-kiviInk">{label}</span>
                    <span className="rounded border border-kiviGreen/40 bg-kiviGreen/15 px-1.5 py-0.5 font-mono text-[10px] font-semibold text-kiviGreen">
                      {t.is_curve || t.rows <= 1 ? '1D' : '2D'}
                    </span>
                  </div>
                  <div className="mb-2 font-mono text-[10px] text-kiviMuted tabular-nums">
                    0x{t.address.toString(16).toUpperCase()} | {t.rows}x{t.cols} | {dtypeBadge}{' '}
                    {t.little_endian ? 'LE' : 'BE'}
                  </div>
                  <div className="flex gap-1.5">
                    {onAddMaps && (
                      <button
                        type="button"
                        onClick={() => onAddMaps([blindTableToMapDef(entry, idx)], true)}
                        className="flex-1 rounded border border-kiviAccent/40 bg-kiviAccent/15 py-1 text-[10px] font-semibold text-kiviAccent hover:bg-kiviAccent/25"
                      >
                        Add Map
                      </button>
                    )}
                    <button
                      type="button"
                      onClick={() =>
                        onJumpToWaveform(
                          t.address,
                          t.address + t.rows * t.cols * Math.max(1, Math.floor(t.cell_bits / 8))
                        )
                      }
                      className="rounded border border-kiviBorder bg-kiviSurface px-2 py-1 text-[10px] text-kiviSecondary hover:bg-kiviElevated hover:text-kiviInk"
                    >
                      Waveform
                    </button>
                  </div>
                </div>
              );
            })
          )
        ) : activeTab === 'window' ? (
          windowCandidates.length === 0 ? (
            <div className="flex flex-col items-center justify-center p-6 text-center text-xs text-kiviMuted">
              <Search className="mb-2 h-8 w-8 opacity-40" />
              <p>Run 16-bit window detector to find sliding gradient regions.</p>
            </div>
          ) : (
            windowCandidates.map((c) => (
              <div
                key={`win-${c.offset}-${c.rows}x${c.cols}`}
                className="flex items-center justify-between rounded border border-kiviBorder bg-kiviBg px-2.5 py-2 font-mono text-[10px] text-kiviSecondary tabular-nums"
              >
                <span>
                  0x{c.offset.toString(16).toUpperCase()} | {c.rows}x{c.cols} |{' '}
                  {c.is_little_endian ? 'LE' : 'BE'} | {c.score.toFixed(1)}
                </span>
                <button
                  type="button"
                  onClick={() => onJumpToWaveform(c.offset, c.offset + c.rows * c.cols * 2)}
                  className="rounded border border-kiviBorder bg-kiviSurface px-1.5 py-0.5 text-[10px] hover:text-kiviInk"
                >
                  Jump
                </button>
              </div>
            ))
          )
        ) : detectedCandidates.length === 0 ? (
          <div className="flex flex-col items-center justify-center p-6 text-center text-xs text-kiviMuted">
            <Search className="mb-2 h-8 w-8 opacity-40" />
            <p>No heuristic candidates scanned yet.</p>
          </div>
        ) : (
          detectedCandidates.map((c, idx) => (
            <div
              key={`${c.id}-${c.start_addr}-${idx}`}
              className="rounded-md border border-kiviBorder bg-kiviBg p-2.5 text-xs transition-colors hover:border-kiviAccent/50"
            >
              <div className="mb-1.5 flex items-start justify-between gap-1">
                <span className="truncate text-[11px] font-semibold text-kiviInk" title={c.name_hint}>
                  {c.name_hint}
                </span>
                <span
                  className={`rounded border px-1.5 py-0.5 font-mono text-[10px] font-semibold tabular-nums ${
                    c.confidence >= 80
                      ? 'border-kiviGreen/40 bg-kiviGreen/15 text-kiviGreen'
                      : 'border-kiviWarn/40 bg-kiviWarn/15 text-kiviWarn'
                  }`}
                >
                  {c.confidence}%
                </span>
              </div>

              <div className="mb-2 space-y-0.5 font-mono text-[10px] text-kiviMuted tabular-nums">
                <div className="flex justify-between">
                  <span>Address:</span>
                  <span className="text-kiviSecondary">
                    0x{c.start_addr.toString(16).toUpperCase()}..0x
                    {c.end_addr.toString(16).toUpperCase()}
                  </span>
                </div>
                <div className="flex justify-between">
                  <span>Dimensions:</span>
                  <span className="text-kiviSecondary">
                    {c.rows} x {c.cols} ({c.dtype})
                  </span>
                </div>
              </div>

              <div className="flex items-center gap-1.5 border-t border-kiviBorder pt-1.5">
                <button
                  type="button"
                  onClick={() => onLoadCandidate(c)}
                  className="flex-1 rounded border border-kiviAccent/40 bg-kiviAccent/15 py-1 text-center text-[10px] font-semibold text-kiviAccent transition-colors hover:bg-kiviAccent/25"
                >
                  Load Map
                </button>
                <button
                  type="button"
                  onClick={() => onJumpToWaveform(c.start_addr, c.end_addr)}
                  className="rounded border border-kiviBorder bg-kiviSurface px-2 py-1 text-[10px] text-kiviSecondary transition-colors hover:bg-kiviElevated hover:text-kiviInk"
                >
                  Waveform
                </button>
              </div>
            </div>
          ))
        )}
      </div>
    </aside>
  );
};
