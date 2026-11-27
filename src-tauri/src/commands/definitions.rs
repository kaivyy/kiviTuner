use super::window_io::mutate_active_rom_with_undo;
use super::AppState;
use kivi_core::undo_history::UndoStatus;
use serde::{Deserialize, Serialize};
use tauri::State;

// ---------------------------------------------------------------------------
// Definition import (A2L / DAMOS), map editing algebra, similarity & classifier
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct A2lMapSummary {
    pub name: String,
    pub description: String,
    pub kind: String,
    pub address: u32,
    pub rows: usize,
    pub cols: usize,
    pub record_layout: String,
    pub conversion: String,
    pub unit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct A2lImportSummary {
    pub ecu_base_suggestions: Vec<u32>,
    pub characteristic_count: usize,
    pub map_count: usize,
    pub curve_count: usize,
    pub value_count: usize,
    pub axis_pts_count: usize,
    pub memory_segment_count: usize,
    pub entries: Vec<A2lMapSummary>,
    pub diagnostics: Vec<String>,
}

pub fn parse_a2l_content_inner(src: &str) -> Result<A2lImportSummary, String> {
    use kivi_scanner::a2l_parse::{parse_a2l, A2lCharacteristicType};

    if src.trim().is_empty() {
        return Err("A2L input is empty".to_string());
    }
    let doc = parse_a2l(src).map_err(|e| e.to_string())?;
    let mut map_count = 0usize;
    let mut curve_count = 0usize;
    let mut value_count = 0usize;
    let mut entries = Vec::with_capacity(doc.characteristics.len());

    for c in &doc.characteristics {
        let kind_str = match c.kind {
            A2lCharacteristicType::Map => {
                map_count += 1;
                "MAP"
            }
            A2lCharacteristicType::Curve => {
                curve_count += 1;
                "CURVE"
            }
            A2lCharacteristicType::Value => {
                value_count += 1;
                "VALUE"
            }
            A2lCharacteristicType::Ascii => "ASCII",
            A2lCharacteristicType::ValBlk => "VAL_BLK",
            A2lCharacteristicType::Unknown => "UNKNOWN",
        };
        let (rows, cols) = c.dimensions().unwrap_or((1, 1));
        let unit = doc
            .conversion_for(&c.conversion)
            .map(|cv| cv.unit.clone())
            .unwrap_or_default();
        entries.push(A2lMapSummary {
            name: c.name.clone(),
            description: c.description.clone(),
            kind: kind_str.to_string(),
            address: c.address,
            rows,
            cols,
            record_layout: c.record_layout.clone(),
            conversion: c.conversion.clone(),
            unit,
        });
    }

    let mut ecu_base_suggestions: Vec<u32> = Vec::new();
    for seg in &doc.memory_segments {
        if seg.size > 0
            && !matches!(seg.prg_type.as_str(), "VARIABLES" | "SERAM" | "OFFLINE_DATA")
            && !matches!(seg.memory_type.as_str(), "RAM" | "REGISTER")
            && !ecu_base_suggestions.contains(&seg.address)
        {
            ecu_base_suggestions.push(seg.address);
        }
    }

    Ok(A2lImportSummary {
        ecu_base_suggestions,
        characteristic_count: doc.characteristics.len(),
        map_count,
        curve_count,
        value_count,
        axis_pts_count: doc.axis_pts.len(),
        memory_segment_count: doc.memory_segments.len(),
        entries,
        diagnostics: doc.diagnostics,
    })
}

pub fn parse_a2l_file_inner(path: &str) -> Result<A2lImportSummary, String> {
    let bytes = super::read_bounded_file_bytes(path, super::MAX_FILE_READ_BYTES)?;
    let text = String::from_utf8_lossy(&bytes);
    parse_a2l_content_inner(&text)
}

#[tauri::command]
pub fn parse_a2l_content(content: String) -> Result<A2lImportSummary, String> {
    parse_a2l_content_inner(&content)
}

#[tauri::command]
pub fn parse_a2l_file(path: String) -> Result<A2lImportSummary, String> {
    parse_a2l_file_inner(&path)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DamosEntrySummary {
    pub source: String,
    pub name: String,
    pub description: String,
    pub address: u32,
    pub rows: usize,
    pub cols: usize,
    pub elem_bytes: usize,
    pub is_signed: bool,
    pub is_2d_map: bool,
    pub is_1d_curve: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DamosImportSummary {
    pub ecu: Option<String>,
    pub family: Option<String>,
    pub base_address: Option<u32>,
    pub ump_count: usize,
    pub spz_total_count: usize,
    pub spz_2d_map_count: usize,
    pub spz_1d_curve_count: usize,
    pub characteristic_count: usize,
    pub entries: Vec<DamosEntrySummary>,
}

pub fn parse_damos_content_inner(src: &str) -> Result<DamosImportSummary, String> {
    use kivi_scanner::damos_parse::parse_damos;

    let doc = parse_damos(src).map_err(|e| e.to_string())?;
    let mut spz_2d_map_count = 0usize;
    let mut spz_1d_curve_count = 0usize;
    let mut entries = Vec::with_capacity(doc.spz_objects.len() + doc.maps.len());

    for o in &doc.spz_objects {
        let is_2d = o.is_2d_map();
        let is_1d = o.is_1d_curve();
        if is_2d {
            spz_2d_map_count += 1;
        }
        if is_1d {
            spz_1d_curve_count += 1;
        }
        entries.push(DamosEntrySummary {
            source: format!("SPZ_TYPE_{}", o.spz_kind),
            name: o.name.clone(),
            description: o.description.clone(),
            address: o.address,
            rows: o.rows,
            cols: o.cols,
            elem_bytes: o.elem_bytes,
            is_signed: o.is_signed,
            is_2d_map: is_2d,
            is_1d_curve: is_1d,
        });
    }
    for m in &doc.maps {
        let is_2d = m.rows > 1 && m.cols > 1;
        let is_1d = m.cols >= 4 && m.rows <= 1;
        entries.push(DamosEntrySummary {
            source: "UMP".to_string(),
            name: m.name.clone(),
            description: m.description.clone(),
            address: m.address,
            rows: m.rows,
            cols: m.cols,
            elem_bytes: 1,
            is_signed: false,
            is_2d_map: is_2d,
            is_1d_curve: is_1d,
        });
    }

    Ok(DamosImportSummary {
        ecu: doc.ecu,
        family: doc.family,
        base_address: doc.base_address,
        ump_count: doc.maps.len(),
        spz_total_count: doc.spz_objects.len(),
        spz_2d_map_count,
        spz_1d_curve_count,
        characteristic_count: doc.characteristics.len(),
        entries,
    })
}

pub fn parse_damos_file_inner(path: &str) -> Result<DamosImportSummary, String> {
    let bytes = super::read_bounded_file_bytes(path, super::MAX_FILE_READ_BYTES)?;
    let text = String::from_utf8_lossy(&bytes);
    parse_damos_content_inner(&text)
}

#[tauri::command]
pub fn parse_damos_content(content: String) -> Result<DamosImportSummary, String> {
    parse_damos_content_inner(&content)
}

#[tauri::command]
pub fn parse_damos_file(path: String) -> Result<DamosImportSummary, String> {
    parse_damos_file_inner(&path)
}



#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairChecksumResult {
    pub rom: Vec<u8>,
    pub match_info: kivi_checksum::multi_ecu::DetectedChecksumMatch,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairChecksumWorkspaceResult {
    pub sha256: String,
    pub match_info: kivi_checksum::multi_ecu::DetectedChecksumMatch,
    pub undo: UndoStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyXdfPatchResult {
    pub rom: Vec<u8>,
    pub status_after: kivi_defs::patch::PatchStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyXdfPatchWorkspaceResult {
    pub sha256: String,
    pub status_after: kivi_defs::patch::PatchStatus,
    pub undo: UndoStatus,
}

pub fn list_checksum_engines_inner() -> Vec<kivi_checksum::multi_ecu::ChecksumEngineInfo> {
    kivi_checksum::multi_ecu::list_supported_checksum_engines()
}

#[tauri::command]
pub fn list_checksum_engines() -> Vec<kivi_checksum::multi_ecu::ChecksumEngineInfo> {
    list_checksum_engines_inner()
}

pub fn auto_detect_checksums_inner(
    rom: Option<&[u8]>,
    state: &AppState,
) -> Result<Vec<kivi_checksum::multi_ecu::DetectedChecksumMatch>, String> {
    if let Some(bytes) = rom {
        return Ok(kivi_checksum::multi_ecu::auto_detect_checksums(bytes));
    }
    let guard = super::lock_or_err(&state.memory, "memory")?;
    let mem = guard.as_ref().ok_or_else(|| "No binary loaded".to_string())?;
    Ok(kivi_checksum::multi_ecu::auto_detect_checksums(
        mem.as_slice(),
    ))
}

#[tauri::command]
pub fn auto_detect_checksums(
    rom: Option<Vec<u8>>,
    state: State<'_, AppState>,
) -> Result<Vec<kivi_checksum::multi_ecu::DetectedChecksumMatch>, String> {
    auto_detect_checksums_inner(rom.as_deref(), &state)
}

pub fn repair_checksum_inner(
    rom: Option<&[u8]>,
    algorithm_id: &str,
    mode: kivi_checksum::multi_ecu::ChecksumRepairMode,
    state: &AppState,
) -> Result<RepairChecksumResult, String> {
    if let Some(bytes) = rom {
        let (fixed, match_info) =
            kivi_checksum::multi_ecu::repair_checksum_by_id(bytes, algorithm_id, mode)?;
        return Ok(RepairChecksumResult {
            rom: fixed,
            match_info,
        });
    }

    let (fixed, _hashes, _undo, match_info) = mutate_active_rom_with_undo(
        state,
        &format!("Repair checksum ({algorithm_id})"),
        |bytes| kivi_checksum::multi_ecu::repair_checksum_by_id(bytes, algorithm_id, mode),
    )?;
    Ok(RepairChecksumResult {
        rom: fixed,
        match_info,
    })
}

#[tauri::command]
pub fn repair_checksum(
    rom: Option<Vec<u8>>,
    algorithm_id: String,
    mode: kivi_checksum::multi_ecu::ChecksumRepairMode,
    state: State<'_, AppState>,
) -> Result<RepairChecksumResult, String> {
    repair_checksum_inner(rom.as_deref(), &algorithm_id, mode, &state)
}

#[tauri::command]
pub fn repair_checksum_workspace(
    algorithm_id: String,
    mode: kivi_checksum::multi_ecu::ChecksumRepairMode,
    state: State<'_, AppState>,
) -> Result<RepairChecksumWorkspaceResult, String> {
    let (_fixed, hashes, undo, match_info) = mutate_active_rom_with_undo(
        &state,
        &format!("Repair checksum ({algorithm_id})"),
        |bytes| kivi_checksum::multi_ecu::repair_checksum_by_id(bytes, &algorithm_id, mode),
    )?;
    Ok(RepairChecksumWorkspaceResult {
        sha256: hashes.sha256,
        match_info,
        undo,
    })
}

pub fn parse_xdf_full_details_inner(
    xml: &str,
    state: &AppState,
) -> Result<kivi_defs::model::Definitions, String> {
    let defs = kivi_defs::xdf::parse_xdf_full(xml).map_err(|e| e.to_string())?;
    *super::lock_or_err(&state.maps, "maps")? = defs.maps.clone();
    Ok(defs)
}

#[tauri::command]
pub fn parse_xdf_full_details(
    xml: String,
    state: State<'_, AppState>,
) -> Result<kivi_defs::model::Definitions, String> {
    parse_xdf_full_details_inner(&xml, &state)
}

pub fn apply_xdf_patch_inner(
    rom: Option<&[u8]>,
    patch: &kivi_defs::model::PatchDef,
    revert: bool,
    state: &AppState,
) -> Result<ApplyXdfPatchResult, String> {
    if let Some(bytes) = rom {
        let fixed = kivi_defs::patch::apply_patch_safe(bytes, patch, revert)?;
        let status_after = kivi_defs::patch::inspect_patch_status(&fixed, patch);
        return Ok(ApplyXdfPatchResult {
            rom: fixed,
            status_after,
        });
    }

    let action = if revert { "Revert" } else { "Apply" };
    let (fixed, _hashes, _undo, status_after) = mutate_active_rom_with_undo(
        state,
        &format!("{action} XDF patch {}", patch.name),
        |bytes| {
            let fixed = kivi_defs::patch::apply_patch_safe(bytes, patch, revert)?;
            let status_after = kivi_defs::patch::inspect_patch_status(&fixed, patch);
            Ok((fixed, status_after))
        },
    )?;
    Ok(ApplyXdfPatchResult {
        rom: fixed,
        status_after,
    })
}

#[tauri::command]
pub fn apply_xdf_patch(
    rom: Option<Vec<u8>>,
    patch: kivi_defs::model::PatchDef,
    revert: bool,
    state: State<'_, AppState>,
) -> Result<ApplyXdfPatchResult, String> {
    apply_xdf_patch_inner(rom.as_deref(), &patch, revert, &state)
}

#[tauri::command]
pub fn apply_xdf_patch_workspace(
    patch: kivi_defs::model::PatchDef,
    revert: bool,
    state: State<'_, AppState>,
) -> Result<ApplyXdfPatchWorkspaceResult, String> {
    let action = if revert { "Revert" } else { "Apply" };
    let (_fixed, hashes, undo, status_after) = mutate_active_rom_with_undo(
        &state,
        &format!("{action} XDF patch {}", patch.name),
        |bytes| {
            let fixed = kivi_defs::patch::apply_patch_safe(bytes, &patch, revert)?;
            let status_after = kivi_defs::patch::inspect_patch_status(&fixed, &patch);
            Ok((fixed, status_after))
        },
    )?;
    Ok(ApplyXdfPatchWorkspaceResult {
        sha256: hashes.sha256,
        status_after,
        undo,
    })
}
