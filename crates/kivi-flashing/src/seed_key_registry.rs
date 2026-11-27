//! Unified OEM Seed/Key registry combining GM VPW/GMLAN/Other, VAG SA2 bytecode,
//! Subaru Denso/Hitachi, Bosch ME7/EDC16, and Honda Keihin/Shindengen.

use crate::gm_seed_key_tables::{gm_can_key, gm_vpw_key, GM_OTHER_ALGO_BASE};
use crate::honda_kline::ShindengenSeedKeyEngine;
use crate::sa2_seed_key::{Sa2Error, Sa2SeedKey};
use crate::specialized_flasher::FeistelCanKernelSecurity;
use crate::uds::SeedKeyAlgorithm;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SeedKeyEngineInfo {
    pub id: String,
    pub name: String,
    pub family: String,
    pub key_bits: u8,
    pub verified_for_live_hardware: bool,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SeedKeyResult {
    pub algorithm_id: String,
    pub key_hex: String,
    pub key_bytes: Vec<u8>,
}

/// Returns true only when `algorithm_id` is backed by a documented real implementation
/// rather than a simulated bench placeholder.
pub fn is_seed_key_verified_for_live(algorithm_id: &str) -> bool {
    list_seed_key_algorithms()
        .into_iter()
        .find(|info| info.id == algorithm_id)
        .is_some_and(|info| info.verified_for_live_hardware)
}

/// Lists all supported offline Seed/Key algorithms across OEM families.
pub fn list_seed_key_algorithms() -> Vec<SeedKeyEngineInfo> {
    vec![
        SeedKeyEngineInfo {
            id: "gm_vpw".into(),
            name: "GM Delco Class-2 VPW (256 bytecode algos)".into(),
            family: "GM Delco".into(),
            key_bits: 16,
            verified_for_live_hardware: true,
            notes: "Pass 16-bit seed hex + algo_index (0..255)".into(),
        },
        SeedKeyEngineInfo {
            id: "gm_gmlan".into(),
            name: "GM CAN / GMLAN (256 bytecode algos)".into(),
            family: "GM Delco".into(),
            key_bits: 16,
            verified_for_live_hardware: true,
            notes: "Pass 16-bit seed hex + algo_index (0..255, E38/E67/P12/E92)".into(),
        },
        SeedKeyEngineInfo {
            id: "gm_other".into(),
            name: "GM Other CAN / E92 Legacy (256 bytecode algos)".into(),
            family: "GM Delco".into(),
            key_bits: 16,
            verified_for_live_hardware: true,
            notes: "Pass 16-bit seed hex + algo_index (0..255, E92 OSID 12672612 = algo 1)".into(),
        },
        SeedKeyEngineInfo {
            id: "vag_sa2".into(),
            name: "VAG SA2 Seed/Key Bytecode VM (ODX/FRF/SGO)".into(),
            family: "VAG / Bosch / Simos".into(),
            key_bits: 32,
            verified_for_live_hardware: true,
            notes: "Pass 32-bit seed hex + hex SA2 bytecode tape in extra_hex".into(),
        },
        SeedKeyEngineInfo {
            id: "bosch_me7".into(),
            name: "Bosch ME7 / Delphi Standard Seed/Key".into(),
            family: "Bosch".into(),
            key_bits: 32,
            verified_for_live_hardware: true,
            notes: "Bit rotation + mask (default 0x48A35912 rotl 5)".into(),
        },
        SeedKeyEngineInfo {
            id: "bosch_edc16".into(),
            name: "Bosch EDC16 CAN/K-Line Level 1/3".into(),
            family: "Bosch".into(),
            key_bits: 32,
            verified_for_live_hardware: false,
            notes: "Simulated/bench transform only (blocked on live hardware; mask 0x3D147A59)".into(),
        },
        SeedKeyEngineInfo {
            id: "subaru_denso_sh705x".into(),
            name: "Subaru Denso SH705x / SSM CAN Security".into(),
            family: "Subaru Denso".into(),
            key_bits: 32,
            verified_for_live_hardware: true,
            notes: "16-round Feistel key solver (CAN kernel / SSM)".into(),
        },
        SeedKeyEngineInfo {
            id: "honda_shindengen_rom".into(),
            name: "Honda Shindengen Write Key (from ROM signature)".into(),
            family: "Honda Moto".into(),
            key_bits: 16,
            verified_for_live_hardware: true,
            notes: "Pass 16-bit seed hex + ROM bytes in extra_bytes".into(),
        },
    ]
}

fn parse_hex_bytes(s: &str) -> Result<Vec<u8>, String> {
    let clean: String = s
        .trim()
        .trim_start_matches("0x")
        .trim_start_matches("0X")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    if clean.is_empty() || !clean.len().is_multiple_of(2) {
        return Err("Hex input must have a non-empty even number of digits".to_string());
    }
    let mut out = Vec::with_capacity(clean.len() / 2);
    for chunk in clean.as_bytes().chunks_exact(2) {
        let pair = std::str::from_utf8(chunk).map_err(|_| "Invalid UTF8".to_string())?;
        out.push(u8::from_str_radix(pair, 16).map_err(|_| format!("Invalid hex '{pair}'"))?);
    }
    Ok(out)
}

fn parse_u16_be_from_hex(s: &str) -> Result<u16, String> {
    let bytes = parse_hex_bytes(s)?;
    match bytes.len() {
        2 => Ok(u16::from_be_bytes([bytes[0], bytes[1]])),
        other => Err(format!("Expected 2 seed bytes, got {other}")),
    }
}

fn parse_u32_be_from_hex(s: &str) -> Result<u32, String> {
    let bytes = parse_hex_bytes(s)?;
    match bytes.len() {
        2 => Ok(u16::from_be_bytes([bytes[0], bytes[1]]) as u32),
        4 => Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])),
        other => Err(format!("Expected 2 or 4 seed bytes, got {other}")),
    }
}

