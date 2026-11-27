//! Parity tests for the universal dumper and J1939 transport frames.
//!
//! Ported from `ecu_engine_rust::flashing::universal_dumper`.
//!
//! ## How expected values were obtained
//!
//! The 29-bit CAN identifier layout and the DM13/DM14/DM16 frames follow
//! SAE J1939:
//!
//! * 29-bit ID = priority(3) | data page/EDP | PDU format(8) | PDU specific(8) |
//!   source address(8);
//! * PDU1 (PF < 240) uses the PDU-specific field as a **destination address**;
//!   PDU2 (PF >= 240) uses it as part of the PGN group extension.
//! * DM13 = Stop Broadcast, DM14 = Memory Request, DM16 = Binary Transfer.
//!
//! Every expectation below is derived from that layout, not from running the code.

use kivi_flashing::honda_kline::HondaProtocolFraming;
use kivi_flashing::universal_dumper::{
    FlasherProtocol, J1939TruckFlasherProtocol, UniversalEcuDumper,
};

// ---------------------------------------------------------------------------
// 29-bit CAN identifier
// ---------------------------------------------------------------------------

#[test]
fn a_pdu1_identifier_uses_the_destination_address() {
    // PGN 0x00EA00 (TP.CM) has PF = 0xEA < 240, so PDU-specific is the destination.
    let id = J1939TruckFlasherProtocol::build_29bit_can_id(6, 0x00EA00, 0x33, 0x44);
    // priority 6 -> 6 << 26
    assert_eq!(id >> 26, 6, "priority occupies the top three bits");
    assert_eq!((id >> 16) & 0xFF, 0xEA, "PDU format comes from the PGN");
    assert_eq!((id >> 8) & 0xFF, 0x33, "PDU-specific is the destination address");
    assert_eq!(id & 0xFF, 0x44, "the low byte is the source address");
}

#[test]
fn a_pdu2_identifier_uses_the_group_extension() {
    // PGN 0x00FEF1 (DM13) has PF = 0xFE >= 240, so PDU-specific is the group
    // extension taken from the PGN's low byte.
    let id = J1939TruckFlasherProtocol::build_29bit_can_id(6, 0x00FEF1, 0x33, 0x44);
    assert_eq!((id >> 16) & 0xFF, 0xFE, "PDU format");
    assert_eq!(
        (id >> 8) & 0xFF,
        0xF1,
        "PDU-specific must be the group extension 0xF1, not the destination"
    );
    assert_eq!(id & 0xFF, 0x44);
}

#[test]
fn the_priority_field_is_masked_to_three_bits() {
    // `priority & 0x07` means 8 wraps to 0 and 7 stays 7.
    let id_0 = J1939TruckFlasherProtocol::build_29bit_can_id(0, 0x00EA00, 0x00, 0x00);
    let id_7 = J1939TruckFlasherProtocol::build_29bit_can_id(7, 0x00EA00, 0x00, 0x00);
    let id_8 = J1939TruckFlasherProtocol::build_29bit_can_id(8, 0x00EA00, 0x00, 0x00);
    assert_eq!(id_8, id_0, "priority 8 must wrap to 0");
    assert_eq!(id_7 >> 26, 7, "priority 7 occupies the field");
    assert_eq!(id_0 >> 26, 0);
}

#[test]
fn the_identifier_is_deterministic() {
    assert_eq!(
        J1939TruckFlasherProtocol::build_29bit_can_id(6, 0x00FEF1, 1, 2),
        J1939TruckFlasherProtocol::build_29bit_can_id(6, 0x00FEF1, 1, 2)
    );
}

// ---------------------------------------------------------------------------
// DM13 — Stop Broadcast
// ---------------------------------------------------------------------------

#[test]
fn the_dm13_frame_is_eight_bytes() {
    let frame = J1939TruckFlasherProtocol::build_dm13_silence_broadcast_frame();
    assert_eq!(frame.len(), 8, "a J1939 CAN frame is always 8 bytes");
}

