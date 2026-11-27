import type { CellDisplayMode, SelectionRange } from '../HeatmapTable';

export function serializeSelectionToTsv(
  matrix: number[][],
  sel: SelectionRange,
  decimals: number
): string {
  const rows: string[] = [];
  for (let r = sel.minR; r <= sel.maxR; r++) {
    const rowVals = matrix[r].slice(sel.minC, sel.maxC + 1);
    rows.push(rowVals.map((v) => v.toFixed(decimals)).join('\t'));
  }
  return rows.join('\n');
}

export function applyTsvPaste(
  matrix: number[][],
  sel: SelectionRange,
  decimals: number,
  text: string
): { next: number[][]; pastedCount: number; range: SelectionRange } | null {
  const numRows = matrix.length;
  const numCols = matrix[0]?.length ?? 0;
  if (!text || text.trim() === '' || numRows === 0 || numCols === 0) return null;

  const lines = text
    .trim()
    .split(/\r?\n/)
    .map((l) => l.split(/[\t, ]+/).map((val) => parseFloat(val.trim())));

  const startR = sel.minR;
  const startC = sel.minC;
  const next = matrix.map((row) => [...row]);
  let pastedCount = 0;

  for (let rOffset = 0; rOffset < lines.length; rOffset++) {
    const targetR = startR + rOffset;
    if (targetR >= numRows) break;
    for (let cOffset = 0; cOffset < lines[rOffset].length; cOffset++) {
      const targetC = startC + cOffset;
      if (targetC >= numCols) break;
      const val = lines[rOffset][cOffset];
      if (!isNaN(val)) {
        next[targetR][targetC] = Number(val.toFixed(decimals));
        pastedCount++;
      }
    }
  }

  if (pastedCount === 0) return null;
  return {
    next,
    pastedCount,
    range: {
      minR: startR,
      maxR: Math.min(numRows - 1, startR + lines.length - 1),
      minC: startC,
      maxC: Math.min(numCols - 1, startC + (lines[0]?.length ?? 1) - 1),
    },
  };
}

export function applyPasteSpecial(
  matrix: number[][],
  sel: SelectionRange,
  decimals: number,
  mode: 'add' | 'multiply',
  text: string
): { next: number[][]; modifiedCount: number } | null {
  const numRows = matrix.length;
  const numCols = matrix[0]?.length ?? 0;
  if (!text || text.trim() === '' || numRows === 0 || numCols === 0) return null;

  const lines = text
    .trim()
    .split(/\r?\n/)
    .map((l) => l.split(/[\t, ]+/).map((val) => parseFloat(val.trim())));

  const startR = sel.minR;
  const startC = sel.minC;
  const next = matrix.map((row) => [...row]);
  let modifiedCount = 0;

  // Single number applied across entire selection
  if (
    lines.length === 1 &&
    lines[0].length === 1 &&
    !isNaN(lines[0][0]) &&
    (sel.maxR > sel.minR || sel.maxC > sel.minC)
  ) {
    const val = lines[0][0];
    for (let r = sel.minR; r <= sel.maxR; r++) {
      for (let c = sel.minC; c <= sel.maxC; c++) {
        const orig = matrix[r][c];
        const result = mode === 'add' ? orig + val : orig * val;
        next[r][c] = Number(result.toFixed(decimals));
        modifiedCount++;
      }
    }
  } else {
    // Multi-cell grid paste
    for (let rOffset = 0; rOffset < lines.length; rOffset++) {
      const targetR = startR + rOffset;
      if (targetR >= numRows) break;
      for (let cOffset = 0; cOffset < lines[rOffset].length; cOffset++) {
        const targetC = startC + cOffset;
        if (targetC >= numCols) break;
        const clipVal = lines[rOffset][cOffset];
        if (!isNaN(clipVal)) {
          const orig = matrix[targetR][targetC];
          const result = mode === 'add' ? orig + clipVal : orig * clipVal;
          next[targetR][targetC] = Number(result.toFixed(decimals));
          modifiedCount++;
        }
      }
    }
  }

  if (modifiedCount === 0) return null;
  return { next, modifiedCount };
}

export function formatHeatmapCellValue(
  val: number,
  r: number,
  c: number,
  displayMode: CellDisplayMode,
  decimals: number,
  baseMatrix?: number[][]
): string {
  switch (displayMode) {
    case 'raw_hex': {
      const rounded = Math.round(val);
      const masked =
        rounded < 0
          ? Math.abs(rounded) <= 128
            ? rounded & 0xff
            : rounded & 0xffff
          : rounded;
      const hex = masked.toString(16).toUpperCase();
      return `0x${hex.length % 2 !== 0 ? '0' + hex : hex}`;
    }
    case 'raw_dec':
      return `${Math.round(val)}`;
    case 'delta': {
      const baseVal = baseMatrix?.[r]?.[c] ?? val;
      const delta = val - baseVal;
      const sign = delta > 0 ? '+' : '';
      return `${sign}${delta.toFixed(decimals)}`;
    }
    case 'percent_delta': {
      const baseVal = baseMatrix?.[r]?.[c] ?? val;
      const delta = val - baseVal;
      const pct = baseVal !== 0 ? (delta / Math.abs(baseVal)) * 100 : delta !== 0 ? 100 : 0;
      const sign = pct > 0 ? '+' : '';
      return `${sign}${pct.toFixed(1)}%`;
    }
    case 'engineering':
    default:
      return val.toFixed(decimals);
  }
}
