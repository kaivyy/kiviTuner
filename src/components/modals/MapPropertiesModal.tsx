import React, { useEffect, useState } from 'react';
import { Sliders } from 'lucide-react';
import { FloatingWindow } from '../layout/FloatingWindow';
import { TOOL_WINDOW_SPECS } from '../layout/toolWindowRegistry';
import type { AxisDef, DType, Endian, MapDef } from '../../types/kivi';

export interface MapPropertiesModalProps {
  isOpen: boolean;
  onClose: () => void;
  initialMap: MapDef | null;
  mode: 'create' | 'edit';
  createPresetType?: 'table' | 'axis' | 'scalar' | 'flag';
  romSize: number;
  onSaveMap: (mapDef: MapDef) => Promise<void> | void;
}

const DTYPES: DType[] = ['u8', 'i8', 'u16', 'i16', 'u32', 'i32', 'f32'];

function parseHexOrDec(raw: string, fallback: number): number {
  const s = raw.trim();
  if (!s) return fallback;
  const parsed = s.toLowerCase().startsWith('0x') ? Number.parseInt(s, 16) : Number.parseInt(s, 10);
  return Number.isFinite(parsed) && parsed >= 0 ? parsed : fallback;
}

function dtypeBytes(dtype: DType): number {
  if (dtype === 'u8' || dtype === 'i8') return 1;
  if (dtype === 'u16' || dtype === 'i16') return 2;
  return 4;
}

function buildDefaultPresetDims(
  preset?: 'table' | 'axis' | 'scalar' | 'flag'
): { rows: number; cols: number; category: string; name: string } {
  if (preset === 'axis') return { rows: 1, cols: 16, category: 'Axes', name: 'New 1D Axis' };
  if (preset === 'scalar') return { rows: 1, cols: 1, category: 'Scalars', name: 'New 1x1 Scalar' };
  if (preset === 'flag') return { rows: 1, cols: 1, category: 'Flags', name: 'New Bitmask Flag' };
  return { rows: 16, cols: 16, category: 'Custom Tables', name: 'New 2D Map' };
}

