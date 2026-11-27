//! Hardware [`KernelSession`] adapters for Honda K-Line (Keihin / Shindengen)
//! and multi-architecture in-RAM Flash Kernels over a shared [`Device`].
//!
//! Verified via loopback device tests against exact byte-level wire framing.
//! Physical ECU execution requires bench hardware and stabilization checks.

use crate::hardware_kernel_session::SharedDevice;
use crate::honda_kline::{
    HondaEcuType, HondaEcuVendor, HondaKLineSession, HondaProtocolFraming, ShindengenSeedKeyEngine,
};
use crate::j2534_types::{tx_flags, PassThruMsg, ProtocolId};
use crate::kernel_session::{KernelError, KernelOperation, KernelSession, SessionPhase};
use crate::ram_kernel::{KernelArchitecture, KernelCommandDispatcher, RamKernelSession};
use crate::uds::uds_sid;

// ===========================================================================
// 1. Honda K-Line Kernel Session (Keihin H8C/M32R & Shindengen V850/RH850)
// ===========================================================================

pub struct HondaKLineKernelSession {
    device: SharedDevice,
    honda: HondaKLineSession,
    image_size: usize,
    enable_fast_baud: bool,
    shindengen_rom: Option<Vec<u8>>,
    erased_full_chip: bool,
    phase: SessionPhase,
    operation: Option<KernelOperation>,
    cleanup_count: usize,
}

impl HondaKLineKernelSession {
    pub fn new(
        device: SharedDevice,
        vendor: HondaEcuVendor,
        ecu_type: HondaEcuType,
        image_size: usize,
    ) -> Self {
        Self {
            device,
            honda: HondaKLineSession::new(vendor, ecu_type),
            image_size,
            enable_fast_baud: true,
            shindengen_rom: None,
            erased_full_chip: false,
            phase: SessionPhase::Idle,
            operation: None,
            cleanup_count: 0,
        }
    }

    pub fn with_fast_baud(mut self, enabled: bool) -> Self {
        self.enable_fast_baud = enabled;
        self
    }

    pub fn with_shindengen_key_rom(mut self, rom_bytes: Vec<u8>) -> Self {
        self.shindengen_rom = Some(rom_bytes);
        self
    }

    fn exchange_frame(&mut self, frame: &[u8]) -> Result<Vec<u8>, KernelError> {
        let mut dev = self
            .device
            .0
            .lock()
            .map_err(|_| KernelError::UploadRejected)?;
        let proto = dev.connected_protocol().unwrap_or(ProtocolId::Iso14230);
        let msg = PassThruMsg::with_flags(proto, 0, frame.to_vec());
        dev.send(&msg).map_err(|_| KernelError::UploadRejected)?;

        // K-Line is single-wire half-duplex: discard TX echo frames before accepting ECU reply.
        for _ in 0..4 {
            let reply = dev.receive().map_err(|_| KernelError::KernelDidNotStart)?;
            if reply.data == frame {
                continue;
            }
            if !HondaProtocolFraming::verify_packet(&reply.data) {
                return Err(KernelError::NegativeResponse(0x10));
            }
            if reply.data.len() >= 4 && reply.data[2] == 0x7F {
                let nrc = reply.data.get(4).copied().unwrap_or(0x10);
                return Err(KernelError::NegativeResponse(nrc));
            }
            return Ok(reply.data);
        }
        Err(KernelError::KernelDidNotStart)
    }
}

impl KernelSession for HondaKLineKernelSession {
    fn phase(&self) -> SessionPhase {
        self.phase
    }

    fn operation(&self) -> Option<KernelOperation> {
        self.operation
    }

    fn is_kernel_running(&self) -> bool {
        matches!(self.phase, SessionPhase::KernelRunning)
    }

