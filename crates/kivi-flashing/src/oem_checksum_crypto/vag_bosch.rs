/// 1. Continental Simos 8 / 12 / 18 Dual Checksum (`CRC32-MPEG2` + `ECM3` 64-Bit) & Counter-XOR.
pub struct SimosChecksumAndCrypto;

impl SimosChecksumAndCrypto {
    /// Non-reflected `CRC32` (`poly = 0x04C11DB7`, `init = 0x0`, `xor_out = 0x0`) used by Simos 18.
    pub fn crc32_vw_mpeg2(data: &[u8], init_crc: u32) -> u32 {
        let mut crc = init_crc;
        for &b in data {
            crc ^= (b as u32) << 24;
            for _ in 0..8 {
                if (crc & 0x8000_0000) != 0 {
                    crc = (crc << 1) ^ 0x04C1_1DB7;
                } else {
                    crc <<= 1;
                }
            }
        }
        crc
    }

    /// Simos Counter-XOR stream transformation (`simos_xor.py`): `out[i] = data[i] ^ (i as u8)`.
    pub fn simos_counter_xor(data: &[u8]) -> Vec<u8> {
        data.iter()
            .enumerate()
            .map(|(i, &b)| b ^ (i as u8))
            .collect()
    }

    /// Validates and fixes the Simos block `CRC32` table at `header_offset` (`0x300` standard or `0x340` `CBOOT_TEMP`).
    pub fn validate_and_fix_simos_block_crc32(
        block_data: &mut [u8],
        base_address: u32,
        is_cboot_temp: bool,
    ) -> Result<(u32, u32, bool), String> {
        let hoff = if is_cboot_temp { 0x340usize } else { 0x300usize };
        if block_data.len() < hoff + 20 {
            return Err("Simos block too small for CRC32 descriptor table".to_string());
        }
        let stored_crc = u32::from_le_bytes([
            block_data[hoff + 4],
            block_data[hoff + 5],
            block_data[hoff + 6],
            block_data[hoff + 7],
        ]);
        let n_areas = block_data[hoff + 8] as usize;
        if n_areas == 0 || n_areas > 16 || hoff + 12 + n_areas * 8 > block_data.len() {
            return Err(format!("Invalid Simos CRC32 area count: {}", n_areas));
        }

        let mut calc_crc = 0u32;
        for i in 0..n_areas {
            let off = hoff + 12 + i * 8;
            let start_abs = u32::from_le_bytes([
                block_data[off],
                block_data[off + 1],
                block_data[off + 2],
                block_data[off + 3],
            ]);
            let end_abs = u32::from_le_bytes([
                block_data[off + 4],
                block_data[off + 5],
                block_data[off + 6],
                block_data[off + 7],
            ]);
            if start_abs < base_address || end_abs < start_abs {
                return Err("Simos CRC32 range below base_address".to_string());
            }
            let s_rel = (start_abs - base_address) as usize;
            let e_rel = usize::try_from((end_abs as u64 - base_address as u64) + 1)
                .map_err(|_| "Simos CRC32 range overflows platform address space".to_string())?;
            if e_rel > block_data.len() {
                return Err("Simos CRC32 range exceeds block bounds".to_string());
            }
            calc_crc = Self::crc32_vw_mpeg2(&block_data[s_rel..e_rel], calc_crc);
        }

        let was_valid = stored_crc == calc_crc;
        if !was_valid {
            block_data[hoff + 4..hoff + 8].copy_from_slice(&calc_crc.to_le_bytes());
        }
        Ok((stored_crc, calc_crc, was_valid))
    }

