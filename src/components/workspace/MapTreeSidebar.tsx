import React, { useEffect, useMemo, useState } from 'react';
import {
  ChevronRight,
  ChevronsDownUp,
  ChevronsUpDown,
  Columns,
  FileCode,
  Folder,
  FolderOpen,
  Pencil,
  Plus,
  Radar,
  Trash2,
  X,
} from 'lucide-react';
import {
  buildMapFolderTree,
  collectFolderPaths,
  countMapsByParamType,
  filterMapTree,
  filterMapTreeByParamType,
  findFolderPathsForMap,
  type MapFolderNode,
  type MapParamTypeFilter,
} from '../editor';
import type { DetectedMapCandidate, MapDef } from '../../types/kivi';

type MapTreeGroups = ReturnType<typeof filterMapTree>;

interface MapTreeSidebarProps {
  totalMaps: number;
  mapsCount: number;
  mapFilter: string;
  onChangeMapFilter: (value: string) => void;
  mapTree: MapTreeGroups;
  selectedMapId: string | null;
  onSelectMap: (id: string) => void;
  onOpenMapInSplit?: (id: string) => void;
  onCreateMap?: () => void;
  onEditMap?: (map: MapDef) => void;
  onDeleteMap?: (id: string) => void;
  potentialMaps?: DetectedMapCandidate[];
  onInspectPotentialInHex?: (address: number) => void;
  onPromotePotentialMap?: (candidate: DetectedMapCandidate) => void;
  onClose?: () => void;
  animState?: 'open' | 'closing';
}

interface FolderBranchProps {
  node: MapFolderNode;
  depth: number;
  expandedPaths: Set<string>;
  forceExpandAll: boolean;
  onToggleFolder: (path: string) => void;
  selectedMapId: string | null;
  onSelectMap: (id: string) => void;
  onOpenMapInSplit?: (id: string) => void;
  onEditMap?: (map: MapDef) => void;
  onDeleteMap?: (id: string) => void;
}

