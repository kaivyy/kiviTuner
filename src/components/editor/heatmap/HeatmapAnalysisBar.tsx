import React from 'react';
import {
  formatChecksumValue,
  CHECKSUM_ALGORITHMS,
  type ChecksumAlgorithmId,
  type ChecksumPanelState,
} from '../checksumPanel';
import type { SmoothingPreviewState } from '../smoothingPreview';
import {
  describeInterpolation,
  describeAxisDomain,
  type AxisSource,
  type MapInterpolationState,
} from '../mapInterpolation';

type AxisDomain = { values: number[]; source: AxisSource };

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
  if (!showChecksum && !enableSmoothingPreview && !showInterpolation) {
    return null;
  }

  return (
    <div className="flex flex-wrap items-center gap-3 border-b border-kiviBorder bg-kiviBg px-3 py-1.5 text-[11px]">
      {showChecksum && (
        <div className="flex items-center gap-1.5">
          <span className="font-semibold uppercase tracking-wide text-kiviMuted">Checksum</span>
          <select
            aria-label="Checksum algorithm"
            value={checksumAlgorithm}
            onChange={(e) => onChangeChecksumAlgorithm(e.target.value as ChecksumAlgorithmId)}
            className="rounded border border-kiviBorder bg-kiviSurface px-1.5 py-0.5 text-kiviSecondary"
          >
            {CHECKSUM_ALGORITHMS.map((a) => (
              <option key={a.id} value={a.id}>
                {a.label}
              </option>
            ))}
          </select>
          <button
            type="button"
            onClick={onComputeChecksum}
            disabled={!canComputeChecksum}
            className="rounded border border-kiviBorder bg-kiviElevated px-2 py-0.5 text-kiviInk hover:bg-kiviBorderStrong disabled:opacity-40"
          >
            {checksumPending ? 'Computing…' : 'Compute'}
          </button>
          <span className="font-mono text-kiviMuted" aria-live="polite">
            {!hasChecksumRom && 'No ROM loaded'}
            {hasChecksumRom && checksumState?.kind === 'ok' &&
              `${formatChecksumValue(checksumState.value, checksumState.widthBits)} (${checksumState.widthBits}-bit, ${checksumState.inputLen} B)`}
            {hasChecksumRom && checksumState?.kind === 'unsupported' &&
              `${checksumState.algorithm} is not implemented by the backend`}
            {hasChecksumRom && checksumState?.kind === 'error' && `Error: ${checksumState.message}`}
          </span>
        </div>
      )}

      {enableSmoothingPreview && (
        <div className="flex items-center gap-1.5">
          <span className="font-semibold uppercase tracking-wide text-kiviMuted">Smoothing</span>
          <button
            type="button"
            onClick={onTogglePreview}
            disabled={previewPending}
            className={`rounded border px-2 py-0.5 transition-colors disabled:opacity-40 ${
              previewEnabled
                ? 'border-kiviAccent bg-kiviAccent/20 text-kiviAccent'
                : 'border-kiviBorder bg-kiviElevated text-kiviInk hover:bg-kiviBorderStrong'
            }`}
          >
            {previewEnabled ? 'Hide preview' : 'Preview'}
          </button>
          <label className="flex items-center gap-1 text-kiviMuted">
            <span>α</span>
            <input
              type="range"
              aria-label="Smoothing alpha"
              min={0.05}
              max={0.95}
              step={0.05}
              value={previewAlpha}
              onChange={(e) => onAlphaChange(Number(e.target.value))}
              className="w-20 accent-kiviAccent"
            />
            <span className="w-8 font-mono">{previewAlpha.toFixed(2)}</span>
          </label>
          <span className="text-kiviMuted" aria-live="polite">
            {previewPending && 'Computing…'}
            {!previewPending && previewState?.kind === 'invalid-alpha' && previewState.message}
            {!previewPending && previewState?.kind === 'no-map' && previewState.message}
            {!previewPending && previewState?.kind === 'error' && previewState.message}
            {!previewPending && previewState?.kind === 'ok' &&
              `max |curvature| ${previewState.report.max_abs_curvature.toFixed(1)} at (${previewState.report.max_curvature_row},${previewState.report.max_curvature_col})`}
          </span>
        </div>
      )}

      {showInterpolation && (
        <div className="flex items-center gap-1.5">
          <span className="font-semibold uppercase tracking-wide text-kiviMuted">Interpolate</span>
          <label className="flex items-center gap-1 text-kiviMuted">
            <span title={`X axis domain: ${describeAxisDomain(interpXAxis)}`}>
              X ({describeAxisDomain(interpXAxis)})
            </span>
            <input
              type="text"
              inputMode="decimal"
              aria-label="Interpolation X"
              value={interpX}
              onChange={(e) => onChangeInterpX(e.target.value)}
              className="w-16 rounded border border-kiviBorder bg-kiviSurface px-1 py-0.5 font-mono text-kiviInk"
            />
          </label>
          <label className="flex items-center gap-1 text-kiviMuted">
            <span title={`Y axis domain: ${describeAxisDomain(interpYAxis)}`}>
              Y ({describeAxisDomain(interpYAxis)})
            </span>
            <input
              type="text"
              inputMode="decimal"
              aria-label="Interpolation Y"
              value={interpY}
              onChange={(e) => onChangeInterpY(e.target.value)}
              className="w-16 rounded border border-kiviBorder bg-kiviSurface px-1 py-0.5 font-mono text-kiviInk"
            />
          </label>
          <button
            type="button"
            onClick={onRunInterpolation}
            disabled={interpPending || interpX === '' || interpY === ''}
            className="rounded border border-kiviBorder bg-kiviElevated px-2 py-0.5 text-kiviInk hover:bg-kiviBorderStrong disabled:opacity-40"
          >
            {interpPending ? 'Querying…' : 'Query'}
          </button>
          <span className="font-mono text-kiviMuted" aria-live="polite">
            {interpState?.kind === 'no-map' && interpState.message}
            {interpState?.kind === 'invalid-coordinate' && interpState.message}
            {interpState?.kind === 'degenerate' && interpState.message}
            {interpState?.kind === 'dimension-mismatch' && interpState.message}
            {interpState?.kind === 'error' && interpState.message}
            {interpState?.kind === 'ok' && describeInterpolation(interpState.result)}
          </span>
        </div>
      )}
    </div>
  );
};
