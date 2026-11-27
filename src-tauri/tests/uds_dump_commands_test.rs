//! Tests for the UDS 0x23 dump command via explicit device injection.

use kivi_flashing::device::{Device, DeviceKind, MockDevice, ScriptedReply, TransportKind};
use kivi_flashing::j2534_types::{PassThruMsg, ProtocolId};
use kivi_flashing::kernel_session::MockKernelSession;
use kivituner::flash_commands::{dump_ecu_uds23_inner, FlashEngineState, TransportChoice};

fn attach_scripted_uds_device(state: &FlashEngineState, replies: Vec<Vec<u8>>) {
    let mut dev = MockDevice::new(DeviceKind::Mock, "Scripted Bench", TransportKind::InMemory);
    let _ = dev.open();
    let _ = dev.connect(ProtocolId::Iso15765, 500_000);
    for data in replies {
        dev.push_reply(ScriptedReply::Message(PassThruMsg::new(
            ProtocolId::Iso15765,
            data,
        )));
    }
    state.attach_custom_transport(
        Box::new(dev),
        Box::new(MockKernelSession::new(0x1000)),
        TransportChoice::Serial,
    );
}

#[test]
fn dumping_without_a_device_is_refused() {
    let state = FlashEngineState::default();
    let err = dump_ecu_uds23_inner(0x0000, 0x100, 0x100, &state).expect_err("no device");
    assert_eq!(err, "No flash device attached");
}

#[test]
fn a_dump_over_uds23_returns_the_scripted_bytes() {
    let state = FlashEngineState::default();
    attach_scripted_uds_device(&state, vec![vec![0x63, 0xAA, 0xBB, 0xCC, 0xDD]]);

    let out = dump_ecu_uds23_inner(0x0000, 4, 4, &state).expect("dump");
    assert_eq!(out.bytes, vec![0xAA, 0xBB, 0xCC, 0xDD]);
    assert_eq!(out.blocks_read, 1);
    assert_eq!(out.base_address, 0);
}

#[test]
fn a_zero_length_dump_is_refused() {
    let state = FlashEngineState::default();
    attach_scripted_uds_device(&state, vec![]);
    let err = dump_ecu_uds23_inner(0x0000, 0, 0x100, &state).expect_err("invalid");
    assert!(err.contains("invalid"), "error was: {err}");
}

#[test]
fn a_negative_response_is_reported_with_its_code() {
    let state = FlashEngineState::default();
    attach_scripted_uds_device(&state, vec![vec![0x7F, 0x23, 0x31]]);

    let err = dump_ecu_uds23_inner(0x0000, 1, 1, &state).expect_err("nrc");
    assert!(err.contains("0x31"), "error was: {err}");
}

#[test]
fn a_short_block_is_refused_rather_than_truncated() {
    let state = FlashEngineState::default();
    attach_scripted_uds_device(&state, vec![vec![0x63, 0xAA]]);

    let err = dump_ecu_uds23_inner(0x0000, 4, 4, &state).expect_err("short");
    assert!(err.contains("brick"), "error was: {err}");
}
