use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TelemetryStatusFlags {
    pub check_engine_light: bool,
    pub limp_mode: bool,
    pub low_voltage: bool,
    pub overheating: bool,
    pub stale_sample: bool,
    pub communication_fault: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TelemetryFrame {
    pub timestamp_ms: u64,
    pub sequence: u64,
    pub is_simulated: bool,
    pub rpm: f32,
    pub tps: f32,
    pub map_kpa: f32,
    pub ect_c: f32,
    pub iat_c: f32,
    pub battery_volts: f32,
    pub ignition_advance_deg: f32,
    pub injector_pulse_width_ms: f32,
    pub afr: f32,
    pub speed_kph: f32,
    pub gear: u8,
    pub status_flags: TelemetryStatusFlags,
}

impl Default for TelemetryFrame {
    fn default() -> Self {
        Self {
            timestamp_ms: 0,
            sequence: 0,
            is_simulated: true,
            rpm: 0.0,
            tps: 0.0,
            map_kpa: 101.3,
            ect_c: 25.0,
            iat_c: 25.0,
            battery_volts: 12.6,
            ignition_advance_deg: 10.0,
            injector_pulse_width_ms: 0.0,
            afr: 14.7,
            speed_kph: 0.0,
            gear: 0,
            status_flags: TelemetryStatusFlags::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EcuIdentity {
    pub ecu_id: String,
    pub vin: Option<String>,
    pub software_version: String,
    pub hardware_version: String,
    pub protocol_name: String,
    pub is_simulated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", content = "details")]
pub enum ConnectionState {
    Disconnected,
    Connecting { attempt: u32 },
    Connected { identity: EcuIdentity },
    Reconnecting { reason: String },
    Faulted { error: String },
}
