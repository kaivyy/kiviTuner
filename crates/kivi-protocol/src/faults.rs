//! Fault injection specifications for bench testing and resilience verification.

use serde::{Deserialize, Serialize};

/// Fault scenarios that can be injected into virtual ECU simulators.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(tag = "type", content = "value")]
pub enum SimFault {
    /// Normal operation without injected anomalies.
    #[default]
    None,
    /// Simulates a communication timeout where the ECU stops responding.
    Timeout,
    /// Generates corrupted or out-of-spec frames violating telemetry validation.
    InvalidFrame,
    /// Simulates an abrupt hardware disconnect or cable severed.
    Disconnect,
    /// Freezes telemetry updates for a given count of sample ticks.
    StaleSamples(u32),
    /// Overrides a specific sensor value with an arbitrary (potentially out-of-range) float.
    SensorRange { sensor: String, value: f32 },
}
