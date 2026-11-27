//! IPC handlers for Phase 3/4 ECU definitions and container formats:
//! ECU XML, ECU `.ini`, binary/plaintext `.kp` & `.ols`, Intel HEX / Motorola SREC /
//! BMW SP-Daten, GM Delco E92 RLE, and VAG SGO / FRF codecs.

use super::rom::save_bin_data_inner;
use super::AppState;
use kivi_core::types::{DType, Endian};
use kivi_defs::ecu_ini::{export_ecu_ini, parse_ecu_ini};
use kivi_defs::ecu_xml::{export_ecu_xml, parse_ecu_xml};
use kivi_defs::model::{Definitions, MapDef, Scaling};
use kivi_defs::xdf_export::export_xdf_definitions;
use kivi_formats::bmw_spdaten::{
    export_intel_hex, export_motorola_s3, parse_hex_or_srec, HexSegment,
};
use kivi_formats::gm_e92_rle::{
    compress as gm_compress, decompress as gm_decompress, is_wrapped as gm_is_wrapped,
    unwrap as gm_unwrap, wrap as gm_wrap,
};
use kivi_formats::ols_project::parse_ols;
use kivi_formats::sgo_frf_pack::{
    frf_pack_odx, frf_unpack_odx, pack_sgo_container, SgoPackBlockSpec,
};
use kivi_scanner::kp_parse::{export_kp_ar_text, parse_kp_any, KpParsedAny};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tauri::State;

