use super::window_io::now_epoch_ms;
use super::AppState;
use kivi_checksum::plugins::honda_keihin::{compute_keihin_checksum_byte, verify_keihin_checksum};
use kivi_core::hash::BinaryHashes;
use kivi_core::hex_ingest::ingest_firmware;
use kivi_core::ingest::ingest_binary;
use kivi_core::undo_history::BytePatch;
use kivi_core::verify::{check_blank, verify_buffer};
use kivi_defs::expr::{evaluate_scaling, invert_scaling};
use kivi_defs::model::MapDef;
use kivi_defs::xdf::parse_xdf;
use serde::{Deserialize, Serialize};
use tauri::State;

pub(crate) fn load_bin_data_with_meta(
    data: &[u8],
    label: Option<&str>,
    file_path: Option<&str>,
    state: &AppState,
) -> Result<BinaryHashes, String> {
    let mem = ingest_binary(data).map_err(|e| e.to_string())?;
    let hashes = mem.hashes().clone();
    let mut docs_guard = super::lock_or_err(&state.documents, "documents")?;
    let mut mem_guard = super::lock_or_err(&state.memory, "memory")?;
    let mut orig_guard = super::lock_or_err(&state.original_memory, "original_memory")?;
    let mut undo_guard = super::lock_or_err(&state.undo, "undo")?;
    docs_guard.ensure_active_metadata(label, file_path);
    *orig_guard = Some(mem.clone());
    *mem_guard = Some(mem);
    undo_guard.clear();
    Ok(hashes)
}

pub fn load_bin_data_inner(data: &[u8], state: &AppState) -> Result<BinaryHashes, String> {
    load_bin_data_with_meta(data, None, None, state)
}

pub fn load_bin_file_inner(path: &str, state: &AppState) -> Result<BinaryHashes, String> {
    let data = super::read_bounded_file_bytes(path, super::MAX_FILE_READ_BYTES)?;
    let label = std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned());
    load_bin_data_with_meta(&data, label.as_deref(), Some(path), state)
}

/// Reads the base address of the loaded ROM, or `None` when unknown.
///
/// A raw `.bin` has no inherent absolute address, so this is `None` until the
/// user states one via [`set_rom_base_address_inner`].
pub fn get_rom_base_address_inner(state: &AppState) -> Result<Option<u32>, String> {
    let guard = super::lock_or_err(&state.memory, "memory")?;
    match guard.as_ref() {
        Some(mem) => Ok(mem.base_address()),
        None => Err("No binary loaded".to_string()),
    }
}

/// States the base address of the loaded ROM.
///
/// Passing `None` clears the address back to "unknown". Passing `Some(0)`
/// records a *known* zero address, which is deliberately distinct from unknown.
/// Only metadata changes; the ROM bytes and hashes are untouched.
pub fn set_rom_base_address_inner(base: Option<u32>, state: &AppState) -> Result<(), String> {
    let mut guard = super::lock_or_err(&state.memory, "memory")?;
    match guard.as_mut() {
        Some(mem) => {
            mem.set_base_address(base);
            Ok(())
        }
        None => Err("No binary loaded".to_string()),
    }
}

pub fn parse_xdf_content_inner(xml: &str, state: &AppState) -> Result<Vec<MapDef>, String> {
    let defs = parse_xdf(xml).map_err(|e| e.to_string())?;
    *super::lock_or_err(&state.maps, "maps")? = defs.clone();
    Ok(defs)
}

pub fn parse_xdf_file_inner(path: &str, state: &AppState) -> Result<Vec<MapDef>, String> {
    let content = super::read_bounded_file_string(path, super::MAX_FILE_READ_BYTES)?;
    parse_xdf_content_inner(&content, state)
}

