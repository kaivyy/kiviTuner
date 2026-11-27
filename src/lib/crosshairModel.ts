import type { CorrelationResult } from '../types/correlation.ts';
import type { CameraState, Point3D } from './surface3d.ts';
import { projectPoint } from './surface3d.ts';

/**
 * Parameters for resolving a discrete cell index from correlation state.
 */
export interface ResolveCorrelatedCellParams {
  matrixDimensions: { numRows: number; numCols: number };
  activeCorrelation: CorrelationResult | null;
  targetTableId?: string | null;
  currentTableId?: string | null;
}

/**
 * Discrete row and column coordinates for a calibrated cell.
 */
export interface CellCoordinate {
  r: number;
  c: number;
}

/**
 * Deterministically resolves the active telemetry correlation to a discrete map cell index (r, c).
 *
 * Enforces strict validation:
 * - Rejects null, invalid, stale, communication-faulted, or out-of-range correlations.
 * - Enforces table ID matching between correlation state and target view.
 * - Validates array index bounds [0..numRows) and [0..numCols).
 */
export function resolveCorrelatedCell(
  params: ResolveCorrelatedCellParams
): CellCoordinate | null {
  const { matrixDimensions, activeCorrelation, targetTableId, currentTableId } = params;

  if (!activeCorrelation || !activeCorrelation.is_valid) {
    return null;
  }

  // Ensure correlation status is strictly Valid
  if (activeCorrelation.status.status !== 'Valid') {
    return null;
  }

  // If table IDs are specified, ensure they match
  if (targetTableId !== undefined && currentTableId !== undefined) {
    if (targetTableId !== currentTableId) {
      return null;
    }
  }

  // If correlation result itself carries a table_id, verify match
  if (activeCorrelation.table_id && currentTableId) {
    if (activeCorrelation.table_id !== currentTableId) {
      return null;
    }
  }

  const row = activeCorrelation.selected_row;
  const col = activeCorrelation.selected_col;

  if (row === null || row === undefined || col === null || col === undefined) {
    return null;
  }

  if (!Number.isInteger(row) || !Number.isInteger(col)) {
    return null;
  }

  const { numRows, numCols } = matrixDimensions;
  if (row < 0 || row >= numRows || col < 0 || col >= numCols) {
    return null;
  }

  return { r: row, c: col };
}

/**
 * Parameters for computing cell visual crosshair state.
 */
export interface HeatmapCellCrosshairParams {
  r: number;
  c: number;
  correlatedCell: CellCoordinate | null;
  isSelected?: boolean;
  isAnchor?: boolean;
  isEditing?: boolean;
}

/**
 * Heatmap cell crosshair styling and accessibility metadata.
 */
export interface HeatmapCellCrosshairState {
  isCorrelated: boolean;
  reticleClass: string;
  ariaAttributes: {
    'aria-current'?: 'location';
  };
  tooltip: string | undefined;
}

/**
 * Computes visual classes, accessibility attributes, and semantic descriptions
 * for a heatmap cell in relation to the telemetry crosshair.
 */
export function getHeatmapCellCrosshairState(
  params: HeatmapCellCrosshairParams
): HeatmapCellCrosshairState {
  const { r, c, correlatedCell } = params;
  const isCorrelated = Boolean(
    correlatedCell && correlatedCell.r === r && correlatedCell.c === c
  );

  if (!isCorrelated) {
    return {
      isCorrelated: false,
      reticleClass: '',
      ariaAttributes: {},
      tooltip: undefined,
    };
  }

  return {
    isCorrelated: true,
    reticleClass: 'ring-2 ring-amber-400 ring-offset-1 ring-offset-slate-900 z-20',
    ariaAttributes: {
      'aria-current': 'location',
    },
    tooltip: `Telemetry operating point: Row ${r}, Col ${c}`,
  };
}

/**
 * Parameters for projecting 3D surface crosshair geometry.
 */
export interface Surface3DCrosshairParams {
  r: number;
  c: number;
  matrix: number[][];
  minVal: number;
  center: Point3D;
  camera: CameraState;
  width: number;
  height: number;
  effectiveYScaling: number;
}

/**
 * 2D projected coordinates for 3D crosshair reticle.
 */
export interface Surface3DCrosshairGeometry {
  topProj: { x: number; y: number };
  baseProj: { x: number; y: number };
  isValid: boolean;
}

/**
 * Projects the 3D surface coordinate for a correlated cell into 2D canvas coordinates.
 *
 * Preserves actual geometry:
 * - Column maps to X axis
 * - Elevation maps to Y axis (scaled)
 * - Row maps to Z axis
 */
export function get3DSurfaceCrosshairGeometry(
  params: Surface3DCrosshairParams
): Surface3DCrosshairGeometry | null {
  const {
    r,
    c,
    matrix,
    minVal,
    center,
    camera,
    width,
    height,
    effectiveYScaling,
  } = params;

  if (!matrix || matrix.length === 0 || !matrix[0] || matrix[0].length === 0) {
    return null;
  }

  const numRows = matrix.length;
  const numCols = matrix[0].length;

  if (r < 0 || r >= numRows || c < 0 || c >= numCols) {
    return null;
  }

  const val = matrix[r][c];
  if (val === undefined || !Number.isFinite(val)) {
    return null;
  }

  const topPoint: Point3D = { x: c, y: val, z: r };
  const basePoint: Point3D = { x: c, y: minVal, z: r };

  const topProj = projectPoint(
    topPoint,
    center,
    camera,
    width,
    height,
    effectiveYScaling
  );
  const baseProj = projectPoint(
    basePoint,
    center,
    camera,
    width,
    height,
    effectiveYScaling
  );

  if (
    !Number.isFinite(topProj.x) ||
    !Number.isFinite(topProj.y) ||
    !Number.isFinite(baseProj.x) ||
    !Number.isFinite(baseProj.y)
  ) {
    return null;
  }

  return {
    topProj,
    baseProj,
    isValid: true,
  };
}
