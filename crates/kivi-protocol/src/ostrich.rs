//! Ostrich 2.0 real-time EPROM emulator serial driver (Moates hardware protocol).
//!
//! Implements the 921600 baud 8N1 packet framing:
//! - `'V'` firmware/hardware version probe (3-byte response)
//! - `'N'` extended serial/device query (optional metadata)
//! - `'Z'` 64 KB SRAM bank select (`0..=7`) + 8-bit wrapping sum
//! - `'W'` page write (1..=256 bytes, `0x00` length encoding for 256) + `'O'` ACK
//! - `'R'` page read (1..=256 bytes) + 8-bit trailing checksum verification
//!
//! All serial I/O calls are wrapped in bounded timeouts (`tokio::time::timeout`).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::Duration;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_serial::{DataBits, FlowControl, Parity, SerialPortBuilderExt, SerialStream, StopBits};

pub const OSTRICH_DEFAULT_BAUD: u32 = 921_600;
pub const OSTRICH_MAX_BANK: u8 = 7;
pub const OSTRICH_PAGE_MAX_BYTES: usize = 256;
pub const OSTRICH_IO_TIMEOUT: Duration = Duration::from_millis(1500);
pub const OSTRICH_ACK_BYTE: u8 = b'O';

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum OstrichError {
    #[error("Invalid bank index {0} (valid 0..=7)")]
    InvalidBank(u8),
    #[error("Invalid page payload length {0} (must be 1..=256)")]
    InvalidPageLength(usize),
    #[error("Transfer exceeds 64 KB bank window (start=0x{start:04X}, len={len})")]
    AddressOverflow { start: u16, len: usize },
    #[error("Transport I/O error: {0}")]
    Transport(String),
    #[error("Emulator communication timed out after {0} ms")]
    Timeout(u64),
    #[error("Emulator rejected command with byte 0x{0:02X}")]
    NegativeAck(u8),
    #[error("Checksum mismatch on reply (expected 0x{expected:02X}, got 0x{actual:02X})")]
    ChecksumMismatch { expected: u8, actual: u8 },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OstrichVersion {
    pub hardware_type: u8,
    pub firmware_major: u8,
    pub firmware_minor: u8,
    pub hardware_id_char: String,
    pub display: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OstrichTransferSummary {
    pub bank: u8,
    pub start_addr: u16,
    pub bytes_written: usize,
    pub chunks: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OstrichVerifyReport {
    pub bank: u8,
    pub start_addr: u16,
    pub bytes_checked: usize,
    pub verified: bool,
    pub first_mismatch_offset: Option<usize>,
}

/// Computes the 8-bit wrapping additive checksum used by Ostrich 2.0 packets.
pub fn ostrich_checksum(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0u8, |acc, &b| acc.wrapping_add(b))
}

fn encode_page_len(len: usize) -> Result<u8, OstrichError> {
    if len == 0 || len > OSTRICH_PAGE_MAX_BYTES {
        return Err(OstrichError::InvalidPageLength(len));
    }
    Ok(if len == OSTRICH_PAGE_MAX_BYTES {
        0
    } else {
        len as u8
    })
}

pub fn encode_version_cmd() -> [u8; 1] {
    *b"V"
}

pub fn parse_version_reply(reply: &[u8; 3]) -> OstrichVersion {
    let id_ch = if reply[2].is_ascii_graphic() {
        (reply[2] as char).to_string()
    } else {
        format!("0x{:02X}", reply[2])
    };
    OstrichVersion {
        hardware_type: reply[0],
        firmware_major: reply[1],
        firmware_minor: reply[2],
        hardware_id_char: id_ch.clone(),
        display: format!("HW {}.{} ({})", reply[0], reply[1], id_ch),
    }
}

pub fn encode_bank_select_cmd(bank: u8) -> Result<[u8; 3], OstrichError> {
    if bank > OSTRICH_MAX_BANK {
        return Err(OstrichError::InvalidBank(bank));
    }
    let header = [b'Z', bank];
    Ok([header[0], header[1], ostrich_checksum(&header)])
}

/// Encodes a single page write (`<= 256` bytes) at 16-bit bank offset `(addr_hi << 8) | addr_lo`.
pub fn encode_write_page_cmd(addr_hi: u8, addr_lo: u8, payload: &[u8]) -> Result<Vec<u8>, OstrichError> {
    let len_byte = encode_page_len(payload.len())?;
    let mut pkt = Vec::with_capacity(5 + payload.len());
    pkt.push(b'W');
    pkt.push(len_byte);
    pkt.push(addr_hi);
    pkt.push(addr_lo);
    pkt.extend_from_slice(payload);
    let sum = ostrich_checksum(&pkt);
    pkt.push(sum);
    Ok(pkt)
}

/// Encodes a single page read request (`<= 256` bytes).
pub fn encode_read_page_cmd(addr_hi: u8, addr_lo: u8, len: usize) -> Result<[u8; 5], OstrichError> {
    let len_byte = encode_page_len(len)?;
    let body = [b'R', len_byte, addr_hi, addr_lo];
    let sum = ostrich_checksum(&body);
    Ok([body[0], body[1], body[2], body[3], sum])
}

/// Validates a read reply buffer consisting of `expected_len` payload bytes + 1 trailing checksum byte.
pub fn verify_read_reply(expected_len: usize, reply_with_checksum: &[u8]) -> Result<Vec<u8>, OstrichError> {
    if expected_len == 0 || expected_len > OSTRICH_PAGE_MAX_BYTES {
        return Err(OstrichError::InvalidPageLength(expected_len));
    }
    if reply_with_checksum.len() != expected_len + 1 {
        return Err(OstrichError::Transport(format!(
            "Expected {} bytes in read reply, got {}",
            expected_len + 1,
            reply_with_checksum.len()
        )));
    }
    let payload = &reply_with_checksum[..expected_len];
    let expected = ostrich_checksum(payload);
    let actual = reply_with_checksum[expected_len];
    if expected != actual {
        return Err(OstrichError::ChecksumMismatch { expected, actual });
    }
    Ok(payload.to_vec())
}

#[async_trait]
pub trait OstrichTransport: Send {
    async fn write_all(&mut self, data: &[u8]) -> Result<(), OstrichError>;
    async fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), OstrichError>;
    async fn flush(&mut self) -> Result<(), OstrichError>;
}

pub struct SerialOstrichTransport {
    stream: SerialStream,
    timeout: Duration,
}

impl fmt::Debug for SerialOstrichTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SerialOstrichTransport")
            .field("timeout", &self.timeout)
            .finish()
    }
}

