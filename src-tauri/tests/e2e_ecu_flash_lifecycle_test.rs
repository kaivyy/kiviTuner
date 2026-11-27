//! True end-to-end ECU read/checksum/edit/checksum/write/verify/recovery lifecycle test.
//!
//! Drives production `UdsHardwareKernelSession`, `flash_engine`, `flash_safety`,
//! `RecoveryJournalManager`, `flash_commands`, `kivi_core`, `kivi_defs`, and
//! `kivi_checksum` through a real ISO-14229-1 / ISO-15765 CAN wire-level
//! `VirtualUdsEcuDevice` carrying a real OEM ROM (`38770-MKH-D42.bin`).

use kivi_checksum::oem::registry::{auto_detect_oem_checksums, repair_oem_by_id};
use kivi_core::bin_export::validate_dumped_bin_integrity;
use kivi_core::hash::compute_hashes;
use kivi_core::types::{DType, Endian};
use kivi_defs::model::{MapDef, Scaling};
use kivi_flashing::device::{Device, DeviceError, DeviceInfo, DeviceKind, TransportKind};
use kivi_flashing::hardware_kernel_session::{SharedDevice, UdsHardwareKernelSession};
use kivi_flashing::j2534_types::{PassThruMsg, ProtocolId};
use kivi_flashing::uds::SeedKeyAlgorithm;
use kivituner::commands::{load_bin_data_inner, update_map_matrix_inner, AppState};
use kivituner::flash_commands::{
    dump_ecu_inner, flash_ecu_inner, FlashEngineState, SafetyContext, TransportChoice,
};
use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Arc, Mutex};

const ECU_TX_ID: u32 = 0x07E0;
const ECU_RX_ID: u32 = 0x07E8;

struct VirtualEcuCore {
    flash: Vec<u8>,
    open: bool,
    connected: Option<ProtocolId>,
    voltage_mv: u32,
    session_sub: u8,
    unlocked: bool,
    pending_seed: u32,
    download_addr: Option<usize>,
    download_end: usize,
    expected_block_ctr: u8,
    erase_count: usize,
    saw_alfid_24: bool,
    max_nrc78_burst: usize,
    ecu_reset_count: usize,
    rx_queue: VecDeque<PassThruMsg>,
}

impl VirtualEcuCore {
    fn new(flash: Vec<u8>) -> Self {
        Self {
            flash,
            open: false,
            connected: None,
            voltage_mv: 12_650,
            session_sub: 0x01,
            unlocked: false,
            pending_seed: 0xA1B2_C3D4,
            download_addr: None,
            download_end: 0,
            expected_block_ctr: 1,
            erase_count: 0,
            saw_alfid_24: false,
            max_nrc78_burst: 0,
            ecu_reset_count: 0,
            rx_queue: VecDeque::new(),
        }
    }

    fn push_reply(&mut self, proto: ProtocolId, pdu: Vec<u8>) {
        let mut wire = Vec::with_capacity(4 + pdu.len());
        wire.extend_from_slice(&ECU_RX_ID.to_be_bytes());
        wire.extend_from_slice(&pdu);
        self.rx_queue.push_back(PassThruMsg::new(proto, wire));
    }

    fn push_nrc(&mut self, proto: ProtocolId, sid: u8, nrc: u8) {
        self.push_reply(proto, vec![0x7F, sid, nrc]);
    }

