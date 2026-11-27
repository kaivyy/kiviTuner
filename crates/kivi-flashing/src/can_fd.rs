//! Ported from `ecu_engine_rust::flashing::can_fd`.
//!
//! Pure byte-level protocol logic: builders, parsers, encoders, state machines.
//! No I/O, no device access, no `unsafe`. Transport is the caller's concern.
//!
//! Verified by unit tests with hand-derived expectations. **Not** validated
//! against a physical ECU; no such claim is made.

/// CAN FD Data Length Code (DLC) to payload size mapper.
/// In CAN FD, DLC values 9..15 represent non-linear byte sizes up to 64 bytes.
pub struct CanFdDlcMapper;

impl CanFdDlcMapper {
    /// Convert DLC code (0..15) to exact payload length in bytes.
    pub fn dlc_to_len(dlc: u8) -> usize {
        match dlc {
            0..=8 => dlc as usize,
            9 => 12,
            10 => 16,
            11 => 20,
            12 => 24,
            13 => 32,
            14 => 48,
            15 => 64,
            _ => 64,
        }
    }

    /// Convert payload length in bytes to smallest valid CAN FD DLC code.
    pub fn len_to_dlc(len: usize) -> u8 {
        match len {
            0..=8 => len as u8,
            9..=12 => 9,
            13..=16 => 10,
            17..=20 => 11,
            21..=24 => 12,
            25..=32 => 13,
            33..=48 => 14,
            _ => 15,
        }
    }
}

/// CAN FD Message Frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanFdFrame {
    pub can_id: u32,
    pub is_extended_id: bool,
    pub bit_rate_switch: bool, // BRS: Transmit data payload at fast baud (e.g. 2 Mbps or 5 Mbps)
    pub error_state_indicator: bool,
    pub data: Vec<u8>,
}

impl CanFdFrame {
    pub fn new(can_id: u32, is_extended_id: bool, bit_rate_switch: bool, data: Vec<u8>) -> Self {
        let valid_len = CanFdDlcMapper::dlc_to_len(CanFdDlcMapper::len_to_dlc(data.len()));
        let mut padded = data;
        if padded.len() < valid_len {
            padded.resize(valid_len, 0xCC); // Standard CAN FD ISO padding byte
        }
        Self {
            can_id,
            is_extended_id,
            bit_rate_switch,
            error_state_indicator: false,
            data: padded,
        }
    }
}

/// ISO 15765-2 Flow Control Status for CAN FD Multi-Frame transfers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanFdFlowStatus {
    ContinueToSend = 0x00,
    Wait = 0x01,
    Overflow = 0x02,
}

/// ISO 15765-2:2016 CAN FD Transport Protocol Segmenter & Reassembler.
pub struct CanFdIsoTp;

impl CanFdIsoTp {
    pub const CAN_FD_MAX_PAYLOAD: usize = 64;
    pub const MAX_SINGLE_FRAME_DATA: usize = 62;
    pub const MAX_FF12_INITIAL_DATA: usize = 62;
    pub const MAX_FF32_INITIAL_DATA: usize = 58;
    pub const MAX_CONSECUTIVE_FRAME_DATA: usize = 63;

    /// Build Single Frame (SF) for CAN FD.
    /// In CAN FD, payload up to 62 bytes fits into a single frame using escape code (Byte 0 = 0x00, Byte 1 = Length).
    pub fn build_single_frame(can_id: u32, payload: &[u8]) -> Result<CanFdFrame, &'static str> {
        if payload.is_empty() || payload.len() > Self::MAX_SINGLE_FRAME_DATA {
            return Err("Payload size invalid for CAN FD Single Frame (max 62 bytes)");
        }

        let mut data = Vec::with_capacity(64);
        if payload.len() <= 7 {
            // Classical format: Byte 0 = [Type 0x00 | Length 1..7]
            data.push(payload.len() as u8);
        } else {
            // CAN FD Extended Single Frame: Byte 0 = 0x00, Byte 1 = Length (8..62)
            data.push(0x00);
            data.push(payload.len() as u8);
        }
        data.extend_from_slice(payload);

