//! Ported from `ecu_engine_rust::physics::chemistry`.
//!
//! Pure physics maths: no I/O, no hardware access, deterministic.
//! Constants and formulas are reproduced from the source; deviations are noted
//! inline. Verified by unit tests with hand-derived expectations, **not** by
//! comparison against a physical ECU.

/// Standard Fuel Type with Known Stoichiometric Air-Fuel Ratio (AFR).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CombustionFuel {
    GasolinePureE0,  // 14.70 : 1
    GasoholE10,      // 14.08 : 1 (Standard commercial pump fuel)
    FlexFuelE85,     // 9.76 : 1
    EthanolPureE100, // 9.00 : 1
    MethanolPureM100,// 6.47 : 1
    Diesel,          // 14.50 : 1
    LpgPropane,      // 15.67 : 1
    CngMethane,      // 17.20 : 1
}

impl CombustionFuel {
    /// Return the exact stoichiometric Air-to-Fuel Ratio (AFR).
    pub fn stoichiometric_afr(&self) -> f64 {
        match self {
            CombustionFuel::GasolinePureE0 => 14.70,
            CombustionFuel::GasoholE10 => 14.08,
            CombustionFuel::FlexFuelE85 => 9.76,
            CombustionFuel::EthanolPureE100 => 9.00,
            CombustionFuel::MethanolPureM100 => 6.47,
            CombustionFuel::Diesel => 14.50,
            CombustionFuel::LpgPropane => 15.67,
            CombustionFuel::CngMethane => 17.20,
        }
    }

    /// Latent heat of vaporization in kJ/kg (evaporative charge cooling capacity).
    pub fn heat_of_vaporization_kj_kg(&self) -> f64 {
        match self {
            CombustionFuel::GasolinePureE0 | CombustionFuel::GasoholE10 => 350.0,
            CombustionFuel::FlexFuelE85 => 720.0,
            CombustionFuel::EthanolPureE100 => 840.0,
            CombustionFuel::MethanolPureM100 => 1100.0,
            CombustionFuel::Diesel => 270.0,
            CombustionFuel::LpgPropane | CombustionFuel::CngMethane => 425.0,
        }
    }
}

/// Flex-Fuel Sensor & Blend Chemistry Calculator.
pub struct FlexFuelCalculator;

impl FlexFuelCalculator {
    /// Calculate effective stoichiometric AFR for an arbitrary Ethanol percentage (0% to 100%).
    ///
    /// Blend Equation:
    /// AFR_stoich(E%) = (1 - E/100) * 14.70 + (E/100) * 9.00
    pub fn calculate_blend_stoich_afr(ethanol_percent: f64) -> f64 {
        let e_ratio = (ethanol_percent / 100.0).clamp(0.0, 1.0);
        (1.0 - e_ratio) * 14.70 + e_ratio * 9.00
    }

    /// Calculate the fuel volumetric delivery scaling multiplier relative to pure gasoline (E0).
    /// Used by flex-fuel ECUs to scale base injector pulse width.
    pub fn calculate_fuel_volume_multiplier(ethanol_percent: f64) -> f64 {
        let stoich_blend = Self::calculate_blend_stoich_afr(ethanol_percent);
        // Multiplier = 14.70 / stoich_blend
        14.70 / stoich_blend
    }

    /// Convert raw Wideband sensor Lambda to measured Air-Fuel Ratio (AFR).
    #[inline]
    pub fn lambda_to_afr(lambda: f64, fuel_type: CombustionFuel) -> f64 {
        lambda * fuel_type.stoichiometric_afr()
    }

    /// Convert measured Air-Fuel Ratio (AFR) to dimensionless Lambda.
    #[inline]
    pub fn afr_to_lambda(afr: f64, fuel_type: CombustionFuel) -> f64 {
        afr / fuel_type.stoichiometric_afr()
    }
}

/// Turbocharger Compressor Thermodynamics & Intercooler Efficiency Model.
pub struct TurbochargerThermodynamics;

