//! Tests for dumping a ROM over standard transports using UDS service 0x23.
//!
//! ## Why this path exists
//!
//! ISO 14229 **service 0x23 (`ReadMemoryByAddress`)** enables full ROM readback
//! over standard J2534, ELM327, or DoIP interfaces in an extended diagnostic session
//! without requiring a proprietary RAM kernel.
//!
//! ## Why the driver sends raw UDS
//!
//! J2534 configured for `ISO15765` performs ISO-TP segmentation in the
//! interface, and an ELM327 performs it in the adapter. Sending raw UDS bytes and
//! letting the transport frame them is what makes one driver work on all of
//! them.

use kivi_flashing::device::{Device, DeviceKind, MockDevice, ScriptedReply, TransportKind};
use kivi_flashing::j2534_types::{PassThruMsg, ProtocolId};
use kivi_flashing::uds_dump::{
    build_read_request, dump_via_uds23, parse_read_response, UdsDumpError, UdsDumpOptions,
};

fn bench() -> MockDevice {
    let mut d = MockDevice::new(DeviceKind::Mock, "Bench", TransportKind::InMemory);
    d.open().unwrap();
    d.connect(ProtocolId::Iso15765, 500_000).unwrap();
    d
}

fn reply(data: Vec<u8>) -> ScriptedReply {
    ScriptedReply::Message(PassThruMsg::new(ProtocolId::Iso15765, data))
}

// ---------------------------------------------------------------------------
// Request encoding
// ---------------------------------------------------------------------------

#[test]
fn a_read_request_is_service_23_with_the_documented_format_identifier() {
    // ISO 14229-1: 0x23 | addressAndLengthFormatIdentifier | address | size
    // 0x24 = high nibble 2 (2-byte size), low nibble 4 (4-byte address).
    let req = build_read_request(0x0001_0000, 0x0100);
    assert_eq!(req[0], 0x23);
    assert_eq!(req[1], 0x24);
    assert_eq!(&req[2..6], &[0x00, 0x01, 0x00, 0x00]);
    assert_eq!(&req[6..8], &[0x01, 0x00]);
    assert_eq!(req.len(), 8);
}

#[test]
fn the_request_encodes_a_large_address_big_endian() {
    let req = build_read_request(0xFFFF_FFFF, 0x0001);
    assert_eq!(&req[2..6], &[0xFF, 0xFF, 0xFF, 0xFF]);
}

// ---------------------------------------------------------------------------
// Response parsing
// ---------------------------------------------------------------------------

#[test]
fn a_positive_response_yields_its_data() {
    // 0x63 = 0x23 + 0x40
    let r = parse_read_response(&[0x63, 0xAA, 0xBB, 0xCC]).expect("parse");
    assert_eq!(r, vec![0xAA, 0xBB, 0xCC]);
}

#[test]
fn a_negative_response_yields_an_error() {
    let err = parse_read_response(&[0x7F, 0x23, 0x31]).expect_err("negative");
    assert!(matches!(err, UdsDumpError::NegativeResponse { nrc: 0x31, .. }));
}

#[test]
fn a_pending_response_is_distinguishable_from_a_real_failure() {
    // NRC 0x78 = requestCorrectlyReceived-ResponsePending. It is not a failure;
    // the caller must wait and read again.
    let err = parse_read_response(&[0x7F, 0x23, 0x78]).expect_err("pending");
    assert!(matches!(err, UdsDumpError::ResponsePending));
}

#[test]
fn an_empty_response_is_an_error() {
    assert!(matches!(
        parse_read_response(&[]).unwrap_err(),
        UdsDumpError::EmptyResponse
    ));
}

#[test]
fn a_wrong_service_echo_is_rejected() {
    // A reply to 0x22 must not be mistaken for a reply to 0x23.
    assert!(parse_read_response(&[0x62, 0xF1, 0x90]).is_err());
}

// ---------------------------------------------------------------------------
// Full dump
// ---------------------------------------------------------------------------