    /// Validates and fixes the Simos `ECM3` 64-Bit Runtime Monitor Checksum (`CAL + 0x400`)
    /// by summing 32-bit LE words across `CAL` and `ASW1` region pairs into a `u64` accumulator.
    pub fn validate_and_fix_simos_ecm3_64bit(
        cal_data: &mut [u8],
        cal_base_address: u32,
        cal_regions: &[(u32, u32)],
        asw1_base_sum: u64,
    ) -> Result<(u64, u64, bool), String> {
        if cal_data.len() < 0x408 {
            return Err("Simos CAL block too small for ECM3 64-bit checksum at 0x400".to_string());
        }
        let stored_sum = u64::from_le_bytes([
            cal_data[0x400],
            cal_data[0x401],
            cal_data[0x402],
            cal_data[0x403],
            cal_data[0x404],
            cal_data[0x405],
            cal_data[0x406],
            cal_data[0x407],
        ]);

        let mut total_sum = asw1_base_sum;
        for &(start_abs, end_abs) in cal_regions {
            if start_abs < cal_base_address || end_abs < start_abs {
                return Err("ECM3 CAL region out of range".to_string());
            }
            let s_rel = (start_abs - cal_base_address) as usize;
            let e_rel = usize::try_from((end_abs as u64 - cal_base_address as u64) + 1)
                .map_err(|_| "ECM3 CAL region overflows platform address space".to_string())?;
            if e_rel > cal_data.len() || (e_rel - s_rel) % 4 != 0 {
                return Err("ECM3 CAL region bounds or 4-byte alignment invalid".to_string());
            }
            for chunk in cal_data[s_rel..e_rel].chunks_exact(4) {
                let word = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]) as u64;
                total_sum = total_sum.wrapping_add(word);
            }
        }

        let was_valid = stored_sum == total_sum;
        if !was_valid {
            cal_data[0x400..0x408].copy_from_slice(&total_sum.to_le_bytes());
        }
        Ok((stored_sum, total_sum, was_valid))
    }
}


/// 2. VAG DSG (`DQ250 MQB` Progressive S-Box Cipher, `DSG JAMCRC`, & `DQ381` Block Checksum).
pub struct DsgMechatronicCryptoAndChecksum;

impl DsgMechatronicCryptoAndChecksum {
    /// Exact 256-byte substitution table extracted from `DQ250_MQB_0D9300012L_4516` (`0x8001053C`).
    pub const MQB_DSG_SBOX: [u8; 256] = [
        0x45, 0x93, 0xCA, 0x72, 0x84, 0x5D, 0x11, 0xE9, 0x78, 0xB9, 0x9F, 0xA2, 0x65, 0xE2, 0x8B, 0xE5,
        0xC9, 0x77, 0xCD, 0x59, 0xF7, 0x89, 0x01, 0xBF, 0x02, 0xF3, 0x68, 0xE0, 0xDC, 0x30, 0xC4, 0xD4,
        0xF5, 0x25, 0xCC, 0x87, 0x23, 0xC8, 0xEE, 0x3A, 0x12, 0xAB, 0x70, 0x61, 0x00, 0x41, 0xD3, 0x0D,
        0xC0, 0xF4, 0x22, 0xAF, 0xD1, 0xCB, 0xB0, 0xB7, 0x15, 0x27, 0x8F, 0xD9, 0x09, 0xAC, 0x5C, 0xF2,
        0xF1, 0xC7, 0x2A, 0x21, 0x56, 0x8C, 0x06, 0x35, 0x08, 0xE7, 0xC3, 0x73, 0x49, 0x39, 0xCF, 0x40,
        0x43, 0x6B, 0xC6, 0x6E, 0xEA, 0x90, 0x6D, 0x1B, 0xB4, 0x53, 0xD2, 0x5F, 0xB6, 0x74, 0x16, 0xD7,
        0x2B, 0x97, 0x5A, 0x8A, 0x58, 0xC5, 0x33, 0x75, 0xFC, 0xAD, 0x3B, 0x60, 0xCE, 0x62, 0x91, 0x2D,
        0x0C, 0x3C, 0xDA, 0x42, 0x26, 0x92, 0x83, 0xFA, 0x18, 0x28, 0x32, 0x07, 0x4A, 0xBB, 0x54, 0x2C,
        0xB8, 0x9D, 0x4C, 0xE4, 0xA4, 0x71, 0x5E, 0xA7, 0xED, 0x2F, 0x37, 0x76, 0x34, 0xD8, 0xC1, 0x8E,
        0x44, 0x63, 0x64, 0x51, 0xF6, 0x1E, 0xC2, 0xF8, 0x69, 0x0A, 0xDF, 0xE6, 0xF9, 0x9E, 0x4E, 0xA8,
        0xFF, 0x88, 0x9A, 0x5B, 0x0F, 0x1F, 0x3F, 0x24, 0xE3, 0xFE, 0x9B, 0xB3, 0xA5, 0x80, 0x86, 0x96,
        0x1D, 0x50, 0xDE, 0x04, 0xA6, 0x55, 0x3D, 0x05, 0x9C, 0xEB, 0x8D, 0xB5, 0xEF, 0x94, 0x6C, 0x31,
        0xA0, 0x20, 0x14, 0x13, 0x4F, 0x66, 0xF0, 0x29, 0x46, 0xBD, 0xD6, 0x4B, 0xDB, 0xBE, 0x36, 0x0B,
        0x85, 0xA1, 0xB2, 0xB1, 0x19, 0x10, 0xFD, 0x1C, 0x81, 0xE1, 0x0E, 0x7D, 0xA9, 0xE8, 0x47, 0xD5,
        0x1A, 0x03, 0x57, 0x38, 0xBC, 0xAE, 0x98, 0xFB, 0xAA, 0x7E, 0x6A, 0x52, 0x6F, 0x17, 0xA3, 0x67,
        0x7A, 0x7B, 0xDD, 0xEC, 0x2E, 0x3E, 0x7F, 0x7C, 0x48, 0xD0, 0x99, 0xBA, 0x79, 0x95, 0x4D, 0x82,
    ];

