import type { MenuBarCallbacks } from '../components/layout/menuModel.ts';
import { isDesktopApp } from './bridge/romBridge.ts';

export const NATIVE_MENU_EVENT = 'kivituner:native-menu';

export const NATIVE_MENU_ACTION_IDS = [
  'app.about',
  'app.check-updates',
  'help.guide',
  'help.shortcuts',
  'help.reset-settings',
  'file.open-rom',
  'file.save',
  'file.save-as',
  'file.export-csv',
  'file.export-report',
  'file.close-rom',
  'edit.undo',
  'edit.redo',
  'edit.cell-math',
  'edit.smooth-3x3',
  'edit.interp-2d',
  'edit.revert',
  'def.load-xdf',
  'def.import-json',
  'def.save-json',
  'def.new-table',
  'def.new-scalar',
  'view.table',
  'view.split-3d',
  'view.split-wave',
  'view.waveform',
  'view.full-3d',
  'view.toggle-split',
  'view.command-palette',
  'view.toggle-theme',
  'view.reset-layout',
  'rev.scanner',
  'rev.diff',
  'rev.verify-checksum',
  'rev.fix-checksum',
  'rev.workbench',
  'ecu.connect',
  'ecu.disconnect',
  'ecu.flash',
  'ecu.telemetry',
  'ecu.autotune',
  'ecu.catalogue',
] as const;

/** Maps a Tauri native menu event ID onto the workspace's active callback handler. */
export function dispatchNativeMenuAction(id: string, callbacks: MenuBarCallbacks): boolean {
  switch (id) {
    case 'app.about':
      callbacks.onAbout?.();
      return true;
    case 'app.check-updates':
      callbacks.onCheckForUpdates?.();
      return true;
    case 'help.guide':
      callbacks.onOpenHowToUse?.();
      return true;
    case 'help.shortcuts':
      callbacks.onOpenShortcuts?.();
      return true;
    case 'help.reset-settings':
      callbacks.onResetSettings?.();
      return true;
    case 'file.open-rom':
      callbacks.onOpenRom?.();
      return true;
    case 'file.save':
      callbacks.onSaveRom?.();
      return true;
    case 'file.save-as':
      callbacks.onSaveAsRom?.();
      return true;
    case 'file.export-csv':
      callbacks.onExportCsv?.();
      return true;
    case 'file.export-report':
      callbacks.onExportReport?.();
      return true;
    case 'file.close-rom':
      callbacks.onCloseRom?.();
      return true;
    case 'edit.undo':
      callbacks.onUndo?.();
      return true;
    case 'edit.redo':
      callbacks.onRedo?.();
      return true;
    case 'edit.cell-math':
      callbacks.onCellMath?.('add');
      return true;
    case 'edit.smooth-3x3':
      callbacks.onSmooth3x3?.();
      return true;
    case 'edit.interp-2d':
      callbacks.onInterpolate?.('bilinear');
      return true;
    case 'edit.revert':
      callbacks.onRollbackProject?.();
      return true;
    case 'def.load-xdf':
      callbacks.onLoadXdf?.();
      return true;
    case 'def.import-json':
      callbacks.onImportDef?.();
      return true;
    case 'def.save-json':
      callbacks.onSaveKiviJson?.();
      return true;
    case 'def.new-table':
      callbacks.onCreateParameter?.('table');
      return true;
    case 'def.new-scalar':
      callbacks.onCreateParameter?.('scalar');
      return true;
    case 'view.table':
      callbacks.onLayoutChange?.('table');
      return true;
    case 'view.split-3d':
      callbacks.onLayoutChange?.('split');
      return true;
    case 'view.split-wave':
      callbacks.onLayoutChange?.('split_waveform');
      return true;
    case 'view.waveform':
      callbacks.onLayoutChange?.('waveform');
      return true;
    case 'view.full-3d':
      callbacks.onLayoutChange?.('surface3d');
      return true;
    case 'view.toggle-split':
      callbacks.onToggleSplitView?.();
      return true;
    case 'view.command-palette':
      callbacks.onOpenCommandPalette?.();
      return true;
    case 'view.toggle-theme':
      callbacks.onToggleTheme?.();
      return true;
    case 'view.reset-layout':
      callbacks.onResetLayout?.();
      return true;
    case 'rev.scanner':
      callbacks.onRunScanner?.();
      return true;
    case 'rev.diff':
      callbacks.onOpenDiffModal?.();
      return true;
    case 'rev.verify-checksum':
      callbacks.onVerifyChecksum?.();
      return true;
    case 'rev.fix-checksum':
      callbacks.onFixChecksum?.();
      return true;
    case 'rev.workbench':
      callbacks.onOpenWorkbench?.();
      return true;
    case 'ecu.connect':
      callbacks.onConnectEcu?.();
      return true;
    case 'ecu.disconnect':
      callbacks.onDisconnectEcu?.();
      return true;
    case 'ecu.flash':
      callbacks.onReadFlash?.();
      return true;
    case 'ecu.telemetry':
      callbacks.onToggleTelemetry?.();
      return true;
    case 'ecu.autotune':
      callbacks.onAfrAutoTune?.();
      return true;
    case 'ecu.catalogue':
      callbacks.onOpenCatalogue?.();
      return true;
    default:
      return false;
  }
}

/**
 * Subscribes once to native OS menu actions emitted by the Tauri backend.
 * Guards async unlisten registration so StrictMode remounts cannot leak listeners.
 */
export function subscribeNativeMenuEvents(getCallbacks: () => MenuBarCallbacks): () => void {
  if (!isDesktopApp()) {
    return () => {};
  }

  let disposed = false;
  let unlistenFn: (() => void) | null = null;

  void import('@tauri-apps/api/event').then(async ({ listen }) => {
    const unlisten = await listen<string>(NATIVE_MENU_EVENT, (ev) => {
      if (disposed || typeof ev.payload !== 'string') return;
      dispatchNativeMenuAction(ev.payload, getCallbacks());
    });
    if (disposed) {
      unlisten();
      return;
    }
    unlistenFn = unlisten;
  });

  return () => {
    disposed = true;
    if (unlistenFn) {
      unlistenFn();
      unlistenFn = null;
    }
  };
}
