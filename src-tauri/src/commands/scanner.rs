use super::window_io::ScanProgressEvent;
use super::AppState;
use tauri::State;

#[tauri::command]
pub fn scan_rom_heuristics(
    rom_data: Vec<u8>,
    min_confidence: u8,
) -> Result<Vec<kivi_scanner::model::DetectedMapCandidate>, String> {
    Ok(kivi_scanner::scan_rom_with_options(&rom_data, min_confidence, None))
}

#[tauri::command]
pub fn scan_rom_heuristics_workspace(
    min_confidence: u8,
    state: State<'_, AppState>,
) -> Result<Vec<kivi_scanner::model::DetectedMapCandidate>, String> {
    let guard = super::lock_or_err(&state.memory, "memory")?;
    let mem = guard.as_ref().ok_or_else(|| "No binary loaded".to_string())?;
    Ok(kivi_scanner::scan_rom_with_options(
        mem.as_slice(),
        min_confidence,
        None,
    ))
}

// ---------------------------------------------------------------------------
// Wave 4: heuristic window scanner integration
// ---------------------------------------------------------------------------

/// Runs the exhaustive 16-bit window scanner over a ROM buffer.
///
/// This is a **separate detector** from `scan_rom_heuristics`: that one finds
/// periodic runs by pitch, this one scans every rectangular window by entropy,
/// smoothness and row autocorrelation. Their scores are **not comparable** and
/// the results must not be merged or cross-deduplicated.
pub fn scan_rom_heuristic_windows_inner(
    rom: &[u8],
    candidate_widths: &[usize],
    min_rows: usize,
    max_rows: usize,
    little_endian: bool,
) -> Result<Vec<kivi_scanner::heuristic_scan::DiscoveredMap>, String> {
    if rom.is_empty() {
        return Err("No ROM data to scan".to_string());
    }
    if candidate_widths.is_empty() {
        return Err("At least one candidate width is required".to_string());
    }
    if candidate_widths.contains(&0) {
        return Err("Candidate widths must be greater than zero".to_string());
    }
    if min_rows == 0 || max_rows < min_rows {
        return Err(format!(
            "Invalid row range: min_rows={} max_rows={}",
            min_rows, max_rows
        ));
    }
    // Guard against an unbounded scan on a huge width range.
    let width_span: usize = candidate_widths
        .iter()
        .try_fold(0usize, |acc, &w| acc.checked_add(w))
        .ok_or_else(|| "Scan window combination too large (limit 4096)".to_string())?;
    let row_span = max_rows - min_rows + 1;
    if width_span.saturating_mul(row_span) > 4096 {
        return Err("Scan window combination too large (limit 4096)".to_string());
    }

    Ok(kivi_scanner::heuristic_scan::scan_rom_16bit(
        rom,
        candidate_widths,
        min_rows,
        max_rows,
        little_endian,
    ))
}

#[tauri::command]
pub fn scan_rom_heuristic_windows(
    rom_data: Vec<u8>,
    candidate_widths: Vec<usize>,
    min_rows: usize,
    max_rows: usize,
    little_endian: bool,
) -> Result<Vec<kivi_scanner::heuristic_scan::DiscoveredMap>, String> {
    scan_rom_heuristic_windows_inner(
        &rom_data,
        &candidate_widths,
        min_rows,
        max_rows,
        little_endian,
    )
}

#[tauri::command]
pub fn scan_rom_heuristic_windows_workspace(
    candidate_widths: Vec<usize>,
    min_rows: usize,
    max_rows: usize,
    little_endian: bool,
    state: State<'_, AppState>,
) -> Result<Vec<kivi_scanner::heuristic_scan::DiscoveredMap>, String> {
    let guard = super::lock_or_err(&state.memory, "memory")?;
    let mem = guard.as_ref().ok_or_else(|| "No binary loaded".to_string())?;
    scan_rom_heuristic_windows_inner(
        mem.as_slice(),
        &candidate_widths,
        min_rows,
        max_rows,
        little_endian,
    )
}

