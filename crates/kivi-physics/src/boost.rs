//! Ported from `ecu_engine_rust::physics::boost`.
//!
//! Pure physics maths: no I/O, no hardware access, deterministic.
//! Constants and formulas are reproduced from the source; deviations are noted
//! inline. Verified by unit tests with hand-derived expectations, **not** by
//! comparison against a physical ECU.

/// Turbocharger Closed-Loop PID Controller with Anti-Windup.
pub struct BoostController {
    pub kp: f64,
    pub ki: f64,
    pub kd: f64,
    /// Anti-windup integral clamp limits (Duty Cycle %)
    pub i_min: f64,
    pub i_max: f64,
    /// Output duty cycle limits (Duty Cycle %, typically 0% to 95%)
    pub wgdc_min: f64,
    pub wgdc_max: f64,
    /// Overboost safety shutdown threshold in hPa (e.g. 2600 hPa = 1.6 bar gauge)
    pub overboost_cutoff_hpa: f64,

    // Internal controller state
    integral_acc: f64,
    prev_error: f64,
}

impl Default for BoostController {
    fn default() -> Self {
        Self {
            kp: 0.08,             // Proportional gain (e.g. 0.08% WGDC per hPa error)
            ki: 0.04,             // Integral gain
            kd: 0.015,            // Derivative gain
            i_min: -20.0,         // Max negative integral trim (-20% WGDC)
            i_max: 20.0,          // Max positive integral trim (+20% WGDC)
            wgdc_min: 0.0,        // Fully open wastegate (spring base pressure)
            wgdc_max: 95.0,       // Max safe solenoid duty cycle
            overboost_cutoff_hpa: 2600.0,
            integral_acc: 0.0,
            prev_error: 0.0,
        }
    }
}

impl BoostController {
    /// Update boost closed-loop control and calculate final commanded Wastegate Duty Cycle (%).
    ///
    /// Parameters:
    /// - `target_boost_hpa`: Target manifold absolute pressure in hPa (e.g. 2200 hPa)
    /// - `actual_boost_hpa`: Measured manifold absolute pressure from MAP sensor in hPa
    /// - `base_feedforward_wgdc`: Open-loop pre-control table value (RPM x Load) in %
    /// - `dt_seconds`: Control loop period in seconds (e.g. 0.02s for 50 Hz loop)
    ///
    /// Returns:
    /// - `Result<(final_wgdc, p_term, i_term, d_term), OverboostCutoff>`
    pub fn update(
        &mut self,
        target_boost_hpa: f64,
        actual_boost_hpa: f64,
        base_feedforward_wgdc: f64,
        dt_seconds: f64,
    ) -> Result<(f64, f64, f64, f64), &'static str> {
        // 1. Overboost Safety Protection Check
        if actual_boost_hpa >= self.overboost_cutoff_hpa {
            self.integral_acc = 0.0; // Reset integrator on safety event
            return Err("OVERBOOST_CUTOFF_ACTIVE: Boost pressure exceeds hard engine safety limit!");
        }

        let dt = dt_seconds.clamp(0.001, 0.5);
        let error = target_boost_hpa - actual_boost_hpa;

        // 2. Proportional Term
        let p_term = self.kp * error;

        // 3. Integral Term with Anti-Windup Clamping
        self.integral_acc = (self.integral_acc + self.ki * error * dt).clamp(self.i_min, self.i_max);
        let i_term = self.integral_acc;

        // 4. Derivative Term (filtered on error rate of change)
        let d_term = self.kd * ((error - self.prev_error) / dt);
        self.prev_error = error;

        // 5. Combine Feedforward Pre-Control + Closed-Loop Feedback PID
        let unconstrained_wgdc = base_feedforward_wgdc + p_term + i_term + d_term;
        let final_wgdc = unconstrained_wgdc.clamp(self.wgdc_min, self.wgdc_max);

        Ok((final_wgdc, p_term, i_term, d_term))
    }

    /// Reset internal controller state.
    pub fn reset(&mut self) {
        self.integral_acc = 0.0;
        self.prev_error = 0.0;
    }
}