    fn handle_uds(&mut self, proto: ProtocolId, pdu: &[u8]) {
        if pdu.is_empty() {
            return;
        }
        let sid = pdu[0];
        match sid {
            0x10 => {
                // DiagnosticSessionControl
                let sub = *pdu.get(1).unwrap_or(&0);
                self.session_sub = sub;
                if sub != 0x02 {
                    self.unlocked = false;
                }
                self.push_reply(proto, vec![0x50, sub, 0x00, 0x32, 0x01, 0xF4]);
            }
            0x27 => {
                // SecurityAccess
                if self.session_sub != 0x02 {
                    self.push_nrc(proto, sid, 0x7E);
                    return;
                }
                let sub = *pdu.get(1).unwrap_or(&0);
                if sub == 0x01 {
                    let s = self.pending_seed.to_be_bytes();
                    self.push_reply(proto, vec![0x67, 0x01, s[0], s[1], s[2], s[3]]);
                } else if sub == 0x02 && pdu.len() >= 6 {
                    let expected = SeedKeyAlgorithm::default().compute_key(self.pending_seed);
                    let got = u32::from_be_bytes([pdu[2], pdu[3], pdu[4], pdu[5]]);
                    if got == expected {
                        self.unlocked = true;
                        self.push_reply(proto, vec![0x67, 0x02]);
                    } else {
                        self.unlocked = false;
                        self.push_nrc(proto, sid, 0x35);
                    }
                } else {
                    self.push_nrc(proto, sid, 0x13);
                }
            }
            0x23 => {
                // ReadMemoryByAddress (ALFID 0x14 = 1B len, 0x24 = 2B len)
                if pdu.len() < 7 {
                    self.push_nrc(proto, sid, 0x13);
                    return;
                }
                let alfid = pdu[1];
                let addr = u32::from_be_bytes([pdu[2], pdu[3], pdu[4], pdu[5]]) as usize;
                let len = match alfid {
                    0x14 => pdu[6] as usize,
                    0x24 if pdu.len() >= 8 => {
                        self.saw_alfid_24 = true;
                        u16::from_be_bytes([pdu[6], pdu[7]]) as usize
                    }
                    _ => {
                        self.push_nrc(proto, sid, 0x31);
                        return;
                    }
                };
                if addr.checked_add(len).is_none_or(|end| end > self.flash.len()) {
                    self.push_nrc(proto, sid, 0x31);
                    return;
                }
                let mut reply = Vec::with_capacity(1 + len);
                reply.push(0x63);
                reply.extend_from_slice(&self.flash[addr..addr + len]);
                self.push_reply(proto, reply);
            }
            0x31 => {
                // RoutineControl 0x01 0xFF 0x00 EraseMemory
                if !self.unlocked {
                    self.push_nrc(proto, sid, 0x33);
                    return;
                }
                if pdu.len() < 13 || pdu[1..5] != [0x01, 0xFF, 0x00, 0x44] {
                    self.push_nrc(proto, sid, 0x31);
                    return;
                }
                let addr = u32::from_be_bytes([pdu[5], pdu[6], pdu[7], pdu[8]]) as usize;
                let len = u32::from_be_bytes([pdu[9], pdu[10], pdu[11], pdu[12]]) as usize;
                if addr.checked_add(len).is_none_or(|end| end > self.flash.len()) {
                    self.push_nrc(proto, sid, 0x31);
                    return;
                }
                self.erase_count += 1;
                self.flash[addr..addr + len].fill(0xFF);
                // Emit 12 NRC 0x78 ResponsePending frames (exceeds old limit of 8) before ACK.
                self.max_nrc78_burst = self.max_nrc78_burst.max(12);
                for _ in 0..12 {
                    self.push_nrc(proto, sid, 0x78);
                }
                self.push_reply(proto, vec![0x71, 0x01, 0xFF, 0x00]);
            }
            0x34 => {
                // RequestDownload (4-byte maxNumberOfBlockLength with lengthFormatId = 0x40)
                if !self.unlocked {
                    self.push_nrc(proto, sid, 0x33);
                    return;
                }
                if pdu.len() < 11 {
                    self.push_nrc(proto, sid, 0x13);
                    return;
                }
                let addr = u32::from_be_bytes([pdu[3], pdu[4], pdu[5], pdu[6]]) as usize;
                let len = u32::from_be_bytes([pdu[7], pdu[8], pdu[9], pdu[10]]) as usize;
                if addr.checked_add(len).is_none_or(|end| end > self.flash.len()) {
                    self.push_nrc(proto, sid, 0x31);
                    return;
                }
                self.download_addr = Some(addr);
                self.download_end = addr + len;
                self.expected_block_ctr = 1;
                // 4-byte max block length = 0x00000202 (514 bytes -> chunk 512)
                self.push_reply(proto, vec![0x74, 0x40, 0x00, 0x00, 0x02, 0x02]);
            }
            0x36 => {
                // TransferData
                let Some(cursor) = self.download_addr else {
                    self.push_nrc(proto, sid, 0x24);
                    return;
                };
                if pdu.len() < 2 || pdu[1] != self.expected_block_ctr {
                    self.push_nrc(proto, sid, 0x73);
                    return;
                }
                let payload = &pdu[2..];
                if cursor + payload.len() > self.download_end {
                    self.push_nrc(proto, sid, 0x71);
                    return;
                }
                self.flash[cursor..cursor + payload.len()].copy_from_slice(payload);
                self.download_addr = Some(cursor + payload.len());
                let echoed = self.expected_block_ctr;
                self.expected_block_ctr = self.expected_block_ctr.wrapping_add(1);
                self.push_reply(proto, vec![0x76, echoed]);
            }
            0x37 => {
                // RequestTransferExit
                self.download_addr = None;
                self.push_reply(proto, vec![0x77]);
            }
            0x11 => {
                // ECUReset
                self.session_sub = 0x01;
                self.unlocked = false;
                self.ecu_reset_count += 1;
                self.push_reply(proto, vec![0x51, 0x01]);
            }
            _ => self.push_nrc(proto, sid, 0x11),
        }
    }
}

