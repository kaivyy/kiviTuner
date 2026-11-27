//! Tests for the device abstraction.
//!
//! The device layer is the seam that was missing: everything in this crate could
//! build frames, but nothing could carry them. These tests pin the contract a
//! real J2534 / serial / DoIP driver must satisfy, using an in-memory device so
//! no hardware is required.

use kivi_flashing::device::{
    Device, DeviceError, DeviceInfo, DeviceKind, MockDevice, ScriptedReply, TransportKind,
};
use kivi_flashing::j2534_types::{J2534Error, PassThruMsg, ProtocolId};

fn open_mock() -> MockDevice {
    let mut d = MockDevice::new(DeviceKind::J2534, "Mock J2534", TransportKind::Usb);
    d.set_script(vec![]);
    d.open().expect("open must succeed");
    d
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

#[test]
fn a_device_starts_closed() {
    let d = MockDevice::new(DeviceKind::J2534, "Mock", TransportKind::Usb);
    assert!(!d.is_open());
    assert!(!d.is_connected());
}

#[test]
fn open_transitions_to_open() {
    let d = open_mock();
    assert!(d.is_open());
    assert!(!d.is_connected(), "open alone must not connect a channel");
}

#[test]
fn opening_twice_is_rejected() {
    let mut d = open_mock();
    let err = d.open().expect_err("a second open must be refused");
    assert_eq!(err, DeviceError::AlreadyOpen);
}

#[test]
fn connect_requires_open_first() {
    let mut d = MockDevice::new(DeviceKind::J2534, "Mock", TransportKind::Usb);
    d.set_script(vec![]);
    let err = d
        .connect(ProtocolId::Iso15765, 500_000)
        .expect_err("connect before open must be refused");
    assert_eq!(err, DeviceError::NotOpen);
}

#[test]
fn connect_after_open_succeeds_and_records_the_protocol() {
    let mut d = open_mock();
    d.connect(ProtocolId::Iso15765, 500_000).expect("connect");
    assert!(d.is_connected());
    assert_eq!(d.connected_protocol(), Some(ProtocolId::Iso15765));
}

#[test]
fn disconnecting_returns_to_open_but_not_closed() {
    let mut d = open_mock();
    d.connect(ProtocolId::Iso15765, 500_000).unwrap();
    d.disconnect().expect("disconnect");
    assert!(d.is_open(), "the DLL handle stays loaded");
    assert!(!d.is_connected());
    assert_eq!(d.connected_protocol(), None);
}

#[test]
fn closing_an_open_device_succeeds() {
    let mut d = open_mock();
    d.close().expect("close");
    assert!(!d.is_open());
}

#[test]
fn closing_a_closed_device_is_not_an_error() {
    // Close must be idempotent: it runs on every teardown path, including ones
    // that follow an earlier failure.
    let mut d = MockDevice::new(DeviceKind::J2534, "Mock", TransportKind::Usb);
    d.set_script(vec![]);
    d.close().expect("closing an already-closed device must be a no-op");
}

// ---------------------------------------------------------------------------
// Send / receive
// ---------------------------------------------------------------------------

#[test]
fn sending_requires_a_connected_channel() {
    let mut d = open_mock();
    let msg = PassThruMsg::new(ProtocolId::Iso15765, vec![0x02, 0x10, 0x03]);
    let err = d.send(&msg).expect_err("send without connect must fail");
    assert_eq!(err, DeviceError::NotConnected);
}

#[test]
fn a_sent_message_is_recorded() {
    let mut d = open_mock();
    d.connect(ProtocolId::Iso15765, 500_000).unwrap();
    let msg = PassThruMsg::new(ProtocolId::Iso15765, vec![0x02, 0x10, 0x03]);
    d.send(&msg).expect("send");
    assert_eq!(d.sent_messages(), vec![msg]);
}

#[test]
fn receiving_with_no_scripted_reply_times_out() {
    let mut d = open_mock();
    d.connect(ProtocolId::Iso15765, 500_000).unwrap();
    let err = d.receive().expect_err("an empty script must time out");
    assert_eq!(err, DeviceError::Timeout);
}

#[test]
fn receiving_returns_the_scripted_reply_in_order() {
    let mut d = open_mock();
    d.set_script(vec![
        ScriptedReply::Message(PassThruMsg::new(ProtocolId::Iso15765, vec![0x02, 0x50, 0x03])),
        ScriptedReply::Message(PassThruMsg::new(ProtocolId::Iso15765, vec![0x02, 0x50, 0x02])),
    ]);
    d.connect(ProtocolId::Iso15765, 500_000).unwrap();

    let first = d.receive().expect("first");
    assert_eq!(first.data, vec![0x02, 0x50, 0x03]);
    let second = d.receive().expect("second");
    assert_eq!(second.data, vec![0x02, 0x50, 0x02]);
}

#[test]
fn a_scripted_error_surfaces_as_a_device_error() {
    let mut d = open_mock();
    d.set_script(vec![ScriptedReply::Error(J2534Error::Failed)]);
    d.connect(ProtocolId::Iso15765, 500_000).unwrap();

    let err = d.receive().expect_err("the scripted error must surface");
    assert_eq!(err, DeviceError::Driver(J2534Error::Failed));
}

#[test]
fn the_script_is_consumed_not_replayed() {
    let mut d = open_mock();
    d.set_script(vec![ScriptedReply::Message(PassThruMsg::new(
        ProtocolId::Iso15765,
        vec![0xAA],
    ))]);
    d.connect(ProtocolId::Iso15765, 500_000).unwrap();

    assert!(d.receive().is_ok());
    assert_eq!(
        d.receive().expect_err("script must be exhausted"),
        DeviceError::Timeout
    );
}

// ---------------------------------------------------------------------------
// Metadata
// ---------------------------------------------------------------------------

#[test]
fn device_info_reports_what_was_constructed() {
    let d = MockDevice::new(DeviceKind::J2534, "Scanmatik SM2", TransportKind::Usb);
    let info: DeviceInfo = d.info();
    assert_eq!(info.kind, DeviceKind::J2534);
    assert_eq!(info.name, "Scanmatik SM2");
    assert_eq!(info.transport, TransportKind::Usb);
}

#[test]
fn a_real_device_reports_that_it_is_not_a_mock() {
    let d = MockDevice::new(DeviceKind::J2534, "Mock", TransportKind::Usb);
    assert!(d.is_mock());
}

// ---------------------------------------------------------------------------
// Voltage — the pre-flash interlock input
// ---------------------------------------------------------------------------

#[test]
fn read_voltage_returns_the_scripted_value() {
    let mut d = open_mock();
    d.set_voltage_millivolts(12_600);
    assert_eq!(d.read_voltage_millivolts().unwrap(), 12_600);
}

#[test]
fn read_voltage_requires_open() {
    let mut d = MockDevice::new(DeviceKind::J2534, "Mock", TransportKind::Usb);
    assert_eq!(
        d.read_voltage_millivolts().expect_err("closed device"),
        DeviceError::NotOpen
    );
}

// ---------------------------------------------------------------------------
// Trait object safety
// ---------------------------------------------------------------------------

#[test]
fn the_device_trait_is_object_safe() {
    // The engine must hold `Box<dyn Device>`; if this stops compiling the
    // orchestrator cannot be written generically.
    let d = MockDevice::new(DeviceKind::J2534, "Mock", TransportKind::Usb);
    let boxed: Box<dyn Device> = Box::new(d);
    assert_eq!(boxed.info().kind, DeviceKind::J2534);
}
