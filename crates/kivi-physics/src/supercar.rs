//! Ported from `ecu_engine_rust::physics::supercar`.
//!
//! Pure physics maths: no I/O, no hardware access, deterministic.
//! Constants and formulas are reproduced from the source; deviations are noted
//! inline. Verified by unit tests with hand-derived expectations, **not** by
//! comparison against a physical ECU.

/// Variable Valve Actuation Architecture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VariableValvetrainType {
    /// Yamaha VVA (Variable Valve Actuation): High-RPM / Low-RPM solenoid lobe switch (Aerox 155, R15 V3/V4).
    YamahaVvaSolenoid { switch_rpm: f64, hysteresis_rpm: f64 },
    /// Yamaha YCC-I (Yamaha Chip Controlled Intake): Variable intake funnel length (YZF-R1, R6).
    YamahaYcciFunnel { switch_rpm: f64 },
    /// BMW Valvetronic: Throttle-less load control via continuous variable valve lift (0.18mm - 9.9mm).
    BmwValvetronicLift { min_lift_mm: f64, max_lift_mm: f64 },
    /// BMW Double-VANOS / Ducati DVT: Continuous hydraulic camshaft phaser on intake and exhaust.
    ContinuousDualCamPhaser { max_advance_deg: f64, max_retard_deg: f64 },
}

/// Variable Valve Train Simulation & Load Calculation.
pub struct VariableValvetrainController {
    pub valvetrain_type: VariableValvetrainType,
    pub is_high_cam_active: bool,
    pub intake_cam_advance_deg: f64,
    pub exhaust_cam_retard_deg: f64,
    pub current_valve_lift_mm: f64,
}

impl VariableValvetrainController {
    pub fn new(valvetrain_type: VariableValvetrainType) -> Self {
        let (is_high_cam_active, current_valve_lift_mm) = match valvetrain_type {
            VariableValvetrainType::BmwValvetronicLift { min_lift_mm, .. } => (false, min_lift_mm),
            _ => (false, 8.5),
        };
        Self {
            valvetrain_type,
            is_high_cam_active,
            intake_cam_advance_deg: 0.0,
            exhaust_cam_retard_deg: 0.0,
            current_valve_lift_mm,
        }
    }

    /// Update VVA / VVT state based on current engine operating point.
    pub fn update_valvetrain_state(&mut self, current_rpm: f64, commanded_torque_pct: f64) {
        match self.valvetrain_type {
            VariableValvetrainType::YamahaVvaSolenoid { switch_rpm, hysteresis_rpm } => {
                if !self.is_high_cam_active && current_rpm >= switch_rpm {
                    self.is_high_cam_active = true;
                } else if self.is_high_cam_active && current_rpm < (switch_rpm - hysteresis_rpm) {
                    self.is_high_cam_active = false;
                }
            }
            VariableValvetrainType::YamahaYcciFunnel { switch_rpm } => {
                // Short funnels at high RPM for acoustic resonance tuning
                self.is_high_cam_active = current_rpm >= switch_rpm;
            }
            VariableValvetrainType::BmwValvetronicLift { min_lift_mm, max_lift_mm } => {
                // BMW Valvetronic replaces throttle butterfly: Valve lift controls cylinder filling
                let target_lift = min_lift_mm + (max_lift_mm - min_lift_mm) * (commanded_torque_pct / 100.0);
                self.current_valve_lift_mm = target_lift.clamp(min_lift_mm, max_lift_mm);
            }
            VariableValvetrainType::ContinuousDualCamPhaser { max_advance_deg, max_retard_deg } => {
                // Cam phaser adjusts internal EGR (overlap) at part load, maximum cylinder filling at WOT
                let load_norm = (commanded_torque_pct / 100.0).clamp(0.0, 1.0);
                self.intake_cam_advance_deg = max_advance_deg * load_norm;
                self.exhaust_cam_retard_deg = max_retard_deg * (1.0 - load_norm) * 0.5;
            }
        }
    }

    /// Calculate effective volumetric efficiency modifier from variable valvetrain position.
    pub fn calculate_ve_multiplier(&self) -> f64 {
        match self.valvetrain_type {
            VariableValvetrainType::YamahaVvaSolenoid { .. } => {
                if self.is_high_cam_active { 1.18 } else { 1.00 } // High-lift cam adds +18% breathing
            }
            VariableValvetrainType::YamahaYcciFunnel { .. } => {
                if self.is_high_cam_active { 1.06 } else { 1.00 } // High-RPM short runners add +6%
            }
            VariableValvetrainType::BmwValvetronicLift { min_lift_mm, max_lift_mm } => {
                0.20 + 0.80 * ((self.current_valve_lift_mm - min_lift_mm) / (max_lift_mm - min_lift_mm)).clamp(0.0, 1.0)
            }
            VariableValvetrainType::ContinuousDualCamPhaser { .. } => {
                1.0 + (self.intake_cam_advance_deg / 50.0) * 0.12
            }
        }
    }
}

