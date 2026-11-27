//! Ported from `ecu_engine_rust::physics::multi_cylinder`.
//!
//! Pure physics maths: no I/O, no hardware access, deterministic.
//! Constants and formulas are reproduced from the source; deviations are noted
//! inline. Verified by unit tests with hand-derived expectations, **not** by
//! comparison against a physical ECU.

/// Comprehensive Engine Cylinder Configuration & Geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineCylinderLayout {
    SingleCylinder,
    ParallelTwin360,     // Even firing: 360°-360° (BMW F800, vintage British twins)
    ParallelTwin180,     // Uneven firing: 180°-540° (Ninja 250/300, CBR250RR MC51)
    ParallelTwin270,     // Crossplane twin: 270°-450° (Yamaha MT-07, R7, Africa Twin)
    VTwin45Uneven,       // Harley-Davidson: 315°-405° firing spacing
    VTwin90Ducati,       // Ducati L-Twin / Suzuki V-Strom: 270°-450° perfect primary balance
    BoxerFlat4,          // Subaru EJ/FA/FB, Porsche 718: 1-3-2-4 180° firing
    Inline4Flatplane,    // Standard I4: 1-3-4-2 180° firing
    Inline4Crossplane,   // Yamaha YZF-R1: 1-3-2-4 uneven 270°-180°-90°-180° (Zero inertial torque)
    Inline6Even,         // BMW N54/B58, Toyota 2JZ: 1-5-3-6-2-4 120° firing (Perfect balance)
    V6Even60,            // Standard 60° V6: 120° even firing (Nissan VQ35, Honda J35)
    V6Odd90,             // 90° V6 without split-pin crank: 90°-150° uneven firing
    V8Crossplane90,      // American V8 / GM LS / Ford Modular: 1-8-7-2-6-5-4-3 (Burble sound)
    V8Flatplane180,      // Ferrari 458/F8, Corvette Z06 C8: 1-5-3-7-4-8-2-6 (Screaming sound)
    V10Even72,           // Lamborghini Gallardo/Huracan, Audi R8 V10: 72° firing
    V12Even60,           // Ferrari V12, Aston Martin, BMW M70: 60° firing
}

impl EngineCylinderLayout {
    /// Return the total number of combustion cylinders.
    pub fn cylinder_count(&self) -> usize {
        match self {
            EngineCylinderLayout::SingleCylinder => 1,
            EngineCylinderLayout::ParallelTwin360
            | EngineCylinderLayout::ParallelTwin180
            | EngineCylinderLayout::ParallelTwin270
            | EngineCylinderLayout::VTwin45Uneven
            | EngineCylinderLayout::VTwin90Ducati => 2,
            EngineCylinderLayout::BoxerFlat4
            | EngineCylinderLayout::Inline4Flatplane
            | EngineCylinderLayout::Inline4Crossplane => 4,
            EngineCylinderLayout::Inline6Even
            | EngineCylinderLayout::V6Even60
            | EngineCylinderLayout::V6Odd90 => 6,
            EngineCylinderLayout::V8Crossplane90
            | EngineCylinderLayout::V8Flatplane180 => 8,
            EngineCylinderLayout::V10Even72 => 10,
            EngineCylinderLayout::V12Even60 => 12,
        }
    }

    /// Return total exhaust cylinder banks (1 for inline/singles, 2 for V, Boxer, and twin-pipe setups).
    pub fn bank_count(&self) -> usize {
        match self {
            EngineCylinderLayout::SingleCylinder | EngineCylinderLayout::ParallelTwin360 => 1,
            _ => 2, // Most modern twins, Boxers, V6, V8 have dual separate exhaust banks
        }
    }

    /// Return which exhaust bank (0 for Bank 1, 1 for Bank 2) a given cylinder (0-indexed) belongs to.
    pub fn cylinder_to_bank(&self, cylinder_idx: usize) -> usize {
        match self {
            EngineCylinderLayout::SingleCylinder | EngineCylinderLayout::ParallelTwin360 => 0,
            EngineCylinderLayout::ParallelTwin180
            | EngineCylinderLayout::ParallelTwin270
            | EngineCylinderLayout::VTwin45Uneven
            | EngineCylinderLayout::VTwin90Ducati => {
                // Cyl 0 is Front/Left (Bank 1), Cyl 1 is Rear/Right (Bank 2)
                cylinder_idx % 2
            }
            EngineCylinderLayout::BoxerFlat4 => {
                // Subaru: Cyl 1, 3 on Bank 1 (Right), Cyl 2, 4 on Bank 2 (Left)
                if cylinder_idx == 0 || cylinder_idx == 2 { 0 } else { 1 }
            }
            EngineCylinderLayout::Inline4Flatplane | EngineCylinderLayout::Inline4Crossplane => {
                // Often paired as Bank 1 (Cyl 1 & 4), Bank 2 (Cyl 2 & 3) in dual-cat headers
                if cylinder_idx == 0 || cylinder_idx == 3 { 0 } else { 1 }
            }
            EngineCylinderLayout::Inline6Even
            | EngineCylinderLayout::V6Even60
            | EngineCylinderLayout::V6Odd90 => {
                // Inline 6 & V6: Cyl 1, 2, 3 on Bank 1, Cyl 4, 5, 6 on Bank 2
                if cylinder_idx < 3 { 0 } else { 1 }
            }
            EngineCylinderLayout::V8Crossplane90 | EngineCylinderLayout::V8Flatplane180 => {
                // Bank 1: Cylinders 1, 3, 5, 7 | Bank 2: Cylinders 2, 4, 6, 8 (or 1-4 / 5-8)
                cylinder_idx % 2
            }
            EngineCylinderLayout::V10Even72 => {
                if cylinder_idx < 5 { 0 } else { 1 }
            }
            EngineCylinderLayout::V12Even60 => {
                if cylinder_idx < 6 { 0 } else { 1 }
            }
        }
    }

