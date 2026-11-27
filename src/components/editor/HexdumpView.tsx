/**
 * Virtualized hex viewer with 4 KiB LRU window cache, pattern search,
 * live multi-type data inspector strip, byte selection, and race-safe async paging.
 */

import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  HEX_COLUMN_PRESETS,
  findAdjacentMark,
  findContainingMark,
  formatHexCellValue,
  formatOffset,
  isMarked,
  parseAddress,
  parseHexSearchQuery,
  printable,
  type HexNumberFormat,
  type HexWordMode,
  type MarkedRange,
} from './hexdumpModel';
import {
  bridgeReadRomWindow,
  bridgeSearchRomPattern,
} from '../../lib/bridge/windowBridge';
import { useMapStore } from '../../stores/useMapStore';

export interface HexdumpViewProps {
  data: Uint8Array | null;
  romSize?: number;
  romLoaded?: boolean;
  romRevision?: number;
  activeDocId?: string | null;
  /** Byte ranges to outline, e.g. discovered maps. */
  marks?: MarkedRange[];
  /** Rows to prefetch per virtual page step (retained for prop compatibility). */
  pageRows?: number;
  jumpAddress?: number | null;
  onCreateMapFromHex?: (startAddress: number, byteLength: number) => void;
  onPatchByte?: (offset: number, value: number) => Promise<boolean> | boolean | void;
}

const ROW_HEIGHT = 22;
const OVERSCAN_ROWS = 12;
const PAGE_SIZE = 4096;
const MAX_CACHED_PAGES = 16;

