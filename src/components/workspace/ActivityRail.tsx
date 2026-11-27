import React from 'react';
import {
  Activity,
  Binary,
  Command,
  FolderTree,
  GitCompare,
  Moon,
  PanelRight,
  Search,
  Sliders,
  Sun,
  Zap,
} from 'lucide-react';
import type { KiviTheme } from '../../stores/useThemeStore';
import {
  resolveActiveRailItem,
  type ActivityRailItemId,
} from './activityRailModel';

export interface ActivityRailProps {
  showMapTree: boolean;
  onToggleMapTree: () => void;
  showInspector?: boolean;
  onToggleInspector?: () => void;
  isScannerOpen: boolean;
  onToggleScanner: () => void;
  activeTabKind: string | null;
  activeRailItem?: ActivityRailItemId | null;
  isWorkbenchOpen?: boolean;
  isFlashPanelOpen?: boolean;
  isDiffModalOpen?: boolean;
  isCommandPaletteOpen?: boolean;
  onOpenHex: () => void;
  onOpenWaveform: () => void;
  onOpenWorkbench: () => void;
  onOpenFlash: () => void;
  onOpenDiff: () => void;
  onOpenCommandPalette: () => void;
  theme: KiviTheme;
  onToggleTheme: () => void;
}

interface RailButtonProps {
  itemId?: ActivityRailItemId;
  label: string;
  active?: boolean;
  openIndicator?: boolean;
  onClick: () => void;
  children: React.ReactNode;
}

const RailButton: React.FC<RailButtonProps> = ({
  itemId,
  label,
  active = false,
  openIndicator = false,
  onClick,
  children,
}) => (
  <button
    type="button"
    data-rail-item={itemId}
    data-rail-active={active ? 'true' : 'false'}
    aria-label={label}
    aria-pressed={Boolean(active || openIndicator)}
    title={label}
    onClick={onClick}
    className={`relative flex h-9 w-9 items-center justify-center rounded-md transition-[background-color,color,transform] duration-150 ease-out active:scale-95 ${
      active
        ? 'bg-kiviAccent/15 text-kiviAccent'
        : openIndicator
          ? 'text-kiviInk hover:bg-kiviElevated'
          : 'text-kiviSecondary hover:bg-kiviElevated hover:text-kiviInk'
    }`}
  >
    {active && (
      <span
        aria-hidden="true"
        className="pointer-events-none absolute left-0 top-2 bottom-2 w-[2px] rounded-r bg-kiviAccent"
      />
    )}
    {!active && openIndicator && (
      <span
        aria-hidden="true"
        className="pointer-events-none absolute right-1.5 top-1.5 h-1.5 w-1.5 rounded-full bg-kiviAccent/70"
      />
    )}
    {children}
  </button>
);

export const ActivityRail: React.FC<ActivityRailProps> = ({
  showMapTree,
  onToggleMapTree,
  showInspector = false,
  onToggleInspector,
  isScannerOpen,
  onToggleScanner,
  activeTabKind,
  activeRailItem,
  isWorkbenchOpen = false,
  isFlashPanelOpen = false,
  isDiffModalOpen = false,
  isCommandPaletteOpen = false,
  onOpenHex,
  onOpenWaveform,
  onOpenWorkbench,
  onOpenFlash,
  onOpenDiff,
  onOpenCommandPalette,
  theme,
  onToggleTheme,
}) => {
  const resolvedActive =
    activeRailItem !== undefined
      ? activeRailItem
      : resolveActiveRailItem({
          focusedItem: null,
          showMapTree,
          isScannerOpen,
          showInspector,
          activeTabKind,
          isWorkbenchOpen,
          isFlashPanelOpen,
          isDiffModalOpen,
          isCommandPaletteOpen,
        });

  return (
    <aside
      aria-label="Workspace tools rail"
      className="relative z-40 hidden w-11 shrink-0 flex-col items-center justify-between border-r border-kiviBorder/60 bg-kiviSurface py-2 md:flex"
    >
      <div className="flex flex-col items-center gap-1.5">
        <RailButton
          itemId="maps"
          label="Toggle Map Explorer"
          active={resolvedActive === 'maps'}
          openIndicator={showMapTree}
          onClick={onToggleMapTree}
        >
          <FolderTree className="h-4 w-4" />
        </RailButton>

        <RailButton
          itemId="scanner"
          label="Toggle Heuristic Auto-Scanner"
          active={resolvedActive === 'scanner'}
          openIndicator={isScannerOpen}
          onClick={onToggleScanner}
        >
          <Search className="h-4 w-4" />
        </RailButton>

        <RailButton
          itemId="hex"
          label="Hex Viewer Tab"
          active={resolvedActive === 'hex'}
          onClick={onOpenHex}
        >
          <Binary className="h-4 w-4" />
        </RailButton>

        <RailButton
          itemId="waveform"
          label="2D Binary Waveform Tab"
          active={resolvedActive === 'waveform'}
          onClick={onOpenWaveform}
        >
          <Activity className="h-4 w-4" />
        </RailButton>

        <div className="my-1 h-px w-6 bg-kiviBorder/50" />

        <RailButton
          itemId="workbench"
          label="Calibration Workbench (Ctrl+Shift+W)"
          active={resolvedActive === 'workbench'}
          onClick={onOpenWorkbench}
        >
          <Sliders className="h-4 w-4" />
        </RailButton>

        <RailButton
          itemId="flash"
          label="Hardware ECU Flash & Dump"
          active={resolvedActive === 'flash'}
          onClick={onOpenFlash}
        >
          <Zap className="h-4 w-4" />
        </RailButton>

        <RailButton
          itemId="diff"
          label="Binary Diff & Verification"
          active={resolvedActive === 'diff'}
          onClick={onOpenDiff}
        >
          <GitCompare className="h-4 w-4" />
        </RailButton>
      </div>

      <div className="flex flex-col items-center gap-1.5">
        {onToggleInspector && (
          <RailButton
            itemId="inspector"
            label="Toggle Inspector Panel"
            active={resolvedActive === 'inspector'}
            openIndicator={showInspector}
            onClick={onToggleInspector}
          >
            <PanelRight className="h-4 w-4" />
          </RailButton>
        )}

        <RailButton
          itemId="palette"
          label="Command Palette (Ctrl+K)"
          active={resolvedActive === 'palette'}
          onClick={onOpenCommandPalette}
        >
          <Command className="h-4 w-4" />
        </RailButton>

        <RailButton
          label={theme === 'dark' ? 'Switch to Lab Light Theme' : 'Switch to Carbon Dark Theme'}
          onClick={onToggleTheme}
        >
          {theme === 'dark' ? <Sun className="h-4 w-4" /> : <Moon className="h-4 w-4" />}
        </RailButton>
      </div>
    </aside>
  );
};

