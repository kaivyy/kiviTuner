//! Ported from `ecu_engine_rust::flashing::flash_writer`.
//!
//! Pure byte-level protocol logic: builders, parsers, encoders, state machines.
//! No I/O, no device access, no `unsafe`. Transport is the caller's concern.
//!
//! Verified by unit tests with hand-derived expectations. **Not** validated
//! against a physical ECU; no such claim is made.

use crate::chip_verifier::{ChipVerifier, BlankCheckResult, VerifyDifference};

/// Flash Sector Definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlashSector {
    pub sector_index: usize,
    pub start_address: u32,
    pub length: u32,
    pub is_boot_sector: bool,
}

/// Pre-Flash Safety Checklist Status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreFlashCheckStatus {
    ReadyToFlash,
    VoltageTooLow(u32), // Millivolts, e.g. 11800 mV
    EngineRunningRpm(u32),
    SecurityAccessLocked,
}

/// Flash Writing Progress State.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlashWriteState {
    Idle,
    PreFlightSafetyChecks,
    DisablingCommAndDtc,
    ErasingSector { sector_index: usize, total_sectors: usize },
    WritingData { bytes_written: usize, total_bytes: usize },
    VerifyingChecksum,
    Completed,
    Failed(&'static str),
}

/// Universal Flash Writer Engine.
pub struct UniversalFlashWriter {
    pub sectors: Vec<FlashSector>,
    pub chunk_size: usize,
    pub allow_boot_sector_write: bool,
    pub state: FlashWriteState,
    pub rolling_sequence_counter: u8,
    pub erase_retries_0x78: usize,
    pub max_erase_retries: usize,
}

impl UniversalFlashWriter {
    pub fn new(sectors: Vec<FlashSector>, chunk_size: usize, allow_boot_sector_write: bool) -> Self {
        Self {
            sectors,
            chunk_size: chunk_size.clamp(128, 4096),
            allow_boot_sector_write,
            state: FlashWriteState::Idle,
            rolling_sequence_counter: 1,
            erase_retries_0x78: 0,
            max_erase_retries: 30, // 30 retries @ 500ms = 15 seconds max erase wait
        }
    }

    /// Perform pre-flash safety checks: Voltage >= 12.2V (12,200 mV) and Engine RPM == 0.
    pub fn perform_pre_flash_checks(
        supply_millivolts: u32,
        engine_rpm: u32,
        is_security_unlocked: bool,
    ) -> PreFlashCheckStatus {
        if supply_millivolts < 12_200 {
            return PreFlashCheckStatus::VoltageTooLow(supply_millivolts);
        }
        if engine_rpm > 0 {
            return PreFlashCheckStatus::EngineRunningRpm(engine_rpm);
        }
        if !is_security_unlocked {
            return PreFlashCheckStatus::SecurityAccessLocked;
        }
        PreFlashCheckStatus::ReadyToFlash
    }

    /// Build Communication & DTC Suppression Commands (UDS 0x28 0x03 and 0x85 0x02).
    pub fn build_comm_and_dtc_suppression_packets() -> (Vec<u8>, Vec<u8>) {
        let comm_control = vec![0x28, 0x03, 0x01]; // Disable non-diagnostic Rx and Tx
        let dtc_setting = vec![0x85, 0x02];        // Turn off DTC recording
        (comm_control, dtc_setting)
    }

    /// Build Erase Sector Command (UDS Service 0x31 0x01 0xFF 0x00).
    /// Enforces Boot Sector Preservation Policy!
    pub fn build_erase_sector_request(&mut self, sector_idx: usize) -> Result<Vec<u8>, &'static str> {
        let sector = self.sectors.get(sector_idx).ok_or("Invalid sector index")?;

        if sector.is_boot_sector && !self.allow_boot_sector_write {
            return Err("BOOT SECTOR WRITE BLOCKED: Overwriting boot block risks irreversible silicon bricking!");
        }

        self.state = FlashWriteState::ErasingSector {
            sector_index: sector_idx,
            total_sectors: self.sectors.len(),
        };

