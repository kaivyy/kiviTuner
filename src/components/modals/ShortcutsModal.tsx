import React, { useState, useEffect, useMemo, useRef } from 'react';
import { Keyboard, Search, Sliders, Box, Activity, Command, Gauge, X } from 'lucide-react';
import { FloatingWindow } from '../layout/FloatingWindow';
import { TOOL_WINDOW_SPECS } from '../layout/toolWindowRegistry';
import {
  SHORTCUTS_REGISTRY,
  type ShortcutItem,
  type CategoryFilter,
} from './shortcutsData.ts';

export * from './shortcutsData.ts';

export interface ShortcutsModalProps {
  isOpen: boolean;
  onClose: () => void;
  standalone?: boolean;
}

export const ShortcutsModal: React.FC<ShortcutsModalProps> = ({
  isOpen,
  onClose,
  standalone = false,
}) => {
  const [searchQuery, setSearchQuery] = useState('');
  const [activeCategory, setActiveCategory] = useState<CategoryFilter>('All');
  const searchInputRef = useRef<HTMLInputElement>(null);

  // Focus search input when modal opens
  useEffect(() => {
    if (isOpen) {
      setSearchQuery('');
      setActiveCategory('All');
      const timer = setTimeout(() => {
        searchInputRef.current?.focus();
      }, 50);
      return () => clearTimeout(timer);
    }
  }, [isOpen]);

  // Filter shortcuts
  const filteredShortcuts = useMemo(() => {
    const q = searchQuery.trim().toLowerCase();
    return SHORTCUTS_REGISTRY.filter((item) => {
      const matchesCategory =
        activeCategory === 'All' || item.category === activeCategory;
      if (!matchesCategory) return false;

      if (!q) return true;

      const keysMatch = item.keys.some((k) => k.toLowerCase().includes(q));
      const actionMatch = item.action.toLowerCase().includes(q);
      const descMatch = item.description.toLowerCase().includes(q);
      const catMatch = item.category.toLowerCase().includes(q);

      return keysMatch || actionMatch || descMatch || catMatch;
    });
  }, [searchQuery, activeCategory]);

  // Group by category when 'All' is selected
  const categoriesList: Array<{
    name: 'Navigation & Workspace' | '2D Heatmap Matrix' | '3D Surface View' | '2D Waveform' | 'Acquisition';
    icon: React.ReactNode;
  }> = [
    {
      name: 'Navigation & Workspace',
      icon: <Command className="h-4 w-4 text-kiviAccent" />,
    },
    {
      name: '2D Heatmap Matrix',
      icon: <Sliders className="h-4 w-4 text-kiviGreen" />,
    },
    {
      name: '3D Surface View',
      icon: <Box className="h-4 w-4 text-kiviWarn" />,
    },
    {
      name: '2D Waveform',
      icon: <Activity className="h-4 w-4 text-indigo-400" />,
    },
    {
      name: 'Acquisition',
      icon: <Gauge className="h-4 w-4 text-kiviRed" />,
    },
  ];

  const spec = TOOL_WINDOW_SPECS.shortcuts;

  return (
    <FloatingWindow
      windowId="shortcuts"
      isOpen={isOpen}
      onClose={onClose}
      title="Keyboard Shortcuts Cheatsheet"
      titleId="shortcuts-modal-title"
      badge="Keyboard Reference"
      icon={<Keyboard className="h-4 w-4" />}
      defaultWidth={spec.width}
      defaultHeight={spec.height}
      minWidth={spec.minWidth}
      minHeight={spec.minHeight}
      dock={spec.dock}
      standalone={standalone}
      popoutToolId="shortcuts"
      closeLabel="Close shortcuts dialog"
      footer={
        <div className="flex shrink-0 flex-wrap items-center justify-between gap-2 border-t border-kiviBorder bg-kiviBg px-6 py-2.5 text-xs text-kiviMuted">
          <div className="flex items-center gap-2">
            <span className="inline-flex items-center justify-center rounded border border-kiviBorder/70 bg-kiviElevated px-1.5 py-0.5 font-mono text-[10px] text-kiviSecondary">
              Esc
            </span>
            <span>Closes active window</span>
          </div>
          <div className="text-[11px] text-kiviMuted">
            kiviTuner Calibration Suite &bull; Keyboard Reference
          </div>
        </div>
      }
    >
      <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
        {/* Search Bar & Category Filters */}
        <div className="shrink-0 border-b border-kiviBorder bg-kiviSurface px-6 py-3 space-y-3">
          <div className="relative">
            <Search className="absolute left-3 top-2.5 h-4 w-4 text-kiviMuted" />
            <input
              ref={searchInputRef}
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder="Search shortcuts by key or action (e.g. Ctrl, Smooth, Interpolate, Zoom)..."
              className="w-full rounded-md border border-kiviBorder/70 bg-kiviBg py-2 pl-9 pr-8 text-xs text-kiviInk placeholder:text-kiviMuted focus:border-kiviAccent/40 focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-kiviAccent focus:ring-1 focus:ring-kiviAccent transition-colors"
            />
            {searchQuery && (
              <button
                type="button"
                onClick={() => setSearchQuery('')}
                className="absolute right-2.5 top-2.5 text-kiviMuted hover:text-kiviInk"
              >
                <X className="h-4 w-4" />
              </button>
            )}
          </div>

          {/* Category Tabs */}
          <div className="flex flex-wrap items-center gap-1.5 text-xs">
            <button
              type="button"
              onClick={() => setActiveCategory('All')}
              className={`rounded-md px-3 py-1 font-medium transition-colors ${
                activeCategory === 'All'
                  ? 'bg-kiviAccent/15 text-kiviAccent border border-kiviAccent/40'
                  : 'bg-kiviElevated text-kiviMuted hover:bg-kiviElevated hover:text-kiviInk'
              }`}
            >
              All ({SHORTCUTS_REGISTRY.length})
            </button>
            {categoriesList.map((cat) => {
              const count = SHORTCUTS_REGISTRY.filter((s) => s.category === cat.name).length;
              const isSelected = activeCategory === cat.name;
              return (
                <button
                  key={cat.name}
                  type="button"
                  onClick={() => setActiveCategory(cat.name)}
                  className={`flex items-center gap-1.5 rounded-md px-2.5 py-1 font-medium transition-colors ${
                    isSelected
                      ? 'bg-kiviAccent/15 text-kiviAccent border border-kiviAccent/40'
                      : 'bg-kiviElevated text-kiviMuted hover:bg-kiviElevated hover:text-kiviInk'
                  }`}
                >
                  {cat.icon}
                  <span>{cat.name}</span>
                  <span className="text-[10px] text-kiviMuted">({count})</span>
                </button>
              );
            })}
          </div>
        </div>

        {/* Shortcuts List Content */}
        <div className="flex-1 overflow-y-auto p-6 space-y-6">
          {filteredShortcuts.length === 0 ? (
            <div className="flex flex-col items-center justify-center py-12 text-center text-kiviMuted">
              <Keyboard className="h-8 w-8 text-kiviMuted mb-2" />
              <p className="text-sm font-medium text-kiviMuted">No shortcuts matched your search</p>
              <p className="text-xs text-kiviMuted mt-1">
                Try searching for keys like &quot;Ctrl&quot;, &quot;Shift&quot;, or terms like &quot;Interpolate&quot;
              </p>
            </div>
          ) : activeCategory === 'All' && !searchQuery ? (
            // Render sectioned view by category when browsing all without search
            categoriesList.map((cat) => {
              const items = filteredShortcuts.filter((s) => s.category === cat.name);
              if (items.length === 0) return null;
              return (
                <div key={cat.name} className="space-y-2.5">
                  <div className="flex items-center gap-2 border-b border-kiviBorder pb-1.5">
                    {cat.icon}
                    <h3 className="text-xs font-semibold uppercase tracking-wider text-kiviSecondary">
                      {cat.name}
                    </h3>
                  </div>
                  <div className="grid grid-cols-1 md:grid-cols-2 gap-2">
                    {items.map((shortcut) => (
                      <ShortcutCard key={shortcut.id} item={shortcut} />
                    ))}
                  </div>
                </div>
              );
            })
          ) : (
            // Render flat grid when filtered
            <div className="grid grid-cols-1 md:grid-cols-2 gap-2">
              {filteredShortcuts.map((shortcut) => (
                <ShortcutCard key={shortcut.id} item={shortcut} />
              ))}
            </div>
          )}
        </div>
      </div>
    </FloatingWindow>
  );
};

interface ShortcutCardProps {
  item: ShortcutItem;
}

const ShortcutCard: React.FC<ShortcutCardProps> = ({ item }) => {
  return (
    <div className="flex items-center justify-between rounded-lg border border-kiviBorder bg-kiviBg p-2.5 hover:border-kiviBorder/70 hover:bg-kiviBg transition-colors">
      <div className="flex flex-col pr-3">
        <span className="text-xs font-medium text-kiviInk">{item.action}</span>
        <span className="text-[11px] text-kiviMuted leading-tight">{item.description}</span>
      </div>

      <div className="flex items-center gap-1 shrink-0">
        {item.keys.map((key, idx) => (
          <React.Fragment key={idx}>
            <kbd className="inline-block rounded border border-kiviBorder/70 bg-kiviElevated px-2 py-0.5 font-mono text-[11px] font-semibold text-kiviAccent">
              {key}
            </kbd>
            {idx < item.keys.length - 1 && <span className="text-[10px] text-kiviMuted">+</span>}
          </React.Fragment>
        ))}
      </div>
    </div>
  );
};
