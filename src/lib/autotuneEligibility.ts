import type { MapDef } from '../types/kivi.ts';
import type {
  AutoTuneConfig,
  CellRecommendation,
  RecommendationReport,
  AutoTuneResponse,
} from '../types/correlation.ts';

/**
 * Canonical reason documenting why ROM mutation is disabled in Milestone 3C.
 */
export const PATH_B_MUTATION_BLOCKER_REASON =
  'Transactional application disabled: repository currently lacks atomic ROM transaction and undo/rollback API in Tauri AppState (Milestone 3C Path B safety contract)';

/**
 * Result of evaluating whether a calibration table is eligible for AFR auto-tuning.
 */
export interface MapEligibilityResult {
  isEligible: boolean;
  blockerReason: string | null;
}

/**
 * Evaluates whether a calibration table is an eligible fuel-delivery map.
 * Strictly blocks non-fuel maps (Ignition, Boost, Knock, Dwell, Target AFR, etc.).
 */
export function evaluateMapEligibility(mapDef: MapDef | null): MapEligibilityResult {
  if (!mapDef) {
    return {
      isEligible: false,
      blockerReason: 'No calibration map selected',
    };
  }

  const category = (mapDef.category ?? '').trim().toLowerCase();
  const name = (mapDef.name ?? '').trim().toLowerCase();

  // Ignition tables
  if (category.includes('ignition') || name.includes('ignition') || name.includes('spark')) {
    return {
      isEligible: false,
      blockerReason: 'Category "Ignition" is incompatible with AFR auto-tune. Only Fuel delivery tables are supported.',
    };
  }

  // Boost tables
  if (category.includes('boost') || name.includes('boost')) {
    return {
      isEligible: false,
      blockerReason: 'Category "Boost" is incompatible with AFR auto-tune. Only Fuel delivery tables are supported.',
    };
  }

  // Knock tables
  if (category.includes('knock') || name.includes('knock')) {
    return {
      isEligible: false,
      blockerReason: 'Category "Knock" is incompatible with AFR auto-tune. Only Fuel delivery tables are supported.',
    };
  }

  // Dwell tables
  if (category.includes('dwell') || name.includes('dwell')) {
    return {
      isEligible: false,
      blockerReason: 'Category "Dwell" is incompatible with AFR auto-tune. Only Fuel delivery tables are supported.',
    };
  }

  // Target AFR tables (this is the reference target, not delivery)
  if (category.includes('targetafr') || category.includes('target_afr') || name.includes('target afr') || name.includes('lambda target')) {
    return {
      isEligible: false,
      blockerReason: 'Category "Target AFR" is incompatible with AFR auto-tune (serves as reference target, not fuel delivery).',
    };
  }

  // Only Fuel category is permitted
  if (category !== 'fuel') {
    return {
      isEligible: false,
      blockerReason: `Category "${mapDef.category}" is incompatible with AFR auto-tune. Only Fuel delivery tables are supported.`,
    };
  }

  return {
    isEligible: true,
    blockerReason: null,
  };
}

/**
 * Result of evaluating an individual cell recommendation.
 */
export interface CellEligibilityResult {
  isEligible: boolean;
  isClamped: boolean;
  reason?: string;
}

/**
 * Evaluates whether an individual cell recommendation meets strict correctness,
 * statistical confidence, and coordinate boundary rules.
 */
export function evaluateCellEligibility(
  rec: CellRecommendation,
  mapDef: MapDef,
  config: AutoTuneConfig
): CellEligibilityResult {
  // 1. Coordinate boundaries
  if (
    rec.row < 0 ||
    rec.row >= mapDef.rows ||
    rec.col < 0 ||
    rec.col >= mapDef.cols ||
    !Number.isInteger(rec.row) ||
    !Number.isInteger(rec.col)
  ) {
    return {
      isEligible: false,
      isClamped: false,
      reason: `Out of bounds cell coordinate (${rec.row}, ${rec.col}) for ${mapDef.rows}x${mapDef.cols} map`,
    };
  }

  // 2. Numerical validity (no NaN or Infinities)
  if (
    !Number.isFinite(rec.proposed_value) ||
    !Number.isFinite(rec.delta_percentage) ||
    !Number.isFinite(rec.original_value)
  ) {
    return {
      isEligible: false,
      isClamped: false,
      reason: 'Non-finite numerical recommendation (NaN or Infinity)',
    };
  }

  // 3. Statistical status
  if (rec.status === 'InsufficientEvidence') {
    return {
      isEligible: false,
      isClamped: false,
      reason: `Insufficient evidence (sample count ${rec.sample_count} < min ${config.min_samples_per_cell})`,
    };
  }

  // 4. Maximum delta bounds check
  if (Math.abs(rec.delta_percentage) > config.max_cell_delta_pct + 1e-4) {
    return {
      isEligible: false,
      isClamped: true,
      reason: `Delta ${rec.delta_percentage.toFixed(1)}% exceeds configured clamp limit ±${config.max_cell_delta_pct}%`,
    };
  }

  return {
    isEligible: true,
    isClamped: rec.is_clamped,
  };
}

