use crate::errors::ProtocolError;
use crate::model::{ConnectionState, EcuIdentity, TelemetryFrame};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

/// Metadata describing an ECU communication driver or simulator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriverDescriptor {
    pub id: String,
    pub name: String,
    pub is_simulator: bool,
}

/// Asynchronous ECU driver trait for connection management and telemetry streaming.
#[async_trait]
pub trait EcuDriver: Send + Sync {
    /// Returns the static descriptor metadata for this driver.
    fn descriptor(&self) -> DriverDescriptor;

    /// Establishes communication with the physical or virtual ECU.
    async fn connect(&self) -> Result<EcuIdentity, ProtocolError>;

    /// Gracefully tears down the connection session.
    async fn disconnect(&self) -> Result<(), ProtocolError>;

    /// Queries the current connection lifecycle state.
    fn connection_state(&self) -> ConnectionState;
}

/// Source of real-time telemetry frames over a broadcast channel.
pub trait TelemetrySource: Send + Sync {
    /// Subscribes to the broadcast stream of live telemetry frames.
    fn subscribe_telemetry(&self) -> broadcast::Receiver<TelemetryFrame>;
}