impl SerialOstrichTransport {
    pub fn open(port: &str, baud_rate: u32) -> Result<Self, OstrichError> {
        let stream = tokio_serial::new(port, baud_rate)
            .data_bits(DataBits::Eight)
            .parity(Parity::None)
            .stop_bits(StopBits::One)
            .flow_control(FlowControl::None)
            .open_native_async()
            .map_err(|e| OstrichError::Transport(format!("Failed to open {port}: {e}")))?;
        Ok(Self {
            stream,
            timeout: OSTRICH_IO_TIMEOUT,
        })
    }
}

#[async_trait]
impl OstrichTransport for SerialOstrichTransport {
    async fn write_all(&mut self, data: &[u8]) -> Result<(), OstrichError> {
        tokio::time::timeout(self.timeout, self.stream.write_all(data))
            .await
            .map_err(|_| OstrichError::Timeout(self.timeout.as_millis() as u64))?
            .map_err(|e| OstrichError::Transport(e.to_string()))
    }

    async fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), OstrichError> {
        tokio::time::timeout(self.timeout, self.stream.read_exact(buf))
            .await
            .map_err(|_| OstrichError::Timeout(self.timeout.as_millis() as u64))?
            .map_err(|e| OstrichError::Transport(e.to_string()))?;
        Ok(())
    }

    async fn flush(&mut self) -> Result<(), OstrichError> {
        tokio::time::timeout(self.timeout, self.stream.flush())
            .await
            .map_err(|_| OstrichError::Timeout(self.timeout.as_millis() as u64))?
            .map_err(|e| OstrichError::Transport(e.to_string()))
    }
}