    /// Decrypts a `DQ250 MQB` payload using the progressive substitution cipher (`0x800164AC`).
    pub fn dq250_decrypt(data: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(data.len());
        let mut offset: u32 = 0;
        let mut rolling_stream_offset: u32 = 0;
        let mut last_data: u32 = 0;

        for &b in data {
            let idx = ((b as u32).wrapping_add(offset) & 0xFF) as usize;
            let plain = Self::MQB_DSG_SBOX[idx];
            offset = offset.wrapping_add(plain as u32);
            offset = offset.wrapping_add(last_data);
            rolling_stream_offset = rolling_stream_offset.wrapping_add(0x167);
            let sbox_roll = Self::MQB_DSG_SBOX[((rolling_stream_offset >> 8) & 0xFF) as usize] as u32;
            offset = offset.wrapping_add(sbox_roll);
            last_data = plain as u32;
            out.push(plain);
        }
        out
    }

    /// Encrypts a `DQ250 MQB` payload using the inverse progressive substitution cipher.
    pub fn dq250_encrypt(data: &[u8]) -> Vec<u8> {
        let mut inv_sbox = [0u8; 256];
        for (i, &val) in Self::MQB_DSG_SBOX.iter().enumerate() {
            inv_sbox[val as usize] = i as u8;
        }

        let mut out = Vec::with_capacity(data.len());
        let mut offset: u32 = 0;
        let mut rolling_stream_offset: u32 = 0;
        let mut last_data: u32 = 0;

        for &plain in data {
            let match_index = inv_sbox[plain as usize] as u32;
            let cipher_byte = (match_index.wrapping_sub(offset) & 0xFF) as u8;
            offset = offset.wrapping_add(plain as u32);
            offset = offset.wrapping_add(last_data);
            rolling_stream_offset = rolling_stream_offset.wrapping_add(0x167);
            let sbox_roll = Self::MQB_DSG_SBOX[((rolling_stream_offset >> 8) & 0xFF) as usize] as u32;
            offset = offset.wrapping_add(sbox_roll);
            last_data = plain as u32;
            out.push(cipher_byte);
        }
        out
    }

    /// Standard IEEE 802.3 reflected `CRC32`.
    pub fn crc32_ieee(data: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for &b in data {
            crc ^= b as u32;
            for _ in 0..8 {
                if (crc & 1) != 0 {
                    crc = (crc >> 1) ^ 0xEDB8_8320;
                } else {
                    crc >>= 1;
                }
            }
        }
        !crc
    }

    /// Validates and fixes the `DSG JAMCRC` (`0xFFFF_FFFF - crc32_ieee(data[..len-4])`) stored as LE `u32` at `len - 4`.
    pub fn validate_and_fix_dsg_jamcrc(data: &mut [u8]) -> Result<(u32, u32, bool), String> {
        if data.len() < 8 {
            return Err("DSG block too short for JAMCRC".to_string());
        }
        let cks_off = data.len() - 4;
        let stored = u32::from_le_bytes([
            data[cks_off],
            data[cks_off + 1],
            data[cks_off + 2],
            data[cks_off + 3],
        ]);
        let calc = 0xFFFF_FFFFu32.wrapping_sub(Self::crc32_ieee(&data[..cks_off]));
        let valid = stored == calc;
        if !valid {
            data[cks_off..cks_off + 4].copy_from_slice(&calc.to_le_bytes());
        }
        Ok((stored, calc, valid))
    }

