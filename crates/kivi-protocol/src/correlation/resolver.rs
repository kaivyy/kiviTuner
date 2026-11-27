use super::binding::resolve_axis_channel;
use super::types::{
    AxisCoordinate, AxisDescriptor, AxisResolutionError, AxisRole, CorrelationResult,
    CorrelationStatus, TableBindingConfig, TelemetryChannel,
};
use kivi_core::memory_map::MemoryMap;
use kivi_defs::expr::evaluate_scaling;
use kivi_defs::model::{AxisDef, MapDef};
use crate::model::TelemetryFrame;

/// Epsilon threshold for floating point boundary equality.
const EPSILON: f64 = 1e-7;

/// Resolves a single telemetry value against a slice of axis breakpoints.
///
/// Features:
/// - Supports strictly ascending and strictly descending monotonic axes.
/// - Returns interval index, continuous fractional coordinate, nearest cell index, and exact match flag.
/// - Rejects empty axes, single-point axes, duplicate breakpoints, non-monotonic values, and non-finite numbers.
/// - Strictly rejects values outside the breakpoint range with `OutOfRange` (never silently clamped).
pub fn resolve_axis_coordinate(
    breakpoints: &[f64],
    value: f64,
) -> Result<AxisCoordinate, AxisResolutionError> {
    if breakpoints.is_empty() {
        return Err(AxisResolutionError::EmptyAxis);
    }
    if breakpoints.len() == 1 {
        return Err(AxisResolutionError::SinglePointAxis);
    }
    if !value.is_finite() {
        return Err(AxisResolutionError::NonFiniteValue {
            value: value as f32,
        });
    }

    let n = breakpoints.len();

    // Verify all breakpoints are finite
    for (idx, &bp) in breakpoints.iter().enumerate() {
        if !bp.is_finite() {
            return Err(AxisResolutionError::NonFiniteBreakpoint { index: idx });
        }
    }

    // Determine direction and verify strict monotonicity
    let is_ascending = breakpoints[0] < breakpoints[n - 1];

    if is_ascending {
        for i in 0..n - 1 {
            if (breakpoints[i] - breakpoints[i + 1]).abs() < EPSILON {
                return Err(AxisResolutionError::DuplicateBreakpoints {
                    val: breakpoints[i],
                    first_idx: i,
                    second_idx: i + 1,
                });
            }
            if breakpoints[i] > breakpoints[i + 1] {
                return Err(AxisResolutionError::NonMonotonic {
                    index: i + 1,
                    prev: breakpoints[i],
                    curr: breakpoints[i + 1],
                });
            }
        }

        let min = breakpoints[0];
        let max = breakpoints[n - 1];

        if value < min - EPSILON || value > max + EPSILON {
            return Err(AxisResolutionError::OutOfRange { value, min, max });
        }

        // Exact endpoint matching
        if (value - min).abs() < EPSILON {
            return Ok(AxisCoordinate {
                interval_index: 0,
                fractional_pos: 0.0,
                nearest_index: 0,
                is_exact_match: true,
            });
        }
        if (value - max).abs() < EPSILON {
            return Ok(AxisCoordinate {
                interval_index: n - 2,
                fractional_pos: (n - 1) as f64,
                nearest_index: n - 1,
                is_exact_match: true,
            });
        }

        // Binary search for interval index i where breakpoints[i] <= value && value <= breakpoints[i+1]
        let mut low = 0;
        let mut high = n - 2;
        let mut idx = 0;

        while low <= high {
            let mid = (low + high) / 2;
            if breakpoints[mid] <= value {
                idx = mid;
                low = mid + 1;
            } else {
                if mid == 0 {
                    break;
                }
                high = mid - 1;
            }
        }

        let span = breakpoints[idx + 1] - breakpoints[idx];
        let t = (value - breakpoints[idx]) / span;
        let fractional_pos = idx as f64 + t;
        let nearest_index = if t <= 0.5 { idx } else { idx + 1 };
        let is_exact_match =
            (value - breakpoints[idx]).abs() < EPSILON || (value - breakpoints[idx + 1]).abs() < EPSILON;

        Ok(AxisCoordinate {
            interval_index: idx,
            fractional_pos,
            nearest_index,
            is_exact_match,
        })
    } else {
        // Strictly descending
        for i in 0..n - 1 {
            if (breakpoints[i] - breakpoints[i + 1]).abs() < EPSILON {
                return Err(AxisResolutionError::DuplicateBreakpoints {
                    val: breakpoints[i],
                    first_idx: i,
                    second_idx: i + 1,
                });
            }
            if breakpoints[i] < breakpoints[i + 1] {
                return Err(AxisResolutionError::NonMonotonic {
                    index: i + 1,
                    prev: breakpoints[i],
                    curr: breakpoints[i + 1],
                });
            }
        }

        let min = breakpoints[n - 1];
        let max = breakpoints[0];

        if value < min - EPSILON || value > max + EPSILON {
            return Err(AxisResolutionError::OutOfRange { value, min, max });
        }

        // Exact endpoint matching
        if (value - max).abs() < EPSILON {
            return Ok(AxisCoordinate {
                interval_index: 0,
                fractional_pos: 0.0,
                nearest_index: 0,
                is_exact_match: true,
            });
        }
        if (value - min).abs() < EPSILON {
            return Ok(AxisCoordinate {
                interval_index: n - 2,
                fractional_pos: (n - 1) as f64,
                nearest_index: n - 1,
                is_exact_match: true,
            });
        }

        // Binary search for interval index i where breakpoints[i] >= value && value >= breakpoints[i+1]
        let mut low = 0;
        let mut high = n - 2;
        let mut idx = 0;

        while low <= high {
            let mid = (low + high) / 2;
            if breakpoints[mid] >= value {
                idx = mid;
                low = mid + 1;
            } else {
                if mid == 0 {
                    break;
                }
                high = mid - 1;
            }
        }

        let span = breakpoints[idx] - breakpoints[idx + 1];
        let t = (breakpoints[idx] - value) / span;
        let fractional_pos = idx as f64 + t;
        let nearest_index = if t <= 0.5 { idx } else { idx + 1 };
        let is_exact_match =
            (value - breakpoints[idx]).abs() < EPSILON || (value - breakpoints[idx + 1]).abs() < EPSILON;

        Ok(AxisCoordinate {
            interval_index: idx,
            fractional_pos,
            nearest_index,
            is_exact_match,
        })
    }
}

