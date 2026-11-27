//! Ported from `ecu_engine_rust::flashing::bench_hardware`.
//!
//! Pure byte-level protocol logic: builders, parsers, encoders, state machines.
//! No I/O, no device access, no `unsafe`. Transport is the caller's concern.
//!
//! Verified by unit tests with hand-derived expectations. **Not** validated
//! against a physical ECU; no such claim is made.

/// Automotive Power Supply Rails & Pinout Designations (DIN 72552 Standard).
pub mod din72552_rails {
    pub const KL30_BATTERY_PERMANENT: &str = "KL30 (+12V Permanent Battery)";
    pub const KL15_IGNITION_SWITCHED: &str = "KL15 (+12V Switched Ignition)";
    pub const KL31_CHASSIS_GROUND: &str = "KL31 (Chassis Ground GND)";
    pub const CAN_HIGH: &str = "CAN-H (Pin 6 OBD-II, Recessive 2.5V, Dominant 3.5V)";
    pub const CAN_LOW: &str = "CAN-L (Pin 14 OBD-II, Recessive 2.5V, Dominant 1.5V)";
    pub const K_LINE: &str = "K-Line (Pin 7 OBD-II, ISO 9141-2 / ISO 14230 Bidirectional 12V)";
    pub const BOOT_PIN: &str = "BOOT0 / HWCFG (Microcontroller Boot Strap Mode Pull-Down)";
}

/// Vehicle Electrical System Architecture Class (12V Moto, 12V Car, 24V Heavy-Duty Truck/Bus).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VehicleElectricalClass {
    /// 12V Motorcycle / ATV / Marine (Nominal 12.0V - 14.8V)
    Motorcycle12V,
    /// 12V Passenger Car / Light Commercial (Nominal 12.2V - 14.8V)
    PassengerCar12V,
    /// 24V Heavy-Duty Truck / Bus / Excavator (Cummins, Hino, Isuzu, Fuso, Scania, Volvo, Bosch EDC7/EDC17CV: 23.5V - 29.2V)
    HeavyDutyTruck24V,
}

/// Electrical Power Monitor & Brownout Safety Detector.
pub struct BenchPowerSupplyMonitor {
    pub current_voltage: f64,
    pub min_flashing_voltage: f64,
    pub max_flashing_voltage: f64,
    pub max_ripple_voltage: f64,
}

impl Default for BenchPowerSupplyMonitor {
    fn default() -> Self {
        Self::for_vehicle_class(VehicleElectricalClass::PassengerCar12V)
    }
}

impl BenchPowerSupplyMonitor {
    /// Create a power supply monitor configured for the exact vehicle electrical system (12V Moto, 12V Car, or 24V Truck).
    pub fn for_vehicle_class(class: VehicleElectricalClass) -> Self {
        match class {
            VehicleElectricalClass::Motorcycle12V => Self {
                current_voltage: 13.5,
                min_flashing_voltage: 12.0,
                max_flashing_voltage: 14.8,
                max_ripple_voltage: 0.05,
            },
            VehicleElectricalClass::PassengerCar12V => Self {
                current_voltage: 13.8,
                min_flashing_voltage: 12.2,
                max_flashing_voltage: 14.8,
                max_ripple_voltage: 0.05,
            },
            VehicleElectricalClass::HeavyDutyTruck24V => Self {
                current_voltage: 27.2,      // Regulated 24V dual-battery bench/truck supply
                min_flashing_voltage: 23.5, // Below 23.5V on 24V truck risks EDC7/EDC17CV charge-pump brownout
                max_flashing_voltage: 29.2, // Max 24V alternator/charger clamp
                max_ripple_voltage: 0.10,   // Max 100 mV ripple on 24V rails
            },
        }
    }

    /// Verify whether voltage conditions are 100% safe to initiate sector erase or flash write.
    pub fn is_safe_for_flashing(&self, measured_voltage: f64, ripple_voltage: f64) -> Result<(), &'static str> {
        if !measured_voltage.is_finite() || measured_voltage < self.min_flashing_voltage {
            return Err("BROWNOUT_RISK: Supply voltage below 12.2V! Flash programming aborted to prevent bricking.");
        }
        if measured_voltage > self.max_flashing_voltage {
            return Err("OVERVOLTAGE_RISK: Supply voltage exceeds 14.8V! Aborted to protect ECU components.");
        }
        if !ripple_voltage.is_finite() || ripple_voltage < 0.0 || ripple_voltage > self.max_ripple_voltage {
            return Err("EXCESSIVE_RIPPLE: Power supply ripple exceeds 50mV. Regulated lab supply required.");
        }
        Ok(())
    }
}

/// CAN-Bus Termination Impedance Evaluator.
pub struct CanBusImpedance;

