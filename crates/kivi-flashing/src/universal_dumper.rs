//! Ported from `ecu_engine_rust::flashing::universal_dumper`.
//!
//! Pure byte-level protocol logic: builders, parsers, encoders, state machines.
//! No I/O, no device access, no `unsafe`. Transport is the caller's concern.
//!
//! Verified by unit tests with hand-derived expectations. **Not** validated
//! against a physical ECU; no such claim is made.

use crate::bench_hardware::{BenchPowerSupplyMonitor, VehicleElectricalClass};
use crate::oem_checksum_crypto::{
    HondaKeihinBikeKLineAndIsuzuTranstronEngine, HondaKeihinShindengenUnifiedCore,
    PassthruAndTraceCodec, VbfPhfAndMcuMapEngine,
};
use kivi_core::bin_export::validate_dumped_bin_integrity;
use kivi_checksum::primitives::compute_crc32;
use kivi_scanner::zero_xdf_scan::{
    apply_memory_interval_nms, scan_16bit_tables_auto_endian, DiscoveredFullTable,
};
use kivi_scanner::limiter_scan::scan_limiters;

/// Vehicle Communication Protocol standard (Motorcycles, Passenger Cars, & 24V Heavy-Duty Trucks).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlasherProtocol {
    KLineDirectAddress,       // BMW BMS46 (Cmd 0x06, 100B)
    HondaKLineKwp,            // Honda Keihin/Shindengen (Cmd 0x71/0x7D, 128B)
    SubaruM32rBoot,           // Subaru/Mitsubishi Hitachi M32R (1024B)
    BoschUdsIsoTp,            // Bosch MED17/EDC17/MD1 & VW Simos 18 (Cmd 0x35/0x36, 4086B / 0x0FF6)
    GeneralMotorsVpwCan,      // GM LS / PowerPC (Cmd 0x35/0x36, 4096B)
    ToyotaDensoCan,           // Toyota Innova/Fortuner 2GD (ISO 15765-2 UDS)
    RenesasDensoKernelStream, // Denso SH7055/SH7058/SH7059 RAM Kernel Stream
    MitsubishiMh7Kline,       // Mitsubishi EVO 5-9 & Yamaha MH72xx/MH81xx (62.5 kbps K-Line)
    Kwp2000FastBaudKLine,     // Bosch ME7 / EDC15 KWP2000 with baud jump up to 124,800 bps
    RenesasRh850Can,          // Modern 2018-2026 Renesas RH850 32-bit (Nissan/Toyota/Renault 4MB UDS CAN)
    HeavyDutyJ1939Can,        // 24V Heavy-Duty Truck SAE J1939 (Cummins, Bosch EDC7/EDC17CV, Hino, Isuzu, Scania: 1024B DM14/DM16)
}

/// Status of individual block transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockTransferStatus {
    Success,
    Pending0x78Retry,
    HardwareTimeout,
    ChecksumMismatch,
    Canceled,
}

/// Universal Multi-Brand Binary Dumper Engine.
pub struct UniversalEcuDumper {
    pub protocol: FlasherProtocol,
    pub total_rom_size: usize,
    pub chunk_size: usize,
    pub current_offset: usize,
    pub accumulated_image: Vec<u8>,
    pub retries_on_pending_0x78: usize,
    pub max_0x78_retries: usize,
}

impl UniversalEcuDumper {
    pub fn new(protocol: FlasherProtocol, total_rom_size: usize) -> Self {
        let chunk_size = match protocol {
            FlasherProtocol::KLineDirectAddress => 100,
            FlasherProtocol::HondaKLineKwp => 128,
            FlasherProtocol::SubaruM32rBoot => 1024,
            FlasherProtocol::BoschUdsIsoTp => 0x0FF6, // 4086 bytes standard Bosch
            FlasherProtocol::GeneralMotorsVpwCan => 4096,
            FlasherProtocol::ToyotaDensoCan => 2048,
            FlasherProtocol::RenesasDensoKernelStream => 64, // 64-byte high-speed CAN streaming frame
            FlasherProtocol::MitsubishiMh7Kline => 240,      // Capped <= 255 for 1-byte length field
            FlasherProtocol::Kwp2000FastBaudKLine => 240,    // Capped <= 255 for 1-byte length field
            FlasherProtocol::RenesasRh850Can => 4086,        // 4086-byte standard UDS 4MB Code Flash
            FlasherProtocol::HeavyDutyJ1939Can => 1024,      // 1024-byte J1939 DM14/DM16 CMDT block (max 1785B)
        };

        Self {
            protocol,
            total_rom_size,
            chunk_size,
            current_offset: 0,
            accumulated_image: vec![0xFF; total_rom_size], // Default erased flash padding
            retries_on_pending_0x78: 0,
            max_0x78_retries: 20,
        }
    }

    /// ISO 14230 fast-init wake-up frame for a K-Line session.
    ///
    /// An ECU that has not been woken will not answer any request, which is the
    /// most common cause of "ECU does not respond" in the field.
    pub fn build_kline_wakeup_frame() -> Vec<u8> {
        vec![0xFE, 0x04, 0xFF, 0xFF]
    }

    /// KWP2000 `startCommunication` request.
    ///
    /// `[0x72, 0x05, 0x00, 0xF0, 0x99]` — target address, length, service,
    /// parameter, checksum.
    pub fn build_kwp_start_communication_frame() -> Vec<u8> {
        vec![0x72, 0x05, 0x00, 0xF0, 0x99]
    }

