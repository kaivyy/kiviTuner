import test from 'node:test';
import assert from 'node:assert/strict';
import { useWorkspaceTabsStore } from './useWorkspaceTabsStore.ts';

test('useWorkspaceTabsStore manages tabs, split pane, pinning, and cap eviction', () => {
  const s = useWorkspaceTabsStore;
  s.getState().reset();

  s.getState().syncMapTab(
    {
      id: 'fuel_1',
      name: 'Main Fuel',
      category: 'Fuel',
      address: 0x1000,
      rows: 4,
      cols: 4,
      dtype: 'u16',
      endian: 'little',
      scaling: { equation: 'X', unit: 'ms', decimals: 2 },
    },
    'map-split'
  );
  assert.strictEqual(s.getState().tabs.length, 1);
  assert.strictEqual(s.getState().activeTabId, 'map:fuel_1');

  s.getState().openTab({ id: 'tool:hex', kind: 'hex', title: 'Hex Viewer' });
  assert.strictEqual(s.getState().activeTabId, 'tool:hex');

  s.getState().toggleSplitView();
  assert.strictEqual(s.getState().splitTabId, 'map:fuel_1');

  s.getState().setSplitRatio(0.95);
  assert.strictEqual(s.getState().splitRatio, 0.75);

  s.getState().togglePinTab('map:fuel_1');
  s.getState().closeTab('map:fuel_1');
  assert.ok(s.getState().tabs.some((t) => t.id === 'map:fuel_1'), 'pinned tab must not close');

  s.getState().togglePinTab('map:fuel_1');
  s.getState().closeTab('tool:hex');
  assert.strictEqual(s.getState().activeTabId, 'map:fuel_1');
  assert.strictEqual(s.getState().splitTabId, null);

  for (let i = 0; i < 30; i++) {
    s.getState().openTab({ id: `extra-${i}`, kind: 'waveform', title: `Wave ${i}` });
  }
  assert.ok(s.getState().tabs.length <= 24, 'tab strip enforces 24-tab ceiling');
  s.getState().reset();
});

test('syncMapTab respects forceKind so switching from split_3d back to 2D table updates existing tab kind', () => {
  const s = useWorkspaceTabsStore;
  s.getState().reset();

  const fuelMap = {
    id: 'fuel_main',
    name: 'Main Fuel',
    category: 'Fuel',
    address: 0x2000,
    rows: 16,
    cols: 16,
    dtype: 'u16' as const,
    endian: 'little' as const,
    scaling: { equation: 'X', unit: 'ms', decimals: 2 },
  };

  s.getState().syncMapTab(fuelMap, 'map-split');
  assert.strictEqual(s.getState().tabs.find((t) => t.id === 'map:fuel_main')?.kind, 'map-split');

  // Default behavior preserves existing kind when simply re-selecting the map
  s.getState().syncMapTab(fuelMap, 'map-table');
  assert.strictEqual(s.getState().tabs.find((t) => t.id === 'map:fuel_main')?.kind, 'map-split');

  // Explicit layout switch uses forceKind=true and returns to 2D table cleanly
  s.getState().syncMapTab(fuelMap, 'map-table', true);
  assert.strictEqual(s.getState().tabs.find((t) => t.id === 'map:fuel_main')?.kind, 'map-table');

  s.getState().syncMapTab(fuelMap, 'map-3d', true);
  assert.strictEqual(s.getState().tabs.find((t) => t.id === 'map:fuel_main')?.kind, 'map-3d');

  s.getState().syncMapTab(fuelMap, 'map-table', true);
  assert.strictEqual(s.getState().tabs.find((t) => t.id === 'map:fuel_main')?.kind, 'map-table');
  s.getState().reset();
});
