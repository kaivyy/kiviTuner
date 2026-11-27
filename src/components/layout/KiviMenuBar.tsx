import React, { useState, useEffect, useRef, useCallback } from 'react';
import {
  ShieldCheck,
  ShieldAlert,
  Wrench,
  GitCommit,
  Eye,
  ChevronDown,
  ChevronRight,
  Check,
  Circle,
  Menu,
  X,
} from 'lucide-react';
import {
  buildDefaultMenuTree,
  type MenuNode,
  type MenuBarCallbacks,
  type MenuBarState,
} from './menuModel.ts';
import { WindowTrafficLights } from './WindowTrafficLights.tsx';
import { bridgeWindowToggleMaximize } from '../../lib/windowControlsBridge.ts';

const COMPACT_MENU_LABELS: Record<string, string> = {
  file: 'File',
  edit: 'Edit',
  definition: 'XDF',
  view: 'View',
  reverse: 'Reverse',
  ecu: 'ECU',
  help: 'Help',
};

// Re-export all types and tree builders so consumers have single import point
export * from './menuModel.ts';

export interface KiviMenuBarProps {
  menus?: MenuNode[];
  callbacks?: MenuBarCallbacks;
  state?: MenuBarState;

  // Status pills on right
  checksumStatus?: 'valid' | 'invalid' | 'pending' | null;
  onVerifyChecksum?: () => void;
  onFixChecksum?: () => void;

  deltaCount?: number;
  onDeltaClick?: () => void;

  displayModeText?: string;
  romName?: string;
  romSizeBytes?: number;

  // Branding
  brandTitle?: string;
  brandVersion?: string;

  // Global action hooks
  onOpenHowToUse?: () => void;
  onOpenShortcuts?: () => void;
}

