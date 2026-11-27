import { test } from 'node:test';
import assert from 'node:assert/strict';
import { SHORTCUTS_REGISTRY } from './shortcutsData.ts';

test('SHORTCUTS_REGISTRY: all items have required fields and unique IDs', () => {
  const seenIds = new Set<string>();

  for (const item of SHORTCUTS_REGISTRY) {
    assert.ok(item.id, 'Item must have an id');
    assert.ok(!seenIds.has(item.id), `Duplicate shortcut id: ${item.id}`);
    seenIds.add(item.id);

    assert.ok(item.category, `Item ${item.id} must have a category`);
    assert.ok(item.action, `Item ${item.id} must have an action title`);
    assert.ok(item.description, `Item ${item.id} must have a description`);
    assert.ok(item.keys && item.keys.length > 0, `Item ${item.id} must specify keys`);
  }
});

test('SHORTCUTS_REGISTRY: contains all four required categories', () => {
  const categories = new Set(SHORTCUTS_REGISTRY.map((s) => s.category));

  assert.ok(categories.has('Navigation & Workspace'));
  assert.ok(categories.has('2D Heatmap Matrix'));
  assert.ok(categories.has('3D Surface View'));
  assert.ok(categories.has('2D Waveform'));
});

test('SHORTCUTS_REGISTRY: includes all essential ECU tuner shortcuts', () => {
  const keyCombos = SHORTCUTS_REGISTRY.map((s) => s.keys.join('+'));

  // Global & Navigation
  assert.ok(keyCombos.includes('Ctrl+O'), 'Ctrl+O present');
  assert.ok(keyCombos.includes('Ctrl+S'), 'Ctrl+S present');
  assert.ok(keyCombos.includes('Ctrl+D'), 'Ctrl+D present');
  assert.ok(keyCombos.includes('Ctrl+B'), 'Ctrl+B present');
  assert.ok(keyCombos.includes('Ctrl+F'), 'Ctrl+F present');
  assert.ok(keyCombos.includes('Ctrl+\\'), 'Ctrl+\\ present');
  assert.ok(keyCombos.includes('F1'), 'F1 present');
  assert.ok(keyCombos.includes('Esc'), 'Esc present');

  // 2D Table Matrix Editing
  assert.ok(keyCombos.includes('+'), '+ Increment present');
  assert.ok(keyCombos.includes('-'), '- Decrement present');
  assert.ok(keyCombos.includes('%'), '% Scale present');
  assert.ok(keyCombos.includes('='), '= Set value present');
  assert.ok(keyCombos.includes('S'), 'S Smooth present');
  assert.ok(keyCombos.includes('I'), 'I Bilinear interpolation present');
  assert.ok(keyCombos.includes('H'), 'H Horizontal interpolation present');
  assert.ok(keyCombos.includes('V'), 'V Vertical interpolation present');
  assert.ok(keyCombos.includes('Ctrl+A'), 'Ctrl+A Select All present');
  assert.ok(keyCombos.includes('Ctrl+C'), 'Ctrl+C Copy present');
  assert.ok(keyCombos.includes('Ctrl+V'), 'Ctrl+V Paste present');

  // 3D Surface View Controls
  assert.ok(keyCombos.includes('Left Click+Drag'), 'Rotate camera present');
  assert.ok(keyCombos.includes('Right Click+Drag'), 'Pan camera present');
  assert.ok(keyCombos.includes('Scroll Wheel'), 'Zoom camera present');
  assert.ok(keyCombos.includes('R'), 'R Reset camera preset present');
  assert.ok(keyCombos.includes('T'), 'T Top-down preset present');
  assert.ok(keyCombos.includes('F'), 'F Front profile preset present');

  // 2D Waveform Controls
  assert.ok(keyCombos.includes('['), '[ Decrease pitch present');
  assert.ok(keyCombos.includes(']'), '] Increase pitch present');
  assert.ok(keyCombos.includes('Shift+Drag'), 'Shift+Drag range select present');
});
