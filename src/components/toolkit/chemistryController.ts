/**
 * Engineering toolkit controller: chemistry and airflow.
 *
 * Wraps the Wave 8 chemistry backend commands so the UI never calls `invoke`
 * directly and never needs to know about Tauri.
 *
 * ## What this is for
 *
 * Two tabs of the engineering toolkit:
 *
 * - **chemistry** — what does an ethanol blend do to stoich AFR, fuel volume,
 *   and latent heat.
 * - **airflow** — what are the compressor discharge, intercooler outlet, and
 *   exhaust gas temperatures for an operating point.
 *
 * ## Safety
 *
 * Every function here is analysis. None writes to a ROM, an ECU, or a file.
 */

import {
  bridgeComputeBlend,
  bridgeComputeCompressor,
  bridgeComputeThrottleFlow,
  type BlendResult,
  type CompressorResult,
} from '../../lib/tauriBridge.ts';

export type { BlendResult, CompressorResult };

/** Common result wrapper so the UI has one shape to render. */
export type ToolkitState<T> =
  | { kind: 'ok'; data: T }
  | { kind: 'no-backend'; message: string }
  | { kind: 'error'; message: string };

/** The backend is desktop-only for these calculations. */
const NO_BACKEND =
  'This calculation requires the desktop application. It is not available in the browser build.';

/**
 * The throttle-flow backend command is not yet exposed.
 *
 * The Rust side has no `compute_throttle_flow` command, so there is nothing to
 * invoke. This controller reports that honestly rather than inventing a
 * command name the backend would reject.
 */

function wrap<T>(value: T | null): ToolkitState<T> {
  if (value === null) return { kind: 'no-backend', message: NO_BACKEND };
  return { kind: 'ok', data: value };
}

/** Rejects non-finite numbers before they reach the backend. */
function requireFinite(name: string, value: number): string | null {
  if (!Number.isFinite(value)) return `${name} must be a finite number.`;
  return null;
}

/** Rejects numbers outside an inclusive range. */
function requireRange(
  name: string,
  value: number,
  min: number,
  max: number
): string | null {
  if (value < min || value > max) {
    return `${name} must be between ${min} and ${max}.`;
  }
  return null;
}

// ---------------------------------------------------------------------------
// Flex-fuel blend
// ---------------------------------------------------------------------------

/**
 * Computes flex-fuel blend properties for an ethanol percentage.
 *
 * `ethanolPercent` is a percentage (0 = pump gasoline, 100 = pure ethanol).
 */
export async function runBlend(
  ethanolPercent: number
): Promise<ToolkitState<BlendResult>> {
  const finite = requireFinite('Ethanol percent', ethanolPercent);
  if (finite) return { kind: 'error', message: finite };

  const range = requireRange('Ethanol percent', ethanolPercent, 0, 100);
  if (range) return { kind: 'error', message: range };

  return wrap(await bridgeComputeBlend(ethanolPercent));
}

// ---------------------------------------------------------------------------
// Compressor
// ---------------------------------------------------------------------------

/** Inputs for a turbo compressor calculation. */
export interface CompressorInput {
  /** Compressor inlet temperature, °C. */
  tInC: number;
  /** Compressor pressure ratio (outlet / inlet absolute). */
  pressureRatio: number;
  /** Isentropic efficiency, 0.40..0.95. */
  isentropicEfficiency: number;
  /** Intercooler effectiveness, 0..1. */
  intercoolerEfficiency: number;
  /** Ambient temperature, °C. */
  ambientC: number;
  /** Lambda at the operating point. */
  lambda: number;
  /** Spark retard from MBT, degrees. */
  sparkRetardDeg: number;
}

/**
 * Computes compressor discharge, intercooler outlet, and EGT.
 *
 * All inputs are validated before the bridge is called, so an impossible
 * operating point never reaches the backend.
 */
