/**
 * Read-only ROM verification controller.
 *
 * Compares the active ROM buffer against a reference buffer (typically a stock
 * backup) and produces a report for display. This module never writes to the
 * ROM, the `MemoryMap`, or any map definition, and offers no "apply" action —
 * verification is an observation, not a repair.
 *
 * ## Why both buffers are required
 *
 * `bridgeVerifyRomAgainstReference` treats a missing reference as an empty
 * buffer. Comparing against an empty buffer marks every byte as different,
 * which a user would read as "my ROM is corrupt" when in fact no reference was
 * supplied. The controller therefore refuses the comparison up front instead of
 * letting that misleading result through.
 *
 * ## Why the buffers are copied
 *
 * The bridge serialises the active ROM into the IPC payload and the fallback
 * walks it directly. Passing caller-owned typed arrays through untouched risks
 * the caller mutating them mid-comparison; the copies make the report describe
 * a stable snapshot.
 */

import {
  bridgeVerifyRomAgainstReference,
  type RomVerificationReport,
} from '../../lib/tauriBridge.ts';

export type RomVerificationState =
  | { kind: 'ok'; report: RomVerificationReport }
  | { kind: 'no-active-rom'; message: string }
  | { kind: 'no-reference'; message: string }
  | { kind: 'error'; message: string };

/**
 * Compares `activeRom` against `referenceRom`.
 *
 * Returns a discriminated union so the caller cannot render a report that was
 * never produced. A `null` from the bridge is treated as an error rather than
 * as a match.
 */
export async function runRomVerification(
  activeRom: Uint8Array | null,
  referenceRom: Uint8Array | null,
  maxDifferences = 32
): Promise<RomVerificationState> {
  if (!activeRom || activeRom.length === 0) {
    return {
      kind: 'no-active-rom',
      message: 'Load a ROM before verifying against a reference.',
    };
  }

  if (!referenceRom || referenceRom.length === 0) {
    return {
      kind: 'no-reference',
      message: 'Choose a reference ROM to compare against.',
    };
  }

  try {
    const report = await bridgeVerifyRomAgainstReference(
      Uint8Array.from(referenceRom),
      Uint8Array.from(activeRom),
      maxDifferences
    );

    if (!report) {
      return {
        kind: 'error',
        message: 'ROM verification unavailable.',
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

/**
 * One-line human summary of a verification report.
 *
 * Length mismatch and a blank reference are stated explicitly: both change how
 * a difference count should be read, and neither is visible in the count alone.
 */
export function describeVerification(report: RomVerificationReport): string {
  if (report.matches) {
    return `ROM matches the reference (${report.active_len} bytes).`;
  }

  const parts: string[] = [
    `${report.difference_count} difference${
      report.difference_count === 1 ? '' : 's'
    } found`,
  ];

  if (report.active_len !== report.reference_len) {
    parts.push(
      `length differs: active ${report.active_len} B, reference ${report.reference_len} B`
    );
  }

  if (report.reference_is_blank) {
    parts.push('reference is blank (all 0xFF)');
  }

  return `${parts.join('; ')}.`;
}
