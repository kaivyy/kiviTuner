//! Application-facing tests for ECU dump/flash commands using explicit trait injection.

use kivi_flashing::device::{DeviceKind, MockDevice, TransportKind};
use kivi_flashing::kernel_session::{KernelError, MockKernelSession};
use kivituner::flash_commands::{
    dump_ecu_inner, flash_ecu_inner, list_flash_devices_inner, FlashEngineState, SafetyContext,
    TransportChoice,
};

fn attach_test_double(
    state: &FlashEngineState,
    image_size: usize,
    fail_start: bool,
    blocks: &[(u32, Vec<u8>)],
) {
    let dev = MockDevice::new(DeviceKind::Mock, "Mock Bench", TransportKind::InMemory);
    let mut sess = MockKernelSession::new(image_size);
    if fail_start {
        sess.fail_start_with(KernelError::UploadRejected);
    }
    for (addr, data) in blocks {
        sess.set_block_data(*addr, data.clone());
    }
    state.attach_custom_transport(Box::new(dev), Box::new(sess), TransportChoice::J2534);
}

fn enable_valid_safety(state: &FlashEngineState, expected_size: usize) {
    state.set_safety_context(SafetyContext {
        expected_rom_size: Some(expected_size),
        base_address: 0,
        is_known_ecu: true,
        has_stock_backup: true,
        stock_written_first: true,
        checksum_correction_enabled: true,
    });
}

#[test]
fn a_fresh_engine_state_has_no_device_attached() {
    let state = FlashEngineState::default();
    assert!(!state.has_device());
}

#[test]
fn attaching_a_custom_device_makes_it_listable() {
    let state = FlashEngineState::default();
    attach_test_double(&state, 16 * 1024, false, &[]);
    assert!(state.has_device());
    let devices = list_flash_devices_inner(&state).expect("list");
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].name, "Mock Bench");
    assert!(devices[0].is_mock, "injected mock double reports is_mock");
}

#[test]
fn dumping_without_a_device_is_refused() {
    let state = FlashEngineState::default();
    let err = dump_ecu_inner(0x0000, 0x100, 0x100, &state).expect_err("no device");
    assert_eq!(err, "No flash device attached");
}

#[test]
fn a_dump_returns_the_scripted_bytes() {
    let state = FlashEngineState::default();
    attach_test_double(&state, 0x1000, false, &[(0x0000, vec![0xAB; 0x100])]);

    let out = dump_ecu_inner(0x0000, 0x100, 0x100, &state).expect("dump");
    assert_eq!(out.bytes.len(), 0x100);
    assert_eq!(out.bytes, vec![0xAB; 0x100]);
    assert_eq!(out.base_address, 0x0000);
    assert_eq!(out.blocks_read, 1);
}

#[test]
fn a_dump_always_cleans_up_the_kernel() {
    let state = FlashEngineState::default();
    attach_test_double(&state, 0x1000, false, &[(0x0000, vec![0x01; 0x100])]);

    dump_ecu_inner(0x0000, 0x100, 0x100, &state).expect("dump");
    assert_eq!(
        state.cleanup_count(),
        1,
        "the kernel must be shut down after a dump"
    );
}

#[test]
fn a_failed_dump_still_cleans_up() {
    let state = FlashEngineState::default();
    attach_test_double(&state, 0x1000, false, &[]);

    let err = dump_ecu_inner(0x0000, 0x100, 0x100, &state).expect_err("must fail");
    assert!(err.contains("kernel"), "error was: {err}");
    assert_eq!(
        state.cleanup_count(),
        1,
        "cleanup must run on the error path too"
    );
}

#[test]
fn flashing_without_a_device_is_refused() {
    let state = FlashEngineState::default();
    let err = flash_ecu_inner(0x0000, &[0x00; 0x100], 0x100, &state).expect_err("no device");
    assert_eq!(err, "No flash device attached");
}

#[test]
fn a_flash_writes_and_verifies() {
    let state = FlashEngineState::default();
    attach_test_double(&state, 0x1000, false, &[]);
    enable_valid_safety(&state, 0x100);

    let out = flash_ecu_inner(0x0000, &[0x5A; 0x100], 0x100, &state).expect("flash");
    assert!(out.verified);
    assert_eq!(out.bytes_written, 0x100);
}

#[test]
fn a_flash_cleans_up_after_success() {
    let state = FlashEngineState::default();
    attach_test_double(&state, 0x1000, false, &[]);
    enable_valid_safety(&state, 0x100);

    flash_ecu_inner(0x0000, &[0x5A; 0x100], 0x100, &state).expect("flash");
    assert_eq!(state.cleanup_count(), 1);
}

