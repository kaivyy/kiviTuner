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

export interface ActivityRailProps {
  showMapTree: boolean;
  onToggleMapTree: () => void;
  showInspector?: boolean;
  onToggleInspector?: () => void;
  isScannerOpen: boolean;
  onToggleScanner: () => void;
  activeTabKind: string | null;
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
  label: string;
  active?: boolean;
  onClick: () => void;
  children: React.ReactNode;
}

const RailButton: React.FC<RailButtonProps> = ({ label, active = false, onClick, children }) => (
  <button
    type="button"
    aria-label={label}
    aria-pressed={active}
    title={label}
    onClick={onClick}
    className={`relative flex h-9 w-9 items-center justify-center rounded-md border transition-colors ${
      active
        ? 'border-kiviAccent/60 bg-kiviAccent/15 text-kiviAccent'
        : 'border-transparent text-kiviSecondary hover:bg-kiviElevated hover:text-kiviInk'
    }`}
  >
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
  onOpenHex,
  onOpenWaveform,
  onOpenWorkbench,
  onOpenFlash,
  onOpenDiff,
  onOpenCommandPalette,
  theme,
  onToggleTheme,
}) => {
  return (
    <aside
      aria-label="Workspace tools rail"
      className="hidden w-11 shrink-0 flex-col items-center justify-between border-r border-kiviBorder bg-kiviSurface py-2 md:flex"
    >
      <div className="flex flex-col items-center gap-1.5">
        <RailButton
          label="Toggle Map Explorer"
          active={showMapTree}
          onClick={onToggleMapTree}
        >
          <FolderTree className="h-4 w-4" />
        </RailButton>

        <RailButton
          label="Toggle Heuristic Auto-Scanner"
          active={isScannerOpen}
          onClick={onToggleScanner}
        >
          <Search className="h-4 w-4" />
        </RailButton>

        <RailButton
          label="Hex Viewer Tab"
          active={activeTabKind === 'hex'}
          onClick={onOpenHex}
        >
          <Binary className="h-4 w-4" />
        </RailButton>

        <RailButton
          label="2D Binary Waveform Tab"
          active={activeTabKind === 'waveform'}
          onClick={onOpenWaveform}
        >
          <Activity className="h-4 w-4" />
        </RailButton>

        <div className="my-1 h-px w-6 bg-kiviBorder" />

        <RailButton label="Calibration Workbench (Ctrl+Shift+W)" onClick={onOpenWorkbench}>
          <Sliders className="h-4 w-4" />
        </RailButton>

        <RailButton label="Hardware ECU Flash & Dump" onClick={onOpenFlash}>
          <Zap className="h-4 w-4" />
        </RailButton>

        <RailButton label="Binary Diff & Verification" onClick={onOpenDiff}>
          <GitCompare className="h-4 w-4" />
        </RailButton>
      </div>

      <div className="flex flex-col items-center gap-1.5">
        {onToggleInspector && (
          <RailButton
            label="Toggle Inspector Panel"
            active={showInspector}
            onClick={onToggleInspector}
          >
            <PanelRight className="h-4 w-4" />
          </RailButton>
        )}

        <RailButton label="Command Palette (Ctrl+K)" onClick={onOpenCommandPalette}>
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
  const itemClass = (active: boolean) =>
    `flex min-h-[44px] min-w-[44px] flex-1 flex-col items-center justify-center gap-0.5 rounded px-1 py-1 text-[10px] font-medium whitespace-nowrap transition-colors active:bg-kiviElevated ${
      active
        ? 'border border-kiviAccent/50 bg-kiviAccent/15 text-kiviAccent'
        : 'border border-transparent text-kiviSecondary hover:bg-kiviElevated hover:text-kiviInk'
    }`;

  return (
    <nav
      aria-label="Mobile workspace bar"
      className="flex shrink-0 items-center justify-between gap-1 border-t border-kiviBorder bg-kiviSurface px-1.5 py-1 md:hidden"
    >
      <button type="button" onClick={onToggleMapTree} className={itemClass(showMapTree)}>
        <FolderTree className="h-4 w-4 shrink-0" />
        <span>Maps</span>
      </button>
      <button type="button" onClick={onOpenHex} className={itemClass(activeTabKind === 'hex')}>
        <Binary className="h-4 w-4 shrink-0" />
        <span>Hex</span>
      </button>
      <button type="button" onClick={onOpenWaveform} className={itemClass(activeTabKind === 'waveform')}>
        <Activity className="h-4 w-4 shrink-0" />
        <span>Wave</span>
      </button>
      <button type="button" onClick={onOpenWorkbench} className={itemClass(false)}>
        <Sliders className="h-4 w-4 shrink-0" />
        <span>Bench</span>
      </button>
      <button type="button" onClick={onToggleInspector} className={itemClass(showInspector)}>
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
