//! Companion out-of-process J2534 bridge executable (`kivi-j2534-bridge-x86.exe`).
//!
//! Typically compiled for `i686-pc-windows-msvc` and invoked transparently by
//! 64-bit Windows kiviTuner whenever a legacy 32-bit x86 J2534 vendor DLL is
//! selected, isolating Win32 FFI over stdin/stdout NDJSON lines.

use kivi_flashing::device::{Device, DeviceError, DeviceInfo, DeviceKind, TransportKind};
use kivi_flashing::j2534_bridge::serve_bridge_stdio;
use kivi_flashing::j2534_device::J2534Device;
use kivi_flashing::j2534_types::{PassThruMsg, ProtocolId};
use std::collections::VecDeque;
use std::io::{BufReader, BufWriter};

struct LoopbackBridgeDevice {
    open: bool,
    protocol: Option<ProtocolId>,
    rx: VecDeque<PassThruMsg>,
}

impl LoopbackBridgeDevice {
    fn new() -> Self {
        Self {
            open: false,
            protocol: None,
            rx: VecDeque::new(),
        }
    }
}

impl Device for LoopbackBridgeDevice {
    fn open(&mut self) -> Result<(), DeviceError> {
        if self.open {
            return Err(DeviceError::AlreadyOpen);
        }
        self.open = true;
        Ok(())
    }

    fn close(&mut self) -> Result<(), DeviceError> {
        self.open = false;
        self.protocol = None;
        self.rx.clear();
        Ok(())
    }

    fn connect(&mut self, protocol: ProtocolId, _baud_rate: u32) -> Result<(), DeviceError> {
        if !self.open {
            return Err(DeviceError::NotOpen);
        }
        self.protocol = Some(protocol);
        Ok(())
    }

    fn disconnect(&mut self) -> Result<(), DeviceError> {
        if !self.open {
            return Err(DeviceError::NotOpen);
        }
        if self.protocol.is_none() {
            return Err(DeviceError::NotConnected);
        }
        self.protocol = None;
        self.rx.clear();
        Ok(())
    }

    fn send(&mut self, msg: &PassThruMsg) -> Result<(), DeviceError> {
        if !self.open {
            return Err(DeviceError::NotOpen);
        }
        if self.protocol.is_none() {
            return Err(DeviceError::NotConnected);
        }
        self.rx.push_back(msg.clone());
        Ok(())
    }

    fn receive(&mut self) -> Result<PassThruMsg, DeviceError> {
        if !self.open {
            return Err(DeviceError::NotOpen);
        }
        if self.protocol.is_none() {
            return Err(DeviceError::NotConnected);
        }
        self.rx.pop_front().ok_or(DeviceError::Timeout)
    }

    fn read_voltage_millivolts(&mut self) -> Result<u32, DeviceError> {
        if !self.open {
            return Err(DeviceError::NotOpen);
        }
        Ok(12_800)
    }

    fn info(&self) -> DeviceInfo {
        DeviceInfo {
            kind: DeviceKind::J2534,
            name: "Loopback J2534 Test Driver".into(),
            transport: TransportKind::Usb,
            firmware_version: Some("FW 2.1".into()),
            dll_version: Some("DLL 1.0".into()),
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

fn main() {
    let loopback = std::env::args().any(|a| a == "--self-test-loopback");
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let reader = BufReader::new(stdin.lock());
    let writer = BufWriter::new(stdout.lock());

    let res = if loopback {
        serve_bridge_stdio(reader, writer, |_dll| -> Result<Box<dyn Device>, DeviceError> {
            Ok(Box::new(LoopbackBridgeDevice::new()))
        })
    } else {
        serve_bridge_stdio(reader, writer, |dll| -> Result<Box<dyn Device>, DeviceError> {
            Ok(Box::new(J2534Device::unloaded(dll)))
        })
    };

    if let Err(e) = res {
        eprintln!("kivi-j2534-bridge I/O error: {e}");
        std::process::exit(1);
    }
}
