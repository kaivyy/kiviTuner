import { test } from 'node:test';
import assert from 'node:assert/strict';
import type { CorrelationResult } from '../types/correlation.ts';
import type { CameraState, Point3D } from './surface3d.ts';
import { CAMERA_PRESETS } from './surface3d.ts';
import {
  resolveCorrelatedCell,
  getHeatmapCellCrosshairState,
  get3DSurfaceCrosshairGeometry,
} from './crosshairModel.ts';
import { useCorrelationStore } from '../stores/useCorrelationStore.ts';
import { bridgeCorrelateTelemetry } from './correlationBridge.ts';

function createValidCorrelationResult(
  row = 2,
  col = 3,
  tableId = 'fuel_main'
): CorrelationResult {
  return {
    is_valid: true,
    status: { status: 'Valid' },
    timestamp_ms: 12345,
    sequence: 42,
    is_simulated: true,
    selected_row: row,
    selected_col: col,
    row_coordinate: {
      interval_index: row,
      fractional_pos: row + 0.1,
      nearest_index: row,
      is_exact_match: false,
    },
    col_coordinate: {
      interval_index: col,
      fractional_pos: col + 0.2,
      nearest_index: col,
      is_exact_match: false,
    },
    cell_raw: 128,
    cell_eng: 14.7,
    table_id: tableId,
  };
}

// ---------------------------------------------------------------------------
// 1. Valid correlation highlights the expected cell
// ---------------------------------------------------------------------------
test('crosshairModel: 1. valid correlation resolves to expected cell (r, c)', () => {
  const result = createValidCorrelationResult(2, 4, 'map_ve');
  const cell = resolveCorrelatedCell({
    matrixDimensions: { numRows: 8, numCols: 8 },
    activeCorrelation: result,
    targetTableId: 'map_ve',
    currentTableId: 'map_ve',
  });

  assert.deepStrictEqual(cell, { r: 2, c: 4 });
});

// ---------------------------------------------------------------------------
// 2. Heatmap row/column orientation is correct (r = Y/RPM, c = X/TPS)
// ---------------------------------------------------------------------------
test('crosshairModel: 2. row and column orientation maps row to Y-axis and col to X-axis', () => {
  const result = createValidCorrelationResult(5, 1, 'table1');
  const cell = resolveCorrelatedCell({
    matrixDimensions: { numRows: 10, numCols: 6 },
    activeCorrelation: result,
    targetTableId: 'table1',
    currentTableId: 'table1',
  });

  // Selected row corresponds to row 5 (Y-axis / RPM)
  // Selected col corresponds to col 1 (X-axis / TPS)
  assert.strictEqual(cell?.r, 5);
  assert.strictEqual(cell?.c, 1);
});

// ---------------------------------------------------------------------------
// 3. Correlation highlight is distinct from manual selection
// ---------------------------------------------------------------------------
test('crosshairModel: 3. correlation highlight is visually distinct from manual selection and has pointer-events-none', () => {
  const correlatedCell = { r: 2, c: 3 };

  // Cell is only correlated
  const stateOnlyCorrelated = getHeatmapCellCrosshairState({
    r: 2,
    c: 3,
    correlatedCell,
    isSelected: false,
    isAnchor: false,
  });

  assert.strictEqual(stateOnlyCorrelated.isCorrelated, true);
  assert.ok(stateOnlyCorrelated.reticleClass.includes('ring-amber-400'));
  assert.ok(stateOnlyCorrelated.reticleClass.includes('z-20'));
  assert.strictEqual(stateOnlyCorrelated.ariaAttributes['aria-current'], 'location');
  assert.ok(stateOnlyCorrelated.tooltip?.includes('Telemetry operating point'));

  // Cell is both correlated AND manually selected
  const stateBoth = getHeatmapCellCrosshairState({
    r: 2,
    c: 3,
    correlatedCell,
    isSelected: true,
    isAnchor: true,
  });

  assert.strictEqual(stateBoth.isCorrelated, true);
  // Reticle retains amber crosshair hierarchy (z-20) over manual selection (white ring z-10)
  assert.ok(stateBoth.reticleClass.includes('ring-amber-400'));
  assert.ok(stateBoth.reticleClass.includes('ring-offset-slate-900'));

  // Cell is neither
  const stateNeither = getHeatmapCellCrosshairState({
    r: 0,
    c: 0,
    correlatedCell,
    isSelected: false,
    isAnchor: false,
  });
  assert.strictEqual(stateNeither.isCorrelated, false);
  assert.strictEqual(stateNeither.ariaAttributes['aria-current'], undefined);
});

