/**
 * Pure geometry and scale math for the Ferrari SF90 / Lamborghini Revuelto /
 * BMW M inspired supercar telemetry cluster.
 */

export const CLUSTER_MAX_RPM = 12000;
export const CLUSTER_REDLINE_RPM = 10000;
export const SHIFT_LIGHT_START_RPM = 4500;
export const SHIFT_LIGHT_END_RPM = 11800;
export const SHIFT_LIGHT_COUNT = 16;
export const TACH_SEGMENT_COUNT = 36;
export const TACH_START_DEG = -215;
export const TACH_SWEEP_DEG = 250;
export const TACH_CX = 180;
export const TACH_CY = 128;

export type SemanticZone = 'ok' | 'accent' | 'warn' | 'danger';

export interface ShiftLightSegment {
  index: number;
  thresholdRpm: number;
  zone: Exclude<SemanticZone, 'accent'>;
  active: boolean;
}

export interface TachSegment {
  index: number;
  startDeg: number;
  endDeg: number;
  thresholdRpm: number;
  zone: SemanticZone;
  active: boolean;
  path: string;
}

export type AfrClassification = 'rich' | 'stoich' | 'lean';

function clamp(v: number, lo: number, hi: number): number {
  if (!Number.isFinite(v)) return lo;
  return Math.max(lo, Math.min(hi, v));
}

function polarToCartesian(cx: number, cy: number, r: number, angleDeg: number): [number, number] {
  const rad = (angleDeg * Math.PI) / 180;
  return [cx + r * Math.cos(rad), cy + r * Math.sin(rad)];
}

/**
 * Generates an SVG annular sector path between `startDeg` and `endDeg`.
 * Supports both clockwise (`endDeg > startDeg`) and counter-clockwise sweeps.
 */
export function describeArcSegmentPath(
  cx: number,
  cy: number,
  rInner: number,
  rOuter: number,
  startDeg: number,
  endDeg: number
): string {
  const cw = endDeg >= startDeg;
  const sweepFlag = cw ? 1 : 0;
  const invSweepFlag = cw ? 0 : 1;
  const largeArcFlag = Math.abs(endDeg - startDeg) > 180 ? 1 : 0;

  const [ox1, oy1] = polarToCartesian(cx, cy, rOuter, startDeg);
  const [ox2, oy2] = polarToCartesian(cx, cy, rOuter, endDeg);
  const [ix2, iy2] = polarToCartesian(cx, cy, rInner, endDeg);
  const [ix1, iy1] = polarToCartesian(cx, cy, rInner, startDeg);

  return [
    `M ${ox1.toFixed(2)} ${oy1.toFixed(2)}`,
    `A ${rOuter} ${rOuter} 0 ${largeArcFlag} ${sweepFlag} ${ox2.toFixed(2)} ${oy2.toFixed(2)}`,
    `L ${ix2.toFixed(2)} ${iy2.toFixed(2)}`,
    `A ${rInner} ${rInner} 0 ${largeArcFlag} ${invSweepFlag} ${ix1.toFixed(2)} ${iy1.toFixed(2)}`,
    'Z',
  ].join(' ');
}

/**
 * Computes the 16 sequential BMW M / F1 LED shift lights across 4,500..11,800 RPM.
 */
export function computeShiftLights(rpm: number): {
  segments: ShiftLightSegment[];
  isShiftAlert: boolean;
} {
  const clampedRpm = clamp(rpm, 0, CLUSTER_MAX_RPM);
  const span = SHIFT_LIGHT_END_RPM - SHIFT_LIGHT_START_RPM;
  const segments: ShiftLightSegment[] = [];

  for (let i = 0; i < SHIFT_LIGHT_COUNT; i++) {
    const thresholdRpm = Math.round(
      SHIFT_LIGHT_START_RPM + (i / (SHIFT_LIGHT_COUNT - 1)) * span
    );
    const zone: ShiftLightSegment['zone'] =
      i < 6 ? 'ok' : i < 11 ? 'warn' : 'danger';
    segments.push({
      index: i,
      thresholdRpm,
      zone,
      active: clampedRpm >= thresholdRpm && clampedRpm > 0,
    });
  }

  return {
    segments,
    isShiftAlert: clampedRpm >= CLUSTER_REDLINE_RPM,
  };
}

/**
 * Computes the 36 angular arc blocks around the hero tachometer dial (`-215°`..`+35°`).
 */
