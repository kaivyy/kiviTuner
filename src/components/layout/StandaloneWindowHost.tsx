import React, { Suspense, lazy, useEffect, useMemo, useRef, useState } from 'react';
import { Activity, Binary, Box, Columns, Table2 } from 'lucide-react';
import { applyThemeToDocument, useThemeStore, type KiviTheme } from '../../stores/useThemeStore';
import { useMapStore } from '../../stores/useMapStore';
import type { WorkspaceTab, WorkspaceTabKind } from '../../stores/useWorkspaceTabsStore';
import {
  bridgeWindowClose,
  type StandaloneTabParams,
  type ToolWindowId,
} from '../../lib/windowControlsBridge';
import {
  bridgeConvertDiffToMap,
  isDesktopApp,
  type DiffRegion,
} from '../../lib/tauriBridge';
import {
  emitWorkspaceSyncEvent,
  subscribeWorkspaceSyncEvents,
} from '../../lib/workspaceSyncBridge';
import type { MapDef } from '../../types/kivi';
import { FloatingWindow } from './FloatingWindow';
import { WorkspaceTabContent } from '../workspace/WorkspaceTabContent';

const TelemetryDevPanel = lazy(() =>
  import('../telemetry/TelemetryDevPanel').then((m) => ({ default: m.TelemetryDevPanel }))
);
const FlashPanel = lazy(() =>
  import('../toolkit/FlashPanel').then((m) => ({ default: m.FlashPanel }))
);
const EngineeringToolkit = lazy(() =>
  import('../toolkit/EngineeringToolkit').then((m) => ({ default: m.EngineeringToolkit }))
);
const CataloguePanel = lazy(() =>
  import('../toolkit/CataloguePanel').then((m) => ({ default: m.CataloguePanel }))
);
const ShortcutsModal = lazy(() =>
  import('../modals/ShortcutsModal').then((m) => ({ default: m.ShortcutsModal }))
);
const AboutModal = lazy(() =>
  import('../modals/AboutModal').then((m) => ({ default: m.AboutModal }))
);
const HowToUseModal = lazy(() =>
  import('../modals/HowToUseModal').then((m) => ({ default: m.HowToUseModal }))
);
const BinDiffModal = lazy(() =>
  import('../modals/BinDiffModal').then((m) => ({ default: m.BinDiffModal }))
);
const CalibrationWorkbenchModal = lazy(() =>
  import('../modals/CalibrationWorkbenchModal').then((m) => ({
    default: m.CalibrationWorkbenchModal,
  }))
);
const AutoTuneReviewModal = lazy(() =>
  import('../autotune/AutoTuneReviewModal').then((m) => ({ default: m.AutoTuneReviewModal }))
);

export interface StandaloneWindowHostProps {
  toolId?: ToolWindowId;
  tabParams?: StandaloneTabParams;
}

function useSyncedWindowTheme(): void {
  const theme = useThemeStore((s) => s.theme);
  useEffect(() => {
    applyThemeToDocument(theme);
    const onStorage = (e: StorageEvent) => {
      if (e.key === 'kivituner-theme' && (e.newValue === 'light' || e.newValue === 'dark')) {
        useThemeStore.getState().setTheme(e.newValue as KiviTheme);
      }
    };
    window.addEventListener('storage', onStorage);
    return () => window.removeEventListener('storage', onStorage);
  }, [theme]);
}

