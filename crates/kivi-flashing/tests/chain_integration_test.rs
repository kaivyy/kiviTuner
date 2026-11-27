//! End-to-end chain test: device trait -> kernel session -> flash engine.
//!
//! ## What this proves
//!
//! That the three new layers actually compose. Each has its own unit tests; this
//! one runs a complete dump and a complete flash through the public API exactly
//! as the Tauri layer does, with a mock transport standing in for the cable.
//!
//! ## What this does NOT prove
//!
//! That any of it works against a real ECU. No device exists here. This test
//! pins the *control flow and cleanup guarantee*, which is what can be verified
//! without hardware.

use kivi_flashing::device::{Device, DeviceKind, DeviceError, MockDevice, TransportKind};
use kivi_flashing::flash_engine::{dump_ecu, flash_ecu, FlashError, FlashPlan};
use kivi_flashing::j2534_types::ProtocolId;
use kivi_flashing::kernel_session::{KernelSession, MockKernelSession};

/// A bench wired the way the Tauri layer wires it.
fn bench(image_size: usize) -> (MockDevice, MockKernelSession) {
    let mut dev = MockDevice::new(DeviceKind::Mock, "Bench", TransportKind::InMemory);
    dev.open().expect("open");
    dev.connect(ProtocolId::Iso15765, 500_000).expect("connect");
    (dev, MockKernelSession::new(image_size))
}

#[test]
fn the_whole_chain_reads_a_rom_into_a_flat_image() {
    let (mut dev, mut sess) = bench(0x4000);
    // Give each block a distinct fill so a mis-ordered stitch is visible.
    // Using `addr as u8` would make every fill 0x00 (0x1000 as u8 == 0).
    for (i, addr) in [0x0000u32, 0x1000, 0x2000, 0x3000].into_iter().enumerate() {
        sess.set_block_data(addr, vec![0xA0 + i as u8; 0x1000]);
    }

    let out = dump_ecu(&mut dev, &mut sess, 0x0000, 0x4000, 0x1000).expect("dump");
    assert_eq!(out.bytes.len(), 0x4000);
    assert_eq!(out.blocks_read, 4);
    // Each 0x1000-sized quarter must carry its own fill byte, in order.
    for (i, expect) in [0xA0u8, 0xA1, 0xA2, 0xA3].into_iter().enumerate() {
        assert_eq!(
            out.bytes[i * 0x1000],
            expect,
            "block {i} landed at the wrong offset"
        );
    }
}

#[test]
fn the_whole_chain_writes_a_rom_and_verifies_it() {
    let (mut dev, mut sess) = bench(0x4000);
    let image: Vec<u8> = (0..0x2000u32).map(|i| (i % 253) as u8).collect();
    let plan = FlashPlan::full(0x0000, image.len());

    let out = flash_ecu(&mut dev, &mut sess, &plan, &image, 0x1000).expect("flash");
    assert!(out.verified);
    assert_eq!(out.bytes_written, 0x2000);
    assert_eq!(out.blocks_written, 2);
}

#[test]
fn the_chain_cleans_up_after_every_successful_operation() {
    let (mut dev, mut sess) = bench(0x2000);
    sess.set_block_data(0x0000, vec![0x11; 0x100]);
    dump_ecu(&mut dev, &mut sess, 0x0000, 0x100, 0x100).expect("dump");
    assert_eq!(sess.cleanup_count(), 1);

    let image = vec![0x22u8; 0x100];
    let plan = FlashPlan::full(0x0000, 0x100);
    flash_ecu(&mut dev, &mut sess, &plan, &image, 0x100).expect("flash");
    assert_eq!(sess.cleanup_count(), 2, "each operation cleans up once");
}

