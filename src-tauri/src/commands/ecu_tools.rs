//! IPC handlers for Phase 2–4 native ECU engines:
//! fingerprinting, Denso SH descriptor scanning, DTC table/switch scanning,
//! 22+ OEM checksum engines, blind checksum sweep, XDF `<XDFCHECKSUM>`,
//! unified Seed/Key challenge solver, and motorcycle flash boundary checks.

use super::window_io::{mutate_active_rom_with_undo, ScanProgressEvent};
use super::AppState;
use kivi_checksum::custom_plugin::{
    parse_custom_checksum_plugin, repair_custom_checksum_plugin,
    sample_custom_checksum_plugin_json, verify_custom_checksum_plugin, CustomChecksumReport,
};
use kivi_checksum::oem::bosch_edc17::{
    blind_checksum_sweep, repair_edc17_crc32c_owned, verify_edc17_crc32c_owned,
    OwnedTriCoreCrcBlockSpec,
};
use kivi_checksum::oem::registry::{
    auto_detect_oem_checksums, list_oem_algorithms, repair_oem_by_id, verify_oem_by_id,
};
use kivi_checksum::oem::{OemAlgorithmInfo, OemChecksumReport, SweepMatch};
use kivi_checksum::xdf_checksum::{
    parse_xdf_checksums, repair_xdf_checksums, verify_xdf_checksum, XdfChecksumResult,
    XdfChecksumSpec,
};
use kivi_flashing::moto_ecu_db::{
    check_moto_write_boundaries, list_moto_ecu_profiles, MotoEcuProfile,
};
use kivi_flashing::seed_key_registry::{
    compute_oem_seed_key, list_seed_key_algorithms, SeedKeyEngineInfo, SeedKeyResult,
};
use kivi_protocol::ostrich::{
    OstrichClient, OstrichTransferSummary, OstrichVerifyReport, OstrichVersion,
    SerialOstrichTransport, OSTRICH_DEFAULT_BAUD,
};
use kivi_scanner::denso_descriptor_maps::{
    scan_denso_sh_descriptor_maps, DensoDescriptorMapHit,
};
use kivi_scanner::dtc_table_scan::{scan_dtc_tables, DtcTableCandidate};
use kivi_scanner::ecu_fingerprint::{identify_ecu_rom, EcuFingerprint};
use serde::{Deserialize, Serialize};
use tauri::State;

