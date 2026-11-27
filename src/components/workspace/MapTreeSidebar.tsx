import React, { useMemo, useState } from 'react';
import { ChevronDown, ChevronRight, Columns, FileCode, Pencil, Plus, Radar, Trash2, X } from 'lucide-react';
import {
  countMapsByParamType,
  filterMapTree,
  filterMapTreeByParamType,
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
}

export const MapTreeSidebar: React.FC<MapTreeSidebarProps> = ({
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
}) => {
  const [paramTypeFilter, setParamTypeFilter] = useState<MapParamTypeFilter>('all');
  const [potentialOpen, setPotentialOpen] = useState<boolean>(true);

  const flatFilteredMaps = useMemo(() => mapTree.flatMap((g) => g.maps), [mapTree]);
  const typeCounts = useMemo(() => countMapsByParamType(flatFilteredMaps), [flatFilteredMaps]);
  const visibleTree = useMemo(
    () => filterMapTreeByParamType(mapTree, paramTypeFilter),
    [mapTree, paramTypeFilter]
  );

  const filterChips: Array<{ id: MapParamTypeFilter; label: string }> = [
    { id: 'all', label: 'All' },
    { id: '2d', label: '2D' },
    { id: '1d', label: '1D' },
    { id: 'scalar', label: '1x1' },
  ];

  return (
  <aside className="fixed inset-y-0 left-0 md:left-11 z-40 flex w-[min(85vw,16rem)] flex-col border-r border-kiviBorder bg-kiviSurface shadow-2xl lg:static lg:left-auto lg:z-auto lg:w-60 lg:shrink-0 lg:shadow-none xl:w-64">
    <div className="flex items-center justify-between border-b border-kiviBorder px-3 py-2 text-xs font-semibold uppercase tracking-wider text-kiviSecondary">
      <span className="whitespace-nowrap">Map Explorer</span>
      <div className="flex items-center gap-1.5">
        <span className="font-mono text-[11px] font-normal text-kiviMuted tabular-nums">
          {totalMaps}
        </span>
        {onCreateMap && (
          <button
            type="button"
            onClick={onCreateMap}
            aria-label="Create new map definition"
            title="Create new map definition"
            className="flex h-6 w-6 items-center justify-center rounded border border-kiviBorder bg-kiviBg text-kiviSecondary transition-colors hover:border-kiviAccent/60 hover:text-kiviAccent"
          >
            <Plus className="h-3.5 w-3.5" />
          </button>
        )}
        {onClose && (
          <button
            type="button"
            aria-label="Close map explorer"
            onClick={onClose}
            className="flex min-h-[36px] min-w-[36px] items-center justify-center rounded text-kiviMuted hover:bg-kiviElevated hover:text-kiviInk lg:hidden"
          >
            <X className="h-4 w-4" />
          </button>
        )}
      </div>
    </div>

    <div className="space-y-1.5 border-b border-kiviBorder px-2 py-1.5">
      <input
        type="text"
        aria-label="Filter maps"
        placeholder="Filter maps..."
        value={mapFilter}
        onChange={(e) => onChangeMapFilter(e.target.value)}
        className="w-full rounded border border-kiviBorderStrong bg-kiviBg px-2 py-1 text-[11px] text-kiviInk placeholder:text-kiviMuted focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-kiviAccent"
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

    <div className="flex-1 overflow-y-auto p-2 space-y-2.5">
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
              {potentialOpen ? (
                <ChevronDown className="h-3 w-3 shrink-0" />
              ) : (
                <ChevronRight className="h-3 w-3 shrink-0" />
              )}
              <Radar className="h-3 w-3 shrink-0" />
              <span>Potential Maps</span>
            </span>
            <span className="font-mono tabular-nums">{potentialMaps.length}</span>
          </button>

          {potentialOpen && (
            <ul className="mt-1 max-h-40 space-y-0.5 overflow-y-auto">
              {potentialMaps.slice(0, 30).map((cand, idx) => (
                <li
                  key={`${cand.id}-${cand.start_addr}-${idx}`}
                  className="group flex items-center gap-1 rounded border border-transparent px-1.5 py-0.5 text-[11px] hover:border-kiviWarn/40 hover:bg-kiviBg"
                >
                  <button
                    type="button"
                    onClick={() => onInspectPotentialInHex?.(cand.start_addr)}
                    onDoubleClick={() => onPromotePotentialMap?.(cand)}
                    title={`Click to inspect in Hex (0x${cand.start_addr.toString(16).toUpperCase()}), Double-click or + to promote`}
                    className="flex min-w-0 flex-1 items-center justify-between gap-1 text-left text-kiviSecondary hover:text-kiviInk"
                  >
                    <span className="truncate">{cand.name_hint}</span>
                    <span className="shrink-0 font-mono text-[9px] text-kiviWarn tabular-nums">
                      {cand.rows}x{cand.cols}
                    </span>
                  </button>
                  {onPromotePotentialMap && (
                    <button
                      type="button"
                      aria-label={`Promote ${cand.name_hint} to map`}
                      title="Convert potential map to editable table"
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

      {visibleTree.length === 0 ? (
        <div className="flex flex-col items-center justify-center p-6 text-center text-xs text-kiviMuted">
          <FileCode className="mb-2 h-8 w-8 text-kiviMuted opacity-50" />
          <p>{mapsCount === 0 ? 'No map definitions loaded.' : 'No map matches the current filter.'}</p>
          <p className="mt-1 text-[11px] text-kiviMuted">
            {mapsCount === 0 ? 'Import XDF, A2L, INI, or use Auto-Scan from the Activity Rail.' : ''}
          </p>
        </div>
      ) : (
        <div className="space-y-2">
          {visibleTree.map((group) => (
            <div key={group.category}>
              <div className="flex items-center justify-between px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-kiviMuted">
                <span className="truncate">{group.category}</span>
                <span className="tabular-nums">{group.maps.length}</span>
              </div>
              <ul className="space-y-0.5">
                {group.maps.map((map) => {
                  const isSelected = map.id === selectedMapId;
                  return (
                    <li key={map.id} className="group flex items-center gap-0.5">
                      <button
                        type="button"
                        onClick={() => onSelectMap(map.id)}
                        className={`flex min-w-0 flex-1 items-center justify-between rounded px-2 py-1 text-left text-xs transition-colors ${
                          isSelected
                            ? 'border border-kiviAccent/40 bg-kiviAccent/15 font-medium text-kiviAccent'
                            : 'border border-transparent text-kiviSecondary hover:bg-kiviElevated hover:text-kiviInk'
                        }`}
                      >
                        <span className="truncate">{map.name}</span>
                        <span className="ml-1 shrink-0 font-mono text-[10px] text-kiviMuted tabular-nums">
                          {map.rows}x{map.cols}
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
            </div>
          ))}
        </div>
      )}
    </div>
  </aside>
  );
};