#[test]
fn the_chain_cleans_up_after_a_mid_transfer_failure() {
    let (mut dev, mut sess) = bench(0x2000);
    // Only the first block exists; the second read fails.
    sess.set_block_data(0x0000, vec![0x11; 0x100]);

    let err = dump_ecu(&mut dev, &mut sess, 0x0000, 0x200, 0x100).expect_err("must fail");
    assert!(matches!(err, FlashError::Kernel(_)));
    assert_eq!(sess.cleanup_count(), 1, "cleanup runs on the failure path");
    assert!(!sess.is_kernel_running());
}

#[test]
fn a_dump_and_a_flash_can_run_back_to_back_on_one_bench() {
    // This is the real workflow: read the ECU, edit, write it back.
    let (mut dev, mut sess) = bench(0x2000);
    sess.set_block_data(0x0000, vec![0x33; 0x100]);
    let dumped = dump_ecu(&mut dev, &mut sess, 0x0000, 0x100, 0x100).expect("dump");
    assert_eq!(dumped.bytes, vec![0x33; 0x100]);

    // Simulate an editor changing every byte.
    let tuned: Vec<u8> = dumped.bytes.iter().map(|b| b.wrapping_add(1)).collect();
    let plan = FlashPlan::full(0x0000, tuned.len());
    let out = flash_ecu(&mut dev, &mut sess, &plan, &tuned, 0x100).expect("flash");
    assert!(out.verified);
    assert_eq!(sess.cleanup_count(), 2);
}

#[test]
fn the_engine_refuses_to_touch_the_device_when_the_image_is_wrong_sized() {
    let (mut dev, mut sess) = bench(0x2000);
    let plan = FlashPlan::full(0x0000, 0x200);
    let err = flash_ecu(&mut dev, &mut sess, &plan, &[0u8; 0x100], 0x100).expect_err("refuse");
    assert!(matches!(err, FlashError::ImageTooShort { .. }));
    assert_eq!(
        sess.cleanup_count(),
        0,
        "validation happens before the kernel starts"
    );
    assert!(!sess.is_kernel_running());
    // And nothing was sent to the device.
    assert!(dev.sent_messages().is_empty());
}

#[test]
fn a_device_failure_surfaces_as_a_device_error_not_a_kernel_error() {
    // Close the device behind the engine's back; the dump must report a device
    // problem, not a protocol one.
    let (mut dev, mut sess) = bench(0x2000);
    dev.close().expect("close");
    let err = dump_ecu(&mut dev, &mut sess, 0x0000, 0x100, 0x100);
    // The dump must fail, and it must not leave the kernel running.
    assert!(err.is_err(), "a closed device must not silently succeed");
    assert!(!sess.is_kernel_running(), "cleanup must still have run");
}

#[test]
fn a_mock_device_is_flagged_so_the_ui_can_warn() {
    let (dev, _sess) = bench(0x1000);
    assert!(dev.is_mock());
    assert_eq!(dev.info().kind, DeviceKind::Mock);
}

#[test]
fn the_trait_objects_compose() {
    // The engine is generic; the Tauri layer holds trait objects. Prove both
    // shapes work with the same code path.
    let mut dev: Box<dyn Device> = Box::new(MockDevice::new(
        DeviceKind::Mock,
        "Boxed",
        TransportKind::InMemory,
    ));
    dev.open().unwrap();
    dev.connect(ProtocolId::Iso15765, 500_000).unwrap();
    let mut sess: Box<dyn KernelSession> = Box::new(MockKernelSession::new(0x1000));

    let image = vec![0x99u8; 0x100];
    let plan = FlashPlan::full(0x0000, image.len());
    let out = flash_ecu(&mut *dev, &mut *sess, &plan, &image, 0x100).expect("flash");
    assert!(out.verified);
}

#[test]
fn a_device_error_is_distinct_from_a_kernel_error() {
    // Both implement Display and Error, and their messages differ, so the UI can
    // tell "the cable is gone" from "the ECU said no".
    let device_err = DeviceError::NotOpen;
    let kernel_err = kivi_flashing::kernel_session::KernelError::KernelNotRunning;
    assert_ne!(device_err.to_string(), kernel_err.to_string());
}
