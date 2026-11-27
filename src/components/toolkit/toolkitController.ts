/**
 * Engineering toolkit controller.
 *
 * Wraps the Wave 8 backend commands so the UI never calls `invoke` directly and
 * never needs to know about Tauri.
 *
 * ## What this is for
 *
 * kiviTuner is a combined flashing + calibration editor. These are the
 * **calculation** tools a tuner reaches for while calibrating: how long should
 * the injector open, how much air is in the cylinder, what timing is commanded,
 * what does this sensor voltage mean, what does this chip's layout look like.
 *
 * ## Safety
 *
 * Every function here is analysis. None writes to a ROM, an ECU, or a file.
 */

import {
  bridgeComputeAirMass,
  bridgeComputeFuelPulse,
  bridgeComputeSparkTiming,
  bridgeComputeTorque,
  bridgeDecodeSensor,
  bridgeComputeHistogram,
  bridgeDescribeFlashTargetView,
  type AirMassResult,
  type FlashTargetView,
  type HistogramResult,
  type FuelPulseRequest,
  type FuelPulseResult,
  type SensorDecodeResult,
  type SparkTimingResult,
  type TorqueResult,
} from '../../lib/tauriBridge.ts';

/** Common result wrapper so the UI has one shape to render. */
export type ToolkitState<T> =
  | { kind: 'ok'; data: T }
  | { kind: 'no-backend'; message: string }
  | { kind: 'error'; message: string };

/** The backend is desktop-only for these calculations. */
const NO_BACKEND =
  'This calculation requires the desktop application. It is not available in the browser build.';

function wrap<T>(value: T | null): ToolkitState<T> {
  if (value === null) return { kind: 'no-backend', message: NO_BACKEND };
  return { kind: 'ok', data: value };
}

/** Rejects non-finite numbers before they reach the backend. */
function requireFinite(name: string, value: number): string | null {
  if (!Number.isFinite(value)) return `${name} must be a finite number.`;
  return null;
}

// ---------------------------------------------------------------------------
// Fuel pulse
// ---------------------------------------------------------------------------

/** Computes injector pulse width. */
export async function runFuelPulse(
  req: FuelPulseRequest
): Promise<ToolkitState<FuelPulseResult>> {
  for (const [name, v] of Object.entries(req)) {
    if (typeof v === 'number') {
      const err = requireFinite(name, v);
      if (err) return { kind: 'error', message: err };
    }
  }
  if (req.target_lambda <= 0) {
    return { kind: 'error', message: 'Target lambda must be greater than zero.' };
  }
  if (req.air_mass_g < 0) {
    return { kind: 'error', message: 'Air mass cannot be negative.' };
  }
  return wrap(await bridgeComputeFuelPulse(req));
}

// ---------------------------------------------------------------------------
// Air mass
// ---------------------------------------------------------------------------

/** Estimates cylinder air mass and charge temperature. */
export async function runAirMass(
  displacementCc: number,
  cylinders: number,
  mapKpa: number,
  veRatio: number,
  iatC: number,
  ectC: number,
  rpm: number
): Promise<ToolkitState<AirMassResult>> {
  if (cylinders <= 0) return { kind: 'error', message: 'Cylinder count must be positive.' };
  if (displacementCc <= 0) {
    return { kind: 'error', message: 'Displacement must be positive.' };
  }
  if (rpm <= 0) return { kind: 'error', message: 'RPM must be positive.' };
  if (mapKpa <= 0) return { kind: 'error', message: 'MAP must be positive.' };

  return wrap(
    await bridgeComputeAirMass(displacementCc, cylinders, mapKpa, veRatio, iatC, ectC, rpm)
  );
}

// ---------------------------------------------------------------------------
// Spark timing
// ---------------------------------------------------------------------------