#[test]
fn a_single_block_dump_returns_the_data() {
    let mut d = bench();
    d.set_script(vec![reply(vec![0x63, 0x11, 0x22, 0x33, 0x44])]);

    let opts = UdsDumpOptions::new(0x0000_0000, 4, 4);
    let out = dump_via_uds23(&mut d, &opts).expect("dump");
    assert_eq!(out.bytes, vec![0x11, 0x22, 0x33, 0x44]);
    assert_eq!(out.blocks_read, 1);
}

#[test]
fn a_multi_block_dump_concatenates_in_order() {
    let mut d = bench();
    d.set_script(vec![
        reply(vec![0x63, 0x01, 0x02]),
        reply(vec![0x63, 0x03, 0x04]),
        reply(vec![0x63, 0x05, 0x06]),
    ]);

    let opts = UdsDumpOptions::new(0x0000_0000, 6, 2);
    let out = dump_via_uds23(&mut d, &opts).expect("dump");
    assert_eq!(out.bytes, vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06]);
    assert_eq!(out.blocks_read, 3);
}

#[test]
fn the_dump_sends_one_request_per_block() {
    let mut d = bench();
    d.set_script(vec![reply(vec![0x63, 0xAA]), reply(vec![0x63, 0xBB])]);

    let opts = UdsDumpOptions::new(0x1000, 2, 1);
    dump_via_uds23(&mut d, &opts).expect("dump");

    let sent = d.sent_messages();
    assert_eq!(sent.len(), 2);
    // The second request must ask for the second byte.
    assert_eq!(&sent[1].data[2..6], &[0x00, 0x00, 0x10, 0x01]);
}

#[test]
fn a_pending_response_is_retried_then_the_dump_continues() {
    let mut d = bench();
    d.set_script(vec![
        reply(vec![0x7F, 0x23, 0x78]), // pending
        reply(vec![0x7F, 0x23, 0x78]), // second pending
        reply(vec![0x63, 0xAA]),
    ]);

    let opts = UdsDumpOptions::new(0x0000, 1, 1);
    let out = dump_via_uds23(&mut d, &opts).expect("dump");
    assert_eq!(out.bytes, vec![0xAA]);
    assert_eq!(out.pending_retries, 2);
    // ISO 14229-1 §7.5.5: client must wait on NRC 0x78 rather than re-transmitting 0x23.
    assert_eq!(d.sent_messages().len(), 1);
}

#[test]
fn too_many_pending_responses_fail() {
    let mut d = bench();
    d.set_script(vec![
        reply(vec![0x7F, 0x23, 0x78]),
        reply(vec![0x7F, 0x23, 0x78]),
        reply(vec![0x7F, 0x23, 0x78]),
    ]);

    let mut opts = UdsDumpOptions::new(0x0000, 1, 1);
    opts.max_pending_retries = 2;
    let err = dump_via_uds23(&mut d, &opts).expect_err("must give up");
    assert!(matches!(err, UdsDumpError::TooManyPendingRetries { .. }));
}

#[test]
fn a_negative_response_aborts_the_dump() {
    let mut d = bench();
    d.set_script(vec![
        reply(vec![0x63, 0xAA]),
        reply(vec![0x7F, 0x23, 0x31]), // requestOutOfRange
    ]);

    let opts = UdsDumpOptions::new(0x0000, 2, 1);
    let err = dump_via_uds23(&mut d, &opts).expect_err("must abort");
    assert!(matches!(err, UdsDumpError::NegativeResponse { nrc: 0x31, .. }));
}

#[test]
fn a_short_block_is_an_error_not_a_silent_truncation() {
    // If the ECU returns fewer bytes than asked, the image would be short and
    // writing it back would brick the ECU.
    let mut d = bench();
    d.set_script(vec![reply(vec![0x63, 0xAA])]);

    let opts = UdsDumpOptions::new(0x0000, 4, 4);
    let err = dump_via_uds23(&mut d, &opts).expect_err("short block");
    assert!(matches!(err, UdsDumpError::ShortBlock { .. }));
}

#[test]
fn a_device_timeout_is_reported_as_such() {
    let mut d = bench();
    d.set_script(vec![]); // no replies
    let opts = UdsDumpOptions::new(0x0000, 1, 1);
    let err = dump_via_uds23(&mut d, &opts).expect_err("timeout");
    assert!(matches!(err, UdsDumpError::Device(_)));
}

