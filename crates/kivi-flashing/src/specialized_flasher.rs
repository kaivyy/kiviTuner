//! Ported from `ecu_engine_rust::flashing::specialized_flasher`.
//!
//! Pure byte-level protocol logic: builders, parsers, encoders, state machines.
//! No I/O, no device access, no `unsafe`. Transport is the caller's concern.
//!
//! Verified by unit tests with hand-derived expectations. **Not** validated
//! against a physical ECU; no such claim is made.

/// 16-Round Reversible Feistel CAN SecurityAccess & RAM Kernel Cipher.
pub struct FeistelCanKernelSecurity;

impl FeistelCanKernelSecurity {
    /// 32-entry non-linear substitution table for Feistel round key expansion.
    pub const INDEX_TRANSFORM: [u8; 32] = [
        0x5, 0x6, 0x7, 0x1, 0x9, 0xC, 0xD, 0x8,
        0xA, 0xD, 0x2, 0xB, 0xF, 0x4, 0x0, 0x3,
        0xB, 0x4, 0x6, 0x0, 0xF, 0x2, 0xD, 0x9,
        0x5, 0xC, 0x1, 0xA, 0x3, 0xD, 0xE, 0x8,
    ];

    /// 16-element round key substitution table for stock SecurityAccess.
    pub const STOCK_KEY_TABLE: [u16; 16] = [
        0x78B1, 0x4625, 0x201C, 0x9EA5,
        0xAD6B, 0x35F4, 0xFD21, 0x5E71,
        0xB046, 0x7F4A, 0x4B75, 0x93F9,
        0x1895, 0x8961, 0x3ECC, 0x862B,
    ];

    /// 4-round encryption key schedule for RAM kernel upload scrambling.
    pub const ENCRYPT_TABLE: [u16; 4] = [0xC85B, 0x32C0, 0xE282, 0x92A0];
    /// 4-round decryption key schedule for RAM kernel upload scrambling.
    pub const DECRYPT_TABLE: [u16; 4] = [0x92A0, 0xE282, 0x32C0, 0xC85B];

    /// Reflected CRC-32 polynomial used by microcontroller flash kernels.
    pub const KERNEL_CRC_POLYNOMIAL: u32 = 0x5AA5A55A;

    /// Feistel round key computation: mixes lower 16 bits with round key using substitution table.
    pub fn round_key(low: u16, key: u16) -> u16 {
        let mut index = (low ^ key) as u32;
        index = index.wrapping_add(index << 16);
        let mut result: u16 = 0;
        for n in 0..4 {
            let nibble = Self::INDEX_TRANSFORM[((index >> (n * 4)) & 0x1F) as usize];
            result = result.wrapping_add((nibble as u16) << (n * 4));
        }
        (result >> 3).wrapping_add(result << 13) // rotate right 3 / rotate left 13
    }

    /// Single forward Feistel round: (high, low) -> (low, round_key(low) ^ high).
    pub fn round(state: u32, key: u16) -> u32 {
        let low = (state & 0xFFFF) as u16;
        let high = (state >> 16) as u16;
        let new_low = Self::round_key(low, key) ^ high;
        (new_low as u32) | ((low as u32) << 16)
    }

    /// Single inverse Feistel round.
    pub fn inverse_round(state: u32, key: u16) -> u32 {
        let low = (state & 0xFFFF) as u16;
        let high = (state >> 16) as u16;
        let orig_high = Self::round_key(high, key) ^ low;
        ((high as u32) & 0xFFFF) | ((orig_high as u32) << 16)
    }

    /// Feistel network transformation across a sequence of round keys with final 16-bit half swap.
    pub fn feistel(word: u32, keys: &[u16]) -> u32 {
        let mut state = word;
        for &k in keys {
            state = Self::round(state, k);
        }
        (state >> 16) | (state << 16)
    }

    /// Solve 4-byte SecurityAccess key from seed (16-round Feistel network).
    pub fn solve_stock_security_key(seed_bytes: [u8; 4]) -> [u8; 4] {
        let state = u32::from_be_bytes(seed_bytes);
        let rev_keys: Vec<u16> = Self::STOCK_KEY_TABLE.iter().rev().copied().collect();
        let key = Self::feistel(state, &rev_keys);
        key.to_be_bytes()
    }

