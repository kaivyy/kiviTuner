use super::AppState;
use kivi_checksum::primitives::{compute_crc32, crc16_ccitt, crc32_castagnoli, crc32_simos};
use kivi_core::flash_target::{describe_flash_target as core_describe_flash_target, is_block_write_safe, FlashBlockType};
use kivi_core::sector::{plan_differential_flash as core_plan_differential_flash, total_bytes_to_flash};
use kivi_defs::interp::{lookup_2d_map, search_interval};
use kivi_defs::smoothing::{laplacian_curvature_2d, spatial_smooth_2d};
use serde::{Deserialize, Serialize};
use tauri::State;

/// Serializable per-sector plan entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SectorPlanEntry {
    pub sector_index: usize,
    pub start_address: u32,
    pub length: usize,
    pub is_modified: bool,
    pub modified_bytes_count: usize,
}

/// Serializable differential planning report.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DifferentialPlanReport {
    pub sectors: Vec<SectorPlanEntry>,
    /// Sum of `length` across modified sectors.
    pub total_bytes_to_flash: usize,
    /// Length of the active ROM buffer.
    pub rom_len: usize,
    /// Number of modified sectors.
    pub modified_sector_count: usize,
}

/// Plans which sectors differ between the active ROM and `tuned_data`.
///
/// Read-only analysis: the active ROM buffer is never mutated. No hardware
/// access occurs and no erase/write command is produced.
pub fn plan_differential_flash_inner(
    tuned_data: &[u8],
    sector_sizes: &[usize],
    state: &AppState,
) -> Result<DifferentialPlanReport, String> {
    let guard = super::lock_or_err(&state.memory, "memory")?;
    let mem = guard
        .as_ref()
        .ok_or_else(|| "No binary loaded".to_string())?;

    let stock = mem.as_slice();
    let plan = core_plan_differential_flash(stock, tuned_data, sector_sizes)
        .map_err(|e| e.to_string())?;

    let total_bytes_to_flash = total_bytes_to_flash(&plan);
    let modified_sector_count = plan.iter().filter(|s| s.is_modified).count();

    let sectors = plan
        .into_iter()
        .map(|s| SectorPlanEntry {
            sector_index: s.sector_index,
            start_address: s.start_address,
            length: s.length,
            is_modified: s.is_modified,
            modified_bytes_count: s.modified_bytes_count,
        })
        .collect();

    Ok(DifferentialPlanReport {
        sectors,
        total_bytes_to_flash,
        rom_len: stock.len(),
        modified_sector_count,
    })
}

/// Serializable flash block descriptor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlashBlockEntry {
    pub start_address: u32,
    pub length: u32,
    /// Debug name of the block type (`Boot`, `Parameter`, `Calibration`,
    /// `OperatingSystem`).
    pub block_type: String,
    pub write_safe: bool,
}

/// Serializable flash target description.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlashTargetReport {
    pub jedec_id: u32,
    pub model_name: String,
    pub total_size_bytes: u32,
    pub blocks: Vec<FlashBlockEntry>,
    pub boot_block_count: usize,
    pub write_safe_block_count: usize,
}

/// Describes a NOR flash target from its JEDEC id.
///
/// Read-only metadata lookup; performs no hardware access.
pub fn describe_flash_target_inner(chip_id: u32) -> Result<FlashTargetReport, String> {
    let info = core_describe_flash_target(chip_id).map_err(|e| e.to_string())?;

    let boot_block_count = info
        .blocks
        .iter()
        .filter(|b| b.block_type == FlashBlockType::Boot)
        .count();
    let write_safe_block_count = info.blocks.iter().filter(|b| is_block_write_safe(b)).count();

    let blocks = info
        .blocks
        .into_iter()
        .map(|b| FlashBlockEntry {
            start_address: b.start_address,
            length: b.length,
            block_type: format!("{:?}", b.block_type),
            write_safe: is_block_write_safe(&b),
        })
        .collect();

    Ok(FlashTargetReport {
        jedec_id: info.jedec_id,
        model_name: info.model_name.to_string(),
        total_size_bytes: info.total_size_bytes,
        blocks,
        boot_block_count,
        write_safe_block_count,
    })
}

