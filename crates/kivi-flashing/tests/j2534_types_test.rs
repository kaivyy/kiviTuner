//! Tests for the J2534 type layer.
//!
//! Every expected value matches the published SAE J2534-1 v04.04 API specification.

use kivi_flashing::j2534_types::{
    connect_flags, filter_type, ioctl, rx_status, tx_flags, J2534Error, PassThruMsg, ProtocolId,
    PASSTHRU_MSG_MAX_DATA,
};

// ---------------------------------------------------------------------------
// Protocol identifiers
// ---------------------------------------------------------------------------

#[test]
fn protocol_ids_match_the_published_values() {
    // From J2534.h lines 25-30.
    assert_eq!(ProtocolId::J1850Vpw.as_u32(), 0x01);
    assert_eq!(ProtocolId::J1850Pwm.as_u32(), 0x02);
    assert_eq!(ProtocolId::Iso9141.as_u32(), 0x03);
    assert_eq!(ProtocolId::Iso14230.as_u32(), 0x04);
    assert_eq!(ProtocolId::Can.as_u32(), 0x05);
    assert_eq!(ProtocolId::Iso15765.as_u32(), 0x06);
    assert_eq!(ProtocolId::SciAEngine.as_u32(), 0x07);
    assert_eq!(ProtocolId::SciATrans.as_u32(), 0x08);
    assert_eq!(ProtocolId::SciBEngine.as_u32(), 0x09);
    assert_eq!(ProtocolId::SciBTrans.as_u32(), 0x0A);
}

#[test]
fn every_known_protocol_round_trips() {
    for p in [
        ProtocolId::J1850Vpw,
        ProtocolId::J1850Pwm,
        ProtocolId::Iso9141,
        ProtocolId::Iso14230,
        ProtocolId::Can,
        ProtocolId::Iso15765,
        ProtocolId::SciAEngine,
        ProtocolId::SciATrans,
        ProtocolId::SciBEngine,
        ProtocolId::SciBTrans,
    ] {
        assert_eq!(ProtocolId::from_u32(p.as_u32()), p);
    }
}

#[test]
fn an_unknown_protocol_is_preserved_not_coerced() {
    // A driver may report a vendor extension; losing it would be a silent bug.
    assert_eq!(ProtocolId::from_u32(0x7F), ProtocolId::Unknown(0x7F));
    assert_eq!(ProtocolId::Unknown(0x7F).as_u32(), 0x7F);
}

#[test]
fn only_iso15765_carries_segmented_transfers() {
    assert!(ProtocolId::Iso15765.is_iso15765());
    for p in [
        ProtocolId::J1850Vpw,
        ProtocolId::J1850Pwm,
        ProtocolId::Iso9141,
        ProtocolId::Iso14230,
        ProtocolId::Can,
        ProtocolId::SciAEngine,
    ] {
        assert!(!p.is_iso15765(), "{p:?} must not claim ISO-TP");
    }
}

// ---------------------------------------------------------------------------
// Error codes
// ---------------------------------------------------------------------------

#[test]
fn error_codes_match_the_published_values() {
    // From J2534.h, the ERR_* block.
    assert_eq!(J2534Error::Success.as_u32(), 0x00);
    assert_eq!(J2534Error::NotSupported.as_u32(), 0x01);
    assert_eq!(J2534Error::InvalidChannelId.as_u32(), 0x02);
    assert_eq!(J2534Error::InvalidProtocolId.as_u32(), 0x03);
    assert_eq!(J2534Error::NullParameter.as_u32(), 0x04);
    assert_eq!(J2534Error::InvalidIoctlValue.as_u32(), 0x05);
    assert_eq!(J2534Error::InvalidFlags.as_u32(), 0x06);
    assert_eq!(J2534Error::Failed.as_u32(), 0x07);
}

#[test]
fn every_known_error_round_trips() {
    for e in [
        J2534Error::Success,
        J2534Error::NotSupported,
        J2534Error::InvalidChannelId,
        J2534Error::InvalidProtocolId,
        J2534Error::NullParameter,
        J2534Error::InvalidIoctlValue,
        J2534Error::InvalidFlags,
        J2534Error::Failed,
    ] {
        assert_eq!(J2534Error::from_u32(e.as_u32()), e);
    }
}

#[test]
fn only_code_zero_is_success() {
    assert!(J2534Error::Success.is_success());
    for e in [
        J2534Error::NotSupported,
        J2534Error::InvalidChannelId,
        J2534Error::InvalidProtocolId,
        J2534Error::NullParameter,
        J2534Error::InvalidIoctlValue,
        J2534Error::InvalidFlags,
        J2534Error::Failed,
        J2534Error::Unknown(0x99),
    ] {
        assert!(!e.is_success(), "{e:?} must not be treated as success");
    }
}

#[test]
fn an_unknown_error_keeps_its_code_in_the_message() {
    let e = J2534Error::Unknown(0x2A);
    assert_eq!(e.to_string(), "unknown J2534 error 0x2a");
    assert_eq!(J2534Error::from_u32(0x2A), J2534Error::Unknown(0x2A));
}

#[test]
fn errors_are_usable_as_std_errors() {
    let e: Box<dyn std::error::Error> = Box::new(J2534Error::Failed);
    assert!(e.to_string().contains("PassThruGetLastError"));
}

// ---------------------------------------------------------------------------
// Flag constants
// ---------------------------------------------------------------------------

#[test]
fn tx_flag_values_match_the_header() {
    assert_eq!(tx_flags::CAN_29BIT_ID, 0x0000_0100);
    assert_eq!(tx_flags::ISO15765_ADDR_TYPE, 0x0000_0080);
    assert_eq!(tx_flags::ISO15765_FRAME_PAD, 0x0000_0040);
    assert_eq!(tx_flags::WAIT_P3_MIN_ONLY, 0x0000_0200);
}

