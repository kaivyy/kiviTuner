//! Thermodynamic models for internal combustion engines.
//!
//! Ported from `ecu_engine_rust::physics::thermo`. Implements unit conversions,
//! dynamic charge-temperature estimation, and the speed-density air-mass model.
//!
//! All functions are pure and deterministic.

/// Specific gas constant for dry air, `J / (kg · K)`.
pub const R_SPECIFIC_AIR: f64 = 287.058;

/// Standard atmospheric pressure at sea level, in pascals.
pub const P_STD_ATMOSPHERE: f64 = 101325.0;

/// Standard temperature (20 °C) in kelvin.
pub const T_STD_KELVIN: f64 = 293.15;

/// Converts Celsius to Kelvin.
#[inline]
pub fn celsius_to_kelvin(celsius: f64) -> f64 {
    celsius + 273.15
}

/// Converts Kelvin to Celsius.
#[inline]
pub fn kelvin_to_celsius(kelvin: f64) -> f64 {
    kelvin - 273.15
}

/// Converts cubic centimetres to cubic metres.
#[inline]
pub fn cc_to_cubic_meters(cc: f64) -> f64 {
    cc * 1e-6
}

/// Converts kilopascals to pascals.
#[inline]
pub fn kpa_to_pascals(kpa: f64) -> f64 {
    kpa * 1000.0
}

/// Dynamic manifold charge-temperature estimation.
///
/// Air entering the intake runner absorbs heat from the head and runner walls.
/// At low airflow (idle) it spends longer in contact and approaches coolant
/// temperature; at high airflow (WOT) it approaches intake air temperature.
///
/// `T_charge = T_iat + (T_ect - T_iat) · beta(airflow)`
pub struct ChargeTemperatureModel {
    /// Heat-transfer coefficient at zero airflow (idle). Typically 0.65–0.85.
    pub beta_idle: f64,
    /// Heat-transfer coefficient at maximum airflow (WOT). Typically 0.05–0.15.
    pub beta_wot: f64,
    /// Airflow (g/s) at which the blend is half-way between idle and WOT.
    pub airflow_half_point: f64,
}

impl Default for ChargeTemperatureModel {
    fn default() -> Self {
        Self {
            beta_idle: 0.75,
            beta_wot: 0.10,
            airflow_half_point: 25.0,
        }
    }
}

impl ChargeTemperatureModel {
    /// Blending factor for a given airflow, using exponential decay from idle.
    pub fn calculate_beta(&self, airflow_gps: f64) -> f64 {
        let normalized = (airflow_gps / self.airflow_half_point).max(0.0);
        let weight = (-normalized).exp();
        self.beta_wot + (self.beta_idle - self.beta_wot) * weight
    }

    /// Estimated charge temperature in kelvin.
    pub fn estimate_t_charge_k(
        &self,
        iat_celsius: f64,
        ect_celsius: f64,
        airflow_gps: f64,
    ) -> f64 {
        let t_iat_k = celsius_to_kelvin(iat_celsius);
        let t_ect_k = celsius_to_kelvin(ect_celsius);
        let beta = self.calculate_beta(airflow_gps);
        t_iat_k + (t_ect_k - t_iat_k) * beta
    }
}

/// Speed-density engine model.
pub struct SpeedDensityEngine {
    /// Total displacement in cubic centimetres.
    pub displacement_cc: f64,
    /// Cylinder count.
    pub num_cylinders: u32,
    /// Charge-temperature estimator.
    pub charge_temp_model: ChargeTemperatureModel,
}

impl SpeedDensityEngine {
    /// Creates an engine with the default charge-temperature model.
    pub fn new(displacement_cc: f64, num_cylinders: u32) -> Self {
        Self {
            displacement_cc,
            num_cylinders,
            charge_temp_model: ChargeTemperatureModel::default(),
        }
    }

    /// Volume of one cylinder in cubic metres.
    #[inline]
    pub fn cylinder_volume_m3(&self) -> f64 {
        cc_to_cubic_meters(self.displacement_cc / self.num_cylinders as f64)
    }

    /// Air mass inducted per cylinder per intake stroke, in grams.
    ///
    /// `m_air = (P_map · V_cyl · VE) / (R_air · T_charge)`
    ///
    /// `map_kpa` is manifold absolute pressure, `ve_ratio` a fraction (0.85 = 85%),
    /// `iat_celsius` / `ect_celsius` are temperatures, `rpm` engine speed.
    pub fn calculate_cylinder_air_mass_grams(
        &self,
        map_kpa: f64,
        ve_ratio: f64,
        iat_celsius: f64,
        ect_celsius: f64,
        rpm: f64,
    ) -> f64 {
        let p_pascals = kpa_to_pascals(map_kpa);
        let v_cyl = self.cylinder_volume_m3();

        // Approximate airflow to feed the charge-temperature model.
        let est_strokes_per_sec = (rpm / 60.0) * (self.num_cylinders as f64 / 2.0);
        let crude_air_density =
            p_pascals / (R_SPECIFIC_AIR * celsius_to_kelvin(iat_celsius));
        let est_airflow_gps =
            crude_air_density * v_cyl * ve_ratio * est_strokes_per_sec * 1000.0;

        let t_charge_k =
            self.charge_temp_model
                .estimate_t_charge_k(iat_celsius, ect_celsius, est_airflow_gps);

        let m_air_kg = (p_pascals * v_cyl * ve_ratio) / (R_SPECIFIC_AIR * t_charge_k);
        m_air_kg * 1000.0
    }

    /// Back-calculates volumetric efficiency from measured mass airflow (g/s).
    ///
    /// Returns `0.0` when `rpm` or `map_kpa` is non-positive, matching the source.
    pub fn calculate_ve_from_maf(
        &self,
        maf_gps: f64,
        map_kpa: f64,
        iat_celsius: f64,
        ect_celsius: f64,
        rpm: f64,
    ) -> f64 {
        if rpm <= 0.0 || map_kpa <= 0.0 {
            return 0.0;
        }
        let p_pascals = kpa_to_pascals(map_kpa);
        let v_total_m3 = cc_to_cubic_meters(self.displacement_cc);
        let t_charge_k =
            self.charge_temp_model
                .estimate_t_charge_k(iat_celsius, ect_celsius, maf_gps);
        let maf_kg_per_sec = maf_gps / 1000.0;

        // 4-stroke: total intake volume per second = V_total · (RPM / 120).
        let swept_volume_per_sec = v_total_m3 * (rpm / 120.0);
        let theoretical_mass_flow =
            (p_pascals * swept_volume_per_sec) / (R_SPECIFIC_AIR * t_charge_k);

        maf_kg_per_sec / theoretical_mass_flow
    }
}
