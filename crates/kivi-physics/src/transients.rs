//! Ported from `ecu_engine_rust::physics::transients`.
//!
//! Pure physics maths: no I/O, no hardware access, deterministic.
//! Constants and formulas are reproduced from the source; deviations are noted
//! inline. Verified by unit tests with hand-derived expectations, **not** by
//! comparison against a physical ECU.

/// Deceleration Fuel Cut-Off (DFCO) State Machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DfcoState {
    Disabled,
    Monitoring,
    ActiveFuelCut,
    SoftRecoveryRamp { ramp_step: u8 },
}

pub struct DfcoController {
    pub state: DfcoState,
    pub min_rpm_enable: f64,    // e.g. 1600 RPM
    pub min_rpm_disable: f64,   // e.g. 1200 RPM (hysteresis)
    pub max_tps_enable_pct: f64,// e.g. 1.2% (closed throttle)
    pub min_speed_kph: f64,     // e.g. 25 km/h
    pub min_ect_celsius: f64,   // e.g. 60.0°C (engine fully warmed up)
}

impl Default for DfcoController {
    fn default() -> Self {
        Self {
            state: DfcoState::Disabled,
            min_rpm_enable: 1600.0,
            min_rpm_disable: 1200.0,
            max_tps_enable_pct: 1.2,
            min_speed_kph: 25.0,
            min_ect_celsius: 60.0,
        }
    }
}

impl DfcoController {
    /// Update DFCO state machine and return fuel multiplier (0.0 = full cut, 1.0 = normal delivery).
    pub fn update(&mut self, rpm: f64, tps_pct: f64, speed_kph: f64, ect_celsius: f64) -> f64 {
        // Pre-requisite condition: engine must be warm
        if ect_celsius < self.min_ect_celsius {
            self.state = DfcoState::Disabled;
            return 1.0;
        }

        match self.state {
            DfcoState::Disabled | DfcoState::Monitoring => {
                if tps_pct <= self.max_tps_enable_pct
                    && rpm >= self.min_rpm_enable
                    && speed_kph >= self.min_speed_kph
                {
                    self.state = DfcoState::ActiveFuelCut;
                    0.0 // Cut fuel 100%
                } else {
                    self.state = DfcoState::Monitoring;
                    1.0
                }
            }
            DfcoState::ActiveFuelCut => {
                // Exit conditions: driver steps on gas OR engine RPM approaches idle
                if tps_pct > self.max_tps_enable_pct || rpm <= self.min_rpm_disable || speed_kph < 10.0 {
                    self.state = DfcoState::SoftRecoveryRamp { ramp_step: 1 };
                    0.35 // Start soft re-entry at 35% fuel to prevent driveline shock
                } else {
                    0.0 // Remain in fuel cut
                }
            }
            DfcoState::SoftRecoveryRamp { ramp_step } => {
                if ramp_step >= 5 {
                    self.state = DfcoState::Monitoring;
                    1.0 // Fully recovered to normal fueling
                } else {
                    self.state = DfcoState::SoftRecoveryRamp { ramp_step: ramp_step + 1 };
                    0.35 + (ramp_step as f64 * 0.15) // Smooth linear ramp: 35% -> 50% -> 65% -> 80% -> 95% -> 100%
                }
            }
        }
    }
}

/// Anti-Stall Spark Intervention (Stall Saver).
/// Instantly advances ignition timing when engine RPM dips below target idle before mechanical IAC can react.
pub struct AntiStallSaver {
    pub critical_stall_rpm: f64, // e.g. 550 RPM (normal idle is ~750 RPM)
    pub max_spark_adder_deg: f64, // e.g. +12.0 degrees BTDC
}

impl Default for AntiStallSaver {
    fn default() -> Self {
        Self {
            critical_stall_rpm: 550.0,
            max_spark_adder_deg: 12.0,
        }
    }
}

impl AntiStallSaver {
    /// Calculate instant emergency ignition spark adder degrees.
    pub fn calculate_spark_adder(&self, current_rpm: f64, target_idle_rpm: f64) -> f64 {
        if current_rpm >= target_idle_rpm {
            return 0.0;
        }

        let undershoot = target_idle_rpm - current_rpm;
        let stall_window = (target_idle_rpm - self.critical_stall_rpm).max(50.0);

        let ratio = (undershoot / stall_window).clamp(0.0, 1.0);
        ratio * self.max_spark_adder_deg
    }
}

/// Highway Lean Cruise Economy Mode.
/// Leans out target AFR during steady-state cruising to maximize fuel economy.
pub struct HighwayLeanCruise {
    pub min_speed_kph: f64,     // e.g. 70 km/h
    pub min_gear: u8,          // e.g. 5th or 6th gear
    pub steady_timer_seconds: f64,
    pub required_delay_seconds: f64, // e.g. 20.0 seconds of steady cruising
    pub max_lean_lambda: f64,   // e.g. 1.10 (AFR ~ 16.2 for gasoline)
    pub spark_advance_adder_deg: f64, // e.g. +4.5 degrees (compensates for slower lean flame speed)
}

