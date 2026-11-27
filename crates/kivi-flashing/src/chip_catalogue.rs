//! Programmable EEPROM/Flash device catalogue (`batronix_chips.csv`, 6 742 entries).
//!
//! Used offline to identify memory chips (29F400, 29BL802C, AM27128A, etc.)
//! by name or JEDEC manufacturer/device identifier pair.

use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// Raw CSV bytes (`Latin-1` encoded, semicolon-delimited, 5 columns, no header).
const BATRONIX_CSV: &[u8] = include_bytes!("../data/batronix_chips.csv");

/// One chip entry from the embedded programmer catalogue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChipCatalogEntry {
    pub manufacturer: String,
    pub chip_name: String,
    pub package: String,
    pub programmer_id: Option<u32>,
    /// Parsed `(register_or_offset, id_value)` pairs from the CSV JEDEC column.
    pub jedec_id_pairs: Vec<(u32, u32)>,
}

static CATALOGUE: OnceLock<Vec<ChipCatalogEntry>> = OnceLock::new();

fn parse_hex_u32(token: &str) -> Option<u32> {
    let t = token.trim();
    if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16).ok()
    } else {
        t.parse::<u32>().ok()
    }
}

fn parse_jedec_pairs(raw: &str) -> Vec<(u32, u32)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let mut pairs = Vec::new();
    for item in trimmed.split(',') {
        let Some((k, v)) = item.split_once(':') else {
            continue;
        };
        if let (Some(reg), Some(val)) = (parse_hex_u32(k), parse_hex_u32(v)) {
            pairs.push((reg, val));
        }
    }
    pairs
}

fn load_catalogue() -> &'static [ChipCatalogEntry] {
    CATALOGUE.get_or_init(|| {
        // CSV contains ISO-8859-1 bytes in some vendor/package names; lossy decode avoids panics.
        let text = String::from_utf8_lossy(BATRONIX_CSV);
        let mut entries = Vec::with_capacity(6800);
        for raw_line in text.lines() {
            let line = raw_line.trim_end_matches('\r');
            if line.trim().is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.split(';').collect();
            if parts.len() < 4 {
                continue;
            }
            let manufacturer = parts[0].trim().to_string();
            let chip_name = parts[1].trim().to_string();
            let package = parts[2].trim().to_string();
            let programmer_id = parts[3].trim().parse::<u32>().ok();
            let jedec_id_pairs = parts
                .get(4)
                .map(|s| parse_jedec_pairs(s))
                .unwrap_or_default();

            entries.push(ChipCatalogEntry {
                manufacturer,
                chip_name,
                package,
                programmer_id,
                jedec_id_pairs,
            });
        }
        entries
    })
}

/// Total number of parsed chip entries in the embedded catalogue.
pub fn chip_catalogue_count() -> usize {
    load_catalogue().len()
}

/// Searches entries whose manufacturer, chip name, or package matches `query`
/// (case-insensitive). An empty query returns nothing rather than dumping all rows.
pub fn find_chips(query: &str) -> Vec<&'static ChipCatalogEntry> {
    let q = query.trim().to_ascii_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    load_catalogue()
        .iter()
        .filter(|e| {
            e.manufacturer.to_ascii_lowercase().contains(&q)
                || e.chip_name.to_ascii_lowercase().contains(&q)
                || e.package.to_ascii_lowercase().contains(&q)
        })
        .collect()
}

/// Finds chips matching a JEDEC `(manufacturer_id, device_id)` identifier pair.
pub fn find_chips_by_jedec(manufacturer_id: u32, device_id: u32) -> Vec<&'static ChipCatalogEntry> {
    load_catalogue()
        .iter()
        .filter(|e| {
            if e.jedec_id_pairs.len() < 2 {
                return false;
            }
            let mfr_match = e
                .jedec_id_pairs
                .iter()
                .find(|&&(reg, _)| reg == 0)
                .or_else(|| e.jedec_id_pairs.first())
                .is_some_and(|&(_, val)| val == manufacturer_id);
            let dev_match = e
                .jedec_id_pairs
                .iter()
                .skip(1)
                .any(|&(_, val)| val == device_id);
            mfr_match && dev_match
        })
        .collect()
}
