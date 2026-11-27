//! Ported from `ecu_engine_rust::flashing::bench_protocol`.
//!
//! Pure byte-level protocol logic: builders, parsers, encoders, state machines.
//! No I/O, no device access, no `unsafe`. Transport is the caller's concern.
//!
//! Verified by unit tests with hand-derived expectations. **Not** validated
//! against a physical ECU; no such claim is made.

/// Microcontroller and ECU Architecture Family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchitectureFamily {
    TriCore17xx,
    TriCoreAurix2xx,
    PowerPcMpc5xx,
    RenesasSuperH7058,
    RenesasRh850,
    SiemensContinental,
    DensoGen2,
    KeihinMoto,
}

/// Bench Mode Wiring & Pinout Definition (DIN 72552 & GPT Pins).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiBusBenchPinout {
    pub kl30_permanent_12v: bool,
    pub kl15_switched_ignition: bool,
    pub kl31_ground: bool,
    pub can_high_connected: bool,
    pub can_low_connected: bool,
    pub kline_connected: bool,
    pub gpt1_clock_sync: bool,
    pub gpt2_clock_sync: bool,
}

impl MultiBusBenchPinout {
    /// Standard Bench Mode pinout for 32-bit Microcontrollers (TriCore / Aurix).
    pub fn standard_bench() -> Self {
        Self {
            kl30_permanent_12v: true,
            kl15_switched_ignition: true,
            kl31_ground: true,
            can_high_connected: true,
            can_low_connected: true,
            kline_connected: false,
            gpt1_clock_sync: true, // GPT pins required for password readout without opening ECU
            gpt2_clock_sync: true,
        }
    }

    /// Check if wiring is complete for Bench Mode.
    pub fn is_wiring_valid(&self, family: ArchitectureFamily) -> Result<bool, &'static str> {
        if !self.kl30_permanent_12v || !self.kl15_switched_ignition || !self.kl31_ground {
            return Err("Power supply wiring incomplete: KL30, KL15, and KL31 are strictly required");
        }

        match family {
            ArchitectureFamily::TriCore17xx
            | ArchitectureFamily::TriCoreAurix2xx
            | ArchitectureFamily::SiemensContinental => {
                if !self.can_high_connected || !self.can_low_connected {
                    return Err("Bench mode requires differential CAN-H and CAN-L connection");
                }
                if !self.gpt1_clock_sync || !self.gpt2_clock_sync {
                    return Err("Bench mode requires GPT1 and GPT2 clock sync pins to read security password");
                }
            }
            ArchitectureFamily::RenesasRh850 | ArchitectureFamily::KeihinMoto => {
                if !self.kline_connected {
                    return Err("Motorcycle Bench mode requires K-Line pin connection");
                }
            }
            _ => {}
        }

        Ok(true)
    }
}

/// Differential Additive Checksum Patcher for Cryptographically Protected ECUs.
pub struct DifferentialDeltaPatcher;

impl DifferentialDeltaPatcher {
    /// Determines if ECU family requires original baseline file to seal checksum.
    pub fn requires_baseline_rom(family: ArchitectureFamily) -> bool {
        match family {
            ArchitectureFamily::TriCore17xx
            | ArchitectureFamily::TriCoreAurix2xx
            | ArchitectureFamily::SiemensContinental => true,
            _ => false,
        }
    }

    /// Calculates byte differences between Original and Tuned ROM,
    /// computes additive inverse delta, and updates the checksum register cell.
    pub fn compute_additive_delta_patch(
        ori_bytes: &[u8],
        tuned_bytes: &mut [u8],
        checksum_patch_offset: usize,
    ) -> Result<i64, &'static str> {
        if ori_bytes.len() != tuned_bytes.len() {
            return Err("Original and Tuned binary buffers must have identical size");
        }
        if checksum_patch_offset
            .checked_add(4)
            .is_none_or(|e| e > tuned_bytes.len())
        {
            return Err("Checksum patch offset out of bounds");
        }

