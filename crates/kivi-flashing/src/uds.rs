//! Ported from `ecu_engine_rust::flashing::uds`.
//!
//! Pure byte-level protocol logic: builders, parsers, encoders, state machines.
//! No I/O, no device access, no `unsafe`. Transport is the caller's concern.
//!
//! Verified by unit tests with hand-derived expectations. **Not** validated
//! against a physical ECU; no such claim is made.

/// UDS Standard Service Identifiers (SID).
pub mod uds_sid {
    pub const DIAGNOSTIC_SESSION_CONTROL: u8 = 0x10;
    pub const ECU_RESET: u8 = 0x11;
    pub const SECURITY_ACCESS: u8 = 0x27;
    pub const COMMUNICATION_CONTROL: u8 = 0x28;
    pub const TESTER_PRESENT: u8 = 0x3E;
    pub const CONTROL_DTC_SETTING: u8 = 0x85;
    pub const ROUTINE_CONTROL: u8 = 0x31;
    pub const REQUEST_DOWNLOAD: u8 = 0x34;
    pub const TRANSFER_DATA: u8 = 0x36;
    pub const REQUEST_TRANSFER_EXIT: u8 = 0x37;
    pub const NEGATIVE_RESPONSE: u8 = 0x7F;
}

/// UDS Negative Response Codes (NRC).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UdsNegativeResponseCode {
    GeneralReject,
    ServiceNotSupported,
    SubFunctionNotSupported,
    IncorrectMessageLength,
    ConditionsNotCorrect,
    RequestSequenceError,
    RequestOutOfRange,
    SecurityAccessDenied,
    InvalidKey,
    ExceededNumberOfAttempts,
    RequiredTimeDelayNotExpired,
    GeneralProgrammingFailure,
    WrongBlockSequenceCounter,
    Unknown(u8),
}

impl From<u8> for UdsNegativeResponseCode {
    fn from(code: u8) -> Self {
        match code {
            0x10 => Self::GeneralReject,
            0x11 => Self::ServiceNotSupported,
            0x12 => Self::SubFunctionNotSupported,
            0x13 => Self::IncorrectMessageLength,
            0x22 => Self::ConditionsNotCorrect,
            0x24 => Self::RequestSequenceError,
            0x31 => Self::RequestOutOfRange,
            0x33 => Self::SecurityAccessDenied,
            0x35 => Self::InvalidKey,
            0x36 => Self::ExceededNumberOfAttempts,
            0x37 => Self::RequiredTimeDelayNotExpired,
            0x72 => Self::GeneralProgrammingFailure,
            0x73 => Self::WrongBlockSequenceCounter,
            other => Self::Unknown(other),
        }
    }
}

/// Flashing Protocol State Machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlashingState {
    Idle,
    ExtendedSessionActive,
    DtcDisabled,
    CommunicationDisabled,
    ProgrammingSessionActive,
    SecurityKeyPending(u32),
    SecurityUnlocked,
    MemoryErased,
    TransferringData {
        block_counter: u8,
        bytes_transferred: usize,
        total_bytes: usize,
    },
    TransferExitPending,
    TransferExited,
    ChecksumVerified,
    Completed,
    Failed(UdsNegativeResponseCode),
}

/// Standard Seed-Key Algorithm (Bit-Rotation and Polynomial Mask).
pub struct SeedKeyAlgorithm {
    pub secret_mask: u32,
    pub rotate_bits: u32,
}

impl Default for SeedKeyAlgorithm {
    fn default() -> Self {
        // Typical automotive OEM mask (e.g. Bosch ME7 / Delphi)
        Self {
            secret_mask: 0x48A35912,
            rotate_bits: 5,
        }
    }
}

impl SeedKeyAlgorithm {
    /// Compute response key from ECU seed challenge.
    pub fn compute_key(&self, seed: u32) -> u32 {
        if seed == 0 {
            return 0; // Seed of 0 indicates security is already unlocked
        }
        let rotated = seed.rotate_left(self.rotate_bits);
        rotated ^ self.secret_mask
    }
}

