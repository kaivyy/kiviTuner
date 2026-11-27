/**
 * Pure geometry, viewport clamping, minimum-resize bounds, and z-order stacking
 * logic for frameless in-app floating windows.
 */

export interface WindowRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface WindowBounds {
  minWidth: number;
  minHeight: number;
}

export type WindowDock = 'center' | 'right';
export type ResizeEdge = 'n' | 's' | 'e' | 'w' | 'ne' | 'nw' | 'se' | 'sw';

export const VIEWPORT_TOP_INSET = 34;
export const VIEWPORT_EDGE_PAD = 8;
export const TITLEBAR_HEIGHT = 32;
export const MIN_VISIBLE_TITLE_X = 140;
export const FLOATING_BASE_Z = 50;

function clamp(v: number, lo: number, hi: number): number {
  if (hi < lo) return lo;
  return Math.max(lo, Math.min(hi, v));
}

/** Resolves effective minimum width/height clamped cleanly to narrow mobile viewports. */
export function resolveEffectiveMinimums(
  minWidth: number,
  minHeight: number,
  viewportW: number,
  viewportH: number
): WindowBounds {
  const maxAvailW = Math.max(220, viewportW - VIEWPORT_EDGE_PAD * 2);
  const maxAvailH = Math.max(180, viewportH - VIEWPORT_TOP_INSET - VIEWPORT_EDGE_PAD);
  return {
    minWidth: Math.min(Math.max(220, minWidth), maxAvailW),
    minHeight: Math.min(Math.max(180, minHeight), maxAvailH),
  };
}

/** Computes an initial viewport-safe rectangle respecting minimum size and optional docking. */
export function computeInitialRect(
  viewportW: number,
  viewportH: number,
  defaultW: number,
  defaultH: number,
  minW: number,
  minH: number,
  cascadeOffset = 0,
  dock: WindowDock = 'center'
): WindowRect {
  const safeVw = Math.max(300, viewportW);
  const safeVh = Math.max(300, viewportH);
  const mins = resolveEffectiveMinimums(minW, minH, safeVw, safeVh);

  const maxAvailW = Math.max(mins.minWidth, safeVw - VIEWPORT_EDGE_PAD * 2);
  const maxAvailH = Math.max(
    mins.minHeight,
    safeVh - VIEWPORT_TOP_INSET - VIEWPORT_EDGE_PAD * 2
  );

  if (safeVw < 640) {
    return {
      x: VIEWPORT_EDGE_PAD,
      y: VIEWPORT_TOP_INSET + 4,
      width: maxAvailW,
      height: clamp(defaultH, mins.minHeight, maxAvailH),
    };
  }

  const width = clamp(defaultW, mins.minWidth, maxAvailW);
  const height = clamp(defaultH, mins.minHeight, maxAvailH);
  const step = (Math.abs(Math.trunc(cascadeOffset)) % 5) * 22;

  let rawX: number;
  if (dock === 'right') {
    rawX = safeVw - width - 16 - step;
  } else {
    rawX = Math.round((safeVw - width) / 2) + step;
  }

  const rawY =
    dock === 'right'
      ? VIEWPORT_TOP_INSET + 8 + step
      : Math.max(
          VIEWPORT_TOP_INSET + 8,
          Math.round((safeVh - VIEWPORT_TOP_INSET - height) / 2) + VIEWPORT_TOP_INSET + step
        );

  const clamped = clampWindowPosition({ x: rawX, y: rawY, width, height }, safeVw, safeVh);
  return {
    x: clamp(clamped.x, VIEWPORT_EDGE_PAD, Math.max(VIEWPORT_EDGE_PAD, safeVw - width - VIEWPORT_EDGE_PAD)),
    y: clamp(
      clamped.y,
      VIEWPORT_TOP_INSET + 4,
      Math.max(VIEWPORT_TOP_INSET + 4, safeVh - height - VIEWPORT_EDGE_PAD)
    ),
    width,
    height,
  };
}

/** Clamps window drag position so the top titlebar always remains reachable in viewport. */
export function clampWindowPosition(
  rect: WindowRect,
  viewportW: number,
  viewportH: number
): WindowRect {
  const safeVw = Math.max(300, viewportW);
  const safeVh = Math.max(300, viewportH);
  const keepInsideX = Math.min(rect.width, MIN_VISIBLE_TITLE_X);
  const minX = -rect.width + keepInsideX;
  const maxX = safeVw - keepInsideX;
  const minY = VIEWPORT_TOP_INSET;
  const maxY = Math.max(minY, safeVh - TITLEBAR_HEIGHT - VIEWPORT_EDGE_PAD);

  return {
    ...rect,
    x: Math.round(clamp(rect.x, minX, maxX)),
    y: Math.round(clamp(rect.y, minY, maxY)),
  };
}

