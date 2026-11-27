import { invoke, isTauri } from '@tauri-apps/api/core';
import type { MapDef } from '../types/kivi.ts';
import type { TelemetryFrame } from '../types/telemetry.ts';
import type {
  CorrelateTelemetryRequest,
  CorrelateTelemetryResponse,
  CorrelationCommandError,
  AutoTuneRequest,
  AutoTuneResponse,
  AutoTuneCommandError,
  CorrelationResult,
  RecommendationReport,
  CellRecommendation,
  AxisAssociationMode,
  TableBindingConfig,
  AxisBindingError,
} from '../types/correlation.ts';
import { useTelemetryStore } from '../stores/useTelemetryStore.ts';
import { useMapStore } from '../stores/useMapStore.ts';

/**
 * Checks whether the application is running inside a Tauri container.
 */
export function isDesktopApp(): boolean {
  try {
    return isTauri();
  } catch {
    return false;
  }
}

/**
 * Normalizes an unknown error into a structured `CorrelationCommandError`.
 */
export function normalizeCorrelationError(err: unknown): CorrelationCommandError {
  if (err && typeof err === 'object') {
    const obj = err as Record<string, unknown>;
    if (typeof obj.error === 'string') {
      return err as CorrelationCommandError;
    }
  }
  const message = err instanceof Error ? err.message : String(err);
  return {
    error: 'InternalError',
    details: { message },
  };
}

/**
 * Normalizes an unknown error into a structured `AutoTuneCommandError`.
 */
export function normalizeAutoTuneError(err: unknown): AutoTuneCommandError {
  if (err && typeof err === 'object') {
    const obj = err as Record<string, unknown>;
    if (typeof obj.error === 'string') {
      return err as AutoTuneCommandError;
    }
  }
  const message = err instanceof Error ? err.message : String(err);
  return {
    error: 'InternalError',
    details: { message },
  };
}

/**
 * Resolves a coordinate along an axis slice in browser fallback mode.
 */
function resolveFallbackAxisCoordinate(
  breakpoints: number[],
  value: number
): { index: number; frac: number; isExact: boolean; outOfRange: boolean } {
  if (breakpoints.length < 2 || !Number.isFinite(value)) {
    return { index: 0, frac: 0, isExact: false, outOfRange: true };
  }
  const isAscending = breakpoints[0] < breakpoints[breakpoints.length - 1];
  const min = isAscending ? breakpoints[0] : breakpoints[breakpoints.length - 1];
  const max = isAscending ? breakpoints[breakpoints.length - 1] : breakpoints[0];

  if (value < min - 1e-6 || value > max + 1e-6) {
    return { index: 0, frac: 0, isExact: false, outOfRange: true };
  }

  const n = breakpoints.length;
  if (isAscending) {
    for (let i = 0; i < n - 1; i++) {
      if (value >= breakpoints[i] - 1e-6 && value <= breakpoints[i + 1] + 1e-6) {
        const span = breakpoints[i + 1] - breakpoints[i];
        const t = span > 0 ? (value - breakpoints[i]) / span : 0;
        const nearest = t <= 0.5 ? i : i + 1;
        const isExact = Math.abs(value - breakpoints[i]) < 1e-6 || Math.abs(value - breakpoints[i + 1]) < 1e-6;
        return { index: nearest, frac: i + t, isExact, outOfRange: false };
      }
    }
  } else {
    for (let i = 0; i < n - 1; i++) {
      if (value <= breakpoints[i] + 1e-6 && value >= breakpoints[i + 1] - 1e-6) {
        const span = breakpoints[i] - breakpoints[i + 1];
        const t = span > 0 ? (breakpoints[i] - value) / span : 0;
        const nearest = t <= 0.5 ? i : i + 1;
        const isExact = Math.abs(value - breakpoints[i]) < 1e-6 || Math.abs(value - breakpoints[i + 1]) < 1e-6;
        return { index: nearest, frac: i + t, isExact, outOfRange: false };
      }
    }
  }

  return { index: 0, frac: 0, isExact: false, outOfRange: false };
}

/**
 * Resolves an active telemetry channel from axis unit and association mode,
 * matching canonical Rust resolve_axis_channel semantics.
 */