#[test]
fn a_flash_with_an_empty_image_is_refused() {
    let state = FlashEngineState::default();
    attach_test_double(&state, 0x1000, false, &[]);
    let err = flash_ecu_inner(0x0000, &[], 0x100, &state).expect_err("must refuse");
    assert!(err.contains("empty"), "error was: {err}");
}

#[test]
fn a_flash_reports_a_kernel_failure_as_text() {
    let state = FlashEngineState::default();
    attach_test_double(&state, 0x1000, true, &[]);
    enable_valid_safety(&state, 0x100);

    let err = flash_ecu_inner(0x0000, &[0x5A; 0x100], 0x100, &state).expect_err("kernel failed");
    assert!(!err.is_empty());
    assert_eq!(state.cleanup_count(), 1);
}

#[test]
fn discovering_drivers_never_fails_on_a_machine_with_none() {
    let drivers = kivituner::flash_commands::list_j2534_drivers_inner().expect("discovery");
    for d in &drivers {
        assert!(!d.name.is_empty());
        assert!(!d.path.is_empty());
        assert!(d.score > 0, "a discovered driver must have a non-zero score");
    }
}

#[test]
fn a_discovered_windows_dll_is_reported_as_not_loadable_on_linux() {
    let drivers = kivituner::flash_commands::list_j2534_drivers_inner().expect("discovery");
    let dll: Vec<_> = drivers
        .iter()
        .filter(|d| d.name.to_ascii_lowercase().ends_with(".dll"))
        .collect();

    if dll.is_empty() {
        return;
    }

    #[cfg(target_os = "linux")]
    for d in dll {
        assert!(
            !d.loadable,
            "{} is a Windows PE and cannot be loaded on Linux, but was reported loadable",
            d.name
        );
    }
}

#[test]
fn driver_paths_are_absolute() {
    let drivers = kivituner::flash_commands::list_j2534_drivers_inner().expect("discovery");
    for d in drivers {
        assert!(
            std::path::Path::new(&d.path).is_absolute(),
            "driver path must be absolute: {}",
            d.path
        );
    }
}

#[test]
fn a_real_transport_refuses_a_flash_without_a_stock_backup() {
    let state = FlashEngineState::default();
    attach_test_double(&state, 0x1000, false, &[]);
    state.set_safety_context(SafetyContext {
        expected_rom_size: Some(0x100),
        base_address: 0,
        is_known_ecu: true,
        has_stock_backup: false,
        stock_written_first: false,
        checksum_correction_enabled: true,
    });
    let err = flash_ecu_inner(0x0000, &[0xAA; 0x100], 0x100, &state)
        .expect_err("strict safety policy must refuse flash without stock backup");
    assert!(!err.is_empty());
}

#[test]
fn the_safety_gate_refuses_a_wrong_size_image() {
    let state = FlashEngineState::default();
    attach_test_double(&state, 0x1000, false, &[]);
    enable_valid_safety(&state, 0x200);
    let err = flash_ecu_inner(0x0000, &[0xAA; 0x100], 0x100, &state)
        .expect_err("a wrong-size image must be refused");
    assert!(err.contains("bricks the ECU"), "error was: {err}");
}

#[test]
fn marking_a_backup_clears_the_backup_requirement() {
    let state = FlashEngineState::default();
    attach_test_double(&state, 0x1000, false, &[]);
    state.mark_stock_backup();
    state.mark_stock_written();
    let t = state.flash_target(0x100);
    assert!(t.has_stock_backup);
    assert!(t.stock_written_first);
}

#[test]
fn the_safety_context_starts_at_its_most_cautious_values() {
    let state = FlashEngineState::default();
    let t = state.flash_target(0x100);
    assert!(!t.is_known_ecu, "an unknown ECU must be the default");
    assert!(!t.has_stock_backup, "no backup must be the default");
    assert!(!t.stock_written_first, "stock not written must be the default");
    assert!(t.checksum_correction_enabled);
}

#[test]
fn hardware_verification_matrix_and_experimental_unlock_behave_honestly() {
    use kivi_flashing::hardware_matrix::HardwareVerificationTier;
    use kivi_flashing::seed_key_registry::is_seed_key_verified_for_live;
    use kivituner::flash_commands::{
        get_experimental_write_unlock_inner, list_hardware_verification_matrix_inner,
        set_experimental_write_unlock_inner,
    };

    let matrix = list_hardware_verification_matrix_inner();
    assert!(!matrix.is_empty());
    assert!(matrix
        .iter()
        .all(|t| t.tier != HardwareVerificationTier::BenchVerified));

    let state = FlashEngineState::default();
    assert!(!get_experimental_write_unlock_inner(&state));
    assert!(set_experimental_write_unlock_inner(true, &state));
    assert!(get_experimental_write_unlock_inner(&state));
    assert!(!set_experimental_write_unlock_inner(false, &state));

    assert!(is_seed_key_verified_for_live("bosch_me7"));
    assert!(!is_seed_key_verified_for_live("bosch_edc16"));
}