#[derive(Clone)]
struct VirtualUdsEcuDevice(Arc<Mutex<VirtualEcuCore>>);

impl VirtualUdsEcuDevice {
    fn new(core: Arc<Mutex<VirtualEcuCore>>) -> Self {
        Self(core)
    }
}

impl Device for VirtualUdsEcuDevice {
    fn open(&mut self) -> Result<(), DeviceError> {
        let mut g = self.0.lock().unwrap();
        g.open = true;
        Ok(())
    }
    fn close(&mut self) -> Result<(), DeviceError> {
        let mut g = self.0.lock().unwrap();
        g.open = false;
        g.connected = None;
        Ok(())
    }
    fn connect(&mut self, protocol: ProtocolId, _baud_rate: u32) -> Result<(), DeviceError> {
        let mut g = self.0.lock().unwrap();
        if !g.open {
            return Err(DeviceError::NotOpen);
        }
        g.connected = Some(protocol);
        Ok(())
    }
    fn disconnect(&mut self) -> Result<(), DeviceError> {
        self.0.lock().unwrap().connected = None;
        Ok(())
    }
    fn send(&mut self, msg: &PassThruMsg) -> Result<(), DeviceError> {
        let mut g = self.0.lock().unwrap();
        let proto = g.connected.ok_or(DeviceError::NotConnected)?;
        if msg.data.len() < 5 {
            return Err(DeviceError::Transport("short CAN frame".to_string()));
        }
        let can_id = u32::from_be_bytes([msg.data[0], msg.data[1], msg.data[2], msg.data[3]]);
        if can_id != ECU_TX_ID {
            return Err(DeviceError::Transport(format!("unexpected CAN ID 0x{can_id:X}")));
        }
        g.handle_uds(proto, &msg.data[4..]);
        Ok(())
    }
    fn receive(&mut self) -> Result<PassThruMsg, DeviceError> {
        self.0
            .lock()
            .unwrap()
            .rx_queue
            .pop_front()
            .ok_or(DeviceError::Timeout)
    }
    fn read_voltage_millivolts(&mut self) -> Result<u32, DeviceError> {
        let g = self.0.lock().unwrap();
        if !g.open {
            return Err(DeviceError::NotOpen);
        }
        Ok(g.voltage_mv)
    }
    fn info(&self) -> DeviceInfo {
        DeviceInfo {
            kind: DeviceKind::J2534,
            name: "Virtual ISO-14229 Bench ECU".to_string(),
            transport: TransportKind::Usb,
            firmware_version: Some("1.0".to_string()),
            dll_version: Some("1.0".to_string()),
        }
    }
    fn is_open(&self) -> bool {
        self.0.lock().unwrap().open
    }
    fn is_connected(&self) -> bool {
        self.0.lock().unwrap().connected.is_some()
    }
    fn connected_protocol(&self) -> Option<ProtocolId> {
        self.0.lock().unwrap().connected
    }
    fn is_mock(&self) -> bool {
        false
    }
}