    /// Validates and fixes the `DQ381` block checksum: Big-Endian IEEE `CRC32` stored at `0x44`
    /// over `[u32_be(0x38) - base_address ..= u32_be(0x3C) - base_address]`.
    pub fn validate_and_fix_dq381_block_crc32(
        data: &mut [u8],
        base_address: u32,
    ) -> Result<(u32, u32, bool), String> {
        if data.len() < 0x48 {
            return Err("DQ381 block too short for 0x38..0x48 header".to_string());
        }
        let start_abs = u32::from_be_bytes([data[0x38], data[0x39], data[0x3A], data[0x3B]]);
        let end_abs = u32::from_be_bytes([data[0x3C], data[0x3D], data[0x3E], data[0x3F]]);
        let stored = u32::from_be_bytes([data[0x44], data[0x45], data[0x46], data[0x47]]);

        if start_abs < base_address || end_abs < start_abs {
            return Err("DQ381 checksum range below base_address".to_string());
        }
        let s_rel = (start_abs - base_address) as usize;
        let e_rel = usize::try_from((end_abs as u64 - base_address as u64) + 1)
            .map_err(|_| "DQ381 checksum range overflows platform address space".to_string())?;
        if e_rel > data.len() {
            return Err("DQ381 checksum range exceeds block length".to_string());
        }

        let calc = Self::crc32_ieee(&data[s_rel..e_rel]);
        let valid = stored == calc;
        if !valid {
            data[0x44..0x48].copy_from_slice(&calc.to_be_bytes());
        }
        Ok((stored, calc, valid))
    }
}


/// 4. Bosch ME7.x Complete 3-Layer Checksum Suite (`MainChecksum`, `MultipointChecksum`, & `RollingChecksum`).
pub struct BoschMe7ChecksumSuite;

impl BoschMe7ChecksumSuite {
    /// Layer 1 (`MainChecksum.cs`): Sums 16-bit LE words across `address_ranges` and stores `[sum_u32_le, (!sum)_u32_le]`.
    pub fn validate_and_fix_me7_main_checksum(
        rom: &mut [u8],
        address_ranges: &[(usize, usize)],
        checksum_offset: usize,
    ) -> Result<(u32, u32, bool), String> {
        if checksum_offset.checked_add(8).is_none_or(|e| e > rom.len()) {
            return Err("ME7 main checksum offset out of bounds".to_string());
        }
        let mut sum: u32 = 0;
        for &(start, end) in address_ranges {
            if end > rom.len() || start >= end || (end - start) % 2 != 0 {
                return Err("ME7 main checksum range invalid".to_string());
            }
            for chunk in rom[start..end].chunks_exact(2) {
                sum = sum.wrapping_add(u16::from_le_bytes([chunk[0], chunk[1]]) as u32);
            }
        }
        let inv_sum = !sum;
        let stored_sum = u32::from_le_bytes([
            rom[checksum_offset],
            rom[checksum_offset + 1],
            rom[checksum_offset + 2],
            rom[checksum_offset + 3],
        ]);
        let stored_inv = u32::from_le_bytes([
            rom[checksum_offset + 4],
            rom[checksum_offset + 5],
            rom[checksum_offset + 6],
            rom[checksum_offset + 7],
        ]);
        let valid = stored_sum == sum && stored_inv == inv_sum;
        if !valid {
            rom[checksum_offset..checksum_offset + 4].copy_from_slice(&sum.to_le_bytes());
            rom[checksum_offset + 4..checksum_offset + 8].copy_from_slice(&inv_sum.to_le_bytes());
        }
        Ok((stored_sum, sum, valid))
    }

