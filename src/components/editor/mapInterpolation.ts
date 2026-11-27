/**
 * Read-only map interpolation controller.
 *
 * ## Coordinate domain
 *
 * The backend `interpolate_map_at` resolves `x`/`y` against **engineering axis
 * values** taken from the XDF, falling back to cell indices when the axis
 * carries no usable breakpoints. `axis_breakpoints` in `commands.rs` applies
 * exactly one rule:
 *
 *     values.len() == len && len >= 2  ->  use values
 *     otherwise                        ->  0..len
 *
 * This module mirrors that rule verbatim. If the two ever disagree the same
 * query returns a value from a different place in the table, with no error to
 * show for it — which is why `resolveAxisBreakpoints` is exported and tested
 * against the Rust behaviour rather than inlined.
 *
 * ## What this module deliberately does not do
 *
 * It does not read `generateAxisLabels` (`src/lib/matrixRom.ts`), which
 * synthesises plausible-looking RPM/TPS ranges that are not present in the XDF.
 * Feeding those to the backend would query a coordinate domain the ROM does not
 * have.
 *
 * The result is never written back to a table; interpolation is an observation.
 */

import { bridgeInterpolateMapAt, type MapInterpolationResult } from '../../lib/tauriBridge.ts';

/** Axis domain actually used for a query. */
export type AxisSource = 'xdf' | 'index';

/** Minimal shape of a map the controller needs; avoids depending on the full `MapDef`. */
export interface InterpolatableMap {
  rows: number;
  cols: number;
  xAxisValues: number[] | null;
  yAxisValues: number[] | null;
}

export type MapInterpolationState =
  | {
      kind: 'ok';
      result: MapInterpolationResult;
      axisSource: { x: AxisSource; y: AxisSource };
      axisValues: { x: number[]; y: number[] };
    }
  | { kind: 'no-map'; message: string }
  | { kind: 'invalid-coordinate'; message: string }
  | { kind: 'degenerate'; message: string }
  | { kind: 'dimension-mismatch'; message: string }
  | { kind: 'error'; message: string };

/**
 * Applies the backend's breakpoint rule.
 *
 * Kept identical to `axis_breakpoints` in `src-tauri/src/commands.rs`; a
 * divergence here would silently move the query into the wrong domain.
 */
export function resolveAxisBreakpoints(
  values: number[] | null | undefined,
  len: number
): { values: number[]; source: AxisSource } {
  if (
    Array.isArray(values) &&
    values.length === len &&
    len >= 2 &&
    values.every((v) => typeof v === 'number' && Number.isFinite(v))
  ) {
    return { values: values.slice(), source: 'xdf' };
  }
  return { values: Array.from({ length: len }, (_, i) => i), source: 'index' };
}

/**
 * Interpolates `matrix` at engineering coordinate (`x`, `y`).
 *
 * All input validation happens before the bridge is called, so a `null` coming
 * back genuinely means the bridge could not compute rather than "bad input".
 */
export async function computeMapInterpolation(
  mapId: string,
  map: InterpolatableMap | null,
  matrix: number[][] | null,
  x: number,
  y: number
): Promise<MapInterpolationState> {
  if (!mapId) {
    // Without an id the desktop path cannot resolve the map in Rust, the invoke
    // fails, and the bridge silently falls back to JS — so the backend command
    // this batch integrates would never actually run.
    return { kind: 'no-map', message: 'No map selected.' };
  }

  if (!map) {
    return { kind: 'no-map', message: 'No map selected.' };
  }

  if (!matrix || matrix.length === 0) {
    return { kind: 'no-map', message: 'No map data available to interpolate.' };
  }

  if (!Number.isFinite(x) || !Number.isFinite(y)) {
    return {
      kind: 'invalid-coordinate',
      message: 'Coordinates must be finite numbers.',
    };
  }

  if (map.rows < 2 || map.cols < 2) {
    return {
      kind: 'degenerate',
      message: `A ${map.rows}×${map.cols} map has no interval to interpolate over.`,
    };
  }

  if (matrix.length !== map.rows || matrix.some((row) => row.length !== map.cols)) {
    return {
      kind: 'dimension-mismatch',
      message: 'Map matrix shape does not match the map definition.',
    };
  }

  const xAxis = resolveAxisBreakpoints(map.xAxisValues, map.cols);
  const yAxis = resolveAxisBreakpoints(map.yAxisValues, map.rows);

  try {
    const result = await bridgeInterpolateMapAt(
      mapId,
      matrix,
      xAxis.values,
      yAxis.values,
      x,
      y
    );

    if (!result) {
      return {
        kind: 'error',
        message: 'Interpolation unavailable for this map.',
      };
    }

    if (!Number.isFinite(result.value)) {
      return {
        kind: 'error',
        message: 'Interpolation produced a non-finite value.',
      };
    }

    return {
      kind: 'ok',
      result,
      axisSource: { x: xAxis.source, y: yAxis.source },
      axisValues: { x: xAxis.values, y: yAxis.values },
    };
  } catch (err) {
    return {
      kind: 'error',
      message: err instanceof Error ? err.message : String(err),
    };
  }
}

/**
 * Human label for the domain an axis query must be expressed in.
 *
 * The user cannot tell from the UI whether to type an engineering value or a
 * cell index, so the panel states it. An empty axis has no valid index at all —
 * rendering `index 0–-1` there would invite a query that cannot succeed.
 */
export function describeAxisDomain(axis: { values: number[]; source: AxisSource }): string {
  if (axis.values.length === 0) return 'empty';
  if (axis.source === 'index') return `index 0–${axis.values.length - 1}`;
  return `${axis.values[0]}–${axis.values[axis.values.length - 1]}`;
}

/**
 * One-line summary of an interpolation result.
 *
 * The bracketing cells are named so the user can confirm the value came from
 * where they expected, and clamping is stated because a clamped value is an
 * extrapolation the ROM does not actually contain.
 */
export function describeInterpolation(result: MapInterpolationResult): string {
  const value = result.value.toFixed(1);
  const cells = `col ${result.x_low}–${result.x_high}, row ${result.y_low}–${result.y_high}`;

  const notes: string[] = [];
  if (result.x_clamped || result.y_clamped) {
    const axes = [result.x_clamped ? 'X' : null, result.y_clamped ? 'Y' : null]
      .filter(Boolean)
      .join(' and ');
    notes.push(`${axes} clamped to the axis range`);
  }

  return notes.length > 0 ? `${value} (${cells}); ${notes.join(', ')}` : `${value} (${cells})`;
}