    /// Firing Top Dead Center (TDC) crank angle in degrees [0.0° .. 720.0°] for each cylinder.
    pub fn firing_angles_degrees(&self) -> Vec<f64> {
        match self {
            EngineCylinderLayout::SingleCylinder => vec![0.0],
            EngineCylinderLayout::ParallelTwin360 => vec![0.0, 360.0],
            EngineCylinderLayout::ParallelTwin180 => vec![0.0, 180.0], // CBR250RR / Ninja 250
            EngineCylinderLayout::ParallelTwin270 => vec![0.0, 270.0], // Yamaha MT-07
            EngineCylinderLayout::VTwin45Uneven => vec![0.0, 315.0],   // Harley 45°
            EngineCylinderLayout::VTwin90Ducati => vec![0.0, 270.0],   // Ducati 90°
            EngineCylinderLayout::BoxerFlat4 => vec![0.0, 180.0, 360.0, 540.0], // 1-3-2-4
            EngineCylinderLayout::Inline4Flatplane => vec![0.0, 180.0, 360.0, 540.0], // 1-3-4-2
            EngineCylinderLayout::Inline4Crossplane => vec![0.0, 270.0, 450.0, 540.0], // R1 crossplane
            EngineCylinderLayout::Inline6Even => vec![0.0, 120.0, 240.0, 360.0, 480.0, 600.0], // 1-5-3-6-2-4
            EngineCylinderLayout::V6Even60 => vec![0.0, 120.0, 240.0, 360.0, 480.0, 600.0],
            EngineCylinderLayout::V6Odd90 => vec![0.0, 90.0, 240.0, 330.0, 480.0, 570.0],
            EngineCylinderLayout::V8Crossplane90 => {
                vec![0.0, 90.0, 180.0, 270.0, 360.0, 450.0, 540.0, 630.0]
            }
            EngineCylinderLayout::V8Flatplane180 => {
                vec![0.0, 90.0, 180.0, 270.0, 360.0, 450.0, 540.0, 630.0]
            }
            EngineCylinderLayout::V10Even72 => {
                (0..10).map(|i| i as f64 * 72.0).collect()
            }
            EngineCylinderLayout::V12Even60 => {
                (0..12).map(|i| i as f64 * 60.0).collect()
            }
        }
    }
}

/// Individual Cylinder Fuel & Spark Trimming Coordinator (Bosch FKKVS / KFZW0..KFZW7).
pub struct IndividualCylinderTrimmer {
    pub layout: EngineCylinderLayout,
    /// Percentage fuel trim per cylinder (-20.0% to +20.0%)
    pub fuel_trim_percent: Vec<f64>,
    /// Base ignition spark trim per cylinder in degrees (-10.0° to +10.0°)
    pub spark_trim_degrees: Vec<f64>,
    /// Dynamic knock retard per cylinder in degrees (0.0° to -15.0°)
    pub knock_retard_degrees: Vec<f64>,
}

impl IndividualCylinderTrimmer {
    pub fn new(layout: EngineCylinderLayout) -> Self {
        let n = layout.cylinder_count();
        Self {
            layout,
            fuel_trim_percent: vec![0.0; n],
            spark_trim_degrees: vec![0.0; n],
            knock_retard_degrees: vec![0.0; n],
        }
    }

    /// Calculate final commanded fuel pulse width (ms) for cylinder `cyl_idx`.
    pub fn calculate_cylinder_fuel_pw(&self, cyl_idx: usize, base_pw_ms: f64) -> Result<f64, &'static str> {
        if cyl_idx >= self.fuel_trim_percent.len() {
            return Err("Cylinder index out of range");
        }
        let trim_factor = 1.0 + (self.fuel_trim_percent[cyl_idx] / 100.0);
        Ok(base_pw_ms * trim_factor)
    }

    /// Calculate final commanded ignition advance (degrees BTDC) for cylinder `cyl_idx`.
    pub fn calculate_cylinder_spark_advance(
        &self,
        cyl_idx: usize,
        base_spark_deg: f64,
    ) -> Result<f64, &'static str> {
        if cyl_idx >= self.spark_trim_degrees.len() {
            return Err("Cylinder index out of range");
        }
        let trim = self.spark_trim_degrees[cyl_idx];
        let retard = self.knock_retard_degrees[cyl_idx]; // Negative or zero
        Ok(base_spark_deg + trim + retard)
    }

    /// Apply individual cylinder knock retard when detonation is detected in cylinder `cyl_idx`'s knock window.
    pub fn apply_individual_knock_retard(&mut self, cyl_idx: usize, step_retard_deg: f64) {
        if cyl_idx < self.knock_retard_degrees.len() {
            // Retard is subtracted (clamped to max -12.0°)
            self.knock_retard_degrees[cyl_idx] = (self.knock_retard_degrees[cyl_idx] - step_retard_deg).max(-12.0);
        }
    }

    /// Decay individual knock retards back towards 0.0° over time (IIR recovery).
    pub fn decay_knock_retards(&mut self, decay_factor: f64) {
        for retard in self.knock_retard_degrees.iter_mut() {
            if *retard < 0.0 {
                *retard *= decay_factor; // e.g. 0.95
                if retard.abs() < 0.05 {
                    *retard = 0.0;
                }
            }
        }
    }
}

