use super::types::{
    AxisAssociationMode, AxisBindingError, AxisDescriptor, TelemetryChannel,
};

/// Normalizes a unit string by trimming, lowercasing, and normalizing punctuation.
pub fn normalize_unit(raw_unit: &str) -> String {
    let trimmed = raw_unit.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let lower = trimmed.to_lowercase();
    match lower.as_str() {
        "rpm" | "r/min" | "rev/min" | "revolutions/min" | "n" => "rpm".to_string(),
        "%" | "pct" | "percent" | "percentage" | "tps" | "deg tps" | "throttle" => "%".to_string(),
        "kpa" | "kilopascal" | "k-pa" => "kpa".to_string(),
        "mbar" | "millibar" => "mbar".to_string(),
        "bar" => "bar".to_string(),
        "psi" | "psig" => "psi".to_string(),
        "deg c" | "°c" | "degc" | "c" | "celsius" => "c".to_string(),
        "deg f" | "°f" | "degf" | "f" | "fahrenheit" => "f".to_string(),
        "v" | "volt" | "volts" | "voltage" | "vdc" => "v".to_string(),
        "ms" | "msec" | "millisecond" | "milliseconds" | "pw" => "ms".to_string(),
        "afr" | "air/fuel" | "air fuel ratio" => "afr".to_string(),
        "lambda" | "λ" => "lambda".to_string(),
        "km/h" | "kph" | "kmh" => "km/h".to_string(),
        "mph" => "mph".to_string(),
        "deg" | "deg btdc" | "°btdc" | "btdc" | "advance" | "timing" => "deg btdc".to_string(),
        "gear" => "gear".to_string(),
        _ => lower,
    }
}

/// Matches an unambiguous telemetry channel from a normalized or raw unit string.
pub fn match_channel_by_unit(raw_unit: &str) -> Result<TelemetryChannel, AxisBindingError> {
    let normalized = normalize_unit(raw_unit);
    if normalized.is_empty() {
        return Err(AxisBindingError::MissingUnit);
    }

    match normalized.as_str() {
        "rpm" => Ok(TelemetryChannel::Rpm),
        "%" => Ok(TelemetryChannel::Tps),
        "kpa" | "mbar" | "bar" | "psi" => Ok(TelemetryChannel::MapKpa),
        "c" | "f" => Ok(TelemetryChannel::EctC),
        "v" => Ok(TelemetryChannel::BatteryVolts),
        "ms" => Ok(TelemetryChannel::InjectorPulseWidthMs),
        "afr" | "lambda" => Ok(TelemetryChannel::Afr),
        "km/h" | "mph" => Ok(TelemetryChannel::SpeedKph),
        "deg btdc" => Ok(TelemetryChannel::IgnitionAdvanceDeg),
        "gear" => Ok(TelemetryChannel::Gear),
        "temp" | "temperature" => Err(AxisBindingError::AmbiguousMatch {
            input: raw_unit.to_string(),
            matched: vec![TelemetryChannel::EctC, TelemetryChannel::IatC],
        }),
        "pressure" => Err(AxisBindingError::AmbiguousMatch {
            input: raw_unit.to_string(),
            matched: vec![TelemetryChannel::MapKpa],
        }),
        unsupported => Err(AxisBindingError::UnsupportedUnit {
            unit: unsupported.to_string(),
        }),
    }
}

/// Validates that axis label strings represent valid numeric coordinates (e.g. integers or floats).
///
/// Under empirical the reference implementation rules (`XDFTableEditor.htm`), string labels (e.g. "Park", "Idle", "Cruise")
/// cannot be resolved to telemetry data acquisition coordinates and must be rejected with `NonNumericLabels`.
pub fn validate_numeric_labels(labels: &[String]) -> Result<Vec<f64>, AxisBindingError> {
    let mut numeric_values = Vec::with_capacity(labels.len());
    for (idx, label) in labels.iter().enumerate() {
        let trimmed = label.trim();
        let stripped = trimmed.strip_suffix('%').unwrap_or(trimmed).trim();
        match stripped.parse::<f64>() {
            Ok(val) => numeric_values.push(val),
            Err(_) => {
                return Err(AxisBindingError::NonNumericLabels {
                    label_sample: label.clone(),
                    index: idx,
                });
            }
        }
    }
    Ok(numeric_values)
}

/// Resolves a telemetry channel for an axis descriptor, honoring association mode precedence
/// and validating numeric coordinates.
pub fn resolve_axis_channel(
    descriptor: &AxisDescriptor,
    explicit_override: Option<&AxisAssociationMode>,
) -> Result<TelemetryChannel, AxisBindingError> {
    let effective_mode = explicit_override.unwrap_or(&descriptor.association_mode);

    match effective_mode {
        AxisAssociationMode::NoLink => Err(AxisBindingError::NoLink),
        AxisAssociationMode::ExplicitLink(target) => {
            let trimmed = target.trim();
            if trimmed.is_empty() {
                return Err(AxisBindingError::InvalidExplicitReference {
                    target: target.clone(),
                    reason: "Target reference is empty or whitespace".to_string(),
                });
            }

            let lower = trimmed.to_lowercase();
            match lower.as_str() {
                "rpm" | "engine_speed" | "speed_rpm" => Ok(TelemetryChannel::Rpm),
                "tps" | "throttle" | "throttle_position" => Ok(TelemetryChannel::Tps),
                "map" | "map_kpa" | "manifold_pressure" | "boost" => Ok(TelemetryChannel::MapKpa),
                "ect" | "ect_c" | "coolant" | "coolant_temp" => Ok(TelemetryChannel::EctC),
                "iat" | "iat_c" | "air_temp" | "intake_air_temp" => Ok(TelemetryChannel::IatC),
                "battery" | "battery_volts" | "volts" | "voltage" => {
                    Ok(TelemetryChannel::BatteryVolts)
                }
                "ignition_advance" | "ignition_advance_deg" | "timing" | "spark" => {
                    Ok(TelemetryChannel::IgnitionAdvanceDeg)
                }
                "injector_pulse_width" | "injector_pulse_width_ms" | "pw" | "pulse_width" => {
                    Ok(TelemetryChannel::InjectorPulseWidthMs)
                }
                "afr" | "air_fuel_ratio" | "lambda" => Ok(TelemetryChannel::Afr),
                "speed" | "speed_kph" | "vehicle_speed" => Ok(TelemetryChannel::SpeedKph),
                "gear" | "gear_position" => Ok(TelemetryChannel::Gear),
                _ => Err(AxisBindingError::InvalidExplicitReference {
                    target: target.clone(),
                    reason: format!("Unknown channel target '{}'", target),
                }),
            }
        }
        AxisAssociationMode::UnitMatch => {
            if !descriptor.labels.is_empty() {
                validate_numeric_labels(&descriptor.labels)?;
            }
            match_channel_by_unit(&descriptor.unit)
        }
    }
}