/// Plans which sectors differ between the active ROM and `tuned_data`.
///
/// Read-only analysis; produces no erase or write command.
#[tauri::command]
pub fn plan_differential_flash(
    tuned_data: Vec<u8>,
    sector_sizes: Vec<usize>,
    state: State<'_, AppState>,
) -> Result<DifferentialPlanReport, String> {
    plan_differential_flash_inner(&tuned_data, &sector_sizes, &state)
}

/// Describes a NOR flash target from its JEDEC id.
#[tauri::command]
pub fn describe_flash_target(
    chip_id: u32,
) -> Result<FlashTargetReport, String> {
    describe_flash_target_inner(chip_id)
}

/// Result of a pure checksum computation over caller-supplied data.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChecksumResult {
    /// Algorithm identifier echoed back from the request.
    pub algorithm: String,
    /// Width of the checksum in bits (16 or 32).
    pub width_bits: u8,
    /// Computed value, zero-extended into `u64`.
    pub value: u64,
    /// Number of input bytes hashed.
    pub input_len: usize,
}

/// Computes a checksum over explicitly supplied data.
///
/// Supported algorithms:
/// * `crc32_ieee` — IEEE 802.3 (reflected, poly `0xEDB88320`)
/// * `crc32_castagnoli` — CRC-32C (reflected, poly `0x82F63B78`)
/// * `crc32_simos` — non-reflected CRC-32 (poly `0x04C11DB7`, init/xor 0)
/// * `crc16_ccitt` — CCITT-FALSE (poly `0x1021`, init `0xFFFF`)
///
/// `crc16_modbus` is deliberately **not** supported: the source engine declares
/// the enum variant but never implements it, so there is no reference behavior.
///
/// Pure calculation: does not read or mutate the active ROM and performs no
/// hardware access.
pub fn compute_checksum_inner(
    algorithm: &str,
    data: &[u8],
) -> Result<ChecksumResult, String> {
    let (width_bits, value) = match algorithm {
        "crc32_ieee" => (32u8, compute_crc32(data) as u64),
        "crc32_castagnoli" => (32, crc32_castagnoli(data) as u64),
        "crc32_simos" => (32, crc32_simos(data) as u64),
        "crc16_ccitt" => (16, crc16_ccitt(data) as u64),
        other => {
            return Err(format!(
                "Unsupported checksum algorithm: {} (supported: crc32_ieee, \
                 crc32_castagnoli, crc32_simos, crc16_ccitt)",
                other
            ))
        }
    };

    Ok(ChecksumResult {
        algorithm: algorithm.to_string(),
        width_bits,
        value,
        input_len: data.len(),
    })
}

/// Computes a checksum over the active workspace ROM without copying bytes over IPC.
#[tauri::command]
pub fn compute_workspace_checksum(
    algorithm: String,
    state: State<'_, AppState>,
) -> Result<ChecksumResult, String> {
    let guard = super::lock_or_err(&state.memory, "memory")?;
    let mem = guard.as_ref().ok_or_else(|| "No binary loaded".to_string())?;
    compute_checksum_inner(&algorithm, mem.as_slice())
}

/// Computes a checksum over caller-supplied bytes.
///
/// Pure calculation; no ROM or hardware access.
#[tauri::command]
pub fn compute_checksum(
    algorithm: String,
    data: Vec<u8>,
) -> Result<ChecksumResult, String> {
    compute_checksum_inner(&algorithm, &data)
}