    /// Layer 2 (`MultipointChecksum.cs`): Processes 16-byte descriptor entries `[start_u32_le, end_u32_le, cks_u32_le, inv_cks_u32_le]`.
    pub fn validate_and_fix_me7_multipoint_descriptors(
        rom: &mut [u8],
        base_address: u32,
        descriptor_table_offset: usize,
        num_descriptors: usize,
    ) -> Result<usize, String> {
        if num_descriptors
            .checked_mul(16)
            .and_then(|bytes| descriptor_table_offset.checked_add(bytes))
            .is_none_or(|end| end > rom.len())
        {
            return Err("ME7 multipoint descriptor table out of bounds".to_string());
        }
        let mut fixed_count = 0usize;
        for i in 0..num_descriptors {
            let desc_off = descriptor_table_offset + i * 16;
            let start_abs = u32::from_le_bytes([
                rom[desc_off],
                rom[desc_off + 1],
                rom[desc_off + 2],
                rom[desc_off + 3],
            ]);
            let end_abs = u32::from_le_bytes([
                rom[desc_off + 4],
                rom[desc_off + 5],
                rom[desc_off + 6],
                rom[desc_off + 7],
            ]);
            if start_abs == 0xFFFF_FFFF && end_abs == 0xFFFF_FFFF {
                break;
            }
            if start_abs < base_address || end_abs < start_abs {
                continue;
            }
            let s_rel = (start_abs - base_address) as usize;
            let e_rel = usize::try_from((end_abs as u64 - base_address as u64) + 1)
                .map_err(|_| "ME7 multipoint descriptor range overflows platform address space".to_string())?;
            if e_rel > rom.len() || (e_rel - s_rel) % 2 != 0 {
                continue;
            }
            let mut sum: u32 = 0;
            for chunk in rom[s_rel..e_rel].chunks_exact(2) {
                sum = sum.wrapping_add(u16::from_le_bytes([chunk[0], chunk[1]]) as u32);
            }
            let inv_sum = !sum;
            let stored_sum = u32::from_le_bytes([
                rom[desc_off + 8],
                rom[desc_off + 9],
                rom[desc_off + 10],
                rom[desc_off + 11],
            ]);
            let stored_inv = u32::from_le_bytes([
                rom[desc_off + 12],
                rom[desc_off + 13],
                rom[desc_off + 14],
                rom[desc_off + 15],
            ]);
            if stored_sum != sum || stored_inv != inv_sum {
                rom[desc_off + 8..desc_off + 12].copy_from_slice(&sum.to_le_bytes());
                rom[desc_off + 12..desc_off + 16].copy_from_slice(&inv_sum.to_le_bytes());
                fixed_count += 1;
            }
        }
        Ok(fixed_count)
    }

    /// Layer 3 (`RollingChecksum.cs`): Table-driven polynomial accumulator using a 256-entry 32-bit ROM seed table:
    /// `seed = u32_le(seed_table + ((current_byte ^ (cks & 0xFF)) << 2)); cks = (cks >> 8) ^ seed`, stored inverted (`!cks`).
    pub fn validate_and_fix_me7_rolling_checksum(
        rom: &mut [u8],
        seed_table_offset: usize,
        address_ranges: &[(usize, usize)],
        checksum_offset: usize,
    ) -> Result<(u32, u32, bool), String> {
        if seed_table_offset.checked_add(1024).is_none_or(|e| e > rom.len())
            || checksum_offset.checked_add(4).is_none_or(|e| e > rom.len())
        {
            return Err("ME7 rolling checksum seed table or target offset out of bounds".to_string());
        }
        let mut cks: u32 = 0;
        let mut init_bytes_seen: usize = 0;

        for &(start, end) in address_ranges {
            if end > rom.len() || start >= end {
                return Err("ME7 rolling checksum range out of bounds".to_string());
            }
            for &b in &rom[start..end] {
                if init_bytes_seen < 4 {
                    // First 4 bytes across the sequence of ranges initialize `cks` in Little-Endian
                    cks |= (b as u32) << (init_bytes_seen * 8);
                    init_bytes_seen += 1;
                    continue;
                }
                let table_idx = ((b ^ ((cks & 0xFF) as u8)) as usize) << 2;
                let s_off = seed_table_offset + table_idx;
                let seed = u32::from_le_bytes([rom[s_off], rom[s_off + 1], rom[s_off + 2], rom[s_off + 3]]);
                cks = (cks >> 8) ^ seed;
            }
        }

        let calc_stored = !cks;
        let existing_stored = u32::from_le_bytes([
            rom[checksum_offset],
            rom[checksum_offset + 1],
            rom[checksum_offset + 2],
            rom[checksum_offset + 3],
        ]);
        let valid = existing_stored == calc_stored;
        if !valid {
            rom[checksum_offset..checksum_offset + 4].copy_from_slice(&calc_stored.to_le_bytes());
        }
        Ok((existing_stored, calc_stored, valid))
    }
}

