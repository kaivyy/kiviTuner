//! Ported from `ecu_engine_rust::flashing::vid_block`.
//!
//! Pure byte-level protocol logic: builders, parsers, encoders, state machines.
//! No I/O, no device access, no `unsafe`. Transport is the caller's concern.
//!
//! Verified by unit tests with hand-derived expectations. **Not** validated
//! against a physical ECU; no such claim is made.

/// Vehicle Identity Data container.
#[derive(Debug, Clone, PartialEq)]
pub struct VehicleIdentityData {
    pub vin: String,
    pub axle_ratio: f64,
    pub tire_revs_per_mile: u16,
    pub immobilizer_keys: [u8; 8],
    pub options_mask: u32,
    pub raw_payload: Vec<u8>,
}

/// Vehicle Identity Data (VID) Block Scanner and Grafting Engine.
pub struct VehicleIdentityBlockManager;

impl VehicleIdentityBlockManager {
    /// 3-byte signature preceding standard VID blocks: `0x56 0x49 0x44` ("VID").
    pub const VID_MAGIC: [u8; 3] = [0x56, 0x49, 0x44];

    /// Check whether a 17-byte slice complies with ISO 3779 VIN specifications
    /// (Alphanumeric excluding I, O, Q).
    pub fn is_valid_iso3779_vin(slice: &[u8]) -> bool {
        if slice.len() != 17 {
            return false;
        }
        for &b in slice {
            let is_digit = b.is_ascii_digit();
            let is_valid_alpha = b.is_ascii_uppercase() && b != b'I' && b != b'O' && b != b'Q';
            if !is_digit && !is_valid_alpha {
                return false;
            }
        }
        true
    }

    /// Search raw ROM buffer for a valid 17-character ISO 3779 VIN string.
    pub fn locate_vin(rom: &[u8]) -> Option<(usize, String)> {
        if rom.len() < 17 {
            return None;
        }
        for i in 0..=rom.len() - 17 {
            let candidate = &rom[i..i + 17];
            if Self::is_valid_iso3779_vin(candidate) {
                if let Ok(vin_str) = std::str::from_utf8(candidate) {
                    return Some((i, vin_str.to_string()));
                }
            }
        }
        None
    }

    /// Locate VID block by scanning for the "VID" magic signature.
    pub fn locate_vid_block(rom: &[u8]) -> Option<usize> {
        if rom.len() < 64 {
            return None;
        }
        for i in 0..=rom.len() - 64 {
            if rom[i..i + 3] == Self::VID_MAGIC {
                return Some(i);
            }
        }
        None
    }

    /// Extract structured VID parameters from the identified offset.
    pub fn extract_vid_data(rom: &[u8], offset: usize) -> Result<VehicleIdentityData, &'static str> {
        let Some(end) = offset.checked_add(64) else {
            return Err("VID block offset overflows");
        };
        if end > rom.len() {
            return Err("VID block offset out of bounds");
        }

        let slice = &rom[offset..end];
        let vin_bytes = &slice[4..21];
        let vin = std::str::from_utf8(vin_bytes)
            .map_err(|_| "Invalid UTF-8 in VIN field")?
            .to_string();

        let axle_ratio_raw = u16::from_be_bytes([slice[22], slice[23]]) as f64;
        let axle_ratio = axle_ratio_raw / 100.0;

        let tire_revs_per_mile = u16::from_be_bytes([slice[24], slice[25]]);

        let mut immo_keys = [0u8; 8];
        immo_keys.copy_from_slice(&slice[26..34]);

        let options_mask = u32::from_be_bytes([slice[34], slice[35], slice[36], slice[37]]);

        Ok(VehicleIdentityData {
            vin,
            axle_ratio,
            tire_revs_per_mile,
            immobilizer_keys: immo_keys,
            options_mask,
            raw_payload: slice.to_vec(),
        })
    }

    /// Automatically transfer and graft vehicle identity from Stock ROM to Tuned ROM.
    pub fn auto_transfer_vid(stock_rom: &[u8], tuned_rom: &mut [u8]) -> Result<usize, &'static str> {
        // 1. Locate VID block in stock ROM
        let stock_offset = Self::locate_vid_block(stock_rom)
            .or_else(|| Self::locate_vin(stock_rom).map(|(off, _)| off.saturating_sub(4)))
            .ok_or("Could not locate vehicle identity block in stock ROM")?;

        // 2. Locate destination VID block in tuned ROM
        let tuned_offset = Self::locate_vid_block(tuned_rom)
            .or_else(|| Self::locate_vin(tuned_rom).map(|(off, _)| off.saturating_sub(4)))
            .ok_or("Could not locate vehicle identity destination in tuned ROM")?;

        // 3. Extract stock identity
        let vid = Self::extract_vid_data(stock_rom, stock_offset)?;

        // 4. Graft 64-byte payload into tuned ROM
        let Some(tuned_end) = tuned_offset.checked_add(64) else {
            return Err("Destination VID offset overflows");
        };
        if tuned_end > tuned_rom.len() {
            return Err("Destination ROM capacity insufficient for VID graft");
        }
        tuned_rom[tuned_offset..tuned_end].copy_from_slice(&vid.raw_payload);

        Ok(tuned_offset)
    }
}
