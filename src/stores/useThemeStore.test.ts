import test from 'node:test';
import assert from 'node:assert/strict';
import { useThemeStore } from './useThemeStore.ts';

test('useThemeStore toggles cleanly between Carbon Dark and Lab Light themes', () => {
  const s = useThemeStore;
  s.getState().setTheme('dark');
  assert.strictEqual(s.getState().theme, 'dark');

  const next = s.getState().toggleTheme();
  assert.strictEqual(next, 'light');
  assert.strictEqual(s.getState().theme, 'light');

  s.getState().toggleTheme();
  assert.strictEqual(s.getState().theme, 'dark');
});
