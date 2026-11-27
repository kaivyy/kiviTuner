//! Bounded ECU bus flight recorder and forwarding [`Device`] wrapper.
//!
//! Captures every `open`, `connect`, `send`, `receive`, `disconnect`, and `close`
//! event from any live or virtual transport so physical bench debugging has a
//! full deterministic trace without modifying protocol encoders.

use crate::device::{Device, DeviceError, DeviceInfo};
use crate::j2534_types::{PassThruMsg, ProtocolId};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fmt::Write as _;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub const DEFAULT_FLIGHT_RECORDER_CAPACITY: usize = 4096;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum BusDirection {
    Tx,
    Rx,
    Control,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BusFrameRecord {
    pub seq: u64,
    pub timestamp_ms: u64,
    pub protocol: String,
    pub baud_rate: u32,
    pub direction: BusDirection,
    pub data_hex: String,
    pub note: Option<String>,
}

struct FlightRecorderInner {
    capacity: usize,
    next_seq: u64,
    frames: VecDeque<BusFrameRecord>,
}

/// Thread-safe bounded ring buffer capturing bus frames and lifecycle events.
pub struct BusFlightRecorder {
    inner: Mutex<FlightRecorderInner>,
    started_at: Instant,
    epoch_base_ms: u64,
}

impl Default for BusFlightRecorder {
    fn default() -> Self {
        Self::new(DEFAULT_FLIGHT_RECORDER_CAPACITY)
    }
}

impl BusFlightRecorder {
    pub fn new(capacity: usize) -> Self {
        let cap = capacity.max(1);
        let epoch_base_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);
        Self {
            inner: Mutex::new(FlightRecorderInner {
                capacity: cap,
                next_seq: 1,
                frames: VecDeque::with_capacity(cap.min(1024)),
            }),
            started_at: Instant::now(),
            epoch_base_ms,
        }
    }

    pub fn record(
        &self,
        protocol: &str,
        baud_rate: u32,
        direction: BusDirection,
        data: &[u8],
        note: Option<String>,
    ) {
        let timestamp_ms = self
            .epoch_base_ms
            .saturating_add(self.started_at.elapsed().as_millis() as u64);
        let data_hex = encode_hex(data);
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let seq = guard.next_seq;
        guard.next_seq = guard.next_seq.saturating_add(1);
        if guard.frames.len() >= guard.capacity {
            guard.frames.pop_front();
        }
        guard.frames.push_back(BusFrameRecord {
            seq,
            timestamp_ms,
            protocol: protocol.to_string(),
            baud_rate,
            direction,
            data_hex,
            note,
        });
    }

    pub fn snapshot(&self, limit: Option<usize>) -> Vec<BusFrameRecord> {
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        match limit {
            Some(max) if max < guard.frames.len() => {
                let skip = guard.frames.len() - max;
                guard.frames.iter().skip(skip).cloned().collect()
            }
            _ => guard.frames.iter().cloned().collect(),
        }
    }

    pub fn len(&self) -> usize {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .frames
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn clear(&self) -> usize {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let cleared = guard.frames.len();
        guard.frames.clear();
        cleared
    }

    /// Exports captured records as newline-delimited JSON.
    pub fn export_jsonl(&self) -> String {
        let records = self.snapshot(None);
        let mut out = String::new();
        for rec in &records {
            if let Ok(line) = serde_json::to_string(rec) {
                out.push_str(&line);
                out.push('\n');
            }
        }
        out
    }

    /// Exports captured `Tx` and `Rx` records in Vector-compatible ASCII `.asc` CAN trace format.
    pub fn export_can_asc(&self) -> String {
        let records = self.snapshot(None);
        let mut out = String::from(
            "date Thu Jan 01 00:00:00.000 am 1970\nbase hex  timestamps absolute\ninternal events logged\n// kiviTuner Bus Flight Recorder\nBegin Triggerblock Thu Jan 01 00:00:00.000 am 1970\n",
        );
        let first_ts = records.first().map_or(0, |r| r.timestamp_ms);
        for rec in &records {
            let dir_str = match rec.direction {
                BusDirection::Tx => "Tx",
                BusDirection::Rx => "Rx",
                BusDirection::Control | BusDirection::Error => continue,
            };
            let rel_secs = rec.timestamp_ms.saturating_sub(first_ts) as f64 / 1000.0;
            let bytes = decode_hex_pairs(&rec.data_hex);
            let (arb_id, payload) = if bytes.len() >= 4
                && matches!(rec.protocol.as_str(), "Can" | "Iso15765")
            {
                let id = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                (id, &bytes[4..])
            } else {
                (0x7DFu32, bytes.as_slice())
            };
            let hex_bytes = payload
                .iter()
                .map(|b| format!("{b:02X}"))
                .collect::<Vec<_>>()
                .join(" ");
            let _ = writeln!(
                out,
                "   {rel_secs:.6} 1  {arb_id:03X}             {dir_str}   d {} {hex_bytes}",
                payload.len()
            );
        }
        out.push_str("End TriggerBlock\n");
        out
    }

    /// Extracts all captured `Rx` frame payloads as compact uppercase hex strings for replay tests.
    pub fn extract_rx_hex_script(&self) -> Vec<String> {
        self.snapshot(None)
            .into_iter()
            .filter(|r| r.direction == BusDirection::Rx)
            .map(|r| r.data_hex)
            .collect()
    }
}

fn encode_hex(data: &[u8]) -> String {
    let mut s = String::with_capacity(data.len() * 2);
    for &b in data {
        let _ = write!(s, "{b:02X}");
    }
    s
}

fn decode_hex_pairs(hex: &str) -> Vec<u8> {
    let clean: Vec<u8> = hex
        .bytes()
        .filter(|b| !b.is_ascii_whitespace())
        .collect();
    clean
        .chunks_exact(2)
        .filter_map(|c| {
            let pair = std::str::from_utf8(c).ok()?;
            u8::from_str_radix(pair, 16).ok()
        })
        .collect()
}

fn protocol_label(proto: Option<ProtocolId>) -> &'static str {
    match proto {
        Some(ProtocolId::J1850Vpw) => "J1850Vpw",
        Some(ProtocolId::J1850Pwm) => "J1850Pwm",
        Some(ProtocolId::Iso9141) => "Iso9141",
        Some(ProtocolId::Iso14230) => "Iso14230",
        Some(ProtocolId::Can) => "Can",
        Some(ProtocolId::Iso15765) => "Iso15765",
        Some(ProtocolId::SciAEngine) => "SciAEngine",
        Some(ProtocolId::SciATrans) => "SciATrans",
        Some(ProtocolId::SciBEngine) => "SciBEngine",
        Some(ProtocolId::SciBTrans) => "SciBTrans",
        Some(ProtocolId::Unknown(_)) => "Unknown",
        None => "Unconnected",
    }
}

