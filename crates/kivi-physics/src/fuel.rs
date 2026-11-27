//! Ported from `ecu_engine_rust::physics::fuel`.
//!
//! Pure physics maths: no I/O, no hardware, deterministic.
//!
//! ## Fidelity
//!
//! Constants, thresholds and formulas are reproduced from the source. Any
//! deliberate deviation is documented inline. This module is exercised by
//! `tests/physics_fuel_test.rs`; hardware-dependent behaviour is **not** claimed
//! to be validated (there is no device in this environment).

/// Standard stoichiometric Air-Fuel Ratios (AFR) for different fuels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FuelType {
    PureGasoline, // 14.7:1
    E10Gasoline,  // 14.13:1 (Standard pump gas in many countries)
    E85Ethanol,   // 9.76:1
    PureMethanol, // 6.47:1
    Diesel,       // 14.5:1
    Custom(f64),  // User-defined stoichiometric ratio
}

impl FuelType {
    pub fn stoichiometric_afr(&self) -> f64 {
        match self {
            FuelType::PureGasoline => 14.7,
            FuelType::E10Gasoline => 14.13,
            FuelType::E85Ethanol => 9.76,
            FuelType::PureMethanol => 6.47,
            FuelType::Diesel => 14.5,
            FuelType::Custom(afr) => *afr,
        }
    }

    /// Fuel density in grams per cubic centimeter (g/cc).
    pub fn density_g_per_cc(&self) -> f64 {
        match self {
            FuelType::PureGasoline => 0.737,
            FuelType::E10Gasoline => 0.745,
            FuelType::E85Ethanol => 0.785,
            FuelType::PureMethanol => 0.792,
            FuelType::Diesel => 0.832,
            FuelType::Custom(_) => 0.740,
        }
    }
}

/// Injector Specification and Battery Dead-Time Curve.
pub struct FuelInjector {
    /// Static flow rate in cc/min at reference fuel pressure (typically 300 kPa / 43.5 psi)
    pub flow_rate_cc_min: f64,
    /// Reference fuel rail differential pressure in kPa
    pub ref_pressure_kpa: f64,
    /// Battery voltage curve (Volts) for dead-time lookup
    pub dead_time_volts: [f64; 6],
    /// Dead-time values in milliseconds corresponding to `dead_time_volts`
    pub dead_time_ms: [f64; 6],
}

impl Default for FuelInjector {
    fn default() -> Self {
        // Typical modern high-impedance 200 cc/min injector (Yamaha Aerox / 150cc class)
        Self {
            flow_rate_cc_min: 200.0,
            ref_pressure_kpa: 300.0,
            dead_time_volts: [8.0, 10.0, 12.0, 13.5, 14.0, 16.0],
            dead_time_ms: [1.85, 1.42, 1.08, 0.92, 0.88, 0.74],
        }
    }
}

impl FuelInjector {
    /// Calculate static mass flow rate in milligrams per millisecond (mg/ms).
    ///
    /// 1 cc/min = (density * 1000 mg) / 60,000 ms
    pub fn flow_rate_mg_per_ms(&self, fuel_type: &FuelType, actual_diff_pressure_kpa: f64) -> f64 {
        let density = fuel_type.density_g_per_cc();
        // Bernoulli pressure scaling: Q_actual = Q_ref * sqrt(P_actual / P_ref)
        let pressure_scale = if self.ref_pressure_kpa > 0.0 {
            (actual_diff_pressure_kpa / self.ref_pressure_kpa).max(0.1).sqrt()
        } else {
            1.0
        };
        let scaled_cc_min = self.flow_rate_cc_min * pressure_scale;
        (scaled_cc_min * density * 1000.0) / 60000.0
    }

    /// Interpolate battery voltage latency (dead time) in milliseconds.
    pub fn get_dead_time_ms(&self, battery_volts: f64) -> f64 {
        let n = self.dead_time_volts.len();
        if battery_volts <= self.dead_time_volts[0] {
            return self.dead_time_ms[0];
        }
        if battery_volts >= self.dead_time_volts[n - 1] {
            return self.dead_time_ms[n - 1];
        }
        // Linear interpolation
        for i in 0..n - 1 {
            if battery_volts >= self.dead_time_volts[i] && battery_volts <= self.dead_time_volts[i + 1] {
                let t = (battery_volts - self.dead_time_volts[i])
                    / (self.dead_time_volts[i + 1] - self.dead_time_volts[i]);
                return self.dead_time_ms[i] + t * (self.dead_time_ms[i + 1] - self.dead_time_ms[i]);
            }
        }
        self.dead_time_ms[3] // Default fallback
    }
}

