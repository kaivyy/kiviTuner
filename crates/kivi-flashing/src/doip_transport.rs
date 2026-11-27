//! Ported from `ecu_engine_rust::flashing::doip_transport`.
//!
//! Pure byte-level protocol logic: builders, parsers, encoders, state machines.
//! No I/O, no device access, no `unsafe`. Transport is the caller's concern.
//!
//! Verified by unit tests with hand-derived expectations. **Not** validated
//! against a physical ECU; no such claim is made.

/// Standard ISO 13400-2 Payload Types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoIpPayloadType {
    GenericHeaderNack,
    VehicleIdentificationRequest,
    VehicleIdentificationRequestWithEid,
    VehicleIdentificationRequestWithVin,
    VehicleIdentificationResponse,
    RoutingActivationRequest,
    RoutingActivationResponse,
    AliveCheckRequest,
    AliveCheckResponse,
    DiagnosticMessage,
    DiagnosticMessagePositiveAck,
    DiagnosticMessageNegativeAck,
    Unknown(u16),
}

impl From<u16> for DoIpPayloadType {
    fn from(code: u16) -> Self {
        match code {
            0x0000 => Self::GenericHeaderNack,
            0x0001 => Self::VehicleIdentificationRequest,
            0x0002 => Self::VehicleIdentificationRequestWithEid,
            0x0003 => Self::VehicleIdentificationRequestWithVin,
            0x0004 => Self::VehicleIdentificationResponse,
            0x0005 => Self::RoutingActivationRequest,
            0x0006 => Self::RoutingActivationResponse,
            0x0007 => Self::AliveCheckRequest,
            0x0008 => Self::AliveCheckResponse,
            0x8001 => Self::DiagnosticMessage,
            0x8002 => Self::DiagnosticMessagePositiveAck,
            0x8003 => Self::DiagnosticMessageNegativeAck,
            other => Self::Unknown(other),
        }
    }
}

/// DoIP Generic Header (8 bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoIpMessage {
    pub protocol_version: u8,
    pub payload_type: DoIpPayloadType,
    pub payload: Vec<u8>,
}

impl DoIpMessage {
    pub const DEFAULT_PROTOCOL_VERSION: u8 = 0x02; // ISO 13400-2:2012
    pub const DOIP_TCP_PORT: u16 = 13400;

    pub fn new(payload_type: DoIpPayloadType, payload: Vec<u8>) -> Self {
        Self {
            protocol_version: Self::DEFAULT_PROTOCOL_VERSION,
            payload_type,
            payload,
        }
    }

    /// Serialize message to 8-byte header + payload wire format.
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(8 + self.payload.len());
        bytes.push(self.protocol_version);
        bytes.push(!self.protocol_version); // Inverse protocol version (byte 1)
        
        let type_u16 = match self.payload_type {
            DoIpPayloadType::GenericHeaderNack => 0x0000,
            DoIpPayloadType::VehicleIdentificationRequest => 0x0001,
            DoIpPayloadType::VehicleIdentificationRequestWithEid => 0x0002,
            DoIpPayloadType::VehicleIdentificationRequestWithVin => 0x0003,
            DoIpPayloadType::VehicleIdentificationResponse => 0x0004,
            DoIpPayloadType::RoutingActivationRequest => 0x0005,
            DoIpPayloadType::RoutingActivationResponse => 0x0006,
            DoIpPayloadType::AliveCheckRequest => 0x0007,
            DoIpPayloadType::AliveCheckResponse => 0x0008,
            DoIpPayloadType::DiagnosticMessage => 0x8001,
            DoIpPayloadType::DiagnosticMessagePositiveAck => 0x8002,
            DoIpPayloadType::DiagnosticMessageNegativeAck => 0x8003,
            DoIpPayloadType::Unknown(val) => val,
        };
        bytes.extend_from_slice(&type_u16.to_be_bytes());
        bytes.extend_from_slice(&(self.payload.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&self.payload);
        bytes
    }

