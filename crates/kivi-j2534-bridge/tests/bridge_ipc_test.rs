//! Spawns the actual compiled `kivi-j2534-bridge` executable in `--self-test-loopback`
//! mode and drives it through [`BridgedJ2534Device`] over real OS stdin/stdout pipes.

use kivi_flashing::device::{Device, DeviceError, DeviceKind};
use kivi_flashing::j2534_bridge::BridgedJ2534Device;
use kivi_flashing::j2534_types::{PassThruMsg, ProtocolId};
use std::path::PathBuf;

#[test]
fn real_child_process_bridge_handles_full_lifecycle_and_errors() {
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_kivi-j2534-bridge"));
    let mut dev = BridgedJ2534Device::new(bin, r"C:\Windows\SysWOW64\op20pt32.dll")
        .with_bridge_args(vec!["--self-test-loopback".to_string()]);

    assert!(!dev.is_open());
    assert!(!dev.is_mock());
    assert!(dev.uses_can_id_prefix());

    // Error before open stays typed
    assert_eq!(dev.connect(ProtocolId::Iso15765, 500_000), Err(DeviceError::NotOpen));

    dev.open().expect("open bridged loopback");
    assert!(dev.is_open());
    assert_eq!(dev.info().kind, DeviceKind::J2534);
    assert_eq!(dev.info().firmware_version.as_deref(), Some("FW 2.1"));
    assert_eq!(dev.read_voltage_millivolts().expect("vbatt"), 12_800);

    dev.connect(ProtocolId::Iso15765, 500_000)
        .expect("connect");
    assert!(dev.is_connected());

    let tx = PassThruMsg::new(ProtocolId::Iso15765, vec![0x00, 0x00, 0x07, 0xE0, 0x3E, 0x00]);
    dev.send(&tx).expect("send");
    let rx = dev.receive().expect("receive");
    assert_eq!(rx.data, tx.data);
    assert_eq!(rx.protocol_id, ProtocolId::Iso15765);

    // Queue empty -> Timeout preserved cleanly
    assert_eq!(dev.receive(), Err(DeviceError::Timeout));

    dev.disconnect().expect("disconnect");
    assert!(!dev.is_connected());
    assert!(dev.is_open());

    dev.close().expect("close");
    assert!(!dev.is_open());

    // Re-opening after close spawns a fresh child cleanly
    dev.open().expect("re-open");
    assert_eq!(dev.read_voltage_millivolts().expect("vbatt 2"), 12_800);
    // Deliberately drop without close() to verify zombie-free Drop cleanup
}
