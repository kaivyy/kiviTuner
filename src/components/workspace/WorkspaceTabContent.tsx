import React, { useEffect, useRef, useState } from 'react';
import type { MapDef } from '../../types/kivi';
import type { WorkspaceTab } from '../../stores/useWorkspaceTabsStore';
import { useMapStore } from '../../stores/useMapStore';
import { useMapMatrix } from '../../hooks/useMapMatrix';
import type { DetectedMapCandidate, DiffRegion } from '../../lib/tauriBridge';
import {
  HeatmapTable,
  HexdumpView,
  ScalarFlagEditor,
  Surface3DView,
  isScalarOrFlagMap,
  type CellDisplayMode,
  type MarkedRange,
  type SelectionRange,
  type TablePalette,
} from '../editor';
import { KiviWaveform } from '../waveform';
import { SplitResizeHandle } from '../layout/SplitResizeHandle';
import { computeSplitRatio } from '../layout/resizeModel';

export interface WorkspaceTabContentProps {
  tab: WorkspaceTab;
  mapDef?: MapDef;
  displayMode?: CellDisplayMode;
  onCellDisplayModeChange?: (mode: CellDisplayMode) => void;
  palette?: TablePalette;
  onPaletteChange?: (palette: TablePalette) => void;
  hexMarks?: MarkedRange[];
  hexJumpAddress?: number | null;
  onCreateMapFromHex?: (startAddress: number, byteLength: number) => void;
  onPatchByte?: (offset: number, value: number) => Promise<boolean> | boolean | void;
  waveformPitch?: number;
  onWaveformPitchChange?: (pitch: number) => void;
  waveformIs16Bit?: boolean;
  waveformIsBigEndian?: boolean;
  diffRegions?: DiffRegion[];
  detectedCandidates?: DetectedMapCandidate[];
  onSelectWaveformRange?: (start: number, end: number) => void;
  onJumpToWaveform?: (startAddr: number, endAddr: number) => void;
  onRevertToOriginal?: (mapDef: MapDef, selection?: SelectionRange | null) => void;
  onEditMapProperties?: (mapDef: MapDef) => void;
}

/**
 * Self-contained live editor view for any workspace tab (in split pane,
 * detached floating window, or standalone secondary OS window).
 */