        let mut delta: i64 = 0;
        for i in 0..ori_bytes.len() {
            if i >= checksum_patch_offset && i < checksum_patch_offset + 4 {
                continue;
            }
            let diff = tuned_bytes[i] as i64 - ori_bytes[i] as i64;
            delta += diff;
        }

        let ori_cks = u32::from_be_bytes([
            ori_bytes[checksum_patch_offset],
            ori_bytes[checksum_patch_offset + 1],
            ori_bytes[checksum_patch_offset + 2],
            ori_bytes[checksum_patch_offset + 3],
        ]) as i64;

        let new_cks = (ori_cks - delta).rem_euclid(1i64 << 32) as u32;

        let new_bytes = new_cks.to_be_bytes();
        tuned_bytes[checksum_patch_offset..checksum_patch_offset + 4].copy_from_slice(&new_bytes);

        Ok(delta)
    }
}

/// 256-Byte S-Box Rotational Bitwise Stream Cipher.
pub struct RotationalSBoxCipher;

impl RotationalSBoxCipher {
    /// Decrypt data buffer in-place using S-Box substitution and bitwise left rotation.
    /// Formula: `out[i] = ROL(in[i] ^ SBox[i & 0xFF], 1)`
    pub fn decrypt_stream(data: &mut [u8], sbox: &[u8; 256]) {
        for (i, b) in data.iter_mut().enumerate() {
            let k = sbox[i & 0xFF];
            let x = *b ^ k;
            *b = (x << 1) | (x >> 7);
        }
    }

    /// Encrypt data buffer in-place using bitwise right rotation and S-Box substitution.
    /// Formula: `out[i] = ROR(in[i], 1) ^ SBox[i & 0xFF]`
    pub fn encrypt_stream(data: &mut [u8], sbox: &[u8; 256]) {
        for (i, b) in data.iter_mut().enumerate() {
            let ror1 = (*b >> 1) | (*b << 7);
            *b = ror1 ^ sbox[i & 0xFF];
        }
    }
}

/// Universal Feistel & Rotate-XOR SecurityAccess Solvers.
pub struct UniversalSecurityAccess;

impl UniversalSecurityAccess {
    /// 5-round Feistel-style network (K = 0x1A76B3C4) for 32-bit automotive controllers.
    pub fn solve_feistel_5round(seed_bytes: [u8; 4]) -> [u8; 4] {
        let mut s = u32::from_be_bytes(seed_bytes);
        const K: u32 = 0x1A76B3C4;

        for _ in 0..5 {
            s = (s ^ K).rotate_right(3);
            s = s.wrapping_add(K);
        }

        s.to_be_bytes()
    }

    /// 5-round dynamic rotate-XOR (K = 0x5FBD5DBD) for 32-bit automotive controllers.
    pub fn solve_rotate_xor_5round(seed_bytes: [u8; 4]) -> [u8; 4] {
        let mut s = u32::from_be_bytes(seed_bytes);
        const K: u32 = 0x5FBD5DBD;

        for i in 0..5 {
            let shift = (i * 7) & 31;
            s ^= K;
            s = s.rotate_left(shift);
        }

        s.to_be_bytes()
    }
}

/// Microcontroller CBOOT RSA Signature Bypass Patcher.
pub struct DevelopmentSampleModePatcher;

impl DevelopmentSampleModePatcher {
    /// Machine code needle: `mov d15, #0; j +2; mov d15, #1; mov d2, d15`
    pub const NEEDLE: [u8; 8] = [0xDA, 0x00, 0x3C, 0x02, 0xDA, 0x01, 0x02, 0xF2];

    /// Machine code patch: `nop; nop; mov d15, #1; mov d2, d15`
    pub const PATCH: [u8; 8] = [0x00, 0x00, 0x00, 0x00, 0xDA, 0x01, 0x02, 0xF2];