    fn start(&mut self, operation: KernelOperation) -> Result<(), KernelError> {
        {
            let mut dev = self.device.0.lock().map_err(|_| KernelError::UploadRejected)?;
            if !dev.is_mock()
                && !matches!(
                    dev.connected_protocol(),
                    Some(ProtocolId::Iso14230 | ProtocolId::Iso9141)
                )
            {
                dev.set_kline_no_checksum(true);
                let _ = dev.disconnect();
                if dev.connect(ProtocolId::Iso14230, 10_400).is_err() {
                    dev.connect(ProtocolId::Iso9141, 10_400)
                        .map_err(|_| KernelError::KernelDidNotStart)?;
                }
            }
            match dev.kline_wakeup_pulse(70, 120) {
                Ok(()) | Err(crate::device::DeviceError::Unsupported(_)) => {}
                Err(_) => return Err(KernelError::KernelDidNotStart),
            }
        }

        let wakeup = HondaKLineSession::build_wakeup_packet();
        let _ = self.exchange_frame(&wakeup)?;

        if self.enable_fast_baud {
            let baud_cmd = self.honda.build_fast_baud_command();
            let _ = self.exchange_frame(&baud_cmd)?;
            {
                let mut dev = self
                    .device
                    .0
                    .lock()
                    .map_err(|_| KernelError::UploadRejected)?;
                let proto = dev.connected_protocol().unwrap_or(ProtocolId::Iso14230);
                dev.disconnect().map_err(|_| KernelError::UploadRejected)?;
                dev.connect(proto, 62_500)
                    .map_err(|_| KernelError::UploadRejected)?;
            }
            self.honda.is_fast_baud_active = true;
            self.honda.current_baud = 62_500;
        }
        self.erased_full_chip = false;

        let unlock_frames = self.honda.build_unlock_request();
        match self.honda.vendor {
            HondaEcuVendor::Keihin => {
                for pkt in unlock_frames {
                    let _ = self.exchange_frame(&pkt)?;
                }
            }
            HondaEcuVendor::Shindengen => {
                let seed_req = unlock_frames
                    .into_iter()
                    .next()
                    .ok_or(KernelError::UploadRejected)?;
                let rep = self.exchange_frame(&seed_req)?;
                if matches!(operation, KernelOperation::Write | KernelOperation::ReadModifyWrite) {
                    if rep.len() < 7 {
                        return Err(KernelError::UploadRejected);
                    }
                    let seed = [rep[rep.len() - 3], rep[rep.len() - 2]];
                    let rom = self
                        .shindengen_rom
                        .as_deref()
                        .ok_or(KernelError::UploadRejected)?;
                    let key = ShindengenSeedKeyEngine::compute_write_key(rom, seed)
                        .map_err(|_| KernelError::UploadRejected)?;
                    let key_pkt = HondaProtocolFraming::build_packet(0x7B, 0x02, 0x06, &key);
                    let _ = self.exchange_frame(&key_pkt)?;
                }
            }
        }

        self.phase = SessionPhase::KernelRunning;
        self.operation = Some(operation);
        Ok(())
    }

    fn read_memory_block(&mut self, address: u32, length: usize) -> Result<Vec<u8>, KernelError> {
        if !self.is_kernel_running() {
            return Err(KernelError::KernelNotRunning);
        }
        let mut out = Vec::with_capacity(length);
        let mut offset = 0usize;
        while offset < length {
            let take = (length - offset).min(128);
            let req = self
                .honda
                .build_read_chunk_request(address as usize + offset, take as u8);
            let rep = self.exchange_frame(&req)?;
            if rep.len() < take + 4 {
                return Err(KernelError::LengthMismatch {
                    expected: take,
                    got: rep.len().saturating_sub(4),
                });
            }
            let payload_end = rep.len() - 1;
            let payload_start = payload_end - take;
            out.extend_from_slice(&rep[payload_start..payload_end]);
            offset += take;
        }
        Ok(out)
    }

    fn erase_range(&mut self, address: u32, _length: usize) -> Result<(), KernelError> {
        if !self.is_kernel_running() {
            return Err(KernelError::KernelNotRunning);
        }
        if self.erased_full_chip {
            return Err(KernelError::UploadRejected);
        }
        let _ = address;
        let cmd = self.honda.build_erase_command();
        let _ = self.exchange_frame(&cmd)?;
        self.erased_full_chip = true;
        Ok(())
    }

    fn write_range(&mut self, address: u32, data: &[u8]) -> Result<(), KernelError> {
        if !self.is_kernel_running() {
            return Err(KernelError::KernelNotRunning);
        }
        for (idx, chunk) in data.chunks(128).enumerate() {
            let addr = address as usize + idx * 128;
            let pkt = self.honda.build_write_chunk_request(addr, chunk);
            let _ = self.exchange_frame(&pkt)?;
        }
        Ok(())
    }

    fn verify_image(&mut self, address: u32, expected: &[u8]) -> Result<bool, KernelError> {
        Ok(self.compare_image(address, expected)?.is_empty())
    }

