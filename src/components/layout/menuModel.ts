import React from 'react';

export type MenuNodeType = 'action' | 'submenu' | 'checkbox' | 'radio' | 'separator';

export interface MenuNode {
  id: string;
  label: string;
  type?: MenuNodeType;
  shortcut?: string;
  icon?: React.ReactNode;
  checked?: boolean;
  disabled?: boolean;
  children?: MenuNode[];
  onSelect?: () => void;
}

export interface MenuBarCallbacks {
  onOpenRom?: () => void;
  onOpenRecent?: (entryId: string) => void;
  onClearRecentRoms?: () => void;
  onReferenceSlot?: (slot: number) => void;
  onClearReferences?: () => void;
  onSaveRom?: () => void;
  onSaveAsRom?: () => void;
  onExportCsv?: () => void;
  onExportReport?: () => void;
  onCloseRom?: () => void;

  onUndo?: () => void;
  onRedo?: () => void;
  onCommitProject?: () => void;
  onRollbackProject?: () => void;
  onViewLedger?: () => void;
  onCopy?: () => void;
  onPaste?: () => void;
  onPasteSpecial?: (mode: 'add' | 'multiply' | 'hex') => void;
  onSelectAll?: () => void;
  onInvertSelection?: () => void;
  onClearSelection?: () => void;
  onCellMath?: (op: 'add' | 'subtract' | 'scale' | 'set') => void;
  onSmooth3x3?: () => void;
  onInterpolate?: (mode: 'bilinear' | 'horizontal' | 'vertical') => void;

  onLoadXdf?: () => void;
  onSaveKiviJson?: () => void;
  onExportDef?: (format: 'xdf' | 'json' | 'ini' | 'kp') => void;
  onImportDef?: () => void;
  onCreateParameter?: (type: 'table' | 'axis' | 'scalar' | 'flag') => void;
  onXdfHeader?: () => void;

  onLayoutChange?: (layout: string) => void;
  onDisplayModeChange?: (mode: string) => void;
  onPaletteChange?: (palette: string) => void;
  onToggleSplitView?: () => void;
  onOpenCommandPalette?: () => void;
  onToggleTheme?: () => void;
  onResetLayout?: () => void;

  onExtractWaveformMap?: () => void;
  onRunScanner?: () => void;
  onScannerSensitivityChange?: (sens: 'strict' | 'balanced' | 'aggressive') => void;
  onClearScanner?: () => void;
  onOpenDiffModal?: () => void;
  onQuickDiff?: (target: 'base' | 'stage1') => void;
  onExtractDiffs?: () => void;
  onVerifyChecksum?: () => void;
  onFixChecksum?: () => void;

  onDriverChange?: (driver: string) => void;
  onConnectEcu?: () => void;
  onDisconnectEcu?: () => void;
  onReadEcuId?: () => void;
  /** Opens reference catalogue. */
  onOpenCatalogue?: () => void;
  /** Opens calibration workbench modal. */
  onOpenWorkbench?: () => void;
  onToggleTelemetry?: () => void;
  onToggleCrosshairSync?: () => void;
  onAfrAutoTune?: () => void;
  onStartLogging?: () => void;
  onStopLogging?: () => void;
  onExportLogCsv?: () => void;
  onReadFlash?: () => void;
  onWriteFlash?: () => void;

  onOpenHowToUse?: () => void;
  onOpenShortcuts?: () => void;
  onResetSettings?: () => void;
  onAbout?: () => void;
}

export interface MenuBarRecentRom {
  id: string;
  label: string;
  size: number;
  sha256: string;
}

export interface MenuBarState {
  romLoaded?: boolean;
  canUndo?: boolean;
  canRedo?: boolean;
  currentLayout?: string;
  displayMode?: string;
  palette?: string;
  scannerSensitivity?: 'strict' | 'balanced' | 'aggressive';
  ecuDriver?: string;
  theme?: 'dark' | 'light';
  crosshairSync?: boolean;
  ecuConnected?: boolean;
  isLogging?: boolean;
  recentRoms?: MenuBarRecentRom[];
}

