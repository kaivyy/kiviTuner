//! Tauri IPC handlers and hardware transport state for ECU dump & flash operations.
//!
//! All production paths attach real hardware transports:
//! - J2534 Pass-Thru DLLs (`J2534Device`)
//! - Native Tactrix OpenPort 2.0 USB-CDC (`LiveOpenPort2Device`)
//! - Serial ELM327 / STN1110 (`SerialElmDevice`)
//! - ISO 13400 Diagnostics over IP (`DoIpDevice`)
//! - Linux SocketCAN (`SocketCanDevice`)
//!
//! Test doubles live strictly under `[dev-dependencies]` and are injected in
//! integration tests via [`FlashEngineState::attach_custom_transport`].

use crate::commands::AppState;
pub use crate::flash_stream::{DumpStreamPayload, FlashProgressEvent, UdsDumpStreamPayload};
use crate::flash_stream::PreparedFlashPhase;
use kivi_checksum::oem::registry::{auto_detect_oem_checksums, repair_oem_by_id, verify_oem_by_id};
use kivi_core::bin_export::validate_dumped_bin_integrity;
use kivi_core::hash::compute_hashes;
use kivi_flashing::device::{Device, DeviceError};
use kivi_flashing::doip_device::DoIpDevice;
use kivi_flashing::flash_engine::{
    dump_ecu_with_progress as engine_dump_ecu_with_progress,
    flash_ecu_with_progress as engine_flash_ecu_with_progress, FlashPlan,
};
use kivi_flashing::flight_recorder::{BusFlightRecorder, BusFrameRecord, RecordingDevice};
use kivi_flashing::flash_safety::{check_flash_safety, FlashSafetyPolicy, FlashTarget};
use kivi_flashing::hardware_kernel_session::{SharedDevice, UdsHardwareKernelSession};
use kivi_flashing::hardware_matrix::{hardware_verification_matrix, HardwareTargetCapability};
use kivi_flashing::honda_kline::{HondaEcuType, HondaEcuVendor};
use kivi_flashing::j2534_bridge::{find_j2534_bridge_32_path, BridgedJ2534Device};
use kivi_flashing::j2534_loader::{
    can_load_via_32bit_bridge, candidate_score, describe_incompatibility_with_bridge,
    inspect_library_binary, is_binary_compatible_with_host, DriverRegistry, LibraryBinaryInfo,
};
use kivi_flashing::j2534_types::ProtocolId;
use kivi_flashing::kernel_session::KernelSession;
use kivi_flashing::kline_serial_device::{RawKLineDevice, DEFAULT_KLINE_BAUD};
use kivi_flashing::openport2_serial::LiveOpenPort2Device;
use kivi_flashing::passthru_discovery::{
    list_serial_ports as discover_serial_ports, list_socketcan_interfaces, scan_standard_paths,
    SerialPortDescriptor,
};
use kivi_flashing::ram_kernel::KernelArchitecture;
use kivi_flashing::recovery_journal::{JournalStage, RecoveryJournalManager};
use kivi_flashing::seed_key_registry::is_seed_key_verified_for_live;
use kivi_flashing::serial_elm_device::{
    derive_elm_rx_can_id, ElmBusProfile, SerialElmDevice, WifiElmDevice, ELM_AUTO_BAUD_CANDIDATES,
};
use kivi_flashing::socketcan_device::SocketCanDevice;
use kivi_flashing::specialized_kernel_sessions::{
    HondaKLineKernelSession, RamKernelHardwareSession,
};
use kivi_flashing::uds_dump::{dump_via_uds23_with_progress, UdsDumpOptions};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::State;

use crate::commands::lock_or_err;

/// One attached flash device, as the frontend receives it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FlashDeviceDescriptor {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub transport: String,
    /// `true` only when a test injects a non-hardware double.
    pub is_mock: bool,
}

/// Result of a dump, as the frontend receives it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DumpResultPayload {
    pub base_address: u32,
    pub bytes: Vec<u8>,
    pub blocks_read: usize,
}

/// Result of a flash.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FlashResultPayload {
    pub verified: bool,
    pub bytes_written: usize,
    pub blocks_written: usize,
}

/// Result of a UDS service 0x23 (`ReadMemoryByAddress`) dump.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UdsDumpPayload {
    pub base_address: u32,
    pub bytes: Vec<u8>,
    pub blocks_read: usize,
    pub pending_retries: usize,
}

/// Enumerated serial port entry returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SerialPortRow {
    pub port_name: String,
    pub description: String,
    pub vid: Option<u16>,
    pub pid: Option<u16>,
    pub is_openport2: bool,
    pub adapter_hint: String,
}

impl From<SerialPortDescriptor> for SerialPortRow {
    fn from(d: SerialPortDescriptor) -> Self {
        Self {
            port_name: d.port_name,
            description: d.description,
            vid: d.vid,
            pid: d.pid,
            is_openport2: d.is_openport2,
            adapter_hint: d.adapter_hint,
        }
    }
}

/// Hardware transport selection attached to [`FlashEngineState`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransportChoice {
    /// A SAE J2534 pass-thru interface loaded from a vendor DLL / shared library.
    J2534,
    /// Native Tactrix OpenPort 2.0 USB-CDC (`at...` firmware protocol).
    OpenPort2,
    /// Serial / USB / Bluetooth SPP ELM327 / STN1110 / OBDLink adapter.
    Serial,
    /// Wi-Fi TCP ELM327 / STN1110 adapter (`192.168.0.10:35000`).
    ElmWifi,
    /// Raw USB-UART K-Line cable (FTDI / CH340 KKL 409.1).
    KLineSerial,
    /// ISO 13400 Diagnostics over IP (DoIP) Ethernet gateway.
    DoIp,
    /// Linux SocketCAN (`AF_CAN` raw CAN/ISO-TP interface).
    SocketCan,
}

impl TransportChoice {
    pub fn as_str(self) -> &'static str {
        match self {
            TransportChoice::J2534 => "j2534",
            TransportChoice::OpenPort2 => "openport2",
            TransportChoice::Serial => "serial",
            TransportChoice::ElmWifi => "elm_wifi",
            TransportChoice::KLineSerial => "kline_serial",
            TransportChoice::DoIp => "doip",
            TransportChoice::SocketCan => "socketcan",
        }
    }
}

/// Build the descriptor the UI shows for a transport.
pub fn describe_transport(choice: TransportChoice, name: &str) -> FlashDeviceDescriptor {
    FlashDeviceDescriptor {
        id: choice.as_str().to_string(),
        name: name.to_string(),
        kind: choice.as_str().to_string(),
        transport: match choice {
            TransportChoice::J2534 => "Usb".to_string(),
            TransportChoice::OpenPort2 => "UsbSerial".to_string(),
            TransportChoice::Serial | TransportChoice::KLineSerial => "Serial".to_string(),
            TransportChoice::ElmWifi => "Wifi".to_string(),
            TransportChoice::DoIp => "TcpIp".to_string(),
            TransportChoice::SocketCan => "Can".to_string(),
        },
        is_mock: false,
    }
}

/// What the caller knows about the target, for the safety gate.
#[derive(Debug, Clone, Copy)]
pub struct SafetyContext {
    pub expected_rom_size: Option<usize>,
    pub base_address: u32,
    pub is_known_ecu: bool,
    pub has_stock_backup: bool,
    pub stock_written_first: bool,
    pub checksum_correction_enabled: bool,
}

impl Default for SafetyContext {
    fn default() -> Self {
        Self {
            expected_rom_size: None,
            base_address: 0,
            is_known_ecu: false,
            has_stock_backup: false,
            stock_written_first: false,
            checksum_correction_enabled: true,
        }
    }
}

struct AttachedBench {
    device: Box<dyn Device + Send>,
    session: Box<dyn KernelSession + Send>,
    shared: Option<SharedDevice>,
}