fn load_real_keihin_or_fixture() -> (Vec<u8>, bool) {
    let real = Path::new(
        "/root/ecu_master_database/01_MOTORCYCLES/HONDA/BIG_BIKE_CBR650_CB1000_XADV/38770-MKH-D42.bin",
    );
    if real.exists() {
        return (std::fs::read(real).expect("read real MKH ROM"), true);
    }
    let mut synthetic = vec![0xFFu8; 0x80000];
    synthetic[0x100..0x103].copy_from_slice(b"MKH");
    for i in 0..64u8 {
        synthetic[0x20000 + i as usize] = 40u8.saturating_add(i);
    }
    let (fixed, _) = repair_oem_by_id(&synthetic, "honda_keihin_moto").expect("seed keihin");
    (fixed, false)
}

fn attach_virtual_uds(
    flash_state: &FlashEngineState,
    core: &Arc<Mutex<VirtualEcuCore>>,
    algo: &str,
) {
    let rom_len = core.lock().unwrap().flash.len();
    let mut dev = VirtualUdsEcuDevice::new(core.clone());
    dev.open().unwrap();
    dev.connect(ProtocolId::Iso15765, 500_000).unwrap();
    let shared = SharedDevice::new(Box::new(dev));
    let session = UdsHardwareKernelSession::new(shared.clone(), rom_len).with_seed_key(algo, 0);
    flash_state.attach_custom_transport(Box::new(shared), Box::new(session), TransportChoice::J2534);
    flash_state.set_experimental_write_unlocked(true);
}

