//! Tests for the ELM327 serial device.
//!
//! ## Why ELM327 matters here
//!
//! ELM327 is the cheapest path to a real car: a $15 USB adapter instead of a
//! $300 J2534 interface. It cannot flash — it is an AT-command text bridge with
//! a 1020 ms ceiling — but it *can* read the ECU and identify it, which is
//! enough for the "colok → tembak → sedap" half of the chain.
//!
//! ## What is tested
//!
//! The command encoding and the reply parsing, both of which are pure functions.
//! Opening a real port is not tested: no adapter exists here.

use kivi_flashing::elm_device::{ElmDevice, ElmError, ElmResponse};

// ---------------------------------------------------------------------------
// Initialisation sequence
// ---------------------------------------------------------------------------

#[test]
fn the_init_sequence_matches_the_documented_order() {
    // Standard ELM327 initialization sequence: reset, disable echo/linefeeds/spaces,
    // then select automatic protocol detection.
    let seq = ElmDevice::init_sequence();
    assert_eq!(
        seq,
        vec!["ATZ", "ATE0", "ATL0", "ATS0", "ATSP0"],
        "init must reset, silence echo/linefeeds/spaces, then select auto protocol"
    );
}

#[test]
fn the_init_sequence_ends_with_protocol_auto() {
    // Selecting a fixed protocol before knowing the car would fail on every
    // vehicle that is not that protocol.
    let seq = ElmDevice::init_sequence();
    assert_eq!(seq.last(), Some(&"ATSP0"));
}

// ---------------------------------------------------------------------------
// Command encoding
// ---------------------------------------------------------------------------

#[test]
fn a_command_is_encoded_with_a_carriage_return() {
    assert_eq!(ElmDevice::encode("010C"), "010C\r");
}

#[test]
fn an_empty_command_becomes_a_bare_carriage_return() {
    // Sending a bare CR/LF before ATZ wakes up low-power adapters and clones;
    // the encoder must not panic on an empty command.
    assert_eq!(ElmDevice::encode(""), "\r");
}

#[test]
fn a_command_with_trailing_whitespace_is_trimmed() {
    assert_eq!(ElmDevice::encode(" 010C "), "010C\r");
}

// ---------------------------------------------------------------------------
// Reply parsing
// ---------------------------------------------------------------------------

#[test]
fn a_plain_ok_reply_is_recognised() {
    let r = ElmResponse::parse("OK\r\r>");
    assert!(r.is_ok());
    assert_eq!(r.text, "OK");
}

#[test]
fn a_data_reply_keeps_its_hex_bytes() {
    let r = ElmResponse::parse("41 0C 1A F8\r\r>");
    assert_eq!(r.bytes, vec![0x41, 0x0C, 0x1A, 0xF8]);
}

#[test]
fn spaces_in_the_reply_are_optional() {
    let with = ElmResponse::parse("410C1AF8\r\r>");
    let without = ElmResponse::parse("41 0C 1A F8\r\r>");
    assert_eq!(with.bytes, without.bytes);
}

#[test]
fn the_elm_prompt_is_stripped() {
    let r = ElmResponse::parse("41 0C 1A F8\r\r>");
    assert!(
        !r.text.contains('>'),
        "the '>' prompt must not leak into the payload: {:?}",
        r.text
    );
}

#[test]
fn a_no_data_reply_is_reported_as_no_data() {
    let r = ElmResponse::parse("NO DATA\r\r>");
    assert!(matches!(r.status, ElmResponse::NO_DATA));
    assert!(r.bytes.is_empty());
}

#[test]
fn an_adapter_error_reply_is_reported() {
    // "?" is what an ELM327 sends for an unrecognised command.
    let r = ElmResponse::parse("?\r\r>");
    assert!(matches!(r.status, ElmResponse::ERROR));
}

#[test]
fn a_searching_reply_is_reported_as_busy() {
    let r = ElmResponse::parse("SEARCHING...\r41 0C 1A F8\r\r>");
    // The status line is informational; the payload must still be extracted.
    assert_eq!(r.bytes, vec![0x41, 0x0C, 0x1A, 0xF8]);
}