/// Active session protocol family selected for the attached transport.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SessionProtocolChoice {
    UdsStandard {
        tx_can_id: Option<u32>,
        seed_key_algo_id: Option<String>,
        seed_key_param: Option<u16>,
        security_level: Option<u8>,
    },
    HondaKeihinKLine {
        fast_baud: Option<bool>,
    },
    HondaShindengenKLine {
        fast_baud: Option<bool>,
    },
    RamKernel {
        arch: String,
        ram_load_address: u32,
        kernel_payload: Vec<u8>,
        tx_can_id: Option<u32>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DumpAuditReport {
    pub sha256: String,
    pub integrity_checked: bool,
    pub detected_checksum_families: Vec<String>,
    pub all_checksums_valid: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FlashAuditReport {
    pub voltage_mv: Option<u32>,
    pub checksum_families: Vec<String>,
    pub checksum_auto_repaired: bool,
    pub checksum_verified_ok: bool,
    pub pre_repair_sha256: String,
    pub flashed_sha256: String,
    pub journal_stage: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecoveryJournalSnapshot {
    pub target_ecm_id: String,
    pub target_rom_crc32: u32,
    pub total_rom_size: usize,
    pub active_sector_index: usize,
    pub bytes_written: usize,
    pub stage: String,
}

#[derive(Default)]
struct FlashSessionState {
    bench: Option<AttachedBench>,
    cleanup_count: usize,
    transport: Option<TransportChoice>,
    safety: SafetyContext,
    selected_seed_key_algo: Option<String>,
    stock_backup_bytes: Option<Vec<u8>>,
    stock_backup_sha256: Option<String>,
    recovery_journal: Option<RecoveryJournalManager>,
    last_dump_audit: Option<DumpAuditReport>,
    last_flash_audit: Option<FlashAuditReport>,
}

impl FlashSessionState {
    fn reset_hardware_session(&mut self) {
        if let Some(mut old) = self.bench.take() {
            let _ = old.device.close();
        }
        self.cleanup_count = 0;
        self.transport = None;
        self.safety = SafetyContext::default();
        self.selected_seed_key_algo = None;
        self.stock_backup_bytes = None;
        self.stock_backup_sha256 = None;
        self.recovery_journal = None;
        self.last_dump_audit = None;
        self.last_flash_audit = None;
    }
}

/// Owns the attached flash transport, safety unlock gate, and bus flight recorder.
#[derive(Clone)]
pub struct FlashEngineState {
    session: Arc<Mutex<FlashSessionState>>,
    flight_recorder: Arc<BusFlightRecorder>,
    allow_experimental_write: Arc<AtomicBool>,
}

impl Default for FlashEngineState {
    fn default() -> Self {
        Self {
            session: Arc::new(Mutex::new(FlashSessionState::default())),
            flight_recorder: Arc::new(BusFlightRecorder::default()),
            allow_experimental_write: Arc::new(AtomicBool::new(false)),
        }
    }
}

fn journal_stage_label(stage: JournalStage) -> &'static str {
    match stage {
        JournalStage::Initialized => "Initialized",
        JournalStage::ErasingSectors => "ErasingSectors",
        JournalStage::WritingBlocks => "WritingBlocks",
        JournalStage::VerifyingChecksum => "VerifyingChecksum",
        JournalStage::FlashCompleted => "FlashCompleted",
        JournalStage::InterruptedAbnormally => "InterruptedAbnormally",
    }
}

fn now_epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

impl FlashEngineState {
    pub(crate) fn clone_handle(&self) -> Self {
        self.clone()
    }

    /// Returns a handle to the bounded bus flight recorder.
    pub fn flight_recorder(&self) -> Arc<BusFlightRecorder> {
        Arc::clone(&self.flight_recorder)
    }

    /// Reads the live hardware expert/bench write-unlock flag (defaults to `false`).
    pub fn is_experimental_write_unlocked(&self) -> bool {
        self.allow_experimental_write.load(Ordering::Acquire)
    }

    /// Updates the live hardware expert/bench write-unlock flag.
    pub fn set_experimental_write_unlocked(&self, enabled: bool) -> bool {
        self.allow_experimental_write
            .store(enabled, Ordering::Release);
        enabled
    }

    /// Runs `f` against the currently attached hardware or test [`Device`].
    pub(crate) fn with_attached_device<T>(
        &self,
        f: impl FnOnce(&mut dyn Device) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut guard = lock_or_err(&self.session, "flash_session")?;
        let bench = guard.bench.as_mut().ok_or("No flash/OBD device attached")?;
        f(&mut *bench.device)
    }

    /// Returns the kind of hardware transport currently attached, if any.
    pub fn selected_transport(&self) -> Option<TransportChoice> {
        self.session.lock().ok().and_then(|s| s.transport)
    }

    /// Injects a caller-provided [`Device`] and [`KernelSession`] (used by integration tests).
    ///
    /// Preserves any pre-configured `safety` and `stock_backup_*` context so safety gate
    /// integration tests can set safety state before attaching a virtual ECU transport.
    pub fn attach_custom_transport(
        &self,
        device: Box<dyn Device + Send>,
        session: Box<dyn KernelSession + Send>,
        choice: TransportChoice,
    ) {
        if let Ok(mut s) = self.session.lock() {
            s.cleanup_count = 0;
            s.recovery_journal = None;
            s.last_flash_audit = None;
            s.bench = Some(AttachedBench {
                device,
                session,
                shared: None,
            });
            s.transport = Some(choice);
        }
    }

    /// Injects a shareable [`SharedDevice`] and [`KernelSession`] for integration tests
    /// that exercise [`configure_flash_session_inner`] protocol switching.
    pub fn attach_shared_transport(
        &self,
        shared: SharedDevice,
        session: Box<dyn KernelSession + Send>,
        choice: TransportChoice,
    ) {
        if let Ok(mut s) = self.session.lock() {
            s.cleanup_count = 0;
            s.recovery_journal = None;
            s.last_flash_audit = None;
            s.selected_seed_key_algo = None;
            s.bench = Some(AttachedBench {
                device: Box::new(shared.clone()),
                session,
                shared: Some(shared),
            });
            s.transport = Some(choice);
        }
    }

    /// Detaches the currently active device, closes its hardware handle, and clears session state.
    pub fn detach(&self) {
        if let Ok(mut s) = self.session.lock() {
            s.reset_hardware_session();
        }
    }

    /// Build a [`FlashTarget`] from the recorded safety context for an image of `image_len`.
    pub fn flash_target(&self, image_len: usize) -> FlashTarget {
        let ctx = self.session.lock().map(|s| s.safety).unwrap_or_default();
        FlashTarget {
            rom_size: ctx.expected_rom_size.unwrap_or(image_len),
            base_address: ctx.base_address,
            is_known_ecu: ctx.is_known_ecu,
            has_stock_backup: ctx.has_stock_backup,
            stock_written_first: ctx.stock_written_first,
            checksum_correction_enabled: ctx.checksum_correction_enabled,
        }
    }

    pub fn set_safety_context(&self, ctx: SafetyContext) {
        if let Ok(mut s) = self.session.lock() {
            s.safety = ctx;
        }
    }

    pub fn mark_stock_backup(&self) {
        if let Ok(mut s) = self.session.lock() {
            s.safety.has_stock_backup = true;
        }
    }

    pub fn mark_stock_written(&self) {
        if let Ok(mut s) = self.session.lock() {
            s.safety.stock_written_first = true;
        }
    }

    pub fn has_device(&self) -> bool {
        self.session
            .lock()
            .map(|s| s.bench.is_some())
            .unwrap_or(false)
    }

    pub fn cleanup_count(&self) -> usize {
        self.session.lock().map(|s| s.cleanup_count).unwrap_or(0)
    }

    pub fn stock_backup_sha256(&self) -> Option<String> {
        self.session
            .lock()
            .ok()
            .and_then(|s| s.stock_backup_sha256.clone())
    }

    pub fn stock_backup_bytes(&self) -> Option<Vec<u8>> {
        self.session
            .lock()
            .ok()
            .and_then(|s| s.stock_backup_bytes.clone())
    }

    pub fn last_dump_audit(&self) -> Option<DumpAuditReport> {
        self.session
            .lock()
            .ok()
            .and_then(|s| s.last_dump_audit.clone())
    }

    pub fn last_flash_audit(&self) -> Option<FlashAuditReport> {
        self.session
            .lock()
            .ok()
            .and_then(|s| s.last_flash_audit.clone())
    }

    pub fn recovery_journal_snapshot(&self) -> Option<RecoveryJournalSnapshot> {
        self.session.lock().ok().and_then(|s| {
            s.recovery_journal.as_ref().map(|j| RecoveryJournalSnapshot {
                target_ecm_id: j.checkpoint.target_ecm_id.clone(),
                target_rom_crc32: j.checkpoint.target_rom_crc32,
                total_rom_size: j.checkpoint.total_rom_size,
                active_sector_index: j.checkpoint.active_sector_index,
                bytes_written: j.checkpoint.bytes_written,
                stage: journal_stage_label(j.checkpoint.stage).to_string(),
            })
        })
    }
}

fn try_open_and_connect(
    device: &mut (dyn Device + Send),
    choice: TransportChoice,
    protocol: ProtocolId,
    baud_rate: u32,
) -> Result<(), String> {
    device
        .open()
        .map_err(|e| format!("could not open {} transport: {e}", choice.as_str()))?;
    if let Err(e) = device.connect(protocol, baud_rate) {
        let _ = device.close();
        return Err(format!("could not connect {} channel: {e}", choice.as_str()));
    }
    Ok(())
}

fn shares_hardware_port_family(a: Option<TransportChoice>, b: TransportChoice) -> bool {
    match a {
        Some(prev) if prev == b => true,
        Some(TransportChoice::Serial | TransportChoice::OpenPort2 | TransportChoice::KLineSerial)
            if matches!(
                b,
                TransportChoice::Serial | TransportChoice::OpenPort2 | TransportChoice::KLineSerial
            ) =>
        {
            true
        }
        _ => false,
    }
}

fn is_missing_driver_or_target(err: &str) -> bool {
    let lower = err.to_ascii_lowercase();
    lower.contains("no such file")
        || lower.contains("cannot open shared object")
        || lower.contains("not found")
        || lower.contains("does not exist")
}

fn attach_real_device(
    state: &FlashEngineState,
    device: Box<dyn Device + Send>,
    choice: TransportChoice,
    protocol: ProtocolId,
    baud_rate: u32,
) -> Result<(), String> {
    let mut device: Box<dyn Device + Send> =
        Box::new(RecordingDevice::new(device, state.flight_recorder()));
    if let Err(first_err) = try_open_and_connect(&mut *device, choice, protocol, baud_rate) {
        let can_retry_after_closing = if is_missing_driver_or_target(&first_err) {
            false
        } else {
            let mut s = lock_or_err(&state.session, "flash_session")?;
            let has_conflicting_real = s
                .bench
                .as_ref()
                .is_some_and(|b| !b.device.is_mock())
                && shares_hardware_port_family(s.transport, choice);
            if has_conflicting_real {
                s.reset_hardware_session();
                true
            } else {
                false
            }
        };
        if !can_retry_after_closing {
            return Err(first_err);
        }
        try_open_and_connect(&mut *device, choice, protocol, baud_rate)?;
    }

    let shared = SharedDevice::new(device);
    let session = UdsHardwareKernelSession::new(shared.clone(), 0x40_0000);

    let mut s = match lock_or_err(&state.session, "flash_session") {
        Ok(guard) => guard,
        Err(err) => {
            if let Ok(mut dev) = shared.0.lock() {
                let _ = dev.close();
            }
            return Err(err);
        }
    };
    s.reset_hardware_session();
    s.bench = Some(AttachedBench {
        device: Box::new(shared.clone()),
        session: Box::new(session),
        shared: Some(shared),
    });
    s.transport = Some(choice);
    Ok(())
}

fn parse_kernel_arch(raw: &str) -> Result<KernelArchitecture, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "m68k" | "motorola68k" => Ok(KernelArchitecture::Motorola68k),
        "mpc5xx" | "powerpc_mpc5xx" => Ok(KernelArchitecture::PowerPcMpc5xx),
        "e200z6" | "powerpc_e200z6" => Ok(KernelArchitecture::PowerPce200z6),
        "m32r" | "mitsubishi_m32r" => Ok(KernelArchitecture::MitsubishiM32R),
        "tricore" | "infineon_tricore" => Ok(KernelArchitecture::InfineonTriCore),
        "superh" | "renesas_superh" | "sh7058" => Ok(KernelArchitecture::RenesasSuperH),
        other => Err(format!("Unsupported RAM kernel architecture '{other}'")),
    }
}

