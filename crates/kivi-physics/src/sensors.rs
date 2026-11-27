//! Ported from `ecu_engine_rust::physics::sensors`.
//!
//! Pure physics maths: no I/O, no hardware access, deterministic.
//! Constants and formulas are reproduced from the source; deviations are noted
//! inline. Verified by unit tests with hand-derived expectations, **not** by
//! comparison against a physical ECU.

use super::thermo::{celsius_to_kelvin, kelvin_to_celsius};

/// Steinhart-Hart NTC Thermistor Calibration Model.
///
/// Governing equation:
/// 1 / T = A + B * ln(R) + C * (ln(R))^3
/// (where T is in Kelvin, R is in Ohms)
#[derive(Debug, Clone, Copy)]
pub struct SteinhartHartModel {
    pub a: f64,
    pub b: f64,
    pub c: f64,
}

impl Default for SteinhartHartModel {
    fn default() -> Self {
        // Typical automotive standard GM / Bosch NTC coolant temperature sensor
        // Calibration points: -20°C (15100 ohm), 20°C (2500 ohm), 80°C (330 ohm)
        Self {
            a: 1.40e-3,
            b: 2.37e-4,
            c: 9.90e-8,
        }
    }
}

impl SteinhartHartModel {
    /// Calculate temperature in Celsius from thermistor resistance in Ohms.
    pub fn resistance_to_celsius(&self, resistance_ohms: f64) -> f64 {
        if resistance_ohms <= 0.0 {
            return -40.0;
        }
        let ln_r = resistance_ohms.ln();
        let inv_t = self.a + self.b * ln_r + self.c * ln_r.powi(3);
        if inv_t <= 0.0 {
            return 150.0;
        }
        kelvin_to_celsius(1.0 / inv_t)
    }

    /// Calculate thermistor resistance in Ohms from temperature in Celsius.
    pub fn celsius_to_resistance(&self, celsius: f64) -> f64 {
        let t_k = celsius_to_kelvin(celsius);
        let y = (self.a - (1.0 / t_k)) / (2.0 * self.c);
        let x = ((self.b / (3.0 * self.c)).powi(3) + y.powi(2)).sqrt();
        let ln_r = (x - y).cbrt() - (x + y).cbrt();
        ln_r.exp()
    }

    /// Analytical 3-Point Solver for Steinhart-Hart Coefficients.
    ///
    /// Given 3 experimental pairs: (T1_celsius, R1_ohms), (T2_celsius, R2_ohms), (T3_celsius, R3_ohms),
    /// solves the 3x3 linear system via Gaussian elimination.
    pub fn solve_from_three_points(
        t1_c: f64,
        r1: f64,
        t2_c: f64,
        r2: f64,
        t3_c: f64,
        r3: f64,
    ) -> Result<Self, &'static str> {
        let y1 = 1.0 / celsius_to_kelvin(t1_c);
        let y2 = 1.0 / celsius_to_kelvin(t2_c);
        let y3 = 1.0 / celsius_to_kelvin(t3_c);

        let l1 = r1.ln();
        let l2 = r2.ln();
        let l3 = r3.ln();

        let l1_3 = l1.powi(3);
        let l2_3 = l2.powi(3);
        let l3_3 = l3.powi(3);

        // Solve using Cramer's Rule for:
        // [ 1  L1  L1^3 ] [ A ] = [ Y1 ]
        // [ 1  L2  L2^3 ] [ B ]   [ Y2 ]
        // [ 1  L3  L3^3 ] [ C ]   [ Y3 ]

        let det = (l2 - l1) * (l3_3 - l1_3) - (l3 - l1) * (l2_3 - l1_3);
        if det.abs() < 1e-15 {
            return Err("Singular matrix: calibration points are too close or identical");
        }

        let delta_y2 = y2 - y1;
        let delta_y3 = y3 - y1;

        let delta_l2 = l2 - l1;
        let delta_l3 = l3 - l1;

