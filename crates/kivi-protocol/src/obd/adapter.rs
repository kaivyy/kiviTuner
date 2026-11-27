use crate::obd::errors::ObdCodecError;

/// Maximum permitted ISO-TP diagnostic payload length in bytes (standard 12-bit FF limit).
pub const MAX_ISOTP_PAYLOAD_BYTES: usize = 4095;

/// Parses a raw ASCII string response from an OBD-II adapter (ELM327 / STN11xx)
/// into a vector of diagnostic bytes.
///
/// Handles:
/// - Single-line standard hex responses (e.g. `"41 0C 1A F8\r\r>"`).
/// - Adapter error tokens (`NO DATA`, `?`, `BUS BUSY`, `CAN ERROR`, etc.).
/// - Multi-line ISO-TP framed responses (e.g. `"014\r0: 49 02 ...\r1: ..."`).
/// - Sequence order verification and length enforcement.
pub fn parse_adapter_response(raw: &str) -> Result<Vec<u8>, ObdCodecError> {
    // 1. Normalize line endings and collect non-empty trimmed lines
    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines: Vec<&str> = Vec::new();

    for line in normalized.lines() {
        let trimmed = line.trim();
        // Skip empty lines, standalone prompts, and adapter progress notifications
        if trimmed.is_empty() || trimmed == ">" || trimmed.eq_ignore_ascii_case("SEARCHING...") {
            continue;
        }
        // If line ends with prompt '>', strip it
        let cleaned = trimmed.trim_end_matches('>').trim();
        if !cleaned.is_empty() {
            lines.push(cleaned);
        }
    }

    if lines.is_empty() {
        return Err(ObdCodecError::MalformedText(
            "empty response from adapter".to_string(),
        ));
    }

    // Skip echoed command line if character echo was active on adapter.
    // Verifies exact 2-byte command format and confirms matching positive response in subsequent lines.
    if is_echo_line(&lines) {
        lines.remove(0);
    }

    // 2. Check for adapter error conditions and status tokens
    for line in &lines {
        let upper = line.to_ascii_uppercase();
        if upper == "NO DATA" {
            return Err(ObdCodecError::NoData);
        }
        if upper == "?"
            || upper == "BUS BUSY"
            || upper == "CAN ERROR"
            || upper == "BUFFER FULL"
            || upper == "UNABLE TO CONNECT"
            || upper == "STOPPED"
            || upper == "FB ERROR"
            || upper == "DATA ERROR"
            || upper.starts_with("ERR")
        {
            return Err(ObdCodecError::AdapterError((*line).to_string()));
        }
    }

    // 3. Determine if this is a multi-line ISO-TP framed response
    let has_iso_tp_frames = lines.iter().any(|line| {
        if let Some((prefix, _)) = line.split_once(':') {
            prefix.trim().chars().all(|c| c.is_ascii_hexdigit())
        } else {
            false
        }
    });

    if has_iso_tp_frames {
        parse_multiline_isotp(&lines)
    } else {
        parse_single_or_flat_hex(&lines)
    }
}

/// Parses a multi-line ISO-TP response with optional declared byte length and frame sequence tags.
fn parse_multiline_isotp(lines: &[&str]) -> Result<Vec<u8>, ObdCodecError> {
    let mut declared_len: Option<usize> = None;
    let mut expected_seq: usize = 0;
    let mut payload = Vec::new();

    for line in lines {
        if let Some((seq_str, data_str)) = line.split_once(':') {
            let seq = usize::from_str_radix(seq_str.trim(), 16).map_err(|_| {
                ObdCodecError::MalformedText(format!(
                    "invalid ISO-TP sequence index: '{}'",
                    seq_str
                ))
            })?;

            if seq != expected_seq {
                return Err(ObdCodecError::MalformedText(format!(
                    "ISO-TP frame out of order: expected seq {}, received {}",
                    expected_seq, seq
                )));
            }
            expected_seq = (expected_seq + 1) % 16;

            // Parse hex bytes in the frame line
            for tok in data_str.split_whitespace() {
                let byte = u8::from_str_radix(tok, 16).map_err(|_| {
                    ObdCodecError::MalformedText(format!("invalid hex byte in frame: '{}'", tok))
                })?;
                payload.push(byte);
                if payload.len() > MAX_ISOTP_PAYLOAD_BYTES {
                    return Err(ObdCodecError::OversizedPayload {
                        expected: MAX_ISOTP_PAYLOAD_BYTES,
                        actual: payload.len(),
                    });
                }
            }
        } else {
            // First line without ':' in a multi-line response is the declared ISO-TP length
            if declared_len.is_none() && payload.is_empty() {
                let trimmed = line.trim();
                if let Ok(len) = usize::from_str_radix(trimmed, 16) {
                    if len > MAX_ISOTP_PAYLOAD_BYTES {
                        return Err(ObdCodecError::OversizedPayload {
                            expected: MAX_ISOTP_PAYLOAD_BYTES,
                            actual: len,
                        });
                    }
                    declared_len = Some(len);
                    continue;
                }
            }
            return Err(ObdCodecError::MalformedText(format!(
                "unexpected line in ISO-TP multi-line response: '{}'",
                line
            )));
        }
    }

    // If a declared length was given in header, enforce it
    if let Some(target_len) = declared_len {
        if payload.len() < target_len {
            return Err(ObdCodecError::TruncatedPayload {
                expected: target_len,
                actual: payload.len(),
            });
        }
        payload.truncate(target_len);
    }

    Ok(payload)
}