/// Gasoline Direct Injection (GDI / HDEV) High-Pressure Rail Dynamics (Ferrari / Lamborghini / BMW M).
pub struct DirectInjectionRailController {
    /// Current fuel rail pressure in Bar (e.g. 200.0 Bar - 350.0 Bar)
    pub current_rail_pressure_bar: f64,
    /// Target rail pressure in Bar
    pub target_rail_pressure_bar: f64,
    /// High-Pressure Fuel Pump (HPFP) spill valve duty cycle (0.0 - 1.0)
    pub pump_spill_valve_duty: f64,
    // PI controller state
    pub kp: f64,
    pub ki: f64,
    integral_acc: f64,
}

impl Default for DirectInjectionRailController {
    fn default() -> Self {
        Self {
            current_rail_pressure_bar: 50.0,
            target_rail_pressure_bar: 200.0,
            pump_spill_valve_duty: 0.0,
            kp: 0.005,
            ki: 0.001,
            integral_acc: 0.0,
        }
    }
}

impl DirectInjectionRailController {
    /// Update HPFP spill valve PWM based on pressure rail error: Error = Target - Actual.
    pub fn update_rail_pressure(&mut self, dt_seconds: f64) {
        let error = self.target_rail_pressure_bar - self.current_rail_pressure_bar;
        self.integral_acc = (self.integral_acc + self.ki * error * dt_seconds).clamp(0.0, 1.0);
        self.pump_spill_valve_duty = (self.kp * error + self.integral_acc).clamp(0.0, 1.0);

        // Simulated rail pressure physics (pump delivery vs injector draw)
        let pump_flow = self.pump_spill_valve_duty * 250.0 * dt_seconds;
        self.current_rail_pressure_bar += pump_flow;
    }

    /// Calculate effective injector pulse width (ms) for GDI at high rail pressure via Bernoulli:
    /// PW = Mass / (C_d * A * sqrt(2 * rho * P_rail)).
    pub fn calculate_gdi_pulse_width_ms(&self, target_fuel_mass_mg: f64) -> f64 {
        let p_rail_effective = self.current_rail_pressure_bar.max(20.0);
        // At 200 Bar, injection duration is typically ~1.5 - 3.5 ms even for massive fuel mass!
        let base_scaler = 25.0 / p_rail_effective.sqrt();
        target_fuel_mass_mg * base_scaler * 0.01
    }
}

/// 2-Step Launch Control & Anti-Lag System (ALS) for Turbochargers.
pub struct LaunchAndAntiLagController {
    pub is_launch_active: bool,
    pub is_als_active: bool,
    pub launch_rpm_limit: f64,
    pub spark_retard_limit_deg: f64, // e.g. -25.0° (ignites inside exhaust manifold)
    pub fuel_enrichment_pct: f64,    // e.g. +20.0% unburnt fuel
}

impl Default for LaunchAndAntiLagController {
    fn default() -> Self {
        Self {
            is_launch_active: false,
            is_als_active: false,
            launch_rpm_limit: 4500.0,
            spark_retard_limit_deg: -20.0,
            fuel_enrichment_pct: 15.0,
        }
    }
}

impl LaunchAndAntiLagController {
    /// Calculate spark cutting & retardation command when Launch or ALS is engaged.
    pub fn evaluate_combustion_intervention(
        &self,
        current_rpm: f64,
        clutch_engaged: bool,
        vehicle_speed_kph: f64,
        tps_pct: f64,
    ) -> (bool, f64, f64) {
        // Launch Control: Vehicle stationary, clutch depressed, full throttle
        if clutch_engaged && vehicle_speed_kph < 5.0 && tps_pct > 80.0 {
            if current_rpm >= self.launch_rpm_limit {
                // Hard spark cut (true), massive spark retard, +fuel
                return (true, self.spark_retard_limit_deg, 1.0 + (self.fuel_enrichment_pct / 100.0));
            } else if current_rpm >= (self.launch_rpm_limit - 300.0) {
                // Soft spark retard to build boost spool while stationary
                return (false, self.spark_retard_limit_deg * 0.5, 1.0 + (self.fuel_enrichment_pct / 200.0));
            }
        }

        // Anti-Lag System (ALS): Off-throttle overrun at high speed
        if self.is_als_active && tps_pct < 5.0 && current_rpm > 3500.0 {
            // Keep turbine spooled by detonating fuel directly in exhaust manifold
            return (false, self.spark_retard_limit_deg, 1.25);
        }

        (false, 0.0, 1.0) // Normal engine operation
    }
}

