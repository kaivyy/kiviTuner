//! Worker actors and background stream coordination for ECU protocols.

use crate::model::TelemetryFrame;
use tokio::sync::broadcast;

/// Standard default capacity for telemetry broadcast channels (64 frames = ~3.2 seconds at 20 Hz).
pub const TELEMETRY_BROADCAST_CAPACITY: usize = 64;

/// Utility to create a standardized broadcast channel for telemetry streams.
pub fn create_telemetry_broadcast_channel() -> (
    broadcast::Sender<TelemetryFrame>,
    broadcast::Receiver<TelemetryFrame>,
) {
    broadcast::channel(TELEMETRY_BROADCAST_CAPACITY)
}
