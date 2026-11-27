//! Ported from `ecu_engine_rust::flashing::ram_kernel`.
//!
//! Pure byte-level protocol logic: builders, parsers, encoders, state machines.
//! No I/O, no device access, no `unsafe`. Transport is the caller's concern.
//!
//! Verified by unit tests with hand-derived expectations. **Not** validated
//! against a physical ECU; no such claim is made.

/// Target MCU Architecture for Flash Kernel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KernelArchitecture {
    Motorola68k,      // GM P01/P59 (68332, RAM at 0xFF8000)
    PowerPcMpc5xx,    // GM E38 / Bosch EDC16 (PowerPC, RAM at 0x3FC434)
    PowerPce200z6,    // GM E39/E92 (MPC5566, RAM at 0x40007004)
    MitsubishiM32R,   // Subaru / Mitsubishi Hitachi (uj20_kernel.bin, RAM at 0x008000)
    InfineonTriCore,  // Simos 18 CBOOT RAM (TC1791, RAM at 0xD0008000)
    RenesasSuperH,    // Denso Toyota 2GD / Keihin CBR250RR (SH7058 / SH72544)
}

/// State of in-RAM Flash Kernel execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KernelState {
    Disconnected,
    KernelUploaded,
    KernelRunning,
    ReadingFlash,
    WritingFlash,
    RestoringStockOs,
}

/// In-RAM Flash Kernel Ingestion & Streaming Session Controller.
pub struct RamKernelSession {
    pub arch: KernelArchitecture,
    pub ram_load_address: u32,
    pub kernel_payload: Vec<u8>,
    pub state: KernelState,
    pub max_read_block_size: usize,
    pub bytes_streamed: usize,
    pub watchdogs_scratched: usize,
}

impl RamKernelSession {
    pub fn new(arch: KernelArchitecture, ram_load_address: u32, kernel_payload: Vec<u8>) -> Self {
        let max_block = match arch {
            KernelArchitecture::Motorola68k => 4096,
            KernelArchitecture::PowerPcMpc5xx | KernelArchitecture::PowerPce200z6 => 4096,
            KernelArchitecture::MitsubishiM32R => 1024,
            KernelArchitecture::InfineonTriCore => 4086,
            KernelArchitecture::RenesasSuperH => 2048,
        };

        Self {
            arch,
            ram_load_address,
            kernel_payload,
            state: KernelState::Disconnected,
            max_read_block_size: max_block,
            bytes_streamed: 0,
            watchdogs_scratched: 0,
        }
    }

    /// Build Request Permission to Download Kernel into RAM (UDS Service 0x34).
    pub fn build_kernel_upload_request(&self) -> Vec<u8> {
        let len = self.kernel_payload.len() as u32;
        let mut pkt = Vec::with_capacity(11);
        pkt.push(0x34); // RequestDownload
        pkt.push(0x00); // Uncompressed, unencrypted
        pkt.push(0x44); // 4-byte address, 4-byte length
        pkt.extend_from_slice(&self.ram_load_address.to_be_bytes());
        pkt.extend_from_slice(&len.to_be_bytes());
        pkt
    }

    /// Split Kernel binary into sequential TransferData (Service 0x36) blocks.
    pub fn build_kernel_transfer_blocks(&self, block_chunk_size: usize) -> Vec<Vec<u8>> {
        if block_chunk_size == 0 {
            return Vec::new();
        }
        let mut blocks = Vec::new();
        let mut block_seq: u8 = 1;

        for chunk in self.kernel_payload.chunks(block_chunk_size) {
            let mut pkt = Vec::with_capacity(chunk.len() + 2);
            pkt.push(0x36); // TransferData
            pkt.push(block_seq);
            pkt.extend_from_slice(chunk);
            blocks.push(pkt);
            block_seq = block_seq.wrapping_add(1);
        }

        blocks
    }

    /// Build Jump to RAM Entry Point instruction (Mode 0x31 Routine Control or Mode 0x34 Entry).
    pub fn build_execute_kernel_command(&mut self) -> Vec<u8> {
        self.state = KernelState::KernelRunning;
        let mut pkt = vec![0x31, 0x01, 0x02, 0x00]; // StartRoutine: Run RAM Kernel
        pkt.extend_from_slice(&self.ram_load_address.to_be_bytes());
        pkt
    }

    /// Build In-RAM Fast Physical Flash Read Command (Mode 0x35 Request Upload).
    pub fn build_ram_read_block_command(&self, flash_physical_addr: u32, length: u16) -> Vec<u8> {
        let mut pkt = vec![
            0x6D, // VPW/CAN Header target ECU
            0xF0,
            0x10,
            0x35, // Mode 35: Read from Flash via RAM pointer
            0x01,
        ];
        pkt.extend_from_slice(&length.to_be_bytes());
        pkt.push((flash_physical_addr >> 16) as u8);
        pkt.push((flash_physical_addr >> 8) as u8);
        pkt.push((flash_physical_addr & 0xFF) as u8);
        pkt
    }

    /// Process received flash data stream from running RAM Kernel.
    pub fn process_streamed_flash_chunk(&mut self, payload: &[u8], dest_buffer: &mut [u8], offset: usize) -> Result<usize, &'static str> {
        let Some(end) = offset.checked_add(payload.len()) else {
            return Err("Streamed flash offset + payload length overflow");
        };
        if end > dest_buffer.len() {
            return Err("Streamed flash payload exceeds destination buffer capacity");
        }
        dest_buffer[offset..end].copy_from_slice(payload);
        self.bytes_streamed += payload.len();
        Ok(payload.len())
    }

    /// Scratch hardware watchdog timer to prevent ECU reboot during prolonged reading.
    pub fn scratch_watchdog_ping(&mut self) -> Vec<u8> {
        self.watchdogs_scratched += 1;
        vec![0x3E, 0x80] // TesterPresent with suppress positive response
    }

    /// Return ECU safely to Stock Operating System (Exit Kernel).
    pub fn build_exit_kernel_and_reboot(&mut self) -> Vec<u8> {
        self.state = KernelState::RestoringStockOs;
        vec![0x11, 0x01] // ECUReset HardReset
    }
}

/// Microcontroller RAM Flash Kernel Command Protocol (Embedded Diagnostic In-RAM Bootloader).
pub struct KernelCommandDispatcher;

impl KernelCommandDispatcher {
    pub const CMD_PING: u8 = 0x01;
    pub const CMD_QUERY_CHIP: u8 = 0x02;
    pub const CMD_ERASE_SECTOR: u8 = 0x03;
    pub const CMD_WRITE_CHUNK: u8 = 0x04;
    pub const CMD_VERIFY_CRC: u8 = 0x05;
    pub const CMD_SOFT_RESET: u8 = 0x06;

    pub const RSP_ACK_OFFSET: u8 = 0x40;

    /// Build binary command packet to send to running RAM kernel.
    pub fn build_kernel_packet(cmd: u8, address: u32, payload: &[u8]) -> Vec<u8> {
        let mut pkt = Vec::with_capacity(5 + payload.len());
        pkt.push(cmd);
        pkt.extend_from_slice(&address.to_be_bytes());
        pkt.extend_from_slice(payload);
        pkt
    }

    /// Parse response packet from running RAM kernel.
    pub fn parse_kernel_response(response: &[u8]) -> Result<(u8, &[u8]), &'static str> {
        if response.is_empty() {
            return Err("Empty kernel response");
        }
        let rsp_code = response[0];
        let payload = &response[1..];
        Ok((rsp_code, payload))
    }
}