export interface MobileBottomNavProps {
  showMapTree: boolean;
  onToggleMapTree: () => void;
  showInspector: boolean;
  onToggleInspector: () => void;
  activeTabKind: string | null;
  onOpenHex: () => void;
  onOpenWaveform: () => void;
  onOpenWorkbench: () => void;
  theme: KiviTheme;
  onToggleTheme: () => void;
}

export const MobileBottomNav: React.FC<MobileBottomNavProps> = ({
  showMapTree,
  onToggleMapTree,
  showInspector,
  onToggleInspector,
  activeTabKind,
  onOpenHex,
  onOpenWaveform,
  onOpenWorkbench,
  theme,
  onToggleTheme,
}) => {
  const activeMobile: 'maps' | 'inspector' | 'hex' | 'waveform' | null = showMapTree
    ? 'maps'
    : showInspector
      ? 'inspector'
      : activeTabKind === 'hex'
        ? 'hex'
        : activeTabKind === 'waveform'
          ? 'waveform'
          : null;

  const itemClass = (active: boolean) =>
    `flex min-h-[44px] min-w-[44px] flex-1 flex-col items-center justify-center gap-0.5 rounded-md px-1 py-1 text-[10px] font-medium whitespace-nowrap transition-[background-color,color,transform] duration-150 ease-out active:scale-95 ${
      active
        ? 'bg-kiviAccent/15 text-kiviAccent'
        : 'text-kiviSecondary hover:bg-kiviElevated hover:text-kiviInk'
    }`;

  return (
    <nav
      aria-label="Mobile workspace bar"
      className="flex shrink-0 items-center justify-between gap-1 border-t border-kiviBorder/60 bg-kiviSurface px-1.5 py-1 md:hidden"
    >
      <button type="button" onClick={onToggleMapTree} className={itemClass(activeMobile === 'maps')}>
        <FolderTree className="h-4 w-4 shrink-0" />
        <span>Maps</span>
      </button>
      <button type="button" onClick={onOpenHex} className={itemClass(activeMobile === 'hex')}>
        <Binary className="h-4 w-4 shrink-0" />
        <span>Hex</span>
      </button>
      <button type="button" onClick={onOpenWaveform} className={itemClass(activeMobile === 'waveform')}>
        <Activity className="h-4 w-4 shrink-0" />
        <span>Wave</span>
      </button>
      <button type="button" onClick={onOpenWorkbench} className={itemClass(false)}>
        <Sliders className="h-4 w-4 shrink-0" />
        <span>Bench</span>
      </button>
      <button type="button" onClick={onToggleInspector} className={itemClass(activeMobile === 'inspector')}>
        <PanelRight className="h-4 w-4 shrink-0" />
        <span>Info</span>
      </button>
      <button
        type="button"
        onClick={onToggleTheme}
        aria-label={theme === 'dark' ? 'Switch to Lab Light Theme' : 'Switch to Carbon Dark Theme'}
        title={theme === 'dark' ? 'Switch to Lab Light Theme' : 'Switch to Carbon Dark Theme'}
        className={itemClass(false)}
      >
        {theme === 'dark' ? (
          <Sun className="h-4 w-4 shrink-0" />
        ) : (
          <Moon className="h-4 w-4 shrink-0" />
        )}
        <span>{theme === 'dark' ? 'Light' : 'Dark'}</span>
      </button>
    </nav>
  );
};