#[test]
fn live_device_safety_gate_locks_writes_until_unlocked_and_blocks_simulated_seed_key() {
    use kivi_flashing::device::{Device, DeviceError, DeviceInfo};
    use kivi_flashing::flight_recorder::RecordingDevice;
    use kivi_flashing::hardware_kernel_session::{SharedDevice, UdsHardwareKernelSession};
    use kivi_flashing::j2534_types::{PassThruMsg, ProtocolId};
    use kivituner::flash_commands::{
        clear_bus_flight_log_inner, configure_flash_session_inner, export_bus_flight_log_asc_inner,
        export_bus_flight_log_jsonl_inner, get_bus_flight_log_inner,
        set_experimental_write_unlock_inner, SessionProtocolChoice,
    };

    struct LiveStubDevice(MockDevice);
    impl Device for LiveStubDevice {
        fn open(&mut self) -> Result<(), DeviceError> {
            self.0.open()
        }
        fn close(&mut self) -> Result<(), DeviceError> {
            self.0.close()
        }
        fn connect(&mut self, p: ProtocolId, b: u32) -> Result<(), DeviceError> {
            self.0.connect(p, b)
        }
        fn disconnect(&mut self) -> Result<(), DeviceError> {
            self.0.disconnect()
        }
        fn send(&mut self, m: &PassThruMsg) -> Result<(), DeviceError> {
            self.0.send(m)
        }
        fn receive(&mut self) -> Result<PassThruMsg, DeviceError> {
            self.0.receive()
        }
        fn read_voltage_millivolts(&mut self) -> Result<u32, DeviceError> {
            self.0.read_voltage_millivolts()
        }
        fn info(&self) -> DeviceInfo {
            self.0.info()
        }
        fn is_open(&self) -> bool {
            self.0.is_open()
        }
        fn is_connected(&self) -> bool {
            self.0.is_connected()
        }
        fn connected_protocol(&self) -> Option<ProtocolId> {
            self.0.connected_protocol()
        }
        fn is_mock(&self) -> bool {
            false
        }
    }

    let state = FlashEngineState::default();
    let mock = MockDevice::new(DeviceKind::OpenPort2, "Live Stub", TransportKind::Usb);
    let recording = RecordingDevice::new(Box::new(LiveStubDevice(mock)), state.flight_recorder());
    let mut shared = SharedDevice::new(Box::new(recording));
    let sess = UdsHardwareKernelSession::new(shared.clone(), 0x100);
    state.attach_shared_transport(
        shared.clone(),
        Box::new(sess),
        TransportChoice::OpenPort2,
    );
    enable_valid_safety(&state, 0x100);

    let err_locked = flash_ecu_inner(0, &[0x5A; 0x100], 0x100, &state)
        .expect_err("unbenchmarked live hardware write must be locked by default");
    assert!(err_locked.contains("ExperimentalUntested"));

    let err_unverified_algo = configure_flash_session_inner(
        SessionProtocolChoice::UdsStandard {
            tx_can_id: Some(0x7E0),
            seed_key_algo_id: Some("bosch_edc16".to_string()),
            seed_key_param: Some(0),
            security_level: Some(1),
        },
        0x100,
        &state,
        None,
    )
    .expect_err("simulated bosch_edc16 seed-key must be rejected on live hardware");
    assert!(err_unverified_algo.contains("simulated/unverified"));

    set_experimental_write_unlock_inner(true, &state);
    // RecordingDevice captures open/connect/send/close cleanly through SharedDevice
    shared.open().expect("open");
    shared
        .connect(ProtocolId::Iso15765, 500_000)
        .expect("connect");
    shared
        .send(&PassThruMsg::new(
            ProtocolId::Iso15765,
            vec![0x00, 0x00, 0x07, 0xE0, 0x10, 0x01],
        ))
        .expect("send");

    let frames = get_bus_flight_log_inner(None, &state);
    assert!(frames.len() >= 3);
    assert!(export_bus_flight_log_asc_inner(&state).contains("7E0"));
    assert!(export_bus_flight_log_jsonl_inner(&state).lines().count() >= 3);
    assert!(clear_bus_flight_log_inner(&state) >= 3);
    assert!(get_bus_flight_log_inner(None, &state).is_empty());
}
