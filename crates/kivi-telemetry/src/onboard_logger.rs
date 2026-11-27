//! Ported from `ecu_engine_rust::telemetry::onboard_logger`.
//!
//! Pure parsing/analysis logic. No socket, no serial port, no device access.
//! Verified by unit tests with hand-derived expectations; **not** validated
//! against live hardware.

/// On-Board Log Session Metadata.
#[derive(Debug, Clone)]
pub struct OnboardLogSession {
    pub session_id: u16,
    pub sample_count: u32,
    pub packet_size_bytes: usize,
    pub samples: Vec<Vec<u8>>,
}

/// Standalone On-Board Flash Dump Parser.
pub struct OnboardFlashLogParser;

impl OnboardFlashLogParser {
    /// Parse raw flash memory dump containing multiple standalone logged sessions.
    ///
    /// Frame Format per Session Header:
    /// [0..2]: Magic Marker (0xAA, 0x55)
    /// [2..4]: Session ID (u16 BE)
    /// [4..8]: Sample Count (u32 BE)
    /// [8]: Packet Size (u8)
    /// [9..]: Raw data frames
    pub fn parse_flash_dump(raw_flash: &[u8]) -> Result<Vec<OnboardLogSession>, &'static str> {
        let mut sessions = Vec::new();
        let mut offset = 0;
        let total_len = raw_flash.len();

        while offset + 9 <= total_len {
            // Search for Session Header Magic 0xAA, 0x55
            if raw_flash[offset] == 0xAA && raw_flash[offset + 1] == 0x55 {
                let session_id = u16::from_be_bytes([raw_flash[offset + 2], raw_flash[offset + 3]]);
                let sample_count = u32::from_be_bytes([
                    raw_flash[offset + 4],
                    raw_flash[offset + 5],
                    raw_flash[offset + 6],
                    raw_flash[offset + 7],
                ]);
                let packet_size = raw_flash[offset + 8] as usize;

                if packet_size == 0 || packet_size > 128 {
                    offset += 2;
                    continue;
                }

                let Some(session_data_len) = (sample_count as usize).checked_mul(packet_size)
                else {
                    return Err("Unexpected value in log metadata. Session appears corrupt.");
                };
                if offset
                    .checked_add(9)
                    .and_then(|o| o.checked_add(session_data_len))
                    .is_none_or(|end| end > total_len)
                {
                    return Err("Unexpected value in log metadata. Session appears corrupt.");
                }

                let mut samples = Vec::with_capacity(sample_count as usize);
                let mut data_curr = offset + 9;

                for _ in 0..sample_count {
                    let packet = raw_flash[data_curr..data_curr + packet_size].to_vec();
                    samples.push(packet);
                    data_curr += packet_size;
                }

                sessions.push(OnboardLogSession {
                    session_id,
                    sample_count,
                    packet_size_bytes: packet_size,
                    samples,
                });

                offset = data_curr;
            } else {
                offset += 1;
            }
        }

        Ok(sessions)
    }
}
