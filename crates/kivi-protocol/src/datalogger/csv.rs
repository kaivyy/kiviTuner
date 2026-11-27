//! Versioned CSV v1.0 serializer and strict validating parser for datalog sessions.

use std::io::{BufRead, Write};

use crate::errors::ProtocolError;
use crate::model::{EcuIdentity, TelemetryFrame, TelemetryStatusFlags};
use crate::validation::validate_telemetry_frame;

use super::session::{DatalogSession, SessionMetadata};

/// Schema magic header identifier for v1.0 datalog sessions.
pub const MAGIC_HEADER: &str = "# kiviTuner Datalog Session v1.0";

/// Column header row for v1.0 datalog telemetry frame tabular data.
pub const COLUMN_HEADER: &str =
    "timestamp_ms,sequence,rpm,tps,map_kpa,ect_c,iat_c,battery_volts,ignition_advance_deg,injector_pulse_width_ms,afr,speed_kph,gear,status_flags";

/// Maximum allowed payload size for CSV ingestion (50 MB).
pub const MAX_PAYLOAD_BYTES: usize = 50 * 1024 * 1024;

/// Maximum allowed telemetry data rows in a single session (500,000 samples).
pub const MAX_DATA_ROWS: usize = 500_000;

/// Encodes `TelemetryStatusFlags` into a 1-byte hex string format (`0xNN`).
///
/// Bit layout:
/// - Bit 0 (`0x01`): `check_engine_light`
/// - Bit 1 (`0x02`): `limp_mode`
/// - Bit 2 (`0x04`): `low_voltage`
/// - Bit 3 (`0x08`): `overheating`
/// - Bit 4 (`0x10`): `stale_sample`
/// - Bit 5 (`0x20`): `communication_fault`
/// - Bits 6-7: reserved (written as 0)
pub fn flags_to_hex(flags: &TelemetryStatusFlags) -> String {
    let mut byte: u8 = 0;
    if flags.check_engine_light {
        byte |= 0x01;
    }
    if flags.limp_mode {
        byte |= 0x02;
    }
    if flags.low_voltage {
        byte |= 0x04;
    }
    if flags.overheating {
        byte |= 0x08;
    }
    if flags.stale_sample {
        byte |= 0x10;
    }
    if flags.communication_fault {
        byte |= 0x20;
    }
    format!("0x{:02X}", byte)
}

/// Decodes a 1-byte hex string (with or without `0x`/`0X` prefix) into `TelemetryStatusFlags`.
///
/// Reserved bits 6-7 are ignored on parse for forward compatibility.
pub fn flags_from_hex(s: &str) -> Result<TelemetryStatusFlags, ProtocolError> {
    let trimmed = s.trim();
    let hex_part = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);

    let byte = u8::from_str_radix(hex_part, 16).map_err(|e| {
        ProtocolError::InvalidFormat(format!("Invalid status flags hex '{}': {}", s, e))
    })?;

    Ok(TelemetryStatusFlags {
        check_engine_light: (byte & 0x01) != 0,
        limp_mode: (byte & 0x02) != 0,
        low_voltage: (byte & 0x04) != 0,
        overheating: (byte & 0x08) != 0,
        stale_sample: (byte & 0x10) != 0,
        communication_fault: (byte & 0x20) != 0,
    })
}

