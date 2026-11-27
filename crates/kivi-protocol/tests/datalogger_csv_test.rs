use kivi_protocol::datalogger::{
    flags_from_hex, flags_to_hex, parse_session_from_reader, parse_session_from_str,
    serialize_session_to_string, DatalogSession, SessionMetadata,
};
use kivi_protocol::{EcuIdentity, ProtocolError, TelemetryFrame, TelemetryStatusFlags};
use std::io::Cursor;

fn make_valid_frame(seq: u64, timestamp_ms: u64) -> TelemetryFrame {
    TelemetryFrame {
        timestamp_ms,
        sequence: seq,
        is_simulated: false,
        rpm: 3200.0 + (seq as f32 * 100.0),
        tps: 45.0,
        map_kpa: 105.0,
        ect_c: 88.0,
        iat_c: 32.0,
        battery_volts: 13.85,
        ignition_advance_deg: 24.5,
        injector_pulse_width_ms: 6.25,
        afr: 14.20,
        speed_kph: 85.0,
        gear: 3,
        status_flags: TelemetryStatusFlags::default(),
    }
}

fn make_valid_session() -> DatalogSession {
    let mut metadata = SessionMetadata::new("ses-20261024-001", "2026-10-24T18:00:00Z");
    metadata.ended_at_iso = Some("2026-10-24T18:01:30Z".to_string());
    metadata.ecu_identity = Some(EcuIdentity {
        ecu_id: "SIM-BENCH-01".to_string(),
        vin: None,
        software_version: String::new(),
        hardware_version: String::new(),
        protocol_name: "VirtualBench".to_string(),
        is_simulated: false,
    });
    metadata.sample_rate_hz = 20.0;
    metadata.total_samples_recorded = 5;
    metadata.dropped_samples_count = 1;
    metadata.notes = "Dyno Pull 3rd Gear WOT".to_string();

    let mut frames = Vec::new();
    for i in 0..5 {
        let mut frame = make_valid_frame(i, i * 50);
        if i == 1 {
            frame.status_flags.check_engine_light = true;
        } else if i == 2 {
            frame.status_flags.limp_mode = true;
            frame.status_flags.overheating = true;
        } else if i == 3 {
            frame.status_flags.low_voltage = true;
            frame.status_flags.communication_fault = true;
        } else if i == 4 {
            frame.status_flags.stale_sample = true;
        }
        frames.push(frame);
    }

    DatalogSession::new(metadata, frames)
}

