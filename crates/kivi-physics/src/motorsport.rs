//! Ported from `ecu_engine_rust::physics::motorsport`.
//!
//! Pure physics maths: no I/O, no hardware access, deterministic.
//! Constants and formulas are reproduced from the source; deviations are noted
//! inline. Verified by unit tests with hand-derived expectations, **not** by
//! comparison against a physical ECU.

/// Current Transmission Gear State.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransmissionGear {
    Neutral,
    Gear1,
    Gear2,
    Gear3,
    Gear4,
    Gear5,
    Gear6,
}

impl TransmissionGear {
    pub fn as_index(&self) -> usize {
        match self {
            TransmissionGear::Neutral => 0,
            TransmissionGear::Gear1 => 1,
            TransmissionGear::Gear2 => 2,
            TransmissionGear::Gear3 => 3,
            TransmissionGear::Gear4 => 4,
            TransmissionGear::Gear5 => 5,
            TransmissionGear::Gear6 => 6,
        }
    }
}

/// Transmission Gear Estimator from Speed / RPM Ratio (VSS / Engine RPM).
pub struct GearRatioEstimator {
    /// Nominal Gear Ratios (RPM per km/h) for Gear 1..6
    pub gear_ratios: [f64; 6],
    pub tolerance_pct: f64,
}

impl Default for GearRatioEstimator {
    fn default() -> Self {
        // Example sports bike / performance car ratios (RPM / km/h)
        // Gear 1: 125 RPM/(km/h), Gear 2: 90, Gear 3: 70, Gear 4: 55, Gear 5: 45, Gear 6: 38
        Self {
            gear_ratios: [125.0, 90.0, 70.0, 55.0, 45.0, 38.0],
            tolerance_pct: 12.0, // 12% tolerance for clutch slip / tire growth
        }
    }
}

impl GearRatioEstimator {
    /// Detect currently engaged gear based on vehicle speed (km/h) and engine RPM.
    pub fn detect_gear(&self, vehicle_speed_kph: f64, engine_rpm: f64) -> TransmissionGear {
        if vehicle_speed_kph < 3.0 || engine_rpm < 500.0 {
            return TransmissionGear::Neutral;
        }

        let ratio = engine_rpm / vehicle_speed_kph;

        for (idx, &nominal) in self.gear_ratios.iter().enumerate() {
            let error_pct = ((ratio - nominal) / nominal).abs() * 100.0;
            if error_pct <= self.tolerance_pct {
                return match idx {
                    0 => TransmissionGear::Gear1,
                    1 => TransmissionGear::Gear2,
                    2 => TransmissionGear::Gear3,
                    3 => TransmissionGear::Gear4,
                    4 => TransmissionGear::Gear5,
                    5 => TransmissionGear::Gear6,
                    _ => TransmissionGear::Neutral,
                };
            }
        }

        TransmissionGear::Neutral
    }
}

/// Gear-Dependent Boost & Torque Limiter (Boost-By-Gear / Traction Management).
pub struct BoostByGearController {
    /// Target maximum boost pressure in Bar per gear [Neutral, G1, G2, G3, G4, G5, G6]
    pub max_boost_targets_bar: [f64; 7],
    /// Throttle By Wire (TBW) maximum opening percentage per gear
    pub max_tbw_open_pct: [f64; 7],
}

impl Default for BoostByGearController {
    fn default() -> Self {
        Self {
            // Gear 1 & 2 limited to prevent wheelspin / wheelie; Gears 4-6 full boost!
            max_boost_targets_bar: [0.30, 0.65, 0.95, 1.35, 1.60, 1.60, 1.60],
            max_tbw_open_pct: [35.0, 60.0, 75.0, 100.0, 100.0, 100.0, 100.0],
        }
    }
}

impl BoostByGearController {
    /// Get allowed maximum boost target and TBW throttle clamp for active gear.
    pub fn get_gear_limits(&self, gear: TransmissionGear) -> (f64, f64) {
        let idx = gear.as_index();
        (self.max_boost_targets_bar[idx], self.max_tbw_open_pct[idx])
    }
}

