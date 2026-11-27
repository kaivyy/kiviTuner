import React, { useCallback, useMemo, useRef, useState } from 'react';

export interface CurveProfileChartProps {
  title: string;
  values: number[];
  baseValues?: number[] | null;
  xLabels: (string | number)[];
  xAxisName?: string;
  unit?: string;
  decimals?: number;
  readOnly?: boolean;
  onPatchPoint?: (index: number, nextValue: number) => void;
}

export const CurveProfileChart: React.FC<CurveProfileChartProps> = ({
  title,
  values,
  baseValues,
  xLabels,
  xAxisName = 'Index',
  unit = '',
  decimals = 2,
  readOnly = false,
  onPatchPoint,
}) => {
  const svgRef = useRef<SVGSVGElement | null>(null);
  const [hoverIndex, setHoverIndex] = useState<number | null>(null);
  const [dragIndex, setDragIndex] = useState<number | null>(null);
  const [dragVal, setDragVal] = useState<number | null>(null);

  const width = 640;
  const height = 170;
  const padLeft = 44;
  const padRight = 18;
  const padTop = 14;
  const padBottom = 26;
  const plotW = width - padLeft - padRight;
  const plotH = height - padTop - padBottom;

  const { minY, maxY } = useMemo(() => {
    const all = [...values];
    if (baseValues) {
      for (const v of baseValues) {
        if (Number.isFinite(v)) all.push(v);
      }
    }
    const valid = all.filter((v) => Number.isFinite(v));
    if (valid.length === 0) return { minY: 0, maxY: 100 };
    let lo = Math.min(...valid);
    let hi = Math.max(...valid);
    if (Math.abs(hi - lo) < 1e-6) {
      lo -= 1;
      hi += 1;
    } else {
      const margin = (hi - lo) * 0.08;
      lo -= margin;
      hi += margin;
    }
    return { minY: lo, maxY: hi };
  }, [values, baseValues]);

  const toX = useCallback(
    (idx: number) => {
      if (values.length <= 1) return padLeft + plotW / 2;
      return padLeft + (idx / (values.length - 1)) * plotW;
    },
    [values.length, plotW]
  );

  const toY = useCallback(
    (val: number) => {
      const ratio = (val - minY) / Math.max(1e-9, maxY - minY);
      return padTop + (1 - Math.max(0, Math.min(1, ratio))) * plotH;
    },
    [minY, maxY, plotH]
  );

  const valueFromClientY = useCallback(
    (clientY: number) => {
      const rect = svgRef.current?.getBoundingClientRect();
      if (!rect || rect.height <= 0) return null;
      const svgY = ((clientY - rect.top) / rect.height) * height;
      const clampedPlotY = Math.max(padTop, Math.min(padTop + plotH, svgY));
      const ratio = 1 - (clampedPlotY - padTop) / plotH;
      const val = minY + ratio * (maxY - minY);
      const factor = Math.pow(10, Math.min(4, Math.max(0, decimals)));
      return Math.round(val * factor) / factor;
    },
    [minY, maxY, decimals, plotH]
  );

  const displayValues = useMemo(
    () =>
      values.map((v, idx) => (dragIndex === idx && dragVal !== null ? dragVal : v)),
    [values, dragIndex, dragVal]
  );

  const livePath = useMemo(() => {
    if (displayValues.length === 0) return '';
    return displayValues
      .map((v, idx) => `${idx === 0 ? 'M' : 'L'} ${toX(idx).toFixed(1)} ${toY(v).toFixed(1)}`)
      .join(' ');
  }, [displayValues, toX, toY]);

  const basePath = useMemo(() => {
    if (!baseValues || baseValues.length === 0) return '';
    return baseValues
      .map((v, idx) => `${idx === 0 ? 'M' : 'L'} ${toX(idx).toFixed(1)} ${toY(v).toFixed(1)}`)
      .join(' ');
  }, [baseValues, toX, toY]);

  const activeIdx = dragIndex ?? hoverIndex;
  const activeVal = activeIdx !== null ? displayValues[activeIdx] : null;
  const activeBaseVal = activeIdx !== null ? baseValues?.[activeIdx] : undefined;

  return (
    <div className="rounded-lg border border-kiviBorder bg-kiviSurface p-2.5">
      <div className="mb-1.5 flex flex-wrap items-center justify-between gap-2 text-[11px]">
        <span className="font-semibold text-kiviInk">
          Curve Profile: {title}{' '}
          <span className="font-normal text-kiviMuted">({xAxisName})</span>
        </span>
        <div className="flex items-center gap-3 font-mono text-[10px] text-kiviSecondary tabular-nums">
          {baseValues && (
            <span className="flex items-center gap-1">
              <span className="inline-block w-3 border-t border-dashed border-kiviMuted" />
              <span>Stock</span>
            </span>
          )}
          <span className="flex items-center gap-1">
            <span className="inline-block h-2 w-2 rounded-full bg-kiviAccent" />
            <span>Live</span>
          </span>
          {activeIdx !== null && activeVal !== null && (
            <span className="text-kiviInk">
              [{xLabels[activeIdx] ?? activeIdx}] {activeVal.toFixed(decimals)}
              {unit ? ` ${unit}` : ''}
              {activeBaseVal !== undefined
                ? ` (${activeVal - activeBaseVal >= 0 ? '+' : ''}${(activeVal - activeBaseVal).toFixed(decimals)})`
                : ''}
            </span>
          )}
        </div>
      </div>

      <svg
        ref={svgRef}
        viewBox={`0 0 ${width} ${height}`}
        className="h-40 w-full select-none rounded border border-kiviBorder bg-kiviBg"
        onPointerMove={(e) => {
          if (dragIndex !== null && !readOnly) {
            const v = valueFromClientY(e.clientY);
            if (v !== null) setDragVal(v);
          }
        }}
        onPointerUp={() => {
          if (dragIndex !== null && dragVal !== null && !readOnly) {
            onPatchPoint?.(dragIndex, dragVal);
          }
          setDragIndex(null);
          setDragVal(null);
        }}
        onPointerLeave={() => {
          if (dragIndex !== null && dragVal !== null && !readOnly) {
            onPatchPoint?.(dragIndex, dragVal);
          }
          setDragIndex(null);
          setDragVal(null);
          setHoverIndex(null);
        }}
      >
        {[0, 0.25, 0.5, 0.75, 1].map((frac) => {
          const y = padTop + frac * plotH;
          const labelVal = maxY - frac * (maxY - minY);
          return (
            <g key={frac}>
              <line
                x1={padLeft}
                y1={y}
                x2={width - padRight}
                y2={y}
                stroke="var(--kivi-border)"
                strokeWidth="1"
              />
              <text
                x={padLeft - 6}
                y={y + 3}
                textAnchor="end"
                fontSize="9"
                fontFamily="monospace"
                fill="var(--kivi-muted)"
              >
                {labelVal.toFixed(decimals > 1 ? 1 : 0)}
              </text>
            </g>
          );
        })}

        {basePath && (
          <path
            d={basePath}
            fill="none"
            stroke="var(--kivi-muted)"
            strokeWidth="1.5"
            strokeDasharray="4 3"
          />
        )}

        {livePath && (
          <path
            d={livePath}
            fill="none"
            stroke="var(--kivi-accent)"
            strokeWidth="2"
          />
        )}

        {displayValues.map((v, idx) => {
          const cx = toX(idx);
          const cy = toY(v);
          const isHot = activeIdx === idx;
          const labelStep = Math.max(1, Math.ceil(values.length / 8));
          return (
            <g key={idx}>
              {idx % labelStep === 0 && (
                <text
                  x={cx}
                  y={height - 7}
                  textAnchor="middle"
                  fontSize="9"
                  fontFamily="monospace"
                  fill="var(--kivi-muted)"
                >
                  {String(xLabels[idx] ?? idx)}
                </text>
              )}
              <circle
                cx={cx}
                cy={cy}
                r={isHot ? 5.5 : 3.5}
                fill={isHot ? 'var(--kivi-accent)' : 'var(--kivi-surface)'}
                stroke="var(--kivi-accent)"
                strokeWidth="2"
                className={readOnly ? '' : 'cursor-ns-resize'}
                onPointerEnter={() => setHoverIndex(idx)}
                onPointerDown={(e) => {
                  if (readOnly || !onPatchPoint) return;
                  e.stopPropagation();
                  e.currentTarget.setPointerCapture?.(e.pointerId);
                  setDragIndex(idx);
                  setDragVal(v);
                }}
              />
            </g>
          );
        })}
      </svg>
    </div>
  );
};