#[test]
fn test_fixture_a_lossless_roundtrip_lf_and_crlf() {
    let original = make_valid_session();

    // Serialize to CSV string (LF)
    let csv_out = serialize_session_to_string(&original).expect("Serialization failed");
    assert!(csv_out.starts_with("# kiviTuner Datalog Session v1.0\n"));
    assert!(csv_out.contains("# session_id: ses-20261024-001\n"));
    assert!(csv_out.contains("# started_at: 2026-10-24T18:00:00Z\n"));
    assert!(csv_out.contains("# ended_at: 2026-10-24T18:01:30Z\n"));
    assert!(csv_out.contains("# ecu_id: SIM-BENCH-01\n"));
    assert!(csv_out.contains("# protocol: VirtualBench\n"));
    assert!(csv_out.contains("# sample_rate_hz: 20.0\n"));
    assert!(csv_out.contains("# total_samples: 5\n"));
    assert!(csv_out.contains("# dropped_samples: 1\n"));
    assert!(csv_out.contains("# notes: Dyno Pull 3rd Gear WOT\n"));
    assert!(csv_out.contains("timestamp_ms,sequence,rpm,tps,map_kpa,ect_c,iat_c,battery_volts,ignition_advance_deg,injector_pulse_width_ms,afr,speed_kph,gear,status_flags\n"));

    // Parse back from LF
    let parsed = parse_session_from_str(&csv_out).expect("Parse LF failed");

    // Metadata assertions
    assert_eq!(parsed.metadata.session_id, original.metadata.session_id);
    assert_eq!(parsed.metadata.started_at_iso, original.metadata.started_at_iso);
    assert_eq!(parsed.metadata.ended_at_iso, original.metadata.ended_at_iso);
    assert_eq!(parsed.metadata.schema_version, 1);
    assert_eq!(
        parsed.metadata.ecu_identity.as_ref().map(|e| e.ecu_id.as_str()),
        Some("SIM-BENCH-01")
    );
    assert_eq!(
        parsed.metadata.ecu_identity.as_ref().map(|e| e.protocol_name.as_str()),
        Some("VirtualBench")
    );
    assert!((parsed.metadata.sample_rate_hz - original.metadata.sample_rate_hz).abs() < 1e-4);
    assert_eq!(parsed.metadata.total_samples_recorded, 5);
    assert_eq!(parsed.metadata.dropped_samples_count, 1);
    assert_eq!(parsed.metadata.notes, "Dyno Pull 3rd Gear WOT");

    // Frames assertions
    assert_eq!(parsed.frames.len(), original.frames.len());
    for (i, (f_orig, f_parsed)) in original.frames.iter().zip(parsed.frames.iter()).enumerate() {
        assert_eq!(f_parsed.timestamp_ms, f_orig.timestamp_ms, "Frame {} timestamp", i);
        assert_eq!(f_parsed.sequence, f_orig.sequence, "Frame {} seq", i);
        assert!((f_parsed.rpm - f_orig.rpm).abs() < 0.15, "Frame {} rpm", i);
        assert!((f_parsed.tps - f_orig.tps).abs() < 0.15, "Frame {} tps", i);
        assert!((f_parsed.map_kpa - f_orig.map_kpa).abs() < 0.15, "Frame {} map", i);
        assert!((f_parsed.ect_c - f_orig.ect_c).abs() < 0.15, "Frame {} ect", i);
        assert!((f_parsed.iat_c - f_orig.iat_c).abs() < 0.15, "Frame {} iat", i);
        assert!((f_parsed.battery_volts - f_orig.battery_volts).abs() < 0.02, "Frame {} battery", i);
        assert!((f_parsed.ignition_advance_deg - f_orig.ignition_advance_deg).abs() < 0.15, "Frame {} ign", i);
        assert!((f_parsed.injector_pulse_width_ms - f_orig.injector_pulse_width_ms).abs() < 0.02, "Frame {} ipw", i);
        assert!((f_parsed.afr - f_orig.afr).abs() < 0.02, "Frame {} afr", i);
        assert!((f_parsed.speed_kph - f_orig.speed_kph).abs() < 0.15, "Frame {} speed", i);
        assert_eq!(f_parsed.gear, f_orig.gear, "Frame {} gear", i);
        assert_eq!(f_parsed.status_flags, f_orig.status_flags, "Frame {} flags", i);
    }

    // Now test CRLF input: replace '\n' with '\r\n'
    let csv_crlf = csv_out.replace('\n', "\r\n");
    let parsed_crlf = parse_session_from_str(&csv_crlf).expect("Parse CRLF failed");
    assert_eq!(parsed_crlf.frames.len(), original.frames.len());
    assert_eq!(parsed_crlf.metadata.notes, original.metadata.notes);
}

