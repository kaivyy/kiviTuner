//! Real J2534 driver binding, behind the `j2534-ffi` feature.
//!
//! ## What this does
//!
//! Loads a vendor `*.dll` / `*.so` with `libloading`, resolves the fourteen
//! standard `PassThru*` entry points, and exposes them as typed function
//! pointers. This is the piece that turns kiviTuner from "knows what a frame
//! looks like" into "can put that frame on a bus".
//!
//! ## Why it is feature-gated
//!
//! `libloading` and `unsafe` are only needed when a driver is actually loaded.
//! Keeping them behind `j2534-ffi` means the default build stays portable,
//! `unsafe`-free and fully testable offline. That matters because the offline
//! suite is the only verification this project can perform — no ECU exists here.
//!
//! ## Safety
//!
//! Every `unsafe` block below is a call into a C ABI defined by SAE J2534. The
//! contract cannot be checked by the compiler; it is checked by:
//!
//! - the symbol list matching the published API,
//! - the message struct being `#[repr(C)]` with the published layout
//!   (see [`crate::j2534_abi`]),
//! - every pointer being derived from a live, owned value for the duration of
//!   the call.
//!
//! ## What is NOT verified
//!
//! No function here has been called against real hardware. Loading a driver and
//! resolving its symbols is tested; executing `PassThruConnect` against an ECU
//! is not, and must not be claimed.

use crate::j2534_abi::PassthruMsgAbi;
use libloading::{Library, Symbol};
use std::ffi::{c_char, c_void};
use std::path::Path;

/// The fourteen entry points every SAE J2534-1/2 driver must export.
///
/// Defined by the published SAE J2534-1 v04.04 PassThru API specification.
pub const J2534_REQUIRED_SYMBOLS: &[&str] = &[
    "PassThruClose",
    "PassThruConnect",
    "PassThruDisconnect",
    "PassThruGetLastError",
    "PassThruIoctl",
    "PassThruOpen",
    "PassThruReadMsgs",
    "PassThruReadVersion",
    "PassThruSetProgrammingVoltage",
    "PassThruStartMsgFilter",
    "PassThruStartPeriodicMsg",
    "PassThruStopMsgFilter",
    "PassThruStopPeriodicMsg",
    "PassThruWriteMsgs",
];

// SAE J2534-1 v04.04 C ABI signatures: stdcall on Win32, C on Unix (`extern "system"`),
// using exact 32-bit `u32` fields matching LLP64 / Win32 `unsigned long`.
type PassThruOpenFn = unsafe extern "system" fn(*const c_char, *mut u32) -> u32;
type PassThruCloseFn = unsafe extern "system" fn(u32) -> u32;
type PassThruConnectFn =
    unsafe extern "system" fn(u32, u32, u32, u32, *mut u32) -> u32;
type PassThruDisconnectFn = unsafe extern "system" fn(u32) -> u32;
type PassThruReadMsgsFn =
    unsafe extern "system" fn(u32, *mut PassthruMsgAbi, *mut u32, u32) -> u32;
type PassThruWriteMsgsFn =
    unsafe extern "system" fn(u32, *const PassthruMsgAbi, *mut u32, u32) -> u32;
type PassThruStartMsgFilterFn = unsafe extern "system" fn(
    u32,
    u32,
    *const PassthruMsgAbi,
    *const PassthruMsgAbi,
    *const PassthruMsgAbi,
    *mut u32,
) -> u32;
type PassThruStopMsgFilterFn = unsafe extern "system" fn(u32, u32) -> u32;
type PassThruIoctlFn = unsafe extern "system" fn(u32, u32, *mut c_void, *mut c_void) -> u32;
type PassThruGetLastErrorFn = unsafe extern "system" fn(*mut c_char) -> u32;
type PassThruReadVersionFn =
    unsafe extern "system" fn(u32, *mut c_char, *mut c_char, *mut c_char) -> u32;
type PassThruSetProgrammingVoltageFn = unsafe extern "system" fn(u32, u32, u32) -> u32;
type PassThruStartPeriodicMsgFn =
    unsafe extern "system" fn(u32, *const PassthruMsgAbi, *mut u32, u32) -> u32;
type PassThruStopPeriodicMsgFn = unsafe extern "system" fn(u32, u32) -> u32;

/// Why a driver could not be loaded or used.
#[derive(Debug)]
pub enum FfiError {
    /// The shared library could not be opened.
    Load { path: String, message: String },
    /// A required symbol is missing from the library.
    MissingSymbol { symbol: String, message: String },
    /// The driver returned a non-zero status.
    Driver(u32),
    /// The driver returned a non-zero status and supplied no text.
    DriverWithMessage { code: u32, message: String },
}