        // RoutineControl StartRoutine EraseMemory (0x31 0x01 0xFF 0x00)
        let mut pkt = vec![0x31, 0x01, 0xFF, 0x00, 0x44];
        pkt.extend_from_slice(&sector.start_address.to_be_bytes());
        pkt.extend_from_slice(&sector.length.to_be_bytes());
        Ok(pkt)
    }

    /// Handle Erase Response (checks for NRC 0x78 ResponsePending loop).
    pub fn handle_erase_response(&mut self, response_bytes: &[u8]) -> Result<bool, &'static str> {
        if response_bytes.is_empty() {
            return Err("Empty response during erase");
        }

        // Check for NRC: 0x7F 0x31 0x78
        if response_bytes.len() >= 3 && response_bytes[0] == 0x7F {
            if response_bytes[2] == 0x78 {
                self.erase_retries_0x78 += 1;
                if self.erase_retries_0x78 > self.max_erase_retries {
                    return Err("Erase timeout exceeded: Flash controller stuck in busy state");
                }
                return Ok(false); // Still pending, continue waiting
            }
            return Err("Erase memory rejected by ECU");
        }

        // Positive response: 0x71 0x01 (Routine Completed)
        if response_bytes[0] == 0x71 {
            self.erase_retries_0x78 = 0;
            return Ok(true); // Erase successfully completed
        }

        Err("Unknown erase response packet")
    }

    /// Build Request Download (Service 0x34) for upcoming binary block.
    /// Enforces 32-bit bounds and Boot Sector Preservation Policy.
    pub fn build_request_download(
        &self,
        start_addr: u32,
        block_len: u32,
    ) -> Result<Vec<u8>, &'static str> {
        if block_len == 0 {
            return Err("RequestDownload block length must be greater than zero");
        }
        let req_end = (start_addr as u64)
            .checked_add(block_len as u64)
            .filter(|&e| e <= (u32::MAX as u64) + 1)
            .ok_or("RequestDownload range overflows 32-bit ECU address space")?;

        if !self.allow_boot_sector_write {
            for sector in &self.sectors {
                if !sector.is_boot_sector || sector.length == 0 {
                    continue;
                }
                let sec_start = sector.start_address as u64;
                let sec_end = sec_start + sector.length as u64;
                if (start_addr as u64) < sec_end && req_end > sec_start {
                    return Err("BOOT SECTOR WRITE BLOCKED: RequestDownload overlaps protected boot sector!");
                }
            }
        }

        let mut pkt = vec![0x34, 0x00, 0x44];
        pkt.extend_from_slice(&start_addr.to_be_bytes());
        pkt.extend_from_slice(&block_len.to_be_bytes());
        Ok(pkt)
    }

    /// Build TransferData (Service 0x36) chunk with ISO 14229 rolling sequence counter (1..255).
    pub fn build_transfer_data_chunk(&mut self, chunk_payload: &[u8]) -> Vec<u8> {
        let mut pkt = Vec::with_capacity(chunk_payload.len() + 2);
        pkt.push(0x36); // TransferData
        pkt.push(self.rolling_sequence_counter);
        pkt.extend_from_slice(chunk_payload);

        // Increment sequence counter wrapping from 255 back to 0 (or 1 depending on ISO 14229-1)
        self.rolling_sequence_counter = self.rolling_sequence_counter.wrapping_add(1);
        pkt
    }

    /// Build Request Transfer Exit (Service 0x37).
    pub fn build_request_transfer_exit() -> Vec<u8> {
        vec![0x37]
    }

    /// Build Final Post-Flash Checksum Routine Verification (0x31 0x01 0x02 0x02) & Compare with Expected CRC.
    pub fn build_checksum_verification_routine(expected_crc32: u32) -> Vec<u8> {
        let mut pkt = vec![0x31, 0x01, 0x02, 0x02];
        pkt.extend_from_slice(&expected_crc32.to_be_bytes());
        pkt
    }

    /// Validate that an erased memory slice is 100% blank (0xFF) before writing data.
    /// Prevents writing into dirty NOR Flash cells.
    pub fn verify_sector_blank(&mut self, readback_erased_sector: &[u8]) -> Result<(), (usize, u8)> {
        match ChipVerifier::check_blank(readback_erased_sector) {
            BlankCheckResult::Blank => Ok(()),
            BlankCheckResult::NotBlank { first_non_blank_offset, found_byte } => {
                self.state = FlashWriteState::Failed("Sector blank check failed: non-0xFF byte detected");
                Err((first_non_blank_offset, found_byte))
            }
        }
    }

    /// Complete post-flash verification comparing local target binary buffer against data read back from ECU.
    pub fn verify_post_flash_payload(
        &mut self,
        target_payload: &[u8],
        readback_chip_payload: &[u8],
        max_discrepancies: usize,
    ) -> Result<(), Vec<VerifyDifference>> {
        self.state = FlashWriteState::VerifyingChecksum;
        match ChipVerifier::verify_buffer(target_payload, readback_chip_payload, max_discrepancies) {
            Ok(()) => {
                self.state = FlashWriteState::Completed;
                Ok(())
            }
            Err(diffs) => {
                self.state = FlashWriteState::Failed("Post-flash payload verification mismatch");
                Err(diffs)
            }
        }
    }
}