    /// Search and patch CBOOT buffer in-place to enforce sample mode return = 1.
    pub fn patch_sample_mode(cboot: &mut [u8]) -> Result<usize, &'static str> {
        let mut patch_count = 0;
        let mut i = 0;
        while i + 8 <= cboot.len() {
            if cboot[i..i + 8] == Self::NEEDLE {
                cboot[i..i + 8].copy_from_slice(&Self::PATCH);
                patch_count += 1;
                i += 8;
            } else {
                i += 1;
            }
        }

        if patch_count == 0 {
            Err("Needle pattern not found in buffer (already patched or different version)")
        } else {
            Ok(patch_count)
        }
    }
}

/// Physical Flash Block Functional Type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryBlockType {
    Boot,
    Parameter,
    Calibration,
    OperatingSystem,
}

/// Physical Flash Block Memory Range Descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryRangeDescriptor {
    pub start_address: u32,
    pub length: u32,
    pub block_type: MemoryBlockType,
}

/// Physical Silicon Flash Chip Information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiliconChipInfo {
    pub jedec_id: u32,
    pub model_name: &'static str,
    pub total_size_bytes: u32,
    pub blocks: Vec<MemoryRangeDescriptor>,
}

/// Universal Physical NOR Flash Silicon Classifier & Partition Resolver.
pub struct FlashSiliconClassifier;

impl FlashSiliconClassifier {
    /// Identify physical NOR flash chip and map its partition architecture from 32-bit Chip ID.
    pub fn identify(chip_id: u32) -> Result<SiliconChipInfo, &'static str> {
        match chip_id {
            // Intel 28F200BX (256 KiB)
            0x00892274 | 0x00892275 => Ok(SiliconChipInfo {
                jedec_id: chip_id,
                model_name: "Intel 28F200BX 256KiB",
                total_size_bytes: 256 * 1024,
                blocks: vec![
                    MemoryRangeDescriptor { start_address: 0x00000, length: 0x04000, block_type: MemoryBlockType::Boot },
                    MemoryRangeDescriptor { start_address: 0x04000, length: 0x02000, block_type: MemoryBlockType::Parameter },
                    MemoryRangeDescriptor { start_address: 0x06000, length: 0x02000, block_type: MemoryBlockType::Parameter },
                    MemoryRangeDescriptor { start_address: 0x08000, length: 0x18000, block_type: MemoryBlockType::Calibration },
                    MemoryRangeDescriptor { start_address: 0x20000, length: 0x20000, block_type: MemoryBlockType::OperatingSystem },
                ],
            }),

            // Intel 28F400B (512 KiB)
            0x00894471 => Ok(SiliconChipInfo {
                jedec_id: chip_id,
                model_name: "Intel 28F400B 512KiB",
                total_size_bytes: 512 * 1024,
                blocks: vec![
                    MemoryRangeDescriptor { start_address: 0x00000, length: 0x04000, block_type: MemoryBlockType::Boot },
                    MemoryRangeDescriptor { start_address: 0x04000, length: 0x02000, block_type: MemoryBlockType::Parameter },
                    MemoryRangeDescriptor { start_address: 0x06000, length: 0x02000, block_type: MemoryBlockType::Parameter },
                    MemoryRangeDescriptor { start_address: 0x08000, length: 0x18000, block_type: MemoryBlockType::Calibration },
                    MemoryRangeDescriptor { start_address: 0x20000, length: 0x20000, block_type: MemoryBlockType::OperatingSystem },
                    MemoryRangeDescriptor { start_address: 0x40000, length: 0x20000, block_type: MemoryBlockType::OperatingSystem },
                    MemoryRangeDescriptor { start_address: 0x60000, length: 0x20000, block_type: MemoryBlockType::OperatingSystem },
                ],
            }),

            // AMD / Spansion Am29F400BT (512 KiB Top-Boot)
            0x00012258 => Ok(SiliconChipInfo {
                jedec_id: chip_id,
                model_name: "AMD Am29F400BT 512KiB Top-Boot",
                total_size_bytes: 512 * 1024,
                blocks: vec![
                    MemoryRangeDescriptor { start_address: 0x00000, length: 0x10000, block_type: MemoryBlockType::OperatingSystem },
                    MemoryRangeDescriptor { start_address: 0x10000, length: 0x10000, block_type: MemoryBlockType::OperatingSystem },
                    MemoryRangeDescriptor { start_address: 0x20000, length: 0x10000, block_type: MemoryBlockType::OperatingSystem },
                    MemoryRangeDescriptor { start_address: 0x30000, length: 0x10000, block_type: MemoryBlockType::Calibration },
                    MemoryRangeDescriptor { start_address: 0x70000, length: 0x08000, block_type: MemoryBlockType::Parameter },
                    MemoryRangeDescriptor { start_address: 0x78000, length: 0x04000, block_type: MemoryBlockType::Parameter },
                    MemoryRangeDescriptor { start_address: 0x7C000, length: 0x04000, block_type: MemoryBlockType::Boot },
                ],
            }),

            // AMD / Spansion Am29F800BB (1 MiB Bottom-Boot)
            0x000122D7 => Ok(SiliconChipInfo {
                jedec_id: chip_id,
                model_name: "AMD Am29F800BB 1024KiB Bottom-Boot",
                total_size_bytes: 1024 * 1024,
                blocks: vec![
                    MemoryRangeDescriptor { start_address: 0x00000, length: 0x04000, block_type: MemoryBlockType::Boot },
                    MemoryRangeDescriptor { start_address: 0x04000, length: 0x02000, block_type: MemoryBlockType::Parameter },
                    MemoryRangeDescriptor { start_address: 0x06000, length: 0x02000, block_type: MemoryBlockType::Parameter },
                    MemoryRangeDescriptor { start_address: 0x08000, length: 0x18000, block_type: MemoryBlockType::Calibration },
                    MemoryRangeDescriptor { start_address: 0x20000, length: 0x20000, block_type: MemoryBlockType::OperatingSystem },
                    MemoryRangeDescriptor { start_address: 0x40000, length: 0x40000, block_type: MemoryBlockType::OperatingSystem },
                    MemoryRangeDescriptor { start_address: 0x80000, length: 0x80000, block_type: MemoryBlockType::OperatingSystem },
                ],
            }),

            _ => Err("Unknown silicon flash chip ID"),
        }
    }

    /// Check if target block is safe to erase/write (safeguards bootloader sector from deletion).
    pub fn is_block_write_safe(block: &MemoryRangeDescriptor) -> bool {
        block.block_type != MemoryBlockType::Boot
    }
}

