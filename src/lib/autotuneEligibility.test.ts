import { test } from 'node:test';
import assert from 'node:assert/strict';
import type { MapDef } from '../types/kivi.ts';
import type {
  CellRecommendation,
  RecommendationReport,
} from '../types/correlation.ts';
import { DEFAULT_AUTOTUNE_CONFIG } from '../types/correlation.ts';
import {
  evaluateMapEligibility,
  evaluateCellEligibility,
  validateRecommendationBatch,
  checkReportFreshness,
  isTransactionalApplicationSupported,
  PATH_B_MUTATION_BLOCKER_REASON,
} from './autotuneEligibility.ts';

function createTestMap(category = 'Fuel'): MapDef {
  return {
    id: 'fuel_main_ve',
    name: 'Main VE Fuel Table',
    category,
    address: 0x1000,
    rows: 4,
    cols: 4,
    dtype: 'u8',
    endian: 'little',
    scaling: { decimals: 1, equation: 'X', unit: '%' },
  };
}

function createCellRecommendation(overrides: Partial<CellRecommendation> = {}): CellRecommendation {
  return {
    row: 1,
    col: 2,
    status: 'Ready',
    sample_count: 25,
    effective_sample_count: 24.5,
    original_value: 80.0,
    proposed_value: 86.4,
    delta_percentage: 8.0,
    unit: '%',
    mean_measured_afr: 16.0,
    target_afr: 14.7,
    std_dev_afr: 0.45,
    min_measured_afr: 15.5,
    max_measured_afr: 16.5,
    is_clamped: false,
    warning: null,
    ...overrides,
  };
}

function createReport(cells: CellRecommendation[] = []): RecommendationReport {
  return {
    table_id: 'fuel_main_ve',
    total_frames_scanned: 100,
    eligible_frames_used: 80,
    cells,
    rejections: {
      rejected_stale: 5,
      rejected_fault: 0,
      rejected_out_of_bounds: 2,
      rejected_cold_engine: 8,
      rejected_fuel_cut: 3,
      rejected_transient_throttle: 2,
      rejected_sensor_out_of_range: 0,
      rejected_non_finite: 0,
    },
    blocker_reason: null,
  };
}

// ---------------------------------------------------------------------------
// 10. Map category eligibility: Fuel only; Ignition, Boost, Knock, Dwell, Target AFR blocked
// ---------------------------------------------------------------------------
test('autotuneEligibility: 10. permits Fuel tables and blocks Ignition, Boost, Knock, Dwell, TargetAfr', () => {
  // Fuel table is eligible
  const fuelMap = createTestMap('Fuel');
  const fuelResult = evaluateMapEligibility(fuelMap);
  assert.strictEqual(fuelResult.isEligible, true);
  assert.strictEqual(fuelResult.blockerReason, null);

  // Incompatible table categories
  const blockedCategories = ['Ignition', 'Boost', 'Knock', 'Dwell', 'TargetAfr', 'Transmission'];
  for (const cat of blockedCategories) {
    const map = createTestMap(cat);
    const res = evaluateMapEligibility(map);
    assert.strictEqual(res.isEligible, false, `Category ${cat} must be blocked`);
    assert.ok(res.blockerReason?.includes(cat) || res.blockerReason?.includes('incompatible'));
  }
});

// ---------------------------------------------------------------------------
// 11. Cell coordinate validation: out-of-bounds rows and columns are rejected
// ---------------------------------------------------------------------------
test('autotuneEligibility: 11. out-of-bounds coordinates are rejected', () => {
  const map = createTestMap(); // 4x4 matrix: valid indices 0..3

  // Row out of bounds
  const negRow = createCellRecommendation({ row: -1, col: 0 });
  const resNegRow = evaluateCellEligibility(negRow, map, DEFAULT_AUTOTUNE_CONFIG);
  assert.strictEqual(resNegRow.isEligible, false);
  assert.ok(resNegRow.reason?.includes('Out of bounds'));

  const overflowRow = createCellRecommendation({ row: 4, col: 0 });
  const resOverflowRow = evaluateCellEligibility(overflowRow, map, DEFAULT_AUTOTUNE_CONFIG);
  assert.strictEqual(resOverflowRow.isEligible, false);

  // Col out of bounds
  const overflowCol = createCellRecommendation({ row: 0, col: 5 });
  const resOverflowCol = evaluateCellEligibility(overflowCol, map, DEFAULT_AUTOTUNE_CONFIG);
  assert.strictEqual(resOverflowCol.isEligible, false);
});

// ---------------------------------------------------------------------------
// 12. Non-finite values: NaN, +Infinity, -Infinity are rejected
// ---------------------------------------------------------------------------
test('autotuneEligibility: 12. non-finite values (NaN, Infinity) are rejected', () => {
  const map = createTestMap();

  const nanProposed = createCellRecommendation({ proposed_value: NaN });
  assert.strictEqual(evaluateCellEligibility(nanProposed, map, DEFAULT_AUTOTUNE_CONFIG).isEligible, false);

  const infProposed = createCellRecommendation({ proposed_value: Infinity });
  assert.strictEqual(evaluateCellEligibility(infProposed, map, DEFAULT_AUTOTUNE_CONFIG).isEligible, false);

  const nanDelta = createCellRecommendation({ delta_percentage: NaN });
  assert.strictEqual(evaluateCellEligibility(nanDelta, map, DEFAULT_AUTOTUNE_CONFIG).isEligible, false);

  const nanOrig = createCellRecommendation({ original_value: NaN });
  assert.strictEqual(evaluateCellEligibility(nanOrig, map, DEFAULT_AUTOTUNE_CONFIG).isEligible, false);
});

