import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { NATIVE_MENU_ACTION_IDS, dispatchNativeMenuAction } from './nativeMenuBridge.ts';
import type { MenuBarCallbacks } from '../components/layout/menuModel.ts';

test('NATIVE_MENU_ACTION_IDS stays in exact 1-to-1 parity with Rust native_menu::NATIVE_MENU_IDS', () => {
  const rs = readFileSync('src-tauri/src/native_menu.rs', 'utf8');
  const blockMatch = rs.match(/pub const NATIVE_MENU_IDS:\s*&\[&str\]\s*=\s*&\[([\s\S]*?)\];/);
  assert.ok(blockMatch, 'expected NATIVE_MENU_IDS constant in native_menu.rs');

  const rustIds = Array.from(blockMatch[1].matchAll(/"([^"]+)"/g), (m) => m[1]);
  assert.equal(new Set(rustIds).size, rustIds.length, 'Rust NATIVE_MENU_IDS contains duplicates');
  assert.deepEqual([...NATIVE_MENU_ACTION_IDS].sort(), [...rustIds].sort());
});

test('dispatchNativeMenuAction invokes every known handler and ignores unknown IDs safely', () => {
  const calls: string[] = [];
  const callbacks: MenuBarCallbacks = {
    onAbout: () => calls.push('app.about'),
    onCheckForUpdates: () => calls.push('app.check-updates'),
    onOpenHowToUse: () => calls.push('help.guide'),
    onOpenShortcuts: () => calls.push('help.shortcuts'),
    onResetSettings: () => calls.push('help.reset-settings'),
    onOpenRom: () => calls.push('file.open-rom'),
    onSaveRom: () => calls.push('file.save'),
    onSaveAsRom: () => calls.push('file.save-as'),
    onExportCsv: () => calls.push('file.export-csv'),
    onExportReport: () => calls.push('file.export-report'),
    onCloseRom: () => calls.push('file.close-rom'),
    onUndo: () => calls.push('edit.undo'),
    onRedo: () => calls.push('edit.redo'),
    onCellMath: (op) => {
      assert.equal(op, 'add');
      calls.push('edit.cell-math');
    },
    onSmooth3x3: () => calls.push('edit.smooth-3x3'),
    onInterpolate: (mode) => {
      assert.equal(mode, 'bilinear');
      calls.push('edit.interp-2d');
    },
    onRollbackProject: () => calls.push('edit.revert'),
    onLoadXdf: () => calls.push('def.load-xdf'),
    onImportDef: () => calls.push('def.import-json'),
    onSaveKiviJson: () => calls.push('def.save-json'),
    onCreateParameter: (preset) => calls.push(`def.new-${preset}`),
    onLayoutChange: (preset) => calls.push(`view.${preset}`),
    onToggleSplitView: () => calls.push('view.toggle-split'),
    onOpenCommandPalette: () => calls.push('view.command-palette'),
    onToggleTheme: () => calls.push('view.toggle-theme'),
    onResetLayout: () => calls.push('view.reset-layout'),
    onRunScanner: () => calls.push('rev.scanner'),
    onOpenDiffModal: () => calls.push('rev.diff'),
    onVerifyChecksum: () => calls.push('rev.verify-checksum'),
    onFixChecksum: () => calls.push('rev.fix-checksum'),
    onOpenWorkbench: () => calls.push('rev.workbench'),
    onConnectEcu: () => calls.push('ecu.connect'),
    onDisconnectEcu: () => calls.push('ecu.disconnect'),
    onReadFlash: () => calls.push('ecu.flash'),
    onToggleTelemetry: () => calls.push('ecu.telemetry'),
    onAfrAutoTune: () => calls.push('ecu.autotune'),
    onOpenCatalogue: () => calls.push('ecu.catalogue'),
  };

  for (const id of NATIVE_MENU_ACTION_IDS) {
    assert.equal(dispatchNativeMenuAction(id, callbacks), true, `unhandled ID ${id}`);
  }
  assert.equal(calls.length, NATIVE_MENU_ACTION_IDS.length);
  assert.equal(dispatchNativeMenuAction('unknown.nonexistent', callbacks), false);
});