export const KiviMenuBar: React.FC<KiviMenuBarProps> = ({
  menus: customMenus,
  callbacks,
  state,
  checksumStatus,
  onVerifyChecksum,
  onFixChecksum,
  deltaCount = 0,
  onDeltaClick,
  displayModeText = '16-Bit BE | Eng Val',
  romName,
  romSizeBytes = 0,
  brandTitle = 'kiviTuner',
  brandVersion = 'v0.1.0',
  onOpenHowToUse,
  onOpenShortcuts,
}) => {
  // Build menu hierarchy from props or defaults
  const menus = React.useMemo(() => {
    if (customMenus && customMenus.length > 0) return customMenus;
    const effectiveCallbacks: MenuBarCallbacks = {
      ...callbacks,
      onOpenHowToUse: onOpenHowToUse || callbacks?.onOpenHowToUse,
      onOpenShortcuts: onOpenShortcuts || callbacks?.onOpenShortcuts,
      onVerifyChecksum: onVerifyChecksum || callbacks?.onVerifyChecksum,
      onFixChecksum: onFixChecksum || callbacks?.onFixChecksum,
    };
    return buildDefaultMenuTree(effectiveCallbacks, state);
  }, [customMenus, callbacks, state, onOpenHowToUse, onOpenShortcuts, onVerifyChecksum, onFixChecksum]);

  // Which root top-level menu is open ('file', 'edit', etc.)
  const [activeRootId, setActiveRootId] = useState<string | null>(null);

  // Path of open submenus [level1Id, level2Id, ...]
  const [activeSubmenuPath, setActiveSubmenuPath] = useState<string[]>([]);
  const [isMobileMenuOpen, setIsMobileMenuOpen] = useState(false);

  const menuBarRef = useRef<HTMLDivElement>(null);

  // Close menus on click outside
  useEffect(() => {
    const handleOutsideClick = (e: MouseEvent) => {
      if (menuBarRef.current && !menuBarRef.current.contains(e.target as Node)) {
        setActiveRootId(null);
        setActiveSubmenuPath([]);
        setIsMobileMenuOpen(false);
      }
    };
    document.addEventListener('mousedown', handleOutsideClick);
    return () => document.removeEventListener('mousedown', handleOutsideClick);
  }, []);

  // Close menus on Escape key (F1 is owned globally by useGlobalShortcuts)
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        if (activeRootId !== null || isMobileMenuOpen) {
          e.preventDefault();
          setActiveRootId(null);
          setActiveSubmenuPath([]);
          setIsMobileMenuOpen(false);
        }
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [activeRootId, isMobileMenuOpen]);

  const handleRootClick = (menuId: string) => {
    if (activeRootId === menuId) {
      setActiveRootId(null);
      setActiveSubmenuPath([]);
    } else {
      setActiveRootId(menuId);
      setActiveSubmenuPath([]);
    }
  };

  const handleRootMouseEnter = (menuId: string) => {
    // If another root menu is already open, hovering switches active menu immediately
    if (activeRootId !== null && activeRootId !== menuId) {
      setActiveRootId(menuId);
      setActiveSubmenuPath([]);
    }
  };

  const closeAll = useCallback(() => {
    setActiveRootId(null);
    setActiveSubmenuPath([]);
    setIsMobileMenuOpen(false);
  }, []);

  return (
    <div
      ref={menuBarRef}
      className="relative z-[90] flex h-10 w-full shrink-0 select-none items-center justify-between gap-1.5 bg-kiviSurface/95 px-2.5 text-xs text-kiviInk backdrop-blur-sm md:h-8 md:gap-2 md:px-3"
    >
      {/* Left: Tahoe Traffic Lights + Brand Identity + Mobile Menu Trigger + Desktop Cascading Menus */}
      <div className="flex min-w-0 shrink-0 items-center gap-2">
        <div className="flex shrink-0 items-center pr-1">
          <WindowTrafficLights />
        </div>

        {/* Brand Logo & Name */}
        <div className="flex shrink-0 items-center gap-1.5 pr-1">
          <img
            src="/favicon.svg"
            alt="kiviTuner"
            className="h-4 w-4 shrink-0 rounded"
          />
          <span className="whitespace-nowrap text-[11px] font-bold tracking-wider text-kiviInk">
            {brandTitle}
          </span>
          <span className="hidden whitespace-nowrap rounded border border-kiviBorder/80 bg-kiviBg/80 px-1 py-0.2 font-mono text-[9px] text-kiviMuted sm:inline-block">
            {brandVersion}
          </span>
        </div>

        {/* Mobile Menu Trigger (< 768px) */}
        <button
          type="button"
          aria-label="Toggle application menu"
          aria-expanded={isMobileMenuOpen}
          onClick={() => setIsMobileMenuOpen((prev) => !prev)}
          className="flex min-h-[30px] shrink-0 items-center gap-1 whitespace-nowrap rounded border border-kiviBorderStrong bg-kiviElevated px-2 py-0.5 text-[11px] font-medium text-kiviInk transition-colors active:bg-kiviBorder md:hidden"
        >
          {isMobileMenuOpen ? (
            <X className="h-3.5 w-3.5 shrink-0 text-kiviAccent" />
          ) : (
            <Menu className="h-3.5 w-3.5 shrink-0 text-kiviAccent" />
          )}
          <span>Menu</span>
        </button>

        {/* 7 Top-Level Menu Buttons (>= 768px) */}
        <nav className="hidden items-center gap-0.5 md:flex" role="menubar">
          {menus.map((menu) => {
            const isOpen = activeRootId === menu.id;
            const compactLabel = COMPACT_MENU_LABELS[menu.id] ?? menu.label;
            return (
              <div key={menu.id} className="relative shrink-0">
                <button
                  type="button"
                  role="menuitem"
                  aria-label={menu.label}
                  title={menu.label}
                  aria-haspopup="true"
                  aria-expanded={isOpen}
                  onClick={() => handleRootClick(menu.id)}
                  onMouseEnter={() => handleRootMouseEnter(menu.id)}
                  className={`shrink-0 whitespace-nowrap rounded px-1.5 py-0.5 text-[11px] font-medium transition-colors cursor-pointer ${
                    isOpen
                      ? 'bg-kiviElevated text-kiviInk ring-1 ring-kiviBorderStrong'
                      : 'text-kiviSecondary hover:bg-kiviElevated hover:text-kiviInk'
                  }`}
                >
                  <span className="whitespace-nowrap 2xl:hidden">{compactLabel}</span>
                  <span className="hidden whitespace-nowrap 2xl:inline">{menu.label}</span>
                </button>

                {/* Root Dropdown Menu Flyout */}
                {isOpen && menu.children && (
                  <div className="absolute left-0 top-full z-50 mt-1">
                    <FlyoutMenuPanel
                      items={menu.children}
                      depth={0}
                      activeSubmenuPath={activeSubmenuPath}
                      setActiveSubmenuPath={setActiveSubmenuPath}
                      onCloseAll={closeAll}
                    />
                  </div>
                )}
              </div>
            );
          })}
        </nav>
      </div>

      {/* Draggable frameless desktop titlebar spacer */}
      <div
        data-tauri-drag-region
        aria-label="Window drag region"
        onDoubleClick={() => void bridgeWindowToggleMaximize(false)}
        className="h-full min-w-[4px] flex-1 cursor-default"
      />

      {/* Right: Compact Real-time Status Pills */}
      <div className="flex shrink-0 items-center gap-1 text-[10px] md:gap-1.5">
        {/* 1. Checksum Pill */}
        {checksumStatus === 'valid' ? (
          <button
            type="button"
            onClick={onVerifyChecksum}
            title="Checksum integrity valid. Click to re-verify."
            className="flex h-6 min-w-[68px] shrink-0 items-center justify-center gap-1 whitespace-nowrap rounded border border-kiviGreen/40 bg-kiviGreen/10 px-1.5 py-0.5 font-mono text-[10px] tabular-nums text-kiviGreen transition-colors hover:bg-kiviGreen/20"
          >
            <ShieldCheck className="h-3 w-3 shrink-0" />
            <span>CS: OK</span>
          </button>
        ) : checksumStatus === 'invalid' ? (
          <button
            type="button"
            onClick={onFixChecksum}
            title="Checksum mismatch detected! Click to calculate and auto-fix."
            className="flex h-6 min-w-[68px] shrink-0 items-center justify-center gap-1 whitespace-nowrap rounded border border-kiviRed/50 bg-kiviRed/10 px-1.5 py-0.5 font-mono text-[10px] tabular-nums text-kiviRed transition-colors hover:bg-kiviRed/20"
          >
            <ShieldAlert className="h-3 w-3 shrink-0" />
            <span>CS: BAD</span>
            <Wrench className="ml-0.5 h-3 w-3 shrink-0 text-kiviWarn" />
          </button>
        ) : (
          <button
            type="button"
            onClick={onVerifyChecksum}
            title="Click to verify calibration checksum integrity"
            className="flex h-6 min-w-[68px] shrink-0 items-center justify-center gap-1 whitespace-nowrap rounded border border-kiviBorderStrong bg-kiviBg px-1.5 py-0.5 font-mono text-[10px] tabular-nums text-kiviSecondary transition-colors hover:bg-kiviElevated hover:text-kiviInk"
          >
            <ShieldCheck className="h-3 w-3 shrink-0 text-kiviMuted" />
            <span>CS: CHK</span>
          </button>
        )}

        {/* 2. Delta Counter Pill */}
        <button
          type="button"
          onClick={onDeltaClick}
          title={deltaCount > 0 ? `${deltaCount} modified cells since last commit` : 'No uncommitted changes'}
          className={`hidden h-6 min-w-[76px] shrink-0 items-center justify-center gap-1 whitespace-nowrap rounded border px-1.5 py-0.5 font-mono text-[10px] tabular-nums transition-colors lg:flex ${
            deltaCount > 0
              ? 'border-kiviAccent/50 bg-kiviAccent/10 text-kiviAccent hover:bg-kiviAccent/20'
              : 'border-kiviBorder bg-kiviBg text-kiviSecondary hover:bg-kiviElevated hover:text-kiviInk'
          }`}
        >
          <GitCommit className={`h-3 w-3 shrink-0 ${deltaCount > 0 ? 'text-kiviAccent' : 'text-kiviMuted'}`} />
          <span>{deltaCount} {deltaCount === 1 ? 'Delta' : 'Deltas'}</span>
        </button>

        {/* 3. Display Mode Pill */}
        <div
          title="Active calibration table data representation"
          className="hidden h-6 shrink-0 items-center gap-1 whitespace-nowrap rounded border border-kiviBorder bg-kiviBg px-2 py-0.5 font-mono text-[10px] text-kiviSecondary 2xl:flex"
        >
          <Eye className="h-3 w-3 shrink-0 text-kiviMuted" />
          <span>{displayModeText}</span>
        </div>

        {/* 4. ROM Metadata Pill */}
        <div
          title={romName ? `Active ROM: ${romName} (${romSizeBytes} bytes)` : 'No ROM calibration loaded'}
          className="hidden h-6 shrink-0 items-center gap-1 whitespace-nowrap rounded border border-kiviBorder bg-kiviBg px-2 py-0.5 font-mono text-[10px] text-kiviSecondary xl:flex"
        >
          <span className={`h-1.5 w-1.5 shrink-0 rounded-full ${romName ? 'bg-kiviGreen' : 'bg-kiviMuted'}`} />
          <span className="max-w-[140px] truncate">
            {romName
              ? `${romName} (${(romSizeBytes / 1024).toFixed(0)} KB)`
              : 'No ROM Loaded'}
          </span>
        </div>
      </div>

      {/* Mobile Full Accordion Sheet (< 768px) */}
      {isMobileMenuOpen && (
        <div
          role="dialog"
          aria-label="Application navigation menu"
          className="fixed inset-x-0 top-10 bottom-0 z-50 overflow-y-auto border-t border-kiviBorder bg-kiviSurface p-3 md:hidden"
        >
          <div className="space-y-1.5">
            {menus.map((menu) => (
              <MobileMenuAccordion key={menu.id} node={menu} depth={0} onCloseAll={closeAll} />
            ))}
          </div>
        </div>
      )}
    </div>
  );
};

