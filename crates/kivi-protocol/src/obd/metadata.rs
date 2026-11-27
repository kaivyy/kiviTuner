use crate::obd::errors::ObdCodecError;

/// Decodes the Vehicle Identification Number (VIN) from a Mode 09 PID 0x02 payload.
///
/// Under SAE J1979 / ISO 15765-4:
/// - Total payload length is exactly 20 bytes:
///   `[0x49, 0x02, NODI (0x01), 17 ASCII characters]`.
pub fn decode_vin(payload: &[u8]) -> Result<String, ObdCodecError> {
    const EXPECTED_VIN_LEN: usize = 20;

    if payload.len() < EXPECTED_VIN_LEN {
        return Err(ObdCodecError::TruncatedPayload {
            expected: EXPECTED_VIN_LEN,
            actual: payload.len(),
        });
    }
    if payload.len() > EXPECTED_VIN_LEN {
        return Err(ObdCodecError::OversizedPayload {
            expected: EXPECTED_VIN_LEN,
            actual: payload.len(),
        });
    }

    if payload[0] != 0x49 {
        return Err(ObdCodecError::UnexpectedService {
            expected: 0x49,
            actual: payload[0],
        });
    }

    if payload[1] != 0x02 {
        return Err(ObdCodecError::UnexpectedPid {
            expected: 0x02,
            actual: payload[1],
        });
    }

    // payload[2] is number of data items (NODI), typically 0x01
    let vin_bytes = &payload[3..20];
    let vin_str = std::str::from_utf8(vin_bytes).map_err(|e| {
        ObdCodecError::NonAsciiMetadata(format!("invalid UTF-8/ASCII VIN bytes: {}", e))
    })?;

    // Validate ASCII printable characters
    if !vin_str.chars().all(|c| c.is_ascii_graphic()) {
        return Err(ObdCodecError::NonAsciiMetadata(format!(
            "VIN contains non-printable characters: {:?}",
            vin_str
        )));
    }

    Ok(vin_str.to_string())
}

/// Decodes the Calibration Identification (Cal ID) from a Mode 09 PID 0x04 payload.
///
/// Under SAE J1979:
/// - Minimum payload length is 4 bytes: `[0x49, 0x04, NODI, cal_id_bytes...]`.
pub fn decode_calibration_id(payload: &[u8]) -> Result<String, ObdCodecError> {
    const MIN_CAL_ID_LEN: usize = 4;

    if payload.len() < MIN_CAL_ID_LEN {
        return Err(ObdCodecError::TruncatedPayload {
            expected: MIN_CAL_ID_LEN,
            actual: payload.len(),
        });
    }

    if payload[0] != 0x49 {
        return Err(ObdCodecError::UnexpectedService {
            expected: 0x49,
            actual: payload[0],
        });
    }

    if payload[1] != 0x04 {
        return Err(ObdCodecError::UnexpectedPid {
            expected: 0x04,
            actual: payload[1],
        });
    }

    // payload[2] is number of data items
    let cal_id_bytes = &payload[3..];
    let cal_id_str = std::str::from_utf8(cal_id_bytes).map_err(|e| {
        ObdCodecError::NonAsciiMetadata(format!(
            "invalid UTF-8/ASCII Calibration ID bytes: {}",
            e
        ))
    })?;

    // Trim trailing null bytes and whitespace common in ECU memory padding
    let cleaned = cal_id_str.trim_matches(['\0', ' '].as_slice());

    Ok(cleaned.to_string())
}