    fn compare_image(
        &mut self,
        address: u32,
        expected: &[u8],
    ) -> Result<Vec<(u32, u8, u8)>, KernelError> {
        let mut mismatches = Vec::new();
        let mut offset = 0usize;
        while offset < expected.len() {
            let take = (expected.len() - offset).min(128);
            let got = self.read_memory_block(address + offset as u32, take)?;
            for i in 0..take {
                if got[i] != expected[offset + i] {
                    mismatches.push((address + (offset + i) as u32, got[i], expected[offset + i]));
                }
            }
            offset += take;
        }
        Ok(mismatches)
    }

    fn cleanup(&mut self) -> Result<(), KernelError> {
        self.cleanup_count += 1;
        if self.is_kernel_running() {
            let exit_cmd = HondaKLineSession::build_wakeup_packet();
            let is_mock = self
                .device
                .0
                .lock()
                .map(|d| d.is_mock())
                .unwrap_or(false);
            if let Err(e) = self.exchange_frame(&exit_cmd) {
                if !is_mock {
                    self.phase = SessionPhase::RestoreFailed;
                    self.operation = None;
                    self.erased_full_chip = false;
                    return Err(KernelError::RestoreFailed(format!(
                        "Honda K-Line cleanup exit failed: {e:?}"
                    )));
                }
            }
        }
        self.erased_full_chip = false;
        self.phase = SessionPhase::Idle;
        self.operation = None;
        Ok(())
    }

    fn keep_alive(&mut self) -> Result<(), KernelError> {
        if !self.is_kernel_running() {
            return Err(KernelError::KernelNotRunning);
        }
        let wakeup = HondaKLineSession::build_wakeup_packet();
        let _ = self.exchange_frame(&wakeup)?;
        Ok(())
    }

    fn image_size(&self) -> usize {
        self.image_size
    }

    fn protocol(&self) -> Option<ProtocolId> {
        self.device
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .connected_protocol()
    }

    fn cleanup_count(&self) -> usize {
        self.cleanup_count
    }
}

// ===========================================================================
// 2. Multi-Arch In-RAM Flash Kernel Session
// ===========================================================================

pub struct RamKernelHardwareSession {
    device: SharedDevice,
    ram: RamKernelSession,
    tx_can_id: u32,
    image_size: usize,
    phase: SessionPhase,
    operation: Option<KernelOperation>,
    cleanup_count: usize,
}

impl RamKernelHardwareSession {
    pub fn new(
        device: SharedDevice,
        arch: KernelArchitecture,
        ram_load_address: u32,
        kernel_payload: Vec<u8>,
        image_size: usize,
    ) -> Self {
        Self {
            device,
            ram: RamKernelSession::new(arch, ram_load_address, kernel_payload),
            tx_can_id: 0x07E0,
            image_size,
            phase: SessionPhase::Idle,
            operation: None,
            cleanup_count: 0,
        }
    }

    pub fn with_tx_can_id(mut self, tx_can_id: u32) -> Self {
        self.tx_can_id = tx_can_id;
        self
    }

    fn pdu_exchange(&mut self, pdu: &[u8]) -> Result<Vec<u8>, KernelError> {
        let mut dev = self
            .device
            .0
            .lock()
            .map_err(|_| KernelError::UploadRejected)?;
        let proto = dev.connected_protocol().unwrap_or(ProtocolId::Iso15765);
        let framed_can =
            dev.uses_can_id_prefix() && (proto.is_iso15765() || matches!(proto, ProtocolId::Can));
        let mut wire = Vec::with_capacity(4 + pdu.len());
        if framed_can {
            wire.extend_from_slice(&self.tx_can_id.to_be_bytes());
        }
        wire.extend_from_slice(pdu);
        let flags = if framed_can {
            tx_flags::ISO15765_FRAME_PAD
        } else {
            0
        };
        let msg = PassThruMsg::with_flags(proto, flags, wire);
        dev.send(&msg).map_err(|_| KernelError::UploadRejected)?;

        for _ in 0..64 {
            let reply = dev.receive().map_err(|_| KernelError::KernelDidNotStart)?;
            let body = if framed_can {
                if reply.data.len() < 4 {
                    return Err(KernelError::KernelDidNotStart);
                }
                &reply.data[4..]
            } else {
                reply.data.as_slice()
            };
            if body.is_empty() {
                continue;
            }
            if body[0] == uds_sid::NEGATIVE_RESPONSE && body.len() >= 3 {
                if body[2] == 0x78 {
                    continue;
                }
                return Err(KernelError::NegativeResponse(body[2]));
            }
            return Ok(body.to_vec());
        }
        Err(KernelError::KernelDidNotStart)
    }

