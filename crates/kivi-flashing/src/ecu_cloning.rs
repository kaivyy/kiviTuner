//! Ported from `ecu_engine_rust::flashing::ecu_cloning`.
//!
//! Pure byte-level protocol logic: builders, parsers, encoders, state machines.
//! No I/O, no device access, no `unsafe`. Transport is the caller's concern.
//!
//! Verified by unit tests with hand-derived expectations. **Not** validated
//! against a physical ECU; no such claim is made.

use crate::bench_protocol::CvnCorrectionEngine;

/// 4-Partition Memory Selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartitionComponent {
    /// `Maps`: Calibration tables only
    Maps,
    /// `Micro`: Internal Microcontroller Flash (`TC17xx`, `MPC5xx`, `SH705x`, `RH850`)
    Micro,
    /// `ExtFlash`: External NOR Flash chip (`Am29F400`, `S29CD016`, `M58BW016`)
    ExtFlash,
    /// `Eeprom`: Internal/External EEPROM (Immo ISN, VIN, Injector Coding, Adaptations)
    Eeprom,
}

/// One-Time Programmable (OTP) Silicon Sector Descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OtpZoneDescriptor {
    pub start_offset: usize,
    pub length: usize,
    pub is_hardware_locked: bool,
}

/// Complete 4-Partition ECU Backup Container (`Maps`, `Micro`, `ExtFlash`, `Eeprom`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuadPartitionBackupContainer {
    pub ecu_hw_number: String,
    pub ecu_sw_number: String,
    pub univocal_mcu_uid: [u8; 16],
    pub bsl_password: Vec<u8>,
    pub maps_data: Vec<u8>,
    pub micro_flash: Vec<u8>,
    pub ext_flash: Vec<u8>,
    pub eeprom_data: Vec<u8>,
    pub otp_zones: Vec<OtpZoneDescriptor>,
}

impl QuadPartitionBackupContainer {
    pub const CONTAINER_MAGIC: &'static [u8; 8] = b"KIVIBAK1";
    pub const CONTAINER_MAGIC_V2: &'static [u8; 8] = b"KIVIBAK2";

    /// Serialize all 4 components (`Maps`, `Micro`, `ExtFlash`, `Eeprom`), MCU UID, BSL Password,
    /// and OTP zone descriptors into a single CRC32-sealed backup container file.
    pub fn pack_single_backup_file(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(Self::CONTAINER_MAGIC_V2);

        // UID (16 bytes)
        buf.extend_from_slice(&self.univocal_mcu_uid);

        // Helper closure to write length-prefixed slice
        let write_blob = |out: &mut Vec<u8>, data: &[u8]| {
            let len = data.len() as u32;
            out.extend_from_slice(&len.to_be_bytes());
            out.extend_from_slice(data);
        };

        write_blob(&mut buf, self.ecu_hw_number.as_bytes());
        write_blob(&mut buf, self.ecu_sw_number.as_bytes());
        write_blob(&mut buf, &self.bsl_password);
        write_blob(&mut buf, &self.maps_data);
        write_blob(&mut buf, &self.micro_flash);
        write_blob(&mut buf, &self.ext_flash);
        write_blob(&mut buf, &self.eeprom_data);

        let otp_count = u32::try_from(self.otp_zones.len()).unwrap_or(u32::MAX);
        buf.extend_from_slice(&otp_count.to_be_bytes());
        for zone in &self.otp_zones[..otp_count as usize] {
            buf.extend_from_slice(&(zone.start_offset as u64).to_be_bytes());
            buf.extend_from_slice(&(zone.length as u64).to_be_bytes());
            buf.push(u8::from(zone.is_hardware_locked));
        }

        // Append IEEE 802.3 CRC32 over entire container payload
        let crc = CvnCorrectionEngine::calculate_calibration_cvn(&buf);
        buf.extend_from_slice(&crc.to_be_bytes());
        buf
    }

