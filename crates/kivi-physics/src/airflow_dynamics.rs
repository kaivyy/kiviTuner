//! Ported from `ecu_engine_rust::physics::airflow_dynamics`.
//!
//! Pure physics maths: no I/O, no hardware access, deterministic.
//! Constants and formulas are reproduced from the source; deviations are noted
//! inline. Verified by unit tests with hand-derived expectations, **not** by
//! comparison against a physical ECU.

/// Physical constants for dry air.
pub const R_AIR: f64 = 287.058;     // J/(kg·K)
pub const GAMMA_AIR: f64 = 1.40;    // Specific heat ratio (Cp/Cv)

/// Compressible Orifice Flow Calculator (Saint-Venant & Wantzel formulation).
pub struct CompressibleThrottleFlow;

impl CompressibleThrottleFlow {
    /// Critical pressure ratio where flow reaches speed of sound (Mach 1) at throttle plate:
    /// r_crit = (2 / (gamma + 1))^(gamma / (gamma - 1)) ≈ 0.52828 for air.
    pub fn critical_pressure_ratio() -> f64 {
        let exponent = GAMMA_AIR / (GAMMA_AIR - 1.0);
        (2.0 / (GAMMA_AIR + 1.0)).powf(exponent)
    }

    /// Normalized flow factor Psi(r) as a function of downstream/upstream pressure ratio r = P_down / P_up.
    pub fn flow_factor_psi(pressure_ratio: f64) -> f64 {
        let r = pressure_ratio.clamp(0.01, 1.0);
        let r_crit = Self::critical_pressure_ratio();

        if r <= r_crit {
            // Sonic flow (choked flow): Throat air velocity is pegged at speed of sound!
            let factor = GAMMA_AIR / (GAMMA_AIR + 1.0);
            let exp = 2.0 / (GAMMA_AIR - 1.0);
            (factor * (2.0 / (GAMMA_AIR + 1.0)).powf(exp)).sqrt()
        } else {
            // Subsonic flow: Flow rate depends on pressure differential
            let factor = GAMMA_AIR / (GAMMA_AIR - 1.0);
            let term1 = r.powf(2.0 / GAMMA_AIR);
            let term2 = r.powf((GAMMA_AIR + 1.0) / GAMMA_AIR);
            (factor * (term1 - term2)).max(0.0).sqrt()
        }
    }

    /// Calculate instantaneous mass flow rate (kg/s) through throttle body:
    /// m_dot = C_d * A(alpha) * P_amb / sqrt(R * T_amb) * sqrt(2) * Psi(r).
    pub fn calculate_throttle_mass_flow(
        effective_area_m2: f64,
        p_ambient_pa: f64,
        t_ambient_kelvin: f64,
        p_manifold_pa: f64,
    ) -> f64 {
        let r = p_manifold_pa / p_ambient_pa.max(1.0);
        let psi = Self::flow_factor_psi(r);
        let density_factor = (2.0 / (R_AIR * t_ambient_kelvin.max(200.0))).sqrt();
        effective_area_m2 * p_ambient_pa * density_factor * psi
    }
}

/// Dynamic Intake Manifold Filling Integrator (Bosch Motronic / Continental).
/// dP_m / dt = (R * T_m / V_m) * (m_dot_in - m_dot_cyl)
pub struct ManifoldFillingModel {
    pub manifold_volume_m3: f64,
    pub current_pressure_pa: f64,
    pub manifold_temp_kelvin: f64,
}

impl ManifoldFillingModel {
    pub fn new(manifold_volume_m3: f64, initial_pressure_pa: f64, manifold_temp_kelvin: f64) -> Self {
        Self {
            manifold_volume_m3: manifold_volume_m3.max(0.0005),
            current_pressure_pa: initial_pressure_pa,
            manifold_temp_kelvin,
        }
    }

    /// Update manifold air pressure using Euler integration step over dt_seconds:
    /// Returns (new_pressure_kpa, dP_dt_kpa_per_sec).
    pub fn integrate_step(
        &mut self,
        mass_flow_in_kg_s: f64,
        engine_displacement_m3: f64,
        engine_rpm: f64,
        volumetric_efficiency: f64,
        dt_seconds: f64,
    ) -> (f64, f64) {
        // Engine pumping discharge rate: m_dot_cyl = (VE * Vd * N / 120) * (P_m / (R * T_m))
        let pumping_rate_m3_s = (volumetric_efficiency * engine_displacement_m3 * engine_rpm) / 120.0;
        let cylinder_air_density = self.current_pressure_pa / (R_AIR * self.manifold_temp_kelvin);
        let mass_flow_out_kg_s = pumping_rate_m3_s * cylinder_air_density;

        // Differential pressure rate
        let net_mass_rate = mass_flow_in_kg_s - mass_flow_out_kg_s;
        let dp_dt = (R_AIR * self.manifold_temp_kelvin / self.manifold_volume_m3) * net_mass_rate;

        // Forward Euler step
        self.current_pressure_pa = (self.current_pressure_pa + dp_dt * dt_seconds).clamp(15_000.0, 450_000.0);

        (self.current_pressure_pa / 1000.0, dp_dt / 1000.0)
    }
}