/// Puckett-Aquino Tau-X Transient Fuel Film (Wall-Wetting) Model.
///
/// Models fuel puddle dynamics in port fuel injection (PFI):
/// - A fraction `X` of injected fuel sticks to the intake port walls.
/// - The fuel puddle evaporates with time constant `tau` (seconds).
pub struct WallWettingModel {
    /// Fraction of injected fuel hitting port wall (0.0 to 1.0)
    pub x_fraction: f64,
    /// Evaporation time constant in seconds (typically 0.1s to 0.4s)
    pub tau_seconds: f64,
    /// Current stored fuel film puddle mass on walls in milligrams
    pub puddle_mass_mg: f64,
}

impl Default for WallWettingModel {
    fn default() -> Self {
        Self {
            x_fraction: 0.28,
            tau_seconds: 0.22,
            puddle_mass_mg: 0.0,
        }
    }
}

impl WallWettingModel {
    /// Update wall film puddle dynamics and calculate required injected fuel mass (mg)
    /// to deliver the exact desired cylinder fuel mass (mg).
    ///
    /// Governing equations:
    /// m_evap = puddle_mass / tau * dt
    /// m_cyl = (1 - X) * m_injected + m_evap
    /// => m_injected = (m_cyl_desired - m_evap) / (1 - X)
    pub fn calculate_injected_fuel_mg(&mut self, desired_cyl_fuel_mg: f64, dt_seconds: f64) -> f64 {
        let dt = dt_seconds.max(0.001);
        let evap_rate = self.puddle_mass_mg / self.tau_seconds;
        let m_evap = (evap_rate * dt).min(self.puddle_mass_mg);

        let one_minus_x = (1.0 - self.x_fraction).max(0.05);
        let raw_injected = (desired_cyl_fuel_mg - m_evap) / one_minus_x;
        let m_injected = raw_injected.max(0.0);

        // Update stored puddle mass: dM/dt = X * m_inj - m_evap
        let added_to_puddle = self.x_fraction * m_injected;
        self.puddle_mass_mg = (self.puddle_mass_mg + added_to_puddle - m_evap).max(0.0);

        m_injected
    }
}

/// Comprehensive Fuel Metering Engine.
pub struct FuelController {
    pub fuel_type: FuelType,
    pub injector: FuelInjector,
    pub wall_wetting: WallWettingModel,
}

impl FuelController {
    pub fn new(fuel_type: FuelType, injector: FuelInjector) -> Self {
        Self {
            fuel_type,
            injector,
            wall_wetting: WallWettingModel::default(),
        }
    }

    /// Calculate total gross injector pulse width in milliseconds (ms).
    ///
    /// Parameters:
    /// - `air_mass_cylinder_grams`: Calculated air charge mass
    /// - `target_lambda`: Lambda target (1.0 = stoichiometric, 0.85 = rich power, 1.05 = lean cruise)
    /// - `battery_volts`: Vehicle electrical system voltage
    /// - `rail_diff_pressure_kpa`: Differential fuel pressure
    /// - `cycle_dt_sec`: Engine cycle period in seconds (120 / RPM for 4-stroke single cylinder)
    pub fn calculate_pulse_width_ms(
        &mut self,
        air_mass_cylinder_grams: f64,
        target_lambda: f64,
        battery_volts: f64,
        rail_diff_pressure_kpa: f64,
        cycle_dt_sec: f64,
    ) -> (f64, f64, f64) {
        let target_afr = self.fuel_type.stoichiometric_afr() * target_lambda;
        let desired_fuel_grams = air_mass_cylinder_grams / target_afr;
        let desired_fuel_mg = desired_fuel_grams * 1000.0;

        // Apply transient wall wetting compensation
        let compensated_fuel_mg = self.wall_wetting.calculate_injected_fuel_mg(desired_fuel_mg, cycle_dt_sec);

        let flow_rate_mg_ms = self.injector.flow_rate_mg_per_ms(&self.fuel_type, rail_diff_pressure_kpa);
        let effective_pw_ms = compensated_fuel_mg / flow_rate_mg_ms;
        let dead_time_ms = self.injector.get_dead_time_ms(battery_volts);
        let gross_pw_ms = effective_pw_ms + dead_time_ms;

        (gross_pw_ms, effective_pw_ms, dead_time_ms)
    }
}