const StandaloneRomWindowHost: React.FC<{ toolId: ToolWindowId; onClose: () => void }> = ({
  toolId,
  onClose,
}) => {
  const romLoaded = useMapStore((s) => s.romLoaded);
  const romSize = useMapStore((s) => s.romSize);
  const romData = useMapStore((s) => s.romData);
  const romDocuments = useMapStore((s) => s.romDocuments);
  const activeDocId = useMapStore((s) => s.activeDocId);
  const hashes = useMapStore((s) => s.hashes);
  const maps = useMapStore((s) => s.maps);
  const selectedMapId = useMapStore((s) => s.selectedMapId);
  const romRevision = useMapStore((s) => s.romRevision);
  const activeMatrix = useMapStore((s) => s.activeMatrix);
  const checksumAlgorithmName = useMapStore((s) => s.checksumAlgorithmName);
  const [syncedDiffs, setSyncedDiffs] = useState<DiffRegion[]>([]);

  const activeDocSummary = romDocuments.find((d) => d.id === activeDocId || d.is_active);
  const romLabel = activeDocSummary?.label ?? (romSize > 0 ? 'loaded.bin' : null);
  const selectedMap = useMemo(
    () => maps.find((m) => m.id === selectedMapId) ?? null,
    [maps, selectedMapId]
  );

  useEffect(() => {
    if (isDesktopApp()) {
      void useMapStore.getState().refreshRomSession();
    }
    const unsub = subscribeWorkspaceSyncEvents((ev) => {
      if (ev.type === 'workspace-snapshot') {
        useMapStore.getState().setMaps(ev.maps, false);
        useMapStore.getState().selectMap(ev.selectedMapId);
        setSyncedDiffs(ev.diffs);
      } else if (ev.type === 'diffs-updated') {
        setSyncedDiffs(ev.diffs);
      } else if (ev.type === 'rom-updated' && isDesktopApp()) {
        void useMapStore.getState().refreshRomSession();
      }
    });
    void emitWorkspaceSyncEvent({ type: 'request-snapshot' });
    return unsub;
  }, []);

  useEffect(() => {
    if ((toolId === 'workbench' || toolId === 'autotune') && selectedMap && (romLoaded || romData)) {
      void useMapStore.getState().extractMatrix(selectedMap);
    }
  }, [toolId, selectedMap, romLoaded, romData, romRevision]);

  switch (toolId) {
    case 'telemetry':
      return <TelemetryDevPanel standalone onClose={onClose} />;
    case 'flash':
      return (
        <FlashPanel
          isOpen
          standalone
          onClose={onClose}
          rom={romData}
          onDumped={async (bytes, baseAddress) => {
            await useMapStore.getState().loadRomBuffer(bytes, undefined, {
              label: `ECU Dump @ 0x${baseAddress.toString(16).toUpperCase()}`,
            });
            useMapStore.getState().setRomBaseAddress(baseAddress);
            void emitWorkspaceSyncEvent({ type: 'rom-updated' });
          }}
        />
      );
    case 'diff':
      return (
        <BinDiffModal
          isOpen
          standalone
          onClose={onClose}
          baseRom={romData}
          currentDiffs={syncedDiffs}
          onDiffsChange={(nextDiffs) => {
            setSyncedDiffs(nextDiffs);
            void emitWorkspaceSyncEvent({ type: 'diffs-updated', diffs: nextDiffs });
          }}
          onJumpToWaveform={(startAddr, endAddr) => {
            void emitWorkspaceSyncEvent({ type: 'jump-waveform', startAddr, endAddr });
          }}
          onExtractMap={async (region) => {
            const mapDef = await bridgeConvertDiffToMap(region);
            useMapStore.getState().addMaps([mapDef]);
            useMapStore.getState().selectMap(mapDef.id);
            void emitWorkspaceSyncEvent({ type: 'add-maps', maps: [mapDef], selectFirst: true });
          }}
          onExtractAllTables={async (regions) => {
            const extracted: MapDef[] = [];
            for (const r of regions) {
              extracted.push(await bridgeConvertDiffToMap(r));
            }
            if (extracted.length > 0) {
              useMapStore.getState().addMaps(extracted);
              useMapStore.getState().selectMap(extracted[0].id);
              void emitWorkspaceSyncEvent({
                type: 'add-maps',
                maps: extracted,
                selectFirst: true,
              });
            }
          }}
          onVbfLoadRom={async (bytes, baseAddress) => {
            await useMapStore.getState().loadRomBuffer(bytes, undefined, { label: 'VBF Segment' });
            useMapStore.getState().setRomBaseAddress(baseAddress);
            void emitWorkspaceSyncEvent({ type: 'rom-updated' });
          }}
        />
      );
    case 'workbench':
      return (
        <CalibrationWorkbenchModal
          isOpen
          standalone
          onClose={onClose}
          rom={romData}
          romLoaded={romLoaded}
          romSize={romSize}
          maps={maps}
          selectedMap={selectedMap}
          activeMatrix={activeMatrix}
          selection={null}
          onAddMaps={(newMaps, selectFirst) => {
            useMapStore.getState().addMaps(newMaps);
            void emitWorkspaceSyncEvent({
              type: 'add-maps',
              maps: newMaps,
              selectFirst,
            });
          }}
          onSelectMap={(mapId) => {
            useMapStore.getState().selectMap(mapId);
            void emitWorkspaceSyncEvent({ type: 'select-map', mapId });
          }}
          onRomUpdated={async (_nextRom, checksumOk) => {
            if (isDesktopApp()) {
              await useMapStore.getState().refreshRomSession();
            }
            void emitWorkspaceSyncEvent({ type: 'rom-updated', checksumOk });
          }}
          onMatrixUpdated={async (mapDef, nextMatrix) => {
            await useMapStore.getState().updateMatrix(mapDef, nextMatrix);
            void emitWorkspaceSyncEvent({ type: 'rom-updated' });
          }}
          onUpdateMapScaling={(mapId, equation) => {
            const target = useMapStore.getState().maps.find((m) => m.id === mapId);
            if (target) {
              void useMapStore
                .getState()
                .updateMapDef({ ...target, scaling: { ...target.scaling, equation } });
            }
            void emitWorkspaceSyncEvent({ type: 'update-map-scaling', mapId, equation });
          }}
        />
      );
    case 'autotune':
      return <AutoTuneReviewModal isOpen standalone onClose={onClose} />;
    case 'about':
    default:
      return (
        <AboutModal
          isOpen
          standalone
          onClose={onClose}
          isDesktop={isDesktopApp()}
          romLabel={romLabel ?? 'None'}
          romSize={romSize}
          sha256={hashes?.sha256 ?? null}
          mapsCount={maps.length}
          checksumAlgorithmName={checksumAlgorithmName}
        />
      );
  }
};