interface MobileMenuAccordionProps {
  node: MenuNode;
  depth: number;
  onCloseAll: () => void;
}

const MobileMenuAccordion: React.FC<MobileMenuAccordionProps> = ({ node, depth, onCloseAll }) => {
  const [expanded, setExpanded] = useState(false);

  if (node.type === 'separator') {
    return <div className="my-1 h-px bg-kiviBorder" role="separator" />;
  }

  const hasChildren = node.type === 'submenu' && Boolean(node.children && node.children.length > 0);
  if (hasChildren) {
    return (
      <div className="rounded border border-kiviBorder bg-kiviBg/60">
        <button
          type="button"
          aria-expanded={expanded}
          onClick={() => setExpanded((v) => !v)}
          className="flex min-h-[44px] w-full items-center justify-between gap-2 px-3 py-2 text-left text-xs font-medium text-kiviInk active:bg-kiviElevated"
        >
          <span className="truncate">{node.label}</span>
          {expanded ? (
            <ChevronDown className="h-4 w-4 shrink-0 text-kiviAccent" />
          ) : (
            <ChevronRight className="h-4 w-4 shrink-0 text-kiviMuted" />
          )}
        </button>
        {expanded && node.children && (
          <div className="space-y-1 border-t border-kiviBorder bg-kiviSurface p-1.5 pl-3">
            {node.children.map((child) => (
              <MobileMenuAccordion
                key={child.id}
                node={child}
                depth={depth + 1}
                onCloseAll={onCloseAll}
              />
            ))}
          </div>
        )}
      </div>
    );
  }

  return (
    <button
      type="button"
      disabled={node.disabled}
      onClick={() => {
        if (node.disabled) return;
        node.onSelect?.();
        onCloseAll();
      }}
      className={`flex min-h-[44px] w-full items-center justify-between gap-2 rounded px-3 py-2 text-left text-xs transition-colors ${
        node.disabled
          ? 'text-kiviMuted opacity-50 cursor-not-allowed'
          : 'text-kiviSecondary hover:bg-kiviElevated hover:text-kiviInk active:bg-kiviElevated'
      }`}
    >
      <span className="flex min-w-0 items-center gap-2">
        {node.type === 'radio' && (
          <span className="flex h-3.5 w-3.5 shrink-0 items-center justify-center">
            {node.checked ? (
              <span className="h-2 w-2 rounded-full bg-kiviAccent" />
            ) : (
              <Circle className="h-2 w-2 text-kiviMuted" />
            )}
          </span>
        )}
        {node.type === 'checkbox' && (
          <span
            className={`flex h-3.5 w-3.5 shrink-0 items-center justify-center rounded border ${
              node.checked
                ? 'border-kiviAccent bg-kiviAccent text-kiviAccentFg'
                : 'border-kiviBorderStrong bg-kiviBg'
            }`}
          >
            {node.checked && <Check className="h-2.5 w-2.5 stroke-[3]" />}
          </span>
        )}
        <span className="truncate">{node.label}</span>
      </span>
      {node.shortcut && (
        <span className="shrink-0 whitespace-nowrap font-mono text-[10px] text-kiviMuted">
          {node.shortcut}
        </span>
      )}
    </button>
  );
};

