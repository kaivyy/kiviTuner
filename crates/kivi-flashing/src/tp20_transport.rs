//! Ported from `ecu_engine_rust::flashing::tp20_transport`.
//!
//! Pure byte-level protocol logic: builders, parsers, encoders, state machines.
//! No I/O, no device access, no `unsafe`. Transport is the caller's concern.
//!
//! Verified by unit tests with hand-derived expectations. **Not** validated
//! against a physical ECU; no such claim is made.

/// Broadcast CAN ID for TP 2.0 dynamic channel establishment.
pub const TP20_BROADCAST_CAN_ID: u32 = 0x200;

/// Timing multipliers in milliseconds for TP 2.0 timing byte decoding.
pub const TIMING_MULTIPLIERS_MS: [f64; 4] = [0.1, 1.0, 10.0, 100.0];

/// TP 2.0 Frame Type / Opcode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tp20Opcode {
    DataIntermediate = 0x0,
    DataLast = 0x1,
    DataIntermediateAckRequest = 0x2,
    DataLastAckRequest = 0x3,
    ControlParameter = 0xA,
    Acknowledge = 0xB,
}

/// Dynamic Channel Parameters negotiated during connection.
#[derive(Debug, Clone, PartialEq)]
pub struct Tp20ChannelParameters {
    pub block_size: u8,
    pub t1_timeout_ms: f64,
    pub t3_timing_ms: f64,
}

/// Volkswagen Group TP 2.0 Protocol Engine.
pub struct VagTp20Protocol;

impl VagTp20Protocol {
    /// Decode a TP 2.0 single timing byte into milliseconds:
    /// High 2 bits: multiplier index (0.1ms, 1.0ms, 10.0ms, 100.0ms)
    /// Low 6 bits: count (0..63)
    pub fn decode_timing_byte_ms(value: u8) -> f64 {
        let mult_idx = (value >> 6) as usize;
        let count = (value & 0x3F) as f64;
        TIMING_MULTIPLIERS_MS[mult_idx] * count
    }

    /// Build Channel Setup Request frame to broadcast address 0x200.
    /// Wire layout (VAG TP 2.0 little-endian CAN IDs with validity high-nibble):
    /// `[dest_logical, 0xC0, rx_lo(0x00), rx_hi_invalid(0x10), tx_lo, tx_hi_valid, app_type(0x01)]`
    pub fn build_channel_setup_request(
        dest_logical_addr: u8,
        client_rx_can_id: u16,
    ) -> [u8; 7] {
        let tx_lo = (client_rx_can_id & 0xFF) as u8;
        let tx_hi = ((client_rx_can_id >> 8) & 0x07) as u8;
        [
            dest_logical_addr,
            0xC0, // Setup Request opcode
            0x00,
            0x10, // ECU RX ID marked invalid (nibble 0x1) so ECU assigns it
            tx_lo,
            tx_hi, // Client listen / ECU TX target ID (validity 0x0 = valid)
            0x01,  // Application type 0x01 (KWP2000 Diagnostics)
        ]
    }

    /// Parse Channel Setup Positive Response (`0xD0` opcode) into `(ecu_rx_can_id, ecu_tx_can_id, app_type)`.
    pub fn parse_channel_setup_ids(resp: &[u8]) -> Result<(u16, u16, u8), &'static str> {
        if resp.len() < 7 || resp[1] != 0xD0 {
            return Err("Invalid TP 2.0 channel setup response or opcode not 0xD0");
        }
        if (resp[3] & 0x10) != 0 || (resp[5] & 0x10) != 0 {
            return Err("TP 2.0 channel setup response marked CAN ID invalid");
        }
        let ecu_rx_can_id = u16::from_le_bytes([resp[2], resp[3]]) & 0x07FF;
        let ecu_tx_can_id = u16::from_le_bytes([resp[4], resp[5]]) & 0x07FF;
        let app_type = resp[6];
        Ok((ecu_rx_can_id, ecu_tx_can_id, app_type))
    }

    /// Parse Channel Setup Positive Response from ECU (`0xD0` opcode).
    /// Returns `(ecu_tx_can_id, app_type)`.
    pub fn parse_channel_setup_response(resp: &[u8]) -> Result<(u16, u8), &'static str> {
        let (_, ecu_tx_can_id, app_type) = Self::parse_channel_setup_ids(resp)?;
        Ok((ecu_tx_can_id, app_type))
    }

    /// Build Channel Parameters Request (`0xA0`) proposing block size and timeouts.
    pub fn build_channel_parameters_request(block_size: u8) -> [u8; 6] {
        // Standard default: Block size 15, T1 100ms (0x8A), T3 5ms (0x32)
        [0xA0, block_size, 0x8A, 0xFF, 0x32, 0xFF]
    }

    /// Build Acknowledgment (ACK) frame for a received sequence number.
    /// Format: `0xB0 | ((received_seq + 1) & 0x0F)`.
    pub fn build_ack_frame(received_sequence: u8) -> [u8; 1] {
        [0xB0 | ((received_sequence.wrapping_add(1)) & 0x0F)]
    }

    /// Classify received TP 2.0 frame into `(opcode, sequence_number)`.
    pub fn classify_frame(frame: &[u8]) -> Result<(Tp20Opcode, u8), &'static str> {
        if frame.is_empty() {
            return Err("Empty TP 2.0 frame");
        }
        let op_nibble = frame[0] >> 4;
        let seq_nibble = frame[0] & 0x0F;

        let opcode = match op_nibble {
            0x0 => Tp20Opcode::DataIntermediate,
            0x1 => Tp20Opcode::DataLast,
            0x2 => Tp20Opcode::DataIntermediateAckRequest,
            0x3 => Tp20Opcode::DataLastAckRequest,
            0xA => Tp20Opcode::ControlParameter,
            0xB => Tp20Opcode::Acknowledge,
            _ => return Err("Unknown TP 2.0 opcode"),
        };

        Ok((opcode, seq_nibble))
    }

    /// Segment a multi-byte diagnostic payload into a series of classic CAN frames (max 7 bytes payload each).
    pub fn segment_payload(payload: &[u8], block_size: u8) -> Vec<Vec<u8>> {
        if block_size == 0 {
            return Vec::new();
        }
        let mut frames = Vec::new();
        let mut seq: u8 = 0;
        let chunks: Vec<&[u8]> = payload.chunks(7).collect();
        let total_chunks = chunks.len();

        for (idx, &chunk) in chunks.iter().enumerate() {
            let is_last = idx + 1 == total_chunks;
            let needs_ack = (idx + 1) % (block_size as usize) == 0 || is_last;

            let opcode_nibble = match (is_last, needs_ack) {
                (false, false) => 0x0, // DataIntermediate
                (true, false) => 0x1,  // DataLast
                (false, true) => 0x2,  // DataIntermediateAckRequest
                (true, true) => 0x3,   // DataLastAckRequest
            };

            let header = (opcode_nibble << 4) | (seq & 0x0F);
            let mut frame = Vec::with_capacity(1 + chunk.len());
            frame.push(header);
            frame.extend_from_slice(chunk);
            frames.push(frame);

            seq = (seq + 1) & 0x0F;
        }

        frames
    }
}