    /// Parse complete wire frame into DoIP message.
    pub fn decode(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() < 8 {
            return Err("DoIP frame too short (minimum 8 bytes header required)");
        }

        let ver = bytes[0];
        let inv_ver = bytes[1];
        if (ver ^ inv_ver) != 0xFF {
            return Err("Invalid DoIP header: inverse protocol version mismatch");
        }

        let type_code = u16::from_be_bytes([bytes[2], bytes[3]]);
        let payload_len = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;

        let Some(end) = 8usize.checked_add(payload_len) else {
            return Err("DoIP payload length overflows platform address space");
        };
        if bytes.len() < end {
            return Err("DoIP payload shorter than header length field");
        }

        let payload = bytes[8..end].to_vec();
        Ok(Self {
            protocol_version: ver,
            payload_type: DoIpPayloadType::from(type_code),
            payload,
        })
    }
}

/// DoIP Client Session & State Machine.
pub struct DoIpSession {
    pub tester_logical_address: u16,
    pub target_ecu_logical_address: u16,
    pub is_routing_activated: bool,
}

impl DoIpSession {
    pub const DEFAULT_TESTER_ADDRESS: u16 = 0x0E00; // Standard External Test Equipment
    pub const DEFAULT_ENGINE_ECU_ADDRESS: u16 = 0x0001; // Primary Powertrain Controller

    pub fn new(tester_addr: u16, target_ecu_addr: u16) -> Self {
        Self {
            tester_logical_address: tester_addr,
            target_ecu_logical_address: target_ecu_addr,
            is_routing_activated: false,
        }
    }

    /// Build Routing Activation Request (Payload Type 0x0005).
    pub fn build_routing_activation_request(&self) -> DoIpMessage {
        let mut payload = Vec::with_capacity(7);
        payload.extend_from_slice(&self.tester_logical_address.to_be_bytes());
        payload.push(0x00); // Activation Type: 0x00 = Default
        payload.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // 4 bytes reserved
        DoIpMessage::new(DoIpPayloadType::RoutingActivationRequest, payload)
    }

    /// Ingest and parse Routing Activation Response (Payload Type 0x0006).
    pub fn process_routing_activation_response(&mut self, msg: &DoIpMessage) -> Result<(), &'static str> {
        if msg.payload_type != DoIpPayloadType::RoutingActivationResponse {
            return Err("Expected RoutingActivationResponse payload");
        }
        if msg.payload.len() < 5 {
            return Err("RoutingActivationResponse payload too short");
        }

        let response_code = msg.payload[4];
        if response_code == 0x10 {
            // 0x10 = Routing successfully activated
            self.is_routing_activated = true;
            Ok(())
        } else {
            Err("Routing activation denied by DoIP gateway")
        }
    }

    /// Wrap raw UDS diagnostic message bytes into a DoIP DiagnosticMessage (Payload Type 0x8001).
    pub fn build_diagnostic_message(&self, uds_bytes: &[u8]) -> Result<DoIpMessage, &'static str> {
        if !self.is_routing_activated {
            return Err("Cannot send diagnostic message: DoIP routing not activated");
        }

        let mut payload = Vec::with_capacity(4 + uds_bytes.len());
        payload.extend_from_slice(&self.tester_logical_address.to_be_bytes());
        payload.extend_from_slice(&self.target_ecu_logical_address.to_be_bytes());
        payload.extend_from_slice(uds_bytes);

        Ok(DoIpMessage::new(DoIpPayloadType::DiagnosticMessage, payload))
    }

    /// Unwrap received DoIP DiagnosticMessage into raw UDS diagnostic response bytes.
    pub fn unwrap_diagnostic_message(&self, msg: &DoIpMessage) -> Result<Vec<u8>, &'static str> {
        if msg.payload_type != DoIpPayloadType::DiagnosticMessage {
            return Err("Expected DoIP DiagnosticMessage payload");
        }
        if msg.payload.len() < 4 {
            return Err("DiagnosticMessage payload shorter than 4-byte address header");
        }

        let target = u16::from_be_bytes([msg.payload[2], msg.payload[3]]);
        if target != self.tester_logical_address {
            return Err("DiagnosticMessage target address does not match tester address");
        }

        Ok(msg.payload[4..].to_vec())
    }
}
