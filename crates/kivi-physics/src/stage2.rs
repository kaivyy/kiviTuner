//! Ported from `ecu_engine_rust::physics::stage2`.
//!
//! Pure physics maths: no I/O, no hardware access, deterministic.
//! Constants and formulas are reproduced from the source; deviations are noted
//! inline. Verified by unit tests with hand-derived expectations, **not** by
//! comparison against a physical ECU.

// `DtcSwitchMatrix` lives in the flashing crate, which is not yet a dependency
// here. `stage2` only needs it for the DTC-switch helper; that helper is
// deferred with the rest of the flashing port.
use crate::dtc::DtcSwitchMatrix;

/// Target Fuel Lambda Calibration Configuration.
#[derive(Debug, Clone, Copy)]
pub struct Stage2FuelConfig {
    /// Target Lambda at High RPM / WOT (typically 0.82 - 0.84 for gasoline, AFR ~ 12.0 - 12.3)
    pub wot_target_lambda: f64,
    /// RPM threshold above which WOT enrichment applies
    pub high_rpm_threshold: f64,
}

impl Default for Stage2FuelConfig {
    fn default() -> Self {
        Self {
            wot_target_lambda: 0.83,
            high_rpm_threshold: 4000.0,
        }
    }
}

/// Stage 2 Ignition Spark Advance Configuration.
#[derive(Debug, Clone, Copy)]
pub struct Stage2IgnitionConfig {
    /// Fixed advance addition in degrees (typically +1.5° to +3.0° on RON 98)
    pub advance_degrees_addition: f64,
    /// Absolute maximum ignition timing clamp in degrees BTDC (e.g. 42.0° to prevent engine knock)
    pub max_timing_clamp_deg: f64,
    /// Load/MAP threshold (e.g. 70.0 kPa or 60% TPS) above which advance applies
    pub high_load_threshold: f64,
}

impl Default for Stage2IgnitionConfig {
    fn default() -> Self {
        Self {
            advance_degrees_addition: 2.0, // +2.0 degrees safe baseline for Stage 2
            max_timing_clamp_deg: 40.0,
            high_load_threshold: 65.0,
        }
    }
}

/// Stage 2 Turbo Boost Configuration.
#[derive(Debug, Clone, Copy)]
pub struct Stage2BoostConfig {
    /// Percentage multiplier on Target Absolute Manifold Pressure (e.g. 1.15 = +15% boost, ~0.25 bar)
    pub boost_target_multiplier: f64,
    /// Base Wastegate Duty Cycle (WGDC) percentage addition (e.g. +6.0% duty cycle)
    pub wgdc_offset_addition: f64,
    /// Hard overboost safety cutoff ceiling in hPa (e.g. 2850 hPa)
    pub overboost_cutoff_ceiling_hpa: f64,
}

impl Default for Stage2BoostConfig {
    fn default() -> Self {
        Self {
            boost_target_multiplier: 1.15, // +15% boost
            wgdc_offset_addition: 5.0,     // +5% WGDC
            overboost_cutoff_ceiling_hpa: 2850.0,
        }
    }
}

/// Stage 2 Calibration Transformer Engine.
pub struct Stage2Transformer;

impl Stage2Transformer {
    /// Transform Volumetric Efficiency (VE) / Fuel Table for Stage 2 free-flow intake and decat exhaust.
    ///
    /// Increases volumetric efficiency in high-airflow regions where backpressure was eliminated.
    pub fn transform_ve_surface(
        ve_matrix: &mut [Vec<f64>],
        rpm_axis: &[f64],
        load_axis: &[f64],
        intake_exhaust_flow_gain_pct: f64, // Typically 8.0% to 15.0%
    ) -> Result<(), &'static str> {
        let rows = ve_matrix.len();
        if rows == 0 || rows != load_axis.len() || ve_matrix[0].len() != rpm_axis.len() {
            return Err("VE table dimensions must match axis lengths");
        }

        let gain_factor = 1.0 + (intake_exhaust_flow_gain_pct / 100.0);

        for (r, row) in ve_matrix.iter_mut().enumerate() {
            let load = load_axis[r];
            for (c, cell) in row.iter_mut().enumerate() {
                let rpm = rpm_axis[c];
                // Apply flow gain primarily in mid-to-high RPM and high load
                if rpm >= 3500.0 && load >= 60.0 {
                    let rpm_weight = ((rpm - 3500.0) / 4500.0).clamp(0.0, 1.0);
                    let load_weight = ((load - 60.0) / 40.0).clamp(0.0, 1.0);
                    let combined_weight = rpm_weight * load_weight;

                    let local_gain = 1.0 + (gain_factor - 1.0) * combined_weight;
                    *cell = (*cell * local_gain).min(1.35); // Clamp max VE at 135%
                }
            }
        }
        Ok(())
    }

    /// Transform Ignition Spark Advance Table for Stage 2 (RON 98 + High Flow).
    pub fn transform_ignition_timing(
        spark_matrix: &mut [Vec<f64>],
        rpm_axis: &[f64],
        load_axis: &[f64],
        config: &Stage2IgnitionConfig,
    ) -> Result<(), &'static str> {
        let rows = spark_matrix.len();
        if rows == 0 || rows != load_axis.len() || spark_matrix[0].len() != rpm_axis.len() {
            return Err("Spark table dimensions must match axis lengths");
        }

        for (r, row) in spark_matrix.iter_mut().enumerate() {
            let load = load_axis[r];
            for (c, cell) in row.iter_mut().enumerate() {
                let rpm = rpm_axis[c];
                // Advance timing in power zones (mid-high RPM, high load)
                if rpm >= 3000.0 && load >= config.high_load_threshold {
                    let new_advance = *cell + config.advance_degrees_addition;
                    *cell = new_advance.min(config.max_timing_clamp_deg);
                }
            }
        }
        Ok(())
    }

    /// Transform Boost Target Table for Turbocharged Applications.
    pub fn transform_boost_surface(
        boost_matrix: &mut [Vec<f64>],
        config: &Stage2BoostConfig,
    ) {
        for row in boost_matrix.iter_mut() {
            for cell in row.iter_mut() {
                let boosted = *cell * config.boost_target_multiplier;
                *cell = boosted.min(config.overboost_cutoff_ceiling_hpa);
            }
        }
    }

    /// Unlock Torque Limiters by applying positive scaling (+20% - 25%).
    pub fn unlock_torque_limiters(
        torque_limit_matrix: &mut [Vec<f64>],
        percentage_increase: f64,
    ) {
        let factor = 1.0 + (percentage_increase / 100.0);
        for row in torque_limit_matrix.iter_mut() {
            for cell in row.iter_mut() {
                *cell *= factor;
            }
        }
    }

    /// Apply Stage 2 Decat DTC Delete (disables P0420 secondary catalytic converter fault).
    pub fn apply_decat_patch(rom: &mut [u8], cdkat_switch_address: usize) -> Result<(), &'static str> {
        DtcSwitchMatrix::patch_dtc_disable(rom, cdkat_switch_address)
    }
}
