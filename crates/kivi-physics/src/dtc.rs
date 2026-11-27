//! Diagnostic trouble codes, fault debounce, and the DTC switch matrix.
//!
//! Ported from `ecu_engine_rust::flashing::dtc`.
//!
//! Pure byte-level logic: DTC parsing/formatting, a debounce integrator, and a
//! one-byte ROM patch. No I/O, no hardware. Verified by unit tests with
//! hand-derived expectations.

/// Standard OBD-II DTC Category Prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DtcCategory {
    Powertrain, // 'P'
    Chassis,    // 'C'
    Body,       // 'B'
    Network,    // 'U'
}

/// Structured Diagnostic Trouble Code Representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DtcCode {
    pub category: DtcCategory,
    pub is_standard: bool, // 0/2 = SAE standard, 1/3 = Manufacturer/reserved
    pub sub_category: u8,  // Exact 2-bit second digit (0..=3)
    pub code_hex: u16,     // e.g. 0x0420 for P0420 (Catalyst System Efficiency)
}

impl DtcCode {
    /// Format DTC to standard 5-character string (e.g. "P0420").
    pub fn to_string(&self) -> String {
        let prefix = match self.category {
            DtcCategory::Powertrain => 'P',
            DtcCategory::Chassis => 'C',
            DtcCategory::Body => 'B',
            DtcCategory::Network => 'U',
        };
        let digit2 = (b'0' + (self.sub_category & 3)) as char;
        let low_digits = format!("{:03X}", self.code_hex & 0x0FFF);
        format!("{}{}{}", prefix, digit2, &low_digits[low_digits.len() - 3..])
    }

    /// Parse DTC from standard 5-character string (e.g. "P0100").
    pub fn parse(text: &str) -> Result<Self, &'static str> {
        let trimmed = text.trim();
        if trimmed.len() != 5 {
            return Err("DTC code must be exactly 5 characters (e.g. 'P0420')");
        }
        let category = match trimmed.chars().next().unwrap().to_ascii_uppercase() {
            'P' => DtcCategory::Powertrain,
            'C' => DtcCategory::Chassis,
            'B' => DtcCategory::Body,
            'U' => DtcCategory::Network,
            _ => return Err("Invalid DTC prefix, must be P, C, B, or U"),
        };

        let digit2 = trimmed.chars().nth(1).unwrap();
        let (is_standard, sub_category) = match digit2 {
            '0' => (true, 0),
            '1' => (false, 1),
            '2' => (true, 2),
            '3' => (false, 3),
            _ => return Err("Invalid DTC second digit"),
        };

        let hex_val = u16::from_str_radix(&trimmed[2..], 16)
            .map_err(|_| "Invalid hex digits in DTC")?;

        Ok(Self {
            category,
            is_standard,
            sub_category,
            code_hex: hex_val,
        })
    }
}

/// DTC Configuration Switch State (Bosch / OEM Fault Configuration Byte).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DtcSwitchState {
    Disabled = 0x00,           // DTC Delete: ECU completely ignores fault, no freeze frame, no MIL
    ActiveNoMil = 0x01,        // Stored silently without lighting Check Engine Lamp
    ActiveMilTwoTrip = 0x02,   // Lights MIL after 2 consecutive drive cycles
    ActiveMilImmediate = 0x03, // Lights MIL immediately on first failure
}

/// Fault Debounce Integrator Model.
///
/// Prevents false positive fault triggers due to transient sensor noise.
pub struct FaultDebounceIntegrator {
    pub fault_counter: i32,
    pub trip_threshold: i32,  // Counter threshold to trigger fault (e.g. +40 cycles)
    pub clear_threshold: i32, // Counter threshold to clear fault (e.g. -40 cycles)
}

impl Default for FaultDebounceIntegrator {
    fn default() -> Self {
        Self {
            fault_counter: 0,
            trip_threshold: 40,
            clear_threshold: -40,
        }
    }
}

impl FaultDebounceIntegrator {
    /// Update debounce integrator with symptom status.
    /// Returns true if fault should be locked in freeze frame.
    pub fn update(&mut self, symptom_active: bool) -> bool {
        if symptom_active {
            self.fault_counter = (self.fault_counter + 1).min(self.trip_threshold + 5);
        } else {
            self.fault_counter = (self.fault_counter - 1).max(self.clear_threshold - 5);
        }
        self.fault_counter >= self.trip_threshold
    }

    pub fn reset(&mut self) {
        self.fault_counter = 0;
    }
}

/// DTC Switch Matrix Manager ("DTC Delete Engine").
pub struct DtcSwitchMatrix;

impl DtcSwitchMatrix {
    /// Apply a DTC Delete patch: sets the corresponding configuration byte to 0x00 (Disabled).
    pub fn patch_dtc_disable(
        rom: &mut [u8],
        switch_address: usize,
    ) -> Result<(), &'static str> {
        if switch_address >= rom.len() {
            return Err("DTC switch address exceeds ROM size");
        }
        rom[switch_address] = DtcSwitchState::Disabled as u8;
        Ok(())
    }
}