/** Resizes a floating window from any of 8 edges/corners without violating minimums or opposite edges. */
export function applyWindowResize(
  startRect: WindowRect,
  edge: ResizeEdge,
  dx: number,
  dy: number,
  bounds: WindowBounds,
  viewportW: number,
  viewportH: number
): WindowRect {
  const safeVw = Math.max(300, viewportW);
  const safeVh = Math.max(300, viewportH);
  const eff = resolveEffectiveMinimums(bounds.minWidth, bounds.minHeight, safeVw, safeVh);

  let { x, y, width, height } = startRect;
  const startRight = startRect.x + startRect.width;
  const startBottom = startRect.y + startRect.height;

  if (edge.includes('e')) {
    const maxW = Math.max(eff.minWidth, safeVw - startRect.x - VIEWPORT_EDGE_PAD);
    width = clamp(startRect.width + dx, eff.minWidth, maxW);
  }
  if (edge.includes('w')) {
    const minX = VIEWPORT_EDGE_PAD;
    const maxX = startRight - eff.minWidth;
    x = clamp(startRect.x + dx, minX, maxX);
    width = startRight - x;
  }
  if (edge.includes('s')) {
    const maxH = Math.max(eff.minHeight, safeVh - startRect.y - VIEWPORT_EDGE_PAD);
    height = clamp(startRect.height + dy, eff.minHeight, maxH);
  }
  if (edge.includes('n')) {
    const minY = VIEWPORT_TOP_INSET;
    const maxY = startBottom - eff.minHeight;
    y = clamp(startRect.y + dy, minY, maxY);
    height = startBottom - y;
  }

  return {
    x: Math.round(x),
    y: Math.round(y),
    width: Math.round(width),
    height: Math.round(height),
  };
}

/** Computes a fullscreen-within-workspace rectangle for the green maximize button. */
export function computeMaximizedRect(viewportW: number, viewportH: number): WindowRect {
  const safeVw = Math.max(300, viewportW);
  const safeVh = Math.max(300, viewportH);
  return {
    x: VIEWPORT_EDGE_PAD,
    y: VIEWPORT_TOP_INSET,
    width: Math.max(240, safeVw - VIEWPORT_EDGE_PAD * 2),
    height: Math.max(200, safeVh - VIEWPORT_TOP_INSET - VIEWPORT_EDGE_PAD),
  };
}

// ---------------------------------------------------------------------------
// Focus / Z-Order Stack Registry for In-App Floating Windows
// ---------------------------------------------------------------------------

let stackOrder: string[] = [];
const minimizedMap = new Map<string, string>();
const stackListeners = new Set<() => void>();
const restoreListeners = new Set<(id: string) => void>();

export interface MinimizedWindowEntry {
  id: string;
  title: string;
}

function notifyStack(): void {
  for (const listener of stackListeners) {
    listener();
  }
}

export function subscribeWindowStack(listener: () => void): () => void {
  stackListeners.add(listener);
  return () => {
    stackListeners.delete(listener);
  };
}

export function subscribeWindowRestore(listener: (id: string) => void): () => void {
  restoreListeners.add(listener);
  return () => {
    restoreListeners.delete(listener);
  };
}

export function registerFloatingWindow(id: string): number {
  if (!stackOrder.includes(id)) {
    stackOrder = [...stackOrder, id];
    notifyStack();
  }
  return stackOrder.length - 1;
}

export function unregisterFloatingWindow(id: string): void {
  const hadMinimized = minimizedMap.delete(id);
  const hadStack = stackOrder.includes(id);
  if (!hadStack && !hadMinimized) return;
  if (hadStack) {
    stackOrder = stackOrder.filter((item) => item !== id);
  }
  notifyStack();
}

export function setFloatingWindowMinimized(id: string, title: string, minimized: boolean): void {
  if (minimized) {
    if (minimizedMap.get(id) === title) return;
    minimizedMap.set(id, title);
    notifyStack();
  } else if (minimizedMap.delete(id)) {
    notifyStack();
  }
}

export function getMinimizedFloatingWindows(): MinimizedWindowEntry[] {
  return Array.from(minimizedMap.entries()).map(([id, title]) => ({ id, title }));
}

export function requestRestoreFloatingWindow(id: string): void {
  minimizedMap.delete(id);
  focusFloatingWindow(id);
  notifyStack();
  for (const listener of restoreListeners) {
    listener(id);
  }
}

export function focusFloatingWindow(id: string): void {
  if (stackOrder[stackOrder.length - 1] === id) return;
  stackOrder = [...stackOrder.filter((item) => item !== id), id];
  notifyStack();
}

export function getWindowZIndex(id: string): number {
  const idx = stackOrder.indexOf(id);
  return FLOATING_BASE_Z + (idx >= 0 ? idx * 2 : 0);
}

export function getTopFloatingWindowId(): string | null {
  return stackOrder.length > 0 ? stackOrder[stackOrder.length - 1] : null;
}

export function getOpenFloatingWindowCount(): number {
  return stackOrder.length;
}

export function resetWindowStackForTests(): void {
  stackOrder = [];
  minimizedMap.clear();
  notifyStack();
}
