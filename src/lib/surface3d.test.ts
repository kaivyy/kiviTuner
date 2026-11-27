import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  rotateEuler,
  projectPoint,
  buildQuads,
  sortQuadsByDepth,
  calculateMatrixCenter,
  getMatrixExtremes,
  calculateAutoYScaling,
  getPaletteColor,
  getShadedPaletteColor,
  computeQuadShade,
  computeAxisTickIndices,
  normalizeMatrixFor3D,
  findClosestCell,
  CAMERA_PRESETS,
  type Point3D,
  type CameraState,
} from './surface3d.ts';

test('rotateEuler: identity rotation leaves point invariant', () => {
  const p: Point3D = { x: 3, y: 4, z: 5 };
  const center: Point3D = { x: 1, y: 1, z: 1 };
  const rotated = rotateEuler(p, center, 0, 0);

  assert.equal(Math.round(rotated.x), 3);
  assert.equal(Math.round(rotated.y), 4);
  assert.equal(Math.round(rotated.z), 5);
});

test('rotateEuler: rotation around origin', () => {
  const origin: Point3D = { x: 0, y: 0, z: 0 };

  // Yaw 90 degrees (Math.PI / 2) around Y axis:
  // (1, 0, 0) -> (0, 0, -1)
  const pX: Point3D = { x: 1, y: 0, z: 0 };
  const rotYaw = rotateEuler(pX, origin, Math.PI / 2, 0);
  assert.ok(Math.abs(rotYaw.x) < 1e-10);
  assert.ok(Math.abs(rotYaw.y) < 1e-10);
  assert.ok(Math.abs(rotYaw.z - (-1)) < 1e-10);

  // Pitch 90 degrees (Math.PI / 2) around X axis:
  // (0, 1, 0) -> (0, 0, 1)
  const pY: Point3D = { x: 0, y: 1, z: 0 };
  const rotPitch = rotateEuler(pY, origin, 0, Math.PI / 2);
  assert.ok(Math.abs(rotPitch.x) < 1e-10);
  assert.ok(Math.abs(rotPitch.y) < 1e-10);
  assert.ok(Math.abs(rotPitch.z - 1) < 1e-10);
});

test('rotateEuler: rotation around arbitrary center preserves distance and center invariance', () => {
  const center: Point3D = { x: 10, y: 20, z: 30 };

  // Center itself rotated should remain identical
  const rotCenter = rotateEuler(center, center, 1.2, -0.8);
  assert.ok(Math.abs(rotCenter.x - center.x) < 1e-10);
  assert.ok(Math.abs(rotCenter.y - center.y) < 1e-10);
  assert.ok(Math.abs(rotCenter.z - center.z) < 1e-10);

  // An offset point preserves Euclidean distance to center
  const p: Point3D = { x: 13, y: 24, z: 30 }; // Distance = 5
  const rotP = rotateEuler(p, center, 0.75, -0.4);
  const distAfter = Math.hypot(rotP.x - center.x, rotP.y - center.y, rotP.z - center.z);
  assert.ok(Math.abs(distAfter - 5) < 1e-10);
});

test('projectPoint: projects center to canvas midpoint when pan is zero', () => {
  const center: Point3D = { x: 7, y: 20, z: 5 };
  const cam: CameraState = {
    yaw: 0,
    pitch: 0,
    scale: 25,
    panX: 0,
    panY: 0,
  };

  const canvasWidth = 800;
  const canvasHeight = 600;

  const proj = projectPoint(center, center, cam, canvasWidth, canvasHeight);
  assert.equal(proj.x, 400);
  assert.equal(proj.y, 300);
  assert.equal(proj.zDepth, 0);
});

test('projectPoint: applies pan and scale correctly', () => {
  const center: Point3D = { x: 0, y: 0, z: 0 };
  const cam: CameraState = {
    yaw: 0,
    pitch: 0,
    scale: 10,
    panX: 50,
    panY: -30,
  };

  // With yaw = 0, pitch = 0: viewX = x, viewY = y
  // projX = 400 + 50 + (2 * 10) = 470
  // projY = 300 - 30 - (3 * 10) = 240
  const p: Point3D = { x: 2, y: 3, z: 0 };
  const proj = projectPoint(p, center, cam, 800, 600);

  assert.equal(proj.x, 470);
  assert.equal(proj.y, 240);
});

test('buildQuads: generates exactly (N-1) * (M-1) quads for N x M matrix', () => {
  // 3 x 4 matrix: rows = 3, cols = 4
  // Expected quads = (3 - 1) * (4 - 1) = 2 * 3 = 6
  const matrix = [
    [10, 11, 12, 13],
    [20, 21, 22, 23],
    [30, 31, 32, 33],
  ];

  const quads = buildQuads(matrix);
  assert.equal(quads.length, 6);

  // Check first quad (0, 0)
  const q0 = quads[0];
  assert.equal(q0.r, 0);
  assert.equal(q0.c, 0);
  assert.deepEqual(q0.p0, { x: 0, y: 10, z: 0 });
  assert.deepEqual(q0.p1, { x: 1, y: 11, z: 0 });
  assert.deepEqual(q0.p2, { x: 1, y: 21, z: 1 });
  assert.deepEqual(q0.p3, { x: 0, y: 20, z: 1 });
  assert.equal(q0.avgZ, 0.5);
  assert.equal(q0.avgValue, (10 + 11 + 21 + 20) / 4);

  // Empty or degenerate matrices return empty quads array
  assert.deepEqual(buildQuads([]), []);
  assert.deepEqual(buildQuads([[1, 2, 3]]), []);
  assert.deepEqual(buildQuads([[1], [2], [3]]), []);
});