/// Calibration Verification Number (CVN) Correction & Balancing Engine (SAE J1979 Mode 09 PID 06).
/// Implements True IEEE 802.3 CRC32 Polynomial (0xEDB88320) and Galois Field GF(2)
/// 4-byte reverse algebraic balancer for bit-exact factory CVN preservation.
pub struct CvnCorrectionEngine;

impl CvnCorrectionEngine {
    pub const CRC32_POLY: u32 = 0xEDB8_8320;

    /// Compile-time precomputed IEEE 802.3 CRC32 lookup table.
    const fn generate_crc32_table() -> [u32; 256] {
        let mut table = [0u32; 256];
        let mut i = 0;
        while i < 256 {
            let mut c = i as u32;
            let mut j = 0;
            while j < 8 {
                if c & 1 != 0 {
                    c = Self::CRC32_POLY ^ (c >> 1);
                } else {
                    c >>= 1;
                }
                j += 1;
            }
            table[i] = c;
            i += 1;
        }
        table
    }

    const CRC32_TABLE: [u32; 256] = Self::generate_crc32_table();

    /// Calculate raw CRC32 with arbitrary initial and final XOR parameters.
    pub fn crc32_raw(data: &[u8], init: u32, xor_out: u32) -> u32 {
        let mut crc = init;
        for &b in data {
            crc = Self::CRC32_TABLE[((crc ^ (b as u32)) & 0xFF) as usize] ^ (crc >> 8);
        }
        crc ^ xor_out
    }

