/// 6. ST10 (`ST10F272M` / `C167`) UART BSL Payloads & CAN BCM `10 85` 4-Gate Precondition Evaluator.
pub struct St10BslAndCanBcmEngine;

impl St10BslAndCanBcmEngine {
    /// Builds the exact 32-byte ST10/C166 machine-code payload that writes the CAN bootloader lock-in flag
    /// `0x001119AB` to RAM `0x00F9FC` and triggers `SRST` (`0xB7 0x48 0xB7 0xB7`).
    pub fn make_unlock_can_payload() -> [u8; 32] {
        [
            0xA5, 0x5A, 0xA5, 0xA5, // DISWDT
            0xE6, 0xF0, 0xAB, 0x19, // MOV R0, #0x19AB
            0xF6, 0xF0, 0xFC, 0xF9, // MOV 0xF9FC, R0
            0xE6, 0xF0, 0x11, 0x00, // MOV R0, #0x0011
            0xF6, 0xF0, 0xFE, 0xF9, // MOV 0xF9FE, R0
            0xB7, 0x48, 0xB7, 0xB7, // SRST -> Bootloader checks 0xF9FC == 0x001119AB
            0xCC, 0x00, 0xCC, 0x00, 0xCC, 0x00, 0xCC, 0x00,
        ]
    }

    /// Builds the exact 32-byte ST10 UART BSL Stage-1 miniloader (`0xFA40`) that receives Stage-2 into XRAM `0xE000`.
    pub fn make_bsl_miniloader_payload(stage2_len: u16) -> [u8; 32] {
        let end_addr = 0xE000u16.wrapping_add(stage2_len);
        [
            0xA5,
            0x5A,
            0xA5,
            0xA5, // DISWDT
            0xE6,
            0xF0,
            0x00,
            0xE0, // MOV R0, #0xE000
            0x9A,
            0xB7,
            0xFE,
            0x70, // JNB S0RIR, $
            0x7E,
            0xB7, // BCLR S0RIR
            0xF2,
            0xF1,
            0xB2,
            0xFE, // MOV R1, S0RBUF
            0xB9,
            0x20, // MOVB [R0], RL1
            0x08,
            0x01, // ADD R0, #1
            0x46,
            0xF0,
            (end_addr & 0xFF) as u8,
            (end_addr >> 8) as u8, // CMP R0, #end_addr
            0x3D,
            0xF6, // JMPR cc_NZ, loop
            0xFA,
            0x00,
            0x00,
            0xE0, // JMPS 0, 0xE000
        ]
    }

    /// Evaluates the 4 CPU-local RAM gates required before CAN BCM `10 85` (`DiagnosticSessionControl(ProgrammingSession)`).
    pub fn evaluate_can_bcm_10_85_preconditions(
        supply_raw_00f2e6: u16,
        attempt_counter_00f88e: u8,
        flag_002d09: u8,
        flag_000068: u8,
    ) -> Result<(), &'static str> {
        if supply_raw_00f2e6 <= 0x2EE0 {
            return Err("NRC_0x22_GATE1_VOLTAGE_TOO_LOW: RAM[0x00F2E6] <= 0x2EE0 (12,000 raw)");
        }
        if attempt_counter_00f88e >= 4 {
            return Err("NRC_0x22_GATE2_ATTEMPT_LOCKOUT: RAM[0x00F88E] >= 4 failed attempts");
        }
        if (flag_002d09 & 0x80) != 0 {
            return Err("NRC_0x22_GATE3_INHIBIT_FLAG_2D09: bit 7 of RAM[0x002D09] is set");
        }
        if (flag_000068 & 0x02) != 0 {
            return Err("NRC_0x22_GATE4_VEHICLE_BUSY_0068: bit 1 of RAM[0x000068] is set");
        }
        Ok(())
    }

}


/// 8. Saab Trionic 5 / 7 / 8 Security Access & End-Marker Checksum.
pub struct SaabTrionicSecurityAndChecksum;

impl SaabTrionicSecurityAndChecksum {
    /// Calculates the Saab Trionic 8 (`T8`) 16-bit SecurityAccess key for levels `0x01`, `0xFB`, or `0xFD`.
    pub fn calculate_trionic8_access_key(seed: [u8; 2], level: u8) -> [u8; 2] {
        let val = ((seed[0] as u32) << 8) | (seed[1] as u32);
        let mut key = (((val >> 5) | (val << 11)).wrapping_add(0xB988)) & 0xFFFF;
        match level {
            0xFB => {
                key ^= 0x8749;
                key = key.wrapping_add(0x06D3) & 0xFFFF;
                key ^= 0xCFDF;
            }
            0xFD => {
                key /= 3;
                key ^= 0x8749;
                key = key.wrapping_add(0x0ACF) & 0xFFFF;
                key ^= 0x81BF;
            }
            _ => {}
        }
        [((key >> 8) & 0xFF) as u8, (key & 0xFF) as u8]
    }

