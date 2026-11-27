import { test, beforeEach } from 'node:test';
import assert from 'node:assert/strict';
import type { MapDef } from '../types/kivi.ts';
import type { TelemetryFrame } from '../types/telemetry.ts';
import { useCorrelationStore } from './useCorrelationStore.ts';
import { useTelemetryStore } from './useTelemetryStore.ts';
import { DEFAULT_AUTOTUNE_CONFIG } from '../types/correlation.ts';

function createTestMap(id = 'fuel_table_1'): MapDef {
  return {
    id,
    name: 'Primary Fuel Table',
    category: 'Fuel',
    address: 0x2000,
    rows: 2,
    cols: 2,
    dtype: 'u8',
    endian: 'big',
    column_major: false,
    scaling: { equation: 'X', unit: '%', decimals: 1 },
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

beforeEach(() => {
  useCorrelationStore.getState().reset();
  useTelemetryStore.getState().reset();
});

test('useCorrelationStore: initial state is idle and clean', () => {
  const state = useCorrelationStore.getState();
  assert.equal(state.correlationStatus, 'idle');
  assert.equal(state.activeCorrelation, null);
  assert.equal(state.correlationError, null);
  assert.equal(state.autoTuneStatus, 'idle');
  assert.equal(state.activeReport, null);
  assert.equal(state.autoTuneError, null);
  assert.equal(state.requestGeneration, 0);
});

test('useCorrelationStore: valid correlation request transitions to success and updates activeCorrelation', async () => {
  const map = createTestMap();
  const frame = createTestFrame(1000, 20, 14.7);

  useCorrelationStore.getState().setTargetTable(map.id, map);
  await useCorrelationStore.getState().correlateFrame(frame);

  const state = useCorrelationStore.getState();
  assert.equal(state.correlationStatus, 'success');
  assert.ok(state.activeCorrelation);
  assert.equal(state.activeCorrelation.is_valid, true);
  assert.equal(state.activeCorrelation.selected_row, 0);
  assert.equal(state.activeCorrelation.selected_col, 0);
  assert.equal(state.correlationError, null);
});

test('useCorrelationStore: structured IPC error sets error state and clears activeCorrelation', async () => {
  // Missing table ID and map definition
  useCorrelationStore.getState().setTargetTable(null, null);
  await useCorrelationStore.getState().correlateFrame(createTestFrame());

  const state = useCorrelationStore.getState();
  assert.equal(state.correlationStatus, 'error');
  assert.equal(state.activeCorrelation, null);
  assert.ok(state.correlationError);
});

test('useCorrelationStore: rapid consecutive requests resolving out-of-order cannot overwrite newer result', async () => {
  const map = createTestMap();
  useCorrelationStore.getState().setTargetTable(map.id, map);

  const store = useCorrelationStore.getState();

  // Create two deferred responses using controllable promises
  let resolveReq1: (val: unknown) => void = () => {};
  const req1Promise = new Promise((resolve) => {
    resolveReq1 = resolve;
  });

  // Override bridge dispatch with mock to control resolution timing
  const originalCorrelate = store.correlateFrameWithBackend;
  let callCount = 0;
  store.correlateFrameWithBackend = async (req) => {
    callCount++;
    if (callCount === 1) {
      await req1Promise;
      return {
        request_id: req.request_id,
        source_epoch: req.source_epoch,
        table_id: map.id,
        source: 'provided',
        result: {
          is_valid: true,
          status: { status: 'Valid' as const },
          timestamp_ms: 100,
          sequence: 1,
          is_simulated: false,
          selected_row: 0,
          selected_col: 0,
          row_coordinate: null,
          col_coordinate: null,
          cell_raw: null,
          cell_eng: null,
          table_id: map.id,
        },
      };
    } else {
      // Call 2 resolves immediately with cell (1, 1)
      return {
        request_id: req.request_id,
        source_epoch: req.source_epoch,
        table_id: map.id,
        source: 'provided',
        result: {
          is_valid: true,
          status: { status: 'Valid' as const },
          timestamp_ms: 200,
          sequence: 2,
          is_simulated: false,
          selected_row: 1,
          selected_col: 1,
          row_coordinate: null,
          col_coordinate: null,
          cell_raw: null,
          cell_eng: null,
          table_id: map.id,
        },
      };
    }
  };

  // Dispatch Request 1 (delayed)
  const p1 = store.correlateFrame(createTestFrame(1000, 20));
  // Dispatch Request 2 (fast)
  const p2 = store.correlateFrame(createTestFrame(2000, 40));

  await p2;
  // State should now reflect Request 2 (row 1, col 1)
  assert.equal(useCorrelationStore.getState().activeCorrelation?.selected_row, 1);

  // Now let Request 1 resolve
  resolveReq1(null);
  await p1;

  // State must STILL reflect Request 2 because Request 1 has older generation!
  assert.equal(useCorrelationStore.getState().activeCorrelation?.selected_row, 1);

  // Restore bridge function
  store.correlateFrameWithBackend = originalCorrelate;
});

test('useCorrelationStore: source switch invalidates previous correlation result', async () => {
  const map = createTestMap();
  useCorrelationStore.getState().setTargetTable(map.id, map);
  await useCorrelationStore.getState().correlateFrame(createTestFrame(1000, 20));

  assert.ok(useCorrelationStore.getState().activeCorrelation);

  // Simulate source change (e.g. live -> playback)
  useCorrelationStore.getState().invalidateOnSourceChange('playback');

  const state = useCorrelationStore.getState();
  assert.equal(state.activeCorrelation, null);
  assert.equal(state.correlationStatus, 'idle');
});

test('useCorrelationStore: playback seek invalidates previous correlation result', async () => {
  const map = createTestMap();
  useCorrelationStore.getState().setTargetTable(map.id, map);
  await useCorrelationStore.getState().correlateFrame(createTestFrame(1000, 20));

  assert.ok(useCorrelationStore.getState().activeCorrelation);

  // Playback seek occurred
  useCorrelationStore.getState().invalidateOnSeek();

  const state = useCorrelationStore.getState();
  assert.equal(state.activeCorrelation, null);
  assert.equal(state.correlationStatus, 'idle');
});

test('useCorrelationStore: disconnect or unavailable telemetry clears correlation state', async () => {
  const map = createTestMap();
  useCorrelationStore.getState().setTargetTable(map.id, map);
  await useCorrelationStore.getState().correlateFrame(createTestFrame(1000, 20));

  assert.ok(useCorrelationStore.getState().activeCorrelation);

  // Telemetry disconnected
  useCorrelationStore.getState().invalidateOnDisconnect();

  const state = useCorrelationStore.getState();
  assert.equal(state.activeCorrelation, null);
  assert.equal(state.correlationStatus, 'unavailable');
});

test('useCorrelationStore: changing target table invalidates both correlation and auto-tune state', async () => {
  const map1 = createTestMap('table_1');
  const map2 = createTestMap('table_2');

  useCorrelationStore.getState().setTargetTable(map1.id, map1);
  await useCorrelationStore.getState().correlateFrame(createTestFrame());

  assert.ok(useCorrelationStore.getState().activeCorrelation);

  // Switch table to map2
  useCorrelationStore.getState().setTargetTable(map2.id, map2);

  const state = useCorrelationStore.getState();
  assert.equal(state.activeCorrelation, null);
  assert.equal(state.activeReport, null);
  assert.equal(state.targetTableId, 'table_2');
});

test('useCorrelationStore: preserves request_id and source_epoch in response', async () => {
  const map = createTestMap();
  useCorrelationStore.getState().setTargetTable(map.id, map);
  useCorrelationStore.getState().setSourceEpoch(99);

  await useCorrelationStore.getState().correlateFrame(createTestFrame());

  const state = useCorrelationStore.getState();
  assert.ok(state.activeCorrelationResponse);
  assert.equal(state.activeCorrelationResponse.source_epoch, 99);
  assert.ok(state.activeCorrelationResponse.request_id?.startsWith('corr-'));
});

test('useCorrelationStore: Auto-Tune request succeeds and retains recommendation report with provenance', async () => {
  const map = createTestMap();
  useCorrelationStore.getState().setTargetTable(map.id, map);

  const frames = [
    createTestFrame(1000, 20, 16.0),
    createTestFrame(1000, 20, 16.0),
    createTestFrame(1000, 20, 16.0),
    createTestFrame(1000, 20, 16.0),
    createTestFrame(1000, 20, 16.0),
  ];

  await useCorrelationStore.getState().computeAutoTuneRecommendations({
    config: { ...DEFAULT_AUTOTUNE_CONFIG, min_samples_per_cell: 5 },
    frames,
  });

  const state = useCorrelationStore.getState();
  assert.equal(state.autoTuneStatus, 'success');
  assert.ok(state.activeReport);
  assert.equal(state.activeReport.eligible_frames_used, 5);
  assert.equal(state.activeReportResponse?.source_provenance, 'provided');
});

test('useCorrelationStore: insufficient evidence report is NOT treated as an IPC error', async () => {
  const map = createTestMap();
  useCorrelationStore.getState().setTargetTable(map.id, map);

  // Only 2 frames (< 5 required)
  const frames = [createTestFrame(1000, 20, 16.0), createTestFrame(1000, 20, 16.0)];

  await useCorrelationStore.getState().computeAutoTuneRecommendations({
    config: { ...DEFAULT_AUTOTUNE_CONFIG, min_samples_per_cell: 5 },
    frames,
  });

  const state = useCorrelationStore.getState();
  // Status must be success, NOT error!
  assert.equal(state.autoTuneStatus, 'success');
  assert.equal(state.autoTuneError, null);
  assert.ok(state.activeReport);

  const cell = state.activeReport.cells.find((c) => c.row === 0 && c.col === 0);
  assert.ok(cell);
  assert.equal(cell.status, 'InsufficientEvidence');
  assert.equal(cell.proposed_value, cell.original_value);
});

test('useCorrelationStore: changing auto-tune config marks active report stale', async () => {
  const map = createTestMap();
  useCorrelationStore.getState().setTargetTable(map.id, map);

  await useCorrelationStore.getState().computeAutoTuneRecommendations({
    config: DEFAULT_AUTOTUNE_CONFIG,
    frames: [createTestFrame(), createTestFrame()],
  });

  assert.ok(useCorrelationStore.getState().activeReport);

  // Update config
  useCorrelationStore.getState().setAutoTuneConfig({
    ...DEFAULT_AUTOTUNE_CONFIG,
    min_ect_c: 80.0,
  });

  const state = useCorrelationStore.getState();
  assert.equal(state.activeReport, null);
  assert.equal(state.autoTuneStatus, 'idle');
});

test('useCorrelationStore: isolation - zero mutations to ROM bytes or project history', () => {
  const mapStoreInitial = { ...useTelemetryStore.getState() };
  // Check that useCorrelationStore has no methods or actions that mutate ROM or apply recommendations
  const store = useCorrelationStore.getState();
  assert.equal((store as unknown as { applyRecommendations?: unknown }).applyRecommendations, undefined);
  assert.equal((store as unknown as { writeRom?: unknown }).writeRom, undefined);
  assert.deepEqual(useTelemetryStore.getState(), mapStoreInitial);
});

test('useCorrelationStore: asynchronous correlation with obsolete source_epoch is discarded', async () => {
  const map = createTestMap();
  useCorrelationStore.getState().setTargetTable(map.id, map);
  useCorrelationStore.getState().setSourceEpoch(1);

  // Mock correlateFrameWithBackend to resolve slowly with old epoch 1
  let resolveSlow: ((val: any) => void) | null = null;
  useCorrelationStore.setState({
    correlateFrameWithBackend: () =>
      new Promise((resolve) => {
        resolveSlow = resolve;
      }),
  });

  const correlatePromise = useCorrelationStore.getState().correlateFrame(createTestFrame());

  // Meanwhile, source epoch changes to 2 (e.g. source switch or seek)
  useCorrelationStore.getState().setSourceEpoch(2);

  // Now the slow response arrives carrying old source_epoch 1
  resolveSlow!({
    request_id: 'corr-old',
    source_epoch: 1,
    table_id: map.id,
    source: 'live',
    result: {
      is_valid: true,
      status: { status: 'Valid' },
      timestamp_ms: 100,
      sequence: 1,
      is_simulated: false,
      selected_row: 1,
      selected_col: 1,
      row_coordinate: null,
      col_coordinate: null,
      cell_raw: null,
      cell_eng: null,
      table_id: map.id,
    },
  });

  await correlatePromise;

  // Stale result from epoch 1 must NOT be stored
  const state = useCorrelationStore.getState();
  assert.equal(state.activeCorrelation, null);
});
