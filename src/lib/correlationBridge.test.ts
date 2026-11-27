import { test } from 'node:test';
import assert from 'node:assert/strict';
import type { MapDef } from '../types/kivi.ts';
import type { TelemetryFrame } from '../types/telemetry.ts';
import {
  bridgeCorrelateTelemetry,
  bridgeComputeAutoTuneRecommendations,
  normalizeCorrelationError,
  normalizeAutoTuneError,
} from './correlationBridge.ts';
import { DEFAULT_AUTOTUNE_CONFIG } from '../types/correlation.ts';

function createTestMap(id = 've_table'): MapDef {
  return {
    id,
    name: 'Main VE Fuel Table',
    category: 'Fuel',
    address: 0x1000,
    rows: 2,
    cols: 2,
    dtype: 'u8',
    endian: 'big',
    column_major: false,
    scaling: {
      equation: 'X',
      unit: '%',
      decimals: 1,
    },
    y_axis: {
      address: 0,
      length: 2,
      dtype: 'u16',
      endian: 'big',
      scaling: { equation: 'X', unit: 'RPM', decimals: 0 },
      values: [1000, 2000],
    },
    x_axis: {
      address: 0,
      length: 2,
      dtype: 'u8',
      endian: 'big',
      scaling: { equation: 'X', unit: 'kPa', decimals: 0 },
      values: [20, 40],
    },
  };
}

function createTestFrame(rpm = 1000, map_kpa = 20, afr = 14.7): TelemetryFrame {
  return {
    timestamp_ms: 100,
    sequence: 1,
    is_simulated: false,
    rpm,
    tps: 25.0,
    map_kpa,
    ect_c: 85.0,
    iat_c: 30.0,
    battery_volts: 13.8,
    ignition_advance_deg: 20.0,
    injector_pulse_width_ms: 3.5,
    afr,
    speed_kph: 60.0,
    gear: 3,
    status_flags: {
      check_engine_light: false,
      limp_mode: false,
      low_voltage: false,
      overheating: false,
      stale_sample: false,
      communication_fault: false,
    },
  };
}

test('correlation bridge: resolves valid table coordinate in fallback mode', async () => {
  const map = createTestMap();
  const frame = createTestFrame(1000, 20, 14.7);

  const resp = await bridgeCorrelateTelemetry({
    table_id: map.id,
    map_def: map,
    frame,
    request_id: 'test-req-1',
    source_epoch: 5,
  });

  assert.equal(resp.request_id, 'test-req-1');
  assert.equal(resp.source_epoch, 5);
  assert.equal(resp.table_id, map.id);
  assert.equal(resp.result.is_valid, true);
  assert.equal(resp.result.selected_row, 0);
  assert.equal(resp.result.selected_col, 0);
});

test('correlation bridge: normalizes structured error on missing map', async () => {
  await assert.rejects(
    async () => {
      await bridgeCorrelateTelemetry({
        table_id: undefined,
        map_def: undefined,
      });
    },
    (err: unknown) => {
      const normalized = normalizeCorrelationError(err);
      return normalized.error === 'MapNotFound';
    }
  );
});

test('correlation bridge: handles stale sample rejection in fallback', async () => {
  const map = createTestMap();
  const frame = createTestFrame(1000, 20, 14.7);
  frame.status_flags.stale_sample = true;

  const resp = await bridgeCorrelateTelemetry({
    map_def: map,
    frame,
  });

  assert.equal(resp.result.is_valid, false);
  assert.equal(resp.result.status.status, 'StaleSample');
});

