import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { MapDef } from '../types/kivi';
import { useMapStore } from '../stores/useMapStore';
import {
  bridgeExtractMapMatrix,
  bridgeExtractOriginalMapMatrix,
  isDesktopApp,
} from '../lib/tauriBridge';
import { resolveEffectiveMapAxes } from '../lib/matrixRom';
import type { CellWriteInput } from '../lib/bridge/windowBridge';

export interface UseMapMatrixResult {
  matrix: number[][] | null;
  baseMatrix: number[][] | null;
  xLabels: (string | number)[];
  yLabels: (string | number)[];
  xAxisName: string;
  yAxisName: string;
  interpolationAxes: { x: number[] | null; y: number[] | null };
  onChangeMatrix: (next: number[][]) => void;
  onPatchCells: (edits: CellWriteInput[], label?: string) => void;
}

/**
 * Race-safe per-map matrix extractor & writer for split panes, detached floating tabs,
 * and standalone windows. Reuses active store matrices when `mapDef` is selected, and
 * otherwise extracts directly from active + stock ROM keyed by `(docId, revision, mapDef)`.
 */
export function useMapMatrix(mapDef: MapDef | undefined): UseMapMatrixResult {
  const selectedMapId = useMapStore((s) => s.selectedMapId);
  const activeMatrix = useMapStore((s) => s.activeMatrix);
  const storeBaseMatrix = useMapStore((s) => s.baseMatrix);
  const romData = useMapStore((s) => s.romData);
  const originalRomData = useMapStore((s) => s.originalRomData);
  const romLoaded = useMapStore((s) => s.romLoaded);
  const romRevision = useMapStore((s) => s.romRevision);
  const activeDocId = useMapStore((s) => s.activeDocId);

  const isPrimarySelected = Boolean(mapDef && mapDef.id === selectedMapId && activeMatrix);
  const hasRom = romLoaded || Boolean(romData && romData.length > 0);

  const [extracted, setExtracted] = useState<{
    key: string;
    matrix: number[][] | null;
    baseMatrix: number[][] | null;
  }>({ key: '', matrix: null, baseMatrix: null });

  const reqSeqRef = useRef(0);

  const extractKey = mapDef
    ? `${activeDocId ?? 'none'}:${romRevision}:${mapDef.id}:${mapDef.address}:${mapDef.rows}x${mapDef.cols}:${mapDef.dtype}:${mapDef.endian}:${mapDef.scaling.equation}`
    : '';

  useEffect(() => {
    if (!mapDef || !hasRom || isPrimarySelected) {
      return;
    }
    const seq = ++reqSeqRef.current;
    const desktop = isDesktopApp();
    let cancelled = false;

    const run = async () => {
      try {
        if (desktop) {
          const [matrix, stock] = await Promise.all([
            bridgeExtractMapMatrix(mapDef, null),
            bridgeExtractOriginalMapMatrix(mapDef, null),
          ]);
          if (cancelled || seq !== reqSeqRef.current) return;
          setExtracted({
            key: extractKey,
            matrix,
            baseMatrix: stock ?? matrix.map((r) => [...r]),
          });
          return;
        }
        const matrix = await bridgeExtractMapMatrix(mapDef, romData);
        const stock = originalRomData
          ? await bridgeExtractOriginalMapMatrix(mapDef, originalRomData)
          : null;
        if (cancelled || seq !== reqSeqRef.current) return;
        setExtracted({
          key: extractKey,
          matrix,
          baseMatrix: stock ?? matrix.map((r) => [...r]),
        });
      } catch (err) {
        console.warn('useMapMatrix extract error:', err);
      }
    };

    void run();
    return () => {
      cancelled = true;
    };
  }, [mapDef, hasRom, isPrimarySelected, extractKey, romData, originalRomData]);

  const resolvedMatrix = useMemo(() => {
    if (!mapDef || !hasRom) return null;
    if (isPrimarySelected) return activeMatrix;
    return extracted.key === extractKey ? extracted.matrix : null;
  }, [mapDef, hasRom, isPrimarySelected, activeMatrix, extracted, extractKey]);

  const resolvedBaseMatrix = useMemo(() => {
    if (!mapDef || !hasRom) return null;
    if (isPrimarySelected) return storeBaseMatrix;
    return extracted.key === extractKey ? extracted.baseMatrix : null;
  }, [mapDef, hasRom, isPrimarySelected, storeBaseMatrix, extracted, extractKey]);

  const axes = useMemo(() => resolveEffectiveMapAxes(mapDef), [mapDef]);

  const onChangeMatrix = useCallback(
    (next: number[][]) => {
      if (!mapDef) return;
      if (!isPrimarySelected) {
        setExtracted({
          key: extractKey,
          matrix: next,
          baseMatrix: resolvedBaseMatrix,
        });
      }
      void useMapStore.getState().updateMatrix(mapDef, next);
    },
    [mapDef, isPrimarySelected, extractKey, resolvedBaseMatrix]
  );

  const onPatchCells = useCallback(
    (edits: CellWriteInput[], label?: string) => {
      if (!mapDef || edits.length === 0) return;
      if (!isPrimarySelected && resolvedMatrix) {
        const patched = resolvedMatrix.map((r) => [...r]);
        for (const edit of edits) {
          if (
            edit.row >= 0 &&
            edit.row < patched.length &&
            edit.col >= 0 &&
            edit.col < patched[edit.row].length
          ) {
            patched[edit.row][edit.col] = edit.value;
          }
        }
        setExtracted({
          key: extractKey,
          matrix: patched,
          baseMatrix: resolvedBaseMatrix,
        });
      }
      void useMapStore.getState().patchActiveMapCells(mapDef, edits, label);
    },
    [mapDef, isPrimarySelected, resolvedMatrix, extractKey, resolvedBaseMatrix]
  );

  return {
    matrix: resolvedMatrix,
    baseMatrix: resolvedBaseMatrix,
    xLabels: axes.xLabels,
    yLabels: axes.yLabels,
    xAxisName: axes.xAxisName,
    yAxisName: axes.yAxisName,
    interpolationAxes: axes.interpolationAxes,
    onChangeMatrix,
    onPatchCells,
  };
}