    fn send_kernel_cmd(&mut self, cmd: u8, address: u32, payload: &[u8]) -> Result<Vec<u8>, KernelError> {
        let pkt = KernelCommandDispatcher::build_kernel_packet(cmd, address, payload);
        let rsp = self.pdu_exchange(&pkt)?;
        let (code, body) = KernelCommandDispatcher::parse_kernel_response(&rsp)
            .map_err(|_| KernelError::KernelDidNotStart)?;
        if code != cmd.wrapping_add(KernelCommandDispatcher::RSP_ACK_OFFSET) {
            return Err(KernelError::NegativeResponse(code));
        }
        Ok(body.to_vec())
    }
}

impl KernelSession for RamKernelHardwareSession {
    fn phase(&self) -> SessionPhase {
        self.phase
    }

    fn operation(&self) -> Option<KernelOperation> {
        self.operation
    }

    fn is_kernel_running(&self) -> bool {
        matches!(self.phase, SessionPhase::KernelRunning)
    }

    fn start(&mut self, operation: KernelOperation) -> Result<(), KernelError> {
        if !self.ram.kernel_payload.is_empty() {
            let req = self.ram.build_kernel_upload_request();
            let dl_rsp = self.pdu_exchange(&req)?;
            if dl_rsp.first().copied() != Some(uds_sid::REQUEST_DOWNLOAD + 0x40) {
                return Err(KernelError::UploadRejected);
            }
            for blk in self.ram.build_kernel_transfer_blocks(252) {
                let td_rsp = self.pdu_exchange(&blk)?;
                if td_rsp.first().copied() != Some(uds_sid::TRANSFER_DATA + 0x40) {
                    return Err(KernelError::UploadRejected);
                }
            }
            let exit_rsp = self.pdu_exchange(&[uds_sid::REQUEST_TRANSFER_EXIT])?;
            if exit_rsp.first().copied() != Some(uds_sid::REQUEST_TRANSFER_EXIT + 0x40) {
                return Err(KernelError::UploadRejected);
            }
            let exec_cmd = self.ram.build_execute_kernel_command();
            let exec_rsp = self.pdu_exchange(&exec_cmd)?;
            if let Some(&req_sid) = exec_cmd.first() {
                if exec_rsp.first().copied() != Some(req_sid.wrapping_add(0x40)) {
                    return Err(KernelError::KernelDidNotStart);
                }
            }
        }

        let _ = self.send_kernel_cmd(KernelCommandDispatcher::CMD_PING, 0, &[])?;
        self.phase = SessionPhase::KernelRunning;
        self.operation = Some(operation);
        Ok(())
    }

    fn read_memory_block(&mut self, address: u32, length: usize) -> Result<Vec<u8>, KernelError> {
        if !self.is_kernel_running() {
            return Err(KernelError::KernelNotRunning);
        }
        let max_blk = self.ram.max_read_block_size.clamp(16, 2048);
        let mut out = Vec::with_capacity(length);
        let mut offset = 0usize;
        while offset < length {
            let take = (length - offset).min(max_blk);
            let cmd = self
                .ram
                .build_ram_read_block_command(address + offset as u32, take as u16);
            let rsp = self.pdu_exchange(&cmd)?;
            let data = if rsp.first().copied() == Some(0x75) {
                &rsp[1..]
            } else {
                rsp.as_slice()
            };
            if data.len() != take {
                return Err(KernelError::LengthMismatch {
                    expected: take,
                    got: data.len(),
                });
            }
            out.extend_from_slice(data);
            offset += take;
        }
        Ok(out)
    }

    fn erase_range(&mut self, address: u32, length: usize) -> Result<(), KernelError> {
        if !self.is_kernel_running() {
            return Err(KernelError::KernelNotRunning);
        }
        let _ = self.send_kernel_cmd(
            KernelCommandDispatcher::CMD_ERASE_SECTOR,
            address,
            &(length as u32).to_be_bytes(),
        )?;
        Ok(())
    }