pub fn configure_flash_session_inner(
    protocol: SessionProtocolChoice,
    image_size: usize,
    flash_state: &FlashEngineState,
    app_state: Option<&AppState>,
) -> Result<(), String> {
    let shindengen_rom = if let Some(app) = app_state {
        if matches!(protocol, SessionProtocolChoice::HondaShindengenKLine { .. }) {
            let guard = lock_or_err(&app.memory, "memory")?;
            guard.as_ref().map(|mem| mem.as_slice().to_vec())
        } else {
            None
        }
    } else {
        None
    };

    let mut guard = lock_or_err(&flash_state.session, "flash_session")?;
    let bench = guard.bench.as_mut().ok_or("No flash device attached")?;
    let shared = bench.shared.clone().ok_or_else(|| {
        "Session protocol switching requires a hardware transport attached via attach_*".to_string()
    })?;
    let sz = image_size.max(1024);

    let mut selected_seed_key_algo = None;
    let new_session: Box<dyn KernelSession + Send> = match protocol {
        SessionProtocolChoice::UdsStandard {
            tx_can_id,
            seed_key_algo_id,
            seed_key_param,
            security_level,
        } => {
            let mut s = UdsHardwareKernelSession::new(shared, sz);
            if let Some(id) = tx_can_id {
                s = s.with_tx_can_id(id);
            }
            if let Some(lvl) = security_level {
                s = s.with_security_level(lvl);
            }
            if let Some(algo) = seed_key_algo_id {
                if !bench.device.is_mock() && !is_seed_key_verified_for_live(&algo) {
                    return Err(format!(
                        "Seed-key algorithm '{algo}' is marked simulated/unverified and is blocked on live hardware."
                    ));
                }
                selected_seed_key_algo = Some(algo.clone());
                s = s.with_seed_key(algo, seed_key_param.unwrap_or(0));
            }
            Box::new(s)
        }
        SessionProtocolChoice::HondaKeihinKLine { fast_baud } => Box::new(
            HondaKLineKernelSession::new(
                shared,
                HondaEcuVendor::Keihin,
                HondaEcuType::KeihinM32R,
                sz,
            )
            .with_fast_baud(fast_baud.unwrap_or(true)),
        ),
        SessionProtocolChoice::HondaShindengenKLine { fast_baud } => {
            let mut s = HondaKLineKernelSession::new(
                shared,
                HondaEcuVendor::Shindengen,
                HondaEcuType::ShindengenRH850,
                sz,
            )
            .with_fast_baud(fast_baud.unwrap_or(true));
            if let Some(rom) = shindengen_rom {
                s = s.with_shindengen_key_rom(rom);
            }
            Box::new(s)
        }
        SessionProtocolChoice::RamKernel {
            arch,
            ram_load_address,
            kernel_payload,
            tx_can_id,
        } => {
            let parsed_arch = parse_kernel_arch(&arch)?;
            let mut s = RamKernelHardwareSession::new(
                shared,
                parsed_arch,
                ram_load_address,
                kernel_payload,
                sz,
            );
            if let Some(id) = tx_can_id {
                s = s.with_tx_can_id(id);
            }
            Box::new(s)
        }
    };

    bench.session = new_session;
    guard.selected_seed_key_algo = selected_seed_key_algo;
    Ok(())
}

/// Attach a real SAE J2534 pass-thru driver (`op20pt32`, `smj2534`, etc.).
///
/// Automatically routes 32-bit x86 J2534 DLLs through the companion 32-bit
/// out-of-process bridge when running inside a 64-bit Windows build.
pub fn attach_j2534_inner(path: &str, state: &FlashEngineState) -> Result<(), String> {
    if let Ok(info) = read_library_binary_info(path) {
        if let Some(bridge_exe) = find_j2534_bridge_32_path() {
            if can_load_via_32bit_bridge(
                &info,
                std::env::consts::OS,
                std::env::consts::ARCH,
                true,
            ) {
                return attach_real_device(
                    state,
                    Box::new(BridgedJ2534Device::new(bridge_exe, path)),
                    TransportChoice::J2534,
                    ProtocolId::Iso15765,
                    500_000,
                );
            }
        }
    }
    #[cfg(feature = "j2534-ffi")]
    {
        use kivi_flashing::j2534_device::J2534Device;
        attach_real_device(
            state,
            Box::new(J2534Device::unloaded(path)),
            TransportChoice::J2534,
            ProtocolId::Iso15765,
            500_000,
        )
    }
    #[cfg(not(feature = "j2534-ffi"))]
    {
        let _ = (path, state);
        Err("this build has no J2534 support; rebuild with --features j2534-ffi".to_string())
    }
}