/// Parses single-line or flat hex response lines.
///
/// When multiple ECUs respond to a functional broadcast without CAN headers (`ATH0`)
/// with distinct lines beginning with the same positive service header (e.g. two `41 00 ...` replies),
/// keeps the primary ECU response line instead of concatenating unrelated ECU frames.
fn parse_single_or_flat_hex(lines: &[&str]) -> Result<Vec<u8>, ObdCodecError> {
    let mut per_line: Vec<Vec<u8>> = Vec::with_capacity(lines.len());
    for line in lines {
        let mut row = Vec::new();
        for tok in line.split_whitespace() {
            let byte = u8::from_str_radix(tok, 16).map_err(|_| {
                ObdCodecError::MalformedText(format!("invalid hex byte: '{}'", tok))
            })?;
            row.push(byte);
        }
        if !row.is_empty() {
            per_line.push(row);
        }
    }

    if per_line.is_empty() {
        return Err(ObdCodecError::MalformedText(
            "no valid hex bytes in adapter response".to_string(),
        ));
    }

    if per_line.len() > 1 {
        let first = &per_line[0];
        let is_multi_ecu_repeat = first.len() >= 2
            && per_line.iter().skip(1).all(|r| {
                r.len() >= 2
                    && r[0] == first[0]
                    && (r[0] == 0x43 || r[0] == 0x47 || r[0] == 0x4A || r[1] == first[1])
            });
        if is_multi_ecu_repeat {
            return Ok(per_line.swap_remove(0));
        }
    }

    Ok(per_line.into_iter().flatten().collect())
}

/// Extracts the mode and PID/item if a line represents an echoed 2-byte Mode 01 or Mode 09 command.
///
/// Supported formats derived from the typed command model:
/// - Spaced: exactly two 2-digit hex tokens (e.g. "01 0C", "09 02")
/// - Compact: exactly one 4-digit hex token (e.g. "010C", "0902")
fn parse_command_echo(line: &str) -> Option<(u8, u8)> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let (mode_str, pid_str) = match tokens.as_slice() {
        [t0, t1] if t0.is_ascii() && t1.is_ascii() && t0.len() == 2 && t1.len() == 2 => {
            (*t0, *t1)
        }
        [t0] if t0.is_ascii() && t0.len() == 4 => (&t0[..2], &t0[2..]),
        _ => return None,
    };

    let mode = u8::from_str_radix(mode_str, 16).ok()?;
    let pid = u8::from_str_radix(pid_str, 16).ok()?;

    if mode == 0x01 || mode == 0x09 {
        Some((mode, pid))
    } else {
        None
    }
}

/// Checks if a line begins with the expected diagnostic response service and PID.
fn matches_diagnostic_response(line: &str, expected_service: u8, expected_pid: u8) -> bool {
    let data_line = if let Some((seq_str, rest)) = line.split_once(':') {
        if usize::from_str_radix(seq_str.trim(), 16).ok() != Some(0) {
            return false;
        }
        rest.trim()
    } else {
        line.trim()
    };

    let mut hex_tokens = data_line.split_whitespace();
    let s_tok = match hex_tokens.next() {
        Some(t) => t,
        None => return false,
    };
    let p_tok = match hex_tokens.next() {
        Some(t) => t,
        None => return false,
    };

    let service = match u8::from_str_radix(s_tok, 16) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let pid = match u8::from_str_radix(p_tok, 16) {
        Ok(p) => p,
        Err(_) => return false,
    };

    service == expected_service && pid == expected_pid
}

/// Determines whether line 0 is a confirmed echoed diagnostic command line.
fn is_echo_line(lines: &[&str]) -> bool {
    if lines.len() < 2 {
        return false;
    }
    let (mode, pid) = match parse_command_echo(lines[0]) {
        Some(pair) => pair,
        None => return false,
    };
    let expected_service = mode.wrapping_add(0x40);

    // Case 1: Line 1 directly contains the diagnostic response (single-frame or ISO-TP frame 0)
    if matches_diagnostic_response(lines[1], expected_service, pid) {
        return true;
    }

    // Case 2: Line 1 is an ISO-TP declared length header, and Line 2 contains the diagnostic response
    if lines.len() >= 3 {
        let trimmed_len = lines[1].trim();
        if let Ok(declared) = usize::from_str_radix(trimmed_len, 16) {
            if declared <= MAX_ISOTP_PAYLOAD_BYTES
                && matches_diagnostic_response(lines[2], expected_service, pid)
            {
                return true;
            }
        }
    }

    false
}
