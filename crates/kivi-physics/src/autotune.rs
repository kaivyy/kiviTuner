//! Ported from `ecu_engine_rust::physics::autotune`.
//!
//! Pure physics maths: no I/O, no hardware access, deterministic.
//! Constants and formulas are reproduced from the source; deviations are noted
//! inline. Verified by unit tests with hand-derived expectations, **not** by
//! comparison against a physical ECU.

/// Datalog sample point captured from vehicle telemetry.
#[derive(Debug, Clone, Copy)]
pub struct TelemetrySample {
    pub timestamp_ms: u64,
    pub rpm: f64,
    pub map_kpa: f64,
    pub tps_percent: f64,
    pub ect_celsius: f64,
    pub measured_afr: f64,
    pub target_afr: f64,
}

/// Transient lockout filter parameters to isolate steady-state operation.
pub struct TransientFilterConfig {
    /// Maximum allowed rate of change of TPS (%/second)
    pub max_dtps_pct_per_sec: f64,
    /// Maximum allowed rate of change of MAP (kPa/second)
    pub max_dmap_kpa_per_sec: f64,
    /// Minimum coolant temperature for closed-loop tuning (°C)
    pub min_ect_celsius: f64,
    /// Maximum allowable lean AFR to reject Deceleration Fuel Cutoff (DFCO)
    pub max_valid_afr: f64,
    /// Minimum allowable rich AFR to reject sensor errors
    pub min_valid_afr: f64,
}

impl Default for TransientFilterConfig {
    fn default() -> Self {
        Self {
            max_dtps_pct_per_sec: 15.0,  // Filter out rapid throttle movements
            max_dmap_kpa_per_sec: 25.0,  // Filter out rapid manifold pressure transients
            min_ect_celsius: 75.0,       // Warm engine only
            max_valid_afr: 17.5,         // Reject DFCO cutoff
            min_valid_afr: 10.0,         // Reject sensor flooding
        }
    }
}

/// Exhaust Gas Transport Delay Model.
///
/// Exhaust gas takes finite time to travel from the exhaust valve to the wideband O2 sensor.
/// Transport delay is inversely proportional to mass airflow (or engine speed).
pub struct TransportDelayModel {
    /// Distance from exhaust valve to O2 sensor in meters (e.g. 0.45 m)
    pub sensor_distance_meters: f64,
}

impl Default for TransportDelayModel {
    fn default() -> Self {
        Self { sensor_distance_meters: 0.45 }
    }
}

impl TransportDelayModel {
    /// Estimate exhaust transport delay in milliseconds based on RPM.
    ///
    /// Approximate exhaust gas velocity ranges from 15 m/s at idle to 90 m/s at redline.
    pub fn estimate_delay_ms(&self, rpm: f64) -> u64 {
        let clamped_rpm = rpm.clamp(800.0, 12000.0);
        // Linear velocity model: v = 15 + (rpm - 800) / (12000 - 800) * 75
        let velocity_m_s = 15.0 + ((clamped_rpm - 800.0) / 11200.0) * 75.0;
        let delay_sec = self.sensor_distance_meters / velocity_m_s;
        (delay_sec * 1000.0) as u64
    }
}

/// Automated Volumetric Efficiency (VE) Map Corrector.
pub struct VeAutoTuneEngine {
    pub filter_config: TransientFilterConfig,
    pub delay_model: TransportDelayModel,
    /// Learning rate gamma (0.1 to 0.5): blends new correction into current VE cell
    pub learning_rate: f64,
}

impl Default for VeAutoTuneEngine {
    fn default() -> Self {
        Self {
            filter_config: TransientFilterConfig::default(),
            delay_model: TransportDelayModel::default(),
            learning_rate: 0.35, // 35% correction per batch step
        }
    }
}

impl VeAutoTuneEngine {
    /// Determine if a telemetry sample is in steady-state (not transient, not in DFCO, warm engine).
    pub fn is_sample_valid(&self, current: &TelemetrySample, previous: &TelemetrySample) -> bool {
        // Temperature check
        if current.ect_celsius < self.filter_config.min_ect_celsius {
            return false;
        }

        // AFR validity (reject DFCO or sensor errors)
        if current.measured_afr > self.filter_config.max_valid_afr
            || current.measured_afr < self.filter_config.min_valid_afr
        {
            return false;
        }

        let dt_sec = ((current.timestamp_ms.saturating_sub(previous.timestamp_ms)) as f64) / 1000.0;
        if dt_sec <= 0.001 || dt_sec > 1.0 {
            return false;
        }

        // Calculate derivatives
        let dtps = (current.tps_percent - previous.tps_percent).abs() / dt_sec;
        let dmap = (current.map_kpa - previous.map_kpa).abs() / dt_sec;

        if dtps > self.filter_config.max_dtps_pct_per_sec {
            return false;
        }
        if dmap > self.filter_config.max_dmap_kpa_per_sec {
            return false;
        }

        true
    }

    /// Calculate the recommended corrected VE cell value based on measured AFR feedback.
    ///
    /// Formula:
    /// VE_target = VE_current * (AFR_measured / AFR_target)
    /// VE_new = (1 - gamma) * VE_current + gamma * VE_target
    pub fn calculate_corrected_ve_cell(
        &self,
        current_ve: f64,
        measured_afr: f64,
        target_afr: f64,
    ) -> f64 {
        if target_afr <= 0.0 || measured_afr <= 0.0 {
            return current_ve;
        }

        let ideal_target_ve = current_ve * (measured_afr / target_afr);
        
        // Clamp maximum single-step correction to +/- 20% to prevent overshoot
        let max_correction = current_ve * 1.20;
        let min_correction = current_ve * 0.80;
        let bounded_target = ideal_target_ve.clamp(min_correction, max_correction);

        // Exponential smoothing update
        (1.0 - self.learning_rate) * current_ve + self.learning_rate * bounded_target
    }
}
