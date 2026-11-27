import React from 'react';
import { Check } from 'lucide-react';

export interface SelectionStats {
  count: number;
  rows: number;
  cols: number;
  min: number;
  max: number;
  avg: number;
}

interface HeatmapFooterBarProps {
  selectionStats: SelectionStats | null;
  decimals: number;
  toastMessage: string | null;
}

export const HeatmapFooterBar: React.FC<HeatmapFooterBarProps> = ({
  selectionStats,
  decimals,
  toastMessage,
}) => (
  <>
    <div className="flex min-h-[30px] flex-wrap items-center justify-between border-t border-kiviBorder bg-kiviSurface px-3 py-1.5 text-xs text-kiviMuted tabular-nums">
      <div className="flex items-center gap-4">
        {selectionStats ? (
          <div className="flex flex-wrap items-center gap-3 font-mono text-[11px] tabular-nums text-kiviSecondary">
            <span className="font-medium text-kiviAccent">
              Selected: {selectionStats.rows}&times;{selectionStats.cols} ({selectionStats.count}{' '}
              cells)
            </span>
            <span className="inline-block min-w-[76px]">Min: {selectionStats.min.toFixed(decimals)}</span>
            <span className="inline-block min-w-[76px]">Avg: {selectionStats.avg.toFixed(decimals)}</span>
            <span className="inline-block min-w-[76px]">Max: {selectionStats.max.toFixed(decimals)}</span>
          </div>
        ) : (
          <span className="text-[11px] text-kiviMuted">Click and drag to select cells</span>
        )}
      </div>

      <div className="flex items-center gap-3 text-[11px] text-kiviMuted">
        <span>
          <kbd className="rounded bg-kiviElevated px-1 py-0.5 font-mono text-[10px] text-kiviSecondary border border-kiviBorder">
            +
          </kbd>{' '}
          /{' '}
          <kbd className="rounded bg-kiviElevated px-1 py-0.5 font-mono text-[10px] text-kiviSecondary border border-kiviBorder">
            -
          </kbd>{' '}
          Step
        </span>
        <span>
          <kbd className="rounded bg-kiviElevated px-1 py-0.5 font-mono text-[10px] text-kiviSecondary border border-kiviBorder">
            S
          </kbd>{' '}
          Smooth
        </span>
        <span>
          <kbd className="rounded bg-kiviElevated px-1 py-0.5 font-mono text-[10px] text-kiviSecondary border border-kiviBorder">
            I
          </kbd>{' '}
          Bilinear
        </span>
        <span>
          <kbd className="rounded bg-kiviElevated px-1 py-0.5 font-mono text-[10px] text-kiviSecondary border border-kiviBorder">
            P
          </kbd>{' '}
          %
        </span>
        <span>
          <kbd className="rounded bg-kiviElevated px-1 py-0.5 font-mono text-[10px] text-kiviSecondary border border-kiviBorder">
            Ctrl+C
          </kbd>
          /
          <kbd className="rounded bg-kiviElevated px-1 py-0.5 font-mono text-[10px] text-kiviSecondary border border-kiviBorder">
            V
          </kbd>
        </span>
      </div>
    </div>

    {toastMessage && (
      <div className="absolute top-12 left-1/2 -translate-x-1/2 z-40 flex items-center gap-2 rounded bg-kiviSurface border border-kiviAccent/60 px-3 py-1.5 text-xs text-kiviInk shadow-xl animate-fade-in">
        <Check className="h-3.5 w-3.5 text-kiviAccent" />
        <span>{toastMessage}</span>
      </div>
    )}
  </>
);