    /// Calculate SAE J1979 Mode 09 PID 06 CVN using True CRC32 (IEEE 802.3 / Bosch MEVD17 standard).
    pub fn calculate_calibration_cvn(calibration_data: &[u8]) -> u32 {
        Self::crc32_raw(calibration_data, 0xFFFF_FFFF, 0xFFFF_FFFF)
    }

    /// Reverses 1 bit of CRC32 accumulator when stepped with a zero bit.
    #[inline(always)]
    fn step_backward_bit_zero(reg: u32) -> u32 {
        if (reg & 0x8000_0000) != 0 {
            ((reg ^ Self::CRC32_POLY) << 1) | 1
        } else {
            reg << 1
        }
    }

    /// Reverses 1 byte (8 bits) of CRC32 accumulator through zero bytes in GF(2).
    #[inline(always)]
    fn step_backward_byte_zero(mut reg: u32) -> u32 {
        for _ in 0..8 {
            reg = Self::step_backward_bit_zero(reg);
        }
        reg
    }

    /// 32x32 binary matrix multiplication over GF(2).
    fn mat_mul_gf2(a: &[u32; 32], b: &[u32; 32]) -> [u32; 32] {
        let mut res = [0u32; 32];
        for i in 0..32 {
            let mut row_val = 0u32;
            for j in 0..32 {
                if (a[i] & (1 << j)) != 0 {
                    row_val ^= b[j];
                }
            }
            res[i] = row_val;
        }
        res
    }

    /// 32x32 binary matrix-vector multiplication over GF(2).
    fn mat_vec_mul_gf2(m: &[u32; 32], v: u32) -> u32 {
        let mut res = 0u32;
        for i in 0..32 {
            if (m[i] & v).count_ones() % 2 != 0 {
                res |= 1 << i;
            }
        }
        res
    }

    /// Build transition matrix for stepping backward 1 zero byte in GF(2).
    fn build_step_backward_byte_matrix() -> [u32; 32] {
        let mut cols = [0u32; 32];
        for j in 0..32 {
            cols[j] = Self::step_backward_byte_zero(1 << j);
        }
        let mut rows = [0u32; 32];
        for i in 0..32 {
            for j in 0..32 {
                if (cols[j] & (1 << i)) != 0 {
                    rows[i] |= 1 << j;
                }
            }
        }
        rows
    }

    /// Matrix exponentiation over GF(2) to step backward L bytes in O(log L) time.
    fn mat_pow_gf2(mut base: [u32; 32], mut exp: usize) -> [u32; 32] {
        let mut result = [0u32; 32];
        for i in 0..32 {
            result[i] = 1 << i;
        }
        while exp > 0 {
            if exp & 1 == 1 {
                result = Self::mat_mul_gf2(&result, &base);
            }
            base = Self::mat_mul_gf2(&base, &base);
            exp >>= 1;
        }
        result
    }

    /// Step CRC32 accumulator backwards through `num_zeros` bytes in microseconds.
    pub fn step_backward_zeros(reg: u32, num_zeros: usize) -> u32 {
        let base = Self::build_step_backward_byte_matrix();
        let pow_mat = Self::mat_pow_gf2(base, num_zeros);
        Self::mat_vec_mul_gf2(&pow_mat, reg)
    }

    /// Gaussian elimination solver over GF(2) to solve A * x = target (mod 2).
    fn solve_gf2(cols: &[u32; 32], target: u32) -> Option<u32> {
        let mut rows = [0u64; 32];
        for i in 0..32 {
            for j in 0..32 {
                if (cols[j] & (1 << i)) != 0 {
                    rows[i] |= 1u64 << j;
                }
            }
            if (target & (1 << i)) != 0 {
                rows[i] |= 1u64 << 32;
            }
        }

        for col in 0..32 {
            let mut pivot = None;
            for r in col..32 {
                if (rows[r] & (1u64 << col)) != 0 {
                    pivot = Some(r);
                    break;
                }
            }
            let pivot = pivot?;
            rows.swap(col, pivot);

            for r in 0..32 {
                if r != col && (rows[r] & (1u64 << col)) != 0 {
                    rows[r] ^= rows[col];
                }
            }
        }

        let mut sol = 0u32;
        for col in 0..32 {
            if (rows[col] & (1u64 << 32)) != 0 {
                sol |= 1 << col;
            }
        }
        Some(sol)
    }

