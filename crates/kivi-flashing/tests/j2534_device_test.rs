//! Tests for the J2534 -> Device adapter.
//!
//! The adapter is the last join: it turns the raw FFI entry points into the
//! [`kivi_flashing::device::Device`] trait the flash engine already drives.
//! Without it, the FFI layer and the orchestrator cannot meet.
//!
//! Gated on `j2534-ffi` because it wraps the FFI handle.

#![cfg(feature = "j2534-ffi")]

use kivi_flashing::device::{Device, DeviceKind, DeviceError, TransportKind};
use kivi_flashing::j2534_types::{J2534Error, PassThruMsg, ProtocolId};

#[test]
fn the_adapter_reports_j2534_as_its_kind() {
    // Constructing an adapter without loading a driver must be possible so the
    // device picker can describe it before it is opened.
    let info = kivi_flashing::j2534_device::J2534Device::describe("/path/to/driver.dll");
    assert_eq!(info.kind, DeviceKind::J2534);
    assert_eq!(info.transport, TransportKind::Usb);
}

#[test]
fn a_fresh_adapter_is_closed_and_disconnected() {
    let dev = kivi_flashing::j2534_device::J2534Device::unloaded("/path/to/driver.dll");
    assert!(!dev.is_open());
    assert!(!dev.is_connected());
}

#[test]
fn a_fresh_adapter_is_not_a_mock() {
    let dev = kivi_flashing::j2534_device::J2534Device::unloaded("/path/to/driver.dll");
    assert!(!dev.is_mock(), "a real driver must never claim to be a mock");
}

#[test]
fn sending_before_open_is_refused() {
    let mut dev = kivi_flashing::j2534_device::J2534Device::unloaded("/path/to/driver.dll");
    let msg = PassThruMsg::new(ProtocolId::Iso15765, vec![0x02, 0x10, 0x03]);
    assert_eq!(
        dev.send(&msg).expect_err("send before open"),
        DeviceError::NotOpen
    );
}

#[test]
fn receiving_before_open_is_refused() {
    let mut dev = kivi_flashing::j2534_device::J2534Device::unloaded("/path/to/driver.dll");
    assert_eq!(
        dev.receive().expect_err("receive before open"),
        DeviceError::NotOpen
    );
}

#[test]
fn connecting_before_open_is_refused() {
    let mut dev = kivi_flashing::j2534_device::J2534Device::unloaded("/path/to/driver.dll");
    assert_eq!(
        dev.connect(ProtocolId::Iso15765, 500_000)
            .expect_err("connect before open"),
        DeviceError::NotOpen
    );
}

#[test]
fn opening_a_missing_driver_reports_not_found_or_transport() {
    // The file does not exist, so opening must fail with a named error rather
    // than panicking.
    let mut dev =
        kivi_flashing::j2534_device::J2534Device::unloaded("/definitely/not/a/driver.so");
    match dev.open() {
        Ok(()) => panic!("opening a missing driver must fail"),
        Err(DeviceError::NotFound(_)) | Err(DeviceError::Transport(_)) => {}
        Err(other) => panic!("unexpected error kind: {other:?}"),
    }
    assert!(!dev.is_open(), "a failed open must leave the device closed");
}

#[test]
fn closing_an_unopened_adapter_is_a_no_op() {
    let mut dev = kivi_flashing::j2534_device::J2534Device::unloaded("/path/to/driver.dll");
    dev.close().expect("close must be idempotent");
    assert!(!dev.is_open());
}

#[test]
fn the_adapter_maps_a_driver_error_into_a_device_error() {
    // The mapping itself is the contract; exercise it directly.
    let mapped: DeviceError = kivi_flashing::j2534_device::map_driver_error(J2534Error::Failed);
    assert!(matches!(mapped, DeviceError::Driver(J2534Error::Failed)));
}

#[test]
fn the_adapter_is_usable_as_a_boxed_device() {
    let dev = kivi_flashing::j2534_device::J2534Device::unloaded("/path/to/driver.dll");
    let boxed: Box<dyn Device> = Box::new(dev);
    assert_eq!(boxed.info().kind, DeviceKind::J2534);
}