impl std::fmt::Display for FfiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FfiError::Load { path, message } => {
                write!(f, "could not load J2534 driver '{path}': {message}")
            }
            FfiError::MissingSymbol { symbol, message } => {
                write!(f, "driver is missing symbol '{symbol}': {message}")
            }
            FfiError::Driver(code) => write!(f, "driver returned status {code:#04x}"),
            FfiError::DriverWithMessage { code, message } => {
                write!(f, "driver returned status {code:#04x}: {message}")
            }
        }
    }
}

impl std::error::Error for FfiError {}

/// Resolved entry points of a loaded J2534 driver.
///
/// The [`Library`] is kept alive for as long as this struct: dropping it would
/// unload the code the function pointers refer to.
pub struct J2534EntryPoints {
    open: PassThruOpenFn,
    close: PassThruCloseFn,
    connect: PassThruConnectFn,
    disconnect: PassThruDisconnectFn,
    read_msgs: PassThruReadMsgsFn,
    write_msgs: PassThruWriteMsgsFn,
    start_msg_filter: PassThruStartMsgFilterFn,
    stop_msg_filter: PassThruStopMsgFilterFn,
    ioctl: PassThruIoctlFn,
    get_last_error: PassThruGetLastErrorFn,
    read_version: PassThruReadVersionFn,
    set_programming_voltage: PassThruSetProgrammingVoltageFn,
    start_periodic_msg: PassThruStartPeriodicMsgFn,
    stop_periodic_msg: PassThruStopPeriodicMsgFn,
    // Field order matters for drop order: Rust drops fields in declaration order,
    // so `_lib` is declared last to outlive all fields before unloading.
    _lib: Library,
}

/// Copy a symbol out of the library, mapping the loader error to [`FfiError`].
unsafe fn resolve<'a, T>(
    lib: &'a Library,
    name: &str,
) -> Result<Symbol<'a, T>, FfiError> {
    lib.get::<T>(name.as_bytes()).map_err(|e| FfiError::MissingSymbol {
        symbol: name.to_string(),
        message: e.to_string(),
    })
}

impl J2534EntryPoints {
    /// Load a driver and resolve every required entry point.
    ///
    /// Fails with [`FfiError::Load`] if the file is not a loadable library and
    /// with [`FfiError::MissingSymbol`] if it is not a J2534 driver.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, FfiError> {
        let path_ref = path.as_ref();
        let display = path_ref.display().to_string();

        // SAFETY: `Library::new` / `load_with_flags` runs the OS dynamic loader.
        // On Windows, `LOAD_WITH_ALTERED_SEARCH_PATH` lets sibling vendor DLLs
        // (e.g. `ftd2xx.dll`) beside the J2534 DLL resolve reliably when an
        // absolute path is provided, falling back to `Library::new` otherwise.
        #[cfg(target_os = "windows")]
        let lib = unsafe {
            use libloading::os::windows::{Library as WinLibrary, LOAD_WITH_ALTERED_SEARCH_PATH};
            WinLibrary::load_with_flags(path_ref, LOAD_WITH_ALTERED_SEARCH_PATH)
                .map(Library::from)
                .or_else(|_| Library::new(path_ref))
        }
        .map_err(|e| FfiError::Load {
            path: display,
            message: e.to_string(),
        })?;

        #[cfg(not(target_os = "windows"))]
        let lib = unsafe { Library::new(path_ref) }.map_err(|e| FfiError::Load {
            path: display,
            message: e.to_string(),
        })?;