/// Comprehensive UDS Flashing Orchestrator.
pub struct UdsFlashingSession {
    pub state: FlashingState,
    pub seed_key: SeedKeyAlgorithm,
    pub max_block_size: usize,
    pub target_memory_address: u32,
    pub firmware_payload: Vec<u8>,
}

impl UdsFlashingSession {
    pub fn new(firmware_bytes: Vec<u8>, target_address: u32) -> Self {
        Self {
            state: FlashingState::Idle,
            seed_key: SeedKeyAlgorithm::default(),
            max_block_size: 4095, // Default maximum ISO-TP CAN block length
            target_memory_address: target_address,
            firmware_payload: firmware_bytes,
        }
    }

    /// Build the next outbound request frame to transmit to the vehicle ECU.
    pub fn build_next_request(&mut self) -> Result<Vec<u8>, &'static str> {
        match self.state {
            FlashingState::Idle => {
                // Step 1: Request Extended Diagnostic Session (0x10, 0x03)
                Ok(vec![uds_sid::DIAGNOSTIC_SESSION_CONTROL, 0x03])
            }
            FlashingState::ExtendedSessionActive => {
                // Step 2: Disable DTC setting during flash (0x85, 0x02)
                Ok(vec![uds_sid::CONTROL_DTC_SETTING, 0x02])
            }
            FlashingState::DtcDisabled => {
                // Step 3: Disable normal communication on CAN bus (0x28, 0x03, 0x01)
                Ok(vec![uds_sid::COMMUNICATION_CONTROL, 0x03, 0x01])
            }
            FlashingState::CommunicationDisabled => {
                // Step 4: Request Programming Session (0x10, 0x02) -> Bootloader entry
                Ok(vec![uds_sid::DIAGNOSTIC_SESSION_CONTROL, 0x02])
            }
            FlashingState::ProgrammingSessionActive => {
                // Step 5a: Request Security Access Seed (0x27, 0x01)
                Ok(vec![uds_sid::SECURITY_ACCESS, 0x01])
            }
            FlashingState::SecurityKeyPending(key) => {
                // Step 5b: Send Security Access Key (0x27, 0x02, key_be[4])
                let mut frame = vec![uds_sid::SECURITY_ACCESS, 0x02];
                frame.extend_from_slice(&key.to_be_bytes());
                Ok(frame)
            }
            FlashingState::SecurityUnlocked => {
                // Step 6: Start Routine Erase Memory (0x31, 0x01, 0xFF, 0x00)
                let total_len = self.firmware_payload.len() as u32;
                let mut frame = vec![uds_sid::ROUTINE_CONTROL, 0x01, 0xFF, 0x00, 0x44];
                frame.extend_from_slice(&self.target_memory_address.to_be_bytes());
                frame.extend_from_slice(&total_len.to_be_bytes());
                Ok(frame)
            }
            FlashingState::MemoryErased => {
                // Step 7: Request Download (0x34, dataFormat=0x00, 4-byte addr, 4-byte size)
                let total_len = self.firmware_payload.len() as u32;
                let mut frame = vec![uds_sid::REQUEST_DOWNLOAD, 0x00, 0x44];
                frame.extend_from_slice(&self.target_memory_address.to_be_bytes());
                frame.extend_from_slice(&total_len.to_be_bytes());
                Ok(frame)
            }
            FlashingState::TransferringData { block_counter, bytes_transferred, total_bytes } => {
                if bytes_transferred >= total_bytes {
                    Ok(vec![uds_sid::REQUEST_TRANSFER_EXIT])
                } else {
                    if self.max_block_size < 3 {
                        return Err("ECU maxNumberOfBlockLength too small (< 3)");
                    }
                    // Send next data block (0x36, blockCounter, bytes...)
                    let remaining = total_bytes - bytes_transferred;
                    let chunk_len = remaining.min(self.max_block_size - 2);
                    let mut frame = vec![uds_sid::TRANSFER_DATA, block_counter];
                    frame.extend_from_slice(
                        &self.firmware_payload[bytes_transferred..bytes_transferred + chunk_len],
                    );
                    Ok(frame)
                }
            }
            FlashingState::TransferExitPending => {
                // Step 8: Complete download block via RequestTransferExit (0x37)
                Ok(vec![uds_sid::REQUEST_TRANSFER_EXIT])
            }
            FlashingState::TransferExited => {
                // Step 9: Verify memory checksum via RoutineControl (0x31, 0x01, 0x02, 0x02)
                Ok(vec![uds_sid::ROUTINE_CONTROL, 0x01, 0x02, 0x02])
            }
            FlashingState::ChecksumVerified => {
                // Step 10: Hard ECU Reset to reboot into new firmware (0x11, 0x01)
                Ok(vec![uds_sid::ECU_RESET, 0x01])
            }
            FlashingState::Completed => Err("Flashing already completed"),
            FlashingState::Failed(_) => Err("Flashing session has failed"),
        }
    }

    /// Process inbound response frame received from vehicle ECU.
    pub fn process_response(&mut self, response: &[u8]) -> Result<(), String> {
        if response.is_empty() {
            return Err("Empty response received".to_string());
        }

        // Check for Negative Response (0x7F)
        if response[0] == uds_sid::NEGATIVE_RESPONSE {
            if response.len() >= 3 {
                if response[2] == 0x78 {
                    // ISO 14229-1 §7.5.5: requestCorrectlyReceived-ResponsePending
                    return Ok(());
                }
                let nrc = UdsNegativeResponseCode::from(response[2]);
                self.state = FlashingState::Failed(nrc);
                return Err(format!("ECU Negative Response: Service 0x{:02X}, NRC {:?}", response[1], nrc));
            }
            return Err("Malformed Negative Response".to_string());
        }

        // Handle Positive Responses (Response SID = Request SID + 0x40)
        match (self.state, response[0]) {
            (FlashingState::Idle, 0x50) => {
                self.state = FlashingState::ExtendedSessionActive;
                Ok(())
            }
            (FlashingState::ExtendedSessionActive, 0xC5) => {
                self.state = FlashingState::DtcDisabled;
                Ok(())
            }
            (FlashingState::DtcDisabled, 0x68) => {
                self.state = FlashingState::CommunicationDisabled;
                Ok(())
            }
            (FlashingState::CommunicationDisabled, 0x50) => {
                self.state = FlashingState::ProgrammingSessionActive;
                Ok(())
            }
            (FlashingState::ProgrammingSessionActive, 0x67) => {
                // Seed received: [ 0x67, 0x01, Seed0, Seed1, Seed2, Seed3 ]
                if response.len() >= 6 && response[1] == 0x01 {
                    let seed = u32::from_be_bytes([response[2], response[3], response[4], response[5]]);
                    let key = self.seed_key.compute_key(seed);
                    self.state = FlashingState::SecurityKeyPending(key);
                    Ok(())
                } else {
                    Err("Invalid seed length or sub-function from ECU".to_string())
                }
            }
            (FlashingState::SecurityKeyPending(_), 0x67) => {
                if response.len() >= 2 && response[1] == 0x02 {
                    self.state = FlashingState::SecurityUnlocked;
                    Ok(())
                } else {
                    Err("Invalid SecurityAccess key confirmation response".to_string())
                }
            }
            (FlashingState::SecurityUnlocked, 0x71) => {
                // Erase routine confirmed
                self.state = FlashingState::MemoryErased;
                Ok(())
            }
            (FlashingState::MemoryErased, 0x74) => {
                // RequestDownload confirmed: parse lengthFormatIdentifier & maxNumberOfBlockLength
                if response.len() >= 3 {
                    let len_bytes = ((response[1] >> 4) & 0x0F) as usize;
                    if (1..=4).contains(&len_bytes) && response.len() >= 2 + len_bytes {
                        let mut max_len: usize = 0;
                        for &b in &response[2..2 + len_bytes] {
                            max_len = (max_len << 8) | (b as usize);
                        }
                        self.max_block_size = max_len.clamp(3, 4095);
                    }
                }
                let total = self.firmware_payload.len();
                if total == 0 {
                    self.state = FlashingState::TransferExitPending;
                } else {
                    self.state = FlashingState::TransferringData {
                        block_counter: 1,
                        bytes_transferred: 0,
                        total_bytes: total,
                    };
                }
                Ok(())
            }
            (FlashingState::TransferringData { block_counter, bytes_transferred, total_bytes }, 0x76) => {
                if response.len() < 2 || response[1] != block_counter {
                    return Err(format!(
                        "TransferData block sequence mismatch: expected 0x{:02X}, got {:?}",
                        block_counter,
                        response.get(1)
                    ));
                }
                if self.max_block_size < 3 {
                    return Err("ECU maxNumberOfBlockLength too small (< 3)".to_string());
                }
                let remaining = total_bytes - bytes_transferred;
                let chunk_len = remaining.min(self.max_block_size - 2);
                let new_transferred = bytes_transferred + chunk_len;
                let next_counter = block_counter.wrapping_add(1);

                if new_transferred >= total_bytes {
                    self.state = FlashingState::TransferExitPending;
                } else {
                    self.state = FlashingState::TransferringData {
                        block_counter: next_counter,
                        bytes_transferred: new_transferred,
                        total_bytes,
                    };
                }
                Ok(())
            }
            (FlashingState::TransferExitPending, 0x77) => {
                self.state = FlashingState::TransferExited;
                Ok(())
            }
            (FlashingState::TransferExited, 0x71) => {
                self.state = FlashingState::ChecksumVerified;
                Ok(())
            }
            (FlashingState::ChecksumVerified, 0x51) => {
                self.state = FlashingState::Completed;
                Ok(())
            }
            _ => Err(format!("Unexpected response 0x{:02X} for state {:?}", response[0], self.state)),
        }
    }

    /// Progress ratio of the firmware flashing process (0.0 to 1.0) for UI display.
    pub fn progress_ratio(&self) -> f64 {
        match self.state {
            FlashingState::Idle => 0.0,
            FlashingState::ExtendedSessionActive => 0.05,
            FlashingState::DtcDisabled => 0.08,
            FlashingState::CommunicationDisabled => 0.12,
            FlashingState::ProgrammingSessionActive => 0.15,
            FlashingState::SecurityKeyPending(_) => 0.18,
            FlashingState::SecurityUnlocked => 0.20,
            FlashingState::MemoryErased => 0.25,
            FlashingState::TransferringData { bytes_transferred, total_bytes, .. } => {
                if total_bytes == 0 {
                    0.25
                } else {
                    0.25 + 0.65 * (bytes_transferred as f64 / total_bytes as f64)
                }
            }
            FlashingState::TransferExitPending => 0.90,
            FlashingState::TransferExited => 0.93,
            FlashingState::ChecksumVerified => 0.96,
            FlashingState::Completed => 1.0,
            FlashingState::Failed(_) => 0.0,
        }
    }
}

