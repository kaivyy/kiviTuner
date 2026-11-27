import type { MotoEcuProfile } from '../../lib/bridge/ecuToolsBridge.ts';
import type { SessionProtocolChoice } from '../../lib/flashBridge.ts';

export interface StandardRomPreset {
  id: string;
  label: string;
  sizeBytes: number;
  startAddress: number;
  blockSize: number;
  protocol: SessionProtocolChoice['kind'];
  description: string;
}

export const STANDARD_ROM_PRESETS: readonly StandardRomPreset[] = [
  {
    id: 'std_64k',
    label: 'Generic 64 KiB Full Read (0x10000)',
    sizeBytes: 64 * 1024,
    startAddress: 0x0000,
    blockSize: 0x100,
    protocol: 'uds_standard',
    description: 'Compact 8/16-bit ECU image (65,536 bytes)',
  },
  {
    id: 'std_128k',
    label: 'Generic 128 KiB Full Read (0x20000)',
    sizeBytes: 128 * 1024,
    startAddress: 0x0000,
    blockSize: 0x100,
    protocol: 'uds_standard',
    description: 'Small motorcycle & RL78/H8 class flash (131,072 bytes)',
  },
  {
    id: 'std_256k',
    label: 'Generic 256 KiB Full Read (0x40000)',
    sizeBytes: 256 * 1024,
    startAddress: 0x0000,
    blockSize: 0x100,
    protocol: 'uds_standard',
    description: 'M32R / Keihin / early CAN ECU image (262,144 bytes)',
  },
  {
    id: 'std_512k',
    label: 'Generic 512 KiB Full Read (0x80000)',
    sizeBytes: 512 * 1024,
    startAddress: 0x0000,
    blockSize: 0x100,
    protocol: 'uds_standard',
    description: 'Renesas SH7055 / early Denso / 512K CAN ECU (524,288 bytes)',
  },
  {
    id: 'std_1m',
    label: 'Generic 1 MiB Full Read (0x100000)',
    sizeBytes: 1024 * 1024,
    startAddress: 0x0000,
    blockSize: 0x100,
    protocol: 'uds_standard',
    description: 'SH7058 / Subaru / Honda 1M full dump (1,048,576 bytes)',
  },
  {
    id: 'std_2m',
    label: 'Generic 2 MiB Full Read (0x200000)',
    sizeBytes: 2 * 1024 * 1024,
    startAddress: 0x0000,
    blockSize: 0x100,
    protocol: 'uds_standard',
    description: 'MED17 / TriCore TC1766-class full dump (2,097,152 bytes)',
  },
  {
    id: 'std_4m',
    label: 'Generic 4 MiB Full Read (0x400000)',
    sizeBytes: 4 * 1024 * 1024,
    startAddress: 0x0000,
    blockSize: 0x100,
    protocol: 'uds_standard',
    description: 'Modern TC1797 / SH725xx 4 MiB full image (4,194,304 bytes)',
  },
];

/**
 * Mirrors built-in profiles from crates/kivi-flashing/src/moto_ecu_db.rs
 * for browser preview builds without a Tauri backend.
 */