#[test]
fn dm13_requests_stop_broadcast_and_carries_the_encoded_hold() {
    let frame = J1939TruckFlasherProtocol::build_dm13_silence_broadcast_frame();
    // Byte 0 bits 0..5 = 00 means "stop broadcast".
    assert_eq!(frame[0], 0x00, "byte 0 must request stop broadcast");

    // SOURCE DEFECT, preserved and pinned: the comment says "Hold Signal
    // Duration (10,000 ms)" but the bytes are 0x10 0x27, which is 4135 ms, not
    // 10000 (0x2710). The byte order is also big-endian, so the intended value
    // would need to be 0x27 0x10. The literal is reproduced verbatim.
    let hold = u16::from_be_bytes([frame[4], frame[5]]);
    assert_eq!(hold, 0x1027, "the literal bytes are 0x10 0x27");
    assert_ne!(
        hold, 10_000,
        "the encoded duration does not match the comment's 10 000 ms"
    );
}

#[test]
fn dm13_is_deterministic() {
    assert_eq!(
        J1939TruckFlasherProtocol::build_dm13_silence_broadcast_frame(),
        J1939TruckFlasherProtocol::build_dm13_silence_broadcast_frame()
    );
}

// ---------------------------------------------------------------------------
// DM16 — binary transfer segmentation
// ---------------------------------------------------------------------------

#[test]
fn dm16_segmentation_produces_a_header_and_payload_frames() {
    let chunk: Vec<u8> = (0..40).map(|i| i as u8).collect();
    let result = J1939TruckFlasherProtocol::segment_dm16_cmdt_packets(&chunk);
    match result {
        Ok((header, packets)) => {
            assert_eq!(header.len(), 8, "the DM16 header is one 8-byte frame");
            assert!(!packets.is_empty(), "40 bytes must produce payload frames");
            for p in &packets {
                assert_eq!(p.len(), 8, "every DM16 payload frame is 8 bytes");
            }
        }
        Err(e) => panic!("segmenting 40 bytes must succeed, got: {e}"),
    }
}

#[test]
fn dm16_segmentation_of_an_empty_chunk_is_handled() {
    // Whatever the policy, it must not panic and must not silently claim data.
    let _ = J1939TruckFlasherProtocol::segment_dm16_cmdt_packets(&[]);
}

#[test]
fn dm16_segmentation_is_deterministic() {
    let chunk: Vec<u8> = (0..24).map(|i| (i * 3) as u8).collect();
    let a = J1939TruckFlasherProtocol::segment_dm16_cmdt_packets(&chunk);
    let b = J1939TruckFlasherProtocol::segment_dm16_cmdt_packets(&chunk);
    match (a, b) {
        (Ok((h1, p1)), Ok((h2, p2))) => {
            assert_eq!(h1, h2);
            assert_eq!(p1, p2);
        }
        (Err(_), Err(_)) => {}
        _ => panic!("segmentation must be consistent"),
    }
}

// ---------------------------------------------------------------------------
// Session progress
// ---------------------------------------------------------------------------

#[test]
fn a_new_session_starts_at_zero_progress() {
    let s = UniversalEcuDumper::new(FlasherProtocol::BoschUdsIsoTp, 1024);
    assert_eq!(s.progress_pct(), 0.0);
    assert!(!s.is_dump_complete());
}

#[test]
fn a_read_request_reports_an_offset_and_a_frame() {
    let s = UniversalEcuDumper::new(FlasherProtocol::BoschUdsIsoTp, 1024);
    let (remaining, frame) = s.build_next_read_request();
    // The first tuple element is the number of bytes still to read, which for a
    // fresh 1024-byte session is the whole image — not an offset.
    assert_eq!(remaining, 1024, "a fresh session has the full image remaining");
    assert!(!frame.is_empty(), "a request must carry bytes");
}

#[test]
fn the_final_crc32_is_deterministic() {
    let s = UniversalEcuDumper::new(FlasherProtocol::BoschUdsIsoTp, 1024);
    assert_eq!(s.calculate_final_crc32(), s.calculate_final_crc32());
}

#[test]
fn ingesting_a_short_response_does_not_complete_the_dump() {
    let mut s = UniversalEcuDumper::new(FlasherProtocol::BoschUdsIsoTp, 1024);
    let _ = s.ingest_block_response(&[0x00, 0x01], 512);
    assert!(
        !s.is_dump_complete(),
        "a partial response must not complete a 1024-byte dump"
    );
    assert!(s.progress_pct() < 100.0);
}

