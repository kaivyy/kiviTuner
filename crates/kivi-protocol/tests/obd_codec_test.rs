//! Unit and fixture-based tests for the read-only OBD-II codec.
//!
//! Covers:
//! - Supported-PID bitmap mapping algorithm and boundary conditions (Section 5).
//! - Closed typed read-only command domain and serialization (Section 6).
//! - Adapter text parsing, error tokens, and multi-line ISO-TP reassembly (Section 7).
//! - Mode 01 diagnostic payload decoding, equations, and wideband O2 PID 0x34 (Section 8).
//! - Mode 09 metadata decoding for VIN and Calibration ID (Section 9).
//! - Negative tests, length checks, and error handling (Section 10).

use kivi_protocol::obd::adapter::parse_adapter_response;
use kivi_protocol::obd::bitmap::{
    calculate_pid_bitmap_coordinates, is_pid_supported_in_bitmap, SupportedPidsBitmap,
};
use kivi_protocol::obd::command::{AdapterCommand, Mode01Pid, Mode09Item, ObdCommand};
use kivi_protocol::obd::errors::ObdCodecError;
use kivi_protocol::obd::metadata::{decode_calibration_id, decode_vin};
use kivi_protocol::obd::pid::{decode_mode01_payload, DecodedMode01Value, WidebandO2Sample};

// ===========================================================================
// SECTION 5: Supported-PID Bitmap Mapping Tests
// ===========================================================================

#[test]
fn test_bitmap_mapping_mandatory_examples() {
    // 1. PID 0x01 in base 0x00: Byte A (index 0), bit 7, mask 0x80
    let c1 = calculate_pid_bitmap_coordinates(0x01, 0x00)
        .expect("PID 0x01 should be valid in base 0x00");
    assert_eq!(c1.byte_index, 0); // Byte A
    assert_eq!(c1.bit_index, 7);
    assert_eq!(c1.bit_mask, 0x80);
    assert_eq!(c1.global_bit_index, 31);

    // 2. PID 0x1F in base 0x00: Byte D (index 3), bit 1, mask 0x02
    let c2 = calculate_pid_bitmap_coordinates(0x1F, 0x00)
        .expect("PID 0x1F should be valid in base 0x00");
    assert_eq!(c2.byte_index, 3); // Byte D
    assert_eq!(c2.bit_index, 1);
    assert_eq!(c2.bit_mask, 0x02);
    assert_eq!(c2.global_bit_index, 1);

    // 3. PID 0x20 in base 0x00: Byte D (index 3), bit 0, mask 0x01 (continuation indicator)
    let c3 = calculate_pid_bitmap_coordinates(0x20, 0x00)
        .expect("PID 0x20 should be valid in base 0x00");
    assert_eq!(c3.byte_index, 3); // Byte D
    assert_eq!(c3.bit_index, 0);
    assert_eq!(c3.bit_mask, 0x01);
    assert_eq!(c3.global_bit_index, 0);

    // 4. PID 0x21 in base 0x20: Byte A (index 0), bit 7, mask 0x80
    let c4 = calculate_pid_bitmap_coordinates(0x21, 0x20)
        .expect("PID 0x21 should be valid in base 0x20");
    assert_eq!(c4.byte_index, 0); // Byte A
    assert_eq!(c4.bit_index, 7);
    assert_eq!(c4.bit_mask, 0x80);
    assert_eq!(c4.global_bit_index, 31);

    // 5. PID 0x34 in base 0x20:
    // Offset = 0x34 - 0x21 = 0x13 = 19 decimal.
    // byte_index = 19 / 8 = 2 (Byte C).
    // bit_index = 7 - (19 % 8) = 7 - 3 = 4 (Bit 4, mask 0x10).
    // Global bit numbering: 31 - 19 = 12.
    let c5 = calculate_pid_bitmap_coordinates(0x34, 0x20)
        .expect("PID 0x34 should be valid in base 0x20");
    assert_eq!(c5.byte_index, 2); // Byte C
    assert_eq!(c5.bit_index, 4);
    assert_eq!(c5.bit_mask, 0x10);
    assert_eq!(c5.global_bit_index, 12);

    // 6. PID 0x3F in base 0x20: Byte D (index 3), bit 1, mask 0x02
    let c6 = calculate_pid_bitmap_coordinates(0x3F, 0x20)
        .expect("PID 0x3F should be valid in base 0x20");
    assert_eq!(c6.byte_index, 3); // Byte D
    assert_eq!(c6.bit_index, 1);
    assert_eq!(c6.bit_mask, 0x02);
    assert_eq!(c6.global_bit_index, 1);

    // 7. PID 0x40 in base 0x20: Byte D (index 3), bit 0, mask 0x01 (continuation indicator)
    let c7 = calculate_pid_bitmap_coordinates(0x40, 0x20)
        .expect("PID 0x40 should be valid in base 0x20");
    assert_eq!(c7.byte_index, 3); // Byte D
    assert_eq!(c7.bit_index, 0);
    assert_eq!(c7.bit_mask, 0x01);
    assert_eq!(c7.global_bit_index, 0);
}

