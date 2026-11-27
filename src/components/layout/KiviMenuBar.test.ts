import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  buildDefaultMenuTree,
  findMenuNode,
  collectShortcuts,
  calculateFlyoutPosition,
  type MenuBarState,
  type RectLike,
} from './menuModel.ts';

test('KiviMenuBar: all 7 primary top-level menus exist with correct IDs', () => {
  const tree = buildDefaultMenuTree();
  assert.equal(tree.length, 7, 'Menu bar should have exactly 7 top-level menus');

  const expectedRootMenus = [
    { id: 'file', label: 'File' },
    { id: 'edit', label: 'Edit & Table Math' },
    { id: 'definition', label: 'Definition (XDF)' },
    { id: 'view', label: 'View & Workspace' },
    { id: 'reverse', label: 'Reverse Engineer' },
    { id: 'ecu', label: 'Acquisition & ECU' },
    { id: 'help', label: 'Help' },
  ];

  for (let i = 0; i < expectedRootMenus.length; i++) {
    assert.equal(tree[i].id, expectedRootMenus[i].id);
    assert.equal(tree[i].label, expectedRootMenus[i].label);
    assert.equal(tree[i].type, 'submenu');
    assert.ok(tree[i].children && tree[i].children!.length > 0, `${tree[i].id} should have children`);
  }
});

test('KiviMenuBar: submenus exist and nesting works', () => {
  const tree = buildDefaultMenuTree();

  // File submenus
  const fileRecent = findMenuNode(tree, 'file-open-recent');
  assert.ok(fileRecent, 'Recent ROMs submenu exists');
  assert.equal(fileRecent?.type, 'submenu');
  assert.ok(findMenuNode(tree, 'file-recent-empty'), 'Empty placeholder shown when no recent ROMs exist');

  const populatedTree = buildDefaultMenuTree(
    {},
    {
      recentRoms: [
        { id: 'r1', label: 'tune_stage1.bin', size: 65536, sha256: 'abc' },
        { id: 'r2', label: 'stock_base.bin', size: 131072, sha256: 'def' },
      ],
    }
  );
  const populatedRecent = findMenuNode(populatedTree, 'file-open-recent');
  assert.ok(findMenuNode(populatedTree, 'file-recent-1'));
  assert.ok(findMenuNode(populatedTree, 'file-recent-clear'));
  assert.ok((populatedRecent?.children?.length ?? 0) >= 3);

  const fileRef = findMenuNode(tree, 'file-reference-slots');
  assert.ok(fileRef, 'Reference Slots submenu exists');
  assert.equal(findMenuNode(tree, 'file-demo-roms'), null);

  // Edit submenus
  const editHistory = findMenuNode(tree, 'edit-history');
  assert.ok(editHistory, 'Project History submenu exists');
  assert.ok(findMenuNode(tree, 'edit-history-commit'));
  assert.ok(findMenuNode(tree, 'edit-history-rollback'));

  const editInterpolate = findMenuNode(tree, 'edit-interpolate');
  assert.ok(editInterpolate, 'Interpolate submenu exists');
  assert.ok(findMenuNode(tree, 'edit-interp-bilinear'));
  assert.ok(findMenuNode(tree, 'edit-interp-h'));
  assert.ok(findMenuNode(tree, 'edit-interp-v'));

  // View submenus
  const viewLayout = findMenuNode(tree, 'view-layout-presets');
  assert.ok(viewLayout, 'Layout Presets submenu exists');
  const viewPalette = findMenuNode(tree, 'view-palette');
  assert.ok(viewPalette, 'Heatmap Color Palette submenu exists');

  // Reverse Engineer submenus
  const revDiff = findMenuNode(tree, 'rev-diff');
  assert.ok(revDiff, 'Binary Diff Engine submenu exists');
  assert.ok(findMenuNode(tree, 'rev-diff-modal'));
  assert.ok(findMenuNode(tree, 'rev-diff-base'));

  // Acquisition & ECU submenus
  const ecuDriver = findMenuNode(tree, 'ecu-driver');
  assert.ok(ecuDriver, 'Driver Profile submenu exists');
  assert.ok(findMenuNode(tree, 'ecu-driver-keihin'));
  assert.ok(findMenuNode(tree, 'ecu-driver-yamaha'));
});