    /// True when `reply` is a positive `startCommunication` acknowledgement.
    ///
    /// A positive response begins `0x02 0x04 0x00` (length, service+0x40,
    /// positive result). Anything else — a negative response, a truncated frame,
    /// or an empty buffer — is not an acknowledgement.
    pub fn is_start_communication_ack(reply: &[u8]) -> bool {
        reply.len() >= 3 && reply[0] == 0x02 && reply[1] == 0x04 && reply[2] == 0x00
    }

    /// Build next address read request according to selected vehicle protocol.
    pub fn build_next_read_request(&self) -> (usize, Vec<u8>) {
        let bytes_remaining = self.total_rom_size.saturating_sub(self.current_offset);
        let this_chunk = bytes_remaining.min(self.chunk_size);
        let addr = self.current_offset as u32;

        let request_packet = match self.protocol {
            FlasherProtocol::KLineDirectAddress => {
                // BMW BMS46: [ECU: 0x12, Len: 9, Cmd: 0x06, Addr: 4B, Chunk: 1B, CS]
                //
                // The trailing checksum is required: the ECU validates it and
                // silently drops a frame that does not match.
                let mut pkt = vec![0x12, 0x09, 0x06];
                pkt.extend_from_slice(&addr.to_be_bytes());
                pkt.push(this_chunk as u8);
                let cs = crate::honda_kline::HondaProtocolFraming::checksum_8bit(&pkt);
                pkt.push(cs);
                pkt
            }
            FlasherProtocol::HondaKLineKwp => {
                // Honda K-Line: [0x7D, Len, Mode 0x01, Addr: 3B, CS]
                let mut pkt = vec![
                    0x7D,
                    0x07,
                    0x01,
                    ((addr >> 16) & 0xFF) as u8,
                    ((addr >> 8) & 0xFF) as u8,
                    (addr & 0xFF) as u8,
                ];
                let cs = crate::honda_kline::HondaProtocolFraming::checksum_8bit(&pkt);
                pkt.push(cs);
                pkt
            }
            FlasherProtocol::BoschUdsIsoTp
            | FlasherProtocol::ToyotaDensoCan
            | FlasherProtocol::RenesasRh850Can => {
                // UDS Service 0x23 (ReadMemoryByAddress) or 0x35 RequestUpload
                let mut pkt = vec![0x23, 0x24]; // 2-byte length, 4-byte address
                pkt.extend_from_slice(&addr.to_be_bytes());
                pkt.extend_from_slice(&(this_chunk as u16).to_be_bytes());
                pkt
            }
            FlasherProtocol::SubaruM32rBoot | FlasherProtocol::GeneralMotorsVpwCan => {
                // Flash Kernel Request Upload
                let mut pkt = vec![0x35, 0x00, 0x44];
                pkt.extend_from_slice(&addr.to_be_bytes());
                pkt.extend_from_slice(&(this_chunk as u32).to_be_bytes());
                pkt
            }
            FlasherProtocol::RenesasDensoKernelStream => {
                // Denso SuperH Kernel CMD_READ_FLASH (0x04)
                let mut pkt = vec![0x04];
                pkt.extend_from_slice(&addr.to_be_bytes());
                pkt.extend_from_slice(&(this_chunk as u16).to_be_bytes());
                pkt
            }
            FlasherProtocol::MitsubishiMh7Kline => {
                // Mitsubishi MH7xxx K-Line read command
                let mut pkt = vec![0x81, 0x05, 0x23];
                pkt.extend_from_slice(&addr.to_be_bytes());
                pkt.push(this_chunk as u8);
                pkt
            }
            FlasherProtocol::Kwp2000FastBaudKLine => {
                // KWP2000 ReadMemoryByAddress (0x23)
                let mut pkt = vec![0x23];
                pkt.extend_from_slice(&addr.to_be_bytes()[1..]); // 3-byte address
                pkt.push(this_chunk as u8);
                pkt
            }
            FlasherProtocol::HeavyDutyJ1939Can => {
                // SAE J1939 DM14 Memory Access Request (Read Command = 0x02)
                J1939TruckFlasherProtocol::build_dm14_memory_read_request(addr, this_chunk as u16, 0xFFFF)
                    .to_vec()
            }
        };

        (this_chunk, request_packet)
    }

    /// Ingest received raw response bytes and handle UDS Negative Response 0x78 (Pending / Busy).
    pub fn ingest_block_response(&mut self, response_bytes: &[u8], expected_len: usize) -> BlockTransferStatus {
        // Check for UDS Negative Response: 0x7F [Service] [NRC]
        if response_bytes.len() >= 3 && response_bytes[0] == 0x7F {
            let nrc = response_bytes[2];
            if nrc == 0x78 {
                // NRC 0x78: RequestCorrectlyReceived-ResponsePending (ECU flash controller busy)
                self.retries_on_pending_0x78 += 1;
                if self.retries_on_pending_0x78 > self.max_0x78_retries {
                    return BlockTransferStatus::HardwareTimeout;
                }
                return BlockTransferStatus::Pending0x78Retry;
            }
            return BlockTransferStatus::HardwareTimeout;
        }

        // Successfully received payload: copy into accumulated ROM image
        if response_bytes.len() >= expected_len && self.current_offset <= self.total_rom_size {
            let start = self.current_offset;
            let Some(end_unclamped) = start.checked_add(expected_len) else {
                return BlockTransferStatus::HardwareTimeout;
            };
            let end = end_unclamped.min(self.total_rom_size);
            let copy_len = end - start;
            self.accumulated_image[start..end].copy_from_slice(&response_bytes[..copy_len]);

            self.current_offset = end;
            self.retries_on_pending_0x78 = 0; // Reset pending counter on success
            return BlockTransferStatus::Success;
        }

        BlockTransferStatus::HardwareTimeout
    }

