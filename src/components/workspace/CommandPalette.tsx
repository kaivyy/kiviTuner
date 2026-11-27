import React, { useEffect, useMemo, useRef, useState } from 'react';
import { Command, CornerDownLeft, Search, X } from 'lucide-react';
import type { MapDef } from '../../types/kivi';
import { useAnimatedPresence } from '../../hooks/useAnimatedPresence';

export interface CommandPaletteAction {
  id: string;
  label: string;
  category: string;
  shortcut?: string;
  disabled?: boolean;
  onExecute: () => void;
}

export interface CommandPaletteProps {
  isOpen: boolean;
  onClose: () => void;
  maps: MapDef[];
  onSelectMap: (map: MapDef) => void;
  actions: CommandPaletteAction[];
}

interface PaletteItem {
  id: string;
  label: string;
  meta: string;
  shortcut?: string;
  disabled?: boolean;
  run: () => void;
}

export const CommandPalette: React.FC<CommandPaletteProps> = ({
  isOpen,
  onClose,
  maps,
  onSelectMap,
  actions,
}) => {
  const [query, setQuery] = useState('');
  const [selectedIdx, setSelectedIdx] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const presence = useAnimatedPresence(isOpen, 110);

  useEffect(() => {
    if (isOpen) {
      setQuery('');
      setSelectedIdx(0);
      setTimeout(() => inputRef.current?.focus(), 0);
    }
  }, [isOpen]);

  const items = useMemo<PaletteItem[]>(() => {
    const q = query.trim().toLowerCase();
    const actionItems: PaletteItem[] = actions
      .filter(
        (a) =>
          !a.disabled &&
          (!q ||
            a.label.toLowerCase().includes(q) ||
            a.category.toLowerCase().includes(q) ||
            (a.shortcut?.toLowerCase().includes(q) ?? false))
      )
      .map((a) => ({
        id: `act:${a.id}`,
        label: a.label,
        meta: a.category,
        shortcut: a.shortcut,
        run: a.onExecute,
      }));

    const mapItems: PaletteItem[] = maps
      .filter((m) => {
        if (!q) return true;
        const addrHex = `0x${m.address.toString(16)}`;
        return (
          m.name.toLowerCase().includes(q) ||
          m.category.toLowerCase().includes(q) ||
          addrHex.toLowerCase().includes(q)
        );
      })
      .slice(0, 60)
      .map((m) => ({
        id: `map:${m.id}`,
        label: m.name,
        meta: `${m.category || 'Map'} | ${m.rows}x${m.cols} @ 0x${m.address.toString(16).toUpperCase()}`,
        run: () => onSelectMap(m),
      }));

    return [...actionItems, ...mapItems];
  }, [query, actions, maps, onSelectMap]);

  useEffect(() => {
    if (selectedIdx >= items.length) {
      setSelectedIdx(Math.max(0, items.length - 1));
    }
  }, [items.length, selectedIdx]);

  if (!presence.mounted) return null;

  const runItem = (item: PaletteItem | undefined) => {
    if (!item || item.disabled) return;
    onClose();
    item.run();
  };

  return (
    <div
      className={`fixed inset-0 z-50 flex items-start justify-center bg-slate-950/40 pt-20 px-4 ${
        presence.state === 'closing' ? 'kivi-anim-backdrop-out' : 'kivi-anim-backdrop-in'
      }`}
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
      role="dialog"
      aria-modal="true"
      aria-label="Command palette"
    >
      <div
        className={`w-full max-w-xl overflow-hidden rounded-2xl kivi-floating-window bg-kiviSurface ${
          presence.state === 'closing' ? 'kivi-anim-pop-out' : 'kivi-anim-pop-in'
        }`}
      >
        <div className="flex h-10 shrink-0 items-center gap-2 border-b border-kiviBorder/50 px-3">
          <Search className="h-4 w-4 text-kiviAccent" />
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              setSelectedIdx(0);
            }}
            onKeyDown={(e) => {
              if (e.key === 'Escape') {
                e.preventDefault();
                onClose();
              } else if (e.key === 'ArrowDown') {
                e.preventDefault();
                setSelectedIdx((i) => (items.length ? (i + 1) % items.length : 0));
              } else if (e.key === 'ArrowUp') {
                e.preventDefault();
                setSelectedIdx((i) => (items.length ? (i - 1 + items.length) % items.length : 0));
              } else if (e.key === 'Enter') {
                e.preventDefault();
                runItem(items[selectedIdx]);
              }
            }}
            placeholder="Search calibration maps (name, hex offset) or workspace commands..."
            className="flex-1 bg-transparent text-xs text-kiviInk placeholder:text-kiviMuted focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-kiviAccent"
          />
          <button
            type="button"
            aria-label="Close command palette"
            onClick={onClose}
            className="rounded p-1 text-kiviMuted hover:bg-kiviElevated hover:text-kiviInk"
          >
            <X className="h-3.5 w-3.5" />
          </button>
        </div>

        <div className="max-h-80 overflow-y-auto py-1">
          {items.length === 0 ? (
            <div className="px-4 py-8 text-center text-xs text-kiviMuted">
              No matching maps or workspace actions.
            </div>
          ) : (
            items.map((item, idx) => {
              const active = idx === selectedIdx;
              return (
                <button
                  key={item.id}
                  type="button"
                  onMouseEnter={() => setSelectedIdx(idx)}
                  onClick={() => runItem(item)}
                  className={`flex w-full items-center justify-between px-3 py-2 text-left text-xs transition-colors ${
                    active ? 'bg-kiviAccent/15 text-kiviInk' : 'text-kiviSecondary hover:bg-kiviElevated'
                  }`}
                >
                  <div className="min-w-0 pr-3">
                    <div className="truncate font-medium">{item.label}</div>
                    <div className="truncate font-mono text-[10px] text-kiviMuted tabular-nums">
                      {item.meta}
                    </div>
                  </div>
                  <div className="flex shrink-0 items-center gap-2">
                    {item.shortcut && (
                      <span className="rounded border border-kiviBorder bg-kiviBg px-1.5 py-0.5 font-mono text-[10px] text-kiviMuted">
                        {item.shortcut}
                      </span>
                    )}
                    {active && <CornerDownLeft className="h-3 w-3 text-kiviAccent" />}
                  </div>
                </button>
              );
            })
          )}
        </div>

        <div className="flex items-center justify-between border-t border-kiviBorder bg-kiviBg px-3 py-1.5 text-[10px] text-kiviMuted">
          <span className="flex items-center gap-1">
            <Command className="h-3 w-3 text-kiviAccent" />
            <span>Up/Down to navigate, Enter to run, Esc to close</span>
          </span>
          <span className="tabular-nums">{items.length} results</span>
        </div>
      </div>
    </div>
  );
};