export const FALLBACK_MOTO_PROFILES: readonly MotoEcuProfile[] = [
  {
    id: 'yamaha_64k_kline',
    make: 'Yamaha',
    model: 'YZF-R15 / MT / Vixion 64K (Write-Protected Boot)',
    years: '2008-2016',
    part_number: 'YAMAHA-64K',
    mcu: 'Mitsubishi/Mitsu 64K',
    rom_size_bytes: 64 * 1024,
    writable_start: 0x1800,
    writable_end_incl: 0x7fff,
    protected_ranges: [
      [0x0000, 0x17ff],
      [0x8000, 0xffff],
    ],
    checksum_offset: 0x7ffe,
    ecm_id_hex: null,
  },
  {
    id: 'yamaha_128k_kline',
    make: 'Yamaha',
    model: 'Aerox / NMAX / R25 128K (BBP-H594A)',
    years: '2017-2024',
    part_number: 'BBP-H594A-01/11',
    mcu: 'Renesas RL78',
    rom_size_bytes: 128 * 1024,
    writable_start: 0x00000,
    writable_end_incl: 0x1bfff,
    protected_ranges: [[0x1c000, 0x1ffff]],
    checksum_offset: 0x1bffe,
    ecm_id_hex: null,
  },
  {
    id: 'honda_cbr250rr_k64',
    make: 'Honda',
    model: 'CBR 250 RR (MC51)',
    years: '2017-2023',
    part_number: '38770-K64-N01/N04/ND1',
    mcu: 'Keihin SH7058',
    rom_size_bytes: 512 * 1024,
    writable_start: 0x08000,
    writable_end_incl: 0x7ffff,
    protected_ranges: [[0x00000, 0x07fff]],
    checksum_offset: 0x7fff8,
    ecm_id_hex: null,
  },
  {
    id: 'honda_cbr1000rr_06_07',
    make: 'Honda',
    model: 'CBR 1000 RR',
    years: '2006-2007',
    part_number: '38770-MEL-D21',
    mcu: 'Keihin M32R 256K',
    rom_size_bytes: 256 * 1024,
    writable_start: 0x08000,
    writable_end_incl: 0x3ffff,
    protected_ranges: [[0x00000, 0x07fff]],
    checksum_offset: 0x3fff8,
    ecm_id_hex: '01002B0101',
  },
  {
    id: 'honda_cbr1000rr_17_18',
    make: 'Honda',
    model: 'CBR 1000 RR (SC77)',
    years: '2017-2018',
    part_number: '38770-MKF-D43',
    mcu: 'Keihin 1 MiB',
    rom_size_bytes: 1024 * 1024,
    writable_start: 0x10000,
    writable_end_incl: 0xfffff,
    protected_ranges: [[0x00000, 0x0ffff]],
    checksum_offset: 0xffff8,
    ecm_id_hex: '0102490101',
  },
  {
    id: 'honda_cbr600rr_07_12',
    make: 'Honda',
    model: 'CBR 600 RR',
    years: '2007-2012',
    part_number: '38770-MFJ-D04/D42',
    mcu: 'Keihin M32R 256K',
    rom_size_bytes: 256 * 1024,
    writable_start: 0x08000,
    writable_end_incl: 0x3ffff,
    protected_ranges: [[0x00000, 0x07fff]],
    checksum_offset: 0x3fff8,
    ecm_id_hex: '0100330103',
  },
  {
    id: 'honda_cb500_13_16',
    make: 'Honda',
    model: 'CB 500 F / CBR 500 R',
    years: '2013-2016',
    part_number: '38770-MGZ-A03/D02',
    mcu: 'Keihin 256K',
    rom_size_bytes: 256 * 1024,
    writable_start: 0x08000,
    writable_end_incl: 0x3ffff,
    protected_ranges: [[0x00000, 0x07fff]],
    checksum_offset: 0x3fff8,
    ecm_id_hex: '0101250501',
  },
  {
    id: 'honda_cbr250r_kyj',
    make: 'Honda',
    model: 'CBR 250 R',
    years: '2011-2014',
    part_number: '38770-KYJ-922',
    mcu: 'Keihin 56K',
    rom_size_bytes: 56 * 1024,
    writable_start: 0x04000,
    writable_end_incl: 0x0dfff,
    protected_ranges: [[0x00000, 0x03fff]],
    checksum_offset: 0x0dfef,
    ecm_id_hex: '0100B80502',
  },
  {
    id: 'honda_msx125_k26',
    make: 'Honda',
    model: 'MSX 125 / Grom',
    years: '2013-2019',
    part_number: '38770-K26-911',
    mcu: 'Keihin 56K/64K',
    rom_size_bytes: 64 * 1024,
    writable_start: 0x04000,
    writable_end_incl: 0x0ffff,
    protected_ranges: [[0x00000, 0x03fff]],
    checksum_offset: null,
    ecm_id_hex: '0101350501',
  },
  {
    id: 'honda_vfr1200f',
    make: 'Honda',
    model: 'VFR 1200 F',
    years: '2010-2011',
    part_number: '38770-MGE-D02',
    mcu: 'Keihin 512K',
    rom_size_bytes: 512 * 1024,
    writable_start: 0x08000,
    writable_end_incl: 0x7ffff,
    protected_ranges: [[0x00000, 0x07fff]],
    checksum_offset: 0x7fff8,
    ecm_id_hex: '0100AA0102',
  },
];