        let delta_l2_3 = l2_3 - l1_3;
        let delta_l3_3 = l3_3 - l1_3;

        let c = (delta_y3 * delta_l2 - delta_y2 * delta_l3) / (delta_l3_3 * delta_l2 - delta_l2_3 * delta_l3);
        let b = (delta_y2 - c * delta_l2_3) / delta_l2;
        let a = y1 - b * l1 - c * l1_3;

        Ok(Self { a, b, c })
    }
}

/// Analog-to-Digital Converter (ADC) Circuit Model for Pull-up Thermistors.
pub struct AdcThermistorCircuit {
    pub pullup_resistor_ohms: f64,
    pub max_adc_counts: u32,
    pub steinhart_hart: SteinhartHartModel,
}

impl Default for AdcThermistorCircuit {
    fn default() -> Self {
        Self {
            pullup_resistor_ohms: 2490.0, // Standard 2.49k precision pull-up in automotive ECUs
            max_adc_counts: 1023,        // 10-bit ADC
            steinhart_hart: SteinhartHartModel::default(),
        }
    }
}

impl AdcThermistorCircuit {
    /// Convert raw ADC counts to temperature in Celsius.
    pub fn adc_to_celsius(&self, adc_count: u32) -> f64 {
        let clamped_adc = adc_count.clamp(1, self.max_adc_counts - 1) as f64;
        let max_adc = self.max_adc_counts as f64;

        // Voltage divider: V_adc = V_ref * (R_therm / (R_pullup + R_therm))
        // => R_therm = R_pullup * (ADC / (MAX_ADC - ADC))
        let r_thermistor = self.pullup_resistor_ohms * (clamped_adc / (max_adc - clamped_adc));
        self.steinhart_hart.resistance_to_celsius(r_thermistor)
    }
}

/// Manifold Absolute Pressure (MAP) Sensor Linear Transfer Function.
pub struct MapSensor {
    /// Pressure at minimum voltage output (0.5V) in kPa
    pub p_min_kpa: f64,
    /// Pressure at maximum voltage output (4.5V) in kPa
    pub p_max_kpa: f64,
    /// Minimum output voltage (typically 0.5V)
    pub v_min: f64,
    /// Maximum output voltage (typically 4.5V)
    pub v_max: f64,
}

impl Default for MapSensor {
    fn default() -> Self {
        // Standard 1-bar naturally aspirated MAP sensor (e.g. GM 1-bar or Yamaha sensor)
        Self {
            p_min_kpa: 10.0,
            p_max_kpa: 105.0,
            v_min: 0.5,
            v_max: 4.5,
        }
    }
}

impl MapSensor {
    /// Calculate pressure in kPa from measured sensor voltage.
    pub fn voltage_to_kpa(&self, voltage: f64) -> f64 {
        let v_clamped = voltage.clamp(self.v_min, self.v_max);
        let slope = (self.p_max_kpa - self.p_min_kpa) / (self.v_max - self.v_min);
        self.p_min_kpa + (v_clamped - self.v_min) * slope
    }
}

/// Throttle Position Sensor (TPS) Calibrator.
pub struct ThrottlePositionSensor {
    pub adc_closed: u32,
    pub adc_wot: u32,
}

impl Default for ThrottlePositionSensor {
    fn default() -> Self {
        Self {
            adc_closed: 180, // ~0.88V on 10-bit ADC
            adc_wot: 920,    // ~4.50V on 10-bit ADC
        }
    }
}

impl ThrottlePositionSensor {
    /// Calculate percentage throttle opening (0.0% to 100.0%).
    pub fn adc_to_percent(&self, adc_raw: u32) -> f64 {
        let clamped = adc_raw.clamp(self.adc_closed, self.adc_wot);
        let range = (self.adc_wot - self.adc_closed) as f64;
        if range <= 0.0 {
            return 0.0;
        }
        ((clamped - self.adc_closed) as f64 / range) * 100.0
    }
}