impl TurbochargerThermodynamics {
    /// Calculate air temperature exiting turbocharger compressor outlet.
    ///
    /// Equation: T_out = T_in * [ 1 + (1 / eta_c) * ( (P_out / P_in)^((gamma - 1) / gamma) - 1 ) ]
    /// - `t_in_celsius`: Ambient air temperature at air filter
    /// - `pressure_ratio`: P_out / P_in (e.g. 2.2 bar absolute / 1.0 bar ambient = 2.2)
    /// - `compressor_isentropic_efficiency`: eta_c (typically 0.68 - 0.76)
    pub fn compressor_discharge_temp_celsius(
        t_in_celsius: f64,
        pressure_ratio: f64,
        compressor_isentropic_efficiency: f64,
    ) -> Result<f64, &'static str> {
        if pressure_ratio < 1.0 {
            return Err("Pressure ratio must be >= 1.0");
        }
        let eta_c = compressor_isentropic_efficiency.clamp(0.40, 0.95);
        let t_in_kelvin = t_in_celsius + 273.15;
        let gamma = 1.40; // Specific heat ratio of dry air
        let exp = (gamma - 1.0) / gamma; // 0.2857

        let ideal_temp_ratio = pressure_ratio.powf(exp);
        let t_out_kelvin = t_in_kelvin * (1.0 + (ideal_temp_ratio - 1.0) / eta_c);

        Ok(t_out_kelvin - 273.15)
    }

    /// Calculate manifold charge temperature after passing through the intercooler heat exchanger.
    ///
    /// Equation: T_manifold = T_hot_in - epsilon_ic * (T_hot_in - T_ambient)
    pub fn intercooler_outlet_temp_celsius(
        t_hot_in_celsius: f64,
        t_ambient_celsius: f64,
        intercooler_efficiency: f64, // typically 0.70 to 0.85
    ) -> f64 {
        let eps = intercooler_efficiency.clamp(0.0, 1.0);
        t_hot_in_celsius - eps * (t_hot_in_celsius - t_ambient_celsius)
    }
}

/// Exhaust Gas Temperature (EGT) Estimation & Thermal Component Protection (Bosch BTS / ATR).
pub struct ExhaustThermalProtection;

impl ExhaustThermalProtection {
    /// Estimate Exhaust Gas Temperature (EGT) in Celsius based on engine load, RPM, Lambda, and Spark Retard.
    ///
    /// Principles:
    /// - Retarding ignition timing increases EGT (~15°C per degree of retard) because combustion spills into exhaust.
    /// - Enriching fuel (lowering Lambda) decreases EGT (~60°C per 0.1 Lambda reduction) via evaporative cooling.
    pub fn estimate_egt_celsius(
        base_egt_celsius: f64,
        current_lambda: f64,
        spark_retard_from_mbt_deg: f64,
    ) -> f64 {
        // Lambda cooling effect (reference stoichiometric lambda = 1.0)
        let lambda_delta = current_lambda - 1.0;
        let lambda_temp_effect = lambda_delta * 600.0; // e.g. Lambda 0.85 (-0.15) cools EGT by -90°C

        // Spark retard heating effect
        let spark_temp_effect = spark_retard_from_mbt_deg.max(0.0) * 14.5; // +14.5°C per degree retard

        (base_egt_celsius + lambda_temp_effect + spark_temp_effect).clamp(200.0, 1150.0)
    }

    /// Calculate required Component Protection Target Lambda (Bosch BTS `KFFWZ`)
    /// when measured or modeled EGT exceeds safe turbine/catalyst limits (typically 900°C - 950°C).
    pub fn calculate_component_protection_lambda(
        current_egt_celsius: f64,
        max_safe_egt_celsius: f64, // e.g. 920.0°C
        nominal_target_lambda: f64,
    ) -> f64 {
        if current_egt_celsius <= max_safe_egt_celsius {
            return nominal_target_lambda;
        }

        // For every 10°C over limit, enrich Lambda by 0.015
        let over_temp = current_egt_celsius - max_safe_egt_celsius;
        let enrichment = (over_temp / 10.0) * 0.015;
        (nominal_target_lambda - enrichment).clamp(0.72, nominal_target_lambda)
    }
}
