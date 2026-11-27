import React from 'react';
import { Cpu, Download, Layers, ShieldCheck } from 'lucide-react';
import { FloatingWindow } from '../layout/FloatingWindow';
import { TOOL_WINDOW_SPECS } from '../layout/toolWindowRegistry';
import { APP_VERSION } from '../../lib/updaterBridge';
import { useUpdateStore } from '../../stores/useUpdateStore';

interface AboutModalProps {
  isOpen: boolean;
  onClose: () => void;
  isDesktop: boolean;
  romLabel: string;
  romSize: number;
  sha256: string | null;
  mapsCount: number;
  checksumAlgorithmName: string | null;
  standalone?: boolean;
}

export const AboutModal: React.FC<AboutModalProps> = ({
  isOpen,
  onClose,
  isDesktop,
  romLabel,
  romSize,
  sha256,
  mapsCount,
  checksumAlgorithmName,
  standalone = false,
}) => {
  const spec = TOOL_WINDOW_SPECS.about;

  return (
    <FloatingWindow
      windowId="about"
      isOpen={isOpen}
      onClose={onClose}
      title="About kiviTuner"
      titleId="about-modal-title"
      icon={<Cpu className="h-4 w-4" />}
      defaultWidth={spec.width}
      defaultHeight={spec.height}
      minWidth={spec.minWidth}
      minHeight={spec.minHeight}
      dock={spec.dock}
      standalone={standalone}
      popoutToolId="about"
      closeLabel="Close About dialog"
      footer={
        <div className="flex shrink-0 items-center justify-between gap-2 border-t border-kiviBorder bg-kiviElevated px-5 py-2.5">
          <button
            type="button"
            onClick={() => void useUpdateStore.getState().checkForUpdates(false)}
            className="inline-flex items-center gap-1.5 rounded border border-kiviBorder/70 bg-kiviSurface px-3 py-1.5 text-xs font-medium text-kiviInk hover:bg-kiviBg"
          >
            <Download className="h-3.5 w-3.5 text-kiviAccent" />
            <span>Check for Updates</span>
          </button>
          <button
            type="button"
            onClick={onClose}
            className="rounded border border-kiviAccent bg-kiviAccent px-3 py-1.5 text-xs font-semibold text-kiviAccentFg hover:opacity-90"
          >
            Close
          </button>
        </div>
      }
    >
      <div className="flex-1 space-y-4 overflow-y-auto p-5 text-xs text-kiviSecondary">
        <p className="text-[11px] text-kiviMuted">
          ECU Binary Calibration &amp; Reverse Engineering Workbench
        </p>
        <p className="leading-relaxed">
          kiviTuner combines XDF/A2L calibration editing, 2D binary waveform slicing, synchronized
          hex inspection, automated 2D/1D table scanning, and multi-OEM checksum verification.
        </p>

        <div className="space-y-2 rounded-md border border-kiviBorder bg-kiviBg p-3 font-mono text-[11px] tabular-nums">
          <div className="flex items-center justify-between gap-2">
            <span className="text-kiviMuted">Version</span>
            <span className="text-kiviAccent font-semibold">v{APP_VERSION}</span>
          </div>
          <div className="flex items-center justify-between gap-2">
            <span className="text-kiviMuted">Runtime Mode</span>
            <span className="text-kiviInk">{isDesktop ? 'Desktop (Rust Tauri)' : 'Web Preview'}</span>
          </div>
          <div className="flex items-center justify-between gap-2">
            <span className="text-kiviMuted">Active ROM</span>
            <span className="truncate max-w-[200px] text-kiviInk">{romLabel}</span>
          </div>
          <div className="flex items-center justify-between gap-2">
            <span className="text-kiviMuted">ROM Size</span>
            <span className="text-kiviInk">
              {romSize > 0 ? `${romSize.toLocaleString()} bytes` : 'Not loaded'}
            </span>
          </div>
          <div className="flex items-center justify-between gap-2">
            <span className="text-kiviMuted">Loaded Maps</span>
            <span className="text-kiviInk">{mapsCount}</span>
          </div>
          <div className="flex items-center justify-between gap-2">
            <span className="text-kiviMuted">Checksum Family</span>
            <span className="text-kiviInk">{checksumAlgorithmName ?? 'Auto / Unverified'}</span>
          </div>
          {sha256 && (
            <div className="flex items-center justify-between gap-3">
              <span className="text-kiviMuted">SHA-256</span>
              <span className="truncate text-kiviAccent" title={sha256}>
                {sha256.slice(0, 16)}...
              </span>
            </div>
          )}
        </div>

        <div className="grid grid-cols-[repeat(auto-fit,minmax(175px,1fr))] gap-2 text-[11px]">
          <div className="flex items-center gap-2 rounded border border-kiviBorder bg-kiviBg px-2.5 py-2">
            <Layers className="h-3.5 w-3.5 shrink-0 text-kiviAccent" />
            <span>u8 / i8 / u16 / i16 / u32 / i32 / f32</span>
          </div>
          <div className="flex items-center gap-2 rounded border border-kiviBorder bg-kiviBg px-2.5 py-2">
            <ShieldCheck className="h-3.5 w-3.5 shrink-0 text-kiviGreen" />
            <span>Affine &amp; Keihin / OEM Checksums</span>
          </div>
        </div>
      </div>
    </FloatingWindow>
  );
};
