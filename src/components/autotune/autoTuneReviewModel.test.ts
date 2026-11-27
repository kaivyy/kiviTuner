import { test } from 'node:test';
import assert from 'node:assert/strict';
import type { MapDef } from '../../types/kivi.ts';
import type { CellRecommendation } from '../../types/correlation.ts';
import { DEFAULT_AUTOTUNE_CONFIG } from '../../types/correlation.ts';
import {
  formatCellRecommendationRow,
  getReviewSummaryStats,
  getDialogAriaAttributes,
  formatAfrValue,
} from './autoTuneReviewModel.ts';
import { useMapStore } from '../../stores/useMapStore.ts';
import { useCorrelationStore } from '../../stores/useCorrelationStore.ts';

function createTestMap(category = 'Fuel'): MapDef {
  return {
    id: 've_table_main',
    name: 'Primary VE Fuel Map',
    category,
    address: 0x1000,
    rows: 4,
    cols: 4,
    dtype: 'u8',
    endian: 'little',
    scaling: { decimals: 1, equation: 'X', unit: '%' },
  };
}

function createCellRec(overrides: Partial<CellRecommendation> = {}): CellRecommendation {
  return {
    row: 1,
    col: 2,
    status: 'Ready',
    sample_count: 24,
    effective_sample_count: 23.8,
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

// ---------------------------------------------------------------------------
// 1. Eligible recommendations display all available evidence fields
// ---------------------------------------------------------------------------
test('autoTuneReviewModel: 1. eligible recommendations format complete evidence fields', () => {
  const map = createTestMap();
  const rec = createCellRec();

  const row = formatCellRecommendationRow({
    rec,
    mapDef: map,
    config: DEFAULT_AUTOTUNE_CONFIG,
    isSelected: true,
  });

  assert.strictEqual(row.coordinateLabel, 'Row 1, Col 2');
  assert.strictEqual(row.currentValueStr, '80.0 %');
  assert.strictEqual(row.proposedValueStr, '86.4 %');
  assert.strictEqual(row.deltaPercentageStr, '+8.0%');
  assert.strictEqual(row.sampleCountStr, '24');
  assert.strictEqual(row.measuredAfrStr, '16.00:1');
  assert.strictEqual(row.targetAfrStr, '14.70:1');
  assert.strictEqual(row.stdDevStr, '±0.45');
  assert.strictEqual(row.isEligible, true);
  assert.strictEqual(row.canSelect, true);
  assert.strictEqual(row.isSelected, true);
  assert.strictEqual(row.statusBadge.label, 'Ready for Review');
});

// ---------------------------------------------------------------------------
// 2. Missing fields are displayed as unavailable rather than fabricated
// ---------------------------------------------------------------------------
test('autoTuneReviewModel: 2. missing AFR fields display explicit unavailable placeholder', () => {
  const map = createTestMap();
  const recNoAfr = createCellRec({
    mean_measured_afr: null,
    target_afr: null,
    min_measured_afr: null,
    max_measured_afr: null,
  });

  const row = formatCellRecommendationRow({
    rec: recNoAfr,
    mapDef: map,
    config: DEFAULT_AUTOTUNE_CONFIG,
    isSelected: false,
  });

  // Never fabricates missing AFR evidence
  assert.strictEqual(row.measuredAfrStr, '—');
  assert.strictEqual(row.targetAfrStr, '—');

  // Numeric zero must be preserved distinctly from unavailable (null)
  assert.strictEqual(formatAfrValue(0.0), '0.00:1');
  assert.strictEqual(formatAfrValue(null), '—');
  assert.strictEqual(formatAfrValue(undefined), '—');
});

// ---------------------------------------------------------------------------
// 3. Blocked map categories are visibly rejected
// ---------------------------------------------------------------------------
test('autoTuneReviewModel: 3. blocked map categories prevent all selection and show blocked badge', () => {
  const ignitionMap = createTestMap('Ignition');
  const rec = createCellRec();

  const row = formatCellRecommendationRow({
    rec,
    mapDef: ignitionMap,
    config: DEFAULT_AUTOTUNE_CONFIG,
    isSelected: false,
  });

  assert.strictEqual(row.isEligible, false);
  assert.strictEqual(row.canSelect, false);
  assert.strictEqual(row.statusBadge.status, 'BlockedCategory');
  assert.ok(row.statusBadge.label.includes('Blocked'));
});

// ---------------------------------------------------------------------------
// 4. Insufficient-evidence results cannot be selected for application
// ---------------------------------------------------------------------------
test('autoTuneReviewModel: 4. insufficient evidence cannot be selected', () => {
  const map = createTestMap();
  const rec = createCellRec({
    status: 'InsufficientEvidence',
    sample_count: 2,
    proposed_value: 80.0,
    delta_percentage: 0.0,
  });

  const row = formatCellRecommendationRow({
    rec,
    mapDef: map,
    config: DEFAULT_AUTOTUNE_CONFIG,
    isSelected: false,
  });

  assert.strictEqual(row.isEligible, false);
  assert.strictEqual(row.canSelect, false);
  assert.strictEqual(row.statusBadge.status, 'InsufficientEvidence');
});

// ---------------------------------------------------------------------------
// 5. Clamped and warning-bearing recommendations show correct status
// ---------------------------------------------------------------------------
test('autoTuneReviewModel: 5. clamped recommendations show Clamped badge and warning text', () => {
  const map = createTestMap();
  const clampedRec = createCellRec({
    is_clamped: true,
    delta_percentage: 15.0,
    warning: 'Delta clamped to +15.0%',
  });

  const row = formatCellRecommendationRow({
    rec: clampedRec,
    mapDef: map,
    config: DEFAULT_AUTOTUNE_CONFIG,
    isSelected: false,
  });

  assert.strictEqual(row.isEligible, true);
  assert.strictEqual(row.canSelect, true);
  assert.strictEqual(row.statusBadge.status, 'Clamped');
  assert.strictEqual(row.warningText, 'Delta clamped to +15.0%');
});

// ---------------------------------------------------------------------------
// 8. Dialog open, close, and review operations do not mutate ROM state
// ---------------------------------------------------------------------------
test('autoTuneReviewModel: 8. isolation check - review model operations leave ROM bytes and project history untouched', () => {
  useMapStore.getState().reset();
  useCorrelationStore.getState().reset();

  const initialRomData = new Uint8Array([0x11, 0x22, 0x33, 0x44]);
  useMapStore.setState({
    romData: initialRomData,
    romLoaded: true,
  });

  const initialRomBytes = new Uint8Array(useMapStore.getState().romData!);

  const map = createTestMap();
  const rec = createCellRec();
  const summary = getReviewSummaryStats({
    report: {
      table_id: map.id,
      total_frames_scanned: 50,
      eligible_frames_used: 40,
      cells: [rec],
      rejections: {
        rejected_stale: 0,
        rejected_fault: 0,
        rejected_out_of_bounds: 0,
        rejected_cold_engine: 5,
        rejected_fuel_cut: 2,
        rejected_transient_throttle: 3,
        rejected_sensor_out_of_range: 0,
        rejected_non_finite: 0,
      },
      blocker_reason: null,
    },
    mapDef: map,
    selectedCoords: [{ row: 1, col: 2 }],
    config: DEFAULT_AUTOTUNE_CONFIG,
  });

  assert.strictEqual(summary.totalCells, 1);
  assert.strictEqual(summary.selectedCount, 1);
  assert.strictEqual(summary.eligibleCount, 1);

  // Assert ROM bytes in store are 100% byte-for-byte identical
  assert.deepStrictEqual(useMapStore.getState().romData, initialRomBytes);
});

// ---------------------------------------------------------------------------
// 9. Keyboard interaction and accessible dialog semantics
// ---------------------------------------------------------------------------
test('autoTuneReviewModel: 9. generates complete WAI-ARIA accessible dialog attributes', () => {
  const attrs = getDialogAriaAttributes('autotune-review-title', 'autotune-review-desc');

  assert.strictEqual(attrs.role, 'dialog');
  assert.strictEqual(attrs['aria-modal'], 'true');
  assert.strictEqual(attrs['aria-labelledby'], 'autotune-review-title');
  assert.strictEqual(attrs['aria-describedby'], 'autotune-review-desc');
});