/// Flat-Foot Shifting / No-Lift-Shift (NLS) & Rolling Anti-Lag (Roll Launch).
pub struct FlatShiftAndRollingAntilagController {
    pub nls_hold_rpm_offset: f64,
    pub nls_spark_cut: bool,
    pub nls_spark_retard_deg: f64,
    pub roll_launch_active: bool,
    pub roll_launch_target_speed_kph: f64,
}

impl Default for FlatShiftAndRollingAntilagController {
    fn default() -> Self {
        Self {
            nls_hold_rpm_offset: 200.0,    // Target RPM = RPM_shift - 200
            nls_spark_cut: false,
            nls_spark_retard_deg: -15.0,   // Spark retard maintains boost during upshift
            roll_launch_active: false,
            roll_launch_target_speed_kph: 60.0, // e.g. 60 km/h roll race
        }
    }
}

impl FlatShiftAndRollingAntilagController {
    /// Evaluate Flat-Foot Shifting: Driver keeps throttle at 100% and dips clutch pedal to change gears.
    pub fn evaluate_flat_shift(
        &self,
        clutch_switch_pressed: bool,
        tps_pct: f64,
        current_rpm: f64,
        shift_rpm_trigger: f64,
    ) -> (bool, f64) {
        if clutch_switch_pressed && tps_pct > 85.0 && current_rpm >= (shift_rpm_trigger - 500.0) {
            // NLS Engaged: Cut ignition and retard spark to hold RPM and spool turbo!
            (true, self.nls_spark_retard_deg)
        } else {
            (false, 0.0)
        }
    }

    /// Evaluate Rolling Anti-Lag (Cruising on highway, driver holds steering wheel button to build boost before launch).
    pub fn evaluate_rolling_antilag(
        &self,
        button_pressed: bool,
        tps_pct: f64,
        vehicle_speed_kph: f64,
    ) -> (bool, f64, f64) {
        if button_pressed && tps_pct > 75.0 && (vehicle_speed_kph - self.roll_launch_target_speed_kph).abs() < 10.0 {
            // Rolling launch: Retard spark to -25° and inject extra fuel (+20%) to build full boost against vehicle drag
            (true, -25.0, 1.20)
        } else {
            (false, 0.0, 1.0)
        }
    }
}

/// Water-Methanol Injection (WMI) Secondary Flow Scaler.
pub struct WaterMethInjectionController {
    pub min_boost_enable_bar: f64,
    pub max_flow_cc_min: f64,
    pub current_pump_duty_pct: f64,
    pub octane_equivalent_boost_adder_deg: f64, // e.g. +3.5° spark advance when WMI flows
}

impl Default for WaterMethInjectionController {
    fn default() -> Self {
        Self {
            min_boost_enable_bar: 0.80,    // Spray activates above 0.8 Bar boost
            max_flow_cc_min: 500.0,        // 500 cc/min 50/50 Water-Methanol
            current_pump_duty_pct: 0.0,
            octane_equivalent_boost_adder_deg: 3.5,
        }
    }
}

impl WaterMethInjectionController {
    /// Calculate WMI pump PWM duty cycle based on manifold boost pressure.
    pub fn update_wmi_flow(&mut self, current_boost_bar: f64, max_rated_boost_bar: f64) -> (f64, f64) {
        if current_boost_bar < self.min_boost_enable_bar {
            self.current_pump_duty_pct = 0.0;
            return (0.0, 0.0);
        }

        let boost_range = (max_rated_boost_bar - self.min_boost_enable_bar).max(0.1);
        let progress = ((current_boost_bar - self.min_boost_enable_bar) / boost_range).clamp(0.0, 1.0);

        self.current_pump_duty_pct = 20.0 + (80.0 * progress); // 20% minimum prime duty
        let flow_cc = self.max_flow_cc_min * (self.current_pump_duty_pct / 100.0);
        let spark_adder = self.octane_equivalent_boost_adder_deg * progress;

        (flow_cc, spark_adder)
    }
}

