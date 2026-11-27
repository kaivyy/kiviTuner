//! Real ISO-14229 UDS hardware [`KernelSession`] over a shared physical [`Device`].
//!
//! Executes standard UDS flashing/readback transitions:
//! `0x10` Session Control -> `0x27` SecurityAccess -> `0x31` Erase Memory ->
//! `0x34` RequestDownload -> `0x36` TransferData -> `0x37` RequestTransferExit ->
//! `0x23`/`0x35` ReadMemoryByAddress -> `0x11` ECUReset on cleanup.

use crate::device::{Device, DeviceError, DeviceInfo};
use crate::j2534_types::{tx_flags, PassThruMsg, ProtocolId};
use crate::kernel_session::{KernelError, KernelOperation, KernelSession, SessionPhase};
use crate::seed_key_registry::compute_oem_seed_key;
use crate::uds::uds_sid;
use std::sync::{Arc, Mutex};

const MAX_UDS_PENDING_REPLIES: usize = 64;
const MAX_UDS_READ_CHUNK: usize = 0x0FF0;

/// Shared [`Device`] wrapper so both [`crate::flash_engine`] and [`UdsHardwareKernelSession`]
/// drive the same physical interface safely without ownership splits.
#[derive(Clone)]
pub struct SharedDevice(pub Arc<Mutex<Box<dyn Device + Send>>>);

impl SharedDevice {
    pub fn new(device: Box<dyn Device + Send>) -> Self {
        Self(Arc::new(Mutex::new(device)))
    }
}

impl SharedDevice {
    fn lock_device(&self) -> Result<std::sync::MutexGuard<'_, Box<dyn Device + Send>>, DeviceError> {
        self.0
            .lock()
            .map_err(|_| DeviceError::Transport("shared device mutex poisoned".to_string()))
    }
}

impl Device for SharedDevice {
    fn open(&mut self) -> Result<(), DeviceError> {
        self.lock_device()?.open()
    }
    fn close(&mut self) -> Result<(), DeviceError> {
        self.lock_device()?.close()
    }
    fn connect(&mut self, protocol: ProtocolId, baud_rate: u32) -> Result<(), DeviceError> {
        self.lock_device()?.connect(protocol, baud_rate)
    }
    fn disconnect(&mut self) -> Result<(), DeviceError> {
        self.lock_device()?.disconnect()
    }
    fn send(&mut self, msg: &PassThruMsg) -> Result<(), DeviceError> {
        self.lock_device()?.send(msg)
    }
    fn receive(&mut self) -> Result<PassThruMsg, DeviceError> {
        self.lock_device()?.receive()
    }
    fn read_voltage_millivolts(&mut self) -> Result<u32, DeviceError> {
        self.lock_device()?.read_voltage_millivolts()
    }
    fn info(&self) -> DeviceInfo {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .info()
    }
    fn is_open(&self) -> bool {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_open()
    }
    fn is_connected(&self) -> bool {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_connected()
    }
    fn connected_protocol(&self) -> Option<ProtocolId> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .connected_protocol()
    }
    fn is_mock(&self) -> bool {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_mock()
    }
    fn uses_can_id_prefix(&self) -> bool {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .uses_can_id_prefix()
    }
    fn supports_flashing(&self) -> bool {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .supports_flashing()
    }
    fn configure_can_ids(&mut self, tx_id: u32, rx_id: u32) -> Result<(), DeviceError> {
        self.lock_device()?.configure_can_ids(tx_id, rx_id)
    }
    fn set_kline_no_checksum(&mut self, enabled: bool) {
        if let Ok(mut d) = self.lock_device() {
            d.set_kline_no_checksum(enabled);
        }
    }
    fn kline_wakeup_pulse(&mut self, low_ms: u64, high_ms: u64) -> Result<(), DeviceError> {
        self.lock_device()?.kline_wakeup_pulse(low_ms, high_ms)
    }
    fn send_raw_command(&mut self, cmd: &str) -> Result<String, DeviceError> {
        self.lock_device()?.send_raw_command(cmd)
    }
}