test('KiviMenuBar: supports 3-level deep nested flyouts (Menu -> Submenu -> Sub-submenu)', () => {
  const tree = buildDefaultMenuTree();

  // Reverse Engineer (level 1) -> Heuristic Map Scanner (level 2) -> Sensitivity (level 3) -> Strict (level 4 leaf)
  const revMenu = findMenuNode(tree, 'reverse');
  assert.ok(revMenu, 'Level 1: Reverse Engineer exists');

  const scannerSubmenu = revMenu?.children?.find((c) => c.id === 'rev-scanner');
  assert.ok(scannerSubmenu, 'Level 2: Heuristic Map Scanner submenu exists');
  assert.equal(scannerSubmenu?.type, 'submenu');

  const sensSubmenu = scannerSubmenu?.children?.find((c) => c.id === 'rev-scan-sens');
  assert.ok(sensSubmenu, 'Level 3: Sensitivity flyout submenu exists');
  assert.equal(sensSubmenu?.type, 'submenu');

  const strictRadio = sensSubmenu?.children?.find((c) => c.id === 'rev-scan-sens-strict');
  assert.ok(strictRadio, 'Level 4: Strict radio node exists');
  assert.equal(strictRadio?.type, 'radio');
});

test('KiviMenuBar: major keyboard shortcuts are mapped correctly', () => {
  const tree = buildDefaultMenuTree();
  const shortcuts = collectShortcuts(tree);
  const shortcutMap = new Map<string, string>();
  for (const s of shortcuts) {
    shortcutMap.set(s.id, s.shortcut);
  }

  // Verify critical shortcuts
  assert.equal(shortcutMap.get('file-open'), 'Ctrl+O');
  assert.equal(shortcutMap.get('file-save'), 'Ctrl+S');
  assert.equal(shortcutMap.get('file-save-as'), 'Ctrl+Shift+S');
  assert.equal(shortcutMap.get('edit-undo'), 'Ctrl+Z');
  assert.equal(shortcutMap.get('edit-redo'), 'Ctrl+Y');
  assert.equal(shortcutMap.get('edit-history-commit'), 'Ctrl+Shift+C');
  assert.equal(shortcutMap.get('edit-history-rollback'), 'Ctrl+Shift+Z');
  assert.equal(shortcutMap.get('edit-clip-copy'), 'Ctrl+C');
  assert.equal(shortcutMap.get('edit-clip-paste'), 'Ctrl+V');
  assert.equal(shortcutMap.get('edit-sel-all'), 'Ctrl+A');
  assert.equal(shortcutMap.get('edit-smooth'), 'S');
  assert.equal(shortcutMap.get('edit-interp-bilinear'), 'I');
  assert.equal(shortcutMap.get('edit-interp-h'), 'H');
  assert.equal(shortcutMap.get('edit-interp-v'), 'V');
  assert.equal(shortcutMap.get('def-load'), 'Ctrl+D');
  assert.equal(shortcutMap.get('view-layout-split-3d'), 'Ctrl+\\');
  assert.equal(shortcutMap.get('rev-scan-run'), 'Ctrl+F');
  assert.equal(shortcutMap.get('rev-diff-modal'), 'Ctrl+B');
  assert.equal(shortcutMap.get('ecu-log-start'), 'F9');
  assert.equal(shortcutMap.get('help-shortcuts'), 'F1');
});