test('autotune bridge: computes recommendations in fallback mode with provenance', async () => {
  const map = createTestMap();
  const frames = [
    createTestFrame(1000, 20, 16.0),
    createTestFrame(1000, 20, 16.0),
    createTestFrame(1000, 20, 16.0),
    createTestFrame(1000, 20, 16.0),
    createTestFrame(1000, 20, 16.0),
  ];

  const resp = await bridgeComputeAutoTuneRecommendations({
    map_def: map,
    config: {
      ...DEFAULT_AUTOTUNE_CONFIG,
      min_samples_per_cell: 5,
    },
    frames,
    request_id: 'tune-req-1',
  });

  assert.equal(resp.request_id, 'tune-req-1');
  assert.equal(resp.table_id, map.id);
  assert.equal(resp.source_provenance, 'provided');
  assert.equal(resp.report.total_frames_scanned, 5);
  assert.equal(resp.report.eligible_frames_used, 5);
  assert.equal(resp.report.blocker_reason, null);

  const cell = resp.report.cells.find((c) => c.row === 0 && c.col === 0);
  assert.ok(cell);
  assert.equal(cell.status, 'Ready');
  assert.equal(cell.sample_count, 5);
  assert.ok(cell.proposed_value > cell.original_value); // lean -> add fuel
});

test('autotune bridge: blocks incompatible table category (Ignition)', async () => {
  const map = createTestMap('spark_table');
  map.category = 'Ignition';
  map.scaling.unit = 'deg';

  const resp = await bridgeComputeAutoTuneRecommendations({
    map_def: map,
    config: DEFAULT_AUTOTUNE_CONFIG,
    frames: [createTestFrame()],
  });

  assert.equal(resp.report.blocker_reason !== null, true);
});

test('autotune bridge: blocks Boost, Knock, Dwell, and Target AFR categories in fallback', async () => {
  for (const cat of ['Boost', 'Knock', 'Dwell', 'TargetAfr']) {
    const map = createTestMap(`table_${cat}`);
    map.category = cat;
    const resp = await bridgeComputeAutoTuneRecommendations({
      map_def: map,
      config: DEFAULT_AUTOTUNE_CONFIG,
      frames: [createTestFrame()],
    });
    assert.ok(resp.report.blocker_reason !== null, `Category ${cat} must be blocked`);
  }
});

test('correlation bridge: dynamically resolves TPS for X-axis in fallback', async () => {
  const map = createTestMap('alpha_n_ve');
  map.x_axis = {
    address: 0,
    length: 2,
    dtype: 'u8',
    endian: 'big',
    scaling: { equation: 'X', unit: '% TPS', decimals: 0 },
    values: [10, 50],
  };

  // Frame has TPS = 50.0 and MAP = 20.0 (which would match col 0 if MAP was checked)
  const frame = createTestFrame(1000, 20.0, 14.7);
  frame.tps = 50.0;

  const resp = await bridgeCorrelateTelemetry({
    map_def: map,
    frame,
  });

  assert.equal(resp.result.is_valid, true);
  // Column should resolve to 1 based on TPS = 50.0 matching breakpoint 50
  assert.equal(resp.result.selected_col, 1);
});

test('autotune bridge: normalizes structured error on invalid configuration', async () => {
  const map = createTestMap();
  await assert.rejects(
    async () => {
      await bridgeComputeAutoTuneRecommendations({
        map_def: map,
        config: {
          ...DEFAULT_AUTOTUNE_CONFIG,
          target_afr: { source: 'Constant', data: 40.0 }, // Implausible target AFR
        },
      });
    },
    (err: unknown) => {
      const normalized = normalizeAutoTuneError(err);
      return normalized.error === 'InvalidConfiguration';
    }
  );
});

test('correlation bridge parity: explicit binding overrides axis unit in fallback mode', async () => {
  const map = createTestMap('map_with_override');
  // Axis unit is kPa (normally MAP), but explicit binding directs to TPS
  map.x_axis!.scaling.unit = 'kPa';
  map.x_axis!.values = [10.0, 50.0];

  const frame = createTestFrame(1000, 10.0, 14.7);
  frame.tps = 50.0;
  frame.map_kpa = 10.0;

  const resp = await bridgeCorrelateTelemetry({
    map_def: map,
    frame,
    binding_config: {
      x_axis: { mode: 'ExplicitLink', target: 'tps' },
    },
  });

  assert.equal(resp.result.is_valid, true);
  // Column should resolve to 1 because explicit TPS (50.0) was used instead of MAP (10.0)
  assert.equal(resp.result.selected_col, 1);
});

