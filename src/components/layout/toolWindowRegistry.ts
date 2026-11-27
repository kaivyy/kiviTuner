import {
  bridgeOpenToolWindow,
  type ToolWindowId,
} from '../../lib/windowControlsBridge';
import { isDesktopApp } from '../../lib/tauriBridge';
import type { WindowDock } from './floatingWindowModel';

export interface ToolWindowPreset {
  toolId: ToolWindowId;
  title: string;
  width: number;
  height: number;
  minWidth: number;
  minHeight: number;
  dock: WindowDock;
  supportsDesktopPopout: boolean;
}

export const TOOL_WINDOW_SPECS: Record<ToolWindowId, ToolWindowPreset> = {
  telemetry: {
    toolId: 'telemetry',
    title: 'ECU Live Telemetry & Diagnostics',
    width: 580,
    height: 740,
    minWidth: 380,
    minHeight: 440,
    dock: 'right',
    supportsDesktopPopout: true,
  },
  flash: {
    toolId: 'flash',
    title: 'ECU Read & Flash',
    width: 700,
    height: 740,
    minWidth: 420,
    minHeight: 460,
    dock: 'center',
    supportsDesktopPopout: true,
  },
  toolkit: {
    toolId: 'toolkit',
    title: 'Engineering Toolkit',
    width: 880,
    height: 640,
    minWidth: 460,
    minHeight: 420,
    dock: 'center',
    supportsDesktopPopout: true,
  },
  catalogue: {
    toolId: 'catalogue',
    title: 'Reference Super-Catalogue',
    width: 880,
    height: 640,
    minWidth: 460,
    minHeight: 420,
    dock: 'center',
    supportsDesktopPopout: true,
  },
  diff: {
    toolId: 'diff',
    title: 'Binary ROM Difference Analyzer',
    width: 960,
    height: 720,
    minWidth: 520,
    minHeight: 440,
    dock: 'center',
    supportsDesktopPopout: false,
  },
  workbench: {
    toolId: 'workbench',
    title: 'Calibration Workbench',
    width: 960,
    height: 720,
    minWidth: 520,
    minHeight: 440,
    dock: 'center',
    supportsDesktopPopout: false,
  },
  autotune: {
    toolId: 'autotune',
    title: 'AFR Auto-Tune Assistant',
    width: 960,
    height: 700,
    minWidth: 520,
    minHeight: 440,
    dock: 'center',
    supportsDesktopPopout: false,
  },
    'map-props': {
    toolId: 'map-props',
    title: 'Map Properties',
    width: 640,
    height: 580,
    minWidth: 400,
    minHeight: 380,
    dock: 'center',
    supportsDesktopPopout: false,
  },
  guide: {
    toolId: 'guide',
    title: 'How to Use kiviTuner',
    width: 880,
    height: 620,
    minWidth: 460,
    minHeight: 420,
    dock: 'center',
    supportsDesktopPopout: true,
  },
  shortcuts: {
    toolId: 'shortcuts',
    title: 'Keyboard Shortcuts Cheatsheet',
    width: 820,
    height: 620,
    minWidth: 440,
    minHeight: 400,
    dock: 'center',
    supportsDesktopPopout: true,
  },
  about: {
    toolId: 'about',
    title: 'About kiviTuner',
    width: 460,
    height: 440,
    minWidth: 360,
    minHeight: 340,
    dock: 'center',
    supportsDesktopPopout: true,
  },
};

/**
 * On Tauri desktop builds, launches standalone-capable tools as separate frameless OS windows;
 * on web or for workspace-bound tools, falls back seamlessly to the in-app `FloatingWindow`.
 */
export async function openToolOrFloating(
  toolId: ToolWindowId,
  openInApp: () => void
): Promise<void> {
  const spec = TOOL_WINDOW_SPECS[toolId];
  if (spec.supportsDesktopPopout && isDesktopApp()) {
    const opened = await bridgeOpenToolWindow({
      toolId: spec.toolId,
      title: spec.title,
      width: spec.width,
      height: spec.height,
      minWidth: spec.minWidth,
      minHeight: spec.minHeight,
    });
    if (opened) return;
  }
  openInApp();
}
