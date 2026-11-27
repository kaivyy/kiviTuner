//! Tests for hardware transport selection in `FlashEngineState`.

use kivi_flashing::device::{DeviceKind, MockDevice, TransportKind};
use kivi_flashing::kernel_session::MockKernelSession;
use kivituner::flash_commands::{
    attach_doip_inner, attach_elm_wifi_inner, attach_j2534_inner, attach_kline_serial_inner,
    attach_openport2_inner, attach_serial_inner, attach_socketcan_inner, describe_transport,
    detach_transport_inner, list_flash_devices_inner, list_serial_ports_inner, FlashEngineState,
    TransportChoice,
};

#[test]
fn a_fresh_state_has_no_transport_selected() {
    let state = FlashEngineState::default();
    assert!(!state.has_device());
    assert_eq!(state.selected_transport(), None);
}

#[test]
fn attaching_custom_transport_records_choice_and_detach_clears_it() {
    let state = FlashEngineState::default();
    let dev = MockDevice::new(DeviceKind::Serial, "Injected Elm", TransportKind::Serial);
    let sess = MockKernelSession::new(0x4000);
    state.attach_custom_transport(Box::new(dev), Box::new(sess), TransportChoice::Serial);
    assert_eq!(state.selected_transport(), Some(TransportChoice::Serial));

    let devices = list_flash_devices_inner(&state).expect("list");
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].kind, "serial");

    detach_transport_inner(&state);
    assert!(!state.has_device());
    assert_eq!(state.selected_transport(), None);
}

#[test]
fn attaching_a_j2534_driver_that_does_not_exist_fails_without_changing_state() {
    let state = FlashEngineState::default();
    let dev = MockDevice::new(DeviceKind::Serial, "Existing Serial", TransportKind::Serial);
    state.attach_custom_transport(
        Box::new(dev),
        Box::new(MockKernelSession::new(0x4000)),
        TransportChoice::Serial,
    );

    let err = attach_j2534_inner("/definitely/not/a/driver.so", &state)
        .expect_err("a missing driver must not attach");
    assert!(!err.is_empty());
    assert_eq!(state.selected_transport(), Some(TransportChoice::Serial));
}

#[test]
fn missing_hardware_ports_and_interfaces_fail_cleanly() {
    let state = FlashEngineState::default();
    assert!(attach_serial_inner("/dev/definitely-not-a-port", &state).is_err());
    assert!(attach_elm_wifi_inner("127.0.0.1:19998", None, &state).is_err());
    assert!(attach_kline_serial_inner("/dev/definitely-not-kline", None, &state).is_err());
    assert!(attach_openport2_inner("/dev/definitely-not-op2", &state).is_err());
    assert!(attach_socketcan_inner("nonexistent_can99", &state).is_err());
    assert!(attach_doip_inner("127.0.0.1:19999", 0x0E80, 0x1010, &state).is_err());
    assert_eq!(state.selected_transport(), None);
    let _ = list_serial_ports_inner().expect("serial port enumeration never panics");
}

#[test]
fn real_transport_descriptors_are_never_reported_as_mock() {
    for (choice, label, kind) in [
        (TransportChoice::J2534, "smj2534.dll", "j2534"),
        (TransportChoice::OpenPort2, "/dev/ttyACM0", "openport2"),
        (TransportChoice::Serial, "/dev/ttyUSB0", "serial"),
        (TransportChoice::ElmWifi, "192.168.0.10:35000", "elm_wifi"),
        (TransportChoice::KLineSerial, "/dev/ttyUSB1", "kline_serial"),
        (TransportChoice::DoIp, "192.168.0.10:13400", "doip"),
        (TransportChoice::SocketCan, "can0", "socketcan"),
    ] {
        let d = describe_transport(choice, label);
        assert!(!d.is_mock);
        assert_eq!(d.kind, kind);
    }
}

#[test]
fn the_transport_choice_is_serialisable_for_the_ui() {
    for choice in [
        TransportChoice::J2534,
        TransportChoice::OpenPort2,
        TransportChoice::Serial,
        TransportChoice::ElmWifi,
        TransportChoice::KLineSerial,
        TransportChoice::DoIp,
        TransportChoice::SocketCan,
    ] {
        let json = serde_json::to_string(&choice).expect("serialise");
        let back: TransportChoice = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(back, choice);
    }
    assert_eq!(TransportChoice::J2534.as_str(), "j2534");
    assert_eq!(TransportChoice::OpenPort2.as_str(), "openport2");
    assert_eq!(TransportChoice::Serial.as_str(), "serial");
    assert_eq!(TransportChoice::ElmWifi.as_str(), "elm_wifi");
    assert_eq!(TransportChoice::KLineSerial.as_str(), "kline_serial");
    assert_eq!(TransportChoice::DoIp.as_str(), "doip");
    assert_eq!(TransportChoice::SocketCan.as_str(), "socketcan");
}