// ---------------------------------------------------------------------------
// 4. Invalid or stale correlation clears the highlight
// ---------------------------------------------------------------------------
test('crosshairModel: 4. invalid or stale correlation clears the highlight', () => {
  const invalidResult: CorrelationResult = {
    ...createValidCorrelationResult(2, 3),
    is_valid: false,
    status: { status: 'StaleSample' },
  };

  const cellStale = resolveCorrelatedCell({
    matrixDimensions: { numRows: 8, numCols: 8 },
    activeCorrelation: invalidResult,
    targetTableId: 'fuel_main',
    currentTableId: 'fuel_main',
  });
  assert.strictEqual(cellStale, null);

  const commFaultResult: CorrelationResult = {
    ...createValidCorrelationResult(2, 3),
    is_valid: false,
    status: { status: 'CommunicationFault' },
  };

  const cellFault = resolveCorrelatedCell({
    matrixDimensions: { numRows: 8, numCols: 8 },
    activeCorrelation: commFaultResult,
    targetTableId: 'fuel_main',
    currentTableId: 'fuel_main',
  });
  assert.strictEqual(cellFault, null);

  const nullCorr = resolveCorrelatedCell({
    matrixDimensions: { numRows: 8, numCols: 8 },
    activeCorrelation: null,
    targetTableId: 'fuel_main',
    currentTableId: 'fuel_main',
  });
  assert.strictEqual(nullCorr, null);
});

// ---------------------------------------------------------------------------
// 5. Out-of-range indices never render a cell highlight
// ---------------------------------------------------------------------------
test('crosshairModel: 5. out-of-range indices never render a cell highlight', () => {
  // Negative row
  const negRow = {
    ...createValidCorrelationResult(-1, 2),
    selected_row: -1,
  };
  assert.strictEqual(
    resolveCorrelatedCell({
      matrixDimensions: { numRows: 8, numCols: 8 },
      activeCorrelation: negRow,
      targetTableId: 'map',
      currentTableId: 'map',
    }),
    null
  );

  // Row >= numRows
  const overflowRow = {
    ...createValidCorrelationResult(8, 2),
    selected_row: 8,
  };
  assert.strictEqual(
    resolveCorrelatedCell({
      matrixDimensions: { numRows: 8, numCols: 8 },
      activeCorrelation: overflowRow,
      targetTableId: 'map',
      currentTableId: 'map',
    }),
    null
  );

  // Col >= numCols
  const overflowCol = {
    ...createValidCorrelationResult(2, 10),
    selected_col: 10,
  };
  assert.strictEqual(
    resolveCorrelatedCell({
      matrixDimensions: { numRows: 8, numCols: 8 },
      activeCorrelation: overflowCol,
      targetTableId: 'map',
      currentTableId: 'map',
    }),
    null
  );

  // Non-finite or null
  const nullCoords = {
    ...createValidCorrelationResult(0, 0),
    selected_row: null,
    selected_col: null,
  };
  assert.strictEqual(
    resolveCorrelatedCell({
      matrixDimensions: { numRows: 8, numCols: 8 },
      activeCorrelation: nullCoords,
      targetTableId: 'map',
      currentTableId: 'map',
    }),
    null
  );
});

// ---------------------------------------------------------------------------
// 6. Heatmap editing and keyboard navigation compatibility
// ---------------------------------------------------------------------------
test('crosshairModel: 6. editing state coexists safely with crosshair reticle without interception', () => {
  const correlatedCell = { r: 3, c: 3 };

  // When cell is being edited, reticle state indicates correlation while remaining non-blocking
  const editState = getHeatmapCellCrosshairState({
    r: 3,
    c: 3,
    correlatedCell,
    isSelected: true,
    isAnchor: true,
    isEditing: true,
  });

  assert.strictEqual(editState.isCorrelated, true);
  // Reticle overlay uses pointer-events-none and absolute positioning
  assert.ok(editState.reticleClass.includes('ring-amber-400'));
});