/// Async high-level client for the Ostrich 2.0 real-time EPROM emulator.
pub struct OstrichClient<T: OstrichTransport> {
    transport: T,
}

impl<T: OstrichTransport> OstrichClient<T> {
    pub fn new(transport: T) -> Self {
        Self { transport }
    }

    pub fn into_inner(self) -> T {
        self.transport
    }

    async fn read_ack(&mut self) -> Result<(), OstrichError> {
        let mut ack = [0u8; 1];
        self.transport.read_exact(&mut ack).await?;
        if ack[0] != OSTRICH_ACK_BYTE {
            return Err(OstrichError::NegativeAck(ack[0]));
        }
        Ok(())
    }

    pub async fn probe(&mut self) -> Result<OstrichVersion, OstrichError> {
        self.transport.write_all(&encode_version_cmd()).await?;
        self.transport.flush().await?;
        let mut reply = [0u8; 3];
        self.transport.read_exact(&mut reply).await?;
        Ok(parse_version_reply(&reply))
    }

    pub async fn select_bank(&mut self, bank: u8) -> Result<(), OstrichError> {
        let cmd = encode_bank_select_cmd(bank)?;
        self.transport.write_all(&cmd).await?;
        self.transport.flush().await?;
        self.read_ack().await
    }

    /// Writes `data` into `bank` starting at `start_addr`, chunking automatically into <= 256B pages.
    pub async fn write_range(
        &mut self,
        bank: u8,
        start_addr: u16,
        data: &[u8],
    ) -> Result<OstrichTransferSummary, OstrichError> {
        if bank > OSTRICH_MAX_BANK {
            return Err(OstrichError::InvalidBank(bank));
        }
        let end = (start_addr as usize)
            .checked_add(data.len())
            .ok_or(OstrichError::AddressOverflow {
                start: start_addr,
                len: data.len(),
            })?;
        if end > 0x1_0000 {
            return Err(OstrichError::AddressOverflow {
                start: start_addr,
                len: data.len(),
            });
        }
        self.select_bank(bank).await?;
        let mut offset = 0usize;
        let mut chunks = 0usize;
        while offset < data.len() {
            let chunk_len = (data.len() - offset).min(OSTRICH_PAGE_MAX_BYTES);
            let addr = (start_addr as usize + offset) as u16;
            let [addr_hi, addr_lo] = addr.to_be_bytes();
            let pkt = encode_write_page_cmd(addr_hi, addr_lo, &data[offset..offset + chunk_len])?;
            self.transport.write_all(&pkt).await?;
            self.transport.flush().await?;
            self.read_ack().await?;
            offset += chunk_len;
            chunks += 1;
        }
        Ok(OstrichTransferSummary {
            bank,
            start_addr,
            bytes_written: data.len(),
            chunks,
        })
    }

    /// Reads `len` bytes from `bank` starting at `start_addr`, verifying every page checksum.
    pub async fn read_range(
        &mut self,
        bank: u8,
        start_addr: u16,
        len: usize,
    ) -> Result<Vec<u8>, OstrichError> {
        if bank > OSTRICH_MAX_BANK {
            return Err(OstrichError::InvalidBank(bank));
        }
        let end = (start_addr as usize)
            .checked_add(len)
            .ok_or(OstrichError::AddressOverflow {
                start: start_addr,
                len,
            })?;
        if end > 0x1_0000 {
            return Err(OstrichError::AddressOverflow {
                start: start_addr,
                len,
            });
        }
        self.select_bank(bank).await?;
        let mut out = Vec::with_capacity(len);
        let mut offset = 0usize;
        while offset < len {
            let chunk_len = (len - offset).min(OSTRICH_PAGE_MAX_BYTES);
            let addr = (start_addr as usize + offset) as u16;
            let [addr_hi, addr_lo] = addr.to_be_bytes();
            let cmd = encode_read_page_cmd(addr_hi, addr_lo, chunk_len)?;
            self.transport.write_all(&cmd).await?;
            self.transport.flush().await?;
            let mut reply = vec![0u8; chunk_len + 1];
            self.transport.read_exact(&mut reply).await?;
            let verified = verify_read_reply(chunk_len, &reply)?;
            out.extend_from_slice(&verified);
            offset += chunk_len;
        }
        Ok(out)
    }