/// Extracts a channel's floating-point engineering value from a `TelemetryFrame`.
pub fn extract_channel_value(
    frame: &TelemetryFrame,
    channel: TelemetryChannel,
) -> Result<f64, AxisResolutionError> {
    let val_f32 = match channel {
        TelemetryChannel::Rpm => frame.rpm,
        TelemetryChannel::Tps => frame.tps,
        TelemetryChannel::MapKpa => frame.map_kpa,
        TelemetryChannel::EctC => frame.ect_c,
        TelemetryChannel::IatC => frame.iat_c,
        TelemetryChannel::BatteryVolts => frame.battery_volts,
        TelemetryChannel::IgnitionAdvanceDeg => frame.ignition_advance_deg,
        TelemetryChannel::InjectorPulseWidthMs => frame.injector_pulse_width_ms,
        TelemetryChannel::Afr => frame.afr,
        TelemetryChannel::SpeedKph => frame.speed_kph,
        TelemetryChannel::Gear => frame.gear as f32,
    };

    if !val_f32.is_finite() {
        return Err(AxisResolutionError::NonFiniteValue { value: val_f32 });
    }
    Ok(val_f32 as f64)
}

/// Retrieves axis breakpoint values from `AxisDef.values` or reads them from ROM at `AxisDef.address`.
pub fn extract_axis_breakpoints(
    axis_def: &AxisDef,
    mem_opt: Option<&MemoryMap>,
) -> Result<Vec<f64>, AxisResolutionError> {
    if !axis_def.values.is_empty() {
        return Ok(axis_def.values.clone());
    }
    if axis_def.address > 0 && axis_def.length > 0 {
        if let Some(mem) = mem_opt {
            let byte_size = axis_def.dtype.byte_size();
            let remaining = mem
                .len()
                .checked_sub(axis_def.address)
                .ok_or(AxisResolutionError::EmptyAxis)?;
            let max_elems = remaining / byte_size.max(1);
            let mut values = Vec::with_capacity(axis_def.length.min(max_elems));
            for i in 0..axis_def.length {
                let off = i
                    .checked_mul(byte_size)
                    .and_then(|d| axis_def.address.checked_add(d))
                    .ok_or(AxisResolutionError::EmptyAxis)?;
                match mem.read_value(off, axis_def.dtype, axis_def.endian) {
                    Ok(raw) => {
                        let eng = evaluate_scaling(&axis_def.scaling.equation, raw)
                            .unwrap_or(raw);
                        values.push(eng);
                    }
                    Err(_) => return Err(AxisResolutionError::EmptyAxis),
                }
            }
            return Ok(values);
        }
    }
    Err(AxisResolutionError::EmptyAxis)
}

