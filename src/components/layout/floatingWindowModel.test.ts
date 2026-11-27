import test from 'node:test';
import assert from 'node:assert/strict';
import {
  VIEWPORT_TOP_INSET,
  applyWindowResize,
  clampWindowPosition,
  computeInitialRect,
  focusFloatingWindow,
  getTopFloatingWindowId,
  getWindowZIndex,
  registerFloatingWindow,
  resetWindowStackForTests,
  unregisterFloatingWindow,
} from './floatingWindowModel.ts';

test('computeInitialRect keeps floating windows inside desktop and mobile viewports', () => {
  const desktop = computeInitialRect(1440, 900, 700, 740, 420, 460, 0, 'center');
  assert.ok(desktop.x >= 8);
  assert.ok(desktop.y >= VIEWPORT_TOP_INSET);
  assert.ok(desktop.width >= 420 && desktop.width <= 1424);
  assert.ok(desktop.height >= 460 && desktop.height <= 900 - VIEWPORT_TOP_INSET);

  const mobile = computeInitialRect(375, 812, 880, 640, 460, 420, 0, 'center');
  assert.equal(mobile.x, 8);
  assert.equal(mobile.width, 359);
  assert.ok(mobile.height <= 812 - VIEWPORT_TOP_INSET);
});

test('clampWindowPosition keeps titlebar grab handle reachable across viewport edges', () => {
  const topClipped = clampWindowPosition(
    { x: 200, y: -120, width: 500, height: 400 },
    1440,
    900
  );
  assert.equal(topClipped.y, VIEWPORT_TOP_INSET);

  const offRight = clampWindowPosition(
    { x: 2000, y: 200, width: 500, height: 400 },
    1440,
    900
  );
  assert.equal(offRight.x, 1440 - 140);

  const offLeft = clampWindowPosition(
    { x: -1000, y: 200, width: 500, height: 400 },
    1440,
    900
  );
  assert.equal(offLeft.x, -500 + 140);
});

test('applyWindowResize enforces strict minimum bounds from SE and NW without drifting opposite edge', () => {
  const start = { x: 400, y: 200, width: 600, height: 500 };
  const bounds = { minWidth: 380, minHeight: 440 };

  const shrunkSe = applyWindowResize(start, 'se', -500, -500, bounds, 1440, 900);
  assert.deepEqual(shrunkSe, { x: 400, y: 200, width: 380, height: 440 });

  const shrunkNw = applyWindowResize(start, 'nw', 500, 500, bounds, 1440, 900);
  // Opposite bottom-right edge (1000, 700) must stay pinned!
  assert.equal(shrunkNw.x + shrunkNw.width, 1000);
  assert.equal(shrunkNw.y + shrunkNw.height, 700);
  assert.equal(shrunkNw.width, 380);
  assert.equal(shrunkNw.height, 440);
});

test('floating window focus stack updates top window and z-index cleanly', () => {
  resetWindowStackForTests();
  registerFloatingWindow('telemetry');
  registerFloatingWindow('flash');
  assert.equal(getTopFloatingWindowId(), 'flash');
  assert.ok(getWindowZIndex('flash') > getWindowZIndex('telemetry'));

  focusFloatingWindow('telemetry');
  assert.equal(getTopFloatingWindowId(), 'telemetry');
  assert.ok(getWindowZIndex('telemetry') > getWindowZIndex('flash'));

  unregisterFloatingWindow('telemetry');
  assert.equal(getTopFloatingWindowId(), 'flash');
});