    /// Invert and balance True CRC32 CVN using 4-byte padding cell in Galois Field GF(2).
    /// Compensates for arbitrary byte edits (avalanche effect) so calibration matches target original CVN 100%.
    pub fn correct_cvn_with_balancer(
        calibration_data: &mut [u8],
        balancer_offset: usize,
        target_original_cvn: u32,
    ) -> Result<u32, &'static str> {
        if balancer_offset
            .checked_add(4)
            .is_none_or(|e| e > calibration_data.len())
        {
            return Err("Balancer offset out of bounds");
        }
        if balancer_offset % 4 != 0 {
            return Err("Balancer offset must be 4-byte word aligned");
        }

        // Zero out current balancer cell
        calibration_data[balancer_offset..balancer_offset + 4].copy_from_slice(&[0, 0, 0, 0]);

        // Calculate current CRC32 with zeroed balancer cell
        let current_cvn = Self::calculate_calibration_cvn(calibration_data);
        if current_cvn == target_original_cvn {
            return Ok(0);
        }

        let delta = target_original_cvn ^ current_cvn;
        let l_zeros = calibration_data.len() - (balancer_offset + 4);

        // Step delta backwards through trailing zero bytes
        let target_after_4bytes = Self::step_backward_zeros(delta, l_zeros);

        // Build 32 linear basis columns for the 4 patch bytes
        let mut cols = [0u32; 32];
        for j in 0..32 {
            let val = 1u32 << j;
            let bytes = val.to_le_bytes();
            cols[j] = Self::crc32_raw(&bytes, 0, 0);
        }

        let balancer_val = Self::solve_gf2(&cols, target_after_4bytes)
            .ok_or("Failed to invert GF(2) linear system for CRC32")?;

        let balancer_bytes = balancer_val.to_le_bytes();
        calibration_data[balancer_offset..balancer_offset + 4].copy_from_slice(&balancer_bytes);

        // Verification
        let final_cvn = Self::calculate_calibration_cvn(calibration_data);
        if final_cvn == target_original_cvn {
            Ok(balancer_val)
        } else {
            Err("Failed to balance CVN to target")
        }
    }

    /// Format CVN as 8-character hex string for diagnostic scan tools.
    pub fn format_cvn_hex(cvn: u32) -> String {
        format!("{:08X}", cvn)
    }
}

/// Cloud-Assisted Virtual Read Stock Identification (Mode VR).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtualReadIdentifier {
    pub hardware_part_number: String,
    pub software_calibration_id: String,
    pub verified_stock_sha256: &'static str,
    pub rom_size_bytes: usize,
}

impl VirtualReadIdentifier {
    /// Construct a verified virtual read descriptor from ECU diagnostic identification.
    pub fn new(hw: &str, sw: &str, sha: &'static str, size: usize) -> Self {
        Self {
            hardware_part_number: hw.trim().to_uppercase(),
            software_calibration_id: sw.trim().to_uppercase(),
            verified_stock_sha256: sha,
            rom_size_bytes: size,
        }
    }
}

/// Sensitive Sectors Protection Policy (HIDE_SECTORS masking).
pub struct HideSectorsPolicy;

impl HideSectorsPolicy {
    /// Check whether a memory offset belongs to sensitive/immutable sectors
    /// (e.g. immobilizer transponder keys, OTP serials, injector coding, or bootloader).
    pub fn is_sector_hidden_or_protected(offset: u32, length: u32, protected_ranges: &[(u32, u32)]) -> bool {
        let off64 = offset as u64;
        let end = off64 + (length as u64);
        for &(p_start, p_len) in protected_ranges {
            let p_start64 = p_start as u64;
            let p_end = p_start64 + (p_len as u64);
            if off64 < p_end && end > p_start64 {
                return true;
            }
        }
        false
    }
}
