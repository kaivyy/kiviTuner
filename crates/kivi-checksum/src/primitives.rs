use crc32fast::Hasher;
use kivi_core::types::Endian;

/// Calculates sum of bytes modulo 256.
pub fn sum_modulo_256(data: &[u8]) -> u8 {
    let mut sum: u32 = 0;
    for &b in data {
        sum = sum.wrapping_add(b as u32);
    }
    (sum & 0xFF) as u8
}

/// Calculates 8-bit two's complement.
pub fn twos_complement_8(val: u8) -> u8 {
    (!val).wrapping_add(1)
}

/// Calculates 16-bit two's complement.
pub fn twos_complement_16(val: u16) -> u16 {
    (!val).wrapping_add(1)
}

/// Calculates sum of 16-bit words modulo 65536 according to endianness.
/// If `data` has an odd length, the trailing byte is padded with 0x00.
pub fn sum_modulo_65536(data: &[u8], endian: Endian) -> u16 {
    let mut sum: u32 = 0;
    let (chunks, remainder) = data.as_chunks::<2>();
    for chunk in chunks {
        let word = match endian {
            Endian::Big => u16::from_be_bytes(*chunk),
            Endian::Little => u16::from_le_bytes(*chunk),
        };
        sum = sum.wrapping_add(word as u32);
    }
    if !remainder.is_empty() {
        let word = match endian {
            Endian::Big => u16::from_be_bytes([remainder[0], 0x00]),
            Endian::Little => u16::from_le_bytes([remainder[0], 0x00]),
        };
        sum = sum.wrapping_add(word as u32);
    }
    (sum & 0xFFFF) as u16
}

/// Computes standard IEEE 802.3 CRC32 checksum for `data`.
pub fn compute_crc32(data: &[u8]) -> u32 {
    let mut hasher = Hasher::new();
    hasher.update(data);
    hasher.finalize()
}

/// Computes CRC32 checksum for `data` with a custom initial seed.
pub fn compute_crc32_with_initial(data: &[u8], initial: u32) -> u32 {
    let mut hasher = Hasher::new_with_initial(initial);
    hasher.update(data);
    hasher.finalize()
}

/// CRC-32C (Castagnoli).
///
/// Ported from `ecu_engine_rust::math::checksum::ChecksumEngine::crc32_castagnoli`.
/// Parameters confirmed from source: reflected, polynomial `0x82F63B78`,
/// initial register `0xFFFFFFFF`, final XOR `0xFFFFFFFF`, input bytes fed
/// most-significant-bit-first within each byte after reflection.
///
/// Empty input yields `0x00000000` (the reflection of `init ^ xor_out`).
pub fn crc32_castagnoli(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0x82F6_3B78 & mask);
        }
    }
    !crc
}

/// Non-reflected (bit-forward) CRC-32 with caller-supplied parameters.
///
/// Ported from `ecu_engine_rust::math::checksum::ChecksumEngine::crc32_unreflected`.
/// The source treats each input byte as the most significant byte of the
/// register (`crc ^= (byte as u32) << 24`), shifts left, and applies `poly`
/// when the top bit is set. `init` is the raw register value (no reflection),
/// and `xor_out` is applied once at the end.
///
/// Because `init` is not complemented and `xor_out` is applied verbatim, an
/// empty buffer returns exactly `init ^ xor_out`.
pub fn crc32_unreflected(data: &[u8], poly: u32, init: u32, xor_out: u32) -> u32 {
    let mut crc = init;
    for &byte in data {
        crc ^= (byte as u32) << 24;
        for _ in 0..8 {
            if (crc & 0x8000_0000) != 0 {
                crc = (crc << 1) ^ poly;
            } else {
                crc <<= 1;
            }
        }
    }
    crc ^ xor_out
}

/// Simos 8 / 12 / 18 block CRC-32.
///
/// Ported from `ecu_engine_rust::math::checksum::ChecksumEngine::crc32_simos`.
/// Equivalent to [`crc32_unreflected`] with polynomial `0x04C11DB7`,
/// initial value `0x00000000`, and final XOR `0x00000000`.
pub fn crc32_simos(data: &[u8]) -> u32 {
    crc32_unreflected(data, 0x04C1_1DB7, 0x0000_0000, 0x0000_0000)
}

/// CRC-16/CCITT-FALSE.
///
/// Ported from `ecu_engine_rust::formats::VbfContainerCodec::crc16_ccitt`.
/// Parameters confirmed from source: polynomial `0x1021`, initial register
/// `0xFFFF`, **no** input/output reflection, **no** final XOR. Each input byte
/// is XORed into the high byte of the register before eight left shifts.
///
/// Empty input yields `0xFFFF`. This is CCITT-FALSE, not XMODEM (init `0x0000`).
pub fn crc16_ccitt(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &b in data {
        crc ^= (b as u16) << 8;
        for _ in 0..8 {
            if (crc & 0x8000) != 0 {
                crc = (crc << 1) ^ 0x1021;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

/// CRC-16/ARC (reflected polynomial `0xA001` / `0x8005` normal).
///
/// Used across Siemens MS41/MS43/GS20/SMG2, Mini EMS2000, and GM E38/E39/E92 CVN.
/// Standard check value: `crc16_arc(b"123456789", 0) == 0xBB3D`.
pub fn crc16_arc(data: &[u8], init: u16) -> u16 {
    let mut crc = init;
    for &b in data {
        crc ^= b as u16;
        for _ in 0..8 {
            crc = if (crc & 1) != 0 {
                (crc >> 1) ^ 0xA001
            } else {
                crc >> 1
            };
        }
    }
    crc
}

/// CRC-8 (reflected polynomial `0x8C` / Dallas-Maxim).
pub fn crc8_maxim(data: &[u8], init: u8) -> u8 {
    let mut crc = init;
    for &b in data {
        crc ^= b;
        for _ in 0..8 {
            crc = if (crc & 1) != 0 {
                (crc >> 1) ^ 0x8C
            } else {
                crc >> 1
            };
        }
    }
    crc
}
