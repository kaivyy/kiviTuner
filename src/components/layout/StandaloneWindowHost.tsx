import React, { Suspense, lazy, useEffect, useMemo, useState } from 'react';
import { applyThemeToDocument, useThemeStore, type KiviTheme } from '../../stores/useThemeStore';
import { useMapStore } from '../../stores/useMapStore';
import { bridgeWindowClose, type ToolWindowId } from '../../lib/windowControlsBridge';
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
  toolId: ToolWindowId;
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
        useMapStore.getState().setMaps(ev.maps);
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
  }, [toolId, selectedMap, romLoaded, romData]);

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

/**
 * Renders an independent tool popup directly as the root of a standalone
 * frameless Tauri desktop window (`?window=<id>`), lazy-loading only the active
 * tool view and skipping heavy workspace ROM subscriptions for static tools.
 */
export const StandaloneWindowHost: React.FC<StandaloneWindowHostProps> = ({ toolId }) => {
  useSyncedWindowTheme();

  const handleClose = () => {
    void bridgeWindowClose();
  };

  let content: React.ReactNode;
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
      content = <StandaloneRomWindowHost toolId={toolId} onClose={handleClose} />;
      break;
  }

  return (
    <Suspense
      fallback={
        <div className="fixed inset-0 flex h-screen w-screen items-center justify-center bg-kiviSurface font-mono text-xs text-kiviMuted">
          Loading {toolId}...
        </div>
      }
    >
      {content}
    </Suspense>
  );
};
