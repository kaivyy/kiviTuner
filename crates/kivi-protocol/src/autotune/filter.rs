use super::config::AutoTuneConfig;
use super::types::RejectionReason;
use crate::model::TelemetryFrame;

/// Stateful sample eligibility filter and rate-of-change tracker.
#[derive(Debug, Default)]
pub struct SampleFilter {
    last_timestamp_ms: Option<u64>,
    last_tps: Option<f32>,
}

impl SampleFilter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Resets the internal state.
    pub fn reset(&mut self) {
        self.last_timestamp_ms = None;
        self.last_tps = None;
    }

    /// Evaluates whether a telemetry frame qualifies for auto-tune analysis.
    ///
    /// Returns:
    /// - `Ok(effective_weight)`: Sample is eligible with a statistical weight in (0.0, 1.0].
    /// - `Err(RejectionReason)`: Sample is disqualified with an explicit reason.
    pub fn evaluate_frame(
        &mut self,
        frame: &TelemetryFrame,
        config: &AutoTuneConfig,
    ) -> Result<f64, RejectionReason> {
        // 1. Hardware and communication status flags (do not pollute TPS delta history with invalid samples)
        if frame.status_flags.stale_sample {
            return Err(RejectionReason::Stale);
        }
        if frame.status_flags.communication_fault {
            return Err(RejectionReason::Fault);
        }

        // 2. Finite numbers check
        if !frame.afr.is_finite()
            || !frame.rpm.is_finite()
            || !frame.tps.is_finite()
            || !frame.map_kpa.is_finite()
            || !frame.ect_c.is_finite()
            || !frame.injector_pulse_width_ms.is_finite()
        {
            return Err(RejectionReason::NonFinite);
        }

        // 3. Sensor bounds validation
        if frame.afr < config.min_afr || frame.afr > config.max_afr {
            self.update_history(frame);
            return Err(RejectionReason::SensorOutOfRange);
        }

        // 4. Engine warm-up state (reject cold start enrichment)
        if frame.ect_c < config.min_ect_c {
            self.update_history(frame);
            return Err(RejectionReason::ColdEngine);
        }

        // 5. Deceleration overrun fuel cut (injector shut off)
        if frame.injector_pulse_width_ms <= 0.0 {
            self.update_history(frame);
            return Err(RejectionReason::FuelCut);
        }

        // 6. Engine operating zone (min/max RPM)
        if frame.rpm < config.min_rpm || frame.rpm > config.max_rpm {
            self.update_history(frame);
            return Err(RejectionReason::OutOfBounds);
        }

        // 7. Transient throttle rate-of-change check (|dTPS/dt|)
        let mut sample_weight = 1.0;
        if let (Some(prev_time), Some(prev_tps)) = (self.last_timestamp_ms, self.last_tps) {
            if frame.timestamp_ms > prev_time {
                let dt_sec = (frame.timestamp_ms - prev_time) as f32 / 1000.0;
                let dtps_dt = (frame.tps - prev_tps).abs() / dt_sec;

                if dtps_dt > config.max_tps_delta_per_sec {
                    self.update_history(frame);
                    return Err(RejectionReason::TransientThrottle);
                }

                // If sample spacing is very tight (< 50ms, i.e. > 20Hz), discount weight to account for temporal dependence
                if dt_sec < 0.050 {
                    sample_weight = (dt_sec / 0.050).clamp(0.2, 1.0) as f64;
                }
            } else if frame.timestamp_ms == prev_time {
                // Consecutive sample with identical timestamp -> temporal duplicate
                sample_weight = 0.2;
            }
        }

        self.update_history(frame);
        Ok(sample_weight)
    }

    fn update_history(&mut self, frame: &TelemetryFrame) {
        self.last_timestamp_ms = Some(frame.timestamp_ms);
        self.last_tps = Some(frame.tps);
    }
}
