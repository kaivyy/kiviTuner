//! Ported from `ecu_engine_rust::flashing::nissan_hitachi`.
//!
//! Pure byte-level protocol logic: builders, parsers, encoders, state machines.
//! No I/O, no device access, no `unsafe`. Transport is the caller's concern.
//!
//! Verified by unit tests with hand-derived expectations. **Not** validated
//! against a physical ECU; no such claim is made.

/// Nissan Hitachi Seed-Key Cryptographic Engine (Keyalg 2: K-Line AT).
pub struct NissanHitachiSeedKey;

impl NissanHitachiSeedKey {
    /// 16-element constant key substitution matrix from Nissan ROM.
    pub const KEYTABLE: [u32; 16] = [
        0x14FA3579, 0x27CD3964, 0x1777FE32, 0x9931AF12,
        0x75DB3A49, 0x19294CAA, 0x0FF18CD76, 0x0788236D,
        0x5A6F7CBB, 0x7A992254, 0x0ADFD5414, 0x343CFBCB,
        0x0C2F51639, 0x6A6D5813, 0x3729FF68, 0x22A2C751,
    ];

    /// Compute Nissan K-Line AT 32-bit Key from 4-byte seed (genkey2).
    pub fn compute_key2(seed_bytes: [u8; 4]) -> [u8; 4] {
        let mut seed = u32::from_be_bytes(seed_bytes);

        // Bit transposition to compute loop iterations (ecx) and key index (ki)
        let mut ecx: u32 = ((seed & 1) << 6)
            | (((seed >> 9) & 1) << 4)
            | (((seed >> 1) & 1) << 3)
            | (((seed >> 11) & 1) << 2)
            | (((seed >> 2) & 1) << 1)
            | ((seed >> 5) & 1);
        ecx = ecx.wrapping_add(0x1F);

        let ki = (((seed & 1) << 3)
            | (((seed >> 1) & 1) << 2)
            | (((seed >> 2) & 1) << 1)
            | ((seed >> 9) & 1)) as usize;

        // XOR Permutation Loop
        for _ in 0..ecx {
            if (seed & 0x80000000) != 0 {
                seed = seed.wrapping_add(seed);
                seed ^= Self::KEYTABLE[ki & 0x0F];
            } else {
                seed = seed.wrapping_add(seed);
            }
        }

        seed.to_be_bytes()
    }
}

/// Smart Differential Sector Flashing Planner.
/// Only erases and writes sectors that actually changed between Original ROM and Tuned ROM,
/// cutting flash wear and reducing flash time by up to 90%.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DifferentialSectorPlan {
    pub sector_index: usize,
    pub start_address: u32,
    pub length: usize,
    pub is_modified: bool,
    pub modified_bytes_count: usize,
}

pub struct DifferentialFlashPlanner;

impl DifferentialFlashPlanner {
    /// Compare stock ROM vs tuned ROM across sector layout boundaries.
    pub fn create_plan(
        stock_rom: &[u8],
        tuned_rom: &[u8],
        sector_sizes: &[usize],
    ) -> Result<Vec<DifferentialSectorPlan>, &'static str> {
        if stock_rom.len() != tuned_rom.len() {
            return Err("Stock and Tuned ROM sizes must match for differential planning");
        }

        let mut plan = Vec::new();
        let mut current_offset = 0;

        for (idx, &sec_len) in sector_sizes.iter().enumerate() {
            if current_offset >= tuned_rom.len() {
                break;
            }

            let end_offset = current_offset
                .checked_add(sec_len)
                .ok_or("Differential planner sector size overflow")?
                .min(tuned_rom.len());
            let stock_slice = &stock_rom[current_offset..end_offset];
            let tuned_slice = &tuned_rom[current_offset..end_offset];

            let mut diff_count = 0;
            for i in 0..stock_slice.len() {
                if stock_slice[i] != tuned_slice[i] {
                    diff_count += 1;
                }
            }

            plan.push(DifferentialSectorPlan {
                sector_index: idx,
                start_address: current_offset as u32,
                length: end_offset - current_offset,
                is_modified: diff_count > 0,
                modified_bytes_count: diff_count,
            });

            current_offset = end_offset;
        }

        Ok(plan)
    }

    /// Calculate total bytes that need to be flashed under the differential plan.
    pub fn total_bytes_to_flash(plan: &[DifferentialSectorPlan]) -> usize {
        plan.iter()
            .filter(|p| p.is_modified)
            .map(|p| p.length)
            .sum()
    }
}

/// Renesas SH7054 / SH7055 / SH7058 / SH7059 `FTDAR` Built-in Flash Microcode & 24-bit Sign-Extended RAM Mapper
/// for Nissan, Subaru, and Suzuki SuperH ECUs (`npkern` `sh7055_35.c` / `sh7058.c` memory model).
pub struct RenesasSh705xFtdarProgrammer;

impl RenesasSh705xFtdarProgrammer {
    /// Register address of `FTDAR` (Flash Transfer Destination Address Register) on SH7055/SH7058.
    pub const FTDAR_REGISTER_ADDR: u32 = 0xFFFF_EC03;
    /// Default on-chip RAM download window for `FTDAR` built-in flash microcode (`0xFFFF8000`).
    pub const FTDAR_RAM_WINDOW_32: u32 = 0xFFFF_8000;