    /// Scramble RAM kernel binary payload before transmission over CAN.
    pub fn encrypt_kernel_payload(data: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(data.len() & !3);
        let chunks = data.chunks_exact(4);
        for c in chunks {
            let word = u32::from_be_bytes([c[0], c[1], c[2], c[3]]);
            let enc = Self::feistel(word, &Self::ENCRYPT_TABLE);
            out.extend_from_slice(&enc.to_be_bytes());
        }
        out
    }

    /// Descramble RAM kernel binary payload (exact inverse of encrypt).
    pub fn decrypt_kernel_payload(data: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(data.len() & !3);
        let chunks = data.chunks_exact(4);
        for c in chunks {
            let word = u32::from_be_bytes([c[0], c[1], c[2], c[3]]);
            let dec = Self::feistel(word, &Self::DECRYPT_TABLE);
            out.extend_from_slice(&dec.to_be_bytes());
        }
        out
    }

    /// Calculate reflected CRC-32 with 0x5AA5A55A polynomial.
    pub fn calculate_kernel_crc(data: &[u8]) -> u32 {
        let mut crc: u32 = 0xFFFFFFFF;
        for &byte in data {
            let mut c = byte as u32;
            for _ in 0..8 {
                let bit = (crc ^ c) & 1;
                crc = if bit != 0 {
                    (crc >> 1) ^ Self::KERNEL_CRC_POLYNOMIAL
                } else {
                    crc >> 1
                };
                c >>= 1;
            }
        }
        !crc
    }
}

/// Multi-Region Checksum Record Descriptor (12-byte entry in ROM table).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionChecksumRecord {
    pub index: usize,
    pub start_address: u32,
    pub end_address: u32,
    pub stored_diff: u32,
    pub computed_diff: u32,
    pub is_blank: bool,
    pub is_valid: bool,
}

/// Multi-Region Table Checksum Resolver for 32-bit RISC Automotive ROMs.
pub struct MultiRegionTableChecksum;

impl MultiRegionTableChecksum {
    pub const MAGIC_DIFF: u32 = 0x5AA5A55A;

    /// Verify 17-record checksum table in ROM buffer.
    pub fn verify_rom_table(
        rom_bytes: &[u8],
        table_start_offset: usize,
        record_count: usize,
    ) -> Result<Vec<RegionChecksumRecord>, &'static str> {
        let table_len = record_count
            .checked_mul(12)
            .ok_or("Checksum table size overflow")?;
        if table_start_offset
            .checked_add(table_len)
            .is_none_or(|end| end > rom_bytes.len())
        {
            return Err("Checksum table offset out of bounds");
        }

        let mut records = Vec::with_capacity(record_count);

        for rec_idx in 0..record_count {
            let base = table_start_offset + rec_idx * 12;
            let start_addr = u32::from_be_bytes([
                rom_bytes[base], rom_bytes[base + 1], rom_bytes[base + 2], rom_bytes[base + 3],
            ]);
            let end_addr = u32::from_be_bytes([
                rom_bytes[base + 4], rom_bytes[base + 5], rom_bytes[base + 6], rom_bytes[base + 7],
            ]);
            let stored_diff = u32::from_be_bytes([
                rom_bytes[base + 8], rom_bytes[base + 9], rom_bytes[base + 10], rom_bytes[base + 11],
            ]);

            let is_blank = start_addr == 0 && end_addr == 0;
            if start_addr > end_addr {
                return Err("Invalid region range: start address exceeds end address");
            }
            let mut sum: u32 = 0;

            if !is_blank {
                let s = start_addr as usize;
                let e = end_addr as usize;
                if e > rom_bytes.len() {
                    return Err("Region address exceeds ROM size");
                }
                let mut ptr = s;
                while ptr + 4 <= e {
                    let w = u32::from_be_bytes([
                        rom_bytes[ptr], rom_bytes[ptr + 1], rom_bytes[ptr + 2], rom_bytes[ptr + 3],
                    ]);
                    sum = sum.wrapping_add(w);
                    ptr += 4;
                }
            }

            let computed_diff = if is_blank {
                stored_diff
            } else {
                Self::MAGIC_DIFF.wrapping_sub(sum)
            };

            let is_valid = is_blank || stored_diff == computed_diff;

            records.push(RegionChecksumRecord {
                index: rec_idx,
                start_address: start_addr,
                end_address: end_addr,
                stored_diff,
                computed_diff,
                is_blank,
                is_valid,
            });
        }