export const WorkspaceTabContent: React.FC<WorkspaceTabContentProps> = ({
  tab,
  mapDef,
  displayMode: propDisplayMode,
  onCellDisplayModeChange,
  palette: propPalette,
  onPaletteChange,
  hexMarks = [],
  hexJumpAddress = null,
  onCreateMapFromHex,
  onPatchByte,
  waveformPitch = 16,
  onWaveformPitchChange,
  waveformIs16Bit = false,
  waveformIsBigEndian = false,
  diffRegions = [],
  detectedCandidates = [],
  onSelectWaveformRange,
  onJumpToWaveform,
  onRevertToOriginal,
  onEditMapProperties,
}) => {
  const romData = useMapStore((s) => s.romData);
  const romLoaded = useMapStore((s) => s.romLoaded);
  const romSize = useMapStore((s) => s.romSize);
  const romRevision = useMapStore((s) => s.romRevision);
  const activeDocId = useMapStore((s) => s.activeDocId);

  const [localDisplayMode, setLocalDisplayMode] = useState<CellDisplayMode>('engineering');
  const [localPalette, setLocalPalette] = useState<TablePalette>('classic');
  const [localSelection, setLocalSelection] = useState<SelectionRange | null>(null);
  const [forceGridView, setForceGridView] = useState(false);

  const [splitRatio, setSplitRatio] = useState(0.5);
  const [isSplitRow, setIsSplitRow] = useState(true);
  const splitHostRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    setLocalSelection(null);
    setForceGridView(false);
  }, [mapDef?.id]);

  useEffect(() => {
    const el = splitHostRef.current;
    if (!el || tab.kind !== 'map-split') return;
    const updateDir = () => {
      setIsSplitRow(el.clientWidth >= 760);
    };
    updateDir();
    const ro = new ResizeObserver(updateDir);
    ro.observe(el);
    return () => ro.disconnect();
  }, [tab.kind]);

  const effectiveDisplayMode = propDisplayMode ?? localDisplayMode;
  const handleDisplayModeChange = onCellDisplayModeChange ?? setLocalDisplayMode;
  const effectivePalette = propPalette ?? localPalette;
  const handlePaletteChange = onPaletteChange ?? setLocalPalette;

  const {
    matrix,
    baseMatrix,
    xLabels,
    yLabels,
    xAxisName,
    yAxisName,
    interpolationAxes,
    onChangeMatrix,
    onPatchCells,
  } = useMapMatrix(mapDef);

  const hasRom = romLoaded || Boolean(romData && romData.length > 0);

  if (tab.kind === 'hex') {
    return (
      <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
        <HexdumpView
          data={romData}
          romLoaded={romLoaded}
          romSize={romSize}
          romRevision={romRevision}
          activeDocId={activeDocId}
          marks={hexMarks}
          jumpAddress={hexJumpAddress}
          onCreateMapFromHex={onCreateMapFromHex}
          onPatchByte={onPatchByte}
        />
      </div>
    );
  }

  if (tab.kind === 'waveform') {
    return (
      <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-auto p-2">
        <KiviWaveform
          data={romData}
          romSize={romSize}
          romRevision={romRevision}
          activeDocId={activeDocId}
          pitch={waveformPitch}
          is16Bit={waveformIs16Bit}
          isBigEndian={waveformIsBigEndian}
          onPitchChange={onWaveformPitchChange}
          onSelectRange={onSelectWaveformRange}
          diffRegions={diffRegions}
          detectedCandidates={detectedCandidates}
          onSelectRegion={onJumpToWaveform}
          height={360}
        />
      </div>
    );
  }

  if (!hasRom || !mapDef || !matrix) {
    return (
      <div className="flex h-full min-h-[180px] flex-1 items-center justify-center p-4 text-xs text-kiviMuted">
        {hasRom ? 'Extracting map data...' : 'Load a ROM to inspect this tab.'}
      </div>
    );
  }

  if (tab.kind === 'map-3d') {
    return (
      <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
        <Surface3DView
          title={mapDef.name}
          tableId={mapDef.id}
          matrix={matrix}
          baseMatrix={baseMatrix || undefined}
          xLabels={xLabels}
          yLabels={yLabels}
          xAxisName={xAxisName}
          yAxisName={yAxisName}
          unit={mapDef.scaling.unit || 'raw'}
          decimals={mapDef.scaling.decimals ?? 1}
          selectedRange={localSelection}
          onSelectCell={(r, c) => setLocalSelection({ minR: r, maxR: r, minC: c, maxC: c })}
          onChangeMatrix={onChangeMatrix}
          palette={effectivePalette}
        />
      </div>
    );
  }

  if (tab.kind === 'map-split') {
    return (
      <div
        ref={splitHostRef}
        className={`flex min-h-0 min-w-0 flex-1 overflow-hidden ${
          isSplitRow ? 'flex-row' : 'flex-col'
        }`}
      >
        <div
          style={{ flex: `0 0 ${(splitRatio * 100).toFixed(2)}%` }}
          className="flex min-h-[130px] min-w-[200px] flex-col overflow-hidden"
        >
          <HeatmapTable
            fillHeight
            title={`${mapDef.name} (2D)`}
            tableId={mapDef.id}
            matrix={matrix}
            baseMatrix={baseMatrix || undefined}
            cellDisplayMode={effectiveDisplayMode}
            palette={effectivePalette}
            xLabels={xLabels}
            yLabels={yLabels}
            xAxisName={xAxisName}
            yAxisName={yAxisName}
            unit={mapDef.scaling.unit || 'raw'}
            decimals={mapDef.scaling.decimals ?? 1}
            selectedRange={localSelection}
            onSelectRange={setLocalSelection}
            onChangeMatrix={onChangeMatrix}
            onPatchCells={onPatchCells}
            checksumRom={romData}
            enableSmoothingPreview
            interpolationAxes={interpolationAxes}
            onCellDisplayModeChange={handleDisplayModeChange}
            onPaletteChange={handlePaletteChange}
            onRevertToOriginal={
              onRevertToOriginal ? (sel) => onRevertToOriginal(mapDef, sel) : undefined
            }
            onEditMapProperties={
              onEditMapProperties ? () => onEditMapProperties(mapDef) : undefined
            }
          />
        </div>
        <SplitResizeHandle
          orientation={isSplitRow ? 'vertical' : 'horizontal'}
          ariaLabel="Resize detached 2D and 3D split"
          onDragMove={(clientX, clientY) => {
            const host = splitHostRef.current;
            if (!host) return;
            const rect = host.getBoundingClientRect();
            const next = computeSplitRatio({
              pointerPos: isSplitRow ? clientX : clientY,
              containerStart: isSplitRow ? rect.left : rect.top,
              containerSize: isSplitRow ? rect.width : rect.height,
              minRatio: 0.25,
              maxRatio: 0.75,
            });
            setSplitRatio(next);
          }}
          onStep={(delta) => {
            const size = isSplitRow
              ? splitHostRef.current?.clientWidth || 760
              : splitHostRef.current?.clientHeight || 520;
            setSplitRatio((r) => Math.max(0.25, Math.min(0.75, r + delta / size)));
          }}
          onReset={() => setSplitRatio(0.5)}
        />
        <div className="flex min-h-[130px] min-w-[200px] flex-1 flex-col overflow-hidden">
          <Surface3DView
            title={`${mapDef.name} (3D)`}
            tableId={mapDef.id}
            matrix={matrix}
            baseMatrix={baseMatrix || undefined}
            xLabels={xLabels}
            yLabels={yLabels}
            xAxisName={xAxisName}
            yAxisName={yAxisName}
            unit={mapDef.scaling.unit || 'raw'}
            decimals={mapDef.scaling.decimals ?? 1}
            selectedRange={localSelection}
            onSelectCell={(r, c) => setLocalSelection({ minR: r, maxR: r, minC: c, maxC: c })}
            onChangeMatrix={onChangeMatrix}
            palette={effectivePalette}
          />
        </div>
      </div>
    );
  }

  if (isScalarOrFlagMap(mapDef) && !forceGridView) {
    return (
      <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-auto p-3">
        <ScalarFlagEditor
          mapDef={mapDef}
          value={matrix[0]?.[0] ?? 0}
          baseValue={baseMatrix?.[0]?.[0] ?? null}
          onCommitValue={(nextVal) =>
            onPatchCells([{ row: 0, col: 0, value: nextVal }], `Edit ${mapDef.name}`)
          }
          onRevert={
            onRevertToOriginal
              ? () => onRevertToOriginal(mapDef, { minR: 0, maxR: 0, minC: 0, maxC: 0 })
              : undefined
          }
          onEditProperties={onEditMapProperties ? () => onEditMapProperties(mapDef) : undefined}
          onSwitchToGrid={() => setForceGridView(true)}
        />
      </div>
    );
  }

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
      <HeatmapTable
        fillHeight
        title={mapDef.name}
        tableId={mapDef.id}
        matrix={matrix}
        baseMatrix={baseMatrix || undefined}
        cellDisplayMode={effectiveDisplayMode}
        palette={effectivePalette}
        xLabels={xLabels}
        yLabels={yLabels}
        xAxisName={xAxisName}
        yAxisName={yAxisName}
        unit={mapDef.scaling.unit || 'raw'}
        decimals={mapDef.scaling.decimals ?? 1}
        selectedRange={localSelection}
        onSelectRange={setLocalSelection}
        onChangeMatrix={onChangeMatrix}
        onPatchCells={onPatchCells}
        checksumRom={romData}
        enableSmoothingPreview
        interpolationAxes={interpolationAxes}
        onCellDisplayModeChange={handleDisplayModeChange}
        onPaletteChange={handlePaletteChange}
        onRevertToOriginal={
          onRevertToOriginal ? (sel) => onRevertToOriginal(mapDef, sel) : undefined
        }
        onEditMapProperties={
          onEditMapProperties ? () => onEditMapProperties(mapDef) : undefined
        }
      />
    </div>
  );
};
