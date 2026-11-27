use kivi_protocol::obd::adapter::parse_adapter_response;
use kivi_protocol::obd::classification::{classify_command, CommandClassification};
use kivi_protocol::obd::command::{AdapterCommand, Mode01Pid, Mode09Item, ObdCommand};
use kivi_protocol::obd::errors::ObdCodecError;
use kivi_protocol::obd::pid::WidebandO2Sample;
use kivi_protocol::obd::session::is_recognized_adapter_banner;


// ===========================================================================
// SECTION 3: ISO-TP Sequence Rollover and Bounds Tests
// ===========================================================================

#[test]
fn test_isotp_modulo16_sequence_rollover_success() {
    // Construct synthetic 18-frame message that wraps from sequence 15 (0xF) to sequence 0
    // Declared length: 18 frames * 7 bytes = 126 bytes = 0x07E
    let mut raw = String::from("07E\r\n");
    for i in 0..18 {
        let seq = i % 16;
        raw.push_str(&format!("{:X}: 49 04 01 31 32 33 34\r\n", seq));
    }
    raw.push_str("\r\n>");

    let payload = parse_adapter_response(&raw).expect("Modulo-16 sequence rollover failed");
    assert_eq!(payload.len(), 126);
}

#[test]
fn test_isotp_oversized_declared_length_rejected() {
    // Declared length exceeding standard 4095-byte ISO-TP buffer limit (e.g. 0x1000 = 4096)
    let raw = "1000\r\n0: 49 04 01 31 32 33 34\r\n>";
    let err = parse_adapter_response(raw).unwrap_err();
    assert!(matches!(
        err,
        ObdCodecError::OversizedPayload {
            expected: 4095,
            actual: 4096
        }
    ));
}

// ===========================================================================
// SECTION 4: Stoichiometric Ratio Input Validation Tests
// ===========================================================================

#[test]
fn test_wideband_estimated_afr_rejects_invalid_stoich_ratios() {
    let sample = WidebandO2Sample::new(1.0, 0.0);

    // Valid inputs
    assert!((sample.estimated_afr(14.7).unwrap() - 14.7).abs() < 0.001);
    assert!((sample.estimated_afr(14.08).unwrap() - 14.08).abs() < 0.001);

    // Zero
    assert!(matches!(
        sample.estimated_afr(0.0),
        Err(ObdCodecError::InvalidStoichiometricRatio(_))
    ));

    // Negative
    assert!(matches!(
        sample.estimated_afr(-14.7),
        Err(ObdCodecError::InvalidStoichiometricRatio(_))
    ));

    // NaN
    assert!(matches!(
        sample.estimated_afr(f32::NAN),
        Err(ObdCodecError::InvalidStoichiometricRatio(_))
    ));

    // Infinity
    assert!(matches!(
        sample.estimated_afr(f32::INFINITY),
        Err(ObdCodecError::InvalidStoichiometricRatio(_))
    ));
    assert!(matches!(
        sample.estimated_afr(f32::NEG_INFINITY),
        Err(ObdCodecError::InvalidStoichiometricRatio(_))
    ));

    // Invalid equivalence ratio inside sample (e.g. non-positive or NaN)
    let invalid_sample = WidebandO2Sample::new(f32::NAN, 0.0);
    assert!(matches!(
        invalid_sample.estimated_afr(14.7),
        Err(ObdCodecError::InvalidStoichiometricRatio(_))
    ));
}


#[test]
fn test_command_classification_all_variants_categorized() {
    // Mode 01 requests -> EcuDiagnosticRequest
    assert_eq!(
        classify_command(&ObdCommand::Mode01(Mode01Pid::EngineRpm)),
        CommandClassification::EcuDiagnosticRequest
    );

    // Mode 09 requests -> EcuDiagnosticRequest
    assert_eq!(
        classify_command(&ObdCommand::Mode09(Mode09Item::Vin)),
        CommandClassification::EcuDiagnosticRequest
    );

    // Adapter reset -> AdapterReset
    assert_eq!(
        classify_command(&ObdCommand::Adapter(AdapterCommand::Reset)),
        CommandClassification::AdapterReset
    );

    // Adapter config -> AdapterConfiguration
    assert_eq!(
        classify_command(&ObdCommand::Adapter(AdapterCommand::EchoOff)),
        CommandClassification::AdapterConfiguration
    );
    assert_eq!(
        classify_command(&ObdCommand::Adapter(AdapterCommand::SetProtocolCan11_500)),
        CommandClassification::AdapterConfiguration
    );

    // Adapter identity -> AdapterIdentification
    assert_eq!(
        classify_command(&ObdCommand::Adapter(AdapterCommand::QueryElmVersion)),
        CommandClassification::AdapterIdentification
    );
    assert_eq!(
        classify_command(&ObdCommand::Adapter(AdapterCommand::QueryStnDeviceId)),
        CommandClassification::AdapterIdentification
    );
    assert_eq!(
        classify_command(&ObdCommand::Adapter(AdapterCommand::ReadVoltage)),
        CommandClassification::AdapterIdentification
    );
}