/// Ion-Sensing Spark Plug Combustion Diagnostics (Saab Trionic / BMW S85 V10).
pub struct IonSensingCombustionModel {
    pub dc_bias_volts: f64, // Typically 80.0 V
}

impl Default for IonSensingCombustionModel {
    fn default() -> Self {
        Self { dc_bias_volts: 80.0 }
    }
}

impl IonSensingCombustionModel {
    /// Detect Peak Cylinder Pressure (PCP) angle and Detonation (Knock) from raw ionization current signal:
    /// Returns (peak_pressure_crank_deg, is_detonation_present, knock_intensity_uv).
    pub fn analyze_ionization_trace(
        &self,
        crank_angles_deg: &[f64],
        ion_current_microamps: &[f64],
    ) -> (f64, bool, f64) {
        if crank_angles_deg.is_empty() || crank_angles_deg.len() != ion_current_microamps.len() {
            return (15.0, false, 0.0);
        }

        // 1. Find thermal ionization peak (typically 12° to 22° ATDC in optimal MBT combustion)
        let mut max_val = f64::NEG_INFINITY;
        let mut pcp_angle = 15.0;

        for (i, &deg) in crank_angles_deg.iter().enumerate() {
            if deg >= 8.0 && deg <= 35.0 {
                let current = ion_current_microamps[i];
                if current > max_val {
                    max_val = current;
                    pcp_angle = deg;
                }
            }
        }

        // 2. High-Frequency Detonation Ripple Extraction (6 kHz - 8 kHz bandpass filter equivalent)
        // Detonation creates rapid alternating peak-and-trough oscillations (zigzag ripple)
        let mut ripple_energy = 0.0;
        let mut sign_reversals = 0;

        if ion_current_microamps.len() >= 4 {
            for i in 1..ion_current_microamps.len() - 1 {
                let diff_prev = ion_current_microamps[i] - ion_current_microamps[i - 1];
                let diff_next = ion_current_microamps[i + 1] - ion_current_microamps[i];

                if (diff_prev * diff_next) < 0.0 {
                    sign_reversals += 1;
                    ripple_energy += (diff_prev - diff_next).abs();
                }
            }
        }

        // True knock requires both high alternating frequency (>= 3 sign reversals) and high ripple amplitude
        let is_knock = sign_reversals >= 3 && ripple_energy > 40.0;
        (pcp_angle, is_knock, ripple_energy)
    }
}

/// Aerodynamic Turbocharger Compressor Operating Boundary Model.
pub struct TurboCompressorAero {
    pub surge_poly_a0: f64,
    pub surge_poly_a1: f64,
    pub surge_poly_a2: f64,
    pub choke_flow_max_kg_s: f64,
}

impl Default for TurboCompressorAero {
    fn default() -> Self {
        Self {
            // Quadratic surge limit line: Pi_surge = 1.05 + 12.0 * m_corr - 15.0 * m_corr^2
            surge_poly_a0: 1.05,
            surge_poly_a1: 12.0,
            surge_poly_a2: -15.0,
            choke_flow_max_kg_s: 0.45, // Max choke flow for 450 HP turbo
        }
    }
}

impl TurboCompressorAero {
    /// Calculate corrected mass flow rate: m_corr = m_act * sqrt(T_in / T_std) / (P_in / P_std).
    pub fn corrected_mass_flow(actual_mass_flow_kg_s: f64, t_in_k: f64, p_in_kpa: f64) -> f64 {
        let t_std = 298.15;   // 25 °C
        let p_std = 101.325;  // 101.325 kPa
        actual_mass_flow_kg_s * (t_in_k / t_std).sqrt() / (p_in_kpa / p_std)
    }

    /// Check if current compressor operating point (Pressure Ratio Pi vs Corrected Flow) falls into SURGE or CHOKE.
    pub fn evaluate_compressor_safety(
        &self,
        pressure_ratio: f64,
        corrected_mass_flow_kg_s: f64,
    ) -> (&'static str, bool) {
        let m = corrected_mass_flow_kg_s.max(0.0);

        // 1. Choke Check
        if m >= self.choke_flow_max_kg_s {
            return ("TURBO_CHOKE_SONIC_LIMIT", false);
        }

        // 2. Surge Boundary Check: Pi_surge = a0 + a1*m + a2*m^2
        let pi_surge_limit = self.surge_poly_a0 + self.surge_poly_a1 * m + self.surge_poly_a2 * m * m;

        if pressure_ratio > pi_surge_limit {
            // Dangerous compressor surge / flow detachment!
            ("DANGEROUS_COMPRESSOR_SURGE", false)
        } else {
            ("STABLE_AERODYNAMIC_OPERATION", true)
        }
    }
}