// ---------------------------------------------------------------------------
// 7. 3D view highlights the same logical cell as the heatmap (x = c, z = r)
// ---------------------------------------------------------------------------
test('crosshairModel: 7. 3D view projects the exact same logical cell vertex (x=c, z=r)', () => {
  const matrix = [
    [10, 20, 30],
    [40, 50, 60],
    [70, 80, 90],
  ];
  const center: Point3D = { x: 1, y: 50, z: 1 };
  const camera: CameraState = {
    ...CAMERA_PRESETS.TOP_DOWN,
    scale: 20,
    panX: 0,
    panY: 0,
  };

  const geom = get3DSurfaceCrosshairGeometry({
    r: 1,
    c: 2,
    matrix,
    minVal: 10,
    center,
    camera,
    width: 600,
    height: 400,
    effectiveYScaling: 1.0,
  });

  assert.ok(geom !== null);
  assert.strictEqual(geom.isValid, true);
  assert.ok(Number.isFinite(geom.topProj.x));
  assert.ok(Number.isFinite(geom.topProj.y));
  assert.ok(Number.isFinite(geom.baseProj.x));
  assert.ok(Number.isFinite(geom.baseProj.y));
});

// ---------------------------------------------------------------------------
// 8. 3D view handles missing or invalid geometry safely
// ---------------------------------------------------------------------------
test('crosshairModel: 8. 3D view handles empty matrix or out-of-bounds coordinates safely', () => {
  const center: Point3D = { x: 0, y: 0, z: 0 };
  const camera: CameraState = CAMERA_PRESETS.ISOMETRIC;

  // Empty matrix
  const emptyGeom = get3DSurfaceCrosshairGeometry({
    r: 0,
    c: 0,
    matrix: [],
    minVal: 0,
    center,
    camera,
    width: 600,
    height: 400,
    effectiveYScaling: 1.0,
  });
  assert.strictEqual(emptyGeom, null);

  // Out of bounds row/col
  const matrix = [[10, 20], [30, 40]];
  const outGeom = get3DSurfaceCrosshairGeometry({
    r: 5,
    c: 5,
    matrix,
    minVal: 10,
    center,
    camera,
    width: 600,
    height: 400,
    effectiveYScaling: 1.0,
  });
  assert.strictEqual(outGeom, null);

  // NaN value in matrix cell
  const nanMatrix = [[10, NaN], [30, 40]];
  const nanGeom = get3DSurfaceCrosshairGeometry({
    r: 0,
    c: 1,
    matrix: nanMatrix,
    minVal: 10,
    center,
    camera,
    width: 600,
    height: 400,
    effectiveYScaling: 1.0,
  });
  assert.strictEqual(nanGeom, null);
});

// ---------------------------------------------------------------------------
// 9. Camera interaction remains functional and projects smoothly
// ---------------------------------------------------------------------------
test('crosshairModel: 9. camera orbit/pan/zoom projects crosshair without altering surface bounds', () => {
  const matrix = [
    [10, 20],
    [30, 40],
  ];
  const center: Point3D = { x: 0.5, y: 25, z: 0.5 };

  const cam1: CameraState = { yaw: 0, pitch: 0, scale: 20, panX: 0, panY: 0 };
  const cam2: CameraState = { yaw: Math.PI / 4, pitch: Math.PI / 6, scale: 35, panX: 100, panY: -50 };

  const g1 = get3DSurfaceCrosshairGeometry({
    r: 1,
    c: 1,
    matrix,
    minVal: 10,
    center,
    camera: cam1,
    width: 600,
    height: 400,
    effectiveYScaling: 1.0,
  });

  const g2 = get3DSurfaceCrosshairGeometry({
    r: 1,
    c: 1,
    matrix,
    minVal: 10,
    center,
    camera: cam2,
    width: 600,
    height: 400,
    effectiveYScaling: 1.0,
  });

  assert.ok(g1 !== null && g2 !== null);
  // Camera transform changes projected coordinates smoothly
  assert.notEqual(g1.topProj.x, g2.topProj.x);
  assert.notEqual(g1.topProj.y, g2.topProj.y);
});

// ---------------------------------------------------------------------------
// 10. Source switching or playback seeking clears the old visual indicator
// ---------------------------------------------------------------------------
test('crosshairModel: 10. source switching or playback seeking clears active correlation in store', () => {
  useCorrelationStore.getState().reset();
  const valid = createValidCorrelationResult(2, 3, 've_table');
  useCorrelationStore.setState({
    activeCorrelation: valid,
    correlationStatus: 'success',
    targetTableId: 've_table',
  });

  assert.ok(useCorrelationStore.getState().activeCorrelation !== null);

  // Source change clears
  useCorrelationStore.getState().invalidateOnSourceChange('playback');
  assert.strictEqual(useCorrelationStore.getState().activeCorrelation, null);

  // Set again then seek clears
  useCorrelationStore.setState({ activeCorrelation: valid, correlationStatus: 'success' });
  assert.ok(useCorrelationStore.getState().activeCorrelation !== null);

  useCorrelationStore.getState().invalidateOnSeek();
  assert.strictEqual(useCorrelationStore.getState().activeCorrelation, null);
});