/// 10. Audi / VAG Component Protection (`J533` Gateway `AES-128-ECB` & `4G0820043` Climatronic `IKA` Stream).
pub struct VagComponentProtectionCipher;

impl VagComponentProtectionCipher {
    /// Standard FIPS-197 AES Forward S-Box (verified at `0x125B0` in `4G0820043` & `0x0002BC9C` in `J533`).
    pub const AES_SBOX: [u8; 256] = [
        0x63, 0x7C, 0x77, 0x7B, 0xF2, 0x6B, 0x6F, 0xC5, 0x30, 0x01, 0x67, 0x2B, 0xFE, 0xD7, 0xAB, 0x76,
        0xCA, 0x82, 0xC9, 0x7D, 0xFA, 0x59, 0x47, 0xF0, 0xAD, 0xD4, 0xA2, 0xAF, 0x9C, 0xA4, 0x72, 0xC0,
        0xB7, 0xFD, 0x93, 0x26, 0x36, 0x3F, 0xF7, 0xCC, 0x34, 0xA5, 0xE5, 0xF1, 0x71, 0xD8, 0x31, 0x15,
        0x04, 0xC7, 0x23, 0xC3, 0x18, 0x96, 0x05, 0x9A, 0x07, 0x12, 0x80, 0xE2, 0xEB, 0x27, 0xB2, 0x75,
        0x09, 0x83, 0x2C, 0x1A, 0x1B, 0x6E, 0x5A, 0xA0, 0x52, 0x3B, 0xD6, 0xB3, 0x29, 0xE3, 0x2F, 0x84,
        0x53, 0xD1, 0x00, 0xED, 0x20, 0xFC, 0xB1, 0x5B, 0x6A, 0xCB, 0xBE, 0x39, 0x4A, 0x4C, 0x58, 0xCF,
        0xD0, 0xEF, 0xAA, 0xFB, 0x43, 0x4D, 0x33, 0x85, 0x45, 0xF9, 0x02, 0x7F, 0x50, 0x3C, 0x9F, 0xA8,
        0x51, 0xA3, 0x40, 0x8F, 0x92, 0x9D, 0x38, 0xF5, 0xBC, 0xB6, 0xDA, 0x21, 0x10, 0xFF, 0xF3, 0xD2,
        0xCD, 0x0C, 0x13, 0xEC, 0x5F, 0x97, 0x44, 0x17, 0xC4, 0xA7, 0x7E, 0x3D, 0x64, 0x5D, 0x19, 0x73,
        0x60, 0x81, 0x4F, 0xDC, 0x22, 0x2A, 0x90, 0x88, 0x46, 0xEE, 0xB8, 0x14, 0xDE, 0x5E, 0x0B, 0xDB,
        0xE0, 0x32, 0x3A, 0x0A, 0x49, 0x06, 0x24, 0x5C, 0xC2, 0xD3, 0xAC, 0x62, 0x91, 0x95, 0xE4, 0x79,
        0xE7, 0xC8, 0x37, 0x6D, 0x8D, 0xD5, 0x4E, 0xA9, 0x6C, 0x56, 0xF4, 0xEA, 0x65, 0x7A, 0xAE, 0x08,
        0xBA, 0x78, 0x25, 0x2E, 0x1C, 0xA6, 0xB4, 0xC6, 0xE8, 0xDD, 0x74, 0x1F, 0x4B, 0xBD, 0x8B, 0x8A,
        0x70, 0x3E, 0xB5, 0x66, 0x48, 0x03, 0xF6, 0x0E, 0x61, 0x35, 0x57, 0xB9, 0x86, 0xC1, 0x1D, 0x9E,
        0xE1, 0xF8, 0x98, 0x11, 0x69, 0xD9, 0x8E, 0x94, 0x9B, 0x1E, 0x87, 0xE9, 0xCE, 0x55, 0x28, 0xDF,
        0x8C, 0xA1, 0x89, 0x0D, 0xBF, 0xE6, 0x42, 0x68, 0x41, 0x99, 0x2D, 0x0F, 0xB0, 0x54, 0xBB, 0x16,
    ];