export function parseHexOrDec(text: string): number {
  const trimmed = text.trim();
  if (trimmed === '') return Number.NaN;
  if (/^0x[0-9a-f]+$/i.test(trimmed)) return Number.parseInt(trimmed, 16);
  if (/^[0-9]+$/.test(trimmed)) return Number.parseInt(trimmed, 10);
  return Number.NaN;
}

export function formatHexAddr(addr: number, minDigits = 4): string {
  if (!Number.isFinite(addr) || addr < 0) return '0x0000';
  return `0x${Math.floor(addr).toString(16).toUpperCase().padStart(minDigits, '0')}`;
}

export function formatByteSize(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return 'invalid';
  if (bytes >= 1024 * 1024 && bytes % (1024 * 1024) === 0) {
    return `${bytes / (1024 * 1024)} MiB`;
  }
  if (bytes >= 1024 * 1024) {
    return `${(bytes / (1024 * 1024)).toFixed(2)} MiB`;
  }
  if (bytes >= 1024 && bytes % 1024 === 0) {
    return `${bytes / 1024} KiB`;
  }
  if (bytes >= 1024) {
    return `${(bytes / 1024).toFixed(1)} KiB`;
  }
  return `${Math.floor(bytes)} B`;
}

export function inferRecommendedProtocol(
  profile: MotoEcuProfile
): SessionProtocolChoice['kind'] {
  const lower = `${profile.make} ${profile.mcu} ${profile.model}`.toLowerCase();
  if (lower.includes('shindengen')) return 'honda_shindengen_k_line';
  if (profile.make.toLowerCase() === 'honda' || lower.includes('keihin')) {
    return 'honda_keihin_k_line';
  }
  return 'uds_standard';
}

export function validateMotoWriteRange(
  profile: MotoEcuProfile,
  writeStart: number,
  writeLen: number
): { ok: boolean; reason: string | null } {
  if (!Number.isInteger(writeStart) || writeStart < 0) {
    return { ok: false, reason: 'Start address must be a valid non-negative address.' };
  }
  if (!Number.isInteger(writeLen) || writeLen <= 0) {
    return { ok: false, reason: 'Write length must be > 0.' };
  }
  const writeEnd = writeStart + writeLen - 1;
  if (!Number.isSafeInteger(writeEnd) || writeEnd > 0xffffffff) {
    return { ok: false, reason: 'Write range overflow.' };
  }
  for (const [ps, pe] of profile.protected_ranges) {
    if (writeStart <= pe && writeEnd >= ps) {
      return {
        ok: false,
        reason: `${profile.make} (${profile.model}) write ${formatHexAddr(writeStart, 5)}..=${formatHexAddr(writeEnd, 5)} overlaps protected bootloader/guard ${formatHexAddr(ps, 5)}..=${formatHexAddr(pe, 5)}`,
      };
    }
  }
  if (writeStart < profile.writable_start || writeEnd > profile.writable_end_incl) {
    return {
      ok: false,
      reason: `${profile.make} (${profile.model}) allows writes only within ${formatHexAddr(profile.writable_start, 5)}..=${formatHexAddr(profile.writable_end_incl, 5)} (requested ${formatHexAddr(writeStart, 5)}..=${formatHexAddr(writeEnd, 5)})`,
    };
  }
  return { ok: true, reason: null };
}

export type FlashLogLevel = 'info' | 'ok' | 'warn' | 'error';

export interface FlashLogEntry {
  id: number;
  time: string;
  level: FlashLogLevel;
  message: string;
}

export function formatLogClock(now: Date): string {
  const hh = String(now.getHours()).padStart(2, '0');
  const mm = String(now.getMinutes()).padStart(2, '0');
  const ss = String(now.getSeconds()).padStart(2, '0');
  return `${hh}:${mm}:${ss}`;
}

export function appendFlashLog(
  entries: FlashLogEntry[],
  level: FlashLogLevel,
  message: string,
  now: Date = new Date(),
  maxEntries = 200
): FlashLogEntry[] {
  const nextId = entries.length > 0 ? entries[entries.length - 1].id + 1 : 1;
  const next = [
    ...entries,
    {
      id: nextId,
      time: formatLogClock(now),
      level,
      message,
    },
  ];
  return next.length > maxEntries ? next.slice(next.length - maxEntries) : next;
}