/// Solves a SecurityAccess seed challenge with the chosen OEM engine.
pub fn compute_oem_seed_key(
    algorithm_id: &str,
    seed_hex: &str,
    algo_param: u16,
    extra_hex: Option<&str>,
    extra_bytes: Option<&[u8]>,
) -> Result<SeedKeyResult, String> {
    match algorithm_id {
        "gm_vpw" => {
            let seed = parse_u16_be_from_hex(seed_hex)?;
            let key = gm_vpw_key(algo_param as u8, seed);
            let kb = key.to_be_bytes().to_vec();
            Ok(SeedKeyResult {
                algorithm_id: algorithm_id.into(),
                key_hex: format!("{key:04X}"),
                key_bytes: kb,
            })
        }
        "gm_gmlan" => {
            let seed = parse_u16_be_from_hex(seed_hex)?;
            let key = gm_can_key(algo_param & 0xFF, seed);
            let kb = key.to_be_bytes().to_vec();
            Ok(SeedKeyResult {
                algorithm_id: algorithm_id.into(),
                key_hex: format!("{key:04X}"),
                key_bytes: kb,
            })
        }
        "gm_other" => {
            let seed = parse_u16_be_from_hex(seed_hex)?;
            let key = gm_can_key(GM_OTHER_ALGO_BASE + (algo_param & 0xFF), seed);
            let kb = key.to_be_bytes().to_vec();
            Ok(SeedKeyResult {
                algorithm_id: algorithm_id.into(),
                key_hex: format!("{key:04X}"),
                key_bytes: kb,
            })
        }
        "vag_sa2" => {
            let seed = parse_u32_be_from_hex(seed_hex)?;
            let owned_tape;
            let tape = if let Some(h) = extra_hex.filter(|s| !s.trim().is_empty()) {
                owned_tape = parse_hex_bytes(h)?;
                owned_tape.as_slice()
            } else if let Some(b) = extra_bytes.filter(|b| !b.is_empty()) {
                b
            } else {
                return Err("vag_sa2 requires SA2 instruction tape bytes in extra_hex".to_string());
            };
            let key = Sa2SeedKey::new(tape, seed)
                .execute()
                .map_err(|e: Sa2Error| e.to_string())?;
            let kb = key.to_be_bytes().to_vec();
            Ok(SeedKeyResult {
                algorithm_id: algorithm_id.into(),
                key_hex: format!("{key:08X}"),
                key_bytes: kb,
            })
        }
        "bosch_me7" => {
            let seed = parse_u32_be_from_hex(seed_hex)?;
            let key = SeedKeyAlgorithm::default().compute_key(seed);
            let kb = key.to_be_bytes().to_vec();
            Ok(SeedKeyResult {
                algorithm_id: algorithm_id.into(),
                key_hex: format!("{key:08X}"),
                key_bytes: kb,
            })
        }
        "bosch_edc16" => {
            let seed = parse_u32_be_from_hex(seed_hex)?;
            let key = if seed == 0 {
                0
            } else {
                seed.rotate_left(7).wrapping_add(0x3D14_7A59) ^ 0x5A5A_A5A5
            };
            let kb = key.to_be_bytes().to_vec();
            Ok(SeedKeyResult {
                algorithm_id: algorithm_id.into(),
                key_hex: format!("{key:08X}"),
                key_bytes: kb,
            })
        }
        "subaru_denso_sh705x" => {
            let seed = parse_u32_be_from_hex(seed_hex)?;
            let kb = FeistelCanKernelSecurity::solve_stock_security_key(seed.to_be_bytes());
            let key = u32::from_be_bytes(kb);
            Ok(SeedKeyResult {
                algorithm_id: algorithm_id.into(),
                key_hex: format!("{key:08X}"),
                key_bytes: kb.to_vec(),
            })
        }
        "honda_shindengen_rom" => {
            let seed_val = parse_u16_be_from_hex(seed_hex)?;
            let rom = extra_bytes.ok_or_else(|| "honda_shindengen_rom requires loaded ROM bytes".to_string())?;
            let kb = ShindengenSeedKeyEngine::compute_write_key(rom, seed_val.to_be_bytes())
                .map_err(|e| e.to_string())?;
            Ok(SeedKeyResult {
                algorithm_id: algorithm_id.into(),
                key_hex: format!("{:02X}{:02X}", kb[0], kb[1]),
                key_bytes: kb.to_vec(),
            })
        }
        other => Err(format!("Unknown Seed/Key algorithm '{other}'")),
    }
}