/// ISO 14229 / ISO 14230 Periodic TesterPresent Keep-Alive Pacer.
/// Prevents S3 timeout (2,000ms - 5,000ms) from reverting ECU to Default Session during long reads or writes.
pub struct TesterPresentPacer {
    pub interval_ms: u32,
    pub last_sent_ms: u32,
    pub suppress_positive_response: bool,
}

impl Default for TesterPresentPacer {
    fn default() -> Self {
        Self {
            interval_ms: 2000, // Send every 2000ms (S3 server timeout is typically 5000ms)
            last_sent_ms: 0,
            suppress_positive_response: true,
        }
    }
}

impl TesterPresentPacer {
    /// Check whether a TesterPresent message must be dispatched right now.
    pub fn is_time_to_send(&self, current_time_ms: u32) -> bool {
        if self.last_sent_ms == 0 {
            true
        } else {
            current_time_ms.saturating_sub(self.last_sent_ms) >= self.interval_ms
        }
    }

    /// Build standard TesterPresent request packet.
    /// [0x3E, 0x80] (Suppress Positive Response) or [0x3E, 0x00] (Expect 0x7E 0x00 Reply).
    pub fn build_tester_present(&mut self, current_time_ms: u32) -> Vec<u8> {
        self.last_sent_ms = current_time_ms;
        let sub_function = if self.suppress_positive_response { 0x80 } else { 0x00 };
        vec![uds_sid::TESTER_PRESENT, sub_function]
    }
}

