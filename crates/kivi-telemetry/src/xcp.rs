//! Ported from `ecu_engine_rust::telemetry::xcp`.
//!
//! Pure parsing/analysis logic. No socket, no serial port, no device access.
//! Verified by unit tests with hand-derived expectations; **not** validated
//! against live hardware.

/// Standard XCP Command Codes (CRO Packet Codes).
pub mod xcp_cmd {
    pub const CONNECT: u8 = 0xFF;
    pub const DISCONNECT: u8 = 0xFE;
    pub const GET_STATUS: u8 = 0xFD;
    pub const SYNCH: u8 = 0xFC;
    pub const GET_COMM_MODE_INFO: u8 = 0xFB;
    pub const SET_MTA: u8 = 0xF6;
    pub const UPLOAD: u8 = 0xF5;
    pub const SHORT_UPLOAD: u8 = 0xF4;
    pub const DOWNLOAD: u8 = 0xF0;
    pub const START_STOP_XCP_SESSION: u8 = 0xDF;
    pub const SET_DAQ_PTR: u8 = 0xE2;
    pub const WRITE_DAQ: u8 = 0xE1;
    pub const START_STOP_DAQ_LIST: u8 = 0xDE;
    pub const START_STOP_SYNCH: u8 = 0xDD;
}

/// XCP Response Packet Identifiers (DTO Codes).
pub mod xcp_resp {
    pub const POSITIVE_ACK: u8 = 0xFF;
    pub const ERROR_PACKET: u8 = 0xFE;
    pub const EVENT_PACKET: u8 = 0xFD;
    pub const SERVICE_PACKET: u8 = 0xFC;
}

/// XCP Error Codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XcpErrorCode {
    CmdSynch,
    CmdBusy,
    DaqActive,
    PgmActive,
    CmdUnknown,
    CmdSyntax,
    OutOfRange,
    WriteProtected,
    AccessDenied,
    AccessLocked,
    PageNotValid,
    ModeNotValid,
    SegmentNotValid,
    Unknown(u8),
}

impl From<u8> for XcpErrorCode {
    fn from(code: u8) -> Self {
        match code {
            0x00 => Self::CmdSynch,
            0x10 => Self::CmdBusy,
            0x11 => Self::DaqActive,
            0x12 => Self::PgmActive,
            0x20 => Self::CmdUnknown,
            0x21 => Self::CmdSyntax,
            0x22 => Self::OutOfRange,
            0x23 => Self::WriteProtected,
            0x24 => Self::AccessDenied,
            0x25 => Self::AccessLocked,
            0x26 => Self::PageNotValid,
            0x27 => Self::ModeNotValid,
            0x28 => Self::SegmentNotValid,
            other => Self::Unknown(other),
        }
    }
}

/// ASAM XCP Session State.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XcpSessionState {
    Disconnected,
    Connected,
    MemoryTransferAddressSet(u32),
    DaqRunning,
}

/// ASAM XCP on CAN Protocol Handler.
pub struct XcpSession {
    pub state: XcpSessionState,
    pub max_cto_size: usize,
    pub max_dto_size: usize,
    pub is_little_endian: bool,
}

impl Default for XcpSession {
    fn default() -> Self {
        Self {
            state: XcpSessionState::Disconnected,
            max_cto_size: 8, // Standard CAN frame payload limit
            max_dto_size: 8,
            is_little_endian: true,
        }
    }
}

impl XcpSession {
    /// Build XCP `CONNECT` command frame.
    pub fn build_connect_cmd(&self) -> Vec<u8> {
        vec![xcp_cmd::CONNECT, 0x00] // Mode 0 = standard connect
    }

    /// Build XCP `SET_MTA` (Set Memory Transfer Address) command frame.
    pub fn build_set_mta_cmd(&self, address: u32, address_extension: u8) -> Vec<u8> {
        let mut frame = vec![xcp_cmd::SET_MTA, 0x00, 0x00, address_extension];
        let addr_bytes = if self.is_little_endian {
            address.to_le_bytes()
        } else {
            address.to_be_bytes()
        };
        frame.extend_from_slice(&addr_bytes);
        frame
    }

    /// Build XCP `DOWNLOAD` command frame to modify live ECU RAM data.
    pub fn build_download_cmd(&self, data_to_write: &[u8]) -> Result<Vec<u8>, &'static str> {
        if data_to_write.is_empty() {
            return Err("Data to write cannot be empty");
        }
        if self.max_cto_size < 3 {
            return Err("XCP max_cto_size must be at least 3 for DOWNLOAD");
        }
        let max_data = (self.max_cto_size - 2).min(u8::MAX as usize);
        let chunk_len = data_to_write.len().min(max_data);

        let mut frame = vec![xcp_cmd::DOWNLOAD, chunk_len as u8];
        frame.extend_from_slice(&data_to_write[..chunk_len]);
        Ok(frame)
    }

    /// Build XCP `SHORT_UPLOAD` command frame to read live RAM without pre-setting MTA.
    pub fn build_short_upload_cmd(&self, num_bytes: u8, address: u32, address_extension: u8) -> Vec<u8> {
        let mut frame = vec![xcp_cmd::SHORT_UPLOAD, num_bytes, 0x00, address_extension];
        let addr_bytes = if self.is_little_endian {
            address.to_le_bytes()
        } else {
            address.to_be_bytes()
        };
        frame.extend_from_slice(&addr_bytes);
        frame
    }

    /// Process inbound XCP slave response packet from ECU.
    pub fn process_response(&mut self, response: &[u8]) -> Result<Vec<u8>, String> {
        if response.is_empty() {
            return Err("Empty XCP response received".to_string());
        }

        match response[0] {
            xcp_resp::POSITIVE_ACK => {
                // Command executed successfully
                if self.state == XcpSessionState::Disconnected {
                    self.state = XcpSessionState::Connected;
                }
                // Return payload data (excluding PID 0xFF)
                Ok(response[1..].to_vec())
            }
            xcp_resp::ERROR_PACKET => {
                let err_code = if response.len() >= 2 {
                    XcpErrorCode::from(response[1])
                } else {
                    XcpErrorCode::Unknown(0xFF)
                };
                Err(format!("XCP Error Packet: {:?}", err_code))
            }
            other => Err(format!("Unknown XCP Packet Identifier: 0x{:02X}", other)),
        }
    }
}