    pub const RCON: [u8; 10] = [0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80, 0x1B, 0x36];

    fn gmul(mut a: u8, mut b: u8) -> u8 {
        let mut r = 0u8;
        for _ in 0..8 {
            if (b & 1) != 0 {
                r ^= a;
            }
            let hi = a & 0x80;
            a <<= 1;
            if hi != 0 {
                a ^= 0x1B;
            }
            b >>= 1;
        }
        r
    }

    fn expand_key(key: &[u8; 16]) -> [[u8; 4]; 44] {
        let mut w = [[0u8; 4]; 44];
        for i in 0..4 {
            w[i] = [key[4 * i], key[4 * i + 1], key[4 * i + 2], key[4 * i + 3]];
        }
        for i in 4..44 {
            let mut t = w[i - 1];
            if i % 4 == 0 {
                t = [
                    Self::AES_SBOX[t[1] as usize] ^ Self::RCON[i / 4 - 1],
                    Self::AES_SBOX[t[2] as usize],
                    Self::AES_SBOX[t[3] as usize],
                    Self::AES_SBOX[t[0] as usize],
                ];
            }
            for j in 0..4 {
                w[i][j] = w[i - 4][j] ^ t[j];
            }
        }
        w
    }

    /// Pure Rust FIPS-197 `AES-128` single-block encryption (`gw_cp_cipher.py` / `hvac_ika_cipher.py`).
    pub fn aes128_encrypt_block(pt: &[u8; 16], key: &[u8; 16]) -> [u8; 16] {
        let w = Self::expand_key(key);
        let mut s = *pt;

        for c in 0..4 {
            for r in 0..4 {
                s[c * 4 + r] ^= w[c][r];
            }
        }

        for rnd in 1..10 {
            for b in &mut s {
                *b = Self::AES_SBOX[*b as usize];
            }
            // ShiftRows
            let t = s[1]; s[1] = s[5]; s[5] = s[9]; s[9] = s[13]; s[13] = t;
            let t1 = s[2]; let t2 = s[6]; s[2] = s[10]; s[6] = s[14]; s[10] = t1; s[14] = t2;
            let t3 = s[3]; s[3] = s[15]; s[15] = s[11]; s[11] = s[7]; s[7] = t3;

            // MixColumns
            for c in 0..4 {
                let i = c * 4;
                let (a0, a1, a2, a3) = (s[i], s[i + 1], s[i + 2], s[i + 3]);
                s[i] = Self::gmul(a0, 2) ^ Self::gmul(a1, 3) ^ a2 ^ a3;
                s[i + 1] = a0 ^ Self::gmul(a1, 2) ^ Self::gmul(a2, 3) ^ a3;
                s[i + 2] = a0 ^ a1 ^ Self::gmul(a2, 2) ^ Self::gmul(a3, 3);
                s[i + 3] = Self::gmul(a0, 3) ^ a1 ^ a2 ^ Self::gmul(a3, 2);
            }

            for c in 0..4 {
                for r in 0..4 {
                    s[c * 4 + r] ^= w[rnd * 4 + c][r];
                }
            }
        }

        for b in &mut s {
            *b = Self::AES_SBOX[*b as usize];
        }
        let t = s[1]; s[1] = s[5]; s[5] = s[9]; s[9] = s[13]; s[13] = t;
        let t1 = s[2]; let t2 = s[6]; s[2] = s[10]; s[6] = s[14]; s[10] = t1; s[14] = t2;
        let t3 = s[3]; s[3] = s[15]; s[15] = s[11]; s[11] = s[7]; s[7] = t3;

        for c in 0..4 {
            for r in 0..4 {
                s[c * 4 + r] ^= w[40 + c][r];
            }
        }
        s
    }