const StandaloneWorkspaceTabHost: React.FC<{
  tabParams: StandaloneTabParams;
  onClose: () => void;
}> = ({ tabParams, onClose }) => {
  const maps = useMapStore((s) => s.maps);
  const romRevision = useMapStore((s) => s.romRevision);
  const activeDocId = useMapStore((s) => s.activeDocId);
  const [tabKind, setTabKind] = useState<WorkspaceTabKind>(
    (tabParams.kind as WorkspaceTabKind) || 'map-table'
  );
  const [syncedDiffs, setSyncedDiffs] = useState<DiffRegion[]>([]);
  const remoteRomRevRef = useRef<number>(romRevision);

  useEffect(() => {
    if (isDesktopApp()) {
      void useMapStore.getState().refreshRomSession();
    }
    const unsub = subscribeWorkspaceSyncEvents((ev) => {
      if (ev.type === 'workspace-snapshot') {
        useMapStore.getState().setMaps(ev.maps, false);
        setSyncedDiffs(ev.diffs);
      } else if (ev.type === 'diffs-updated') {
        setSyncedDiffs(ev.diffs);
      } else if (ev.type === 'rom-updated' && isDesktopApp()) {
        remoteRomRevRef.current = useMapStore.getState().romRevision + 1;
        void useMapStore.getState().refreshRomSession();
      }
    });
    void emitWorkspaceSyncEvent({ type: 'request-snapshot' });
    return unsub;
  }, []);

  useEffect(() => {
    if (!isDesktopApp()) return;
    if (romRevision === 0 || romRevision === remoteRomRevRef.current) return;
    void emitWorkspaceSyncEvent({ type: 'rom-updated', activeDocId, revision: romRevision });
  }, [romRevision, activeDocId]);

  const mapDef = useMemo(
    () => (tabParams.mapId ? maps.find((m) => m.id === tabParams.mapId) : undefined),
    [maps, tabParams.mapId]
  );

  const tab: WorkspaceTab = useMemo(
    () => ({
      id: tabParams.tabId,
      kind: tabKind,
      title: mapDef?.name ?? tabParams.title,
      mapId: tabParams.mapId,
    }),
    [tabParams.tabId, tabParams.title, tabParams.mapId, mapDef?.name, tabKind]
  );

  const isMapTab =
    tabKind === 'map-table' || tabKind === 'map-3d' || tabKind === 'map-split';

  const icon =
    tabKind === 'map-3d' ? (
      <Box className="h-3.5 w-3.5" />
    ) : tabKind === 'map-split' ? (
      <Columns className="h-3.5 w-3.5" />
    ) : tabKind === 'hex' ? (
      <Binary className="h-3.5 w-3.5" />
    ) : tabKind === 'waveform' ? (
      <Activity className="h-3.5 w-3.5" />
    ) : (
      <Table2 className="h-3.5 w-3.5" />
    );

  return (
    <FloatingWindow
      windowId={`tab-${tabParams.tabId}`}
      isOpen
      standalone
      onClose={onClose}
      title={tab.title}
      badge={mapDef ? `0x${mapDef.address.toString(16).toUpperCase()}` : undefined}
      icon={icon}
      titlebarTone="sidebar"
      headerActions={
        isMapTab ? (
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
                onClick={() => setTabKind(item.kind)}
                className={`rounded px-1.5 py-0.5 font-medium transition-colors ${
                  tabKind === item.kind
                    ? 'bg-kiviAccent text-kiviAccentFg'
                    : 'text-kiviSecondary hover:text-kiviInk'
                }`}
              >
                {item.label}
              </button>
            ))}
          </div>
        ) : undefined
      }
    >
      <div className="flex min-h-0 flex-1 flex-col overflow-hidden bg-kiviBg p-2.5">
        <WorkspaceTabContent
          tab={tab}
          mapDef={mapDef}
          diffRegions={syncedDiffs}
          onPatchByte={(offset, value) =>
            useMapStore
              .getState()
              .patchRomBytes([{ offset, bytes: [value] }], `Hex edit 0x${offset.toString(16).toUpperCase()}`)
          }
          onRevertToOriginal={(targetMap, sel) => {
            void useMapStore.getState().revertMapToOriginal(targetMap, sel);
          }}
        />
      </div>
    </FloatingWindow>
  );
};

