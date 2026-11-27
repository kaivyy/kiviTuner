import React, {
  useState,
  useEffect,
  useRef,
  useCallback,
  useMemo,
  useImperativeHandle,
} from 'react';
import { type ColorPalette } from '../../lib/colorRamp';
import {
  runChecksum,
  CHECKSUM_ALGORITHMS,
  type ChecksumAlgorithmId,
  type ChecksumPanelState,
} from './checksumPanel';
import {
  computeSmoothingPreview,
  isPreviewRenderable,
  type SmoothingPreviewState,
} from './smoothingPreview';
import {
  computeMapInterpolation,
  resolveAxisBreakpoints,
  type InterpolatableMap,
  type MapInterpolationState,
} from './mapInterpolation';
import {
  interpolate2D,
  interpolateHorizontal,
  interpolateVertical,
  adjustSelection,
  scaleSelection,
  smooth3x3,
} from '../../lib/interpolation';
import { useCorrelationStore } from '../../stores/useCorrelationStore';
import { useMapStore } from '../../stores/useMapStore';
import {
  resolveCorrelatedCell,
  type CellCoordinate,
} from '../../lib/crosshairModel';
import {
  applyPasteSpecial,
  applyTsvPaste,
  serializeSelectionToTsv,
} from './heatmap/heatmapClipboard';
import type { HeatmapTableHandle } from './heatmap/heatmapCommands';
import { HeatmapAnalysisBar } from './heatmap/HeatmapAnalysisBar';
import { HeatmapToolbar } from './heatmap/HeatmapToolbar';
import { HeatmapFooterBar } from './heatmap/HeatmapFooterBar';
import { HeatmapDialogs } from './heatmap/HeatmapDialogs';
import { HeatmapRow, rowSelectionProps } from './heatmap/HeatmapRow';
import { CurveProfileChart } from './heatmap/CurveProfileChart';
import {
  createHitTracerState,
  recordCellHit,
  type HitTracerState,
} from '../../lib/hitTracerModel';

export interface SelectionRange {
  minR: number;
  maxR: number;
  minC: number;
  maxC: number;
}

export type CellDisplayMode = 'engineering' | 'raw_hex' | 'raw_dec' | 'delta' | 'percent_delta';
export type TablePalette = ColorPalette;

export interface HeatmapTableProps {
  title: string;
  tableId?: string;
  correlatedCell?: CellCoordinate | null;
  matrix: number[][];
  baseMatrix?: number[][];
  cellDisplayMode?: CellDisplayMode;
  palette?: TablePalette;
  xLabels?: (string | number)[];
  yLabels?: (string | number)[];
  xAxisName?: string;
  yAxisName?: string;
  unit?: string;
  decimals?: number;
  readOnly?: boolean;
  selectedRange?: SelectionRange | null;
  onSelectRange?: (range: SelectionRange | null) => void;
  onChangeMatrix: (newMatrix: number[][]) => void;
  onPatchCells?: (edits: Array<{ row: number; col: number; value: number }>, label?: string) => void;
  onCellDisplayModeChange?: (mode: CellDisplayMode) => void;
  onPaletteChange?: (palette: TablePalette) => void;
  /** Active ROM bytes for the checksum panel. `null` disables the panel. */
  checksumRom?: Uint8Array | null;
  /** Enables the read-only smoothing preview controls. */
  enableSmoothingPreview?: boolean;
  /**
   * Engineering axis breakpoints from the map definition, used for the
   * read-only interpolation query. When an axis is `null` the backend falls
   * back to cell indices, and the panel labels its inputs accordingly.
   */
  interpolationAxes?: {
    x: number[] | null;
    y: number[] | null;
  } | null;
  actionRef?: React.RefObject<HeatmapTableHandle | null>;
  onRevertToOriginal?: (selection?: SelectionRange | null) => void;
  onEditMapProperties?: () => void;
  fillHeight?: boolean;
}

