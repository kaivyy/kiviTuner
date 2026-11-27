/**
 * Checksum controller for the editor.
 *
 * Read-only: delegates to `bridgeComputeChecksum`, which never mutates the ROM.
 *
 * Verified bridge contract (`src/lib/tauriBridge.ts`):
 * - Returns `null` **only** for an unrecognised algorithm id. IPC failures fall
 *   through to the in-process fallback, so a backend error surfaces as a thrown
 *   exception rather than `null`. This is why `null` maps to `unsupported` and
 *   not to `error`.
 */

import { bridgeComputeChecksum } from '../../lib/tauriBridge.ts';

export type ChecksumAlgorithmId =
  | 'crc32_ieee'
  | 'crc32_castagnoli'
  | 'crc32_simos'
  | 'crc16_ccitt'
  | 'crc16_modbus';

export interface ChecksumAlgorithmOption {
  id: ChecksumAlgorithmId;
  label: string;
  /** `false` for ids the backend rejects; the UI disables them. */
  supported: boolean;
}

/**
 * The algorithm set exposed by `compute_checksum_inner`.
 *
 * `crc16_modbus` is listed because the source engine declares it, but the
 * backend never implements it — it returns an error. It is marked unsupported
 * so the UI can show it disabled rather than failing at click time.
 */
export const CHECKSUM_ALGORITHMS: ChecksumAlgorithmOption[] = [
  { id: 'crc32_ieee', label: 'CRC-32/IEEE', supported: true },
  { id: 'crc32_castagnoli', label: 'CRC-32C (Castagnoli)', supported: true },
  { id: 'crc32_simos', label: 'CRC-32 (Simos, unreflected)', supported: true },
  { id: 'crc16_ccitt', label: 'CRC-16/CCITT-FALSE', supported: true },
  { id: 'crc16_modbus', label: 'CRC-16/MODBUS (not implemented)', supported: false },
];

export type ChecksumPanelState =
  | { kind: 'no-rom' }
  | { kind: 'unsupported'; algorithm: string }
  | { kind: 'ok'; value: number; widthBits: number; inputLen: number; algorithm: string }
  | { kind: 'error'; message: string };

/**
 * Computes a checksum over the supplied ROM bytes.
 *
 * Returns `no-rom` when no ROM is loaded. An empty buffer is treated as no ROM
 * because `ingest_binary` rejects zero-length input upstream, so an empty
 * buffer here means the editor has nothing loaded.
 */
export async function runChecksum(
  algorithm: ChecksumAlgorithmId,
  rom: Uint8Array | null,
  workspaceLoaded = false
): Promise<ChecksumPanelState> {
  if ((!rom || rom.length === 0) && !workspaceLoaded) {
    return { kind: 'no-rom' };
  }

  try {
    const result = await bridgeComputeChecksum(algorithm, rom);
    if (result === null) {
      return { kind: 'unsupported', algorithm };
    }
    return {
      kind: 'ok',
      value: result.value,
      widthBits: result.width_bits,
      inputLen: result.input_len,
      algorithm: result.algorithm,
    };
  } catch (err) {
    return { kind: 'error', message: err instanceof Error ? err.message : String(err) };
  }
}

/** Formats a checksum as zero-padded uppercase hex for its width. */
export function formatChecksumValue(value: number, widthBits: number): string {
  const digits = widthBits / 4;
  return `0x${value.toString(16).toUpperCase().padStart(digits, '0')}`;
}