#[test]
fn test_bitmap_pid_out_of_range_rejected() {
    // Below first PID in base 0x00
    assert!(calculate_pid_bitmap_coordinates(0x00, 0x00).is_err());
    // Above last PID in base 0x00
    assert!(calculate_pid_bitmap_coordinates(0x21, 0x00).is_err());
    // Below first PID in base 0x20
    assert!(calculate_pid_bitmap_coordinates(0x20, 0x20).is_err());
    // Above last PID in base 0x20
    assert!(calculate_pid_bitmap_coordinates(0x41, 0x20).is_err());
}

#[test]
fn test_bitmap_decoding_and_support_checks() {
    // Synthetic fixture: Bit 7 of byte A set (PID 0x01), Bit 0 of byte D set (PID 0x20 continuation)
    let data = [0x80, 0x00, 0x00, 0x01];
    let bitmap = SupportedPidsBitmap::from_bytes(0x00, data);

    assert!(bitmap.is_supported(0x01));
    assert!(is_pid_supported_in_bitmap(0x01, 0x00, &data).unwrap());
    assert!(!bitmap.is_supported(0x02));
    assert!(!is_pid_supported_in_bitmap(0x02, 0x00, &data).unwrap());
    assert!(!bitmap.is_supported(0x1F));
    assert!(bitmap.has_next_bitmap_range()); // Continuation bit set

    // Synthetic fixture for base 0x20: Only PID 0x34 set (Byte C, bit 4, mask 0x10 = [0x00, 0x00, 0x10, 0x00])
    let data_20 = [0x00, 0x00, 0x10, 0x00];
    let bitmap_20 = SupportedPidsBitmap::from_bytes(0x20, data_20);

    assert!(bitmap_20.is_supported(0x34));
    assert!(!bitmap_20.is_supported(0x21));
    assert!(!bitmap_20.is_supported(0x33));
    assert!(!bitmap_20.is_supported(0x35));
    assert!(!bitmap_20.has_next_bitmap_range()); // Continuation bit clear

    // Continuation bit set must NOT mark ordinary measurement PID as supported
    let data_cont_only = [0x00, 0x00, 0x00, 0x01];
    let bitmap_cont = SupportedPidsBitmap::from_bytes(0x00, data_cont_only);
    assert!(!bitmap_cont.is_supported(0x1F));
    assert!(!bitmap_cont.is_supported(0x0C));
    assert!(bitmap_cont.has_next_bitmap_range());
}

#[test]
fn test_bitmap_payload_validation_rejects_malformed_lengths() {
    // Mode 01 PID 0x00 diagnostic payload must be exactly 6 bytes: [0x41, 0x00, A, B, C, D]
    let short_payload = [0x41, 0x00, 0xBE, 0x3F, 0xB8]; // 5 bytes (missing byte D)
    let err = SupportedPidsBitmap::decode_from_payload(&short_payload).unwrap_err();
    assert!(matches!(
        err,
        ObdCodecError::TruncatedPayload {
            expected: 6,
            actual: 5
        }
    ));

    let oversized_payload = [0x41, 0x00, 0xBE, 0x3F, 0xB8, 0x13, 0xFF]; // 7 bytes
    let err2 = SupportedPidsBitmap::decode_from_payload(&oversized_payload).unwrap_err();
    assert!(matches!(
        err2,
        ObdCodecError::OversizedPayload {
            expected: 6,
            actual: 7
        }
    ));

    let wrong_service = [0x42, 0x00, 0xBE, 0x3F, 0xB8, 0x13];
    let err3 = SupportedPidsBitmap::decode_from_payload(&wrong_service).unwrap_err();
    assert!(matches!(
        err3,
        ObdCodecError::UnexpectedService {
            expected: 0x41,
            actual: 0x42
        }
    ));
}

