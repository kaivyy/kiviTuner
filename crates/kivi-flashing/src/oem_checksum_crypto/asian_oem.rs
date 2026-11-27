/// 3. Hyundai / Kia Siemens `SIMK41 / SIMK43 / SIMK2K` Multi-Zone Chained `CRC16-ARC` & `SMARTRA` VIN-to-PIN.
pub struct SimkAndSmartraEngine;

impl SimkAndSmartraEngine {
    /// Reflected `CRC16-ARC` (`0xA001` polynomial) with arbitrary initial 16-bit seed.
    pub fn crc16_arc_reflected(data: &[u8], initial_seed: u16) -> u16 {
        let mut crc = initial_seed;
        for &b in data {
            crc ^= b as u16;
            for _ in 0..8 {
                if (crc & 1) != 0 {
                    crc = (crc >> 1) ^ 0xA001;
                } else {
                    crc >>= 1;
                }
            }
        }
        crc
    }

    /// Computes and fixes a chained multi-zone `SIMK` checksum given `(cks_address, init_address, zones)`.
    /// Final CRC16 is byte-swapped (`to_be_bytes` / swapped LE) before storing at `cks_address`.
    pub fn validate_and_fix_simk_zones(
        rom: &mut [u8],
        cks_address: usize,
        init_address: usize,
        zones: &[(usize, usize)],
    ) -> Result<(u16, u16, bool), String> {
        if cks_address.checked_add(2).is_none_or(|e| e > rom.len())
            || init_address.checked_add(2).is_none_or(|e| e > rom.len())
        {
            return Err("SIMK checksum or init address out of bounds".to_string());
        }
        let stored_be = u16::from_be_bytes([rom[cks_address], rom[cks_address + 1]]);
        let mut crc = u16::from_le_bytes([rom[init_address], rom[init_address + 1]]);

        for &(z_start, z_stop) in zones {
            if z_stop > rom.len() || z_start > z_stop {
                return Err("SIMK zone out of bounds".to_string());
            }
            crc = Self::crc16_arc_reflected(&rom[z_start..z_stop], crc);
        }

        // Byte-swap the final CRC (`((crc << 8) & 0xFF00) | ((crc >> 8) & 0x00FF)`)
        // and write `[high_byte, low_byte]` at `cks_address`.
        let valid = stored_be == crc;
        if !valid {
            rom[cks_address] = (crc & 0xFF) as u8;
            rom[cks_address + 1] = (crc >> 8) as u8;
            // Re-sync in BE order so stored_be matches crc
            let bytes = crc.to_be_bytes();
            rom[cks_address..cks_address + 2].copy_from_slice(&bytes);
        }
        Ok((stored_be, crc, valid))
    }

    /// Calculates the Hyundai/Kia `SMARTRA` 6-digit immobilizer PIN from the last 6 digits of the VIN.
    ///
    /// Algorithm: 40 iterations (`0..=0x27`) of 32-bit LFSR shift with polynomial `0x7798_2990`,
    /// followed by `output % 1_000_000`.
    pub fn calculate_smartra_pin(last_6_vin_digits: u32) -> u32 {
        let mut output = last_6_vin_digits;
        let mut index = 0u32;
        loop {
            let msb_set = (output >> 31) != 0;
            output <<= 1;
            if msb_set {
                output ^= 0x7798_2990;
            }
            if index >= 0x27 {
                break;
            }
            index += 1;
        }
        output % 1_000_000
    }
}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HondaEcuHardwareType {
    KeihinOther,
    KeihinH8C,
    KeihinM32R,
    ShindengenV850,
    ShindengenRh850,
    ShindengenRh850B,
}

pub struct HondaKeihinShindengenUnifiedCore;