test('correlation bridge parity: NoLink binding marks correlation invalid with UnboundChannel', async () => {
  const map = createTestMap('map_nolink');

  const resp = await bridgeCorrelateTelemetry({
    map_def: map,
    frame: createTestFrame(),
    binding_config: {
      x_axis: { mode: 'NoLink' },
    },
  });

  assert.equal(resp.result.is_valid, false);
  assert.deepEqual(resp.result.status, {
    status: 'UnboundChannel',
    details: { axis_role: 'x_axis', error: { error: 'NoLink' } },
  });
});

test('correlation bridge parity: missing axis unit produces UnboundChannel without explicit link', async () => {
  const map = createTestMap('map_missing_unit');
  map.x_axis!.scaling.unit = '';

  const resp = await bridgeCorrelateTelemetry({
    map_def: map,
    frame: createTestFrame(),
  });

  assert.equal(resp.result.is_valid, false);
  assert.deepEqual(resp.result.status, {
    status: 'UnboundChannel',
    details: { axis_role: 'x_axis', error: { error: 'MissingUnit' } },
  });
});

test('correlation bridge parity: unsupported axis unit produces UnboundChannel with UnsupportedUnit details', async () => {
  const map = createTestMap('map_unsupported_unit');
  map.x_axis!.scaling.unit = 'furlongs_per_fortnight';

  const resp = await bridgeCorrelateTelemetry({
    map_def: map,
    frame: createTestFrame(),
  });

  assert.equal(resp.result.is_valid, false);
  assert.deepEqual(resp.result.status, {
    status: 'UnboundChannel',
    details: {
      axis_role: 'x_axis',
      error: {
        error: 'UnsupportedUnit',
        details: { unit: 'furlongs_per_fortnight' },
      },
    },
  });
});

test('correlation bridge parity: unit metadata takes precedence over conflicting table name', async () => {
  const map = createTestMap('map_conflicting_name');
  map.name = 'High Boost Manifold Pressure Table'; // Contains "boost" and "pressure"
  map.x_axis!.scaling.unit = '% TPS'; // Unit is unambiguously TPS
  map.x_axis!.values = [10.0, 50.0];

  const frame = createTestFrame(1000, 10.0, 14.7);
  frame.tps = 50.0;
  frame.map_kpa = 10.0;

  const resp = await bridgeCorrelateTelemetry({
    map_def: map,
    frame,
  });

  assert.equal(resp.result.is_valid, true);
  // UnitMatch for % TPS must take precedence over the table name "Boost"
  assert.equal(resp.result.selected_col, 1);
});

test('correlation bridge parity: 1D table with rows=1 correlates row 0 without Y-axis', async () => {
  const map = createTestMap('table_1d_row');
  map.rows = 1;
  map.y_axis = undefined;

  const resp = await bridgeCorrelateTelemetry({
    map_def: map,
    frame: createTestFrame(1000, 20.0, 14.7),
  });

  assert.equal(resp.result.is_valid, true);
  assert.equal(resp.result.selected_row, 0);
  assert.equal(resp.result.selected_col, 0);
  assert.equal(resp.result.row_coordinate?.is_exact_match, true);
  assert.equal(resp.result.row_coordinate?.fractional_pos, 0.0);
});

test('correlation bridge parity: 1D table with cols=1 correlates col 0 without X-axis', async () => {
  const map = createTestMap('table_1d_col');
  map.cols = 1;
  map.x_axis = undefined;

  const resp = await bridgeCorrelateTelemetry({
    map_def: map,
    frame: createTestFrame(1000, 20.0, 14.7),
  });

  assert.equal(resp.result.is_valid, true);
  assert.equal(resp.result.selected_row, 0);
  assert.equal(resp.result.selected_col, 0);
  assert.equal(resp.result.col_coordinate?.is_exact_match, true);
  assert.equal(resp.result.col_coordinate?.fractional_pos, 0.0);
});