// ---------------------------------------------------------------------------
// 1-D curve scanning
// ---------------------------------------------------------------------------

/// Runs the blind 1-D curve scanner over a ROM buffer.
///
/// This detector answers a question the rectangular scanners cannot: a curve
/// has one axis of length one, so `scan_rom_heuristic_windows` (which requires
/// `min_rows >= 1` rows *and* a width) and `scan_rom_with_options` (periodic
/// runs) never report it. In the catalogued BIN+XDF corpus **68%** of declared
/// multi-cell tables are curves, so leaving them out understates what the
/// scanner can find.
///
/// Both byte orders are scanned and concatenated: a ROM does not declare its
/// own endianness, and a curve decoded with the wrong order fails the
/// smoothness gate rather than producing a plausible-looking wrong answer.
pub fn scan_rom_curves_inner(
    rom: &[u8],
    min_points: usize,
    max_points: usize,
    include_8bit: bool,
) -> Result<Vec<kivi_scanner::curve_scan::DiscoveredCurve>, String> {
    use kivi_scanner::curve_scan::{scan_curves, CurveScanOptions, Endian};

    if rom.is_empty() {
        return Err("No ROM data to scan".to_string());
    }
    if min_points < 3 {
        return Err("min_points must be at least 3".to_string());
    }
    if max_points < min_points {
        return Err(format!(
            "Invalid point range: min_points={min_points} max_points={max_points}"
        ));
    }
    // A curve longer than this is a data section, not a calibration curve.
    if max_points > 4096 {
        return Err("max_points is limited to 4096".to_string());
    }

    let mut found = scan_curves(
        rom,
        &CurveScanOptions {
            min_points,
            max_points,
            endian: Endian::Big,
            include_8bit,
            allow_monotonic: false,
            snap_boundaries: false,
        },
    );
    found.extend(scan_curves(
        rom,
        &CurveScanOptions {
            min_points,
            max_points,
            endian: Endian::Little,
            include_8bit: false,
            allow_monotonic: false,
            snap_boundaries: false,
        },
    ));
    found.sort_by(|a, b| a.address.cmp(&b.address).then(b.points.cmp(&a.points)));
    Ok(found)
}

// ---------------------------------------------------------------------------
// Blind auto-mapping
// ---------------------------------------------------------------------------

/// Finds calibration tables in a ROM with **no definition file**.
///
/// This is the zero-knowledge path: no XDF, DAMOS, A2L or map pack is read. The
/// detectors find grid-shaped data, overlapping candidates are suppressed, and
/// monotonic runs next to a table are attached as axes.
///
/// Measured on real ROMs: 198 tables for a 1 MiB Honda, 11 for a 96 KiB Honda,
/// 313 for a 2 MiB Suzuki. Standard map packs typically document 78-108 per 1 MiB,
/// so this is the same order of magnitude — and it replaces 10 563 overlapping fragments.
///
/// The scaling and labels are **proposals**, not facts: a bare ROM states
/// neither, so each table carries the reasoning behind its suggestion.
pub fn blind_automap_inner(
    rom: &[u8],
    overlap_threshold: Option<f64>,
    include_curves: bool,
    include_8bit: bool,
) -> Result<Vec<kivi_scanner::blind_automap::BlindTable>, String> {
    use kivi_scanner::blind_automap::{blind_automap, BlindOptions};

    if rom.is_empty() {
        return Err("No ROM data to scan".to_string());
    }
    if rom.len() < 256 {
        return Err("ROM is too small to contain calibration tables".to_string());
    }
    let threshold = overlap_threshold.unwrap_or(0.0);
    if !(0.0..=1.0).contains(&threshold) {
        return Err(format!(
            "overlap_threshold must be between 0.0 and 1.0, got {threshold}"
        ));
    }

    Ok(blind_automap(
        rom,
        &BlindOptions {
            shapes: kivi_scanner::blind_automap::default_shapes(),
            overlap_threshold: threshold,
            include_curves,
            include_8bit,
        },
    ))
}