/// Forwarding [`Device`] wrapper that logs every transport operation into [`BusFlightRecorder`].
pub struct RecordingDevice {
    inner: Box<dyn Device + Send>,
    recorder: Arc<BusFlightRecorder>,
    baud_rate: u32,
}

impl RecordingDevice {
    pub fn new(inner: Box<dyn Device + Send>, recorder: Arc<BusFlightRecorder>) -> Self {
        Self {
            inner,
            recorder,
            baud_rate: 0,
        }
    }

    fn current_proto_label(&self) -> &'static str {
        protocol_label(self.inner.connected_protocol())
    }
}

impl Device for RecordingDevice {
    fn open(&mut self) -> Result<(), DeviceError> {
        let name = self.inner.info().name;
        match self.inner.open() {
            Ok(()) => {
                self.recorder.record(
                    "Lifecycle",
                    0,
                    BusDirection::Control,
                    &[],
                    Some(format!("open: {name}")),
                );
                Ok(())
            }
            Err(e) => {
                self.recorder.record(
                    "Lifecycle",
                    0,
                    BusDirection::Error,
                    &[],
                    Some(format!("open failed ({name}): {e}")),
                );
                Err(e)
            }
        }
    }

    fn close(&mut self) -> Result<(), DeviceError> {
        let res = self.inner.close();
        match &res {
            Ok(()) => self.recorder.record(
                "Lifecycle",
                0,
                BusDirection::Control,
                &[],
                Some("close".to_string()),
            ),
            Err(e) => self.recorder.record(
                "Lifecycle",
                0,
                BusDirection::Error,
                &[],
                Some(format!("close failed: {e}")),
            ),
        }
        res
    }

    fn connect(&mut self, protocol: ProtocolId, baud_rate: u32) -> Result<(), DeviceError> {
        let label = protocol_label(Some(protocol));
        match self.inner.connect(protocol, baud_rate) {
            Ok(()) => {
                self.baud_rate = baud_rate;
                self.recorder.record(
                    label,
                    baud_rate,
                    BusDirection::Control,
                    &[],
                    Some("connect".to_string()),
                );
                Ok(())
            }
            Err(e) => {
                self.recorder.record(
                    label,
                    baud_rate,
                    BusDirection::Error,
                    &[],
                    Some(format!("connect failed: {e}")),
                );
                Err(e)
            }
        }
    }

    fn disconnect(&mut self) -> Result<(), DeviceError> {
        let label = self.current_proto_label();
        let baud = self.baud_rate;
        let res = self.inner.disconnect();
        match &res {
            Ok(()) => {
                self.recorder.record(
                    label,
                    baud,
                    BusDirection::Control,
                    &[],
                    Some("disconnect".to_string()),
                );
                self.baud_rate = 0;
            }
            Err(e) => self.recorder.record(
                label,
                baud,
                BusDirection::Error,
                &[],
                Some(format!("disconnect failed: {e}")),
            ),
        }
        res
    }