const FolderBranch: React.FC<FolderBranchProps> = ({
  node,
  depth,
  expandedPaths,
  forceExpandAll,
  onToggleFolder,
  selectedMapId,
  onSelectMap,
  onOpenMapInSplit,
  onEditMap,
  onDeleteMap,
}) => {
  const isOpen = forceExpandAll || expandedPaths.has(node.path);

  return (
    <div className={depth > 0 ? 'ml-2.5 border-l border-kiviBorder/70 pl-1.5' : ''}>
      <button
        type="button"
        onClick={() => onToggleFolder(node.path)}
        aria-expanded={isOpen}
        className="flex w-full items-center justify-between gap-1.5 rounded px-1.5 py-1 text-left text-[11px] font-medium text-kiviInk transition-colors hover:bg-kiviElevated"
      >
        <span className="flex min-w-0 items-center gap-1.5">
          <ChevronRight
            className={`h-3 w-3 shrink-0 text-kiviMuted transition-transform duration-150 ${
              isOpen ? 'rotate-90' : ''
            }`}
          />
          {isOpen ? (
            <FolderOpen className="h-3.5 w-3.5 shrink-0 text-kiviAccent" />
          ) : (
            <Folder className="h-3.5 w-3.5 shrink-0 text-kiviWarn" />
          )}
          <span className="truncate">{node.name}</span>
        </span>
        <span className="shrink-0 rounded bg-kiviBg px-1.5 py-0.5 font-mono text-[10px] text-kiviMuted tabular-nums">
          {node.totalCount}
        </span>
      </button>

      {isOpen && (
        <div className="kivi-anim-reveal mt-0.5 space-y-0.5">
          {node.folders.map((sub) => (
            <FolderBranch
              key={sub.id}
              node={sub}
              depth={depth + 1}
              expandedPaths={expandedPaths}
              forceExpandAll={forceExpandAll}
              onToggleFolder={onToggleFolder}
              selectedMapId={selectedMapId}
              onSelectMap={onSelectMap}
              onOpenMapInSplit={onOpenMapInSplit}
              onEditMap={onEditMap}
              onDeleteMap={onDeleteMap}
            />
          ))}

          {node.maps.length > 0 && (
            <ul className="space-y-0.5 pl-3">
              {node.maps.map((map) => {
                const isSelected = map.id === selectedMapId;
                const addrHex = `0x${map.address.toString(16).toUpperCase()}`;
                return (
                  <li key={map.id} className="group flex items-center gap-0.5">
                    <button
                      type="button"
                      onClick={() => onSelectMap(map.id)}
                      title={`${map.name} (${addrHex}, ${map.rows}x${map.cols})`}
                      className={`flex min-w-0 flex-1 items-center justify-between gap-1 rounded px-2 py-1 text-left text-xs transition-colors ${
                        isSelected
                          ? 'border border-kiviAccent/40 bg-kiviAccent/15 font-medium text-kiviAccent'
                          : 'border border-transparent text-kiviSecondary hover:bg-kiviElevated hover:text-kiviInk'
                      }`}
                    >
                      <span className="truncate">{map.name}</span>
                      <span className="shrink-0 font-mono text-[10px] text-kiviMuted tabular-nums">
                        {map.rows}&times;{map.cols}
                      </span>
                    </button>
                    <div className="flex shrink-0 items-center opacity-0 transition-opacity group-hover:opacity-100 focus-within:opacity-100">
                      {onOpenMapInSplit && (
                        <button
                          type="button"
                          onClick={() => onOpenMapInSplit(map.id)}
                          aria-label={`Open ${map.name} in split pane`}
                          title="Open in split pane"
                          className="rounded p-1 text-kiviMuted hover:bg-kiviElevated hover:text-kiviInk"
                        >
                          <Columns className="h-3 w-3" />
                        </button>
                      )}
                      {onEditMap && (
                        <button
                          type="button"
                          onClick={() => onEditMap(map)}
                          aria-label={`Edit properties of ${map.name}`}
                          title="Map properties"
                          className="rounded p-1 text-kiviMuted hover:bg-kiviElevated hover:text-kiviAccent"
                        >
                          <Pencil className="h-3 w-3" />
                        </button>
                      )}
                      {onDeleteMap && (
                        <button
                          type="button"
                          onClick={() => onDeleteMap(map.id)}
                          aria-label={`Remove ${map.name}`}
                          title="Delete map definition"
                          className="rounded p-1 text-kiviMuted hover:bg-kiviElevated hover:text-kiviRed"
                        >
                          <Trash2 className="h-3 w-3" />
                        </button>
                      )}
                    </div>
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      )}
    </div>
  );
};

export const MapTreeSidebar: React.FC<MapTreeSidebarProps> = React.memo(({
  totalMaps,
  mapsCount,
  mapFilter,
  onChangeMapFilter,
  mapTree,
  selectedMapId,
  onSelectMap,
  onOpenMapInSplit,
  onCreateMap,
  onEditMap,
  onDeleteMap,
  potentialMaps = [],
  onInspectPotentialInHex,
  onPromotePotentialMap,
  onClose,
  animState = 'open',
}) => {
  const [paramTypeFilter, setParamTypeFilter] = useState<MapParamTypeFilter>('all');
  const [potentialOpen, setPotentialOpen] = useState<boolean>(false);
  const [expandedPaths, setExpandedPaths] = useState<Set<string>>(() => new Set());

  const flatFilteredMaps = useMemo(() => mapTree.flatMap((g) => g.maps), [mapTree]);
  const typeCounts = useMemo(() => countMapsByParamType(flatFilteredMaps), [flatFilteredMaps]);
  const visibleTree = useMemo(
    () => filterMapTreeByParamType(mapTree, paramTypeFilter),
    [mapTree, paramTypeFilter]
  );
  const folderTree = useMemo(() => buildMapFolderTree(visibleTree), [visibleTree]);

  // Expand small top-level trees automatically on load; keep large category trees tidy & collapsed.
  const folderSignature = useMemo(
    () => folderTree.map((f) => f.path).join('|'),
    [folderTree]
  );
  useEffect(() => {
    setExpandedPaths((prev) => {
      if (folderTree.length === 0) return prev;
      if (prev.size === 0 && folderTree.length <= 6) {
        return new Set(folderTree.map((f) => f.path));
      }
      return prev;
    });
  }, [folderSignature, folderTree]);

  // Always keep ancestor folders of the active map expanded cleanly.
  useEffect(() => {
    const ancestors = findFolderPathsForMap(folderTree, selectedMapId);
    if (ancestors.length === 0) return;
    setExpandedPaths((prev) => {
      let changed = false;
      const next = new Set(prev);
      for (const path of ancestors) {
        if (!next.has(path)) {
          next.add(path);
          changed = true;
        }
      }
      return changed ? next : prev;
    });
  }, [folderTree, selectedMapId]);

  const handleToggleFolder = (path: string) => {
    setExpandedPaths((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  };

  const handleExpandAll = () => {
    setExpandedPaths(new Set(collectFolderPaths(folderTree)));
  };

  const handleCollapseAll = () => {
    setExpandedPaths(new Set());
  };

  const filterChips: Array<{ id: MapParamTypeFilter; label: string }> = [
    { id: 'all', label: 'All' },
    { id: '2d', label: '2D' },
    { id: '1d', label: '1D' },
    { id: 'scalar', label: '1x1' },
  ];

  const forceExpandAll = mapFilter.trim().length > 0;

  return (
    <aside
      className={`absolute inset-y-0 left-0 md:left-11 z-40 flex w-[min(85vw,16.5rem)] flex-col overflow-hidden rounded-none kivi-panel-surface bg-kiviSurface lg:static lg:inset-y-auto lg:left-auto lg:z-auto lg:w-64 lg:shrink-0 lg:shadow-none lg:border-r lg:border-kiviBorder/60 xl:w-72 ${
        animState === 'closing' ? 'kivi-anim-slide-left-out' : 'kivi-anim-slide-left-in'
      }`}
    >
      <div
        data-testid="panel-titlebar-map-explorer"
        className="flex h-9 shrink-0 items-center justify-between gap-2 bg-kiviSurface px-3 text-xs font-semibold uppercase tracking-wider text-kiviSecondary"
      >
        <span className="min-w-0 truncate whitespace-nowrap">Map Explorer</span>
        <div className="flex shrink-0 items-center gap-1">
          <span className="mr-0.5 font-mono text-[11px] font-normal text-kiviMuted tabular-nums">
            {totalMaps}
          </span>
          {folderTree.length > 0 && (
            <>
              <button
                type="button"
                onClick={handleExpandAll}
                aria-label="Expand all folders"
                title="Expand all folders"
                className="flex h-6 w-6 shrink-0 items-center justify-center rounded bg-kiviBg/70 text-kiviMuted transition-colors hover:bg-kiviElevated hover:text-kiviInk"
              >
                <ChevronsUpDown className="h-3 w-3" />
              </button>
              <button
                type="button"
                onClick={handleCollapseAll}
                aria-label="Collapse all folders"
                title="Collapse all folders"
                className="flex h-6 w-6 shrink-0 items-center justify-center rounded bg-kiviBg/70 text-kiviMuted transition-colors hover:bg-kiviElevated hover:text-kiviInk"
              >
                <ChevronsDownUp className="h-3 w-3" />
              </button>
            </>
          )}
          {onCreateMap && (
            <button
              type="button"
              onClick={onCreateMap}
              aria-label="Create new map definition"
              title="Create new map definition"
              className="flex h-6 w-6 shrink-0 items-center justify-center rounded bg-kiviBg/70 text-kiviSecondary transition-colors hover:bg-kiviElevated hover:text-kiviAccent"
            >
              <Plus className="h-3.5 w-3.5" />
            </button>
          )}
          {onClose && (
            <button
              type="button"
              aria-label="Close map explorer"
              onClick={onClose}
              className="flex h-6 w-6 shrink-0 items-center justify-center rounded text-kiviMuted transition-colors hover:bg-kiviElevated hover:text-kiviInk lg:hidden"
            >
              <X className="h-3.5 w-3.5" />
            </button>
          )}
        </div>
      </div>

      <div className="space-y-1.5 border-b border-kiviBorder/50 px-2.5 pb-2 pt-1">
        <input
          type="text"
          aria-label="Filter maps"
          placeholder="Filter maps..."
          value={mapFilter}
          onChange={(e) => onChangeMapFilter(e.target.value)}
          className="w-full rounded border border-kiviBorder/70 bg-kiviBg px-2 py-1 text-[11px] text-kiviInk placeholder:text-kiviMuted focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-kiviAccent"
        />
        <div
          role="group"
          aria-label="Filter by parameter dimension"
          className="grid grid-cols-4 gap-1 text-[10px]"
        >
          {filterChips.map((chip) => {
            const active = paramTypeFilter === chip.id;
            return (
              <button
                key={chip.id}
                type="button"
                onClick={() => setParamTypeFilter(chip.id)}
                className={`rounded border px-1 py-0.5 font-mono transition-colors tabular-nums whitespace-nowrap ${
                  active
                    ? 'border-kiviAccent/60 bg-kiviAccent/15 font-semibold text-kiviAccent'
                    : 'border-kiviBorder bg-kiviBg text-kiviMuted hover:text-kiviInk'
                }`}
              >
                {chip.label} {typeCounts[chip.id]}
              </button>
            );
          })}
        </div>
      </div>

      <div className="flex-1 overflow-y-auto p-2 space-y-2">
        {potentialMaps.length > 0 && (
          <div
            data-testid="explorer-potential-maps"
            className="rounded-lg border border-kiviWarn/40 bg-kiviWarn/5 p-1.5"
          >
            <button
              type="button"
              onClick={() => setPotentialOpen((v) => !v)}
              className="flex w-full items-center justify-between px-1 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-kiviWarn"
            >
              <span className="flex items-center gap-1">
                <ChevronRight
                  className={`h-3 w-3 shrink-0 transition-transform duration-150 ${
                    potentialOpen ? 'rotate-90' : ''
                  }`}
                />
                <Radar className="h-3 w-3 shrink-0" />
                <span>Candidate Regions</span>
              </span>
              <span className="font-mono tabular-nums">{potentialMaps.length}</span>
            </button>

            {potentialOpen && (
              <ul className="kivi-anim-reveal mt-1 max-h-40 space-y-0.5 overflow-y-auto">
                {potentialMaps.slice(0, 30).map((cand, idx) => (
                  <li
                    key={`${cand.id}-${cand.start_addr}-${idx}`}
                    className="group flex items-center gap-1 rounded border border-transparent px-1.5 py-0.5 text-[11px] hover:border-kiviWarn/40 hover:bg-kiviBg"
                  >
                    <button
                      type="button"
                      onClick={() => onInspectPotentialInHex?.(cand.start_addr)}
                      onDoubleClick={() => onPromotePotentialMap?.(cand)}
                      title={`Click to inspect in Hex (0x${cand.start_addr.toString(16).toUpperCase()}), Double-click or + to add`}
                      className="flex min-w-0 flex-1 items-center justify-between gap-1 text-left text-kiviSecondary hover:text-kiviInk"
                    >
                      <span className="truncate">{cand.name_hint}</span>
                      <span className="shrink-0 font-mono text-[9px] text-kiviWarn tabular-nums">
                        {cand.rows}&times;{cand.cols}
                      </span>
                    </button>
                    {onPromotePotentialMap && (
                      <button
                        type="button"
                        aria-label={`Promote ${cand.name_hint} to map`}
                        title="Add candidate to Map Explorer"
                        onClick={() => onPromotePotentialMap(cand)}
                        className="shrink-0 rounded p-0.5 text-kiviWarn opacity-75 hover:bg-kiviWarn/20 hover:opacity-100"
                      >
                        <Plus className="h-3 w-3" />
                      </button>
                    )}
                  </li>
                ))}
              </ul>
            )}
          </div>
        )}

        {folderTree.length === 0 ? (
          <div className="flex flex-col items-center justify-center p-6 text-center text-xs text-kiviMuted">
            <FileCode className="mb-2 h-8 w-8 text-kiviMuted opacity-50" />
            <p>{mapsCount === 0 ? 'No map definitions loaded.' : 'No map matches the current filter.'}</p>
            <p className="mt-1 text-[11px] text-kiviMuted">
              {mapsCount === 0 ? 'Import XDF, A2L, INI, or run Auto-Scan from the Activity Rail.' : ''}
            </p>
          </div>
        ) : (
          <div className="space-y-1">
            {folderTree.map((folder) => (
              <FolderBranch
                key={folder.id}
                node={folder}
                depth={0}
                expandedPaths={expandedPaths}
                forceExpandAll={forceExpandAll}
                onToggleFolder={handleToggleFolder}
                selectedMapId={selectedMapId}
                onSelectMap={onSelectMap}
                onOpenMapInSplit={onOpenMapInSplit}
                onEditMap={onEditMap}
                onDeleteMap={onDeleteMap}
              />
            ))}
          </div>
        )}
      </div>
    </aside>
  );
});