/// Runs the multi-width, bidirectional `Blind V2` scanner enriched with semantic classification.
pub fn blind_automap_v2_inner(
    rom: &[u8],
    include_curves: bool,
    include_8bit: bool,
    include_32bit: bool,
    max_tables: Option<usize>,
) -> Result<Vec<kivi_scanner::blind_automap::BlindTable>, String> {
    use kivi_scanner::blind_automap::blind_v2_candidates_to_tables;
    use kivi_scanner::blind_v2::{blind_discover_v2, BlindV2Options};
    use kivi_scanner::map_classifier::MapClassifierDb;

    if rom.is_empty() {
        return Err("No ROM data to scan".to_string());
    }
    if rom.len() < 256 {
        return Err("ROM is too small to contain calibration tables".to_string());
    }
    if max_tables == Some(0) {
        return Err("max_tables must be at least 1 when provided".to_string());
    }

    let opts = BlindV2Options {
        include_8bit,
        include_32bit,
        include_curves,
        ..Default::default()
    };
    let mut candidates = blind_discover_v2(rom, &opts);
    if let Some(limit) = max_tables {
        if candidates.len() > limit {
            candidates.sort_by(|a, b| {
                b.quality_score
                    .total_cmp(&a.quality_score)
                    .then_with(|| a.address.cmp(&b.address))
            });
            candidates.truncate(limit);
        }
    }
    let starter_db = MapClassifierDb::builtin_starter_db();
    Ok(blind_v2_candidates_to_tables(
        rom,
        &candidates,
        Some(&starter_db),
    ))
}

#[tauri::command]
pub fn blind_automap_v2_rom(
    rom_data: Vec<u8>,
    include_curves: bool,
    include_8bit: bool,
    include_32bit: bool,
    max_tables: Option<usize>,
) -> Result<Vec<kivi_scanner::blind_automap::BlindTable>, String> {
    blind_automap_v2_inner(
        &rom_data,
        include_curves,
        include_8bit,
        include_32bit,
        max_tables,
    )
}

#[tauri::command]
pub fn blind_automap_v2_workspace(
    include_curves: bool,
    include_8bit: bool,
    include_32bit: bool,
    max_tables: Option<usize>,
    state: State<'_, AppState>,
) -> Result<Vec<kivi_scanner::blind_automap::BlindTable>, String> {
    let guard = super::lock_or_err(&state.memory, "memory")?;
    let mem = guard.as_ref().ok_or_else(|| "No binary loaded".to_string())?;
    blind_automap_v2_inner(
        mem.as_slice(),
        include_curves,
        include_8bit,
        include_32bit,
        max_tables,
    )
}

#[tauri::command]
pub async fn blind_automap_v2_stream(
    include_curves: bool,
    include_8bit: bool,
    include_32bit: bool,
    max_tables: Option<usize>,
    on_progress: tauri::ipc::Channel<ScanProgressEvent>,
    state: State<'_, AppState>,
) -> Result<Vec<kivi_scanner::blind_automap::BlindTable>, String> {
    let snapshot = {
        let guard = super::lock_or_err(&state.memory, "memory")?;
        let mem = guard.as_ref().ok_or_else(|| "No binary loaded".to_string())?;
        mem.as_slice().to_vec()
    };
    if snapshot.len() < 256 {
        return Err("ROM is too small to contain calibration tables".to_string());
    }
    if max_tables == Some(0) {
        return Err("max_tables must be at least 1 when provided".to_string());
    }
    let _ = on_progress.send(ScanProgressEvent {
        stage: "discover".to_string(),
        percent: 20,
        message: format!("Scanning {} bytes for maps and axes", snapshot.len()),
    });
    let progress = on_progress.clone();
    let tables = tauri::async_runtime::spawn_blocking(move || {
        use kivi_scanner::blind_automap::blind_v2_candidates_to_tables;
        use kivi_scanner::blind_v2::{blind_discover_v2, BlindV2Options};
        use kivi_scanner::map_classifier::MapClassifierDb;

        let opts = BlindV2Options {
            include_8bit,
            include_32bit,
            include_curves,
            ..Default::default()
        };
        let mut candidates = blind_discover_v2(&snapshot, &opts);
        if let Some(limit) = max_tables {
            if candidates.len() > limit {
                candidates.sort_by(|a, b| {
                    b.quality_score
                        .total_cmp(&a.quality_score)
                        .then_with(|| a.address.cmp(&b.address))
                });
                candidates.truncate(limit);
            }
        }
        let _ = progress.send(ScanProgressEvent {
            stage: "classify".to_string(),
            percent: 75,
            message: format!("Classifying {} candidate regions", candidates.len()),
        });
        let starter_db = MapClassifierDb::builtin_starter_db();
        blind_v2_candidates_to_tables(&snapshot, &candidates, Some(&starter_db))
    })
    .await
    .map_err(|e| format!("Blind V2 scanner failed: {e}"))?;

    let _ = on_progress.send(ScanProgressEvent {
        stage: "complete".to_string(),
        percent: 100,
        message: format!("Discovered {} tables", tables.len()),
    });
    Ok(tables)
}