/// Standard ISO 14229 Data Identifiers (DID) for ECU Identification.
pub mod uds_did {
    pub const BOOT_SOFTWARE_ID: u16 = 0xF180;
    pub const OEM_SPARE_PART_NUMBER: u16 = 0xF187;
    pub const OEM_SOFTWARE_NUMBER: u16 = 0xF188;
    pub const CALIBRATION_SOFTWARE_ID: u16 = 0xF189;
    pub const VIN_DATA_IDENTIFIER: u16 = 0xF190;
    pub const ECU_HARDWARE_NUMBER: u16 = 0xF191;
    pub const SYSTEM_NAME: u16 = 0xF197;
}

/// Parsed ECU Identity Information.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EcuIdentityInfo {
    pub vin: Option<String>,
    pub spare_part_number: Option<String>,
    pub calibration_sw_id: Option<String>,
    pub ecu_hardware_number: Option<String>,
    pub system_name: Option<String>,
}

pub struct EcuIdentificationParser;

impl EcuIdentificationParser {
    /// Build ReadDataByIdentifier (Service 0x22) request packet for given DID.
    pub fn build_read_did_request(did: u16) -> Vec<u8> {
        vec![0x22, (did >> 8) as u8, (did & 0xFF) as u8]
    }

    /// Parse Positive Response (0x62 DID_MSB DID_LSB [DATA...]) and update identity struct.
    pub fn parse_did_response(response: &[u8], info: &mut EcuIdentityInfo) -> Result<u16, &'static str> {
        if response.len() < 3 {
            return Err("Response too short for ReadDataByIdentifier");
        }
        if response[0] != 0x62 {
            return Err("Not a positive ReadDataByIdentifier response (expected 0x62)");
        }