/// Result of a 2D map interpolation at an arbitrary operating point.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MapInterpolationResult {
    /// Interpolated value in the map's engineering units.
    pub value: f64,
    /// Lower X-axis index used for interpolation.
    pub x_low: usize,
    /// Upper X-axis index used for interpolation.
    pub x_high: usize,
    /// Normalized X position within the interval, clamped to `[0,1]`.
    pub x_t: f64,
    /// Lower Y-axis index used for interpolation.
    pub y_low: usize,
    /// Upper Y-axis index used for interpolation.
    pub y_high: usize,
    /// Normalized Y position within the interval, clamped to `[0,1]`.
    pub y_t: f64,
    /// `true` when the requested X was outside the axis and got clamped.
    pub x_clamped: bool,
    /// `true` when the requested Y was outside the axis and got clamped.
    pub y_clamped: bool,
}

/// Interpolates a loaded 2D map at `(x, y)`.
///
/// Axis breakpoints come from the map's `x_axis`/`y_axis` label values. When an
/// axis is missing, cell indices are used as coordinates.
///
/// **Read-only analysis.** The ROM buffer and map definitions are not modified;
/// no interpolation result is written back to any table.
pub fn interpolate_map_at_inner(
    map_id: &str,
    x: f64,
    y: f64,
    state: &AppState,
) -> Result<MapInterpolationResult, String> {
    let (map_def, matrix) = {
        let mem_guard = super::lock_or_err(&state.memory, "memory")?;
        let mem = mem_guard.as_ref().ok_or_else(|| "No binary loaded".to_string())?;
        let maps = super::lock_or_err(&state.maps, "maps")?;
        let map_def = maps
            .iter()
            .find(|m| m.id == map_id)
            .cloned()
            .ok_or_else(|| format!("Map not found: {}", map_id))?;
        let matrix = super::rom::extract_matrix_from_mem(&map_def, mem)?;
        (map_def, matrix)
    };

    if map_def.rows < 2 || map_def.cols < 2 {
        return Err(format!(
            "Map {} must be at least 2x2 to interpolate (got {}x{})",
            map_id, map_def.rows, map_def.cols
        ));
    }

    let x_axis = axis_breakpoints(&map_def.x_axis, map_def.cols);
    let y_axis = axis_breakpoints(&map_def.y_axis, map_def.rows);

    let xi = search_interval(&x_axis, x).map_err(|e| e.to_string())?;
    let yi = search_interval(&y_axis, y).map_err(|e| e.to_string())?;

    let value = lookup_2d_map(&x_axis, &y_axis, &matrix, x, y).map_err(|e| e.to_string())?;

    Ok(MapInterpolationResult {
        value,
        x_low: xi.low,
        x_high: xi.high,
        x_t: xi.t,
        y_low: yi.low,
        y_high: yi.high,
        y_t: yi.t,
        x_clamped: x < x_axis[0] || x > x_axis[x_axis.len() - 1],
        y_clamped: y < y_axis[0] || y > y_axis[y_axis.len() - 1],
    })
}

/// Builds a breakpoint vector for an axis, falling back to cell indices when
/// the definition provides no usable label values.
fn axis_breakpoints(axis: &Option<kivi_defs::model::AxisDef>, len: usize) -> Vec<f64> {
    match axis {
        Some(a) if a.values.len() == len && len >= 2 => a.values.clone(),
        _ => (0..len).map(|i| i as f64).collect(),
    }
}

/// Interpolates a loaded 2D map at an arbitrary operating point.
///
/// Read-only analysis; never writes the result back to any table.
#[tauri::command]
pub fn interpolate_map_at(
    map_id: String,
    x: f64,
    y: f64,
    state: State<'_, AppState>,
) -> Result<MapInterpolationResult, String> {
    interpolate_map_at_inner(&map_id, x, y, &state)
}