#[test]
fn test_fixture_b_boundary_values_pass_validation() {
    let mut metadata = SessionMetadata::new("ses-boundary", "2026-10-24T12:00:00Z");
    metadata.sample_rate_hz = 10.0;
    metadata.total_samples_recorded = 2;

    let row_min = TelemetryFrame {
        timestamp_ms: 0,
        sequence: 0,
        is_simulated: false,
        rpm: 0.0,
        tps: 0.0,
        map_kpa: 10.0,
        ect_c: -40.0,
        iat_c: -40.0,
        battery_volts: 6.0,
        ignition_advance_deg: -20.0,
        injector_pulse_width_ms: 0.0,
        afr: 8.0,
        speed_kph: 0.0,
        gear: 0,
        status_flags: TelemetryStatusFlags::default(),
    };

    let row_max = TelemetryFrame {
        timestamp_ms: 100,
        sequence: 1,
        is_simulated: false,
        rpm: 20000.0,
        tps: 100.0,
        map_kpa: 350.0,
        ect_c: 150.0,
        iat_c: 120.0,
        battery_volts: 18.0,
        ignition_advance_deg: 60.0,
        injector_pulse_width_ms: 50.0,
        afr: 25.0,
        speed_kph: 400.0,
        gear: 8,
        status_flags: TelemetryStatusFlags {
            check_engine_light: true,
            limp_mode: true,
            low_voltage: true,
            overheating: true,
            stale_sample: true,
            communication_fault: true,
        },
    };

    let session = DatalogSession::new(metadata, vec![row_min, row_max]);
    let csv = serialize_session_to_string(&session).expect("Boundary serialize failed");
    let parsed = parse_session_from_str(&csv).expect("Boundary parse failed");

    assert_eq!(parsed.frames.len(), 2);
    assert_eq!(parsed.frames[0].rpm, 0.0);
    assert_eq!(parsed.frames[0].battery_volts, 6.0);
    assert_eq!(parsed.frames[1].rpm, 20000.0);
    assert_eq!(parsed.frames[1].battery_volts, 18.0);
}

#[test]
fn test_fixture_c_rejection_of_invalid_magic_version() {
    // Unsupported schema version v2.0
    let csv_v2 = "# kiviTuner Datalog Session v2.0\n# session_id: s1\ntimestamp_ms,sequence,rpm,tps,map_kpa,ect_c,iat_c,battery_volts,ignition_advance_deg,injector_pulse_width_ms,afr,speed_kph,gear,status_flags\n";
    let err_v2 = parse_session_from_str(csv_v2).unwrap_err();
    match err_v2 {
        ProtocolError::InvalidFormat(msg) => {
            assert!(
                msg.contains("Unsupported CSV schema version: expected v1.0"),
                "Expected version error msg, got: {}",
                msg
            );
        }
        other => panic!("Expected ProtocolError::InvalidFormat, got: {:?}", other),
    }

    // Missing magic header entirely
    let csv_raw = "timestamp_ms,sequence,rpm,tps,map_kpa,ect_c,iat_c,battery_volts,ignition_advance_deg,injector_pulse_width_ms,afr,speed_kph,gear,status_flags\n";
    let err_raw = parse_session_from_str(csv_raw).unwrap_err();
    assert!(matches!(err_raw, ProtocolError::InvalidFormat(_)));
}