        // SAFETY: each `resolve` only reads the library's symbol table; the
        // transmute to a function pointer is sound because the C signatures
        // above are transcribed from the published header.
        unsafe {
            let open = *resolve::<PassThruOpenFn>(&lib, "PassThruOpen")?;
            let close = *resolve::<PassThruCloseFn>(&lib, "PassThruClose")?;
            let connect = *resolve::<PassThruConnectFn>(&lib, "PassThruConnect")?;
            let disconnect = *resolve::<PassThruDisconnectFn>(&lib, "PassThruDisconnect")?;
            let read_msgs = *resolve::<PassThruReadMsgsFn>(&lib, "PassThruReadMsgs")?;
            let write_msgs = *resolve::<PassThruWriteMsgsFn>(&lib, "PassThruWriteMsgs")?;
            let start_msg_filter =
                *resolve::<PassThruStartMsgFilterFn>(&lib, "PassThruStartMsgFilter")?;
            let stop_msg_filter =
                *resolve::<PassThruStopMsgFilterFn>(&lib, "PassThruStopMsgFilter")?;
            let ioctl = *resolve::<PassThruIoctlFn>(&lib, "PassThruIoctl")?;
            let get_last_error = *resolve::<PassThruGetLastErrorFn>(&lib, "PassThruGetLastError")?;
            let read_version = *resolve::<PassThruReadVersionFn>(&lib, "PassThruReadVersion")?;
            let set_programming_voltage =
                *resolve::<PassThruSetProgrammingVoltageFn>(&lib, "PassThruSetProgrammingVoltage")?;
            let start_periodic_msg =
                *resolve::<PassThruStartPeriodicMsgFn>(&lib, "PassThruStartPeriodicMsg")?;
            let stop_periodic_msg =
                *resolve::<PassThruStopPeriodicMsgFn>(&lib, "PassThruStopPeriodicMsg")?;

            Ok(Self {
                _lib: lib,
                open,
                close,
                connect,
                disconnect,
                read_msgs,
                write_msgs,
                start_msg_filter,
                stop_msg_filter,
                ioctl,
                get_last_error,
                read_version,
                set_programming_voltage,
                start_periodic_msg,
                stop_periodic_msg,
            })
        }
    }

    /// Open the device. Returns the device handle.
    pub fn open_device(&self, name: Option<&str>) -> Result<u32, FfiError> {
        let c_name = name
            .map(std::ffi::CString::new)
            .transpose()
            .map_err(|e| FfiError::Load {
                path: name.unwrap_or_default().to_string(),
                message: format!("J2534 device name contains interior NUL byte: {e}"),
            })?;
        let name_ptr = c_name
            .as_ref()
            .map(|c| c.as_ptr())
            .unwrap_or(std::ptr::null());
        let mut device_id: u32 = 0;

        let rc = unsafe { (self.open)(name_ptr, &mut device_id) };
        self.check(rc)?;
        Ok(device_id)
    }

    /// Close the device handle.
    pub fn close_device(&self, device_id: u32) -> Result<(), FfiError> {
        let rc = unsafe { (self.close)(device_id) };
        self.check(rc)
    }

    /// Open a channel on `protocol_id`.
    pub fn connect(
        &self,
        device_id: u32,
        protocol_id: u32,
        flags: u32,
        baud_rate: u32,
    ) -> Result<u32, FfiError> {
        let mut channel_id: u32 = 0;
        let rc = unsafe {
            (self.connect)(device_id, protocol_id, flags, baud_rate, &mut channel_id)
        };
        self.check(rc)?;
        Ok(channel_id)
    }

    /// Close the channel.
    pub fn disconnect(&self, channel_id: u32) -> Result<(), FfiError> {
        let rc = unsafe { (self.disconnect)(channel_id) };
        self.check(rc)
    }

    /// Transmit one message. Returns the number of messages the driver accepted.
    pub fn write_msg(&self, channel_id: u32, msg: &PassthruMsgAbi, timeout_ms: u32) -> Result<u32, FfiError> {
        let mut count: u32 = 1;
        let rc = unsafe {
            (self.write_msgs)(
                channel_id,
                msg as *const PassthruMsgAbi,
                &mut count,
                timeout_ms,
            )
        };
        self.check(rc)?;
        Ok(count)
    }

    /// Receive one message.
    pub fn read_msg(&self, channel_id: u32, timeout_ms: u32) -> Result<PassthruMsgAbi, FfiError> {
        let mut msg = PassthruMsgAbi::zeroed();
        let mut count: u32 = 1;
        let rc = unsafe { (self.read_msgs)(channel_id, &mut msg, &mut count, timeout_ms) };
        if count > 0 && matches!(rc, 0x00 | 0x09 | 0x12) {
            return Ok(msg);
        }
        self.check(rc)?;
        if count == 0 {
            return Err(FfiError::Driver(0x10));
        }
        Ok(msg)
    }

    /// Read battery voltage in millivolts.
    pub fn read_voltage_millivolts(&self, device_id: u32) -> Result<u32, FfiError> {
        let mut out: u32 = 0;
        let rc = unsafe {
            (self.ioctl)(
                device_id,
                crate::j2534_types::ioctl::READ_VBATT,
                std::ptr::null_mut(),
                &mut out as *mut u32 as *mut c_void,
            )
        };
        self.check(rc)?;
        Ok(out)
    }

    /// Clear the receive buffer.
    pub fn clear_rx_buffer(&self, channel_id: u32) -> Result<(), FfiError> {
        let rc = unsafe {
            (self.ioctl)(
                channel_id,
                crate::j2534_types::ioctl::CLEAR_RX_BUFFER,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        self.check(rc)
    }

    /// Clear the transmit buffer.
    pub fn clear_tx_buffer(&self, channel_id: u32) -> Result<(), FfiError> {
        let rc = unsafe {
            (self.ioctl)(
                channel_id,
                crate::j2534_types::ioctl::CLEAR_TX_BUFFER,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        self.check(rc)
    }

    /// Read the driver's firmware / DLL / API version strings.
    pub fn read_version(&self, device_id: u32) -> Result<(String, String, String), FfiError> {
        let mut firmware = [0 as c_char; 80];
        let mut dll = [0 as c_char; 80];
        let mut api = [0 as c_char; 80];
        let rc = unsafe {
            (self.read_version)(
                device_id,
                firmware.as_mut_ptr(),
                dll.as_mut_ptr(),
                api.as_mut_ptr(),
            )
        };
        self.check(rc)?;
        Ok((
            c_buf_to_string(&firmware),
            c_buf_to_string(&dll),
            c_buf_to_string(&api),
        ))
    }

    /// Start a pass/block/flow-control filter.
    pub fn start_msg_filter(
        &self,
        channel_id: u32,
        filter_type: u32,
        mask: Option<&PassthruMsgAbi>,
        pattern: Option<&PassthruMsgAbi>,
        flow_control: Option<&PassthruMsgAbi>,
    ) -> Result<u32, FfiError> {
        let mut filter_id: u32 = 0;
        let m = mask.map(|m| m as *const PassthruMsgAbi).unwrap_or(std::ptr::null());
        let p = pattern
            .map(|p| p as *const PassthruMsgAbi)
            .unwrap_or(std::ptr::null());
        let f = flow_control
            .map(|f| f as *const PassthruMsgAbi)
            .unwrap_or(std::ptr::null());
        let rc = unsafe {
            (self.start_msg_filter)(channel_id, filter_type, m, p, f, &mut filter_id)
        };
        self.check(rc)?;
        Ok(filter_id)
    }

    /// Stop a filter.
    pub fn stop_msg_filter(&self, channel_id: u32, filter_id: u32) -> Result<(), FfiError> {
        let rc = unsafe { (self.stop_msg_filter)(channel_id, filter_id) };
        self.check(rc)
    }

    /// Apply programming voltage to a pin, for bench flashing.
    pub fn set_programming_voltage(
        &self,
        device_id: u32,
        pin: u32,
        voltage_mv: u32,
    ) -> Result<(), FfiError> {
        let rc = unsafe { (self.set_programming_voltage)(device_id, pin, voltage_mv) };
        self.check(rc)
    }

    /// Start a repeating message.
    pub fn start_periodic_msg(
        &self,
        channel_id: u32,
        msg: &PassthruMsgAbi,
        interval_ms: u32,
    ) -> Result<u32, FfiError> {
        let mut msg_id: u32 = 0;
        let rc = unsafe {
            (self.start_periodic_msg)(
                channel_id,
                msg as *const PassthruMsgAbi,
                &mut msg_id,
                interval_ms,
            )
        };
        self.check(rc)?;
        Ok(msg_id)
    }

    /// Stop a repeating message.
    pub fn stop_periodic_msg(&self, channel_id: u32, msg_id: u32) -> Result<(), FfiError> {
        let rc = unsafe { (self.stop_periodic_msg)(channel_id, msg_id) };
        self.check(rc)
    }

    /// Ask the driver for its last error text.
    pub fn last_error_text(&self) -> String {
        let mut buf = [0 as c_char; 256];
        let _ = unsafe { (self.get_last_error)(buf.as_mut_ptr()) };
        c_buf_to_string(&buf)
    }

    /// Map a driver status to `Ok` or [`FfiError`].
    fn check(&self, code: u32) -> Result<(), FfiError> {
        if code == 0 {
            Ok(())
        } else {
            let text = self.last_error_text();
            if text.is_empty() {
                Err(FfiError::Driver(code))
            } else {
                Err(FfiError::DriverWithMessage {
                    code,
                    message: text,
                })
            }
        }
    }
}

/// Read a NUL-terminated C string out of a fixed buffer.
fn c_buf_to_string(buf: &[c_char]) -> String {
    let bytes: Vec<u8> = buf
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8_lossy(&bytes).to_string()
}