/// Byte offset of cell `(row, col)` inside a table.
///
/// `major_stride_bits` / `minor_stride_bits` come from the XDF
/// (`mmedmajorstridebits` / `mmedminorstridebits`) and are `0` when the table is
/// packed. A non-zero value overrides the packed pitch, which matters because a
/// padded table's rows are **not** adjacent: reading it contiguously returns
/// cells from the wrong row. Negative strides walk backwards from the base
/// address.
///
/// Read and write both go through this function so a stride can never be
/// honoured on one path and ignored on the other.
pub(crate) fn cell_offset(
    map_def: &MapDef,
    row: usize,
    col: usize,
    bytes_per_elem: usize,
) -> i64 {
    let elem_bits = (bytes_per_elem as i64).saturating_mul(8);
    let major_len = if map_def.column_major {
        map_def.rows
    } else {
        map_def.cols
    };
    let row_bits: i64 = if map_def.major_stride_bits == 0 {
        (major_len as i64).saturating_mul(elem_bits)
    } else {
        map_def.major_stride_bits as i64
    };
    let col_bits: i64 = if map_def.minor_stride_bits == 0 {
        elem_bits
    } else {
        map_def.minor_stride_bits as i64
    };
    let base_bits = (map_def.address as i64).saturating_mul(8);
    let bit = base_bits
        .saturating_add((row as i64).saturating_mul(row_bits))
        .saturating_add((col as i64).saturating_mul(col_bits));
    bit.div_euclid(8)
}

pub(crate) fn extract_matrix_from_mem(
    map_def: &MapDef,
    mem: &kivi_core::memory_map::MemoryMap,
) -> Result<Vec<Vec<f64>>, String> {
    let bytes_per_elem = map_def.dtype.byte_size();
    let decimals = map_def.scaling.decimals as i32;
    let factor = 10f64.powi(decimals);

    let mut matrix = Vec::with_capacity(map_def.rows);
    for r in 0..map_def.rows {
        let mut row = Vec::with_capacity(map_def.cols);
        for c in 0..map_def.cols {
            // Strides are declared in memory order, so row/col are passed
            // as-is; `column_major` only reorders which logical cell maps to
            // which grid position.
            let (grid_row, grid_col) = if map_def.column_major {
                (c, r)
            } else {
                (r, c)
            };
            let offset = cell_offset(map_def, grid_row, grid_col, bytes_per_elem);
            if offset < 0 {
                return Err(format!(
                    "Map cell ({r},{c}) resolves to a negative offset; check the declared stride"
                ));
            }
            let raw = mem
                .read_value(offset as usize, map_def.dtype, map_def.endian)
                .map_err(|e| e.to_string())?;
            let scaled = evaluate_scaling(&map_def.scaling.equation, raw)
                .map_err(|e| e.to_string())?;

            let rounded = if scaled.is_finite() && decimals >= 0 {
                (scaled * factor).round() / factor
            } else {
                scaled
            };
            row.push(rounded);
        }
        matrix.push(row);
    }
    Ok(matrix)
}

pub fn extract_map_matrix_inner(
    map_def: &MapDef,
    state: &AppState,
) -> Result<Vec<Vec<f64>>, String> {
    let guard = super::lock_or_err(&state.memory, "memory")?;
    let mem = guard.as_ref().ok_or_else(|| "No binary loaded".to_string())?;
    extract_matrix_from_mem(map_def, mem)
}

pub fn extract_original_map_matrix_inner(
    map_def: &MapDef,
    state: &AppState,
) -> Result<Vec<Vec<f64>>, String> {
    let orig_guard = super::lock_or_err(&state.original_memory, "original_memory")?;
    if let Some(orig) = orig_guard.as_ref() {
        return extract_matrix_from_mem(map_def, orig);
    }
    drop(orig_guard);
    extract_map_matrix_inner(map_def, state)
}

fn plan_matrix_writes(
    map_def: &MapDef,
    matrix: &[Vec<f64>],
    rom_len: usize,
) -> Result<Vec<(usize, f64)>, String> {
    let bytes_per_elem = map_def.dtype.byte_size();
    let mut planned = Vec::with_capacity(map_def.rows.saturating_mul(map_def.cols));
    for (r, row) in matrix.iter().enumerate().take(map_def.rows) {
        for (c, &scaled) in row.iter().enumerate().take(map_def.cols) {
            let (grid_row, grid_col) = if map_def.column_major {
                (c, r)
            } else {
                (r, c)
            };
            let offset = cell_offset(map_def, grid_row, grid_col, bytes_per_elem);
            if offset < 0 {
                return Err(format!(
                    "Map cell ({r},{c}) resolves to a negative offset; check the declared stride"
                ));
            }
            let off = offset as usize;
            if off.saturating_add(bytes_per_elem) > rom_len {
                return Err(format!(
                    "Map cell ({r},{c}) at 0x{off:X} exceeds ROM length 0x{:X}",
                    rom_len
                ));
            }
            let raw = invert_scaling(&map_def.scaling.equation, scaled)
                .map_err(|e| e.to_string())?;
            planned.push((off, raw));
        }
    }
    Ok(planned)
}

