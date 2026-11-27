use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};

pub(crate) const DB: &str = "/root/ecu_master_database";

pub(crate) struct RomSpec {
    pub(crate) id: &'static str,
    pub(crate) family: &'static str,
    pub(crate) rel_path: &'static str,
}

pub(crate) const ROMS: &[RomSpec] = &[
    RomSpec {
        id: "SUZUKI_5JCZSNC0",
        family: "Suzuki_Hayabusa_Denso",
        rel_path: "01_MOTORCYCLES/SUZUKI/BIG_BIKE_GSXR_HAYABUSA/5JCZSNC0.bin",
    },
    RomSpec {
        id: "SUZUKI_5JCZSJB0",
        family: "Suzuki_Hayabusa_Denso",
        rel_path: "01_MOTORCYCLES/SUZUKI/BIG_BIKE_GSXR_HAYABUSA/5JCZSJB0.bin",
    },
    RomSpec {
        id: "AUDI_RS4_8D0907551F",
        family: "VAG_ME7_1_TwinTurbo",
        rel_path: "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_RS4_B5_2_7TT_COSWORTH/8D0907551F.bin",
    },
    RomSpec {
        id: "AUDI_S4_8D0907551M",
        family: "VAG_ME7_1_TwinTurbo",
        rel_path: "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-0002.bin",
    },
    RomSpec {
        id: "BMW_MS43_512K_RHD",
        family: "BMW_Siemens_MS43",
        rel_path: "02_CARS/BMW/SIEMENS_MS43/Siemens_MS43_MS430069_E46_M54B22_EU4_RHD.bin",
    },
    RomSpec {
        id: "VW_GOLF5_VR6_ME711",
        family: "VAG_ME7_1_1_VR6",
        rel_path: "02_CARS/VAG_AUDI_VOLKSWAGEN/VAG_TDI_EDC15_EDC16_ORIGINAL_BINS/VW Golf 5 3.2 VR6 260HP 022906032CE 0261208468 377419.ori",
    },
    RomSpec {
        id: "BMW_ME9_770115XE",
        family: "BMW_Bosch_ME9",
        rel_path: "02_CARS/BMW/BOSCH_ME9_VALVETRONIC_N42_N46_N62/770115XE.BIN",
    },
    RomSpec {
        id: "BMW_B58_8M",
        family: "BMW_Bosch_MG1CS003_B58",
        rel_path: "02_CARS/BMW/B48_B58_GEN1_GEN2_A2L_XDF/00005D55508C05_b58o1_original.bin",
    },
    RomSpec {
        id: "VOLVO_ME7_50GPHJ",
        family: "Volvo_ME7",
        rel_path: "02_CARS/VOLVO_SAAB/VOLVO_ME7_T5_R/50GPHJ-23GPPVD0-MAN-AWD-US - Speed Density.bin",
    },
    RomSpec {
        id: "HONDA_CBR250RR",
        family: "Honda_Keihin_SH7058",
        rel_path: "01_MOTORCYCLES/HONDA/SPORT_CBR250RR/CBR250RR-38770-K64-ND1_BIN.bin",
    },
];

#[derive(Clone, Copy)]
pub(crate) enum RefFormat {
    Xdf,
    A2l,
    DamosMe7,
}

pub(crate) struct RefSpec {
    pub(crate) eval_id: &'static str,
    pub(crate) rom_id: &'static str,
    pub(crate) format: RefFormat,
    pub(crate) rel_path: &'static str,
    pub(crate) disclosure: &'static str,
}