#[test]
fn test_adapter_banner_grammar_strict_validation() {
    // Valid ELM banners
    assert!(is_recognized_adapter_banner("ELM327 v1.5"));
    assert!(is_recognized_adapter_banner("ELM327 v1.4b"));
    assert!(is_recognized_adapter_banner("ELM327 v2.1"));
    assert!(is_recognized_adapter_banner("ELM327 v1.3a"));
    assert!(is_recognized_adapter_banner("ELM 327 v1.5"));
    assert!(is_recognized_adapter_banner("ELM v1.5"));
    assert!(is_recognized_adapter_banner("elm327 v1.5"));

    // Valid STN banners
    assert!(is_recognized_adapter_banner("STN1130 v3.3.0"));
    assert!(is_recognized_adapter_banner("STN1170 v3.3.0"));
    assert!(is_recognized_adapter_banner("STN2120 v4.2.1"));
    assert!(is_recognized_adapter_banner("STN1110 v2.5"));
    assert!(is_recognized_adapter_banner("stn1130 v3.3.0"));

    // Valid OBDLink banners
    assert!(is_recognized_adapter_banner("OBDLink SX v4.2.1"));
    assert!(is_recognized_adapter_banner("OBDLink EX v4.2.1"));
    assert!(is_recognized_adapter_banner("OBDLink MX+ v5.2"));
    assert!(is_recognized_adapter_banner("OBDLink LX v4.2.1"));
    assert!(is_recognized_adapter_banner("OBDLink v4.2.1"));
    assert!(is_recognized_adapter_banner("obdlink sx 4.2.1"));

    // Strictly rejected malformed lookalikes and garbage
    assert!(!is_recognized_adapter_banner("ELM_CORRUPTED"));
    assert!(!is_recognized_adapter_banner("ELM327_INVALID"));
    assert!(!is_recognized_adapter_banner("UNSUPPORTED STN11 DEVICE"));
    assert!(!is_recognized_adapter_banner("NOT AN OBDLINK ADAPTER"));
    assert!(!is_recognized_adapter_banner("OBDLINK_FAKE"));
    assert!(!is_recognized_adapter_banner("ELM327"));
    assert!(!is_recognized_adapter_banner("ELM"));
    assert!(!is_recognized_adapter_banner("STN11"));
    assert!(!is_recognized_adapter_banner("STN"));
    assert!(!is_recognized_adapter_banner("OBDLINK"));
    assert!(!is_recognized_adapter_banner("OBDLink SX"));
    assert!(!is_recognized_adapter_banner("41 0C 1A F8"));
    assert!(!is_recognized_adapter_banner("OK"));
    assert!(!is_recognized_adapter_banner("?"));
    assert!(!is_recognized_adapter_banner("CAN ERROR"));
    assert!(!is_recognized_adapter_banner("BUS BUSY"));
    assert!(!is_recognized_adapter_banner("BUFFER FULL"));
    assert!(!is_recognized_adapter_banner("STOPPED"));
    assert!(!is_recognized_adapter_banner(""));
    assert!(!is_recognized_adapter_banner("   "));
    assert!(!is_recognized_adapter_banner("ELM327 INVALID_VERSION"));
    assert!(!is_recognized_adapter_banner("STN_UNKNOWN v1.0"));

    // Version-token boundary edge cases (consecutive letters, dots, malformed tokens)
    assert!(!is_recognized_adapter_banner("ELM327 v1abc"));
    assert!(!is_recognized_adapter_banner("ELM327 v1.5.."));
    assert!(!is_recognized_adapter_banner("ELM327 v1.5."));
    assert!(!is_recognized_adapter_banner("ELM327 .1.5"));
    assert!(!is_recognized_adapter_banner("ELM327 1..5"));
    assert!(!is_recognized_adapter_banner("ELM327 v1.4abc"));
    assert!(!is_recognized_adapter_banner("ELM327 v1.5_INVALID"));
    assert!(!is_recognized_adapter_banner("ELM327 v.."));
    assert!(!is_recognized_adapter_banner("ELM327 v"));
    assert!(!is_recognized_adapter_banner("OBDLink SX v1abc"));
    assert!(!is_recognized_adapter_banner("OBDLink SX v1.5.."));
    assert!(!is_recognized_adapter_banner("STN1130 v1abc"));
    assert!(!is_recognized_adapter_banner("STN1130 v1.5.."));
}