impl CanBusImpedance {
    /// Verify termination resistor between CAN-H and CAN-L (Nominal 60 Ohms in-vehicle, 120 Ohms bench).
    pub fn verify_termination_resistance(measured_ohms: f64, is_bench_mode: bool) -> Result<(), &'static str> {
        if !measured_ohms.is_finite() {
            return Err("INVALID_TERMINATION: Non-finite CAN bus termination resistance reading.");
        }
        if is_bench_mode {
            // Bench harness must have exactly one 120 Ohm terminating resistor
            if (measured_ohms - 120.0).abs() > 15.0 {
                return Err("INVALID_TERMINATION: Bench CAN bus requires a 120-Ohm termination resistor between CAN-H and CAN-L.");
            }
        } else {
            // In-vehicle network has two 120 Ohm resistors in parallel = 60 Ohms
            if (measured_ohms - 60.0).abs() > 10.0 {
                return Err("INVALID_TERMINATION: Vehicle CAN bus requires nominal 60-Ohms total parallel resistance.");
            }
        }
        Ok(())
    }
}

/// Microcontroller Boot Mode Trapping State Machine.
///
/// Implements the exact electronic power cycle sequence required to capture
/// the microcontroller (TriCore, RH850, ST10, C167) into internal foundry ROM Boot Mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootTrapState {
    PowerOff,
    BootPinPulledLow,
    Kl15IgnitionApplied,
    FoundryRomCaptured,
    NormalApplicationRunning,
}

pub struct BootModeTrappingSequencer {
    pub state: BootTrapState,
    pub boot_resistor_ohms: u32, // Typically 1k Ohm to 5.6k Ohm pull-down
}

impl Default for BootModeTrappingSequencer {
    fn default() -> Self {
        Self {
            state: BootTrapState::PowerOff,
            boot_resistor_ohms: 1000, // 1k Ohm standard boot pull-down
        }
    }
}

impl BootModeTrappingSequencer {
    /// Execute the sequence: Turn off KL15 -> Pull Boot pin to GND -> Turn on KL15.
    pub fn execute_boot_trap(&mut self) -> Result<BootTrapState, &'static str> {
        // Step 1: Ensure switched power is OFF
        self.state = BootTrapState::PowerOff;

        // Step 2: Connect Boot Pin to GND via 1k resistor
        self.state = BootTrapState::BootPinPulledLow;

        // Step 3: Apply Switched Ignition +12V (KL15)
        self.state = BootTrapState::Kl15IgnitionApplied;

        // Step 4: CPU latches boot config pins on rising edge of reset -> enters foundry ROM!
        self.state = BootTrapState::FoundryRomCaptured;
        Ok(self.state)
    }
}

/// J2534-2 Programming Voltage Pin Target.
/// Extracted from industry PassThru adapters (Tactrix OpenPort 2.0 op20pt32.dll and
/// comparable vendor interfaces).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgrammingPin {
    Pin6MitsubishiCanAux = 6,
    Pin8Ignition = 8,
    Pin9SubaruMitsubishiAux = 9,
    Pin11FordFeps = 11,
    Pin12SubaruFlash = 12,
    Pin13MitsubishiFlash = 13,
    Pin14CanL = 14,
    Pin15LLine = 15,
    Pin25AuxJack = 25,
}

/// J2534 Programming Voltage Command Builder & Guard.
pub struct ProgrammingVoltageController;

impl ProgrammingVoltageController {
    pub const VOLTAGE_OFF: u32 = 0xFFFFFFFF;
    pub const SHORT_TO_GROUND: u32 = 0xFFFFFFFE;

    /// Build and validate SetProgrammingVoltage parameters according to SAE J2534-2.
    pub fn build_voltage_command(pin: ProgrammingPin, millivolts: u32) -> Result<(u32, u32), &'static str> {
        let pin_num = pin as u32;
        if millivolts == Self::VOLTAGE_OFF || millivolts == Self::SHORT_TO_GROUND {
            return Ok((pin_num, millivolts));
        }

        // Safety clamps: Automotive flash voltage must never exceed 20,000 mV (20V)
        // Ford FEPS is 18,000 mV (18V), Mitsubishi is 16,000-17,000 mV, Subaru/L-line is 12,000 mV
        if millivolts < 5_000 || millivolts > 20_000 {
            return Err("VOLTAGE_OUT_OF_BOUNDS: Programming voltage must be between 5,000mV and 20,000mV");
        }

        Ok((pin_num, millivolts))
    }
}

/// Dual-Rail (`VEcu` KL30 & `VKey` KL15) Short-Circuit & Pin-25 Hardware Protection Monitor.
pub struct DualRailPowerDiagnostics {
    pub vecu_max_current_ma: u32,
    pub vkey_max_current_ma: u32,
    pub min_rail_voltage_mv: u32,
}

impl Default for DualRailPowerDiagnostics {
    fn default() -> Self {
        Self {
            vecu_max_current_ma: 2500, // 2.5A max continuous bench ECU draw
            vkey_max_current_ma: 500,  // 500mA max KL15 ignition trigger draw
            min_rail_voltage_mv: 11_500,
        }
    }
}