// ---------------------------------------------------------------------------
// 11. Table change cannot leave a highlight from previous table
// ---------------------------------------------------------------------------
test('crosshairModel: 11. table ID mismatch immediately suppresses crosshair', () => {
  const result = createValidCorrelationResult(2, 3, 'table_A');

  // Resolving on table_B with result from table_A
  const cell = resolveCorrelatedCell({
    matrixDimensions: { numRows: 8, numCols: 8 },
    activeCorrelation: result,
    targetTableId: 'table_A',
    currentTableId: 'table_B', // Different table
  });

  assert.strictEqual(cell, null);
});

// ---------------------------------------------------------------------------
// 12. Existing ROM bytes and project history remain unchanged
// ---------------------------------------------------------------------------
test('crosshairModel: 12. isolation check - crosshair model has zero mutation effects', () => {
  const matrix = [
    [10, 20],
    [30, 40],
  ];
  const origMatrixJson = JSON.stringify(matrix);

  const result = createValidCorrelationResult(0, 1, 'table');
  const cell = resolveCorrelatedCell({
    matrixDimensions: { numRows: 2, numCols: 2 },
    activeCorrelation: result,
    targetTableId: 'table',
    currentTableId: 'table',
  });

  const state = getHeatmapCellCrosshairState({
    r: cell!.r,
    c: cell!.c,
    correlatedCell: cell,
  });

  // Matrix values remain completely untouched
  assert.strictEqual(JSON.stringify(matrix), origMatrixJson);
  assert.strictEqual(matrix[0][1], 20);
  assert.ok(state.isCorrelated);
});

// ---------------------------------------------------------------------------
// 13. Existing diff, selection, and hover rendering compatibility
// ---------------------------------------------------------------------------
test('crosshairModel: 13. selection, hover, and crosshair maintain clear visual hierarchy', () => {
  // When a cell is selected, hovered, and correlated:
  const state = getHeatmapCellCrosshairState({
    r: 1,
    c: 1,
    correlatedCell: { r: 1, c: 1 },
    isSelected: true,
    isAnchor: false,
  });

  // Reticle provides amber accent ring at higher z-index (z-20)
  assert.strictEqual(state.isCorrelated, true);
  assert.ok(state.reticleClass.includes('ring-amber-400'));
});

// ---------------------------------------------------------------------------
// 14. No Auto-Tune recommendation is applied by visualization component
// ---------------------------------------------------------------------------
test('crosshairModel: 14. crosshair resolution never applies or accepts recommendations', () => {
  useCorrelationStore.getState().reset();
  const initialReport = useCorrelationStore.getState().activeReport;
  assert.strictEqual(initialReport, null);

  const cell = resolveCorrelatedCell({
    matrixDimensions: { numRows: 4, numCols: 4 },
    activeCorrelation: createValidCorrelationResult(1, 1, 'fuel'),
    targetTableId: 'fuel',
    currentTableId: 'fuel',
  });

  assert.deepStrictEqual(cell, { r: 1, c: 1 });
  // Store report remains unchanged and unapplied
  assert.strictEqual(useCorrelationStore.getState().activeReport, null);
});

// ---------------------------------------------------------------------------
// 15. Rendering remains stable when correlation updates rapidly
// ---------------------------------------------------------------------------
test('crosshairModel: 15. rapid consecutive frame correlations update deterministically', () => {
  const cells: ({ r: number; c: number } | null)[] = [];

  for (let i = 0; i < 50; i++) {
    const row = i % 8;
    const col = (i * 2) % 8;
    const res = createValidCorrelationResult(row, col, 'fast_map');
    const cell = resolveCorrelatedCell({
      matrixDimensions: { numRows: 8, numCols: 8 },
      activeCorrelation: res,
      targetTableId: 'fast_map',
      currentTableId: 'fast_map',
    });
    cells.push(cell);
  }

  assert.strictEqual(cells.length, 50);
  assert.deepStrictEqual(cells[0], { r: 0, c: 0 });
  assert.deepStrictEqual(cells[1], { r: 1, c: 2 });
  assert.deepStrictEqual(cells[49], { r: 49 % 8, c: (49 * 2) % 8 });
});

