import test from 'node:test';
import assert from 'node:assert/strict';
import { resolveActiveRailItem, type ActiveRailStateInput } from './activityRailModel.ts';

function baseState(overrides: Partial<ActiveRailStateInput> = {}): ActiveRailStateInput {
  return {
    focusedItem: null,
    showMapTree: true,
    isScannerOpen: false,
    showInspector: false,
    activeTabKind: 'map-table',
    isWorkbenchOpen: false,
    isFlashPanelOpen: false,
    isDiffModalOpen: false,
    isCommandPaletteOpen: false,
    ...overrides,
  };
}

test('resolveActiveRailItem keeps a single active item across maps -> scanner -> hex -> waveform -> workbench -> table -> inspector', () => {
  assert.equal(resolveActiveRailItem(baseState({ focusedItem: 'maps' })), 'maps');

  // Moving from Map Explorer to Discovery Scanner leaves Map Explorer docked without dual highlight
  assert.equal(
    resolveActiveRailItem(baseState({ focusedItem: 'scanner', isScannerOpen: true })),
    'scanner'
  );

  // Opening Hex Viewer moves highlight exclusively to hex even if panels remain open
  assert.equal(
    resolveActiveRailItem(
      baseState({ focusedItem: 'hex', isScannerOpen: true, activeTabKind: 'hex' })
    ),
    'hex'
  );

  // Switching to Waveform moves highlight cleanly to waveform
  assert.equal(
    resolveActiveRailItem(
      baseState({ focusedItem: 'waveform', isScannerOpen: true, activeTabKind: 'waveform' })
    ),
    'waveform'
  );

  // Opening Workbench takes primary focus, then closing returns to map-table with no stuck rail bg
  assert.equal(
    resolveActiveRailItem(
      baseState({ focusedItem: 'workbench', isWorkbenchOpen: true, activeTabKind: 'map-table' })
    ),
    'workbench'
  );
  assert.equal(
    resolveActiveRailItem(baseState({ focusedItem: 'workbench', activeTabKind: 'map-table', showMapTree: false })),
    null
  );

  // Opening Inspector highlights inspector exclusively
  assert.equal(
    resolveActiveRailItem(
      baseState({ focusedItem: 'inspector', showInspector: true, activeTabKind: 'map-table' })
    ),
    'inspector'
  );
});