impl HondaKeihinShindengenUnifiedCore {
    pub const SH_BIN_SIG: [u8; 4] = [0x5A, 0x5A, 0x5A, 0x5A];
    pub const KH_H8C_SIG: [u8; 4] = [0xFF, 0xFF, 0xAA, 0xAA];
    pub const KH_M32R_SIG: [u8; 7] = [0xFF, 0xFF, 0xFF, 0xAA, 0xAA, 0xAA, 0xAA];
    pub const KH_RH850_SIG: [u8; 7] = [0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00];
    pub const KH_MPC560_SIG: [u8; 7] = [0xFF; 7];
    pub const SH_W_KEYSIG: [u8; 7] = [0x11, 0x12, 0x13, 0x14, 0x15, 0x15, 0x17];
    pub const SH_R_KEYSIG: [u8; 6] = [0x91, 0x91, 0x0D, 0x00, 0x9E, 0x8D];
    pub const SH_V850_TAG: &'static [u8] = b"SV850";
    pub const SH_RH850_TAG: &'static [u8] = b"SH850";
    pub const SH_START0: [u8; 5] = [0xA5, 0xA5, 0xA5, 0xA5, 0x00];

    fn find_subslice(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
        if needle.is_empty() || from + needle.len() > haystack.len() {
            return None;
        }
        haystack[from..]
            .windows(needle.len())
            .position(|w| w == needle)
            .map(|p| from + p)
    }

    /// Detects the exact Honda ECU hardware type and default start offset.
    pub fn detect_ecu_type(bin: &[u8]) -> (HondaEcuHardwareType, Option<usize>) {
        let n = bin.len();
        if bin.ends_with(&Self::SH_BIN_SIG) {
            let wk = Self::find_subslice(bin, &Self::SH_W_KEYSIG, 0).is_some();
            let iv = Self::find_subslice(bin, Self::SH_V850_TAG, 0);
            let ir = Self::find_subslice(bin, Self::SH_RH850_TAG, 0);
            if let Some(iv_pos) = iv {
                if ir.map_or(true, |ir_pos| iv_pos < ir_pos) {
                    return (HondaEcuHardwareType::ShindengenV850, Some(0x10000));
                }
            }
            if ir.is_some() {
                if n > 40965 && &bin[40960..40965] == &Self::SH_START0 {
                    return (HondaEcuHardwareType::ShindengenRh850B, Some(0xA000));
                }
                return (HondaEcuHardwareType::ShindengenRh850, Some(0x10000));
            }
            if wk {
                return (HondaEcuHardwareType::ShindengenV850, Some(0x10000));
            }
        }
        if (n == 0x8000 || n == 0xE000) && bin.ends_with(&Self::KH_H8C_SIG) {
            return (HondaEcuHardwareType::KeihinH8C, None);
        }
        if n == 0x40000 && bin.ends_with(&Self::KH_M32R_SIG) {
            return (HondaEcuHardwareType::KeihinM32R, None);
        }
        (HondaEcuHardwareType::KeihinOther, None)
    }

    /// Resolves `(start_offset, checksum_offset)` from ROM size and tail signature.
    pub fn size_fallback_offsets(bin: &[u8]) -> Option<(usize, usize)> {
        let n = bin.len();
        match n {
            0xC000 => Some((0x4000, 0x7600)),
            0xE000 => Some((0x0000, 0xDFEF)),
            0x10000 | 0x18000 => Some((0x8000, 0x0000)),
            0x20000 => Some((0x0E000, 0xB000)),
            0x40000 if !bin.ends_with(&Self::SH_BIN_SIG) && bin.ends_with(&Self::KH_RH850_SIG) => {
                Some((0x0000, 0x3FFF8))
            }
            0x80000 if !bin.ends_with(&Self::SH_BIN_SIG) && bin.ends_with(&Self::KH_MPC560_SIG) => {
                Some((0x0000, 0x7FFF8))
            }
            0x100000 if !bin.ends_with(&Self::SH_BIN_SIG) => {
                if bin.ends_with(&Self::KH_M32R_SIG) {
                    Some((0x0000, 0xFFFF8))
                } else {
                    Some((0x0000, 0x7FFF8))
                }
            }
            _ => None,
        }
    }

    /// Keihin 8-bit two's-complement ROM checksum at `ck_offset`.
    pub fn keihin_expected_checksum(bin: &[u8], ck_offset: usize) -> u8 {
        let sum: u8 = bin
            .iter()
            .enumerate()
            .filter(|&(i, _)| i != ck_offset)
            .fold(0u8, |acc, (_, &b)| acc.wrapping_add(b));
        0u8.wrapping_sub(sum)
    }

    /// Honda K-Line carry checksum: `(255 - ((s - 1) >> 8)) & 255`.
    pub fn csum_carry(data: &[u8]) -> u8 {
        let s: i32 = data.iter().map(|&b| b as i32).sum();
        ((255 - ((s - 1) >> 8)) & 0xFF) as u8
    }

