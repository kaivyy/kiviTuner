/**
 * DTC tab controller.
 *
 * Wraps the DTC parse and patch commands.
 *
 * ## Safety
 *
 * `runDtcPatch` returns a **new** ROM buffer with one configuration byte set to
 * "disabled". It does not write to disk, to a device, or to the store — the
 * caller decides what to do with the returned buffer.
 */

import {
  bridgeParseDtc,
  bridgePatchDtcDisable,
  type DtcView,
} from '../../lib/tauriBridge.ts';

export type ToolkitState<T> =
  | { kind: 'ok'; data: T }
  | { kind: 'no-backend'; message: string }
  | { kind: 'error'; message: string };

const NO_BACKEND =
  'This requires the desktop application. It is not available in the browser build.';

function wrap<T>(value: T | null): ToolkitState<T> {
  if (value === null) return { kind: 'no-backend', message: NO_BACKEND };
  return { kind: 'ok', data: value };
}

/**
 * Parses a 5-character diagnostic trouble code.
 *
 * Validated locally first so an obvious typo never reaches the backend.
 */
export async function runDtcParse(text: string): Promise<ToolkitState<DtcView>> {
  const trimmed = text.trim();
  if (trimmed.length !== 5) {
    return {
      kind: 'error',
      message: 'A DTC must be exactly 5 characters (for example P0420).',
    };
  }
  const prefix = trimmed[0].toUpperCase();
  if (!['P', 'C', 'B', 'U'].includes(prefix)) {
    return { kind: 'error', message: 'The DTC prefix must be P, C, B, or U.' };
  }
  return wrap(await bridgeParseDtc(trimmed));
}

/**
 * Produces a ROM copy with the DTC at `switchAddress` disabled.
 *
 * Returns the new buffer; it does not persist anything.
 */
export async function runDtcPatch(
  rom: Uint8Array | null,
  switchAddress: number
): Promise<ToolkitState<Uint8Array>> {
  if (!rom || rom.length === 0) {
    return { kind: 'error', message: 'Load a ROM before patching a DTC switch.' };
  }
  if (!Number.isInteger(switchAddress) || switchAddress < 0) {
    return {
      kind: 'error',
      message: 'Switch address must be a non-negative integer.',
    };
  }
  if (switchAddress >= rom.length) {
    return {
      kind: 'error',
      message: `Switch address 0x${switchAddress.toString(16).toUpperCase()} is outside the ROM (${rom.length} bytes).`,
    };
  }

  const patched = await bridgePatchDtcDisable(rom, switchAddress);
  return wrap(patched);
}

/** The byte value written by a DTC-disable patch. */
export const DTC_DISABLED_BYTE = 0x00;