#[test]
fn test_fixture_d_column_count_mismatch_and_non_numeric_floats() {
    let base_header = "# kiviTuner Datalog Session v1.0\n# session_id: s1\n# started_at: 2026-10-24T18:00:00Z\n# ended_at:\n# ecu_id:\n# protocol:\n# sample_rate_hz: 20.0\n# total_samples: 1\n# dropped_samples: 0\n# notes:\ntimestamp_ms,sequence,rpm,tps,map_kpa,ect_c,iat_c,battery_volts,ignition_advance_deg,injector_pulse_width_ms,afr,speed_kph,gear,status_flags\n";

    // Line 12 has 13 columns (missing status_flags)
    let csv_missing_col = format!("{}100,1,3000.0,40.0,100.0,80.0,30.0,13.5,20.0,5.0,14.7,60.0,3\n", base_header);
    let err_col = parse_session_from_str(&csv_missing_col).unwrap_err();
    match err_col {
        ProtocolError::InvalidFormat(msg) => {
            assert!(msg.contains("Line 12"), "Error should report line 12, got: {}", msg);
            assert!(msg.contains("14 columns"), "Error should report column count, got: {}", msg);
        }
        other => panic!("Expected InvalidFormat, got {:?}", other),
    }

    // Line 12 has non-numeric float for 'rpm'
    let csv_bad_rpm = format!("{}100,1,abc,40.0,100.0,80.0,30.0,13.5,20.0,5.0,14.7,60.0,3,0x00\n", base_header);
    let err_rpm = parse_session_from_str(&csv_bad_rpm).unwrap_err();
    match err_rpm {
        ProtocolError::InvalidFormat(msg) => {
            assert!(msg.contains("Line 12"), "Error should report line 12, got: {}", msg);
            assert!(msg.contains("rpm"), "Error should mention 'rpm', got: {}", msg);
        }
        other => panic!("Expected InvalidFormat, got {:?}", other),
    }

    // Line 12 has non-numeric integer for 'gear'
    let csv_bad_gear = format!("{}100,1,3000.0,40.0,100.0,80.0,30.0,13.5,20.0,5.0,14.7,60.0,three,0x00\n", base_header);
    let err_gear = parse_session_from_str(&csv_bad_gear).unwrap_err();
    match err_gear {
        ProtocolError::InvalidFormat(msg) => {
            assert!(msg.contains("Line 12"), "Error should report line 12, got: {}", msg);
            assert!(msg.contains("gear"), "Error should mention 'gear', got: {}", msg);
        }
        other => panic!("Expected InvalidFormat, got {:?}", other),
    }

    // Line 12 has invalid hex for status_flags
    let csv_bad_hex = format!("{}100,1,3000.0,40.0,100.0,80.0,30.0,13.5,20.0,5.0,14.7,60.0,3,0xZZ\n", base_header);
    let err_hex = parse_session_from_str(&csv_bad_hex).unwrap_err();
    match err_hex {
        ProtocolError::InvalidFormat(msg) => {
            assert!(msg.contains("Line 12"), "Error should report line 12, got: {}", msg);
            assert!(msg.contains("status_flags"), "Error should mention 'status_flags', got: {}", msg);
        }
        other => panic!("Expected InvalidFormat, got {:?}", other),
    }
}

#[test]
fn test_fixture_e_sensor_out_of_range_rejection() {
    let base_header = "# kiviTuner Datalog Session v1.0\n# session_id: s1\n# started_at: 2026-10-24T18:00:00Z\n# ended_at:\n# ecu_id:\n# protocol:\n# sample_rate_hz: 20.0\n# total_samples: 1\n# dropped_samples: 0\n# notes:\ntimestamp_ms,sequence,rpm,tps,map_kpa,ect_c,iat_c,battery_volts,ignition_advance_deg,injector_pulse_width_ms,afr,speed_kph,gear,status_flags\n";

    // Row contains RPM = 25000.0 (max is 20000.0)
    let csv_rpm = format!("{}100,1,25000.0,40.0,100.0,80.0,30.0,13.5,20.0,5.0,14.7,60.0,3,0x00\n", base_header);
    let err_rpm = parse_session_from_str(&csv_rpm).unwrap_err();
    match err_rpm {
        ProtocolError::InvalidFormat(msg) => {
            assert!(msg.contains("Line 12"), "Got: {}", msg);
            assert!(msg.contains("RPM"), "Got: {}", msg);
        }
        other => panic!("Expected InvalidFormat, got {:?}", other),
    }

    // Row contains AFR = 4.0 (min is 8.0)
    let csv_afr = format!("{}100,1,3000.0,40.0,100.0,80.0,30.0,13.5,20.0,5.0,4.0,60.0,3,0x00\n", base_header);
    let err_afr = parse_session_from_str(&csv_afr).unwrap_err();
    match err_afr {
        ProtocolError::InvalidFormat(msg) => {
            assert!(msg.contains("Line 12"), "Got: {}", msg);
            assert!(msg.contains("AFR"), "Got: {}", msg);
        }
        other => panic!("Expected InvalidFormat, got {:?}", other),
    }

    // Row contains Battery = 4.5V (min is 6.0)
    let csv_bat = format!("{}100,1,3000.0,40.0,100.0,80.0,30.0,4.5,20.0,5.0,14.7,60.0,3,0x00\n", base_header);
    let err_bat = parse_session_from_str(&csv_bat).unwrap_err();
    match err_bat {
        ProtocolError::InvalidFormat(msg) => {
            assert!(msg.contains("Line 12"), "Got: {}", msg);
            assert!(msg.contains("Battery"), "Got: {}", msg);
        }
        other => panic!("Expected InvalidFormat, got {:?}", other),
    }
}