pub struct UdsHardwareKernelSession {
    device: SharedDevice,
    tx_can_id: u32,
    security_level: u8,
    image_size: usize,
    phase: SessionPhase,
    operation: Option<KernelOperation>,
    seed_key_algo_id: String,
    seed_key_param: u16,
    cleanup_count: usize,
    block_counter: u8,
    max_transfer_chunk: usize,
}

impl UdsHardwareKernelSession {
    pub fn new(device: SharedDevice, image_size: usize) -> Self {
        Self {
            device,
            tx_can_id: 0x07E0,
            security_level: 0x01,
            image_size,
            phase: SessionPhase::Idle,
            operation: None,
            seed_key_algo_id: "bosch_me7".to_string(),
            seed_key_param: 0,
            cleanup_count: 0,
            block_counter: 1,
            max_transfer_chunk: 0xFF,
        }
    }

    pub fn with_tx_can_id(mut self, tx_can_id: u32) -> Self {
        self.tx_can_id = tx_can_id;
        self
    }

    pub fn with_security_level(mut self, security_level: u8) -> Self {
        self.security_level = if security_level.is_multiple_of(2) {
            security_level.saturating_sub(1).max(1)
        } else {
            security_level
        };
        self
    }

    pub fn with_seed_key(mut self, algo_id: impl Into<String>, param: u16) -> Self {
        self.seed_key_algo_id = algo_id.into();
        self.seed_key_param = param;
        self
    }

    fn uds_exchange(&mut self, pdu: &[u8]) -> Result<Vec<u8>, KernelError> {
        let mut dev = self
            .device
            .0
            .lock()
            .map_err(|_| KernelError::UploadRejected)?;
        let proto = dev.connected_protocol().unwrap_or(ProtocolId::Iso15765);
        let use_can_prefix =
            dev.uses_can_id_prefix() && (proto.is_iso15765() || matches!(proto, ProtocolId::Can));
        let mut wire = Vec::with_capacity(4 + pdu.len());
        if use_can_prefix {
            wire.extend_from_slice(&self.tx_can_id.to_be_bytes());
        }
        wire.extend_from_slice(pdu);
        let flags = if use_can_prefix {
            tx_flags::ISO15765_FRAME_PAD
        } else {
            0
        };
        let msg = PassThruMsg::with_flags(proto, flags, wire);
        dev.send(&msg).map_err(|_| KernelError::UploadRejected)?;

        for _ in 0..MAX_UDS_PENDING_REPLIES {
            let reply = dev.receive().map_err(|_| KernelError::KernelDidNotStart)?;
            let body = if use_can_prefix {
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
                    continue; // NRC 0x78 Response Pending
                }
                return Err(KernelError::NegativeResponse(body[2]));
            }
            return Ok(body.to_vec());
        }
        Err(KernelError::KernelDidNotStart)
    }

    fn uds_exchange_expect(&mut self, pdu: &[u8]) -> Result<Vec<u8>, KernelError> {
        let rep = self.uds_exchange(pdu)?;
        let expected_sid = pdu.first().copied().unwrap_or(0).wrapping_add(0x40);
        if rep.first().copied() != Some(expected_sid) {
            return Err(KernelError::NegativeResponse(0x22));
        }
        Ok(rep)
    }
}