export const HexdumpView: React.FC<HexdumpViewProps> = ({
  data,
  romSize: propRomSize,
  romLoaded = false,
  romRevision = 0,
  activeDocId = null,
  marks = [],
  pageRows = 64,
  jumpAddress = null,
  onCreateMapFromHex,
  onPatchByte,
}) => {
  const originalRomData = useMapStore((s) => s.originalRomData);
  const [bytesPerRow, setBytesPerRow] = useState<number>(16);
  const [wordMode, setWordMode] = useState<HexWordMode>('8bit');
  const [numFormat, setNumFormat] = useState<HexNumberFormat>('hex');
  const [showDiffHighlight, setShowDiffHighlight] = useState<boolean>(true);
  const effectiveBytesPerRow = wordMode === '16bit' && bytesPerRow % 2 !== 0 ? bytesPerRow + 1 : bytesPerRow;
  const effectiveLength = data && data.length > 0 ? data.length : romLoaded ? propRomSize ?? 0 : 0;
  const totalRows = Math.ceil(effectiveLength / effectiveBytesPerRow);

  const scrollRef = useRef<HTMLDivElement | null>(null);
  const pagesRef = useRef<Map<number, Uint8Array>>(new Map());
  const inflightRef = useRef<Set<number>>(new Set());
  const pageGenRef = useRef<number>(0);
  const searchSeqRef = useRef<number>(0);
  const committedEditRef = useRef<boolean>(false);

  const [viewportHeight, setViewportHeight] = useState(480);
  const [scrollTop, setScrollTop] = useState(0);
  const [cacheVersion, setCacheVersion] = useState(0);

  const [jumpText, setJumpText] = useState('0x000000');
  const [searchText, setSearchText] = useState('');
  const [searchHits, setSearchHits] = useState<number[]>([]);
  const [searchHitIdx, setSearchHitIdx] = useState(0);
  const [searchMessage, setSearchMessage] = useState<string | null>(null);
  const [editingCell, setEditingCell] = useState<{ offset: number; value: string; byteCount: 1 | 2 } | null>(null);
  const [cursorOffset, setCursorOffset] = useState<number>(0);
  const [selectionRange, setSelectionRange] = useState<{ start: number; end: number } | null>(null);
  const [inspectorEndian, setInspectorEndian] = useState<'big' | 'little'>('big');

  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const updateHeight = () => {
      const h = el.clientHeight;
      if (h > 0) setViewportHeight(h);
    };
    updateHeight();
    const ro = new ResizeObserver(updateHeight);
    ro.observe(el);
    return () => {
      ro.disconnect();
      pageGenRef.current += 1;
      pagesRef.current.clear();
      inflightRef.current.clear();
    };
  }, []);

  useEffect(() => {
    pageGenRef.current += 1;
    pagesRef.current.clear();
    inflightRef.current.clear();
    setCacheVersion((v) => v + 1);
  }, [data, romRevision, activeDocId, effectiveLength]);

  useEffect(() => {
    searchSeqRef.current += 1;
    setSearchHits([]);
    setSearchHitIdx(0);
    setSearchMessage(null);
    setEditingCell(null);
    setSelectionRange(null);
  }, [activeDocId, effectiveLength]);

  const firstVisibleRow = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN_ROWS);
  const visibleRowCount = Math.max(1, Math.ceil(viewportHeight / ROW_HEIGHT) + OVERSCAN_ROWS * 2);
  const lastVisibleRow = Math.min(Math.max(0, totalRows - 1), firstVisibleRow + visibleRowCount);

  useEffect(() => {
    if (effectiveLength === 0) return;
    const gen = pageGenRef.current;
    const startByte = firstVisibleRow * effectiveBytesPerRow;
    const endByte = Math.min(effectiveLength, (lastVisibleRow + 1) * effectiveBytesPerRow);
    const startPage = Math.floor(startByte / PAGE_SIZE);
    const endPage = Math.floor(Math.max(0, endByte - 1) / PAGE_SIZE);
    const neededPages: number[] = [];

    for (let p = startPage; p <= endPage; p++) {
      neededPages.push(p);
    }
    if ((endPage + 1) * PAGE_SIZE < effectiveLength) {
      neededPages.push(endPage + 1);
    }

    for (const pageIdx of neededPages) {
      if (pagesRef.current.has(pageIdx) || inflightRef.current.has(pageIdx)) continue;
      inflightRef.current.add(pageIdx);
      const offset = pageIdx * PAGE_SIZE;
      const length = Math.min(PAGE_SIZE, effectiveLength - offset);
      void bridgeReadRomWindow(offset, length, data)
        .then((bytes) => {
          if (gen !== pageGenRef.current) return;
          const map = pagesRef.current;
          if (map.has(pageIdx)) map.delete(pageIdx);
          map.set(pageIdx, bytes);
          while (map.size > MAX_CACHED_PAGES) {
            const oldestKey = map.keys().next().value;
            if (oldestKey === undefined) break;
            map.delete(oldestKey);
          }
          setCacheVersion((v) => v + 1);
        })
        .finally(() => {
          inflightRef.current.delete(pageIdx);
        });
    }
  }, [effectiveLength, firstVisibleRow, lastVisibleRow, data, romRevision, activeDocId]);

  const readCachedByte = useCallback(
    (offset: number): number | null => {
      if (offset < 0 || offset >= effectiveLength) return null;
      if (data && offset < data.length) {
        return data[offset];
      }
      const pageIdx = Math.floor(offset / PAGE_SIZE);
      const page = pagesRef.current.get(pageIdx);
      if (!page) return null;
      const inPage = offset - pageIdx * PAGE_SIZE;
      return inPage >= 0 && inPage < page.length ? page[inPage] : null;
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [data, cacheVersion, effectiveLength]
  );

  const visibleRows = useMemo(() => {
    if (effectiveLength === 0 || totalRows === 0) return [];
    const step = wordMode === '16bit' ? 2 : 1;
    const rows: Array<{
      offset: number;
      marked: boolean;
      cells: Array<{
        offset: number;
        bytes: Array<number | null>;
        marked: boolean;
        modified: boolean;
        origBytes: number[];
      }>;
      ascii: string;
    }> = [];

    for (let rowIdx = firstVisibleRow; rowIdx <= lastVisibleRow; rowIdx++) {
      const offset = rowIdx * effectiveBytesPerRow;
      const rowEnd = Math.min(offset + effectiveBytesPerRow, effectiveLength);
      const cells: Array<{
        offset: number;
        bytes: Array<number | null>;
        marked: boolean;
        modified: boolean;
        origBytes: number[];
      }> = [];
      let ascii = '';
      let anyMarked = false;

      for (let i = 0; i < rowEnd - offset; i += step) {
        const bOff = offset + i;
        const b0 = readCachedByte(bOff);
        const b1 = step === 2 && bOff + 1 < rowEnd ? readCachedByte(bOff + 1) : null;
        const cellBytes: Array<number | null> = step === 2 && bOff + 1 < rowEnd ? [b0, b1] : [b0];
        const m0 = isMarked(bOff, marks);
        const m1 = step === 2 && bOff + 1 < rowEnd ? isMarked(bOff + 1, marks) : false;
        const marked = m0 || m1;
        if (marked) anyMarked = true;

        const origBytes: number[] = [];
        let modified = false;
        for (let k = 0; k < cellBytes.length; k++) {
          const curVal = cellBytes[k];
          const origVal =
            originalRomData && bOff + k < originalRomData.length
              ? originalRomData[bOff + k]
              : undefined;
          if (origVal !== undefined) origBytes.push(origVal);
          if (showDiffHighlight && origVal !== undefined && curVal !== null && origVal !== curVal) {
            modified = true;
          }
        }

        cells.push({ offset: bOff, bytes: cellBytes, marked, modified, origBytes });
        ascii += b0 === null ? ' ' : printable(b0);
        if (step === 2 && bOff + 1 < rowEnd) {
          ascii += b1 === null ? ' ' : printable(b1);
        }
      }

      rows.push({ offset, marked: anyMarked, cells, ascii });
    }
    return rows;
  }, [
    effectiveLength,
    totalRows,
    firstVisibleRow,
    lastVisibleRow,
    effectiveBytesPerRow,
    wordMode,
    marks,
    readCachedByte,
    originalRomData,
    showDiffHighlight,
  ]);

  const jumpToOffset = useCallback(
    (addr: number) => {
      const clamped = Math.max(0, Math.min(addr, Math.max(0, effectiveLength - 1)));
      const row = Math.max(0, Math.min(Math.floor(clamped / effectiveBytesPerRow), Math.max(0, totalRows - 1)));
      const targetTop = row * ROW_HEIGHT;
      if (scrollRef.current) {
        scrollRef.current.scrollTop = targetTop;
      }
      setScrollTop(targetTop);
      setCursorOffset(clamped);
      setJumpText(`0x${clamped.toString(16).toUpperCase().padStart(6, '0')}`);
    },
    [totalRows, effectiveLength, effectiveBytesPerRow]
  );

  useEffect(() => {
    if (jumpAddress !== null && jumpAddress !== undefined && effectiveLength > 0) {
      jumpToOffset(jumpAddress);
    }
  }, [jumpAddress, effectiveLength, jumpToOffset]);

  const handleJump = () => {
    const addr = parseAddress(jumpText);
    if (addr === null) return;
    jumpToOffset(addr);
  };

  const handleSearch = async () => {
    const pattern = parseHexSearchQuery(searchText);
    if (!pattern) {
      setSearchHits([]);
      setSearchMessage('Enter hex bytes (e.g. DE AD) or "ASCII"');
      return;
    }
    const seq = ++searchSeqRef.current;
    const res = await bridgeSearchRomPattern(pattern, 0, 512, data);
    if (seq !== searchSeqRef.current) return;
    setSearchHits(res.offsets);
    setSearchHitIdx(0);
    if (res.offsets.length === 0) {
      setSearchMessage('No matches');
    } else {
      setSearchMessage(`${res.offsets.length}${res.truncated ? '+' : ''} matches`);
      jumpToOffset(res.offsets[0]);
    }
  };

  const cycleHit = (delta: number) => {
    if (searchHits.length === 0) return;
    const nextIdx = (searchHitIdx + delta + searchHits.length) % searchHits.length;
    setSearchHitIdx(nextIdx);
    jumpToOffset(searchHits[nextIdx]);
  };

  const commitByteEdit = async () => {
    if (!editingCell || committedEditRef.current) return;
    committedEditRef.current = true;
    const { offset, value, byteCount } = editingCell;
    setEditingCell(null);
    const trimmed = value.trim();
    const parsed =
      numFormat === 'dec' ? Number.parseInt(trimmed, 10) : Number.parseInt(trimmed, 16);
    const maxVal = byteCount === 2 ? 0xffff : 0xff;
    if (!Number.isInteger(parsed) || parsed < 0 || parsed > maxVal) return;

    if (byteCount === 2) {
      const hi = (parsed >> 8) & 0xff;
      const lo = parsed & 0xff;
      const nextBytes = inspectorEndian === 'little' ? [lo, hi] : [hi, lo];
      if (onPatchByte) {
        await onPatchByte(offset, nextBytes[0]);
        await onPatchByte(offset + 1, nextBytes[1]);
      } else {
        await useMapStore
          .getState()
          .patchRomBytes(
            [{ offset, bytes: nextBytes }],
            `Hex word @ 0x${offset.toString(16).toUpperCase()}`
          );
      }
      return;
    }

    if (onPatchByte) {
      await onPatchByte(offset, parsed);
    } else {
      await useMapStore
        .getState()
        .patchRomBytes([{ offset, bytes: [parsed] }], `Hex byte @ 0x${offset.toString(16).toUpperCase()}`);
    }
  };

  const inspectorValues = useMemo(() => {
    const b0 = readCachedByte(cursorOffset);
    const b1 = readCachedByte(cursorOffset + 1);
    const b2 = readCachedByte(cursorOffset + 2);
    const b3 = readCachedByte(cursorOffset + 3);
    if (b0 === null) return null;

    const u8 = b0;
    const i8 = u8 > 127 ? u8 - 256 : u8;
    const bin = u8.toString(2).padStart(8, '0');
    let u16: number | null = null;
    let i16: number | null = null;
    if (b1 !== null) {
      u16 = inspectorEndian === 'little' ? b0 | (b1 << 8) : (b0 << 8) | b1;
      i16 = u16 > 32767 ? u16 - 65536 : u16;
    }
    let u32: number | null = null;
    let i32: number | null = null;
    let f32: number | null = null;
    if (b1 !== null && b2 !== null && b3 !== null) {
      const buf = new Uint8Array([b0, b1, b2, b3]);
      const dv = new DataView(buf.buffer);
      const le = inspectorEndian === 'little';
      u32 = dv.getUint32(0, le);
      i32 = dv.getInt32(0, le);
      f32 = dv.getFloat32(0, le);
    }
    return { u8, i8, u16, i16, u32, i32, f32, bin, ascii: printable(u8) };
  }, [cursorOffset, inspectorEndian, readCachedByte]);

  if (effectiveLength === 0) {
    return (
      <div className="flex h-full items-center justify-center p-6 text-center text-xs text-kiviMuted">
        Load a ROM to inspect its hex bytes.
      </div>
    );
  }

  const currentRow = Math.floor(scrollTop / ROW_HEIGHT);
  const activeSearchHit = searchHits[searchHitIdx] ?? -1;
  const selStart = selectionRange ? Math.min(selectionRange.start, selectionRange.end) : cursorOffset;
  const selEnd = selectionRange ? Math.max(selectionRange.start, selectionRange.end) : cursorOffset;
  const activeMark = findContainingMark(cursorOffset, marks);
  const cellsPerRowCount = wordMode === '16bit' ? Math.ceil(effectiveBytesPerRow / 2) : effectiveBytesPerRow;
  const cellWidthClass =
    wordMode === '16bit'
      ? numFormat === 'dec'
        ? 'w-[46px]'
        : 'w-[38px]'
      : numFormat === 'dec'
        ? 'w-[28px]'
        : 'w-[22px]';

  const stepColumns = (delta: number) => {
    const step = wordMode === '16bit' ? 2 : 1;
    setBytesPerRow((prev) => Math.max(4, Math.min(32, prev + delta * step)));
  };

  const jumpPotentialMark = (dir: 'next' | 'prev') => {
    const next = findAdjacentMark(cursorOffset, marks, dir);
    if (next) jumpToOffset(next.start);
  };

  const handleContainerKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    const tag = (e.target as HTMLElement | null)?.tagName;
    if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || e.ctrlKey || e.metaKey || e.altKey) {
      return;
    }
    if (e.key === 'w' || e.key === 'W') {
      e.preventDefault();
      stepColumns(-1);
    } else if (e.key === 'm' || e.key === 'M') {
      e.preventDefault();
      stepColumns(1);
    } else if ((e.key === 'f' || e.key === 'F') && marks.length > 0) {
      e.preventDefault();
      jumpPotentialMark(e.shiftKey ? 'prev' : 'next');
    }
  };

  return (
    <div
      tabIndex={0}
      onKeyDown={handleContainerKeyDown}
      className="flex h-full min-w-0 flex-col bg-kiviBg font-mono text-[11px] text-kiviInk focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-kiviAccent"
    >
      <div className="flex flex-wrap items-center gap-2 border-b border-kiviBorder/50 bg-kiviSurface px-3 py-1.5">
        <span className="shrink-0 whitespace-nowrap text-kiviMuted">Offset</span>
        <input
          type="text"
          aria-label="Jump to address"
          value={jumpText}
          onChange={(e) => setJumpText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') handleJump();
          }}
          className="w-24 shrink-0 rounded border border-kiviBorder/70 bg-kiviBg px-1.5 py-0.5 text-kiviInk tabular-nums focus-visible:outline-2 focus-visible:outline-kiviAccent"
        />
        <button
          type="button"
          onClick={handleJump}
          className="shrink-0 whitespace-nowrap rounded border border-kiviBorder/70 bg-kiviElevated px-2 py-0.5 text-kiviSecondary hover:text-kiviInk"
        >
          Go
        </button>


        {/* Hex View Controls: 8/16-bit, Hex/Dec, Columns (W/M), Diff Highlight */}
        <div
          role="group"
          aria-label="Hex word width"
          className="flex shrink-0 items-center rounded border border-kiviBorder bg-kiviBg p-0.5 text-[10px]"
        >
          <button
            type="button"
            onClick={() => setWordMode('8bit')}
            className={`rounded px-1.5 py-0.5 transition-colors ${
              wordMode === '8bit' ? 'bg-kiviAccent font-semibold text-kiviAccentFg' : 'text-kiviSecondary hover:text-kiviInk'
            }`}
          >
            8b
          </button>
          <button
            type="button"
            onClick={() => setWordMode('16bit')}
            className={`rounded px-1.5 py-0.5 transition-colors ${
              wordMode === '16bit' ? 'bg-kiviAccent font-semibold text-kiviAccentFg' : 'text-kiviSecondary hover:text-kiviInk'
            }`}
          >
            16b
          </button>
        </div>

        <div
          role="group"
          aria-label="Hex number base"
          className="flex shrink-0 items-center rounded border border-kiviBorder bg-kiviBg p-0.5 text-[10px]"
        >
          <button
            type="button"
            onClick={() => setNumFormat('hex')}
            className={`rounded px-1.5 py-0.5 transition-colors ${
              numFormat === 'hex' ? 'bg-kiviAccent font-semibold text-kiviAccentFg' : 'text-kiviSecondary hover:text-kiviInk'
            }`}
          >
            Hex
          </button>
          <button
            type="button"
            onClick={() => setNumFormat('dec')}
            className={`rounded px-1.5 py-0.5 transition-colors ${
              numFormat === 'dec' ? 'bg-kiviAccent font-semibold text-kiviAccentFg' : 'text-kiviSecondary hover:text-kiviInk'
            }`}
          >
            Dec
          </button>
        </div>

        <div className="flex shrink-0 items-center gap-0.5 rounded border border-kiviBorder bg-kiviBg px-1 py-0.5 text-[10px]">
          <button
            type="button"
            aria-label="Narrow columns (W)"
            title="Decrease columns [W]"
            onClick={() => stepColumns(-1)}
            className="rounded px-1 text-kiviSecondary hover:bg-kiviElevated hover:text-kiviInk"
          >
            -
          </button>
          <select
            aria-label="Bytes per row"
            value={bytesPerRow}
            onChange={(e) => setBytesPerRow(Number.parseInt(e.target.value, 10))}
            className="cursor-pointer bg-transparent text-center font-semibold text-kiviInk tabular-nums focus-visible:ring-2 focus-visible:ring-kiviAccent"
            title="Bytes per row (W/M keys)"
          >
            {!HEX_COLUMN_PRESETS.includes(bytesPerRow as (typeof HEX_COLUMN_PRESETS)[number]) && (
              <option value={bytesPerRow} className="bg-kiviSurface text-kiviInk">
                {bytesPerRow}B
              </option>
            )}
            {HEX_COLUMN_PRESETS.map((n) => (
              <option key={n} value={n} className="bg-kiviSurface text-kiviInk">
                {n}B
              </option>
            ))}
          </select>
          <button
            type="button"
            aria-label="Widen columns (M)"
            title="Increase columns [M]"
            onClick={() => stepColumns(1)}
            className="rounded px-1 text-kiviSecondary hover:bg-kiviElevated hover:text-kiviInk"
          >
            +
          </button>
        </div>

        <button
          type="button"
          aria-pressed={showDiffHighlight}
          onClick={() => setShowDiffHighlight((v) => !v)}
          className={`shrink-0 whitespace-nowrap rounded border px-1.5 py-0.5 text-[10px] font-medium transition-colors ${
            showDiffHighlight
              ? 'border-kiviGreen/50 bg-kiviGreen/15 text-kiviGreen'
              : 'border-kiviBorder bg-kiviBg text-kiviMuted hover:text-kiviInk'
          }`}
          title="Highlight bytes modified vs Stock Base ROM"
        >
          Diff
        </button>

        {marks.length > 0 && (
          <div className="flex shrink-0 items-center gap-1 rounded border border-kiviWarn/40 bg-kiviWarn/10 px-1.5 py-0.5 text-[10px] text-kiviWarn">
            <button
              type="button"
              aria-label="Previous potential map"
              title="Previous potential map [Shift+F]"
              onClick={() => jumpPotentialMark('prev')}
              className="rounded px-1 hover:bg-kiviWarn/20"
            >
              &lsaquo;
            </button>
            <span className="whitespace-nowrap font-semibold tabular-nums">
              Maps ({marks.length})
            </span>
            <button
              type="button"
              aria-label="Next potential map"
              title="Next potential map [F]"
              onClick={() => jumpPotentialMark('next')}
              className="rounded px-1 hover:bg-kiviWarn/20"
            >
              &rsaquo;
            </button>
          </div>
        )}


        <input
          type="text"
          aria-label="Search hex or ASCII"
          placeholder='Search DE AD or "ASC"'
          value={searchText}
          onChange={(e) => setSearchText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') void handleSearch();
          }}
          className="min-w-0 flex-1 rounded border border-kiviBorder/70 bg-kiviBg px-2 py-0.5 text-kiviInk focus-visible:outline-2 focus-visible:outline-kiviAccent sm:w-36 sm:flex-none"
        />
        <button
          type="button"
          onClick={() => void handleSearch()}
          className="min-w-[52px] shrink-0 whitespace-nowrap rounded border border-kiviAccent/40 bg-kiviAccent/15 px-2 py-0.5 text-center text-kiviAccent hover:bg-kiviAccent/25"
        >
          Find
        </button>
        {searchHits.length > 0 && (
          <div className="flex items-center gap-1">
            <button
              type="button"
              onClick={() => cycleHit(-1)}
              aria-label="Previous search match"
              className="rounded border border-kiviBorder px-1.5 py-0.5 text-kiviSecondary hover:bg-kiviElevated"
            >
              &lsaquo;
            </button>
            <span className="text-[10px] text-kiviSecondary tabular-nums">
              {searchHitIdx + 1}/{searchHits.length}
            </span>
            <button
              type="button"
              onClick={() => cycleHit(1)}
              aria-label="Next search match"
              className="rounded border border-kiviBorder px-1.5 py-0.5 text-kiviSecondary hover:bg-kiviElevated"
            >
              &rsaquo;
            </button>
          </div>
        )}
        {searchMessage && <span className="text-[10px] text-kiviMuted">{searchMessage}</span>}

        <div className="ml-auto flex shrink-0 items-center gap-1.5">
          {onCreateMapFromHex && activeMark && (
            <button
              type="button"
              onClick={() =>
                onCreateMapFromHex(activeMark.start, Math.max(1, activeMark.end - activeMark.start))
              }
              className="shrink-0 whitespace-nowrap rounded border border-kiviWarn/50 bg-kiviWarn/15 px-2 py-0.5 text-[10px] font-semibold text-kiviWarn hover:bg-kiviWarn/25"
              title={`Promote potential map ${activeMark.name ?? ''} to table definition`}
            >
              Promote Map
            </button>
          )}
          {onCreateMapFromHex && (
            <button
              type="button"
              onClick={() => {
                const start = selectionRange ? selStart : cursorOffset;
                const len = selectionRange ? selEnd - selStart + 1 : 64;
                onCreateMapFromHex(start, len);
              }}
              className="shrink-0 whitespace-nowrap rounded border border-kiviAccent/40 bg-kiviAccent/15 px-2 py-0.5 text-[10px] font-semibold text-kiviAccent hover:bg-kiviAccent/25"
              title="Create calibration map definition starting at cursor/selection"
            >
              Create Map ({selectionRange ? `${selEnd - selStart + 1}B` : `0x${cursorOffset.toString(16).toUpperCase()}`})
            </button>
          )}
          <button
            type="button"
            aria-label="Previous page"
            onClick={() => jumpToOffset(Math.max(0, (currentRow - pageRows) * effectiveBytesPerRow))}
            className="shrink-0 whitespace-nowrap rounded border border-kiviBorder px-2 py-0.5 text-kiviSecondary hover:bg-kiviElevated"
          >
            &lsaquo;
          </button>
          <button
            type="button"
            aria-label="Next page"
            onClick={() => jumpToOffset((currentRow + pageRows) * effectiveBytesPerRow)}
            className="shrink-0 whitespace-nowrap rounded border border-kiviBorder px-2 py-0.5 text-kiviSecondary hover:bg-kiviElevated"
          >
            &rsaquo;
          </button>
        </div>
      </div>

      <div
        ref={scrollRef}
        onScroll={(e) => setScrollTop(e.currentTarget.scrollTop)}
        className="relative flex-1 overflow-auto px-3 py-1"
      >
        <div style={{ height: `${totalRows * ROW_HEIGHT}px`, position: 'relative' }}>
          <div
            style={{
              position: 'absolute',
              top: `${firstVisibleRow * ROW_HEIGHT}px`,
              left: 0,
              right: 0,
            }}
          >
            {visibleRows.map((row) => (
              <div
                key={row.offset}
                style={{ height: `${ROW_HEIGHT}px` }}
                className={`flex items-center gap-3 whitespace-pre ${
                  row.marked ? 'bg-kiviWarn/10 text-kiviWarn' : 'text-kiviInk'
                }`}
              >
                <span className="w-20 shrink-0 text-kiviMuted tabular-nums">
                  {formatOffset(row.offset)}
                </span>

                <div className="flex items-center gap-1">
                  {row.cells.map((cell) => {
                    const isEditing = editingCell?.offset === cell.offset;
                    const cellSpan = cell.bytes.length;
                    const isSearchHit =
                      activeSearchHit >= cell.offset && activeSearchHit < cell.offset + cellSpan;
                    const isCursor =
                      cursorOffset >= cell.offset && cursorOffset < cell.offset + cellSpan;
                    const isSelectedByte =
                      selectionRange !== null &&
                      cell.offset + cellSpan - 1 >= selStart &&
                      cell.offset <= selEnd;
                    if (isEditing) {
                      return (
                        <input
                          key={cell.offset}
                          type="text"
                          autoFocus
                          maxLength={cellSpan === 2 ? (numFormat === 'dec' ? 5 : 4) : numFormat === 'dec' ? 3 : 2}
                          value={editingCell.value}
                          onChange={(e) =>
                            setEditingCell({ ...editingCell, value: e.target.value })
                          }
                          onBlur={() => void commitByteEdit()}
                          onKeyDown={(e) => {
                            if (e.key === 'Enter') {
                              e.preventDefault();
                              void commitByteEdit();
                            } else if (e.key === 'Escape') {
                              committedEditRef.current = true;
                              setEditingCell(null);
                            }
                          }}
                          className={`${cellWidthClass} rounded border border-kiviAccent bg-kiviBg text-center text-[11px] font-bold text-kiviAccent focus-visible:outline-2 focus-visible:outline-kiviAccent`}
                        />
                      );
                    }
                    const displayVal = formatHexCellValue(
                      cell.bytes,
                      wordMode,
                      inspectorEndian,
                      numFormat
                    );
                    const origHint =
                      cell.modified && cell.origBytes.length > 0
                        ? ` (Orig: ${formatHexCellValue(
                            cell.origBytes,
                            cellSpan === 2 ? '16bit' : '8bit',
                            inspectorEndian,
                            numFormat
                          ).trim()})`
                        : '';
                    return (
                      <button
                        key={cell.offset}
                        type="button"
                        onClick={(e) => {
                          if (e.shiftKey) {
                            setSelectionRange({ start: cursorOffset, end: cell.offset });
                            return;
                          }
                          setSelectionRange(null);
                          setCursorOffset(cell.offset);
                        }}
                        onDoubleClick={() => {
                          if (cell.bytes[0] === null) return;
                          if (cellSpan === 2 && cell.bytes[1] === null) return;
                          committedEditRef.current = false;
                          setCursorOffset(cell.offset);
                          setEditingCell({
                            offset: cell.offset,
                            byteCount: cellSpan === 2 ? 2 : 1,
                            value: displayVal.trim(),
                          });
                        }}
                        title={`0x${cell.offset.toString(16).toUpperCase()}${origHint} - Click inspect, Shift+Click select, Double-Click edit`}
                        className={`${cellWidthClass} rounded px-0.5 text-center tabular-nums transition-colors ${
                          isSearchHit
                            ? 'bg-kiviAccent font-bold text-kiviAccentFg'
                            : isCursor
                              ? 'border border-kiviAccent bg-kiviAccent/20 font-bold text-kiviAccent'
                              : cell.modified
                                ? 'border border-kiviGreen/40 bg-kiviGreen/15 font-semibold text-kiviGreen hover:bg-kiviGreen/25'
                                : isSelectedByte
                                  ? 'bg-kiviAccent/15 text-kiviInk'
                                  : cell.marked
                                    ? 'text-kiviWarn hover:bg-kiviWarn/20'
                                    : 'text-kiviInk hover:bg-kiviElevated'
                        }`}
                      >
                        {displayVal}
                      </button>
                    );
                  })}
                  {row.cells.length < cellsPerRowCount &&
                    Array.from({ length: cellsPerRowCount - row.cells.length }).map((_, idx) => (
                      <span key={`pad-${idx}`} className={`${cellWidthClass} text-center opacity-0`}>
                        00
                      </span>
                    ))}
                </div>

                <span className="pl-2 text-kiviSecondary">{row.ascii}</span>
              </div>
            ))}
          </div>
        </div>
      </div>

      {/* Live Data Inspector Footer Strip */}
      <div className="flex min-h-[30px] flex-wrap items-center justify-between gap-2 border-t border-kiviBorder bg-kiviSurface px-3 py-1.5 text-[10px] text-kiviSecondary tabular-nums">
        <div className="flex flex-wrap items-center gap-3">
          <span className="inline-block min-w-[72px] font-semibold text-kiviAccent">
            @ 0x{cursorOffset.toString(16).toUpperCase().padStart(6, '0')}
          </span>
          <button
            type="button"
            onClick={() => setInspectorEndian((e) => (e === 'big' ? 'little' : 'big'))}
            className="w-7 rounded border border-kiviBorder bg-kiviBg px-1.5 py-0.5 text-center text-[10px] font-semibold text-kiviInk hover:border-kiviAccent"
            title="Toggle Data Inspector byte order"
          >
            {inspectorEndian === 'big' ? 'BE' : 'LE'}
          </button>
          {inspectorValues && (
            <>
              <span className="inline-block min-w-[44px]">u8: <strong className="text-kiviInk">{inspectorValues.u8}</strong></span>
              <span className="inline-block min-w-[48px]">i8: <strong className="text-kiviInk">{inspectorValues.i8}</strong></span>
              {inspectorValues.u16 !== null && (
                <span className="inline-block min-w-[126px]">
                  u16: <strong className="text-kiviInk">{inspectorValues.u16}</strong> / i16:{' '}
                  <strong className="text-kiviInk">{inspectorValues.i16}</strong>
                </span>
              )}
              {inspectorValues.u32 !== null && (
                <span className="inline-block min-w-[186px]">
                  u32: <strong className="text-kiviInk">{inspectorValues.u32}</strong> / f32:{' '}
                  <strong className="text-kiviInk">
                    {Number.isFinite(inspectorValues.f32)
                      ? inspectorValues.f32?.toPrecision(5)
                      : 'NaN'}
                  </strong>
                </span>
              )}
              <span className="inline-block min-w-[82px]">bin: <strong className="text-kiviInk">{inspectorValues.bin}</strong></span>
              <span className="inline-block min-w-[48px]">chr: <strong className="text-kiviInk">&lsquo;{inspectorValues.ascii}&rsquo;</strong></span>
            </>
          )}
        </div>
        <span className="text-kiviMuted">
          {effectiveLength.toLocaleString()} B | Click inspect, Shift+Click range, Double-Click edit
        </span>
      </div>
    </div>
  );
};