#[test]
fn a_stop_reply_is_reported() {
    let r = ElmResponse::parse("STOPPED\r\r>");
    assert!(matches!(r.status, ElmResponse::STOPPED));
}

// ---------------------------------------------------------------------------
// PID decoding through the device
// ---------------------------------------------------------------------------

#[test]
fn rpm_is_decoded_from_a_mode01_reply() {
    // SAE J1979: RPM = ((A * 256) + B) / 4.
    // 0x1A = 26, 0xF8 = 248 -> (26*256 + 248) / 4 = (6656 + 248) / 4 = 1726.
    let r = ElmResponse::parse("41 0C 1A F8\r\r>");
    let rpm = ElmDevice::decode_rpm(&r).expect("rpm");
    assert!((rpm - 1726.0).abs() < 1e-9, "expected 1726 rpm, got {rpm}");
}

#[test]
fn rpm_decoding_rejects_a_wrong_pid_echo() {
    // A reply for PID 0x0D must not be decoded as RPM.
    let r = ElmResponse::parse("41 0D 32\r\r>");
    assert!(ElmDevice::decode_rpm(&r).is_none());
}

#[test]
fn coolant_temperature_is_decoded() {
    // SAE J1979: coolant = A - 40. 0x7B = 123 -> 83 C.
    let r = ElmResponse::parse("41 05 7B\r\r>");
    let t = ElmDevice::decode_coolant_c(&r).expect("coolant");
    assert!((t - 83.0).abs() < 1e-9);
}

#[test]
fn throttle_position_is_decoded_as_a_percentage() {
    // SAE J1979: throttle = A * 100 / 255. 0xFF = 255 -> 100%.
    let r = ElmResponse::parse("41 11 FF\r\r>");
    let t = ElmDevice::decode_throttle_pct(&r).expect("throttle");
    assert!((t - 100.0).abs() < 1e-9);
}

// ---------------------------------------------------------------------------
// Capability honesty
// ---------------------------------------------------------------------------

#[test]
fn the_elm_device_declares_that_it_cannot_flash() {
    // An ELM327 is a text bridge with a 1020 ms ceiling; claiming flash support
    // would be the most dangerous lie in this codebase.
    assert!(!ElmDevice::supports_flashing());
}

#[test]
fn the_elm_device_declares_that_it_can_read_live_data() {
    assert!(ElmDevice::supports_live_data());
}

#[test]
fn the_documented_timeout_ceiling_is_1020_ms() {
    // From the ELM327 AT command specification: "AT ST FF" sets the programmable
    // timeout ceiling to 1020 milliseconds (255 * 4 ms).
    assert_eq!(ElmDevice::MAX_TIMEOUT_MS, 1020);
}

#[test]
fn an_oversized_timeout_is_clamped_to_the_ceiling() {
    assert_eq!(ElmDevice::clamp_timeout_ms(5000), 1020);
    assert_eq!(ElmDevice::clamp_timeout_ms(0), 1);
    assert_eq!(ElmDevice::clamp_timeout_ms(200), 200);
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[test]
fn the_error_type_names_the_failure() {
    let e = ElmError::NoData;
    assert!(e.to_string().to_lowercase().contains("no data"));
}

#[test]
fn multiframe_isotp_response_strips_sequence_prefixes_and_trailing_can_padding() {
    // 011 hex = 17 bytes total; last line contains 3 trailing 0x00 CAN padding bytes.
    let raw = "011\r0: 63 11 22 33 44 55\r1: 66 77 88 99 AA BB CC\r2: DD EE FF 01 00 00 00\r\r>";
    let r = ElmResponse::parse(raw);
    assert_eq!(r.bytes.len(), 17);
    assert_eq!(
        r.bytes,
        vec![
            0x63, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD,
            0xEE, 0xFF, 0x01
        ]
    );
}