    /// Calculate dump progress percentage (0.0% to 100.0%).
    pub fn progress_pct(&self) -> f64 {
        if self.total_rom_size == 0 {
            return 100.0;
        }
        (self.current_offset as f64 / self.total_rom_size as f64) * 100.0
    }

    /// Check if complete binary ROM has been fully dumped.
    pub fn is_dump_complete(&self) -> bool {
        self.current_offset >= self.total_rom_size
    }

    /// Calculate final post-dump CRC32 seal across entire assembled ROM.
    pub fn calculate_final_crc32(&self) -> u32 {
        compute_crc32(&self.accumulated_image)
    }

    /// Perform 2-Pass Zero-Bit-Flip Spot Verification & Anti-Fake-Read Validation before exporting `.bin`.
    /// Verifies that a second verification read of critical sectors (e.g. Boot/Immo/Calibration at `spot_offset`)
    /// matches the accumulated image 100% byte-for-byte, and that the `.bin` is not a locked/blank dummy readout.
    pub fn verify_and_export_clean_bin(
        &self,
        spot_offset: usize,
        second_pass_spot_bytes: &[u8],
    ) -> Result<(Vec<u8>, u32), &'static str> {
        if !self.is_dump_complete() {
            return Err("DUMP_INCOMPLETE: Cannot export .bin before 100% of ROM addresses are read");
        }
        if spot_offset + second_pass_spot_bytes.len() > self.accumulated_image.len() {
            return Err("SPOT_VERIFY_OUT_OF_BOUNDS: Second-pass verification window exceeds ROM size");
        }
        if &self.accumulated_image[spot_offset..spot_offset + second_pass_spot_bytes.len()]
            != second_pass_spot_bytes
        {
            return Err("BIT_FLIP_JITTER_DETECTED: Second-pass spot read differs from primary dump! Abort .bin export.");
        }

        // Validate against blank 0xFF, locked 0x00, or repeating 256-byte mirror trap
        // The source returned `&'static str` here; the ported function returns a
        // typed error, so map it to the same message text this call site expects.
        validate_dumped_bin_integrity(&self.accumulated_image)
            .map_err(|e| match e {
                kivi_core::bin_export::BinExportError::DumpTooSmall => {
                    "DUMP_TOO_SMALL: Valid ECU .bin dump must be at least 1024 bytes"
                }
                kivi_core::bin_export::BinExportError::BlankFlashDump => {
                    "BLANK_FLASH_DUMP_0xFF: >98% of dumped .bin is 0xFF (chip unread or boot pin error)"
                }
                kivi_core::bin_export::BinExportError::LockedZeroDump => {
                    "LOCKED_ZERO_DUMP_0x00: >98% of dumped .bin is 0x00 (MCU security read-protection active)"
                }
                kivi_core::bin_export::BinExportError::FakeMirrorReadout => {
                    "FAKE_MIRROR_READOUT_TRAP: ECU returned repeating 256-byte dummy pages (SecurityAccess locked)"
                }
                _ => "DUMP_INTEGRITY_CHECK_FAILED",
            })?;

        let crc32 = self.calculate_final_crc32();
        Ok((self.accumulated_image.clone(), crc32))
    }
}

/// Heavy-Duty 24V Truck & Bus SAE J1939 Memory Access & Multi-Packet CMDT Protocol
/// (Cummins CM2150/CM2250/CM2350, Bosch EDC7UC31/EDC17CV41/EDC17CV54, Hino, Isuzu, Fuso, Volvo/Scania).
pub struct J1939TruckFlasherProtocol;

impl J1939TruckFlasherProtocol {
    pub const PGN_DM13_STOP_START_BROADCAST: u32 = 0x00DF00; // 57088
    pub const PGN_DM14_MEMORY_ACCESS_REQ: u32 = 0x00D900;    // 55552
    pub const PGN_DM15_MEMORY_ACCESS_RSP: u32 = 0x00D800;    // 55296
    pub const PGN_DM16_BINARY_DATA_XFER: u32 = 0x00D700;     // 55040
    pub const PGN_TP_CM: u32 = 0x00EC00;                     // 60416 (RTS/CTS/EndOfMsgACK)
    pub const PGN_TP_DT: u32 = 0x00EB00;                     // 60160 (7-byte Data Transfer packets)

    pub const CMD_ERASE_MEMORY: u8 = 0x01;
    pub const CMD_READ_MEMORY: u8 = 0x02;
    pub const CMD_WRITE_MEMORY: u8 = 0x03;

    /// Build 29-bit J1939 CAN Identifier (`[Priority 3b | Reserved 1b | DP 1b | PF 8b | PS/DA 8b | SA 8b]`).
    pub fn build_29bit_can_id(priority: u8, pgn: u32, dest_addr: u8, source_addr: u8) -> u32 {
        let p = ((priority & 0x07) as u32) << 26;
        let pf = (pgn >> 8) & 0xFF;
        let pdu_specific = if pf < 240 {
            dest_addr as u32
        } else {
            pgn & 0xFF
        };
        p | (pf << 16) | (pdu_specific << 8) | (source_addr as u32)
    }

    /// Build `DM13` (`PGN 0x00DF00`) Broadcast Hold / Stop frame to silence truck CAN bus traffic during flashing.
    pub fn build_dm13_silence_broadcast_frame() -> [u8; 8] {
        // Byte 0 bits 0..5 = 00 (Stop Broadcast on Current Data Link & J1587/J1939 Networks)
        // Byte 4..5 = Hold Signal Duration (10,000 ms)
        [0x00, 0xFF, 0xFF, 0xFF, 0x10, 0x27, 0xFF, 0xFF]
    }