/// Attach a native Tactrix OpenPort 2.0 USB-CDC serial device (`/dev/ttyACM*` or `COMx`).
pub fn attach_openport2_inner(port: &str, state: &FlashEngineState) -> Result<(), String> {
    attach_real_device(
        state,
        Box::new(LiveOpenPort2Device::new(port)),
        TransportChoice::OpenPort2,
        ProtocolId::Iso15765,
        500_000,
    )
}

/// Attach an ELM327 / STN1110 / OBDLink serial or Bluetooth SPP adapter (auto-probing common baud rates).
pub fn attach_serial_inner(port: &str, state: &FlashEngineState) -> Result<(), String> {
    attach_serial_configured_inner(port, None, Some(ElmBusProfile::Auto), state).map(|_| ())
}

/// Attach an ELM327 / STN1110 serial adapter at a custom baud rate.
pub fn attach_serial_with_baud_inner(
    port: &str,
    baud_rate: u32,
    state: &FlashEngineState,
) -> Result<(), String> {
    attach_serial_configured_inner(port, Some(baud_rate), None, state).map(|_| ())
}

/// Attach an ELM327 USB / Bluetooth SPP adapter with optional baud auto-probe and vehicle bus profile.
pub fn attach_serial_configured_inner(
    port: &str,
    baud_rate: Option<u32>,
    bus_profile: Option<ElmBusProfile>,
    state: &FlashEngineState,
) -> Result<u32, String> {
    let profile = bus_profile.unwrap_or(ElmBusProfile::Auto);
    let target_proto = profile.protocol_id();
    let target_bus_baud = profile.bus_baud_rate();

    if let Some(baud) = baud_rate.filter(|&b| b > 0) {
        let mut dev = SerialElmDevice::new(port, baud);
        if let Some(p) = bus_profile {
            dev = dev.with_bus_profile(p);
        }
        attach_real_device(
            state,
            Box::new(dev),
            TransportChoice::Serial,
            target_proto,
            target_bus_baud,
        )?;
        return Ok(baud);
    }

    let mut last_err = String::from("ELM327 serial adapter did not respond on any baud rate");
    for &baud in ELM_AUTO_BAUD_CANDIDATES {
        let dev = SerialElmDevice::new(port, baud).with_bus_profile(profile);
        match attach_real_device(
            state,
            Box::new(dev),
            TransportChoice::Serial,
            target_proto,
            target_bus_baud,
        ) {
            Ok(()) => return Ok(baud),
            Err(e) => {
                if is_missing_driver_or_target(&e)
                    || e.to_ascii_lowercase().contains("permission denied")
                    || e.to_ascii_lowercase().contains("access is denied")
                {
                    return Err(e);
                }
                last_err = e;
            }
        }
    }
    Err(last_err)
}

/// Attach a Wi-Fi TCP ELM327 / STN adapter (`192.168.0.10:35000` default).
pub fn attach_elm_wifi_inner(
    host_port: &str,
    bus_profile: Option<ElmBusProfile>,
    state: &FlashEngineState,
) -> Result<(), String> {
    let profile = bus_profile.unwrap_or(ElmBusProfile::Auto);
    let mut dev = WifiElmDevice::new(host_port);
    dev = dev.with_bus_profile(profile);
    attach_real_device(
        state,
        Box::new(dev),
        TransportChoice::ElmWifi,
        profile.protocol_id(),
        profile.bus_baud_rate(),
    )
}

/// Attach a raw USB-UART K-Line cable (FTDI / CH340 KKL 409.1) for Honda/Yamaha K-Line ECUs.
pub fn attach_kline_serial_inner(
    port: &str,
    baud_rate: Option<u32>,
    state: &FlashEngineState,
) -> Result<(), String> {
    let baud = baud_rate.filter(|&b| b > 0).unwrap_or(DEFAULT_KLINE_BAUD);
    attach_real_device(
        state,
        Box::new(RawKLineDevice::new(port, baud)),
        TransportChoice::KLineSerial,
        ProtocolId::Iso14230,
        baud,
    )
}

fn parse_protocol_name(raw: &str) -> Result<ProtocolId, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "iso15765" | "isotp" | "can_isotp" => Ok(ProtocolId::Iso15765),
        "can" | "raw_can" => Ok(ProtocolId::Can),
        "iso14230" | "kwp2000" | "kwp" => Ok(ProtocolId::Iso14230),
        "iso9141" | "kline" => Ok(ProtocolId::Iso9141),
        "j1850_vpw" | "vpw" => Ok(ProtocolId::J1850Vpw),
        "j1850_pwm" | "pwm" => Ok(ProtocolId::J1850Pwm),
        other => Err(format!("Unsupported transport protocol '{other}'")),
    }
}

/// Reconfigures protocol, bus baud rate, CAN TX/RX arbitration IDs, and K-Line checksum mode on the attached device.
pub fn reconfigure_transport_protocol_inner(
    protocol: &str,
    baud_rate: u32,
    tx_can_id: Option<u32>,
    rx_can_id: Option<u32>,
    kline_no_checksum: Option<bool>,
    state: &FlashEngineState,
) -> Result<(), String> {
    let proto = parse_protocol_name(protocol)?;
    state.with_attached_device(|dev| {
        if let Some(no_cs) = kline_no_checksum {
            dev.set_kline_no_checksum(no_cs);
        }
        let effective_tx = tx_can_id.unwrap_or(0x07E0);
        let effective_rx = rx_can_id.unwrap_or_else(|| derive_elm_rx_can_id(effective_tx));
        if tx_can_id.is_some() || rx_can_id.is_some() {
            dev.configure_can_ids(effective_tx, effective_rx)
                .map_err(|e| e.to_string())?;
        }
        let _ = dev.disconnect();
        dev.connect(proto, baud_rate).map_err(|e| e.to_string())
    })
}

/// Attach an ISO 13400 DoIP gateway over TCP (`host:13400`).
pub fn attach_doip_inner(
    host_port: &str,
    source_addr: u16,
    target_addr: u16,
    state: &FlashEngineState,
) -> Result<(), String> {
    attach_real_device(
        state,
        Box::new(DoIpDevice::new(host_port, source_addr, target_addr)),
        TransportChoice::DoIp,
        ProtocolId::Iso15765,
        500_000,
    )
}

/// Attach a native Linux SocketCAN interface (`can0`, `vcan0`, etc.).
pub fn attach_socketcan_inner(interface: &str, state: &FlashEngineState) -> Result<(), String> {
    attach_real_device(
        state,
        Box::new(SocketCanDevice::new(interface)),
        TransportChoice::SocketCan,
        ProtocolId::Can,
        500_000,
    )
}

/// Detach any currently attached flash transport.
pub fn detach_transport_inner(state: &FlashEngineState) {
    state.detach();
}

/// A J2534 driver found on disk or via OS registry / standard paths.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscoveredDriver {
    pub name: String,
    pub path: String,
    pub score: u32,
    pub loadable: bool,
    pub via_bridge: bool,
    pub arch_label: String,
    pub reason: Option<String>,
}

/// Summary returned by [`auto_connect_flash_transport`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AutoConnectOutcome {
    pub connected: bool,
    pub transport: Option<TransportChoice>,
    pub device_name: Option<String>,
    pub target: Option<String>,
    pub attempts: Vec<String>,
}

fn normalized_driver_path_key(path: &str) -> String {
    #[cfg(windows)]
    {
        path.replace('/', "\\").to_ascii_lowercase()
    }
    #[cfg(not(windows))]
    {
        path.to_string()
    }
}

fn push_unique_search_path(
    out: &mut Vec<std::path::PathBuf>,
    seen: &mut HashSet<String>,
    candidate: std::path::PathBuf,
) {
    let key = normalized_driver_path_key(&candidate.to_string_lossy());
    if seen.insert(key) {
        out.push(candidate);
    }
}

