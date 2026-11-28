import React, { useEffect } from 'react';
import {
  Activity,
  Binary,
  Box,
  ChevronLeft,
  ChevronRight,
  Columns,
  FileCode2,
  GitCompare,
  PictureInPicture2,
  Pin,
  Plus,
  Search,
  Sliders,
  Table2,
  X,
  Zap,
} from 'lucide-react';
import type { RomDocumentSummary } from '../../lib/bridge/windowBridge';
import type { WorkspaceTab, WorkspaceTabKind } from '../../stores/useWorkspaceTabsStore';
import { useHorizontalStripScroll } from '../../hooks/useHorizontalStripScroll';

export interface WorkspaceTabBarProps {
  romDocuments: RomDocumentSummary[];
  activeDocId: string | null;
  onSwitchRomDocument: (docId: string) => void;
  onCloseRomDocument: (docId: string) => void;
  onOpenRomDocument: () => void;

  tabs: WorkspaceTab[];
  activeTabId: string | null;
  splitTabId: string | null;
  onActivateTab: (id: string) => void;
  onCloseTab: (id: string) => void;
  onTogglePinTab: (id: string) => void;
  onDetachTab?: (id: string) => void;
  onToggleSplitView: () => void;
  onOpenCommandPalette: () => void;
}

function TabIcon({ kind }: { kind: WorkspaceTabKind }) {
  switch (kind) {
    case 'map-table':
      return <Table2 className="h-3.5 w-3.5 shrink-0" />;
    case 'map-3d':
      return <Box className="h-3.5 w-3.5 shrink-0" />;
    case 'map-split':
      return <Columns className="h-3.5 w-3.5 shrink-0" />;
    case 'hex':
      return <Binary className="h-3.5 w-3.5 shrink-0" />;
    case 'waveform':
      return <Activity className="h-3.5 w-3.5 shrink-0" />;
    case 'scanner':
      return <Search className="h-3.5 w-3.5 shrink-0" />;
    case 'diff':
      return <GitCompare className="h-3.5 w-3.5 shrink-0" />;
    case 'workbench':
      return <Sliders className="h-3.5 w-3.5 shrink-0" />;
    case 'flash':
      return <Zap className="h-3.5 w-3.5 shrink-0" />;
  }
}

function formatRomBytes(bytes: number): string {
  if (bytes >= 1024 * 1024) {
    return `${(bytes / (1024 * 1024)).toFixed(bytes % (1024 * 1024) === 0 ? 0 : 1)} MiB`;
  }
  if (bytes >= 1024) {
    return `${Math.round(bytes / 1024)} KiB`;
  }
  return `${bytes} B`;
}