    fn write_range(&mut self, address: u32, data: &[u8]) -> Result<(), KernelError> {
        if !self.is_kernel_running() {
            return Err(KernelError::KernelNotRunning);
        }
        for (idx, chunk) in data.chunks(256).enumerate() {
            let _ = self.send_kernel_cmd(
                KernelCommandDispatcher::CMD_WRITE_CHUNK,
                address + (idx * 256) as u32,
                chunk,
            )?;
        }
        Ok(())
    }

    fn verify_image(&mut self, address: u32, expected: &[u8]) -> Result<bool, KernelError> {
        Ok(self.compare_image(address, expected)?.is_empty())
    }

    fn compare_image(
        &mut self,
        address: u32,
        expected: &[u8],
    ) -> Result<Vec<(u32, u8, u8)>, KernelError> {
        let mut mismatches = Vec::new();
        let mut offset = 0usize;
        while offset < expected.len() {
            let take = (expected.len() - offset).min(256);
            let got = self.read_memory_block(address + offset as u32, take)?;
            for i in 0..take {
                if got[i] != expected[offset + i] {
                    mismatches.push((address + (offset + i) as u32, got[i], expected[offset + i]));
                }
            }
            offset += take;
        }
        Ok(mismatches)
    }

    fn cleanup(&mut self) -> Result<(), KernelError> {
        self.cleanup_count += 1;
        if self.is_kernel_running() {
            let reset = self.ram.build_exit_kernel_and_reboot();
            if let Err(e) = self.pdu_exchange(&reset) {
                self.phase = SessionPhase::RestoreFailed;
                self.operation = None;
                return Err(KernelError::RestoreFailed(format!(
                    "RAM kernel exit/reboot failed: {e:?}"
                )));
            }
        }
        self.phase = SessionPhase::Idle;
        self.operation = None;
        Ok(())
    }

    fn keep_alive(&mut self) -> Result<(), KernelError> {
        if !self.is_kernel_running() {
            return Err(KernelError::KernelNotRunning);
        }
        let _ = self.ram.scratch_watchdog_ping();
        // Send TesterPresent requesting positive response (0x00) since pdu_exchange awaits RX.
        let _ = self.pdu_exchange(&[uds_sid::TESTER_PRESENT, 0x00])?;
        Ok(())
    }

    fn image_size(&self) -> usize {
        self.image_size
    }

    fn protocol(&self) -> Option<ProtocolId> {
        self.device
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .connected_protocol()
    }