#[test]
fn rx_status_values_match_the_header() {
    assert_eq!(rx_status::TX_MSG_TYPE, 0x0000_0001);
    assert_eq!(rx_status::START_OF_MESSAGE, 0x0000_0002);
    assert_eq!(rx_status::RX_BREAK, 0x0000_0004);
    assert_eq!(rx_status::TX_DONE, 0x0000_0008);
    assert_eq!(rx_status::ISO15765_PADDING_ERROR, 0x0000_0010);
    assert_eq!(rx_status::ISO15765_ADDR_TYPE, 0x0000_0080);
    assert_eq!(rx_status::CAN_29BIT_ID, 0x0000_0100);
}

#[test]
fn connect_flag_values_match_the_header() {
    assert_eq!(connect_flags::CAN_29BIT_ID, 0x0000_0100);
    assert_eq!(connect_flags::ISO9141_NO_CHECKSUM, 0x0000_0200);
    assert_eq!(connect_flags::CAN_ID_BOTH, 0x0000_0800);
    assert_eq!(connect_flags::ISO9141_K_LINE_ONLY, 0x0000_1000);
}

#[test]
fn ioctl_ids_match_the_header() {
    assert_eq!(ioctl::GET_CONFIG, 0x01);
    assert_eq!(ioctl::SET_CONFIG, 0x02);
    assert_eq!(ioctl::READ_VBATT, 0x03);
    assert_eq!(ioctl::FIVE_BAUD_INIT, 0x04);
    assert_eq!(ioctl::FAST_INIT, 0x05);
    assert_eq!(ioctl::CLEAR_TX_BUFFER, 0x07);
    assert_eq!(ioctl::CLEAR_RX_BUFFER, 0x08);
    assert_eq!(ioctl::CLEAR_PERIODIC_MSGS, 0x09);
    assert_eq!(ioctl::CLEAR_MSG_FILTERS, 0x0A);
    assert_eq!(ioctl::READ_PROG_VOLTAGE, 0x0E);
}

#[test]
fn filter_types_match_the_header() {
    assert_eq!(filter_type::PASS_FILTER, 0x01);
    assert_eq!(filter_type::BLOCK_FILTER, 0x02);
    assert_eq!(filter_type::FLOW_CONTROL_FILTER, 0x03);
}

// ---------------------------------------------------------------------------
// Message struct
// ---------------------------------------------------------------------------

#[test]
fn the_message_payload_capacity_matches_the_c_header() {
    // `unsigned char Data[4128]` in PASSTHRU_MSG.
    assert_eq!(PASSTHRU_MSG_MAX_DATA, 4128);
}

#[test]
fn a_new_message_carries_no_flags_and_the_given_payload() {
    let m = PassThruMsg::new(ProtocolId::Iso15765, vec![0x02, 0x10, 0x03]);
    assert_eq!(m.protocol_id, ProtocolId::Iso15765);
    assert_eq!(m.data, vec![0x02, 0x10, 0x03]);
    assert_eq!(m.data_size(), 3);
    assert_eq!(m.tx_flags, 0);
    assert_eq!(m.rx_status, 0);
    assert_eq!(m.timestamp, 0);
    assert_eq!(m.extra_data_index, 0);
}

#[test]
fn flags_are_carried_through_with_flags() {
    let m = PassThruMsg::with_flags(
        ProtocolId::Can,
        tx_flags::CAN_29BIT_ID | tx_flags::ISO15765_FRAME_PAD,
        vec![0xAA],
    );
    assert_eq!(
        m.tx_flags,
        tx_flags::CAN_29BIT_ID | tx_flags::ISO15765_FRAME_PAD
    );
    assert_eq!(m.protocol_id, ProtocolId::Can);
}

#[test]
fn a_received_frame_is_not_a_tx_confirmation() {
    let mut m = PassThruMsg::new(ProtocolId::Iso15765, vec![0x02, 0x50, 0x03]);
    m.rx_status = 0;
    assert!(!m.is_tx_confirmation());
}

#[test]
fn a_tx_confirmation_is_recognised() {
    let mut m = PassThruMsg::new(ProtocolId::Iso15765, vec![]);
    m.rx_status = rx_status::TX_MSG_TYPE;
    assert!(m.is_tx_confirmation());
}

#[test]
fn a_tx_confirmation_that_also_starts_a_message_is_still_a_confirmation() {
    // Drivers set both bits; the test pins that TX_MSG_TYPE wins for the
    // "is this my own frame coming back" question.
    let mut m = PassThruMsg::new(ProtocolId::Iso15765, vec![]);
    m.rx_status = rx_status::TX_MSG_TYPE | rx_status::START_OF_MESSAGE;
    assert!(m.is_tx_confirmation());
    assert!(m.is_first_frame());
}

#[test]
fn a_first_frame_is_recognised() {
    let mut m = PassThruMsg::new(ProtocolId::Iso15765, vec![0x10, 0x14, 0x00, 0x01]);
    m.rx_status = rx_status::START_OF_MESSAGE;
    assert!(m.is_first_frame());
    assert!(!m.is_tx_confirmation());
}

#[test]
fn a_consecutive_frame_is_not_a_first_frame() {
    let mut m = PassThruMsg::new(ProtocolId::Iso15765, vec![0x21, 0xAA]);
    m.rx_status = 0;
    assert!(!m.is_first_frame());
}

#[test]
fn an_empty_payload_reports_zero_size() {
    let m = PassThruMsg::new(ProtocolId::Can, vec![]);
    assert_eq!(m.data_size(), 0);
}