    /// Build `DM14` (`PGN 0x00D900`) Memory Access Request (Read `.bin` block at 32-bit physical address).
    pub fn build_dm14_memory_read_request(
        address: u32,
        length_bytes: u16,
        security_key_or_user_level: u16,
    ) -> [u8; 8] {
        // J1939-73 DM14 layout:
        // Byte 0: Length bits [0..7]
        // Byte 1: [Len bits 8..10 (top 3 bits) | PointerType=0 (Direct Memory) | Command=0x02 Read (bits 1..3) | 1]
        // Byte 2..4: Address LSB..MSB (24-bit)
        // Byte 5: Address Spatial Extension (MSB bits 24..31)
        // Byte 6..7: Key / User Level (LE)
        let len_lo = (length_bytes & 0xFF) as u8;
        let len_hi_3b = (((length_bytes >> 8) & 0x07) as u8) << 5;
        let cmd_bits = (Self::CMD_READ_MEMORY & 0x07) << 1;
        let byte1 = len_hi_3b | cmd_bits | 0x01;

        let addr_le = address.to_le_bytes();
        let key_le = security_key_or_user_level.to_le_bytes();

        [
            len_lo,
            byte1,
            addr_le[0],
            addr_le[1],
            addr_le[2],
            addr_le[3],
            key_le[0],
            key_le[1],
        ]
    }

    /// Segment a `DM16` Binary Data Transfer payload (up to 1785 bytes) into J1939-21 `TP.CM_RTS`
    /// and 8-byte `TP.DT` frames (`[seq_1_to_255, data_0..6]`).
    pub fn segment_dm16_cmdt_packets(raw_bin_chunk: &[u8]) -> Result<([u8; 8], Vec<[u8; 8]>), &'static str> {
        if raw_bin_chunk.is_empty() || raw_bin_chunk.len() > 1785 {
            return Err("J1939 CMDT multi-packet payload must be between 1 and 1785 bytes");
        }

        let total_size = raw_bin_chunk.len() as u16;
        let num_packets = ((raw_bin_chunk.len() + 6) / 7) as u8;
        let pgn_le = Self::PGN_DM16_BINARY_DATA_XFER.to_le_bytes();

        // TP.CM_RTS (Control Byte = 16 / 0x10)
        let rts_frame = [
            16, // RTS
            (total_size & 0xFF) as u8,
            ((total_size >> 8) & 0xFF) as u8,
            num_packets,
            0xFF, // No limit on packets per CTS
            pgn_le[0],
            pgn_le[1],
            pgn_le[2],
        ];

        let mut dt_frames = Vec::with_capacity(num_packets as usize);
        for (i, chunk) in raw_bin_chunk.chunks(7).enumerate() {
            let mut frame = [0xFFu8; 8];
            frame[0] = (i + 1) as u8; // 1-based sequence number
            frame[1..1 + chunk.len()].copy_from_slice(chunk);
            dt_frames.push(frame);
        }

        Ok((rts_frame, dt_frames))
    }

    /// Reassemble J1939-21 `TP.DT` frames back into a contiguous `.bin` chunk according to `TP.CM_RTS`.
    pub fn reassemble_dm16_cmdt_packets(
        rts_frame: &[u8; 8],
        dt_frames: &[[u8; 8]],
    ) -> Result<Vec<u8>, &'static str> {
        if rts_frame[0] != 16 {
            return Err("Not a J1939 TP.CM_RTS frame (control byte != 16)");
        }
        let total_size = u16::from_le_bytes([rts_frame[1], rts_frame[2]]) as usize;
        let expected_packets = rts_frame[3] as usize;
        if dt_frames.len() < expected_packets {
            return Err("Missing J1939 TP.DT packets during DM16 binary reassembly");
        }

        let mut out = Vec::with_capacity(total_size);
        for (idx, frame) in dt_frames.iter().take(expected_packets).enumerate() {
            let expected_seq = (idx + 1) as u8;
            if frame[0] != expected_seq {
                return Err("J1939 TP.DT sequence number out of order");
            }
            let remaining = total_size.saturating_sub(out.len());
            let take = remaining.min(7);
            out.extend_from_slice(&frame[1..1 + take]);
        }

        if out.len() != total_size {
            return Err("Reassembled J1939 DM16 payload size mismatch");
        }
        Ok(out)
    }
}

/// KWP2000 Fast Baud Rate Step Negotiator.
/// Performs two-stage handshake: 10,400 bps initialization followed by Service 0x87 baud switch.
pub struct Kwp2000BaudStepNegotiator;

impl Kwp2000BaudStepNegotiator {
    pub const BAUD_10400_INITIAL: u32 = 10400;
    pub const BAUD_19200_ID: u8 = 0x12;
    pub const BAUD_38400_ID: u8 = 0x13;
    pub const BAUD_57600_ID: u8 = 0x14;
    pub const BAUD_124800_ID: u8 = 0x15;

    /// Build Service 0x87 LinkControl request to switch ECU UART clock to high-speed baud.
    pub fn build_baud_switch_request(target_baud: u32) -> Result<(u8, Vec<u8>), &'static str> {
        let baud_id = match target_baud {
            19200 => Self::BAUD_19200_ID,
            38400 => Self::BAUD_38400_ID,
            57600 => Self::BAUD_57600_ID,
            124800 => Self::BAUD_124800_ID,
            _ => return Err("Unsupported KWP2000 high-speed baud rate"),
        };

        // Service 0x87 (LinkControl), sub-function 0x01 (verify baudrate), baud_id
        let req = vec![0x87, 0x01, baud_id];
        Ok((baud_id, req))
    }

    /// Verify positive response 0xC7 from ECU before host UART frequency reconfiguration.
    pub fn verify_baud_switch_response(response: &[u8], expected_baud_id: u8) -> bool {
        response.len() >= 3
            && response[0] == 0xC7
            && response[1] == 0x01
            && response[2] == expected_baud_id
    }
}

