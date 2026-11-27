import React, { useEffect, useState } from 'react';
import { Binary, Check, Grid, Minus, Plus, RotateCcw, Settings2, Sliders, ToggleLeft } from 'lucide-react';
import type { MapDef } from '../../types/kivi';
import {
  decodeBitmask,
  dtypeBitWidth,
  formatRawRepresentations,
  isBitmaskFlagMap,
  toggleBitValue,
} from './scalarFlagModel';

export interface ScalarFlagEditorProps {
  mapDef: MapDef;
  value: number;
  baseValue?: number | null;
  readOnly?: boolean;
  onCommitValue: (nextValue: number) => void;
  onRevert?: () => void;
  onEditProperties?: () => void;
  onSwitchToGrid: () => void;
}

/**
 * Dedicated 1x1 Scalar Constant & Bitmask Flag Editor.
 * Keeps `tabular-nums` alignment and provides one-click fallback to the raw 1x1 grid.
 */
export const ScalarFlagEditor: React.FC<ScalarFlagEditorProps> = ({
  mapDef,
  value,
  baseValue = null,
  readOnly = false,
  onCommitValue,
  onRevert,
  onEditProperties,
  onSwitchToGrid,
}) => {
  const decimals = mapDef.scaling.decimals ?? 2;
  const unit = mapDef.scaling.unit || 'raw';
  const bitWidth = dtypeBitWidth(mapDef.dtype);
  const flagWidth: 8 | 16 = bitWidth === 8 ? 8 : 16;
  const preferFlags = isBitmaskFlagMap(mapDef);
  const stepSize = decimals > 0 ? Number((1 / Math.pow(10, Math.min(2, decimals))).toFixed(2)) : 1;

  const [draft, setDraft] = useState(() => value.toFixed(decimals));
  const [showBitmaskPanel, setShowBitmaskPanel] = useState<boolean>(preferFlags || bitWidth <= 16);

  useEffect(() => {
    setDraft(Number.isFinite(value) ? value.toFixed(decimals) : '0');
  }, [value, decimals, mapDef.id]);

  useEffect(() => {
    setShowBitmaskPanel(isBitmaskFlagMap(mapDef) || dtypeBitWidth(mapDef.dtype) <= 16);
  }, [mapDef]);

  const rawFormats = formatRawRepresentations(value, mapDef.dtype);
  const bitItems = decodeBitmask(value, flagWidth);
  const activeBitsCount = bitItems.filter((b) => b.enabled).length;

  const delta = baseValue !== null && baseValue !== undefined ? value - baseValue : null;
  const pctDelta =
    delta !== null && baseValue !== null && baseValue !== undefined && Math.abs(baseValue) > 1e-9
      ? (delta / Math.abs(baseValue)) * 100
      : null;

  const handleApplyInput = (e: React.FormEvent) => {
    e.preventDefault();
    if (readOnly) return;
    const parsed = Number.parseFloat(draft);
    if (!Number.isFinite(parsed)) return;
    onCommitValue(Number(parsed.toFixed(decimals)));
  };

  return (
    <div
      data-testid="scalar-flag-editor"
      className="flex flex-1 flex-col gap-3 rounded-lg border border-kiviBorder bg-kiviSurface p-4 font-mono"
    >
      {/* Header Strip */}
      <div className="flex flex-wrap items-center justify-between gap-2 border-b border-kiviBorder pb-3">
        <div className="flex min-w-0 items-center gap-2">
          {preferFlags ? (
            <ToggleLeft className="h-4 w-4 shrink-0 text-kiviAccent" />
          ) : (
            <Sliders className="h-4 w-4 shrink-0 text-kiviAccent" />
          )}
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-2">
              <h3 className="truncate text-sm font-semibold text-kiviInk">{mapDef.name}</h3>
              <span className="rounded border border-kiviBorder bg-kiviBg px-2 py-0.5 text-[10px] text-kiviSecondary">
                {preferFlags ? '1x1 Bitmask Flag' : '1x1 Scalar Constant'}
              </span>
              <span className="rounded border border-kiviAccent/40 bg-kiviAccent/10 px-2 py-0.5 text-[10px] font-semibold text-kiviAccent">
                {unit}
              </span>
            </div>
            <div className="mt-0.5 flex flex-wrap items-center gap-3 text-[11px] text-kiviMuted tabular-nums">
              <span>Offset: 0x{mapDef.address.toString(16).toUpperCase().padStart(6, '0')}</span>
              <span>Type: {mapDef.dtype.toUpperCase()} ({mapDef.endian})</span>
              <span>Eq: {mapDef.scaling.equation || 'X'}</span>
            </div>
          </div>
        </div>

        <div className="flex flex-wrap items-center gap-1.5 text-xs">
          {onRevert && (
            <button
              type="button"
              disabled={readOnly || baseValue === null}
              onClick={onRevert}
              className="inline-flex items-center gap-1 rounded border border-kiviWarn/40 bg-kiviWarn/10 px-2.5 py-1 text-[11px] font-medium text-kiviWarn transition-colors hover:bg-kiviWarn/20 disabled:opacity-40"
            >
              <RotateCcw className="h-3.5 w-3.5 shrink-0" />
              <span>Revert Stock</span>
            </button>
          )}
          {onEditProperties && (
            <button
              type="button"
              onClick={onEditProperties}
              className="inline-flex items-center gap-1 rounded border border-kiviBorderStrong bg-kiviBg px-2.5 py-1 text-[11px] text-kiviSecondary transition-colors hover:border-kiviAccent hover:text-kiviInk"
            >
              <Settings2 className="h-3.5 w-3.5 shrink-0" />
              <span>Properties</span>
            </button>
          )}
          <button
            type="button"
            onClick={onSwitchToGrid}
            className="inline-flex items-center gap-1 rounded border border-kiviBorderStrong bg-kiviElevated px-2.5 py-1 text-[11px] text-kiviInk transition-colors hover:border-kiviAccent"
          >
            <Grid className="h-3.5 w-3.5 shrink-0 text-kiviAccent" />
            <span>Grid View</span>
          </button>
        </div>
      </div>

      {/* Main 2-Column Cockpits: Calibrated Scalar + Bitmask Flags */}
      <div className="grid grid-cols-1 gap-3 xl:grid-cols-2">
        {/* Left: Calibrated Scalar Editor */}
        <section className="flex flex-col justify-between gap-3 rounded-lg border border-kiviBorder bg-kiviBg p-3.5">
          <div className="flex items-center justify-between gap-2">
            <span className="text-[11px] font-semibold uppercase tracking-wider text-kiviSecondary">
              Calibrated Parameter Value
            </span>
            <span className="text-[10px] text-kiviMuted tabular-nums">
              {decimals} dec | {unit}
            </span>
          </div>

          <form onSubmit={handleApplyInput} className="flex flex-wrap items-center gap-2">
            <button
              type="button"
              disabled={readOnly}
              aria-label="Decrement scalar value"
              onClick={() => onCommitValue(Number((value - stepSize).toFixed(decimals)))}
              className="flex h-9 w-9 shrink-0 items-center justify-center rounded border border-kiviBorderStrong bg-kiviSurface text-kiviSecondary hover:bg-kiviElevated hover:text-kiviInk disabled:opacity-40"
            >
              <Minus className="h-3.5 w-3.5 text-kiviRed" />
            </button>

            <input
              type="number"
              step="any"
              disabled={readOnly}
              aria-label="Scalar parameter value"
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              className="h-9 min-w-[140px] flex-1 rounded border border-kiviBorderStrong bg-kiviSurface px-3 text-base font-bold text-kiviInk tabular-nums focus:border-kiviAccent focus-visible:ring-2 focus-visible:ring-kiviAccent disabled:opacity-50"
            />

            <button
              type="button"
              disabled={readOnly}
              aria-label="Increment scalar value"
              onClick={() => onCommitValue(Number((value + stepSize).toFixed(decimals)))}
              className="flex h-9 w-9 shrink-0 items-center justify-center rounded border border-kiviBorderStrong bg-kiviSurface text-kiviSecondary hover:bg-kiviElevated hover:text-kiviInk disabled:opacity-40"
            >
              <Plus className="h-3.5 w-3.5 text-kiviGreen" />
            </button>

            <button
              type="submit"
              disabled={readOnly}
              className="inline-flex h-9 items-center gap-1.5 rounded bg-kiviAccent px-3 text-xs font-semibold text-kiviAccentFg transition-opacity hover:opacity-90 disabled:opacity-40"
            >
              <Check className="h-3.5 w-3.5 shrink-0" />
              <span>Apply</span>
            </button>
          </form>

          {/* Synchronized HEX / DEC / BIN & Baseline Delta */}
          <div className="grid grid-cols-3 gap-2 text-[11px] tabular-nums">
            <div className="rounded border border-kiviBorder bg-kiviSurface px-2.5 py-1.5">
              <span className="block text-[9px] uppercase text-kiviMuted">Raw Hex</span>
              <span className="font-semibold text-kiviAccent">{rawFormats.hex}</span>
            </div>
            <div className="rounded border border-kiviBorder bg-kiviSurface px-2.5 py-1.5">
              <span className="block text-[9px] uppercase text-kiviMuted">Decimal</span>
              <span className="font-semibold text-kiviInk">{rawFormats.dec}</span>
            </div>
            <div className="rounded border border-kiviBorder bg-kiviSurface px-2.5 py-1.5">
              <span className="block text-[9px] uppercase text-kiviMuted">Binary</span>
              <span className="block truncate font-semibold text-kiviSecondary" title={rawFormats.bin}>
                {rawFormats.bin}
              </span>
            </div>
          </div>

          <div className="flex flex-wrap items-center justify-between gap-2 border-t border-kiviBorder/70 pt-2 text-[11px] tabular-nums">
            <span className="text-kiviMuted">
              Stock Baseline:{' '}
              <strong className="text-kiviSecondary">
                {baseValue !== null && baseValue !== undefined ? `${baseValue.toFixed(decimals)} ${unit}` : 'Unavailable'}
              </strong>
            </span>
            <span
              className={`font-semibold ${
                delta === null || Math.abs(delta) < 1e-9
                  ? 'text-kiviMuted'
                  : delta > 0
                    ? 'text-kiviGreen'
                    : 'text-kiviWarn'
              }`}
            >
              Delta:{' '}
              {delta !== null
                ? `${delta >= 0 ? '+' : ''}${delta.toFixed(decimals)}${
                    pctDelta !== null ? ` (${pctDelta >= 0 ? '+' : ''}${pctDelta.toFixed(1)}%)` : ''
                  }`
                : '--'}
            </span>
          </div>
        </section>

        {/* Right: Bitmask Flag Switchboard */}
        <section className="flex flex-col justify-between gap-2.5 rounded-lg border border-kiviBorder bg-kiviBg p-3.5">
          <div className="flex items-center justify-between gap-2">
            <span className="flex items-center gap-1.5 text-[11px] font-semibold uppercase tracking-wider text-kiviSecondary">
              <Binary className="h-3.5 w-3.5 text-kiviAccent" />
              <span>Bitmask Switches ({flagWidth}-Bit)</span>
            </span>
            <div className="flex items-center gap-2">
              <span className="rounded border border-kiviBorder bg-kiviSurface px-2 py-0.5 text-[10px] text-kiviMuted tabular-nums">
                {activeBitsCount}/{flagWidth} ON
              </span>
              {bitWidth > 16 && (
                <button
                  type="button"
                  onClick={() => setShowBitmaskPanel((v) => !v)}
                  className="text-[10px] text-kiviAccent hover:underline"
                >
                  {showBitmaskPanel ? 'Hide bits' : 'Show bits'}
                </button>
              )}
            </div>
          </div>

          {showBitmaskPanel ? (
            <div
              className={`grid gap-1.5 ${
                flagWidth === 16 ? 'grid-cols-2 sm:grid-cols-4' : 'grid-cols-2 sm:grid-cols-4'
              }`}
            >
              {bitItems.map((item) => (
                <button
                  key={item.bit}
                  type="button"
                  disabled={readOnly}
                  aria-pressed={item.enabled}
                  aria-label={`Bit ${item.bit} (${item.maskHex})`}
                  onClick={() => {
                    const nextInt = toggleBitValue(value, item.bit, flagWidth);
                    onCommitValue(nextInt);
                  }}
                  className={`flex items-center justify-between rounded border px-2.5 py-2 text-[11px] transition-colors disabled:opacity-40 ${
                    item.enabled
                      ? 'border-kiviGreen/60 bg-kiviGreen/15 text-kiviGreen'
                      : 'border-kiviBorder bg-kiviSurface text-kiviSecondary hover:border-kiviBorderStrong hover:text-kiviInk'
                  }`}
                >
                  <span className="text-left leading-tight">
                    <span className="block font-semibold">Bit {item.bit}</span>
                    <span className="block text-[9px] text-kiviMuted">{item.maskHex}</span>
                  </span>
                  <span
                    className={`rounded px-1.5 py-0.5 text-[9px] font-bold ${
                      item.enabled
                        ? 'bg-kiviGreen text-kiviAccentFg'
                        : 'border border-kiviBorder bg-kiviBg text-kiviMuted'
                    }`}
                  >
                    {item.enabled ? 'ON' : 'OFF'}
                  </span>
                </button>
              ))}
            </div>
          ) : (
            <div className="flex flex-1 items-center justify-center rounded border border-dashed border-kiviBorder p-4 text-center text-[11px] text-kiviMuted">
              32-bit floating / integer constant. Use the numeric control or switch to Raw Bits above.
            </div>
          )}
        </section>
      </div>
    </div>
  );
};
