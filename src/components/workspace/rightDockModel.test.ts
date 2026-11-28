import test from 'node:test';
import assert from 'node:assert/strict';
import {
  resolveRightDockAnimState,
  toggleRightDockPanel,
} from './rightDockModel.ts';

test('toggleRightDockPanel opens, swaps without overlap, and closes cleanly', () => {
  assert.equal(toggleRightDockPanel(null, 'inspector'), 'inspector');
  assert.equal(toggleRightDockPanel('inspector', 'scanner'), 'scanner');
  assert.equal(toggleRightDockPanel('scanner', 'inspector'), 'inspector');
  assert.equal(toggleRightDockPanel('scanner', 'scanner'), null);
  assert.equal(toggleRightDockPanel('inspector', 'inspector'), null);
});

test('resolveRightDockAnimState distinguishes initial slide-in, direct swap, and closing slide-out', () => {
  assert.equal(resolveRightDockAnimState('open', false), 'open');
  assert.equal(resolveRightDockAnimState('open', true), 'swap');
  assert.equal(resolveRightDockAnimState('closing', false), 'closing');
  assert.equal(resolveRightDockAnimState('closing', true), 'closing');
});
