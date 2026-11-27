/**
 * Read-only smoothing preview controller.
 *
 * ## Safety
 *
 * This module never produces an edit. Its output is rendered as a disposable
 * preview and is never passed to `onChangeMatrix`, `MemoryMap`, map
 * definitions, or ROM bytes.
 *
 * ## Why alpha is validated here
 *
 * The Rust core rejects `alpha <= 0` and `alpha >= 1`, but **accepts `NaN`**
 * (both comparisons are false for NaN) and returns a matrix filled with NaN.
 * The frontend bridge has the same gap. Validating here is the only place that
 * can guarantee the UI never renders NaN as calibration data.
 *
 * ## Why `null` is not treated as "no data"
 *
 * `bridgeAnalyzeMapSmoothing` returns `null` for three different conditions:
 * no map supplied, invalid alpha, or an IPC failure that fell through to the
 * in-process fallback with unusable input. It cannot distinguish them. We
 * therefore validate all inputs *before* calling it, so a `null` coming back
 * genuinely means the bridge could not compute.
 */

import { bridgeAnalyzeMapSmoothing, type MapSmoothingReport } from '../../lib/tauriBridge.ts';

export type SmoothingPreviewState =
  | { kind: 'ok'; report: MapSmoothingReport }
  | { kind: 'invalid-alpha'; message: string }
  | { kind: 'no-map'; message: string }
  | { kind: 'error'; message: string };

/** Alpha must be finite and strictly inside (0, 1). */
function isValidAlpha(alpha: number): boolean {
  return Number.isFinite(alpha) && alpha > 0 && alpha < 1;
}

/** True when the matrix is non-empty and every cell is a finite number. */
export function isPreviewRenderable(matrix: number[][] | null | undefined): boolean {
  if (!matrix || matrix.length === 0) return false;
  for (const row of matrix) {
    if (!Array.isArray(row)) return false;
    for (const v of row) {
      if (typeof v !== 'number' || !Number.isFinite(v)) return false;
    }
  }
  return true;
}

/**
 * Computes a smoothing preview for the given map.
 *
 * The `matrix` is the currently displayed data. On desktop the bridge forwards
 * only `mapId` and `alpha` to Rust, which reads the map from application state;
 * the matrix is used by the in-process fallback. Both paths must describe the
 * same map — the caller is responsible for passing the matrix of `mapId`.
 */
export async function computeSmoothingPreview(
  mapId: string,
  matrix: number[][] | null,
  alpha: number
): Promise<SmoothingPreviewState> {
  if (!isValidAlpha(alpha)) {
    return {
      kind: 'invalid-alpha',
      message: 'Smoothing factor must be greater than 0 and less than 1.',
    };
  }

  if (!matrix || matrix.length === 0) {
    return { kind: 'no-map', message: 'No map data available to preview.' };
  }

  if (!isPreviewRenderable(matrix)) {
    return {
      kind: 'error',
      message: 'Map contains non-finite values and cannot be previewed.',
    };
  }

  try {
    const report = await bridgeAnalyzeMapSmoothing(mapId, matrix, alpha);
    if (!report) {
      return {
        kind: 'error',
        message: 'Smoothing preview unavailable for this map.',
      };
    }

    // Defend against a NaN-producing path that bypassed validation.
    if (!isPreviewRenderable(report.smoothed) || !Number.isFinite(report.max_abs_curvature)) {
      return {
        kind: 'error',
        message: 'Smoothing produced non-finite values; preview withheld.',
      };
    }

    return { kind: 'ok', report };
  } catch (err) {
    return {
      kind: 'error',
      message: err instanceof Error ? err.message : String(err),
    };
  }
}