#[test]
fn test_fixture_f_timestamp_non_monotonic_rollback_rejection() {
    let base_header = "# kiviTuner Datalog Session v1.0\n# session_id: s1\n# started_at: 2026-10-24T18:00:00Z\n# ended_at:\n# ecu_id:\n# protocol:\n# sample_rate_hz: 20.0\n# total_samples: 5\n# dropped_samples: 0\n# notes:\ntimestamp_ms,sequence,rpm,tps,map_kpa,ect_c,iat_c,battery_volts,ignition_advance_deg,injector_pulse_width_ms,afr,speed_kph,gear,status_flags\n";

    let mut body = String::from(base_header);
    // Rows 1-4 (lines 12-15) have timestamps 50, 100, 150, 200
    body.push_str("50,1,3000.0,40.0,100.0,80.0,30.0,13.5,20.0,5.0,14.7,60.0,3,0x00\n"); // Line 12
    body.push_str("100,2,3050.0,40.0,100.0,80.0,30.0,13.5,20.0,5.0,14.7,60.0,3,0x00\n"); // Line 13
    body.push_str("150,3,3100.0,40.0,100.0,80.0,30.0,13.5,20.0,5.0,14.7,60.0,3,0x00\n"); // Line 14
    body.push_str("200,4,3150.0,40.0,100.0,80.0,30.0,13.5,20.0,5.0,14.7,60.0,3,0x00\n"); // Line 15
    // Row 5 (line 16) rolls back to timestamp 120 (< 200)
    body.push_str("120,5,3200.0,40.0,100.0,80.0,30.0,13.5,20.0,5.0,14.7,60.0,3,0x00\n"); // Line 16

    let err = parse_session_from_str(&body).unwrap_err();
    match err {
        ProtocolError::InvalidFormat(msg) => {
            assert!(msg.contains("Line 16"), "Should report rollback at Line 16, got: {}", msg);
            assert!(
                msg.contains("non-monotonic") || msg.contains("rollback"),
                "Should indicate non-monotonic rollback, got: {}",
                msg
            );
        }
        other => panic!("Expected InvalidFormat, got {:?}", other),
    }
}

#[test]
fn test_flags_hex_encoding_decoding_and_reserved_bits() {
    let mut flags = TelemetryStatusFlags::default();
    assert_eq!(flags_to_hex(&flags), "0x00");

    flags.check_engine_light = true;
    assert_eq!(flags_to_hex(&flags), "0x01");

    flags.limp_mode = true;
    assert_eq!(flags_to_hex(&flags), "0x03");

    flags.low_voltage = true;
    assert_eq!(flags_to_hex(&flags), "0x07");

    flags.overheating = true;
    assert_eq!(flags_to_hex(&flags), "0x0F");

    flags.stale_sample = true;
    assert_eq!(flags_to_hex(&flags), "0x1F");

    flags.communication_fault = true;
    assert_eq!(flags_to_hex(&flags), "0x3F");

    // Decode standard
    let decoded = flags_from_hex("0x3F").expect("decode 0x3F");
    assert_eq!(decoded, flags);

    // Reserved bits 6-7 (0xC0 | 0x01 = 0xC1) must be ignored on parse
    let with_reserved = flags_from_hex("0xC1").expect("decode 0xC1");
    assert!(with_reserved.check_engine_light);
    assert!(!with_reserved.limp_mode);

    // Case insensitivity and prefix tolerance
    let lower = flags_from_hex("0x0f").expect("decode 0x0f");
    assert!(lower.check_engine_light && lower.limp_mode && lower.low_voltage && lower.overheating);

    let no_prefix = flags_from_hex("05").expect("decode 05");
    assert!(no_prefix.check_engine_light && no_prefix.low_voltage);
}

