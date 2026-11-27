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
 * Returns raw RGB `[r, g, b]` components in `[0, 255]` for a palette & value.
 */
export function getPaletteRgb(
  value: number,
  min: number,
  max: number,
  palette: SurfacePalette = 'classic'
): [number, number, number] {
  if (!Number.isFinite(value) || !Number.isFinite(min) || !Number.isFinite(max) || min === max) {
    return [30, 41, 59];
  }

  const t = Math.max(0, Math.min(1, (value - min) / (max - min)));

  switch (palette) {
    case 'classic': {
      const { r, g, b } = getHeatmapRgb(value, min, max);
      return [r, g, b];
    }

    case 'thermal': {
      if (t < 0.33) {
        const seg = t / 0.33;
        return [
          Math.round(11 + (153 - 11) * seg),
          Math.round(9 + (27 - 9) * seg),
          Math.round(43 + (27 - 43) * seg),
        ];
      }
      if (t < 0.66) {
        const seg = (t - 0.33) / 0.33;
        return [
          Math.round(153 + (234 - 153) * seg),
          Math.round(27 + (88 - 27) * seg),
          Math.round(27 + (12 - 27) * seg),
        ];
      }
      const seg = (t - 0.66) / 0.34;
      return [
        Math.round(234 + (255 - 234) * seg),
        Math.round(88 + (250 - 88) * seg),
        Math.round(12 + (200 - 12) * seg),
      ];
    }

    case 'spectrum': {
      // Blue (240 deg) -> Green (120 deg) -> Red (0 deg), s=0.90, l=0.50
      const hue = (1 - t) * 240;
      const c = 0.9;
      const x = c * (1 - Math.abs(((hue / 60) % 2) - 1));
      const m = 0.05;
      let r1 = 0;
      let g1 = 0;
      let b1 = 0;
      if (hue < 60) {
        r1 = c;
        g1 = x;
      } else if (hue < 120) {
        r1 = x;
        g1 = c;
      } else if (hue < 180) {
        g1 = c;
        b1 = x;
      } else {
        g1 = x;
        b1 = c;
      }
      return [
        Math.round((r1 + m) * 255),
        Math.round((g1 + m) * 255),
        Math.round((b1 + m) * 255),
      ];
    }

    case 'monochrome': {
      return [
        Math.round(15 + (240 - 15) * t),
        Math.round(23 + (245 - 23) * t),
        Math.round(42 + (255 - 42) * t),
      ];
    }
  }
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
  const [r, g, b] = getPaletteRgb(value, min, max, palette);
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}

/**
 * Computes a directional Lambertian illumination factor in `[0.72, 1.18]` from a quad's
 * upward surface normal so ridges, peaks, and valleys read clearly without neon glare.
 */
export function computeQuadShade(quad: Quad3D, yScaling: number = 1): number {
  const ux = quad.p1.x - quad.p0.x;
  const uy = (quad.p1.y - quad.p0.y) * yScaling;
  const uz = quad.p1.z - quad.p0.z;

  const vx = quad.p3.x - quad.p0.x;
  const vy = (quad.p3.y - quad.p0.y) * yScaling;
  const vz = quad.p3.z - quad.p0.z;

  // v x u yields a +Y-facing normal in our (X=col, Y=value, Z=row) grid space.
  const nx = vy * uz - vz * uy;
  const ny = vz * ux - vx * uz;
  const nz = vx * uy - vy * ux;
  const nLen = Math.hypot(nx, ny, nz);
  if (nLen <= 1e-9 || !Number.isFinite(nLen)) return 1.0;

  const lx = 0.42;
  const ly = 0.84;
  const lz = 0.34;
  const lLen = Math.hypot(lx, ly, lz);
  const dot = (nx * lx + ny * ly + nz * lz) / (nLen * lLen);
  const clampedDot = Math.max(0, Math.min(1, dot));
  return 0.72 + clampedDot * 0.46;
}

/**
 * Returns an RGBA string shaded by directional relief factor in `[0.72, 1.18]`.
 */
export function getShadedPaletteColor(
  value: number,
  min: number,
  max: number,
  palette: SurfacePalette = 'classic',
  shade: number = 1.0,
  alpha: number = 1.0
): string {
  const [r, g, b] = getPaletteRgb(value, min, max, palette);
  const clampByte = (v: number) => Math.max(0, Math.min(255, Math.round(v)));
  return `rgba(${clampByte(r * shade)}, ${clampByte(g * shade)}, ${clampByte(b * shade)}, ${alpha})`;
}

/**
 * Picks evenly distributed axis tick indices always including first (`0`) and last (`count - 1`).
 */
export function computeAxisTickIndices(count: number, maxTicks: number = 6): number[] {
  if (count <= 0 || maxTicks <= 0) return [];
  if (count === 1) return [0];
  if (count <= maxTicks) {
    return Array.from({ length: count }, (_, i) => i);
  }
  const ticks = new Set<number>([0, count - 1]);
  const steps = Math.max(1, maxTicks - 1);
  for (let i = 1; i < steps; i++) {
    ticks.add(Math.round((i * (count - 1)) / steps));
  }
  return Array.from(ticks).sort((a, b) => a - b);
}

/**
 * Converts `1xN` / `Nx1` 1D curves into a clean `2xN` ribbon so 1D tables render meaningfully in 3D.
 */
export function normalizeMatrixFor3D(matrix: number[][]): {
  matrix: number[][];
  is1DRibbon: boolean;
  transpose1D: boolean;
} {
  if (!matrix || matrix.length === 0 || !matrix[0] || matrix[0].length === 0) {
    return { matrix: [], is1DRibbon: false, transpose1D: false };
  }
  const rows = matrix.length;
  const cols = matrix[0].length;
  if (rows === 1 && cols >= 2) {
    const row = [...matrix[0]];
    return { matrix: [row, [...row]], is1DRibbon: true, transpose1D: false };
  }
  if (rows >= 2 && cols === 1) {
    const colVals = matrix.map((r) => r[0] ?? 0);
    return { matrix: [colVals, [...colVals]], is1DRibbon: true, transpose1D: true };
  }
  return { matrix, is1DRibbon: false, transpose1D: false };
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
