import React, { useState } from 'react';
import { ChevronRight, Crosshair, ShieldCheck, SlidersHorizontal } from 'lucide-react';
import {
  formatChecksumValue,
  CHECKSUM_ALGORITHMS,
  type ChecksumAlgorithmId,
  type ChecksumPanelState,
} from '../checksumPanel';
import type { SmoothingPreviewState } from '../smoothingPreview';
import { HorizontalScrollStrip } from '../../layout/HorizontalScrollStrip';
import {
  describeInterpolation,
  describeAxisDomain,
  type AxisSource,
  type MapInterpolationState,
} from '../mapInterpolation';

type AxisDomain = { values: number[]; source: AxisSource };
type AnalysisToolTab = 'checksum' | 'smoothing' | 'interpolate';

interface HeatmapAnalysisBarProps {
  showChecksum: boolean;
  hasChecksumRom: boolean;
  checksumAlgorithm: ChecksumAlgorithmId;
  onChangeChecksumAlgorithm: (algo: ChecksumAlgorithmId) => void;
  checksumState: ChecksumPanelState | null;
  checksumPending: boolean;
  canComputeChecksum: boolean;
  onComputeChecksum: () => void;
  enableSmoothingPreview: boolean;
  previewEnabled: boolean;
  previewPending: boolean;
  previewAlpha: number;
  previewState: SmoothingPreviewState | null;
  onTogglePreview: () => void;
  onAlphaChange: (alpha: number) => void;
  showInterpolation: boolean;
  interpXAxis: AxisDomain;
  interpYAxis: AxisDomain;
  interpX: string;
  interpY: string;
  onChangeInterpX: (v: string) => void;
  onChangeInterpY: (v: string) => void;
  interpPending: boolean;
  interpState: MapInterpolationState | null;
  onRunInterpolation: () => void;
}

