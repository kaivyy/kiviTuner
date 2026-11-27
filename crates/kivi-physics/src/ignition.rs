//! Ported from `ecu_engine_rust::physics::ignition`.
//!
//! Pure physics maths: no I/O, no hardware, deterministic.
//!
//! ## Fidelity
//!
//! Constants, thresholds and formulas are reproduced from the source. Any
//! deliberate deviation is documented inline. This module is exercised by
//! `tests/physics_ignition_test.rs`; hardware-dependent behaviour is **not** claimed
//! to be validated (there is no device in this environment).

/// Parameters for the Wiebe combustion mass fraction burned (MFB) model.
///
/// Models the fraction of fuel energy released as a function of crank angle.
/// Formula: x_b(theta) = 1 - exp( -a * ((theta - theta_0) / delta_theta)^(m + 1) )
pub struct WiebeCombustionModel {
    /// Wiebe efficiency parameter 'a' (typically 5.0 for complete combustion)
    pub a: f64,
    /// Wiebe form factor 'm' (typically 2.0 for spark ignition engines)
    pub m: f64,
    /// Total combustion duration in crank angle degrees (typically 40° to 60°)
    pub duration_degrees: f64,
}

impl Default for WiebeCombustionModel {
    fn default() -> Self {
        Self {
            a: 5.0,
            m: 2.0,
            duration_degrees: 70.0, // Typical combustion duration (60° to 75° at 7000 RPM)
        }
    }
}

impl WiebeCombustionModel {
    /// Calculate the Mass Fraction Burned (0.0 to 1.0) at a given crank angle.
    ///
    /// Parameters:
    /// - `crank_angle`: Current crank angle in degrees (-180 to +180 relative to TDC)
    /// - `spark_timing_btdc`: Commanded ignition timing in degrees Before Top Dead Center (BTDC)
    pub fn mass_fraction_burned(&self, crank_angle_atdc: f64, spark_timing_btdc: f64) -> f64 {
        let theta_0 = -spark_timing_btdc; // Start of spark in ATDC coordinates
        if crank_angle_atdc <= theta_0 {
            return 0.0;
        }
        let progress = (crank_angle_atdc - theta_0) / self.duration_degrees;
        if progress >= 1.0 {
            return 1.0;
        }
        1.0 - (-self.a * progress.powf(self.m + 1.0)).exp()
    }

    /// Calculate the crank angle degree where 50% of the fuel is burned (MFB50 / theta_50).
    /// For Maximum Brake Torque (MBT), theta_50 should optimally occur at 8° to 10° ATDC.
    pub fn calculate_mfb50_angle_atdc(&self, spark_timing_btdc: f64) -> f64 {
        // x_b = 0.5 => 0.5 = exp( -a * (prog)^(m+1) )
        // ln(0.5) = -a * prog^(m+1) => prog = ( -ln(0.5) / a )^(1 / (m+1))
        let half_burn_progress = (-0.5f64.ln() / self.a).powf(1.0 / (self.m + 1.0));
        let theta_0 = -spark_timing_btdc;
        theta_0 + (half_burn_progress * self.duration_degrees)
    }
}

/// Dynamic Closed-Loop Knock Retard Controller.
///
/// Features rapid retard on knock detection and gradual recovery decay.
pub struct KnockRetardController {
    /// Current knock retard angle in degrees (always >= 0.0)
    pub current_retard_deg: f64,
    /// Instant retard angle added per knock event (degrees, e.g. 2.0°)
    pub retard_step_deg: f64,
    /// Maximum allowable knock retard (degrees, e.g. 12.0°)
    pub max_retard_deg: f64,
    /// Recovery advance rate in degrees per second (e.g. 0.5° / sec)
    pub recovery_rate_deg_per_sec: f64,
}

impl Default for KnockRetardController {
    fn default() -> Self {
        Self {
            current_retard_deg: 0.0,
            retard_step_deg: 2.0,
            max_retard_deg: 12.0,
            recovery_rate_deg_per_sec: 0.75,
        }
    }
}

impl KnockRetardController {
    /// Triggered when knock sensor signal exceeds threshold.
    pub fn on_knock_detected(&mut self, severity_multiplier: f64) {
        let step = self.retard_step_deg * severity_multiplier.max(1.0);
        self.current_retard_deg = (self.current_retard_deg + step).min(self.max_retard_deg);
    }

    /// Update recovery advance decay during knock-free periods.
    pub fn update_recovery(&mut self, dt_seconds: f64) {
        if self.current_retard_deg > 0.0 {
            let recovery = self.recovery_rate_deg_per_sec * dt_seconds;
            self.current_retard_deg = (self.current_retard_deg - recovery).max(0.0);
        }
    }
}

/// Ignition System Orchestrator.
pub struct IgnitionController {
    pub wiebe_model: WiebeCombustionModel,
    pub knock_controller: KnockRetardController,
    /// Maximum allowable mechanical spark advance (degrees BTDC)
    pub max_spark_advance_btdc: f64,
    /// Minimum allowable spark advance (retard limit, e.g. -10° BTDC / 10° ATDC)
    pub min_spark_advance_btdc: f64,
}

impl Default for IgnitionController {
    fn default() -> Self {
        Self {
            wiebe_model: WiebeCombustionModel::default(),
            knock_controller: KnockRetardController::default(),
            max_spark_advance_btdc: 55.0,
            min_spark_advance_btdc: -10.0,
        }
    }
}

impl IgnitionController {
    /// Calculate final commanded spark timing in degrees BTDC.
    ///
    /// Parameters:
    /// - `base_timing_btdc`: Interpolated value from Base Spark Map (RPM x Load)
    /// - `ect_correction_deg`: Coolant temperature spark adder (warmup advance)
    /// - `iat_correction_deg`: Intake air temperature spark trim (retard on hot air to prevent knock)
    pub fn calculate_commanded_spark_timing(
        &self,
        base_timing_btdc: f64,
        ect_correction_deg: f64,
        iat_correction_deg: f64,
    ) -> f64 {
        let unconstrained_timing = base_timing_btdc + ect_correction_deg + iat_correction_deg
            - self.knock_controller.current_retard_deg;

        // Apply ECU hard safety limits
        unconstrained_timing
            .max(self.min_spark_advance_btdc)
            .min(self.max_spark_advance_btdc)
    }
}