// ===========================================================================
// SECTION 6: Typed Read-Only Command Domain Tests
// ===========================================================================

#[test]
fn test_command_serialization_deterministic_ascii() {
    // Mode 01 requests
    assert_eq!(
        ObdCommand::Mode01(Mode01Pid::SupportedPids01_20).encode(),
        "01 00\r"
    );
    assert_eq!(
        ObdCommand::Mode01(Mode01Pid::SupportedPids21_40).encode(),
        "01 20\r"
    );
    assert_eq!(ObdCommand::Mode01(Mode01Pid::EngineRpm).encode(), "01 0C\r");
    assert_eq!(
        ObdCommand::Mode01(Mode01Pid::ThrottlePosition).encode(),
        "01 11\r"
    );
    assert_eq!(
        ObdCommand::Mode01(Mode01Pid::ManifoldPressure).encode(),
        "01 0B\r"
    );
    assert_eq!(
        ObdCommand::Mode01(Mode01Pid::CoolantTemp).encode(),
        "01 05\r"
    );
    assert_eq!(
        ObdCommand::Mode01(Mode01Pid::IntakeAirTemp).encode(),
        "01 0F\r"
    );
    assert_eq!(
        ObdCommand::Mode01(Mode01Pid::ControlModuleVoltage).encode(),
        "01 42\r"
    );
    assert_eq!(
        ObdCommand::Mode01(Mode01Pid::VehicleSpeed).encode(),
        "01 0D\r"
    );
    assert_eq!(
        ObdCommand::Mode01(Mode01Pid::WidebandLambda1).encode(),
        "01 34\r"
    );

    // Mode 09 requests
    assert_eq!(
        ObdCommand::Mode09(Mode09Item::SupportedPids09_00).encode(),
        "09 00\r"
    );
    assert_eq!(ObdCommand::Mode09(Mode09Item::Vin).encode(), "09 02\r");
    assert_eq!(
        ObdCommand::Mode09(Mode09Item::CalibrationId).encode(),
        "09 04\r"
    );

    // Adapter configuration commands
    assert_eq!(ObdCommand::Adapter(AdapterCommand::Reset).encode(), "ATZ\r");
    assert_eq!(
        ObdCommand::Adapter(AdapterCommand::EchoOff).encode(),
        "ATE0\r"
    );
    assert_eq!(
        ObdCommand::Adapter(AdapterCommand::LinefeedsOff).encode(),
        "ATL0\r"
    );
    assert_eq!(
        ObdCommand::Adapter(AdapterCommand::SpacesOn).encode(),
        "ATS1\r"
    );
    assert_eq!(
        ObdCommand::Adapter(AdapterCommand::HeadersOff).encode(),
        "ATH0\r"
    );
    assert_eq!(
        ObdCommand::Adapter(AdapterCommand::AdaptiveTimingAuto).encode(),
        "ATAT1\r"
    );
    assert_eq!(
        ObdCommand::Adapter(AdapterCommand::SetProtocolCan11_500).encode(),
        "ATSP6\r"
    );
    assert_eq!(
        ObdCommand::Adapter(AdapterCommand::ReadVoltage).encode(),
        "ATRV\r"
    );
    assert_eq!(
        ObdCommand::Adapter(AdapterCommand::ReadProtocol).encode(),
        "ATDPN\r"
    );
    assert_eq!(
        ObdCommand::Adapter(AdapterCommand::QueryElmVersion).encode(),
        "ATI\r"
    );
    assert_eq!(
        ObdCommand::Adapter(AdapterCommand::QueryStnDeviceId).encode(),
        "STDI\r"
    );
    assert_eq!(
        ObdCommand::Adapter(AdapterCommand::QueryStnFirmwareVersion).encode(),
        "STI\r"
    );
    assert_eq!(
        ObdCommand::Adapter(AdapterCommand::QueryStnSerialNumber).encode(),
        "STSN\r"
    );
}

// ===========================================================================
// SECTION 7: Adapter Text & Multi-Line Parsing Tests
// ===========================================================================

#[test]
fn test_adapter_text_parsing_single_line_positive() {
    let raw = "41 0C 1A F8\r\r>";
    let payload = parse_adapter_response(raw).expect("Should parse valid single line");
    assert_eq!(payload, vec![0x41, 0x0C, 0x1A, 0xF8]);
}