pub(crate) const REFS: &[RefSpec] = &[
    RefSpec {
        eval_id: "SUZUKI_5JCZSNC0_XDF",
        rom_id: "SUZUKI_5JCZSNC0",
        format: RefFormat::Xdf,
        rel_path: "01_MOTORCYCLES/SUZUKI/BIG_BIKE_GSXR_HAYABUSA/5JCZSNC0.xdf",
        disclosure: "",
    },
    RefSpec {
        eval_id: "SUZUKI_5JCZSJB0_XDF",
        rom_id: "SUZUKI_5JCZSJB0",
        format: RefFormat::Xdf,
        rel_path: "01_MOTORCYCLES/SUZUKI/BIG_BIKE_GSXR_HAYABUSA/5JCZSJB0.xdf",
        disclosure: "",
    },
    RefSpec {
        eval_id: "AUDI_RS4_8D0907551F_XDF",
        rom_id: "AUDI_RS4_8D0907551F",
        format: RefFormat::Xdf,
        rel_path: "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_RS4_B5_2_7TT_COSWORTH/BRO 8D0907551F.xdf",
        disclosure: "",
    },
    RefSpec {
        eval_id: "AUDI_S4_8D0907551M_XDF",
        rom_id: "AUDI_S4_8D0907551M",
        format: RefFormat::Xdf,
        rel_path: "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-20190711.xdf",
        disclosure: "",
    },
    RefSpec {
        eval_id: "AUDI_S4_8D0907551M_DAMOS",
        rom_id: "AUDI_S4_8D0907551M",
        format: RefFormat::DamosMe7,
        rel_path: "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-0002.dam",
        disclosure: "ME7 DAMOS mixed flash+RAM; InternalRam ($380000-$38FFFF) and unmapped excluded via translate_me7",
    },
    RefSpec {
        eval_id: "BMW_MS43_512K_RHD_XDF",
        rom_id: "BMW_MS43_512K_RHD",
        format: RefFormat::Xdf,
        rel_path: "02_CARS/BMW/SIEMENS_MS43/Siemens_MS43_MS430069_ENG_512K_v1.16.xdf",
        disclosure: "XDF BASEOFFSET=0x70000 (subtract=0) applied",
    },
    RefSpec {
        eval_id: "VW_GOLF5_VR6_ME711_A2L",
        rom_id: "VW_GOLF5_VR6_ME711",
        format: RefFormat::A2l,
        rel_path: "02_CARS/VAG_AUDI_VOLKSWAGEN/GOLF_VR6_ABA_BINS/022906032CE-6733.A2L",
        disclosure: "A2L MEMORY_SEGMENT translated via build_translator_for_rom",
    },
    RefSpec {
        eval_id: "BMW_ME9_770115XE_A2L",
        rom_id: "BMW_ME9_770115XE",
        format: RefFormat::A2l,
        rel_path: "02_CARS/BMW/BOSCH_ME9_VALVETRONIC_N42_N46_N62/770115XE.a2l",
        disclosure: "A2L MEMORY_SEGMENT translated via build_translator_for_rom",
    },
    RefSpec {
        eval_id: "BMW_B58_8M_A2L",
        rom_id: "BMW_B58_8M",
        format: RefFormat::A2l,
        rel_path: "02_CARS/BMW/B48_B58_GEN1_GEN2_A2L_XDF/R4C9L8N5B_5d55-080_060_005_v7.2.3.a2l",
        disclosure: "Software revision delta: R4C9L8N5B (5d55-080_060_005 v7.2.3) vs 00005D55508C05 BIN (60.6% varying)",
    },
    RefSpec {
        eval_id: "BMW_B58_8M_XDF",
        rom_id: "BMW_B58_8M",
        format: RefFormat::Xdf,
        rel_path: "02_CARS/BMW/B48_B58_GEN1_GEN2_A2L_XDF/R4C9L8P7B_5d55-080_072_007_v7.3.xdf",
        disclosure: "Same folder XDF R4C9L8P7B (5d55-080_072_007 v7.3) vs 00005D55508C05 BIN",
    },
    RefSpec {
        eval_id: "VOLVO_ME7_50GPHJ_XDF",
        rom_id: "VOLVO_ME7_50GPHJ",
        format: RefFormat::Xdf,
        rel_path: "02_CARS/VOLVO_SAAB/VOLVO_ME7_T5_R/50GPHJ 4.xdf",
        disclosure: "Blank XDF header; filename-bound compatibility note from Wave 3.3 audit",
    },
    RefSpec {
        eval_id: "HONDA_CBR250RR_XDF",
        rom_id: "HONDA_CBR250RR",
        format: RefFormat::Xdf,
        rel_path: "01_MOTORCYCLES/HONDA/SPORT_CBR250RR/CBR250RR_38770-K64-ND1_PUBLIC_v1.xdf",
        disclosure: "",
    },
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FrozenCandidate {
    pub(crate) address: usize,
    pub(crate) rows: usize,
    pub(crate) cols: usize,
    pub(crate) elem_bytes: usize,
    pub(crate) little_endian: bool,
    pub(crate) is_curve: bool,
    pub(crate) source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FrozenScannerRun {
    pub(crate) scanner: String,
    pub(crate) runtime_ms: f64,
    pub(crate) ms_per_mib: f64,
    pub(crate) candidate_count: usize,
    pub(crate) candidates_per_mib: f64,
    pub(crate) candidates: Vec<FrozenCandidate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FrozenRomDiscovery {
    pub(crate) id: String,
    pub(crate) family: String,
    pub(crate) path: String,
    pub(crate) is_negative_control_twin: bool,
    pub(crate) twin_source_rom: Option<String>,
    pub(crate) bytes: usize,
    pub(crate) sha256: String,
    pub(crate) scanners: BTreeMap<String, FrozenScannerRun>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct FrozenFileEnvelope {
    pub(crate) wave: String,
    pub(crate) phase: String,
    pub(crate) protocol: String,
    pub(crate) entries: Vec<FrozenRomDiscovery>,
}

#[derive(Clone)]
pub(crate) struct RefItem {
    pub(crate) address: usize,
    pub(crate) rows: usize,
    pub(crate) cols: usize,
    pub(crate) elem_bytes: usize,
    pub(crate) is_2d: bool,
    pub(crate) has_ref_axes: bool,
}

pub(crate) struct ParsedRefSummary {
    pub(crate) format_name: &'static str,
    pub(crate) raw_items: usize,
    pub(crate) ram_excluded: usize,
    pub(crate) unmapped_excluded: usize,
    pub(crate) oob_excluded: usize,
    pub(crate) dedup_items: Vec<RefItem>,
    pub(crate) translator_note: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct ScannerEvalMetrics {
    pub(crate) scanner: String,
    pub(crate) total_candidates: usize,
    pub(crate) candidates_per_mib: f64,
    pub(crate) runtime_ms: f64,
    pub(crate) ms_per_mib: f64,
    pub(crate) recall_2d_exact: f64,
    pub(crate) recall_2d_near_cells2: f64,
    pub(crate) recall_2d_near_abs8: f64,
    pub(crate) recall_2d_overlap: f64,
    pub(crate) hits_2d_exact: usize,
    pub(crate) hits_2d_cells2: usize,
    pub(crate) hits_2d_abs8: usize,
    pub(crate) hits_2d_overlap: usize,
    pub(crate) recall_1d_abs8: f64,
    pub(crate) recall_1d_overlap: f64,
    pub(crate) hits_1d_abs8: usize,
    pub(crate) hits_1d_overlap: usize,
    pub(crate) recall_2d_u8_abs8: Option<f64>,
    pub(crate) recall_2d_u16_abs8: Option<f64>,
    pub(crate) recall_2d_u8_overlap: Option<f64>,
    pub(crate) recall_2d_u16_overlap: Option<f64>,
    pub(crate) recall_2d_axis_supported_abs8: Option<f64>,
    pub(crate) recall_2d_axis_free_abs8: Option<f64>,
    pub(crate) precision_abs8: f64,
    pub(crate) precision_overlap: f64,
    pub(crate) matched_cands_abs8: usize,
    pub(crate) matched_cands_overlap: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct RefEvalReport {
    pub(crate) eval_id: String,
    pub(crate) rom_id: String,
    pub(crate) format: String,
    pub(crate) ref_path: String,
    pub(crate) disclosure: String,
    pub(crate) translator_note: String,
    pub(crate) rom_bytes: usize,
    pub(crate) raw_ref_entries: usize,
    pub(crate) ram_excluded: usize,
    pub(crate) unmapped_excluded: usize,
    pub(crate) oob_excluded: usize,
    pub(crate) dedup_in_bounds_2d: usize,
    pub(crate) dedup_in_bounds_1d: usize,
    pub(crate) ref_2d_varying: usize,
    pub(crate) ref_1d_varying: usize,
    pub(crate) ref_varying_pct: f64,
    pub(crate) ref_2d_u8_varying: usize,
    pub(crate) ref_2d_u16_varying: usize,
    pub(crate) ref_2d_axis_supported_varying: usize,
    pub(crate) ref_2d_axis_free_varying: usize,
    pub(crate) scanners: BTreeMap<String, ScannerEvalMetrics>,
}

#[derive(Debug, Serialize)]
pub(crate) struct TwinControlReport {
    pub(crate) twin_id: String,
    pub(crate) source_rom: String,
    pub(crate) bytes: usize,
    pub(crate) verify_a_histogram_equal: bool,
    pub(crate) fp_counts: BTreeMap<String, usize>,
    pub(crate) fp_per_mib: BTreeMap<String, f64>,
}

#[derive(Debug, Serialize)]
pub(crate) struct PuXdfVsDamosAudit {
    pub(crate) rom_id: String,
    pub(crate) xdf_eval_id: String,
    pub(crate) damos_eval_id: String,
    pub(crate) xdf_2d_varying: usize,
    pub(crate) xdf_1d_varying: usize,
    pub(crate) damos_2d_varying: usize,
    pub(crate) damos_1d_varying: usize,
    pub(crate) exact_2d_overlap: usize,
    pub(crate) span_2d_overlap: usize,
    pub(crate) xdf_label_freq_c_2d_pct: f64,
    pub(crate) blind_v2_2d_candidates: usize,
    pub(crate) v2_matched_xdf_overlap: usize,
    pub(crate) v2_matched_damos_overlap: usize,
    pub(crate) v2_matched_union_overlap: usize,
    pub(crate) v2_xdf_apparent_precision_pct: f64,
    pub(crate) v2_damos_factory_precision_pct: f64,
    pub(crate) v2_union_precision_pct: f64,
    pub(crate) v2_xdf_apparent_fp_rescued_by_damos: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct BenchmarkResults {
    pub(crate) wave: String,
    pub(crate) phase: String,
    pub(crate) frozen_candidates_path: String,
    pub(crate) frozen_candidates_sha256: String,
    pub(crate) twins: Vec<TwinControlReport>,
    pub(crate) evaluations: Vec<RefEvalReport>,
    pub(crate) macro_averages: BTreeMap<String, BTreeMap<String, f64>>,
    pub(crate) pu_xdf_vs_damos_audit: Option<PuXdfVsDamosAudit>,
}

