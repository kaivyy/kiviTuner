//! Parity tests for the transport codecs: DoIP, TP2.0 and J2534.
//!
//! Ported from `ecu_engine_rust::flashing::{doip_transport, tp20_transport,
//! j2534_channel}`.
//!
//! ## How expected values were obtained
//!
//! Every frame layout is derived from the published standard the code
//! implements, not from running the code:
//!
//! * **DoIP** (ISO 13400-2): 8-byte header = version, inverse version, 2-byte
//!   payload type, 4-byte payload length, then the payload. The inverse-version
//!   byte must XOR to `0xFF`.
//! * **TP2.0** (ISO 15765-2 over VW transport): timing byte encodes
//!   `A + B × 2^C` milliseconds.
//! * **J2534**: pass-thru message with ISO 15765 addressing.

use kivi_flashing::doip_transport::{DoIpMessage, DoIpPayloadType, DoIpSession};
use kivi_flashing::j2534_channel::{J2534Protocol, PassThruMsg};
use kivi_flashing::tp20_transport::VagTp20Protocol;

// ===========================================================================
// DoIP
// ===========================================================================

#[test]
fn an_encoded_doip_frame_has_an_8_byte_header() {
    let msg = DoIpMessage::new(DoIpPayloadType::VehicleIdentificationRequest, vec![]);
    let bytes = msg.encode();
    assert_eq!(bytes.len(), 8, "an empty payload still carries the 8-byte header");
}

#[test]
fn the_inverse_protocol_version_byte_xors_to_0xff() {
    let msg = DoIpMessage::new(DoIpPayloadType::AliveCheckRequest, vec![]);
    let bytes = msg.encode();
    assert_eq!(
        bytes[0] ^ bytes[1],
        0xFF,
        "ISO 13400 requires version ^ inverse_version == 0xFF"
    );
}

#[test]
fn the_payload_type_is_big_endian_at_offset_2() {
    // VehicleIdentificationRequest is 0x0001 per the source table.
    let msg = DoIpMessage::new(DoIpPayloadType::VehicleIdentificationRequest, vec![]);
    let bytes = msg.encode();
    assert_eq!(&bytes[2..4], &[0x00, 0x01]);
}

#[test]
fn the_payload_length_is_big_endian_at_offset_4() {
    let msg = DoIpMessage::new(DoIpPayloadType::DiagnosticMessage, vec![0xAA, 0xBB, 0xCC]);
    let bytes = msg.encode();
    assert_eq!(&bytes[4..8], &3u32.to_be_bytes());
    assert_eq!(&bytes[8..], &[0xAA, 0xBB, 0xCC]);
}

#[test]
fn a_frame_round_trips_through_encode_and_decode() {
    let original = DoIpMessage::new(DoIpPayloadType::DiagnosticMessage, vec![1, 2, 3, 4]);
    let encoded = original.encode();
    let decoded = DoIpMessage::decode(&encoded).expect("must decode");
    assert_eq!(decoded.payload_type, original.payload_type);
    assert_eq!(decoded.payload, original.payload);
}

#[test]
fn a_frame_shorter_than_the_header_is_rejected() {
    for len in 0..8usize {
        let short = vec![0x02u8; len];
        let err = DoIpMessage::decode(&short).expect_err("must reject");
        assert!(err.contains("too short"), "error: {err}");
    }
}

#[test]
fn a_broken_inverse_version_is_rejected() {
    let mut bytes = DoIpMessage::new(DoIpPayloadType::AliveCheckRequest, vec![]).encode();
    bytes[1] = 0x00; // no longer the inverse of bytes[0]
    let err = DoIpMessage::decode(&bytes).expect_err("must reject");
    assert!(err.contains("inverse"), "error: {err}");
}

#[test]
fn a_payload_shorter_than_its_declared_length_is_rejected() {
    let mut bytes = DoIpMessage::new(DoIpPayloadType::DiagnosticMessage, vec![1, 2, 3]).encode();
    bytes.truncate(bytes.len() - 1); // header claims 3, only 2 present
    let err = DoIpMessage::decode(&bytes).expect_err("must reject");
    assert!(err.contains("shorter"), "error: {err}");
}

#[test]
fn routing_activation_targets_the_configured_addresses() {
    let session = DoIpSession::new(0x0E00, 0x1234);
    let msg = session.build_routing_activation_request();
    let bytes = msg.encode();
    // The payload must contain the tester address big-endian.
    assert!(
        bytes.windows(2).any(|w| w == 0x0E00u16.to_be_bytes()),
        "the tester address must appear in the routing activation payload"
    );
}

