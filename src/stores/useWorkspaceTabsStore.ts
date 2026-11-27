import { create } from 'zustand';
import type { MapDef } from '../types/kivi.ts';

export type WorkspaceTabKind =
  | 'map-table'
  | 'map-3d'
  | 'map-split'
  | 'hex'
  | 'waveform'
  | 'scanner'
  | 'diff'
  | 'workbench'
  | 'flash';

export interface WorkspaceTab {
  id: string;
  kind: WorkspaceTabKind;
  title: string;
  mapId?: string;
  pinned?: boolean;
  closable?: boolean;
}

export interface WorkspaceTabsState {
  tabs: WorkspaceTab[];
  activeTabId: string | null;
  splitTabId: string | null;
  splitRatio: number;

  openTab: (tab: WorkspaceTab) => void;
  activateTab: (id: string) => void;
  closeTab: (id: string) => void;
  togglePinTab: (id: string) => void;
  setTabKind: (id: string, kind: WorkspaceTabKind) => void;
  openInSplit: (tab: WorkspaceTab) => void;
  closeSplit: () => void;
  toggleSplitView: (fallbackTab?: WorkspaceTab) => void;
  setSplitRatio: (ratio: number) => void;
  closeOtherTabs: (keepId: string) => void;
  syncMapTab: (
    mapDef: MapDef,
    preferredKind?: 'map-table' | 'map-3d' | 'map-split',
    forceKind?: boolean
  ) => string;
  reset: () => void;
}

const MAX_TABS = 24;

function trimOverflowTabs(tabs: WorkspaceTab[], activeId: string | null, splitId: string | null): WorkspaceTab[] {
  if (tabs.length <= MAX_TABS) return tabs;
  const next = [...tabs];
  while (next.length > MAX_TABS) {
    const evictIdx = next.findIndex((t) => !t.pinned && t.id !== activeId && t.id !== splitId);
    if (evictIdx < 0) break;
    next.splice(evictIdx, 1);
  }
  return next;
}

export const useWorkspaceTabsStore = create<WorkspaceTabsState>((set, get) => ({
  tabs: [],
  activeTabId: null,
  splitTabId: null,
  splitRatio: 0.52,

  openTab: (tab) =>
    set((state) => {
      const existing = state.tabs.find((t) => t.id === tab.id);
      if (existing) {
        const updated = state.tabs.map((t) =>
          t.id === tab.id ? { ...t, title: tab.title, mapId: tab.mapId ?? t.mapId } : t
        );
        return {
          tabs: updated,
          activeTabId: tab.id,
          splitTabId: state.splitTabId === tab.id ? null : state.splitTabId,
        };
      }
      const added = trimOverflowTabs(
        [...state.tabs, { closable: true, pinned: false, ...tab }],
        tab.id,
        state.splitTabId
      );
      return { tabs: added, activeTabId: tab.id };
    }),

  activateTab: (id) =>
    set((state) => {
      if (!state.tabs.some((t) => t.id === id)) return state;
      return {
        activeTabId: id,
        splitTabId: state.splitTabId === id ? state.activeTabId : state.splitTabId,
      };
    }),

  closeTab: (id) =>
    set((state) => {
      const target = state.tabs.find((t) => t.id === id);
      if (!target || target.pinned || target.closable === false) return state;
      const idx = state.tabs.indexOf(target);
      const nextTabs = state.tabs.filter((t) => t.id !== id);
      let nextActive = state.activeTabId;
      if (nextActive === id) {
        if (nextTabs.length === 0) {
          nextActive = null;
        } else {
          const fallbackIdx = Math.min(idx, nextTabs.length - 1);
          nextActive = nextTabs[fallbackIdx].id;
        }
      }
      const nextSplit = state.splitTabId === id || state.splitTabId === nextActive ? null : state.splitTabId;
      return {
        tabs: nextTabs,
        activeTabId: nextActive,
        splitTabId: nextSplit,
      };
    }),

  togglePinTab: (id) =>
    set((state) => ({
      tabs: state.tabs.map((t) => (t.id === id ? { ...t, pinned: !t.pinned } : t)),
    })),

  setTabKind: (id, kind) =>
    set((state) => ({
      tabs: state.tabs.map((t) => (t.id === id ? { ...t, kind } : t)),
    })),

  openInSplit: (tab) =>
    set((state) => {
      const hasTab = state.tabs.some((t) => t.id === tab.id);
      const nextTabs = hasTab
        ? state.tabs.map((t) => (t.id === tab.id ? { ...t, ...tab } : t))
        : trimOverflowTabs(
            [...state.tabs, { closable: true, pinned: false, ...tab }],
            state.activeTabId,
            tab.id
          );
      if (!state.activeTabId) {
        return { tabs: nextTabs, activeTabId: tab.id, splitTabId: null };
      }
      if (state.activeTabId === tab.id) {
        return { tabs: nextTabs };
      }
      return { tabs: nextTabs, splitTabId: tab.id };
    }),

  closeSplit: () => set({ splitTabId: null }),

  toggleSplitView: (fallbackTab) =>
    set((state) => {
      if (state.splitTabId) {
        return { splitTabId: null };
      }
      const candidate = state.tabs.find((t) => t.id !== state.activeTabId);
      if (candidate) {
        return { splitTabId: candidate.id };
      }
      if (!fallbackTab) return state;
      const inserted = { closable: true, pinned: false, ...fallbackTab };
      if (!state.activeTabId) {
        return { tabs: [inserted], activeTabId: inserted.id, splitTabId: null };
      }
      if (inserted.id === state.activeTabId) return state;
      return {
        tabs: [...state.tabs, inserted],
        splitTabId: inserted.id,
      };
    }),

  setSplitRatio: (ratio) =>
    set({ splitRatio: Math.max(0.25, Math.min(0.75, ratio)) }),

  closeOtherTabs: (keepId) =>
    set((state) => {
      const nextTabs = state.tabs.filter((t) => t.id === keepId || t.pinned || t.closable === false);
      const nextActive = nextTabs.some((t) => t.id === state.activeTabId)
        ? state.activeTabId
        : nextTabs[0]?.id ?? null;
      const nextSplit = nextTabs.some((t) => t.id === state.splitTabId) && state.splitTabId !== nextActive
        ? state.splitTabId
        : null;
      return { tabs: nextTabs, activeTabId: nextActive, splitTabId: nextSplit };
    }),

  syncMapTab: (mapDef, preferredKind = 'map-split', forceKind = false) => {
    const id = `map:${mapDef.id}`;
    const existing = get().tabs.find((t) => t.id === id);
    const resolvedKind = forceKind ? preferredKind : existing?.kind ?? preferredKind;
    const nextTab: WorkspaceTab = {
      id,
      kind: resolvedKind,
      title: mapDef.name,
      mapId: mapDef.id,
      closable: true,
    };
    get().openTab(nextTab);
    if (forceKind && existing && existing.kind !== resolvedKind) {
      get().setTabKind(id, resolvedKind);
    }
    return id;
  },

  reset: () => set({ tabs: [], activeTabId: null, splitTabId: null, splitRatio: 0.52 }),
}));
