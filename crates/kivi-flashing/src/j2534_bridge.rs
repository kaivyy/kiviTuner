//! Out-of-process 32-bit J2534 bridge transport and stdio NDJSON server loop.
//!
//! Enables a 64-bit Windows build of kiviTuner to load legacy 32-bit vendor
//! J2534 DLLs (`op20pt32.dll`, `mvci32.dll`, `smj2534.dll`) via a companion
//! 32-bit helper process (`kivi-j2534-bridge-x86.exe`), isolating 32-bit FFI
//! inside a separate process boundary while exposing the standard [`Device`] trait.

use crate::device::{Device, DeviceError, DeviceInfo, DeviceKind, TransportKind};
use crate::j2534_types::{J2534Error, PassThruMsg, ProtocolId};
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

pub const BRIDGE_PROTOCOL_VERSION: &str = "1";
pub const MAX_BRIDGE_LINE_BYTES: usize = 65_536;
pub const BRIDGE_SHUTDOWN_GRACE_MS: u64 = 250;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum BridgeRequest {
    Ping,
    Open {
        dll_path: String,
    },
    Connect {
        protocol_id: u32,
        baud_rate: u32,
    },
    Send {
        protocol_id: u32,
        tx_flags: u32,
        data_hex: String,
    },
    Receive,
    ReadVoltageMv,
    Disconnect,
    Close,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BridgeErrorKind {
    NotOpen,
    AlreadyOpen,
    NotConnected,
    Timeout,
    Driver,
    NotFound,
    Transport,
    Unsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum BridgeResponse {
    Ok,
    Pong {
        arch: String,
        version: String,
    },
    OpenOk {
        name: String,
        firmware_version: Option<String>,
        dll_version: Option<String>,
    },
    Msg {
        protocol_id: u32,
        rx_status: u32,
        timestamp: u32,
        data_hex: String,
    },
    Voltage {
        mv: u32,
    },
    Err {
        kind: BridgeErrorKind,
        code: Option<u32>,
        message: String,
    },
}

fn j2534_error_to_u32(err: J2534Error) -> u32 {
    err.as_u32()
}

pub fn device_error_to_bridge(err: DeviceError) -> BridgeResponse {
    let message = err.to_string();
    let (kind, code) = match err {
        DeviceError::NotOpen => (BridgeErrorKind::NotOpen, None),
        DeviceError::AlreadyOpen => (BridgeErrorKind::AlreadyOpen, None),
        DeviceError::NotConnected => (BridgeErrorKind::NotConnected, None),
        DeviceError::Timeout => (BridgeErrorKind::Timeout, None),
        DeviceError::Driver(code) => (BridgeErrorKind::Driver, Some(j2534_error_to_u32(code))),
        DeviceError::NotFound(_) => (BridgeErrorKind::NotFound, None),
        DeviceError::Transport(_) => (BridgeErrorKind::Transport, None),
        DeviceError::Unsupported(_) => (BridgeErrorKind::Unsupported, None),
    };
    BridgeResponse::Err {
        kind,
        code,
        message,
    }
}

pub fn bridge_err_to_device(kind: BridgeErrorKind, code: Option<u32>, message: String) -> DeviceError {
    match kind {
        BridgeErrorKind::NotOpen => DeviceError::NotOpen,
        BridgeErrorKind::AlreadyOpen => DeviceError::AlreadyOpen,
        BridgeErrorKind::NotConnected => DeviceError::NotConnected,
        BridgeErrorKind::Timeout => DeviceError::Timeout,
        BridgeErrorKind::Driver => DeviceError::Driver(J2534Error::from_u32(code.unwrap_or(0x07))),
        BridgeErrorKind::NotFound => DeviceError::NotFound(message),
        BridgeErrorKind::Transport => DeviceError::Transport(message),
        BridgeErrorKind::Unsupported => DeviceError::Unsupported("bridged operation unsupported"),
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        let _ = write!(s, "{b:02X}");
    }
    s
}

fn decode_hex(hex: &str) -> Result<Vec<u8>, String> {
    let clean: String = hex.chars().filter(|c| !c.is_whitespace()).collect();
    if !clean.len().is_multiple_of(2) {
        return Err("odd hex string length in J2534 bridge payload".to_string());
    }
    let mut out = Vec::with_capacity(clean.len() / 2);
    for pair in clean.as_bytes().as_chunks::<2>().0 {
        let s = std::str::from_utf8(pair).map_err(|_| "non-UTF8 hex byte".to_string())?;
        let val = u8::from_str_radix(s, 16)
            .map_err(|_| format!("invalid hex byte '{s}' in bridge payload"))?;
        out.push(val);
    }
    Ok(out)
}

fn read_bounded_line<R: BufRead>(
    reader: &mut R,
    max_bytes: usize,
) -> Result<Option<String>, String> {
    let mut raw = Vec::with_capacity(256);
    loop {
        let chunk = reader
            .fill_buf()
            .map_err(|e| format!("bridge stdout read failed: {e}"))?;
        if chunk.is_empty() {
            if raw.is_empty() {
                return Ok(None);
            }
            let s = String::from_utf8(raw)
                .map_err(|e| format!("bridge emitted non-UTF8 response: {e}"))?;
            return Ok(Some(s));
        }
        if let Some(nl) = chunk.iter().position(|&b| b == b'\n') {
            if raw.len() + nl + 1 > max_bytes {
                return Err(format!("bridge response line exceeds {max_bytes} bytes"));
            }
            raw.extend_from_slice(&chunk[..=nl]);
            reader.consume(nl + 1);
            let s = String::from_utf8(raw)
                .map_err(|e| format!("bridge emitted non-UTF8 response: {e}"))?;
            return Ok(Some(s));
        }
        let take = chunk.len();
        if raw.len() + take > max_bytes {
            return Err(format!("bridge response line exceeds {max_bytes} bytes"));
        }
        raw.extend_from_slice(chunk);
        reader.consume(take);
    }
}

/// Locates the 32-bit out-of-process J2534 bridge helper binary, if installed.
pub fn find_j2534_bridge_32_path() -> Option<PathBuf> {
    if let Ok(explicit) = std::env::var("KIVI_J2534_BRIDGE_32") {
        let p = PathBuf::from(explicit.trim());
        if p.is_file() {
            return Some(p);
        }
    }

    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;
    let candidates = [
        exe_dir.join("kivi-j2534-bridge-x86.exe"),
        exe_dir.join("binaries").join("kivi-j2534-bridge-x86.exe"),
        exe_dir.join("resources").join("binaries").join("kivi-j2534-bridge-x86.exe"),
        exe_dir.join("resources").join("kivi-j2534-bridge-x86.exe"),
        exe_dir.join("..").join("binaries").join("kivi-j2534-bridge-x86.exe"),
    ];
    candidates.into_iter().find(|p| p.is_file())
}

struct BridgeProcess {
    child: Child,
    stdin: Option<BufWriter<ChildStdin>>,
    stdout: BufReader<ChildStdout>,
}

impl BridgeProcess {
    fn spawn(exe: &Path, extra_args: &[String]) -> Result<Self, DeviceError> {
        let mut cmd = Command::new(exe);
        cmd.args(extra_args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = cmd.spawn().map_err(|e| {
            DeviceError::NotFound(format!(
                "failed to launch 32-bit J2534 bridge '{}': {e}",
                exe.display()
            ))
        })?;
        let stdin = child.stdin.take().ok_or_else(|| {
            let _ = child.kill();
            let _ = child.wait();
            DeviceError::Transport("32-bit J2534 bridge stdin unavailable".into())
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            let _ = child.kill();
            let _ = child.wait();
            DeviceError::Transport("32-bit J2534 bridge stdout unavailable".into())
        })?;
        Ok(Self {
            child,
            stdin: Some(BufWriter::new(stdin)),
            stdout: BufReader::new(stdout),
        })
    }

    fn exchange(&mut self, req: &BridgeRequest) -> Result<BridgeResponse, DeviceError> {
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| DeviceError::Transport("32-bit J2534 bridge closed".into()))?;
        let line = serde_json::to_string(req)
            .map_err(|e| DeviceError::Transport(format!("bridge serialize error: {e}")))?;
        writeln!(stdin, "{line}")
            .and_then(|()| stdin.flush())
            .map_err(|e| DeviceError::Transport(format!("bridge pipe write error: {e}")))?;

        let raw = read_bounded_line(&mut self.stdout, MAX_BRIDGE_LINE_BYTES)
            .map_err(DeviceError::Transport)?
            .ok_or_else(|| {
                DeviceError::Transport("32-bit J2534 bridge exited unexpectedly".into())
            })?;
        serde_json::from_str(raw.trim_end_matches(&['\r', '\n'][..]))
            .map_err(|e| DeviceError::Transport(format!("invalid bridge response JSON: {e}")))
    }

    fn shutdown(mut self) {
        // Close stdin first so the helper cleanly observes EOF and exits on its own.
        drop(self.stdin.take());
        let deadline = Instant::now() + Duration::from_millis(BRIDGE_SHUTDOWN_GRACE_MS);
        while Instant::now() < deadline {
            match self.child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) => std::thread::sleep(Duration::from_millis(5)),
                Err(_) => break,
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// [`Device`] implementation proxying SAE J2534 calls to a 32-bit companion process.
pub struct BridgedJ2534Device {
    bridge_exe: PathBuf,
    bridge_extra_args: Vec<String>,
    dll_path: String,
    name: String,
    firmware_version: Option<String>,
    dll_version: Option<String>,
    process: Option<BridgeProcess>,
    open: bool,
    protocol: Option<ProtocolId>,
}

impl Drop for BridgedJ2534Device {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

impl BridgedJ2534Device {
    pub fn new(bridge_exe: PathBuf, dll_path: impl Into<String>) -> Self {
        let dll_path = dll_path.into();
        let file_name = Path::new(&dll_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("J2534 (32-bit Bridge)")
            .to_string();
        Self {
            bridge_exe,
            bridge_extra_args: Vec::new(),
            dll_path,
            name: format!("{file_name} [32-bit Bridge]"),
            firmware_version: None,
            dll_version: None,
            process: None,
            open: false,
            protocol: None,
        }
    }

    pub fn with_bridge_args(mut self, args: Vec<String>) -> Self {
        self.bridge_extra_args = args;
        self
    }

    fn exchange_response(&mut self, req: BridgeRequest) -> Result<BridgeResponse, DeviceError> {
        let res = {
            let proc = self.process.as_mut().ok_or(DeviceError::NotOpen)?;
            proc.exchange(&req)
        };
        match res {
            Ok(BridgeResponse::Err {
                kind,
                code,
                message,
            }) => Err(bridge_err_to_device(kind, code, message)),
            Ok(resp) => Ok(resp),
            Err(e) => {
                let child_dead = self
                    .process
                    .as_mut()
                    .and_then(|p| p.child.try_wait().ok().flatten())
                    .is_some();
                let is_pipe_or_exit = matches!(
                    &e,
                    DeviceError::Transport(msg)
                        if msg.contains("exited unexpectedly") || msg.contains("pipe write error")
                );
                if child_dead || is_pipe_or_exit {
                    if let Some(proc) = self.process.take() {
                        proc.shutdown();
                    }
                    self.open = false;
                    self.protocol = None;
                }
                Err(e)
            }
        }
    }
}

impl Device for BridgedJ2534Device {
    fn open(&mut self) -> Result<(), DeviceError> {
        if self.open {
            return Err(DeviceError::AlreadyOpen);
        }
        let mut proc = BridgeProcess::spawn(&self.bridge_exe, &self.bridge_extra_args)?;
        let res = proc.exchange(&BridgeRequest::Open {
            dll_path: self.dll_path.clone(),
        });
        match res {
            Ok(BridgeResponse::OpenOk {
                name,
                firmware_version,
                dll_version,
            }) => {
                if !name.trim().is_empty() {
                    self.name = format!("{name} [32-bit Bridge]");
                }
                self.firmware_version = firmware_version;
                self.dll_version = dll_version;
                self.process = Some(proc);
                self.open = true;
                self.protocol = None;
                Ok(())
            }
            Ok(BridgeResponse::Err {
                kind,
                code,
                message,
            }) => {
                proc.shutdown();
                Err(bridge_err_to_device(kind, code, message))
            }
            Ok(_) => {
                proc.shutdown();
                Err(DeviceError::Transport(
                    "unexpected response to bridge open request".into(),
                ))
            }
            Err(e) => {
                proc.shutdown();
                Err(e)
            }
        }
    }

    fn close(&mut self) -> Result<(), DeviceError> {
        let mut first_err = None;
        if let Some(mut proc) = self.process.take() {
            if self.open {
                if let Ok(BridgeResponse::Err {
                    kind,
                    code,
                    message,
                }) = proc.exchange(&BridgeRequest::Close)
                {
                    first_err = Some(bridge_err_to_device(kind, code, message));
                }
            }
            proc.shutdown();
        }
        self.open = false;
        self.protocol = None;
        self.firmware_version = None;
        self.dll_version = None;
        match first_err {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    fn connect(&mut self, protocol: ProtocolId, baud_rate: u32) -> Result<(), DeviceError> {
        if !self.open {
            return Err(DeviceError::NotOpen);
        }
        match self.exchange_response(BridgeRequest::Connect {
            protocol_id: protocol.as_u32(),
            baud_rate,
        })? {
            BridgeResponse::Ok => {
                self.protocol = Some(protocol);
                Ok(())
            }
            _ => Err(DeviceError::Transport("unexpected bridge connect reply".into())),
        }
    }

    fn disconnect(&mut self) -> Result<(), DeviceError> {
        if !self.open {
            return Err(DeviceError::NotOpen);
        }
        let res = self.exchange_response(BridgeRequest::Disconnect);
        self.protocol = None;
        match res? {
            BridgeResponse::Ok => Ok(()),
            _ => Err(DeviceError::Transport("unexpected bridge disconnect reply".into())),
        }
    }

    fn send(&mut self, msg: &PassThruMsg) -> Result<(), DeviceError> {
        if !self.open {
            return Err(DeviceError::NotOpen);
        }
        match self.exchange_response(BridgeRequest::Send {
            protocol_id: msg.protocol_id.as_u32(),
            tx_flags: msg.tx_flags,
            data_hex: encode_hex(&msg.data),
        })? {
            BridgeResponse::Ok => Ok(()),
            _ => Err(DeviceError::Transport("unexpected bridge send reply".into())),
        }
    }

    fn receive(&mut self) -> Result<PassThruMsg, DeviceError> {
        if !self.open {
            return Err(DeviceError::NotOpen);
        }
        match self.exchange_response(BridgeRequest::Receive)? {
            BridgeResponse::Msg {
                protocol_id,
                rx_status,
                timestamp,
                data_hex,
            } => {
                let data = decode_hex(&data_hex).map_err(DeviceError::Transport)?;
                let mut msg = PassThruMsg::new(ProtocolId::from_u32(protocol_id), data);
                msg.rx_status = rx_status;
                msg.timestamp = timestamp;
                Ok(msg)
            }
            _ => Err(DeviceError::Transport("unexpected bridge receive reply".into())),
        }
    }

    fn read_voltage_millivolts(&mut self) -> Result<u32, DeviceError> {
        if !self.open {
            return Err(DeviceError::NotOpen);
        }
        match self.exchange_response(BridgeRequest::ReadVoltageMv)? {
            BridgeResponse::Voltage { mv } => Ok(mv),
            _ => Err(DeviceError::Transport("unexpected bridge voltage reply".into())),
        }
    }

    fn info(&self) -> DeviceInfo {
        DeviceInfo {
            kind: DeviceKind::J2534,
            name: self.name.clone(),
            transport: TransportKind::Usb,
            firmware_version: self.firmware_version.clone(),
            dll_version: self.dll_version.clone(),
        }
    }

    fn is_open(&self) -> bool {
        self.open
    }

    fn is_connected(&self) -> bool {
        self.protocol.is_some()
    }

    fn connected_protocol(&self) -> Option<ProtocolId> {
        self.protocol
    }

    fn is_mock(&self) -> bool {
        false
    }
}

fn handle_bridge_request<F>(
    req: BridgeRequest,
    dev_slot: &mut Option<Box<dyn Device>>,
    open_factory: &mut F,
) -> (BridgeResponse, bool)
where
    F: FnMut(&str) -> Result<Box<dyn Device>, DeviceError>,
{
    match req {
        BridgeRequest::Ping => (
            BridgeResponse::Pong {
                arch: std::env::consts::ARCH.to_string(),
                version: BRIDGE_PROTOCOL_VERSION.to_string(),
            },
            false,
        ),
        BridgeRequest::Open { dll_path } => {
            if dev_slot.as_ref().is_some_and(|d| d.is_open()) {
                return (device_error_to_bridge(DeviceError::AlreadyOpen), false);
            }
            let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut dev = open_factory(&dll_path)?;
                dev.open()?;
                let info = dev.info();
                Ok::<_, DeviceError>((dev, info))
            }));
            match res {
                Ok(Ok((dev, info))) => {
                    *dev_slot = Some(dev);
                    (
                        BridgeResponse::OpenOk {
                            name: info.name,
                            firmware_version: info.firmware_version,
                            dll_version: info.dll_version,
                        },
                        false,
                    )
                }
                Ok(Err(e)) => (device_error_to_bridge(e), false),
                Err(_) => (
                    BridgeResponse::Err {
                        kind: BridgeErrorKind::Transport,
                        code: None,
                        message: "J2534 driver panicked during open".into(),
                    },
                    false,
                ),
            }
        }
        BridgeRequest::Connect {
            protocol_id,
            baud_rate,
        } => {
            let Some(dev) = dev_slot.as_mut() else {
                return (device_error_to_bridge(DeviceError::NotOpen), false);
            };
            let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                dev.connect(ProtocolId::from_u32(protocol_id), baud_rate)
            }));
            match res {
                Ok(Ok(())) => (BridgeResponse::Ok, false),
                Ok(Err(e)) => (device_error_to_bridge(e), false),
                Err(_) => (
                    BridgeResponse::Err {
                        kind: BridgeErrorKind::Transport,
                        code: None,
                        message: "J2534 driver panicked during connect".into(),
                    },
                    false,
                ),
            }
        }
        BridgeRequest::Send {
            protocol_id,
            tx_flags,
            data_hex,
        } => {
            let Some(dev) = dev_slot.as_mut() else {
                return (device_error_to_bridge(DeviceError::NotOpen), false);
            };
            let payload = match decode_hex(&data_hex) {
                Ok(b) => b,
                Err(e) => {
                    return (
                        BridgeResponse::Err {
                            kind: BridgeErrorKind::Transport,
                            code: None,
                            message: e,
                        },
                        false,
                    );
                }
            };
            let msg = PassThruMsg::with_flags(ProtocolId::from_u32(protocol_id), tx_flags, payload);
            let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| dev.send(&msg)));
            match res {
                Ok(Ok(())) => (BridgeResponse::Ok, false),
                Ok(Err(e)) => (device_error_to_bridge(e), false),
                Err(_) => (
                    BridgeResponse::Err {
                        kind: BridgeErrorKind::Transport,
                        code: None,
                        message: "J2534 driver panicked during send".into(),
                    },
                    false,
                ),
            }
        }
        BridgeRequest::Receive => {
            let Some(dev) = dev_slot.as_mut() else {
                return (device_error_to_bridge(DeviceError::NotOpen), false);
            };
            let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| dev.receive()));
            match res {
                Ok(Ok(msg)) => (
                    BridgeResponse::Msg {
                        protocol_id: msg.protocol_id.as_u32(),
                        rx_status: msg.rx_status,
                        timestamp: msg.timestamp,
                        data_hex: encode_hex(&msg.data),
                    },
                    false,
                ),
                Ok(Err(e)) => (device_error_to_bridge(e), false),
                Err(_) => (
                    BridgeResponse::Err {
                        kind: BridgeErrorKind::Transport,
                        code: None,
                        message: "J2534 driver panicked during receive".into(),
                    },
                    false,
                ),
            }
        }
        BridgeRequest::ReadVoltageMv => {
            let Some(dev) = dev_slot.as_mut() else {
                return (device_error_to_bridge(DeviceError::NotOpen), false);
            };
            let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                dev.read_voltage_millivolts()
            }));
            match res {
                Ok(Ok(mv)) => (BridgeResponse::Voltage { mv }, false),
                Ok(Err(e)) => (device_error_to_bridge(e), false),
                Err(_) => (
                    BridgeResponse::Err {
                        kind: BridgeErrorKind::Transport,
                        code: None,
                        message: "J2534 driver panicked during ReadVbatt".into(),
                    },
                    false,
                ),
            }
        }
        BridgeRequest::Disconnect => {
            let Some(dev) = dev_slot.as_mut() else {
                return (device_error_to_bridge(DeviceError::NotOpen), false);
            };
            let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| dev.disconnect()));
            match res {
                Ok(Ok(())) => (BridgeResponse::Ok, false),
                Ok(Err(e)) => (device_error_to_bridge(e), false),
                Err(_) => (
                    BridgeResponse::Err {
                        kind: BridgeErrorKind::Transport,
                        code: None,
                        message: "J2534 driver panicked during disconnect".into(),
                    },
                    false,
                ),
            }
        }
        BridgeRequest::Close => {
            let resp = if let Some(mut dev) = dev_slot.take() {
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| dev.close())) {
                    Ok(Ok(())) => BridgeResponse::Ok,
                    Ok(Err(e)) => device_error_to_bridge(e),
                    Err(_) => BridgeResponse::Err {
                        kind: BridgeErrorKind::Transport,
                        code: None,
                        message: "J2534 driver panicked during close".into(),
                    },
                }
            } else {
                BridgeResponse::Ok
            };
            (resp, true)
        }
    }
}

