import React from 'react';
import {
  Plus,
  Minus,
  Percent,
  Copy,
  ClipboardPaste,
  ClipboardList,
  Grid,
  ArrowLeftRight,
  ArrowUpDown,
  SlidersHorizontal,
  RotateCcw,
  Settings2,
  LineChart,
  Footprints,
} from 'lucide-react';
import type { CellDisplayMode, TablePalette } from '../HeatmapTable';

interface HeatmapToolbarProps {
  title: string;
  unit: string;
  numRows: number;
  numCols: number;
  stepSize: number;
  displayMode: CellDisplayMode;
  palette: TablePalette;
  hasBaseMatrix?: boolean;
  is1D?: boolean;
  showSliceChart?: boolean;
  onToggleSliceChart?: () => void;
  hitTraceEnabled?: boolean;
  hitTotalCount?: number;
  onToggleHitTrace?: () => void;
  onResetHitTrace?: () => void;
  onStepAdjust: (delta: number) => void;
  onInterpolate2D: () => void;
  onInterpolateHorizontal: () => void;
  onInterpolateVertical: () => void;
  onSmooth3x3: () => void;
  onOpenPercentModal: () => void;
  onRevertToOriginal?: () => void;
  onEditMapProperties?: () => void;
  onCopyTSV: () => void;
  onPasteTSV: () => void;
  onOpenPasteSpecial: () => void;
  onChangeDisplayMode: (mode: CellDisplayMode) => void;
  onChangePalette: (palette: TablePalette) => void;
}