#[test]
fn e2e_real_rom_sedot_ceksum_edit_ceksum_tulis_verifikasi() {
    let (stock_rom, is_real) = load_real_keihin_or_fixture();
    let stock_hash = compute_hashes(&stock_rom).sha256;
    let rom_len = stock_rom.len();

    let core = Arc::new(Mutex::new(VirtualEcuCore::new(stock_rom.clone())));
    let flash_state = FlashEngineState::default();
    attach_virtual_uds(&flash_state, &core, "bosch_me7");

    // 1. SEDOT (Read ECU via real UDS 0x23 with block_size = 512 -> exercises ALFID 0x24!)
    let dump = dump_ecu_inner(0, rom_len, 512, &flash_state).expect("E2E UDS dump");
    assert_eq!(dump.bytes.len(), rom_len);
    assert_eq!(dump.bytes, stock_rom);
    assert!(core.lock().unwrap().saw_alfid_24, "block_size=512 must send ALFID=0x24");
    assert_eq!(flash_state.cleanup_count(), 1);
    assert_eq!(core.lock().unwrap().ecu_reset_count, 1);
    assert_eq!(flash_state.stock_backup_sha256().as_deref(), Some(stock_hash.as_str()));

    let dump_audit = flash_state.last_dump_audit().expect("dump audit");
    assert!(dump_audit.integrity_checked);
    assert!(dump_audit.detected_checksum_families.iter().any(|f| f == "honda_keihin_moto"));
    assert!(dump_audit.all_checksums_valid);

    // 2. EDIT via kivi-core & kivi-defs with decimal inverse scaling (zero -1 LSB drift!)
    let app_state = AppState::default();
    load_bin_data_inner(&dump.bytes, &app_state).expect("ingest dump");

    let target_off = 0x20000usize;
    let orig_byte = dump.bytes[target_off];
    let new_raw: u8 = orig_byte.wrapping_add(17);
    let physical = (new_raw as f64) * 0.3; // 0.3 scaling exercises IEEE-754 inverse rounding!

    let cell_map = MapDef {
        id: "e2e_cal_cell".into(),
        name: "Calibration Cell".into(),
        category: "Fuel".into(),
        address: target_off,
        rows: 1,
        cols: 1,
        dtype: DType::U8,
        endian: Endian::Big,
        column_major: false,
        major_stride_bits: 0,
        minor_stride_bits: 0,
        scaling: Scaling {
            equation: "X * 0.3".into(),
            unit: "ms".into(),
            decimals: 2,
        },
        x_axis: None,
        y_axis: None,
    };
    update_map_matrix_inner(&cell_map, &[vec![physical]], &app_state).expect("edit calibration");

    let edited_bytes = app_state
        .memory
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .as_slice()
        .to_vec();
    assert_eq!(edited_bytes[target_off], new_raw, "inverse scaling must have 0 LSB error");

    // 3. CEKSUM CHECK AFTER EDIT: checksum must now report STALE before repair!
    let stale_reports = auto_detect_oem_checksums(&edited_bytes);
    let keihin_stale = stale_reports
        .iter()
        .find(|r| r.algorithm_id == "honda_keihin_moto")
        .expect("keihin detect");
    assert!(!keihin_stale.all_ok, "edited ROM checksum must be STALE before pre-flash repair");

    // 4. KEAMANAN: flash must refuse before marking ECU catalogue/stock written first!
    let err_before_consent = flash_ecu_inner(0, &edited_bytes, 512, &flash_state)
        .expect_err("must require known ECU / stock written consent");
    assert!(!err_before_consent.is_empty());
    assert_eq!(core.lock().unwrap().erase_count, 0);

    // Confirm safety flags and trigger TULIS + AUTO-CEKSUM + VERIFIKASI!
    flash_state.set_safety_context(SafetyContext {
        expected_rom_size: Some(rom_len),
        base_address: 0,
        is_known_ecu: true,
        has_stock_backup: true,
        stock_written_first: true,
        checksum_correction_enabled: true,
    });

    let outcome = flash_ecu_inner(0, &edited_bytes, 512, &flash_state).expect("E2E flash");
    assert!(outcome.verified);
    assert_eq!(outcome.bytes_written, rom_len);
    assert_eq!(flash_state.cleanup_count(), 2);

    let g = core.lock().unwrap();
    assert_eq!(g.erase_count, 1);
    assert_eq!(g.max_nrc78_burst, 12);
    assert_eq!(g.ecu_reset_count, 2);

    // 5. POST-FLASH HARDWARE & CRYPTOGRAPHIC VERIFICATION
    let post_reports = auto_detect_oem_checksums(&g.flash);
    assert!(post_reports.iter().any(|r| r.algorithm_id == "honda_keihin_moto" && r.all_ok));

    let flash_audit = flash_state.last_flash_audit().expect("flash audit");
    assert_eq!(flash_audit.voltage_mv, Some(12_650));
    assert!(flash_audit.checksum_auto_repaired);
    assert!(flash_audit.checksum_verified_ok);
    assert_eq!(flash_audit.journal_stage, "FlashCompleted");

    let journal = flash_state.recovery_journal_snapshot().expect("journal");
    assert_eq!(journal.stage, "FlashCompleted");
    assert_eq!(journal.bytes_written, rom_len);

    // Diff between Stock ECU and Flashed ECU must contain ONLY the edited cell + checksum byte!
    let diff_offsets: Vec<usize> = stock_rom
        .iter()
        .zip(g.flash.iter())
        .enumerate()
        .filter_map(|(i, (&a, &b))| (a != b).then_some(i))
        .collect();
    assert_eq!(
        diff_offsets,
        vec![target_off, rom_len - 8],
        "expected ONLY edited byte 0x{target_off:X} and checksum byte 0x{:X} to change",
        rom_len - 8
    );

    eprintln!(
        "[E2E] RealROM={is_real}, Size={rom_len}B, ALFID24={}, NRC78_Burst={}, Voltage={:?}mV, ChecksumRepaired={}, DiffOffsets={diff_offsets:?}",
        g.saw_alfid_24,
        g.max_nrc78_burst,
        flash_audit.voltage_mv,
        flash_audit.checksum_auto_repaired
    );
}