#[tauri::command]
pub fn blind_automap_rom(
    rom_data: Vec<u8>,
    overlap_threshold: Option<f64>,
    include_curves: bool,
    include_8bit: bool,
) -> Result<Vec<kivi_scanner::blind_automap::BlindTable>, String> {
    blind_automap_inner(&rom_data, overlap_threshold, include_curves, include_8bit)
}

#[tauri::command]
pub fn blind_automap_workspace(
    overlap_threshold: Option<f64>,
    include_curves: bool,
    include_8bit: bool,
    state: State<'_, AppState>,
) -> Result<Vec<kivi_scanner::blind_automap::BlindTable>, String> {
    let guard = super::lock_or_err(&state.memory, "memory")?;
    let mem = guard.as_ref().ok_or_else(|| "No binary loaded".to_string())?;
    blind_automap_inner(mem.as_slice(), overlap_threshold, include_curves, include_8bit)
}

// ---------------------------------------------------------------------------
// Denso / Toyota interleaved map scanning
// ---------------------------------------------------------------------------

/// Runs the Denso interleaved-map scanner over a ROM buffer.
///
/// Denso and Toyota store the Y-axis breakpoint **inside** each row, before the
/// data:
///
/// ```text
/// [Y_0] [Z_00 Z_01 ... Z_0(W-1)]
/// [Y_1] [Z_10 Z_11 ... Z_1(W-1)]
/// ```
///
/// No rectangular scanner can find these, because the data words are not
/// contiguous — every row is interrupted by its own breakpoint. That is why this
/// is a separate detector rather than another shape for `scan_rom_curves`.
pub fn scan_rom_denso_interleaved_inner(
    rom: &[u8],
    candidate_cols: &[usize],
    min_rows: usize,
    max_rows: usize,
) -> Result<Vec<kivi_scanner::denso::DensoInterleavedMap>, String> {
    if rom.is_empty() {
        return Err("No ROM data to scan".to_string());
    }
    if candidate_cols.is_empty() {
        return Err("At least one candidate width is required".to_string());
    }
    if candidate_cols.contains(&0) {
        return Err("Candidate column counts must be greater than zero".to_string());
    }
    if min_rows == 0 || max_rows < min_rows {
        return Err(format!(
            "Invalid row range: min_rows={min_rows} max_rows={max_rows}"
        ));
    }
    // Guard against an unbounded scan: 16 shapes x 4096 offsets is already
    // generous for a 256 KiB image.
    let col_span: usize = candidate_cols
        .iter()
        .try_fold(0usize, |acc, &c| acc.checked_add(c))
        .ok_or_else(|| "Scan window combination too large (limit 4096)".to_string())?;
    let combos = col_span.saturating_mul(max_rows - min_rows + 1);
    if combos > 4096 {
        return Err("Scan window combination too large (limit 4096)".to_string());
    }

    Ok(kivi_scanner::denso::DensoMapScanner::scan_interleaved_maps(
        rom,
        candidate_cols,
        min_rows,
        max_rows,
    ))
}

