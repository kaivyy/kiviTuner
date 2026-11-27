//! Telemetry session manager coordinating hardware ECU drivers and streaming for Tauri.
//!
//! Enumerates real serial ports for OBD-II (`obd2:<port>`) and INI serial/Speeduino
//! (`ini_serial:<port>`) drivers. Never substitutes a simulator on failure; test suites
//! supply explicit drivers via [`TelemetryManager::connect_driver`].

use kivi_flashing::passthru_discovery::list_serial_ports;
use kivi_protocol::driver::{DriverDescriptor, EcuDriver, TelemetrySource};
use kivi_protocol::model::{ConnectionState, EcuIdentity, TelemetryFrame};
use kivi_protocol::obd::driver::{Obd2Driver, Obd2DriverConfig};
use kivi_protocol::obd::framing::{ProtocolSelection, SerialBaudRate};
use kivi_protocol::obd::serial::SerialObdTransport;
use kivi_protocol::obd::wifi::{WifiObdTransport, DEFAULT_WIFI_OBD_ENDPOINT};
use kivi_protocol::session::ProtocolSessionManager;
use kivi_protocol::IniSerialDriver;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;

/// Manages live ECU protocol session and cached latest telemetry frame.
pub struct TelemetryManager {
    session: Mutex<ProtocolSessionManager>,
    latest_frame: Arc<Mutex<Option<TelemetryFrame>>>,
    forward_task: Mutex<Option<JoinHandle<()>>>,
    connection_gen: Arc<AtomicU64>,
}

impl Default for TelemetryManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TelemetryManager {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.forward_task.try_lock() {
            if let Some(handle) = guard.take() {
                handle.abort();
            }
        }
    }
}

fn parse_obd_protocol_suffix(spec: &str) -> (&str, ProtocolSelection) {
    let Some((head, tag)) = spec.rsplit_once('#') else {
        return (spec, ProtocolSelection::Auto);
    };
    let proto = match tag.trim().to_ascii_lowercase().as_str() {
        "can500" | "can11_500" => ProtocolSelection::Can11Bit500k,
        "can29_500" | "can29" => ProtocolSelection::Can29Bit500k,
        "can250" | "can11_250" => ProtocolSelection::Can11Bit250k,
        "can29_250" => ProtocolSelection::Can29Bit250k,
        "kwp" | "kwp_fast" => ProtocolSelection::KwpFastInit,
        "kwp5" => ProtocolSelection::Kwp5BaudInit,
        "iso9141" => ProtocolSelection::Iso9141,
        _ => ProtocolSelection::Auto,
    };
    (head, proto)
}

fn parse_serial_baud_token(val: u32) -> Option<SerialBaudRate> {
    match val {
        9_600 => Some(SerialBaudRate::Baud9600),
        38_400 => Some(SerialBaudRate::Baud38400),
        115_200 => Some(SerialBaudRate::Baud115200),
        230_400 => Some(SerialBaudRate::Baud230400),
        500_000 => Some(SerialBaudRate::Baud500000),
        _ => None,
    }
}