impl Default for HighwayLeanCruise {
    fn default() -> Self {
        Self {
            min_speed_kph: 70.0,
            min_gear: 5,
            steady_timer_seconds: 0.0,
            required_delay_seconds: 20.0,
            max_lean_lambda: 1.10,
            spark_advance_adder_deg: 4.5,
        }
    }
}

impl HighwayLeanCruise {
    /// Update cruising state and return (lambda_multiplier, spark_advance_adder).
    pub fn update(
        &mut self,
        speed_kph: f64,
        current_gear: u8,
        delta_tps_abs: f64, // rate of throttle pedal movement (%/s)
        dt_seconds: f64,
    ) -> (f64, f64) {
        let is_steady = speed_kph >= self.min_speed_kph
            && current_gear >= self.min_gear
            && delta_tps_abs < 2.0; // Driver holding steady throttle

        if is_steady {
            self.steady_timer_seconds += dt_seconds;
            if self.steady_timer_seconds >= self.required_delay_seconds {
                // Lean cruise active!
                return (self.max_lean_lambda, self.spark_advance_adder_deg);
            }
        } else {
            self.steady_timer_seconds = 0.0; // Reset timer immediately on acceleration or braking
        }

        (1.0, 0.0) // Normal stoich cruising
    }
}

/// Idle Air Control (IAC) Feedforward Anticipator.
/// Predicts engine load additions before RPM drops to maintain dead-flat idle stability.
pub struct IdleAirAnticipator {
    pub base_idle_steps: u16,
    pub ac_compressor_adder_steps: u16,  // Steps added when A/C turns on
    pub radiator_fan_adder_steps: u16,   // Steps added when cooling fan starts
    pub drive_gear_adder_steps: u16,     // Steps added when shifted into Drive (automatic transmission)
}

impl Default for IdleAirAnticipator {
    fn default() -> Self {
        Self {
            base_idle_steps: 45,
            ac_compressor_adder_steps: 18,
            radiator_fan_adder_steps: 8,
            drive_gear_adder_steps: 12,
        }
    }
}

impl IdleAirAnticipator {
    /// Calculate commanded total IAC stepper motor steps.
    pub fn calculate_commanded_steps(
        &self,
        base_target_steps: u16,
        ac_clutch_engaged: bool,
        radiator_fan_active: bool,
        in_drive_gear: bool,
    ) -> u16 {
        let mut steps = base_target_steps;
        if ac_clutch_engaged {
            steps += self.ac_compressor_adder_steps;
        }
        if radiator_fan_active {
            steps += self.radiator_fan_adder_steps;
        }
        if in_drive_gear {
            steps += self.drive_gear_adder_steps;
        }
        steps
    }
}

/// Piecewise Multi-Segment Mass Air Flow (MAF) Linearizer.
/// Reconstructs GM Delco / Bosch piecewise polynomial MAF voltage-to-airflow curves.
pub struct PiecewiseMafLinearizer {
    pub voltage_breakpoints: Vec<f64>,
    pub airflow_gps_values: Vec<f64>, // grams per second
}

impl Default for PiecewiseMafLinearizer {
    fn default() -> Self {
        // Typical Bosch / Hitachi hot-wire 0-5V curve (idle ~ 4 g/s up to WOT ~ 280 g/s)
        Self {
            voltage_breakpoints: vec![0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.5, 4.0, 4.5, 5.0],
            airflow_gps_values: vec![2.5, 5.8, 12.0, 24.5, 45.0, 80.0, 130.0, 195.0, 275.0, 360.0],
        }
    }
}

impl PiecewiseMafLinearizer {
    /// Calculate air mass flow rate in grams per second (g/s) from raw sensor voltage.
    pub fn voltage_to_airflow_gps(&self, sensor_volts: f64) -> f64 {
        let v = sensor_volts.clamp(self.voltage_breakpoints[0], self.voltage_breakpoints[self.voltage_breakpoints.len() - 1]);

        let mut idx = 0;
        while idx < self.voltage_breakpoints.len() - 1 && self.voltage_breakpoints[idx + 1] < v {
            idx += 1;
        }

        let v0 = self.voltage_breakpoints[idx];
        let v1 = self.voltage_breakpoints[idx + 1];
        let f0 = self.airflow_gps_values[idx];
        let f1 = self.airflow_gps_values[idx + 1];

        let ratio = if (v1 - v0).abs() < 1e-6 { 0.0 } else { (v - v0) / (v1 - v0) };
        f0 + ratio * (f1 - f0)
    }
}
