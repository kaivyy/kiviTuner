//! Ported from `ecu_engine_rust::ui::gauges`.
//!
//! Pure logic, no I/O. Verified by unit tests where applicable; not validated
//! against physical hardware.

/// Analog Dial Gauge Model with Second-Order Damped Needle Physics.
pub struct AnalogGauge {
    pub min_val: f64,
    pub max_val: f64,
    pub redline_val: f64,
    /// Starting angle in radians (typically 135° = 3*pi/4)
    pub arc_min_rad: f64,
    /// Ending angle in radians (typically 405° = 9*pi/4)
    pub arc_max_rad: f64,
    pub center_x: f64,
    pub center_y: f64,
    pub radius: f64,

    // Physical needle inertia and damping properties
    pub current_angle_rad: f64,
    pub angular_velocity: f64,
    pub spring_stiffness: f64,   // 'k' tension pull towards target
    pub damping_coefficient: f64, // 'c' friction damping (prevents infinite oscillation)
    pub inertia: f64,            // 'J' rotational mass
}

impl Default for AnalogGauge {
    fn default() -> Self {
        Self {
            min_val: 0.0,
            max_val: 10000.0, // e.g. 10k RPM Tachometer
            redline_val: 8500.0,
            arc_min_rad: 135.0f64.to_radians(),
            arc_max_rad: 405.0f64.to_radians(),
            center_x: 200.0,
            center_y: 200.0,
            radius: 150.0,
            current_angle_rad: 135.0f64.to_radians(),
            angular_velocity: 0.0,
            spring_stiffness: 140.0,
            damping_coefficient: 18.0,
            inertia: 1.0,
        }
    }
}

impl AnalogGauge {
    /// Calculate target angle in radians for a given sensor input value.
    pub fn value_to_target_angle(&self, val: f64) -> f64 {
        let clamped = val.clamp(self.min_val, self.max_val);
        let ratio = (clamped - self.min_val) / (self.max_val - self.min_val);
        self.arc_min_rad + ratio * (self.arc_max_rad - self.arc_min_rad)
    }

    /// Step physical needle motion using numerical Verlet/Euler integration (dt in seconds).
    /// Simulates authentic mechanical gauge needle bounce and smooth acceleration.
    pub fn update_physics(&mut self, target_val: f64, dt_seconds: f64) {
        let target_angle = self.value_to_target_angle(target_val);
        let dt = dt_seconds.clamp(0.001, 0.05);

        // Spring-damper differential equation: J * theta'' + c * theta' + k * (theta - theta_target) = 0
        let displacement = self.current_angle_rad - target_angle;
        let spring_force = -self.spring_stiffness * displacement;
        let damping_force = -self.damping_coefficient * self.angular_velocity;
        let angular_accel = (spring_force + damping_force) / self.inertia;

        self.angular_velocity += angular_accel * dt;
        self.current_angle_rad += self.angular_velocity * dt;
    }

    /// Get needle tip coordinate (x, y) for 2D UI rendering canvas.
    pub fn needle_tip_xy(&self) -> (f64, f64) {
        let needle_len = self.radius * 0.88;
        let x = self.center_x + needle_len * self.current_angle_rad.cos();
        let y = self.center_y + needle_len * self.current_angle_rad.sin();
        (x, y)
    }

    /// Check if current reading is in warning redline zone.
    pub fn is_in_redline(&self, val: f64) -> bool {
        val >= self.redline_val
    }
}

/// Digital Readout with Exponential Moving Average (EMA) Jitter Reduction.
pub struct DigitalReadout {
    pub current_smoothed: f64,
    /// Smoothing weight (0.05 = heavily smoothed, 0.50 = responsive)
    pub alpha: f64,
    pub decimal_places: usize,
}

impl Default for DigitalReadout {
    fn default() -> Self {
        Self {
            current_smoothed: 0.0,
            alpha: 0.25,
            decimal_places: 1,
        }
    }
}

impl DigitalReadout {
    /// Ingest raw noisy sensor value and compute smoothed display output.
    pub fn update(&mut self, raw_value: f64) -> String {
        self.current_smoothed = (1.0 - self.alpha) * self.current_smoothed + self.alpha * raw_value;
        format!("{:.*}", self.decimal_places, self.current_smoothed)
    }
}

/// Status LED Warning Indicator (`MMSTATUSGAUGE`).
pub struct StatusLed {
    pub is_active: bool,
    pub active_color_rgba: (u8, u8, u8, u8),
    pub inactive_color_rgba: (u8, u8, u8, u8),
}

impl Default for StatusLed {
    fn default() -> Self {
        Self {
            is_active: false,
            active_color_rgba: (255, 30, 30, 255),  // Glowing bright red on alert
            inactive_color_rgba: (60, 10, 10, 180), // Dark matte bezel when off
        }
    }
}