    fn cleanup_count(&self) -> usize {
        self.cleanup_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::{Device, DeviceKind, MockDevice, ScriptedReply, TransportKind};

    fn kline_reply(header: u8, service: u8, sub: u8, payload: &[u8]) -> ScriptedReply {
        let pkt = HondaProtocolFraming::build_packet(header, service, sub, payload);
        ScriptedReply::Message(PassThruMsg::with_flags(ProtocolId::Iso14230, 0, pkt))
    }

    fn can_reply(pdu: Vec<u8>) -> ScriptedReply {
        let mut wire = vec![0x00, 0x00, 0x07, 0xE8];
        wire.extend_from_slice(&pdu);
        ScriptedReply::Message(PassThruMsg::with_flags(
            ProtocolId::Iso15765,
            tx_flags::ISO15765_FRAME_PAD,
            wire,
        ))
    }

    #[test]
    fn honda_keihin_kline_session_roundtrips_read_write_and_cleanup() {
        let mut dev = MockDevice::new(DeviceKind::Mock, "KLine Bench", TransportKind::Serial);
        dev.open().unwrap();
        dev.connect(ProtocolId::Iso14230, 10_400).unwrap();
        dev.set_script(vec![
            kline_reply(0xFE, 0x72, 0x00, &[]),              // wakeup ACK
            kline_reply(0x7E, 0x41, 0x0E, &[]),              // fast baud ACK
            kline_reply(0x7D, 0x41, 0x02, &[]),              // PGM unlock ACK
            kline_reply(0x7D, 0x41, 0x03, &[]),              // -FI unlock ACK
            kline_reply(0x7E, 0x41, 0x0B, &[]),              // erase ACK
            kline_reply(0x7E, 0x41, 0x10, &[]),              // write ACK
            kline_reply(0x7D, 0x41, 0x10, &[0xAA, 0xBB]),    // readback compare
        ]);

        let shared = SharedDevice::new(Box::new(dev));
        let mut sess = HondaKLineKernelSession::new(
            shared,
            HondaEcuVendor::Keihin,
            HondaEcuType::KeihinM32R,
            2,
        );
        sess.start(KernelOperation::Write).unwrap();
        assert!(sess.is_kernel_running());
        sess.erase_range(0x100, 2).unwrap();
        sess.write_range(0x100, &[0xAA, 0xBB]).unwrap();
        assert!(sess.verify_image(0x100, &[0xAA, 0xBB]).unwrap());
        sess.cleanup().unwrap();
        assert!(!sess.is_kernel_running());
        assert_eq!(sess.cleanup_count(), 1);
    }

    #[test]
    fn honda_shindengen_kline_solves_challenge_key_from_rom() {
        let mut rom = vec![0xFFu8; 64];
        rom[10..17].copy_from_slice(&ShindengenSeedKeyEngine::WRITE_KEY_SIG);
        rom[17..19].copy_from_slice(&0x0003u16.to_le_bytes()); // v5
        rom[19..21].copy_from_slice(&0x0002u16.to_le_bytes()); // v6
        rom[21..23].copy_from_slice(&0x0005u16.to_le_bytes()); // v7
        rom[25] = 0xEE;
        rom[26] = 0x00;

        let mut dev = MockDevice::new(DeviceKind::Mock, "Shindengen Bench", TransportKind::Serial);
        dev.open().unwrap();
        dev.connect(ProtocolId::Iso14230, 10_400).unwrap();
        dev.set_script(vec![
            kline_reply(0xFE, 0x72, 0x00, &[]),              // wakeup
            kline_reply(0x7B, 0x42, 0x05, &[0x12, 0x34]),    // seed 0x1234
            kline_reply(0x7B, 0x42, 0x06, &[0x00]),          // key ACK
            kline_reply(0x7B, 0x42, 0x10, &[0xDE, 0xAD]),    // read 2 bytes
        ]);

        let shared = SharedDevice::new(Box::new(dev));
        let mut sess = HondaKLineKernelSession::new(
            shared,
            HondaEcuVendor::Shindengen,
            HondaEcuType::ShindengenRH850,
            2,
        )
        .with_fast_baud(false)
        .with_shindengen_key_rom(rom);

        sess.start(KernelOperation::ReadModifyWrite).unwrap();
        let data = sess.read_memory_block(0x00, 2).unwrap();
        assert_eq!(data, vec![0xDE, 0xAD]);
    }

    #[test]
    fn ram_kernel_hardware_session_uploads_executes_writes_verifies_and_resets() {
        let mut dev = MockDevice::new(DeviceKind::Mock, "RAM Kernel Bench", TransportKind::Usb)
            .with_can_id_prefix(true);
        dev.open().unwrap();
        dev.connect(ProtocolId::Iso15765, 500_000).unwrap();
        dev.set_script(vec![
            can_reply(vec![0x74, 0x20, 0x04, 0x00]), // RequestDownload ACK
            can_reply(vec![0x76, 0x01]),             // TransferData ACK
            can_reply(vec![0x77]),                   // RequestTransferExit ACK
            can_reply(vec![0x71, 0x01, 0x02, 0x00]), // Execute Kernel ACK
            can_reply(vec![
                KernelCommandDispatcher::CMD_PING + KernelCommandDispatcher::RSP_ACK_OFFSET,
            ]),
            can_reply(vec![
                KernelCommandDispatcher::CMD_ERASE_SECTOR + KernelCommandDispatcher::RSP_ACK_OFFSET,
            ]),
            can_reply(vec![
                KernelCommandDispatcher::CMD_WRITE_CHUNK + KernelCommandDispatcher::RSP_ACK_OFFSET,
            ]),
            can_reply(vec![0x75, 0x55, 0x66, 0x77, 0x88]), // RAM read back
            can_reply(vec![0x51, 0x01]),                   // ECUReset ACK
        ]);

        let shared = SharedDevice::new(Box::new(dev));
        let mut sess = RamKernelHardwareSession::new(
            shared,
            KernelArchitecture::Motorola68k,
            0x00FF_8000,
            vec![0xAA; 16],
            4,
        );
        sess.start(KernelOperation::Write).unwrap();
        sess.erase_range(0x1000, 4).unwrap();
        sess.write_range(0x1000, &[0x55, 0x66, 0x77, 0x88]).unwrap();
        assert!(sess.verify_image(0x1000, &[0x55, 0x66, 0x77, 0x88]).unwrap());
        sess.cleanup().unwrap();
        assert_eq!(sess.cleanup_count(), 1);
        assert!(!sess.is_kernel_running());
    }
}
