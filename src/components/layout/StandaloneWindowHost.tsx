import React, { useEffect } from 'react';
import { applyThemeToDocument, useThemeStore, type KiviTheme } from '../../stores/useThemeStore';
import { useMapStore } from '../../stores/useMapStore';
import { bridgeWindowClose, type ToolWindowId } from '../../lib/windowControlsBridge';
import { isDesktopApp } from '../../lib/tauriBridge';
import { TelemetryDevPanel } from '../telemetry/TelemetryDevPanel';
import { FlashPanel } from '../toolkit/FlashPanel';
import { EngineeringToolkit } from '../toolkit/EngineeringToolkit';
import { CataloguePanel } from '../toolkit/CataloguePanel';
import { ShortcutsModal } from '../modals/ShortcutsModal';
import { AboutModal } from '../modals/AboutModal';
import { HowToUseModal } from '../modals/HowToUseModal';

export interface StandaloneWindowHostProps {
  toolId: ToolWindowId;
}

/**
 * Renders an independent tool popup directly as the root of a standalone
 * frameless Tauri desktop window (`?window=<id>`), keeping theme and workspace ROM
 * metadata synchronized with the main window.
 */
export const StandaloneWindowHost: React.FC<StandaloneWindowHostProps> = ({ toolId }) => {
  const theme = useThemeStore((s) => s.theme);
  const romSize = useMapStore((s) => s.romSize);
  const romData = useMapStore((s) => s.romData);
  const romDocuments = useMapStore((s) => s.romDocuments);
  const activeDocId = useMapStore((s) => s.activeDocId);
  const hashes = useMapStore((s) => s.hashes);
  const mapsCount = useMapStore((s) => s.maps.length);
  const checksumAlgorithmName = useMapStore((s) => s.checksumAlgorithmName);
  const activeDocSummary = romDocuments.find((d) => d.id === activeDocId || d.is_active);
  const romLabel = activeDocSummary?.label ?? (romSize > 0 ? 'loaded.bin' : null);

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

  useEffect(() => {
    if (isDesktopApp()) {
      void useMapStore.getState().refreshRomSession();
    }
  }, []);

  const handleClose = () => {
    void bridgeWindowClose();
  };

  switch (toolId) {
    case 'telemetry':
      return <TelemetryDevPanel standalone onClose={handleClose} />;
    case 'flash':
      return (
        <FlashPanel
          isOpen
          standalone
          onClose={handleClose}
          rom={romData}
          onDumped={async (bytes, baseAddress) => {
            await useMapStore.getState().loadRomBuffer(bytes, undefined, {
              label: `ECU Dump @ 0x${baseAddress.toString(16).toUpperCase()}`,
            });
            useMapStore.getState().setRomBaseAddress(baseAddress);
          }}
        />
      );
    case 'toolkit':
      return <EngineeringToolkit isOpen standalone onClose={handleClose} />;
    case 'catalogue':
      return <CataloguePanel isOpen standalone onClose={handleClose} />;
    case 'guide':
      return <HowToUseModal isOpen standalone onClose={handleClose} />;
    case 'shortcuts':
      return <ShortcutsModal isOpen standalone onClose={handleClose} />;
    case 'about':
    default:
      return (
        <AboutModal
          isOpen
          standalone
          onClose={handleClose}
          isDesktop={isDesktopApp()}
          romLabel={romLabel ?? 'None'}
          romSize={romSize}
          sha256={hashes?.sha256 ?? null}
          mapsCount={mapsCount}
          checksumAlgorithmName={checksumAlgorithmName}
        />
      );
  }
};
