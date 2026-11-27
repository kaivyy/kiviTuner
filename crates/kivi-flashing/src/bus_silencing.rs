//! Ported from `ecu_engine_rust::flashing::bus_silencing`.
//!
//! Pure byte-level protocol logic: builders, parsers, encoders, state machines.
//! No I/O, no device access, no `unsafe`. Transport is the caller's concern.
//!
//! Verified by unit tests with hand-derived expectations. **Not** validated
//! against a physical ECU; no such claim is made.

/// UDS Service 0x85 ControlDTCSetting Types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DtcSettingType {
    On = 0x01,
    Off = 0x02,
}

/// UDS Service 0x28 CommunicationControl Subfunctions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommunicationControlType {
    EnableRxAndTx = 0x00,
    EnableRxAndDisableTx = 0x01,
    DisableRxAndEnableTx = 0x02,
    DisableRxAndTx = 0x03,
}

/// Pre-Flash and Post-Flash Bus Management Interlock Controller.
pub struct BusSilencingInterlock;

impl BusSilencingInterlock {
    /// Build UDS Service 0x85 ControlDTCSetting frame.
    /// `[0x85, setting_type]`
    pub fn build_control_dtc_setting(setting: DtcSettingType) -> Vec<u8> {
        vec![0x85, setting as u8]
    }

    /// Build UDS Service 0x28 CommunicationControl frame.
    /// `[0x28, control_type, communication_type]`
    /// Communication type 0x01 = normal inter-ECU application messages.
    pub fn build_communication_control(
        control: CommunicationControlType,
        comm_type: u8,
    ) -> Vec<u8> {
        vec![0x28, control as u8, comm_type]
    }

    /// Build UDS Service 0x14 ClearDiagnosticInformation frame.
    /// Group of DTCs `0xFFFFFF` = Clear all emission and non-emission fault codes.
    pub fn build_clear_all_dtcs() -> Vec<u8> {
        vec![0x14, 0xFF, 0xFF, 0xFF]
    }

    /// Build UDS Service 0x19 ReadDTCInformation frame (Subfunction 0x02: ReportDTCByStatusMask).
    /// Mask 0x08 = Pending DTCs, Mask 0x09 = Confirmed + Pending DTCs.
    pub fn build_read_dtc_by_status_mask(mask: u8) -> Vec<u8> {
        vec![0x19, 0x02, mask]
    }

    /// Complete pre-flash sequence:
    /// 1. Disable DTC recording across all vehicle modules (`0x85 0x02`).
    /// 2. Silence normal chatter on CAN bus (`0x28 0x03 0x01`).
    pub fn build_pre_flash_silencing_sequence() -> Vec<Vec<u8>> {
        vec![
            Self::build_control_dtc_setting(DtcSettingType::Off),
            Self::build_communication_control(CommunicationControlType::DisableRxAndTx, 0x01),
        ]
    }

    /// Complete post-flash restoration sequence:
    /// 1. Re-enable network messages (`0x28 0x00 0x01`).
    /// 2. Re-enable DTC recording (`0x85 0x01`).
    /// 3. Clear residual flashing-induced communication timeout DTCs (`0x14 0xFF 0xFF 0xFF`).
    pub fn build_post_flash_restoration_sequence() -> Vec<Vec<u8>> {
        vec![
            Self::build_communication_control(CommunicationControlType::EnableRxAndTx, 0x01),
            Self::build_control_dtc_setting(DtcSettingType::On),
            Self::build_clear_all_dtcs(),
        ]
    }

    /// Parse UDS Service 0x19 0x02 response and extract list of 3-byte DTC codes.
    pub fn parse_dtc_list_response(response: &[u8]) -> Result<Vec<(String, u8)>, &'static str> {
        if response.len() < 3 || response[0] != 0x59 || response[1] != 0x02 {
            return Err("Invalid UDS ReadDTCInformation positive response");
        }
        let dtc_bytes = &response[3..];
        let mut results = Vec::new();

        for chunk in dtc_bytes.chunks_exact(4) {
            let b0 = chunk[0];
            let b1 = chunk[1];
            let _b2 = chunk[2];
            let status = chunk[3];

            let prefix = match b0 >> 6 {
                0 => 'P',
                1 => 'C',
                2 => 'B',
                3 => 'U',
                _ => 'P',
            };
            let code_str = format!(
                "{}{:01X}{:01X}{:01X}{:01X}",
                prefix,
                (b0 >> 4) & 0x03,
                b0 & 0x0F,
                (b1 >> 4) & 0x0F,
                b1 & 0x0F
            );
            results.push((code_str, status));
        }

        Ok(results)
    }
}