    /// Unpack and verify CRC32 integrity of a single backup container file (`IDS_LOADER_ERROR_20003`).
    pub fn unpack_single_backup_file(raw: &[u8]) -> Result<Self, &'static str> {
        if raw.len() < 8 + 16 + 7 * 4 + 4 {
            return Err("IDS_LOADER_ERROR_20002: Backup container file is truncated");
        }
        let is_v2 = &raw[0..8] == Self::CONTAINER_MAGIC_V2;
        if !is_v2 && &raw[0..8] != Self::CONTAINER_MAGIC {
            return Err("IDS_LOADER_ERROR_20001: Invalid backup container magic header");
        }

        let payload_end = raw.len() - 4;
        let expected_crc = u32::from_be_bytes([
            raw[payload_end],
            raw[payload_end + 1],
            raw[payload_end + 2],
            raw[payload_end + 3],
        ]);
        let computed_crc = CvnCorrectionEngine::calculate_calibration_cvn(&raw[..payload_end]);
        if expected_crc != computed_crc {
            return Err("IDS_LOADER_ERROR_20003: Backup container CRC32 verification failed");
        }

        let mut uid = [0u8; 16];
        uid.copy_from_slice(&raw[8..24]);
        let mut cursor = 24usize;

        let read_blob = |cur: &mut usize| -> Result<Vec<u8>, &'static str> {
            let len_end = cur
                .checked_add(4)
                .filter(|&end| end <= payload_end)
                .ok_or("IDS_LOADER_ERROR_20002: Truncated blob length in backup container")?;
            let len = u32::from_be_bytes([
                raw[*cur],
                raw[*cur + 1],
                raw[*cur + 2],
                raw[*cur + 3],
            ]) as usize;
            *cur = len_end;
            let data_end = cur
                .checked_add(len)
                .filter(|&end| end <= payload_end)
                .ok_or("IDS_LOADER_ERROR_20002: Truncated blob payload in backup container")?;
            let slice = raw[*cur..data_end].to_vec();
            *cur = data_end;
            Ok(slice)
        };

        let hw_bytes = read_blob(&mut cursor)?;
        let sw_bytes = read_blob(&mut cursor)?;
        let bsl_password = read_blob(&mut cursor)?;
        let maps_data = read_blob(&mut cursor)?;
        let micro_flash = read_blob(&mut cursor)?;
        let ext_flash = read_blob(&mut cursor)?;
        let eeprom_data = read_blob(&mut cursor)?;

        let otp_zones = if is_v2 {
            let count_end = cursor
                .checked_add(4)
                .filter(|&end| end <= payload_end)
                .ok_or("IDS_LOADER_ERROR_20002: Truncated OTP zone count in backup container")?;
            let count = u32::from_be_bytes([
                raw[cursor],
                raw[cursor + 1],
                raw[cursor + 2],
                raw[cursor + 3],
            ]) as usize;
            cursor = count_end;
            let zones_end = count
                .checked_mul(17)
                .and_then(|n| cursor.checked_add(n))
                .filter(|&end| end <= payload_end)
                .ok_or("IDS_LOADER_ERROR_20002: Truncated OTP zone table in backup container")?;
            let mut zones = Vec::with_capacity(count);
            while cursor < zones_end {
                let start_offset = usize::try_from(u64::from_be_bytes([
                    raw[cursor],
                    raw[cursor + 1],
                    raw[cursor + 2],
                    raw[cursor + 3],
                    raw[cursor + 4],
                    raw[cursor + 5],
                    raw[cursor + 6],
                    raw[cursor + 7],
                ]))
                .map_err(|_| "IDS_LOADER_ERROR_20002: OTP start_offset overflows usize")?;
                let length = usize::try_from(u64::from_be_bytes([
                    raw[cursor + 8],
                    raw[cursor + 9],
                    raw[cursor + 10],
                    raw[cursor + 11],
                    raw[cursor + 12],
                    raw[cursor + 13],
                    raw[cursor + 14],
                    raw[cursor + 15],
                ]))
                .map_err(|_| "IDS_LOADER_ERROR_20002: OTP length overflows usize")?;
                let is_hardware_locked = raw[cursor + 16] != 0;
                zones.push(OtpZoneDescriptor {
                    start_offset,
                    length,
                    is_hardware_locked,
                });
                cursor += 17;
            }
            zones
        } else {
            Vec::new()
        };

        let ecu_hw_number = String::from_utf8(hw_bytes).map_err(|_| "Invalid UTF-8 in HW number")?;
        let ecu_sw_number = String::from_utf8(sw_bytes).map_err(|_| "Invalid UTF-8 in SW number")?;

        Ok(Self {
            ecu_hw_number,
            ecu_sw_number,
            univocal_mcu_uid: uid,
            bsl_password,
            maps_data,
            micro_flash,
            ext_flash,
            eeprom_data,
            otp_zones,
        })
    }
}