/// Pit-Lane Speed Limiter (Motorsport Regulation Clamping).
pub struct PitLaneSpeedLimiter {
    pub pit_limit_kph: f64, // Typically 60.0 km/h (MotoGP) or 80.0 km/h (F1 / GT3)
    pub is_enabled: bool,
    pub progressive_cut_span_kph: f64,
}

impl Default for PitLaneSpeedLimiter {
    fn default() -> Self {
        Self {
            pit_limit_kph: 60.0,
            is_enabled: false,
            progressive_cut_span_kph: 3.0,
        }
    }
}

impl PitLaneSpeedLimiter {
    /// Calculate fuel/spark cut percentage to lock vehicle speed strictly to pit limit.
    pub fn calculate_speed_limit_cut(&self, vehicle_speed_kph: f64) -> f64 {
        if !self.is_enabled {
            return 0.0; // No limit
        }

        if vehicle_speed_kph >= self.pit_limit_kph {
            1.0 // 100% cut (hard spark cut)
        } else if vehicle_speed_kph >= (self.pit_limit_kph - self.progressive_cut_span_kph) {
            let progress = (vehicle_speed_kph - (self.pit_limit_kph - self.progressive_cut_span_kph))
                / self.progressive_cut_span_kph;
            progress.clamp(0.0, 0.85) // Progressive soft spark cut
        } else {
            0.0
        }
    }
}

/// Exhaust Burble / Popcorn Decel Mode Controller.
/// Implements deceleration crackle and negative ignition retard calibration curves.
/// Delays deceleration fuel cut (DFCO) and applies negative spark retard during throttle liftoff.
pub struct PopcornBurbleController {
    pub min_rpm: f64,
    pub max_rpm: f64,
    pub max_tps_threshold: f64,
    pub spark_retard_atdc: f64, // Typically -10.0° to -25.0° (ATDC)
    pub dfco_delay_ms: u32,     // Window duration (e.g. 1500 ms)
    pub max_safe_egt_celsius: f64, // Thermal protection clamp (e.g. 900°C)
    pub is_enabled: bool,
}

impl Default for PopcornBurbleController {
    fn default() -> Self {
        Self {
            min_rpm: 2200.0,
            max_rpm: 6500.0,
            max_tps_threshold: 2.0, // Throttle fully released (< 2%)
            spark_retard_atdc: -15.0, // -15 degrees ATDC ignition timing
            dfco_delay_ms: 1500,     // 1.5 second burble window
            max_safe_egt_celsius: 900.0, // Abort if exhaust exceeds 900°C
            is_enabled: true,
        }
    }
}

impl PopcornBurbleController {
    /// Evaluate whether burble/popcorn mode is active on throttle lift-off.
    /// Returns Some((spark_angle_deg, keep_fuel_on)) if active, or None if normal engine overrun.
    pub fn evaluate_burble(
        &self,
        tps_pct: f64,
        rpm: f64,
        current_egt_celsius: f64,
        coast_elapsed_ms: u32,
    ) -> Option<(f64, bool)> {
        if !self.is_enabled {
            return None;
        }

        // Throttle must be closed (foot off pedal)
        if tps_pct > self.max_tps_threshold {
            return None;
        }

        // RPM must be within active burble window
        if rpm < self.min_rpm || rpm > self.max_rpm {
            return None;
        }

        // EGT Thermal component protection
        if current_egt_celsius >= self.max_safe_egt_celsius {
            return None; // Disengage to protect turbo exhaust turbine and catalytic converter
        }

        // Active only during the configured delay window
        if coast_elapsed_ms <= self.dfco_delay_ms {
            Some((self.spark_retard_atdc, true)) // Force fuel on + deep negative spark!
        } else {
            None // Resume normal fuel cut off (DFCO)
        }
    }
}