function resolveFallbackChannel(
  unit: string | undefined,
  binding?: AxisAssociationMode
): { channel?: keyof TelemetryFrame; error?: AxisBindingError } {
  const mode = binding?.mode ?? 'UnitMatch';

  if (mode === 'NoLink') {
    return { error: { error: 'NoLink' } };
  }

  if (mode === 'ExplicitLink') {
    const rawTarget = (binding as { mode: 'ExplicitLink'; target: string }).target;
    const target = rawTarget?.trim().toLowerCase();
    if (!target) {
      return {
        error: {
          error: 'InvalidExplicitReference',
          details: { target: rawTarget || '', reason: 'Empty explicit target' },
        },
      };
    }
    switch (target) {
      case 'rpm':
      case 'engine_speed':
      case 'speed_rpm':
        return { channel: 'rpm' };
      case 'tps':
      case 'throttle':
      case 'throttle_position':
        return { channel: 'tps' };
      case 'map':
      case 'map_kpa':
      case 'manifold_pressure':
      case 'boost':
        return { channel: 'map_kpa' };
      case 'ect':
      case 'ect_c':
      case 'coolant':
      case 'coolant_temp':
        return { channel: 'ect_c' };
      case 'iat':
      case 'iat_c':
      case 'air_temp':
      case 'intake_air_temp':
        return { channel: 'iat_c' };
      case 'battery':
      case 'battery_volts':
      case 'volts':
      case 'voltage':
        return { channel: 'battery_volts' };
      case 'ignition_advance':
      case 'ignition_advance_deg':
      case 'timing':
      case 'spark':
        return { channel: 'ignition_advance_deg' };
      case 'injector_pulse_width':
      case 'injector_pulse_width_ms':
      case 'pw':
      case 'pulse_width':
        return { channel: 'injector_pulse_width_ms' };
      case 'afr':
      case 'air_fuel_ratio':
      case 'lambda':
        return { channel: 'afr' };
      case 'speed':
      case 'speed_kph':
      case 'vehicle_speed':
        return { channel: 'speed_kph' };
      case 'gear':
      case 'gear_position':
        return { channel: 'gear' };
      default:
        return {
          error: {
            error: 'InvalidExplicitReference',
            details: { target: rawTarget, reason: `Unknown target '${target}'` },
          },
        };
    }
  }

  // UnitMatch mode
  const rawUnit = (unit ?? '').trim();
  if (!rawUnit) {
    return { error: { error: 'MissingUnit' } };
  }

  const u = rawUnit.toLowerCase();
  if (u === 'rpm' || u === 'r/min' || u === 'rev/min' || u === 'n') {
    return { channel: 'rpm' };
  }
  if (
    u === '%' ||
    u === 'pct' ||
    u === 'percent' ||
    u === 'tps' ||
    u === 'deg tps' ||
    u === 'throttle' ||
    u.includes('%') ||
    u.includes('tps')
  ) {
    return { channel: 'tps' };
  }
  if (
    u === 'kpa' ||
    u === 'mbar' ||
    u === 'bar' ||
    u === 'psi' ||
    u.includes('kpa') ||
    u.includes('mbar')
  ) {
    return { channel: 'map_kpa' };
  }
  if (u === 'c' || u === '°c' || u === 'deg c' || u === 'degc' || u === 'clt' || u === 'ect') {
    return { channel: 'ect_c' };
  }
  if (u === 'iat' || u === 'mat') {
    return { channel: 'iat_c' };
  }
  if (u === 'v' || u === 'volt' || u === 'volts' || u === 'voltage') {
    return { channel: 'battery_volts' };
  }
  if (u === 'ms' || u === 'msec' || u === 'pw') {
    return { channel: 'injector_pulse_width_ms' };
  }
  if (u === 'afr' || u === 'lambda' || u === 'λ') {
    return { channel: 'afr' };
  }
  if (u === 'km/h' || u === 'kph' || u === 'mph') {
    return { channel: 'speed_kph' };
  }
  if (u === 'gear') {
    return { channel: 'gear' };
  }

  return { error: { error: 'UnsupportedUnit', details: { unit: rawUnit } } };
}

/**
 * Pure fallback correlation algorithm for browser environment with canonical binding rules.
 */