interface FlyoutMenuPanelProps {
  items: MenuNode[];
  depth: number;
  activeSubmenuPath: string[];
  setActiveSubmenuPath: React.Dispatch<React.SetStateAction<string[]>>;
  onCloseAll: () => void;
}

const FlyoutMenuPanel: React.FC<FlyoutMenuPanelProps> = ({
  items,
  depth,
  activeSubmenuPath,
  setActiveSubmenuPath,
  onCloseAll,
}) => {
  return (
    <div
      role="menu"
      className="min-w-[220px] max-w-[280px] rounded-lg border border-kiviBorderStrong bg-kiviSurface py-1.5 shadow-2xl text-kiviInk select-none"
    >
      {items.map((item) => (
        <FlyoutMenuItem
          key={item.id}
          item={item}
          depth={depth}
          activeSubmenuPath={activeSubmenuPath}
          setActiveSubmenuPath={setActiveSubmenuPath}
          onCloseAll={onCloseAll}
        />
      ))}
    </div>
  );
};

interface FlyoutMenuItemProps {
  item: MenuNode;
  depth: number;
  activeSubmenuPath: string[];
  setActiveSubmenuPath: React.Dispatch<React.SetStateAction<string[]>>;
  onCloseAll: () => void;
}

const FlyoutMenuItem: React.FC<FlyoutMenuItemProps> = ({
  item,
  depth,
  activeSubmenuPath,
  setActiveSubmenuPath,
  onCloseAll,
}) => {
  const itemRef = useRef<HTMLDivElement>(null);
  const [flipLeft, setFlipLeft] = useState(false);
  const [verticalOffset, setVerticalOffset] = useState<number>(0);

  const isSubmenu = item.type === 'submenu' && Boolean(item.children && item.children.length > 0);
  const isExpanded = activeSubmenuPath[depth] === item.id;

  // Measure viewport boundary and flip if near screen edge
  const updatePositionClamping = useCallback(() => {
    if (!itemRef.current) return;
    const rect = itemRef.current.getBoundingClientRect();
    const flyoutWidth = 230;
    const flyoutEstimatedHeight = (item.children?.length ?? 5) * 30 + 16;

    // Viewport right edge clamp: flip left if rect.right + flyoutWidth > window.innerWidth
    if (rect.right + flyoutWidth > window.innerWidth - 8) {
      setFlipLeft(true);
    } else {
      setFlipLeft(false);
    }

    // Viewport bottom edge clamp: shift upwards if rect.top + flyoutHeight > window.innerHeight
    if (rect.top + flyoutEstimatedHeight > window.innerHeight - 8) {
      const overflow = rect.top + flyoutEstimatedHeight - (window.innerHeight - 8);
      setVerticalOffset(-overflow);
    } else {
      setVerticalOffset(0);
    }
  }, [item.children]);

  const handleMouseEnter = () => {
    if (item.disabled) return;

    if (isSubmenu) {
      updatePositionClamping();
      setActiveSubmenuPath((prev) => [...prev.slice(0, depth), item.id]);
    } else {
      // Hovering a non-submenu item collapses child flyouts at this depth
      setActiveSubmenuPath((prev) => prev.slice(0, depth));
    }
  };

  const handleClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (item.disabled) return;

    if (item.type === 'separator') return;

    if (isSubmenu) {
      // On touch or click on submenu, toggle flyout
      updatePositionClamping();
      setActiveSubmenuPath((prev) =>
        prev[depth] === item.id ? prev.slice(0, depth) : [...prev.slice(0, depth), item.id]
      );
      return;
    }

    // Selectable action, radio, or checkbox
    item.onSelect?.();
    onCloseAll();
  };

  if (item.type === 'separator') {
    return <div className="my-1 h-px bg-kiviBorder mx-1.5" role="separator" />;
  }

  return (
    <div
      ref={itemRef}
      role="menuitem"
      aria-disabled={item.disabled}
      aria-haspopup={isSubmenu ? 'true' : undefined}
      aria-expanded={isSubmenu ? isExpanded : undefined}
      onClick={handleClick}
      onMouseEnter={handleMouseEnter}
      className={`relative flex items-center justify-between px-3 py-1.5 text-xs transition-colors cursor-pointer select-none ${
        item.disabled
          ? 'text-kiviMuted opacity-50 cursor-not-allowed'
          : isExpanded
          ? 'bg-kiviElevated text-kiviInk'
          : 'text-kiviSecondary hover:bg-kiviElevated hover:text-kiviInk'
      }`}
    >
      <div className="flex items-center gap-2 truncate pr-2">
        {/* State Indicators (Radio / Checkbox) */}
        {item.type === 'radio' && (
          <span className="flex h-3.5 w-3.5 items-center justify-center shrink-0">
            {item.checked ? (
              <span className="h-2 w-2 rounded-full bg-kiviAccent" />
            ) : (
              <Circle className="h-2 w-2 text-kiviMuted" />
            )}
          </span>
        )}

        {item.type === 'checkbox' && (
          <span
            className={`flex h-3.5 w-3.5 items-center justify-center rounded border shrink-0 ${
              item.checked
                ? 'border-kiviAccent bg-kiviAccent text-kiviAccentFg'
                : 'border-kiviBorderStrong bg-kiviBg'
            }`}
          >
            {item.checked && <Check className="h-2.5 w-2.5 stroke-[3]" />}
          </span>
        )}

        {/* Custom Icon if provided */}
        {item.icon && <span className="shrink-0">{item.icon}</span>}

        {/* Label */}
        <span className="truncate whitespace-nowrap">{item.label}</span>
      </div>

      {/* Right side: Shortcut label or Submenu Arrow */}
      <div className="flex items-center gap-1.5 shrink-0 ml-auto pl-2">
        {item.shortcut && (
          <span className="font-mono text-[10px] text-kiviMuted tracking-wider whitespace-nowrap">
            {item.shortcut}
          </span>
        )}

        {isSubmenu && <ChevronRight className="h-3.5 w-3.5 shrink-0 text-kiviMuted" />}
      </div>

      {/* Child Submenu Flyout (Recursive up to 3 levels deep) */}
      {isSubmenu && isExpanded && item.children && (
        <div
          style={{
            top: `${verticalOffset}px`,
            left: flipLeft ? undefined : '100%',
            right: flipLeft ? '100%' : undefined,
          }}
          className={`absolute z-50 ${flipLeft ? 'mr-1' : 'ml-1'}`}
        >
          <FlyoutMenuPanel
            items={item.children}
            depth={depth + 1}
            activeSubmenuPath={activeSubmenuPath}
            setActiveSubmenuPath={setActiveSubmenuPath}
            onCloseAll={onCloseAll}
          />
        </div>
      )}
    </div>
  );
};
