import React from 'react';
import {
  Activity,
  Binary,
  Box,
  Columns,
  FileCode2,
  GitCompare,
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

export const WorkspaceTabBar: React.FC<WorkspaceTabBarProps> = ({
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
  onToggleSplitView,
  onOpenCommandPalette,
}) => {
  return (
    <div className="shrink-0 select-none border-b border-kiviBorder bg-kiviSurface">
      {/* Multi-ROM Document Project Strip */}
      {romDocuments.length > 0 && (
        <div className="no-scrollbar flex h-7 items-center gap-1 overflow-x-auto border-b border-kiviBorder/70 bg-kiviBg px-2 text-[11px]">
          <span className="mr-1 shrink-0 whitespace-nowrap text-[10px] font-semibold uppercase tracking-wider text-kiviMuted">
            ROMs
          </span>
          {romDocuments.map((doc) => {
            const isActive = doc.id === activeDocId || doc.is_active;
            return (
              <div
                key={doc.id}
                className={`group flex shrink-0 items-center gap-1.5 whitespace-nowrap rounded border px-2 py-0.5 transition-colors ${
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
            className="ml-1 flex shrink-0 items-center gap-1 whitespace-nowrap rounded border border-kiviBorder px-1.5 py-0.5 text-[10px] text-kiviSecondary hover:border-kiviAccent/50 hover:text-kiviInk"
            title="Open another BIN/ROM tab side-by-side"
          >
            <Plus className="h-3 w-3 shrink-0" />
            <span>Open ROM</span>
          </button>
        </div>
      )}

      {/* Workspace Editor Tabs Bar */}
      <div className="flex h-8 items-center justify-between gap-2 px-2">
        <div className="no-scrollbar flex min-w-0 flex-1 items-center gap-1 overflow-x-auto">
          {tabs.length === 0 ? (
            <span className="truncate px-2 text-[11px] text-kiviMuted">
              No open tabs. Select a map or launch Hex / Waveform / Scanner.
            </span>
          ) : (
            tabs.map((tab) => {
              const isActive = tab.id === activeTabId;
              const isSplit = tab.id === splitTabId;
              return (
                <div
                  key={tab.id}
                  onMouseDown={(e) => {
                    if (e.button === 1 && !tab.pinned && tab.closable !== false) {
                      e.preventDefault();
                      onCloseTab(tab.id);
                    }
                  }}
                  className={`flex shrink-0 items-center gap-1.5 whitespace-nowrap rounded-t border-b-2 px-2.5 py-1 text-xs transition-colors ${
                    isActive
                      ? 'border-kiviAccent bg-kiviBg text-kiviInk font-medium'
                      : isSplit
                      ? 'border-kiviWarn bg-kiviElevated text-kiviInk'
                      : 'border-transparent text-kiviSecondary hover:bg-kiviElevated hover:text-kiviInk'
                  }`}
                >
                  <button
                    type="button"
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

        {/* Right Quick Workspace Controls */}
        <div className="flex shrink-0 items-center gap-1">
          <button
            type="button"
            onClick={onToggleSplitView}
            className={`hidden shrink-0 items-center gap-1 whitespace-nowrap rounded border px-2 py-0.5 text-[11px] transition-colors md:flex ${
              splitTabId
                ? 'border-kiviAccent bg-kiviAccent/15 text-kiviAccent font-medium'
                : 'border-kiviBorder text-kiviSecondary hover:bg-kiviElevated hover:text-kiviInk'
            }`}
            title="Toggle side-by-side Split Workspace"
          >
            <Columns className="h-3 w-3 shrink-0" />
            <span>Split</span>
          </button>
          <button
            type="button"
            onClick={onOpenCommandPalette}
            className="hidden shrink-0 whitespace-nowrap rounded border border-kiviBorder bg-kiviElevated px-2 py-0.5 font-mono text-[10px] text-kiviMuted hover:text-kiviInk lg:inline-flex"
            title="Open Command Palette (Ctrl+K)"
          >
            Ctrl+K
          </button>
        </div>
      </div>
    </div>
  );
};
