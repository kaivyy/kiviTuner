//! Ported from `ecu_engine_rust::telemetry::adx`.
//!
//! Pure parsing/analysis logic. No socket, no serial port, no device access.
//! Verified by unit tests with hand-derived expectations; **not** validated
//! against live hardware.

use kivi_defs::ast::eval_calibration_equation;

/// ADX Packet Framing Mode.
#[derive(Debug, Clone, PartialEq)]
pub enum FramingMode {
    StaticLength(usize),
    HeaderFooter {
        header: Vec<u8>,
        footer: Vec<u8>,
        body_length: usize,
    },
}

/// ADX Checksum Mode on Serial Frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FrameChecksumType {
    None,
    Sum8Modulo256,       // (Sum of body bytes) & 0xFF == LastByte
    TwoComplementSum,    // (0x100 - (Sum & 0xFF)) & 0xFF == LastByte (Standard GM ALDL)
    XorLrc,              // XOR of all bytes == LastByte
}

/// Telemetry Channel Definition.
#[derive(Debug, Clone)]
pub struct AdxValueChannel {
    pub id: String,
    pub title: String,
    pub packet_offset: usize,
    pub size_bits: usize, // 8, 16, or 32
    pub is_little_endian: bool,
    pub equation: String,
}

impl AdxValueChannel {
    /// Extract channel engineering value from valid packet payload.
    pub fn extract_value(&self, payload: &[u8]) -> Result<f64, String> {
        let byte_len = self.size_bits / 8;
        if byte_len == 0
            || self
                .packet_offset
                .checked_add(byte_len)
                .is_none_or(|end| end > payload.len())
        {
            return Err("Packet payload too short for channel offset".to_string());
        }

        let raw_int: f64 = match self.size_bits {
            8 => payload[self.packet_offset] as f64,
            16 => {
                let b0 = payload[self.packet_offset] as u16;
                let b1 = payload[self.packet_offset + 1] as u16;
                let val = if self.is_little_endian {
                    b0 | (b1 << 8)
                } else {
                    (b0 << 8) | b1
                };
                val as f64
            }
            32 => {
                let b0 = payload[self.packet_offset] as u32;
                let b1 = payload[self.packet_offset + 1] as u32;
                let b2 = payload[self.packet_offset + 2] as u32;
                let b3 = payload[self.packet_offset + 3] as u32;
                let val = if self.is_little_endian {
                    b0 | (b1 << 8) | (b2 << 16) | (b3 << 24)
                } else {
                    (b0 << 24) | (b1 << 16) | (b2 << 8) | b3
                };
                val as f64
            }
            _ => return Err(format!("Unsupported bit size: {}", self.size_bits)),
        };

        eval_calibration_equation(&self.equation, raw_int)
    }
}

/// Bitfield Channel Definition (Digital Switch/Lamp status).
#[derive(Debug, Clone)]
pub struct AdxBitmaskChannel {
    pub id: String,
    pub title: String,
    pub packet_offset: usize,
    pub bitmask: u8, // e.g. 0x80 for bit 7
    pub active_high: bool,
}

impl AdxBitmaskChannel {
    pub fn is_active(&self, payload: &[u8]) -> bool {
        if self.packet_offset >= payload.len() {
            return false;
        }
        let byte_val = payload[self.packet_offset];
        let bit_set = (byte_val & self.bitmask) != 0;
        if self.active_high {
            bit_set
        } else {
            !bit_set
        }
    }
}

/// Streaming Serial Packet Synchronizer & Decoder.
pub struct AdxFramer {
    pub mode: FramingMode,
    pub checksum_type: FrameChecksumType,
    rx_buffer: Vec<u8>,
}

impl AdxFramer {
    pub fn new(mode: FramingMode, checksum_type: FrameChecksumType) -> Self {
        Self {
            mode,
            checksum_type,
            rx_buffer: Vec::with_capacity(1024),
        }
    }

    /// Ingest newly arrived raw serial bytes from UART COM port.
    pub fn push_bytes(&mut self, incoming: &[u8]) {
        self.rx_buffer.extend_from_slice(incoming);
    }

    /// Attempt to extract next validated packet payload from internal buffer.
    pub fn poll_next_packet(&mut self) -> Option<Vec<u8>> {
        match &self.mode {
            FramingMode::StaticLength(expected_len) => {
                if self.rx_buffer.len() < *expected_len {
                    return None;
                }
                let packet: Vec<u8> = self.rx_buffer.drain(0..*expected_len).collect();
                if self.validate_checksum(&packet) {
                    Some(packet)
                } else {
                    None
                }
            }
            FramingMode::HeaderFooter { header, footer, body_length } => {
                let total_expected = header
                    .len()
                    .checked_add(*body_length)
                    .and_then(|n| n.checked_add(footer.len()))?;
                if total_expected == 0 || self.rx_buffer.len() < total_expected {
                    return None;
                }

                // Scan for header + matching footer sequence, validating checksum before consuming.
                let mut i = 0usize;
                while i <= self.rx_buffer.len().saturating_sub(total_expected) {
                    if &self.rx_buffer[i..i + header.len()] == header.as_slice() {
                        let footer_pos = i + header.len() + *body_length;
                        if &self.rx_buffer[footer_pos..footer_pos + footer.len()] == footer.as_slice()
                        {
                            let body_start = i + header.len();
                            let body_end = body_start + *body_length;
                            let valid = self.validate_checksum(&self.rx_buffer[body_start..body_end]);
                            if valid {
                                let body = self.rx_buffer[body_start..body_end].to_vec();
                                self.rx_buffer.drain(0..i + total_expected);
                                return Some(body);
                            }
                            // False sync match: advance past one byte and keep searching without losing real frames.
                            self.rx_buffer.drain(0..i + 1);
                            i = 0;
                            if self.rx_buffer.len() < total_expected {
                                return None;
                            }
                            continue;
                        }
                    }
                    i += 1;
                }

                let trim_limit = total_expected.max(2048);
                if self.rx_buffer.len() > trim_limit {
                    let drop_count = self.rx_buffer.len() - trim_limit / 2;
                    self.rx_buffer.drain(0..drop_count);
                }
                None
            }
        }
    }

    fn validate_checksum(&self, body: &[u8]) -> bool {
        if body.is_empty() {
            return false;
        }
        match self.checksum_type {
            FrameChecksumType::None => true,
            FrameChecksumType::Sum8Modulo256 => {
                let data_len = body.len() - 1;
                let sum: u32 = body[0..data_len].iter().map(|&b| b as u32).sum();
                let chk = (sum & 0xFF) as u8;
                chk == body[data_len]
            }
            FrameChecksumType::TwoComplementSum => {
                let data_len = body.len() - 1;
                let sum: u32 = body[0..data_len].iter().map(|&b| b as u32).sum();
                let chk = ((0x100 - (sum & 0xFF)) & 0xFF) as u8;
                chk == body[data_len]
            }
            FrameChecksumType::XorLrc => {
                let data_len = body.len() - 1;
                let mut xor = 0u8;
                for &b in &body[0..data_len] {
                    xor ^= b;
                }
                xor == body[data_len]
            }
        }
    }
}