    /// Convert a 24-bit diagnostic address (`0xFF1000..0xFFBFFF`) to the 32-bit sign-extended
    /// SuperH CPU RAM address (`0xFFFF1000..0xFFFFBFFF`).
    pub fn sign_extend_ram_address_24_to_32(addr24: u32) -> Result<u32, &'static str> {
        let masked = addr24 & 0x00FF_FFFF;
        if (masked & 0x0080_0000) == 0 {
            return Err("Address does not have bit 23 set for SH705x sign-extended RAM space");
        }
        let full32 = 0xFF00_0000 | masked;
        if !(0xFFFF_0000..=0xFFFF_DFFF).contains(&full32) {
            return Err("Address falls outside valid SH705x on-chip RAM window (0xFFFF0000..0xFFFFDFFF)");
        }
        Ok(full32)
    }

    /// Compress a 32-bit SuperH CPU RAM address (`0xFFFF8000`) into a 3-byte KWP2000/UDS 24-bit address (`[0xFF, 0x80, 0x00]`).
    pub fn to_24bit_diagnostic_bytes(addr32: u32) -> Result<[u8; 3], &'static str> {
        if (addr32 & 0xFFFF_0000) != 0xFFFF_0000 {
            return Err("SH705x RAM address must reside in 0xFFFFxxxx segment");
        }
        Ok([
            ((addr32 >> 16) & 0xFF) as u8,
            ((addr32 >> 8) & 0xFF) as u8,
            (addr32 & 0xFF) as u8,
        ])
    }

    /// Build a 128-byte or 256-byte page write command packet for the `npkern` SH705x RAM kernel.
    pub fn build_ftdar_page_write_packet(
        flash_page_address: u32,
        page_data: &[u8],
    ) -> Result<Vec<u8>, &'static str> {
        if page_data.len() != 128 && page_data.len() != 256 {
            return Err("SH705x FTDAR flash microcode requires 128-byte or 256-byte aligned pages");
        }
        if (flash_page_address as usize) % page_data.len() != 0 {
            return Err("Flash target address is not aligned to page size");
        }

        let mut pkt = Vec::with_capacity(7 + page_data.len() + 1);
        pkt.push(0x74); // npkern page write SID
        pkt.extend_from_slice(&(page_data.len() as u16).to_be_bytes());
        pkt.extend_from_slice(&flash_page_address.to_be_bytes());
        pkt.extend_from_slice(page_data);

        // Append 8-bit additive checksum
        let sum: u8 = pkt.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
        pkt.push(sum);
        Ok(pkt)
    }
}

/// Mitsubishi Melco (`MH7201`/`MH7202`/`MH7203`/`MH8102`/`MH8104`/`MH8106`/`MH8115`/`MH8601`/`MH8611`)
/// Bootloader & Kernel Protocol.
pub struct MitsubishiMelcoBootProtocol;

impl MitsubishiMelcoBootProtocol {
    pub const BOOT_SYNC_REQUEST: u8 = 0x55;
    pub const BOOT_SYNC_RESPONSE: u8 = 0xAA;
    pub const DEFAULT_AUX_VPP_MV: u32 = 16_500; // 16.5V Aux Boot Pin programming voltage

    /// Verify bootloader UART/K-Line sync byte exchange (`0x55 -> 0xAA`) after applying 16.5V Aux Vpp.
    pub fn verify_boot_sync_response(rx_byte: u8) -> Result<(), &'static str> {
        if rx_byte == Self::BOOT_SYNC_RESPONSE {
            Ok(())
        } else {
            Err("MELCO_BOOT_SYNC_FAILED: Expected 0xAA bootloader acknowledgment after Aux Vpp pulse")
        }
    }

    /// Build Melco Kernel Frame (`[cmd, len_hi, len_lo, payload..., checksum_u8]`).
    pub fn build_melco_kernel_frame(cmd: u8, payload: &[u8]) -> Vec<u8> {
        let len = payload.len() as u16;
        let mut frame = Vec::with_capacity(4 + payload.len());
        frame.push(cmd);
        frame.extend_from_slice(&len.to_be_bytes());
        frame.extend_from_slice(payload);
        let cks = frame.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
        frame.push(!cks);
        frame
    }

    /// Compute and patch Mitsubishi MH810x / MH8115 32-bit Big-Endian Calibration Checksum (`0x5AA5_A55A` magic marker).
    pub fn update_mh81xx_rom_checksum(
        rom: &mut [u8],
        checksum_offset: usize,
    ) -> Result<u32, &'static str> {
        if !rom.len().is_multiple_of(4)
            || !checksum_offset.is_multiple_of(4)
            || checksum_offset
                .checked_add(4)
                .is_none_or(|end| end > rom.len())
        {
            return Err("Invalid MH81xx ROM size or checksum offset");
        }

        let mut sum: u32 = 0;
        let mut idx = 0;
        while idx < rom.len() {
            if idx != checksum_offset {
                let word = u32::from_be_bytes([rom[idx], rom[idx + 1], rom[idx + 2], rom[idx + 3]]);
                sum = sum.wrapping_add(word);
            }
            idx += 4;
        }

        // Target total 32-bit word sum == 0x5AA5_A55A
        let patch_word = 0x5AA5_A55Au32.wrapping_sub(sum);
        rom[checksum_offset..checksum_offset + 4].copy_from_slice(&patch_word.to_be_bytes());
        Ok(patch_word)
    }
}