/** Computes commanded spark timing. */
export async function runSparkTiming(
  baseBtdc: number,
  ectCorrection: number,
  iatCorrection: number,
  knockRetardDeg: number
): Promise<ToolkitState<SparkTimingResult>> {
  if (knockRetardDeg < 0) {
    return { kind: 'error', message: 'Knock retard cannot be negative.' };
  }
  return wrap(
    await bridgeComputeSparkTiming(baseBtdc, ectCorrection, iatCorrection, knockRetardDeg)
  );
}

// ---------------------------------------------------------------------------
// Torque
// ---------------------------------------------------------------------------

/** Computes the torque structure for an operating point. */
export async function runTorque(
  pedalPercent: number,
  rpm: number,
  mapKpa: number,
  ectC: number,
  accessoryLoadNm: number,
  targetTorqueNm: number
): Promise<ToolkitState<TorqueResult>> {
  if (pedalPercent < 0 || pedalPercent > 100) {
    return { kind: 'error', message: 'Pedal percent must be between 0 and 100.' };
  }
  if (rpm < 0) return { kind: 'error', message: 'RPM cannot be negative.' };
  return wrap(
    await bridgeComputeTorque(pedalPercent, rpm, mapKpa, ectC, accessoryLoadNm, targetTorqueNm)
  );
}

// ---------------------------------------------------------------------------
// Sensor decode
// ---------------------------------------------------------------------------

/** Decodes a raw sensor reading. */
export async function runSensorDecode(
  kind: string,
  raw: number
): Promise<ToolkitState<SensorDecodeResult>> {
  const err = requireFinite('Raw value', raw);
  if (err) return { kind: 'error', message: err };
  return wrap(await bridgeDecodeSensor(kind, raw));
}

// ---------------------------------------------------------------------------
// Flash target
// ---------------------------------------------------------------------------

/** Describes a flash chip layout by JEDEC id. */
export async function runDescribeFlashTarget(
  chipId: number
): Promise<ToolkitState<FlashTargetView>> {
  if (!Number.isInteger(chipId) || chipId < 0) {
    return { kind: 'error', message: 'Chip id must be a non-negative integer.' };
  }
  return wrap(await bridgeDescribeFlashTargetView(chipId));
}

/**
 * True when a chip description shows a size mismatch between the declared total
 * and the sum of its blocks.
 *
 * This is how the preserved `Am29F400BT` defect surfaces: the source declares
 * 524 288 bytes but its blocks only cover 327 680, leaving a 0x30000 gap. The UI
 * must show that rather than hide it.
 */
export function hasBlockSizeMismatch(view: FlashTargetView): boolean {
  return view.blocks_total_bytes !== view.total_size_bytes;
}

/** Human summary of a chip description. */
export function describeFlashTarget(view: FlashTargetView): string {
  const parts = [
    `${view.model_name}: ${view.total_size_bytes} bytes declared`,
    `${view.blocks.length} block${view.blocks.length === 1 ? '' : 's'}`,
  ];
  if (hasBlockSizeMismatch(view)) {
    const gap = view.total_size_bytes - view.blocks_total_bytes;
    parts.push(
      `WARNING: blocks cover ${view.blocks_total_bytes} bytes, ${gap} unaccounted`
    );
  }
  return parts.join(' · ');
}

// ---------------------------------------------------------------------------
// Histogram
// ---------------------------------------------------------------------------

/**
 * Builds a hit histogram from operating points.
 *
 * Both axes need at least two breakpoints, and at least one point must be
 * supplied — a hit map of nothing is meaningless rather than empty.
 */
export async function runHistogram(
  rowAxis: number[],
  colAxis: number[],
  points: Array<[number, number]>
): Promise<ToolkitState<HistogramResult>> {
  if (rowAxis.length < 2 || colAxis.length < 2) {
    return { kind: 'error', message: 'Both axes need at least two breakpoints.' };
  }
  if (points.length === 0) {
    return { kind: 'error', message: 'Supply at least one operating point.' };
  }
  return wrap(await bridgeComputeHistogram(rowAxis, colAxis, points));
}