#[tauri::command]
pub fn scan_rom_denso_interleaved(
    rom_data: Vec<u8>,
    candidate_cols: Vec<usize>,
    min_rows: usize,
    max_rows: usize,
) -> Result<Vec<kivi_scanner::denso::DensoInterleavedMap>, String> {
    scan_rom_denso_interleaved_inner(&rom_data, &candidate_cols, min_rows, max_rows)
}

#[tauri::command]
pub fn scan_rom_denso_interleaved_workspace(
    candidate_cols: Vec<usize>,
    min_rows: usize,
    max_rows: usize,
    state: State<'_, AppState>,
) -> Result<Vec<kivi_scanner::denso::DensoInterleavedMap>, String> {
    let guard = super::lock_or_err(&state.memory, "memory")?;
    let mem = guard.as_ref().ok_or_else(|| "No binary loaded".to_string())?;
    scan_rom_denso_interleaved_inner(mem.as_slice(), &candidate_cols, min_rows, max_rows)
}

#[tauri::command]
pub fn scan_rom_curves(
    rom_data: Vec<u8>,
    min_points: usize,
    max_points: usize,
    include_8bit: bool,
) -> Result<Vec<kivi_scanner::curve_scan::DiscoveredCurve>, String> {
    scan_rom_curves_inner(&rom_data, min_points, max_points, include_8bit)
}

#[tauri::command]
pub fn scan_rom_curves_workspace(
    min_points: usize,
    max_points: usize,
    include_8bit: bool,
    state: State<'_, AppState>,
) -> Result<Vec<kivi_scanner::curve_scan::DiscoveredCurve>, String> {
    let guard = super::lock_or_err(&state.memory, "memory")?;
    let mem = guard.as_ref().ok_or_else(|| "No binary loaded".to_string())?;
    scan_rom_curves_inner(mem.as_slice(), min_points, max_points, include_8bit)
}


pub fn find_similar_maps_inner(
    source: kivi_scanner::similar_maps::SimilarMapSpec,
    options: Option<kivi_scanner::similar_maps::SimilarMapOptions>,
    state: &AppState,
) -> Result<Vec<kivi_scanner::similar_maps::SimilarMapHit>, String> {
    use kivi_scanner::similar_maps::{decode_map_flat, find_similar_maps_in_rom};

    let guard = super::lock_or_err(&state.memory, "memory")?;
    let mem = guard.as_ref().ok_or_else(|| "No binary loaded".to_string())?;
    if decode_map_flat(mem.as_slice(), &source).is_none() {
        return Err("Invalid source map specification or out of ROM bounds".to_string());
    }
    let opts = options.unwrap_or_default();
    Ok(find_similar_maps_in_rom(mem.as_slice(), &source, &opts))
}

#[tauri::command]
pub fn find_similar_maps(
    source: kivi_scanner::similar_maps::SimilarMapSpec,
    options: Option<kivi_scanner::similar_maps::SimilarMapOptions>,
    state: State<'_, AppState>,
) -> Result<Vec<kivi_scanner::similar_maps::SimilarMapHit>, String> {
    find_similar_maps_inner(source, options, &state)
}

pub fn find_migrated_maps_inner(
    source_rom: &[u8],
    target_rom: &[u8],
    source: kivi_scanner::similar_maps::SimilarMapSpec,
    options: Option<kivi_scanner::similar_maps::SimilarMapOptions>,
) -> Result<Vec<kivi_scanner::similar_maps::SimilarMapHit>, String> {
    use kivi_scanner::similar_maps::{decode_map_flat, find_migrated_map_across_roms};

    if source_rom.is_empty() || target_rom.is_empty() {
        return Err("Source and target ROM buffers must be non-empty".to_string());
    }
    if decode_map_flat(source_rom, &source).is_none() {
        return Err("Invalid source map specification or out of source ROM bounds".to_string());
    }
    let opts = options.unwrap_or_default();
    Ok(find_migrated_map_across_roms(
        source_rom, target_rom, &source, &opts,
    ))
}