/// 3-Phase Guided ECU Cloning State (`IDS_CLONE_PHASE_0/1/2`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClonePhase {
    /// Phase 0: Connect Original ECU and read Full Backup
    Phase0ReadOriginal,
    /// Phase 1: Disconnect Original ECU and connect Target (Donor) ECU
    Phase1SwitchToDonor,
    /// Phase 2: Validate Donor OTP & UID, adapt image, and write to Donor ECU
    Phase2ValidateAndAdaptDonor,
    /// Cloning successfully completed
    CloneCompleted,
}

/// 3-Phase Guided ECU Cloning Controller.
pub struct EcuCloneSession {
    pub phase: ClonePhase,
    pub original_backup: Option<QuadPartitionBackupContainer>,
}

impl Default for EcuCloneSession {
    fn default() -> Self {
        Self::new()
    }
}

impl EcuCloneSession {
    pub fn new() -> Self {
        Self {
            phase: ClonePhase::Phase0ReadOriginal,
            original_backup: None,
        }
    }

    /// Phase 0: Capture Original ECU backup (`Micro` + `ExtFlash` + `Eeprom` + `OTP`) and advance to Phase 1.
    pub fn capture_original_ecu(&mut self, backup: QuadPartitionBackupContainer) -> Result<ClonePhase, &'static str> {
        if backup.micro_flash.is_empty() && backup.ext_flash.is_empty() {
            return Err("Original ECU backup must contain Micro Flash or External Flash data");
        }
        self.original_backup = Some(backup);
        self.phase = ClonePhase::Phase1SwitchToDonor;
        Ok(self.phase)
    }

    /// Phase 1: Confirm user disconnected Original ECU and connected Donor ECU.
    pub fn confirm_donor_connected(
        &mut self,
        donor_uid: &[u8; 16],
    ) -> Result<ClonePhase, &'static str> {
        let ori = self
            .original_backup
            .as_ref()
            .ok_or("Cannot enter Phase 1 before reading Original ECU")?;
        if donor_uid == &ori.univocal_mcu_uid {
            return Err("CLONE_ERR_20055: Target ECU has identical UID to Original ECU (did you forget to swap ECUs?)");
        }
        self.phase = ClonePhase::Phase2ValidateAndAdaptDonor;
        Ok(self.phase)
    }

    /// Phase 2: Validate OTP (One-Time Programmable) Zones and adapt the
    /// Original `Micro` flash image so immutable Donor OTP sectors are preserved while cloning the rest of the ECU.
    pub fn adapt_clone_image_for_donor(
        &mut self,
        donor_hw_number: &str,
        donor_micro_flash: &[u8],
        preserve_donor_otp: bool,
    ) -> Result<QuadPartitionBackupContainer, &'static str> {
        if self.phase != ClonePhase::Phase2ValidateAndAdaptDonor {
            return Err("Clone session is not in Phase 2");
        }

        let ori = self
            .original_backup
            .as_ref()
            .ok_or("Missing Original ECU backup")?;

        if ori.ecu_hw_number != donor_hw_number {
            return Err("CLONE_ERR_21008: Donor ECU Hardware Number does not match Original ECU");
        }
        if ori.micro_flash.len() != donor_micro_flash.len() {
            return Err("Donor Micro Flash size does not match Original Micro Flash size");
        }

        let mut adapted_micro = ori.micro_flash.clone();

        for otp in &ori.otp_zones {
            let end = otp.start_offset.saturating_add(otp.length);
            if end > donor_micro_flash.len() {
                return Err("OTP zone descriptor out of bounds");
            }
            let ori_otp = &ori.micro_flash[otp.start_offset..end];
            let donor_otp = &donor_micro_flash[otp.start_offset..end];

            if otp.is_hardware_locked && ori_otp != donor_otp {
                if preserve_donor_otp {
                    // Retain Donor's locked OTP bytes so hardware flash write will not fail
                    adapted_micro[otp.start_offset..end].copy_from_slice(donor_otp);
                } else {
                    return Err("CLONE_ERR_21009: OTP Zone is locked and differs between Original and Donor ECU");
                }
            }
        }

        let mut adapted_container = ori.clone();
        adapted_container.micro_flash = adapted_micro;
        self.phase = ClonePhase::CloneCompleted;
        Ok(adapted_container)
    }
}