impl TelemetryManager {
    /// Creates a new idle [`TelemetryManager`].
    pub fn new() -> Self {
        Self {
            session: Mutex::new(ProtocolSessionManager::new()),
            latest_frame: Arc::new(Mutex::new(None)),
            forward_task: Mutex::new(None),
            connection_gen: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Enumerates real hardware serial ports and Wi-Fi ELM327 targets.
    pub fn list_drivers(&self) -> Vec<DriverDescriptor> {
        let mut drivers = Vec::new();
        for port in list_serial_ports() {
            let label_tag = if port.adapter_hint == "bluetooth_spp" {
                "OBD-II Bluetooth SPP"
            } else {
                "OBD-II ELM327/STN"
            };
            drivers.push(DriverDescriptor {
                id: format!("obd2:{}", port.port_name),
                name: format!("{label_tag} ({}, Auto Baud)", port.port_name),
                is_simulator: false,
            });
            drivers.push(DriverDescriptor {
                id: format!("ini_serial:{}", port.port_name),
                name: format!("INI / Speeduino Serial ({})", port.port_name),
                is_simulator: false,
            });
        }
        drivers.push(DriverDescriptor {
            id: format!("obd2_wifi:{DEFAULT_WIFI_OBD_ENDPOINT}"),
            name: format!("OBD-II ELM327 Wi-Fi ({DEFAULT_WIFI_OBD_ENDPOINT})"),
            is_simulator: false,
        });
        drivers
    }

    /// Connects to `obd2:<port>[@<baud>][#<proto>]`, `obd2_wifi:<host:port>[#<proto>]`, or `ini_serial:<port>`.
    pub async fn connect(
        &self,
        driver_id: &str,
        app_handle: Option<AppHandle>,
    ) -> Result<EcuIdentity, String> {
        if let Some(rest) = driver_id.strip_prefix("obd2:") {
            let (port_and_baud, proto) = parse_obd_protocol_suffix(rest);
            let (port, explicit_baud) = if let Some((p, b_str)) = port_and_baud.rsplit_once('@') {
                if let Ok(b_num) = b_str.parse::<u32>() {
                    if let Some(b_enum) = parse_serial_baud_token(b_num) {
                        (p, Some(b_enum))
                    } else {
                        (port_and_baud, None)
                    }
                } else {
                    (port_and_baud, None)
                }
            } else {
                (port_and_baud, None)
            };
            let timeout = Duration::from_millis(if proto == ProtocolSelection::Auto {
                3500
            } else {
                2000
            });
            if let Some(baud) = explicit_baud {
                let cfg = Obd2DriverConfig {
                    protocol: proto,
                    baud_rate: baud,
                    command_timeout: timeout,
                    ..Obd2DriverConfig::default()
                };
                let transport = SerialObdTransport::new(port, baud);
                let driver = Arc::new(Obd2Driver::new(transport, cfg));
                return self.connect_driver(driver, app_handle).await;
            }
            let candidates = [
                SerialBaudRate::Baud38400,
                SerialBaudRate::Baud115200,
                SerialBaudRate::Baud9600,
            ];
            let mut last_err = String::from("OBD serial handshake failed");
            for baud in candidates {
                let cfg = Obd2DriverConfig {
                    protocol: proto,
                    baud_rate: baud,
                    command_timeout: timeout,
                    ..Obd2DriverConfig::default()
                };
                let transport = SerialObdTransport::new(port, baud);
                let driver = Arc::new(Obd2Driver::new(transport, cfg));
                match self.connect_driver(driver, app_handle.clone()).await {
                    Ok(id) => return Ok(id),
                    Err(e) => {
                        let lower = e.to_ascii_lowercase();
                        if lower.contains("failed to open serial port")
                            || lower.contains("no such file")
                            || lower.contains("not found")
                        {
                            return Err(e);
                        }
                        last_err = e;
                    }
                }
            }
            return Err(last_err);
        }
        if let Some(rest) = driver_id.strip_prefix("obd2_wifi:") {
            let (endpoint, proto) = parse_obd_protocol_suffix(rest);
            let cfg = Obd2DriverConfig {
                protocol: proto,
                command_timeout: Duration::from_millis(3500),
                ..Obd2DriverConfig::default()
            };
            let transport = WifiObdTransport::new(endpoint);
            let driver = Arc::new(Obd2Driver::new(transport, cfg));
            return self.connect_driver(driver, app_handle).await;
        }
        if let Some(port) = driver_id.strip_prefix("ini_serial:") {
            let driver = Arc::new(IniSerialDriver::new(port, 115_200));
            return self.connect_driver(driver, app_handle).await;
        }
        Err(format!(
            "Unknown hardware telemetry driver '{driver_id}': use 'obd2:<port>', 'obd2_wifi:<host:port>', or 'ini_serial:<port>'"
        ))
    }

    /// Connects any [`EcuDriver`] + [`TelemetrySource`] implementation and spawns forwarding.
    pub async fn connect_driver<D>(
        &self,
        driver: Arc<D>,
        app_handle: Option<AppHandle>,
    ) -> Result<EcuIdentity, String>
    where
        D: EcuDriver + TelemetrySource + 'static,
    {
        let mut session = self.session.lock().await;

        if let Some(prev_task) = self.forward_task.lock().await.take() {
            prev_task.abort();
        }
        let my_gen = self.connection_gen.fetch_add(1, Ordering::AcqRel) + 1;
        *self.latest_frame.lock().await = None;

        let identity = session.connect(driver).await.map_err(|e| e.to_string())?;
        let mut rx = match session.subscribe() {
            Ok(rx) => rx,
            Err(e) => {
                let _ = session.disconnect().await;
                return Err(e.to_string());
            }
        };

        if let Some(ref app) = app_handle {
            let _ = app.emit(
                "kivi://ecu-status",
                &ConnectionState::Connected {
                    identity: identity.clone(),
                },
            );
        }

        let latest_frame = self.latest_frame.clone();
        let connection_gen = self.connection_gen.clone();
        let app = app_handle;
        let handle = tauri::async_runtime::spawn(async move {
            loop {
                if connection_gen.load(Ordering::Acquire) != my_gen {
                    return;
                }
                match rx.recv().await {
                    Ok(frame) => {
                        if connection_gen.load(Ordering::Acquire) != my_gen {
                            return;
                        }
                        *latest_frame.lock().await = Some(frame.clone());
                        if let Some(ref a) = app {
                            let _ = a.emit("kivi://telemetry", &frame);
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
            if connection_gen.load(Ordering::Acquire) == my_gen {
                if let Some(ref a) = app {
                    let _ = a.emit("kivi://ecu-status", &ConnectionState::Disconnected);
                }
            }
        });
        *self.forward_task.lock().await = Some(handle);

        Ok(identity)
    }

    /// Disconnects the active ECU telemetry session.
    pub async fn disconnect(&self, app_handle: Option<AppHandle>) -> Result<(), String> {
        let mut session = self.session.lock().await;
        self.connection_gen.fetch_add(1, Ordering::AcqRel);
        if let Some(prev_task) = self.forward_task.lock().await.take() {
            prev_task.abort();
        }
        let disconnect_res = session.disconnect().await.map_err(|e| e.to_string());
        *self.latest_frame.lock().await = None;

        if let Some(ref app) = app_handle {
            let _ = app.emit("kivi://ecu-status", &ConnectionState::Disconnected);
        }
        disconnect_res
    }

    /// Returns the current connection state.
    pub async fn get_status(&self) -> ConnectionState {
        self.session.lock().await.current_state()
    }

    /// Returns a copy of the latest received telemetry frame, if any.
    pub async fn get_latest_frame(&self) -> Option<TelemetryFrame> {
        self.latest_frame.lock().await.clone()
    }

    /// Subscribes to the active telemetry broadcast stream.
    pub async fn subscribe(&self) -> Result<tokio::sync::broadcast::Receiver<TelemetryFrame>, String> {
        self.session.lock().await.subscribe().map_err(|e| e.to_string())
    }

    /// Atomically subscribes to the broadcast stream and snapshots the matching ECU identity.
    pub async fn subscribe_with_identity(
        &self,
    ) -> Result<(tokio::sync::broadcast::Receiver<TelemetryFrame>, Option<EcuIdentity>), String> {
        let session = self.session.lock().await;
        let rx = session.subscribe().map_err(|e| e.to_string())?;
        let identity = match session.current_state() {
            ConnectionState::Connected { identity } => Some(identity),
            _ => None,
        };
        Ok((rx, identity))
    }

    /// Returns the connected ECU identity, if currently connected.
    pub async fn get_identity(&self) -> Option<EcuIdentity> {
        match self.get_status().await {
            ConnectionState::Connected { identity } => Some(identity),
            _ => None,
        }
    }
}