/**
 * Builds the complete 7-menu calibration studio hierarchy.
 */
export function buildDefaultMenuTree(
  cb: MenuBarCallbacks = {},
  state: MenuBarState = {}
): MenuNode[] {
  const currentLayout = state.currentLayout ?? 'table';
  const displayMode = state.displayMode ?? 'eng';
  const palette = state.palette ?? 'thermal';
  const sensitivity = state.scannerSensitivity ?? 'balanced';
  const ecuDriver = state.ecuDriver ?? 'keihin';
  const romLoaded = state.romLoaded ?? false;
  const recentRoms = state.recentRoms ?? [];
  const recentChildren: MenuNode[] =
    recentRoms.length > 0
      ? [
          ...recentRoms.map((entry, idx) => ({
            id: `file-recent-${idx + 1}`,
            label: `${entry.label} (${Math.max(1, Math.round(entry.size / 1024))} KiB)`,
            type: 'action' as const,
            onSelect: () => cb.onOpenRecent?.(entry.id),
          })),
          { id: 'file-recent-sep', label: '', type: 'separator' as const },
          {
            id: 'file-recent-clear',
            label: 'Clear Recent ROMs',
            type: 'action' as const,
            onSelect: cb.onClearRecentRoms,
          },
        ]
      : [
          {
            id: 'file-recent-empty',
            label: 'No recent ROMs yet',
            type: 'action' as const,
            disabled: true,
          },
        ];

  return [
    // 1. FILE MENU
    {
      id: 'file',
      label: 'File',
      type: 'submenu',
      children: [
        {
          id: 'file-open',
          label: 'Open ROM...',
          type: 'action',
          shortcut: 'Ctrl+O',
          onSelect: cb.onOpenRom,
        },
        {
          id: 'file-open-recent',
          label: 'Recent ROMs',
          type: 'submenu',
          children: recentChildren,
        },
        {
          id: 'file-reference-slots',
          label: 'Reference Slots',
          type: 'submenu',
          children: [
            {
              id: 'file-ref-1',
              label: 'Slot 1: Stock Base ROM',
              type: 'action',
              onSelect: () => cb.onReferenceSlot?.(1),
            },
            {
              id: 'file-ref-2',
              label: 'Slot 2: (Empty)',
              type: 'action',
              disabled: true,
              onSelect: () => cb.onReferenceSlot?.(2),
            },
            {
              id: 'file-ref-clear',
              label: 'Clear References',
              type: 'action',
              onSelect: cb.onClearReferences,
            },
          ],
        },
        { id: 'file-sep-1', label: '', type: 'separator' },
        {
          id: 'file-save',
          label: 'Save',
          type: 'action',
          shortcut: 'Ctrl+S',
          disabled: !romLoaded,
          onSelect: cb.onSaveRom,
        },
        {
          id: 'file-save-as',
          label: 'Save As...',
          type: 'action',
          shortcut: 'Ctrl+Shift+S',
          disabled: !romLoaded,
          onSelect: cb.onSaveAsRom,
        },
        {
          id: 'file-export-csv',
          label: 'Export CSV / Deltas...',
          type: 'action',
          disabled: !romLoaded,
          onSelect: cb.onExportCsv,
        },
        {
          id: 'file-export-report',
          label: 'Export Calibration Report...',
          type: 'action',
          disabled: !romLoaded,
          onSelect: cb.onExportReport,
        },
        { id: 'file-sep-2', label: '', type: 'separator' },
        {
          id: 'file-close',
          label: 'Close ROM',
          type: 'action',
          disabled: !romLoaded,
          onSelect: cb.onCloseRom,
        },
      ],
    },

    // 2. EDIT & TABLE MATH MENU
    {
      id: 'edit',
      label: 'Edit & Table Math',
      type: 'submenu',
      children: [
        {
          id: 'edit-undo',
          label: 'Undo',
          type: 'action',
          shortcut: 'Ctrl+Z',
          disabled: !(state.canUndo ?? true),
          onSelect: cb.onUndo,
        },
        {
          id: 'edit-redo',
          label: 'Redo',
          type: 'action',
          shortcut: 'Ctrl+Y',
          disabled: !(state.canRedo ?? false),
          onSelect: cb.onRedo,
        },
        { id: 'edit-sep-1', label: '', type: 'separator' },
        {
          id: 'edit-history',
          label: 'Project History',
          type: 'submenu',
          children: [
            {
              id: 'edit-history-commit',
              label: 'Commit Project...',
              type: 'action',
              shortcut: 'Ctrl+Shift+C',
              onSelect: cb.onCommitProject,
            },
            {
              id: 'edit-history-rollback',
              label: 'Rollback Changes',
              type: 'action',
              shortcut: 'Ctrl+Shift+Z',
              onSelect: cb.onRollbackProject,
            },
            {
              id: 'edit-history-ledger',
              label: 'View Delta Ledger',
              type: 'action',
              onSelect: cb.onViewLedger,
            },
          ],
        },
        {
          id: 'edit-clipboard',
          label: 'Clipboard Special',
          type: 'submenu',
          children: [
            {
              id: 'edit-clip-copy',
              label: 'Copy Selection',
              type: 'action',
              shortcut: 'Ctrl+C',
              onSelect: cb.onCopy,
            },
            {
              id: 'edit-clip-paste',
              label: 'Paste',
              type: 'action',
              shortcut: 'Ctrl+V',
              onSelect: cb.onPaste,
            },
            {
              id: 'edit-clip-paste-add',
              label: 'Paste & Add',
              type: 'action',
              onSelect: () => cb.onPasteSpecial?.('add'),
            },
            {
              id: 'edit-clip-paste-mult',
              label: 'Paste & Multiply',
              type: 'action',
              onSelect: () => cb.onPasteSpecial?.('multiply'),
            },
            {
              id: 'edit-clip-paste-hex',
              label: 'Paste Raw Hex',
              type: 'action',
              onSelect: () => cb.onPasteSpecial?.('hex'),
            },
          ],
        },
        { id: 'edit-sep-2', label: '', type: 'separator' },
        {
          id: 'edit-selection',
          label: 'Selection Tools',
          type: 'submenu',
          children: [
            {
              id: 'edit-sel-all',
              label: 'Select All',
              type: 'action',
              shortcut: 'Ctrl+A',
              onSelect: cb.onSelectAll,
            },
            {
              id: 'edit-sel-invert',
              label: 'Invert Selection',
              type: 'action',
              onSelect: cb.onInvertSelection,
            },
            {
              id: 'edit-sel-clear',
              label: 'Clear Selection',
              type: 'action',
              shortcut: 'Esc',
              onSelect: cb.onClearSelection,
            },
          ],
        },
        {
          id: 'edit-math',
          label: 'Cell Math & Scaling',
          type: 'submenu',
          children: [
            {
              id: 'edit-math-add',
              label: 'Add Constant',
              type: 'action',
              shortcut: '+',
              onSelect: () => cb.onCellMath?.('add'),
            },
            {
              id: 'edit-math-sub',
              label: 'Subtract Constant',
              type: 'action',
              shortcut: '-',
              onSelect: () => cb.onCellMath?.('subtract'),
            },
            {
              id: 'edit-math-scale',
              label: 'Scale Percentage',
              type: 'action',
              shortcut: '%',
              onSelect: () => cb.onCellMath?.('scale'),
            },
            {
              id: 'edit-math-set',
              label: 'Set Fixed Value',
              type: 'action',
              shortcut: '=',
              onSelect: () => cb.onCellMath?.('set'),
            },
          ],
        },
        {
          id: 'edit-smooth',
          label: 'Smooth 3x3',
          type: 'action',
          shortcut: 'S',
          onSelect: cb.onSmooth3x3,
        },
        {
          id: 'edit-interpolate',
          label: 'Interpolate',
          type: 'submenu',
          children: [
            {
              id: 'edit-interp-bilinear',
              label: 'Bilinear 2D',
              type: 'action',
              shortcut: 'I',
              onSelect: () => cb.onInterpolate?.('bilinear'),
            },
            {
              id: 'edit-interp-h',
              label: 'Horizontal',
              type: 'action',
              shortcut: 'H',
              onSelect: () => cb.onInterpolate?.('horizontal'),
            },
            {
              id: 'edit-interp-v',
              label: 'Vertical',
              type: 'action',
              shortcut: 'V',
              onSelect: () => cb.onInterpolate?.('vertical'),
            },
          ],
        },
      ],
    },

    // 3. DEFINITION (XDF) MENU
    {
      id: 'definition',
      label: 'Definition (XDF)',
      type: 'submenu',
      children: [
        {
          id: 'def-load',
          label: 'Load XDF...',
          type: 'action',
          shortcut: 'Ctrl+D',
          onSelect: cb.onLoadXdf,
        },
        {
          id: 'def-save-kivi',
          label: 'Save .kivi.json',
          type: 'action',
          onSelect: cb.onSaveKiviJson,
        },
        {
          id: 'def-io',
          label: 'Import / Export Definitions',
          type: 'submenu',
          children: [
            {
              id: 'def-export-xdf',
              label: 'Export as XDF (.xdf)',
              type: 'action',
              onSelect: () => cb.onExportDef?.('xdf'),
            },
            {
              id: 'def-export-json',
              label: 'Export as .kivi.json',
              type: 'action',
              onSelect: () => cb.onExportDef?.('json'),
            },
            {
              id: 'def-export-ini',
              label: 'Export as ECU INI (.ini)',
              type: 'action',
              onSelect: () => cb.onExportDef?.('ini'),
            },
            {
              id: 'def-export-kp',
              label: 'Export as KP Map Pack (.kp)',
              type: 'action',
              onSelect: () => cb.onExportDef?.('kp'),
            },
            {
              id: 'def-import-json',
              label: 'Import .kivi.json',
              type: 'action',
              onSelect: cb.onImportDef,
            },
          ],
        },
        { id: 'def-sep-1', label: '', type: 'separator' },
        {
          id: 'def-new-table',
          label: 'Create New Table 2D/3D',
          type: 'action',
          onSelect: () => cb.onCreateParameter?.('table'),
        },
        {
          id: 'def-new-axis',
          label: 'Create 1D Axis',
          type: 'action',
          onSelect: () => cb.onCreateParameter?.('axis'),
        },
        {
          id: 'def-new-scalar',
          label: 'Create 1x1 Scalar',
          type: 'action',
          onSelect: () => cb.onCreateParameter?.('scalar'),
        },
        {
          id: 'def-new-flag',
          label: 'Create Bitmask Flag',
          type: 'action',
          onSelect: () => cb.onCreateParameter?.('flag'),
        },
        { id: 'def-sep-2', label: '', type: 'separator' },
        {
          id: 'def-header',
          label: 'XDF Header & Properties...',
          type: 'action',
          onSelect: cb.onXdfHeader,
        },
      ],
    },

    // 4. VIEW & WORKSPACE MENU
    {
      id: 'view',
      label: 'View & Workspace',
      type: 'submenu',
      children: [
        {
          id: 'view-layout-presets',
          label: 'Layout Presets',
          type: 'submenu',
          children: [
            {
              id: 'view-layout-2d',
              label: 'Standard 2D',
              type: 'radio',
              checked: currentLayout === 'table',
              onSelect: () => cb.onLayoutChange?.('table'),
            },
            {
              id: 'view-layout-split-3d',
              label: 'Split 2D+3D',
              type: 'radio',
              shortcut: 'Ctrl+\\',
              checked: currentLayout === 'split',
              onSelect: () => cb.onLayoutChange?.('split'),
            },
            {
              id: 'view-layout-split-wave',
              label: 'Split 2D+Waveform',
              type: 'radio',
              checked: currentLayout === 'split-waveform',
              onSelect: () => cb.onLayoutChange?.('split-waveform'),
            },
            {
              id: 'view-layout-wave',
              label: 'Full Waveform',
              type: 'radio',
              checked: currentLayout === 'waveform',
              onSelect: () => cb.onLayoutChange?.('waveform'),
            },
            {
              id: 'view-layout-3d',
              label: 'Full 3D',
              type: 'radio',
              checked: currentLayout === 'surface3d',
              onSelect: () => cb.onLayoutChange?.('surface3d'),
            },
          ],
        },
        { id: 'view-sep-1', label: '', type: 'separator' },
        {
          id: 'view-cell-display',
          label: 'Cell Display Mode',
          type: 'submenu',
          children: [
            {
              id: 'view-disp-eng',
              label: 'Engineering Values',
              type: 'radio',
              checked: displayMode === 'eng',
              onSelect: () => cb.onDisplayModeChange?.('eng'),
            },
            {
              id: 'view-disp-hex',
              label: 'Raw Hex',
              type: 'radio',
              checked: displayMode === 'hex',
              onSelect: () => cb.onDisplayModeChange?.('hex'),
            },
            {
              id: 'view-disp-dec',
              label: 'Raw Dec',
              type: 'radio',
              checked: displayMode === 'dec',
              onSelect: () => cb.onDisplayModeChange?.('dec'),
            },
            {
              id: 'view-disp-delta',
              label: 'Delta vs Base',
              type: 'radio',
              checked: displayMode === 'delta',
              onSelect: () => cb.onDisplayModeChange?.('delta'),
            },
            {
              id: 'view-disp-pct-delta',
              label: '% Delta vs Base',
              type: 'radio',
              checked: displayMode === 'pct-delta',
              onSelect: () => cb.onDisplayModeChange?.('pct-delta'),
            },
          ],
        },
        { id: 'view-sep-2', label: '', type: 'separator' },
        {
          id: 'view-palette',
          label: 'Heatmap Color Palette',
          type: 'submenu',
          children: [
            {
              id: 'view-pal-classic',
              label: 'Classic Tuner',
              type: 'radio',
              checked: palette === 'classic',
              onSelect: () => cb.onPaletteChange?.('classic'),
            },
            {
              id: 'view-pal-thermal',
              label: 'Thermal Ramp',
              type: 'radio',
              checked: palette === 'thermal',
              onSelect: () => cb.onPaletteChange?.('thermal'),
            },
            {
              id: 'view-pal-spectrum',
              label: 'Spectrum Gradient',
              type: 'radio',
              checked: palette === 'spectrum',
              onSelect: () => cb.onPaletteChange?.('spectrum'),
            },
            {
              id: 'view-pal-mono',
              label: 'Monochrome',
              type: 'radio',
              checked: palette === 'mono',
              onSelect: () => cb.onPaletteChange?.('mono'),
            },
          ],
        },
        { id: 'view-sep-3', label: '', type: 'separator' },
        {
          id: 'view-split-toggle',
          label: 'Toggle Split Workspace',
          type: 'action',
          onSelect: cb.onToggleSplitView,
        },
        {
          id: 'view-command-palette',
          label: 'Command Palette...',
          type: 'action',
          shortcut: 'Ctrl+K',
          onSelect: cb.onOpenCommandPalette,
        },
        {
          id: 'view-theme-toggle',
          label: state.theme === 'light' ? 'Switch to Carbon Dark Theme' : 'Switch to Lab Light Theme',
          type: 'action',
          onSelect: cb.onToggleTheme,
        },
        {
          id: 'view-reset',
          label: 'Reset Layout',
          type: 'action',
          onSelect: cb.onResetLayout,
        },
      ],
    },

    // 5. REVERSE ENGINEER MENU
    {
      id: 'reverse',
      label: 'Reverse Engineer',
      type: 'submenu',
      children: [
        {
          id: 'rev-extract-waveform',
          label: 'Extract Map from Waveform',
          type: 'action',
          onSelect: cb.onExtractWaveformMap,
        },
        {
          id: 'rev-scanner',
          label: 'Heuristic Map Scanner',
          type: 'submenu',
          children: [
            {
              id: 'rev-scan-run',
              label: 'Run Auto-Scan',
              type: 'action',
              shortcut: 'Ctrl+F',
              onSelect: cb.onRunScanner,
            },
            {
              id: 'rev-scan-sens',
              label: 'Sensitivity',
              type: 'submenu',
              children: [
                {
                  id: 'rev-scan-sens-strict',
                  label: 'Strict',
                  type: 'radio',
                  checked: sensitivity === 'strict',
                  onSelect: () => cb.onScannerSensitivityChange?.('strict'),
                },
                {
                  id: 'rev-scan-sens-balanced',
                  label: 'Balanced',
                  type: 'radio',
                  checked: sensitivity === 'balanced',
                  onSelect: () => cb.onScannerSensitivityChange?.('balanced'),
                },
                {
                  id: 'rev-scan-sens-aggressive',
                  label: 'Aggressive',
                  type: 'radio',
                  checked: sensitivity === 'aggressive',
                  onSelect: () => cb.onScannerSensitivityChange?.('aggressive'),
                },
              ],
            },
            {
              id: 'rev-scan-clear',
              label: 'Clear Scan Results',
              type: 'action',
              onSelect: cb.onClearScanner,
            },
          ],
        },
        {
          id: 'rev-diff',
          label: 'Binary Diff Engine',
          type: 'submenu',
          children: [
            {
              id: 'rev-diff-modal',
              label: 'Open Diff Modal',
              type: 'action',
              shortcut: 'Ctrl+B',
              onSelect: cb.onOpenDiffModal,
            },
            {
              id: 'rev-diff-base',
              label: 'Quick Diff vs Base ROM',
              type: 'action',
              onSelect: () => cb.onQuickDiff?.('base'),
            },
            {
              id: 'rev-diff-stage1',
              label: 'Quick Diff vs Stage-1',
              type: 'action',
              onSelect: () => cb.onQuickDiff?.('stage1'),
            },
            {
              id: 'rev-diff-extract',
              label: 'Extract All Diffs to Maps',
              type: 'action',
              onSelect: cb.onExtractDiffs,
            },
          ],
        },
        { id: 'rev-sep-1', label: '', type: 'separator' },
        {
          id: 'rev-checksum',
          label: 'Checksum & Integrity',
          type: 'submenu',
          children: [
            {
              id: 'rev-cs-verify',
              label: 'Verify Checksum',
              type: 'action',
              onSelect: cb.onVerifyChecksum,
            },
            {
              id: 'rev-cs-fix',
              label: 'Auto-Fix Checksum',
              type: 'action',
              onSelect: cb.onFixChecksum,
            },
          ],
        },
        {
          id: 'rev-workbench',
          label: 'Calibration Workbench',
          type: 'action',
          shortcut: 'Ctrl+Shift+W',
          onSelect: cb.onOpenWorkbench,
        },
      ],
    },

    // 6. ACQUISITION & ECU MENU
    {
      id: 'ecu',
      label: 'Acquisition & ECU',
      type: 'submenu',
      children: [
        {
          id: 'ecu-driver',
          label: 'Driver Profile',
          type: 'submenu',
          children: [
            {
              id: 'ecu-driver-keihin',
              label: 'Keihin K-Line',
              type: 'radio',
              checked: ecuDriver === 'keihin',
              onSelect: () => cb.onDriverChange?.('keihin'),
            },
            {
              id: 'ecu-driver-yamaha',
              label: 'Yamaha/Suzuki KWP',
              type: 'radio',
              checked: ecuDriver === 'yamaha',
              onSelect: () => cb.onDriverChange?.('yamaha'),
            },
            {
              id: 'ecu-driver-j2534',
              label: 'J2534 PassThru',
              type: 'radio',
              checked: ecuDriver === 'j2534',
              onSelect: () => cb.onDriverChange?.('j2534'),
            },
            {
              id: 'ecu-driver-speeduino',
              label: 'Speeduino/rusEFI',
              type: 'radio',
              checked: ecuDriver === 'speeduino',
              onSelect: () => cb.onDriverChange?.('speeduino'),
            },
          ],
        },
        { id: 'ecu-sep-1', label: '', type: 'separator' },
        {
          id: 'ecu-session',
          label: 'Connection & Session',
          type: 'submenu',
          children: [
            {
              id: 'ecu-connect-fast',
              label: 'Connect (Fast Init 10400 bps)',
              type: 'action',
              disabled: state.ecuConnected,
              onSelect: cb.onConnectEcu,
            },
            {
              id: 'ecu-disconnect',
              label: 'Disconnect',
              type: 'action',
              disabled: !state.ecuConnected,
              onSelect: cb.onDisconnectEcu,
            },
            {
              id: 'ecu-read-id',
              label: 'Read ECU ID',
              type: 'action',
              onSelect: cb.onReadEcuId,
            },
            {
              id: 'ecu-catalogue',
              label: 'Reference Catalogue…',
              type: 'action',
              onSelect: cb.onOpenCatalogue,
            },
          ],
        },
        {
          id: 'ecu-telemetry',
          label: 'Live Telemetry & Auto-Tune',
          type: 'submenu',
          children: [
            {
              id: 'ecu-telem-dash',
              label: 'Telemetry Dashboard (20Hz)',
              type: 'action',
              onSelect: cb.onToggleTelemetry,
            },
            {
              id: 'ecu-telem-crosshair',
              label: 'Live Crosshair Sync',
              type: 'checkbox',
              checked: state.crosshairSync ?? true,
              onSelect: cb.onToggleCrosshairSync,
            },
            {
              id: 'ecu-telem-afr',
              label: 'AFR Auto-Tune Assistant',
              type: 'action',
              onSelect: cb.onAfrAutoTune,
            },
          ],
        },
        {
          id: 'ecu-logging',
          label: 'Datalogging',
          type: 'submenu',
          children: [
            {
              id: 'ecu-log-start',
              label: 'Start Datalog',
              type: 'action',
              shortcut: 'F9',
              disabled: Boolean(state.isLogging || !state.ecuConnected),
              onSelect: cb.onStartLogging,
            },
            {
              id: 'ecu-log-stop',
              label: 'Stop Datalog',
              type: 'action',
              disabled: Boolean(!state.isLogging),
              onSelect: cb.onStopLogging,
            },
            {
              id: 'ecu-log-export',
              label: 'Export CSV Log...',
              type: 'action',
              disabled: Boolean(state.isLogging),
              onSelect: cb.onExportLogCsv,
            },
          ],
        },
        { id: 'ecu-sep-2', label: '', type: 'separator' },
        {
          id: 'ecu-flash',
          label: 'Flash ECU',
          type: 'submenu',
          children: [
            {
              id: 'ecu-flash-read',
              label: 'Read Flash',
              type: 'action',
              onSelect: cb.onReadFlash,
            },
            {
              id: 'ecu-flash-write',
              label: 'Write Flash',
              type: 'action',
              onSelect: cb.onWriteFlash,
            },
          ],
        },
      ],
    },

    // 7. HELP MENU
    {
      id: 'help',
      label: 'Help',
      type: 'submenu',
      children: [
        {
          id: 'help-how-to-use',
          label: 'How to Use',
          type: 'action',
          onSelect: cb.onOpenHowToUse,
        },
        {
          id: 'help-shortcuts',
          label: 'Keyboard Shortcuts',
          type: 'action',
          shortcut: 'F1',
          onSelect: cb.onOpenShortcuts,
        },
        {
          id: 'help-reset',
          label: 'Reset UI Settings',
          type: 'action',
          onSelect: cb.onResetSettings,
        },
        { id: 'help-sep-1', label: '', type: 'separator' },
        {
          id: 'help-about',
          label: 'About kiviTuner',
          type: 'action',
          onSelect: cb.onAbout,
        },
      ],
    },
  ];
}