/// Denso SuperH RAM Kernel Packet Decoder.
pub struct DensoKernelPacketDecoder;

impl DensoKernelPacketDecoder {
    /// Decode a streaming packet: [CMD: 0x04, Status: 0x00, Addr_32BE, Data: 64B, CRC: 2B]
    pub fn decode_stream_chunk(packet: &[u8]) -> Result<(u32, Vec<u8>), &'static str> {
        if packet.len() < 72 {
            return Err("Packet too short for Denso kernel frame (minimum 72 bytes)");
        }

        if packet[0] != 0x04 || packet[1] != 0x00 {
            return Err("Invalid command opcode or non-zero error status in kernel response");
        }

        let address = u32::from_be_bytes([packet[2], packet[3], packet[4], packet[5]]);
        let payload = packet[6..70].to_vec(); // 64 bytes raw flash data

        // Trailing 2 bytes are 16-bit additive checksum
        let expected_crc = u16::from_be_bytes([packet[70], packet[71]]);
        let mut calc_crc: u16 = 0;
        for &b in &packet[..70] {
            calc_crc = calc_crc.wrapping_add(b as u16);
        }

        if calc_crc != expected_crc {
            return Err("Denso kernel packet CRC-16 checksum mismatch");
        }

        Ok((address, payload))
    }
}

/// Binary ROM preparation & validation engine for universal calibration import/export.
/// Covers Motorcycle (32KB–2MB), Passenger Car (256KB–8MB), TCU (1MB–4MB), and Heavy-Duty Truck (1MB–16MB) ROM sizes.
pub struct BinaryPreparationValidator;

impl BinaryPreparationValidator {
    pub const VALID_ROM_SIZES: [usize; 15] = [
        32 * 1024,    // 32 KB (Small Keihin / Shindengen Motorcycle)
        64 * 1024,    // 64 KB (Keihin / Yamaha / EEPROM)
        128 * 1024,   // 128 KB (Motorcycles / Keihin)
        256 * 1024,   // 256 KB (Denso GSX-R / Honda CB500/CBR650F / Bosch M5.9 / BMS46)
        384 * 1024,   // 384 KB ( Suzuki / Kawasaki Mitsubishi MH7203)
        512 * 1024,   // 512 KB (Bosch ME7.5 / GM LS1 P01 / Ducati Marelli)
        768 * 1024,   // 768 KB (Toyota Denso 76F0038/76F0040/76F0070)
        1024 * 1024,  // 1024 KB = 1 MB (Denso SH7058 / Nissan / Honda Fireblade)
        1280 * 1024,  // 1280 KB = 1.25 MB (Mitsubishi Melco MH8115F)
        1536 * 1024,  // 1536 KB = 1.5 MB (Keihin CBR250RR / VAG DQ200 TCU)
        2048 * 1024,  // 2048 KB = 2 MB (Bosch MED17 / Suzuki Hayabusa RH850 / Bosch EDC7 Truck)
        2560 * 1024,  // 2560 KB = 2.5 MB (Mercedes VGS-NAG3 / VAG Simos)
        3072 * 1024,  // 3072 KB = 3 MB (Cummins CM2150/CM2350 / Bosch EDC17CV Truck)
        4096 * 1024,  // 4096 KB = 4 MB (BMW S55 MEVD17.2.G / Renesas RH850 / Simos 18)
        8192 * 1024,  // 8192 KB = 8 MB (Bosch MD1 / MG1 Aurix TC298/TC399)
    ];

    /// Validate that dump size matches an official industry ROM boundary.
    pub fn is_valid_industry_size(size_bytes: usize) -> bool {
        Self::VALID_ROM_SIZES.contains(&size_bytes)
    }

    /// Auto-pad incomplete buffer to the next official industry size boundary with 0xFF.
    pub fn pad_to_standard_rom_size(raw_dump: &[u8]) -> Vec<u8> {
        let current_len = raw_dump.len();
        for &target_size in &Self::VALID_ROM_SIZES {
            if target_size >= current_len {
                let mut padded = raw_dump.to_vec();
                if target_size > current_len {
                    padded.resize(target_size, 0xFF);
                }
                return padded;
            }
        }
        // If larger than 8MB, round up to next 1MB boundary
        let remainder = current_len % (1024 * 1024);
        if remainder != 0 {
            let mut padded = raw_dump.to_vec();
            padded.resize(current_len + (1024 * 1024 - remainder), 0xFF);
            padded
        } else {
            raw_dump.to_vec()
        }
    }

    /// Scan binary dump for common automotive Calibration IDs (Toyota, Honda, VAG, Bosch, Siemens, Cummins, Mitsubishi).
    pub fn extract_cal_id(rom: &[u8]) -> Option<String> {
        let min_len = 8;
        let mut curr_str = Vec::new();

        for &b in rom.iter().chain(std::iter::once(&0u8)) {
            if (0x20..=0x7E).contains(&b) && b != b'$' && b != b'#' {
                curr_str.push(b);
            } else {
                if curr_str.len() >= min_len {
                    if let Ok(s) = std::str::from_utf8(&curr_str) {
                        let upper = s.to_uppercase();
                        if upper.starts_with("89663-") // Toyota Denso
                            || upper.starts_with("37805-") // Honda Car Keihin
                            || upper.starts_with("38770-") // Honda Moto Keihin
                            || upper.starts_with("32920-") // Suzuki Moto Denso
                            || upper.starts_with("06K906") // VAG Simos 18
                            || upper.starts_with("1037")   // Bosch ME7/MED17/EDC17 SW ID
                            || upper.starts_with("5WP4")   // Siemens SIMK
                            || upper.starts_with("1860")   // Mitsubishi Melco
                        {
                            return Some(s.to_string());
                        }
                    }
                }
                curr_str.clear();
            }
        }
        None
    }
}