fn driver_search_paths() -> Vec<std::path::PathBuf> {
    let mut paths = Vec::new();
    let mut seen = HashSet::new();

    if let Ok(pf) = std::env::var("ProgramFiles") {
        let base = std::path::PathBuf::from(pf);
        for rel in [
            "J2534",
            r"OpenECU\OpenPort 2.0\drivers\openport 2.0",
            "Scanmatik",
            r"XHorse\MVCI Driver for TOYOTA TIS",
        ] {
            push_unique_search_path(&mut paths, &mut seen, base.join(rel));
        }
    }
    if let Ok(pfx86) = std::env::var("ProgramFiles(x86)") {
        let base = std::path::PathBuf::from(pfx86);
        for rel in [
            "J2534",
            r"OpenECU\OpenPort 2.0\drivers\openport 2.0",
            "Scanmatik",
            r"XHorse\MVCI Driver for TOYOTA TIS",
        ] {
            push_unique_search_path(&mut paths, &mut seen, base.join(rel));
        }
    }
    if let Ok(win) = std::env::var("SystemRoot").or_else(|_| std::env::var("WINDIR")) {
        let base = std::path::PathBuf::from(win);
        push_unique_search_path(&mut paths, &mut seen, base.join("System32"));
        push_unique_search_path(&mut paths, &mut seen, base.join("SysWOW64"));
    }

    for static_path in [
        r"C:\Program Files\J2534",
        r"C:\Program Files (x86)\J2534",
        r"C:\Program Files\OpenECU\OpenPort 2.0\drivers\openport 2.0",
        r"C:\Program Files (x86)\OpenECU\OpenPort 2.0\drivers\openport 2.0",
        r"C:\Program Files\Scanmatik",
        r"C:\Program Files (x86)\Scanmatik",
        r"C:\Program Files\XHorse\MVCI Driver for TOYOTA TIS",
        r"C:\Program Files (x86)\XHorse\MVCI Driver for TOYOTA TIS",
        r"C:\Windows\System32",
        r"C:\Windows\SysWOW64",
        "/usr/local/lib",
        "/usr/lib",
        "/usr/lib64",
        "/usr/lib/x86_64-linux-gnu",
        "/usr/lib/aarch64-linux-gnu",
        "/opt/homebrew/lib",
        "/Library/Application Support/PassThru",
    ] {
        push_unique_search_path(&mut paths, &mut seen, std::path::PathBuf::from(static_path));
    }

    if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        let h = std::path::PathBuf::from(home);
        push_unique_search_path(&mut paths, &mut seen, h.join(".passthru"));
        push_unique_search_path(&mut paths, &mut seen, h.join(".local/lib"));
        push_unique_search_path(
            &mut paths,
            &mut seen,
            h.join("Library/Application Support/PassThru"),
        );
    }
    if let Ok(extra) = std::env::var("KIVI_J2534_DIR") {
        for p in std::env::split_paths(&extra) {
            push_unique_search_path(&mut paths, &mut seen, p);
        }
    }
    paths
}

fn read_library_binary_info(path: &str) -> Result<LibraryBinaryInfo, String> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|_| "File cannot be opened on disk.".to_string())?;
    let mut buf = Vec::with_capacity(4096);
    file.take(65536)
        .read_to_end(&mut buf)
        .map_err(|e| format!("Failed reading library header: {e}"))?;
    Ok(inspect_library_binary(&buf))
}

fn inspect_driver_file(path: &str, bridge_available: bool) -> (bool, bool, String, Option<String>) {
    let info = match read_library_binary_info(path) {
        Ok(i) => i,
        Err(reason) => return (false, false, "Unreadable".to_string(), Some(reason)),
    };
    let direct_ok = is_binary_compatible_with_host(&info);
    let via_bridge = !direct_ok
        && can_load_via_32bit_bridge(
            &info,
            std::env::consts::OS,
            std::env::consts::ARCH,
            bridge_available,
        );
    let loadable = direct_ok || via_bridge;
    let reason = describe_incompatibility_with_bridge(
        &info,
        std::env::consts::OS,
        std::env::consts::ARCH,
        bridge_available,
    );
    (loadable, via_bridge, info.label, reason)
}