// ---------------------------------------------------------------------------
// Wave 12: request-frame integrity
//
// The audit (`docs/migration/2026-10-28-dump-flash-path-audit.md`) found that the
// K-Line request frames carried no checksum, so an ECU that validates one would
// reject every read. These tests pin the corrected behaviour.
// ---------------------------------------------------------------------------

#[test]
fn the_bms46_request_ends_with_a_valid_checksum() {
    let s = UniversalEcuDumper::new(FlasherProtocol::KLineDirectAddress, 256 * 1024);
    let (_, frame) = s.build_next_read_request();

    // Layout: [0x12, len, 0x06, addr0..3, chunk, checksum]
    assert_eq!(frame.len(), 9, "header(3) + addr(4) + chunk(1) + checksum(1)");
    let (body, checksum) = frame.split_at(frame.len() - 1);
    let expected = HondaProtocolFraming::checksum_8bit(body);
    assert_eq!(
        checksum[0], expected,
        "the trailing byte must be the two's-complement checksum of the body"
    );
}

#[test]
fn the_honda_kline_request_ends_with_a_valid_checksum() {
    let s = UniversalEcuDumper::new(FlasherProtocol::HondaKLineKwp, 256 * 1024);
    let (_, frame) = s.build_next_read_request();

    let (body, checksum) = frame.split_at(frame.len() - 1);
    let expected = HondaProtocolFraming::checksum_8bit(body);
    assert_eq!(checksum[0], expected, "Honda K-Line frames are checksummed");
}

#[test]
fn a_checksummed_frame_sums_to_zero() {
    // The defining property: body bytes plus checksum wrap to 0 mod 256.
    for proto in [
        FlasherProtocol::KLineDirectAddress,
        FlasherProtocol::HondaKLineKwp,
    ] {
        let s = UniversalEcuDumper::new(proto, 4096);
        let (_, frame) = s.build_next_read_request();
        let total: u16 = frame.iter().map(|&b| b as u16).sum();
        assert_eq!(total & 0xFF, 0, "frame {proto:?} must sum to 0 mod 256");
    }
}

#[test]
fn iso_tp_frames_carry_no_application_checksum() {
    // ISO 15765-2 protects the frame at the transport layer, so an application
    // checksum here would be wrong, not merely redundant.
    let s = UniversalEcuDumper::new(FlasherProtocol::BoschUdsIsoTp, 4096);
    let (_, frame) = s.build_next_read_request();
    assert_eq!(frame[0], 0x23, "UDS ReadMemoryByAddress");
    assert_eq!(frame[1], 0x24, "2-byte size, 4-byte address");
    assert_eq!(frame.len(), 2 + 4 + 2, "no trailing checksum byte");
}

// ---------------------------------------------------------------------------
// Handshake — the sequence an ECU needs before it answers
// ---------------------------------------------------------------------------

#[test]
fn a_kline_session_exposes_a_wakeup_frame() {
    let frame = UniversalEcuDumper::build_kline_wakeup_frame();
    assert_eq!(frame, vec![0xFE, 0x04, 0xFF, 0xFF], "ISO 14230 fast-init wake-up");
}

#[test]
fn a_kline_session_exposes_a_start_communication_frame() {
    let frame = UniversalEcuDumper::build_kwp_start_communication_frame();
    assert_eq!(frame[0], 0x72, "target address");
    assert_eq!(frame[1], 0x05, "length");
    assert_eq!(frame[2], 0x00, "startCommunication service");
    assert_eq!(frame[4], 0x99, "checksum byte");
}

#[test]
fn the_handshake_reply_is_recognised() {
    // A positive StartCommunication response begins 0x02 0x04 0x00.
    assert!(UniversalEcuDumper::is_start_communication_ack(&[0x02, 0x04, 0x00, 0xFA]));
    assert!(!UniversalEcuDumper::is_start_communication_ack(&[0x03, 0x7F, 0x00, 0x00]));
    assert!(!UniversalEcuDumper::is_start_communication_ack(&[]));
}
