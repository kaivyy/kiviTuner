import { test } from 'node:test';
import assert from 'node:assert/strict';
import { SHORTCUTS_REGISTRY } from './shortcutsData.ts';

test('SHORTCUTS_REGISTRY: F9 is registered for datalog recording in Acquisition category', () => {
  const f9Shortcuts = SHORTCUTS_REGISTRY.filter((s) =>
    s.keys.length === 1 && s.keys[0].toUpperCase() === 'F9'
  );

  assert.strictEqual(
    f9Shortcuts.length,
    1,
    'F9 must be registered exactly once in SHORTCUTS_REGISTRY'
  );

  const f9 = f9Shortcuts[0];
  assert.strictEqual(f9.id, 'acq-toggle-record');
  assert.strictEqual(f9.category, 'Acquisition');
  assert.strictEqual(f9.action, 'Toggle Datalog Recording');
  assert.ok(f9.description.length > 0);
});

test('SHORTCUTS_REGISTRY: zero key collisions across all registered shortcuts', () => {
  const keyMap = new Map<string, string>();

  for (const item of SHORTCUTS_REGISTRY) {
    const keyCombo = item.keys.map((k) => k.toLowerCase()).sort().join('+');
    // Multi-action single keys (e.g. contextual presets like S or F) can differ by category,
    // but function keys like F9 or critical global keys must have zero global collision.
    if (item.keys.length === 1 && item.keys[0].startsWith('F')) {
      assert.ok(
        !keyMap.has(keyCombo),
        `Function key collision detected: ${item.keys[0]} used by ${keyMap.get(keyCombo)} and ${item.id}`
      );
      keyMap.set(keyCombo, item.id);
    }
  }
});