/// Dual-Bank Independent Closed-Loop Lambda Coordinator (V6 / V8 / Dual Exhaust).
pub struct DualBankLambdaController {
    /// Short-Term Fuel Trim for Bank 1 (percentage, e.g. +3.5%)
    pub stft_bank1_pct: f64,
    /// Short-Term Fuel Trim for Bank 2
    pub stft_bank2_pct: f64,
    /// Long-Term Fuel Trim for Bank 1
    pub ltft_bank1_pct: f64,
    /// Long-Term Fuel Trim for Bank 2
    pub ltft_bank2_pct: f64,

    // PI controller gains
    pub kp: f64,
    pub ki: f64,
    integral_acc_bank1: f64,
    integral_acc_bank2: f64,
}

impl Default for DualBankLambdaController {
    fn default() -> Self {
        Self {
            stft_bank1_pct: 0.0,
            stft_bank2_pct: 0.0,
            ltft_bank1_pct: 0.0,
            ltft_bank2_pct: 0.0,
            kp: 12.0,
            ki: 4.0,
            integral_acc_bank1: 0.0,
            integral_acc_bank2: 0.0,
        }
    }
}

impl DualBankLambdaController {
    /// Update dual-bank closed-loop feedback using measured Lambda from Bank 1 and Bank 2 O2 sensors.
    pub fn update(
        &mut self,
        target_lambda: f64,
        actual_lambda_bank1: f64,
        actual_lambda_bank2: f64,
        dt_seconds: f64,
    ) {
        let dt = dt_seconds.clamp(0.005, 0.2);

        // Bank 1 PI Error (Error = Actual - Target; positive error means Lean -> need more fuel)
        let error_bank1 = actual_lambda_bank1 - target_lambda;
        self.integral_acc_bank1 = (self.integral_acc_bank1 + self.ki * error_bank1 * dt).clamp(-15.0, 15.0);
        self.stft_bank1_pct = (self.kp * error_bank1 + self.integral_acc_bank1).clamp(-25.0, 25.0);

        // Bank 2 PI Error
        let error_bank2 = actual_lambda_bank2 - target_lambda;
        self.integral_acc_bank2 = (self.integral_acc_bank2 + self.ki * error_bank2 * dt).clamp(-15.0, 15.0);
        self.stft_bank2_pct = (self.kp * error_bank2 + self.integral_acc_bank2).clamp(-25.0, 25.0);
    }

    /// Get total fuel scaling multiplier for a specific cylinder bank (0 = Bank 1, 1 = Bank 2).
    pub fn bank_fuel_multiplier(&self, bank_idx: usize) -> f64 {
        if bank_idx == 0 {
            1.0 + (self.stft_bank1_pct + self.ltft_bank1_pct) / 100.0
        } else {
            1.0 + (self.stft_bank2_pct + self.ltft_bank2_pct) / 100.0
        }
    }
}

/// Cylinder-Specific Knock Detection Angular Window.
pub struct KnockDetectionWindow {
    pub window_start_crank_deg: f64, // typically TDC + 10° (expansion stroke)
    pub window_end_crank_deg: f64,   // typically TDC + 60°
}

impl Default for KnockDetectionWindow {
    fn default() -> Self {
        Self {
            window_start_crank_deg: 10.0,
            window_end_crank_deg: 55.0,
        }
    }
}

impl KnockDetectionWindow {
    /// Check whether the current global engine crank angle (0° - 720°) falls within cylinder `cyl_idx`'s knock window.
    pub fn is_crank_in_cylinder_window(
        &self,
        current_crank_deg: f64,
        cyl_tdc_angle_deg: f64,
    ) -> bool {
        let norm_crank = current_crank_deg.rem_euclid(720.0);
        let win_start = (cyl_tdc_angle_deg + self.window_start_crank_deg).rem_euclid(720.0);
        let win_end = (cyl_tdc_angle_deg + self.window_end_crank_deg).rem_euclid(720.0);

        if win_start <= win_end {
            norm_crank >= win_start && norm_crank <= win_end
        } else {
            // Window wraps around 720° / 0°
            norm_crank >= win_start || norm_crank <= win_end
        }
    }
}