test('KiviMenuBar: reflects dynamic state (checked radios/checkboxes and disabled items)', () => {
  // Test with state: layout = 'split', displayMode = 'hex', palette = 'spectrum', sensitivity = 'strict', romLoaded = false
  const state: MenuBarState = {
    romLoaded: false,
    canUndo: false,
    canRedo: true,
    currentLayout: 'split',
    displayMode: 'hex',
    palette: 'spectrum',
    scannerSensitivity: 'strict',
    ecuDriver: 'yamaha',
    crosshairSync: true,
    ecuConnected: true,
    isLogging: true,
  };

  const tree = buildDefaultMenuTree({}, state);

  // File save should be disabled when ROM is not loaded
  const fileSave = findMenuNode(tree, 'file-save');
  assert.equal(fileSave?.disabled, true);

  // Undo disabled, redo enabled
  const editUndo = findMenuNode(tree, 'edit-undo');
  assert.equal(editUndo?.disabled, true);
  const editRedo = findMenuNode(tree, 'edit-redo');
  assert.equal(editRedo?.disabled, false);

  // View layout radios
  const layoutSplit = findMenuNode(tree, 'view-layout-split-3d');
  assert.equal(layoutSplit?.checked, true);
  const layout2d = findMenuNode(tree, 'view-layout-2d');
  assert.equal(layout2d?.checked, false);

  // Cell display mode
  const dispHex = findMenuNode(tree, 'view-disp-hex');
  assert.equal(dispHex?.checked, true);
  const dispEng = findMenuNode(tree, 'view-disp-eng');
  assert.equal(dispEng?.checked, false);

  // Palette
  const palSpectrum = findMenuNode(tree, 'view-pal-spectrum');
  assert.equal(palSpectrum?.checked, true);

  // Scanner sensitivity
  const sensStrict = findMenuNode(tree, 'rev-scan-sens-strict');
  assert.equal(sensStrict?.checked, true);

  // Crosshair sync checkbox
  const crosshair = findMenuNode(tree, 'ecu-telem-crosshair');
  assert.equal(crosshair?.checked, true);

  // ECU session buttons
  const connectBtn = findMenuNode(tree, 'ecu-connect-fast');
  assert.equal(connectBtn?.disabled, true, 'Connect disabled when already connected');
  const disconnectBtn = findMenuNode(tree, 'ecu-disconnect');
  assert.equal(disconnectBtn?.disabled, false, 'Disconnect enabled when connected');
});

test('KiviMenuBar: triggers onSelect callbacks when menu items are clicked', () => {
  let openRomTriggered = false;
  let refSlotTriggered = 0;
  let layoutChangedTo: string | null = null;

  const tree = buildDefaultMenuTree({
    onOpenRom: () => {
      openRomTriggered = true;
    },
    onReferenceSlot: (slot) => {
      refSlotTriggered = slot;
    },
    onLayoutChange: (layout) => {
      layoutChangedTo = layout;
    },
  });

  const openRomNode = findMenuNode(tree, 'file-open');
  openRomNode?.onSelect?.();
  assert.equal(openRomTriggered, true);

  const refSlotNode = findMenuNode(tree, 'file-ref-1');
  refSlotNode?.onSelect?.();
  assert.equal(refSlotTriggered, 1);

  assert.equal(findMenuNode(tree, 'file-demo-stock'), null);
  assert.equal(findMenuNode(tree, 'ecu-driver-demo'), null);
  assert.equal(findMenuNode(tree, 'ecu-flash-sim'), null);

  const layout3dNode = findMenuNode(tree, 'view-layout-3d');
  layout3dNode?.onSelect?.();
  assert.equal(layoutChangedTo, 'surface3d');
});

test('calculateFlyoutPosition: boundary clamping and flipping logic', () => {
  const viewportWidth = 1000;
  const viewportHeight = 800;
  const flyoutWidth = 220;
  const flyoutHeight = 300;

  // Case 1: normal position well within bounds
  const triggerNormal: RectLike = {
    left: 200,
    top: 100,
    right: 350,
    bottom: 125,
    width: 150,
    height: 25,
  };
  const posNormal = calculateFlyoutPosition(
    triggerNormal,
    flyoutWidth,
    flyoutHeight,
    viewportWidth,
    viewportHeight,
    false
  );
  assert.equal(posNormal.flippedHorizontal, false);
  assert.equal(posNormal.flippedVertical, false);
  assert.equal(posNormal.x, triggerNormal.right - 2);

  // Case 2: near right edge (rect.right + 220 > viewportWidth) -> flips leftward!
  const triggerRightEdge: RectLike = {
    left: 820,
    top: 150,
    right: 950,
    bottom: 175,
    width: 130,
    height: 25,
  };
  const posRightEdge = calculateFlyoutPosition(
    triggerRightEdge,
    flyoutWidth,
    flyoutHeight,
    viewportWidth,
    viewportHeight,
    false
  );
  assert.equal(posRightEdge.flippedHorizontal, true, 'Should flip leftwards');
  assert.equal(posRightEdge.x, triggerRightEdge.left - flyoutWidth + 2);

  // Case 3: near bottom edge -> clamped vertically
  const triggerBottomEdge: RectLike = {
    left: 200,
    top: 650,
    right: 350,
    bottom: 675,
    width: 150,
    height: 25,
  };
  const posBottomEdge = calculateFlyoutPosition(
    triggerBottomEdge,
    flyoutWidth,
    flyoutHeight,
    viewportWidth,
    viewportHeight,
    false
  );
  assert.equal(posBottomEdge.flippedVertical, true, 'Should clamp upwards');
  assert.ok(posBottomEdge.y + flyoutHeight <= viewportHeight);
});

