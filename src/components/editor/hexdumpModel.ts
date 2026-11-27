/**
 * Hex dump view model.
 *
 * ## Why this exists
 *
 * A side-by-side hex dump displays the raw bytes beside the map tree, with
 * potential maps outlined inline so a tuner can inspect raw cells or spot addresses by eye.
 *
 * ## Layout
 *
 * Classic 16-bytes-per-row layout: offset, sixteen hex pairs, then the ASCII
 * rendering with non-printables as `.`. Addresses are zero-padded to eight
 * digits so columns line up regardless of ROM size.
 *
 * ## Potential-map overlay
 *
 * The dump can mark byte ranges that a scan flagged. Marking is a pure function
 * of the ranges, so the renderer stays dumb and the logic stays testable.
 */

/** One rendered row of the dump. */
export interface HexRow {
  /** Byte offset of the first cell in this row. */
  offset: number;
  /** Hex pairs, space-separated, with gaps padded so columns align. */
  hex: string;
  /** ASCII rendering, one character per byte. */
  ascii: string;
  /** True when any byte in this row is inside a marked range. */
  marked: boolean;
}

/** A byte range to highlight, as `[start, end)` in file offsets. */
export interface MarkedRange {
  start: number;
  end: number;
  name?: string;
}

/** Default bytes per row. */
export const BYTES_PER_ROW = 16;

/** Standard hex column count presets. */
export const HEX_COLUMN_PRESETS = [8, 12, 16, 20, 24, 32] as const;

export type HexWordMode = '8bit' | '16bit';
export type HexNumberFormat = 'hex' | 'dec';

/** Format one 8-bit or 16-bit cell cleanly in hex or fixed-width decimal. */
export function formatHexCellValue(
  bytes: Array<number | null>,
  wordMode: HexWordMode,
  endian: 'big' | 'little',
  numFormat: HexNumberFormat
): string {
  if (wordMode === '8bit' || bytes.length < 2) {
    const b = bytes[0] ?? null;
    if (b === null) return numFormat === 'dec' ? '...' : '..';
    if (numFormat === 'dec') return String(b).padStart(3, ' ');
    return b.toString(16).toUpperCase().padStart(2, '0');
  }
  const b0 = bytes[0] ?? null;
  const b1 = bytes[1] ?? null;
  if (b0 === null || b1 === null) return numFormat === 'dec' ? '.....' : '....';
  const word = endian === 'little' ? b0 | (b1 << 8) : (b0 << 8) | b1;
  if (numFormat === 'dec') return String(word).padStart(5, ' ');
  return word.toString(16).toUpperCase().padStart(4, '0');
}

/** Finds the mark containing `offset`, if any. */
export function findContainingMark(offset: number, marks: MarkedRange[]): MarkedRange | null {
  for (const m of marks) {
    if (offset >= m.start && offset < m.end) return m;
  }
  return null;
}

/**
 * Jumps (`F` / `Shift+F`) to the next or previous marked range
 * relative to `cursorOffset`, wrapping around cleanly.
 */
export function findAdjacentMark(
  cursorOffset: number,
  marks: MarkedRange[],
  direction: 'next' | 'prev'
): MarkedRange | null {
  if (marks.length === 0) return null;
  const sorted = [...marks].sort((a, b) => a.start - b.start);
  if (direction === 'next') {
    const ahead = sorted.find((m) => m.start > cursorOffset);
    return ahead ?? sorted[0];
  }
  for (let i = sorted.length - 1; i >= 0; i--) {
    if (sorted[i].start < cursorOffset) return sorted[i];
  }
  return sorted[sorted.length - 1];
}

/** Format one byte as two upper-case hex digits. */
function hex2(b: number): string {
  return b.toString(16).toUpperCase().padStart(2, '0');
}

/** Format an offset as eight zero-padded hex digits. */
export function formatOffset(offset: number): string {
  return offset.toString(16).toUpperCase().padStart(8, '0');
}

/** Render one byte as ASCII, or `.` when not printable. */
export function printable(b: number): string {
  return b >= 0x20 && b <= 0x7e ? String.fromCharCode(b) : '.';
}

/**
 * Build the rows for `[start, start+length)`.
 *
 * Rows are emitted even when partially outside the data, so a selection that
 * ends mid-row still shows its bytes rather than dropping them.
 */
