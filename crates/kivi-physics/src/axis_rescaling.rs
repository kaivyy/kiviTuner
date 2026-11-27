//! Ported from `ecu_engine_rust::physics::axis_rescaling`.
//!
//! Pure physics maths: no I/O, no hardware access, deterministic.
//! Constants and formulas are reproduced from the source; deviations are noted
//! inline. Verified by unit tests with hand-derived expectations, **not** by
//! comparison against a physical ECU.

use kivi_defs::interp::lerp_1d;

/// Fuel Injector Flow Scaling & Dead-Time (Battery Latency) Adjuster.
pub struct InjectorScaler;

impl InjectorScaler {
    /// Rescale injection pulse width (PW) when swapping to larger/smaller fuel injectors.
    ///
    /// Equation:
    /// PW_new = (PW_old - DeadTime_old) * (Flow_old / Flow_new) + DeadTime_new
    pub fn rescale_pulse_width(
        pw_old_ms: f64,
        dead_time_old_ms: f64,
        flow_old_cc: f64,
        flow_new_cc: f64,
        dead_time_new_ms: f64,
    ) -> Result<f64, &'static str> {
        if flow_old_cc <= 0.0 || flow_new_cc <= 0.0 {
            return Err("Injector flow rate must be positive");
        }
        let effective_pw_old = (pw_old_ms - dead_time_old_ms).max(0.0);
        let flow_ratio = flow_old_cc / flow_new_cc;
        let effective_pw_new = effective_pw_old * flow_ratio;
        Ok(effective_pw_new + dead_time_new_ms)
    }
}

/// 1D Table Axis Resampler.
pub struct AxisResampler;

impl AxisResampler {
    /// Resample a 1D curve from an old axis grid to a newly rescaled axis grid using linear interpolation.
    ///
    /// Clamps endpoints gracefully outside the original range.
    pub fn resample_1d(
        old_axis: &[f64],
        old_values: &[f64],
        new_axis: &[f64],
    ) -> Result<Vec<f64>, &'static str> {
        if old_axis.len() != old_values.len() || old_axis.is_empty() {
            return Err("Old axis and values must have equal non-zero length");
        }
        if new_axis.is_empty() {
            return Err("New axis cannot be empty");
        }

        let mut resampled = Vec::with_capacity(new_axis.len());

        for &target_x in new_axis {
            if target_x <= old_axis[0] {
                resampled.push(old_values[0]);
            } else if target_x >= old_axis[old_axis.len() - 1] {
                resampled.push(old_values[old_values.len() - 1]);
            } else {
                // Find bounding interval
                let mut idx = 0;
                while idx < old_axis.len() - 1 && old_axis[idx + 1] < target_x {
                    idx += 1;
                }
                let x0 = old_axis[idx];
                let x1 = old_axis[idx + 1];
                let y0 = old_values[idx];
                let y1 = old_values[idx + 1];
                resampled.push(lerp_1d(x0, y0, x1, y1, target_x));
            }
        }

        Ok(resampled)
    }
}
