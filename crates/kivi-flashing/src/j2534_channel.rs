//! Ported from `ecu_engine_rust::flashing::j2534_channel`.
//!
//! Pure byte-level protocol logic: builders, parsers, encoders, state machines.
//! No I/O, no device access, no `unsafe`. Transport is the caller's concern.
//!
//! Verified by unit tests with hand-derived expectations. **Not** validated
//! against a physical ECU; no such claim is made.

/// Standard SAE J2534 Protocol Identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum J2534Protocol {
    J1850Vpw = 1,
    J1850Pwm = 2,
    Iso9141 = 3,
    Iso14230Kwp = 4,
    Can = 5,
    Iso15765IsoTp = 6,
}

/// Standard PassThru Filter Type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassThruFilterType {
    PassFilter = 1,
    BlockFilter = 2,
    FlowControlFilter = 3,
}

/// SAE J2534 PassThru Message (matches native Windows DLL ABI).
#[derive(Debug, Clone)]
pub struct PassThruMsg {
    pub protocol_id: u32,
    pub rx_status: u32,
    pub tx_flags: u32,
    pub timestamp_micros: u32,
    pub data: Vec<u8>,
}

impl PassThruMsg {
    pub const FLAG_CAN_29BIT_ID: u32 = 0x00000100;
    pub const FLAG_ISO15765_PAD: u32 = 0x00000040;

    pub fn new(protocol: J2534Protocol, data: Vec<u8>) -> Self {
        Self {
            protocol_id: protocol as u32,
            rx_status: 0,
            tx_flags: 0,
            timestamp_micros: 0,
            data,
        }
    }

    /// Build standard ISO 15765 CAN Request message with 11-bit CAN arbitration ID (e.g. 0x7E0 for ECM).
    pub fn build_iso15765_request(can_id: u32, payload: &[u8]) -> Self {
        let mut msg_data = Vec::with_capacity(payload.len() + 4);
        msg_data.extend_from_slice(&can_id.to_be_bytes()); // First 4 bytes are CAN ID
        msg_data.extend_from_slice(payload);

        let mut msg = Self::new(J2534Protocol::Iso15765IsoTp, msg_data);
        msg.tx_flags = Self::FLAG_ISO15765_PAD; // Pad unused frame bytes to 8 with 0x00 / 0xAA
        msg
    }

    /// Extract received CAN ID and payload.
    pub fn unpack_iso15765_response(&self) -> Result<(u32, Vec<u8>), &'static str> {
        if self.data.len() < 4 {
            return Err("PassThru message shorter than 4-byte CAN ID header");
        }
        let can_id = u32::from_be_bytes([self.data[0], self.data[1], self.data[2], self.data[3]]);
        let payload = self.data[4..].to_vec();
        Ok((can_id, payload))
    }
}

/// J2534 Flow Control Filter Definition.
#[derive(Debug, Clone)]
pub struct FlowControlFilter {
    pub mask: Vec<u8>,
    pub pattern: Vec<u8>,
    pub flow_control: Vec<u8>,
}

impl FlowControlFilter {
    /// Create standard OBD-II ECM Flow Control filter (0x7E8 response matches 0x7E0 flow control).
    pub fn standard_obd2_ecm() -> Self {
        Self {
            mask: vec![0xFF, 0xFF, 0xFF, 0xFF],
            pattern: vec![0x00, 0x00, 0x07, 0xE8],       // Filter for ECM response 0x7E8
            flow_control: vec![0x00, 0x00, 0x07, 0xE0],  // Transmit Flow Control to 0x7E0
        }
    }
}

/// J2534 Rate Pacing & Separation Time (STmin) Manager.
pub struct StMinRatePacer {
    pub stmin_micros: u32,
    pub last_frame_timestamp_micros: u32,
    has_sent: bool,
}

impl StMinRatePacer {
    pub fn new(stmin_millis: u32) -> Self {
        Self {
            stmin_micros: stmin_millis.saturating_mul(1000),
            last_frame_timestamp_micros: 0,
            has_sent: false,
        }
    }

    /// Calculate required sleep delay (micros) before transmitting next consecutive frame.
    pub fn compute_required_delay_micros(&self, current_timestamp_micros: u32) -> u32 {
        if !self.has_sent {
            return 0;
        }
        let elapsed = current_timestamp_micros.wrapping_sub(self.last_frame_timestamp_micros);
        self.stmin_micros.saturating_sub(elapsed)
    }

    /// Update timestamp after frame transmission.
    pub fn record_frame_sent(&mut self, current_timestamp_micros: u32) {
        self.last_frame_timestamp_micros = current_timestamp_micros;
        self.has_sent = true;
    }
}

/// CAN Controller Bit Timing Configuration.
/// Extracted from industry PassThru adapters (Scanmatik 2 Pro, Tactrix OpenPort 2.0).
#[derive(Debug, Clone, PartialEq)]
pub struct CanBitTiming {
    pub prescaler: u32,
    pub prop_seg: u32,
    pub phase_seg1: u32,
    pub phase_seg2: u32,
    pub sjw: u32,
    pub total_tq: u32,
    pub sample_point_percent: f64,
    pub actual_baud: u32,
}

