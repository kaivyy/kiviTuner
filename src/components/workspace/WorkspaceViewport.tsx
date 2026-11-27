import React, { useEffect, useState } from 'react';
import {
  Activity,
  Binary,
  Box,
  Calculator,
  Columns,
  Grid,
  PlusCircle,
  Search,
  Sliders,
  X,
} from 'lucide-react';
import type { LayoutPreset } from '../../App';
import { KiviWaveform } from '../waveform';
import {
  HeatmapTable,
  HexdumpView,
  ScalarFlagEditor,
  Surface3DView,
  isScalarOrFlagMap,
  type CellDisplayMode,
  type HeatmapTableHandle as TableEditorHandle,
  type MarkedRange,
  type SelectionRange,
  type TablePalette,
} from '../editor';
import type { DetectedMapCandidate, DiffRegion } from '../../lib/tauriBridge';
import type { MapDef } from '../../types/kivi';
import type { WorkspaceTab } from '../../stores/useWorkspaceTabsStore';

interface WorkspaceViewportProps {
  layoutPreset: LayoutPreset;
  setLayoutPreset: (preset: LayoutPreset) => void;
  selectedMap: MapDef | undefined;
  activeMatrix: number[][] | null;
  baseMatrix: number[][] | null;
  displayMode: CellDisplayMode;
  setDisplayMode: (mode: CellDisplayMode) => void;
  palette: TablePalette;
  setPalette: (palette: TablePalette) => void;
  xLabels: (string | number)[];
  yLabels: (string | number)[];
  xAxisName: string;
  yAxisName: string;
  interpolationAxes: { x: number[] | null; y: number[] | null };
  tableSelection: SelectionRange | null;
  setTableSelection: (range: SelectionRange | null) => void;
  onChangeMatrix: (next: number[][]) => void;
  onPatchCells?: (edits: Array<{ row: number; col: number; value: number }>, label?: string) => void;
  romData: Uint8Array | null;
  romLoaded?: boolean;
  romSize?: number;
  romRevision?: number;
  activeDocId?: string | null;
  hexMarks?: MarkedRange[];
  hexJumpAddress?: number | null;
  onCreateMapFromHex?: (startAddress: number, byteLength: number) => void;
  heatmapActionRef?: React.RefObject<TableEditorHandle | null>;
  onRevertToOriginal?: (selection?: SelectionRange | null) => void;
  onEditMapProperties?: () => void;
  waveformPitch: number;
  setWaveformPitch: (pitch: number) => void;
  waveformIs16Bit: boolean;
  waveformIsBigEndian: boolean;
  diffRegions: DiffRegion[];
  detectedCandidates: DetectedMapCandidate[];
  selectedRange: { start: number; end: number } | null;
  onSelectRange: (start: number, end: number) => void;
  onJumpToWaveform: (startAddr: number, endAddr: number) => void;
  onExtractMap: () => void;
  onOpenToolkit: () => void;
  onRunScanner: () => void;
  activeTab?: WorkspaceTab;
  splitTab?: WorkspaceTab;
  onCloseSplit?: () => void;
}