/// Correlates a telemetry frame against a calibration table definition.
///
/// Returns a structured `CorrelationResult` containing resolved coordinates,
/// discrete cell selections, raw and engineering values, and explicit failure status.
pub fn correlate_frame(
    frame: &TelemetryFrame,
    map_def: &MapDef,
    mem_opt: Option<&MemoryMap>,
    config: Option<&TableBindingConfig>,
) -> CorrelationResult {
    let mut res = CorrelationResult {
        is_valid: false,
        status: CorrelationStatus::Valid,
        timestamp_ms: frame.timestamp_ms,
        sequence: frame.sequence,
        is_simulated: frame.is_simulated,
        selected_row: None,
        selected_col: None,
        row_coordinate: None,
        col_coordinate: None,
        cell_raw: None,
        cell_eng: None,
        table_id: map_def.id.clone(),
    };

    // Stale samples and communication faults immediately reject correlation
    if frame.status_flags.stale_sample {
        res.status = CorrelationStatus::StaleSample;
        return res;
    }
    if frame.status_flags.communication_fault {
        res.status = CorrelationStatus::CommunicationFault;
        return res;
    }

    if map_def.rows == 0 || map_def.cols == 0 {
        res.status = CorrelationStatus::DimensionMismatch {
            reason: format!(
                "Table dimensions cannot be zero: {}x{}",
                map_def.rows, map_def.cols
            ),
        };
        return res;
    }

    // Resolve Y-Axis (rows)
    if map_def.rows == 1 {
        res.selected_row = Some(0);
        res.row_coordinate = Some(AxisCoordinate {
            interval_index: 0,
            fractional_pos: 0.0,
            nearest_index: 0,
            is_exact_match: true,
        });
    } else {
        let y_axis_def = match &map_def.y_axis {
            Some(def) => def,
            None => {
                res.status = CorrelationStatus::DimensionMismatch {
                    reason: format!(
                        "Table has {} rows but y_axis definition is missing",
                        map_def.rows
                    ),
                };
                return res;
            }
        };

        let y_desc = AxisDescriptor::from_axis_def(y_axis_def, &map_def.name, AxisRole::YAxis);
        let y_channel = match resolve_axis_channel(&y_desc, config.and_then(|c| c.y_axis.as_ref()))
        {
            Ok(ch) => ch,
            Err(err) => {
                res.status = CorrelationStatus::UnboundChannel {
                    axis_role: AxisRole::YAxis,
                    error: err,
                };
                return res;
            }
        };

        let y_val = match extract_channel_value(frame, y_channel) {
            Ok(v) => v,
            Err(err) => {
                res.status = CorrelationStatus::AxisOutOfRange {
                    axis_role: AxisRole::YAxis,
                    error: err,
                };
                return res;
            }
        };

        let y_breakpoints = match extract_axis_breakpoints(y_axis_def, mem_opt) {
            Ok(bps) => bps,
            Err(err) => {
                res.status = CorrelationStatus::AxisOutOfRange {
                    axis_role: AxisRole::YAxis,
                    error: err,
                };
                return res;
            }
        };

        let y_coord = match resolve_axis_coordinate(&y_breakpoints, y_val) {
            Ok(coord) => coord,
            Err(err) => {
                res.status = CorrelationStatus::AxisOutOfRange {
                    axis_role: AxisRole::YAxis,
                    error: err,
                };
                return res;
            }
        };

        res.selected_row = Some(y_coord.nearest_index);
        res.row_coordinate = Some(y_coord);
    }

    // Resolve X-Axis (cols)
    if map_def.cols == 1 {
        res.selected_col = Some(0);
        res.col_coordinate = Some(AxisCoordinate {
            interval_index: 0,
            fractional_pos: 0.0,
            nearest_index: 0,
            is_exact_match: true,
        });
    } else {
        let x_axis_def = match &map_def.x_axis {
            Some(def) => def,
            None => {
                res.status = CorrelationStatus::DimensionMismatch {
                    reason: format!(
                        "Table has {} columns but x_axis definition is missing",
                        map_def.cols
                    ),
                };
                return res;
            }
        };

        let x_desc = AxisDescriptor::from_axis_def(x_axis_def, &map_def.name, AxisRole::XAxis);
        let x_channel = match resolve_axis_channel(&x_desc, config.and_then(|c| c.x_axis.as_ref()))
        {
            Ok(ch) => ch,
            Err(err) => {
                res.status = CorrelationStatus::UnboundChannel {
                    axis_role: AxisRole::XAxis,
                    error: err,
                };
                return res;
            }
        };

        let x_val = match extract_channel_value(frame, x_channel) {
            Ok(v) => v,
            Err(err) => {
                res.status = CorrelationStatus::AxisOutOfRange {
                    axis_role: AxisRole::XAxis,
                    error: err,
                };
                return res;
            }
        };

        let x_breakpoints = match extract_axis_breakpoints(x_axis_def, mem_opt) {
            Ok(bps) => bps,
            Err(err) => {
                res.status = CorrelationStatus::AxisOutOfRange {
                    axis_role: AxisRole::XAxis,
                    error: err,
                };
                return res;
            }
        };

        let x_coord = match resolve_axis_coordinate(&x_breakpoints, x_val) {
            Ok(coord) => coord,
            Err(err) => {
                res.status = CorrelationStatus::AxisOutOfRange {
                    axis_role: AxisRole::XAxis,
                    error: err,
                };
                return res;
            }
        };

        res.selected_col = Some(x_coord.nearest_index);
        res.col_coordinate = Some(x_coord);
    }

    // Validate cell indices against table dimensions and retrieve cell values
    if let (Some(r), Some(c)) = (res.selected_row, res.selected_col) {
        if r < map_def.rows && c < map_def.cols {
            if let Some(mem) = mem_opt {
                let bytes_per_elem = map_def.dtype.byte_size();
                let idx = if map_def.column_major {
                    c.checked_mul(map_def.rows).and_then(|v| v.checked_add(r))
                } else {
                    r.checked_mul(map_def.cols).and_then(|v| v.checked_add(c))
                };
                if let Some(offset) = idx
                    .and_then(|i| i.checked_mul(bytes_per_elem))
                    .and_then(|b| map_def.address.checked_add(b))
                {
                    if let Ok(raw) = mem.read_value(offset, map_def.dtype, map_def.endian) {
                        res.cell_raw = Some(raw);
                        if let Ok(eng) = evaluate_scaling(&map_def.scaling.equation, raw) {
                            res.cell_eng = Some(eng);
                        } else {
                            res.cell_eng = Some(raw);
                        }
                    }
                }
            }
            res.is_valid = true;
            res.status = CorrelationStatus::Valid;
        } else {
            res.status = CorrelationStatus::DimensionMismatch {
                reason: format!(
                    "Selected cell ({}, {}) out of bounds for table {}x{}",
                    r, c, map_def.rows, map_def.cols
                ),
            };
        }
    }

    res
}