export const HeatmapTable: React.FC<HeatmapTableProps> = React.memo(({
  title,
  tableId,
  correlatedCell: propCorrelatedCell,
  matrix,
  baseMatrix,
  cellDisplayMode: propCellDisplayMode,
  palette: propPalette,
  xLabels,
  yLabels,
  xAxisName = 'TPS / Load',
  yAxisName = 'RPM',
  unit = 'Raw',
  decimals = 1,
  readOnly = false,
  selectedRange: propSelectedRange,
  onSelectRange,
  onChangeMatrix,
  onPatchCells,
  onCellDisplayModeChange,
  onPaletteChange,
  checksumRom = null,
  enableSmoothingPreview = false,
  interpolationAxes = null,
  actionRef,
  onRevertToOriginal,
  onEditMapProperties,
  fillHeight = false,
}) => {
  const numRows = matrix.length;
  const numCols = matrix[0]?.length ?? 0;

  const activeCorrelation = useCorrelationStore((s) => s.activeCorrelation);
  const targetTableId = useCorrelationStore((s) => s.targetTableId);
  const workspaceRomLoaded = useMapStore((s) => s.romLoaded);

  const activeCorrelatedCell: CellCoordinate | null = useMemo(() => {
    if (propCorrelatedCell !== undefined) {
      if (
        propCorrelatedCell &&
        propCorrelatedCell.r >= 0 &&
        propCorrelatedCell.r < numRows &&
        propCorrelatedCell.c >= 0 &&
        propCorrelatedCell.c < numCols
      ) {
        return propCorrelatedCell;
      }
      return null;
    }
    return resolveCorrelatedCell({
      matrixDimensions: { numRows, numCols },
      activeCorrelation,
      targetTableId,
      currentTableId: tableId,
    });
  }, [propCorrelatedCell, numRows, numCols, activeCorrelation, targetTableId, tableId]);

  const [internalDisplayMode, setInternalDisplayMode] = useState<CellDisplayMode>('engineering');
  const displayMode = propCellDisplayMode ?? internalDisplayMode;
  const setDisplayMode = useCallback(
    (mode: CellDisplayMode) => {
      setInternalDisplayMode(mode);
      onCellDisplayModeChange?.(mode);
    },
    [onCellDisplayModeChange]
  );

  const [internalPalette, setInternalPalette] = useState<TablePalette>('classic');
  const palette = propPalette ?? internalPalette;
  const setPalette = useCallback(
    (pal: TablePalette) => {
      setInternalPalette(pal);
      onPaletteChange?.(pal);
    },
    [onPaletteChange]
  );

  const [anchorCell, setAnchorCell] = useState<{ r: number; c: number } | null>(null);
  const [cursorCell, setCursorCell] = useState<{ r: number; c: number } | null>(null);
  const [internalSelection, setInternalSelection] = useState<SelectionRange | null>(null);
  const selection = propSelectedRange !== undefined ? propSelectedRange : internalSelection;
  const setSelection = useCallback(
    (newSel: SelectionRange | null) => {
      setInternalSelection(newSel);
      onSelectRange?.(newSel);
    },
    [onSelectRange]
  );
  const [isDragging, setIsDragging] = useState<boolean>(false);

  const [checksumAlgorithm, setChecksumAlgorithm] = useState<ChecksumAlgorithmId>('crc32_ieee');
  const [checksumState, setChecksumState] = useState<ChecksumPanelState | null>(null);
  const [checksumPending, setChecksumPending] = useState<boolean>(false);
  const checksumRequestRef = useRef(0);

  const [previewAlpha, setPreviewAlpha] = useState<number>(0.5);
  const [previewEnabled, setPreviewEnabled] = useState<boolean>(false);
  const [previewState, setPreviewState] = useState<SmoothingPreviewState | null>(null);
  const [previewPending, setPreviewPending] = useState<boolean>(false);
  const previewRequestRef = useRef(0);

  useEffect(() => {
    setPreviewState(null);
    setPreviewEnabled(false);
    previewRequestRef.current += 1;
  }, [tableId]);

  const [interpX, setInterpX] = useState<string>('');
  const [interpY, setInterpY] = useState<string>('');
  const [interpState, setInterpState] = useState<MapInterpolationState | null>(null);
  const [interpPending, setInterpPending] = useState<boolean>(false);
  const interpRequestRef = useRef(0);

  useEffect(() => {
    setInterpState(null);
    setInterpX('');
    setInterpY('');
    interpRequestRef.current += 1;
  }, [tableId]);

  const [editingCell, setEditingCell] = useState<{ r: number; c: number; value: string } | null>(null);
  const [isPercentModalOpen, setIsPercentModalOpen] = useState<boolean>(false);
  const [percentInput, setPercentInput] = useState<string>('5');
  const [isPasteSpecialOpen, setIsPasteSpecialOpen] = useState<boolean>(false);
  const [pasteSpecialMode, setPasteSpecialMode] = useState<'add' | 'multiply'>('add');
  const [pasteSpecialValue, setPasteSpecialValue] = useState<string>('5');
  const [isSetConstantOpen, setIsSetConstantOpen] = useState<boolean>(false);
  const [constantInput, setConstantInput] = useState<string>('0');
  const [showSliceChart, setShowSliceChart] = useState<boolean>(false);
  const [hitTraceEnabled, setHitTraceEnabled] = useState<boolean>(true);
  const [hitTracer, setHitTracer] = useState<HitTracerState>(() => createHitTracerState());

  useEffect(() => {
    setHitTracer(createHitTracerState());
  }, [tableId, numRows, numCols]);

  useEffect(() => {
    if (!hitTraceEnabled || !activeCorrelatedCell) return;
    setHitTracer((prev) =>
      recordCellHit(prev, activeCorrelatedCell.r, activeCorrelatedCell.c)
    );
  }, [activeCorrelatedCell, hitTraceEnabled]);
  const [toastMessage, setToastMessage] = useState<string | null>(null);
  const toastTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    return () => {
      if (toastTimeoutRef.current) clearTimeout(toastTimeoutRef.current);
    };
  }, []);

  const showToast = useCallback((msg: string) => {
    if (toastTimeoutRef.current) clearTimeout(toastTimeoutRef.current);
    setToastMessage(msg);
    toastTimeoutRef.current = setTimeout(() => setToastMessage(null), 2500);
  }, []);

  const { minVal, maxVal } = useMemo(() => {
    let min = Infinity;
    let max = -Infinity;
    for (let r = 0; r < matrix.length; r++) {
      for (let c = 0; c < matrix[r].length; c++) {
        const v = matrix[r][c];
        if (v < min) min = v;
        if (v > max) max = v;
      }
    }
    return {
      minVal: Number.isFinite(min) ? min : 0,
      maxVal: Number.isFinite(max) ? max : 100,
    };
  }, [matrix]);

  const selectionStats = useMemo(() => {
    if (!selection) return null;
    let count = 0;
    let sum = 0;
    let min = Infinity;
    let max = -Infinity;
    for (let r = selection.minR; r <= selection.maxR; r++) {
      for (let c = selection.minC; c <= selection.maxC; c++) {
        const v = matrix[r]?.[c];
        if (v !== undefined) {
          count++;
          sum += v;
          if (v < min) min = v;
          if (v > max) max = v;
        }
      }
    }
    if (count === 0) return null;
    return {
      count,
      rows: selection.maxR - selection.minR + 1,
      cols: selection.maxC - selection.minC + 1,
      min,
      max,
      avg: sum / count,
    };
  }, [matrix, selection]);

  const activeSelection: SelectionRange = useMemo(
    () => selection ?? { minR: 0, maxR: 0, minC: 0, maxC: 0 },
    [selection]
  );

  useEffect(() => {
    const handleGlobalMouseUp = () => {
      if (isDragging) setIsDragging(false);
    };
    window.addEventListener('mouseup', handleGlobalMouseUp);
    return () => window.removeEventListener('mouseup', handleGlobalMouseUp);
  }, [isDragging]);

  const commitEdit = useCallback(() => {
    if (!editingCell || readOnly) return;
    const { r, c, value } = editingCell;
    const parsed = parseFloat(value);
    if (!isNaN(parsed)) {
      const rounded = Number(parsed.toFixed(decimals));
      if (onPatchCells) {
        onPatchCells([{ row: r, col: c, value: rounded }], `Edit ${title} [${r},${c}]`);
      } else {
        const next = matrix.map((row) => [...row]);
        next[r][c] = rounded;
        onChangeMatrix(next);
      }
    }
    setEditingCell(null);
  }, [editingCell, readOnly, matrix, decimals, onChangeMatrix, onPatchCells, title]);

  const handleCellMouseDown = useCallback((r: number, c: number, e: React.MouseEvent) => {
    if (editingCell) commitEdit();
    e.preventDefault();
    containerRef.current?.focus();
    if (e.shiftKey && anchorCell) {
      setCursorCell({ r, c });
      setSelection({
        minR: Math.min(anchorCell.r, r),
        maxR: Math.max(anchorCell.r, r),
        minC: Math.min(anchorCell.c, c),
        maxC: Math.max(anchorCell.c, c),
      });
    } else {
      setAnchorCell({ r, c });
      setCursorCell({ r, c });
      setSelection({ minR: r, maxR: r, minC: c, maxC: c });
      setIsDragging(true);
    }
  }, [editingCell, commitEdit, anchorCell, setSelection]);

  const handleCellMouseEnter = useCallback((r: number, c: number) => {
    if (!isDragging || !anchorCell) return;
    setCursorCell({ r, c });
    setSelection({
      minR: Math.min(anchorCell.r, r),
      maxR: Math.max(anchorCell.r, r),
      minC: Math.min(anchorCell.c, c),
      maxC: Math.max(anchorCell.c, c),
    });
  }, [isDragging, anchorCell, setSelection]);

  const handleRowHeaderClick = useCallback((r: number) => {
    setAnchorCell({ r, c: 0 });
    setCursorCell({ r, c: numCols - 1 });
    setSelection({ minR: r, maxR: r, minC: 0, maxC: numCols - 1 });
    containerRef.current?.focus();
  }, [numCols, setSelection]);

  const handleColHeaderClick = (c: number) => {
    setAnchorCell({ r: 0, c });
    setCursorCell({ r: numRows - 1, c });
    setSelection({ minR: 0, maxR: numRows - 1, minC: c, maxC: c });
    containerRef.current?.focus();
  };

  const handleSelectAll = useCallback(() => {
    if (numRows === 0 || numCols === 0) return;
    setAnchorCell({ r: 0, c: 0 });
    setCursorCell({ r: numRows - 1, c: numCols - 1 });
    setSelection({ minR: 0, maxR: numRows - 1, minC: 0, maxC: numCols - 1 });
    containerRef.current?.focus();
  }, [numRows, numCols, setSelection]);

  const stepSize = decimals >= 2 ? 0.25 : 1;
  const handleStepAdjust = useCallback(
    (delta: number) => {
      if (readOnly || numRows === 0 || numCols === 0) return;
      const sel = activeSelection;
      const count = (sel.maxR - sel.minR + 1) * (sel.maxC - sel.minC + 1);
      if (onPatchCells && count <= 32) {
        const edits: Array<{ row: number; col: number; value: number }> = [];
        for (let r = sel.minR; r <= sel.maxR; r++) {
          for (let c = sel.minC; c <= sel.maxC; c++) {
            const cur = matrix[r]?.[c];
            if (cur !== undefined) {
              edits.push({ row: r, col: c, value: Number((cur + delta).toFixed(decimals)) });
            }
          }
        }
        onPatchCells(edits, `Step ${title} (${delta >= 0 ? '+' : ''}${delta})`);
        return;
      }
      onChangeMatrix(adjustSelection(matrix, sel.minR, sel.maxR, sel.minC, sel.maxC, delta, decimals));
    },
    [readOnly, numRows, numCols, activeSelection, matrix, decimals, onChangeMatrix, onPatchCells, title]
  );

  const handleInterpolate2D = useCallback(() => {
    if (readOnly || numRows === 0 || numCols === 0) return;
    const sel = activeSelection;
    onChangeMatrix(interpolate2D(matrix, sel.minR, sel.maxR, sel.minC, sel.maxC));
    showToast(`Bilinear interpolated ${sel.maxR - sel.minR + 1}x${sel.maxC - sel.minC + 1} cells`);
  }, [readOnly, numRows, numCols, activeSelection, matrix, onChangeMatrix, showToast]);

  const handleInterpolateHorizontal = useCallback(() => {
    if (readOnly || numRows === 0 || numCols === 0) return;
    const sel = activeSelection;
    onChangeMatrix(interpolateHorizontal(matrix, sel.minR, sel.maxR, sel.minC, sel.maxC));
    showToast('Horizontal interpolated rows');
  }, [readOnly, numRows, numCols, activeSelection, matrix, onChangeMatrix, showToast]);

  const handleInterpolateVertical = useCallback(() => {
    if (readOnly || numRows === 0 || numCols === 0) return;
    const sel = activeSelection;
    onChangeMatrix(interpolateVertical(matrix, sel.minR, sel.maxR, sel.minC, sel.maxC));
    showToast('Vertical interpolated columns');
  }, [readOnly, numRows, numCols, activeSelection, matrix, onChangeMatrix, showToast]);

  const handleSmooth3x3 = useCallback(() => {
    if (readOnly || numRows === 0 || numCols === 0) return;
    const sel = activeSelection;
    onChangeMatrix(smooth3x3(matrix, sel.minR, sel.maxR, sel.minC, sel.maxC, decimals));
    showToast(`Smoothed 3x3 on selection (${sel.maxR - sel.minR + 1}x${sel.maxC - sel.minC + 1} cells)`);
  }, [readOnly, numRows, numCols, activeSelection, matrix, decimals, onChangeMatrix, showToast]);

  const handleApplyPercentage = useCallback(
    (pct: number) => {
      if (readOnly || numRows === 0 || numCols === 0 || isNaN(pct)) return;
      const sel = activeSelection;
      onChangeMatrix(scaleSelection(matrix, sel.minR, sel.maxR, sel.minC, sel.maxC, pct, decimals));
      setIsPercentModalOpen(false);
      showToast(`Adjusted selection by ${pct > 0 ? '+' : ''}${pct}%`);
    },
    [readOnly, numRows, numCols, activeSelection, matrix, decimals, onChangeMatrix, showToast]
  );

  const handleCopyTSV = useCallback(async () => {
    if (numRows === 0 || numCols === 0) return;
    const text = serializeSelectionToTsv(matrix, activeSelection, decimals);
    try {
      await navigator.clipboard.writeText(text);
      showToast(
        `Copied ${activeSelection.maxR - activeSelection.minR + 1}x${activeSelection.maxC - activeSelection.minC + 1} cells (TSV)`
      );
    } catch {
      showToast('Clipboard copy failed');
    }
  }, [numRows, numCols, activeSelection, matrix, decimals, showToast]);

  const handlePasteTSV = useCallback(
    async (clipboardText?: string) => {
      if (readOnly || numRows === 0 || numCols === 0) return;
      let text = clipboardText;
      if (!text) {
        try {
          text = await navigator.clipboard.readText();
        } catch {
          showToast('Clipboard read failed');
          return;
        }
      }
      const res = applyTsvPaste(matrix, activeSelection, decimals, text);
      if (!res) return;
      onChangeMatrix(res.next);
      setSelection(res.range);
      showToast(`Pasted ${res.pastedCount} cells from clipboard`);
    },
    [readOnly, numRows, numCols, activeSelection, matrix, decimals, onChangeMatrix, setSelection, showToast]
  );

  const handlePasteSpecial = useCallback(
    async (mode: 'add' | 'multiply', explicitText?: string) => {
      if (readOnly || numRows === 0 || numCols === 0) return;
      let text = explicitText;
      if (!text) {
        try {
          text = await navigator.clipboard.readText();
        } catch {
          showToast('Clipboard read failed');
          return;
        }
      }
      if (!text || text.trim() === '') {
        showToast('Clipboard is empty');
        return;
      }
      const res = applyPasteSpecial(matrix, activeSelection, decimals, mode, text);
      if (!res) return;
      onChangeMatrix(res.next);
      setIsPasteSpecialOpen(false);
      showToast(
        `Paste Special (${mode === 'add' ? 'Added' : 'Multiplied'}) ${res.modifiedCount} cells`
      );
    },
    [readOnly, numRows, numCols, activeSelection, matrix, decimals, onChangeMatrix, showToast]
  );

  const handleApplySetConstant = useCallback(
    (constantVal: number) => {
      if (readOnly || numRows === 0 || numCols === 0 || !Number.isFinite(constantVal)) return;
      const rounded = Number(constantVal.toFixed(decimals));
      const sel = activeSelection;
      const edits: Array<{ row: number; col: number; value: number }> = [];
      const next = matrix.map((row, r) =>
        row.map((val, c) => {
          if (r >= sel.minR && r <= sel.maxR && c >= sel.minC && c <= sel.maxC) {
            edits.push({ row: r, col: c, value: rounded });
            return rounded;
          }
          return val;
        })
      );
      if (onPatchCells && edits.length <= 64) {
        onPatchCells(edits, `Set Constant (${rounded}) on ${title}`);
      } else {
        onChangeMatrix(next);
      }
      setIsSetConstantOpen(false);
      showToast(`Assigned ${rounded} to ${edits.length} cells`);
    },
    [readOnly, numRows, numCols, decimals, activeSelection, matrix, onPatchCells, onChangeMatrix, title, showToast]
  );

  const handleRevertSelection = useCallback(() => {
    if (readOnly || numRows === 0 || numCols === 0) return;
    if (onRevertToOriginal) {
      onRevertToOriginal(selection ?? activeSelection);
      return;
    }
    if (!baseMatrix) {
      showToast('No stock baseline loaded for revert');
      return;
    }
    const sel = activeSelection;
    const edits: Array<{ row: number; col: number; value: number }> = [];
    const next = matrix.map((row, r) =>
      row.map((val, c) => {
        if (r >= sel.minR && r <= sel.maxR && c >= sel.minC && c <= sel.maxC) {
          const stock = baseMatrix[r]?.[c];
          if (stock !== undefined && stock !== val) {
            edits.push({ row: r, col: c, value: stock });
            return stock;
          }
        }
        return val;
      })
    );
    if (edits.length === 0) {
      showToast('Selection already matches stock');
      return;
    }
    if (onPatchCells && edits.length <= 64) {
      onPatchCells(edits, `Revert ${edits.length} cell(s) on ${title}`);
    } else {
      onChangeMatrix(next);
    }
    showToast(`Reverted ${edits.length} cell(s) to stock`);
  }, [readOnly, numRows, numCols, onRevertToOriginal, selection, activeSelection, baseMatrix, matrix, onPatchCells, onChangeMatrix, title, showToast]);

  const handleInvertSelection = useCallback(() => {
    if (numRows === 0 || numCols === 0) return;
    const sel = activeSelection;
    const coversAll =
      sel.minR === 0 && sel.maxR === numRows - 1 && sel.minC === 0 && sel.maxC === numCols - 1;
    if (coversAll) {
      const target = cursorCell || anchorCell || { r: 0, c: 0 };
      setSelection({ minR: target.r, maxR: target.r, minC: target.c, maxC: target.c });
      return;
    }
    if (sel.minR > 0) {
      setSelection({ minR: 0, maxR: sel.minR - 1, minC: 0, maxC: numCols - 1 });
    } else if (sel.maxR < numRows - 1) {
      setSelection({ minR: sel.maxR + 1, maxR: numRows - 1, minC: 0, maxC: numCols - 1 });
    } else if (sel.minC > 0) {
      setSelection({ minR: 0, maxR: numRows - 1, minC: 0, maxC: sel.minC - 1 });
    } else if (sel.maxC < numCols - 1) {
      setSelection({ minR: 0, maxR: numRows - 1, minC: sel.maxC + 1, maxC: numCols - 1 });
    }
  }, [numRows, numCols, activeSelection, cursorCell, anchorCell, setSelection]);

  useImperativeHandle(
    actionRef,
    () => ({
      copySelection: () => void handleCopyTSV(),
      pasteSelection: () => void handlePasteTSV(),
      pasteSpecial: (mode) => {
        if (mode === 'multiply' || mode === 'add') {
          setPasteSpecialMode(mode);
          setIsPasteSpecialOpen(true);
        } else {
          setDisplayMode('raw_hex');
        }
      },
      selectAll: handleSelectAll,
      invertSelection: handleInvertSelection,
      clearSelection: () => {
        setAnchorCell(null);
        setCursorCell(null);
        setSelection(null);
      },
      openMathDialog: () => setIsPercentModalOpen(true),
      applySmooth3x3: handleSmooth3x3,
      applyInterpolate: (mode) => {
        if (mode === 'horizontal') handleInterpolateHorizontal();
        else if (mode === 'vertical') handleInterpolateVertical();
        else handleInterpolate2D();
      },
      revertSelectionToOriginal: handleRevertSelection,
    }),
    [
      handleCopyTSV,
      handlePasteTSV,
      setDisplayMode,
      handleSelectAll,
      handleInvertSelection,
      setSelection,
      handleSmooth3x3,
      handleInterpolateHorizontal,
      handleInterpolateVertical,
      handleInterpolate2D,
      handleRevertSelection,
    ]
  );

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (isPercentModalOpen || isPasteSpecialOpen || isSetConstantOpen) {
      if (e.key === 'Escape') {
        e.preventDefault();
        setIsPercentModalOpen(false);
        setIsPasteSpecialOpen(false);
        setIsSetConstantOpen(false);
      }
      return;
    }
    if (editingCell) {
      if (e.key === 'Enter') {
        e.preventDefault();
        commitEdit();
        if (editingCell.r < numRows - 1) {
          const nextR = editingCell.r + 1;
          setAnchorCell({ r: nextR, c: editingCell.c });
          setCursorCell({ r: nextR, c: editingCell.c });
          setSelection({ minR: nextR, maxR: nextR, minC: editingCell.c, maxC: editingCell.c });
        }
      } else if (e.key === 'Escape') {
        e.preventDefault();
        setEditingCell(null);
      }
      return;
    }

    const noModifier = !e.ctrlKey && !e.metaKey && !e.altKey;

    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'c') {
      e.preventDefault();
      handleCopyTSV();
      return;
    }
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'v') {
      e.preventDefault();
      if (e.shiftKey) setIsPasteSpecialOpen(true);
      else handlePasteTSV();
      return;
    }
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'a') {
      e.preventDefault();
      handleSelectAll();
      return;
    }
    if (e.key === '+' || (e.key === '=' && e.shiftKey)) {
      e.preventDefault();
      handleStepAdjust(stepSize);
      return;
    }
    if (e.key === '=' && noModifier && !e.shiftKey) {
      e.preventDefault();
      const target = cursorCell || anchorCell || { r: activeSelection.minR, c: activeSelection.minC };
      const currentVal = matrix[target.r]?.[target.c] ?? 0;
      setConstantInput(currentVal.toFixed(decimals));
      setIsSetConstantOpen(true);
      return;
    }
    if (e.key === '*') {
      e.preventDefault();
      setPasteSpecialMode('multiply');
      setIsPasteSpecialOpen(true);
      return;
    }
    if (e.key === '-' || e.key === '_') {
      e.preventDefault();
      handleStepAdjust(-stepSize);
      return;
    }
    if ((e.key === 'Backspace' || e.key === 'Delete') && noModifier) {
      e.preventDefault();
      handleRevertSelection();
      return;
    }
    if (noModifier && e.key.toLowerCase() === 's') {
      e.preventDefault();
      handleSmooth3x3();
      return;
    }
    if (noModifier && e.key.toLowerCase() === 'i') {
      e.preventDefault();
      handleInterpolate2D();
      return;
    }
    if (noModifier && e.key.toLowerCase() === 'h') {
      e.preventDefault();
      handleInterpolateHorizontal();
      return;
    }
    if (noModifier && e.key.toLowerCase() === 'v') {
      e.preventDefault();
      handleInterpolateVertical();
      return;
    }
    if (noModifier && (e.key.toLowerCase() === 'p' || e.key === '%')) {
      e.preventDefault();
      setIsPercentModalOpen(true);
      return;
    }
    if (e.key === 'Enter') {
      e.preventDefault();
      const target = cursorCell || anchorCell || { r: activeSelection.minR, c: activeSelection.minC };
      const currentVal = matrix[target.r]?.[target.c] ?? 0;
      setEditingCell({ r: target.r, c: target.c, value: currentVal.toFixed(decimals) });
      return;
    }

    if (['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight'].includes(e.key)) {
      e.preventDefault();
      const anchor = anchorCell || { r: activeSelection.minR, c: activeSelection.minC };
      const current = e.shiftKey ? (cursorCell || anchor) : anchor;
      let newR = current.r;
      let newC = current.c;
      if (e.key === 'ArrowUp') newR = Math.max(0, current.r - 1);
      if (e.key === 'ArrowDown') newR = Math.min(numRows - 1, current.r + 1);
      if (e.key === 'ArrowLeft') newC = Math.max(0, current.c - 1);
      if (e.key === 'ArrowRight') newC = Math.min(numCols - 1, current.c + 1);
      if (e.shiftKey) {
        if (!anchorCell) setAnchorCell(anchor);
        setCursorCell({ r: newR, c: newC });
        setSelection({
          minR: Math.min(anchor.r, newR),
          maxR: Math.max(anchor.r, newR),
          minC: Math.min(anchor.c, newC),
          maxC: Math.max(anchor.c, newC),
        });
      } else {
        setAnchorCell({ r: newR, c: newC });
        setCursorCell({ r: newR, c: newC });
        setSelection({ minR: newR, maxR: newR, minC: newC, maxC: newC });
      }
    }
  };

  const handleContainerPaste = (e: React.ClipboardEvent) => {
    if (editingCell) return;
    const text = e.clipboardData.getData('text');
    if (text) {
      e.preventDefault();
      handlePasteTSV(text);
    }
  };

  const handleComputeChecksum = useCallback(async () => {
    if ((!checksumRom || checksumRom.length === 0) && !workspaceRomLoaded) {
      setChecksumState({ kind: 'no-rom' });
      return;
    }
    const requestId = ++checksumRequestRef.current;
    setChecksumPending(true);
    setChecksumState(null);
    const result = await runChecksum(checksumAlgorithm, checksumRom, workspaceRomLoaded);
    if (requestId !== checksumRequestRef.current) return;
    setChecksumState(result);
    setChecksumPending(false);
  }, [checksumAlgorithm, checksumRom, workspaceRomLoaded]);

  const runPreview = useCallback(
    async (alpha: number) => {
      const requestId = ++previewRequestRef.current;
      setPreviewPending(true);
      const result = await computeSmoothingPreview(tableId ?? '', matrix, alpha);
      if (requestId !== previewRequestRef.current) return;
      setPreviewState(result);
      setPreviewPending(false);
    },
    [matrix, tableId]
  );

  const handleTogglePreview = useCallback(() => {
    if (previewEnabled) {
      setPreviewEnabled(false);
      setPreviewState(null);
      previewRequestRef.current += 1;
      return;
    }
    setPreviewEnabled(true);
    void runPreview(previewAlpha);
  }, [previewAlpha, previewEnabled, runPreview]);

  const handleAlphaChange = useCallback(
    (next: number) => {
      setPreviewAlpha(next);
      if (previewEnabled) void runPreview(next);
    },
    [previewEnabled, runPreview]
  );

  const previewMatrix: number[][] | null =
    previewEnabled && previewState?.kind === 'ok' && isPreviewRenderable(previewState.report.smoothed)
      ? previewState.report.smoothed
      : null;

  const interpXAxis = useMemo(
    () => resolveAxisBreakpoints(interpolationAxes?.x ?? null, numCols),
    [interpolationAxes?.x, numCols]
  );
  const interpYAxis = useMemo(
    () => resolveAxisBreakpoints(interpolationAxes?.y ?? null, numRows),
    [interpolationAxes?.y, numRows]
  );
  const interpMapShape: InterpolatableMap = useMemo(
    () => ({
      rows: numRows,
      cols: numCols,
      xAxisValues: interpolationAxes?.x ?? null,
      yAxisValues: interpolationAxes?.y ?? null,
    }),
    [numRows, numCols, interpolationAxes?.x, interpolationAxes?.y]
  );

  const handleRunInterpolation = useCallback(async () => {
    const x = Number(interpX);
    const y = Number(interpY);
    const requestId = ++interpRequestRef.current;
    setInterpPending(true);
    const result = await computeMapInterpolation(tableId ?? '', interpMapShape, matrix, x, y);
    if (requestId !== interpRequestRef.current) return;
    setInterpState(result);
    setInterpPending(false);
  }, [interpX, interpY, interpMapShape, matrix, tableId]);

  const tableScrollRef = useRef<HTMLDivElement>(null);
  const [tableScrollTop, setTableScrollTop] = useState(0);
  const [tableViewportH, setTableViewportH] = useState(500);

  useEffect(() => {
    const el = tableScrollRef.current;
    if (!el) return;
    const syncH = () => {
      if (el.clientHeight > 0) setTableViewportH(el.clientHeight);
    };
    syncH();
    const ro = new ResizeObserver(syncH);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const virtualizeRows = numRows > 48;
  const virtualStartRow = virtualizeRows ? Math.max(0, Math.floor(tableScrollTop / 28) - 8) : 0;
  const virtualEndRow = virtualizeRows
    ? Math.min(numRows - 1, virtualStartRow + Math.ceil(tableViewportH / 28) + 16)
    : numRows - 1;

  const hasChecksumRom = workspaceRomLoaded || (!!checksumRom && checksumRom.length > 0);
  const selectedChecksumOption = CHECKSUM_ALGORITHMS.find((a) => a.id === checksumAlgorithm);
  const canComputeChecksum = hasChecksumRom && !!selectedChecksumOption?.supported && !checksumPending;

  const is1D = numRows === 1 || numCols === 1;
  const activeSliceRowIdx = Math.max(
    0,
    Math.min(numRows - 1, cursorCell?.r ?? anchorCell?.r ?? activeSelection.minR)
  );
  const curveValues = useMemo(() => {
    if (numRows === 0 || numCols === 0) return [];
    if (numCols === 1 && numRows > 1) return matrix.map((r) => r[0] ?? 0);
    return matrix[activeSliceRowIdx] ?? [];
  }, [matrix, numRows, numCols, activeSliceRowIdx]);
  const baseCurveValues = useMemo(() => {
    if (!baseMatrix || baseMatrix.length === 0) return null;
    if (numCols === 1 && numRows > 1) return baseMatrix.map((r) => r[0] ?? 0);
    return baseMatrix[activeSliceRowIdx] ?? null;
  }, [baseMatrix, numRows, numCols, activeSliceRowIdx]);
  const curveLabels = useMemo(() => {
    if (numCols === 1 && numRows > 1) return yLabels ?? matrix.map((_, idx) => idx);
    return xLabels ?? (matrix[0] ? matrix[0].map((_, idx) => idx) : []);
  }, [numCols, numRows, xLabels, yLabels, matrix]);

  return (
    <div
      ref={containerRef}
      tabIndex={0}
      onKeyDown={handleKeyDown}
      onPaste={handleContainerPaste}
      className={`relative flex w-full min-w-0 flex-col rounded-lg border border-kiviBorder bg-kiviSurface shadow-md focus-visible:outline-2 focus-visible:outline-kiviAccent ${
        fillHeight ? 'h-full min-h-0 flex-1 overflow-hidden' : ''
      }`}
    >
      <HeatmapAnalysisBar
        showChecksum={checksumRom !== undefined}
        hasChecksumRom={hasChecksumRom}
        checksumAlgorithm={checksumAlgorithm}
        onChangeChecksumAlgorithm={(algo) => {
          setChecksumAlgorithm(algo);
          setChecksumState(null);
        }}
        checksumState={checksumState}
        checksumPending={checksumPending}
        canComputeChecksum={canComputeChecksum}
        onComputeChecksum={() => void handleComputeChecksum()}
        enableSmoothingPreview={enableSmoothingPreview}
        previewEnabled={previewEnabled}
        previewPending={previewPending}
        previewAlpha={previewAlpha}
        previewState={previewState}
        onTogglePreview={handleTogglePreview}
        onAlphaChange={handleAlphaChange}
        showInterpolation={interpolationAxes !== undefined}
        interpXAxis={interpXAxis}
        interpYAxis={interpYAxis}
        interpX={interpX}
        interpY={interpY}
        onChangeInterpX={setInterpX}
        onChangeInterpY={setInterpY}
        interpPending={interpPending}
        interpState={interpState}
        onRunInterpolation={() => void handleRunInterpolation()}
      />

      <HeatmapToolbar
        title={title}
        unit={unit}
        numRows={numRows}
        numCols={numCols}
        stepSize={stepSize}
        displayMode={displayMode}
        palette={palette}
        hasBaseMatrix={Boolean(baseMatrix && baseMatrix.length > 0)}
        is1D={is1D}
        showSliceChart={showSliceChart}
        onToggleSliceChart={() => setShowSliceChart((v) => !v)}
        hitTraceEnabled={hitTraceEnabled}
        hitTotalCount={hitTracer.totalHits}
        onToggleHitTrace={() => setHitTraceEnabled((v) => !v)}
        onResetHitTrace={() => setHitTracer(createHitTracerState())}
        onStepAdjust={handleStepAdjust}
        onInterpolate2D={handleInterpolate2D}
        onInterpolateHorizontal={handleInterpolateHorizontal}
        onInterpolateVertical={handleInterpolateVertical}
        onSmooth3x3={handleSmooth3x3}
        onOpenPercentModal={() => setIsPercentModalOpen(true)}
        onRevertToOriginal={handleRevertSelection}
        onEditMapProperties={onEditMapProperties}
        onCopyTSV={handleCopyTSV}
        onPasteTSV={() => handlePasteTSV()}
        onOpenPasteSpecial={() => setIsPasteSpecialOpen(true)}
        onChangeDisplayMode={setDisplayMode}
        onChangePalette={setPalette}
      />

      {(is1D || showSliceChart) && curveValues.length > 1 && (
        <div className="border-b border-kiviBorder p-2">
          <CurveProfileChart
            title={numCols === 1 && numRows > 1 ? `${title} (Column)` : is1D ? title : `${title} [Row ${yLabels?.[activeSliceRowIdx] ?? activeSliceRowIdx}]`}
            values={curveValues}
            baseValues={baseCurveValues}
            xLabels={curveLabels}
            xAxisName={numCols === 1 && numRows > 1 ? yAxisName : xAxisName}
            unit={unit}
            decimals={decimals}
            readOnly={readOnly}
            onPatchPoint={(idx, nextVal) => {
              const targetR = numCols === 1 && numRows > 1 ? idx : activeSliceRowIdx;
              const targetC = numCols === 1 && numRows > 1 ? 0 : idx;
              if (onPatchCells) {
                onPatchCells(
                  [{ row: targetR, col: targetC, value: nextVal }],
                  `Curve point [${targetR},${targetC}]`
                );
              } else {
                const next = matrix.map((r) => [...r]);
                if (next[targetR]?.[targetC] !== undefined) {
                  next[targetR][targetC] = nextVal;
                  onChangeMatrix(next);
                }
              }
            }}
          />
        </div>
      )}

      <div
        ref={tableScrollRef}
        onScroll={(e) => {
          if (virtualizeRows) setTableScrollTop(e.currentTarget.scrollTop);
        }}
        className={`relative min-w-0 overflow-auto select-none p-1 ${
          fillHeight ? 'min-h-0 flex-1' : 'max-h-[540px]'
        }`}
      >
        <table className="w-max min-w-full border-collapse text-center font-mono text-xs">
          <thead>
            <tr>
              <th
                onClick={handleSelectAll}
                title="Click to select entire matrix [Ctrl+A]"
                className="sticky top-0 left-0 z-30 min-w-[68px] whitespace-nowrap border border-kiviBorder bg-kiviSurface px-2.5 py-1.5 text-[11px] text-kiviSecondary cursor-pointer hover:bg-kiviElevated hover:text-kiviInk transition-colors"
              >
                <div className="flex flex-col items-center leading-tight">
                  <span className="text-[10px] text-kiviSecondary">{yAxisName}</span>
                  <span className="text-[9px] text-kiviMuted">\ {xAxisName}</span>
                </div>
              </th>
              {Array.from({ length: numCols }).map((_, colIdx) => {
                const label = xLabels?.[colIdx] ?? `${colIdx}`;
                const isColInSelection =
                  selection && colIdx >= selection.minC && colIdx <= selection.maxC;
                return (
                  <th
                    key={colIdx}
                    onClick={() => handleColHeaderClick(colIdx)}
                    title={`Column ${colIdx} (${xAxisName}): Click to select column`}
                    className={`sticky top-0 z-20 min-w-[56px] whitespace-nowrap border border-kiviBorder px-2 py-1 cursor-pointer tabular-nums transition-colors ${
                      isColInSelection
                        ? 'bg-kiviAccent/20 text-kiviAccent font-semibold'
                        : 'bg-kiviSurface text-kiviSecondary hover:bg-kiviElevated hover:text-kiviInk'
                    }`}
                  >
                    {label}
                  </th>
                );
              })}
            </tr>
          </thead>

          <tbody>
            {virtualizeRows && virtualStartRow > 0 && (
              <tr aria-hidden="true">
                <td colSpan={numCols + 1} style={{ height: `${virtualStartRow * 28}px`, padding: 0, border: 0 }} />
              </tr>
            )}
            {matrix.slice(virtualStartRow, virtualEndRow + 1).map((row, sliceIdx) => {
              const rIdx = virtualStartRow + sliceIdx;
              const yLabel = yLabels?.[rIdx] ?? `${rIdx}`;
              const selProps = rowSelectionProps(selection, rIdx);
              return (
                <HeatmapRow
                  key={rIdx}
                  row={row}
                  baseRow={baseMatrix?.[rIdx]}
                  previewRow={previewMatrix?.[rIdx]}
                  rIdx={rIdx}
                  yLabel={yLabel}
                  yAxisName={yAxisName}
                  isRowInSelection={selProps.isRowInSelection}
                  selectionMinC={selProps.selectionMinC}
                  selectionMaxC={selProps.selectionMaxC}
                  anchorCol={anchorCell?.r === rIdx ? anchorCell.c : null}
                  editingCol={editingCell?.r === rIdx ? editingCell.c : null}
                  editingValue={editingCell?.r === rIdx ? editingCell.value : ''}
                  correlatedCol={activeCorrelatedCell?.r === rIdx ? activeCorrelatedCell.c : null}
                  hitTraceEnabled={hitTraceEnabled}
                  hitTracer={hitTracer}
                  minVal={minVal}
                  maxVal={maxVal}
                  palette={palette}
                  displayMode={displayMode}
                  decimals={decimals}
                  readOnly={readOnly}
                  onRowHeaderClick={handleRowHeaderClick}
                  onCellMouseDown={handleCellMouseDown}
                  onCellMouseEnter={handleCellMouseEnter}
                  onStartEdit={(r, c, v) =>
                    setEditingCell({ r, c, value: v.toFixed(decimals) })
                  }
                  onChangeEditingValue={(value) =>
                    setEditingCell((prev) => (prev ? { ...prev, value } : null))
                  }
                  onCommitEdit={commitEdit}
                />
              );
            })}
            {virtualizeRows && virtualEndRow < numRows - 1 && (
              <tr aria-hidden="true">
                <td
                  colSpan={numCols + 1}
                  style={{ height: `${(numRows - 1 - virtualEndRow) * 28}px`, padding: 0, border: 0 }}
                />
              </tr>
            )}
          </tbody>
        </table>
      </div>

      <HeatmapFooterBar
        selectionStats={selectionStats}
        decimals={decimals}
        toastMessage={toastMessage}
      />

      <HeatmapDialogs
        isPercentModalOpen={isPercentModalOpen}
        percentInput={percentInput}
        selectedCellCount={selectionStats?.count ?? 1}
        onChangePercentInput={setPercentInput}
        onApplyPercentage={handleApplyPercentage}
        onClosePercentModal={() => setIsPercentModalOpen(false)}
        isPasteSpecialOpen={isPasteSpecialOpen}
        pasteSpecialMode={pasteSpecialMode}
        pasteSpecialValue={pasteSpecialValue}
        onChangePasteSpecialMode={setPasteSpecialMode}
        onChangePasteSpecialValue={setPasteSpecialValue}
        onRunPasteSpecial={(mode, val) => void handlePasteSpecial(mode, val)}
        onClosePasteSpecial={() => setIsPasteSpecialOpen(false)}
      />

      {isSetConstantOpen && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"
          role="dialog"
          aria-modal="true"
          aria-label="Set Constant Cell Value"
          onClick={(e) => {
            if (e.target === e.currentTarget) setIsSetConstantOpen(false);
          }}
        >
          <form
            onSubmit={(e) => {
              e.preventDefault();
              handleApplySetConstant(Number.parseFloat(constantInput));
            }}
            className="w-full max-w-xs space-y-3 rounded-lg border border-kiviBorder bg-kiviSurface p-4 text-xs text-kiviInk shadow-xl"
          >
            <h4 className="text-sm font-semibold">Set Constant Value (=)</h4>
            <p className="text-[11px] text-kiviSecondary">
              Assign value to {selectionStats?.count ?? 1} selected cell(s):
            </p>
            <input
              type="number"
              step="any"
              autoFocus
              aria-label="Constant cell value"
              value={constantInput}
              onChange={(e) => setConstantInput(e.target.value)}
              className="w-full rounded border border-kiviBorderStrong bg-kiviBg px-2.5 py-1.5 font-mono text-xs text-kiviInk focus-visible:outline-2 focus-visible:outline-kiviAccent"
            />
            <div className="flex justify-end gap-2">
              <button
                type="button"
                onClick={() => setIsSetConstantOpen(false)}
                className="rounded border border-kiviBorder bg-kiviBg px-3 py-1 text-kiviSecondary hover:text-kiviInk"
              >
                Cancel
              </button>
              <button
                type="submit"
                className="rounded bg-kiviAccent px-3 py-1 font-semibold text-kiviAccentFg"
              >
                Apply
              </button>
            </div>
          </form>
        </div>
      )}
    </div>
  );
});