export const WorkspaceViewport: React.FC<WorkspaceViewportProps> = React.memo(({
  layoutPreset,
  setLayoutPreset,
  selectedMap,
  activeMatrix,
  baseMatrix,
  displayMode,
  setDisplayMode,
  palette,
  setPalette,
  xLabels,
  yLabels,
  xAxisName,
  yAxisName,
  interpolationAxes,
  tableSelection,
  setTableSelection,
  onChangeMatrix,
  onPatchCells,
  romData,
  romLoaded = false,
  romSize = 0,
  romRevision = 0,
  activeDocId = null,
  hexMarks = [],
  hexJumpAddress = null,
  onCreateMapFromHex,
  heatmapActionRef,
  onRevertToOriginal,
  onEditMapProperties,
  waveformPitch,
  setWaveformPitch,
  waveformIs16Bit,
  waveformIsBigEndian,
  diffRegions,
  detectedCandidates,
  selectedRange,
  onSelectRange,
  onJumpToWaveform,
  onExtractMap,
  onOpenToolkit,
  onRunScanner,
  activeTab,
  splitTab,
  onCloseSplit,
}) => {
  const [forceGridView, setForceGridView] = useState(false);

  useEffect(() => {
    setForceGridView(false);
  }, [selectedMap?.id]);

  const effectiveRomReady = romLoaded || Boolean(romData && romData.length > 0);
  const showScalarFlagPrimary = Boolean(
    selectedMap && activeMatrix && isScalarOrFlagMap(selectedMap) && !forceGridView
  );
  const showHexPrimary = activeTab?.kind === 'hex';
  const effectivePreset: LayoutPreset =
    activeTab?.kind === 'waveform'
      ? 'full_waveform'
      : activeTab?.kind === 'map-3d'
        ? 'full_3d'
        : activeTab?.kind === 'map-split'
          ? 'split_3d'
          : layoutPreset;

  const renderSplitPane = () => {
    if (!splitTab) return null;
    return (
      <section className="flex min-h-0 min-w-0 flex-1 flex-col border-t border-kiviBorder bg-kiviBg lg:border-t-0 lg:border-l">
        <div
          data-testid="panel-titlebar-split"
          className="flex h-9 shrink-0 items-center justify-between gap-2 bg-kiviSurface px-3 text-xs"
        >
          <span className="min-w-0 truncate whitespace-nowrap font-medium text-kiviInk">
            Split: {splitTab.title}
          </span>
          {onCloseSplit && (
            <button
              type="button"
              aria-label="Close split pane"
              onClick={onCloseSplit}
              className="flex h-6 w-6 shrink-0 items-center justify-center rounded text-kiviMuted transition-colors hover:bg-kiviElevated hover:text-kiviInk"
            >
              <X className="h-3.5 w-3.5" />
            </button>
          )}
        </div>
        <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-auto p-3">
          {splitTab.kind === 'hex' ? (
            <HexdumpView
              data={romData}
              romLoaded={romLoaded}
              romSize={romSize}
              romRevision={romRevision}
              activeDocId={activeDocId}
              marks={hexMarks}
              jumpAddress={hexJumpAddress}
              onCreateMapFromHex={onCreateMapFromHex}
            />
          ) : splitTab.kind === 'waveform' ? (
            <KiviWaveform
              data={romData}
              romSize={romSize}
              romRevision={romRevision}
              activeDocId={activeDocId}
              pitch={waveformPitch}
              is16Bit={waveformIs16Bit}
              isBigEndian={waveformIsBigEndian}
              onPitchChange={setWaveformPitch}
              onSelectRange={onSelectRange}
              diffRegions={diffRegions}
              detectedCandidates={detectedCandidates}
              onSelectRegion={onJumpToWaveform}
              height={380}
            />
          ) : selectedMap && activeMatrix ? (
            splitTab.kind === 'map-3d' ? (
              <div className="flex min-h-[340px] min-w-0 flex-1 flex-col overflow-hidden">
                <Surface3DView
                  title={`${selectedMap.name} (3D)`}
                  tableId={selectedMap.id}
                  matrix={activeMatrix}
                  baseMatrix={baseMatrix || undefined}
                  xLabels={xLabels}
                  yLabels={yLabels}
                  xAxisName={xAxisName}
                  yAxisName={yAxisName}
                  unit={selectedMap.scaling.unit || 'raw'}
                  decimals={selectedMap.scaling.decimals ?? 1}
                  selectedRange={tableSelection}
                  onSelectCell={(r, c) => setTableSelection({ minR: r, maxR: r, minC: c, maxC: c })}
                  onChangeMatrix={onChangeMatrix}
                  palette={palette}
                />
              </div>
            ) : (
              <HeatmapTable
                fillHeight
                title={selectedMap.name}
                tableId={selectedMap.id}
                matrix={activeMatrix}
                baseMatrix={baseMatrix || undefined}
                cellDisplayMode={displayMode}
                palette={palette}
                xLabels={xLabels}
                yLabels={yLabels}
                xAxisName={xAxisName}
                yAxisName={yAxisName}
                unit={selectedMap.scaling.unit || 'raw'}
                decimals={selectedMap.scaling.decimals ?? 1}
                selectedRange={tableSelection}
                onSelectRange={setTableSelection}
                onChangeMatrix={onChangeMatrix}
                onPatchCells={onPatchCells}
                checksumRom={romData}
                enableSmoothingPreview
                interpolationAxes={interpolationAxes}
                onCellDisplayModeChange={setDisplayMode}
                onPaletteChange={setPalette}
                onRevertToOriginal={onRevertToOriginal}
                onEditMapProperties={onEditMapProperties}
              />
            )
          ) : (
            <div className="flex h-full items-center justify-center text-xs text-kiviMuted">
              Select a map or open a ROM to populate split pane.
            </div>
          )}
        </div>
      </section>
    );
  };

  return (
    <main className="flex min-w-0 flex-1 flex-col overflow-hidden bg-kiviBg">
      <div
        data-testid="workspace-viewbar"
        className="flex h-9 shrink-0 flex-nowrap items-center justify-between gap-2 bg-kiviSurface px-2.5 md:px-3"
      >
        <div className="flex min-w-0 flex-1 items-center gap-2">
          <div className="no-scrollbar flex min-w-0 max-w-full items-center gap-0.5 overflow-x-auto rounded bg-kiviBg p-0.5 text-xs">
            <button
              type="button"
              onClick={() => setLayoutPreset('standard')}
              className={`flex h-6 shrink-0 items-center gap-1.5 whitespace-nowrap rounded px-2 font-medium transition-colors md:px-2.5 ${
                effectivePreset === 'standard' && !showHexPrimary
                  ? 'bg-kiviAccent font-semibold text-kiviAccentFg'
                  : 'text-kiviSecondary hover:text-kiviInk'
              }`}
              title="Standard 2D Table View"
            >
              <Grid className="h-3.5 w-3.5 shrink-0" />
              <span>2D Table</span>
            </button>

            <button
              type="button"
              onClick={() => setLayoutPreset('split_3d')}
              className={`flex h-6 shrink-0 items-center gap-1.5 whitespace-nowrap rounded px-2 font-medium transition-colors md:px-2.5 ${
                effectivePreset === 'split_3d' && !showHexPrimary
                  ? 'bg-kiviAccent font-semibold text-kiviAccentFg'
                  : 'text-kiviSecondary hover:text-kiviInk'
              }`}
              title="Side-by-side 2D Table & 3D Surface Mesh [Ctrl+\]"
            >
              <Columns className="h-3.5 w-3.5 shrink-0" />
              <span>Split 3D</span>
            </button>

            <button
              type="button"
              onClick={() => setLayoutPreset('split_waveform')}
              className={`flex h-6 shrink-0 items-center gap-1.5 whitespace-nowrap rounded px-2 font-medium transition-colors md:px-2.5 ${
                effectivePreset === 'split_waveform' && !showHexPrimary
                  ? 'bg-kiviAccent font-semibold text-kiviAccentFg'
                  : 'text-kiviSecondary hover:text-kiviInk'
              }`}
              title="2D Table + ROM Waveform"
            >
              <Activity className="h-3.5 w-3.5 shrink-0" />
              <span>Split Wave</span>
            </button>

            <button
              type="button"
              onClick={() => setLayoutPreset('full_waveform')}
              className={`flex h-6 shrink-0 items-center gap-1.5 whitespace-nowrap rounded px-2 font-medium transition-colors md:px-2.5 ${
                effectivePreset === 'full_waveform' && !showHexPrimary
                  ? 'bg-kiviAccent font-semibold text-kiviAccentFg'
                  : 'text-kiviSecondary hover:text-kiviInk'
              }`}
              title="Full Binary Waveform Inspector"
            >
              <Activity className="h-3.5 w-3.5 shrink-0" />
              <span>Full Waveform</span>
            </button>

            <button
              type="button"
              onClick={() => setLayoutPreset('full_3d')}
              className={`flex h-6 shrink-0 items-center gap-1.5 whitespace-nowrap rounded px-2 font-medium transition-colors md:px-2.5 ${
                effectivePreset === 'full_3d' && !showHexPrimary
                  ? 'bg-kiviAccent font-semibold text-kiviAccentFg'
                  : 'text-kiviSecondary hover:text-kiviInk'
              }`}
              title="Full 3D Surface Mesh Viewer"
            >
              <Box className="h-3.5 w-3.5 shrink-0" />
              <span>Full 3D</span>
            </button>
          </div>

          {selectedMap && (
            <span className="ml-2 hidden whitespace-nowrap text-xs text-kiviMuted 2xl:inline">
              Active: <span className="font-medium text-kiviInk">{selectedMap.name}</span>
            </span>
          )}
        </div>

        <div className="flex shrink-0 items-center gap-1.5 md:gap-2">
          <button
            type="button"
            aria-label="Open engineering toolkit"
            onClick={onOpenToolkit}
            className="flex h-6 shrink-0 items-center gap-1.5 whitespace-nowrap rounded border border-kiviBorder/60 bg-kiviElevated px-2.5 text-xs font-medium text-kiviSecondary transition-colors hover:text-kiviInk"
            title="Fuel, air mass, timing, torque, sensor decode, flash chip layout"
          >
            <Calculator className="h-3.5 w-3.5 shrink-0 text-kiviAccent" />
            <span>Toolkit</span>
          </button>

          {selectedRange && (
            <button
              type="button"
              onClick={onExtractMap}
              className="flex h-6 shrink-0 items-center gap-1.5 whitespace-nowrap rounded border border-kiviAccent/50 bg-kiviAccent/15 px-2.5 text-xs font-medium text-kiviAccent transition-colors hover:bg-kiviAccent/25"
            >
              <PlusCircle className="h-3.5 w-3.5 shrink-0" />
              <span className="font-mono tabular-nums">
                Extract Map (0x{selectedRange.start.toString(16).toUpperCase()}..0x
                {selectedRange.end.toString(16).toUpperCase()})
              </span>
            </button>
          )}
        </div>
      </div>

      <div className="flex min-h-0 flex-1 flex-col overflow-hidden lg:flex-row">
        <div
          className={`flex min-h-0 min-w-0 flex-1 flex-col gap-3 p-3 md:gap-4 md:p-4 ${
            showHexPrimary || effectivePreset === 'full_3d'
              ? 'overflow-hidden'
              : effectivePreset === 'split_3d'
                ? 'overflow-y-auto xl:overflow-hidden'
                : 'overflow-y-auto'
          }`}
        >
          {showHexPrimary ? (
            <div className="flex flex-1 flex-col overflow-hidden rounded-md border border-kiviBorder/60">
              <HexdumpView
                data={romData}
                romLoaded={romLoaded}
                romSize={romSize}
                romRevision={romRevision}
                activeDocId={activeDocId}
                marks={hexMarks}
                jumpAddress={hexJumpAddress}
                onCreateMapFromHex={onCreateMapFromHex}
              />
            </div>
          ) : effectivePreset === 'full_waveform' ? (
            <div className="flex min-h-0 flex-1 flex-col">
              <div className="mb-2 flex items-center justify-between">
                <div>
                  <h2 className="flex items-center gap-1.5 text-sm font-semibold text-kiviInk">
                    <Activity className="h-4 w-4 text-kiviAccent" />
                    <span>2D Binary ROM Waveform Explorer</span>
                  </h2>
                  <p className="mt-0.5 text-xs text-kiviMuted">
                    Overlay marks: Red = Diff regions, Amber = Scanned candidate maps. Click a region
                    to inspect or select.
                  </p>
                </div>
              </div>

              <div className="min-h-[340px] flex-1">
                <KiviWaveform
                  data={romData}
                  romSize={romSize}
                  romRevision={romRevision}
                  activeDocId={activeDocId}
                  pitch={waveformPitch}
                  is16Bit={waveformIs16Bit}
                  isBigEndian={waveformIsBigEndian}
                  onPitchChange={setWaveformPitch}
                  onSelectRange={onSelectRange}
                  diffRegions={diffRegions}
                  detectedCandidates={detectedCandidates}
                  onSelectRegion={onJumpToWaveform}
                  height={420}
                />
              </div>
            </div>
          ) : effectivePreset === 'full_3d' ? (
            selectedMap && activeMatrix ? (
              <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
                <Surface3DView
                  title={`${selectedMap.name} (3D Full)`}
                  tableId={selectedMap.id}
                  matrix={activeMatrix}
                  baseMatrix={baseMatrix || undefined}
                  xLabels={xLabels}
                  yLabels={yLabels}
                  xAxisName={xAxisName}
                  yAxisName={yAxisName}
                  unit={selectedMap.scaling.unit || 'raw'}
                  decimals={selectedMap.scaling.decimals ?? 1}
                  selectedRange={tableSelection}
                  onSelectCell={(r, c) => setTableSelection({ minR: r, maxR: r, minC: c, maxC: c })}
                  onChangeMatrix={onChangeMatrix}
                  palette={palette}
                />
              </div>
            ) : (
              <div className="flex flex-1 flex-col items-center justify-center p-8 text-center text-kiviMuted">
                <Box className="mb-2 h-10 w-10 opacity-40" />
                <p className="text-sm font-medium text-kiviSecondary">Select a map to view 3D Mesh</p>
              </div>
            )
          ) : effectivePreset === 'split_3d' ? (
            selectedMap && activeMatrix ? (
              <div className="flex min-h-0 min-w-0 flex-1 flex-col gap-3 xl:grid xl:grid-cols-[minmax(0,1.08fr)_minmax(0,1fr)] xl:overflow-hidden">
                <div className="flex min-h-[320px] min-w-0 flex-col overflow-hidden xl:min-h-0">
                  <HeatmapTable
                    fillHeight
                    title={`${selectedMap.name} (2D)`}
                    tableId={selectedMap.id}
                    matrix={activeMatrix}
                    baseMatrix={baseMatrix || undefined}
                    cellDisplayMode={displayMode}
                    palette={palette}
                    xLabels={xLabels}
                    yLabels={yLabels}
                    xAxisName={xAxisName}
                    yAxisName={yAxisName}
                    unit={selectedMap.scaling.unit || 'raw'}
                    decimals={selectedMap.scaling.decimals ?? 1}
                    selectedRange={tableSelection}
                    onSelectRange={setTableSelection}
                    onChangeMatrix={onChangeMatrix}
                    onPatchCells={onPatchCells}
                    checksumRom={romData}
                    enableSmoothingPreview
                    interpolationAxes={interpolationAxes}
                    onCellDisplayModeChange={setDisplayMode}
                    onPaletteChange={setPalette}
                    actionRef={heatmapActionRef}
                    onRevertToOriginal={onRevertToOriginal}
                    onEditMapProperties={onEditMapProperties}
                  />
                </div>
                <div className="flex h-[380px] min-h-[320px] min-w-0 flex-col overflow-hidden xl:h-full xl:min-h-0">
                  <Surface3DView
                    title={`${selectedMap.name} (3D)`}
                    tableId={selectedMap.id}
                    matrix={activeMatrix}
                    baseMatrix={baseMatrix || undefined}
                    xLabels={xLabels}
                    yLabels={yLabels}
                    xAxisName={xAxisName}
                    yAxisName={yAxisName}
                    unit={selectedMap.scaling.unit || 'raw'}
                    decimals={selectedMap.scaling.decimals ?? 1}
                    selectedRange={tableSelection}
                    onSelectCell={(r, c) => setTableSelection({ minR: r, maxR: r, minC: c, maxC: c })}
                    onChangeMatrix={onChangeMatrix}
                    palette={palette}
                  />
                </div>
              </div>
            ) : (
              <div className="flex flex-1 flex-col items-center justify-center p-8 text-center text-kiviMuted">
                <Columns className="mb-2 h-10 w-10 opacity-40" />
                <p className="text-sm font-medium text-kiviSecondary">
                  Select a table to open Split 3D view
                </p>
              </div>
            )
          ) : effectivePreset === 'split_waveform' ? (
            <div className="flex min-h-0 min-w-0 flex-1 flex-col gap-4">
              {selectedMap && activeMatrix && (
                <div className="min-w-0 overflow-hidden">
                  <HeatmapTable
                    title={selectedMap.name}
                    tableId={selectedMap.id}
                    matrix={activeMatrix}
                    baseMatrix={baseMatrix || undefined}
                    cellDisplayMode={displayMode}
                    palette={palette}
                    xLabels={xLabels}
                    yLabels={yLabels}
                    xAxisName={xAxisName}
                    yAxisName={yAxisName}
                    unit={selectedMap.scaling.unit || 'raw'}
                    decimals={selectedMap.scaling.decimals ?? 1}
                    selectedRange={tableSelection}
                    onSelectRange={setTableSelection}
                    onChangeMatrix={onChangeMatrix}
                    onPatchCells={onPatchCells}
                    checksumRom={romData}
                    enableSmoothingPreview
                    interpolationAxes={interpolationAxes}
                    onCellDisplayModeChange={setDisplayMode}
                    onPaletteChange={setPalette}
                    actionRef={heatmapActionRef}
                    onRevertToOriginal={onRevertToOriginal}
                    onEditMapProperties={onEditMapProperties}
                  />
                </div>
              )}
              <div className="rounded-md border border-kiviBorder/60 bg-kiviSurface p-2">
                <div className="mb-1.5 flex items-center justify-between px-1">
                  <span className="flex items-center gap-1 text-xs font-medium text-kiviSecondary">
                    <Activity className="h-3.5 w-3.5 text-kiviAccent" />
                    <span>ROM Waveform Profile (Pitch: {waveformPitch})</span>
                  </span>
                </div>
                <KiviWaveform
                  data={romData}
                  romSize={romSize}
                  romRevision={romRevision}
                  activeDocId={activeDocId}
                  pitch={waveformPitch}
                  is16Bit={waveformIs16Bit}
                  isBigEndian={waveformIsBigEndian}
                  height={220}
                  diffRegions={diffRegions}
                  detectedCandidates={detectedCandidates}
                  onSelectRange={onSelectRange}
                  onSelectRegion={onJumpToWaveform}
                />
              </div>
            </div>
          ) : (
            <div className="flex min-w-0 flex-1 flex-col gap-4">
              {selectedMap && activeMatrix ? (
                showScalarFlagPrimary ? (
                  <ScalarFlagEditor
                    mapDef={selectedMap}
                    value={activeMatrix[0]?.[0] ?? 0}
                    baseValue={baseMatrix?.[0]?.[0] ?? null}
                    onCommitValue={(nextVal) => {
                      if (onPatchCells) {
                        onPatchCells([{ row: 0, col: 0, value: nextVal }], `Edit ${selectedMap.name}`);
                      } else {
                        onChangeMatrix([[nextVal]]);
                      }
                    }}
                    onRevert={
                      onRevertToOriginal
                        ? () => onRevertToOriginal({ minR: 0, maxR: 0, minC: 0, maxC: 0 })
                        : undefined
                    }
                    onEditProperties={onEditMapProperties}
                    onSwitchToGrid={() => setForceGridView(true)}
                  />
                ) : (
                  <div className="flex min-w-0 flex-1 flex-col gap-2">
                    {isScalarOrFlagMap(selectedMap) && (
                      <div className="flex items-center justify-end">
                        <button
                          type="button"
                          onClick={() => setForceGridView(false)}
                          className="inline-flex items-center gap-1.5 rounded border border-kiviAccent/40 bg-kiviAccent/10 px-2.5 py-1 text-[11px] font-medium text-kiviAccent hover:bg-kiviAccent/20"
                        >
                          <Sliders className="h-3.5 w-3.5" />
                          <span>Switch to Scalar / Flag View</span>
                        </button>
                      </div>
                    )}
                    <HeatmapTable
                      title={selectedMap.name}
                      tableId={selectedMap.id}
                      matrix={activeMatrix}
                      baseMatrix={baseMatrix || undefined}
                      cellDisplayMode={displayMode}
                      palette={palette}
                      xLabels={xLabels}
                      yLabels={yLabels}
                      xAxisName={xAxisName}
                      yAxisName={yAxisName}
                      unit={selectedMap.scaling.unit || 'raw'}
                      decimals={selectedMap.scaling.decimals ?? 1}
                      selectedRange={tableSelection}
                      onSelectRange={setTableSelection}
                      onChangeMatrix={onChangeMatrix}
                      onPatchCells={onPatchCells}
                      checksumRom={romData}
                      enableSmoothingPreview
                      interpolationAxes={interpolationAxes}
                      onCellDisplayModeChange={setDisplayMode}
                      onPaletteChange={setPalette}
                      actionRef={heatmapActionRef}
                      onRevertToOriginal={onRevertToOriginal}
                      onEditMapProperties={onEditMapProperties}
                    />
                  </div>
                )
              ) : (
                <div className="flex flex-1 flex-col items-center justify-center p-8 text-center">
                  <div className="mb-4 flex h-14 w-14 items-center justify-center rounded-lg border border-kiviBorder bg-kiviSurface">
                    <Binary className="h-7 w-7 text-kiviAccent" />
                  </div>
                  <h3 className="mb-1 text-base font-medium text-kiviInk">
                    {effectiveRomReady ? 'Select a Calibration Table or Tool Tab' : 'Open an ECU BIN/ROM to Begin'}
                  </h3>
                  <p className="mb-6 max-w-sm text-xs leading-relaxed text-kiviMuted">
                    {effectiveRomReady
                      ? 'Pick a mapped table from Explorer, scan unknown firmware tables, or inspect bytes in the virtualized Hex and Waveform tabs.'
                      : 'Load a factory BIN, S19, or Intel HEX image and pair it with XDF, A2L, DAMOS, INI, or KP definitions.'}
                  </p>
                  <div className="flex flex-wrap items-center justify-center gap-2.5">
                    <button
                      type="button"
                      onClick={onRunScanner}
                      disabled={!effectiveRomReady}
                      className="flex min-h-[44px] items-center gap-1.5 whitespace-nowrap rounded-md border border-kiviAccent/50 bg-kiviAccent/15 px-4 py-2 text-xs font-semibold text-kiviAccent transition-colors hover:bg-kiviAccent/25 disabled:opacity-40 sm:min-h-0"
                    >
                      <Search className="h-4 w-4 shrink-0" />
                      <span>Run Heuristic Scanner</span>
                    </button>
                    <button
                      type="button"
                      onClick={() => setLayoutPreset('full_waveform')}
                      className="min-h-[44px] whitespace-nowrap rounded-md bg-kiviAccent px-4 py-2 text-xs font-semibold text-kiviAccentFg transition-opacity hover:opacity-90 sm:min-h-0"
                    >
                      Open Waveform Explorer
                    </button>
                  </div>
                </div>
              )}
            </div>
          )}
        </div>

        {renderSplitPane()}
      </div>
    </main>
  );
});