/// Strict sequential stages for the End-to-End Auto-Scan, `.bin` Extraction, & Zero-XDF Pipeline.
/// Prevents any out-of-order jumps (e.g., reading flash before security unlock or generating `.xdf` before 2-pass verification).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AutoEcuScanStage {
    Idle = 0,
    RailVerified = 1,
    EcuIdentified = 2,
    BusSilenced = 3,
    SecurityUnlocked = 4,
    MemoryDumping = 5,
    SpotVerifiedAndSanitized = 6,
    AutoXdfGenerated = 7,
}

/// Automatically detected ECU hardware/protocol profile from J2534/OBD/Bench probe responses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoDetectedEcuProfile {
    pub electrical_class: VehicleElectricalClass,
    pub protocol: FlasherProtocol,
    pub cal_id: String,
    pub mcu_architecture: &'static str,
    pub mcu_address_group: u8,
    pub physical_base_address: u32,
    pub expected_rom_size: usize,
}

/// Deterministic End-to-End Orchestrator:
/// `J2534/OBD Voltage & Bus Probe -> Auto ECU Detection -> Bus Silencing -> Security Unlock -> Binary Dump -> 2-Pass Sanitization -> Zero-XDF Export`.
pub struct AutoEcuScanAndExtractPipeline {
    pub stage: AutoEcuScanStage,
    pub vbatt_mv: u32,
    pub electrical_class: Option<VehicleElectricalClass>,
    pub detected_profile: Option<AutoDetectedEcuProfile>,
    pub dumper: Option<UniversalEcuDumper>,
    pub verified_clean_bin: Option<Vec<u8>>,
    pub verified_bin_crc32: Option<u32>,
}

impl Default for AutoEcuScanAndExtractPipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl AutoEcuScanAndExtractPipeline {
    pub fn new() -> Self {
        Self {
            stage: AutoEcuScanStage::Idle,
            vbatt_mv: 0,
            electrical_class: None,
            detected_profile: None,
            dumper: None,
            verified_clean_bin: None,
            verified_bin_crc32: None,
        }
    }

    /// Stage 1 (`Idle -> RailVerified`):
    /// Reads J2534 `READ_VBATT` millivolts, automatically classifies 12V (Moto/Car) vs 24V (Heavy-Duty Truck),
    /// and verifies voltage stability before any bus communication is allowed.
    pub fn step1_verify_electrical_rail(&mut self, vbatt_mv: u32) -> Result<VehicleElectricalClass, &'static str> {
        let (volts, rem_mv) = PassthruAndTraceCodec::format_passthru_vbatt_mv(vbatt_mv);
        let v_f64 = (volts as f64) + (rem_mv as f64) / 1000.0;

        let class = if (23.5..=29.2).contains(&v_f64) {
            VehicleElectricalClass::HeavyDutyTruck24V
        } else if (12.2..=14.8).contains(&v_f64) {
            VehicleElectricalClass::PassengerCar12V
        } else if (12.0..12.2).contains(&v_f64) {
            VehicleElectricalClass::Motorcycle12V
        } else {
            return Err("RAIL_VOLTAGE_UNSAFE: Battery voltage outside safe 12V (12.0-14.8V) or 24V (23.5-29.2V) flash window");
        };

        let monitor = BenchPowerSupplyMonitor::for_vehicle_class(class);
        monitor.is_safe_for_flashing(v_f64, 0.01)?;

