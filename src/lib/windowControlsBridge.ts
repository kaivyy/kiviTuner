import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { isDesktopApp } from './bridge/romBridge.ts';

export const TOOL_WINDOW_IDS = [
  'telemetry',
  'flash',
  'toolkit',
  'catalogue',
  'diff',
  'workbench',
  'autotune',
  'guide',
  'shortcuts',
  'about',
  'map-props',
] as const;

export type ToolWindowId = (typeof TOOL_WINDOW_IDS)[number];

export interface OpenToolWindowOptions {
  toolId: ToolWindowId;
  title: string;
  width: number;
  height: number;
  minWidth: number;
  minHeight: number;
  alwaysOnTop?: boolean;
}

export interface DesktopWindowLike {
  close: () => Promise<void>;
  minimize: () => Promise<void>;
  toggleMaximize: () => Promise<void>;
  isMaximized: () => Promise<boolean>;
  isFullscreen: () => Promise<boolean>;
  setFullscreen: (fullscreen: boolean) => Promise<void>;
  startDragging: () => Promise<void>;
}

export interface WindowControlsDeps {
  isDesktop?: () => boolean;
  getWindow?: () => DesktopWindowLike;
  invokeFn?: typeof invoke;
}

function defaultIsDesktop(deps?: WindowControlsDeps): boolean {
  try {
    return (deps?.isDesktop ?? isDesktopApp)();
  } catch {
    return false;
  }
}

function desktopWindow(deps?: WindowControlsDeps): DesktopWindowLike {
  return (deps?.getWindow ?? getCurrentWindow)();
}

/** Extracts a validated `?window=<id>` tool window identifier from URL search query. */
export function getStandaloneWindowId(search?: string): ToolWindowId | null {
  const rawSearch =
    search !== undefined
      ? search
      : typeof window !== 'undefined'
        ? window.location.search
        : '';
  if (!rawSearch) return null;
  const params = new URLSearchParams(rawSearch.startsWith('?') ? rawSearch : `?${rawSearch}`);
  const value = params.get('window');
  if (!value) return null;
  return (TOOL_WINDOW_IDS as readonly string[]).includes(value) ? (value as ToolWindowId) : null;
}

/** Spawns or focuses a frameless separate Tauri desktop window; returns `false` in browsers. */
export async function bridgeOpenToolWindow(
  opts: OpenToolWindowOptions,
  deps?: WindowControlsDeps
): Promise<boolean> {
  if (!defaultIsDesktop(deps)) return false;
  try {
    const call = deps?.invokeFn ?? invoke;
    await call<string>('open_tool_window', {
      spec: {
        tool_id: opts.toolId,
        title: opts.title,
        width: opts.width,
        height: opts.height,
        min_width: opts.minWidth,
        min_height: opts.minHeight,
        always_on_top: opts.alwaysOnTop ?? false,
      },
    });
    return true;
  } catch (err) {
    console.warn('open_tool_window IPC error:', err);
    return false;
  }
}

/** Toggles OS always-on-top pinning for the current Tauri desktop window. */
export async function bridgeSetWindowAlwaysOnTop(
  alwaysOnTop: boolean,
  deps?: WindowControlsDeps
): Promise<boolean> {
  if (!defaultIsDesktop(deps)) return false;
  try {
    const call = deps?.invokeFn ?? invoke;
    return await call<boolean>('set_current_window_always_on_top', {
      alwaysOnTop,
    });
  } catch (err) {
    console.warn('set_current_window_always_on_top IPC error:', err);
    return false;
  }
}

export async function bridgeWindowClose(deps?: WindowControlsDeps): Promise<boolean> {
  if (defaultIsDesktop(deps)) {
    try {
      await desktopWindow(deps).close();
      return true;
    } catch {
      return false;
    }
  }
  if (typeof window !== 'undefined' && typeof window.close === 'function') {
    try {
      window.close();
      return true;
    } catch {
      return false;
    }
  }
  return false;
}

export async function bridgeWindowMinimize(deps?: WindowControlsDeps): Promise<boolean> {
  if (defaultIsDesktop(deps)) {
    try {
      await desktopWindow(deps).minimize();
      return true;
    } catch {
      return false;
    }
  }
  if (
    typeof document !== 'undefined' &&
    document.fullscreenElement &&
    typeof document.exitFullscreen === 'function'
  ) {
    try {
      await document.exitFullscreen();
      return true;
    } catch {
      return false;
    }
  }
  return false;
}

export async function bridgeWindowToggleMaximize(
  fullscreen = false,
  deps?: WindowControlsDeps
): Promise<boolean> {
  if (defaultIsDesktop(deps)) {
    try {
      const win = desktopWindow(deps);
      if (fullscreen) {
        const nextFs = !(await win.isFullscreen());
        await win.setFullscreen(nextFs);
        return nextFs;
      }
      await win.toggleMaximize();
      return await win.isMaximized();
    } catch {
      return false;
    }
  }
  if (typeof document !== 'undefined' && document.documentElement) {
    try {
      if (document.fullscreenElement && typeof document.exitFullscreen === 'function') {
        await document.exitFullscreen();
        return false;
      }
      if (typeof document.documentElement.requestFullscreen === 'function') {
        await document.documentElement.requestFullscreen();
        return true;
      }
    } catch {
      return Boolean(document.fullscreenElement);
    }
  }
  return false;
}

export async function bridgeWindowIsMaximized(deps?: WindowControlsDeps): Promise<boolean> {
  if (defaultIsDesktop(deps)) {
    try {
      const win = desktopWindow(deps);
      return (await win.isMaximized()) || (await win.isFullscreen());
    } catch {
      return false;
    }
  }
  if (typeof document !== 'undefined') {
    return Boolean(document.fullscreenElement);
  }
  return false;
}

export async function bridgeWindowStartDragging(deps?: WindowControlsDeps): Promise<boolean> {
  if (defaultIsDesktop(deps)) {
    try {
      await desktopWindow(deps).startDragging();
      return true;
    } catch {
      return false;
    }
  }
  return false;
}

export function subscribeWindowState(onChange: (focused: boolean) => void): () => void {
  if (typeof window === 'undefined' || typeof document === 'undefined') {
    return () => {};
  }
  const onFocus = () => onChange(true);
  const onBlur = () => onChange(false);
  const onResize = () => onChange(document.hasFocus());
  window.addEventListener('focus', onFocus);
  window.addEventListener('blur', onBlur);
  window.addEventListener('resize', onResize);
  document.addEventListener('fullscreenchange', onResize);
  return () => {
    window.removeEventListener('focus', onFocus);
    window.removeEventListener('blur', onBlur);
    window.removeEventListener('resize', onResize);
    document.removeEventListener('fullscreenchange', onResize);
  };
}
