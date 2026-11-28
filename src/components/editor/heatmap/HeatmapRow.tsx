import React from 'react';
import { getHeatmapColor, getHeatmapTextColor, type ColorPalette } from '../../../lib/colorRamp';
import { getHeatmapCellCrosshairState } from '../../../lib/crosshairModel';
import { getCellHitVisual, type HitTracerState } from '../../../lib/hitTracerModel';
import { formatHeatmapCellValue } from './heatmapClipboard';
import type { CellDisplayMode, SelectionRange } from '../HeatmapTable';

export interface HeatmapRowProps {
  row: number[];
  baseRow?: number[];
  previewRow?: number[];
  rIdx: number;
  yLabel: string | number;
  yAxisName: string;
  isRowInSelection: boolean;
  selectionMinC: number | null;
  selectionMaxC: number | null;
  anchorCol: number | null;
  editingCol: number | null;
  editingValue: string;
  correlatedCol: number | null;
  hitTraceEnabled?: boolean;
  hitTracer?: HitTracerState;
  minVal: number;
  maxVal: number;
  palette: ColorPalette;
  displayMode: CellDisplayMode;
  decimals: number;
  readOnly: boolean;
  onRowHeaderClick: (rIdx: number) => void;
  onCellMouseDown: (rIdx: number, cIdx: number, e: React.MouseEvent) => void;
  onCellMouseEnter: (rIdx: number, cIdx: number) => void;
  onStartEdit: (rIdx: number, cIdx: number, val: number) => void;
  onChangeEditingValue: (value: string) => void;
  onCommitEdit: () => void;
}

const ROW_HEIGHT_PX = 28;