export const HeatmapAnalysisBar: React.FC<HeatmapAnalysisBarProps> = ({
  showChecksum,
  hasChecksumRom,
  checksumAlgorithm,
  onChangeChecksumAlgorithm,
  checksumState,
  checksumPending,
  canComputeChecksum,
  onComputeChecksum,
  enableSmoothingPreview,
  previewEnabled,
  previewPending,
  previewAlpha,
  previewState,
  onTogglePreview,
  onAlphaChange,
  showInterpolation,
  interpXAxis,
  interpYAxis,
  interpX,
  interpY,
  onChangeInterpX,
  onChangeInterpY,
  interpPending,
  interpState,
  onRunInterpolation,
}) => {
  const [activeTool, setActiveTool] = useState<AnalysisToolTab | null>(null);

  if (!showChecksum && !enableSmoothingPreview && !showInterpolation) {
    return null;
  }

  const toggleTool = (tool: AnalysisToolTab) => {
    setActiveTool((cur) => (cur === tool ? null : tool));
  };

  const checksumOutputText = !hasChecksumRom
    ? 'No ROM loaded'
    : checksumState?.kind === 'ok'
      ? `${formatChecksumValue(checksumState.value, checksumState.widthBits)} (${checksumState.widthBits}-bit, ${checksumState.inputLen} B)`
      : checksumState?.kind === 'unsupported'
        ? `${checksumState.algorithm} is not implemented by the backend`
        : checksumState?.kind === 'error'
          ? `Error: ${checksumState.message}`
          : null;

  const smoothingOutputText = previewPending
    ? 'Computing smoothing preview...'
    : previewState?.kind === 'invalid-alpha' ||
        previewState?.kind === 'no-map' ||
        previewState?.kind === 'error'
      ? previewState.message
      : previewState?.kind === 'ok'
        ? `max |curvature| ${previewState.report.max_abs_curvature.toFixed(1)} at (${previewState.report.max_curvature_row},${previewState.report.max_curvature_col})`
        : null;

  const interpOutputText =
    interpState?.kind === 'ok'
      ? describeInterpolation(interpState.result)
      : interpState
        ? interpState.message
        : null;

  const collapsedSummary =
    (checksumState?.kind === 'ok' && `CHK: ${formatChecksumValue(checksumState.value, checksumState.widthBits)}`) ||
    (previewEnabled && previewState?.kind === 'ok' && `Smooth α=${previewAlpha.toFixed(2)}`) ||
    (interpState?.kind === 'ok' && describeInterpolation(interpState.result)) ||
    null;

  return (
    <div
      data-testid="heatmap-analysis-bar"
      className="shrink-0 border-b border-kiviBorder/60 bg-kiviBg/80 px-2.5 text-[11px]"
    >
      {/* Compact 1-Row Scrollable Pill Strip (locked h-8 height) */}
      <div className="flex h-8 shrink-0 items-center justify-between gap-2 overflow-hidden">
        <HorizontalScrollStrip
          ariaLabel="Heatmap analysis tools"
          stripTestId="heatmap-analysis-strip"
          depKey={`${activeTool ?? ''}:${showChecksum}:${enableSmoothingPreview}:${showInterpolation}:${previewEnabled}`}
          className="h-8 min-w-0 flex-1"
          stripClassName="gap-1.5"
          arrowSize="xs"
          scrollLeftLabel="Scroll analysis tools left"
          scrollRightLabel="Scroll analysis tools right"
        >
          <span className="mr-0.5 shrink-0 whitespace-nowrap text-[10px] font-semibold uppercase tracking-wider text-kiviMuted">
            Tools
          </span>

          {showChecksum && (
            <button
              type="button"
              aria-expanded={activeTool === 'checksum'}
              onClick={() => toggleTool('checksum')}
              className={`inline-flex h-6 shrink-0 items-center gap-1 whitespace-nowrap rounded-md border px-2 text-[11px] font-medium transition-colors ${
                activeTool === 'checksum'
                  ? 'border-kiviAccent/60 bg-kiviAccent/15 text-kiviAccent'
                  : 'border-kiviBorder/70 bg-kiviSurface text-kiviSecondary hover:text-kiviInk'
              }`}
            >
              <ShieldCheck className="h-3 w-3 shrink-0" />
              <span>Checksum</span>
              <ChevronRight
                className={`h-3 w-3 shrink-0 opacity-75 transition-transform duration-150 ${
                  activeTool === 'checksum' ? 'rotate-90' : ''
                }`}
              />
            </button>
          )}

          {enableSmoothingPreview && (
            <button
              type="button"
              aria-expanded={activeTool === 'smoothing'}
              onClick={() => toggleTool('smoothing')}
              className={`inline-flex h-6 shrink-0 items-center gap-1 whitespace-nowrap rounded-md border px-2 text-[11px] font-medium transition-colors ${
                activeTool === 'smoothing' || previewEnabled
                  ? 'border-kiviAccent/60 bg-kiviAccent/15 text-kiviAccent'
                  : 'border-kiviBorder/70 bg-kiviSurface text-kiviSecondary hover:text-kiviInk'
              }`}
            >
              <SlidersHorizontal className="h-3 w-3 shrink-0" />
              <span>Smoothing</span>
              {previewEnabled && (
                <span className="h-1.5 w-1.5 rounded-full bg-kiviAccent" aria-hidden="true" />
              )}
              <ChevronRight
                className={`h-3 w-3 shrink-0 opacity-75 transition-transform duration-150 ${
                  activeTool === 'smoothing' ? 'rotate-90' : ''
                }`}
              />
            </button>
          )}

          {showInterpolation && (
            <button
              type="button"
              aria-expanded={activeTool === 'interpolate'}
              onClick={() => toggleTool('interpolate')}
              className={`inline-flex h-6 shrink-0 items-center gap-1 whitespace-nowrap rounded-md border px-2 text-[11px] font-medium transition-colors ${
                activeTool === 'interpolate'
                  ? 'border-kiviAccent/60 bg-kiviAccent/15 text-kiviAccent'
                  : 'border-kiviBorder/70 bg-kiviSurface text-kiviSecondary hover:text-kiviInk'
              }`}
            >
              <Crosshair className="h-3 w-3 shrink-0" />
              <span>Interpolate</span>
              <ChevronRight
                className={`h-3 w-3 shrink-0 opacity-75 transition-transform duration-150 ${
                  activeTool === 'interpolate' ? 'rotate-90' : ''
                }`}
              />
            </button>
          )}
        </HorizontalScrollStrip>

        {!activeTool && collapsedSummary && (
          <span className="max-w-[180px] shrink-0 truncate font-mono text-[10px] text-kiviSecondary tabular-nums">
            {collapsedSummary}
          </span>
        )}
      </div>

      {/* Expanded Checksum Drawer */}
      {activeTool === 'checksum' && showChecksum && (
        <div className="kivi-anim-reveal mt-2 space-y-1.5 rounded-md border border-kiviBorder/60 bg-kiviSurface/80 p-2">
          <div className="flex flex-wrap items-center gap-2">
            <span className="shrink-0 whitespace-nowrap font-semibold uppercase tracking-wide text-kiviMuted">
              Algorithm
            </span>
            <select
              aria-label="Checksum algorithm"
              value={checksumAlgorithm}
              onChange={(e) => onChangeChecksumAlgorithm(e.target.value as ChecksumAlgorithmId)}
              className="shrink-0 whitespace-nowrap rounded-md border border-kiviBorder/70 bg-kiviBg px-2 py-1 text-kiviInk"
            >
              {CHECKSUM_ALGORITHMS.map((a) => (
                <option key={a.id} value={a.id}>
                  {a.label}
                </option>
              ))}
            </select>
            <button
              type="button"
              data-testid="analysis-compute-btn"
              onClick={onComputeChecksum}
              disabled={!canComputeChecksum}
              className="min-w-[96px] shrink-0 whitespace-nowrap rounded-md border border-kiviBorder/70 bg-kiviElevated px-2.5 py-1 text-center font-medium text-kiviInk hover:border-kiviAccent/60 disabled:opacity-40"
            >
              {checksumPending ? 'Computing...' : 'Compute'}
            </button>
          </div>
          {checksumOutputText && (
            <div
              data-testid="analysis-checksum-output"
              aria-live="polite"
              className="w-full break-words rounded-lg border border-kiviBorder/50 bg-kiviBg px-2.5 py-1 font-mono text-[11px] text-kiviSecondary tabular-nums"
            >
              {checksumOutputText}
            </div>
          )}
        </div>
      )}

      {/* Expanded Smoothing Drawer */}
      {activeTool === 'smoothing' && enableSmoothingPreview && (
        <div className="kivi-anim-reveal mt-2 space-y-1.5 rounded-md border border-kiviBorder/60 bg-kiviSurface/80 p-2">
          <div className="flex flex-wrap items-center gap-2.5">
            <button
              type="button"
              data-testid="analysis-preview-btn"
              onClick={onTogglePreview}
              disabled={previewPending}
              className={`min-w-[108px] shrink-0 whitespace-nowrap rounded-md border px-2.5 py-1 text-center font-medium transition-colors disabled:opacity-40 ${
                previewEnabled
                  ? 'border-kiviAccent/60 bg-kiviAccent/15 text-kiviAccent'
                  : 'border-kiviBorder/70 bg-kiviElevated text-kiviInk hover:border-kiviAccent/60'
              }`}
            >
              {previewEnabled ? 'Hide preview' : 'Preview'}
            </button>
            <label className="flex shrink-0 items-center gap-1.5 whitespace-nowrap text-kiviMuted">
              <span>α</span>
              <input
                type="range"
                aria-label="Smoothing alpha"
                min={0.05}
                max={0.95}
                step={0.05}
                value={previewAlpha}
                onChange={(e) => onAlphaChange(Number(e.target.value))}
                className="w-24 accent-kiviAccent"
              />
              <span className="w-8 font-mono text-kiviInk tabular-nums">
                {previewAlpha.toFixed(2)}
              </span>
            </label>
          </div>
          {smoothingOutputText && (
            <div
              data-testid="analysis-smoothing-output"
              aria-live="polite"
              className="w-full break-words rounded-lg border border-kiviBorder/50 bg-kiviBg px-2.5 py-1 font-mono text-[11px] text-kiviSecondary tabular-nums"
            >
              {smoothingOutputText}
            </div>
          )}
        </div>
      )}

      {/* Expanded Interpolate Drawer */}
      {activeTool === 'interpolate' && showInterpolation && (
        <div className="kivi-anim-reveal mt-2 space-y-1.5 rounded-md border border-kiviBorder/60 bg-kiviSurface/80 p-2">
          <div className="flex flex-wrap items-center gap-2">
            <label className="flex shrink-0 items-center gap-1.5 whitespace-nowrap text-kiviMuted">
              <span
                className="whitespace-nowrap"
                title={`X axis domain: ${describeAxisDomain(interpXAxis)}`}
              >
                X ({describeAxisDomain(interpXAxis)})
              </span>
              <input
                type="text"
                inputMode="decimal"
                aria-label="Interpolation X"
                value={interpX}
                onChange={(e) => onChangeInterpX(e.target.value)}
                className="w-20 shrink-0 rounded-md border border-kiviBorder/70 bg-kiviBg px-2 py-1 font-mono text-kiviInk"
              />
            </label>
            <label className="flex shrink-0 items-center gap-1.5 whitespace-nowrap text-kiviMuted">
              <span
                className="whitespace-nowrap"
                title={`Y axis domain: ${describeAxisDomain(interpYAxis)}`}
              >
                Y ({describeAxisDomain(interpYAxis)})
              </span>
              <input
                type="text"
                inputMode="decimal"
                aria-label="Interpolation Y"
                value={interpY}
                onChange={(e) => onChangeInterpY(e.target.value)}
                className="w-20 shrink-0 rounded-md border border-kiviBorder/70 bg-kiviBg px-2 py-1 font-mono text-kiviInk"
              />
            </label>
            <button
              type="button"
              data-testid="analysis-query-btn"
              onClick={onRunInterpolation}
              disabled={interpPending || interpX === '' || interpY === ''}
              className="min-w-[84px] shrink-0 whitespace-nowrap rounded-md border border-kiviBorder/70 bg-kiviElevated px-2.5 py-1 text-center font-medium text-kiviInk hover:border-kiviAccent/60 disabled:opacity-40"
            >
              {interpPending ? 'Querying...' : 'Query'}
            </button>
          </div>
          {interpOutputText && (
            <div
              data-testid="analysis-interp-output"
              aria-live="polite"
              className="w-full break-words rounded-lg border border-kiviBorder/50 bg-kiviBg px-2.5 py-1 font-mono text-[11px] text-kiviSecondary tabular-nums"
            >
              {interpOutputText}
            </div>
          )}
        </div>
      )}
    </div>
  );
};
