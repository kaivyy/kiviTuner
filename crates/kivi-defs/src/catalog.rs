//! Ported from `ecu_engine_rust::catalog`.
//!
//! Pure logic, no I/O. Verified by unit tests where applicable; not validated
//! against physical hardware.

/// Single ECU Entry from `ecu_catalog.txt`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EcuRecord {
    pub usage: String,        // e.g. "Engine", "Gearbox", "Suspension"
    pub manufacturer: String, // e.g. "Bosch", "Moric", "Siemens/Continental", "Delphi"
    pub group: String,        // e.g. "EDC16", "BMS", "Simos"
    pub model_name: String,   // e.g. "EDC16C39", "BMS-K"
}

/// Single Vehicle Entry from `vehicle_catalog.txt`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VehicleRecord {
    pub category: String,     // e.g. "Motorbike", "Passenger car", "Truck", "Boat"
    pub make: String,         // e.g. "Yamaha", "BMW", "Porsche", "Toyota"
    pub model: String,        // e.g. "NMAX", "Aerox", "M3", "Golf"
    pub submodel: String,
    pub engine_displacement: String, // e.g. "155", "3.0L", "2.0 TSI"
}

/// Single Silicon Chip Entry from `semiconductor_chips.csv`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiliconChipRecord {
    pub manufacturer: String, // e.g. "AMD", "Intel", "ST", "Atmel"
    pub part_number: String,  // e.g. "AM27C256", "27C512", "29F400BT"
    pub programming_voltage: String, // e.g. "12.5V", "5V", "13V"
    pub package_type: String, // e.g. "DIP 28", "PLCC 32", "PSOP 44"
    pub device_id: String,
    pub signature_hex: String,
}

/// In-Memory Automotive Knowledge Catalog.
pub struct AutomotiveKnowledgeCatalog {
    pub ecus: Vec<EcuRecord>,
    pub vehicles: Vec<VehicleRecord>,
    pub chips: Vec<SiliconChipRecord>,
}

impl Default for AutomotiveKnowledgeCatalog {
    fn default() -> Self {
        Self {
            ecus: Vec::new(),
            vehicles: Vec::new(),
            chips: Vec::new(),
        }
    }
}

impl AutomotiveKnowledgeCatalog {
    /// Parse raw text of `ecu_catalog.txt`.
    pub fn load_ecus_from_str(&mut self, text: &str) {
        self.ecus.clear();
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with("//") {
                continue;
            }
            let parts: Vec<&str> = trimmed.split(';').map(|s| s.trim()).collect();
            if parts.len() >= 4 {
                self.ecus.push(EcuRecord {
                    usage: parts[0].to_string(),
                    manufacturer: parts[1].to_string(),
                    group: parts[2].to_string(),
                    model_name: parts[3].to_string(),
                });
            } else if parts.len() == 2 {
                self.ecus.push(EcuRecord {
                    usage: parts[0].to_string(),
                    manufacturer: parts[1].to_string(),
                    group: String::new(),
                    model_name: parts[1].to_string(),
                });
            }
        }
    }

    /// Parse raw text of `vehicle_catalog.txt`.
    pub fn load_vehicles_from_str(&mut self, text: &str) {
        self.vehicles.clear();
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with("//") {
                continue;
            }
            let parts: Vec<&str> = trimmed.split(';').map(|s| s.trim()).collect();
            if parts.len() >= 3 {
                self.vehicles.push(VehicleRecord {
                    category: parts[0].to_string(),
                    make: parts[1].to_string(),
                    model: parts[2].to_string(),
                    submodel: parts.get(3).copied().unwrap_or("").to_string(),
                    engine_displacement: parts.get(4).copied().unwrap_or("").to_string(),
                });
            }
        }
    }

    /// Parse raw text of `semiconductor_chips.csv`.
    pub fn load_chips_from_str(&mut self, text: &str) {
        self.chips.clear();
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with("//") {
                continue;
            }
            let parts: Vec<&str> = trimmed.split(';').map(|s| s.trim()).collect();
            if parts.len() >= 4 {
                self.chips.push(SiliconChipRecord {
                    manufacturer: parts[0].to_string(),
                    part_number: parts[1].to_string(),
                    package_type: parts[2].to_string(),
                    programming_voltage: parts[3].to_string(),
                    device_id: parts.get(4).copied().unwrap_or("").to_string(),
                    signature_hex: parts.get(5).copied().unwrap_or("").to_string(),
                });
            }
        }
    }

    /// Find all ECUs by manufacturer name (case-insensitive substring).
    pub fn find_ecus_by_manufacturer(&self, mfg_query: &str) -> Vec<&EcuRecord> {
        let q = mfg_query.to_lowercase();
        self.ecus
            .iter()
            .filter(|e| e.manufacturer.to_lowercase().contains(&q))
            .collect()
    }

    /// Find vehicles by make and model (e.g. "Yamaha", "155").
    pub fn find_vehicles(&self, make_query: &str, model_query: &str) -> Vec<&VehicleRecord> {
        let make_q = make_query.to_lowercase();
        let model_q = model_query.to_lowercase();
        self.vehicles
            .iter()
            .filter(|v| {
                v.make.to_lowercase().contains(&make_q)
                    && (v.model.to_lowercase().contains(&model_q)
                        || v.engine_displacement.to_lowercase().contains(&model_q))
            })
            .collect()
    }

    /// Search semiconductor chip catalog by part number (e.g. "27C256" or "29F400").
    pub fn find_chips(&self, part_query: &str) -> Vec<&SiliconChipRecord> {
        let q = part_query.to_lowercase();
        self.chips
            .iter()
            .filter(|c| c.part_number.to_lowercase().contains(&q))
            .collect()
    }
}