/// Executes a read-only closure over either explicit bytes or the active workspace ROM without cloning.
fn with_workspace_rom<T>(
    data: Option<&[u8]>,
    state: &AppState,
    f: impl FnOnce(&[u8]) -> Result<T, String>,
) -> Result<T, String> {
    if let Some(bytes) = data {
        if bytes.is_empty() {
            return Err("ROM buffer cannot be empty".to_string());
        }
        return f(bytes);
    }
    let guard = super::lock_or_err(&state.memory, "memory")?;
    let mem = guard.as_ref().ok_or_else(|| "No binary loaded".to_string())?;
    f(mem.as_slice())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OemRepairPayload {
    pub report: OemChecksumReport,
    pub repaired_bytes: Option<Vec<u8>>,
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct XdfRepairPayload {
    pub results: Vec<XdfChecksumResult>,
    pub repaired_bytes: Option<Vec<u8>>,
    pub sha256: Option<String>,
}

// ---------------------------------------------------------------------------
// Fingerprinting, Denso descriptors, and DTC scanner
// ---------------------------------------------------------------------------

pub fn fingerprint_ecu_rom_inner(
    data: Option<Vec<u8>>,
    state: &AppState,
) -> Result<EcuFingerprint, String> {
    with_workspace_rom(data.as_deref(), state, |bytes| Ok(identify_ecu_rom(bytes)))
}

#[tauri::command]
pub fn fingerprint_ecu_rom(
    data: Option<Vec<u8>>,
    state: State<'_, AppState>,
) -> Result<EcuFingerprint, String> {
    fingerprint_ecu_rom_inner(data, &state)
}

pub fn scan_denso_sh_descriptors_inner(
    data: Option<Vec<u8>>,
    state: &AppState,
) -> Result<Vec<DensoDescriptorMapHit>, String> {
    with_workspace_rom(data.as_deref(), state, |bytes| {
        Ok(scan_denso_sh_descriptor_maps(bytes))
    })
}

#[tauri::command]
pub fn scan_denso_sh_descriptors(
    data: Option<Vec<u8>>,
    state: State<'_, AppState>,
) -> Result<Vec<DensoDescriptorMapHit>, String> {
    scan_denso_sh_descriptors_inner(data, &state)
}

pub fn scan_dtc_tables_inner(
    min_entries: Option<usize>,
    data: Option<Vec<u8>>,
    state: &AppState,
) -> Result<Vec<DtcTableCandidate>, String> {
    let min = min_entries.unwrap_or(6).max(2);
    with_workspace_rom(data.as_deref(), state, |bytes| Ok(scan_dtc_tables(bytes, min)))
}

#[tauri::command]
pub fn scan_dtc_tables_cmd(
    min_entries: Option<usize>,
    data: Option<Vec<u8>>,
    state: State<'_, AppState>,
) -> Result<Vec<DtcTableCandidate>, String> {
    scan_dtc_tables_inner(min_entries, data, &state)
}

// ---------------------------------------------------------------------------
// OEM Checksums, Blind Checksum Sweep, and XDF Checksums
// ---------------------------------------------------------------------------

pub fn list_oem_checksum_engines_inner() -> Vec<OemAlgorithmInfo> {
    list_oem_algorithms()
}

#[tauri::command]
pub fn list_oem_checksum_engines() -> Vec<OemAlgorithmInfo> {
    list_oem_checksum_engines_inner()
}

pub fn auto_detect_oem_checksums_inner(
    data: Option<Vec<u8>>,
    state: &AppState,
) -> Result<Vec<OemChecksumReport>, String> {
    with_workspace_rom(data.as_deref(), state, |bytes| Ok(auto_detect_oem_checksums(bytes)))
}

#[tauri::command]
pub fn auto_detect_oem_checksums_cmd(
    data: Option<Vec<u8>>,
    state: State<'_, AppState>,
) -> Result<Vec<OemChecksumReport>, String> {
    auto_detect_oem_checksums_inner(data, &state)
}

pub fn verify_oem_checksum_inner(
    algorithm_id: &str,
    data: Option<Vec<u8>>,
    state: &AppState,
) -> Result<OemChecksumReport, String> {
    with_workspace_rom(data.as_deref(), state, |bytes| verify_oem_by_id(bytes, algorithm_id))
}

#[tauri::command]
pub fn verify_oem_checksum_cmd(
    algorithm_id: String,
    data: Option<Vec<u8>>,
    state: State<'_, AppState>,
) -> Result<OemChecksumReport, String> {
    verify_oem_checksum_inner(&algorithm_id, data, &state)
}

pub fn repair_oem_checksum_inner(
    algorithm_id: &str,
    data: Option<Vec<u8>>,
    apply_to_loaded: bool,
    state: &AppState,
) -> Result<OemRepairPayload, String> {
    if apply_to_loaded && data.is_none() {
        let (_repaired, hashes, _undo, report) = mutate_active_rom_with_undo(
            state,
            &format!("Repair OEM Checksum ({algorithm_id})"),
            |bytes| repair_oem_by_id(bytes, algorithm_id),
        )?;
        return Ok(OemRepairPayload {
            report,
            repaired_bytes: None,
            sha256: Some(hashes.sha256),
        });
    }
    let (repaired, report) = with_workspace_rom(data.as_deref(), state, |bytes| {
        repair_oem_by_id(bytes, algorithm_id)
    })?;
    Ok(OemRepairPayload {
        report,
        repaired_bytes: Some(repaired),
        sha256: None,
    })
}

#[tauri::command]
pub fn repair_oem_checksum_cmd(
    algorithm_id: String,
    data: Option<Vec<u8>>,
    apply_to_loaded: Option<bool>,
    state: State<'_, AppState>,
) -> Result<OemRepairPayload, String> {
    repair_oem_checksum_inner(&algorithm_id, data, apply_to_loaded.unwrap_or(true), &state)
}

#[tauri::command]
pub fn verify_edc17_crc32c_cmd(
    specs: Vec<OwnedTriCoreCrcBlockSpec>,
    data: Option<Vec<u8>>,
    state: State<'_, AppState>,
) -> Result<OemChecksumReport, String> {
    if specs.is_empty() {
        return Err("At least one EDC17 CRC-32C block spec is required".to_string());
    }
    with_workspace_rom(data.as_deref(), &state, |bytes| {
        Ok(verify_edc17_crc32c_owned(bytes, &specs))
    })
}

#[tauri::command]
pub fn repair_edc17_crc32c_cmd(
    specs: Vec<OwnedTriCoreCrcBlockSpec>,
    data: Option<Vec<u8>>,
    apply_to_loaded: Option<bool>,
    state: State<'_, AppState>,
) -> Result<OemRepairPayload, String> {
    if specs.is_empty() {
        return Err("At least one EDC17 CRC-32C block spec is required".to_string());
    }
    let apply = apply_to_loaded.unwrap_or(true);
    if apply && data.is_none() {
        let (_repaired, hashes, _undo, report) =
            mutate_active_rom_with_undo(&state, "Repair EDC17 CRC-32C", |bytes| {
                repair_edc17_crc32c_owned(bytes, &specs)
            })?;
        return Ok(OemRepairPayload {
            report,
            repaired_bytes: None,
            sha256: Some(hashes.sha256),
        });
    }
    let (repaired, report) = with_workspace_rom(data.as_deref(), &state, |bytes| {
        repair_edc17_crc32c_owned(bytes, &specs)
    })?;
    Ok(OemRepairPayload {
        report,
        repaired_bytes: Some(repaired),
        sha256: None,
    })
}

pub fn sweep_blind_checksums_inner(
    data: Option<Vec<u8>>,
    state: &AppState,
) -> Result<Vec<SweepMatch>, String> {
    with_workspace_rom(data.as_deref(), state, |bytes| Ok(blind_checksum_sweep(bytes)))
}

#[tauri::command]
pub fn sweep_blind_checksums_cmd(
    data: Option<Vec<u8>>,
    state: State<'_, AppState>,
) -> Result<Vec<SweepMatch>, String> {
    sweep_blind_checksums_inner(data, &state)
}

#[tauri::command]
pub async fn sweep_blind_checksums_stream(
    on_progress: tauri::ipc::Channel<ScanProgressEvent>,
    state: State<'_, AppState>,
) -> Result<Vec<SweepMatch>, String> {
    let snapshot = {
        let guard = super::lock_or_err(&state.memory, "memory")?;
        let mem = guard.as_ref().ok_or_else(|| "No binary loaded".to_string())?;
        mem.as_slice().to_vec()
    };
    let _ = on_progress.send(ScanProgressEvent {
        stage: "sweeping".to_string(),
        percent: 25,
        message: format!("Sweeping {} bytes across checksum polynomials", snapshot.len()),
    });
    let result = tauri::async_runtime::spawn_blocking(move || blind_checksum_sweep(&snapshot))
        .await
        .map_err(|e| format!("Checksum sweep worker failed: {e}"))?;
    let _ = on_progress.send(ScanProgressEvent {
        stage: "complete".to_string(),
        percent: 100,
        message: format!("Sweep finished ({} candidate matches)", result.len()),
    });
    Ok(result)
}

#[tauri::command]
pub fn parse_xdf_checksums_cmd(xml: String) -> Result<Vec<XdfChecksumSpec>, String> {
    parse_xdf_checksums(&xml)
}

pub fn verify_xdf_checksums_inner(
    xml: &str,
    data: Option<Vec<u8>>,
    state: &AppState,
) -> Result<Vec<XdfChecksumResult>, String> {
    let specs = parse_xdf_checksums(xml)?;
    with_workspace_rom(data.as_deref(), state, |bytes| {
        specs
            .iter()
            .map(|spec| verify_xdf_checksum(bytes, spec))
            .collect()
    })
}

#[tauri::command]
pub fn verify_xdf_checksums_cmd(
    xml: String,
    data: Option<Vec<u8>>,
    state: State<'_, AppState>,
) -> Result<Vec<XdfChecksumResult>, String> {
    verify_xdf_checksums_inner(&xml, data, &state)
}

pub fn repair_xdf_checksums_inner(
    xml: &str,
    apply_to_loaded: bool,
    data: Option<Vec<u8>>,
    state: &AppState,
) -> Result<XdfRepairPayload, String> {
    let specs = parse_xdf_checksums(xml)?;
    if apply_to_loaded && data.is_none() {
        let (_repaired, hashes, _undo, results) =
            mutate_active_rom_with_undo(state, "Repair XDF Checksums", |bytes| {
                repair_xdf_checksums(bytes, &specs)
            })?;
        return Ok(XdfRepairPayload {
            results,
            repaired_bytes: None,
            sha256: Some(hashes.sha256),
        });
    }
    let (repaired, results) = with_workspace_rom(data.as_deref(), state, |bytes| {
        repair_xdf_checksums(bytes, &specs)
    })?;
    Ok(XdfRepairPayload {
        results,
        repaired_bytes: Some(repaired),
        sha256: None,
    })
}

#[tauri::command]
pub fn repair_xdf_checksums_cmd(
    xml: String,
    apply_to_loaded: Option<bool>,
    data: Option<Vec<u8>>,
    state: State<'_, AppState>,
) -> Result<XdfRepairPayload, String> {
    repair_xdf_checksums_inner(&xml, apply_to_loaded.unwrap_or(true), data, &state)
}

// ---------------------------------------------------------------------------
// Unified OEM Seed/Key & Motorcycle ECU Flash Boundaries
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_seed_key_algorithms_cmd() -> Vec<SeedKeyEngineInfo> {
    list_seed_key_algorithms()
}

#[tauri::command]
pub fn compute_oem_seed_key_cmd(
    algorithm_id: String,
    seed_hex: String,
    algo_index: u16,
    extra_hex: Option<String>,
) -> Result<SeedKeyResult, String> {
    compute_oem_seed_key(
        &algorithm_id,
        &seed_hex,
        algo_index,
        extra_hex.as_deref(),
        None,
    )
}

#[tauri::command]
pub fn list_moto_ecu_profiles_cmd(query: Option<String>) -> Vec<MotoEcuProfile> {
    let profiles = list_moto_ecu_profiles();
    let Some(q) = query.map(|s| s.trim().to_ascii_lowercase()).filter(|s| !s.is_empty()) else {
        return profiles;
    };
    profiles
        .into_iter()
        .filter(|p| {
            p.id.to_ascii_lowercase().contains(&q)
                || p.make.to_ascii_lowercase().contains(&q)
                || p.model.to_ascii_lowercase().contains(&q)
                || p.part_number.to_ascii_lowercase().contains(&q)
                || p.mcu.to_ascii_lowercase().contains(&q)
        })
        .collect()
}

#[tauri::command]
pub fn check_moto_write_boundaries_cmd(
    profile_id: String,
    write_start: u32,
    write_len: usize,
) -> Result<(), String> {
    check_moto_write_boundaries(&profile_id, write_start, write_len)
}

// ---------------------------------------------------------------------------
// Declarative JSON custom checksum plugins
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn sample_custom_checksum_plugin_cmd() -> String {
    sample_custom_checksum_plugin_json()
}

pub fn verify_custom_checksum_plugin_inner(
    plugin_json: &str,
    data: Option<&[u8]>,
    state: &AppState,
) -> Result<CustomChecksumReport, String> {
    let plugin = parse_custom_checksum_plugin(plugin_json)?;
    with_workspace_rom(data, state, |bytes| verify_custom_checksum_plugin(bytes, &plugin))
}

#[tauri::command]
pub fn verify_custom_checksum_plugin_cmd(
    plugin_json: String,
    data: Option<Vec<u8>>,
    state: State<'_, AppState>,
) -> Result<CustomChecksumReport, String> {
    verify_custom_checksum_plugin_inner(&plugin_json, data.as_deref(), &state)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomChecksumRepairPayload {
    pub report: CustomChecksumReport,
    pub repaired_bytes: Option<Vec<u8>>,
    pub sha256: Option<String>,
}

pub fn repair_custom_checksum_plugin_inner(
    plugin_json: &str,
    apply_to_loaded: bool,
    data: Option<Vec<u8>>,
    state: &AppState,
) -> Result<CustomChecksumRepairPayload, String> {
    let plugin = parse_custom_checksum_plugin(plugin_json)?;
    if apply_to_loaded && data.is_none() {
        let (_repaired, hashes, _undo, report) = mutate_active_rom_with_undo(
            state,
            &format!("Repair Custom Checksum ({})", plugin.id),
            |bytes| {
                let mut working = bytes.to_vec();
                let report = repair_custom_checksum_plugin(&mut working, &plugin)?;
                Ok((working, report))
            },
        )?;
        return Ok(CustomChecksumRepairPayload {
            report,
            repaired_bytes: None,
            sha256: Some(hashes.sha256),
        });
    }

    let (repaired, report) = with_workspace_rom(data.as_deref(), state, |bytes| {
        let mut working = bytes.to_vec();
        let report = repair_custom_checksum_plugin(&mut working, &plugin)?;
        Ok((working, report))
    })?;
    Ok(CustomChecksumRepairPayload {
        report,
        repaired_bytes: Some(repaired),
        sha256: None,
    })
}

#[tauri::command]
pub fn repair_custom_checksum_plugin_workspace_cmd(
    plugin_json: String,
    apply_to_loaded: Option<bool>,
    data: Option<Vec<u8>>,
    state: State<'_, AppState>,
) -> Result<CustomChecksumRepairPayload, String> {
    repair_custom_checksum_plugin_inner(&plugin_json, apply_to_loaded.unwrap_or(true), data, &state)
}

// ---------------------------------------------------------------------------
// Ostrich 2.0 Real-Time EPROM Emulator
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn ostrich_probe_cmd(
    port: String,
    baud_rate: Option<u32>,
) -> Result<OstrichVersion, String> {
    let baud = baud_rate.unwrap_or(OSTRICH_DEFAULT_BAUD);
    let transport = SerialOstrichTransport::open(&port, baud).map_err(|e| e.to_string())?;
    let mut client = OstrichClient::new(transport);
    client.probe().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ostrich_write_range_cmd(
    port: String,
    bank: u8,
    start_addr: u16,
    bytes: Vec<u8>,
    baud_rate: Option<u32>,
) -> Result<OstrichTransferSummary, String> {
    let baud = baud_rate.unwrap_or(OSTRICH_DEFAULT_BAUD);
    let transport = SerialOstrichTransport::open(&port, baud).map_err(|e| e.to_string())?;
    let mut client = OstrichClient::new(transport);
    client
        .write_range(bank, start_addr, &bytes)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ostrich_upload_active_rom_cmd(
    port: String,
    bank: u8,
    base_addr: Option<u16>,
    baud_rate: Option<u32>,
    state: State<'_, AppState>,
) -> Result<OstrichTransferSummary, String> {
    let rom_bytes = {
        let guard = super::lock_or_err(&state.memory, "memory")?;
        let mem = guard.as_ref().ok_or_else(|| "No binary loaded".to_string())?;
        mem.as_slice().to_vec()
    };
    let baud = baud_rate.unwrap_or(OSTRICH_DEFAULT_BAUD);
    let transport = SerialOstrichTransport::open(&port, baud).map_err(|e| e.to_string())?;
    let mut client = OstrichClient::new(transport);
    client
        .write_range(bank, base_addr.unwrap_or(0), &rom_bytes)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ostrich_verify_active_rom_cmd(
    port: String,
    bank: u8,
    base_addr: Option<u16>,
    baud_rate: Option<u32>,
    state: State<'_, AppState>,
) -> Result<OstrichVerifyReport, String> {
    let rom_bytes = {
        let guard = super::lock_or_err(&state.memory, "memory")?;
        let mem = guard.as_ref().ok_or_else(|| "No binary loaded".to_string())?;
        mem.as_slice().to_vec()
    };
    let baud = baud_rate.unwrap_or(OSTRICH_DEFAULT_BAUD);
    let transport = SerialOstrichTransport::open(&port, baud).map_err(|e| e.to_string())?;
    let mut client = OstrichClient::new(transport);
    client
        .verify_range(bank, base_addr.unwrap_or(0), &rom_bytes)
        .await
        .map_err(|e| e.to_string())
}