    /// Calculates the Saab Trionic 7 (`T7`) 16-bit SecurityAccess key across `KnockKnock` methods `0..=4`.
    pub fn calculate_trionic7_access_key(seed: u16, method: u8) -> u16 {
        let mut key = (seed as u32) << 2;
        key &= 0xFFFF;
        match method {
            0 => {
                key ^= 0x8142;
                key = key.wrapping_sub(0x2356);
            }
            1 => {
                key ^= 0x4081;
                key = key.wrapping_sub(0x1F6F);
            }
            2 => {
                key ^= 0x03DC;
                key = key.wrapping_sub(0x2356);
            }
            3 => {
                key ^= 0x03D7;
                key = key.wrapping_sub(0x2356);
            }
            4 => {
                key ^= 0x0409;
                key = key.wrapping_sub(0x2356);
            }
            _ => {}
        }
        (key & 0xFFFF) as u16
    }

    /// Locates the Trionic 5 (`T5`) end marker `[0x4E, 0xFA, 0xFB, 0xCC]` and computes the 32-bit Big-Endian additive checksum.
    pub fn calculate_trionic5_bin_checksum(bin: &[u8]) -> Result<[u8; 4], &'static str> {
        let marker = [0x4E, 0xFA, 0xFB, 0xCC];
        let pos = bin
            .windows(4)
            .position(|w| w == marker)
            .ok_or("Could not find Trionic 5 end marker (0x4E 0xFA 0xFB 0xCC)")?;
        let code_len = pos + 3;
        let sum: u32 = bin[..=code_len]
            .iter()
            .fold(0u32, |acc, &b| acc.wrapping_add(b as u32));
        Ok(sum.to_be_bytes())
    }
}


/// 9. BMW DS2 K-Line / EWS ISN Synchronizer & Hardware EPROM Burner Protocol.
pub struct BmwDs2AndMoatesBurner;

impl BmwDs2AndMoatesBurner {
    /// Builds a BMW DS2 frame: `[address, total_length, payload..., xor_checksum]`.
    pub fn ds2_build_frame(address: u8, payload: &[u8]) -> Vec<u8> {
        let total_len = (1 + 1 + payload.len() + 1) as u8;
        let mut frame = Vec::with_capacity(total_len as usize);
        frame.push(address);
        frame.push(total_len);
        frame.extend_from_slice(payload);
        let xor_cks = frame.iter().fold(0u8, |acc, &b| acc ^ b);
        frame.push(xor_cks);
        frame
    }

    /// Scans a raw K-Line buffer backwards (skipping echo frames) to extract the last valid DS2 response matching `expected_address`.
    pub fn ds2_extract_response(expected_address: u8, buffer: &[u8]) -> Option<Vec<u8>> {
        if buffer.len() < 4 {
            return None;
        }
        for i in (0..=buffer.len() - 3).rev() {
            if buffer[i] != expected_address {
                continue;
            }
            let frame_len = buffer[i + 1] as usize;
            if frame_len < 3 || i + frame_len > buffer.len() {
                continue;
            }
            let frame = &buffer[i..i + frame_len];
            let expected_cks = frame[..frame_len - 1].iter().fold(0u8, |acc, &b| acc ^ b);
            if expected_cks == frame[frame_len - 1] {
                return Some(frame[..frame_len - 1].to_vec());
            }
        }
        None
    }

    /// Converts 4 BCD-encoded DME ISN bytes (`[0x03, 0x05, 0x02, 0x01]` $\rightarrow$ `3521` decimal $\rightarrow$ `0xDC1`)
    /// into the 2-byte EWS ISN write payload (`((val >> 8) & 0x0F, val & 0xFF)`).
    pub fn dme_bcd_isn_to_ews_bytes(isn_4bytes: [u8; 4]) -> (String, [u8; 2]) {
        let mut dec_val: u32 = 0;
        for &b in &isn_4bytes {
            dec_val = dec_val * 10 + ((b & 0x0F) as u32);
        }
        let masked = dec_val & 0xFFF;
        let hex_str = format!("{:03x}", masked);
        let ews_bytes = [((masked >> 8) & 0x0F) as u8, (masked & 0xFF) as u8];
        (hex_str, ews_bytes)
    }