    /// Shindengen signature-guided 32-bit checksum auto-locator & fixer (`shindengenAutoChecksum`).
    /// Returns the discovered `checksum_offset` (`num - 4`) after updating the 4-byte LE checksum in-place.
    pub fn shindengen_auto_checksum_fix(bin: &mut [u8]) -> Option<usize> {
        let mut search_from = 0usize;
        while let Some(i) = Self::find_subslice(bin, &Self::SH_W_KEYSIG, search_from) {
            if i + Self::SH_W_KEYSIG.len() >= bin.len().saturating_sub(6) {
                break;
            }
            for j in (i + Self::SH_W_KEYSIG.len())..(bin.len() - 6) {
                if bin[j] == 0xD0 && bin[j + 1] == 0x07 {
                    let num = if j % 4 == 2 { j - 6 } else { j };
                    if num < 4 || num > bin.len() {
                        continue;
                    }
                    let sum32_slice = |slice: &[u8]| -> u32 {
                        slice
                            .chunks_exact(4)
                            .fold(0u32, |acc, c| acc.wrapping_add(u32::from_le_bytes([c[0], c[1], c[2], c[3]])))
                    };
                    let tot = sum32_slice(&bin[0..num - 4]).wrapping_add(sum32_slice(&bin[num..]));
                    let w = 0u32.wrapping_sub(tot);
                    let ck_off = num - 4;
                    bin[ck_off..ck_off + 4].copy_from_slice(&w.to_le_bytes());
                    return Some(ck_off);
                }
            }
            search_from = i + 1;
        }
        None
    }

    fn key_from_params(bin: &[u8], param_offset: usize, seed: [u8; 2]) -> [u8; 2] {
        let a = u16::from_le_bytes([bin[param_offset], bin[param_offset + 1]]) as u32;
        let b = u16::from_le_bytes([bin[param_offset + 2], bin[param_offset + 3]]) as u32;
        let c = u16::from_le_bytes([bin[param_offset + 4], bin[param_offset + 5]]) as u32;
        let s = u16::from_be_bytes(seed) as u32;
        let t = (b.wrapping_mul(s.wrapping_add(a))) & 0xFFFF;
        let k = (t.wrapping_add(if c > 0 { s % c } else { s })) & 0xFFFF;
        [(k >> 8) as u8, (k & 0xFF) as u8]
    }

    /// Dynamically extracts the 6-byte parameter block from a Shindengen ROM and calculates the 16-bit Write Key.
    pub fn shindengen_write_key(bin: &[u8], seed: [u8; 2]) -> Option<[u8; 2]> {
        let mut search_from = 0usize;
        while let Some(i) = Self::find_subslice(bin, &Self::SH_W_KEYSIG, search_from) {
            for j in (i + Self::SH_W_KEYSIG.len())..bin.len().saturating_sub(10) {
                if bin[j + 8] == 0xEE && (bin[j + 9] == 0x00 || bin[j + 9] == 0xFF) {
                    return Some(Self::key_from_params(bin, j, seed));
                }
            }
            search_from = i + 1;
        }
        None
    }

    /// Dynamically extracts the 6-byte recovery parameter block (`SH_R_KEYSIG`) and calculates the 16-bit Recovery Key.
    pub fn shindengen_recovery_key(bin: &[u8], seed: [u8; 2]) -> Option<[u8; 2]> {
        let min_len = 6 + Self::SH_R_KEYSIG.len();
        if bin.len() < min_len {
            return None;
        }
        for i in 0..=bin.len() - min_len {
            if &bin[i + 6..i + 6 + Self::SH_R_KEYSIG.len()] == &Self::SH_R_KEYSIG {
                return Some(Self::key_from_params(bin, i, seed));
            }
        }
        None
    }
}


/// 7. Subaru Hitachi (`UJ20 / UJ30` M32R) Unbrick Framing & `SSM2` Indirect VIN Resolver.
pub struct SubaruHitachiUnbrickAndSsm2;

impl SubaruHitachiUnbrickAndSsm2 {
    /// Builds the Hitachi `UJ20/UJ30` (`WA12212970`) Flash Erase command (`0xAF 0x31`).
    pub fn build_uj20_erase_frame() -> [u8; 7] {
        let mut frame = [0x80, 0x10, 0xF0, 0x02, 0xAF, 0x31, 0x00];
        let cks = frame[0..6].iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
        frame[6] = cks;
        frame
    }