#[test]
fn test_adapter_text_parsing_error_responses() {
    assert!(matches!(
        parse_adapter_response("NO DATA\r\r>"),
        Err(ObdCodecError::NoData)
    ));
    assert!(matches!(
        parse_adapter_response("?\r\r>"),
        Err(ObdCodecError::AdapterError(_))
    ));
    assert!(matches!(
        parse_adapter_response("BUS BUSY\r\r>"),
        Err(ObdCodecError::AdapterError(_))
    ));
    assert!(matches!(
        parse_adapter_response("CAN ERROR\r\r>"),
        Err(ObdCodecError::AdapterError(_))
    ));
    assert!(matches!(
        parse_adapter_response("BUFFER FULL\r\r>"),
        Err(ObdCodecError::AdapterError(_))
    ));
    assert!(matches!(
        parse_adapter_response("UNABLE TO CONNECT\r\r>"),
        Err(ObdCodecError::AdapterError(_))
    ));
    assert!(matches!(
        parse_adapter_response("STOPPED\r\r>"),
        Err(ObdCodecError::AdapterError(_))
    ));
}

#[test]
fn test_adapter_text_parsing_multiline_isotp_vin() {
    // Synthetic multi-frame fixture for VIN: exactly 20 bytes payload (0x014 hex)
    // Synthetic VIN: "1KV1TUNER00000001" (17 chars)
    let raw_multiline = "014\r\n0: 49 02 01 31 4B 56\r\n1: 31 54 55 4E 45 52 30\r\n2: 30 30 30 30 30 30 31\r\n\r\n>";
    let payload =
        parse_adapter_response(raw_multiline).expect("Should reassemble multi-line ISO-TP payload");
    assert_eq!(payload.len(), 20);
    assert_eq!(payload[0], 0x49); // Service
    assert_eq!(payload[1], 0x02); // PID
    assert_eq!(payload[2], 0x01); // Item count
    assert_eq!(&payload[3..], b"1KV1TUNER00000001");
}

#[test]
fn test_adapter_text_parsing_multiline_isotp_declared_len_014_with_padding() {
    // Declared length 0x014 (20 bytes), but CAN frame 2 contains a padding byte (0xAA), totaling 21 frame bytes.
    // Parser must preserve declared length 0x014 and truncate payload to exactly 20 bytes.
    let raw = "014\r\n0: 49 02 01 31 4B 56\r\n1: 31 54 55 4E 45 52 30\r\n2: 30 30 30 30 30 30 31 AA\r\n\r\n>";
    let payload = parse_adapter_response(raw)
        .expect("Should reassemble multi-line ISO-TP payload with padding");
    assert_eq!(
        payload.len(),
        20,
        "Declared length 014 must not be stripped; payload must be truncated to 20 bytes"
    );
    assert_eq!(&payload[3..], b"1KV1TUNER00000001");
}

#[test]
fn test_adapter_text_parsing_multiline_isotp_declared_len_01a_with_padding() {
    // Declared length 0x01A (26 bytes) with 27 frame bytes (1 trailing padding byte 0x55).
    let raw = "01A\r\n0: 49 02 01 31 4B 56\r\n1: 31 54 55 4E 45 52 30\r\n2: 30 30 30 30 30 30 31\r\n3: 41 42 43 44 45 46 55\r\n\r\n>";
    let payload = parse_adapter_response(raw).expect("Should parse declared length 01A");
    assert_eq!(
        payload.len(),
        26,
        "Declared length 01A must be preserved and enforce 26 bytes"
    );
}

#[test]
fn test_adapter_text_parsing_multiline_isotp_declared_len_090_with_padding() {
    // Declared length 0x090 (144 bytes) with 1 extra padding byte (145 frame bytes).
    // Build 145 bytes across ISO-TP frames (Frame 0: 6 bytes, Frames 1..20: 7 bytes each = 6 + 140 = 146 bytes).
    let mut raw = String::from("090\r\n0: 49 04 01 02 03 04\r\n");
    let mut total_bytes = 6;
    let mut seq = 1;
    while total_bytes < 145 {
        let chunk_len = (145 - total_bytes).min(7);
        raw.push_str(&format!("{:X}:", seq));
        for _ in 0..chunk_len {
            raw.push_str(" 00");
        }
        raw.push_str("\r\n");
        total_bytes += chunk_len;
        seq = (seq + 1) % 16;
    }
    raw.push('>');

    let payload = parse_adapter_response(&raw).expect("Should parse declared length 090");
    assert_eq!(
        payload.len(),
        144,
        "Declared length 090 must be preserved and truncate padding to 144 bytes"
    );
}