/// Read-only smoothing analysis result for a 2D map.
///
/// Carries the **original** matrix, a **preview** of the smoothed matrix, and
/// the Laplacian curvature map. Nothing here is written back to the ROM or to
/// the map definition; the caller decides what, if anything, to do with it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MapSmoothingReport {
    /// Matrix as read from the ROM, unmodified.
    pub original: Vec<Vec<f64>>,
    /// Preview of the spatially smoothed matrix (not persisted anywhere).
    pub smoothed: Vec<Vec<f64>>,
    /// Discrete Laplacian curvature map; borders are zero.
    pub laplacian: Vec<Vec<f64>>,
    /// Largest absolute curvature found.
    pub max_abs_curvature: f64,
    /// Row of the largest absolute curvature.
    pub max_curvature_row: usize,
    /// Column of the largest absolute curvature.
    pub max_curvature_col: usize,
    pub rows: usize,
    pub cols: usize,
    /// Alpha used for the preview.
    pub alpha: f64,
}

/// Computes a read-only smoothing preview and curvature analysis for a map.
///
/// **Analysis only.** The ROM buffer and map definitions are not modified, and
/// no smoothing result is written back to any table. There is deliberately no
/// "apply" counterpart: smoothing output is a preview for human review.
///
/// When `matrix` is supplied it is analyzed directly, so the preview always
/// reflects exactly what the editor is displaying — including edits that have
/// not yet been written back to `AppState`. When it is `None` the matrix is read
/// from the loaded ROM, preserving the original behaviour.
pub fn analyze_map_smoothing_inner(
    map_id: &str,
    alpha: f64,
    matrix: Option<&[Vec<f64>]>,
    state: &AppState,
) -> Result<MapSmoothingReport, String> {
    if !alpha.is_finite() {
        return Err("Smoothing alpha must be finite".to_string());
    }

    let original = match matrix {
        Some(m) => {
            {
                let guard = super::lock_or_err(&state.memory, "memory")?;
                if guard.is_none() {
                    return Err("No binary loaded".to_string());
                }
            }
            if m.is_empty() {
                return Err("Map matrix is empty".to_string());
            }
            let expected_cols = m[0].len();
            if expected_cols == 0 {
                return Err("Map matrix has empty rows".to_string());
            }
            if m.iter().any(|row| row.len() != expected_cols) {
                return Err("Map matrix rows must have uniform length".to_string());
            }
            // Validate the caller-supplied matrix rather than trusting it.
            if m.iter().any(|row| row.iter().any(|v| !v.is_finite())) {
                return Err("Map matrix contains non-finite values".to_string());
            }
            m.to_vec()
        }
        None => {
            let mem_guard = super::lock_or_err(&state.memory, "memory")?;
            let mem = mem_guard.as_ref().ok_or_else(|| "No binary loaded".to_string())?;
            let maps = super::lock_or_err(&state.maps, "maps")?;
            let map_def = maps
                .iter()
                .find(|m| m.id == map_id)
                .cloned()
                .ok_or_else(|| format!("Map not found: {}", map_id))?;
            super::rom::extract_matrix_from_mem(&map_def, mem)?
        }
    };

    let smoothed = spatial_smooth_2d(&original, alpha).map_err(|e| e.to_string())?;
    let laplacian = laplacian_curvature_2d(&original);

    let mut max_abs = 0.0f64;
    let mut max_row = 0usize;
    let mut max_col = 0usize;
    for (r, row) in laplacian.iter().enumerate() {
        for (c, v) in row.iter().enumerate() {
            let a = v.abs();
            if a > max_abs {
                max_abs = a;
                max_row = r;
                max_col = c;
            }
        }
    }

    Ok(MapSmoothingReport {
        rows: original.len(),
        cols: original.first().map_or(0, |r| r.len()),
        original,
        smoothed,
        laplacian,
        max_abs_curvature: max_abs,
        max_curvature_row: max_row,
        max_curvature_col: max_col,
        alpha,
    })
}

