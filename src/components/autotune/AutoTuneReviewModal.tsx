import React, { useState, useEffect, useMemo, useCallback } from 'react';
import {
  SlidersHorizontal,
  AlertTriangle,
  ShieldAlert,
  Check,
  Info,
  Filter,
  CheckSquare,
  Square,
  RefreshCw,
} from 'lucide-react';
import { FloatingWindow } from '../layout/FloatingWindow';
import { TOOL_WINDOW_SPECS } from '../layout/toolWindowRegistry';
import { useCorrelationStore } from '../../stores/useCorrelationStore.ts';
import { useMapStore } from '../../stores/useMapStore.ts';
import {
  evaluateMapEligibility,
  evaluateCellEligibility,
  checkReportFreshness,
  isTransactionalApplicationSupported,
} from '../../lib/autotuneEligibility.ts';
import {
  formatCellRecommendationRow,
  getReviewSummaryStats,
  getDialogAriaAttributes,
} from './autoTuneReviewModel.ts';

export interface AutoTuneReviewModalProps {
  isOpen: boolean;
  onClose: () => void;
  standalone?: boolean;
}

export type TableFilterMode = 'all' | 'eligible' | 'clamped' | 'insufficient' | 'no_change';

export const AutoTuneReviewModal: React.FC<AutoTuneReviewModalProps> = ({
  isOpen,
  onClose,
  standalone = false,
}) => {
  // Store subscriptions
  const activeReport = useCorrelationStore((s) => s.activeReport);
  const activeReportResponse = useCorrelationStore((s) => s.activeReportResponse);
  const autoTuneStatus = useCorrelationStore((s) => s.autoTuneStatus);
  const autoTuneConfig = useCorrelationStore((s) => s.autoTuneConfig);
  const targetMapDef = useCorrelationStore((s) => s.targetMapDef);
  const computeAutoTuneRecommendations = useCorrelationStore(
    (s) => s.computeAutoTuneRecommendations
  );

  const activeMatrix = useMapStore((s) => s.activeMatrix);
  const selectedMapId = useMapStore((s) => s.selectedMapId);
  const maps = useMapStore((s) => s.maps);

  const activeMapDef = useMemo(() => {
    return targetMapDef ?? maps.find((m) => m.id === selectedMapId) ?? null;
  }, [targetMapDef, maps, selectedMapId]);

  // Local state
  const [selectedCoords, setSelectedCoords] = useState<{ row: number; col: number }[]>([]);
  const [filterMode, setFilterMode] = useState<TableFilterMode>('all');
  const [isRefreshing, setIsRefreshing] = useState<boolean>(false);

  // Map eligibility check
  const mapEligibility = useMemo(() => {
    return evaluateMapEligibility(activeMapDef);
  }, [activeMapDef]);

  // Report freshness check
  const freshness = useMemo(() => {
    return checkReportFreshness({
      report: activeReport,
      currentMap: activeMapDef,
      currentMatrix: activeMatrix,
      activeReportResponse,
    });
  }, [activeReport, activeMapDef, activeMatrix, activeReportResponse]);

  // Path B mutation status
  const mutationSupport = useMemo(() => {
    return isTransactionalApplicationSupported();
  }, []);

  // Summary metrics
  const stats = useMemo(() => {
    return getReviewSummaryStats({
      report: activeReport,
      mapDef: activeMapDef,
      selectedCoords,
      config: autoTuneConfig,
    });
  }, [activeReport, activeMapDef, selectedCoords, autoTuneConfig]);

  // Auto-select all eligible cells when report arrives
  useEffect(() => {
    if (activeReport && activeMapDef && mapEligibility.isEligible) {
      const eligible: { row: number; col: number }[] = [];
      for (const rec of activeReport.cells) {
        const el = evaluateCellEligibility(rec, activeMapDef, autoTuneConfig);
        if (el.isEligible && rec.status !== 'InsufficientEvidence') {
          eligible.push({ row: rec.row, col: rec.col });
        }
      }
      setSelectedCoords(eligible);
    } else {
      setSelectedCoords([]);
    }
  }, [activeReport, activeMapDef, mapEligibility.isEligible, autoTuneConfig]);

  // Toggle selection for an individual cell
  const handleToggleCell = useCallback((row: number, col: number) => {
    setSelectedCoords((prev) => {
      const exists = prev.some((c) => c.row === row && c.col === col);
      if (exists) {
        return prev.filter((c) => !(c.row === row && c.col === col));
      } else {
        return [...prev, { row, col }];
      }
    });
  }, []);

  // Bulk selection actions
  const handleSelectAllEligible = useCallback(() => {
    if (!activeReport || !activeMapDef) return;
    const eligible: { row: number; col: number }[] = [];
    for (const rec of activeReport.cells) {
      const el = evaluateCellEligibility(rec, activeMapDef, autoTuneConfig);
      if (el.isEligible && rec.status !== 'InsufficientEvidence') {
        eligible.push({ row: rec.row, col: rec.col });
      }
    }
    setSelectedCoords(eligible);
  }, [activeReport, activeMapDef, autoTuneConfig]);

  const handleDeselectAll = useCallback(() => {
    setSelectedCoords([]);
  }, []);

  // Re-compute recommendations
  const handleRefreshRecommendations = async () => {
    setIsRefreshing(true);
    try {
      await computeAutoTuneRecommendations();
    } catch (err) {
      console.warn('AutoTune refresh error:', err);
    } finally {
      setIsRefreshing(false);
    }
  };

  // Row formatting and filtering
  const formattedRows = useMemo(() => {
    if (!activeReport || !activeMapDef) return [];

    return activeReport.cells
      .map((rec) => {
        const isSelected = selectedCoords.some(
          (c) => c.row === rec.row && c.col === rec.col
        );
        return formatCellRecommendationRow({
          rec,
          mapDef: activeMapDef,
          config: autoTuneConfig,
          isSelected,
        });
      })
      .filter((row) => {
        if (filterMode === 'eligible') return row.isEligible;
        if (filterMode === 'clamped') return row.statusBadge.status === 'Clamped';
        if (filterMode === 'insufficient') return row.statusBadge.status === 'InsufficientEvidence';
        if (filterMode === 'no_change') return row.statusBadge.status === 'NoChange';
        return true;
      });
  }, [activeReport, activeMapDef, autoTuneConfig, selectedCoords, filterMode]);

  if (!isOpen) return null;

  const dialogAria = getDialogAriaAttributes(
    'autotune-dialog-title',
    'autotune-dialog-desc'
  );

  const spec = TOOL_WINDOW_SPECS.autotune;

  return (
    <FloatingWindow
      windowId="autotune"
      isOpen={isOpen}
      onClose={onClose}
      title="AFR Auto-Tune Assistant"
      titleId={dialogAria['aria-labelledby']}
      ariaDescribedBy={dialogAria['aria-describedby']}
      badge={activeMapDef?.name ?? activeReport?.table_id ?? 'No Map'}
      icon={<SlidersHorizontal className="h-4 w-4" />}
      defaultWidth={spec.width}
      defaultHeight={spec.height}
      minWidth={spec.minWidth}
      minHeight={spec.minHeight}
      dock={spec.dock}
      standalone={standalone}
      popoutToolId="autotune"
      closeLabel="Close dialog"
      headerActions={
        <button
          type="button"
          onClick={handleRefreshRecommendations}
          disabled={isRefreshing || autoTuneStatus === 'loading'}
          className="px-2 py-1 rounded bg-kiviElevated hover:bg-kiviBorderStrong text-kiviSecondary hover:text-kiviInk border border-kiviBorderStrong text-[10px] flex items-center gap-1 transition-colors disabled:opacity-50"
          title="Re-compute Auto-Tune analysis"
        >
          <RefreshCw className={`h-3 w-3 ${isRefreshing ? 'animate-spin' : ''}`} />
          <span className="hidden sm:inline">Re-analyze</span>
        </button>
      }
      footer={
        <div className="p-3 bg-kiviBg border-t border-kiviBorder flex flex-wrap items-center justify-between gap-2 text-xs shrink-0">
          <div className="flex-1 min-w-[200px] space-y-1">
            <div className="flex flex-wrap items-center gap-2">
              <span className="font-semibold text-kiviInk">
                Selected for Review:{' '}
                <span className="text-kiviWarn font-mono font-bold">
                  {stats.selectedCount}
                </span>{' '}
                of {stats.eligibleCount} eligible cells
              </span>
              <span className="text-kiviMuted">&bull;</span>
              <span className="text-kiviMuted">
                Change Range: <strong className="text-kiviInk font-mono">{stats.expectedDeltaRangeStr}</strong>
              </span>
            </div>
            <p className="text-[11px] text-kiviWarn flex items-center gap-1.5 leading-relaxed">
              <Info className="h-3.5 w-3.5 flex-shrink-0" />
              <span>{mutationSupport.blockerReason}</span>
            </p>
          </div>
          <div className="flex items-center gap-2 shrink-0">
            <button
              type="button"
              onClick={onClose}
              className="px-3 py-1.5 rounded-lg bg-kiviElevated hover:bg-kiviBorderStrong text-kiviSecondary hover:text-kiviInk border border-kiviBorderStrong font-medium transition-colors"
            >
              Close
            </button>
            <button
              type="button"
              disabled
              title={mutationSupport.blockerReason}
              className="px-3 py-1.5 rounded-lg bg-kiviWarn/15 text-kiviWarn border border-kiviWarn/40 font-semibold cursor-not-allowed flex items-center gap-1.5"
            >
              <Check className="h-3.5 w-3.5" />
              <span>Apply Changes (Disabled)</span>
            </button>
          </div>
        </div>
      }
    >
      <div className="flex min-h-0 flex-1 flex-col overflow-hidden text-kiviInk font-mono">
        <div className="shrink-0 border-b border-kiviBorder bg-kiviSurface px-4 py-1.5">
          <p id="autotune-dialog-desc" className="text-[11px] text-kiviMuted">
            Statistical closed-loop AFR analysis &bull; Source:{' '}
            <span className="text-kiviInk font-medium capitalize">
              {activeReportResponse?.source_provenance ?? 'live/snapshot'}
            </span>{' '}
            &bull; Scanned:{' '}
            <span className="text-kiviInk font-mono">
              {activeReport?.total_frames_scanned ?? 0}
            </span>{' '}
            frames ({activeReport?.eligible_frames_used ?? 0} eligible)
          </p>
        </div>

        {/* Alerts & Rejections Banner */}
        <div className="p-4 space-y-3 bg-kiviBg border-b border-kiviBorder">
          {!mapEligibility.isEligible && (
            <div className="p-3 rounded-lg bg-kiviRed/10 border border-kiviRed/40 text-kiviRed text-xs flex items-center gap-2.5">
              <ShieldAlert className="h-4 w-4 text-kiviRed flex-shrink-0" />
              <span>{mapEligibility.blockerReason}</span>
            </div>
          )}

          {!freshness.isFresh && (
            <div className="p-3 rounded-lg bg-kiviWarn/10 border border-kiviWarn/40 text-kiviWarn text-xs flex items-center gap-2.5">
              <AlertTriangle className="h-4 w-4 text-kiviWarn flex-shrink-0" />
              <span>{freshness.staleReason}</span>
            </div>
          )}

          {activeReport && (
            <div className="flex flex-wrap items-center gap-2 text-[11px] text-kiviMuted">
              <span className="font-semibold text-kiviSecondary">Filter Rejections:</span>
              <span className="px-2 py-0.5 rounded bg-kiviElevated border border-kiviBorderStrong">
                Cold Engine: <strong className="text-kiviInk">{activeReport.rejections.rejected_cold_engine}</strong>
              </span>
              <span className="px-2 py-0.5 rounded bg-kiviElevated border border-kiviBorderStrong">
                Transient dTPS: <strong className="text-kiviInk">{activeReport.rejections.rejected_transient_throttle}</strong>
              </span>
              <span className="px-2 py-0.5 rounded bg-kiviElevated border border-kiviBorderStrong">
                Fuel Cut: <strong className="text-kiviInk">{activeReport.rejections.rejected_fuel_cut}</strong>
              </span>
              <span className="px-2 py-0.5 rounded bg-kiviElevated border border-kiviBorderStrong">
                Out of Bounds: <strong className="text-kiviInk">{activeReport.rejections.rejected_out_of_bounds}</strong>
              </span>
              <span className="px-2 py-0.5 rounded bg-kiviElevated border border-kiviBorderStrong">
                Stale/Fault: <strong className="text-kiviInk">{activeReport.rejections.rejected_stale + activeReport.rejections.rejected_fault}</strong>
              </span>
            </div>
          )}
        </div>

        {/* Filter Controls & Bulk Selection Bar */}
        <div className="px-4 py-2.5 bg-kiviSurface border-b border-kiviBorder flex flex-wrap items-center justify-between gap-3 text-xs">
          <div className="flex items-center gap-1.5">
            <span className="text-kiviMuted font-medium mr-1 flex items-center gap-1">
              <Filter className="h-3.5 w-3.5 text-kiviMuted" /> Filter:
            </span>
            {(['all', 'eligible', 'clamped', 'insufficient', 'no_change'] as TableFilterMode[]).map((mode) => (
              <button
                key={mode}
                onClick={() => setFilterMode(mode)}
                className={`px-2.5 py-1 rounded text-xs transition-colors capitalize ${
                  filterMode === mode
                    ? 'bg-kiviAccent text-kiviAccentFg font-medium'
                    : 'bg-kiviElevated text-kiviSecondary hover:bg-kiviBorderStrong'
                }`}
              >
                {mode === 'all'
                  ? `All (${stats.totalCells})`
                  : mode === 'eligible'
                  ? `Eligible (${stats.eligibleCount})`
                  : mode === 'clamped'
                  ? `Clamped (${stats.clampedCount})`
                  : mode === 'insufficient'
                  ? `Insufficient (${stats.insufficientCount})`
                  : 'On Target'}
              </button>
            ))}
          </div>

          <div className="flex items-center gap-2">
            <button
              onClick={handleSelectAllEligible}
              disabled={!mapEligibility.isEligible || stats.eligibleCount === 0}
              className="px-2.5 py-1 rounded bg-kiviElevated hover:bg-kiviBorderStrong text-kiviSecondary hover:text-kiviInk border border-kiviBorderStrong flex items-center gap-1.5 transition-colors disabled:opacity-40"
            >
              <CheckSquare className="h-3.5 w-3.5 text-kiviGreen" />
              <span>Select All Eligible</span>
            </button>
            <button
              onClick={handleDeselectAll}
              disabled={selectedCoords.length === 0}
              className="px-2.5 py-1 rounded bg-kiviElevated hover:bg-kiviBorderStrong text-kiviSecondary hover:text-kiviInk border border-kiviBorderStrong flex items-center gap-1.5 transition-colors disabled:opacity-40"
            >
              <Square className="h-3.5 w-3.5 text-kiviMuted" />
              <span>Deselect All</span>
            </button>
          </div>
        </div>

        {/* Recommendation Rows Table */}
        <div className="flex-1 overflow-auto max-h-[460px] p-2">
          {formattedRows.length === 0 ? (
            <div className="flex flex-col items-center justify-center p-12 text-kiviMuted">
              <Info className="h-8 w-8 mb-2 opacity-50" />
              <p className="text-sm">No cell recommendations match the selected filter.</p>
            </div>
          ) : (
            <table className="w-full text-xs text-left border-collapse font-mono">
              <thead>
                <tr className="border-b border-kiviBorder text-kiviMuted bg-kiviBg text-[11px]">
                  <th className="p-2 w-10 text-center">Approve</th>
                  <th className="p-2">Coordinate</th>
                  <th className="p-2 text-right">Current</th>
                  <th className="p-2 text-right">Target AFR</th>
                  <th className="p-2 text-right">Mean AFR</th>
                  <th className="p-2 text-right">Std Dev</th>
                  <th className="p-2 text-right">Proposed</th>
                  <th className="p-2 text-right">Delta %</th>
                  <th className="p-2 text-right">Samples</th>
                  <th className="p-2 text-center">Status / Warning</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-kiviBorder">
                {formattedRows.map((row) => {
                  const isChecked = selectedCoords.some(
                    (c) => c.row === row.row && c.col === row.col
                  );

                  return (
                    <tr
                      key={`${row.row}-${row.col}`}
                      className={`hover:bg-kiviElevated transition-colors ${
                        isChecked ? 'bg-kiviWarn/10' : ''
                      }`}
                    >
                      <td className="p-2 text-center">
                        <input
                          type="checkbox"
                          checked={isChecked}
                          disabled={!row.canSelect}
                          onChange={() => handleToggleCell(row.row, row.col)}
                          className="h-4 w-4 cursor-pointer rounded border-kiviBorderStrong bg-kiviBg accent-kiviWarn focus-visible:ring-2 focus-visible:ring-kiviWarn/30 disabled:cursor-not-allowed disabled:opacity-30"
                          aria-label={`Select recommendation for ${row.coordinateLabel}`}
                        />
                      </td>
                      <td className="p-2 font-medium text-kiviInk">
                        {row.coordinateLabel}
                      </td>
                      <td className="p-2 text-right text-kiviMuted">
                        {row.currentValueStr}
                      </td>
                      <td className="p-2 text-right text-kiviSecondary">
                        {row.targetAfrStr}
                      </td>
                      <td className="p-2 text-right text-kiviInk font-semibold">
                        {row.measuredAfrStr}
                      </td>
                      <td className="p-2 text-right text-kiviMuted text-[11px]">
                        {row.stdDevStr}
                      </td>
                      <td className="p-2 text-right text-kiviWarn font-bold">
                        {row.proposedValueStr}
                      </td>
                      <td className="p-2 text-right">
                        <span
                          className={`font-semibold ${
                            parseFloat(row.deltaPercentageStr) > 0
                              ? 'text-kiviGreen'
                              : parseFloat(row.deltaPercentageStr) < 0
                              ? 'text-kiviRed'
                              : 'text-kiviMuted'
                          }`}
                        >
                          {row.deltaPercentageStr}
                        </span>
                      </td>
                      <td className="p-2 text-right text-kiviSecondary">
                        {row.sampleCountStr}
                      </td>
                      <td className="p-2 text-center">
                        <span
                          className={`inline-block px-2 py-0.5 text-[10px] rounded font-mono font-medium ${row.statusBadge.badgeClass}`}
                          title={row.warningText ?? row.statusBadge.label}
                        >
                          {row.statusBadge.label}
                        </span>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          )}
        </div>
      </div>
    </FloatingWindow>
  );
};