/**
 * Freshness check parameters.
 */
export interface FreshnessCheckParams {
  report: RecommendationReport | null;
  currentMap: MapDef | null;
  currentMatrix: number[][] | null;
  activeReportResponse: AutoTuneResponse | null;
}

/**
 * Freshness evaluation result.
 */
export interface FreshnessResult {
  isFresh: boolean;
  staleReason: string | null;
}

/**
 * Checks whether an Auto-Tune report is fresh and matches the active table and cell values.
 */
export function checkReportFreshness(params: FreshnessCheckParams): FreshnessResult {
  const { report, currentMap, currentMatrix } = params;

  if (!report) {
    return { isFresh: false, staleReason: 'No recommendation report available' };
  }

  if (!currentMap) {
    return { isFresh: false, staleReason: 'No calibration table selected' };
  }

  if (report.table_id !== currentMap.id) {
    return {
      isFresh: false,
      staleReason: `Table ID mismatch: report is for "${report.table_id}", active table is "${currentMap.id}"`,
    };
  }

  if (report.blocker_reason) {
    return {
      isFresh: false,
      staleReason: `Report is blocked: ${report.blocker_reason}`,
    };
  }

  // Verify that targeted cells have not been modified since recommendation was calculated
  if (currentMatrix && report.cells.length > 0) {
    for (const rec of report.cells) {
      if (
        rec.row >= 0 &&
        rec.row < currentMatrix.length &&
        rec.col >= 0 &&
        rec.col < (currentMatrix[rec.row]?.length ?? 0)
      ) {
        const liveVal = currentMatrix[rec.row][rec.col];
        if (Math.abs(liveVal - rec.original_value) > 1e-3) {
          return {
            isFresh: false,
            staleReason: `Cell values modified in active table since recommendation was calculated (Cell [${rec.row}, ${rec.col}]: was ${rec.original_value}, now ${liveVal})`,
          };
        }
      }
    }
  }

  return { isFresh: true, staleReason: null };
}

/**
 * Parameters for batch recommendation validation.
 */
export interface BatchValidationParams {
  selectedCoords: { row: number; col: number }[];
  recommendations: CellRecommendation[];
  mapDef: MapDef;
  config: AutoTuneConfig;
}

/**
 * Result of batch validation.
 */
export interface BatchValidationResult {
  isValid: boolean;
  eligibleCount: number;
  ineligibleCount: number;
  rejectionReasons: string[];
}

/**
 * Validates an entire batch of selected recommendations before application.
 * All selected cells must be eligible; mixed batches are rejected.
 */
export function validateRecommendationBatch(
  params: BatchValidationParams
): BatchValidationResult {
  const { selectedCoords, recommendations, mapDef, config } = params;

  let eligibleCount = 0;
  let ineligibleCount = 0;
  const rejectionReasons: string[] = [];

  for (const coord of selectedCoords) {
    const rec = recommendations.find(
      (r) => r.row === coord.row && r.col === coord.col
    );

    if (!rec) {
      ineligibleCount++;
      rejectionReasons.push(`No recommendation found for coordinate (${coord.row}, ${coord.col})`);
      continue;
    }

    const cellRes = evaluateCellEligibility(rec, mapDef, config);
    if (cellRes.isEligible) {
      eligibleCount++;
    } else {
      ineligibleCount++;
      if (cellRes.reason) {
        rejectionReasons.push(`Cell [${coord.row}, ${coord.col}]: ${cellRes.reason}`);
      }
    }
  }

  const isValid = selectedCoords.length > 0 && ineligibleCount === 0;

  return {
    isValid,
    eligibleCount,
    ineligibleCount,
    rejectionReasons,
  };
}

/**
 * Path classification for transactional application support.
 * Returns false under Path B with documented architectural blocker.
 */
export function isTransactionalApplicationSupported(): {
  supported: boolean;
  blockerReason: string;
} {
  return {
    supported: false,
    blockerReason: PATH_B_MUTATION_BLOCKER_REASON,
  };
}