/// Computes a read-only smoothing preview and curvature analysis.
///
/// Read-only; never writes the result back to any calibration table.
#[tauri::command]
pub fn analyze_map_smoothing(
    map_id: String,
    alpha: f64,
    matrix: Option<Vec<Vec<f64>>>,
    state: State<'_, AppState>,
) -> Result<MapSmoothingReport, String> {
    analyze_map_smoothing_inner(&map_id, alpha, matrix.as_deref(), &state)
}

/// Serializable result of an address-gated binary comparison.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GatedDiffReport {
    /// `UnknownBases` or `MatchingBases(0x...)`.
    pub compatibility: String,
    /// Difference regions.
    pub regions: Vec<kivi_diff::model::DiffRegion>,
}

/// Compares two buffers after checking that their address ranges are compatible.
///
/// `base_addr` / `target_addr` are the lowest physical addresses reported by
/// HEX/SREC ingestion, or `None` for raw BIN (unknown). Mixed known/unknown and
/// conflicting known bases are rejected rather than producing a misleading diff.
///
/// **Read-only.** No ROM, map definition, or calibration state is touched.
pub fn compare_rom_binaries_with_address_inner(
    base_data: &[u8],
    base_addr: Option<u32>,
    target_data: &[u8],
    target_addr: Option<u32>,
    max_gap: usize,
) -> Result<GatedDiffReport, String> {
    let gated = kivi_diff::address::diff_with_address_check(
        base_data,
        base_addr,
        target_data,
        target_addr,
        max_gap,
    )
    .map_err(|e| e.to_string())?;

    let compatibility = match gated.compatibility {
        kivi_diff::address::AddressCompatibility::UnknownBases => "UnknownBases".to_string(),
        kivi_diff::address::AddressCompatibility::MatchingBases(a) => {
            format!("MatchingBases(0x{:X})", a)
        }
    };

    Ok(GatedDiffReport {
        compatibility,
        regions: gated.regions,
    })
}

/// Compares two buffers with address compatibility checking.
///
/// Read-only; performs no hardware access.
#[tauri::command]
pub fn compare_rom_binaries_with_address(
    base_data: Vec<u8>,
    base_addr: Option<u32>,
    target_data: Vec<u8>,
    target_addr: Option<u32>,
    max_gap: usize,
) -> Result<GatedDiffReport, String> {
    compare_rom_binaries_with_address_inner(
        &base_data,
        base_addr,
        &target_data,
        target_addr,
        max_gap,
    )
}

fn parse_optional_u32_header(headers: &tauri::http::HeaderMap, name: &str) -> Result<Option<u32>, String> {
    let Some(val) = headers.get(name) else {
        return Ok(None);
    };
    let s = val.to_str().map_err(|e| format!("Invalid header {name}: {e}"))?.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("null") || s.eq_ignore_ascii_case("none") {
        return Ok(None);
    }
    let parsed = if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16)
    } else {
        s.parse::<u32>()
    }
    .map_err(|e| format!("Invalid u32 header {name}={s}: {e}"))?;
    Ok(Some(parsed))
}

/// Compares the loaded workspace ROM (without copying it) against a raw binary uploaded in `Request`.
#[tauri::command]
pub fn compare_loaded_rom_raw(
    request: tauri::ipc::Request<'_>,
    state: State<'_, AppState>,
) -> Result<GatedDiffReport, String> {
    let headers = request.headers();
    let base_addr_override = parse_optional_u32_header(headers, "x-base-addr")?;
    let target_addr = parse_optional_u32_header(headers, "x-target-addr")?;
    let max_gap = headers
        .get("x-max-gap")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<usize>().ok())
        .unwrap_or(16);
    let target_data = match request.body() {
        tauri::ipc::InvokeBody::Raw(bytes) => bytes.as_slice(),
        tauri::ipc::InvokeBody::Json(_) => {
            return Err("compare_loaded_rom_raw expects raw binary body".to_string())
        }
    };
    let guard = super::lock_or_err(&state.memory, "memory")?;
    let mem = guard.as_ref().ok_or_else(|| "No binary loaded".to_string())?;
    let effective_base = base_addr_override.or_else(|| mem.base_address());
    compare_rom_binaries_with_address_inner(
        mem.as_slice(),
        effective_base,
        target_data,
        target_addr,
        max_gap,
    )
}

