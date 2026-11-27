//! Ported from `ecu_engine_rust::flashing::recovery_journal`.
//!
//! Pure byte-level protocol logic: builders, parsers, encoders, state machines.
//! No I/O, no device access, no `unsafe`. Transport is the caller's concern.
//!
//! Verified by unit tests with hand-derived expectations. **Not** validated
//! against a physical ECU; no such claim is made.

use kivi_checksum::primitives::compute_crc32;

/// Status of the ECU flashing session recorded in journal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JournalStage {
    Initialized,
    ErasingSectors,
    WritingBlocks,
    VerifyingChecksum,
    FlashCompleted,
    InterruptedAbnormally,
}

/// Persistent Flashing Recovery Checkpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlashRecoveryCheckpoint {
    pub target_ecm_id: String,
    pub target_rom_crc32: u32,
    pub total_rom_size: usize,
    pub active_sector_index: usize,
    pub bytes_written: usize,
    pub stage: JournalStage,
    pub timestamp_epoch_secs: u64,
}

/// Recovery Journal Manager.
pub struct RecoveryJournalManager {
    pub checkpoint: FlashRecoveryCheckpoint,
    pub is_recovery_mode_active: bool,
}

impl RecoveryJournalManager {
    pub fn new(target_ecm_id: &str, target_rom: &[u8]) -> Self {
        let crc = compute_crc32(target_rom);
        Self {
            checkpoint: FlashRecoveryCheckpoint {
                target_ecm_id: target_ecm_id.to_string(),
                target_rom_crc32: crc,
                total_rom_size: target_rom.len(),
                active_sector_index: 0,
                bytes_written: 0,
                stage: JournalStage::Initialized,
                timestamp_epoch_secs: 0,
            },
            is_recovery_mode_active: false,
        }
    }

    /// Record start of a sector erase.
    pub fn record_sector_erase_start(&mut self, sector_idx: usize, current_time_secs: u64) {
        self.checkpoint.stage = JournalStage::ErasingSectors;
        self.checkpoint.active_sector_index = sector_idx;
        self.checkpoint.timestamp_epoch_secs = current_time_secs;
    }

    /// Record successfully committed block bytes.
    pub fn record_block_written(&mut self, bytes_added: usize, current_time_secs: u64) {
        self.checkpoint.stage = JournalStage::WritingBlocks;
        self.checkpoint.bytes_written += bytes_added;
        self.checkpoint.timestamp_epoch_secs = current_time_secs;
    }

    /// Mark flash complete.
    pub fn mark_completed(&mut self) {
        self.checkpoint.stage = JournalStage::FlashCompleted;
    }

    /// Check if a previous interrupted session exists that needs recovery.
    pub fn is_recovery_required(&self) -> bool {
        match self.checkpoint.stage {
            JournalStage::ErasingSectors | JournalStage::WritingBlocks | JournalStage::InterruptedAbnormally => true,
            _ => false,
        }
    }

    /// Evaluate resume offset: Returns the exact byte position where flash can be safely resumed.
    pub fn compute_resume_offset(&self) -> usize {
        if self.is_recovery_required() {
            // Resume at the start of the interrupted sector or at written byte boundary
            self.checkpoint.bytes_written
        } else {
            0
        }
    }

    const JOURNAL_MAGIC: &'static [u8; 8] = b"KIVIJRN1";

    /// Serialize recovery checkpoint into binary stream for local NVRAM/disk cache.
    pub fn serialize_checkpoint(&self) -> Vec<u8> {
        let id_bytes = self.checkpoint.target_ecm_id.as_bytes();
        let mut buf = Vec::with_capacity(53 + id_bytes.len());
        buf.extend_from_slice(Self::JOURNAL_MAGIC);
        buf.extend_from_slice(&self.checkpoint.target_rom_crc32.to_be_bytes());
        buf.extend_from_slice(&(self.checkpoint.total_rom_size as u64).to_be_bytes());
        buf.extend_from_slice(&(self.checkpoint.active_sector_index as u64).to_be_bytes());
        buf.extend_from_slice(&(self.checkpoint.bytes_written as u64).to_be_bytes());
        buf.push(match self.checkpoint.stage {
            JournalStage::Initialized => 0,
            JournalStage::ErasingSectors => 1,
            JournalStage::WritingBlocks => 2,
            JournalStage::VerifyingChecksum => 3,
            JournalStage::FlashCompleted => 4,
            JournalStage::InterruptedAbnormally => 5,
        });
        buf.extend_from_slice(&self.checkpoint.timestamp_epoch_secs.to_be_bytes());
        let id_len = u32::try_from(id_bytes.len()).unwrap_or(u32::MAX);
        buf.extend_from_slice(&id_len.to_be_bytes());
        buf.extend_from_slice(&id_bytes[..id_len as usize]);
        let trailer_crc = compute_crc32(&buf);
        buf.extend_from_slice(&trailer_crc.to_be_bytes());
        buf
    }