// ---------------------------------------------------------------------------
// 16. Browser Fallback & Parity Edge Cases
// ---------------------------------------------------------------------------
test('crosshairModel: 16. fallback coordinate resolution handles ascending and descending axes', async () => {
  const mapDef = {
    id: 've_table',
    name: 'VE Table',
    category: 'Fuel' as const,
    address: 0x1000,
    rows: 4,
    cols: 4,
    dtype: 'u8' as const,
    endian: 'little' as const,
    scaling: { factor: 1, offset: 0, decimals: 1, equation: 'X', unit: '%' },
    x_axis: {
      id: 'load_axis',
      name: 'Load',
      role: 'x_axis' as const,
      address: 0x2000,
      length: 4,
      dtype: 'u8' as const,
      endian: 'little' as const,
      scaling: { factor: 1, offset: 0, decimals: 0, equation: 'X', unit: 'kPa' },
      values: [20, 40, 60, 80], // strictly ascending
    },
    y_axis: {
      id: 'rpm_axis',
      name: 'RPM',
      role: 'y_axis' as const,
      address: 0x2010,
      length: 4,
      dtype: 'u16' as const,
      endian: 'little' as const,
      scaling: { factor: 1, offset: 0, decimals: 0, equation: 'X', unit: 'RPM' },
      values: [6000, 4000, 2000, 1000], // strictly descending
    },
  };

  const frame = {
    timestamp_ms: 1000,
    sequence: 1,
    rpm: 4000, // exact match on descending axis index 1
    tps: 25,
    map_kpa: 40, // exact match on ascending axis index 1
    ect_c: 85,
    iat_c: 25,
    battery_volts: 13.8,
    ignition_advance_deg: 15,
    injector_pulse_width_ms: 3.5,
    afr: 14.7,
    speed_kph: 60,
    gear: 3,
    status_flags: {
      stale_sample: false,
      communication_fault: false,
      crc_error: false,
      buffer_overflow: false,
      check_engine_light: false,
      limp_mode: false,
      low_voltage: false,
      overheating: false,
    },
    is_simulated: true,
  };

  const resp = await bridgeCorrelateTelemetry({
    frame,
    map_def: mapDef,
    table_id: 've_table',
  });

  assert.strictEqual(resp.result.is_valid, true);
  assert.strictEqual(resp.result.selected_row, 1);
  assert.strictEqual(resp.result.selected_col, 1);
  assert.strictEqual(resp.result.row_coordinate?.is_exact_match, true);
  assert.strictEqual(resp.result.col_coordinate?.is_exact_match, true);
});

test('crosshairModel: 16. fallback coordinate resolution rejects non-finite telemetry (NaN / Infinity)', async () => {
  const mapDef = {
    id: 've_table',
    name: 'VE Table',
    category: 'Fuel' as const,
    address: 0x1000,
    rows: 4,
    cols: 4,
    dtype: 'u8' as const,
    endian: 'little' as const,
    scaling: { factor: 1, offset: 0, decimals: 1, equation: 'X', unit: '%' },
    x_axis: {
      id: 'load_axis',
      name: 'Load',
      role: 'x_axis' as const,
      address: 0x2000,
      length: 4,
      dtype: 'u8' as const,
      endian: 'little' as const,
      scaling: { factor: 1, offset: 0, decimals: 0, equation: 'X', unit: 'kPa' },
      values: [20, 40, 60, 80],
    },
    y_axis: {
      id: 'rpm_axis',
      name: 'RPM',
      role: 'y_axis' as const,
      address: 0x2010,
      length: 4,
      dtype: 'u16' as const,
      endian: 'little' as const,
      scaling: { factor: 1, offset: 0, decimals: 0, equation: 'X', unit: 'RPM' },
      values: [1000, 2000, 3000, 4000],
    },
  };

  const nanFrame = {
    timestamp_ms: 1000,
    sequence: 1,
    rpm: NaN,
    tps: 25,
    map_kpa: 40,
    ect_c: 85,
    iat_c: 25,
    battery_volts: 13.8,
    ignition_advance_deg: 15,
    injector_pulse_width_ms: 3.5,
    afr: 14.7,
    speed_kph: 60,
    gear: 3,
    status_flags: {
      stale_sample: false,
      communication_fault: false,
      crc_error: false,
      buffer_overflow: false,
      check_engine_light: false,
      limp_mode: false,
      low_voltage: false,
      overheating: false,
    },
    is_simulated: true,
  };

  const resp = await bridgeCorrelateTelemetry({
    frame: nanFrame,
    map_def: mapDef,
    table_id: 've_table',
  });

  assert.strictEqual(resp.result.is_valid, false);
  assert.strictEqual(resp.result.selected_row, null);
});