#[tauri::command]
pub fn find_migrated_maps(
    source_rom: Vec<u8>,
    target_rom: Vec<u8>,
    source: kivi_scanner::similar_maps::SimilarMapSpec,
    options: Option<kivi_scanner::similar_maps::SimilarMapOptions>,
) -> Result<Vec<kivi_scanner::similar_maps::SimilarMapHit>, String> {
    find_migrated_maps_inner(&source_rom, &target_rom, source, options)
}

pub fn classify_blind_tables_inner(
    tables: &[kivi_scanner::blind_automap::BlindTable],
    rom: Option<&[u8]>,
    rules_json: Option<&str>,
) -> Result<Vec<Option<kivi_scanner::map_classifier::MapClassification>>, String> {
    use kivi_scanner::map_classifier::{classify_blind_table, MapClassifierDb};

    let db = match rules_json {
        Some(raw) => MapClassifierDb::from_json_str(raw)
            .map_err(|e| format!("Invalid map classifier rules JSON: {e}"))?,
        None => MapClassifierDb::builtin_starter_db(),
    };
    Ok(tables
        .iter()
        .map(|t| classify_blind_table(t, rom, &db))
        .collect())
}

#[tauri::command]
pub fn classify_blind_tables(
    tables: Vec<kivi_scanner::blind_automap::BlindTable>,
    rom: Option<Vec<u8>>,
    rules_json: Option<String>,
) -> Result<Vec<Option<kivi_scanner::map_classifier::MapClassification>>, String> {
    classify_blind_tables_inner(&tables, rom.as_deref(), rules_json.as_deref())
}

#[tauri::command]
pub fn classify_blind_tables_workspace(
    tables: Vec<kivi_scanner::blind_automap::BlindTable>,
    use_workspace_rom: Option<bool>,
    rules_json: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<Option<kivi_scanner::map_classifier::MapClassification>>, String> {
    if use_workspace_rom.unwrap_or(true) {
        let guard = super::lock_or_err(&state.memory, "memory")?;
        let mem = guard.as_ref().ok_or_else(|| "No binary loaded".to_string())?;
        return classify_blind_tables_inner(&tables, Some(mem.as_slice()), rules_json.as_deref());
    }
    classify_blind_tables_inner(&tables, None, rules_json.as_deref())
}

#[derive(serde::Deserialize)]
struct MigratedMapRawHeader {
    source: kivi_scanner::similar_maps::SimilarMapSpec,
    options: Option<kivi_scanner::similar_maps::SimilarMapOptions>,
}

/// Finds a migrated map between the loaded workspace ROM and a raw target binary in `Request`.
#[tauri::command]
pub fn find_migrated_maps_workspace_raw(
    request: tauri::ipc::Request<'_>,
    state: State<'_, AppState>,
) -> Result<Vec<kivi_scanner::similar_maps::SimilarMapHit>, String> {
    let spec_raw = request
        .headers()
        .get("x-spec-json")
        .ok_or_else(|| "Missing x-spec-json header".to_string())?
        .to_str()
        .map_err(|e| format!("Invalid x-spec-json header: {e}"))?;
    let payload: MigratedMapRawHeader = serde_json::from_str(spec_raw)
        .map_err(|e| format!("Malformed x-spec-json header: {e}"))?;
    let target_rom = match request.body() {
        tauri::ipc::InvokeBody::Raw(bytes) => bytes.as_slice(),
        tauri::ipc::InvokeBody::Json(_) => {
            return Err("find_migrated_maps_workspace_raw expects raw binary body".to_string())
        }
    };
    let guard = super::lock_or_err(&state.memory, "memory")?;
    let mem = guard.as_ref().ok_or_else(|| "No binary loaded".to_string())?;
    find_migrated_maps_inner(mem.as_slice(), target_rom, payload.source, payload.options)
}

// ---------------------------------------------------------------------------
// Declarative Calibration Patch / Map-Transfer Scripts & Stock ROM Registry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CalibrationScriptApplyPayload {
    pub dry_run: kivi_scanner::calibration_script::CalibrationScriptDryRun,
    pub sha256: String,
    pub can_undo: bool,
    pub can_redo: bool,
}