fn decode_hex_bytes(hex: &str) -> Result<Vec<u8>, String> {
    let clean: String = hex.chars().filter(|c| !c.is_whitespace()).collect();
    if !clean.len().is_multiple_of(2) {
        return Err("Hex string must contain an even number of nibbles".to_string());
    }
    let mut out = Vec::with_capacity(clean.len() / 2);
    for pair in clean.as_bytes().as_chunks::<2>().0 {
        let s = std::str::from_utf8(pair).map_err(|e| e.to_string())?;
        let b = u8::from_str_radix(s, 16).map_err(|e| format!("Invalid hex byte '{s}': {e}"))?;
        out.push(b);
    }
    Ok(out)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DefinitionImportSummary {
    pub format: String,
    pub label: String,
    pub table_count: usize,
    pub constant_count: usize,
    pub flag_count: usize,
    pub maps: Vec<MapDef>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EcuIniSummary {
    pub signature: String,
    pub query_command: String,
    pub endianness: String,
    pub page_sizes: Vec<usize>,
    pub blocking_factor: usize,
    pub table_count: usize,
    pub constant_count: usize,
    pub flag_count: usize,
    pub maps: Vec<MapDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OlsMapRow {
    pub name: String,
    pub kind: String,
    pub unit: String,
    pub address: u32,
    pub rows: usize,
    pub cols: usize,
    pub data_size: usize,
    pub scale: f64,
    pub offset: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OlsVersionRow {
    pub index: usize,
    pub name: String,
    pub base_address: u32,
    pub byte_len: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OlsProjectSummary {
    pub format_version: u32,
    pub make: String,
    pub model: String,
    pub ecu_name: String,
    pub hw_number: String,
    pub sw_number: String,
    pub versions: Vec<OlsVersionRow>,
    pub maps: Vec<OlsMapRow>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HexSegmentRow {
    pub address: u32,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HexDocumentSummary {
    pub format: String,
    pub metadata: BTreeMap<String, String>,
    pub entry_point: Option<u32>,
    pub segments: Vec<HexSegmentRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FrfUnpackPayload {
    pub entry_name: String,
    pub odx_xml: String,
    pub raw_len: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SgoBlockInput {
    pub block_address: u32,
    pub crypt_mode: u8,
    pub erase_start: u32,
    pub erase_end: u32,
    pub prog_start: u32,
    pub prog_end: u32,
    pub compress_bcb: bool,
    pub raw_data: Vec<u8>,
}

fn commit_maps_if_requested(
    maps: &[MapDef],
    load_into_workspace: bool,
    state: &AppState,
) -> Result<(), String> {
    if load_into_workspace {
        *super::lock_or_err(&state.maps, "maps")? = maps.to_vec();
    }
    Ok(())
}

pub fn parse_ecu_xml_inner(
    xml: &str,
    load_into_workspace: bool,
    state: &AppState,
) -> Result<DefinitionImportSummary, String> {
    let res = parse_ecu_xml(xml)?;
    commit_maps_if_requested(&res.definitions.maps, load_into_workspace, state)?;
    Ok(DefinitionImportSummary {
        format: "ecu_xml".to_string(),
        label: res.rom_id,
        table_count: res.definitions.maps.len(),
        constant_count: res.definitions.constants.len(),
        flag_count: res.definitions.flags.len(),
        maps: res.definitions.maps,
        warnings: res.warnings,
    })
}

#[tauri::command]
pub fn parse_ecu_xml_cmd(
    xml: String,
    load_into_workspace: Option<bool>,
    state: State<'_, AppState>,
) -> Result<DefinitionImportSummary, String> {
    parse_ecu_xml_inner(&xml, load_into_workspace.unwrap_or(false), &state)
}

#[tauri::command]
pub fn export_ecu_xml_cmd(
    rom_id: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let defs = Definitions {
        maps: super::lock_or_err(&state.maps, "maps")?.clone(),
        ..Default::default()
    };
    Ok(export_ecu_xml(&defs, &rom_id))
}

pub fn parse_ecu_ini_inner(
    ini: &str,
    active_defines: &[String],
    load_into_workspace: bool,
    state: &AppState,
) -> Result<EcuIniSummary, String> {
    let refs: Vec<&str> = active_defines.iter().map(String::as_str).collect();
    let parsed = parse_ecu_ini(ini, &refs);
    let defs = parsed.to_definitions();
    commit_maps_if_requested(&defs.maps, load_into_workspace, state)?;
    Ok(EcuIniSummary {
        signature: parsed.signature,
        query_command: parsed.query_command,
        endianness: parsed.endianness,
        page_sizes: parsed.page_sizes,
        blocking_factor: parsed.blocking_factor,
        table_count: defs.maps.len(),
        constant_count: defs.constants.len(),
        flag_count: defs.flags.len(),
        maps: defs.maps,
    })
}

#[tauri::command]
pub fn parse_ecu_ini_cmd(
    ini: String,
    active_defines: Option<Vec<String>>,
    load_into_workspace: Option<bool>,
    state: State<'_, AppState>,
) -> Result<EcuIniSummary, String> {
    parse_ecu_ini_inner(
        &ini,
        &active_defines.unwrap_or_default(),
        load_into_workspace.unwrap_or(false),
        &state,
    )
}

pub fn parse_kp_file_inner(
    data: &[u8],
    load_into_workspace: bool,
    state: &AppState,
) -> Result<DefinitionImportSummary, String> {
    match parse_kp_any(data)? {
        KpParsedAny::Binary(bin) => {
            let defs = bin.to_definitions();
            commit_maps_if_requested(&defs.maps, load_into_workspace, state)?;
            Ok(DefinitionImportSummary {
                format: "kp_binary".to_string(),
                label: if bin.project_name.is_empty() {
                    bin.filename
                } else {
                    bin.project_name
                },
                table_count: defs.maps.len(),
                constant_count: defs.constants.len(),
                flag_count: defs.flags.len(),
                maps: defs.maps,
                warnings: Vec::new(),
            })
        }
        KpParsedAny::PlaintextAr(pack) => {
            let maps: Vec<MapDef> = pack
                .maps
                .into_iter()
                .enumerate()
                .map(|(idx, m)| MapDef {
                    id: format!("kp-ar-{idx}"),
                    name: m.name,
                    category: "KP Map Pack".to_string(),
                    address: m.address as usize,
                    rows: 1,
                    cols: 1,
                    dtype: DType::U8,
                    endian: Endian::Little,
                    column_major: false,
                    major_stride_bits: 0,
                    minor_stride_bits: 0,
                    scaling: Scaling {
                        equation: "X".to_string(),
                        unit: m.unit,
                        decimals: 2,
                    },
                    x_axis: None,
                    y_axis: None,
                })
                .collect();
            commit_maps_if_requested(&maps, load_into_workspace, state)?;
            Ok(DefinitionImportSummary {
                format: "kp_text".to_string(),
                label: pack.declared_ecu.unwrap_or_else(|| "KP Map Pack".to_string()),
                table_count: maps.len(),
                constant_count: 0,
                flag_count: 0,
                maps,
                warnings: Vec::new(),
            })
        }
    }
}

#[tauri::command]
pub fn parse_kp_file_cmd(
    data: Vec<u8>,
    load_into_workspace: Option<bool>,
    state: State<'_, AppState>,
) -> Result<DefinitionImportSummary, String> {
    parse_kp_file_inner(&data, load_into_workspace.unwrap_or(false), &state)
}

#[tauri::command]
pub fn parse_ols_project_cmd(data: Vec<u8>) -> Result<OlsProjectSummary, String> {
    let proj = parse_ols(&data)?;
    Ok(OlsProjectSummary {
        format_version: proj.header.format_version,
        make: proj.metadata.make,
        model: proj.metadata.model,
        ecu_name: proj.metadata.ecu_name,
        hw_number: proj.metadata.hw_number,
        sw_number: proj.metadata.sw_number,
        versions: proj
            .versions
            .into_iter()
            .map(|v| OlsVersionRow {
                index: v.index,
                name: v.name,
                base_address: v.base_address,
                byte_len: v.rom_data.len(),
            })
            .collect(),
        maps: proj
            .maps
            .into_iter()
            .map(|m| OlsMapRow {
                name: m.name,
                kind: m.kind,
                unit: m.unit,
                address: m.address,
                rows: m.ny,
                cols: m.nx,
                data_size: m.data_size,
                scale: m.scale,
                offset: m.offset,
            })
            .collect(),
        warnings: proj.warnings,
    })
}

#[tauri::command]
pub fn parse_hex_or_srec_cmd(text: String) -> Result<HexDocumentSummary, String> {
    let doc = parse_hex_or_srec(&text)?;
    Ok(HexDocumentSummary {
        format: doc.format,
        metadata: doc.metadata,
        entry_point: doc.entry_point,
        segments: doc
            .segments
            .into_iter()
            .map(|s| HexSegmentRow {
                address: s.address,
                data: s.data,
            })
            .collect(),
    })
}

#[tauri::command]
pub fn export_intel_hex_cmd(
    base_address: u32,
    data: Option<Vec<u8>>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let bytes = match data {
        Some(b) => b,
        None => save_bin_data_inner(&state)?,
    };
    let seg = [HexSegment {
        address: base_address,
        data: bytes,
    }];
    Ok(export_intel_hex(&seg, None, 16))
}

#[tauri::command]
pub fn export_srec_cmd(
    base_address: u32,
    header: Option<String>,
    data: Option<Vec<u8>>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let bytes = match data {
        Some(b) => b,
        None => save_bin_data_inner(&state)?,
    };
    let seg = [HexSegment {
        address: base_address,
        data: bytes,
    }];
    Ok(export_motorola_s3(&seg, header.as_deref(), 16))
}

#[tauri::command]
pub fn gm_e92_rle_compress(data: Vec<u8>, wrap_frame: Option<bool>) -> Vec<u8> {
    let coded = gm_compress(&data);
    if wrap_frame.unwrap_or(false) {
        gm_wrap(&coded)
    } else {
        coded
    }
}

#[tauri::command]
pub fn gm_e92_rle_decompress(data: Vec<u8>) -> Result<Vec<u8>, String> {
    let payload = if gm_is_wrapped(&data) {
        gm_unwrap(&data)?
    } else {
        data.as_slice()
    };
    gm_decompress(payload)
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn pack_sgo_container_cmd(
    format_version: u32,
    part_number: String,
    sw_version: String,
    baud_rate: u32,
    diag_start: u32,
    diag_end: u32,
    sa2_hex: Option<String>,
    blocks: Vec<SgoBlockInput>,
) -> Result<Vec<u8>, String> {
    let sa2 = match sa2_hex {
        Some(ref h) if !h.trim().is_empty() => decode_hex_bytes(h)?,
        _ => Vec::new(),
    };
    let specs: Vec<SgoPackBlockSpec> = blocks
        .into_iter()
        .map(|b| SgoPackBlockSpec {
            block_address: b.block_address,
            crypt_mode: b.crypt_mode,
            erase_start: b.erase_start,
            erase_end: b.erase_end,
            prog_start: b.prog_start,
            prog_end: b.prog_end,
            compress_bcb: b.compress_bcb,
            raw_data: b.raw_data,
        })
        .collect();
    Ok(pack_sgo_container(
        format_version,
        &part_number,
        &sw_version,
        baud_rate,
        diag_start,
        diag_end,
        &sa2,
        &specs,
    ))
}

#[tauri::command]
pub fn unpack_frf_odx_cmd(frf_bytes: Vec<u8>, key_hex: String) -> Result<FrfUnpackPayload, String> {
    let key = decode_hex_bytes(&key_hex)?;
    let (entry_name, odx_bytes) = frf_unpack_odx(&frf_bytes, &key)?;
    let raw_len = odx_bytes.len();
    let odx_xml = String::from_utf8_lossy(&odx_bytes).into_owned();
    Ok(FrfUnpackPayload {
        entry_name,
        odx_xml,
        raw_len,
    })
}

#[tauri::command]
pub fn pack_frf_odx_cmd(
    odx_xml: String,
    entry_name: String,
    key_hex: String,
) -> Result<Vec<u8>, String> {
    let key = decode_hex_bytes(&key_hex)?;
    frf_pack_odx(odx_xml.as_bytes(), &entry_name, &key)
}

fn resolve_export_definitions(
    maps: Option<Vec<MapDef>>,
    state: &AppState,
) -> Result<Definitions, String> {
    let resolved_maps = match maps {
        Some(m) => m,
        None => super::lock_or_err(&state.maps, "maps")?.clone(),
    };
    Ok(Definitions {
        maps: resolved_maps,
        ..Default::default()
    })
}

#[tauri::command]
pub fn export_xdf_xml_cmd(
    title: Option<String>,
    maps: Option<Vec<MapDef>>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let defs = resolve_export_definitions(maps, &state)?;
    Ok(export_xdf_definitions(
        &defs,
        title.as_deref().unwrap_or("kiviTuner Calibration Export"),
    ))
}

#[tauri::command]
pub fn export_ecu_ini_cmd(
    signature: Option<String>,
    page_size: Option<usize>,
    maps: Option<Vec<MapDef>>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let defs = resolve_export_definitions(maps, &state)?;
    Ok(export_ecu_ini(
        &defs,
        signature.as_deref().unwrap_or("kiviTuner Export 1.0"),
        page_size,
    ))
}

#[tauri::command]
pub fn export_kp_ar_cmd(
    ecu_id: Option<String>,
    maps: Option<Vec<MapDef>>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let defs = resolve_export_definitions(maps, &state)?;
    Ok(export_kp_ar_text(&defs, ecu_id.as_deref()))
}