test('sortQuadsByDepth: Painter\'s algorithm puts farthest quad first', () => {
  // 3 x 3 matrix => 4 quads: (0,0), (0,1), (1,0), (1,1)
  const matrix = [
    [10, 10, 10],
    [10, 10, 10],
    [10, 10, 10],
  ];

  const quads = buildQuads(matrix);
  const center = calculateMatrixCenter(matrix);

  // In FRONT_RPM view (yaw = 0, pitch = 0):
  // Camera space uses right-hand coordinates where -Z' is into the screen (away from viewer)
  // and +Z' is towards the viewer. Row 0 quads (midZ = 0.5) have zDepth = -0.5 (farthest),
  // and Row 1 quads (midZ = 1.5) have zDepth = +0.5 (nearest).
  const cam = CAMERA_PRESETS.FRONT_RPM;
  const sorted = sortQuadsByDepth(quads, center, cam);

  assert.equal(sorted.length, 4);

  // Farthest quads (row 0, most negative zDepth) must be rendered first in Painter's algorithm
  assert.equal(sorted[0].r, 0);
  assert.equal(sorted[1].r, 0);
  assert.equal(sorted[2].r, 1);
  assert.equal(sorted[3].r, 1);
});

test('CAMERA_PRESETS: defines all four expected calibration camera angles', () => {
  assert.ok(CAMERA_PRESETS.ISOMETRIC);
  assert.ok(CAMERA_PRESETS.TOP_DOWN);
  assert.ok(CAMERA_PRESETS.FRONT_RPM);
  assert.ok(CAMERA_PRESETS.SIDE_TPS);

  assert.equal(CAMERA_PRESETS.ISOMETRIC.yaw, -Math.PI / 4);
  assert.equal(CAMERA_PRESETS.ISOMETRIC.pitch, Math.PI / 6);

  assert.equal(CAMERA_PRESETS.TOP_DOWN.yaw, 0);
  assert.equal(CAMERA_PRESETS.TOP_DOWN.pitch, Math.PI / 2);

  assert.equal(CAMERA_PRESETS.FRONT_RPM.yaw, 0);
  assert.equal(CAMERA_PRESETS.FRONT_RPM.pitch, 0);

  assert.equal(CAMERA_PRESETS.SIDE_TPS.yaw, Math.PI / 2);
  assert.equal(CAMERA_PRESETS.SIDE_TPS.pitch, 0);
});

test('calculateMatrixCenter and getMatrixExtremes: returns correct bounds', () => {
  const matrix = [
    [10, 20, 30],
    [40, 50, 60],
  ];

  const center = calculateMatrixCenter(matrix);
  assert.equal(center.x, 1); // (3 - 1) / 2
  assert.equal(center.z, 0.5); // (2 - 1) / 2
  assert.equal(center.y, 35); // (10 + 60) / 2

  const extremes = getMatrixExtremes(matrix);
  assert.equal(extremes.min, 10);
  assert.equal(extremes.max, 60);

  const autoScale = calculateAutoYScaling(matrix);
  assert.ok(autoScale > 0);
});

test('getPaletteColor: produces valid CSS colors for all palettes', () => {
  const palettes = ['classic', 'thermal', 'spectrum', 'monochrome'] as const;

  for (const pal of palettes) {
    const minColor = getPaletteColor(0, 0, 100, pal, 0.9);
    const midColor = getPaletteColor(50, 0, 100, pal, 0.9);
    const maxColor = getPaletteColor(100, 0, 100, pal, 0.9);

    assert.ok(minColor.length > 5);
    assert.ok(midColor.length > 5);
    assert.ok(maxColor.length > 5);
  }
});

test('findClosestCell: correctly pinpoints nearest grid point to screen coordinate', () => {
  const matrix = [
    [10, 20],
    [30, 40],
  ];
  const center = calculateMatrixCenter(matrix);
  const cam = CAMERA_PRESETS.ISOMETRIC;
  const canvasWidth = 600;
  const canvasHeight = 400;

  // Project cell (0, 0)
  const p00 = projectPoint({ x: 0, y: 10, z: 0 }, center, cam, canvasWidth, canvasHeight);

  // Search very close to p00
  const hit = findClosestCell(p00.x + 2, p00.y - 1, matrix, center, cam, canvasWidth, canvasHeight);
  assert.ok(hit !== null);
  assert.equal(hit.r, 0);
  assert.equal(hit.c, 0);
  assert.ok(hit.distance < 5);
});

test('computeQuadShade, computeAxisTickIndices, and normalizeMatrixFor3D produce valid CAD plot geometry', () => {
  const flatQuad = buildQuads([
    [10, 10],
    [10, 10],
  ])[0];
  const flatShade = computeQuadShade(flatQuad, 0.2);
  assert.ok(flatShade >= 0.72 && flatShade <= 1.18);

  const shadedColor = getShadedPaletteColor(50, 0, 100, 'spectrum', flatShade, 0.9);
  assert.ok(shadedColor.startsWith('rgba('));

  assert.deepEqual(computeAxisTickIndices(16, 5), [0, 4, 8, 11, 15]);
  assert.deepEqual(computeAxisTickIndices(4, 6), [0, 1, 2, 3]);

  const ribbon1d = normalizeMatrixFor3D([[10, 15, 20, 25]]);
  assert.equal(ribbon1d.is1DRibbon, true);
  assert.equal(ribbon1d.transpose1D, false);
  assert.equal(ribbon1d.matrix.length, 2);
  assert.equal(ribbon1d.matrix[0].length, 4);

  const col1d = normalizeMatrixFor3D([[10], [20], [30]]);
  assert.equal(col1d.is1DRibbon, true);
  assert.equal(col1d.transpose1D, true);
  assert.equal(col1d.matrix.length, 2);
  assert.equal(col1d.matrix[0].length, 3);
});