        let did = u16::from_be_bytes([response[1], response[2]]);
        let payload = &response[3..];
        let text = String::from_utf8_lossy(payload).trim().to_string();

        match did {
            uds_did::VIN_DATA_IDENTIFIER => info.vin = Some(text),
            uds_did::OEM_SPARE_PART_NUMBER => info.spare_part_number = Some(text),
            uds_did::CALIBRATION_SOFTWARE_ID => info.calibration_sw_id = Some(text),
            uds_did::ECU_HARDWARE_NUMBER => info.ecu_hardware_number = Some(text),
            uds_did::SYSTEM_NAME => info.system_name = Some(text),
            _ => {}
        }

        Ok(did)
    }
}

/// Security Access Attempt Counter & Anti-Lockout Penalty Timer Watchdog.
/// Prevents bricking or permanent lockout of ECU bootloader when wrong seed-keys are tried.
pub struct SecurityLockoutWatchdog {
    pub failed_attempts: usize,
    pub max_allowed_attempts: usize,
    pub lockout_penalty_ms: u32,
    pub lockout_start_time_ms: Option<u32>,
}

impl Default for SecurityLockoutWatchdog {
    fn default() -> Self {
        Self {
            failed_attempts: 0,
            max_allowed_attempts: 3, // OEM limit before 10-minute lockout
            lockout_penalty_ms: 600_000, // 10 minutes = 600,000 ms
            lockout_start_time_ms: None,
        }
    }
}