    /// Deserialize recovery checkpoint from stream.
    pub fn deserialize_checkpoint(bytes: &[u8], target_ecm_id: &str) -> Result<Self, &'static str> {
        if bytes.len() < 53 {
            return Err("Checkpoint buffer too short");
        }
        if &bytes[0..8] != Self::JOURNAL_MAGIC {
            return Err("Invalid recovery journal magic header");
        }
        let payload_end = bytes.len() - 4;
        let expected_crc = u32::from_be_bytes([
            bytes[payload_end],
            bytes[payload_end + 1],
            bytes[payload_end + 2],
            bytes[payload_end + 3],
        ]);
        if compute_crc32(&bytes[..payload_end]) != expected_crc {
            return Err("Recovery journal CRC32 verification failed");
        }

        let crc = u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
        let rom_size = usize::try_from(u64::from_be_bytes([
            bytes[12], bytes[13], bytes[14], bytes[15], bytes[16], bytes[17], bytes[18], bytes[19],
        ]))
        .map_err(|_| "Checkpoint total_rom_size overflows platform usize")?;
        let sector_idx = usize::try_from(u64::from_be_bytes([
            bytes[20], bytes[21], bytes[22], bytes[23], bytes[24], bytes[25], bytes[26], bytes[27],
        ]))
        .map_err(|_| "Checkpoint active_sector_index overflows platform usize")?;
        let bytes_written = usize::try_from(u64::from_be_bytes([
            bytes[28], bytes[29], bytes[30], bytes[31], bytes[32], bytes[33], bytes[34], bytes[35],
        ]))
        .map_err(|_| "Checkpoint bytes_written overflows platform usize")?;
        if bytes_written > rom_size {
            return Err("Checkpoint bytes_written exceeds total_rom_size");
        }
        let stage = match bytes[36] {
            0 => JournalStage::Initialized,
            1 => JournalStage::ErasingSectors,
            2 => JournalStage::WritingBlocks,
            3 => JournalStage::VerifyingChecksum,
            4 => JournalStage::FlashCompleted,
            5 => JournalStage::InterruptedAbnormally,
            _ => return Err("Invalid recovery journal stage byte"),
        };
        let timestamp_epoch_secs = u64::from_be_bytes([
            bytes[37], bytes[38], bytes[39], bytes[40], bytes[41], bytes[42], bytes[43], bytes[44],
        ]);
        let id_len = u32::from_be_bytes([bytes[45], bytes[46], bytes[47], bytes[48]]) as usize;
        let id_end = 49usize
            .checked_add(id_len)
            .ok_or("Recovery journal ECM ID length overflow")?;
        if id_end != payload_end {
            return Err("Recovery journal ECM ID length mismatch");
        }
        let stored_ecm_id = std::str::from_utf8(&bytes[49..id_end])
            .map_err(|_| "Invalid UTF-8 in recovery journal ECM ID")?;
        if stored_ecm_id != target_ecm_id {
            return Err("Recovery journal target_ecm_id mismatch");
        }

        let mut mgr = Self {
            checkpoint: FlashRecoveryCheckpoint {
                target_ecm_id: stored_ecm_id.to_string(),
                target_rom_crc32: crc,
                total_rom_size: rom_size,
                active_sector_index: sector_idx,
                bytes_written,
                stage,
                timestamp_epoch_secs,
            },
            is_recovery_mode_active: false,
        };
        mgr.is_recovery_mode_active = mgr.is_recovery_required();
        Ok(mgr)
    }
}