/// Runs the synchronous stdio NDJSON bridge server loop until `Close` or EOF.
pub fn serve_bridge_stdio<R, W, F>(
    mut reader: R,
    mut writer: W,
    mut open_factory: F,
) -> std::io::Result<()>
where
    R: BufRead,
    W: Write,
    F: FnMut(&str) -> Result<Box<dyn Device>, DeviceError>,
{
    let mut dev_slot: Option<Box<dyn Device>> = None;
    loop {
        let line = match read_bounded_line(&mut reader, MAX_BRIDGE_LINE_BYTES) {
            Ok(Some(l)) => l,
            Ok(None) => break,
            Err(err_msg) => {
                let resp = BridgeResponse::Err {
                    kind: BridgeErrorKind::Transport,
                    code: None,
                    message: err_msg,
                };
                if let Ok(json) = serde_json::to_string(&resp) {
                    let _ = writeln!(writer, "{json}");
                    let _ = writer.flush();
                }
                break;
            }
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let (resp, should_exit) = match serde_json::from_str::<BridgeRequest>(trimmed) {
            Ok(req) => handle_bridge_request(req, &mut dev_slot, &mut open_factory),
            Err(e) => (
                BridgeResponse::Err {
                    kind: BridgeErrorKind::Transport,
                    code: None,
                    message: format!("malformed bridge request JSON: {e}"),
                },
                false,
            ),
        };
        let encoded = serde_json::to_string(&resp).unwrap_or_else(|_| {
            r#"{"status":"err","kind":"transport","code":null,"message":"serialize error"}"#
                .to_string()
        });
        writeln!(writer, "{encoded}")?;
        writer.flush()?;
        if should_exit {
            break;
        }
    }
    if let Some(mut dev) = dev_slot.take() {
        let _ = dev.close();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::{MockDevice, ScriptedReply};
    use std::io::Cursor;

    #[test]
    fn bridge_server_executes_full_device_lifecycle_over_ndjson() {
        let input = [
            serde_json::to_string(&BridgeRequest::Ping).unwrap(),
            serde_json::to_string(&BridgeRequest::Open {
                dll_path: "C:\\Windows\\SysWOW64\\op20pt32.dll".into(),
            })
            .unwrap(),
            serde_json::to_string(&BridgeRequest::Connect {
                protocol_id: ProtocolId::Iso15765.as_u32(),
                baud_rate: 500_000,
            })
            .unwrap(),
            serde_json::to_string(&BridgeRequest::ReadVoltageMv).unwrap(),
            serde_json::to_string(&BridgeRequest::Send {
                protocol_id: ProtocolId::Iso15765.as_u32(),
                tx_flags: 0x40,
                data_hex: "000007E01001".into(),
            })
            .unwrap(),
            serde_json::to_string(&BridgeRequest::Receive).unwrap(),
            serde_json::to_string(&BridgeRequest::Disconnect).unwrap(),
            serde_json::to_string(&BridgeRequest::Close).unwrap(),
        ]
        .join("\n")
            + "\n";

        let mut output = Vec::new();
        serve_bridge_stdio(Cursor::new(input.as_bytes()), &mut output, |_dll| {
            let mut m = MockDevice::new(DeviceKind::J2534, "OpenPort2 32-bit", TransportKind::Usb);
            m.set_script(vec![ScriptedReply::Message(PassThruMsg::new(
                ProtocolId::Iso15765,
                vec![0x00, 0x00, 0x07, 0xE8, 0x50, 0x01],
            ))]);
            Ok(Box::new(m))
        })
        .expect("serve bridge");

        let lines: Vec<BridgeResponse> = String::from_utf8(output)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str::<BridgeResponse>(l).unwrap())
            .collect();

        assert_eq!(lines.len(), 8);
        assert!(matches!(lines[0], BridgeResponse::Pong { .. }));
        assert!(matches!(lines[1], BridgeResponse::OpenOk { .. }));
        assert_eq!(lines[2], BridgeResponse::Ok);
        assert_eq!(lines[3], BridgeResponse::Voltage { mv: 12_600 });
        assert_eq!(lines[4], BridgeResponse::Ok);
        assert_eq!(
            lines[5],
            BridgeResponse::Msg {
                protocol_id: ProtocolId::Iso15765.as_u32(),
                rx_status: 0,
                timestamp: 0,
                data_hex: "000007E85001".into(),
            }
        );
        assert_eq!(lines[6], BridgeResponse::Ok);
        assert_eq!(lines[7], BridgeResponse::Ok);
    }

    #[test]
    fn bridge_error_roundtrip_preserves_device_error_variants() {
        for err in [
            DeviceError::NotOpen,
            DeviceError::AlreadyOpen,
            DeviceError::NotConnected,
            DeviceError::Timeout,
            DeviceError::Driver(J2534Error::InvalidChannelId),
            DeviceError::NotFound("missing.dll".into()),
            DeviceError::Transport("io err".into()),
            DeviceError::Unsupported("not implemented"),
        ] {
            let BridgeResponse::Err {
                kind,
                code,
                message,
            } = device_error_to_bridge(err.clone())
            else {
                panic!("expected Err variant");
            };
            let decoded = bridge_err_to_device(kind, code, message);
            match (&err, &decoded) {
                (DeviceError::Unsupported(_), DeviceError::Unsupported(_)) => {}
                (DeviceError::NotFound(_), DeviceError::NotFound(_)) => {}
                (DeviceError::Transport(_), DeviceError::Transport(_)) => {}
                _ => assert_eq!(decoded, err),
            }
        }
    }
}