export function computeTachSegments(rpm: number): TachSegment[] {
  const clampedRpm = clamp(rpm, 0, CLUSTER_MAX_RPM);
  const stepDeg = TACH_SWEEP_DEG / TACH_SEGMENT_COUNT;
  const gapDeg = 1.65;
  const segments: TachSegment[] = [];

  for (let i = 0; i < TACH_SEGMENT_COUNT; i++) {
    const startDeg = TACH_START_DEG + i * stepDeg;
    const endDeg = startDeg + stepDeg - gapDeg;
    const midFraction = (i + 0.5) / TACH_SEGMENT_COUNT;
    const thresholdRpm = ((i + 1) / TACH_SEGMENT_COUNT) * CLUSTER_MAX_RPM;

    let zone: SemanticZone = 'ok';
    if (midFraction >= CLUSTER_REDLINE_RPM / CLUSTER_MAX_RPM) {
      zone = 'danger';
    } else if (midFraction >= 0.68) {
      zone = 'warn';
    } else if (midFraction >= 0.34) {
      zone = 'accent';
    }

    const rOuter = zone === 'danger' ? 105 : 103;
    const rInner = zone === 'danger' ? 84 : 86;

    segments.push({
      index: i,
      startDeg,
      endDeg,
      thresholdRpm,
      zone,
      active: clampedRpm > (i / TACH_SEGMENT_COUNT) * CLUSTER_MAX_RPM && clampedRpm > 0,
      path: describeArcSegmentPath(TACH_CX, TACH_CY, rInner, rOuter, startDeg, endDeg),
    });
  }

  return segments;
}

/**
 * Computes the BMW M style mirrored outer flank wing tracks (`left` TPS, `right` Boost/MAP).
 * Both fill upward from bottom of the side wing to top as `percent` rises 0..100.
 */
export function computeFlankArcPath(percent: number, side: 'left' | 'right'): string {
  const p = clamp(percent, 0, 100);
  if (p <= 0) return '';
  if (side === 'left') {
    const bottomDeg = -204;
    const spanDeg = 46; // -204 -> -158 upward
    const endDeg = bottomDeg + (p / 100) * spanDeg;
    return describeArcSegmentPath(TACH_CX, TACH_CY, 114, 121, bottomDeg, endDeg);
  }
  const bottomDeg = 24;
  const spanDeg = -46; // +24 -> -22 upward (counter-clockwise)
  const endDeg = bottomDeg + (p / 100) * spanDeg;
  return describeArcSegmentPath(TACH_CX, TACH_CY, 114, 121, bottomDeg, endDeg);
}

/** Fixed background track paths for the mirrored left/right flank meters. */
export function computeFlankTrackPath(side: 'left' | 'right'): string {
  return computeFlankArcPath(100, side);
}

/** Open horseshoe structural bezel wrapping -215°..+35° without bottom circle bleed. */
export const OUTER_BEZEL_ARC_PATH = describeArcSegmentPath(
  TACH_CX,
  TACH_CY,
  108.5,
  109.7,
  TACH_START_DEG,
  TACH_START_DEG + TACH_SWEEP_DEG
);

/** Computes `(x, y)` label coordinates for the hero dial numerals (`0, 2, 4, 6, 8, 10, 12`). */
export function computeTachScaleLabels(): Array<{
  rpmThousands: number;
  x: number;
  y: number;
  isRedline: boolean;
}> {
  const marks = [0, 2, 4, 6, 8, 10, 12];
  return marks.map((k) => {
    const angleDeg = TACH_START_DEG + ((k * 1000) / CLUSTER_MAX_RPM) * TACH_SWEEP_DEG;
    const [x, y] = polarToCartesian(TACH_CX, TACH_CY, 70, angleDeg);
    return {
      rpmThousands: k,
      x: Number(x.toFixed(1)),
      y: Number(y.toFixed(1)),
      isRedline: k >= 10,
    };
  });
}

/** Maps AFR `10.0..20.0` to `0..100%` along a calibrated horizontal lambda bar. */
export function computeAfrMarkerPercent(afr: number): number {
  if (!Number.isFinite(afr)) return 47;
  return Math.round(clamp(((afr - 10.0) / 10.0) * 100, 0, 100) * 10) / 10;
}

export function classifyAfr(afr: number): AfrClassification {
  if (!Number.isFinite(afr) || afr <= 14.3) return 'rich';
  if (afr >= 15.1) return 'lean';
  return 'stoich';
}

export function formatGearLabel(gear: number | null | undefined): string {
  if (gear === null || gear === undefined || !Number.isFinite(gear)) return '--';
  const rounded = Math.trunc(gear);
  if (rounded === 0) return 'N';
  if (rounded > 0 && rounded <= 9) return String(rounded);
  return '--';
}