#[tauri::command]
pub fn sample_calibration_script_cmd() -> String {
    kivi_scanner::calibration_script::sample_calibration_script_json()
}

pub fn dry_run_calibration_script_inner(
    script_json: &str,
    rom_data: Option<&[u8]>,
    state: &AppState,
) -> Result<kivi_scanner::calibration_script::CalibrationScriptDryRun, String> {
    let script = kivi_scanner::calibration_script::parse_calibration_script(script_json)?;
    if let Some(bytes) = rom_data {
        if bytes.is_empty() {
            return Err("ROM buffer cannot be empty".to_string());
        }
        return Ok(kivi_scanner::calibration_script::dry_run_calibration_script(
            bytes, &script,
        ));
    }
    let guard = super::lock_or_err(&state.memory, "memory")?;
    let mem = guard.as_ref().ok_or_else(|| "No binary loaded".to_string())?;
    Ok(kivi_scanner::calibration_script::dry_run_calibration_script(
        mem.as_slice(),
        &script,
    ))
}

#[tauri::command]
pub fn dry_run_calibration_script_cmd(
    script_json: String,
    rom_data: Option<Vec<u8>>,
    state: State<'_, AppState>,
) -> Result<kivi_scanner::calibration_script::CalibrationScriptDryRun, String> {
    dry_run_calibration_script_inner(&script_json, rom_data.as_deref(), &state)
}

pub fn apply_calibration_script_workspace_inner(
    script_json: &str,
    state: &AppState,
) -> Result<CalibrationScriptApplyPayload, String> {
    let script = kivi_scanner::calibration_script::parse_calibration_script(script_json)?;
    let (_repaired, hashes, undo, dry_run) = super::window_io::mutate_active_rom_with_undo(
        state,
        &format!("Apply Script: {}", script.name),
        |bytes| kivi_scanner::calibration_script::apply_calibration_script(bytes, &script),
    )?;
    Ok(CalibrationScriptApplyPayload {
        dry_run,
        sha256: hashes.sha256,
        can_undo: undo.can_undo,
        can_redo: undo.can_redo,
    })
}

#[tauri::command]
pub fn apply_calibration_script_workspace_cmd(
    script_json: String,
    state: State<'_, AppState>,
) -> Result<CalibrationScriptApplyPayload, String> {
    apply_calibration_script_workspace_inner(&script_json, &state)
}

pub fn match_stock_rom_registry_inner(
    custom_pack_json: Option<&str>,
    rom_data: Option<&[u8]>,
    state: &AppState,
) -> Result<kivi_scanner::stock_registry::StockRomMatchReport, String> {
    if let Some(bytes) = rom_data {
        return kivi_scanner::stock_registry::match_stock_rom_registry(bytes, custom_pack_json);
    }
    let guard = super::lock_or_err(&state.memory, "memory")?;
    let mem = guard.as_ref().ok_or_else(|| "No binary loaded".to_string())?;
    kivi_scanner::stock_registry::match_stock_rom_registry(mem.as_slice(), custom_pack_json)
}

#[tauri::command]
pub fn match_stock_rom_registry_cmd(
    custom_pack_json: Option<String>,
    rom_data: Option<Vec<u8>>,
    state: State<'_, AppState>,
) -> Result<kivi_scanner::stock_registry::StockRomMatchReport, String> {
    match_stock_rom_registry_inner(custom_pack_json.as_deref(), rom_data.as_deref(), &state)
}