#[test]
fn e2e_safety_gates_block_dangerous_flashes_before_erase() {
    let (stock_rom, _) = load_real_keihin_or_fixture();
    let rom_len = stock_rom.len();
    let mut edited = stock_rom.clone();
    edited[0x20000] ^= 0x55;

    let core = Arc::new(Mutex::new(VirtualEcuCore::new(stock_rom.clone())));
    let flash_state = FlashEngineState::default();
    attach_virtual_uds(&flash_state, &core, "bosch_me7");

    // 1. No stock backup
    flash_state.set_safety_context(SafetyContext {
        expected_rom_size: Some(rom_len),
        base_address: 0,
        is_known_ecu: true,
        has_stock_backup: false,
        stock_written_first: false,
        checksum_correction_enabled: true,
    });
    assert!(flash_ecu_inner(0, &edited, 512, &flash_state).is_err());

    // Enable backup/consent
    flash_state.set_safety_context(SafetyContext {
        expected_rom_size: Some(rom_len),
        base_address: 0,
        is_known_ecu: true,
        has_stock_backup: true,
        stock_written_first: true,
        checksum_correction_enabled: true,
    });

    // 2. Wrong ROM size
    assert!(flash_ecu_inner(0, &edited[..rom_len - 512], 512, &flash_state).is_err());

    // 3. Low battery voltage (11.40V < 12.00V)
    core.lock().unwrap().voltage_mv = 11_400;
    let err_low_v = flash_ecu_inner(0, &edited, 512, &flash_state).unwrap_err();
    assert!(err_low_v.contains("too low"), "unexpected low-V msg: {err_low_v}");

    // 4. High voltage spike (15.50V > 15.00V)
    core.lock().unwrap().voltage_mv = 15_500;
    let err_hi_v = flash_ecu_inner(0, &edited, 512, &flash_state).unwrap_err();
    assert!(err_hi_v.contains("too high"), "unexpected hi-V msg: {err_hi_v}");

    // Restore safe voltage (12.6V)
    core.lock().unwrap().voltage_mv = 12_600;

    // 5. Blank 0xFF image refused before erase
    let blank = vec![0xFFu8; rom_len];
    assert!(flash_ecu_inner(0, &blank, 512, &flash_state).is_err());

    // 6. Stale checksum with correction disabled refused
    flash_state.set_safety_context(SafetyContext {
        expected_rom_size: Some(rom_len),
        base_address: 0,
        is_known_ecu: true,
        has_stock_backup: true,
        stock_written_first: true,
        checksum_correction_enabled: false,
    });
    assert!(flash_ecu_inner(0, &edited, 512, &flash_state).is_err());

    // 7. Wrong Seed/Key algorithm causes NRC 0x35 before erase + cleans up session
    flash_state.set_safety_context(SafetyContext {
        expected_rom_size: Some(rom_len),
        base_address: 0,
        is_known_ecu: true,
        has_stock_backup: true,
        stock_written_first: true,
        checksum_correction_enabled: true,
    });
    attach_virtual_uds(&flash_state, &core, "bosch_edc16"); // mismatches VirtualEcu's bosch_me7
    let err_key = flash_ecu_inner(0, &edited, 512, &flash_state).unwrap_err();
    assert!(!err_key.is_empty());
    assert_eq!(flash_state.cleanup_count(), 1);
    assert_eq!(core.lock().unwrap().erase_count, 0, "ECU flash must NEVER be erased on any safety or key failure");
}

#[test]
fn e2e_dump_integrity_rejects_blank_zero_and_mirror_traps() {
    for (label, bad_image) in [
        ("blank_ff", vec![0xFFu8; 4096]),
        ("locked_zero", vec![0x00u8; 4096]),
        ("mirror_256", {
            let page: Vec<u8> = (0u8..=255).collect();
            page.repeat(16)
        }),
    ] {
        assert!(validate_dumped_bin_integrity(&bad_image).is_err());
        let core = Arc::new(Mutex::new(VirtualEcuCore::new(bad_image)));
        let flash_state = FlashEngineState::default();
        attach_virtual_uds(&flash_state, &core, "bosch_me7");

        let err = dump_ecu_inner(0, 4096, 512, &flash_state)
            .expect_err(&format!("{label} trap must fail integrity check"));
        assert!(err.contains("integrity"), "{label} error was: {err}");
        assert!(flash_state.stock_backup_sha256().is_none());
        assert!(!flash_state.flash_target(4096).has_stock_backup);
    }
}