/// Serializes a `DatalogSession` into versioned CSV v1.0 format, writing to `writer`.
///
/// Emits LF line endings and UTF-8 encoding without BOM.
pub fn serialize_session_to_writer<W: Write>(
    session: &DatalogSession,
    writer: &mut W,
) -> Result<(), ProtocolError> {
    // 1. Magic Header
    writeln!(writer, "{}", MAGIC_HEADER)
        .map_err(|e| ProtocolError::InvalidFormat(format!("Failed to write magic header: {}", e)))?;

    let sanitize = |s: &str| s.replace(['\r', '\n'], " ");

    // 2. Metadata Block Comments
    writeln!(writer, "# session_id: {}", sanitize(&session.metadata.session_id))
        .map_err(|e| ProtocolError::InvalidFormat(e.to_string()))?;
    writeln!(writer, "# started_at: {}", sanitize(&session.metadata.started_at_iso))
        .map_err(|e| ProtocolError::InvalidFormat(e.to_string()))?;

    if let Some(ref ended_at) = session.metadata.ended_at_iso {
        writeln!(writer, "# ended_at: {}", sanitize(ended_at))
            .map_err(|e| ProtocolError::InvalidFormat(e.to_string()))?;
    } else {
        writeln!(writer, "# ended_at:")
            .map_err(|e| ProtocolError::InvalidFormat(e.to_string()))?;
    }

    if let Some(ref ecu) = session.metadata.ecu_identity {
        writeln!(writer, "# ecu_id: {}", sanitize(&ecu.ecu_id))
            .map_err(|e| ProtocolError::InvalidFormat(e.to_string()))?;
        writeln!(writer, "# protocol: {}", sanitize(&ecu.protocol_name))
            .map_err(|e| ProtocolError::InvalidFormat(e.to_string()))?;
    } else {
        writeln!(writer, "# ecu_id:")
            .map_err(|e| ProtocolError::InvalidFormat(e.to_string()))?;
        writeln!(writer, "# protocol:")
            .map_err(|e| ProtocolError::InvalidFormat(e.to_string()))?;
    }

    writeln!(
        writer,
        "# sample_rate_hz: {:.1}",
        session.metadata.sample_rate_hz
    )
    .map_err(|e| ProtocolError::InvalidFormat(e.to_string()))?;
    writeln!(
        writer,
        "# total_samples: {}",
        session.metadata.total_samples_recorded
    )
    .map_err(|e| ProtocolError::InvalidFormat(e.to_string()))?;
    writeln!(
        writer,
        "# dropped_samples: {}",
        session.metadata.dropped_samples_count
    )
    .map_err(|e| ProtocolError::InvalidFormat(e.to_string()))?;
    writeln!(writer, "# notes: {}", sanitize(&session.metadata.notes))
        .map_err(|e| ProtocolError::InvalidFormat(e.to_string()))?;

    // 3. Tabular Column Headers
    writeln!(writer, "{}", COLUMN_HEADER)
        .map_err(|e| ProtocolError::InvalidFormat(e.to_string()))?;

    // 4. Tabular Data Rows
    for frame in &session.frames {
        writeln!(
            writer,
            "{},{},{:.1},{:.1},{:.1},{:.1},{:.1},{:.2},{:.1},{:.2},{:.2},{:.1},{},{}",
            frame.timestamp_ms,
            frame.sequence,
            frame.rpm,
            frame.tps,
            frame.map_kpa,
            frame.ect_c,
            frame.iat_c,
            frame.battery_volts,
            frame.ignition_advance_deg,
            frame.injector_pulse_width_ms,
            frame.afr,
            frame.speed_kph,
            frame.gear,
            flags_to_hex(&frame.status_flags),
        )
        .map_err(|e| ProtocolError::InvalidFormat(e.to_string()))?;
    }

    Ok(())
}

/// Serializes a `DatalogSession` into an in-memory UTF-8 CSV v1.0 string.
pub fn serialize_session_to_string(session: &DatalogSession) -> Result<String, ProtocolError> {
    let mut buf = Vec::new();
    serialize_session_to_writer(session, &mut buf)?;
    String::from_utf8(buf).map_err(|e| ProtocolError::InvalidFormat(e.to_string()))
}

/// Convenience alias for `serialize_session_to_string`.
pub fn export_session_to_csv(session: &DatalogSession) -> Result<String, ProtocolError> {
    serialize_session_to_string(session)
}