impl CanBitTiming {
    /// Calculate optimal bit timing parameters for a given clock and target baud rate.
    pub fn calculate(clock_hz: u32, target_baud: u32, target_sample_point: f64) -> Result<Self, &'static str> {
        if clock_hz == 0
            || target_baud == 0
            || !target_sample_point.is_finite()
            || !(10.0..=95.0).contains(&target_sample_point)
        {
            return Err("Clock, target baud, and sample point (10-95%) must be valid");
        }

        let mut best_timing: Option<CanBitTiming> = None;
        let mut min_error = f64::MAX;

        for total_tq in 8..=25u32 {
            let Some(denom) = target_baud.checked_mul(total_tq) else {
                continue;
            };
            let prescaler = clock_hz / denom;
            if prescaler == 0 || prescaler > 1024 {
                continue;
            }
            let Some(step) = prescaler.checked_mul(total_tq) else {
                continue;
            };
            if !clock_hz.is_multiple_of(step) {
                continue;
            }
            let actual_baud = clock_hz / step;
            if actual_baud != target_baud {
                continue;
            }

            let target_tseg1 = (((target_sample_point / 100.0) * total_tq as f64).round() as u32)
                .saturating_sub(1);
            if !(2..=16).contains(&target_tseg1) {
                continue;
            }
            let tseg2 = total_tq - 1 - target_tseg1;
            if !(1..=8).contains(&tseg2) {
                continue;
            }

            let prop_seg = target_tseg1 / 2;
            let phase_seg1 = target_tseg1 - prop_seg;
            let phase_seg2 = tseg2;
            // ISO 11898-1 §11.3.1.1 / Bosch CAN 2.0B: 1 <= SJW <= min(4, PHASE_SEG1, PHASE_SEG2)
            let sjw = phase_seg1.min(phase_seg2).clamp(1, 4);

            let actual_sp = ((1 + target_tseg1) as f64 / total_tq as f64) * 100.0;
            let err = (actual_sp - target_sample_point).abs();

            if err < min_error {
                min_error = err;
                best_timing = Some(CanBitTiming {
                    prescaler,
                    prop_seg,
                    phase_seg1,
                    phase_seg2,
                    sjw,
                    total_tq,
                    sample_point_percent: actual_sp,
                    actual_baud,
                });
            }
        }

        best_timing.ok_or("Unable to find valid CAN timing configuration")
    }

    /// Parse PassThru / RP1210 descriptor string.
    /// e.g. "CAN:Baud=500000,PROP_SEG=2,PHASE_SEG1=5,PHASE_SEG2=2,SJW=1"
    pub fn from_descriptor(desc: &str) -> Result<Self, &'static str> {
        let mut baud = 500_000;
        let mut prop = 2;
        let mut p1 = 5;
        let mut p2 = 2;
        let mut sjw = 1;

        for part in desc.split(',') {
            let kv: Vec<&str> = part.split('=').collect();
            if kv.len() == 2 {
                let key = kv[0].trim().to_uppercase();
                let val: u32 = kv[1].trim().parse().map_err(|_| "Invalid numeric value")?;
                if key.contains("BAUD") {
                    baud = val;
                } else if key == "PROP_SEG" {
                    prop = val;
                } else if key == "PHASE_SEG1" {
                    p1 = val;
                } else if key == "PHASE_SEG2" {
                    p2 = val;
                } else if key == "SJW" {
                    sjw = val;
                }
            }
        }

        if baud == 0 {
            return Err("CAN baud rate must be > 0");
        }
        if !(1..=8).contains(&prop) || !(1..=8).contains(&p1) || !(1..=8).contains(&p2) {
            return Err("CAN timing segments (PROP_SEG, PHASE_SEG1, PHASE_SEG2) must be 1..=8");
        }
        if !(1..=4).contains(&sjw) {
            return Err("CAN SJW must be 1..=4");
        }

        let total_tq = 1 + prop + p1 + p2;
        let sp = ((1 + prop + p1) as f64 / total_tq as f64) * 100.0;

        Ok(CanBitTiming {
            prescaler: 1,
            prop_seg: prop,
            phase_seg1: p1,
            phase_seg2: p2,
            sjw,
            total_tq,
            sample_point_percent: sp,
            actual_baud: baud,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::CanBitTiming;

    #[test]
    fn calculate_enforces_iso11898_sjw_and_exact_baud() {
        for sp in [37.5, 50.0, 75.0, 80.0, 87.5] {
            if let Ok(t) = CanBitTiming::calculate(16_000_000, 500_000, sp) {
                assert_eq!(t.actual_baud, 500_000);
                assert!(
                    t.sjw >= 1 && t.sjw <= 4 && t.sjw <= t.phase_seg1 && t.sjw <= t.phase_seg2,
                    "ISO 11898-1 violation for SP {sp}%: {t:?}"
                );
            }
        }
        assert!(CanBitTiming::calculate(16_000_000, 500_000, 0.0).is_err());
        assert!(CanBitTiming::calculate(16_000_000, 500_000, f64::NAN).is_err());
        assert!(CanBitTiming::calculate(16_000_000, 500_000, 99.0).is_err());
        // Clock 16_000_005 is not an exact divisor of 500_000 * total_tq
        assert!(CanBitTiming::calculate(16_000_005, 500_000, 75.0).is_err());
    }
}
