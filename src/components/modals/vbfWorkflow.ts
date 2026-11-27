/**
 * Wave 5: VBF import/export workflow controller.
 *
 * ## Why this is an inspection workflow, not a silent conversion
 *
 * A VBF file is a **container of addressed segments**. The application's ROM
 * document is a **single flat buffer**. Loading a multi-segment container into
 * that buffer would silently discard segment boundaries and their addresses —
 * exactly the data loss this workflow must avoid.
 *
 * So the controller:
 *   * parses the container and reports every segment with its address and size;
 *   * only offers "load as ROM" when the container holds exactly one segment,
 *     because that is the only case where the flat buffer is faithful;
 *   * refuses to fabricate a multi-segment flattening.
 *
 * ## Why export is single-segment only
 *
 * The ROM document has no segment table. Exporting it produces a one-segment
 * container at the document's known base address. When the base address is
 * unknown, export is refused rather than defaulting to zero.
 */

import {
  bridgeParseVbf,
  bridgeSerializeVbf,
  type VbfParseReport,
  type VbfSegmentPayload,
} from '../../lib/tauriBridge.ts';

export type VbfImportState =
  | { kind: 'ok'; report: VbfParseReport }
  | { kind: 'no-backend'; message: string }
  | { kind: 'empty'; message: string }
  | { kind: 'error'; message: string };

export type VbfExportState =
  | { kind: 'ok'; bytes: Uint8Array }
  | { kind: 'no-rom'; message: string }
  | { kind: 'unknown-base'; message: string }
  | { kind: 'no-backend'; message: string }
  | { kind: 'error'; message: string };

/**
 * Parses a VBF file for inspection.
 *
 * A `null` from the bridge means the desktop backend is unavailable — the VBF
 * codec is Rust-only and is deliberately **not** reimplemented in TypeScript,
 * because a second parser could drift from the backend.
 */
export async function inspectVbf(bytes: Uint8Array): Promise<VbfImportState> {
  if (bytes.length === 0) {
    return { kind: 'empty', message: 'The selected file is empty.' };
  }

  const report = await bridgeParseVbf(bytes);
  if (report === null) {
    return {
      kind: 'no-backend',
      message:
        'VBF parsing requires the desktop application. It is not available in the browser build.',
    };
  }

  if (report.segments.length === 0) {
    return {
      kind: 'empty',
      message: 'The container parsed successfully but holds no segments.',
    };
  }

  return { kind: 'ok', report };
}

/**
 * True when the container can be loaded into the single-buffer ROM document
 * without losing segment structure — i.e. it has exactly one segment.
 */
export function canLoadAsRom(report: VbfParseReport): boolean {
  return report.segments.length === 1;
}

/**
 * Extracts the bytes to load into the ROM document.
 *
 * Returns `null` for a multi-segment container: flattening it would discard
 * addresses, and this workflow refuses to do that silently.
 */
export function bytesForRom(report: VbfParseReport): Uint8Array | null {
  if (!canLoadAsRom(report)) return null;
  return new Uint8Array(report.segments[0].data);
}

/**
 * Serializes the current ROM as a single-segment VBF container.
 *
 * `baseAddress` must be known. A `null` base is refused rather than assumed to
 * be zero, matching the B4a rule that unknown and zero are different things.
 */
export async function exportRomAsVbf(
  rom: Uint8Array | null,
  baseAddress: number | null,
  swPartNumber: string
): Promise<VbfExportState> {
  if (!rom || rom.length === 0) {
    return { kind: 'no-rom', message: 'Load a ROM before exporting a VBF container.' };
  }
  if (baseAddress === null) {
    return {
      kind: 'unknown-base',
      message:
        'The ROM base address is unknown. Set it in the binary-diff dialog before exporting, ' +
        'so the segment address is real rather than assumed zero.',
    };
  }

  const segments: VbfSegmentPayload[] = [
    { start_address: baseAddress, data: Array.from(rom) },
  ];

  const bytes = await bridgeSerializeVbf(swPartNumber, segments);
  if (bytes === null) {
    return {
      kind: 'no-backend',
      message:
        'VBF serialization requires the desktop application. It is not available in the browser build.',
    };
  }
  return { kind: 'ok', bytes };
}

/** Human summary of one segment, for the inspection list. */
export function describeSegment(segment: VbfSegmentPayload): string {
  const addr = `0x${segment.start_address.toString(16).toUpperCase().padStart(8, '0')}`;
  const size = segment.data.length;
  return `${addr} · ${size} byte${size === 1 ? '' : 's'}`;
}

/**
 * A fixed limitation notice shown wherever VBF is offered.
 *
 * Stated in code so it cannot drift from the UI copy, and asserted by a test.
 */
export const VBF_LIMITATION_NOTICE =
  'Supports the simplified VBF dialect this application implements (ASCII header ' +
  'plus length-prefixed segments). Not validated against OEM VBF files.';