        Ok(records)
    }

    /// Automatically correct mismatched records in ROM buffer in-place.
    pub fn correct_rom_table(
        rom_bytes: &mut [u8],
        table_start_offset: usize,
        record_count: usize,
    ) -> Result<usize, &'static str> {
        let records = Self::verify_rom_table(rom_bytes, table_start_offset, record_count)?;
        let mut corrected_count = 0;

        for r in records {
            if !r.is_valid && !r.is_blank {
                let diff_offset = table_start_offset + r.index * 12 + 8;
                let bytes = r.computed_diff.to_be_bytes();
                rom_bytes[diff_offset..diff_offset + 4].copy_from_slice(&bytes);
                corrected_count += 1;
            }
        }

        Ok(corrected_count)
    }
}

/// DS2 Serial K-Line Frame Protocol & Immobilizer ISN Synchronizer.
pub struct Ds2SerialProtocol;

impl Ds2SerialProtocol {
    /// Diagnostic Address for Engine Management Unit (DME).
    pub const DME_ADDRESS: u8 = 0x12;
    /// Diagnostic Address for Electronic Immobilizer System (EWS).
    pub const EWS_ADDRESS: u8 = 0x44;

    /// Build DS2 diagnostic request frame: `[address, length, payload..., xor_checksum]`.
    pub fn build_frame(target_address: u8, payload: &[u8]) -> Vec<u8> {
        let length = (1 + 1 + payload.len() + 1) as u8;
        let mut frame = Vec::with_capacity(length as usize);
        frame.push(target_address);
        frame.push(length);
        frame.extend_from_slice(payload);

        let mut cks: u8 = 0;
        for &b in &frame {
            cks ^= b;
        }
        frame.push(cks);
        frame
    }

    /// Validate and extract payload from raw serial receive buffer.
    pub fn extract_response(expected_address: u8, buffer: &[u8]) -> Option<Vec<u8>> {
        if buffer.len() < 4 {
            return None;
        }

        // Scan backwards for last matching frame
        for i in (0..=buffer.len() - 4).rev() {
            if buffer[i] == expected_address {
                let frame_len = buffer[i + 1] as usize;
                if frame_len >= 3 && i + frame_len <= buffer.len() {
                    let frame = &buffer[i..i + frame_len];
                    let mut cks: u8 = 0;
                    for &b in &frame[..frame.len() - 1] {
                        cks ^= b;
                    }
                    if cks == frame[frame.len() - 1] {
                        // Return payload excluding address, length, and checksum
                        return Some(frame[2..frame.len() - 1].to_vec());
                    }
                }
            }
        }
        None
    }

    /// Convert 4 BCD nibbles from DME ISN response to 12-bit Immobilizer ISN word.
    /// Example: `[0x03, 0x05, 0x02, 0x01] -> decimal 3521 -> 0x0DC1`.
    pub fn dme_bcd_to_isn(nibbles: [u8; 4]) -> u16 {
        let d0 = (nibbles[0] & 0x0F) as u32;
        let d1 = (nibbles[1] & 0x0F) as u32;
        let d2 = (nibbles[2] & 0x0F) as u32;
        let d3 = (nibbles[3] & 0x0F) as u32;
        let decimal = d0 * 1000 + d1 * 100 + d2 * 10 + d3;
        (decimal & 0x0FFF) as u16
    }

    /// Build DS2 EWS Write ISN command frame (Command `0x61`).
    pub fn build_ews_write_isn(isn: u16) -> Vec<u8> {
        let hi = ((isn >> 8) & 0x0F) as u8;
        let lo = (isn & 0xFF) as u8;
        Self::build_frame(Self::EWS_ADDRESS, &[0x61, hi, lo])
    }
}


/// Trionic 8 CAN SecurityAccess Seed-Key Engine (GM / Opel / Saab).
pub struct Trionic8SecurityAccess;

impl Trionic8SecurityAccess {
    /// Calculate Trionic 8 access key from 2-byte seed for access levels (0xFB or 0xFD).
    pub fn calculate_access_key(seed: [u8; 2], level: u8) -> [u8; 2] {
        let val = ((seed[0] as u32) << 8) | (seed[1] as u32);

        let mut key = (((val >> 5) | (val << 11)) + 0xB988) & 0xFFFF;

        match level {
            0xFB => {
                key ^= 0x8749;
                key = (key + 0x06D3) & 0xFFFF;
                key ^= 0xCFDF;
            }
            0xFD => {
                key /= 3;
                key ^= 0x8749;
                key = (key + 0x0ACF) & 0xFFFF;
                key ^= 0x81BF;
            }
            _ => {}
        }

        [((key >> 8) & 0xFF) as u8, (key & 0xFF) as u8]
    }
}

