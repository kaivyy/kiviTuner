/**
 * Mathematical utilities and coordinate projections for 3D Surface Mesh rendering.
 * Supports Euler camera rotation, perspective/isometric projection, and Painter's depth sorting.
 */

import { getHeatmapRgb } from './colorRamp.ts';

export interface Point3D {
  x: number;
  y: number;
  z: number;
}

export interface ProjectedPoint {
  x: number;
  y: number;
  zDepth: number;
}

export interface Quad3D {
  r: number;
  c: number;
  p0: Point3D;
  p1: Point3D;
  p2: Point3D;
  p3: Point3D;
  avgZ: number;
  avgValue: number;
}

export interface CameraState {
  yaw: number;
  pitch: number;
  scale: number;
  panX: number;
  panY: number;
}

export type SurfacePalette = 'classic' | 'thermal' | 'spectrum' | 'monochrome';

export const CAMERA_PRESETS: Record<'ISOMETRIC' | 'TOP_DOWN' | 'FRONT_RPM' | 'SIDE_TPS', CameraState> = {
  ISOMETRIC: {
    yaw: -Math.PI / 4,
    pitch: Math.PI / 6,
    scale: 1,
    panX: 0,
    panY: 0,
  },
  TOP_DOWN: {
    yaw: 0,
    pitch: Math.PI / 2,
    scale: 1,
    panX: 0,
    panY: 0,
  },
  FRONT_RPM: {
    yaw: 0,
    pitch: 0,
    scale: 1,
    panX: 0,
    panY: 0,
  },
  SIDE_TPS: {
    yaw: Math.PI / 2,
    pitch: 0,
    scale: 1,
    panX: 0,
    panY: 0,
  },
} as const;

/**
 * Rotates a 3D point around an arbitrary center point using Yaw (Y-axis) then Pitch (X-axis).
 */
export function rotateEuler(
  p: Point3D,
  center: Point3D,
  yaw: number,
  pitch: number
): Point3D {
  const dx = p.x - center.x;
  const dy = p.y - center.y;
  const dz = p.z - center.z;

  // Yaw rotation around Y axis
  const cosYaw = Math.cos(yaw);
  const sinYaw = Math.sin(yaw);
  const x1 = dx * cosYaw + dz * sinYaw;
  const y1 = dy;
  const z1 = -dx * sinYaw + dz * cosYaw;

  // Pitch rotation around X axis
  const cosPitch = Math.cos(pitch);
  const sinPitch = Math.sin(pitch);
  const x2 = x1;
  const y2 = y1 * cosPitch - z1 * sinPitch;
  const z2 = y1 * sinPitch + z1 * cosPitch;

  return {
    x: center.x + x2,
    y: center.y + y2,
    z: center.z + z2,
  };
}

/**
 * Projects a 3D point to 2D canvas coordinates using camera transform and canvas dimensions.
 */
export function projectPoint(
  p: Point3D,
  center: Point3D,
  cam: CameraState,
  canvasWidth: number,
  canvasHeight: number,
  yScaling: number = 1
): ProjectedPoint {
  const pScaled: Point3D = {
    x: p.x,
    y: p.y * yScaling,
    z: p.z,
  };

  const centerScaled: Point3D = {
    x: center.x,
    y: center.y * yScaling,
    z: center.z,
  };

  const rotated = rotateEuler(pScaled, centerScaled, cam.yaw, cam.pitch);

  const viewX = rotated.x - centerScaled.x;
  const viewY = rotated.y - centerScaled.y;
  const viewZ = rotated.z - centerScaled.z;

  const projX = canvasWidth / 2 + cam.panX + viewX * cam.scale;
  const projY = canvasHeight / 2 + cam.panY - viewY * cam.scale;

  return {
    x: projX,
    y: projY,
    zDepth: viewZ,
  };
}

/**
 * Generates an array of (N-1) * (M-1) 3D quad polygons from an N x M matrix.
 */
export function buildQuads(matrix: number[][]): Quad3D[] {
  if (!matrix || matrix.length < 2 || !matrix[0] || matrix[0].length < 2) {
    return [];
  }

  const rows = matrix.length;
  const cols = matrix[0].length;
  const quads: Quad3D[] = [];

  for (let r = 0; r < rows - 1; r++) {
    for (let c = 0; c < cols - 1; c++) {
      const v00 = matrix[r][c];
      const v01 = matrix[r][c + 1];
      const v11 = matrix[r + 1][c + 1];
      const v10 = matrix[r + 1][c];

      const p0: Point3D = { x: c, y: v00, z: r };
      const p1: Point3D = { x: c + 1, y: v01, z: r };
      const p2: Point3D = { x: c + 1, y: v11, z: r + 1 };
      const p3: Point3D = { x: c, y: v10, z: r + 1 };

      const avgZ = r + 0.5;
      const avgValue = (v00 + v01 + v11 + v10) / 4;

      quads.push({
        r,
        c,
        p0,
        p1,
        p2,
        p3,
        avgZ,
        avgValue,
      });
    }
  }

  return quads;
}

/**
 * Sorts quads in Painter's algorithm order: farthest first (largest view depth Z') to nearest.
 */
export function sortQuadsByDepth(
  quads: Quad3D[],
  center: Point3D,
  cam: CameraState,
  yScaling: number = 1
): Quad3D[] {
  const centerScaled: Point3D = {
    x: center.x,
    y: center.y * yScaling,
    z: center.z,
  };

  const quadsWithDepth = quads.map((quad) => {
    const midX = quad.c + 0.5;
    const midY = quad.avgValue * yScaling;
    const midZ = quad.r + 0.5;

    const rot = rotateEuler(
      { x: midX, y: midY, z: midZ },
      centerScaled,
      cam.yaw,
      cam.pitch
    );

    const zDepth = rot.z - centerScaled.z;
    return { quad, zDepth };
  });

  // Painter's algorithm: sort ascending by zDepth (farthest rendered first, most negative zDepth in right-hand camera coordinates)
  quadsWithDepth.sort((a, b) => a.zDepth - b.zDepth);

  return quadsWithDepth.map((item) => item.quad);
}