pub fn update_map_matrix_inner(
    map_def: &MapDef,
    matrix: &[Vec<f64>],
    state: &AppState,
) -> Result<(), String> {
    let mut mem_guard = super::lock_or_err(&state.memory, "memory")?;
    let mut undo_guard = super::lock_or_err(&state.undo, "undo")?;
    let mem = mem_guard
        .as_mut()
        .ok_or_else(|| "No binary loaded".to_string())?;
    let planned = plan_matrix_writes(map_def, matrix, mem.len())?;

    let mut patches = Vec::new();
    for (off, raw) in planned {
        let res = mem.write_value_deferred_hash(off, raw, map_def.dtype, map_def.endian);
        let (written_off, old_bytes, new_bytes) = res.map_err(|e| e.to_string())?;
        if old_bytes != new_bytes {
            patches.push(BytePatch {
                offset: written_off,
                old_bytes,
                new_bytes,
            });
        }
    }
    if !patches.is_empty() {
        undo_guard.push(format!("Edit {} matrix", map_def.name), now_epoch_ms(), patches);
        mem.recompute_hashes();
    }
    Ok(())
}

pub fn verify_checksum_keihin_inner(state: &AppState) -> Result<bool, String> {
    let guard = super::lock_or_err(&state.memory, "memory")?;
    if let Some(ref mem) = *guard {
        Ok(verify_keihin_checksum(mem.as_slice()))
    } else {
        Err("No binary loaded".to_string())
    }
}

fn fix_keihin_locked(
    mem: &mut kivi_core::memory_map::MemoryMap,
    undo_guard: &mut kivi_core::undo_history::UndoHistory,
    label: &str,
) -> Result<(), String> {
    let target = compute_keihin_checksum_byte(mem.as_slice()).map_err(|e| e.to_string())?;
    let last_idx = mem.len() - 1;
    let (old_bytes, new_bytes) = mem
        .write_slice_deferred_hash(last_idx, &[target])
        .map_err(|e| e.to_string())?;
    if old_bytes != new_bytes {
        undo_guard.push(
            label.to_string(),
            now_epoch_ms(),
            vec![BytePatch {
                offset: last_idx,
                old_bytes,
                new_bytes,
            }],
        );
        mem.recompute_hashes();
    }
    Ok(())
}

pub fn fix_checksum_keihin_inner(state: &AppState) -> Result<bool, String> {
    let mut mem_guard = super::lock_or_err(&state.memory, "memory")?;
    let mut undo_guard = super::lock_or_err(&state.undo, "undo")?;
    let mem = mem_guard
        .as_mut()
        .ok_or_else(|| "No binary loaded".to_string())?;
    fix_keihin_locked(mem, &mut undo_guard, "Fix Keihin Checksum")?;
    Ok(true)
}

pub fn save_bin_data_inner(state: &AppState) -> Result<Vec<u8>, String> {
    let guard = super::lock_or_err(&state.memory, "memory")?;
    if let Some(ref mem) = *guard {
        Ok(mem.as_slice().to_vec())
    } else {
        Err("No binary loaded".to_string())
    }
}

pub fn save_and_fix_checksum_inner(state: &AppState) -> Result<Vec<u8>, String> {
    let mut mem_guard = super::lock_or_err(&state.memory, "memory")?;
    let mut undo_guard = super::lock_or_err(&state.undo, "undo")?;
    let mem = mem_guard
        .as_mut()
        .ok_or_else(|| "No binary loaded".to_string())?;
    fix_keihin_locked(mem, &mut undo_guard, "Fix Keihin Checksum")?;
    Ok(mem.as_slice().to_vec())
}

