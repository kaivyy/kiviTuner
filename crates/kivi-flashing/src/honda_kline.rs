//! Ported from `ecu_engine_rust::flashing::honda_kline`.
//!
//! Pure byte-level protocol logic: builders, parsers, encoders, state machines.
//! No I/O, no device access, no `unsafe`. Transport is the caller's concern.
//!
//! Verified by unit tests with hand-derived expectations. **Not** validated
//! against a physical ECU; no such claim is made.

/// ECU Vendor & Architecture Family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HondaEcuVendor {
    Keihin,
    Shindengen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HondaEcuType {
    KeihinOther,
    KeihinH8C,
    KeihinM32R,
    ShindengenV850,
    ShindengenRH850,
    ShindengenRH850B,
}

/// Honda Checksum & Packet Framing Engine.
pub struct HondaProtocolFraming;

impl HondaProtocolFraming {
    /// Calculate Honda 8-Bit Two's Complement Checksum:
    /// Checksum = (0x100 - (Sum & 0xFF)) & 0xFF
    pub fn checksum_8bit(data: &[u8]) -> u8 {
        data.iter()
            .fold(0u8, |acc, &b| acc.wrapping_add(b))
            .wrapping_neg()
    }

    /// Build Honda K-Line Packet with auto-calculated checksum.
    pub fn build_packet(header: u8, service: u8, sub_cmd: u8, payload: &[u8]) -> Vec<u8> {
        let total_len = payload.len() + 5; // header + len + service + sub_cmd + payload + cs
        let mut packet = Vec::with_capacity(total_len);
        packet.push(header);
        packet.push(total_len as u8);
        packet.push(service);
        packet.push(sub_cmd);
        packet.extend_from_slice(payload);
        let cs = Self::checksum_8bit(&packet);
        packet.push(cs);
        packet
    }

    /// Verify received packet checksum.
    pub fn verify_packet(packet: &[u8]) -> bool {
        if packet.len() < 4 {
            return false;
        }
        let len = packet.len();
        let expected_cs = Self::checksum_8bit(&packet[..len - 1]);
        packet[len - 1] == expected_cs
    }
}

/// Shindengen Seed-Key Challenge-Response Cryptographic Engine.
pub struct ShindengenSeedKeyEngine;

impl ShindengenSeedKeyEngine {
    pub const WRITE_KEY_SIG: [u8; 7] = [0x11, 0x12, 0x13, 0x14, 0x15, 0x15, 0x17];
    pub const RECOVERY_KEY_SIG: [u8; 6] = [0x91, 0x91, 0x0D, 0x00, 0x9E, 0x8D];

    /// Solve Shindengen Write Key given ECU ROM binary and 2-byte challenge seed [S0, S1].
    pub fn compute_write_key(bin_bytes: &[u8], seed: [u8; 2]) -> Result<[u8; 2], &'static str> {
        let sig_pos = Self::find_pattern(bin_bytes, &Self::WRITE_KEY_SIG)
            .ok_or("Shindengen Write Key signature not found in binary")?;

        let mut loc = sig_pos + Self::WRITE_KEY_SIG.len();
        while loc + 10 <= bin_bytes.len() {
            if bin_bytes[loc + 8] == 0xEE && (bin_bytes[loc + 9] == 0x00 || bin_bytes[loc + 9] == 0xFF) {
                let seed_val = u16::from_be_bytes([seed[0], seed[1]]);
                let v5 = u16::from_le_bytes([bin_bytes[loc], bin_bytes[loc + 1]]);
                let v6 = u16::from_le_bytes([bin_bytes[loc + 2], bin_bytes[loc + 3]]);
                let v7 = u16::from_le_bytes([bin_bytes[loc + 4], bin_bytes[loc + 5]]);

                let v8 = v6.wrapping_mul(seed_val.wrapping_add(v5));
                let key_val = if v7 > 0 {
                    v8.wrapping_add(seed_val % v7)
                } else {
                    v8.wrapping_add(seed_val)
                };

                return Ok([(key_val >> 8) as u8, (key_val & 0xFF) as u8]);
            }
            loc += 1;
        }

        Err("Shindengen EE parameter marker not found")
    }

    /// Solve Shindengen Recovery Key (Emergency unbrick) given ECU ROM binary and 2-byte seed.
    pub fn compute_recovery_key(bin_bytes: &[u8], seed: [u8; 2]) -> Result<[u8; 2], &'static str> {
        let sig_pos = Self::find_pattern(bin_bytes, &Self::RECOVERY_KEY_SIG)
            .ok_or("Shindengen Recovery signature not found")?;

        if sig_pos < 6 {
            return Err("Invalid recovery signature offset");
        }

        let loc0 = sig_pos - 6;
        let seed_val = u16::from_be_bytes([seed[0], seed[1]]);
        let loc2 = u16::from_le_bytes([bin_bytes[loc0], bin_bytes[loc0 + 1]]);
        let loc3 = u16::from_le_bytes([bin_bytes[loc0 + 2], bin_bytes[loc0 + 3]]);
        let loc4 = u16::from_le_bytes([bin_bytes[loc0 + 4], bin_bytes[loc0 + 5]]);

        let v5 = loc3.wrapping_mul(seed_val.wrapping_add(loc2));
        let key_val = if loc4 > 0 {
            v5.wrapping_add(seed_val % loc4)
        } else {
            v5.wrapping_add(seed_val)
        };

        Ok([(key_val >> 8) as u8, (key_val & 0xFF) as u8])
    }

    /// Calculate Shindengen 32-Bit Little-Endian Checksum across full binary.
    pub fn calculate_shindengen_checksum32(bin_bytes: &[u8], checksum_offset: usize) -> u32 {
        let mut total_sum: u32 = 0;
        let n = bin_bytes.len();

        for i in (0..n).step_by(4) {
            if i >= checksum_offset && i < checksum_offset + 4 {
                continue; // Skip checksum field itself
            }
            if i + 4 <= n {
                let word = u32::from_le_bytes([
                    bin_bytes[i],
                    bin_bytes[i + 1],
                    bin_bytes[i + 2],
                    bin_bytes[i + 3],
                ]);
                total_sum = total_sum.wrapping_add(word);
            }
        }

        0u32.wrapping_sub(total_sum)
    }

    fn find_pattern(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack.windows(needle.len()).position(|w| w == needle)
    }
}