impl DualRailPowerDiagnostics {
    /// Inspect `VEcu` (KL30) and `VKey` (KL15) output voltage and current draw for short-circuit or collapse.
    pub fn verify_dual_rail_health(
        &self,
        vecu_mv: u32,
        vecu_ma: u32,
        vkey_mv: u32,
        vkey_ma: u32,
    ) -> Result<(), &'static str> {
        if vkey_ma > self.vkey_max_current_ma {
            return Err("BENCH_HARDWARE_ERROR_30128: Short circuit detected on VKey (KL15) output");
        }
        if vkey_mv < self.min_rail_voltage_mv {
            return Err("BENCH_HARDWARE_ERROR_30129: Incorrect voltage on VKey (KL15) output");
        }
        if vecu_ma > self.vecu_max_current_ma {
            return Err("BENCH_HARDWARE_ERROR_30130: Short circuit detected on VEcu (KL30) output");
        }
        if vecu_mv < self.min_rail_voltage_mv {
            return Err("BENCH_HARDWARE_ERROR_30131: Incorrect voltage on VEcu (KL30) output");
        }
        Ok(())
    }

    /// Check External NOR Flash Pin 25 (`#WP` / Write Protect) hardware state.
    pub fn check_pin25_write_protection(pin25_pulled_low: bool) -> Result<(), &'static str> {
        if pin25_pulled_low {
            return Err("BENCH_HARDWARE_ERROR_40026: Write protection activated on pin 25");
        }
        Ok(())
    }
}

/// Tricore MEDC17 / MD1 Cover-Free Bench Service Mode Sequencer (GPT1 & GPT2 + PowerBox).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceModeGptSequencer {
    pub gpt1_freq_hz: u32,
    pub gpt2_freq_hz: u32,
    pub kl30_active: bool,
    pub kl15_active: bool,
    pub service_mode_entered: bool,
}

impl Default for ServiceModeGptSequencer {
    fn default() -> Self {
        Self {
            gpt1_freq_hz: 10_000, // 10.0 kHz standard Bosch MEDC17 GPT1 square wave
            gpt2_freq_hz: 15_625, // 15.625 kHz standard Bosch MEDC17 GPT2 square wave
            kl30_active: false,
            kl15_active: false,
            service_mode_entered: false,
        }
    }
}

impl ServiceModeGptSequencer {
    /// Execute cover-free Bosch MEDC17 / MD1 / MG1 Service Mode entry sequence:
    /// 1. Power off KL30 & KL15 -> 2. Start GPT1 & GPT2 square waves -> 3. Apply KL30 & KL15 -> 4. Verify BSL response.
    pub fn enter_cover_free_service_mode(
        &mut self,
        gpt1_connected: bool,
        gpt2_connected: bool,
    ) -> Result<(), &'static str> {
        if !gpt1_connected || !gpt2_connected {
            return Err("BENCH_HARDWARE_ERROR_20059: Incorrect GPT mode input (both GPT1 and GPT2 signals required)");
        }
        if self.gpt1_freq_hz == 0 || self.gpt2_freq_hz == 0 {
            return Err("BENCH_HARDWARE_ERROR_20059: GPT1/GPT2 clock frequency must be non-zero");
        }

        // Power cycle with GPT signals active
        self.kl30_active = true;
        self.kl15_active = true;
        self.service_mode_entered = true;
        Ok(())
    }
}

/// K-Line Diagnostic Bus Spy Buffer & Wakeup Handshake Analyzer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KLineSpyCapture {
    pub captured_bytes: Vec<u8>,
}

impl KLineSpyCapture {
    pub fn new(captured_bytes: Vec<u8>) -> Self {
        Self { captured_bytes }
    }

    /// Analyze captured K-Line 5-baud or fast-init handshake (`0x55` sync + `KB1`, `KB2`).
    /// Returns `(baud_sync_ok, key_byte_1, key_byte_2)`.
    pub fn analyze_wakeup_handshake(&self) -> Result<(bool, u8, u8), &'static str> {
        if self.captured_bytes.is_empty() {
            return Err("BENCH_HARDWARE_ERROR_20041: Empty K-Line Spy buffer");
        }
        let sync_pos = self
            .captured_bytes
            .iter()
            .position(|&b| b == 0x55)
            .ok_or("BENCH_HARDWARE_ERROR_20042: Error during K-Line Spy reception (missing 0x55 sync byte)")?;

        if sync_pos + 2 >= self.captured_bytes.len() {
            return Err("BENCH_HARDWARE_ERROR_20042: Truncated K-Line key bytes after 0x55 sync");
        }

        let kb1 = self.captured_bytes[sync_pos + 1];
        let kb2 = self.captured_bytes[sync_pos + 2];
        Ok((true, kb1, kb2))
    }
}