impl KernelSession for UdsHardwareKernelSession {
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
        let session_sub = match operation {
            KernelOperation::Read => 0x03, // Extended diagnostic session for UDS 0x23 reads
            KernelOperation::Write | KernelOperation::ReadModifyWrite => 0x02, // Programming session
        };
        let _ = self.uds_exchange_expect(&[uds_sid::DIAGNOSTIC_SESSION_CONTROL, session_sub])?;
        if matches!(operation, KernelOperation::Write | KernelOperation::ReadModifyWrite) {
            let seed_sub = self.security_level;
            let key_sub = seed_sub.saturating_add(1);
            let seed_rep = self.uds_exchange_expect(&[uds_sid::SECURITY_ACCESS, seed_sub])?;
            if seed_rep.len() < 3 || seed_rep.get(1).copied() != Some(seed_sub) {
                return Err(KernelError::UploadRejected);
            }
            if seed_rep[2..].iter().any(|&b| b != 0) {
                let seed_hex: String = seed_rep[2..].iter().map(|b| format!("{b:02X}")).collect();
                let solved = compute_oem_seed_key(
                    &self.seed_key_algo_id,
                    &seed_hex,
                    self.seed_key_param,
                    None,
                    None,
                )
                .map_err(|_| KernelError::UploadRejected)?;
                let mut key_req = vec![uds_sid::SECURITY_ACCESS, key_sub];
                key_req.extend_from_slice(&solved.key_bytes);
                let _ = self.uds_exchange_expect(&key_req)?;
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
        if length == 0 {
            return Ok(Vec::new());
        }
        let mut out = Vec::with_capacity(length);
        let mut offset = 0usize;
        while offset < length {
            let take = (length - offset).min(MAX_UDS_READ_CHUNK);
            let cur_addr = address
                .checked_add(offset as u32)
                .ok_or(KernelError::NegativeResponse(0x31))?;
            let mut pdu = Vec::with_capacity(8);
            pdu.push(0x23);
            if take <= 0xFF {
                pdu.push(0x14);
                pdu.extend_from_slice(&cur_addr.to_be_bytes());
                pdu.push(take as u8);
            } else {
                pdu.push(0x24);
                pdu.extend_from_slice(&cur_addr.to_be_bytes());
                pdu.extend_from_slice(&(take as u16).to_be_bytes());
            }
            let rep = self.uds_exchange_expect(&pdu)?;
            let data = &rep[1..];
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
        let mut pdu = vec![uds_sid::ROUTINE_CONTROL, 0x01, 0xFF, 0x00, 0x44];
        pdu.extend_from_slice(&address.to_be_bytes());
        pdu.extend_from_slice(&(length as u32).to_be_bytes());
        let _ = self.uds_exchange_expect(&pdu)?;
        Ok(())
    }

    fn write_range(&mut self, address: u32, data: &[u8]) -> Result<(), KernelError> {
        if !self.is_kernel_running() {
            return Err(KernelError::KernelNotRunning);
        }
        let mut dl = vec![uds_sid::REQUEST_DOWNLOAD, 0x00, 0x44];
        dl.extend_from_slice(&address.to_be_bytes());
        dl.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let rep = self.uds_exchange_expect(&dl)?;
        if rep.len() >= 3 {
            let len_bytes = ((rep[1] >> 4) & 0x0F) as usize;
            if (1..=4).contains(&len_bytes) && rep.len() >= 2 + len_bytes {
                let mut max_blk: usize = 0;
                for &b in &rep[2..2 + len_bytes] {
                    max_blk = (max_blk << 8) | (b as usize);
                }
                if max_blk > 4 {
                    self.max_transfer_chunk = max_blk - 2;
                }
            }
        }
        self.block_counter = 1;
        let max_chunk = self
            .max_transfer_chunk
            .clamp(1, crate::j2534_abi::PASSTHRU_MSG_DATA_CAPACITY - 6);
        for chunk in data.chunks(max_chunk) {
            let mut td = Vec::with_capacity(2 + chunk.len());
            td.push(uds_sid::TRANSFER_DATA);
            td.push(self.block_counter);
            td.extend_from_slice(chunk);
            let td_rep = self.uds_exchange_expect(&td)?;
            if td_rep.len() >= 2 && td_rep[1] != self.block_counter {
                return Err(KernelError::UploadRejected);
            }
            self.block_counter = self.block_counter.wrapping_add(1);
        }
        let _ = self.uds_exchange_expect(&[uds_sid::REQUEST_TRANSFER_EXIT])?;
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
            let take = (expected.len() - offset).min(0x80);
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
            if let Err(e) = self.uds_exchange(&[uds_sid::ECU_RESET, 0x01]) {
                self.phase = SessionPhase::RestoreFailed;
                self.operation = None;
                return Err(KernelError::RestoreFailed(format!(
                    "ECU reset during cleanup failed: {e:?}"
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
        let _ = self.uds_exchange_expect(&[uds_sid::TESTER_PRESENT, 0x00])?;
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