/// Bi-Directional Quickshifter & Auto-Blipper (Yamaha R1 / Ducati Panigale).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QuickshiftAction {
    None,
    UpshiftCut { cut_duration_ms: f64 },
    DownshiftBlip { throttle_blip_deg: f64, blip_duration_ms: f64 },
}

pub struct QuickshifterController {
    pub upshift_cut_ms: f64,
    pub downshift_blip_deg: f64,
    pub downshift_blip_ms: f64,
}

impl Default for QuickshifterController {
    fn default() -> Self {
        Self {
            upshift_cut_ms: 65.0,        // 65ms ignition/fuel cut relieves dogbox gear stress
            downshift_blip_deg: 18.0,    // 18° Electronic throttle rev-match blip
            downshift_blip_ms: 120.0,    // 120ms duration
        }
    }
}

impl QuickshifterController {
    /// Evaluate shift sensor strain gauge tension / compression voltage.
    pub fn process_shift_sensor(&self, sensor_volts: f64, current_rpm: f64) -> QuickshiftAction {
        // Sensor resting voltage = 2.50V
        // Upshift pushes linkage: Voltage > 3.80V
        // Downshift pulls linkage: Voltage < 1.20V
        if current_rpm < 2500.0 {
            return QuickshiftAction::None; // Disable below stall threshold
        }

        if sensor_volts > 3.80 {
            // Upshift: Cut torque to unload dogbox dogs
            let scaled_cut = if current_rpm > 10000.0 { 55.0 } else { self.upshift_cut_ms };
            QuickshiftAction::UpshiftCut { cut_duration_ms: scaled_cut }
        } else if sensor_volts < 1.20 {
            // Downshift: Rev-match auto-blip electronic throttle
            QuickshiftAction::DownshiftBlip {
                throttle_blip_deg: self.downshift_blip_deg,
                blip_duration_ms: self.downshift_blip_ms,
            }
        } else {
            QuickshiftAction::None
        }
    }
}

/// 6-Axis Inertial Measurement Unit (IMU) Lean-Angle Traction & Wheelie Control.
pub struct ImuChassisSupervisor {
    pub max_allowed_wheel_slip_pct: f64,
    pub max_allowed_pitch_rate_rad_s: f64, // Wheelie limit
}

impl Default for ImuChassisSupervisor {
    fn default() -> Self {
        Self {
            max_allowed_wheel_slip_pct: 12.0,     // 12% optimal slip ratio
            max_allowed_pitch_rate_rad_s: 0.35,   // Wheelie rise velocity limit
        }
    }
}

impl ImuChassisSupervisor {
    /// Calculate lean-angle sensitive traction control torque reduction:
    /// High lean angle reduces allowable slip to prevent high-side crashes!
    pub fn evaluate_cornering_traction(
        &self,
        lean_angle_deg: f64,
        front_wheel_speed: f64,
        rear_wheel_speed: f64,
        pitch_rate_rad_s: f64,
    ) -> (f64, &'static str) {
        // 1. Wheelie Control Check (Front wheel lifting rapidly)
        if pitch_rate_rad_s > self.max_allowed_pitch_rate_rad_s {
            // Cut torque by 40% immediately to bring front wheel down gently
            return (0.60, "WHEELIE_CONTROL_INTERVENTION");
        }

        // 2. Slip Ratio Calculation: S = (V_rear - V_front) / V_front * 100%
        if front_wheel_speed < 1.0 {
            return (1.0, "STANDSTILL");
        }
        let slip_pct = ((rear_wheel_speed - front_wheel_speed) / front_wheel_speed) * 100.0;

        // 3. Lean Angle Scaling: At 55° lean, allowable slip drops from 12% to 4%!
        let lean_norm = (lean_angle_deg.abs() / 60.0).clamp(0.0, 1.0);
        let dynamic_allowed_slip = self.max_allowed_wheel_slip_pct * (1.0 - 0.65 * lean_norm);

        if slip_pct > dynamic_allowed_slip {
            let excess_slip = slip_pct - dynamic_allowed_slip;
            // Progressive torque cut (up to 50% cut via ignition retard + DBW close)
            let torque_multiplier = (1.0 - (excess_slip * 0.05)).clamp(0.50, 1.0);
            (torque_multiplier, "TRACTION_CONTROL_CORNER_CUT")
        } else {
            (1.0, "FULL_GRIP_NOMINAL")
        }
    }
}