/// Outcome of comparing the active ROM buffer against a reference buffer.
///
/// This is a pure in-memory comparison; it never talks to hardware. The
/// `blank_check` field reports whether the *reference* buffer looks like erased
/// flash (`0xFF` fill), which is useful when validating a backup artifact.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RomVerificationReport {
    /// Whether the two buffers matched over the compared range with equal length.
    pub matches: bool,
    /// Number of recorded differences (capped by `max_differences`).
    pub difference_count: usize,
    /// Recorded differences, truncated per the cap.
    pub differences: Vec<VerifyDifferencePayload>,
    /// Blank-check verdict for the reference buffer.
    pub reference_is_blank: bool,
    /// Length of the active ROM buffer.
    pub active_len: usize,
    /// Length of the reference buffer.
    pub reference_len: usize,
}

/// Serializable mirror of [`kivi_core::verify::VerifyDifference`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VerifyDifferencePayload {
    pub offset: usize,
    pub expected_local_byte: u8,
    pub actual_chip_byte: u8,
}

/// Compares the active ROM buffer against `reference_data`.
///
/// `reference_data` is typically a previously saved backup. `max_differences`
/// caps how many differences are reported back to the caller.
pub fn verify_rom_against_reference_inner(
    reference_data: &[u8],
    max_differences: usize,
    state: &AppState,
) -> Result<RomVerificationReport, String> {
    let guard = super::lock_or_err(&state.memory, "memory")?;
    let mem = guard
        .as_ref()
        .ok_or_else(|| "No binary loaded".to_string())?;

    let active = mem.as_slice();
    let comparison = verify_buffer(active, reference_data, max_differences);

    let (matches, differences) = match comparison {
        Ok(()) => (true, Vec::new()),
        Err(diffs) => (
            false,
            diffs
                .into_iter()
                .map(|d| VerifyDifferencePayload {
                    offset: d.offset,
                    expected_local_byte: d.expected_local_byte,
                    actual_chip_byte: d.actual_chip_byte,
                })
                .collect::<Vec<_>>(),
        ),
    };

    let reference_is_blank = matches!(
        check_blank(reference_data),
        kivi_core::verify::BlankCheckResult::Blank
    );

    Ok(RomVerificationReport {
        matches,
        difference_count: differences.len(),
        differences,
        reference_is_blank,
        active_len: active.len(),
        reference_len: reference_data.len(),
    })
}


// Tauri commands exposed over IPC:

#[tauri::command]
pub fn load_bin_data(data: Vec<u8>, state: State<'_, AppState>) -> Result<BinaryHashes, String> {
    load_bin_data_inner(&data, &state)
}

#[tauri::command]
pub fn load_bin_file(path: String, state: State<'_, AppState>) -> Result<BinaryHashes, String> {
    load_bin_file_inner(&path, &state)
}

#[tauri::command]
pub fn parse_xdf_content(xml: String, state: State<'_, AppState>) -> Result<Vec<MapDef>, String> {
    parse_xdf_content_inner(&xml, &state)
}

#[tauri::command]
pub fn parse_xdf_file(path: String, state: State<'_, AppState>) -> Result<Vec<MapDef>, String> {
    parse_xdf_file_inner(&path, &state)
}

#[tauri::command]
pub fn extract_map_matrix(
    map_def: MapDef,
    state: State<'_, AppState>,
) -> Result<Vec<Vec<f64>>, String> {
    extract_map_matrix_inner(&map_def, &state)
}

#[tauri::command]
pub fn extract_original_map_matrix(
    map_def: MapDef,
    state: State<'_, AppState>,
) -> Result<Vec<Vec<f64>>, String> {
    extract_original_map_matrix_inner(&map_def, &state)
}

#[tauri::command]
pub fn update_map_matrix(
    map_def: MapDef,
    matrix: Vec<Vec<f64>>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    update_map_matrix_inner(&map_def, &matrix, &state)
}

#[tauri::command]
pub fn verify_checksum_keihin(state: State<'_, AppState>) -> Result<bool, String> {
    verify_checksum_keihin_inner(&state)
}