// ---------------------------------------------------------------------------
// 4. Insufficient evidence results cannot be selected for application
// ---------------------------------------------------------------------------
test('autotuneEligibility: 4. insufficient evidence results are marked ineligible', () => {
  const map = createTestMap();
  const insufficient = createCellRecommendation({
    status: 'InsufficientEvidence',
    sample_count: 2,
    proposed_value: 80.0,
    delta_percentage: 0.0,
  });

  const res = evaluateCellEligibility(insufficient, map, DEFAULT_AUTOTUNE_CONFIG);
  assert.strictEqual(res.isEligible, false);
  assert.ok(res.reason?.includes('Insufficient evidence'));
});

// ---------------------------------------------------------------------------
// 5. Clamped recommendations show status but enforce bounds
// ---------------------------------------------------------------------------
test('autotuneEligibility: 5. clamped recommendations within allowed config remain eligible but flagged', () => {
  const map = createTestMap();
  const clamped = createCellRecommendation({
    is_clamped: true,
    delta_percentage: 15.0, // exactly at max_cell_delta_pct
    warning: 'Delta clamped to +15.0%',
  });

  const res = evaluateCellEligibility(clamped, map, DEFAULT_AUTOTUNE_CONFIG);
  assert.strictEqual(res.isEligible, true);
  assert.strictEqual(res.isClamped, true);

  // Delta exceeding configured clamp is rejected
  const overClamped = createCellRecommendation({
    delta_percentage: 25.0, // exceeds max_cell_delta_pct 15.0
  });
  const resOver = evaluateCellEligibility(overClamped, map, DEFAULT_AUTOTUNE_CONFIG);
  assert.strictEqual(resOver.isEligible, false);
});

// ---------------------------------------------------------------------------
// 14. Table ID mismatch prevents application
// ---------------------------------------------------------------------------
test('autotuneEligibility: 14. table ID mismatch invalidates report freshness', () => {
  const mapA = createTestMap();
  mapA.id = 'table_A';

  const reportB = createReport();
  reportB.table_id = 'table_B';

  const freshness = checkReportFreshness({
    report: reportB,
    currentMap: mapA,
    currentMatrix: [[80, 80], [80, 80]],
    activeReportResponse: {
      table_id: 'table_B',
      source_provenance: 'live',
      report: reportB,
    },
  });

  assert.strictEqual(freshness.isFresh, false);
  assert.ok(freshness.staleReason?.includes('Table ID mismatch'));
});

// ---------------------------------------------------------------------------
// 16. Changed targeted cell value makes recommendation stale
// ---------------------------------------------------------------------------
test('autotuneEligibility: 16. modified cell value in matrix invalidates cell freshness', () => {
  const map = createTestMap();
  const rec = createCellRecommendation({
    row: 1,
    col: 2,
    original_value: 80.0,
  });
  const report = createReport([rec]);

  // Current matrix has value 95.0 at (1, 2) instead of original 80.0
  const modifiedMatrix = [
    [80, 80, 80, 80],
    [80, 80, 95.0, 80],
    [80, 80, 80, 80],
    [80, 80, 80, 80],
  ];

  const freshness = checkReportFreshness({
    report,
    currentMap: map,
    currentMatrix: modifiedMatrix,
    activeReportResponse: {
      table_id: map.id,
      source_provenance: 'live',
      report,
    },
  });

  assert.strictEqual(freshness.isFresh, false);
  assert.ok(freshness.staleReason?.includes('Cell values modified'));
});

// ---------------------------------------------------------------------------
// 18. Batch validation rejects entire batch if any cell is ineligible
// ---------------------------------------------------------------------------
test('autotuneEligibility: 18. batch validation rejects entire batch if any selected cell is ineligible', () => {
  const map = createTestMap();
  const validCell = createCellRecommendation({ row: 0, col: 0, proposed_value: 85.0 });
  const invalidCell = createCellRecommendation({
    row: 1,
    col: 1,
    status: 'InsufficientEvidence',
    sample_count: 1,
  });

  const batchValidation = validateRecommendationBatch({
    selectedCoords: [
      { row: 0, col: 0 },
      { row: 1, col: 1 },
    ],
    recommendations: [validCell, invalidCell],
    mapDef: map,
    config: DEFAULT_AUTOTUNE_CONFIG,
  });

  assert.strictEqual(batchValidation.isValid, false);
  assert.strictEqual(batchValidation.eligibleCount, 1);
  assert.strictEqual(batchValidation.ineligibleCount, 1);
  assert.ok(batchValidation.rejectionReasons.length > 0);
});

// ---------------------------------------------------------------------------
// Path B safety constraint: Transactional application is disabled with explicit blocker
// ---------------------------------------------------------------------------
test('autotuneEligibility: Path B - transactional mutation is disabled and reports canonical blocker', () => {
  const status = isTransactionalApplicationSupported();
  assert.strictEqual(status.supported, false);
  assert.strictEqual(status.blockerReason, PATH_B_MUTATION_BLOCKER_REASON);
  assert.ok(status.blockerReason.includes('Path B'));
  assert.ok(status.blockerReason.includes('rollback'));
});