function correlateFallback(
  frame: TelemetryFrame,
  mapDef: MapDef,
  tableId: string,
  bindingConfig?: TableBindingConfig
): CorrelationResult {
  const result: CorrelationResult = {
    is_valid: false,
    status: { status: 'Valid' },
    timestamp_ms: frame.timestamp_ms,
    sequence: frame.sequence,
    is_simulated: frame.is_simulated,
    selected_row: null,
    selected_col: null,
    row_coordinate: null,
    col_coordinate: null,
    cell_raw: null,
    cell_eng: null,
    table_id: tableId,
  };

  if (frame.status_flags.stale_sample) {
    result.status = { status: 'StaleSample' };
    return result;
  }
  if (frame.status_flags.communication_fault) {
    result.status = { status: 'CommunicationFault' };
    return result;
  }

  if (mapDef.rows === 0 || mapDef.cols === 0) {
    result.status = {
      status: 'DimensionMismatch',
      details: { reason: `Table dimensions cannot be zero: ${mapDef.rows}x${mapDef.cols}` },
    };
    return result;
  }

  // 1. Resolve Y-Axis (rows)
  if (mapDef.rows === 1) {
    result.selected_row = 0;
    result.row_coordinate = {
      interval_index: 0,
      fractional_pos: 0.0,
      nearest_index: 0,
      is_exact_match: true,
    };
  } else {
    if (!mapDef.y_axis) {
      result.status = {
        status: 'DimensionMismatch',
        details: { reason: `Table has ${mapDef.rows} rows but y_axis definition is missing` },
      };
      return result;
    }

    const yRes = resolveFallbackChannel(mapDef.y_axis.scaling?.unit, bindingConfig?.y_axis);
    if (yRes.error || !yRes.channel) {
      result.status = {
        status: 'UnboundChannel',
        details: { axis_role: 'y_axis', error: yRes.error ?? { error: 'MissingUnit' } },
      };
      return result;
    }

    const yVal = frame[yRes.channel] as number;
    if (!Number.isFinite(yVal)) {
      result.status = {
        status: 'AxisOutOfRange',
        details: {
          axis_role: 'y_axis',
          error: { error: 'NonFiniteValue', details: { value: yVal } },
        },
      };
      return result;
    }

    const yValues =
      mapDef.y_axis.values && mapDef.y_axis.values.length > 0
        ? mapDef.y_axis.values
        : [1000, 2000];

    const yCoord = resolveFallbackAxisCoordinate(yValues, yVal);
    if (yCoord.outOfRange) {
      result.status = {
        status: 'AxisOutOfRange',
        details: {
          axis_role: 'y_axis',
          error: {
            error: 'OutOfRange',
            details: { value: yVal, min: yValues[0], max: yValues[yValues.length - 1] },
          },
        },
      };
      return result;
    }

    result.selected_row = yCoord.index;
    result.row_coordinate = {
      interval_index: Math.floor(yCoord.frac),
      fractional_pos: yCoord.frac,
      nearest_index: yCoord.index,
      is_exact_match: yCoord.isExact,
    };
  }

  // 2. Resolve X-Axis (cols)
  if (mapDef.cols === 1) {
    result.selected_col = 0;
    result.col_coordinate = {
      interval_index: 0,
      fractional_pos: 0.0,
      nearest_index: 0,
      is_exact_match: true,
    };
  } else {
    if (!mapDef.x_axis) {
      result.status = {
        status: 'DimensionMismatch',
        details: { reason: `Table has ${mapDef.cols} cols but x_axis definition is missing` },
      };
      return result;
    }

    const xRes = resolveFallbackChannel(mapDef.x_axis.scaling?.unit, bindingConfig?.x_axis);
    if (xRes.error || !xRes.channel) {
      result.status = {
        status: 'UnboundChannel',
        details: { axis_role: 'x_axis', error: xRes.error ?? { error: 'MissingUnit' } },
      };
      return result;
    }

    const xVal = frame[xRes.channel] as number;
    if (!Number.isFinite(xVal)) {
      result.status = {
        status: 'AxisOutOfRange',
        details: {
          axis_role: 'x_axis',
          error: { error: 'NonFiniteValue', details: { value: xVal } },
        },
      };
      return result;
    }

    const xValues =
      mapDef.x_axis.values && mapDef.x_axis.values.length > 0
        ? mapDef.x_axis.values
        : [20, 40];

    const xCoord = resolveFallbackAxisCoordinate(xValues, xVal);
    if (xCoord.outOfRange) {
      result.status = {
        status: 'AxisOutOfRange',
        details: {
          axis_role: 'x_axis',
          error: {
            error: 'OutOfRange',
            details: { value: xVal, min: xValues[0], max: xValues[xValues.length - 1] },
          },
        },
      };
      return result;
    }

    result.selected_col = xCoord.index;
    result.col_coordinate = {
      interval_index: Math.floor(xCoord.frac),
      fractional_pos: xCoord.frac,
      nearest_index: xCoord.index,
      is_exact_match: xCoord.isExact,
    };
  }

  // Validate bounds
  if (
    result.selected_row !== null &&
    result.selected_col !== null &&
    result.selected_row < mapDef.rows &&
    result.selected_col < mapDef.cols
  ) {
    const liveVal =
      useMapStore.getState().activeMatrix?.[result.selected_row]?.[result.selected_col];
    if (liveVal !== undefined) {
      result.cell_eng = liveVal;
      result.cell_raw = liveVal;
    }
    result.is_valid = true;
    result.status = { status: 'Valid' };
  } else {
    result.status = {
      status: 'DimensionMismatch',
      details: {
        reason: `Selected cell (${result.selected_row}, ${result.selected_col}) out of bounds for table ${mapDef.rows}x${mapDef.cols}`,
      },
    };
  }

  return result;
}

