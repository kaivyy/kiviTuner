//! Ported from `ecu_engine_rust::flashing::adaptations`.
//!
//! Pure byte-level protocol logic: builders, parsers, encoders, state machines.
//! No I/O, no device access, no `unsafe`. Transport is the caller's concern.
//!
//! Verified by unit tests with hand-derived expectations. **Not** validated
//! against a physical ECU; no such claim is made.

/// Drive-By-Wire (DBW) Electronic Throttle Body Calibration Status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThrottleAlignmentStatus {
    NotPerformed,
    SweepingMinimumStop,
    SweepingMaximumStop,
    PlausibilityCheckOk,
    AlignmentFailed(&'static str),
}

/// Dual-Channel Throttle Position Sensor (TPS1 and TPS2) Plausibility Checker.
pub struct ThrottleBodyAdaptation {
    pub status: ThrottleAlignmentStatus,
    pub min_stop_voltage_tps1: f64,
    pub max_stop_voltage_tps1: f64,
    pub min_stop_voltage_tps2: f64,
    pub max_stop_voltage_tps2: f64,
}

impl Default for ThrottleBodyAdaptation {
    fn default() -> Self {
        Self {
            status: ThrottleAlignmentStatus::NotPerformed,
            min_stop_voltage_tps1: 0.50, // Typical mechanical lower stop (idle)
            max_stop_voltage_tps1: 4.50, // Typical mechanical upper stop (WOT)
            min_stop_voltage_tps2: 4.50, // Inverse potentiometer 2
            max_stop_voltage_tps2: 0.50,
        }
    }
}

impl ThrottleBodyAdaptation {
    /// Plausibility Check: In automotive Drive-by-Wire systems, TPS1 and TPS2 are inverted.
    /// Their voltage sum must always equal the 5.0V sensor supply rail (V_tps1 + V_tps2 = 5.0V +/- 0.2V).
    pub fn verify_dual_tps_plausibility(v_tps1: f64, v_tps2: f64) -> Result<(), &'static str> {
        let sum = v_tps1 + v_tps2;
        if !v_tps1.is_finite() || !v_tps2.is_finite() || !sum.is_finite() || (sum - 5.0).abs() > 0.25 {
            return Err("TPS_PLAUSIBILITY_FAULT: Sum of dual TPS voltages deviates from 5.0V rail! Limp-mode active.");
        }
        Ok(())
    }

    /// Simulate the Key-ON throttle body mechanical sweep adaptation routine.
    pub fn perform_sweep_alignment(&mut self) -> ThrottleAlignmentStatus {
        // Step 1: Drive servo to spring-loaded lower mechanical stop
        self.status = ThrottleAlignmentStatus::SweepingMinimumStop;

        // Step 2: Drive servo to upper mechanical stop
        self.status = ThrottleAlignmentStatus::SweepingMaximumStop;

        // Step 3: Verify sensor plausibility at resting position (0.5V and 4.5V)
        if Self::verify_dual_tps_plausibility(self.min_stop_voltage_tps1, self.min_stop_voltage_tps2).is_ok() {
            self.status = ThrottleAlignmentStatus::PlausibilityCheckOk;
        } else {
            self.status = ThrottleAlignmentStatus::AlignmentFailed("Voltage sum fault during sweep");
        }

        self.status
    }
}

/// Fuel Trim Adaptation Model (STFT & LTFT Reset).
pub struct FuelTrimAdaptation {
    pub short_term_fuel_trim_pct: f64, // STFT (-25% to +25%)
    pub long_term_fuel_trim_pct: f64,  // LTFT (-25% to +25%)
}

impl Default for FuelTrimAdaptation {
    fn default() -> Self {
        Self {
            short_term_fuel_trim_pct: 0.0,
            long_term_fuel_trim_pct: 0.0,
        }
    }
}

impl FuelTrimAdaptation {
    /// Reset learned fuel trims to baseline (0.0%) immediately after flashing a new calibration.
    pub fn reset_adaptations(&mut self) {
        self.short_term_fuel_trim_pct = 0.0;
        self.long_term_fuel_trim_pct = 0.0;
    }

    /// Calculate combined fueling correction multiplier.
    pub fn total_fuel_multiplier(&self) -> f64 {
        1.0 + (self.short_term_fuel_trim_pct + self.long_term_fuel_trim_pct) / 100.0
    }
}

/// Transmission Control Unit (TCU / Mechatronic) Flashing Safety Interlocks.
/// Implements Dual-Clutch & Automatic Transmission Mechatronic Flashing Interlocks (DQ250, DQ381, DQ500, ZF 8HP, TC1782/TC275/TC277).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransmissionSelectorPosition {
    Park,
    Reverse,
    Neutral,
    Drive,
}

pub struct TransmissionMechatronicAdaptation;

impl TransmissionMechatronicAdaptation {
    pub const MIN_SAFE_OIL_TEMP_C: f64 = 30.0;
    pub const MAX_SAFE_OIL_TEMP_C: f64 = 60.0;
    pub const MAX_SAFE_DEPRESSURIZED_BAR: f64 = 5.0;

    /// Verify pre-flashing mechanical & hydraulic safety gates.
    /// Prevents sudden vehicle roll, clutch jerk, or mechatronic valve burnout during reflash.
    pub fn verify_pre_flash_interlocks(
        selector: TransmissionSelectorPosition,
        fluid_temp_celsius: f64,
        line_pressure_bar: f64,
    ) -> Result<(), &'static str> {
        // 1. Selector lever must be strictly in Park or Neutral
        if selector != TransmissionSelectorPosition::Park && selector != TransmissionSelectorPosition::Neutral {
            return Err("TCU_INTERLOCK_FAULT: Transmission selector must be in Park or Neutral before flashing!");
        }

        // 2. Oil temperature must be within thermal adaptation envelope (30°C - 60°C)
        if !fluid_temp_celsius.is_finite() || fluid_temp_celsius < Self::MIN_SAFE_OIL_TEMP_C {
            return Err("TCU_OIL_COLD: Transmission fluid below 30°C or unreadable. Hydraulic viscosity unsafe for adaptation.");
        }
        if fluid_temp_celsius > Self::MAX_SAFE_OIL_TEMP_C {
            return Err("TCU_OIL_HOT: Transmission fluid exceeds 60°C. Reflash aborted to prevent valve body drift.");
        }

        // 3. Hydraulic accumulator pressure must be depressurized (< 5 Bar)
        if !line_pressure_bar.is_finite()
            || line_pressure_bar < 0.0
            || line_pressure_bar > Self::MAX_SAFE_DEPRESSURIZED_BAR
        {
            return Err("TCU_PRESSURE_HIGH: Clutch accumulator pressurized or unreadable! Depressurize system before sector erase.");
        }

        Ok(())
    }

    /// Build UDS RoutineControl (Service 0x31) command to trigger post-flash Clutch Touch-Point Basic Setting.
    pub fn build_clutch_calibration_routine(routine_id: u16) -> Vec<u8> {
        vec![0x31, 0x01, (routine_id >> 8) as u8, (routine_id & 0xFF) as u8]
    }
}