    /// Reads back `[start_addr .. start_addr + expected.len())` and compares byte-for-byte.
    pub async fn verify_range(
        &mut self,
        bank: u8,
        start_addr: u16,
        expected: &[u8],
    ) -> Result<OstrichVerifyReport, OstrichError> {
        let actual = self.read_range(bank, start_addr, expected.len()).await?;
        let first_mismatch = actual
            .iter()
            .zip(expected.iter())
            .position(|(a, b)| a != b);
        Ok(OstrichVerifyReport {
            bank,
            start_addr,
            bytes_checked: expected.len(),
            verified: first_mismatch.is_none(),
            first_mismatch_offset: first_mismatch,
        })
    }
}

/// Deterministic in-memory 512 KB (8 banks x 64 KB) Ostrich 2.0 simulator for unit tests.
#[cfg(any(test, feature = "test-doubles"))]
pub struct MockOstrichEmulator {
    banks: Vec<[u8; 0x1_0000]>,
    active_bank: u8,
    rx_queue: std::collections::VecDeque<u8>,
    pub inject_nack_next_write: bool,
    pub inject_bad_read_checksum: bool,
}

#[cfg(any(test, feature = "test-doubles"))]
impl Default for MockOstrichEmulator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(test, feature = "test-doubles"))]
impl MockOstrichEmulator {
    pub fn new() -> Self {
        Self {
            banks: vec![[0xFFu8; 0x1_0000]; 8],
            active_bank: 0,
            rx_queue: std::collections::VecDeque::new(),
            inject_nack_next_write: false,
            inject_bad_read_checksum: false,
        }
    }

    pub fn bank_bytes(&self, bank: usize) -> &[u8; 0x1_0000] {
        &self.banks[bank]
    }
}

#[cfg(any(test, feature = "test-doubles"))]
#[async_trait]
impl OstrichTransport for MockOstrichEmulator {
    async fn write_all(&mut self, data: &[u8]) -> Result<(), OstrichError> {
        if data.is_empty() {
            return Ok(());
        }
        match data[0] {
            b'V' => {
                self.rx_queue.extend([20u8, 4u8, b'O']);
                Ok(())
            }
            b'Z' => {
                if data.len() != 3 || ostrich_checksum(&data[..2]) != data[2] || data[1] > OSTRICH_MAX_BANK {
                    self.rx_queue.push_back(b'N');
                } else {
                    self.active_bank = data[1];
                    self.rx_queue.push_back(OSTRICH_ACK_BYTE);
                }
                Ok(())
            }
            b'W' => {
                if self.inject_nack_next_write {
                    self.inject_nack_next_write = false;
                    self.rx_queue.push_back(b'E');
                    return Ok(());
                }
                if data.len() < 5 {
                    self.rx_queue.push_back(b'E');
                    return Ok(());
                }
                let payload_len = if data[1] == 0 { 256usize } else { data[1] as usize };
                if data.len() != 4 + payload_len + 1 {
                    self.rx_queue.push_back(b'E');
                    return Ok(());
                }
                let expected_sum = ostrich_checksum(&data[..data.len() - 1]);
                if expected_sum != data[data.len() - 1] {
                    self.rx_queue.push_back(b'C');
                    return Ok(());
                }
                let start = u16::from_be_bytes([data[2], data[3]]) as usize;
                let bank = self.active_bank as usize;
                for (i, &byte) in data[4..4 + payload_len].iter().enumerate() {
                    self.banks[bank][(start + i) & 0xFFFF] = byte;
                }
                self.rx_queue.push_back(OSTRICH_ACK_BYTE);
                Ok(())
            }
            b'R' => {
                if data.len() != 5 || ostrich_checksum(&data[..4]) != data[4] {
                    return Err(OstrichError::Transport("Malformed R packet".into()));
                }
                let len = if data[1] == 0 { 256usize } else { data[1] as usize };
                let start = u16::from_be_bytes([data[2], data[3]]) as usize;
                let bank = self.active_bank as usize;
                let mut payload = Vec::with_capacity(len);
                for i in 0..len {
                    payload.push(self.banks[bank][(start + i) & 0xFFFF]);
                }
                let mut cksum = ostrich_checksum(&payload);
                if self.inject_bad_read_checksum {
                    self.inject_bad_read_checksum = false;
                    cksum ^= 0xFF;
                }
                self.rx_queue.extend(payload);
                self.rx_queue.push_back(cksum);
                Ok(())
            }
            other => Err(OstrichError::Transport(format!("Unsupported opcode 0x{other:02X}"))),
        }
    }