/// Parses and strictly validates a `DatalogSession` from a streaming `BufRead`.
///
/// Handles CRLF and LF transparently, enforces the 50 MB payload limit and 500,000 row limit,
/// and validates line-by-line against `validate_telemetry_frame` and monotonic timestamps.
pub fn parse_session_from_reader<R: BufRead>(reader: R) -> Result<DatalogSession, ProtocolError> {
    let mut total_bytes: usize = 0;
    let mut line_no: usize = 0;
    let mut line = String::new();

    let mut metadata = SessionMetadata::default();
    let mut parsed_ecu_id: Option<String> = None;
    let mut parsed_protocol: Option<String> = None;
    let mut header_found = false;
    let mut frames = Vec::new();
    let mut prev_timestamp: Option<u64> = None;

    let mut bounded_reader = std::io::BufReader::new(std::io::Read::take(
        reader,
        (MAX_PAYLOAD_BYTES + 1) as u64,
    ));
    loop {
        line.clear();
        let bytes_read = bounded_reader.read_line(&mut line).map_err(|e| {
            ProtocolError::InvalidFormat(format!("IO read error at line {}: {}", line_no + 1, e))
        })?;

        if bytes_read == 0 {
            break; // EOF
        }

        total_bytes += bytes_read;
        if total_bytes > MAX_PAYLOAD_BYTES {
            return Err(ProtocolError::InvalidFormat(
                "Payload exceeds 50 MB limit".to_string(),
            ));
        }

        line_no += 1;
        let mut current_str = line.as_str();

        if line_no == 1 {
            // Strip UTF-8 BOM if present
            if current_str.starts_with('\u{FEFF}') {
                current_str = &current_str['\u{FEFF}'.len_utf8()..];
            }
            let trimmed_magic = current_str.trim_end_matches(['\r', '\n']);
            if trimmed_magic != MAGIC_HEADER {
                return Err(ProtocolError::InvalidFormat(
                    "Unsupported CSV schema version: expected v1.0".to_string(),
                ));
            }
            continue;
        }

        let trimmed = current_str.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            continue;
        }

        if !header_found {
            if let Some(comment_body) = trimmed.strip_prefix('#') {
                let comment_trimmed = comment_body.trim();
                if let Some((key, val)) = comment_trimmed.split_once(':') {
                    let k = key.trim();
                    let v = val.trim();
                    match k {
                        "session_id" => metadata.session_id = v.to_string(),
                        "started_at" => metadata.started_at_iso = v.to_string(),
                        "ended_at" => {
                            metadata.ended_at_iso =
                                if v.is_empty() { None } else { Some(v.to_string()) };
                        }
                        "ecu_id" => {
                            if !v.is_empty() {
                                parsed_ecu_id = Some(v.to_string());
                            }
                        }
                        "protocol" => {
                            if !v.is_empty() {
                                parsed_protocol = Some(v.to_string());
                            }
                        }
                        "sample_rate_hz" => {
                            metadata.sample_rate_hz = v.parse::<f32>().map_err(|e| {
                                ProtocolError::InvalidFormat(format!(
                                    "Line {}: invalid sample_rate_hz float '{}': {}",
                                    line_no, v, e
                                ))
                            })?;
                        }
                        "total_samples" => {
                            metadata.total_samples_recorded =
                                v.parse::<usize>().map_err(|e| {
                                    ProtocolError::InvalidFormat(format!(
                                        "Line {}: invalid total_samples integer '{}': {}",
                                        line_no, v, e
                                    ))
                                })?;
                        }
                        "dropped_samples" => {
                            metadata.dropped_samples_count =
                                v.parse::<usize>().map_err(|e| {
                                    ProtocolError::InvalidFormat(format!(
                                        "Line {}: invalid dropped_samples integer '{}': {}",
                                        line_no, v, e
                                    ))
                                })?;
                        }
                        "notes" => metadata.notes = v.to_string(),
                        _ => {} // Forward compatibility: ignore unknown metadata comments
                    }
                }
                continue;
            }

            // Not a comment line, so must be the column header line
            if trimmed.trim() != COLUMN_HEADER {
                return Err(ProtocolError::InvalidFormat(format!(
                    "Line {}: invalid CSV column header",
                    line_no
                )));
            }
            header_found = true;
            continue;
        }

        // header_found == true:
        if trimmed.starts_with('#') {
            continue;
        }

        if frames.len() >= MAX_DATA_ROWS {
            return Err(ProtocolError::InvalidFormat(
                "Payload exceeds 500,000 rows limit".to_string(),
            ));
        }

        let cols: Vec<&str> = trimmed.split(',').map(|s| s.trim()).collect();
        if cols.len() != 14 {
            return Err(ProtocolError::InvalidFormat(format!(
                "Line {}: expected 14 columns, found {}",
                line_no,
                cols.len()
            )));
        }

        let timestamp_ms: u64 = cols[0].parse::<u64>().map_err(|e| {
            ProtocolError::InvalidFormat(format!(
                "Line {}: column 'timestamp_ms' invalid integer: {}",
                line_no, e
            ))
        })?;

        let sequence: u64 = cols[1].parse::<u64>().map_err(|e| {
            ProtocolError::InvalidFormat(format!(
                "Line {}: column 'sequence' invalid integer: {}",
                line_no, e
            ))
        })?;

        let rpm: f32 = cols[2].parse::<f32>().map_err(|e| {
            ProtocolError::InvalidFormat(format!(
                "Line {}: column 'rpm' invalid float: {}",
                line_no, e
            ))
        })?;

        let tps: f32 = cols[3].parse::<f32>().map_err(|e| {
            ProtocolError::InvalidFormat(format!(
                "Line {}: column 'tps' invalid float: {}",
                line_no, e
            ))
        })?;

        let map_kpa: f32 = cols[4].parse::<f32>().map_err(|e| {
            ProtocolError::InvalidFormat(format!(
                "Line {}: column 'map_kpa' invalid float: {}",
                line_no, e
            ))
        })?;

        let ect_c: f32 = cols[5].parse::<f32>().map_err(|e| {
            ProtocolError::InvalidFormat(format!(
                "Line {}: column 'ect_c' invalid float: {}",
                line_no, e
            ))
        })?;

        let iat_c: f32 = cols[6].parse::<f32>().map_err(|e| {
            ProtocolError::InvalidFormat(format!(
                "Line {}: column 'iat_c' invalid float: {}",
                line_no, e
            ))
        })?;

        let battery_volts: f32 = cols[7].parse::<f32>().map_err(|e| {
            ProtocolError::InvalidFormat(format!(
                "Line {}: column 'battery_volts' invalid float: {}",
                line_no, e
            ))
        })?;

        let ignition_advance_deg: f32 = cols[8].parse::<f32>().map_err(|e| {
            ProtocolError::InvalidFormat(format!(
                "Line {}: column 'ignition_advance_deg' invalid float: {}",
                line_no, e
            ))
        })?;

        let injector_pulse_width_ms: f32 = cols[9].parse::<f32>().map_err(|e| {
            ProtocolError::InvalidFormat(format!(
                "Line {}: column 'injector_pulse_width_ms' invalid float: {}",
                line_no, e
            ))
        })?;

        let afr: f32 = cols[10].parse::<f32>().map_err(|e| {
            ProtocolError::InvalidFormat(format!(
                "Line {}: column 'afr' invalid float: {}",
                line_no, e
            ))
        })?;

        let speed_kph: f32 = cols[11].parse::<f32>().map_err(|e| {
            ProtocolError::InvalidFormat(format!(
                "Line {}: column 'speed_kph' invalid float: {}",
                line_no, e
            ))
        })?;

        let gear: u8 = cols[12].parse::<u8>().map_err(|e| {
            ProtocolError::InvalidFormat(format!(
                "Line {}: column 'gear' invalid integer: {}",
                line_no, e
            ))
        })?;

        let status_flags = flags_from_hex(cols[13]).map_err(|e| {
            ProtocolError::InvalidFormat(format!(
                "Line {}: column 'status_flags' invalid hex: {}",
                line_no, e
            ))
        })?;

        // Monotonic timestamp validation
        if let Some(prev_ts) = prev_timestamp {
            if timestamp_ms < prev_ts {
                return Err(ProtocolError::InvalidFormat(format!(
                    "Line {}: non-monotonic timestamp rollback: {} < {}",
                    line_no, timestamp_ms, prev_ts
                )));
            }
        }
        prev_timestamp = Some(timestamp_ms);

        let frame = TelemetryFrame {
            timestamp_ms,
            sequence,
            is_simulated: false,
            rpm,
            tps,
            map_kpa,
            ect_c,
            iat_c,
            battery_volts,
            ignition_advance_deg,
            injector_pulse_width_ms,
            afr,
            speed_kph,
            gear,
            status_flags,
        };

        let report = validate_telemetry_frame(&frame);
        if !report.is_valid() {
            let err_msgs: Vec<String> = report.errors.iter().map(|e| e.to_string()).collect();
            return Err(ProtocolError::InvalidFormat(format!(
                "Line {}: telemetry frame validation failed: {}",
                line_no,
                err_msgs.join("; ")
            )));
        }

        frames.push(frame);
    }

    if line_no == 0 {
        return Err(ProtocolError::InvalidFormat(
            "Unsupported CSV schema version: expected v1.0".to_string(),
        ));
    }

    if !header_found {
        return Err(ProtocolError::InvalidFormat(
            "Missing CSV column header".to_string(),
        ));
    }

    if parsed_ecu_id.is_some() || parsed_protocol.is_some() {
        metadata.ecu_identity = Some(EcuIdentity {
            ecu_id: parsed_ecu_id.unwrap_or_default(),
            vin: None,
            software_version: String::new(),
            hardware_version: String::new(),
            protocol_name: parsed_protocol.unwrap_or_default(),
            is_simulated: false,
        });
    }

    if metadata.total_samples_recorded == 0 && !frames.is_empty() {
        metadata.total_samples_recorded = frames.len();
    }

    metadata.schema_version = 1;

    Ok(DatalogSession::new(metadata, frames))
}

/// Convenience alias for `parse_session_from_reader`.
pub fn parse_session_from_csv<R: BufRead>(reader: R) -> Result<DatalogSession, ProtocolError> {
    parse_session_from_reader(reader)
}

/// Parses and validates a `DatalogSession` from an in-memory CSV string.
pub fn parse_session_from_str(csv_str: &str) -> Result<DatalogSession, ProtocolError> {
    parse_session_from_reader(std::io::Cursor::new(csv_str.as_bytes()))
}