export async function runCompressor(
  input: CompressorInput
): Promise<ToolkitState<CompressorResult>> {
  for (const [name, value] of Object.entries(input)) {
    if (typeof value === 'number') {
      const err = requireFinite(name, value);
      if (err) return { kind: 'error', message: err };
    }
  }

  if (input.pressureRatio < 1.0) {
    return { kind: 'error', message: 'Pressure ratio must be at least 1.0.' };
  }

  const isentropic = requireRange(
    'Isentropic efficiency',
    input.isentropicEfficiency,
    0.4,
    0.95
  );
  if (isentropic) return { kind: 'error', message: isentropic };

  const intercooler = requireRange(
    'Intercooler efficiency',
    input.intercoolerEfficiency,
    0,
    1
  );
  if (intercooler) return { kind: 'error', message: intercooler };

  return wrap(
    await bridgeComputeCompressor(
      input.tInC,
      input.pressureRatio,
      input.isentropicEfficiency,
      input.intercoolerEfficiency,
      input.ambientC,
      input.lambda,
      input.sparkRetardDeg
    )
  );
}

// ---------------------------------------------------------------------------
// Throttle flow
// ---------------------------------------------------------------------------

/** Inputs for a throttle-flow calculation. */
export interface ThrottleFlowInput {
  /** Throttle plate angle, degrees. */
  throttleDeg: number;
  /** Engine speed, rpm. */
  rpm: number;
  /** Intake manifold absolute pressure, kPa. */
  mapKpa: number;
  /** Intake air temperature, °C. */
  iatC: number;
}

/**
 * Result of a throttle-flow calculation.
 *
 * The backend command `compute_throttle_flow` now exists, so this models the
 * real response shape.
 */
export type ThrottleFlowResult = {
  mass_flow_kg_s: number;
  pressure_ratio: number;
  is_choked: boolean;
  critical_ratio: number;
};

/**
 * Computes compressible mass flow through a throttle body.
 *
 * ## Unit bridge
 *
 * The tuner-facing inputs are throttle angle in degrees, MAP in kPa and IAT in
 * °C. The backend works in SI (m², Pa, K), so the conversion is explicit here
 * rather than hidden:
 *
 * * plate area = a 50 mm bore circle, scaled by `sin(angle)` (the effective
 *   open area of a butterfly), with a small idle-bypass floor so a closed plate
 *   still flows something;
 * * ambient = 101 325 Pa, 300 K (the backend takes absolute values);
 * * manifold = `mapKpa * 1000` Pa.
 */
export async function runThrottleFlow(
  input: ThrottleFlowInput
): Promise<ToolkitState<ThrottleFlowResult>> {
  const entries: Array<[string, number]> = [
    ['Throttle angle', input.throttleDeg],
    ['RPM', input.rpm],
    ['MAP', input.mapKpa],
    ['IAT', input.iatC],
  ];
  for (const [name, value] of entries) {
    const err = requireFinite(name, value);
    if (err) return { kind: 'error', message: err };
  }
  if (input.throttleDeg < 0 || input.throttleDeg > 90) {
    return { kind: 'error', message: 'Throttle angle must be between 0 and 90 degrees.' };
  }
  if (input.rpm < 0) {
    return { kind: 'error', message: 'RPM cannot be negative.' };
  }
  if (input.mapKpa < 0) {
    return { kind: 'error', message: 'MAP cannot be negative.' };
  }

  const BORE_M = 0.050;
  const boreArea = Math.PI * (BORE_M / 2) ** 2;
  const openFraction = Math.sin((input.throttleDeg * Math.PI) / 180);
  const effectiveAreaM2 = Math.max(boreArea * openFraction, 1e-6);

  const AMBIENT_PA = 101325;
  const AMBIENT_K = 300;
  const manifoldPa = input.mapKpa * 1000;

  return wrap(
    await bridgeComputeThrottleFlow(effectiveAreaM2, AMBIENT_PA, AMBIENT_K, manifoldPa)
  );
}