        Ok(CanFdFrame::new(can_id, false, true, data))
    }

    /// Parse Single Frame (SF) from CAN FD frame.
    pub fn parse_single_frame(frame: &CanFdFrame) -> Result<Vec<u8>, &'static str> {
        if frame.data.is_empty() {
            return Err("Empty CAN FD frame");
        }

        let first = frame.data[0];
        let (len, offset) = if (first >> 4) == 0x0 && first != 0x00 {
            // Classical Single Frame (1..7 bytes)
            ((first & 0x0F) as usize, 1)
        } else if first == 0x00 {
            // CAN FD Extended Single Frame (8..62 bytes)
            if frame.data.len() < 2 {
                return Err("Truncated CAN FD extended single frame header");
            }
            (frame.data[1] as usize, 2)
        } else {
            return Err("Not a CAN FD Single Frame");
        };

        if frame.data.len() < offset + len {
            return Err("CAN FD frame data shorter than indicated length");
        }

        Ok(frame.data[offset..offset + len].to_vec())
    }

    /// Build First Frame (FF) for CAN FD Multi-Frame transfer (ISO 15765-2:2016).
    /// Automatically uses 12-bit length header for `total_len <= 4095` (up to 62 initial bytes)
    /// or 32-bit extended escape header (`0x10 0x00 + u32_be`) for `total_len > 4095` (up to 58 initial bytes).
    pub fn build_first_frame(
        can_id: u32,
        total_len: usize,
        initial_chunk: &[u8],
    ) -> Result<CanFdFrame, &'static str> {
        if total_len <= Self::MAX_SINGLE_FRAME_DATA {
            return Err("Payload fits in a CAN FD Single Frame; First Frame requires > 62 bytes");
        }

        let mut data = Vec::with_capacity(64);
        if total_len <= 0x0FFF {
            if initial_chunk.len() > Self::MAX_FF12_INITIAL_DATA {
                return Err("12-bit First Frame initial chunk cannot exceed 62 bytes");
            }
            let hi = 0x10 | (((total_len >> 8) & 0x0F) as u8);
            let lo = (total_len & 0xFF) as u8;
            data.push(hi);
            data.push(lo);
            data.extend_from_slice(initial_chunk);
        } else {
            if initial_chunk.len() > Self::MAX_FF32_INITIAL_DATA {
                return Err("32-bit Extended First Frame initial chunk cannot exceed 58 bytes");
            }
            let len_u32 = u32::try_from(total_len).map_err(|_| "Total length exceeds 32-bit ISO-TP limit")?;
            data.push(0x10);
            data.push(0x00); // Escape code for 32-bit FF_DL
            data.extend_from_slice(&len_u32.to_be_bytes());
            data.extend_from_slice(initial_chunk);
        }

        Ok(CanFdFrame::new(can_id, false, true, data))
    }

    /// Parse First Frame (FF) from CAN FD frame.
    /// Returns `(total_message_length, initial_payload_slice)`.
    pub fn parse_first_frame(frame: &CanFdFrame) -> Result<(usize, Vec<u8>), &'static str> {
        if frame.data.len() < 2 {
            return Err("Frame too short for First Frame");
        }
        let b0 = frame.data[0];
        if (b0 >> 4) != 0x01 {
            return Err("Not an ISO-TP First Frame (PCI nibble != 0x1)");
        }

        let b1 = frame.data[1];
        if (b0 & 0x0F) == 0x00 && b1 == 0x00 {
            // 32-bit Extended First Frame
            if frame.data.len() < 6 {
                return Err("Truncated 32-bit Extended First Frame header");
            }
            let total_len = u32::from_be_bytes([
                frame.data[2],
                frame.data[3],
                frame.data[4],
                frame.data[5],
            ]) as usize;
            let chunk = frame.data[6..].to_vec();
            Ok((total_len, chunk))
        } else {
            // Standard 12-bit First Frame
            let total_len = (((b0 & 0x0F) as usize) << 8) | (b1 as usize);
            let chunk = frame.data[2..].to_vec();
            Ok((total_len, chunk))
        }
    }

    /// Build Consecutive Frame (CF) for CAN FD (up to 63 bytes of payload per frame).
    pub fn build_consecutive_frame(
        can_id: u32,
        sequence_number: u8,
        chunk: &[u8],
    ) -> Result<CanFdFrame, &'static str> {
        if chunk.is_empty() || chunk.len() > Self::MAX_CONSECUTIVE_FRAME_DATA {
            return Err("CAN FD Consecutive Frame payload must be 1..63 bytes");
        }
        let mut data = Vec::with_capacity(1 + chunk.len());
        data.push(0x20 | (sequence_number & 0x0F));
        data.extend_from_slice(chunk);
        Ok(CanFdFrame::new(can_id, false, true, data))
    }

    /// Parse Consecutive Frame (CF) from CAN FD frame.
    /// Returns `(sequence_number_0_to_15, raw_payload_bytes)`.
    pub fn parse_consecutive_frame(frame: &CanFdFrame) -> Result<(u8, Vec<u8>), &'static str> {
        if frame.data.len() < 2 {
            return Err("Frame too short for Consecutive Frame");
        }
        let b0 = frame.data[0];
        if (b0 >> 4) != 0x02 {
            return Err("Not an ISO-TP Consecutive Frame (PCI nibble != 0x2)");
        }
        let seq = b0 & 0x0F;
        Ok((seq, frame.data[1..].to_vec()))
    }

    /// Build Flow Control (FC) Frame for CAN FD receiver.
    pub fn build_flow_control_frame(
        can_id: u32,
        status: CanFdFlowStatus,
        block_size: u8,
        st_min_ms: u8,
    ) -> CanFdFrame {
        let data = vec![0x30 | (status as u8 & 0x0F), block_size, st_min_ms];
        CanFdFrame::new(can_id, false, true, data)
    }

    /// Parse Flow Control (FC) Frame.
    /// Returns `(CanFdFlowStatus, block_size, st_min_ms)`.
    pub fn parse_flow_control_frame(
        frame: &CanFdFrame,
    ) -> Result<(CanFdFlowStatus, u8, u8), &'static str> {
        if frame.data.len() < 3 {
            return Err("Frame too short for Flow Control");
        }
        let b0 = frame.data[0];
        if (b0 >> 4) != 0x03 {
            return Err("Not an ISO-TP Flow Control Frame (PCI nibble != 0x3)");
        }
        let status = match b0 & 0x0F {
            0x00 => CanFdFlowStatus::ContinueToSend,
            0x01 => CanFdFlowStatus::Wait,
            0x02 => CanFdFlowStatus::Overflow,
            _ => return Err("Invalid Flow Control status nibble"),
        };
        Ok((status, frame.data[1], frame.data[2]))
    }

    /// Segment an arbitrary UDS payload (from 1 byte up to multi-megabyte blocks) into CAN FD ISO-TP frames.
    /// Uses Single Frame for `<= 62B`, 12-bit FF + 63B CFs for `63..=4095B`, and 32-bit FF + 63B CFs for `> 4095B`.
    pub fn segment_multi_frame(can_id: u32, payload: &[u8]) -> Result<Vec<CanFdFrame>, &'static str> {
        if payload.is_empty() {
            return Err("Cannot segment empty payload");
        }
        if payload.len() <= Self::MAX_SINGLE_FRAME_DATA {
            return Ok(vec![Self::build_single_frame(can_id, payload)?]);
        }

        let ff_cap = if payload.len() <= 0x0FFF {
            Self::MAX_FF12_INITIAL_DATA
        } else {
            Self::MAX_FF32_INITIAL_DATA
        };

        let mut frames = Vec::new();
        frames.push(Self::build_first_frame(
            can_id,
            payload.len(),
            &payload[..ff_cap],
        )?);

        let mut offset = ff_cap;
        let mut seq: u8 = 1;
        while offset < payload.len() {
            let end = (offset + Self::MAX_CONSECUTIVE_FRAME_DATA).min(payload.len());
            frames.push(Self::build_consecutive_frame(
                can_id,
                seq,
                &payload[offset..end],
            )?);
            offset = end;
            seq = (seq + 1) & 0x0F;
        }

        Ok(frames)
    }

    /// Reassemble a sequence of CAN FD ISO-TP frames (either Single Frame or FF + CF sequence)
    /// back into the exact original UDS payload, validating sequence numbers and stripping `0xCC` padding.
    pub fn reassemble_multi_frame(frames: &[CanFdFrame]) -> Result<Vec<u8>, &'static str> {
        if frames.is_empty() {
            return Err("No frames provided for reassembly");
        }

        // Check if Single Frame
        if frames.len() == 1 {
            if let Ok(sf) = Self::parse_single_frame(&frames[0]) {
                return Ok(sf);
            }
        }

        const MAX_CANFD_REASSEMBLY_BYTES: usize = 16 * 1024 * 1024;
        let (total_len, first_chunk) = Self::parse_first_frame(&frames[0])?;
        if total_len == 0 || total_len > MAX_CANFD_REASSEMBLY_BYTES {
            return Err("CAN FD ISO-TP invalid or oversized total payload length");
        }
        let mut assembled = Vec::with_capacity(total_len.min(frames.len().saturating_mul(63)));
        let take_first = first_chunk.len().min(total_len);
        assembled.extend_from_slice(&first_chunk[..take_first]);

        let mut expected_seq: u8 = 1;
        for frame in &frames[1..] {
            if assembled.len() >= total_len {
                break;
            }
            let (seq, chunk) = Self::parse_consecutive_frame(frame)?;
            if seq != expected_seq {
                return Err("CAN FD ISO-TP sequence number mismatch during reassembly");
            }
            let remaining = total_len - assembled.len();
            let take = chunk.len().min(remaining);
            assembled.extend_from_slice(&chunk[..take]);
            expected_seq = (expected_seq + 1) & 0x0F;
        }

        if assembled.len() != total_len {
            return Err("Incomplete CAN FD Multi-Frame transfer: fewer bytes received than FF_DL");
        }

        Ok(assembled)
    }
}