#[tauri::command]
pub fn fix_checksum_keihin(state: State<'_, AppState>) -> Result<bool, String> {
    fix_checksum_keihin_inner(&state)
}

#[tauri::command]
pub fn save_bin_data(state: State<'_, AppState>) -> Result<Vec<u8>, String> {
    save_bin_data_inner(&state)
}

/// Returns the loaded ROM's base address, or `null` when unknown.
#[tauri::command]
pub fn get_rom_base_address(state: State<'_, AppState>) -> Result<Option<u32>, String> {
    get_rom_base_address_inner(&state)
}

/// States the loaded ROM's base address. `null` clears it back to unknown.
#[tauri::command]
pub fn set_rom_base_address(base: Option<u32>, state: State<'_, AppState>) -> Result<(), String> {
    set_rom_base_address_inner(base, &state)
}

#[tauri::command]
pub fn save_and_fix_checksum(state: State<'_, AppState>) -> Result<Vec<u8>, String> {
    save_and_fix_checksum_inner(&state)
}

/// Compares the active ROM buffer against a reference (e.g. a saved backup).
///
/// Pure in-memory comparison; performs no hardware access.
#[tauri::command]
pub fn verify_rom_against_reference(
    reference_data: Vec<u8>,
    max_differences: usize,
    state: State<'_, AppState>,
) -> Result<RomVerificationReport, String> {
    verify_rom_against_reference_inner(&reference_data, max_differences, &state)
}

#[tauri::command]
pub fn compare_rom_binaries(
    base_data: Vec<u8>,
    target_data: Vec<u8>,
    max_gap: usize,
) -> Result<Vec<kivi_diff::model::DiffRegion>, String> {
    Ok(kivi_diff::engine::compare_binaries(&base_data, &target_data, max_gap))
}


#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FirmwareIngestReport {
    /// Detected container format: `IntelHex`, `SRecord`, or `RawBinary`.
    pub format: String,
    /// Lowest record address, or `null` when the input carried no data records
    /// or is a raw binary (which has no inherent address).
    pub base_address: Option<u32>,
    /// Contiguous reconstruction; gaps filled with 0xFF.
    pub bytes: Vec<u8>,
    /// Number of parsed data records.
    pub record_count: usize,
    /// Entry point when a valid start-address record supplied one.
    pub entry_point: Option<u32>,
}

/// Detects and parses a firmware container (Intel HEX, S-Record, or raw BIN).
///
/// **Read-only.** Nothing is written to the ROM, map definitions, or any
/// calibration state; the caller decides what to do with the bytes.
///
/// Raw binary is returned unchanged. Text beginning with `:` or `S` is always
/// parsed as HEX/SREC and surfaces a parse error rather than being compared as
/// ASCII.
pub fn ingest_firmware_inner(data: &[u8]) -> Result<FirmwareIngestReport, String> {
    let r = ingest_firmware(data, 0xFF).map_err(|e| e.to_string())?;
    Ok(FirmwareIngestReport {
        format: format!("{:?}", r.format),
        base_address: r.base_address,
        bytes: r.bytes,
        record_count: r.record_count,
        entry_point: r.entry_point,
    })
}

/// Detects and parses a firmware container.
///
/// Read-only; performs no hardware access.
#[tauri::command]
pub fn ingest_firmware_data(data: Vec<u8>) -> Result<FirmwareIngestReport, String> {
    ingest_firmware_inner(&data)
}

// ---------------------------------------------------------------------------
// Wave 4: VBF container integration
// ---------------------------------------------------------------------------

/// One VBF segment as supplied by, or returned to, the frontend.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VbfSegmentPayload {
    /// Load address of the segment.
    pub start_address: u32,
    /// Segment bytes.
    pub data: Vec<u8>,
}