/**
 * Renders an independent tool or popped-out workspace tab directly as the root of a
 * standalone frameless Tauri desktop window (`?window=<id>` or `?window=tab&...`).
 */
export const StandaloneWindowHost: React.FC<StandaloneWindowHostProps> = ({ toolId, tabParams }) => {
  useSyncedWindowTheme();

  const handleClose = () => {
    void bridgeWindowClose();
  };

  let content: React.ReactNode;
  if (tabParams) {
    content = <StandaloneWorkspaceTabHost tabParams={tabParams} onClose={handleClose} />;
  } else {
    switch (toolId) {
      case 'toolkit':
        content = <EngineeringToolkit isOpen standalone onClose={handleClose} />;
        break;
      case 'catalogue':
        content = <CataloguePanel isOpen standalone onClose={handleClose} />;
        break;
      case 'guide':
        content = <HowToUseModal isOpen standalone onClose={handleClose} />;
        break;
      case 'shortcuts':
        content = <ShortcutsModal isOpen standalone onClose={handleClose} />;
        break;
      default:
        content = <StandaloneRomWindowHost toolId={toolId ?? 'about'} onClose={handleClose} />;
        break;
    }
  }

  return (
    <Suspense
      fallback={
        <div className="fixed inset-0 flex h-screen w-screen items-center justify-center bg-kiviSurface font-mono text-xs text-kiviMuted">
          Loading {tabParams?.title ?? toolId ?? 'window'}...
        </div>
      }
    >
      {content}
    </Suspense>
  );
};