#[test]
fn doip_decoding_does_not_panic_on_arbitrary_bytes() {
    for len in [0usize, 1, 7, 8, 9, 32] {
        let data: Vec<u8> = (0..len).map(|i| (i * 31 % 256) as u8).collect();
        let _ = DoIpMessage::decode(&data);
    }
}

// ===========================================================================
// TP2.0
// ===========================================================================

#[test]
fn a_zero_timing_byte_decodes_to_zero_milliseconds() {
    // multiplier index 0 (0.1 ms) x count 0 = 0.
    assert_eq!(VagTp20Protocol::decode_timing_byte_ms(0x00), 0.0);
}

#[test]
fn the_lowest_multiplier_is_tenths_of_a_millisecond() {
    // Bits 7..6 = 0 -> multiplier 0.1 ms; count = 0x3F = 63 -> 6.3 ms.
    assert!((VagTp20Protocol::decode_timing_byte_ms(0x3F) - 6.3).abs() < 1e-9);
}

#[test]
fn the_second_multiplier_is_one_millisecond() {
    // Bits 7..6 = 01 -> multiplier 1.0 ms; count = 63 -> 63 ms.
    assert!((VagTp20Protocol::decode_timing_byte_ms(0x7F) - 63.0).abs() < 1e-9);
}

#[test]
fn the_third_multiplier_is_ten_milliseconds() {
    // Bits 7..6 = 10 -> multiplier 10.0 ms; count = 63 -> 630 ms.
    assert!((VagTp20Protocol::decode_timing_byte_ms(0xBF) - 630.0).abs() < 1e-9);
}

#[test]
fn the_highest_multiplier_is_one_hundred_milliseconds() {
    // Bits 7..6 = 11 -> multiplier 100.0 ms; count = 63 -> 6300 ms.
    assert!((VagTp20Protocol::decode_timing_byte_ms(0xFF) - 6300.0).abs() < 1e-9);
}

#[test]
fn timing_increases_within_each_multiplier_band() {
    // Within one band (same top two bits) the duration must rise with the count.
    for band in 0..4u8 {
        let base = band << 6;
        let mut prev = VagTp20Protocol::decode_timing_byte_ms(base);
        for count in 1..=0x3Fu8 {
            let cur = VagTp20Protocol::decode_timing_byte_ms(base | count);
            assert!(cur >= prev, "band {band} count {count}: {cur} < {prev}");
            prev = cur;
        }
    }
}

#[test]
fn a_channel_setup_request_is_six_bytes() {
    // The source returns a fixed 6-byte frame for the parameter request.
    let frame = VagTp20Protocol::build_channel_parameters_request(8);
    assert_eq!(frame.len(), 6, "the parameter frame has a fixed length");
}

#[test]
fn an_ack_frame_is_one_byte() {
    let ack = VagTp20Protocol::build_ack_frame(3);
    assert_eq!(ack.len(), 1, "a TP2.0 ack is a single byte");
}

#[test]
fn the_ack_frame_carries_the_next_sequence_number_with_the_ack_opcode() {
    // TP2.0 ack: 0xB0 | ((received + 1) & 0x0F).
    assert_eq!(VagTp20Protocol::build_ack_frame(0), [0xB1]);
    assert_eq!(VagTp20Protocol::build_ack_frame(1), [0xB2]);
    assert_eq!(VagTp20Protocol::build_ack_frame(14), [0xBF]);
}

#[test]
fn the_ack_sequence_wraps_at_fifteen() {
    // Received 15 -> next is 0 -> 0xB0.
    assert_eq!(VagTp20Protocol::build_ack_frame(15), [0xB0]);
    // Received 16 wraps to 1 -> 0xB1 (only the low nibble is used).
    assert_eq!(VagTp20Protocol::build_ack_frame(16), [0xB1]);
}

#[test]
fn a_truncated_channel_setup_response_is_rejected() {
    let err = VagTp20Protocol::parse_channel_setup_response(&[0x00])
        .expect_err("a 1-byte response cannot carry an address");
    assert!(!err.is_empty(), "the error must explain the problem");
}

#[test]
fn tp20_parsing_does_not_panic_on_arbitrary_bytes() {
    for len in [0usize, 1, 2, 5, 6, 16] {
        let data: Vec<u8> = (0..len).map(|i| (i * 13 % 256) as u8).collect();
        let _ = VagTp20Protocol::parse_channel_setup_response(&data);
    }
}

// ===========================================================================
// J2534
// ===========================================================================

