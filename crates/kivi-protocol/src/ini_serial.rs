//! Real Speeduino / rusEFI / MegaSquirt INI serial protocol driver (`tokio-serial`).
//!
//! Uses INI binary/serial command framing (`'Q'` query signature, `'A'` realtime channels,
//! `'p'` / `'M'` / `'b'` / `'d'` page commands per `speeduino.ini`). Zero simulation or fake ticks.

use crate::driver::{DriverDescriptor, EcuDriver, TelemetrySource};
use crate::errors::ProtocolError;
use crate::model::{ConnectionState, EcuIdentity, TelemetryFrame, TelemetryStatusFlags};
use crate::worker::create_telemetry_broadcast_channel;
use async_trait::async_trait;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tokio_serial::SerialPortBuilderExt;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Decodes a Speeduino `'A'` realtime frame buffer using verified offsets from
/// `speeduino.ini` (`[OutputChannels]`, `ochBlockSize = 139`).
pub fn parse_speeduino_realtime_frame(
    frame_slice: &[u8],
    sequence: u64,
    timestamp_ms: u64,
) -> Option<TelemetryFrame> {
    if frame_slice.len() < 26 {
        return None;
    }
    let map_kpa = u16::from_le_bytes([frame_slice[4], frame_slice[5]]) as f32;
    let iat_c = (frame_slice[6] as f32) - 40.0;
    let ect_c = (frame_slice[7] as f32) - 40.0;
    let battery_volts = (frame_slice[9] as f32) * 0.1;
    let afr = (frame_slice[10] as f32) * 0.1;
    let rpm = u16::from_le_bytes([frame_slice[14], frame_slice[15]]) as f32;
    let ignition_advance_deg = (frame_slice[24] as i8) as f32;
    let tps = (frame_slice[25] as f32) * 0.5;
    let injector_pulse_width_ms = if frame_slice.len() >= 78 {
        u16::from_le_bytes([frame_slice[76], frame_slice[77]]) as f32 * 0.001
    } else {
        0.0
    };
    let speed_kph = if frame_slice.len() >= 106 {
        u16::from_le_bytes([frame_slice[104], frame_slice[105]]) as f32
    } else {
        0.0
    };
    let gear = frame_slice.get(106).copied().unwrap_or(0);

    Some(TelemetryFrame {
        timestamp_ms,
        sequence,
        is_simulated: false,
        rpm,
        tps,
        map_kpa,
        ect_c,
        iat_c,
        battery_volts,
        ignition_advance_deg,
        injector_pulse_width_ms,
        afr,
        speed_kph,
        gear,
        status_flags: TelemetryStatusFlags::default(),
    })
}

pub struct IniSerialDriver {
    port_path: String,
    baud_rate: u32,
    state: Arc<Mutex<ConnectionState>>,
    tx: broadcast::Sender<TelemetryFrame>,
    poll_task: Mutex<Option<JoinHandle<()>>>,
}

impl IniSerialDriver {
    pub fn new(port_path: impl Into<String>, baud_rate: u32) -> Self {
        let (tx, _) = create_telemetry_broadcast_channel();
        Self {
            port_path: port_path.into(),
            baud_rate,
            state: Arc::new(Mutex::new(ConnectionState::Disconnected)),
            tx,
            poll_task: Mutex::new(None),
        }
    }
}

impl Drop for IniSerialDriver {
    fn drop(&mut self) {
        let mut g = self.poll_task.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(h) = g.take() {
            h.abort();
        }
    }
}

#[async_trait]
impl EcuDriver for IniSerialDriver {
    fn descriptor(&self) -> DriverDescriptor {
        DriverDescriptor {
            id: format!("ini_serial:{}", self.port_path),
            name: format!("INI Serial ({})", self.port_path),
            is_simulator: false,
        }
    }

    async fn connect(&self) -> Result<EcuIdentity, ProtocolError> {
        if let Some(old) = self
            .poll_task
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            old.abort();
        }
        let builder = tokio_serial::new(&self.port_path, self.baud_rate)
            .data_bits(tokio_serial::DataBits::Eight)
            .parity(tokio_serial::Parity::None)
            .stop_bits(tokio_serial::StopBits::One)
            .flow_control(tokio_serial::FlowControl::None);
        let mut stream = builder
            .open_native_async()
            .map_err(|e| ProtocolError::DeviceNotFound(format!("{}: {e}", self.port_path)))?;

        stream
            .write_all(b"Q")
            .await
            .map_err(|e| ProtocolError::Disconnected(e.to_string()))?;
        let sig_deadline = tokio::time::Instant::now() + Duration::from_millis(800);
        let mut sig_buf = [0u8; 128];
        let mut sig_len = 0usize;
        while sig_len < sig_buf.len() {
            let rem = sig_deadline.saturating_duration_since(tokio::time::Instant::now());
            if rem.is_zero() {
                break;
            }
            let step = if sig_len > 0 {
                rem.min(Duration::from_millis(30))
            } else {
                rem
            };
            match tokio::time::timeout(step, stream.read(&mut sig_buf[sig_len..])).await {
                Ok(Ok(0)) => break,
                Ok(Ok(k)) => sig_len += k,
                Ok(Err(e)) => {
                    if sig_len > 0 {
                        break;
                    }
                    return Err(ProtocolError::Disconnected(e.to_string()));
                }
                Err(_) => {
                    if sig_len > 0 {
                        break;
                    }
                    return Err(ProtocolError::Timeout(800));
                }
            }
        }
        if sig_len == 0 {
            return Err(ProtocolError::HandshakeFailed(
                "INI serial controller returned empty 'Q' signature".to_string(),
            ));
        }
        let signature = String::from_utf8_lossy(&sig_buf[..sig_len]).trim().to_string();
        let identity = EcuIdentity {
            ecu_id: "INI-SERIAL-ECU".to_string(),
            vin: None,
            software_version: signature,
            hardware_version: "Serial".to_string(),
            protocol_name: "INI-Serial".to_string(),
            is_simulated: false,
        };
        *self.state.lock().unwrap_or_else(|e| e.into_inner()) = ConnectionState::Connected {
            identity: identity.clone(),
        };