export const HeatmapToolbar: React.FC<HeatmapToolbarProps> = ({
  title,
  unit,
  numRows,
  numCols,
  stepSize,
  displayMode,
  palette,
  hasBaseMatrix = false,
  is1D = false,
  showSliceChart = false,
  onToggleSliceChart,
  hitTraceEnabled = false,
  hitTotalCount = 0,
  onToggleHitTrace,
  onResetHitTrace,
  onStepAdjust,
  onInterpolate2D,
  onInterpolateHorizontal,
  onInterpolateVertical,
  onSmooth3x3,
  onOpenPercentModal,
  onRevertToOriginal,
  onEditMapProperties,
  onCopyTSV,
  onPasteTSV,
  onOpenPasteSpecial,
  onChangeDisplayMode,
  onChangePalette,
}) => (
  <div className="flex flex-wrap items-center justify-between gap-2 border-b border-kiviBorder bg-kiviElevated/60 px-3 py-2">
    <div className="flex min-w-0 items-center gap-2">
      <Grid className="h-4 w-4 shrink-0 text-kiviAccent" />
      <h3 className="truncate text-sm font-semibold text-kiviInk">{title}</h3>
      <span className="shrink-0 whitespace-nowrap rounded border border-kiviBorder bg-kiviBg px-2 py-0.5 font-mono text-[11px] text-kiviSecondary">
        {unit}
      </span>
      <span className="shrink-0 whitespace-nowrap text-xs text-kiviMuted tabular-nums">
        {numRows} &times; {numCols}
      </span>
    </div>

    <div className="flex flex-wrap items-center gap-1.5 text-xs">
      <div className="flex shrink-0 items-center rounded border border-kiviBorder bg-kiviSurface p-0.5">
        <button
          type="button"
          onClick={() => onStepAdjust(stepSize)}
          title="Increment selection (+ / =)"
          className="flex shrink-0 items-center gap-1 whitespace-nowrap rounded px-2 py-1 text-kiviSecondary hover:bg-kiviElevated hover:text-kiviInk transition-colors"
        >
          <Plus className="h-3.5 w-3.5 shrink-0 text-kiviGreen" />
          <span>+{stepSize}</span>
        </button>
        <button
          type="button"
          onClick={() => onStepAdjust(-stepSize)}
          title="Decrement selection (-)"
          className="flex shrink-0 items-center gap-1 whitespace-nowrap rounded px-2 py-1 text-kiviSecondary hover:bg-kiviElevated hover:text-kiviInk transition-colors"
        >
          <Minus className="h-3.5 w-3.5 shrink-0 text-kiviRed" />
          <span>-{stepSize}</span>
        </button>
      </div>

      <div className="hidden h-4 w-px bg-kiviBorder mx-0.5 sm:block" />

      <div className="flex flex-wrap items-center rounded border border-kiviBorder bg-kiviSurface p-0.5">
        <button
          type="button"
          onClick={onInterpolate2D}
          title="2D Bilinear Interpolate selection [I]"
          className="flex shrink-0 items-center gap-1 whitespace-nowrap rounded px-2 py-1 text-kiviSecondary hover:bg-kiviElevated hover:text-kiviInk transition-colors"
        >
          <Grid className="h-3.5 w-3.5 shrink-0 text-kiviAccent" />
          <span>Bilinear (I)</span>
        </button>
        <button
          type="button"
          onClick={onInterpolateHorizontal}
          title="Horizontal Interpolate row endpoints [H]"
          className="flex shrink-0 items-center gap-1 whitespace-nowrap rounded px-2 py-1 text-kiviSecondary hover:bg-kiviElevated hover:text-kiviInk transition-colors"
        >
          <ArrowLeftRight className="h-3.5 w-3.5 shrink-0 text-kiviAccent" />
          <span>H</span>
        </button>
        <button
          type="button"
          onClick={onInterpolateVertical}
          title="Vertical Interpolate column endpoints [V]"
          className="flex shrink-0 items-center gap-1 whitespace-nowrap rounded px-2 py-1 text-kiviSecondary hover:bg-kiviElevated hover:text-kiviInk transition-colors"
        >
          <ArrowUpDown className="h-3.5 w-3.5 shrink-0 text-kiviAccent" />
          <span>V</span>
        </button>
        <button
          type="button"
          onClick={onSmooth3x3}
          title="Smooth 3x3 Gaussian filter [S]"
          className="flex shrink-0 items-center gap-1 whitespace-nowrap rounded px-2 py-1 text-kiviWarn hover:bg-kiviElevated hover:text-kiviInk transition-colors"
        >
          <SlidersHorizontal className="h-3.5 w-3.5 shrink-0 text-kiviWarn" />
          <span>Smooth 3x3 (S)</span>
        </button>
      </div>

      <div className="hidden h-4 w-px bg-kiviBorder mx-0.5 sm:block" />

      <button
        type="button"
        onClick={onOpenPercentModal}
        title="Adjust selection by percentage [P or %]"
        className="flex shrink-0 items-center gap-1 whitespace-nowrap rounded border border-kiviBorder bg-kiviSurface px-2 py-1 text-kiviSecondary hover:bg-kiviElevated hover:text-kiviInk transition-colors"
      >
        <Percent className="h-3.5 w-3.5 shrink-0 text-kiviWarn" />
        <span>% Adjust</span>
      </button>

      <div className="hidden h-4 w-px bg-kiviBorder mx-0.5 sm:block" />

      <div className="flex shrink-0 items-center rounded border border-kiviBorder bg-kiviBg p-0.5">
        <button
          type="button"
          onClick={onCopyTSV}
          title="Copy selection as TSV [Ctrl+C]"
          className="shrink-0 rounded p-1 text-kiviSecondary transition-colors hover:bg-kiviElevated hover:text-kiviInk"
        >
          <Copy className="h-3.5 w-3.5" />
        </button>
        <button
          type="button"
          onClick={onPasteTSV}
          title="Paste TSV [Ctrl+V]"
          className="shrink-0 rounded p-1 text-kiviSecondary transition-colors hover:bg-kiviElevated hover:text-kiviInk"
        >
          <ClipboardPaste className="h-3.5 w-3.5" />
        </button>
        <button
          type="button"
          onClick={onOpenPasteSpecial}
          title="Paste Special (Add / Multiply) [Ctrl+Shift+V]"
          className="flex shrink-0 items-center gap-1 whitespace-nowrap rounded px-1.5 py-1 text-[11px] text-kiviAccent transition-colors hover:bg-kiviElevated hover:text-kiviInk"
        >
          <ClipboardList className="h-3.5 w-3.5 shrink-0" />
          <span>Special</span>
        </button>
      </div>

      {(onRevertToOriginal || onEditMapProperties || (!is1D && onToggleSliceChart)) && (
        <div className="flex shrink-0 items-center rounded border border-kiviBorder bg-kiviBg p-0.5">
          {!is1D && onToggleSliceChart && (
            <button
              type="button"
              onClick={onToggleSliceChart}
              title="Toggle 2D Row Slice Profile Chart"
              className={`shrink-0 rounded p-1 transition-colors ${
                showSliceChart
                  ? 'bg-kiviAccent/20 text-kiviAccent'
                  : 'text-kiviSecondary hover:bg-kiviElevated hover:text-kiviInk'
              }`}
            >
              <LineChart className="h-3.5 w-3.5" />
            </button>
          )}
          {onToggleHitTrace && (
            <>
              <button
                type="button"
                onClick={onToggleHitTrace}
                title="Toggle live cell hit-trace dwell overlay"
                className={`flex shrink-0 items-center gap-1 whitespace-nowrap rounded px-1.5 py-1 text-[11px] transition-colors ${
                  hitTraceEnabled
                    ? 'bg-kiviAccent/20 text-kiviAccent'
                    : 'text-kiviSecondary hover:bg-kiviElevated hover:text-kiviInk'
                }`}
              >
                <Footprints className="h-3.5 w-3.5 shrink-0" />
                <span>Hit Trail</span>
                {hitTotalCount > 0 && (
                  <span className="font-mono text-[10px] tabular-nums">({hitTotalCount})</span>
                )}
              </button>
              {hitTotalCount > 0 && onResetHitTrace && (
                <button
                  type="button"
                  onClick={onResetHitTrace}
                  title="Clear live hit counts"
                  className="shrink-0 rounded px-1.5 py-1 text-[10px] text-kiviMuted hover:bg-kiviElevated hover:text-kiviInk transition-colors"
                >
                  Clear Hits
                </button>
              )}
            </>
          )}
          {onRevertToOriginal && (
            <button
              type="button"
              disabled={!hasBaseMatrix}
              onClick={onRevertToOriginal}
              title="Revert selected cells to stock baseline [Backspace]"
              className="flex shrink-0 items-center gap-1 whitespace-nowrap rounded px-1.5 py-1 text-[11px] text-kiviWarn transition-colors hover:bg-kiviElevated disabled:opacity-40"
            >
              <RotateCcw className="h-3.5 w-3.5 shrink-0" />
              <span>Revert</span>
            </button>
          )}
          {onEditMapProperties && (
            <button
              type="button"
              onClick={onEditMapProperties}
              title="Edit Map Definition Properties"
              className="flex shrink-0 items-center gap-1 whitespace-nowrap rounded px-1.5 py-1 text-[11px] text-kiviSecondary transition-colors hover:bg-kiviElevated hover:text-kiviInk"
            >
              <Settings2 className="h-3.5 w-3.5 shrink-0" />
              <span>Props</span>
            </button>
          )}
        </div>
      )}

      <div className="mx-0.5 hidden h-4 w-px bg-kiviBorder sm:block" />

      <select
        aria-label="Cell Display Mode"
        value={displayMode}
        onChange={(e) => onChangeDisplayMode(e.target.value as CellDisplayMode)}
        className="cursor-pointer shrink-0 whitespace-nowrap rounded border border-kiviBorderStrong bg-kiviBg px-2 py-1 text-[11px] text-kiviInk focus-visible:outline-2 focus-visible:outline-kiviAccent"
        title="Cell Display Mode"
      >
        <option value="engineering">Engineering</option>
        <option value="raw_hex">Raw Hex</option>
        <option value="raw_dec">Raw Dec</option>
        <option value="delta">Delta vs Base</option>
        <option value="percent_delta">% Delta</option>
      </select>

      <select
        aria-label="Heatmap Palette"
        value={palette}
        onChange={(e) => onChangePalette(e.target.value as TablePalette)}
        className="cursor-pointer shrink-0 whitespace-nowrap rounded border border-kiviBorderStrong bg-kiviBg px-2 py-1 text-[11px] text-kiviInk focus-visible:outline-2 focus-visible:outline-kiviAccent"
        title="Heatmap Palette"
      >
        <option value="classic">Classic</option>
        <option value="thermal">Thermal</option>
        <option value="spectrum">Spectrum</option>
        <option value="monochrome">Monochrome</option>
      </select>
    </div>
  </div>
);