/**
 * Searches a MenuNode tree recursively by ID.
 */
export function findMenuNode(nodes: MenuNode[], id: string): MenuNode | null {
  for (const node of nodes) {
    if (node.id === id) return node;
    if (node.children) {
      const found = findMenuNode(node.children, id);
      if (found) return found;
    }
  }
  return null;
}

/**
 * Extracts all shortcut mappings from the menu tree.
 */
export function collectShortcuts(nodes: MenuNode[]): Array<{ id: string; label: string; shortcut: string }> {
  const result: Array<{ id: string; label: string; shortcut: string }> = [];
  function walk(items: MenuNode[]) {
    for (const item of items) {
      if (item.shortcut) {
        result.push({ id: item.id, label: item.label, shortcut: item.shortcut });
      }
      if (item.children) {
        walk(item.children);
      }
    }
  }
  walk(nodes);
  return result;
}

export interface RectLike {
  left: number;
  top: number;
  right: number;
  bottom: number;
  width: number;
  height: number;
}

export interface PositionResult {
  x: number;
  y: number;
  flippedHorizontal: boolean;
  flippedVertical: boolean;
}

/**
 * Calculates flyout menu position with boundary clamping and horizontal/vertical flipping.
 * If rect.right + flyoutWidth > viewportWidth, flips leftwards.
 * If rect.top + flyoutHeight > viewportHeight, clamps upwards.
 */