/**
 * Invokes the Tauri `correlate_telemetry` IPC command with browser fallback.
 */
export async function bridgeCorrelateTelemetry(
  request: CorrelateTelemetryRequest
): Promise<CorrelateTelemetryResponse> {
  if (isDesktopApp()) {
    try {
      return await invoke<CorrelateTelemetryResponse>('correlate_telemetry', { request });
    } catch (err) {
      throw normalizeCorrelationError(err);
    }
  }

  // Fallback implementation
  const mapDef = request.map_def;
  const tableId = request.table_id || mapDef?.id;

  if (!tableId || !mapDef) {
    throw {
      error: 'MapNotFound',
      details: { table_id: tableId || '<unspecified>' },
    } as CorrelationCommandError;
  }

  if (mapDef.rows === 0 || mapDef.cols === 0) {
    throw {
      error: 'InvalidDimensions',
      details: {
        rows: mapDef.rows,
        cols: mapDef.cols,
        reason: 'Table rows and columns must be greater than zero',
      },
    } as CorrelationCommandError;
  }

  let frame = request.frame;
  let source = 'provided';
  if (!frame) {
    frame = useTelemetryStore.getState().latestFrame ?? undefined;
    source = 'live';
  }

  if (!frame) {
    throw {
      error: 'TelemetryUnavailable',
      details: { reason: 'No active live telemetry or playback frame available' },
    } as CorrelationCommandError;
  }

  const result = correlateFallback(frame, mapDef, tableId, request.binding_config);
  return {
    request_id: request.request_id,
    source_epoch: request.source_epoch,
    table_id: tableId,
    source,
    result,
  };
}

/**
 * Invokes the Tauri `compute_autotune_recommendations` IPC command with browser fallback.
 */