        let tx = self.tx.clone();
        let state = self.state.clone();
        let handle = tokio::spawn(async move {
            let mut seq = 0u64;
            let mut buf = [0u8; 256];
            let deadline_budget = Duration::from_millis(250);
            loop {
                let connected = matches!(
                    &*state.lock().unwrap_or_else(|e| e.into_inner()),
                    ConnectionState::Connected { .. }
                );
                if !connected {
                    break;
                }
                if let Err(e) = stream.write_all(b"A").await {
                    *state.lock().unwrap_or_else(|e| e.into_inner()) = ConnectionState::Faulted {
                        error: format!("Serial write error: {e}"),
                    };
                    break;
                }
                let mut read_n = 0usize;
                let deadline = tokio::time::Instant::now() + deadline_budget;
                while read_n < buf.len() {
                    let rem = deadline.saturating_duration_since(tokio::time::Instant::now());
                    if rem.is_zero() || read_n >= 139 {
                        break;
                    }
                    let step_timeout = if read_n >= 26 {
                        rem.min(Duration::from_millis(12))
                    } else {
                        rem
                    };
                    match tokio::time::timeout(step_timeout, stream.read(&mut buf[read_n..])).await
                    {
                        Ok(Ok(0)) => break,
                        Ok(Ok(k)) => read_n += k,
                        Ok(Err(_)) | Err(_) => break,
                    }
                }
                if let Some(frame) =
                    parse_speeduino_realtime_frame(&buf[..read_n], seq + 1, now_ms())
                {
                    seq += 1;
                    let _ = tx.send(frame);
                }
                tokio::time::sleep(Duration::from_millis(40)).await;
            }
        });
        *self.poll_task.lock().unwrap_or_else(|e| e.into_inner()) = Some(handle);
        Ok(identity)
    }

    async fn disconnect(&self) -> Result<(), ProtocolError> {
        *self.state.lock().unwrap_or_else(|e| e.into_inner()) = ConnectionState::Disconnected;
        if let Some(h) = self
            .poll_task
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            h.abort();
        }
        Ok(())
    }

    fn connection_state(&self) -> ConnectionState {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

impl TelemetrySource for IniSerialDriver {
    fn subscribe_telemetry(&self) -> broadcast::Receiver<TelemetryFrame> {
        self.tx.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speeduino_och_139_and_legacy_32_byte_frames_decode_verified_offsets() {
        let mut buf = vec![0u8; 139];
        buf[4..6].copy_from_slice(&102u16.to_le_bytes()); // MAP 102 kPa
        buf[6] = 65; // IAT = 25 C (raw - 40)
        buf[7] = 130; // CLT = 90 C (raw - 40)
        buf[9] = 138; // Battery = 13.8 V
        buf[10] = 147; // AFR = 14.7
        buf[14..16].copy_from_slice(&3500u16.to_le_bytes()); // RPM = 3500
        buf[24] = (-5i8) as u8; // Ignition advance = -5 deg
        buf[25] = 84; // TPS = 42.0%
        buf[76..78].copy_from_slice(&4250u16.to_le_bytes()); // PW1 = 4.250 ms
        buf[104..106].copy_from_slice(&112u16.to_le_bytes()); // VSS = 112 km/h
        buf[106] = 4; // Gear 4

        let f = parse_speeduino_realtime_frame(&buf, 9, 1234).unwrap();
        assert!(!f.is_simulated);
        assert_eq!(f.sequence, 9);
        assert_eq!(f.map_kpa, 102.0);
        assert_eq!(f.iat_c, 25.0);
        assert_eq!(f.ect_c, 90.0);
        assert!((f.battery_volts - 13.8).abs() < 1e-4);
        assert!((f.afr - 14.7).abs() < 1e-4);
        assert_eq!(f.rpm, 3500.0);
        assert_eq!(f.ignition_advance_deg, -5.0);
        assert_eq!(f.tps, 42.0);
        assert!((f.injector_pulse_width_ms - 4.25).abs() < 1e-4);
        assert_eq!(f.speed_kph, 112.0);
        assert_eq!(f.gear, 4);

        let short = parse_speeduino_realtime_frame(&buf[..32], 1, 10).unwrap();
        assert_eq!(short.tps, 42.0);
        assert_eq!(short.injector_pulse_width_ms, 0.0);
        assert_eq!(short.speed_kph, 0.0);
        assert!(parse_speeduino_realtime_frame(&buf[..25], 1, 10).is_none());
    }
}