/// Foreign Tool Watermark Poisoning Guard.
/// Detects anti-read / lock poison tags that cause intentional checksum corruption when flashed.
pub struct ForeignWatermarkGuard;

impl ForeignWatermarkGuard {
    pub const CMD_MDC_MARKER: &'static [u8] = b"CMD-MDC";
    pub const NOREAD_MARKER: &'static [u8] = b"NOREAD";

    /// Scan binary buffer for foreign poison watermark.
    pub fn validate_safe_for_checksum_and_write(rom_bytes: &[u8]) -> Result<(), &'static str> {
        if Self::contains_subslice(rom_bytes, Self::CMD_MDC_MARKER) {
            return Err("POISON_TAG_DETECTED: Cannot calculate checksum or write a binary containing a foreign lock tag (CMD-MDC)");
        }
        Ok(())
    }

    /// Check if binary contains an obfuscation `"NOREAD"` marker.
    pub fn has_noread_watermark(rom_bytes: &[u8]) -> bool {
        Self::contains_subslice(rom_bytes, Self::NOREAD_MARKER)
    }

    fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
        if needle.is_empty() || haystack.len() < needle.len() {
            return false;
        }
        haystack.windows(needle.len()).any(|w| w == needle)
    }
}

/// EEPROM TPROT (Tuning Protection) Unlocker & BSL Password Extractor.
pub struct TprotEepromUnlocker;

impl TprotEepromUnlocker {
    /// Unlock Continental Simos PCR2.1 / Simos 8.6 / SID208 EEPROM TPROT protection in-place.
    /// Inverts the 8-byte TPROT security block at `unlock_offset` (`byte ^= 0xFF`) and updates
    /// the trailing 16-bit big-endian block additive checksum so OBD writing becomes unlocked.
    pub fn unlock_simos_pcr21_eeprom(
        eeprom: &mut [u8],
        unlock_offset: usize,
    ) -> Result<[u8; 8], &'static str> {
        if unlock_offset
            .checked_add(10)
            .is_none_or(|end| end > eeprom.len())
        {
            return Err("EEPROM buffer too small for 8-byte TPROT unlock + 2-byte checksum");
        }

        let mut unlocked_word = [0u8; 8];
        for i in 0..8 {
            eeprom[unlock_offset + i] ^= 0xFF;
            unlocked_word[i] = eeprom[unlock_offset + i];
        }

        // Recompute 16-bit additive inverse checksum over the 8-byte unlocked block
        let sum: u16 = unlocked_word.iter().map(|&b| b as u16).sum();
        let cks = (!sum).wrapping_add(1);
        let cks_bytes = cks.to_be_bytes();
        eeprom[unlock_offset + 8] = cks_bytes[0];
        eeprom[unlock_offset + 9] = cks_bytes[1];

        Ok(unlocked_word)
    }

    /// Extract 16-byte Bosch MEDC17 / MDG1 BSL / Service Mode Password from TriCore Boot Flash dump.
    pub fn extract_medc17_bsl_password(
        boot_dump: &[u8],
        password_offset: usize,
    ) -> Result<[u8; 16], &'static str> {
        if password_offset
            .checked_add(16)
            .is_none_or(|end| end > boot_dump.len())
        {
            return Err("BSL_ERR_40024: Boot dump truncated before BSL password offset");
        }
        let slice = &boot_dump[password_offset..password_offset + 16];
        if slice.iter().all(|&b| b == 0xFF) || slice.iter().all(|&b| b == 0x00) {
            return Err("BSL_ERR_40023: Blank or unreadable BSL password area");
        }
        let mut pwd = [0u8; 16];
        pwd.copy_from_slice(slice);
        Ok(pwd)
    }
}