export function calculateFlyoutPosition(
  triggerRect: RectLike,
  flyoutWidth: number,
  flyoutHeight: number,
  viewportWidth: number,
  viewportHeight: number,
  isRootDropdown: boolean = false
): PositionResult {
  let x: number;
  let y: number;
  let flippedHorizontal = false;
  let flippedVertical = false;

  if (isRootDropdown) {
    // Root dropdown opens directly beneath the trigger button
    x = triggerRect.left;
    y = triggerRect.bottom;

    if (x + flyoutWidth > viewportWidth - 8) {
      x = Math.max(8, viewportWidth - flyoutWidth - 8);
      flippedHorizontal = true;
    }
    if (y + flyoutHeight > viewportHeight - 8) {
      y = Math.max(8, triggerRect.top - flyoutHeight);
      flippedVertical = true;
    }
  } else {
    // Nested submenu flyout opens to the right
    x = triggerRect.right - 2;
    y = triggerRect.top - 4;

    if (triggerRect.right + flyoutWidth > viewportWidth - 8) {
      // Flip leftwards
      x = triggerRect.left - flyoutWidth + 2;
      flippedHorizontal = true;
      if (x < 8) {
        x = Math.max(8, viewportWidth - flyoutWidth - 8);
      }
    }

    if (y + flyoutHeight > viewportHeight - 8) {
      y = Math.max(8, viewportHeight - flyoutHeight - 8);
      flippedVertical = true;
    }
  }

  return { x, y, flippedHorizontal, flippedVertical };
}