    /// Pure Rust FIPS-197 `AES-128` single-block decryption (`J533` LEAR D4 Gateway CP cipher).
    pub fn aes128_decrypt_block(ct: &[u8; 16], key: &[u8; 16]) -> [u8; 16] {
        let mut inv_sbox = [0u8; 256];
        for (i, &v) in Self::AES_SBOX.iter().enumerate() {
            inv_sbox[v as usize] = i as u8;
        }
        let w = Self::expand_key(key);
        let mut s = *ct;

        for c in 0..4 {
            for r in 0..4 {
                s[c * 4 + r] ^= w[40 + c][r];
            }
        }
        let t = s[1]; s[1] = s[13]; s[13] = s[9]; s[9] = s[5]; s[5] = t;
        let t1 = s[2]; let t2 = s[6]; s[2] = s[10]; s[6] = s[14]; s[10] = t1; s[14] = t2;
        let t3 = s[3]; s[3] = s[7]; s[7] = s[11]; s[11] = s[15]; s[15] = t3;
        for b in &mut s {
            *b = inv_sbox[*b as usize];
        }

        for rnd in (1..10).rev() {
            for c in 0..4 {
                for r in 0..4 {
                    s[c * 4 + r] ^= w[rnd * 4 + c][r];
                }
            }
            for c in 0..4 {
                let i = c * 4;
                let (a0, a1, a2, a3) = (s[i], s[i + 1], s[i + 2], s[i + 3]);
                s[i] = Self::gmul(a0, 14) ^ Self::gmul(a1, 11) ^ Self::gmul(a2, 13) ^ Self::gmul(a3, 9);
                s[i + 1] = Self::gmul(a0, 9) ^ Self::gmul(a1, 14) ^ Self::gmul(a2, 11) ^ Self::gmul(a3, 13);
                s[i + 2] = Self::gmul(a0, 13) ^ Self::gmul(a1, 9) ^ Self::gmul(a2, 14) ^ Self::gmul(a3, 11);
                s[i + 3] = Self::gmul(a0, 11) ^ Self::gmul(a1, 13) ^ Self::gmul(a2, 9) ^ Self::gmul(a3, 14);
            }
            let t = s[1]; s[1] = s[13]; s[13] = s[9]; s[9] = s[5]; s[5] = t;
            let t1 = s[2]; let t2 = s[6]; s[2] = s[10]; s[6] = s[14]; s[10] = t1; s[14] = t2;
            let t3 = s[3]; s[3] = s[7]; s[7] = s[11]; s[11] = s[15]; s[15] = t3;
            for b in &mut s {
                *b = inv_sbox[*b as usize];
            }
        }

        for c in 0..4 {
            for r in 0..4 {
                s[c * 4 + r] ^= w[c][r];
            }
        }
        s
    }

    /// Runs the 3-round Climatronic `4G0820043` forward stream diffusion (`FUN_0006e6e0`) and inverse round (`FUN_0006e75e`).
    pub fn climatronic_ika_stream_roundtrip_check(work_init: [u8; 16], s_selectors: [u8; 4], stream3: [u8; 3]) -> ([u8; 16], [u8; 16]) {
        let mut work = work_init;
        let (s31, s32, s33, s34) = (s_selectors[0], s_selectors[1], s_selectors[2], s_selectors[3]);

        // 3 forward rounds (FUN_0006e6e0)
        for &stream_b in &stream3 {
            let b1 = Self::AES_SBOX[s33 as usize];
            let b3 = Self::AES_SBOX[s32 as usize];
            let b4 = Self::AES_SBOX[s31 as usize];
            let b5 = Self::AES_SBOX[s34 as usize];
            work[0] ^= stream_b ^ b1;
            work[1] ^= b3;
            work[3] ^= b5;
            work[2] ^= b4;
            for i in 0..12 {
                work[i + 4] ^= work[i];
            }
        }
        let after_fwd = work;

        // 3 reverse rounds (FUN_0006e75e)
        for &stream_b in stream3.iter().rev() {
            for i in (0..12).rev() {
                work[i + 4] ^= work[i];
            }
            let b1 = Self::AES_SBOX[s33 as usize];
            let b2 = Self::AES_SBOX[s32 as usize];
            let b3 = Self::AES_SBOX[s31 as usize];
            let b4 = Self::AES_SBOX[s34 as usize];
            work[0] ^= stream_b ^ b1;
            work[2] ^= b3;
            work[1] ^= b2;
            work[3] ^= b4;
        }
        (after_fwd, work)
    }
}


// ============================================================================
// 11. VBF FEISTEL CIPHER (FORMAT 0x01), FORD PHF BINARY CONTAINER PARSER,
//     & 31-GROUP MCU ADDRESS MAPPER
// ============================================================================