#[test]
fn test_adapter_text_parsing_multiline_mode09_vin_with_echo_enabled() {
    // Echo enabled: line 0 is "09 02", followed by declared length "014", followed by frames
    let raw_spaced = "09 02\r\n014\r\n0: 49 02 01 31 4B 56\r\n1: 31 54 55 4E 45 52 30\r\n2: 30 30 30 30 30 30 31 AA\r\n\r\n>";
    let payload_spaced = parse_adapter_response(raw_spaced)
        .expect("Should handle spaced echo followed by declared length");
    assert_eq!(payload_spaced.len(), 20);
    assert_eq!(&payload_spaced[3..], b"1KV1TUNER00000001");

    // Unspaced echo: line 0 is "0902"
    let raw_unspaced = "0902\r\n014\r\n0: 49 02 01 31 4B 56\r\n1: 31 54 55 4E 45 52 30\r\n2: 30 30 30 30 30 30 31\r\n\r\n>";
    let payload_unspaced = parse_adapter_response(raw_unspaced)
        .expect("Should handle compact echo followed by declared length");
    assert_eq!(payload_unspaced.len(), 20);
    assert_eq!(&payload_unspaced[3..], b"1KV1TUNER00000001");
}

#[test]
fn test_adapter_diagnostic_line_resembling_command_not_stripped() {
    // Line 0 resembles a command "01 0C", but line 1 is not a response to 01 0C (it is 02 03).
    // Must NOT be stripped as an echo line!
    let raw = "01 0C\r\n02 03\r\n>";
    let payload =
        parse_adapter_response(raw).expect("Should parse all bytes without stripping line 0");
    // All 4 bytes must be preserved: 0x01, 0x0C, 0x02, 0x03
    assert_eq!(payload, vec![0x01, 0x0C, 0x02, 0x03]);
}

#[test]
fn test_adapter_echo_case_and_whitespace_variations() {
    // Lowercase spaced echo
    let raw_lower_spaced = "01 0c\r\n41 0c 1a f8\r\r>";
    let p1 = parse_adapter_response(raw_lower_spaced).expect("Lower spaced echo");
    assert_eq!(p1, vec![0x41, 0x0C, 0x1A, 0xF8]);

    // Lowercase compact echo
    let raw_lower_compact = "010c\r\n41 0c 1a f8\r\r>";
    let p2 = parse_adapter_response(raw_lower_compact).expect("Lower compact echo");
    assert_eq!(p2, vec![0x41, 0x0C, 0x1A, 0xF8]);

    // Extra whitespace surrounding echo tokens
    let raw_ws = "   01    0C   \r\n41 0C 1A F8\r\r>";
    let p3 = parse_adapter_response(raw_ws).expect("Whitespace padded echo");
    assert_eq!(p3, vec![0x41, 0x0C, 0x1A, 0xF8]);
}

#[test]
fn test_adapter_echo_with_searching_status() {
    // SEARCHING... status line between echoed command and response
    let raw = "01 0C\r\nSEARCHING...\r\n41 0C 1A F8\r\r>";
    let payload = parse_adapter_response(raw).expect("Should handle SEARCHING status with echo");
    assert_eq!(payload, vec![0x41, 0x0C, 0x1A, 0xF8]);
}

#[test]
fn test_adapter_echo_with_adapter_error() {
    // Echo followed by adapter error token
    let raw = "01 0C\r\nCAN ERROR\r\r>";
    let err = parse_adapter_response(raw).unwrap_err();
    assert!(matches!(err, ObdCodecError::AdapterError(_)));

    let raw_no_data = "01 0C\r\nNO DATA\r\r>";
    let err_nd = parse_adapter_response(raw_no_data).unwrap_err();
    assert!(matches!(err_nd, ObdCodecError::NoData));
}

#[test]
fn test_adapter_malformed_ambiguous_echo_candidates() {
    // Single 2-char token cannot be an echo of 2-byte command
    let raw_short = "01\r\n41 0C 1A F8\r\r>";
    let p1 = parse_adapter_response(raw_short).expect("Should parse as flat hex without stripping");
    assert_eq!(p1, vec![0x01, 0x41, 0x0C, 0x1A, 0xF8]);

    // 3-token line cannot be an echo of 2-byte command
    let raw_3tok = "01 0C 1A\r\n41 0C 1A F8\r\r>";
    let p2 = parse_adapter_response(raw_3tok).expect("Should parse as flat hex without stripping");
    assert_eq!(p2, vec![0x01, 0x0C, 0x1A, 0x41, 0x0C, 0x1A, 0xF8]);
}