    async fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), OstrichError> {
        if self.rx_queue.len() < buf.len() {
            return Err(OstrichError::Timeout(10));
        }
        for slot in buf.iter_mut() {
            *slot = self.rx_queue.pop_front().unwrap_or(0);
        }
        Ok(())
    }

    async fn flush(&mut self) -> Result<(), OstrichError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn ostrich_probe_write_read_and_verify_full_64k_bank() {
        let mut client = OstrichClient::new(MockOstrichEmulator::new());
        let ver = client.probe().await.expect("probe");
        assert_eq!(ver.hardware_type, 20);
        assert_eq!(ver.firmware_major, 4);
        assert_eq!(ver.hardware_id_char, "O");

        let payload: Vec<u8> = (0..65_536usize).map(|i| ((i * 13 + 7) & 0xFF) as u8).collect();
        let summary = client.write_range(3, 0x0000, &payload).await.expect("write 64K");
        assert_eq!(summary.bank, 3);
        assert_eq!(summary.bytes_written, 65_536);
        assert_eq!(summary.chunks, 256);

        let verify = client.verify_range(3, 0x0000, &payload).await.expect("verify 64K");
        assert!(verify.verified);
        assert_eq!(verify.first_mismatch_offset, None);

        // Live single-cell overwrite
        client.write_range(3, 0x1234, &[0x99, 0xAA]).await.expect("live cell patch");
        let mismatch = client.verify_range(3, 0x0000, &payload).await.expect("detect patch");
        assert!(!mismatch.verified);
        assert_eq!(mismatch.first_mismatch_offset, Some(0x1234));
    }

    #[tokio::test]
    async fn ostrich_rejects_bad_checksum_nack_and_invalid_bank() {
        let mut emu = MockOstrichEmulator::new();
        emu.inject_nack_next_write = true;
        let mut client = OstrichClient::new(emu);

        let err_bank = client.select_bank(8).await.expect_err("bank 8 invalid");
        assert_eq!(err_bank, OstrichError::InvalidBank(8));

        let err_nack = client
            .write_range(0, 0x0100, &[1, 2, 3])
            .await
            .expect_err("nack injected");
        assert_eq!(err_nack, OstrichError::NegativeAck(b'E'));

        client.write_range(0, 0x0100, &[10, 20, 30]).await.expect("clean write");
        let mut inner = client.into_inner();
        inner.inject_bad_read_checksum = true;
        let mut client2 = OstrichClient::new(inner);
        let err_ck = client2.read_range(0, 0x0100, 3).await.expect_err("bad checksum");
        assert!(matches!(err_ck, OstrichError::ChecksumMismatch { .. }));
    }
}