const HeatmapRowComponent: React.FC<HeatmapRowProps> = ({
  row,
  baseRow,
  previewRow,
  rIdx,
  yLabel,
  yAxisName,
  isRowInSelection,
  selectionMinC,
  selectionMaxC,
  anchorCol,
  editingCol,
  editingValue,
  correlatedCol,
  hitTraceEnabled = false,
  hitTracer,
  minVal,
  maxVal,
  palette,
  displayMode,
  decimals,
  readOnly,
  onRowHeaderClick,
  onCellMouseDown,
  onCellMouseEnter,
  onStartEdit,
  onChangeEditingValue,
  onCommitEdit,
}) => {
  const rowCorrelatedCoord =
    correlatedCol !== null ? { r: rIdx, c: correlatedCol } : null;

  return (
    <tr style={{ height: `${ROW_HEIGHT_PX}px` }}>
      <th
        onClick={() => onRowHeaderClick(rIdx)}
        title={`Row ${rIdx} (${yAxisName}): Click to select row`}
        className={`sticky left-0 z-20 min-w-[68px] whitespace-nowrap border border-kiviBorder px-2.5 py-1 cursor-pointer text-right tabular-nums transition-colors ${
          isRowInSelection
            ? 'bg-kiviAccent/20 text-kiviAccent font-semibold'
            : 'bg-kiviSurface text-kiviSecondary hover:bg-kiviElevated hover:text-kiviInk'
        }`}
      >
        {yLabel}
      </th>
      {row.map((val, cIdx) => {
        const isSelected =
          isRowInSelection &&
          selectionMinC !== null &&
          selectionMaxC !== null &&
          cIdx >= selectionMinC &&
          cIdx <= selectionMaxC;
        const isAnchor = anchorCol === cIdx;
        const isEditing = editingCol === cIdx;

        const crosshairState = getHeatmapCellCrosshairState({
          r: rIdx,
          c: cIdx,
          correlatedCell: rowCorrelatedCoord,
          isSelected,
          isAnchor,
          isEditing,
        });

        const bgColor = getHeatmapColor(val, minVal, maxVal, palette);
        const textColor = getHeatmapTextColor(val, minVal, maxVal, palette);
        const displayStr = formatHeatmapCellValue(
          val,
          rIdx,
          cIdx,
          displayMode,
          decimals,
          baseRow ? { [rIdx]: baseRow } as unknown as number[][] : undefined
        );
        const previewVal = previewRow?.[cIdx];
        const hitVisual =
          hitTraceEnabled && hitTracer ? getCellHitVisual(hitTracer, rIdx, cIdx) : null;

        return (
          <td
            key={cIdx}
            style={{ backgroundColor: bgColor, color: textColor }}
            onMouseDown={(e) => onCellMouseDown(rIdx, cIdx, e)}
            onMouseEnter={() => onCellMouseEnter(rIdx, cIdx)}
            onDoubleClick={() => {
              if (!readOnly) onStartEdit(rIdx, cIdx, val);
            }}
            aria-current={crosshairState.ariaAttributes['aria-current']}
            title={
              hitVisual && hitVisual.count > 0
                ? `${crosshairState.tooltip ? `${crosshairState.tooltip} | ` : ''}Hits: ${hitVisual.count}`
                : crosshairState.tooltip
            }
            className={`relative min-w-[56px] whitespace-nowrap border border-kiviBorder/60 px-1.5 py-1 font-semibold cursor-pointer select-none tabular-nums ${
              crosshairState.isCorrelated
                ? `${crosshairState.reticleClass} ${isAnchor ? 'ring-offset-2' : ''}`
                : isAnchor
                ? 'ring-2 ring-white z-10'
                : isSelected
                ? 'ring-1 ring-white/90 z-10'
                : 'hover:brightness-110'
            }`}
          >
            {hitVisual && hitVisual.count > 0 && (
              <>
                <span
                  aria-hidden="true"
                  className="pointer-events-none absolute inset-x-0 bottom-0 h-[3px] bg-kiviAccent"
                  style={{ opacity: Math.max(0.22, hitVisual.intensity) }}
                />
                {hitVisual.trailRank !== null && hitVisual.trailRank > 0 && (
                  <span
                    aria-hidden="true"
                    className="pointer-events-none absolute inset-0 border border-kiviAccent/50"
                    style={{ opacity: Math.max(0.18, 1 - hitVisual.trailRank * 0.07) }}
                  />
                )}
                <span
                  aria-hidden="true"
                  className="pointer-events-none absolute top-0 right-0.5 text-[9px] leading-none font-mono tabular-nums opacity-75"
                >
                  {hitVisual.count}
                </span>
              </>
            )}
            {crosshairState.isCorrelated && (
              <span
                data-testid="telemetry-crosshair-reticle"
                className="pointer-events-none absolute inset-0 rounded-[2px] border-2 border-kiviWarn/40 shadow-[0_0_6px_rgba(245,158,11,0.45)] flex items-center justify-center z-30"
                aria-hidden="true"
              >
                <span className="w-1.5 h-1.5 rounded-full bg-kiviWarn opacity-90 shadow-sm" />
              </span>
            )}
            <span
              aria-hidden={isEditing || undefined}
              className={`flex flex-col items-center leading-tight ${isEditing ? 'invisible' : ''}`}
            >
              <span>{displayStr}</span>
              {previewVal !== undefined && (
                <span
                  data-testid="smoothing-preview-value"
                  className="text-[10px] font-normal text-kiviWarn"
                >
                  {previewVal.toFixed(decimals)}
                </span>
              )}
            </span>
            {isEditing && (
              <input
                type="text"
                size={1}
                autoFocus
                aria-label={`Edit cell row ${rIdx} column ${cIdx}`}
                value={editingValue}
                onChange={(e) => onChangeEditingValue(e.target.value)}
                onFocus={(e) => e.currentTarget.select()}
                onMouseDown={(e) => e.stopPropagation()}
                onDoubleClick={(e) => e.stopPropagation()}
                onBlur={onCommitEdit}
                className="absolute inset-0 z-20 h-full w-full min-w-0 bg-kiviBg px-1 text-center font-mono text-xs font-bold text-kiviInk outline-none ring-2 ring-inset ring-kiviAccent"
              />
            )}
          </td>
        );
      })}
    </tr>
  );
};

export const HeatmapRow = React.memo(HeatmapRowComponent, (prev, next) => {
  return (
    prev.row === next.row &&
    prev.baseRow === next.baseRow &&
    prev.previewRow === next.previewRow &&
    prev.yLabel === next.yLabel &&
    prev.isRowInSelection === next.isRowInSelection &&
    prev.selectionMinC === next.selectionMinC &&
    prev.selectionMaxC === next.selectionMaxC &&
    prev.anchorCol === next.anchorCol &&
    prev.editingCol === next.editingCol &&
    (prev.editingCol === null || prev.editingValue === next.editingValue) &&
    prev.correlatedCol === next.correlatedCol &&
    prev.minVal === next.minVal &&
    prev.maxVal === next.maxVal &&
    prev.palette === next.palette &&
    prev.displayMode === next.displayMode &&
    prev.decimals === next.decimals &&
    prev.readOnly === next.readOnly &&
    prev.hitTraceEnabled === next.hitTraceEnabled &&
    (!next.hitTraceEnabled || prev.hitTracer === next.hitTracer)
  );
});

export function rowSelectionProps(selection: SelectionRange | null, rIdx: number) {
  const inRow = Boolean(selection && rIdx >= selection.minR && rIdx <= selection.maxR);
  return {
    isRowInSelection: inRow,
    selectionMinC: inRow && selection ? selection.minC : null,
    selectionMaxC: inRow && selection ? selection.maxC : null,
  };
}