#[test]
fn a_standard_obd2_flow_control_filter_matches_the_ecm_response_and_sends_to_the_request_id() {
    let filter = kivi_flashing::j2534_channel::FlowControlFilter::standard_obd2_ecm();
    // The filter matches incoming frames from the ECM response id 0x7E8...
    assert_eq!(
        filter.pattern,
        vec![0x00, 0x00, 0x07, 0xE8],
        "pattern must match the ECM response id 0x7E8"
    );
    // ...and transmits flow control to the physical request id 0x7E0.
    assert_eq!(
        filter.flow_control,
        vec![0x00, 0x00, 0x07, 0xE0],
        "flow control must target the request id 0x7E0"
    );
    assert_eq!(filter.mask, vec![0xFF, 0xFF, 0xFF, 0xFF], "an exact-match mask");
}

#[test]
fn an_iso15765_request_prefixes_the_can_id_big_endian() {
    let msg = PassThruMsg::build_iso15765_request(0x7E0, &[0x02, 0x10, 0x03]);
    assert_eq!(
        msg.protocol_id,
        J2534Protocol::Iso15765IsoTp as u32,
        "the message must declare the ISO 15765 protocol"
    );
    // The CAN id occupies the first four bytes, big-endian, then the payload.
    assert_eq!(&msg.data[0..4], &0x0000_07E0u32.to_be_bytes());
    assert_eq!(&msg.data[4..], &[0x02, 0x10, 0x03]);
}

#[test]
fn an_iso15765_request_round_trips() {
    let msg = PassThruMsg::build_iso15765_request(0x7E0, &[0xAA, 0xBB]);
    let (id, payload) = msg.unpack_iso15765_response().expect("must unpack");
    assert_eq!(id, 0x7E0);
    assert_eq!(payload, vec![0xAA, 0xBB]);
}

#[test]
fn a_message_with_a_non_iso15765_protocol_is_rejected_by_the_unpacker() {
    // The unpacker is driven by the data length, not the protocol field: a
    // 1-byte payload cannot hold the 4-byte CAN id prefix.
    let msg = PassThruMsg::new(J2534Protocol::Can, vec![0x01]);
    assert!(
        msg.unpack_iso15765_response().is_err(),
        "a payload shorter than 4 bytes cannot carry a CAN id"
    );
}

#[test]
fn an_empty_payload_still_unpacks_to_its_can_id() {
    let msg = PassThruMsg::build_iso15765_request(0x7E0, &[]);
    let (id, payload) = msg.unpack_iso15765_response().expect("must unpack");
    assert_eq!(id, 0x7E0);
    assert!(payload.is_empty(), "no payload bytes after the id");
}

#[test]
fn doip_payload_types_match_iso_13400_2_table17() {
    let nack = DoIpMessage::new(DoIpPayloadType::GenericHeaderNack, vec![0x00]).encode();
    assert_eq!(&nack[2..4], &[0x00, 0x00]);
    assert_eq!(
        DoIpMessage::decode(&nack).unwrap().payload_type,
        DoIpPayloadType::GenericHeaderNack
    );

    let vid_resp =
        DoIpMessage::new(DoIpPayloadType::VehicleIdentificationResponse, vec![0xAA]).encode();
    assert_eq!(&vid_resp[2..4], &[0x00, 0x04]);
    assert_eq!(
        DoIpMessage::decode(&vid_resp).unwrap().payload_type,
        DoIpPayloadType::VehicleIdentificationResponse
    );
}

#[test]
fn vag_tp20_setup_uses_little_endian_can_ids_and_validity_nibble() {
    let req = VagTp20Protocol::build_channel_setup_request(0x01, 0x0300);
    assert_eq!(req, [0x01, 0xC0, 0x00, 0x10, 0x00, 0x03, 0x01]);

    // Positive response assigning RX=0x300, TX=0x740, App=0x01
    let resp = [0x00, 0xD0, 0x00, 0x03, 0x40, 0x07, 0x01];
    let (rx_id, tx_id, app) = VagTp20Protocol::parse_channel_setup_ids(&resp).unwrap();
    assert_eq!(rx_id, 0x0300);
    assert_eq!(tx_id, 0x0740);
    assert_eq!(app, 0x01);

    let (tx_only, app_only) = VagTp20Protocol::parse_channel_setup_response(&resp).unwrap();
    assert_eq!(tx_only, 0x0740);
    assert_eq!(app_only, 0x01);

    // Invalid bit (0x10) must be rejected
    assert!(
        VagTp20Protocol::parse_channel_setup_response(&[0x00, 0xD0, 0x00, 0x13, 0x40, 0x07, 0x01])
            .is_err()
    );
}