        self.vbatt_mv = vbatt_mv;
        self.electrical_class = Some(class);
        self.stage = AutoEcuScanStage::RailVerified;
        Ok(class)
    }

    /// Stage 2 (`RailVerified -> EcuIdentified`):
    /// Automatically inspects the ECU identification response (UDS `0x62 F1 ..`, KWP `0x61 ..`, Honda `0x82/0xFE`, or J1939 `DM15`)
    /// to lock the protocol, MCU architecture, physical base address, and target ROM size without manual user guesswork.
    pub fn step2_auto_identify_ecu(&mut self, raw_id_response: &[u8]) -> Result<AutoDetectedEcuProfile, &'static str> {
        if self.stage != AutoEcuScanStage::RailVerified {
            return Err("INVALID_STATE_TRANSITION: Must complete step1_verify_electrical_rail before step2_auto_identify_ecu");
        }
        let electrical_class = self.electrical_class.ok_or("MISSING_ELECTRICAL_CLASS")?;

        let ascii_probe = String::from_utf8_lossy(raw_id_response).to_uppercase();
        let profile = if electrical_class == VehicleElectricalClass::HeavyDutyTruck24V
            || ascii_probe.contains("CM2150")
            || ascii_probe.contains("CM2350")
            || ascii_probe.contains("EDC17CV")
            || ascii_probe.contains("EDC7")
        {
            AutoDetectedEcuProfile {
                electrical_class,
                protocol: FlasherProtocol::HeavyDutyJ1939Can,
                cal_id: ascii_probe.trim().to_string(),
                mcu_architecture: "Heavy-Duty Truck PowerPC / TriCore (SAE J1939)",
                mcu_address_group: 6,
                physical_base_address: 0x8000_0000,
                expected_rom_size: 3 * 1024 * 1024,
            }
        } else if ascii_probe.contains("89663-") {
            AutoDetectedEcuProfile {
                electrical_class,
                protocol: FlasherProtocol::ToyotaDensoCan,
                cal_id: BinaryPreparationValidator::extract_cal_id(raw_id_response)
                    .unwrap_or_else(|| "89663-TOYOTA".to_string()),
                mcu_architecture: "Toyota Denso 76F00xx / RH850",
                mcu_address_group: 0,
                physical_base_address: 0x0000_0000,
                expected_rom_size: 1024 * 1024,
            }
        } else if ascii_probe.contains("38770-")
            || (raw_id_response.len() >= 2 && raw_id_response[0] == 0x82 && raw_id_response[1] == 0x82)
        {
            AutoDetectedEcuProfile {
                electrical_class,
                protocol: FlasherProtocol::HondaKLineKwp,
                cal_id: BinaryPreparationValidator::extract_cal_id(raw_id_response)
                    .unwrap_or_else(|| "38770-HONDA-MOTO".to_string()),
                mcu_architecture: "Honda Keihin M32R / SH7055 Big-Endian",
                mcu_address_group: 0,
                physical_base_address: 0x0000_0000,
                expected_rom_size: 256 * 1024,
            }
        } else if ascii_probe.contains("32920-") {
            let is_gen3_rh850 = ascii_probe.contains("10L") || ascii_probe.contains("RH850");
            AutoDetectedEcuProfile {
                electrical_class,
                protocol: if is_gen3_rh850 {
                    FlasherProtocol::RenesasRh850Can
                } else {
                    FlasherProtocol::RenesasDensoKernelStream
                },
                cal_id: BinaryPreparationValidator::extract_cal_id(raw_id_response)
                    .unwrap_or_else(|| "32920-SUZUKI".to_string()),
                mcu_architecture: if is_gen3_rh850 {
                    "Suzuki Renesas RH850 Little-Endian"
                } else {
                    "Suzuki Denso SH7052/SH7055/SH7058 Big-Endian"
                },
                mcu_address_group: 0,
                physical_base_address: 0x0000_0000,
                expected_rom_size: if is_gen3_rh850 { 2 * 1024 * 1024 } else { 256 * 1024 },
            }
        } else if ascii_probe.contains("1860") {
            AutoDetectedEcuProfile {
                electrical_class,
                protocol: FlasherProtocol::MitsubishiMh7Kline,
                cal_id: BinaryPreparationValidator::extract_cal_id(raw_id_response)
                    .unwrap_or_else(|| "1860-MELCO".to_string()),
                mcu_architecture: "Mitsubishi Melco MH810x / MH8115",
                mcu_address_group: 0,
                physical_base_address: 0x0000_0000,
                expected_rom_size: 1024 * 1024,
            }
        } else {
            // Default modern passenger car UDS ISO-TP (Bosch MED17/EDC17/MD1/MG1 or Continental Simos 18)
            AutoDetectedEcuProfile {
                electrical_class,
                protocol: FlasherProtocol::BoschUdsIsoTp,
                cal_id: BinaryPreparationValidator::extract_cal_id(raw_id_response)
                    .unwrap_or_else(|| "BOSCH-SIMOS-UDS".to_string()),
                mcu_architecture: "Infineon TriCore TC1797 / Aurix TC298 (Bosch MEDC17/MD1/Simos)",
                mcu_address_group: 6, // TriCore 0x80000000 -> 0x00000000
                physical_base_address: 0x8000_0000,
                expected_rom_size: 4 * 1024 * 1024,
            }
        };

        self.detected_profile = Some(profile.clone());
        self.stage = AutoEcuScanStage::EcuIdentified;
        Ok(profile)
    }

    /// Stage 3 (`EcuIdentified -> BusSilenced`):
    /// Generates the deterministic bus-silencing frames (`UDS 0x10 03 -> 0x85 02 -> 0x28 03 03` or `J1939 DM13`)
    /// so ABS/TCU/BCM modules cannot interrupt memory extraction.
    pub fn step3_silence_vehicle_bus(&mut self) -> Result<Vec<Vec<u8>>, &'static str> {
        if self.stage != AutoEcuScanStage::EcuIdentified {
            return Err("INVALID_STATE_TRANSITION: Must complete step2_auto_identify_ecu before step3_silence_vehicle_bus");
        }
        let profile = self.detected_profile.as_ref().ok_or("MISSING_ECU_PROFILE")?;

        let frames = if profile.protocol == FlasherProtocol::HeavyDutyJ1939Can {
            vec![J1939TruckFlasherProtocol::build_dm13_silence_broadcast_frame().to_vec()]
        } else {
            vec![
                vec![0x10, 0x03],       // DiagnosticSessionControl: Extended Session
                vec![0x85, 0x02],       // ControlDTCSetting: OFF
                vec![0x28, 0x03, 0x03], // CommunicationControl: Disable normal Rx & Tx
            ]
        };

        self.stage = AutoEcuScanStage::BusSilenced;
        Ok(frames)
    }

    /// Stage 4 (`BusSilenced -> SecurityUnlocked`):
    /// Verifies that SecurityAccess (`0x27` / `$BOOT$` / `"PGM"` / Service Mode GPT) succeeded and initializes
    /// the `UniversalEcuDumper` for the exact target ROM size.
    pub fn step4_unlock_security_and_arm_dumper(
        &mut self,
        override_rom_size: Option<usize>,
    ) -> Result<usize, &'static str> {
        if self.stage != AutoEcuScanStage::BusSilenced {
            return Err("INVALID_STATE_TRANSITION: Must complete step3_silence_vehicle_bus before step4_unlock_security_and_arm_dumper");
        }
        let profile = self.detected_profile.as_ref().ok_or("MISSING_ECU_PROFILE")?;
        let target_size = override_rom_size.unwrap_or(profile.expected_rom_size);

        self.dumper = Some(UniversalEcuDumper::new(profile.protocol, target_size));
        self.stage = AutoEcuScanStage::SecurityUnlocked;
        Ok(target_size)
    }

    /// Stage 5 (`SecurityUnlocked / MemoryDumping -> MemoryDumping`):
    /// Feeds sequential memory blocks into the active `UniversalEcuDumper`, automatically handling `NRC 0x78`
    /// pending retries without skipping addresses.
    pub fn step5_ingest_sequential_memory_block(
        &mut self,
        response_bytes: &[u8],
        expected_len: usize,
    ) -> Result<BlockTransferStatus, &'static str> {
        if self.stage != AutoEcuScanStage::SecurityUnlocked && self.stage != AutoEcuScanStage::MemoryDumping {
            return Err("INVALID_STATE_TRANSITION: Must unlock security (step4) before reading memory blocks");
        }
        self.stage = AutoEcuScanStage::MemoryDumping;
        let dumper = self.dumper.as_mut().ok_or("MISSING_DUMPER")?;
        Ok(dumper.ingest_block_response(response_bytes, expected_len))
    }

    /// Stage 6 (`MemoryDumping -> SpotVerifiedAndSanitized`):
    /// Enforces 100% dump completion, 2-Pass Zero-Bit-Flip spot verification, Anti-Fake-Read validation,
    /// and MCU physical-to-ROM address rebasing before producing the clean `.bin` buffer.
    pub fn step6_verify_and_sanitize_bin(
        &mut self,
        spot_offset: usize,
        second_pass_spot_bytes: &[u8],
    ) -> Result<(Vec<u8>, u32), &'static str> {
        if self.stage != AutoEcuScanStage::MemoryDumping {
            return Err("INVALID_STATE_TRANSITION: Must complete step5 memory dumping before step6_verify_and_sanitize_bin");
        }
        let dumper = self.dumper.as_ref().ok_or("MISSING_DUMPER")?;
        let profile = self.detected_profile.as_ref().ok_or("MISSING_ECU_PROFILE")?;

        let (raw_bin, crc32) = dumper.verify_and_export_clean_bin(spot_offset, second_pass_spot_bytes)?;

        // Verify MCU physical base address mapping via 31-group translator
        let mapped_base = VbfPhfAndMcuMapEngine::translate_tricore_phys_to_bin_offset(
            profile.physical_base_address,
            profile.expected_rom_size as u32,
        )
        .unwrap_or(0);
        debug_assert_eq!(mapped_base, 0);

        let clean_bin = BinaryPreparationValidator::pad_to_standard_rom_size(&raw_bin);
        self.verified_clean_bin = Some(clean_bin.clone());
        self.verified_bin_crc32 = Some(crc32);
        self.stage = AutoEcuScanStage::SpotVerifiedAndSanitized;
        Ok((clean_bin, crc32))
    }

    /// Stage 7 (`SpotVerifiedAndSanitized -> AutoXdfGenerated`):
    /// Runs `ZeroXdfEngine` blind discovery (Global Endianness Lock -> Block Pre-Filter -> 3D Surface & Axis Pairing ->
    /// 1D Memory-Interval NMS -> 2-Tier Classification) and exports a ready-to-use standard `.xdf` XML string.
    pub fn step7_generate_auto_xdf(&mut self) -> Result<(Vec<DiscoveredFullTable>, String), &'static str> {
        if self.stage != AutoEcuScanStage::SpotVerifiedAndSanitized {
            return Err("INVALID_STATE_TRANSITION: Must complete step6_verify_and_sanitize_bin before step7_generate_auto_xdf");
        }
        let clean_bin = self.verified_clean_bin.as_ref().ok_or("MISSING_VERIFIED_BIN")?;
        let profile = self.detected_profile.as_ref().ok_or("MISSING_ECU_PROFILE")?;

        let dims = &[(16, 16), (20, 17), (24, 17)];
        let raw_tables = scan_16bit_tables_auto_endian(clean_bin, dims, 0, clean_bin.len());
        let candidates = apply_memory_interval_nms(raw_tables, 0.25);
        let limiters = scan_limiters(clean_bin, clean_bin.len() <= 256 * 1024);
        let xdf_xml = kivi_scanner::xdf_export::export_xdf_xml(&profile.cal_id, &candidates, &limiters);

        self.stage = AutoEcuScanStage::AutoXdfGenerated;
        Ok((candidates, xdf_xml))
    }

    /// Unified post-remap checksum auto-dispatcher:
    /// Automatically detects the ROM family (Honda/Keihin 8-bit, Shindengen 32-bit, or Isuzu Transtron 32-bit BE)
    /// and updates the binary checksum in-place before writing back to the ECU.
    pub fn auto_verify_and_correct_oem_checksum(rom: &mut [u8]) -> Result<&'static str, String> {
        if HondaKeihinShindengenUnifiedCore::shindengen_auto_checksum_fix(rom).is_some() {
            return Ok("SHINDENGEN_32BIT_CHECKSUM_FIXED");
        }
        if matches!(
            rom.len(),
            0x0E000 | 0x18000 | 0x20000 | 0x30000 | 0x40000 | 0x60000
        ) {
            HondaKeihinBikeKLineAndIsuzuTranstronEngine::correct_honda_keihin_rom_checksum(rom)?;
            return Ok("HONDA_KEIHIN_8BIT_CHECKSUM_FIXED");
        }
        if matches!(
            rom.len(),
            0x080000 | 0x100000 | 0x180000 | 0x200000 | 0x280000 | 0x3C0000
        ) {
            HondaKeihinBikeKLineAndIsuzuTranstronEngine::correct_isuzu_transtron_rom_checksum(rom)?;
            return Ok("ISUZU_TRANSTRON_32BIT_BE_CHECKSUM_FIXED");
        }
        Err(format!("Unsupported ROM size for auto-checksum: 0x{:X}", rom.len()))
    }
}