/// Plans differential flash sectors between the loaded workspace ROM and a raw tuned binary in `Request`.
#[tauri::command]
pub fn plan_differential_flash_raw(
    request: tauri::ipc::Request<'_>,
    state: State<'_, AppState>,
) -> Result<DifferentialPlanReport, String> {
    let sector_sizes: Vec<usize> = request
        .headers()
        .get("x-sector-sizes")
        .and_then(|v| v.to_str().ok())
        .map(|raw| {
            raw.split(',')
                .filter_map(|p| p.trim().parse::<usize>().ok())
                .collect()
        })
        .unwrap_or_default();
    let tuned_data = match request.body() {
        tauri::ipc::InvokeBody::Raw(bytes) => bytes.as_slice(),
        tauri::ipc::InvokeBody::Json(_) => {
            return Err("plan_differential_flash_raw expects raw binary body".to_string())
        }
    };
    plan_differential_flash_inner(tuned_data, &sector_sizes, &state)
}

#[tauri::command]
pub fn convert_diff_to_map(
    region: kivi_diff::model::DiffRegion,
) -> Result<kivi_defs::model::MapDef, String> {
    Ok(kivi_diff::convert::diff_region_to_map_def(&region))
}


pub fn apply_map_selection_edit_inner(
    matrix: &[Vec<f64>],
    selection: kivi_defs::map_ops::CellSelection,
    op: kivi_defs::map_ops::SelectionEditOp,
) -> Result<Vec<Vec<f64>>, String> {
    kivi_defs::map_ops::apply_selection_edit(matrix, selection, op).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn apply_map_selection_edit(
    matrix: Vec<Vec<f64>>,
    selection: kivi_defs::map_ops::CellSelection,
    op: kivi_defs::map_ops::SelectionEditOp,
) -> Result<Vec<Vec<f64>>, String> {
    apply_map_selection_edit_inner(&matrix, selection, op)
}

pub fn interpolate_map_region_inner(
    matrix: &[Vec<f64>],
    selection: kivi_defs::map_ops::CellSelection,
    options: kivi_defs::map_ops::RegionInterpOptions,
) -> Result<kivi_defs::map_ops::RegionInterpResult, String> {
    kivi_defs::map_ops::interpolate_matrix_region(matrix, selection, options)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn interpolate_map_region(
    matrix: Vec<Vec<f64>>,
    selection: kivi_defs::map_ops::CellSelection,
    options: kivi_defs::map_ops::RegionInterpOptions,
) -> Result<kivi_defs::map_ops::RegionInterpResult, String> {
    interpolate_map_region_inner(&matrix, selection, options)
}

pub fn solve_factor_offset_inner(
    p1: (f64, f64),
    p2: (f64, f64),
    p3: Option<(f64, f64)>,
    swap_input_output: bool,
) -> Result<kivi_defs::map_ops::FactorOffsetSolution, String> {
    match p3 {
        Some(pt3) => {
            kivi_defs::map_ops::solve_reciprocal_conversion(p1, p2, pt3, swap_input_output)
        }
        None => kivi_defs::map_ops::solve_linear_factor_offset(p1, p2, swap_input_output),
    }
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn solve_factor_offset(
    p1: (f64, f64),
    p2: (f64, f64),
    p3: Option<(f64, f64)>,
    swap_input_output: bool,
) -> Result<kivi_defs::map_ops::FactorOffsetSolution, String> {
    solve_factor_offset_inner(p1, p2, p3, swap_input_output)
}

