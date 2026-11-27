import type { MapDef } from '../../types/kivi.ts';
import type {
  AutoTuneConfig,
  CellRecommendation,
  RecommendationReport,
} from '../../types/correlation.ts';
import {
  evaluateMapEligibility,
  evaluateCellEligibility,
} from '../../lib/autotuneEligibility.ts';

export type ReviewStatusBadgeType =
  | 'Ready'
  | 'NoChange'
  | 'Clamped'
  | 'HighVariance'
  | 'InsufficientEvidence'
  | 'BlockedCategory'
  | 'Invalid';

export interface StatusBadgeViewModel {
  status: ReviewStatusBadgeType;
  label: string;
  badgeClass: string;
}

export interface CellRecommendationRowViewModel {
  row: number;
  col: number;
  coordinateLabel: string;
  currentValueStr: string;
  proposedValueStr: string;
  deltaPercentageStr: string;
  sampleCountStr: string;
  measuredAfrStr: string;
  targetAfrStr: string;
  stdDevStr: string;
  isEligible: boolean;
  canSelect: boolean;
  isSelected: boolean;
  statusBadge: StatusBadgeViewModel;
  warningText: string | null;
}

/**
 * Formats an AFR value strictly, distinguishing unavailable (null) from numeric zero.
 */
export function formatAfrValue(val: number | null | undefined): string {
  if (val === null || val === undefined) {
    return '—';
  }
  if (!Number.isFinite(val)) {
    return '—';
  }
  return `${val.toFixed(2)}:1`;
}

/**
 * Formats a CellRecommendation into a comprehensive View Model for table rendering.
 */
export function formatCellRecommendationRow(params: {
  rec: CellRecommendation;
  mapDef: MapDef;
  config: AutoTuneConfig;
  isSelected: boolean;
}): CellRecommendationRowViewModel {
  const { rec, mapDef, config, isSelected } = params;

  const mapEligibility = evaluateMapEligibility(mapDef);
  const cellEligibility = evaluateCellEligibility(rec, mapDef, config);

  const isEligible = mapEligibility.isEligible && cellEligibility.isEligible;
  const canSelect = isEligible && rec.status !== 'InsufficientEvidence';

  let statusBadge: StatusBadgeViewModel;
  if (!mapEligibility.isEligible) {
    statusBadge = {
      status: 'BlockedCategory',
      label: 'Blocked: Incompatible Map',
      badgeClass: 'bg-kiviRed/10 text-kiviRed border border-kiviRed/40',
    };
  } else if (rec.status === 'InsufficientEvidence') {
    statusBadge = {
      status: 'InsufficientEvidence',
      label: 'Insufficient Evidence',
      badgeClass: 'bg-kiviElevated text-kiviMuted border border-kiviBorder/70',
    };
  } else if (rec.status === 'NoChange') {
    statusBadge = {
      status: 'NoChange',
      label: 'On Target (No Change)',
      badgeClass: 'bg-kiviElevated text-kiviGreen border border-kiviBorder/70',
    };
  } else if (rec.is_clamped) {
    statusBadge = {
      status: 'Clamped',
      label: `Clamped (${rec.delta_percentage >= 0 ? '+' : ''}${rec.delta_percentage.toFixed(1)}%)`,
      badgeClass: 'bg-kiviWarn/10 text-kiviWarn border border-kiviWarn/40',
    };
  } else if (rec.status === 'HighVarianceWarning') {
    statusBadge = {
      status: 'HighVariance',
      label: 'High Variance Warning',
      badgeClass: 'bg-kiviWarn/15 text-kiviWarn border border-kiviWarn/40',
    };
  } else {
    statusBadge = {
      status: 'Ready',
      label: 'Ready for Review',
      badgeClass: 'bg-kiviGreen/10 text-kiviGreen border border-kiviGreen/40',
    };
  }

  return {
    row: rec.row,
    col: rec.col,
    coordinateLabel: `Row ${rec.row}, Col ${rec.col}`,
    currentValueStr: `${rec.original_value.toFixed(1)} ${rec.unit}`,
    proposedValueStr: `${rec.proposed_value.toFixed(1)} ${rec.unit}`,
    deltaPercentageStr: `${rec.delta_percentage >= 0 ? '+' : ''}${rec.delta_percentage.toFixed(1)}%`,
    sampleCountStr: `${rec.sample_count}`,
    measuredAfrStr: formatAfrValue(rec.mean_measured_afr),
    targetAfrStr: formatAfrValue(rec.target_afr),
    stdDevStr: `±${rec.std_dev_afr.toFixed(2)}`,
    isEligible,
    canSelect,
    isSelected,
    statusBadge,
    warningText: rec.warning ?? (cellEligibility.reason ?? null),
  };
}

export interface ReviewSummaryStats {
  totalCells: number;
  eligibleCount: number;
  clampedCount: number;
  insufficientCount: number;
  selectedCount: number;
  expectedDeltaRangeStr: string;
}

/**
 * Computes aggregate summary metrics across the report.
 */
export function getReviewSummaryStats(params: {
  report: RecommendationReport | null;
  mapDef: MapDef | null;
  selectedCoords: { row: number; col: number }[];
  config: AutoTuneConfig;
}): ReviewSummaryStats {
  const { report, mapDef, selectedCoords, config } = params;

  if (!report || !mapDef) {
    return {
      totalCells: 0,
      eligibleCount: 0,
      clampedCount: 0,
      insufficientCount: 0,
      selectedCount: 0,
      expectedDeltaRangeStr: '—',
    };
  }

  let eligibleCount = 0;
  let clampedCount = 0;
  let insufficientCount = 0;

  const selectedDeltas: number[] = [];

  for (const rec of report.cells) {
    const el = evaluateCellEligibility(rec, mapDef, config);
    if (el.isEligible) {
      eligibleCount++;
    }
    if (rec.is_clamped) {
      clampedCount++;
    }
    if (rec.status === 'InsufficientEvidence') {
      insufficientCount++;
    }

    const isSel = selectedCoords.some((c) => c.row === rec.row && c.col === rec.col);
    if (isSel && el.isEligible) {
      selectedDeltas.push(rec.delta_percentage);
    }
  }

  let expectedDeltaRangeStr = '—';
  if (selectedDeltas.length > 0) {
    const min = Math.min(...selectedDeltas);
    const max = Math.max(...selectedDeltas);
    expectedDeltaRangeStr = `${min >= 0 ? '+' : ''}${min.toFixed(1)}% to ${max >= 0 ? '+' : ''}${max.toFixed(1)}%`;
  }

  return {
    totalCells: report.cells.length,
    eligibleCount,
    clampedCount,
    insufficientCount,
    selectedCount: selectedCoords.length,
    expectedDeltaRangeStr,
  };
}

/**
 * Builds WAI-ARIA accessible dialog attributes.
 */
export function getDialogAriaAttributes(titleId: string, descId?: string) {
  return {
    role: 'dialog' as const,
    'aria-modal': 'true' as const,
    'aria-labelledby': titleId,
    'aria-describedby': descId,
  };
}