#[test]
fn test_bom_tolerance_on_import() {
    let original = make_valid_session();
    let csv_out = serialize_session_to_string(&original).expect("serialize");
    // Prepend UTF-8 BOM
    let with_bom = format!("\u{FEFF}{}", csv_out);
    let parsed = parse_session_from_str(&with_bom).expect("parse with BOM");
    assert_eq!(parsed.metadata.session_id, original.metadata.session_id);
    assert_eq!(parsed.frames.len(), original.frames.len());
}

#[test]
fn test_payload_size_limit_rejection() {
    // Construct a reader that reports more than 50 MB
    // We can simulate this by feeding a buffer that exceeds 50MB or by testing the reader loop.
    // 50 MB = 50 * 1024 * 1024 = 52_428_800 bytes
    // Rather than allocating 50MB in RAM, we can make a repeating Cursor or a large header:
    let magic = "# kiviTuner Datalog Session v1.0\n# notes: ";
    let padding = "A".repeat(52_428_810);
    let large_csv = format!("{}{}\n", magic, padding);

    let err = parse_session_from_reader(Cursor::new(large_csv)).unwrap_err();
    match err {
        ProtocolError::InvalidFormat(msg) => {
            assert!(msg.contains("50 MB"), "Expected 50 MB limit error, got: {}", msg);
        }
        other => panic!("Expected InvalidFormat, got {:?}", other),
    }
}

struct SyntheticRowStream {
    line_index: usize,
    max_lines: usize,
    current_buf: Vec<u8>,
    pos_in_buf: usize,
}

impl SyntheticRowStream {
    fn new(total_rows: usize) -> Self {
        Self {
            line_index: 0,
            max_lines: total_rows + 2, // header lines + rows
            current_buf: Vec::new(),
            pos_in_buf: 0,
        }
    }
}

impl std::io::Read for SyntheticRowStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.pos_in_buf >= self.current_buf.len() {
            if self.line_index >= self.max_lines {
                return Ok(0); // EOF
            }
            let next_line = if self.line_index == 0 {
                "# kiviTuner Datalog Session v1.0\n".to_string()
            } else if self.line_index == 1 {
                "timestamp_ms,sequence,rpm,tps,map_kpa,ect_c,iat_c,battery_volts,ignition_advance_deg,injector_pulse_width_ms,afr,speed_kph,gear,status_flags\n".to_string()
            } else {
                let row = self.line_index - 1;
                format!(
                    "{},{},3000.0,40.0,100.0,80.0,30.0,13.5,20.0,5.0,14.7,60.0,3,0x00\n",
                    row * 10,
                    row
                )
            };
            self.current_buf = next_line.into_bytes();
            self.pos_in_buf = 0;
            self.line_index += 1;
        }

        let available = self.current_buf.len() - self.pos_in_buf;
        let to_copy = available.min(buf.len());
        buf[..to_copy].copy_from_slice(&self.current_buf[self.pos_in_buf..self.pos_in_buf + to_copy]);
        self.pos_in_buf += to_copy;
        Ok(to_copy)
    }
}

impl std::io::BufRead for SyntheticRowStream {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        if self.pos_in_buf >= self.current_buf.len() {
            if self.line_index >= self.max_lines {
                return Ok(&[]);
            }
            let next_line = if self.line_index == 0 {
                "# kiviTuner Datalog Session v1.0\n".to_string()
            } else if self.line_index == 1 {
                "timestamp_ms,sequence,rpm,tps,map_kpa,ect_c,iat_c,battery_volts,ignition_advance_deg,injector_pulse_width_ms,afr,speed_kph,gear,status_flags\n".to_string()
            } else {
                let row = self.line_index - 1;
                format!(
                    "{},{},3000.0,40.0,100.0,80.0,30.0,13.5,20.0,5.0,14.7,60.0,3,0x00\n",
                    row * 10,
                    row
                )
            };
            self.current_buf = next_line.into_bytes();
            self.pos_in_buf = 0;
            self.line_index += 1;
        }
        Ok(&self.current_buf[self.pos_in_buf..])
    }

    fn consume(&mut self, amt: usize) {
        self.pos_in_buf = (self.pos_in_buf + amt).min(self.current_buf.len());
    }
}