export function buildHexRows(
  data: Uint8Array,
  start: number,
  length: number,
  marks: MarkedRange[] = [],
  bytesPerRow: number = BYTES_PER_ROW
): HexRow[] {
  if (length <= 0 || bytesPerRow <= 0) return [];
  const end = Math.min(start + length, data.length);
  if (start >= end) return [];

  const rows: HexRow[] = [];
  for (let offset = start; offset < end; offset += bytesPerRow) {
    const rowEnd = Math.min(offset + bytesPerRow, end);
    const slice = data.subarray(offset, rowEnd);

    const parts: string[] = [];
    let ascii = '';
    for (let i = 0; i < bytesPerRow; i++) {
      if (i < slice.length) {
        parts.push(hex2(slice[i]));
        ascii += printable(slice[i]);
      } else {
        // Pad short rows so the ASCII column stays aligned.
        parts.push('  ');
      }
    }

    const marked = marks.some((m) => offset < m.end && m.start < rowEnd);
    rows.push({ offset, hex: parts.join(' '), ascii, marked });
  }
  return rows;
}

/**
 * Whether a byte offset falls inside any marked range.
 *
 * Exposed so a cell-level renderer can highlight individual bytes rather than
 * whole rows.
 */
export function isMarked(offset: number, marks: MarkedRange[]): boolean {
  return marks.some((m) => offset >= m.start && offset < m.end);
}

/**
 * Build windowed rows when only a sub-slice `[sliceStart, sliceStart + slice.length)`
 * is loaded in memory (used by desktop single-owner virtualized hex viewer).
 */
export function buildWindowHexRows(
  slice: Uint8Array,
  sliceStart: number,
  start: number,
  length: number,
  marks: MarkedRange[] = [],
  bytesPerRow: number = BYTES_PER_ROW
): HexRow[] {
  if (length <= 0 || bytesPerRow <= 0 || slice.length === 0) return [];
  const sliceEnd = sliceStart + slice.length;
  const clampedStart = Math.max(start, sliceStart);
  const clampedEnd = Math.min(start + length, sliceEnd);
  if (clampedStart >= clampedEnd) return [];

  const rows: HexRow[] = [];
  for (let offset = clampedStart; offset < clampedEnd; offset += bytesPerRow) {
    const relStart = offset - sliceStart;
    const rowEnd = Math.min(offset + bytesPerRow, clampedEnd);
    const count = rowEnd - offset;
    const parts: string[] = [];
    let ascii = '';
    for (let i = 0; i < bytesPerRow; i++) {
      if (i < count) {
        const b = slice[relStart + i];
        parts.push(hex2(b));
        ascii += printable(b);
      } else {
        parts.push('  ');
      }
    }
    const marked = marks.some((m) => offset < m.end && m.start < rowEnd);
    rows.push({ offset, hex: parts.join(' '), ascii, marked });
  }
  return rows;
}

/**
 * Parse a hex byte query (e.g. `DE AD 10`, `0xDEAD`) or quoted ASCII query (`"KEIH"`).
 */
export function parseHexSearchQuery(raw: string): Uint8Array | null {
  const trimmed = raw.trim();
  if (!trimmed) return null;
  if (
    (trimmed.startsWith('"') && trimmed.endsWith('"') && trimmed.length >= 3) ||
    (trimmed.startsWith("'") && trimmed.endsWith("'") && trimmed.length >= 3)
  ) {
    return new TextEncoder().encode(trimmed.slice(1, -1));
  }
  const tokens = trimmed
    .replace(/0x/gi, '')
    .split(/[\s,]+/)
    .filter(Boolean);
  if (tokens.length === 1 && tokens[0].length > 2 && tokens[0].length % 2 === 0 && /^[0-9a-f]+$/i.test(tokens[0])) {
    const out = new Uint8Array(tokens[0].length / 2);
    for (let i = 0; i < out.length; i++) {
      out[i] = Number.parseInt(tokens[0].slice(i * 2, i * 2 + 2), 16);
    }
    return out;
  }
  const bytes: number[] = [];
  for (const tok of tokens) {
    if (!/^[0-9a-f]{1,2}$/i.test(tok)) return null;
    bytes.push(Number.parseInt(tok, 16));
  }
  return bytes.length > 0 ? new Uint8Array(bytes) : null;
}

/**
 * Parse an address typed by the user.
 *
 * Accepts `0x`-prefixed hex or plain decimal, which is what a tuner types.
 * Returns `null` for anything unparseable rather than silently using 0.
 */
export function parseAddress(text: string): number | null {
  const t = text.trim();
  if (t === '') return null;
  if (/^0x[0-9a-f]+$/i.test(t)) {
    const v = Number.parseInt(t.slice(2), 16);
    return Number.isNaN(v) ? null : v;
  }
  if (/^[0-9]+$/.test(t)) {
    const v = Number.parseInt(t, 10);
    return Number.isNaN(v) ? null : v;
  }
  return null;
}
