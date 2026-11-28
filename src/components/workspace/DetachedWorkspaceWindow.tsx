import React from 'react';
import {
  Activity,
  Binary,
  Box,
  Columns,
  ExternalLink,
  PanelTop,
  Table2,
} from 'lucide-react';
import type { MapDef } from '../../types/kivi';
import type { WorkspaceTab, WorkspaceTabKind } from '../../stores/useWorkspaceTabsStore';
import { FloatingWindow } from '../layout/FloatingWindow';
import { isDesktopApp, type DetectedMapCandidate, type DiffRegion } from '../../lib/tauriBridge';
import { bridgeOpenWorkspaceTabWindow } from '../../lib/windowControlsBridge';
import type { CellDisplayMode, MarkedRange, SelectionRange, TablePalette } from '../editor';
import { WorkspaceTabContent } from './WorkspaceTabContent';

export interface DetachedWorkspaceWindowProps {
  tab: WorkspaceTab;
  cascadeIndex: number;
  mapDef?: MapDef;
  displayMode: CellDisplayMode;
  onCellDisplayModeChange: (mode: CellDisplayMode) => void;
  palette: TablePalette;
  onPaletteChange: (palette: TablePalette) => void;
  hexMarks: MarkedRange[];
  hexJumpAddress: number | null;
  onCreateMapFromHex: (startAddress: number, byteLength: number) => void;
  onPatchByte: (offset: number, value: number) => Promise<boolean> | boolean | void;
  waveformPitch: number;
  onWaveformPitchChange: (pitch: number) => void;
  waveformIs16Bit: boolean;
  waveformIsBigEndian: boolean;
  diffRegions: DiffRegion[];
  detectedCandidates: DetectedMapCandidate[];
  onSelectWaveformRange: (start: number, end: number) => void;
  onJumpToWaveform: (startAddr: number, endAddr: number) => void;
  onRevertToOriginal: (mapDef: MapDef, selection?: SelectionRange | null) => void;
  onEditMapProperties: (mapDef: MapDef) => void;
  onSetKind: (id: string, kind: WorkspaceTabKind) => void;
  onDock: (id: string) => void;
  onClose: (id: string) => void;
}

function TabBadgeIcon({ kind }: { kind: WorkspaceTabKind }) {
  switch (kind) {
    case 'map-3d':
      return <Box className="h-3.5 w-3.5" />;
    case 'map-split':
      return <Columns className="h-3.5 w-3.5" />;
    case 'hex':
      return <Binary className="h-3.5 w-3.5" />;
    case 'waveform':
      return <Activity className="h-3.5 w-3.5" />;
    default:
      return <Table2 className="h-3.5 w-3.5" />;
  }
}

export const DetachedWorkspaceWindow: React.FC<DetachedWorkspaceWindowProps> = ({
  tab,
  cascadeIndex,
  mapDef,
  displayMode,
  onCellDisplayModeChange,
  palette,
  onPaletteChange,
  hexMarks,
  hexJumpAddress,
  onCreateMapFromHex,
  onPatchByte,
  waveformPitch,
  onWaveformPitchChange,
  waveformIs16Bit,
  waveformIsBigEndian,
  diffRegions,
  detectedCandidates,
  onSelectWaveformRange,
  onJumpToWaveform,
  onRevertToOriginal,
  onEditMapProperties,
  onSetKind,
  onDock,
  onClose,
}) => {
  const isMapTab =
    tab.kind === 'map-table' || tab.kind === 'map-3d' || tab.kind === 'map-split';

  const handlePopout = () => {
    void bridgeOpenWorkspaceTabWindow({
      tabId: tab.id,
      kind: tab.kind,
      title: tab.title,
      mapId: tab.mapId,
      width: tab.kind === 'map-split' ? 960 : 760,
      height: 580,
    }).then((ok) => {
      if (ok) onClose(tab.id);
    });
  };

  return (
    <FloatingWindow
      windowId={`tab-${tab.id}`}
      isOpen
      onClose={() => onClose(tab.id)}
      title={tab.title}
      badge={mapDef ? `0x${mapDef.address.toString(16).toUpperCase()}` : undefined}
      icon={<TabBadgeIcon kind={tab.kind} />}
      defaultWidth={tab.kind === 'map-split' ? 920 : 680}
      defaultHeight={tab.kind === 'map-split' ? 560 : 520}
      minWidth={380}
      minHeight={320}
      cascadeIndex={cascadeIndex}
      titlebarTone="sidebar"
      closeLabel={`Close ${tab.title}`}
      headerActions={
        <div className="flex items-center gap-1">
          {isMapTab && (
            <div className="flex items-center rounded bg-kiviSurface/80 p-0.5 text-[10px]">
              {(
                [
                  { kind: 'map-table', label: '2D' },
                  { kind: 'map-split', label: 'Split' },
                  { kind: 'map-3d', label: '3D' },
                ] as const
              ).map((item) => (
                <button
                  key={item.kind}
                  type="button"
                  data-testid={`detached-kind-${tab.id}-${item.kind}`}
                  onClick={() => onSetKind(tab.id, item.kind)}
                  className={`rounded px-1.5 py-0.5 font-medium transition-colors ${
                    tab.kind === item.kind
                      ? 'bg-kiviAccent text-kiviAccentFg'
                      : 'text-kiviSecondary hover:text-kiviInk'
                  }`}
                >
                  {item.label}
                </button>
              ))}
            </div>
          )}
          <button
            type="button"
            data-testid={`dock-tab-${tab.id}`}
            aria-label={`Dock ${tab.title}`}
            title="Dock back into main tab bar"
            onClick={() => onDock(tab.id)}
            className="inline-flex h-6 items-center gap-1 rounded bg-kiviElevated/70 px-1.5 text-[10px] font-medium text-kiviSecondary transition-colors hover:bg-kiviElevated hover:text-kiviInk"
          >
            <PanelTop className="h-3 w-3 shrink-0 text-kiviAccent" />
            <span>Dock</span>
          </button>
          {isDesktopApp() && (
            <button
              type="button"
              aria-label={`Pop out ${tab.title} to OS window`}
              title="Open in separate desktop OS window"
              onClick={handlePopout}
              className="inline-flex h-6 items-center gap-1 rounded bg-kiviElevated/70 px-1.5 text-[10px] text-kiviSecondary transition-colors hover:bg-kiviElevated hover:text-kiviInk"
            >
              <ExternalLink className="h-3 w-3 shrink-0" />
            </button>
          )}
        </div>
      }
    >
      <div className="flex min-h-0 flex-1 flex-col overflow-hidden bg-kiviBg p-2.5">
        <WorkspaceTabContent
          tab={tab}
          mapDef={mapDef}
          displayMode={displayMode}
          onCellDisplayModeChange={onCellDisplayModeChange}
          palette={palette}
          onPaletteChange={onPaletteChange}
          hexMarks={hexMarks}
          hexJumpAddress={hexJumpAddress}
          onCreateMapFromHex={onCreateMapFromHex}
          onPatchByte={onPatchByte}
          waveformPitch={waveformPitch}
          onWaveformPitchChange={onWaveformPitchChange}
          waveformIs16Bit={waveformIs16Bit}
          waveformIsBigEndian={waveformIsBigEndian}
          diffRegions={diffRegions}
          detectedCandidates={detectedCandidates}
          onSelectWaveformRange={onSelectWaveformRange}
          onJumpToWaveform={onJumpToWaveform}
          onRevertToOriginal={onRevertToOriginal}
          onEditMapProperties={onEditMapProperties}
        />
      </div>
    </FloatingWindow>
  );
};