#[test]
fn test_max_rows_limit_rejection() {
    // 500_001 rows should trigger row limit
    let stream = SyntheticRowStream::new(500_001);
    let err = parse_session_from_reader(stream).unwrap_err();
    match err {
        ProtocolError::InvalidFormat(msg) => {
            assert!(
                msg.contains("500,000 rows"),
                "Expected 500,000 rows limit error, got: {}",
                msg
            );
        }
        other => panic!("Expected InvalidFormat, got {:?}", other),
    }
}

#[test]
fn test_csv_roundtrip_with_unmeasured_afr_nan() {
    let mut metadata = SessionMetadata::new("ses-unmeasured-afr", "2026-10-26T10:00:00Z");
    metadata.sample_rate_hz = 10.0;
    metadata.total_samples_recorded = 3;

    // Frame 0: Measured AFR 14.7
    let mut f0 = make_valid_frame(0, 0);
    f0.afr = 14.7;

    // Frame 1: Unmeasured AFR (f32::NAN)
    let mut f1 = make_valid_frame(1, 100);
    f1.afr = f32::NAN;

    // Frame 2: Measured AFR 12.5
    let mut f2 = make_valid_frame(2, 200);
    f2.afr = 12.5;

    let original = DatalogSession::new(metadata, vec![f0, f1, f2]);

    // Serialize to CSV string
    let csv_str = serialize_session_to_string(&original).expect("Serialization with NaN AFR failed");

    // Confirm CSV has NaN in the afr column
    assert!(
        csv_str.contains(",NaN,") || csv_str.contains(",nan,"),
        "Serialized CSV should contain NaN in afr column"
    );

    // Parse back from CSV
    let parsed = parse_session_from_str(&csv_str).expect("Parse session with NaN AFR failed");
    assert_eq!(parsed.frames.len(), 3);

    // Frame 0: 14.7
    assert!((parsed.frames[0].afr - 14.7).abs() < 0.05);

    // Frame 1: must be NaN (unavailable/unmeasured, not zero or fabricated)
    assert!(
        parsed.frames[1].afr.is_nan(),
        "Unmeasured AFR must remain NaN after import"
    );

    // Frame 2: 12.5
    assert!((parsed.frames[2].afr - 12.5).abs() < 0.05);
}

#[test]
fn test_csv_rejects_infinite_afr_and_nan_in_other_fields() {
    let base_header = "# kiviTuner Datalog Session v1.0\n# session_id: s1\n# started_at: 2026-10-26T10:00:00Z\n# ended_at:\n# ecu_id:\n# protocol:\n# sample_rate_hz: 20.0\n# total_samples: 1\n# dropped_samples: 0\n# notes:\ntimestamp_ms,sequence,rpm,tps,map_kpa,ect_c,iat_c,battery_volts,ignition_advance_deg,injector_pulse_width_ms,afr,speed_kph,gear,status_flags\n";

    // 1. AFR = inf rejected
    let csv_inf_afr = format!(
        "{}100,1,3000.0,40.0,100.0,80.0,30.0,13.5,20.0,5.0,inf,60.0,3,0x00\n",
        base_header
    );
    let err_inf = parse_session_from_str(&csv_inf_afr).unwrap_err();
    assert!(matches!(err_inf, ProtocolError::InvalidFormat(_)));
    if let ProtocolError::InvalidFormat(msg) = err_inf {
        assert!(msg.contains("AFR"), "Expected AFR error, got: {}", msg);
    }

    // 2. RPM = NaN rejected by validate_telemetry_frame
    let csv_nan_rpm = format!(
        "{}100,1,NaN,40.0,100.0,80.0,30.0,13.5,20.0,5.0,14.7,60.0,3,0x00\n",
        base_header
    );
    let err_nan_rpm = parse_session_from_str(&csv_nan_rpm).unwrap_err();
    assert!(matches!(err_nan_rpm, ProtocolError::InvalidFormat(_)));
    if let ProtocolError::InvalidFormat(msg) = err_nan_rpm {
        assert!(msg.contains("RPM"), "Expected RPM error, got: {}", msg);
    }
}
