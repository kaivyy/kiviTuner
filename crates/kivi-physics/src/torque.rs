//! Ported from `ecu_engine_rust::physics::torque`.
//!
//! Pure physics maths: no I/O, no hardware, deterministic.
//!
//! ## Fidelity
//!
//! Constants, thresholds and formulas are reproduced from the source. Any
//! deliberate deviation is documented inline. This module is exercised by
//! `tests/physics_torque_test.rs`; hardware-dependent behaviour is **not** claimed
//! to be validated (there is no device in this environment).

/// Comprehensive Engine Torque Coordinator.
pub struct TorqueStructureEngine {
    /// Maximum engine flywheel torque rating in Nm
    pub max_engine_torque_nm: f64,
}

impl Default for TorqueStructureEngine {
    fn default() -> Self {
        Self {
            max_engine_torque_nm: 250.0, // e.g. 250 Nm turbo engine (or ~14 Nm for 155cc NA)
        }
    }
}

impl TorqueStructureEngine {
    /// Calculate Driver Requested Torque (Nm) from pedal position (0.0% to 100.0%) and RPM.
    pub fn calculate_driver_requested_torque(&self, pedal_percent: f64, rpm: f64) -> f64 {
        let pedal_ratio = (pedal_percent / 100.0).clamp(0.0, 1.0);
        // Linear-progressive throttle map with high-RPM tapering
        let rpm_taper = (1.0 - (rpm / 12000.0).powi(2)).clamp(0.2, 1.0);
        pedal_ratio * self.max_engine_torque_nm * rpm_taper
    }

    /// Estimate internal friction torque losses (Nm) based on engine temperature and RPM.
    /// Cold oil significantly increases friction drag.
    pub fn estimate_friction_loss_torque(&self, ect_celsius: f64, rpm: f64) -> f64 {
        // Base mechanical hydrodynamic friction ~ f(RPM)
        let base_friction = 8.0 + (rpm / 1000.0) * 2.2;
        // Temperature penalty factor (viscous drag of cold engine oil)
        let temp_penalty = if ect_celsius < 85.0 {
            ((85.0 - ect_celsius) / 85.0).max(0.0) * 12.0
        } else {
            0.0
        };
        base_friction + temp_penalty
    }

    /// Estimate pumping losses (Nm) caused by throttling restriction (manifold vacuum).
    pub fn estimate_pumping_loss_torque(&self, map_kpa: f64) -> f64 {
        // Throttling vacuum (100 kPa ambient - actual MAP)
        let vacuum_kpa = (101.3 - map_kpa).max(0.0);
        vacuum_kpa * 0.12 // ~12 Nm loss at high vacuum (30 kPa idle)
    }

    /// Calculate Target Indicated Combustion Torque (T_ind) required to deliver net flywheel torque.
    ///
    /// T_ind = T_driver + T_friction + T_pumping + T_accessories
    pub fn calculate_target_indicated_torque(
        &self,
        driver_torque_nm: f64,
        friction_loss_nm: f64,
        pumping_loss_nm: f64,
        accessory_load_nm: f64,
    ) -> f64 {
        driver_torque_nm + friction_loss_nm + pumping_loss_nm + accessory_load_nm
    }

    /// Fast Path Intervention: Compute required ignition timing retard (degrees) to achieve immediate torque reduction
    /// (e.g. for Traction Control, Launch Control, or Gear Shift Cut).
    ///
    /// Standard Bosch quadratic efficiency formula:
    /// Eta_spark = 1.0 - k * (Delta_theta_deg)^2
    /// where k is typically ~0.0010 to 0.0015 (at 20° retard, torque is reduced by ~50%).
    pub fn calculate_fast_path_spark_retard(&self, target_reduced_torque: f64, current_torque: f64) -> f64 {
        if current_torque <= 0.0 || target_reduced_torque >= current_torque {
            return 0.0; // No intervention needed
        }
        let torque_ratio = (target_reduced_torque / current_torque).clamp(0.15, 1.0);
        let delta_efficiency = (1.0 - torque_ratio).max(0.0);

        // Delta_theta = sqrt( (1 - Eta) / k ) with k = 0.00125
        let k = 0.00125;
        (delta_efficiency / k).sqrt().min(45.0)
    }
}