export const MapPropertiesModal: React.FC<MapPropertiesModalProps> = ({
  isOpen,
  onClose,
  initialMap,
  mode,
  createPresetType = 'table',
  romSize,
  onSaveMap,
}) => {
  const [name, setName] = useState('');
  const [category, setCategory] = useState('Custom Tables');
  const [addressHex, setAddressHex] = useState('0x0');
  const [rows, setRows] = useState('16');
  const [cols, setCols] = useState('16');
  const [dtype, setDtype] = useState<DType>('u16');
  const [endian, setEndian] = useState<Endian>('big');
  const [columnMajor, setColumnMajor] = useState(false);
  const [majorStrideBits, setMajorStrideBits] = useState('0');
  const [minorStrideBits, setMinorStrideBits] = useState('0');
  const [equation, setEquation] = useState('X');
  const [unit, setUnit] = useState('raw');
  const [decimals, setDecimals] = useState('2');
  const [xValuesCsv, setXValuesCsv] = useState('');
  const [yValuesCsv, setYValuesCsv] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!isOpen) return;
    setError(null);
    if (mode === 'edit' && initialMap) {
      setName(initialMap.name);
      setCategory(initialMap.category || 'Custom Tables');
      setAddressHex(`0x${initialMap.address.toString(16).toUpperCase()}`);
      setRows(String(initialMap.rows));
      setCols(String(initialMap.cols));
      setDtype(initialMap.dtype);
      setEndian(initialMap.endian);
      setColumnMajor(Boolean(initialMap.column_major));
      setMajorStrideBits(String(initialMap.major_stride_bits ?? 0));
      setMinorStrideBits(String(initialMap.minor_stride_bits ?? 0));
      setEquation(initialMap.scaling.equation || 'X');
      setUnit(initialMap.scaling.unit || 'raw');
      setDecimals(String(initialMap.scaling.decimals ?? 2));
      setXValuesCsv(initialMap.x_axis?.values ? initialMap.x_axis.values.join(', ') : '');
      setYValuesCsv(initialMap.y_axis?.values ? initialMap.y_axis.values.join(', ') : '');
    } else {
      const preset = buildDefaultPresetDims(createPresetType);
      setName(preset.name);
      setCategory(preset.category);
      setAddressHex(
        initialMap ? `0x${initialMap.address.toString(16).toUpperCase()}` : '0x0'
      );
      setRows(initialMap ? String(initialMap.rows) : String(preset.rows));
      setCols(initialMap ? String(initialMap.cols) : String(preset.cols));
      setDtype(initialMap?.dtype ?? (createPresetType === 'flag' ? 'u8' : 'u16'));
      setEndian(initialMap?.endian ?? 'big');
      setColumnMajor(Boolean(initialMap?.column_major));
      setMajorStrideBits('0');
      setMinorStrideBits('0');
      setEquation('X');
      setUnit(createPresetType === 'flag' ? 'bitmask' : 'raw');
      setDecimals(createPresetType === 'flag' ? '0' : '2');
      setXValuesCsv('');
      setYValuesCsv('');
    }
  }, [isOpen, mode, initialMap, createPresetType]);

  if (!isOpen) return null;

  const parsedAddr = parseHexOrDec(addressHex, -1);
  const parsedRows = Math.max(1, Number.parseInt(rows, 10) || 1);
  const parsedCols = Math.max(1, Number.parseInt(cols, 10) || 1);
  const parsedMajorStride = Math.max(0, Number.parseInt(majorStrideBits, 10) || 0);
  const parsedMinorStride = Math.max(0, Number.parseInt(minorStrideBits, 10) || 0);
  const byteSize = dtypeBytes(dtype);
  const endOffset = parsedAddr >= 0 ? parsedAddr + parsedRows * parsedCols * byteSize : 0;

  const parseAxis = (csv: string, expectedLen: number, prevAxis?: AxisDef | null): AxisDef | null => {
    const trimmed = csv.trim();
    if (!trimmed) return null;
    const nums = trimmed
      .split(/[\s,;]+/)
      .map((s) => Number.parseFloat(s))
      .filter((n) => Number.isFinite(n));
    if (nums.length === 0) return null;
    return {
      address: prevAxis?.address ?? parsedAddr,
      length: nums.length || expectedLen,
      dtype: prevAxis?.dtype ?? dtype,
      endian: prevAxis?.endian ?? endian,
      scaling: prevAxis?.scaling ?? { equation: 'X', unit: '', decimals: 1 },
      values: nums,
    };
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim()) {
      setError('Map name is required.');
      return;
    }
    if (parsedAddr < 0) {
      setError('Start address must be a valid hex (0x...) or decimal byte offset.');
      return;
    }
    if (romSize > 0 && endOffset > romSize) {
      setError(
        `Range 0x${parsedAddr.toString(16).toUpperCase()}..0x${endOffset.toString(16).toUpperCase()} exceeds ROM size (${romSize} bytes).`
      );
      return;
    }
    setSaving(true);
    setError(null);
    try {
      const nextMap: MapDef = {
        id:
          mode === 'edit' && initialMap
            ? initialMap.id
            : `map_${parsedAddr.toString(16)}_${Date.now()}`,
        name: name.trim(),
        category: category.trim() || 'Custom Tables',
        address: parsedAddr,
        rows: parsedRows,
        cols: parsedCols,
        dtype,
        endian,
        column_major: columnMajor,
        major_stride_bits: parsedMajorStride,
        minor_stride_bits: parsedMinorStride,
        scaling: {
          equation: equation.trim() || 'X',
          unit: unit.trim() || 'raw',
          decimals: Math.min(6, Math.max(0, Number.parseInt(decimals, 10) || 0)),
        },
        x_axis: parseAxis(xValuesCsv, parsedCols, initialMap?.x_axis) ?? (xValuesCsv.trim() ? null : initialMap?.x_axis ?? null),
        y_axis: parseAxis(yValuesCsv, parsedRows, initialMap?.y_axis) ?? (yValuesCsv.trim() ? null : initialMap?.y_axis ?? null),
      };
      await onSaveMap(nextMap);
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSaving(false);
    }
  };

  const spec = TOOL_WINDOW_SPECS['map-props'];

  return (
    <FloatingWindow
      windowId="map-props"
      isOpen={isOpen}
      onClose={onClose}
      title={
        mode === 'edit'
          ? `Map Properties: ${initialMap?.name ?? ''}`
          : 'Create Parameter / Map Definition'
      }
      titleId="map-props-title"
      icon={<Sliders className="h-4 w-4" />}
      defaultWidth={spec.width}
      defaultHeight={spec.height}
      minWidth={spec.minWidth}
      minHeight={spec.minHeight}
      dock={spec.dock}
      closeLabel="Close map properties"
    >
      <form
        onSubmit={(e) => void handleSubmit(e)}
        className="flex min-h-0 flex-1 flex-col overflow-hidden text-kiviInk"
      >
        <div className="grid flex-1 grid-cols-[repeat(auto-fit,minmax(200px,1fr))] gap-3 overflow-y-auto p-5 text-xs">
          <div>
            <label htmlFor="mp-name" className="mb-1 block text-[11px] font-medium text-kiviSecondary">
              Name
            </label>
            <input
              id="mp-name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              className="w-full rounded border border-kiviBorderStrong bg-kiviBg px-2.5 py-1.5 text-xs text-kiviInk focus-visible:outline-2 focus-visible:outline-kiviAccent"
            />
          </div>

          <div>
            <label htmlFor="mp-cat" className="mb-1 block text-[11px] font-medium text-kiviSecondary">
              Category
            </label>
            <input
              id="mp-cat"
              value={category}
              onChange={(e) => setCategory(e.target.value)}
              className="w-full rounded border border-kiviBorderStrong bg-kiviBg px-2.5 py-1.5 text-xs text-kiviInk focus-visible:outline-2 focus-visible:outline-kiviAccent"
            />
          </div>

          <div>
            <label htmlFor="mp-addr" className="mb-1 block text-[11px] font-medium text-kiviSecondary">
              Start Address (Hex / Dec)
            </label>
            <input
              id="mp-addr"
              value={addressHex}
              onChange={(e) => setAddressHex(e.target.value)}
              className="w-full rounded border border-kiviBorderStrong bg-kiviBg px-2.5 py-1.5 font-mono text-xs text-kiviInk tabular-nums focus-visible:outline-2 focus-visible:outline-kiviAccent"
            />
          </div>

          <div className="grid grid-cols-2 gap-2">
            <div>
              <label htmlFor="mp-rows" className="mb-1 block text-[11px] font-medium text-kiviSecondary">
                Rows
              </label>
              <input
                id="mp-rows"
                type="number"
                min={1}
                max={256}
                value={rows}
                onChange={(e) => setRows(e.target.value)}
                className="w-full rounded border border-kiviBorderStrong bg-kiviBg px-2 py-1.5 font-mono text-xs text-kiviInk tabular-nums"
              />
            </div>
            <div>
              <label htmlFor="mp-cols" className="mb-1 block text-[11px] font-medium text-kiviSecondary">
                Cols
              </label>
              <input
                id="mp-cols"
                type="number"
                min={1}
                max={256}
                value={cols}
                onChange={(e) => setCols(e.target.value)}
                className="w-full rounded border border-kiviBorderStrong bg-kiviBg px-2 py-1.5 font-mono text-xs text-kiviInk tabular-nums"
              />
            </div>
          </div>

          <div>
            <label htmlFor="mp-dtype" className="mb-1 block text-[11px] font-medium text-kiviSecondary">
              Data Type
            </label>
            <select
              id="mp-dtype"
              value={dtype}
              onChange={(e) => setDtype(e.target.value as DType)}
              className="w-full rounded border border-kiviBorderStrong bg-kiviBg px-2 py-1.5 font-mono text-xs text-kiviInk"
            >
              {DTYPES.map((d) => (
                <option key={d} value={d}>
                  {d} ({dtypeBytes(d) * 8}-bit)
                </option>
              ))}
            </select>
          </div>

          <div className="grid grid-cols-2 gap-2">
            <div>
              <label htmlFor="mp-endian" className="mb-1 block text-[11px] font-medium text-kiviSecondary">
                Endianness
              </label>
              <select
                id="mp-endian"
                value={endian}
                onChange={(e) => setEndian(e.target.value as Endian)}
                className="w-full rounded border border-kiviBorderStrong bg-kiviBg px-2 py-1.5 text-xs text-kiviInk"
              >
                <option value="big">Big Endian (BE)</option>
                <option value="little">Little Endian (LE)</option>
              </select>
            </div>
            <label className="mt-5 flex items-center gap-2 text-xs text-kiviSecondary">
              <input
                type="checkbox"
                checked={columnMajor}
                onChange={(e) => setColumnMajor(e.target.checked)}
              />
              <span>Column-Major</span>
            </label>
          </div>

          <div className="grid grid-cols-2 gap-2">
            <div>
              <label htmlFor="mp-major-stride" className="mb-1 block text-[11px] font-medium text-kiviSecondary">
                Major Stride (bits)
              </label>
              <input
                id="mp-major-stride"
                type="number"
                min={0}
                value={majorStrideBits}
                onChange={(e) => setMajorStrideBits(e.target.value)}
                className="w-full rounded border border-kiviBorderStrong bg-kiviBg px-2 py-1.5 font-mono text-xs text-kiviInk tabular-nums"
              />
            </div>
            <div>
              <label htmlFor="mp-minor-stride" className="mb-1 block text-[11px] font-medium text-kiviSecondary">
                Minor Stride (bits)
              </label>
              <input
                id="mp-minor-stride"
                type="number"
                min={0}
                value={minorStrideBits}
                onChange={(e) => setMinorStrideBits(e.target.value)}
                className="w-full rounded border border-kiviBorderStrong bg-kiviBg px-2 py-1.5 font-mono text-xs text-kiviInk tabular-nums"
              />
            </div>
          </div>

          <div className="grid grid-cols-3 gap-2">
            <div className="col-span-2">
              <label htmlFor="mp-eq" className="mb-1 block text-[11px] font-medium text-kiviSecondary">
                Scaling Equation (e.g. X * 0.5 + 10)
              </label>
              <input
                id="mp-eq"
                value={equation}
                onChange={(e) => setEquation(e.target.value)}
                className="w-full rounded border border-kiviBorderStrong bg-kiviBg px-2 py-1.5 font-mono text-xs text-kiviInk"
              />
            </div>
            <div>
              <label htmlFor="mp-dec" className="mb-1 block text-[11px] font-medium text-kiviSecondary">
                Decimals
              </label>
              <input
                id="mp-dec"
                type="number"
                min={0}
                max={6}
                value={decimals}
                onChange={(e) => setDecimals(e.target.value)}
                className="w-full rounded border border-kiviBorderStrong bg-kiviBg px-2 py-1.5 font-mono text-xs text-kiviInk tabular-nums"
              />
            </div>
          </div>

          <div className="sm:col-span-2">
            <label htmlFor="mp-unit" className="mb-1 block text-[11px] font-medium text-kiviSecondary">
              Engineering Unit
            </label>
            <input
              id="mp-unit"
              value={unit}
              onChange={(e) => setUnit(e.target.value)}
              className="w-full rounded border border-kiviBorderStrong bg-kiviBg px-2.5 py-1.5 text-xs text-kiviInk"
            />
          </div>

          <div className="sm:col-span-2">
            <label htmlFor="mp-xvals" className="mb-1 block text-[11px] font-medium text-kiviSecondary">
              X-Axis Breakpoints (comma-separated, optional)
            </label>
            <input
              id="mp-xvals"
              placeholder="e.g. 0, 10, 20, 40, 60, 80, 100"
              value={xValuesCsv}
              onChange={(e) => setXValuesCsv(e.target.value)}
              className="w-full rounded border border-kiviBorderStrong bg-kiviBg px-2.5 py-1.5 font-mono text-xs text-kiviInk"
            />
          </div>

          <div className="sm:col-span-2">
            <label htmlFor="mp-yvals" className="mb-1 block text-[11px] font-medium text-kiviSecondary">
              Y-Axis Breakpoints (comma-separated, optional)
            </label>
            <input
              id="mp-yvals"
              placeholder="e.g. 1000, 2000, 3000, 4000, 5000"
              value={yValuesCsv}
              onChange={(e) => setYValuesCsv(e.target.value)}
              className="w-full rounded border border-kiviBorderStrong bg-kiviBg px-2.5 py-1.5 font-mono text-xs text-kiviInk"
            />
          </div>

          <div className="sm:col-span-2 rounded border border-kiviBorder bg-kiviBg px-3 py-2 font-mono text-[11px] text-kiviSecondary tabular-nums">
            Footprint: {parsedRows}x{parsedCols} ({parsedRows * parsedCols} cells, {byteSize * parsedRows * parsedCols} bytes)
            {parsedAddr >= 0 && ` | 0x${parsedAddr.toString(16).toUpperCase()}..0x${endOffset.toString(16).toUpperCase()}`}
          </div>

          {error && (
            <div className="sm:col-span-2 rounded border border-kiviRed/40 bg-kiviRed/10 px-3 py-2 text-xs text-kiviRed">
              {error}
            </div>
          )}
        </div>

        <div className="flex justify-end gap-2 border-t border-kiviBorder bg-kiviElevated px-5 py-3">
          <button
            type="button"
            onClick={onClose}
            className="rounded border border-kiviBorder bg-kiviSurface px-3 py-1.5 text-xs text-kiviSecondary hover:text-kiviInk"
          >
            Cancel
          </button>
          <button
            type="submit"
            disabled={saving}
            className="rounded border border-kiviAccent bg-kiviAccent px-4 py-1.5 text-xs font-semibold text-kiviAccentFg hover:opacity-90 disabled:opacity-50"
          >
            {saving ? 'Saving...' : mode === 'edit' ? 'Apply Changes' : 'Add Parameter'}
          </button>
        </div>
      </form>
    </FloatingWindow>
  );
};
