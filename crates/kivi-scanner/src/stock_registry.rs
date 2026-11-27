//! Local stock ROM hash verification & signature-anchored map registry.
//!
//! Enables tuning engineers to verify unmodified factory integrity via SHA-256
//! and match open-source signature packs to elevate blind map discovery.

use crate::ecu_fingerprint::{identify_ecu_rom, EcuFingerprint};
use crate::pattern_search::MaskedSignatureScanner;
use crate::rom_identity::{extract_rom_tokens, normalise};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MapSignatureAnchor {
    pub name: String,
    pub category: String,
    pub signature: String,
    #[serde(default)]
    pub relative_offset: i64,
    pub rows: usize,
    pub cols: usize,
    pub cell_bits: u8,
    pub little_endian: bool,
    pub unit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StockRomEntry {
    pub id: String,
    pub label: String,
    pub manufacturer: String,
    pub ecu_family: String,
    pub sw_version: String,
    pub size_bytes: usize,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub identity_tokens: Vec<String>,
    #[serde(default)]
    pub map_signatures: Vec<MapSignatureAnchor>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StockRomRegistryPack {
    pub schema_version: u32,
    pub pack_name: String,
    pub entries: Vec<StockRomEntry>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum IntegrityStatus {
    ExactStockMatch,
    FamilyMatchModifiedOrUnknown,
    Unrecognized,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnchoredMapDiscovery {
    pub name: String,
    pub category: String,
    pub address: usize,
    pub rows: usize,
    pub cols: usize,
    pub cell_bits: u8,
    pub little_endian: bool,
    pub unit: String,
    pub match_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StockRomMatchReport {
    pub sha256: String,
    pub rom_size: usize,
    pub pack_name: String,
    pub integrity_status: IntegrityStatus,
    pub matched_entry: Option<StockRomEntry>,
    pub identity_tokens: Vec<String>,
    pub fingerprint: EcuFingerprint,
    pub anchored_maps: Vec<AnchoredMapDiscovery>,
}

/// Returns the built-in open-source stock ECU reference pack (family tokens & known signatures).
pub fn builtin_open_stock_registry() -> StockRomRegistryPack {
    StockRomRegistryPack {
        schema_version: 1,
        pack_name: "kiviTuner Open Stock & Signature Reference Pack".into(),
        entries: vec![
            StockRomEntry {
                id: "honda_keihin_k64_cbr250rr".into(),
                label: "Honda CBR250RR Keihin SH7058".into(),
                manufacturer: "Honda".into(),
                ecu_family: "Keihin Moto SH7058".into(),
                sw_version: "38770-K64".into(),
                size_bytes: 262_144,
                sha256: None,
                identity_tokens: vec!["K64J108".into(), "38770K64".into()],
                map_signatures: vec![],
            },
            StockRomEntry {
                id: "bmw_ms43_430069".into(),
                label: "BMW Siemens MS43 512KB".into(),
                manufacturer: "BMW".into(),
                ecu_family: "Siemens MS43".into(),
                sw_version: "430069".into(),
                size_bytes: 524_288,
                sha256: None,
                identity_tokens: vec!["430069".into(), "MS430069".into()],
                map_signatures: vec![],
            },
            StockRomEntry {
                id: "subaru_denso_sh7058".into(),
                label: "Subaru Denso SH7058 1MB".into(),
                manufacturer: "Subaru".into(),
                ecu_family: "Denso SuperH SH7058".into(),
                sw_version: "A2WC522N".into(),
                size_bytes: 1_048_576,
                sha256: None,
                identity_tokens: vec!["A2WC522N".into()],
                map_signatures: vec![],
            },
            StockRomEntry {
                id: "gm_delco_p01_12212156".into(),
                label: "GM Delco P01 LS1 OS 12212156".into(),
                manufacturer: "GM".into(),
                ecu_family: "Delco P01 512KB".into(),
                sw_version: "12212156".into(),
                size_bytes: 524_288,
                sha256: None,
                identity_tokens: vec!["12212156".into()],
                map_signatures: vec![],
            },
        ],
    }
}

pub fn compute_rom_sha256_hex(rom: &[u8]) -> String {
    let digest = Sha256::digest(rom);
    let mut out = String::with_capacity(64);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

pub fn parse_stock_registry_pack(json: &str) -> Result<StockRomRegistryPack, String> {
    let pack: StockRomRegistryPack =
        serde_json::from_str(json).map_err(|e| format!("Invalid stock registry JSON: {e}"))?;
    if pack.schema_version != 1 {
        return Err(format!(
            "Unsupported stock registry schema_version {} (expected 1)",
            pack.schema_version
        ));
    }
    Ok(pack)
}

fn discover_anchored_maps(rom: &[u8], anchors: &[MapSignatureAnchor]) -> Vec<AnchoredMapDiscovery> {
    let mut discovered = Vec::new();
    for anchor in anchors {
        let Ok(pat) = MaskedSignatureScanner::parse_pattern(&anchor.signature) else {
            continue;
        };
        let hits = MaskedSignatureScanner::find_all(rom, &pat);
        let Some(&first) = hits.first() else {
            continue;
        };
        let target_i = first as i128 + anchor.relative_offset as i128;
        if target_i < 0 || target_i >= rom.len() as i128 {
            continue;
        }
        discovered.push(AnchoredMapDiscovery {
            name: anchor.name.clone(),
            category: anchor.category.clone(),
            address: target_i as usize,
            rows: anchor.rows,
            cols: anchor.cols,
            cell_bits: anchor.cell_bits,
            little_endian: anchor.little_endian,
            unit: anchor.unit.clone(),
            match_count: hits.len(),
        });
    }
    discovered
}

/// Checks `rom` SHA-256, extracted tokens, and ECU fingerprint against either a
/// caller-supplied JSON registry pack or [`builtin_open_stock_registry`].
pub fn match_stock_rom_registry(
    rom: &[u8],
    custom_pack_json: Option<&str>,
) -> Result<StockRomMatchReport, String> {
    if rom.is_empty() {
        return Err("ROM buffer cannot be empty".to_string());
    }
    let pack = match custom_pack_json.map(str::trim).filter(|s| !s.is_empty()) {
        Some(raw) => parse_stock_registry_pack(raw)?,
        None => builtin_open_stock_registry(),
    };

    let sha256 = compute_rom_sha256_hex(rom);
    let tokens: Vec<String> = extract_rom_tokens(rom)
        .into_iter()
        .map(|t| t.token)
        .take(24)
        .collect();
    let fingerprint = identify_ecu_rom(rom);

    // 1. Exact SHA-256 stock verification
    if let Some(exact) = pack.entries.iter().find(|entry| {
        entry
            .sha256
            .as_ref()
            .is_some_and(|expected| expected.trim().eq_ignore_ascii_case(&sha256))
    }) {
        let anchored_maps = discover_anchored_maps(rom, &exact.map_signatures);
        return Ok(StockRomMatchReport {
            sha256,
            rom_size: rom.len(),
            pack_name: pack.pack_name,
            integrity_status: IntegrityStatus::ExactStockMatch,
            matched_entry: Some(exact.clone()),
            identity_tokens: tokens,
            fingerprint,
            anchored_maps,
        });
    }

    // 2. Match via ROM identity tokens or software strings + size compatibility
    let norm_fp_strings: Vec<String> = fingerprint
        .cal_ids
        .iter()
        .chain(fingerprint.sw_numbers.iter())
        .chain(fingerprint.part_numbers.iter())
        .map(|s| normalise(s))
        .collect();

    let token_match = pack.entries.iter().find(|entry| {
        let size_ok = entry.size_bytes == 0 || entry.size_bytes == rom.len();
        if !size_ok {
            return false;
        }
        entry.identity_tokens.iter().any(|needle| {
            let norm_needle = normalise(needle);
            !norm_needle.is_empty()
                && (tokens.iter().any(|t| t.contains(&norm_needle))
                    || norm_fp_strings.iter().any(|t| t.contains(&norm_needle)))
        })
    });

    if let Some(entry) = token_match {
        let anchored_maps = discover_anchored_maps(rom, &entry.map_signatures);
        return Ok(StockRomMatchReport {
            sha256,
            rom_size: rom.len(),
            pack_name: pack.pack_name,
            integrity_status: IntegrityStatus::FamilyMatchModifiedOrUnknown,
            matched_entry: Some(entry.clone()),
            identity_tokens: tokens,
            fingerprint,
            anchored_maps,
        });
    }

    // 3. Even if no catalog entry matched, run map anchors from size-compatible pack entries
    let mut anchored_maps = Vec::new();
    for entry in &pack.entries {
        if entry.size_bytes == 0 || entry.size_bytes == rom.len() {
            anchored_maps.extend(discover_anchored_maps(rom, &entry.map_signatures));
        }
    }

    let status = if fingerprint.confidence >= 0.5 && !fingerprint.manufacturer.eq_ignore_ascii_case("Unknown")
    {
        IntegrityStatus::FamilyMatchModifiedOrUnknown
    } else {
        IntegrityStatus::Unrecognized
    };

    Ok(StockRomMatchReport {
        sha256,
        rom_size: rom.len(),
        pack_name: pack.pack_name,
        integrity_status: status,
        matched_entry: None,
        identity_tokens: tokens,
        fingerprint,
        anchored_maps,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifies_exact_sha256_and_detects_modified_tune() {
        let mut stock = vec![0xAAu8; 512];
        stock[32..40].copy_from_slice(b"K64J108 ");
        stock[128..134].copy_from_slice(&[0x11, 0x22, 0x99, 0x44, 0x55, 0x66]);
        let stock_sha = compute_rom_sha256_hex(&stock);

        let pack = StockRomRegistryPack {
            schema_version: 1,
            pack_name: "Unit Test Pack".into(),
            entries: vec![StockRomEntry {
                id: "test_k64".into(),
                label: "Test K64 Unit ROM".into(),
                manufacturer: "Honda".into(),
                ecu_family: "Keihin".into(),
                sw_version: "K64J108".into(),
                size_bytes: 512,
                sha256: Some(stock_sha.clone()),
                identity_tokens: vec!["K64J108".into()],
                map_signatures: vec![MapSignatureAnchor {
                    name: "Ignition Timing Main".into(),
                    category: "Ignition".into(),
                    signature: "11 22 ?? 44".into(),
                    relative_offset: 4,
                    rows: 1,
                    cols: 2,
                    cell_bits: 8,
                    little_endian: false,
                    unit: "deg".into(),
                }],
            }],
        };
        let pack_json = serde_json::to_string(&pack).unwrap();

        let exact_report = match_stock_rom_registry(&stock, Some(&pack_json)).unwrap();
        assert_eq!(exact_report.integrity_status, IntegrityStatus::ExactStockMatch);
        assert_eq!(exact_report.sha256, stock_sha);
        assert_eq!(exact_report.anchored_maps.len(), 1);
        assert_eq!(exact_report.anchored_maps[0].address, 132);

        let mut tuned = stock;
        tuned[200] ^= 0xFF;
        let mod_report = match_stock_rom_registry(&tuned, Some(&pack_json)).unwrap();
        assert_eq!(
            mod_report.integrity_status,
            IntegrityStatus::FamilyMatchModifiedOrUnknown
        );
        assert_ne!(mod_report.sha256, stock_sha);
        assert_eq!(mod_report.anchored_maps.len(), 1);
    }
}