    fn send(&mut self, msg: &PassThruMsg) -> Result<(), DeviceError> {
        let label = protocol_label(Some(msg.protocol_id));
        match self.inner.send(msg) {
            Ok(()) => {
                self.recorder.record(
                    label,
                    self.baud_rate,
                    BusDirection::Tx,
                    &msg.data,
                    None,
                );
                Ok(())
            }
            Err(e) => {
                self.recorder.record(
                    label,
                    self.baud_rate,
                    BusDirection::Error,
                    &msg.data,
                    Some(format!("send failed: {e}")),
                );
                Err(e)
            }
        }
    }

    fn receive(&mut self) -> Result<PassThruMsg, DeviceError> {
        let label = self.current_proto_label();
        match self.inner.receive() {
            Ok(msg) => {
                let msg_label = protocol_label(Some(msg.protocol_id));
                self.recorder.record(
                    msg_label,
                    self.baud_rate,
                    BusDirection::Rx,
                    &msg.data,
                    None,
                );
                Ok(msg)
            }
            Err(e) => {
                if e != DeviceError::Timeout {
                    self.recorder.record(
                        label,
                        self.baud_rate,
                        BusDirection::Error,
                        &[],
                        Some(format!("receive failed: {e}")),
                    );
                }
                Err(e)
            }
        }
    }

    fn read_voltage_millivolts(&mut self) -> Result<u32, DeviceError> {
        self.inner.read_voltage_millivolts()
    }

    fn info(&self) -> DeviceInfo {
        self.inner.info()
    }

    fn is_open(&self) -> bool {
        self.inner.is_open()
    }

    fn is_connected(&self) -> bool {
        self.inner.is_connected()
    }

    fn connected_protocol(&self) -> Option<ProtocolId> {
        self.inner.connected_protocol()
    }

    fn is_mock(&self) -> bool {
        self.inner.is_mock()
    }

    fn uses_can_id_prefix(&self) -> bool {
        self.inner.uses_can_id_prefix()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::{DeviceKind, MockDevice, ScriptedReply, TransportKind};

    #[test]
    fn ring_buffer_evicts_oldest_when_full() {
        let rec = BusFlightRecorder::new(3);
        for i in 0u8..5 {
            rec.record("Iso15765", 500_000, BusDirection::Tx, &[i], None);
        }
        let snap = rec.snapshot(None);
        assert_eq!(snap.len(), 3);
        assert_eq!(snap[0].data_hex, "02");
        assert_eq!(snap[2].data_hex, "04");
        assert_eq!(snap[0].seq, 3);
        assert_eq!(snap[2].seq, 5);
    }

    #[test]
    fn recording_device_forwards_flags_and_captures_tx_rx() {
        let rec = Arc::new(BusFlightRecorder::new(64));
        let mut mock = MockDevice::new(DeviceKind::Mock, "LoopMock", TransportKind::InMemory)
            .with_can_id_prefix(true);
        mock.set_script(vec![ScriptedReply::Message(PassThruMsg::new(
            ProtocolId::Iso15765,
            vec![0x00, 0x00, 0x07, 0xE8, 0x50, 0x01],
        ))]);

        let mut wrapped = RecordingDevice::new(Box::new(mock), Arc::clone(&rec));
        assert!(wrapped.is_mock());
        assert!(wrapped.uses_can_id_prefix());

        wrapped.open().unwrap();
        wrapped.connect(ProtocolId::Iso15765, 500_000).unwrap();
        wrapped
            .send(&PassThruMsg::new(
                ProtocolId::Iso15765,
                vec![0x00, 0x00, 0x07, 0xE0, 0x10, 0x01],
            ))
            .unwrap();
        let rx = wrapped.receive().unwrap();
        assert_eq!(rx.data, vec![0x00, 0x00, 0x07, 0xE8, 0x50, 0x01]);
        wrapped.disconnect().unwrap();
        wrapped.close().unwrap();

        let frames = rec.snapshot(None);
        assert_eq!(frames.len(), 6);
        assert_eq!(rec.extract_rx_hex_script(), vec!["000007E85001"]);

        let asc = rec.export_can_asc();
        assert!(asc.contains("7E0"));
        assert!(asc.contains("7E8"));
        assert!(asc.contains("Tx"));
        assert!(asc.contains("Rx"));

        let jsonl = rec.export_jsonl();
        assert_eq!(jsonl.lines().count(), 6);
        assert_eq!(rec.clear(), 6);
        assert!(rec.is_empty());
    }
}
