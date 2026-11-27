use crate::driver::{DriverDescriptor, EcuDriver, TelemetrySource};
use crate::errors::ProtocolError;
use crate::model::{ConnectionState, EcuIdentity, TelemetryFrame};
use std::sync::Arc;
use tokio::sync::broadcast;

/// Coordinates ECU driver lifecycles, ensuring a single active session at any time.
pub struct ProtocolSessionManager {
    active_driver: Option<Arc<dyn EcuDriver>>,
    telemetry_rx: Option<broadcast::Receiver<TelemetryFrame>>,
}

impl Default for ProtocolSessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ProtocolSessionManager {
    /// Creates a new, idle session manager with no active driver.
    pub fn new() -> Self {
        Self {
            active_driver: None,
            telemetry_rx: None,
        }
    }

    /// Queries the connection state of the currently active driver, or Disconnected if none.
    pub fn current_state(&self) -> ConnectionState {
        match &self.active_driver {
            Some(driver) => driver.connection_state(),
            None => ConnectionState::Disconnected,
        }
    }

    /// Returns the descriptor of the active driver, if connected.
    pub fn active_descriptor(&self) -> Option<DriverDescriptor> {
        self.active_driver.as_ref().map(|d| d.descriptor())
    }

    /// Connects to an ECU using the provided driver.
    ///
    /// Returns `ProtocolError::PortBusy` if another session is already connected.
    pub async fn connect<D>(&mut self, driver: Arc<D>) -> Result<EcuIdentity, ProtocolError>
    where
        D: EcuDriver + TelemetrySource + 'static,
    {
        if let Some(existing) = &self.active_driver {
            if matches!(
                existing.connection_state(),
                ConnectionState::Connected { .. }
                    | ConnectionState::Connecting { .. }
                    | ConnectionState::Reconnecting { .. }
            ) {
                return Err(ProtocolError::PortBusy(
                    "Another driver session is currently active. Disconnect first.".to_string(),
                ));
            }
        }

        let identity = driver.connect().await?;
        self.telemetry_rx = Some(driver.subscribe_telemetry());
        self.active_driver = Some(driver);
        Ok(identity)
    }

    /// Disconnects from the currently active ECU driver, if any.
    pub async fn disconnect(&mut self) -> Result<(), ProtocolError> {
        self.telemetry_rx = None;
        if let Some(driver) = self.active_driver.take() {
            driver.disconnect().await?;
        }
        Ok(())
    }

    /// Subscribes a new receiver to the active telemetry broadcast stream.
    pub fn subscribe(&self) -> Result<broadcast::Receiver<TelemetryFrame>, ProtocolError> {
        match &self.telemetry_rx {
            Some(rx) => Ok(rx.resubscribe()),
            None => Err(ProtocolError::Disconnected(
                "No active session subscribed to telemetry".to_string(),
            )),
        }
    }
}
