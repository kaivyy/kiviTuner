import type { CommandPaletteAction } from '../components/workspace/CommandPalette';
import type { MapDef } from '../types/kivi';
import type { UndoStatus } from './bridge/windowBridge';
import type { KiviTheme } from '../stores/useThemeStore';

export interface BuildCommandPaletteParams {
  hasRom: boolean;
  selectedMap: MapDef | undefined;
  undoStatus: UndoStatus;
  splitTabId: string | null;
  mapsCount: number;
  theme: KiviTheme;
  onOpenRom: () => void;
  onOpenRomTab: () => void;
  onSaveRom: (saveAs: boolean) => void;
  onCreateMap: () => void;
  onRevertToOriginal: () => void;
  onUndo: () => void;
  onRedo: () => void;
  onOpenHex: () => void;
  onJumpHexMap: (address: number) => void;
  onOpenWaveform: () => void;
  onToggleSplit: () => void;
  onRunScanner: () => void;
  onOpenWorkbench: () => void;
  onOpenFlash: () => void;
  onOpenDiff: () => void;
  onFixChecksum: () => void;
  onExportDef: (format: 'xdf' | 'ini' | 'kp') => void;
  onToggleTheme: () => void;
  onOpenHowToUse?: () => void;
  onCheckForUpdates?: () => void;
}

export function buildCommandPaletteActions(
  p: BuildCommandPaletteParams
): CommandPaletteAction[] {
  return [
    {
      id: 'open-rom',
      label: 'Open Primary BIN/ROM...',
      category: 'File',
      shortcut: 'Ctrl+O',
      onExecute: p.onOpenRom,
    },
    {
      id: 'open-rom-tab',
      label: 'Open Additional ROM Document Tab...',
      category: 'File',
      onExecute: p.onOpenRomTab,
    },
    {
      id: 'save-rom',
      label: 'Save Modified ROM Binary',
      category: 'File',
      shortcut: 'Ctrl+S',
      disabled: !p.hasRom,
      onExecute: () => p.onSaveRom(false),
    },
    {
      id: 'save-as-rom',
      label: 'Save Modified ROM As...',
      category: 'File',
      shortcut: 'Ctrl+Shift+S',
      disabled: !p.hasRom,
      onExecute: () => p.onSaveRom(true),
    },
    {
      id: 'create-map',
      label: 'Create New Map / Parameter Definition...',
      category: 'Definitions',
      onExecute: p.onCreateMap,
    },
    {
      id: 'revert-stock',
      label: 'Revert Active Map to Stock Baseline',
      category: 'Edit',
      disabled: !p.selectedMap || !p.hasRom,
      onExecute: p.onRevertToOriginal,
    },
    {
      id: 'undo',
      label: p.undoStatus.last_label
        ? `Undo: ${p.undoStatus.last_label}`
        : 'Undo Last ROM Edit',
      category: 'Edit',
      shortcut: 'Ctrl+Z',
      disabled: !p.undoStatus.can_undo,
      onExecute: p.onUndo,
    },
    {
      id: 'redo',
      label: 'Redo Undone Edit',
      category: 'Edit',
      shortcut: 'Ctrl+Y',
      disabled: !p.undoStatus.can_redo,
      onExecute: p.onRedo,
    },
    {
      id: 'open-hex',
      label: 'Open Virtualized Hex Dump Tab',
      category: 'View',
      onExecute: p.onOpenHex,
    },
    {
      id: 'jump-hex-map',
      label: 'Jump to Selected Map Address in Hex Dump',
      category: 'View',
      disabled: !p.selectedMap || !p.hasRom,
      onExecute: () => {
        if (p.selectedMap) p.onJumpHexMap(p.selectedMap.address);
      },
    },
    {
      id: 'open-waveform',
      label: 'Open 2D Binary Waveform Tab',
      category: 'View',
      onExecute: p.onOpenWaveform,
    },
    {
      id: 'toggle-split',
      label: p.splitTabId ? 'Close Split Workspace Pane' : 'Open Split Workspace Pane',
      category: 'View',
      onExecute: p.onToggleSplit,
    },
    {
      id: 'run-scanner',
      label: 'Run Discovery Auto-Scanner',
      category: 'Scanner',
      shortcut: 'Ctrl+F',
      disabled: !p.hasRom,
      onExecute: p.onRunScanner,
    },
    {
      id: 'workbench',
      label: 'Open Calibration Workbench (Blind V2, Checksums, Solver)',
      category: 'Tools',
      shortcut: 'Ctrl+Shift+W',
      onExecute: p.onOpenWorkbench,
    },
    {
      id: 'flash',
      label: 'Open Hardware ECU Flash & Dump Panel',
      category: 'Hardware',
      onExecute: p.onOpenFlash,
    },
    {
      id: 'diff',
      label: 'Open Binary Diff & Reference Verification',
      category: 'Analysis',
      shortcut: 'Ctrl+B',
      onExecute: p.onOpenDiff,
    },
    {
      id: 'fix-checksum',
      label: 'Auto-Detect & Repair ECU Checksum',
      category: 'Checksum',
      disabled: !p.hasRom,
      onExecute: p.onFixChecksum,
    },
    {
      id: 'export-xdf',
      label: 'Export Maps as XDF (.xdf)',
      category: 'Definitions',
      disabled: p.mapsCount === 0,
      onExecute: () => p.onExportDef('xdf'),
    },
    {
      id: 'export-ini',
      label: 'Export Maps as ECU INI (.ini)',
      category: 'Definitions',
      disabled: p.mapsCount === 0,
      onExecute: () => p.onExportDef('ini'),
    },
    {
      id: 'export-kp',
      label: 'Export Maps as CSV / KP',
      category: 'Definitions',
      disabled: p.mapsCount === 0,
      onExecute: () => p.onExportDef('kp'),
    },
    {
      id: 'toggle-theme',
      label:
        p.theme === 'dark'
          ? 'Switch to Lab Alabaster Light Theme'
          : 'Switch to Carbon Instrument Dark Theme',
      category: 'Appearance',
      onExecute: p.onToggleTheme,
    },
    ...(p.onOpenHowToUse
      ? [
          {
            id: 'how-to-use',
            label: 'How to Use kiviTuner (Tutorial)',
            category: 'Help',
            onExecute: p.onOpenHowToUse,
          },
        ]
      : []),
    ...(p.onCheckForUpdates
      ? [
          {
            id: 'check-updates',
            label: 'Check for Application Updates...',
            category: 'Help',
            onExecute: p.onCheckForUpdates,
          },
        ]
      : []),
  ];
}