/// Complete Honda Flashing & Dumping Session Controller.
pub struct HondaKLineSession {
    pub vendor: HondaEcuVendor,
    pub ecu_type: HondaEcuType,
    pub is_fast_baud_active: bool,
    pub current_baud: u32,
    pub flash_counter: u16,
}

impl HondaKLineSession {
    pub fn new(vendor: HondaEcuVendor, ecu_type: HondaEcuType) -> Self {
        Self {
            vendor,
            ecu_type,
            is_fast_baud_active: false,
            current_baud: 10400,
            flash_counter: 0,
        }
    }

    /// Build Wake-Up / Tester Present packet.
    pub fn build_wakeup_packet() -> Vec<u8> {
        vec![0xFE, 0x04, 0x72, 0x8C]
    }

    /// Build Fast Baud Switch command (shifts K-Line from 10,400 to 62,500 bps).
    pub fn build_fast_baud_command(&self) -> Vec<u8> {
        match self.vendor {
            HondaEcuVendor::Keihin => vec![0x7E, 0x07, 0x01, 0x0E, 0x01, 0x90, 0xDB],
            HondaEcuVendor::Shindengen => vec![0x7C, 0x07, 0x02, 0x0E, 0x01, 0x00, 0x6C],
        }
    }

    /// Build Unlock Packets.
    /// Keihin uses 'PGM' and '-FI'.
    /// Shindengen uses 0x7B Seed Request.
    pub fn build_unlock_request(&self) -> Vec<Vec<u8>> {
        match self.vendor {
            HondaEcuVendor::Keihin => vec![
                vec![0x7D, 0x08, 0x01, 0x02, 0x50, 0x47, 0x4D, 0x94], // 'PGM'
                vec![0x7D, 0x08, 0x01, 0x03, 0x2D, 0x46, 0x49, 0xBB], // '-FI'
            ],
            HondaEcuVendor::Shindengen => vec![
                HondaProtocolFraming::build_packet(0x7B, 0x02, 0x05, &[0x00, 0x00, 0x00]),
            ],
        }
    }

    /// Build Read Memory Block packet for `chunk_size`-byte chunk at `offset`.
    pub fn build_read_chunk_request(&self, offset: usize, chunk_size: u8) -> Vec<u8> {
        let (header, service) = match self.vendor {
            HondaEcuVendor::Shindengen => (0x7Bu8, 0x02u8),
            HondaEcuVendor::Keihin => (0x7Du8, 0x01u8),
        };

        let mut pkt = vec![
            header,
            0x08, // Total length including chunk_size & checksum
            service,
            ((offset >> 16) & 0xFF) as u8,
            ((offset >> 8) & 0xFF) as u8,
            (offset & 0xFF) as u8,
            chunk_size,
        ];
        let cs = HondaProtocolFraming::checksum_8bit(&pkt);
        pkt.push(cs);
        pkt
    }

    /// Build Write Memory Block packet with 128-byte payload chunk.
    pub fn build_write_chunk_request(&self, offset: usize, chunk: &[u8]) -> Vec<u8> {
        let (header, service) = match self.vendor {
            HondaEcuVendor::Shindengen => (0x7Cu8, 0x02u8),
            HondaEcuVendor::Keihin => (0x7Eu8, 0x01u8),
        };

        let total_len = chunk.len() + 7;
        let mut pkt = Vec::with_capacity(total_len);
        pkt.push(header);
        pkt.push(total_len as u8);
        pkt.push(service);
        pkt.push(((offset >> 16) & 0xFF) as u8);
        pkt.push(((offset >> 8) & 0xFF) as u8);
        pkt.push((offset & 0xFF) as u8);
        pkt.extend_from_slice(chunk);
        let cs = HondaProtocolFraming::checksum_8bit(&pkt);
        pkt.push(cs);
        pkt
    }

    /// Build Flash Erase command.
    pub fn build_erase_command(&self) -> Vec<u8> {
        match self.vendor {
            HondaEcuVendor::Keihin => {
                vec![0x7E, 0x0B, 0x01, 0x0B, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x6E]
            }
            HondaEcuVendor::Shindengen => {
                vec![0x7C, 0x0B, 0x02, 0x0B, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x6F]
            }
        }
    }

    /// Build Read Flash Counter command.
    pub fn build_read_flash_counter_command(&self) -> Vec<u8> {
        match self.vendor {
            HondaEcuVendor::Keihin => vec![0x7D, 0x06, 0x01, 0x01, 0x03, 0x78],
            HondaEcuVendor::Shindengen => vec![0x7B, 0x06, 0x02, 0x01, 0x03, 0x79],
        }
    }
}
