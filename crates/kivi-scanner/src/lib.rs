pub mod damos_parse;
pub mod denso;
pub mod denso_descriptor_maps;
pub mod dtc_table_scan;
pub mod ecu_fingerprint;
pub mod a2l_parse;
pub mod axis_detector;
pub mod axis_anchored;
pub mod blind_automap;
pub mod blind_v2;
pub mod boundary_snap;
pub mod calibration_script;
pub mod curve_scan;
pub mod heuristic_scan;
pub mod heuristic_score;
pub mod kp_parse;
pub mod limiter_scan;
pub mod map_classifier;
pub mod map_quality;
pub mod model;
pub mod nms_score;
pub mod p23_descriptors;
pub mod pair_verify;
pub mod pointer_snap;
pub mod pattern_search;
pub mod scan_metrics;
pub mod rom_identity;
pub mod scorer;
pub mod similar_maps;
pub mod stock_registry;
pub mod table_detector;
pub mod table_split;
pub mod zero_xdf_prefilter;
pub mod xdf_export;
pub mod xdf_parse;
pub mod zero_xdf_scan;

pub use axis_detector::*;
pub use axis_anchored::*;
pub use blind_automap::*;
pub use heuristic_scan::*;
pub use heuristic_score::*;
pub use limiter_scan::*;
pub use model::*;
pub use pattern_search::*;
pub use scorer::*;
pub use table_detector::*;
pub use zero_xdf_prefilter::*;
pub use xdf_export::*;
pub use xdf_parse::*;
pub use zero_xdf_scan::*;

pub fn scan_rom(rom: &[u8], sensitivity: ScannerSensitivity) -> Vec<DetectedMapCandidate> {
    scan_rom_with_options(rom, sensitivity.min_confidence(), None)
}

pub fn scan_rom_with_options(
    rom: &[u8],
    min_confidence: u8,
    pitches: Option<&[usize]>,
) -> Vec<DetectedMapCandidate> {
    let axes = detect_axes(rom);
    let min_rows = if min_confidence >= 85 {
        6
    } else if min_confidence >= 70 {
        5
    } else {
        4
    };
    let min_similarity = 0.80;
    let candidate_pitches = pitches.unwrap_or(DEFAULT_PITCHES);
    let raw_tables = detect_periodic_tables(rom, candidate_pitches, min_similarity, min_rows);
    score_and_pair_with_min_confidence(raw_tables, &axes, min_confidence)
}