/// Serializes segments into a VBF container.
///
/// **Limitation, surfaced to the caller:** this produces the simplified dialect
/// the ported codec supports (ASCII header + length-prefixed segments). It is
/// **not** validated against real OEM VBF files and makes no compatibility claim
/// beyond round-tripping through [`parse_vbf_inner`].
pub fn serialize_vbf_inner(
    sw_part_number: &str,
    segments: &[VbfSegmentPayload],
) -> Result<Vec<u8>, String> {
    let core: Vec<kivi_formats::vbf::FirmwareMemorySegment> = segments
        .iter()
        .map(|s| kivi_formats::vbf::FirmwareMemorySegment {
            start_address: s.start_address,
            data: s.data.clone(),
            crc16_ccitt: 0, // recomputed by the serializer
        })
        .collect();
    Ok(kivi_formats::vbf::serialize_to_vbf(sw_part_number, &core))
}

/// Parses a VBF container into its header and segments.
///
/// Every declared length is validated before slicing, so truncated or hostile
/// input yields an error rather than a panic.
pub fn parse_vbf_inner(vbf_bytes: &[u8]) -> Result<(String, Vec<VbfSegmentPayload>), String> {
    let (header, segments) =
        kivi_formats::vbf::parse_vbf_segments(vbf_bytes).map_err(|e| e.to_string())?;
    Ok((
        header,
        segments
            .into_iter()
            .map(|s| VbfSegmentPayload {
                start_address: s.start_address,
                data: s.data,
            })
            .collect(),
    ))
}

#[tauri::command]
pub fn parse_vbf(vbf_bytes: Vec<u8>) -> Result<VbfParseReport, String> {
    parse_vbf_inner(&vbf_bytes).map(|(header, segments)| VbfParseReport { header, segments })
}

#[tauri::command]
pub fn serialize_vbf(
    sw_part_number: String,
    segments: Vec<VbfSegmentPayload>,
) -> Result<Vec<u8>, String> {
    serialize_vbf_inner(&sw_part_number, &segments)
}

/// Serializable VBF parse result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VbfParseReport {
    /// Raw ASCII header text.
    pub header: String,
    /// Segments in file order.
    pub segments: Vec<VbfSegmentPayload>,
}

// ---------------------------------------------------------------------------
// Wave 4: address translation integration
// ---------------------------------------------------------------------------

/// Result of an address translation, including which rule matched.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddressTranslationReport {
    /// Translated value.
    pub value: u32,
    /// True when no rule matched and the input was returned unchanged.
    pub is_identity_fallback: bool,
    /// Name of the rule that matched, or `null`.
    ///
    /// This is what makes ambiguity visible: the RH850 raw range overlaps the
    /// cached window, so a raw offset there resolves through the *cached* rule.
    /// A caller that sees only `value` would never notice.
    pub matched_rule: Option<String>,
}

/// Translates an address in the requested direction.
///
/// `direction` is `"cpu_to_raw"` or `"raw_to_cpu"`.
///
/// **Not a write-address validator.** This maps between the CPU view and the raw
/// file view for analysis only; it confers no permission to write to a device.
pub fn translate_address_inner(
    direction: &str,
    address: u32,
) -> Result<AddressTranslationReport, String> {
    let translator = kivi_core::address_translation::MemoryAddressTranslator::default();

    let (t, matched) = match direction {
        "cpu_to_raw" => {
            let t = kivi_core::address_translation::cpu_to_raw(&translator, address)?;
            let rule = translator
                .segments
                .iter()
                .find(|s| {
                    address >= s.cpu_start_address && address <= s.cpu_end_address
                })
                .map(|s| s.name.clone());
            (t, rule)
        }
        "raw_to_cpu" => {
            let t = kivi_core::address_translation::raw_to_cpu(&translator, address)?;
            let rule = translator
                .segments
                .iter()
                .find(|s| {
                    let size = s.cpu_end_address - s.cpu_start_address;
                    address >= s.raw_file_offset && address <= s.raw_file_offset + size
                })
                .map(|s| s.name.clone());
            (t, rule)
        }
        other => {
            return Err(format!(
                "Unknown translation direction '{}' (expected 'cpu_to_raw' or 'raw_to_cpu')",
                other
            ))
        }
    };

    Ok(AddressTranslationReport {
        value: t.value,
        is_identity_fallback: t.is_identity_fallback,
        matched_rule: matched,
    })
}

#[tauri::command]
pub fn translate_address(
    direction: String,
    address: u32,
) -> Result<AddressTranslationReport, String> {
    translate_address_inner(&direction, address)
}