/// Kawasaki Diagnostic System (KDS) & Motorcycle Security Access Engine.
/// Tailored for Kawasaki Ninja ZX-25R, ZX-6R, ZX-10R, and H2 (Denso / Mitsubishi Electric ECUs).
/// Supports Part Numbers: 21175-1560, 21175-1561, 21175-1562, 21175-1798.
pub struct KawasakiKdsProtocol;

impl KawasakiKdsProtocol {
    pub const ECU_TARGET_ADDRESS: u8 = 0x11; // Standard KDS Engine ECU K-Line Address
    pub const TESTER_SOURCE_ADDRESS: u8 = 0xF1; // Diagnostic Tool Source Address
    pub const KDS_HEADER_FMT: u8 = 0x81; // KWP2000 physical addressing header

    pub const KNOWN_ZX25R_PART_NUMBERS: [&'static str; 4] = [
        "21175-1560", // ZX-25R 2020 Standard (Non-ABS)
        "21175-1561", // ZX-25R 2020 ABS / SE Quickshifter
        "21175-1562", // ZX-25R 2020 Japan / Euro 5 Spec
        "21175-1798", // ZX-25R 2023+ Facelift TFT Speedometer
    ];

    /// Build KWP2000 Fast Init frame: [0x81, 0x11, 0xF1, 0x81, 0x04]
    pub fn build_kds_fast_init() -> Vec<u8> {
        let mut pkt = vec![Self::KDS_HEADER_FMT, Self::ECU_TARGET_ADDRESS, Self::TESTER_SOURCE_ADDRESS, 0x81];
        let mut cs: u8 = 0;
        for &b in &pkt {
            cs = cs.wrapping_add(b);
        }
        pkt.push(cs);
        pkt
    }

    /// Build ReadECUIdentification (KWP2000 Service 0x1A 0x90) request packet.
    /// Carries 2 payload bytes (`0x1A 0x90`), so format header is `0x82` (`0x80 | 2`).
    pub fn build_read_part_number_request() -> Vec<u8> {
        let mut pkt = vec![0x82, Self::ECU_TARGET_ADDRESS, Self::TESTER_SOURCE_ADDRESS, 0x1A, 0x90];
        let mut cs: u8 = 0;
        for &b in &pkt {
            cs = cs.wrapping_add(b);
        }
        pkt.push(cs);
        pkt
    }

    /// Parse KDS Identification Response (Positive 0x5A 0x90) and extract Kawasaki OEM Part Number.
    pub fn extract_kawasaki_part_number(response: &[u8]) -> Option<String> {
        let min_len = 10;
        let mut curr_str = Vec::new();

        for &b in response {
            if (b >= 0x20 && b <= 0x7E) && b != b'$' && b != b'#' {
                curr_str.push(b);
            } else {
                if curr_str.len() >= min_len {
                    if let Ok(s) = std::str::from_utf8(&curr_str) {
                        let trimmed = s.trim();
                        if trimmed.starts_with("21175-") {
                            return Some(trimmed.to_string());
                        }
                    }
                }
                curr_str.clear();
            }
        }
        if curr_str.len() >= min_len {
            if let Ok(s) = std::str::from_utf8(&curr_str) {
                let trimmed = s.trim();
                if trimmed.starts_with("21175-") {
                    return Some(trimmed.to_string());
                }
            }
        }
        None
    }

    /// Check if extracted part number matches known ZX-25R series.
    pub fn is_zx25r_part_number(part_num: &str) -> bool {
        Self::KNOWN_ZX25R_PART_NUMBERS.contains(&part_num)
    }

    /// Simulated/bench-only Kawasaki Denso SecurityAccess key routine (unverified against live KDS hardware).
    /// Uses non-linear bitwise permutation and rolling XOR key schedule.
    pub fn calculate_kds_key(seed: [u8; 4]) -> [u8; 4] {
        let s32 = u32::from_be_bytes(seed);
        // Kawasaki non-linear bit rotation & mask constant
        let mut k32 = s32.rotate_left(7) ^ 0x4B617761; // 'Kawa'
        k32 = k32.wrapping_add(0x73616B69);             // 'saki'
        k32 = k32.rotate_right(3) ^ 0x5A583235;        // 'ZX25'
        k32.to_be_bytes()
    }
}
