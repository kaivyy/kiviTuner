import { useEffect, type RefObject } from 'react';
import type { LayoutPreset } from '../App';
import { useTelemetryStore } from '../stores/useTelemetryStore';

interface GlobalShortcutsOptions {
  binFileInputRef: RefObject<HTMLInputElement | null>;
  xdfFileInputRef: RefObject<HTMLInputElement | null>;
  setLayoutPreset: React.Dispatch<React.SetStateAction<LayoutPreset>>;
  setIsWorkbenchOpen: (open: boolean) => void;
  setIsDiffModalOpen: (open: boolean) => void;
  setIsShortcutsOpen: (open: boolean) => void;
  onRunScanner: () => void;
  onSaveRom: () => void;
  onSaveAsRom?: () => void;
  onOpenCommandPalette?: () => void;
  onUndo?: () => void;
  onRedo?: () => void;
  onCloseActiveTab?: () => void;
}

export function useGlobalShortcuts({
  binFileInputRef,
  xdfFileInputRef,
  setLayoutPreset,
  setIsWorkbenchOpen,
  setIsDiffModalOpen,
  setIsShortcutsOpen,
  onRunScanner,
  onSaveRom,
  onSaveAsRom,
  onOpenCommandPalette,
  onUndo,
  onRedo,
  onCloseActiveTab,
}: GlobalShortcutsOptions): void {
  useEffect(() => {
    const handleKeyNav = (e: KeyboardEvent) => {
      const targetTag = (e.target as HTMLElement)?.tagName;
      const inTextInput = ['INPUT', 'TEXTAREA'].includes(targetTag);

      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        onOpenCommandPalette?.();
        return;
      }

      if (inTextInput) {
        return;
      }

      if ((e.ctrlKey || e.metaKey) && !e.shiftKey && e.key.toLowerCase() === 'z') {
        e.preventDefault();
        onUndo?.();
        return;
      }

      if (
        (e.ctrlKey || e.metaKey) &&
        (e.key.toLowerCase() === 'y' || (e.shiftKey && e.key.toLowerCase() === 'z'))
      ) {
        e.preventDefault();
        onRedo?.();
        return;
      }

      if ((e.ctrlKey || e.metaKey) && !e.shiftKey && e.key.toLowerCase() === 'w') {
        e.preventDefault();
        onCloseActiveTab?.();
        return;
      }

      if ((e.ctrlKey || e.metaKey) && e.key === '\\') {
        e.preventDefault();
        setLayoutPreset((prev) => (prev === 'split_3d' ? 'standard' : 'split_3d'));
        return;
      }

      if ((e.ctrlKey || e.metaKey) && e.shiftKey && e.key.toLowerCase() === 'w') {
        e.preventDefault();
        setIsWorkbenchOpen(true);
        return;
      }

      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'b') {
        e.preventDefault();
        setIsDiffModalOpen(true);
        return;
      }

      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'f') {
        e.preventDefault();
        onRunScanner();
        return;
      }

      if (e.key === 'F1') {
        e.preventDefault();
        setIsShortcutsOpen(true);
        return;
      }

      if (e.key === 'F9') {
        e.preventDefault();
        const telemetry = useTelemetryStore.getState();
        if (telemetry.isRecording) {
          telemetry.stopRecording();
        } else {
          telemetry.startRecording();
        }
        return;
      }

      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'o') {
        e.preventDefault();
        binFileInputRef.current?.click();
        return;
      }

      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'd') {
        e.preventDefault();
        xdfFileInputRef.current?.click();
        return;
      }

      if ((e.ctrlKey || e.metaKey) && e.shiftKey && e.key.toLowerCase() === 's') {
        e.preventDefault();
        (onSaveAsRom ?? onSaveRom)();
        return;
      }

      if ((e.ctrlKey || e.metaKey) && !e.shiftKey && e.key.toLowerCase() === 's') {
        e.preventDefault();
        onSaveRom();
        return;
      }
    };

    window.addEventListener('keydown', handleKeyNav);
    return () => window.removeEventListener('keydown', handleKeyNav);
  }, [
    binFileInputRef,
    xdfFileInputRef,
    setLayoutPreset,
    setIsWorkbenchOpen,
    setIsDiffModalOpen,
    setIsShortcutsOpen,
    onRunScanner,
    onSaveRom,
    onSaveAsRom,
    onOpenCommandPalette,
    onUndo,
    onRedo,
    onCloseActiveTab,
  ]);
}