test('KiviMenuBar: datalogging menu items reflect isLogging and ecuConnected states', () => {
  // Case 1: ECU disconnected -> Start disabled, Stop disabled, Export enabled
  const disconnectedTree = buildDefaultMenuTree(
    {},
    { ecuConnected: false, isLogging: false }
  );
  const logStartDisc = findMenuNode(disconnectedTree, 'ecu-log-start');
  const logStopDisc = findMenuNode(disconnectedTree, 'ecu-log-stop');
  const logExportDisc = findMenuNode(disconnectedTree, 'ecu-log-export');
  assert.equal(logStartDisc?.disabled, true, 'Start disabled when disconnected');
  assert.equal(logStopDisc?.disabled, true, 'Stop disabled when disconnected');
  assert.equal(logExportDisc?.disabled, false, 'Export enabled when disconnected');

  // Case 2: ECU connected, not logging -> Start enabled, Stop disabled, Export enabled
  const idleConnectedTree = buildDefaultMenuTree(
    {},
    { ecuConnected: true, isLogging: false }
  );
  const logStartIdle = findMenuNode(idleConnectedTree, 'ecu-log-start');
  const logStopIdle = findMenuNode(idleConnectedTree, 'ecu-log-stop');
  const logExportIdle = findMenuNode(idleConnectedTree, 'ecu-log-export');
  assert.equal(logStartIdle?.disabled, false, 'Start enabled when connected and idle');
  assert.equal(logStopIdle?.disabled, true, 'Stop disabled when not logging');
  assert.equal(logExportIdle?.disabled, false, 'Export enabled when not logging');

  // Case 3: Actively logging -> Start disabled, Stop enabled, Export disabled (invalid transition)
  const loggingTree = buildDefaultMenuTree(
    {},
    { ecuConnected: true, isLogging: true }
  );
  const logStartActive = findMenuNode(loggingTree, 'ecu-log-start');
  const logStopActive = findMenuNode(loggingTree, 'ecu-log-stop');
  const logExportActive = findMenuNode(loggingTree, 'ecu-log-export');
  assert.equal(logStartActive?.disabled, true, 'Start disabled while already logging');
  assert.equal(logStopActive?.disabled, false, 'Stop enabled while logging');
  assert.equal(logExportActive?.disabled, true, 'Export disabled while actively logging');
});

test('KiviMenuBar: datalogging menu item callbacks trigger correctly', () => {
  let startCalled = false;
  let stopCalled = false;
  let exportCalled = false;

  const tree = buildDefaultMenuTree(
    {
      onStartLogging: () => {
        startCalled = true;
      },
      onStopLogging: () => {
        stopCalled = true;
      },
      onExportLogCsv: () => {
        exportCalled = true;
      },
    },
    { ecuConnected: true, isLogging: false }
  );

  const startNode = findMenuNode(tree, 'ecu-log-start');
  startNode?.onSelect?.();
  assert.equal(startCalled, true, 'onStartLogging triggered');

  const stopNode = findMenuNode(tree, 'ecu-log-stop');
  stopNode?.onSelect?.();
  assert.equal(stopCalled, true, 'onStopLogging triggered');

  const exportNode = findMenuNode(tree, 'ecu-log-export');
  exportNode?.onSelect?.();
  assert.equal(exportCalled, true, 'onExportLogCsv triggered');
});

test('KiviMenuBar: Help menu exposes How to Use first item and invokes callback', () => {
  let openedGuide = false;
  const tree = buildDefaultMenuTree({
    onOpenHowToUse: () => {
      openedGuide = true;
    },
  });

  const helpMenu = findMenuNode(tree, 'help');
  assert.ok(helpMenu?.children && helpMenu.children.length > 0);
  assert.equal(helpMenu.children[0].id, 'help-how-to-use');
  assert.equal(helpMenu.children[0].label, 'How to Use');

  const node = findMenuNode(tree, 'help-how-to-use');
  node?.onSelect?.();
  assert.equal(openedGuide, true, 'onOpenHowToUse callback invoked');
});