/// Discover J2534 shared libraries/DLLs from standard OS paths, registry, and `.passthru` manifests.
pub fn list_j2534_drivers_inner() -> Result<Vec<DiscoveredDriver>, String> {
    let dirs = driver_search_paths();
    let refs: Vec<&std::path::Path> = dirs.iter().map(|p| p.as_path()).collect();
    let reg = DriverRegistry::scan_directories(&refs);
    let bridge_available = find_j2534_bridge_32_path().is_some();

    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for c in reg.ranked() {
        if seen.insert(normalized_driver_path_key(&c.path)) {
            let (loadable, via_bridge, arch_label, reason) =
                inspect_driver_file(&c.path, bridge_available);
            out.push(DiscoveredDriver {
                name: c.name.clone(),
                path: c.path.clone(),
                score: c.score,
                loadable,
                via_bridge,
                arch_label,
                reason,
            });
        }
    }
    for dev in scan_standard_paths() {
        let path_str = dev.library_path.to_string_lossy().into_owned();
        if seen.insert(normalized_driver_path_key(&path_str)) {
            let score = candidate_score(&dev.name).max(80);
            let (loadable, via_bridge, arch_label, reason) =
                inspect_driver_file(&path_str, bridge_available);
            out.push(DiscoveredDriver {
                name: dev.name,
                path: path_str,
                score,
                loadable,
                via_bridge,
                arch_label,
                reason,
            });
        }
    }
    out.sort_by(|a, b| {
        b.loadable
            .cmp(&a.loadable)
            .then_with(|| a.via_bridge.cmp(&b.via_bridge))
            .then_with(|| b.score.cmp(&a.score))
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(out)
}

/// Enumerate active OS serial ports (`ttyUSB*`, `ttyACM*`, `COM*`).
pub fn list_serial_ports_inner() -> Result<Vec<SerialPortRow>, String> {
    Ok(discover_serial_ports().into_iter().map(SerialPortRow::from).collect())
}

/// Automatically detects and attaches the best available ECU hardware interface.
pub fn auto_connect_flash_transport_inner(state: &FlashEngineState) -> AutoConnectOutcome {
    let mut attempts = Vec::new();
    let ports = discover_serial_ports();
    let drivers = list_j2534_drivers_inner().unwrap_or_default();
    let can_ifaces = list_socketcan_interfaces();

    // 1. Confirmed Tactrix OpenPort 2.0 USB-CDC (native driver, needs no 32-bit DLL)
    for p in ports.iter().filter(|p| p.is_openport2) {
        match attach_openport2_inner(&p.port_name, state) {
            Ok(()) => {
                attempts.push(format!("Connected native OpenPort 2.0 on {} ({})", p.port_name, p.description));
                return AutoConnectOutcome {
                    connected: true,
                    transport: Some(TransportChoice::OpenPort2),
                    device_name: Some(format!("OpenPort 2.0 ({})", p.port_name)),
                    target: Some(p.port_name.clone()),
                    attempts,
                };
            }
            Err(e) => attempts.push(format!("OpenPort 2.0 probe on {} failed: {e}", p.port_name)),
        }
    }

    // 2. Native-architecture J2534 shared libraries (.dll x64/x86, Linux .so, macOS .dylib)
    for d in &drivers {
        if !d.loadable {
            attempts.push(format!(
                "Skipped {} [{}]: {}",
                d.name,
                d.arch_label,
                d.reason.as_deref().unwrap_or("incompatible architecture")
            ));
            continue;
        }
        match attach_j2534_inner(&d.path, state) {
            Ok(()) => {
                attempts.push(format!("Connected J2534 driver {} [{}] ({})", d.name, d.arch_label, d.path));
                return AutoConnectOutcome {
                    connected: true,
                    transport: Some(TransportChoice::J2534),
                    device_name: Some(d.name.clone()),
                    target: Some(d.path.clone()),
                    attempts,
                };
            }
            Err(e) => attempts.push(format!("J2534 attach {} failed: {e}", d.name)),
        }
    }

    // 3. Tactrix / cheap J2534 clone USB-CDC bridge candidates (`ati` firmware handshake)
    for p in ports.iter().filter(|p| p.adapter_hint == "tactrix_clone_candidate") {
        match attach_openport2_inner(&p.port_name, state) {
            Ok(()) => {
                attempts.push(format!("Detected OpenPort-compatible clone on {} ({})", p.port_name, p.description));
                return AutoConnectOutcome {
                    connected: true,
                    transport: Some(TransportChoice::OpenPort2),
                    device_name: Some(format!("OpenPort Clone ({})", p.port_name)),
                    target: Some(p.port_name.clone()),
                    attempts,
                };
            }
            Err(e) => attempts.push(format!("USB-CDC probe on {} ({}) skipped: {e}", p.port_name, p.description)),
        }
    }

    // 4. Native Linux SocketCAN interfaces
    for iface in can_ifaces {
        match attach_socketcan_inner(&iface, state) {
            Ok(()) => {
                attempts.push(format!("Connected SocketCAN interface {iface}"));
                return AutoConnectOutcome {
                    connected: true,
                    transport: Some(TransportChoice::SocketCan),
                    device_name: Some(format!("SocketCAN ({iface})")),
                    target: Some(iface),
                    attempts,
                };
            }
            Err(e) => attempts.push(format!("SocketCAN {iface} attach failed: {e}")),
        }
    }

    // 5. Fallback to ELM327 / USB serial / Bluetooth SPP (auto-baud 38400/115200/9600, read-only live data + diag)
    for p in ports.iter().filter(|p| {
        p.adapter_hint == "elm327_candidate"
            || p.adapter_hint == "bluetooth_spp"
            || p.vid.is_some()
    }) {
        match attach_serial_configured_inner(&p.port_name, None, Some(ElmBusProfile::Auto), state) {
            Ok(baud) => {
                attempts.push(format!(
                    "Connected read-only ELM327 adapter on {} @ {baud} bps ({})",
                    p.port_name, p.description
                ));
                return AutoConnectOutcome {
                    connected: true,
                    transport: Some(TransportChoice::Serial),
                    device_name: Some(format!("ELM327 ({} @ {baud} bps)", p.port_name)),
                    target: Some(p.port_name.clone()),
                    attempts,
                };
            }
            Err(e) => attempts.push(format!("Serial ELM327 probe on {} failed: {e}", p.port_name)),
        }
    }

    if attempts.is_empty() {
        attempts.push(
            "No USB OBD/OpenPort serial ports, compatible J2534 libraries (.dll/.so/.dylib), or SocketCAN interfaces detected."
                .to_string(),
        );
    }

    AutoConnectOutcome {
        connected: false,
        transport: None,
        device_name: None,
        target: None,
        attempts,
    }
}

/// Enumerate the currently attached flash device.
pub fn list_flash_devices_inner(state: &FlashEngineState) -> Result<Vec<FlashDeviceDescriptor>, String> {
    let guard = lock_or_err(&state.session, "flash_session")?;
    let Some(bench) = guard.bench.as_ref() else {
        return Ok(Vec::new());
    };
    let info = bench.device.info();
    Ok(vec![FlashDeviceDescriptor {
        id: info.kind.as_str().to_string(),
        name: info.name,
        kind: info.kind.as_str().to_string(),
        transport: info.transport.as_str().to_string(),
        is_mock: bench.device.is_mock(),
    }])
}

/// Read `[address, address + length)` from the ECU while streaming per-block progress.
pub fn dump_ecu_inner_with_progress<F>(
    address: u32,
    length: usize,
    block_size: usize,
    state: &FlashEngineState,
    on_progress: F,
) -> Result<DumpResultPayload, String>
where
    F: FnMut(usize, usize, usize),
{
    let mut s = lock_or_err(&state.session, "flash_session")?;
    let outcome = {
        let bench = s.bench.as_mut().ok_or("No flash device attached")?;
        let result = engine_dump_ecu_with_progress(
            &mut *bench.device,
            &mut *bench.session,
            address,
            length,
            block_size,
            on_progress,
        );
        s.cleanup_count = bench.session.cleanup_count();
        result.map_err(|e| e.to_string())?
    };

    let integrity_checked = outcome.bytes.len() >= 1024;
    if integrity_checked {
        validate_dumped_bin_integrity(&outcome.bytes)
            .map_err(|e| format!("Dump integrity check failed: {e}"))?;
    }

    let hashes = compute_hashes(&outcome.bytes);
    let reports = auto_detect_oem_checksums(&outcome.bytes);
    let detected_checksum_families: Vec<String> =
        reports.iter().map(|r| r.algorithm_id.clone()).collect();
    let all_checksums_valid = reports.is_empty() || reports.iter().all(|r| r.all_ok);

    s.stock_backup_bytes = Some(outcome.bytes.clone());
    s.stock_backup_sha256 = Some(hashes.sha256.clone());
    s.last_dump_audit = Some(DumpAuditReport {
        sha256: hashes.sha256,
        integrity_checked,
        detected_checksum_families,
        all_checksums_valid,
    });

    s.safety.has_stock_backup = true;
    s.safety.base_address = outcome.base_address;
    s.safety.expected_rom_size = Some(outcome.bytes.len());

    Ok(DumpResultPayload {
        base_address: outcome.base_address,
        bytes: outcome.bytes,
        blocks_read: outcome.blocks_read,
    })
}

/// Read `[address, address + length)` from the ECU using the attached session.
pub fn dump_ecu_inner(
    address: u32,
    length: usize,
    block_size: usize,
    state: &FlashEngineState,
) -> Result<DumpResultPayload, String> {
    dump_ecu_inner_with_progress(address, length, block_size, state, |_, _, _| {})
}

/// Read `[address, address + length)` via ISO 14229 UDS service `0x23` with per-block progress.
pub fn dump_ecu_uds23_inner_with_progress<F>(
    address: u32,
    length: usize,
    block_size: usize,
    state: &FlashEngineState,
    on_progress: F,
) -> Result<UdsDumpPayload, String>
where
    F: FnMut(usize, usize, usize, usize),
{
    if length == 0 || block_size == 0 {
        return Err("invalid dump options: length and block size must be positive".to_string());
    }
    let mut s = lock_or_err(&state.session, "flash_session")?;
    let bench = s.bench.as_mut().ok_or("No flash device attached")?;

    let opts = UdsDumpOptions::new(address, length, block_size);
    let out = dump_via_uds23_with_progress(&mut *bench.device, &opts, on_progress)
        .map_err(|e| e.to_string())?;
    Ok(UdsDumpPayload {
        base_address: out.base_address,
        bytes: out.bytes,
        blocks_read: out.blocks_read,
        pending_retries: out.pending_retries,
    })
}

/// Read `[address, address + length)` via ISO 14229 UDS service `0x23`.
pub fn dump_ecu_uds23_inner(
    address: u32,
    length: usize,
    block_size: usize,
    state: &FlashEngineState,
) -> Result<UdsDumpPayload, String> {
    dump_ecu_uds23_inner_with_progress(address, length, block_size, state, |_, _, _, _| {})
}

fn read_safe_voltage_mv(device: &mut (dyn Device + Send)) -> Result<Option<u32>, String> {
    match device.read_voltage_millivolts() {
        Ok(mv) => {
            if mv < 12_000 {
                return Err(format!(
                    "Battery voltage too low for safe flashing: {:.2}V (minimum 12.00V)",
                    mv as f64 / 1000.0
                ));
            }
            if mv > 15_000 {
                return Err(format!(
                    "Battery voltage too high for safe flashing: {:.2}V (maximum 15.00V)",
                    mv as f64 / 1000.0
                ));
            }
            Ok(Some(mv))
        }
        Err(DeviceError::Unsupported(_)) => Ok(None),
        Err(DeviceError::NotOpen) if device.is_mock() => Ok(None),
        Err(e) => Err(format!("Battery voltage read failed: {e}")),
    }
}

fn verify_and_prepare_checksums(
    image: &[u8],
    checksum_correction_enabled: bool,
    preferred_families: Option<&[String]>,
) -> Result<(Vec<u8>, Vec<String>, bool, bool), String> {
    let current = image.to_vec();
    let mut reports = auto_detect_oem_checksums(&current);
    if reports.is_empty() {
        return Ok((current, Vec::new(), false, true));
    }

    if reports.len() > 1 && reports.iter().any(|r| !r.all_ok) {
        if let Some(pref) = preferred_families.filter(|p| !p.is_empty()) {
            let filtered: Vec<_> = reports
                .iter()
                .filter(|r| pref.contains(&r.algorithm_id))
                .cloned()
                .collect();
            if !filtered.is_empty() {
                reports = filtered;
            }
        }
        if reports.len() > 1 && reports.iter().any(|r| !r.all_ok) {
            let ids: Vec<&str> = reports.iter().map(|r| r.algorithm_id.as_str()).collect();
            return Err(format!(
                "Ambiguous multi-family OEM checksum match [{}]; refusing blind auto-repair without explicit algorithm selection",
                ids.join(", ")
            ));
        }
    }

    let families: Vec<String> = reports.iter().map(|r| r.algorithm_id.clone()).collect();
    let invalid_ids: Vec<String> = reports
        .iter()
        .filter(|r| !r.all_ok)
        .map(|r| r.algorithm_id.clone())
        .collect();

    if invalid_ids.is_empty() {
        return Ok((current, families, false, true));
    }
    if !checksum_correction_enabled {
        return Err(format!(
            "Pre-flash checksum verification failed for [{}] and checksum correction is disabled",
            invalid_ids.join(", ")
        ));
    }

    let mut prepared = current;
    for id in &invalid_ids {
        let (repaired, after) = repair_oem_by_id(&prepared, id)
            .map_err(|e| format!("Pre-flash checksum repair failed for '{id}': {e}"))?;
        if !after.all_ok {
            return Err(format!(
                "Pre-flash checksum repair verification remained invalid for '{id}'"
            ));
        }
        prepared = repaired;
    }

    for id in &families {
        let recheck = verify_oem_by_id(&prepared, id)
            .map_err(|e| format!("Pre-flash checksum re-verification error for '{id}': {e}"))?;
        if !recheck.all_ok {
            return Err(format!(
                "Pre-flash checksum re-verification failed for '{id}' after repairing sibling zones"
            ));
        }
    }

    Ok((prepared, families, true, true))
}

pub(crate) fn flash_ecu_prepared_inner_with_progress<F>(
    address: u32,
    image: &[u8],
    block_size: usize,
    state: &FlashEngineState,
    mut on_stage: F,
) -> Result<(FlashResultPayload, Vec<u8>), String>
where
    F: FnMut(PreparedFlashPhase, usize, usize, usize),
{
    if image.is_empty() {
        return Err("the flash image is empty".to_string());
    }
    let mut s = lock_or_err(&state.session, "flash_session")?;
    {
        let bench = s.bench.as_ref().ok_or("No flash device attached")?;
        if !bench.device.supports_flashing() {
            return Err(format!(
                "Attached transport '{}' is read/diagnostic-only (cannot sustain ECU ROM flashing).",
                bench.device.info().name
            ));
        }
        if !bench.device.is_mock() {
            if !state.is_experimental_write_unlocked() {
                return Err(
                    "Live hardware write is locked by default while targets are ExperimentalUntested. Enable 'Bench / Expert Write Unlock' after confirming power supply, checksums, and stock backup."
                        .to_string(),
                );
            }
            if let Some(ref algo) = s.selected_seed_key_algo {
                if !is_seed_key_verified_for_live(algo) {
                    return Err(format!(
                        "Seed-key algorithm '{algo}' is marked simulated/unverified and is blocked on live hardware."
                    ));
                }
            }
        }
    }

    let total = image.len();
    on_stage(
        PreparedFlashPhase::Preparing("Validating flash safety policy & battery voltage..."),
        0,
        total,
        0,
    );

    let policy = FlashSafetyPolicy::default();
    let target = FlashTarget {
        rom_size: s.safety.expected_rom_size.unwrap_or(total),
        base_address: s.safety.base_address,
        is_known_ecu: s.safety.is_known_ecu,
        has_stock_backup: s.safety.has_stock_backup,
        stock_written_first: s.safety.stock_written_first,
        checksum_correction_enabled: s.safety.checksum_correction_enabled,
    };
    check_flash_safety(&target, image, &policy).map_err(|e| e.to_string())?;

    if total >= 1024 {
        validate_dumped_bin_integrity(image)
            .map_err(|e| format!("Refusing to flash corrupted/blank ROM image: {e}"))?;
    }

    let voltage_mv = {
        let bench = s.bench.as_mut().ok_or("No flash device attached")?;
        read_safe_voltage_mv(&mut *bench.device)?
    };

    let preferred_families: Option<Vec<String>> = s
        .last_dump_audit
        .as_ref()
        .filter(|a| a.all_checksums_valid && !a.detected_checksum_families.is_empty())
        .map(|a| a.detected_checksum_families.clone());
    let pre_repair_sha256 = compute_hashes(image).sha256;
    on_stage(
        PreparedFlashPhase::Preparing("Verifying OEM checksums prior to flash..."),
        0,
        total,
        0,
    );
    let (prepared, checksum_families, checksum_auto_repaired, checksum_verified_ok) =
        verify_and_prepare_checksums(
            image,
            target.checksum_correction_enabled,
            preferred_families.as_deref(),
        )?;
    let flashed_sha256 = compute_hashes(&prepared).sha256;

    {
        let mut mgr = RecoveryJournalManager::new(&format!("ecu@0x{address:06X}"), &prepared);
        let ts = now_epoch_secs();
        mgr.record_sector_erase_start(0, ts);
        s.recovery_journal = Some(mgr);
    }

    let outcome = {
        let bench = s.bench.as_mut().ok_or("No flash device attached")?;
        let plan = FlashPlan::full(address, prepared.len());
        let result = engine_flash_ecu_with_progress(
            &mut *bench.device,
            &mut *bench.session,
            &plan,
            &prepared,
            block_size,
            |stage, done, needed, blocks| {
                on_stage(PreparedFlashPhase::Transfer(stage), done, needed, blocks);
            },
        );
        s.cleanup_count = bench.session.cleanup_count();
        result
    };

    match outcome {
        Ok(res) => {
            let stage_label = if let Some(ref mut mgr) = s.recovery_journal {
                mgr.record_block_written(res.bytes_written, now_epoch_secs());
                mgr.checkpoint.stage = JournalStage::VerifyingChecksum;
                mgr.mark_completed();
                journal_stage_label(mgr.checkpoint.stage).to_string()
            } else {
                "FlashCompleted".to_string()
            };
            s.last_flash_audit = Some(FlashAuditReport {
                voltage_mv,
                checksum_families,
                checksum_auto_repaired,
                checksum_verified_ok,
                pre_repair_sha256,
                flashed_sha256,
                journal_stage: stage_label,
            });
            Ok((
                FlashResultPayload {
                    verified: res.verified,
                    bytes_written: res.bytes_written,
                    blocks_written: res.blocks_written,
                },
                prepared,
            ))
        }
        Err(e) => {
            let stage_label = if let Some(ref mut mgr) = s.recovery_journal {
                mgr.checkpoint.stage = JournalStage::InterruptedAbnormally;
                journal_stage_label(mgr.checkpoint.stage).to_string()
            } else {
                "InterruptedAbnormally".to_string()
            };
            s.last_flash_audit = Some(FlashAuditReport {
                voltage_mv,
                checksum_families,
                checksum_auto_repaired,
                checksum_verified_ok,
                pre_repair_sha256,
                flashed_sha256,
                journal_stage: stage_label,
            });
            Err(e.to_string())
        }
    }
}

fn flash_ecu_prepared_inner(
    address: u32,
    image: &[u8],
    block_size: usize,
    state: &FlashEngineState,
) -> Result<(FlashResultPayload, Vec<u8>), String> {
    flash_ecu_prepared_inner_with_progress(address, image, block_size, state, |_, _, _, _| {})
}

/// Write `image` to the ECU starting at `address`, enforcing [`FlashSafetyPolicy::default`].
pub fn flash_ecu_inner(
    address: u32,
    image: &[u8],
    block_size: usize,
    state: &FlashEngineState,
) -> Result<FlashResultPayload, String> {
    let (outcome, _) = flash_ecu_prepared_inner(address, image, block_size, state)?;
    Ok(outcome)
}

#[tauri::command]
pub fn list_flash_devices(state: State<'_, FlashEngineState>) -> Result<Vec<FlashDeviceDescriptor>, String> {
    list_flash_devices_inner(&state)
}

#[tauri::command]
pub fn list_j2534_drivers() -> Result<Vec<DiscoveredDriver>, String> {
    list_j2534_drivers_inner()
}

#[tauri::command]
pub fn list_serial_ports() -> Result<Vec<SerialPortRow>, String> {
    list_serial_ports_inner()
}

#[tauri::command]
pub fn attach_j2534_transport(path: String, state: State<'_, FlashEngineState>) -> Result<(), String> {
    attach_j2534_inner(&path, &state)
}

#[tauri::command]
pub fn attach_openport2_transport(
    port: String,
    state: State<'_, FlashEngineState>,
) -> Result<(), String> {
    attach_openport2_inner(&port, &state)
}

#[tauri::command]
pub fn attach_serial_transport(
    port: String,
    baud_rate: Option<u32>,
    bus_profile: Option<ElmBusProfile>,
    state: State<'_, FlashEngineState>,
) -> Result<u32, String> {
    attach_serial_configured_inner(&port, baud_rate, bus_profile, &state)
}

#[tauri::command]
pub fn attach_elm_wifi_transport(
    host_port: String,
    bus_profile: Option<ElmBusProfile>,
    state: State<'_, FlashEngineState>,
) -> Result<(), String> {
    attach_elm_wifi_inner(&host_port, bus_profile, &state)
}

#[tauri::command]
pub fn attach_kline_serial_transport(
    port: String,
    baud_rate: Option<u32>,
    state: State<'_, FlashEngineState>,
) -> Result<(), String> {
    attach_kline_serial_inner(&port, baud_rate, &state)
}

#[tauri::command]
pub fn reconfigure_transport_protocol(
    protocol: String,
    baud_rate: u32,
    tx_can_id: Option<u32>,
    rx_can_id: Option<u32>,
    kline_no_checksum: Option<bool>,
    state: State<'_, FlashEngineState>,
) -> Result<(), String> {
    reconfigure_transport_protocol_inner(
        &protocol,
        baud_rate,
        tx_can_id,
        rx_can_id,
        kline_no_checksum,
        &state,
    )
}

#[tauri::command]
pub fn attach_doip_transport(
    host_port: String,
    source_addr: Option<u16>,
    target_addr: Option<u16>,
    state: State<'_, FlashEngineState>,
) -> Result<(), String> {
    attach_doip_inner(
        &host_port,
        source_addr.unwrap_or(0x0E80),
        target_addr.unwrap_or(0x1010),
        &state,
    )
}

#[tauri::command]
pub fn attach_socketcan_transport(
    interface: String,
    state: State<'_, FlashEngineState>,
) -> Result<(), String> {
    attach_socketcan_inner(&interface, &state)
}

#[tauri::command]
pub fn detach_transport(state: State<'_, FlashEngineState>) -> Result<(), String> {
    detach_transport_inner(&state);
    Ok(())
}

#[tauri::command]
pub fn selected_transport(state: State<'_, FlashEngineState>) -> Option<TransportChoice> {
    state.selected_transport()
}

#[tauri::command]
pub fn auto_connect_flash_transport(
    state: State<'_, FlashEngineState>,
) -> Result<AutoConnectOutcome, String> {
    Ok(auto_connect_flash_transport_inner(&state))
}

#[tauri::command]
pub fn dump_ecu_uds23(
    address: u32,
    length: usize,
    block_size: usize,
    state: State<'_, FlashEngineState>,
) -> Result<UdsDumpPayload, String> {
    dump_ecu_uds23_inner(address, length, block_size, &state)
}

#[tauri::command]
pub fn dump_ecu(
    address: u32,
    length: usize,
    block_size: usize,
    state: State<'_, FlashEngineState>,
) -> Result<DumpResultPayload, String> {
    dump_ecu_inner(address, length, block_size, &state)
}

#[tauri::command]
pub fn flash_ecu(
    address: u32,
    image: Vec<u8>,
    block_size: usize,
    state: State<'_, FlashEngineState>,
) -> Result<FlashResultPayload, String> {
    flash_ecu_inner(address, &image, block_size, &state)
}

/// Confirm ECU identification and stock-write safety prerequisites for the attached device.
pub fn confirm_flash_safety_context_inner(
    is_known_ecu: bool,
    stock_written_first: bool,
    expected_rom_size: Option<usize>,
    checksum_correction_enabled: Option<bool>,
    state: &FlashEngineState,
) -> Result<(), String> {
    let mut s = lock_or_err(&state.session, "flash_session")?;
    if s.bench.is_none() {
        return Err("No flash device attached".to_string());
    }
    s.safety.is_known_ecu = is_known_ecu;
    s.safety.stock_written_first = stock_written_first;
    if let Some(sz) = expected_rom_size {
        s.safety.expected_rom_size = Some(sz);
    }
    if let Some(cc) = checksum_correction_enabled {
        s.safety.checksum_correction_enabled = cc;
    }
    Ok(())
}

#[tauri::command]
pub fn confirm_flash_safety_context(
    is_known_ecu: bool,
    stock_written_first: bool,
    expected_rom_size: Option<usize>,
    checksum_correction_enabled: Option<bool>,
    state: State<'_, FlashEngineState>,
) -> Result<(), String> {
    confirm_flash_safety_context_inner(
        is_known_ecu,
        stock_written_first,
        expected_rom_size,
        checksum_correction_enabled,
        &state,
    )
}

#[tauri::command]
pub fn configure_flash_session(
    protocol: SessionProtocolChoice,
    image_size: Option<usize>,
    flash_state: State<'_, FlashEngineState>,
    app_state: State<'_, AppState>,
) -> Result<(), String> {
    configure_flash_session_inner(
        protocol,
        image_size.unwrap_or(0x40_0000),
        &flash_state,
        Some(&app_state),
    )
}

pub fn list_hardware_verification_matrix_inner() -> Vec<HardwareTargetCapability> {
    hardware_verification_matrix()
}

#[tauri::command]
pub fn list_hardware_verification_matrix() -> Vec<HardwareTargetCapability> {
    list_hardware_verification_matrix_inner()
}

pub fn get_experimental_write_unlock_inner(state: &FlashEngineState) -> bool {
    state.is_experimental_write_unlocked()
}

#[tauri::command]
pub fn get_experimental_write_unlock(state: State<'_, FlashEngineState>) -> bool {
    get_experimental_write_unlock_inner(&state)
}

pub fn set_experimental_write_unlock_inner(enabled: bool, state: &FlashEngineState) -> bool {
    state.set_experimental_write_unlocked(enabled)
}

#[tauri::command]
pub fn set_experimental_write_unlock(enabled: bool, state: State<'_, FlashEngineState>) -> bool {
    set_experimental_write_unlock_inner(enabled, &state)
}

pub fn get_bus_flight_log_inner(limit: Option<usize>, state: &FlashEngineState) -> Vec<BusFrameRecord> {
    state.flight_recorder().snapshot(limit)
}

#[tauri::command]
pub fn get_bus_flight_log(limit: Option<usize>, state: State<'_, FlashEngineState>) -> Vec<BusFrameRecord> {
    get_bus_flight_log_inner(limit, &state)
}

pub fn export_bus_flight_log_asc_inner(state: &FlashEngineState) -> String {
    state.flight_recorder().export_can_asc()
}

#[tauri::command]
pub fn export_bus_flight_log_asc(state: State<'_, FlashEngineState>) -> String {
    export_bus_flight_log_asc_inner(&state)
}

pub fn export_bus_flight_log_jsonl_inner(state: &FlashEngineState) -> String {
    state.flight_recorder().export_jsonl()
}

#[tauri::command]
pub fn export_bus_flight_log_jsonl(state: State<'_, FlashEngineState>) -> String {
    export_bus_flight_log_jsonl_inner(&state)
}

pub fn clear_bus_flight_log_inner(state: &FlashEngineState) -> usize {
    state.flight_recorder().clear()
}

#[tauri::command]
pub fn clear_bus_flight_log(state: State<'_, FlashEngineState>) -> usize {
    clear_bus_flight_log_inner(&state)
}