    /// Builds a Hitachi `UJ20/UJ30` 128-byte flash write block frame (`0x61` intermediate, `0x69` final block).
    pub fn build_uj20_write_128b_frame(address_24bit: u32, block_128b: &[u8; 128], is_final_block: bool) -> Vec<u8> {
        let mut frame = Vec::with_capacity(138);
        frame.push(0x80);
        frame.push(0x10);
        frame.push(0xF0);
        frame.push(0x85); // length = 5 + 128 = 133 (0x85)
        frame.push(0xAF);
        frame.push(if is_final_block { 0x69 } else { 0x61 });
        frame.push(((address_24bit >> 16) & 0xFF) as u8);
        frame.push(((address_24bit >> 8) & 0xFF) as u8);
        frame.push((address_24bit & 0xFF) as u8);
        frame.extend_from_slice(block_128b);
        let cks = frame.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
        frame.push(cks);
        frame
    }

    /// Parses the 3-byte indirect VIN pointer returned from `SSM2` addresses `[0xDA, 0xDB, 0xDC]`
    /// and validates the 17-character ISO 3779 VIN.
    pub fn resolve_and_validate_ssm2_vin(pointer_bytes: [u8; 3], vin_raw_17b: &[u8]) -> Result<(u32, String), &'static str> {
        let vin_addr = ((pointer_bytes[0] as u32) << 16)
            | ((pointer_bytes[1] as u32) << 8)
            | (pointer_bytes[2] as u32);
        if vin_raw_17b.len() != 17 {
            return Err("SSM2 VIN buffer must be exactly 17 bytes");
        }
        let mut vin = String::with_capacity(17);
        for (idx, &b) in vin_raw_17b.iter().enumerate() {
            let c = b as char;
            // ISO 3779 excludes 'I', 'O', 'Q'; last 6 characters must be digits 0..9
            let valid_char = if idx >= 11 {
                c.is_ascii_digit()
            } else {
                c.is_ascii_digit() || (c.is_ascii_uppercase() && c != 'I' && c != 'O' && c != 'Q')
            };
            if !valid_char {
                return Err("Invalid ISO 3779 character in SSM2 VIN");
            }
            vin.push(c);
        }
        Ok((vin_addr, vin))
    }
}


// ============================================================================

pub struct DensoSh705xBootAndEepromEngine;

impl DensoSh705xBootAndEepromEngine {
    /// Reflected CRC-32 16-entry nibble table for Denso SH705x RAM kernels.
    pub const DENSO_CRC32_NIBBLE_TABLE: [u32; 16] = [
        0x0000_0000, 0x1DB7_1064, 0x3B6E_20C8, 0x26D9_30AC,
        0x76DC_4190, 0x6B6B_51F4, 0x4DB2_6158, 0x5005_713C,
        0xEDB8_8320, 0xF00F_9344, 0xD6D6_A3E8, 0xCB61_B38C,
        0x9B64_C2B0, 0x86D3_D2D4, 0xA00A_E278, 0xBDBD_F21C,
    ];

    /// Computes the 32-bit Denso RAM kernel unlock key from the `[0xE1, s0..s3]` seed
    /// and chip-specific XOR mask (`0x6703` / `0x6705` / `0x4110` / `0x6714` / `0x4121`)
    /// using 8 inverse CRC-32 nibble rounds followed by `!(state.swap_bytes())`.
    pub fn calculate_denso_ram_kernel_seed_key(seed: u32, chip_xor_key: u32) -> u32 {
        let mut state = seed ^ chip_xor_key;
        for _ in 0..8 {
            let top_nibble = state >> 28;
            let mut matched_idx = 0usize;
            for (idx, &entry) in Self::DENSO_CRC32_NIBBLE_TABLE.iter().enumerate() {
                if (entry >> 28) == top_nibble {
                    matched_idx = idx;
                    break;
                }
            }
            state = ((state ^ Self::DENSO_CRC32_NIBBLE_TABLE[matched_idx]) << 4) | (matched_idx as u32);
        }
        !(state.swap_bytes())
    }

