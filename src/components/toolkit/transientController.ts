/**
 * Transient and axis-rescaling toolkit controller.
 *
 * Wraps the Wave 8 backend commands so the UI never calls `invoke` directly and
 * never needs to know about Tauri.
 *
 * ## What this is for
 *
 * These are the operating-point calculations a tuner reaches for while
 * calibrating: what does DFCO / anti-stall do at this point, what does the MAF
 * voltage mean, what pulse width does a replacement injector need, and what does
 * this 1D curve look like on a new axis.
 *
 * ## Safety
 *
 * Every function here is analysis. None writes to a ROM, an ECU, or a file.
 */

import {
  bridgeComputeTransient,
  bridgeRescaleInjector,
  bridgeResampleAxis,
  type RescaleResult,
  type TransientResult,
} from '../../lib/tauriBridge.ts';

export type { RescaleResult, TransientResult };

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

/** One transient-state operating point. */
export interface TransientInput {
  rpm: number;
  tpsPct: number;
  speedKph: number;
  ectC: number;
  targetIdleRpm: number;
  mafVolts: number;
}

/** One injector swap. */
export interface RescaleInput {
  pwOldMs: number;
  deadOldMs: number;
  flowOldCc: number;
  flowNewCc: number;
  deadNewMs: number;
}

// ---------------------------------------------------------------------------
// Transient state: DFCO, anti-stall, MAF
// ---------------------------------------------------------------------------

/** Evaluates DFCO, anti-stall and MAF for one operating point. */
export async function runTransient(
  input: TransientInput
): Promise<ToolkitState<TransientResult>> {
  for (const [name, value] of [
    ['rpm', input.rpm],
    ['tpsPct', input.tpsPct],
    ['speedKph', input.speedKph],
    ['ectC', input.ectC],
    ['targetIdleRpm', input.targetIdleRpm],
    ['mafVolts', input.mafVolts],
  ] as Array<[string, number]>) {
    const err = requireFinite(name, value);
    if (err) return { kind: 'error', message: err };
  }
  if (input.rpm < 0) {
    return { kind: 'error', message: 'RPM cannot be negative.' };
  }
  if (input.tpsPct < 0 || input.tpsPct > 100) {
    return { kind: 'error', message: 'Throttle percent must be between 0 and 100.' };
  }
  if (input.speedKph < 0) {
    return { kind: 'error', message: 'Vehicle speed cannot be negative.' };
  }
  if (input.mafVolts < 0) {
    return { kind: 'error', message: 'MAF voltage cannot be negative.' };
  }
  return wrap(
    await bridgeComputeTransient(
      input.rpm,
      input.tpsPct,
      input.speedKph,
      input.ectC,
      input.targetIdleRpm,
      input.mafVolts
    )
  );
}

// ---------------------------------------------------------------------------
// Injector rescale
// ---------------------------------------------------------------------------

/** Rescales a pulse width for a different injector flow rate. */
export async function runInjectorRescale(
  input: RescaleInput
): Promise<ToolkitState<RescaleResult>> {
  for (const [name, value] of [
    ['pwOldMs', input.pwOldMs],
    ['deadOldMs', input.deadOldMs],
    ['flowOldCc', input.flowOldCc],
    ['flowNewCc', input.flowNewCc],
    ['deadNewMs', input.deadNewMs],
  ] as Array<[string, number]>) {
    const err = requireFinite(name, value);
    if (err) return { kind: 'error', message: err };
  }
  if (input.pwOldMs < 0) {
    return { kind: 'error', message: 'Old pulse width cannot be negative.' };
  }
  if (input.flowOldCc <= 0) {
    return { kind: 'error', message: 'Old injector flow must be greater than zero.' };
  }
  if (input.flowNewCc <= 0) {
    return { kind: 'error', message: 'New injector flow must be greater than zero.' };
  }
  return wrap(
    await bridgeRescaleInjector(
      input.pwOldMs,
      input.deadOldMs,
      input.flowOldCc,
      input.flowNewCc,
      input.deadNewMs
    )
  );
}

// ---------------------------------------------------------------------------
// Axis resample
// ---------------------------------------------------------------------------

/**
 * Resamples a 1D curve onto a new axis.
 *
 * The old axis and its values must describe the same number of breakpoints, and
 * both the old and new axes must have at least one breakpoint — a curve with no
 * axis is a wiring mistake, not an empty result.
 */
export async function runAxisResample(
  oldAxis: number[],
  oldValues: number[],
  newAxis: number[]
): Promise<ToolkitState<number[]>> {
  if (oldAxis.length === 0) {
    return { kind: 'error', message: 'The old axis must have at least one breakpoint.' };
  }
  if (oldAxis.length !== oldValues.length) {
    return {
      kind: 'error',
      message: 'The old axis and old values must have the same length.',
    };
  }
  if (newAxis.length === 0) {
    return { kind: 'error', message: 'The new axis must have at least one breakpoint.' };
  }
  for (const [name, axis] of [
    ['oldAxis', oldAxis],
    ['oldValues', oldValues],
    ['newAxis', newAxis],
  ] as Array<[string, number[]]>) {
    for (const value of axis) {
      const err = requireFinite(name, value);
      if (err) return { kind: 'error', message: err };
    }
  }
  return wrap(await bridgeResampleAxis(oldAxis, oldValues, newAxis));
}