export async function bridgeComputeAutoTuneRecommendations(
  request: AutoTuneRequest
): Promise<AutoTuneResponse> {
  if (isDesktopApp()) {
    try {
      return await invoke<AutoTuneResponse>('compute_autotune_recommendations', { request });
    } catch (err) {
      throw normalizeAutoTuneError(err);
    }
  }

  // Fallback implementation
  const mapDef = request.map_def;
  const tableId = request.table_id || mapDef?.id;

  if (!tableId || !mapDef) {
    throw {
      error: 'MapNotFound',
      details: { table_id: tableId || '<unspecified>' },
    } as AutoTuneCommandError;
  }

  if (mapDef.rows === 0 || mapDef.cols === 0) {
    throw {
      error: 'InvalidDimensions',
      details: {
        rows: mapDef.rows,
        cols: mapDef.cols,
        reason: 'Table rows and columns must be greater than zero',
      },
    } as AutoTuneCommandError;
  }

  // Validate configuration
  const targetAfrVal =
    request.config.target_afr.source === 'Constant'
      ? request.config.target_afr.data
      : 14.7;

  if (targetAfrVal < 8.0 || targetAfrVal > 22.0 || !Number.isFinite(targetAfrVal)) {
    throw {
      error: 'InvalidConfiguration',
      details: { reason: `Target AFR ${targetAfrVal} is outside plausible combustion limits [8.0, 22.0]` },
    } as AutoTuneCommandError;
  }

  // Check category against complete blocked list matching Rust backend & autotuneEligibility.ts
  const cat = mapDef.category.toLowerCase();
  const name = mapDef.name.toLowerCase();
  const unit = mapDef.scaling.unit.toLowerCase();

  if (
    cat.includes('ignition') ||
    cat.includes('spark') ||
    cat.includes('timing') ||
    unit === 'deg' ||
    unit === '°' ||
    cat.includes('boost') ||
    name.includes('boost') ||
    cat.includes('knock') ||
    name.includes('knock') ||
    cat.includes('dwell') ||
    name.includes('dwell') ||
    cat.includes('targetafr') ||
    cat.includes('target_afr') ||
    name.includes('target afr')
  ) {
    return {
      request_id: request.request_id,
      table_id: tableId,
      source_provenance: 'fallback',
      report: {
        table_id: tableId,
        total_frames_scanned: 0,
        eligible_frames_used: 0,
        cells: [],
        rejections: {
          rejected_stale: 0,
          rejected_fault: 0,
          rejected_out_of_bounds: 0,
          rejected_cold_engine: 0,
          rejected_fuel_cut: 0,
          rejected_transient_throttle: 0,
          rejected_sensor_out_of_range: 0,
          rejected_non_finite: 0,
        },
        blocker_reason: `Incompatible table: Category "${mapDef.category}" cannot be tuned via AFR correction formula`,
      },
    };
  }

  // Resolve frames
  const frames = request.frames ?? useTelemetryStore.getState().history;
  const source_provenance = request.frames ? 'provided' : 'datalog_history';

  let eligible_frames_used = 0;
  const cells: CellRecommendation[] = [];

  for (let r = 0; r < mapDef.rows; r++) {
    for (let c = 0; c < mapDef.cols; c++) {
      const orig = useMapStore.getState().activeMatrix?.[r]?.[c] ?? 80.0;
      const matchingFrames = frames.filter((f) => {
        const corr = correlateFallback(f, mapDef, tableId);
        return corr.is_valid && corr.selected_row === r && corr.selected_col === c;
      });

      const count = matchingFrames.length;
      eligible_frames_used += count;

      if (count < request.config.min_samples_per_cell) {
        cells.push({
          row: r,
          col: c,
          status: 'InsufficientEvidence',
          sample_count: count,
          effective_sample_count: count,
          original_value: orig,
          proposed_value: orig,
          delta_percentage: 0.0,
          unit: mapDef.scaling.unit || '%',
          mean_measured_afr: null,
          target_afr: targetAfrVal,
          std_dev_afr: 0.0,
          min_measured_afr: null,
          max_measured_afr: null,
          is_clamped: false,
          warning: `Insufficient samples (${count} < min ${request.config.min_samples_per_cell})`,
        });
      } else {
        const avgAfr = matchingFrames.reduce((acc, f) => acc + f.afr, 0) / count;
        const factor = avgAfr / targetAfrVal;
        const maxDelta = request.config.max_cell_delta_pct / 100.0;
        const clampedFactor = Math.min(1.0 + maxDelta, Math.max(1.0 - maxDelta, factor));
        const proposed = Math.round(orig * clampedFactor * 10) / 10;
        const delta = Math.round(((proposed / orig - 1.0) * 100) * 100) / 100;
        const isClamped = factor !== clampedFactor;

        cells.push({
          row: r,
          col: c,
          status: Math.abs(delta) < 0.2 ? 'NoChange' : 'Ready',
          sample_count: count,
          effective_sample_count: count,
          original_value: orig,
          proposed_value: proposed,
          delta_percentage: delta,
          unit: mapDef.scaling.unit || '%',
          mean_measured_afr: avgAfr,
          target_afr: targetAfrVal,
          std_dev_afr: 0.1,
          min_measured_afr: avgAfr,
          max_measured_afr: avgAfr,
          is_clamped: isClamped,
          warning: null,
        });
      }
    }
  }

  const report: RecommendationReport = {
    table_id: tableId,
    total_frames_scanned: frames.length,
    eligible_frames_used,
    cells,
    rejections: {
      rejected_stale: 0,
      rejected_fault: 0,
      rejected_out_of_bounds: 0,
      rejected_cold_engine: 0,
      rejected_fuel_cut: 0,
      rejected_transient_throttle: 0,
      rejected_sensor_out_of_range: 0,
      rejected_non_finite: 0,
    },
    blocker_reason: null,
  };

  return {
    request_id: request.request_id,
    table_id: tableId,
    source_provenance,
    report,
  };
}