/**
 * Finds the geometric center of a 2D matrix in 3D grid space.
 */
export function calculateMatrixCenter(matrix: number[][]): Point3D {
  if (!matrix || matrix.length === 0 || !matrix[0] || matrix[0].length === 0) {
    return { x: 0, y: 0, z: 0 };
  }

  const rows = matrix.length;
  const cols = matrix[0].length;
  let minVal = Infinity;
  let maxVal = -Infinity;

  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      const val = matrix[r][c];
      if (val < minVal) minVal = val;
      if (val > maxVal) maxVal = val;
    }
  }

  if (!Number.isFinite(minVal)) minVal = 0;
  if (!Number.isFinite(maxVal)) maxVal = 0;

  return {
    x: (cols - 1) / 2,
    y: (minVal + maxVal) / 2,
    z: (rows - 1) / 2,
  };
}

/**
 * Returns minimum and maximum numeric values in a matrix.
 */
export function getMatrixExtremes(matrix: number[][]): { min: number; max: number } {
  let min = Infinity;
  let max = -Infinity;

  for (let r = 0; r < matrix.length; r++) {
    for (let c = 0; c < matrix[r].length; c++) {
      const val = matrix[r][c];
      if (val < min) min = val;
      if (val > max) max = val;
    }
  }

  return {
    min: Number.isFinite(min) ? min : 0,
    max: Number.isFinite(max) ? max : 1,
  };
}

/**
 * Computes an appropriate Y scaling factor so surface height is proportional to grid dimensions.
 */
export function calculateAutoYScaling(matrix: number[][]): number {
  if (!matrix.length || !matrix[0]?.length) return 1;
  const { min, max } = getMatrixExtremes(matrix);
  const valueSpan = max - min;
  if (valueSpan <= 0.0001) return 1;

  const gridSpan = Math.max(matrix.length, matrix[0].length);
  // Target elevation is approximately 40% of grid span
  const targetElevation = Math.max(2, gridSpan * 0.4);
  return targetElevation / valueSpan;
}

/**
 * Generates an RGBA color string according to the selected palette.
 */
export function getPaletteColor(
  value: number,
  min: number,
  max: number,
  palette: SurfacePalette = 'classic',
  alpha: number = 1.0
): string {
  if (!Number.isFinite(value) || !Number.isFinite(min) || !Number.isFinite(max) || min === max) {
    return `rgba(30, 41, 59, ${alpha})`;
  }

  const t = Math.max(0, Math.min(1, (value - min) / (max - min)));

  switch (palette) {
    case 'classic': {
      const { r, g, b } = getHeatmapRgb(value, min, max);
      return `rgba(${r}, ${g}, ${b}, ${alpha})`;
    }

    case 'thermal': {
      // 4-stop thermal ramp: Dark Navy -> Deep Red -> Amber/Orange -> Yellow-White
      let r = 0;
      let g = 0;
      let b = 0;
      if (t < 0.33) {
        const seg = t / 0.33;
        r = Math.round(11 + (153 - 11) * seg);
        g = Math.round(9 + (27 - 9) * seg);
        b = Math.round(43 + (27 - 43) * seg);
      } else if (t < 0.66) {
        const seg = (t - 0.33) / 0.33;
        r = Math.round(153 + (234 - 153) * seg);
        g = Math.round(27 + (88 - 27) * seg);
        b = Math.round(27 + (12 - 27) * seg);
      } else {
        const seg = (t - 0.66) / 0.34;
        r = Math.round(234 + (255 - 234) * seg);
        g = Math.round(88 + (250 - 88) * seg);
        b = Math.round(12 + (200 - 12) * seg);
      }
      return `rgba(${r}, ${g}, ${b}, ${alpha})`;
    }

    case 'spectrum': {
      // Blue (240 deg) -> Green (120 deg) -> Red (0 deg)
      const hue = Math.round((1 - t) * 240);
      return `hsla(${hue}, 90%, 50%, ${alpha})`;
    }

    case 'monochrome': {
      // Dark slate -> Ice Cyan -> White
      const r = Math.round(15 + (240 - 15) * t);
      const g = Math.round(23 + (245 - 23) * t);
      const b = Math.round(42 + (255 - 42) * t);
      return `rgba(${r}, ${g}, ${b}, ${alpha})`;
    }
  }
}

/**
 * Finds the closest grid cell to a given screen (mouseX, mouseY) coordinate.
 */
export function findClosestCell(
  mouseX: number,
  mouseY: number,
  matrix: number[][],
  center: Point3D,
  cam: CameraState,
  canvasWidth: number,
  canvasHeight: number,
  yScaling: number = 1
): { r: number; c: number; distance: number } | null {
  if (!matrix.length || !matrix[0]?.length) return null;

  let closest: { r: number; c: number; distance: number } | null = null;
  let minDistanceSq = Infinity;

  for (let r = 0; r < matrix.length; r++) {
    for (let c = 0; c < matrix[r].length; c++) {
      const p: Point3D = { x: c, y: matrix[r][c], z: r };
      const proj = projectPoint(p, center, cam, canvasWidth, canvasHeight, yScaling);
      const dx = proj.x - mouseX;
      const dy = proj.y - mouseY;
      const distSq = dx * dx + dy * dy;

      if (distSq < minDistanceSq) {
        minDistanceSq = distSq;
        closest = { r, c, distance: Math.sqrt(distSq) };
      }
    }
  }

  return closest;
}
