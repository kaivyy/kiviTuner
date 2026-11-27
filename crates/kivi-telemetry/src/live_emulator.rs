//! Ported from `ecu_engine_rust::telemetry::live_emulator`.
//!
//! Pure parsing/analysis logic. No socket, no serial port, no device access.
//! Verified by unit tests with hand-derived expectations; **not** validated
//! against live hardware.

use std::collections::HashMap;

/// Hardware Emulation Memory Bank State.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveEmulationBank {
    BankOriginal, // Feeds factory ROM to pass ignition-on self-test
    BankModified, // Feeds tuned ROM once engine is running
}

/// Address Hit Entry with Timestamp Decay (for Live Afterglow & Automark).
#[derive(Debug, Clone, Copy)]
pub struct AddressHitEntry {
    pub last_hit_timestamp_ms: u64,
    pub hit_count: u32,
}

/// Dual-Port SRAM Emulator & Real-Time Live Tuning Engine.
pub struct LiveEmulator {
    /// Internal Dual-Port SRAM Buffer (e.g. 512 KB or 1 MB)
    pub memory_original: Vec<u8>,
    pub memory_modified: Vec<u8>,
    pub active_bank: ActiveEmulationBank,

    // Hardware Startup Bypass Options
    pub startup_switch_enabled: bool,
    pub startup_switch_delay_ms: u64,
    pub session_start_timestamp_ms: u64,

    // Address Hit-Tracing & Afterglow (Cell Tracing & Automark)
    pub address_hits: HashMap<u32, AddressHitEntry>,
    pub afterglow_duration_ms: u64, // e.g. 2000 ms (2 seconds glow)
    pub automark_enabled: bool,
    pub active_cell_selection: Option<u32>,
}

impl LiveEmulator {
    pub fn new(original_bin: Vec<u8>, modified_bin: Vec<u8>) -> Self {
        Self {
            memory_original: original_bin,
            memory_modified: modified_bin,
            active_bank: ActiveEmulationBank::BankOriginal,
            startup_switch_enabled: false,
            startup_switch_delay_ms: 3000, // 3 seconds on original ROM for startup checksum
            session_start_timestamp_ms: 0,
            address_hits: HashMap::new(),
            afterglow_duration_ms: 2000,
            automark_enabled: false,
            active_cell_selection: None,
        }
    }

    /// Read byte from the perspective of the running vehicle ECU (Port A - Vehicle Bus).
    /// Zero wait-state access (~15 ns).
    pub fn ecu_read_byte(&mut self, address: u32, current_time_ms: u64) -> Option<u8> {
        let addr = address as usize;

        // Auto-switch bank after startup delay if enabled (Checksum Bypass)
        if self.startup_switch_enabled && self.active_bank == ActiveEmulationBank::BankOriginal {
            if current_time_ms.saturating_sub(self.session_start_timestamp_ms) >= self.startup_switch_delay_ms {
                self.active_bank = ActiveEmulationBank::BankModified;
            }
        }

        // Record Address Hit for Real-Time UI Tracing
        let entry = self.address_hits.entry(address).or_insert(AddressHitEntry {
            last_hit_timestamp_ms: current_time_ms,
            hit_count: 0,
        });
        entry.last_hit_timestamp_ms = current_time_ms;
        entry.hit_count = entry.hit_count.saturating_add(1);

        if self.automark_enabled {
            self.active_cell_selection = Some(address);
        }

        // Return byte from active bank
        match self.active_bank {
            ActiveEmulationBank::BankOriginal => self.memory_original.get(addr).copied(),
            ActiveEmulationBank::BankModified => self.memory_modified.get(addr).copied(),
        }
    }

    /// Write modified byte from Calibration UI while the engine is running (Port B - Host PC).
    /// Simulates cycle-stealing dual-port RAM write arbitration.
    pub fn host_live_write_byte(&mut self, address: u32, new_value: u8) -> Result<(), &'static str> {
        let addr = address as usize;
        if addr >= self.memory_modified.len() {
            return Err("Address out of emulation buffer bounds");
        }
        // Write instantly into modified buffer
        self.memory_modified[addr] = new_value;
        Ok(())
    }

    /// Check if a memory address is currently glowing (Afterglow intensity from 0.0 to 1.0).
    /// Used by calibration UI to draw trail behind active table cells.
    pub fn get_afterglow_intensity(&self, address: u32, current_time_ms: u64) -> f64 {
        if let Some(entry) = self.address_hits.get(&address) {
            let elapsed = current_time_ms.saturating_sub(entry.last_hit_timestamp_ms);
            if elapsed < self.afterglow_duration_ms {
                1.0 - (elapsed as f64 / self.afterglow_duration_ms as f64)
            } else {
                0.0
            }
        } else {
            0.0
        }
    }

    /// Prune old expired hit traces to maintain memory efficiency.
    pub fn prune_expired_hits(&mut self, current_time_ms: u64) {
        let max_duration = self.afterglow_duration_ms;
        self.address_hits.retain(|_, entry| {
            current_time_ms.saturating_sub(entry.last_hit_timestamp_ms) < max_duration
        });
    }
}