export const WorkspaceTabBar: React.FC<WorkspaceTabBarProps> = React.memo(({
  romDocuments,
  activeDocId,
  onSwitchRomDocument,
  onCloseRomDocument,
  onOpenRomDocument,
  tabs,
  activeTabId,
  splitTabId,
  onActivateTab,
  onCloseTab,
  onTogglePinTab,
  onDetachTab,
  onToggleSplitView,
  onOpenCommandPalette,
}) => {
  const romSig = `${romDocuments.length}:${activeDocId ?? ''}`;
  const tabSig = `${tabs.length}:${activeTabId ?? ''}:${splitTabId ?? ''}`;
  const romStrip = useHorizontalStripScroll(romSig);
  const tabStrip = useHorizontalStripScroll(tabSig);

  useEffect(() => {
    const el = tabStrip.ref.current;
    if (!el || !activeTabId) return;
    const activeEl = el.querySelector<HTMLElement>(`[data-tab-id="${activeTabId}"]`);
    if (activeEl && typeof activeEl.scrollIntoView === 'function') {
      activeEl.scrollIntoView({ inline: 'nearest', block: 'nearest' });
    }
    tabStrip.refresh();
  }, [activeTabId, tabs.length, tabStrip.ref, tabStrip.refresh]);

  return (
    <div className="shrink-0 select-none border-b border-kiviBorder/60 bg-kiviSurface">
      {/* Multi-ROM Document Project Strip */}
      {romDocuments.length > 0 && (
        <div className="flex h-8 shrink-0 items-center gap-1 overflow-hidden border-b border-kiviBorder/50 bg-kiviBg/85 px-2 text-[11px]">
          <span className="mr-1 shrink-0 whitespace-nowrap text-[10px] font-semibold uppercase tracking-wider text-kiviMuted">
            ROMs
          </span>
          {romStrip.hasOverflow && (
            <button
              type="button"
              aria-label="Scroll ROM tabs left"
              disabled={!romStrip.canLeft}
              onClick={() => romStrip.scrollByDelta(-160)}
              className="flex h-6 w-5 shrink-0 items-center justify-center rounded border-0 bg-transparent text-kiviMuted transition-colors hover:text-kiviInk active:scale-95 disabled:pointer-events-none disabled:opacity-25"
            >
              <ChevronLeft className="h-3.5 w-3.5" />
            </button>
          )}
          <div
            ref={romStrip.ref}
            onScroll={romStrip.refresh}
            className="kivi-tab-scrollbar flex h-full min-w-0 flex-1 items-center gap-1 overflow-x-auto overflow-y-hidden"
          >
            {romDocuments.map((doc) => {
              const isActive = doc.id === activeDocId || doc.is_active;
              return (
                <div
                  key={doc.id}
                  className={`kivi-anim-tab-in group flex h-6 shrink-0 items-center gap-1.5 whitespace-nowrap rounded-md border px-2 transition-colors ${
                    isActive
                      ? 'border-kiviAccent/50 bg-kiviSurface text-kiviInk'
                      : 'border-transparent text-kiviSecondary hover:bg-kiviElevated hover:text-kiviInk'
                  }`}
                >
                  <button
                    type="button"
                    onClick={() => onSwitchRomDocument(doc.id)}
                    className="flex items-center gap-1.5 whitespace-nowrap text-left"
                    title={`${doc.label} (${doc.size} bytes${doc.sha256 ? `, SHA-256 ${doc.sha256.slice(0, 8)}` : ''})`}
                  >
                    <FileCode2
                      className={`h-3 w-3 shrink-0 ${isActive ? 'text-kiviAccent' : 'text-kiviMuted'}`}
                    />
                    <span className="max-w-[160px] truncate font-medium">{doc.label}</span>
                    <span className="shrink-0 font-mono text-[10px] text-kiviMuted tabular-nums">
                      {formatRomBytes(doc.size)}
                    </span>
                    {doc.undo.undo_count > 0 && (
                      <span
                        className="shrink-0 rounded bg-kiviWarn/15 px-1 text-[9px] font-semibold text-kiviWarn tabular-nums"
                        title={`${doc.undo.undo_count} uncommitted delta step(s)`}
                      >
                        *{doc.undo.undo_count}
                      </span>
                    )}
                  </button>
                  {romDocuments.length > 1 && (
                    <button
                      type="button"
                      aria-label={`Close ROM ${doc.label}`}
                      onClick={() => onCloseRomDocument(doc.id)}
                      className="shrink-0 rounded p-0.5 text-kiviMuted opacity-60 hover:bg-kiviElevated hover:text-kiviInk hover:opacity-100"
                    >
                      <X className="h-3 w-3" />
                    </button>
                  )}
                </div>
              );
            })}
            <button
              type="button"
              onClick={onOpenRomDocument}
              className="ml-1 flex h-6 shrink-0 items-center gap-1 whitespace-nowrap rounded-md border border-kiviBorder/70 px-1.5 text-[10px] text-kiviSecondary hover:border-kiviAccent/50 hover:text-kiviInk"
              title="Open another BIN/ROM tab side-by-side"
            >
              <Plus className="h-3 w-3 shrink-0" />
              <span>Open ROM</span>
            </button>
          </div>
          {romStrip.hasOverflow && (
            <button
              type="button"
              aria-label="Scroll ROM tabs right"
              disabled={!romStrip.canRight}
              onClick={() => romStrip.scrollByDelta(160)}
              className="flex h-6 w-5 shrink-0 items-center justify-center rounded border-0 bg-transparent text-kiviMuted transition-colors hover:text-kiviInk active:scale-95 disabled:pointer-events-none disabled:opacity-25"
            >
              <ChevronRight className="h-3.5 w-3.5" />
            </button>
          )}
        </div>
      )}

      {/* Workspace Editor Tabs Bar */}
      <div
        data-testid="workspace-editor-tabbar"
        className="flex h-10 shrink-0 items-center justify-between gap-1 overflow-hidden px-2"
      >
        {tabStrip.hasOverflow && (
          <button
            type="button"
            aria-label="Scroll tabs left"
            disabled={!tabStrip.canLeft}
            onClick={() => tabStrip.scrollByDelta(-180)}
            className="flex h-7 w-5 shrink-0 items-center justify-center rounded border-0 bg-transparent text-kiviMuted transition-colors hover:text-kiviInk active:scale-95 disabled:pointer-events-none disabled:opacity-25"
          >
            <ChevronLeft className="h-3.5 w-3.5" />
          </button>
        )}

        <div
          ref={tabStrip.ref}
          data-testid="workspace-tabs-scroll-strip"
          onScroll={tabStrip.refresh}
          className="kivi-tab-scrollbar flex h-full min-w-0 flex-1 items-center gap-1 overflow-x-auto overflow-y-hidden"
        >
          {tabs.length === 0 ? (
            <span className="flex h-7 items-center truncate px-2 text-[11px] text-kiviMuted">
              No open tabs. Select a map or launch Hex / Waveform / Scanner.
            </span>
          ) : (
            tabs.map((tab) => {
              const isActive = tab.id === activeTabId;
              const isSplit = tab.id === splitTabId;
              return (
                <div
                  key={tab.id}
                  data-tab-id={tab.id}
                  onMouseDown={(e) => {
                    if (e.button === 1 && !tab.pinned && tab.closable !== false) {
                      e.preventDefault();
                      onCloseTab(tab.id);
                    }
                  }}
                  className={`kivi-anim-tab-in flex h-7 shrink-0 items-center gap-1.5 whitespace-nowrap rounded-lg border px-2.5 text-xs transition-colors ${
                    isActive
                      ? 'border-kiviAccent/50 bg-kiviBg text-kiviInk font-medium'
                      : isSplit
                      ? 'border-kiviWarn/50 bg-kiviElevated text-kiviInk'
                      : 'border-transparent text-kiviSecondary hover:bg-kiviElevated hover:text-kiviInk'
                  }`}
                >
                  <button
                    type="button"
                    role="tab"
                    aria-selected={isActive}
                    onClick={() => onActivateTab(tab.id)}
                    className="flex items-center gap-1.5 whitespace-nowrap"
                    title={isSplit ? `${tab.title} (Split Pane)` : tab.title}
                  >
                    <span className={isActive ? 'text-kiviAccent' : 'text-kiviMuted'}>
                      <TabIcon kind={tab.kind} />
                    </span>
                    <span className="max-w-[180px] truncate">{tab.title}</span>
                  </button>
                  <button
                    type="button"
                    aria-label={tab.pinned ? `Unpin ${tab.title}` : `Pin ${tab.title}`}
                    onClick={() => onTogglePinTab(tab.id)}
                    className={`rounded p-0.5 hover:bg-kiviElevated ${
                      tab.pinned ? 'text-kiviAccent' : 'text-kiviMuted opacity-50 hover:opacity-100'
                    }`}
                    title={tab.pinned ? 'Unpin tab' : 'Pin tab'}
                  >
                    <Pin className="h-2.5 w-2.5" />
                  </button>
                  {onDetachTab &&
                    (tab.kind === 'map-table' ||
                      tab.kind === 'map-3d' ||
                      tab.kind === 'map-split' ||
                      tab.kind === 'hex' ||
                      tab.kind === 'waveform') && (
                      <button
                        type="button"
                        data-testid={`detach-tab-${tab.id}`}
                        aria-label={`Detach ${tab.title}`}
                        onClick={() => onDetachTab(tab.id)}
                        className="rounded p-0.5 text-kiviMuted opacity-55 transition-colors hover:bg-kiviElevated hover:text-kiviAccent hover:opacity-100"
                        title="Detach tab into floating window"
                      >
                        <PictureInPicture2 className="h-2.5 w-2.5" />
                      </button>
                    )}
                  {!tab.pinned && tab.closable !== false && (
                    <button
                      type="button"
                      aria-label={`Close ${tab.title}`}
                      onClick={() => onCloseTab(tab.id)}
                      className="rounded p-0.5 text-kiviMuted hover:bg-kiviElevated hover:text-kiviInk"
                      title="Close tab (Ctrl+W)"
                    >
                      <X className="h-3 w-3" />
                    </button>
                  )}
                </div>
              );
            })
          )}
        </div>

        {tabStrip.hasOverflow && (
          <button
            type="button"
            aria-label="Scroll tabs right"
            disabled={!tabStrip.canRight}
            onClick={() => tabStrip.scrollByDelta(180)}
            className="flex h-7 w-5 shrink-0 items-center justify-center rounded border-0 bg-transparent text-kiviMuted transition-colors hover:text-kiviInk active:scale-95 disabled:pointer-events-none disabled:opacity-25"
          >
            <ChevronRight className="h-3.5 w-3.5" />
          </button>
        )}

        {/* Right Quick Workspace Controls */}
        <div className="flex shrink-0 items-center gap-1">
          <button
            type="button"
            onClick={onToggleSplitView}
            className={`hidden shrink-0 items-center gap-1 whitespace-nowrap rounded-md border px-2 py-0.5 text-[11px] transition-colors md:flex ${
              splitTabId
                ? 'border-kiviAccent bg-kiviAccent/15 text-kiviAccent font-medium'
                : 'border-kiviBorder/70 text-kiviSecondary hover:bg-kiviElevated hover:text-kiviInk'
            }`}
            title="Toggle side-by-side Split Workspace"
          >
            <Columns className="h-3 w-3 shrink-0" />
            <span>Split</span>
          </button>
          <button
            type="button"
            onClick={onOpenCommandPalette}
            className="hidden shrink-0 whitespace-nowrap rounded-md border border-kiviBorder/70 bg-kiviElevated px-2 py-0.5 font-mono text-[10px] text-kiviMuted hover:text-kiviInk lg:inline-flex"
            title="Open Command Palette (Ctrl+K)"
          >
            Ctrl+K
          </button>
        </div>
      </div>
    </div>
  );
});