impl SecurityLockoutWatchdog {
    /// Ingest SecurityAccess response: handle InvalidKey (0x35), ExceededAttempts (0x36), or DelayNotExpired (0x37).
    pub fn handle_security_response(&mut self, nrc: UdsNegativeResponseCode, current_time_ms: u32) -> Result<(), &'static str> {
        match nrc {
            UdsNegativeResponseCode::InvalidKey => {
                self.failed_attempts += 1;
                if self.failed_attempts >= self.max_allowed_attempts {
                    self.lockout_start_time_ms = Some(current_time_ms);
                    return Err("SECURITY_LOCKOUT: 3 failed attempts! Penalty timer engaged. Halt flashing.");
                }
                Ok(())
            }
            UdsNegativeResponseCode::ExceededNumberOfAttempts | UdsNegativeResponseCode::RequiredTimeDelayNotExpired => {
                self.lockout_start_time_ms = Some(current_time_ms);
                Err("SECURITY_LOCKOUT_ACTIVE: ECU security access locked. Wait for penalty timer to expire.")
            }
            _ => Ok(()),
        }
    }

    /// Check remaining penalty lockout time (in seconds) before next attempt is allowed.
    pub fn remaining_lockout_seconds(&self, current_time_ms: u32) -> u32 {
        if let Some(start) = self.lockout_start_time_ms {
            let elapsed = current_time_ms.saturating_sub(start);
            if elapsed < self.lockout_penalty_ms {
                return (self.lockout_penalty_ms - elapsed) / 1000;
            }
        }
        0
    }
}

/// Standard Automotive CAN Diagnostic Addressing Pairs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EcuModuleRole {
    EngineControlModule,       // ECM / PCM: 0x7E0 (Request) -> 0x7E8 (Response)
    TransmissionControlModule, // TCM / Gearbox: 0x7E1 -> 0x7E9
    HybridControlModule,       // HPCM: 0x7E2 -> 0x7EA
    ElectronicBrakeSystem,     // ABS / ESC: 0x7E3 -> 0x7EB
    BodyControlModule,         // BCM / Gateway: 0x7E4 -> 0x7EC
    AirbagSupplemental,        // SRS: 0x7E5 -> 0x7ED
    Unknown(u32),
}

/// CAN Functional Broadcast to Physical Address Resolver.
pub struct FunctionalAddressResolver;

impl FunctionalAddressResolver {
    pub const FUNCTIONAL_11BIT_ID: u32 = 0x7DF;
    pub const FUNCTIONAL_29BIT_ID: u32 = 0x18DB33F1;

    /// Map received physical response CAN ID (0x7E8..0x7EF) back to corresponding ECU role and request ID.
    pub fn resolve_11bit_response(rx_can_id: u32) -> Result<(EcuModuleRole, u32), &'static str> {
        match rx_can_id {
            0x7E8 => Ok((EcuModuleRole::EngineControlModule, 0x7E0)),
            0x7E9 => Ok((EcuModuleRole::TransmissionControlModule, 0x7E1)),
            0x7EA => Ok((EcuModuleRole::HybridControlModule, 0x7E2)),
            0x7EB => Ok((EcuModuleRole::ElectronicBrakeSystem, 0x7E3)),
            0x7EC => Ok((EcuModuleRole::BodyControlModule, 0x7E4)),
            0x7ED => Ok((EcuModuleRole::AirbagSupplemental, 0x7E5)),
            other if (0x7E8..=0x7EF).contains(&other) => Ok((EcuModuleRole::Unknown(other), other - 8)),
            _ => Err("Invalid physical diagnostic response CAN ID"),
        }
    }

    /// Build standard 11-bit functional broadcast ping frame (e.g. Mode 01 PID 00: [0x02, 0x01, 0x00]).
    pub fn build_functional_scan_request() -> (u32, Vec<u8>) {
        (Self::FUNCTIONAL_11BIT_ID, vec![0x02, 0x01, 0x00])
    }
}