    /// Builds a hardware EPROM burner 256-byte page write packet (`0x106` bytes total, command `0x57 'W'`).
    pub fn build_moates_write_256b_packet(chip_id: u8, address: u16, page_256b: &[u8; 256]) -> Vec<u8> {
        let mut pkt = Vec::with_capacity(0x106);
        pkt.push(chip_id); // 0x35 = SST27SF512, 0x36 = SST27SF256
        pkt.push(0x57); // 'W'
        pkt.push(0x00);
        pkt.push((address >> 8) as u8);
        pkt.push((address & 0xFF) as u8);
        pkt.extend_from_slice(page_256b);
        let sum8 = pkt.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
        pkt.push(sum8);
        pkt
    }
}


// ============================================================================

pub struct PassthruAndTraceCodec;

impl PassthruAndTraceCodec {
    /// Custom 64-character scrambled diagnostic trace alphabet.
    pub const SCRAMBLED_TRACE_ALPHABET_64: &'static [u8; 64] =
        b"vY86gsLNZ9FbEkClmnUz]t2feXqDw7rh0By[ijxRP3T4KSaVHAWd5IucJQGo1OpM";

    /// Converts J2534 `READ_VBATT` millivolts into `(whole_volts, millivolt_remainder)`.
    pub fn format_passthru_vbatt_mv(vbatt_mv: u32) -> (u32, u32) {
        (vbatt_mv / 1000, vbatt_mv % 1000)
    }

    /// Decodes a 3-byte KWP2000/UDS DTC record `[dtc_hi, dtc_lo, status]` into `("Pxxxx"|"Cxxxx"|"Bxxxx"|"Uxxxx", status)`.
    pub fn decode_kwp_uds_3byte_dtc(rec: [u8; 3]) -> (String, u8) {
        let prefix = match (rec[0] >> 6) & 0x03 {
            0 => 'P',
            1 => 'C',
            2 => 'B',
            _ => 'U',
        };
        let code = (((rec[0] & 0x3F) as u16) << 8) | (rec[1] as u16);
        (format!("{}{:04X}", prefix, code), rec[2])
    }

    /// Encodes diagnostic trace bytes using the custom 64-char alphabet (LSB-first 6-bit groups).
    pub fn trace_custom64_encode(data: &[u8]) -> String {
        let mut out = String::with_capacity(((data.len() + 2) / 3) * 4);
        let mut idx = 0usize;
        while idx < data.len() {
            let b0 = data[idx] as u32;
            let b1 = if idx + 1 < data.len() { data[idx + 1] as u32 } else { 0 };
            let b2 = if idx + 2 < data.len() { data[idx + 2] as u32 } else { 0 };
            let val = (b0 << 16) | (b1 << 8) | b2;
            out.push(Self::SCRAMBLED_TRACE_ALPHABET_64[(val & 0x3F) as usize] as char);
            out.push(Self::SCRAMBLED_TRACE_ALPHABET_64[((val >> 6) & 0x3F) as usize] as char);
            out.push(Self::SCRAMBLED_TRACE_ALPHABET_64[((val >> 12) & 0x3F) as usize] as char);
            out.push(Self::SCRAMBLED_TRACE_ALPHABET_64[((val >> 18) & 0x3F) as usize] as char);
            idx += 3;
        }
        out
    }

    /// Decodes a custom-alphabet trace string back into raw bytes.
    pub fn trace_custom64_decode(encoded: &str, expected_len: usize) -> Result<Vec<u8>, String> {
        let bytes = encoded.as_bytes();
        if bytes.len() % 4 != 0 {
            return Err("Encoded trace length must be a multiple of 4".to_string());
        }
        let mut lut = [0xFFu8; 256];
        for (i, &ch) in Self::SCRAMBLED_TRACE_ALPHABET_64.iter().enumerate() {
            lut[ch as usize] = i as u8;
        }
        let mut out = Vec::with_capacity((bytes.len() / 4) * 3);
        for chunk in bytes.chunks_exact(4) {
            let c0 = lut[chunk[0] as usize];
            let c1 = lut[chunk[1] as usize];
            let c2 = lut[chunk[2] as usize];
            let c3 = lut[chunk[3] as usize];
            if c0 == 0xFF || c1 == 0xFF || c2 == 0xFF || c3 == 0xFF {
                return Err("Invalid character in custom64 trace string".to_string());
            }
            let val = (c0 as u32) | ((c1 as u32) << 6) | ((c2 as u32) << 12) | ((c3 as u32) << 18);
            out.push(((val >> 16) & 0xFF) as u8);
            out.push(((val >> 8) & 0xFF) as u8);
            out.push((val & 0xFF) as u8);
        }
        if expected_len <= out.len() {
            out.truncate(expected_len);
        }
        Ok(out)
    }
}