#[test]
fn the_outcome_reports_the_address_it_started_from() {
    let mut d = bench();
    d.set_script(vec![reply(vec![0x63, 0xAA])]);
    let opts = UdsDumpOptions::new(0x8000, 1, 1);
    let out = dump_via_uds23(&mut d, &opts).expect("dump");
    assert_eq!(out.base_address, 0x8000);
}

#[test]
fn a_zero_length_dump_is_refused() {
    let mut d = bench();
    let opts = UdsDumpOptions::new(0x0000, 0, 1);
    assert!(matches!(
        dump_via_uds23(&mut d, &opts).unwrap_err(),
        UdsDumpError::InvalidOptions
    ));
}

#[test]
fn a_zero_block_size_is_refused() {
    let mut d = bench();
    let opts = UdsDumpOptions::new(0x0000, 4, 0);
    assert!(matches!(
        dump_via_uds23(&mut d, &opts).unwrap_err(),
        UdsDumpError::InvalidOptions
    ));
}

#[test]
fn dump_via_uds23_with_progress_reports_each_block() {
    use kivi_flashing::uds_dump::dump_via_uds23_with_progress;

    let mut d = bench();
    d.set_script(vec![
        reply(vec![0x63, 0x01, 0x02]),
        reply(vec![0x63, 0x03, 0x04]),
        reply(vec![0x63, 0x05]),
    ]);
    let opts = UdsDumpOptions::new(0x1000, 5, 2);
    let mut events = Vec::new();
    let out = dump_via_uds23_with_progress(&mut d, &opts, |done, total, blocks, pending| {
        events.push((done, total, blocks, pending));
    })
    .expect("uds dump with progress");
    assert_eq!(out.bytes, vec![1, 2, 3, 4, 5]);
    assert_eq!(events, vec![(2, 5, 1, 0), (4, 5, 2, 0), (5, 5, 3, 0)]);
}

#[test]
fn dump_sets_iso15765_pad_and_29bit_flags_with_can_id_prefix() {
    use kivi_flashing::j2534_types::tx_flags;

    let mut d = MockDevice::new(DeviceKind::J2534, "J2534 Bench", TransportKind::Usb)
        .with_can_id_prefix(true);
    d.open().unwrap();
    d.connect(ProtocolId::Iso15765, 500_000).unwrap();
    d.set_script(vec![reply(vec![0x00, 0x00, 0x07, 0xE8, 0x63, 0xDE, 0xAD])]);

    let opts = UdsDumpOptions::new(0x1000, 2, 2);
    let out = dump_via_uds23(&mut d, &opts).expect("dump 11-bit");
    assert_eq!(out.bytes, vec![0xDE, 0xAD]);
    let sent = d.sent_messages();
    assert_eq!(sent[0].tx_flags, tx_flags::ISO15765_FRAME_PAD);
    assert_eq!(&sent[0].data[0..4], &0x0000_07E0u32.to_be_bytes());

    let mut d29 = MockDevice::new(DeviceKind::J2534, "J2534 Bench 29", TransportKind::Usb)
        .with_can_id_prefix(true);
    d29.open().unwrap();
    d29.connect(ProtocolId::Iso15765, 500_000).unwrap();
    d29.set_script(vec![reply(vec![0x18, 0xDA, 0xF1, 0x10, 0x63, 0xBE, 0xEF])]);

    let opts29 = UdsDumpOptions::new(0x2000, 2, 2).with_tx_can_id(0x18DA_10F1);
    let out29 = dump_via_uds23(&mut d29, &opts29).expect("dump 29-bit");
    assert_eq!(out29.bytes, vec![0xBE, 0xEF]);
    let sent29 = d29.sent_messages();
    assert_eq!(
        sent29[0].tx_flags,
        tx_flags::ISO15765_FRAME_PAD | tx_flags::CAN_29BIT_ID
    );
    assert_eq!(&sent29[0].data[0..4], &0x18DA_10F1u32.to_be_bytes());
}