#[test]
fn test_adapter_text_parsing_multiline_out_of_order_rejected() {
    // Sequence 1 before sequence 0
    let out_of_order =
        "014\r1: 31 54 55 4E 45 52 30\r0: 49 02 01 31 4B 56\r2: 30 30 30 30 30 30 31\r>";
    let err = parse_adapter_response(out_of_order).unwrap_err();
    assert!(matches!(err, ObdCodecError::MalformedText(_)));
}

// ===========================================================================
// SECTION 8: Mode 01 PID Payload Decoding Tests
// ===========================================================================

#[test]
fn test_decode_mode01_rpm() {
    let payload = [0x41, 0x0C, 0x1A, 0xF8]; // (26 * 256 + 248) / 4 = 1726.0
    let val = decode_mode01_payload(Mode01Pid::EngineRpm, &payload).expect("Decode RPM failed");
    match val {
        DecodedMode01Value::EngineRpm(rpm) => {
            assert!((rpm - 1726.0).abs() < 0.001);
        }
        _ => panic!("Expected EngineRpm variant"),
    }

    // Boundary check zero
    let zero_payload = [0x41, 0x0C, 0x00, 0x00];
    assert_eq!(
        decode_mode01_payload(Mode01Pid::EngineRpm, &zero_payload).unwrap(),
        DecodedMode01Value::EngineRpm(0.0)
    );

    // Boundary check max
    let max_payload = [0x41, 0x0C, 0xFF, 0xFF];
    assert_eq!(
        decode_mode01_payload(Mode01Pid::EngineRpm, &max_payload).unwrap(),
        DecodedMode01Value::EngineRpm(16383.75)
    );
}

#[test]
fn test_decode_mode01_tps() {
    let payload = [0x41, 0x11, 0x80]; // 128 * 100 / 255 = 50.196%
    let val =
        decode_mode01_payload(Mode01Pid::ThrottlePosition, &payload).expect("Decode TPS failed");
    match val {
        DecodedMode01Value::ThrottlePosition(pct) => {
            assert!((pct - 50.196).abs() < 0.01);
        }
        _ => panic!("Expected ThrottlePosition variant"),
    }
}

#[test]
fn test_decode_mode01_map() {
    let payload = [0x41, 0x0B, 0x64]; // 100 kPa
    let val =
        decode_mode01_payload(Mode01Pid::ManifoldPressure, &payload).expect("Decode MAP failed");
    assert_eq!(val, DecodedMode01Value::ManifoldPressure(100));
}

#[test]
fn test_decode_mode01_coolant_and_air_temp() {
    let ect_payload = [0x41, 0x05, 0x7D]; // 125 - 40 = 85°C
    let val_ect = decode_mode01_payload(Mode01Pid::CoolantTemp, &ect_payload).unwrap();
    assert_eq!(val_ect, DecodedMode01Value::CoolantTemp(85.0));

    let iat_payload = [0x41, 0x0F, 0x46]; // 70 - 40 = 30°C
    let val_iat = decode_mode01_payload(Mode01Pid::IntakeAirTemp, &iat_payload).unwrap();
    assert_eq!(val_iat, DecodedMode01Value::IntakeAirTemp(30.0));
}

#[test]
fn test_decode_mode01_battery_voltage() {
    let payload = [0x41, 0x42, 0x35, 0xE8]; // (13800) / 1000 = 13.8V
    let val = decode_mode01_payload(Mode01Pid::ControlModuleVoltage, &payload).unwrap();
    match val {
        DecodedMode01Value::ControlModuleVoltage(v) => assert!((v - 13.8).abs() < 0.001),
        _ => panic!("Expected ControlModuleVoltage variant"),
    }
}

#[test]
fn test_decode_mode01_vehicle_speed() {
    let payload = [0x41, 0x0D, 0x3C]; // 60 km/h
    let val = decode_mode01_payload(Mode01Pid::VehicleSpeed, &payload).unwrap();
    assert_eq!(val, DecodedMode01Value::VehicleSpeed(60.0));
}