    /// Forward verification of the Denso RAM kernel 8-round nibble CRC-32 key.
    pub fn verify_denso_ram_kernel_seed_key(seed: u32, chip_xor_key: u32, key: u32) -> bool {
        let mut state = (!key).swap_bytes();
        for _ in 0..8 {
            let idx = (state & 0x0F) as usize;
            state = (state >> 4) ^ Self::DENSO_CRC32_NIBBLE_TABLE[idx];
        }
        (state ^ chip_xor_key) == seed
    }

    /// Builds the `$BOOT$` Direct Boot CAN frame sequence (`0xFF 0x86` wake-up, `0x7A 0x9C` RAM load address
    /// `0xFFFF8000`, `0x7A 0xAE`/`0x7A 0xAC` 6-byte kernel chunks, `0x7A 0xB4` vector `0xFFFF9A80`, `0x7A 0xA0` exec).
    pub fn build_denso_sh705x_boot_frames(kernel: &[u8]) -> Vec<[u8; 8]> {
        let mut frames = Vec::with_capacity((kernel.len() / 6) + 5);
        frames.push([0xFF, 0x86, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
        frames.push([0x7A, 0x9C, 0xFF, 0xFF, 0x80, 0x00, 0x00, 0x00]);
        let mut off = 0usize;
        while off < kernel.len() {
            let rem = kernel.len() - off;
            let mut f = [0u8; 8];
            f[0] = 0x7A;
            f[1] = if rem > 6 { 0xAE } else { 0xAC };
            let take = rem.min(6);
            f[2..2 + take].copy_from_slice(&kernel[off..off + take]);
            frames.push(f);
            off += take;
        }
        frames.push([0x7A, 0xB4, 0xFF, 0xFF, 0x9A, 0x80, 0x00, 0x00]);
        frames.push([0x7A, 0xA0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
        frames
    }

    /// Computes the Denso SH705x sector CRC-32 (`0xB2`/`0xF2`) with initial state `0xFFFFFFFF`,
    /// no final XOR, and Middle-Endian byte ordering `[crc_b2, crc_b3, crc_b0, crc_b1]`.
    pub fn compute_denso_sector_crc32_middle_endian(sector: &[u8]) -> [u8; 4] {
        let mut crc = 0xFFFF_FFFFu32;
        for &b in sector {
            let idx1 = ((crc ^ (b as u32)) & 0x0F) as usize;
            crc = (crc >> 4) ^ Self::DENSO_CRC32_NIBBLE_TABLE[idx1];
            let idx2 = ((crc ^ ((b >> 4) as u32)) & 0x0F) as usize;
            crc = (crc >> 4) ^ Self::DENSO_CRC32_NIBBLE_TABLE[idx2];
        }
        [
            ((crc >> 16) & 0xFF) as u8,
            ((crc >> 24) & 0xFF) as u8,
            (crc & 0xFF) as u8,
            ((crc >> 8) & 0xFF) as u8,
        ]
    }

    /// Builds a 516-byte Denso SH705x flash page write packet:
    /// `[0x34, (addr >> 16) as u8, (addr >> 8) as u8, two_comp_csum, data[0..512]]`.
    pub fn build_denso_512b_page_write_packet(addr: u32, page_512: &[u8; 512]) -> Vec<u8> {
        let mut pkt = Vec::with_capacity(516);
        let b1 = ((addr >> 16) & 0xFF) as u8;
        let b2 = ((addr >> 8) & 0xFF) as u8;
        let mut sum = 0x34u8.wrapping_add(b1).wrapping_add(b2);
        for &b in page_512.iter() {
            sum = sum.wrapping_add(b);
        }
        let csum = (!sum).wrapping_add(1);
        pkt.push(0x34);
        pkt.push(b1);
        pkt.push(b2);
        pkt.push(csum);
        pkt.extend_from_slice(page_512);
        pkt
    }

    /// Builds a word-addressed 93C56/93C66/93C86 serial EEPROM read command `[0xA2, word_hi, word_lo]`.
    pub fn build_denso_eeprom_read_frame(word_addr: u16) -> [u8; 3] {
        [0xA2, (word_addr >> 8) as u8, (word_addr & 0xFF) as u8]
    }

    /// Builds a word-addressed 93C56/93C66/93C86 serial EEPROM write command `[0xAE, word_hi, word_lo, data_hi, data_lo]`.
    pub fn build_denso_eeprom_write_frame(word_addr: u16, word_val: u16) -> [u8; 5] {
        [
            0xAE,
            (word_addr >> 8) as u8,
            (word_addr & 0xFF) as u8,
            (word_val >> 8) as u8,
            (word_val & 0xFF) as u8,
        ]
    }

    /// Follows a SuperH `mov.l @(disp, PC), rN` (`0xD000..0xDFFF`) instruction to its 32-bit literal pool entry
    /// and rounds up to a 512 KB (`0x80000`) boundary (`(val + 0x7FFFF) & 0xFFF80000`) to detect SH7055/SH7058/SH7059 size.
    pub fn resolve_sh705x_movl_pc_literal_and_round(rom: &[u8], insn_pc: usize) -> Option<(u32, u32)> {
        if insn_pc.checked_add(2).is_none_or(|e| e > rom.len()) {
            return None;
        }
        let opcode = u16::from_be_bytes([rom[insn_pc], rom[insn_pc + 1]]);
        if (opcode & 0xF000) != 0xD000 {
            return None;
        }
        let disp = (opcode & 0x00FF) as usize;
        let pool_off = (insn_pc & !3)
            .checked_add(4)
            .and_then(|v| disp.checked_mul(4).and_then(|d| v.checked_add(d)))?;
        if pool_off.checked_add(4).is_none_or(|e| e > rom.len()) {
            return None;
        }
        let val = u32::from_be_bytes([
            rom[pool_off],
            rom[pool_off + 1],
            rom[pool_off + 2],
            rom[pool_off + 3],
        ]);
        let rounded_mcu_size = val.wrapping_add(0x0007_FFFF) & 0xFFF8_0000;
        Some((val, rounded_mcu_size))
    }
}

// ============================================================================
// 13. HONDA BIKE KEIHIN M32R/R8C K-LINE & ISUZU TRANSTRON
//     SH705x/SH725xx CHECKSUM & PROTOCOL ENGINE

// ============================================================================

pub struct HondaKeihinBikeKLineAndIsuzuTranstronEngine;

impl HondaKeihinBikeKLineAndIsuzuTranstronEngine {
    /// Builds the 9-byte Honda Bike Keihin K-Line memory read / probe request:
    /// `[0x82, 0x82, 0x00, 0x09, (addr >> 16) as u8, (addr & 0xFF) as u8, ((addr >> 8) & 0xFF) as u8, len, csum]`.
    pub fn build_honda_kline_read_frame(addr: u32, len: u8) -> [u8; 9] {
        let mut frame = [
            0x82,
            0x82,
            0x00,
            0x09,
            ((addr >> 16) & 0xFF) as u8,
            (addr & 0xFF) as u8,
            ((addr >> 8) & 0xFF) as u8,
            len,
            0x00,
        ];
        let sum8: u8 = frame[0..8].iter().fold(0u8, |a, &b| a.wrapping_add(b));
        frame[8] = (!sum8).wrapping_add(1);
        frame
    }

    /// Builds the 8-byte `"PGM"` bootloader entry frame `[0x7D, 0x08, 0x01, 0x02, 0x50, 0x47, 0x4D, 0x94]`.
    pub fn build_honda_pgm_boot_entry_frame() -> [u8; 8] {
        [0x7D, 0x08, 0x01, 0x02, 0x50, 0x47, 0x4D, 0x94]
    }

    /// Builds a Honda Bike Keihin `0x7E` flash write frame with 16-bit inner two's-complement checksum
    /// and 8-bit outer two's-complement checksum.
    pub fn build_honda_kline_write_frame(addr: u32, next_addr: u32, is_end_of_segment: bool, data: &[u8]) -> Vec<u8> {
        let total_len = data.len() + 11;
        let mut pkt = Vec::with_capacity(total_len);
        pkt.push(0x7E);
        pkt.push(total_len as u8);
        pkt.push(0x01);
        pkt.push(0x06);
        let start_p = (addr >> 4) as u16;
        pkt.push((start_p >> 8) as u8);
        pkt.push((start_p & 0xFF) as u8);
        pkt.extend_from_slice(data);
        if is_end_of_segment {
            pkt.push(0x00);
            pkt.push(0x00);
        } else {
            let next_p = (next_addr >> 4) as u16;
            pkt.push((next_p >> 8) as u8);
            pkt.push((next_p & 0xFF) as u8);
        }
        let inner_sum16: u16 = pkt[4..].iter().fold(0u16, |a, &b| a.wrapping_add(b as u16));
        let inner_csum = (!inner_sum16).wrapping_add(1);
        pkt.push((inner_csum >> 8) as u8);
        pkt.push((inner_csum & 0xFF) as u8);
        let outer_sum8: u8 = pkt.iter().fold(0u8, |a, &b| a.wrapping_add(b));
        pkt.push((!outer_sum8).wrapping_add(1));
        pkt
    }

    /// Verifies and corrects the Honda Bike Keihin M32R (`56/128/192/256/384/512 KB`) and R8C (`96 KB`)
    /// whole-ROM 8-bit checksum at `rom_len - 0x0C`.
    pub fn correct_honda_keihin_rom_checksum(rom: &mut [u8]) -> Result<bool, String> {
        if !matches!(
            rom.len(),
            0x0E000 | 0x18000 | 0x20000 | 0x30000 | 0x40000 | 0x60000 | 0x80000
        ) {
            return Err(format!("Unsupported Honda Bike Keihin ROM size: 0x{:X}", rom.len()));
        }
        let sum8: u8 = rom.iter().fold(0u8, |a, &b| a.wrapping_add(b));
        if sum8 == 0 {
            return Ok(false);
        }
        let csum_off = rom.len() - 0x0C;
        rom[csum_off] = rom[csum_off].wrapping_sub(sum8);
        Ok(true)
    }

    /// Verifies and corrects the Isuzu Transtron SH7055/SH7058/SH7059/SH72533/SH72544
    /// Big-Endian 32-bit word sum checksum across `512 KB`, `1 MB`, `1.5 MB`, `2 MB`, `2.5 MB`, and `3.75 MB` ROMs.
    pub fn correct_isuzu_transtron_rom_checksum(rom: &mut [u8]) -> Result<bool, String> {
        let (start, end, csum_off) = match rom.len() {
            0x08_0000 => (0x00_0000usize, 0x08_0000usize, 0x07_FEFCusize),
            0x10_0000 => {
                if rom[0x02_0000] == 0x2F {
                    (0x00_0000, 0x10_0000, 0x00_FFEC)
                } else {
                    (0x02_0000, 0x10_0000, 0x0F_FEFC)
                }
            }
            0x18_0000 => (0x02_0000, 0x18_0000, 0x17_FEFC),
            0x20_0000 => (0x04_0000, 0x20_0000, 0x1F_FEFC),
            0x28_0000 => {
                if rom[0x07_FFFC..0x08_0000] == [0xFF, 0xFF, 0xFF, 0xFF] {
                    (0x08_0000, 0x28_0000, 0x27_FEFC)
                } else {
                    (0x03_0000, 0x28_0000, 0x27_FEFC)
                }
            }
            0x3C_0000 => (0x03_0000, 0x3C_0000, 0x3B_FEFC),
            other => return Err(format!("Unsupported Isuzu Transtron ROM size: 0x{:X}", other)),
        };

        let mut sum32 = 0u32;
        let mut off = start;
        while off + 4 <= end {
            let w = u32::from_be_bytes([rom[off], rom[off + 1], rom[off + 2], rom[off + 3]]);
            sum32 = sum32.wrapping_add(w);
            off += 4;
        }
        if sum32 == 0 {
            return Ok(false);
        }
        if !csum_off.is_multiple_of(4) || csum_off.checked_add(4).is_none_or(|e| e > rom.len()) {
            return Err(format!("Unaligned or out-of-bounds Isuzu Transtron checksum offset 0x{csum_off:X}"));
        }
        let old_w = u32::from_be_bytes([
            rom[csum_off],
            rom[csum_off + 1],
            rom[csum_off + 2],
            rom[csum_off + 3],
        ]);
        let new_w = old_w.wrapping_sub(sum32);
        rom[csum_off..csum_off + 4].copy_from_slice(&new_w.to_be_bytes());
        Ok(true)
    }
}

// ============================================================================
// 14. PASSTHRU DIAGNOSTIC & CUSTOM-ALPHABET TRACE CODEC