#[test]
fn test_decode_mode01_wideband_o2_pid_0x34() {
    // Synthetic fixture:
    // A=0x80, B=0x00 -> Lambda = 32768 / 32768.0 = 1.000
    // C=0x80, D=0x00 -> Current = (32768 / 256.0) - 128.0 = 0.0 mA
    let payload = [0x41, 0x34, 0x80, 0x00, 0x80, 0x00];
    let val = decode_mode01_payload(Mode01Pid::WidebandLambda1, &payload)
        .expect("Decode wideband failed");
    match val {
        DecodedMode01Value::WidebandLambda1(sample) => {
            assert!((sample.equivalence_ratio - 1.0).abs() < 0.0001);
            assert!((sample.sensor_current_ma - 0.0).abs() < 0.0001);
            // Verify estimated AFR calculation with explicit stoich ratio (14.7 for pure gasoline)
            let afr = sample.estimated_afr(14.7).expect("valid stoich ratio");
            assert!((afr - 14.7).abs() < 0.01);

            // Rejects invalid stoich ratios
            assert!(sample.estimated_afr(0.0).is_err());
            assert!(sample.estimated_afr(-14.7).is_err());
            assert!(sample.estimated_afr(f32::NAN).is_err());
            assert!(sample.estimated_afr(f32::INFINITY).is_err());
            assert!(sample.estimated_afr(f32::NEG_INFINITY).is_err());
        }
        _ => panic!("Expected WidebandLambda1 variant"),
    }

    // Synthetic rich condition fixture:
    // A=0x6E, B=0x00 -> 28160 / 32768.0 = 0.859375 Lambda
    // C=0x60, D=0x00 -> (24576 / 256.0) - 128.0 = 96.0 - 128.0 = -32.0 mA
    let rich_payload = [0x41, 0x34, 0x6E, 0x00, 0x60, 0x00];
    let val_rich = decode_mode01_payload(Mode01Pid::WidebandLambda1, &rich_payload).unwrap();
    match val_rich {
        DecodedMode01Value::WidebandLambda1(sample) => {
            assert!((sample.equivalence_ratio - 0.859375).abs() < 0.0001);
            assert!((sample.sensor_current_ma - (-32.0)).abs() < 0.0001);
            let afr = sample.estimated_afr(14.7).expect("valid stoich ratio");
            assert!((afr - 12.6328).abs() < 0.01);
        }
        _ => panic!("Expected WidebandLambda1 variant"),
    }

    let explicit_sample = WidebandO2Sample::new(1.0, 0.0);
    assert_eq!(explicit_sample.equivalence_ratio, 1.0);
    assert_eq!(explicit_sample.sensor_current_ma, 0.0);

    // Truncated payload: only 2 data bytes instead of 4
    let truncated = [0x41, 0x34, 0x80, 0x00];
    let err = decode_mode01_payload(Mode01Pid::WidebandLambda1, &truncated).unwrap_err();
    assert!(matches!(
        err,
        ObdCodecError::TruncatedPayload {
            expected: 6,
            actual: 4
        }
    ));
}

// ===========================================================================
// SECTION 9: Mode 09 Metadata Decoding Tests
// ===========================================================================

#[test]
fn test_decode_mode09_vin_synthetic() {
    let mut payload = vec![0x49, 0x02, 0x01];
    payload.extend_from_slice(b"1KV1TUNER00000001"); // exactly 17 chars
    let vin = decode_vin(&payload).expect("Decode VIN failed");
    assert_eq!(vin, "1KV1TUNER00000001");

    // Truncated payload (16 chars)
    let mut truncated = vec![0x49, 0x02, 0x01];
    truncated.extend_from_slice(b"1KV1TUNER0000000");
    assert!(matches!(
        decode_vin(&truncated),
        Err(ObdCodecError::TruncatedPayload { .. })
    ));

    // Wrong service ID
    let mut wrong_srv = vec![0x41, 0x02, 0x01];
    wrong_srv.extend_from_slice(b"1KV1TUNER00000001");
    assert!(matches!(
        decode_vin(&wrong_srv),
        Err(ObdCodecError::UnexpectedService { .. })
    ));
}

#[test]
fn test_decode_mode09_calibration_id() {
    let mut payload = vec![0x49, 0x04, 0x01];
    payload.extend_from_slice(b"38770-KV1-0000");
    let cal_id = decode_calibration_id(&payload).expect("Decode Cal ID failed");
    assert_eq!(cal_id, "38770-KV1-0000");
}
