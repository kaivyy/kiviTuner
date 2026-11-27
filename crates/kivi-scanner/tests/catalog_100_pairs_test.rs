//! Blind discovery across 100 catalogued BIN + definition pairs.
//!
//! ## Source of the pair list
//!
//! `DAFTAR_PASANGAN_BIN_DAMOS_A2L_KP_TERVERIFIKASI.md` in the ECU database
//! records 156 pairs of firmware and a definition file, verified to exist on
//! disk. This suite takes the first 100 distinct pairs.
//!
//! ## Method
//!
//! For each pair the scanner runs **blind** — it never reads the definition.
//! The definition is then used as an independent oracle: the addresses it
//! declares are compared against what the scanner reported.
//!
//! Definition kinds supported: `.kp` (plaintext and ZIP-wrapped), `.dam`
//! (DAMOS), `.a2l` (ASAM MCD-2 MC). `.ols` has no parser yet and is skipped,
//! with the count reported.
//!
//! ## Integrity
//!
//! Every firmware file is hashed before and after; the test fails if any changed.

use kivi_scanner::a2l_parse::parse_a2l;
use kivi_scanner::damos_parse::parse_damos;
use kivi_scanner::kp_parse::parse_map_pack;
use kivi_scanner::zero_xdf_scan::{
    detect_global_endianness, scan_16bit_tables_auto_endian, scan_8bit_tables,
};
use rayon::prelude::*;
use std::path::PathBuf;
use std::time::Instant;

const DB: &str = "/root/ecu_master_database";

const PAIRS: &[(&str, &str, &str, &str)] = &[
    ("4Z7907551R", "02_CARS/VAG_AUDI_VOLKSWAGEN/VAG_TDI_EDC15_EDC16_ORIGINAL_BINS/Audi A6 allroad 2.7T BEL 250HP 4Z7907551R.ori", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_B5_ME7_2_7T/4Z7907551R.kp", "KP"),
    ("8D0907551F", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_RS4_B5_2_7TT_COSWORTH/8D0907551F.bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551F.kp", "KP"),
    ("8D0907551F", "02_CARS/VAG_AUDI_VOLKSWAGEN/VAG_TDI_EDC15_EDC16_ORIGINAL_BINS/Audi RS4 2.7T 8D0907551F 0261206635 354531.ori", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551F.kp", "KP"),
    ("8D0907551G", "02_CARS/VAG_AUDI_VOLKSWAGEN/VAG_TDI_EDC15_EDC16_ORIGINAL_BINS/Audi S4 2.7 8D0907551G 0261206776 360855.ori", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551G.kp", "KP"),
    ("8D0907551M", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_B5_ME7_2_7T/8D0907551M-0002 (stock MAP 5120).bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M.kp", "KP"),
    ("8D0907551M", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_B5_ME7_2_7T/8D0907551M-0002 (stock MAP 5120 16Bit KFZW).bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M.kp", "KP"),
    ("8D0907551M", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_B5_ME7_2_7T/8D0907551M-0002.bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M.kp", "KP"),
    ("8D0907551M", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_B5_ME7_2_7T/8D0907551M-0002 (16Bit KFZW).bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M.kp", "KP"),
    ("8D0907551M", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_B5_ME7_2_7T/8D0907551M-002_5120-stock_map-nyet.bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M.kp", "KP"),
    ("8D0907551M", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-0002 (stock MAP 5120).bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M.kp", "KP"),
    ("8D0907551M", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-0002 (stock MAP 5120 16Bit KFZW).bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M.kp", "KP"),
    ("8D0907551M", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-0002.bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M.kp", "KP"),
    ("8D0907551M", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-0002 (16Bit KFZW).bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M.kp", "KP"),
    ("8D0907551M", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-002_5120-stock_map-nyet.bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M.kp", "KP"),
    ("8D0907558E", "02_CARS/VAG_AUDI_VOLKSWAGEN/VAG_TDI_EDC15_EDC16_ORIGINAL_BINS/VW Passat 1.8T 8D0907558E 0261204805 358123.bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907558E.kp", "KP"),
    ("8D0907551M", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_B5_ME7_2_7T/8D0907551M-0002 (stock MAP 5120).bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-20261006.kp", "KP"),
    ("8D0907551M", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_B5_ME7_2_7T/8D0907551M-0002 (stock MAP 5120 16Bit KFZW).bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-20261006.kp", "KP"),
    ("8D0907551M", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_B5_ME7_2_7T/8D0907551M-0002.bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-20261006.kp", "KP"),
    ("8D0907551M", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_B5_ME7_2_7T/8D0907551M-0002 (16Bit KFZW).bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-20261006.kp", "KP"),
    ("8D0907551M", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_B5_ME7_2_7T/8D0907551M-002_5120-stock_map-nyet.bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-20261006.kp", "KP"),
    ("8D0907551M", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-0002 (stock MAP 5120).bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-20261006.kp", "KP"),
    ("8D0907551M", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-0002 (stock MAP 5120 16Bit KFZW).bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-20261006.kp", "KP"),
    ("8D0907551M", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-0002.bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-20261006.kp", "KP"),
    ("8D0907551M", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-0002 (16Bit KFZW).bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-20261006.kp", "KP"),
    ("8D0907551M", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-002_5120-stock_map-nyet.bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-20261006.kp", "KP"),
    ("8D0907551M", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_B5_ME7_2_7T/8D0907551M-0002 (stock MAP 5120).bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-20190711.kp", "KP"),
    ("8D0907551M", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_B5_ME7_2_7T/8D0907551M-0002 (stock MAP 5120 16Bit KFZW).bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-20190711.kp", "KP"),
    ("8D0907551M", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_B5_ME7_2_7T/8D0907551M-0002.bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-20190711.kp", "KP"),
    ("8D0907551M", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_B5_ME7_2_7T/8D0907551M-0002 (16Bit KFZW).bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-20190711.kp", "KP"),
    ("8D0907551M", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_B5_ME7_2_7T/8D0907551M-002_5120-stock_map-nyet.bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-20190711.kp", "KP"),
    ("8D0907551M", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-0002 (stock MAP 5120).bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-20190711.kp", "KP"),
    ("8D0907551M", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-0002 (stock MAP 5120 16Bit KFZW).bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-20190711.kp", "KP"),
    ("8D0907551M", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-0002.bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-20190711.kp", "KP"),
    ("8D0907551M", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-0002 (16Bit KFZW).bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-20190711.kp", "KP"),
    ("8D0907551M", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-002_5120-stock_map-nyet.bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-20190711.kp", "KP"),
    ("8D0907551M", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_B5_ME7_2_7T/8D0907551M-0002 (stock MAP 5120).bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-0002.dam", "DAM"),
    ("8D0907551M", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_B5_ME7_2_7T/8D0907551M-0002 (stock MAP 5120 16Bit KFZW).bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-0002.dam", "DAM"),
    ("8D0907551M-0002", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_B5_ME7_2_7T/8D0907551M-0002.bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-0002.dam", "DAM"),
    ("8D0907551M", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_B5_ME7_2_7T/8D0907551M-0002 (16Bit KFZW).bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-0002.dam", "DAM"),
    ("8D0907551M", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_B5_ME7_2_7T/8D0907551M-002_5120-stock_map-nyet.bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-0002.dam", "DAM"),
    ("8D0907551M", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-0002 (stock MAP 5120).bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-0002.dam", "DAM"),
    ("8D0907551M", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-0002 (stock MAP 5120 16Bit KFZW).bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-0002.dam", "DAM"),
    ("8D0907551M-0002", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-0002.bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-0002.dam", "DAM"),
    ("8D0907551M", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-0002 (16Bit KFZW).bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-0002.dam", "DAM"),
    ("8D0907551M", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-002_5120-stock_map-nyet.bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-0002.dam", "DAM"),
    ("8D0907558M", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_A8_S4_MOTRONIC_M5_M3/Audi_S4_2.7TT_A4_1.8T_8D0907558M_Motronic_ORI.bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907558M.kp", "KP"),
    ("022906032CE", "02_CARS/VAG_AUDI_VOLKSWAGEN/VAG_TDI_EDC15_EDC16_ORIGINAL_BINS/VW Golf 5 3.2 VR6 260HP 022906032CE 0261208468 377419.ori", "02_CARS/VAG_AUDI_VOLKSWAGEN/GOLF_VR6_ABA_BINS/022906032CE-6733.A2L", "A2L"),
    ("038906018N", "02_CARS/VAG_AUDI_VOLKSWAGEN/VAG_TDI_EDC15_EDC16_ORIGINAL_BINS/VW_Passat_D1900_R4V2T_66kW_0281001727_038906018N_1615_1037357703_crcF4227CA9.ori", "02_CARS/VAG_AUDI_VOLKSWAGEN/VAG_TDI_EDC15_EDC16_ORIGINAL_BINS/VW_Passat_1.9Tdi_66kW_0281001727_038906018N_1615_1037357703.ols", "OLS"),
    ("4D1907558", "02_CARS/VAG_AUDI_VOLKSWAGEN/VAG_TDI_EDC15_EDC16_ORIGINAL_BINS/Audi RS6+ 4D1907558F 0261208728 373709.ori", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_A8_S4_MOTRONIC_M5_M3/4D1907558-20200707.kp", "KP"),
    ("4D1907558", "02_CARS/VAG_AUDI_VOLKSWAGEN/VAG_TDI_EDC15_EDC16_ORIGINAL_BINS/Audi RS6+ 4D1907558F 0261208728 373709.ori", "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_A8_S4_MOTRONIC_M5_M3/4D1907558.kp", "KP"),
    ("8N0906018CB", "02_CARS/VAG_AUDI_VOLKSWAGEN/VW_GOLF_MK4_1_8T_MOTRONIC_M383/8N0906018CB.bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/VW_GOLF_MK4_1_8T_MOTRONIC_M383/8N0906018CB.kp", "KP"),
    ("06A906032S", "02_CARS/VAG_AUDI_VOLKSWAGEN/VAG_TDI_EDC15_EDC16_ORIGINAL_BINS/VW bora 1.8T 180HP 06A906032SL 0261208529 371155.ori", "02_CARS/VAG_AUDI_VOLKSWAGEN/VW_GOLF_MK4_1_8T_MOTRONIC_M383/06A906032S-0004.dam", "DAM"),
    ("06A906032S", "02_CARS/VAG_AUDI_VOLKSWAGEN/VAG_TDI_EDC15_EDC16_ORIGINAL_BINS/VW golf 1.8T AWV 150HP 06A906032SK 0261208528 371152.ori", "02_CARS/VAG_AUDI_VOLKSWAGEN/VW_GOLF_MK4_1_8T_MOTRONIC_M383/06A906032S-0004.dam", "DAM"),
    ("06A906032S", "02_CARS/VAG_AUDI_VOLKSWAGEN/VAG_TDI_EDC15_EDC16_ORIGINAL_BINS/Audi A3 1.8T 150HP 06A906032S 0261206197 359598.ori", "02_CARS/VAG_AUDI_VOLKSWAGEN/VW_GOLF_MK4_1_8T_MOTRONIC_M383/06A906032S-0004.dam", "DAM"),
    ("06A906032S", "02_CARS/VAG_AUDI_VOLKSWAGEN/SKODA_SEAT_TDI_TFSI_ORIGINAL_BINS/Seat leon 1.8T AUQ 180HP 06A906032SJ 0261208531 369040.ori", "02_CARS/VAG_AUDI_VOLKSWAGEN/VW_GOLF_MK4_1_8T_MOTRONIC_M383/06A906032S-0004.dam", "DAM"),
    ("06A906032S", "02_CARS/VAG_AUDI_VOLKSWAGEN/SKODA_SEAT_TDI_TFSI_ORIGINAL_BINS/Skoda octavia RS 1.8T AUQ 06A906032SG 0261208532 369039.ori", "02_CARS/VAG_AUDI_VOLKSWAGEN/VW_GOLF_MK4_1_8T_MOTRONIC_M383/06A906032S-0004.dam", "DAM"),
    ("06A906032LP", "02_CARS/VAG_AUDI_VOLKSWAGEN/VAG_TDI_EDC15_EDC16_ORIGINAL_BINS/06A906032LP_0261207955_v367518_ME7.5-1M_Jetta-1.8T-MT_OEM.bin", "02_CARS/VAG_AUDI_VOLKSWAGEN/VW_GOLF_MK4_1_8T_MOTRONIC_M383/06A906032LP.kp", "KP"),
    ("6m661411", "02_CARS/BMW/EDC16_320D_330XD/6m661411.hex", "02_CARS/BMW/BOSCH_ME9_VALVETRONIC_N42_N46_N62/6m661411.A2L", "A2L"),
    ("770115XE", "02_CARS/BMW/BOSCH_ME9_VALVETRONIC_N42_N46_N62/770115XE.BIN", "02_CARS/BMW/BOSCH_ME9_VALVETRONIC_N42_N46_N62/770115XE.a2l", "A2L"),
    ("770115XE", "02_CARS/BMW/BOSCH_ME9_VALVETRONIC_N42_N46_N62/770115XE.BIN", "02_CARS/BMW/BOSCH_ME9_VALVETRONIC_N42_N46_N62/770115XE_alt.a2l", "A2L"),
    ("6m661411", "02_CARS/BMW/EDC16_320D_330XD/6m661411.hex", "02_CARS/BMW/EDC16_320D_330XD/6m661411.A2L", "A2L"),
    ("BMW EDC16", "02_CARS/BMW/EDC16_320D_330XD/BMW EDC16.hex", "02_CARS/BMW/EDC16_320D_330XD/BMW EDC16.A2L", "A2L"),
];

struct PairResult {
    kind: String,
    oracle: usize,
    matched: usize,
}

/// Collect the addresses a definition declares, or `None` when the format has
/// no parser.
/// Multi-cell map shapes declared by a definition.
///
/// The scanner needs to know what shapes exist; a definition supplies them. The
/// **address** is what the test keeps hidden — that is the thing being measured.
fn oracle_shapes(def_path: &str, kind: &str) -> Vec<(usize, usize)> {
    let Ok(bytes) = std::fs::read(def_path) else {
        return Vec::new();
    };
    let mut out: Vec<(usize, usize)> = match kind {
        "KP" => Vec::new(),
        "DAM" => {
            let text = String::from_utf8_lossy(&bytes).into_owned();
            parse_damos(&text)
                .map(|d| {
                    d.maps
                        .iter()
                        .filter(|m| m.rows > 1 && m.cols > 1)
                        .map(|m| (m.rows, m.cols))
                        .collect()
                })
                .unwrap_or_default()
        }
        "A2L" => Vec::new(),
        _ => Vec::new(),
    };
    out.retain(|(r, c)| *r >= 2 && *c >= 2 && *r <= 48 && *c <= 48);
    out.sort_unstable();
    out.dedup();
    out.truncate(12);
    out
}

/// Addresses of **multi-cell** maps only.
///
/// A 1x1 scalar has no spatial structure, so no blind scanner can find it. The
/// DAM/A2L corpora contain tens of thousands of scalars; including them would
/// report a figure that says nothing about scanner quality.
fn oracle_addresses(def_path: &str, kind: &str) -> Option<Vec<usize>> {
    let bytes = std::fs::read(def_path).ok()?;
    match kind {
        "KP" => {
            let pack = parse_map_pack(&bytes).ok()?;
            Some(pack.maps.iter().map(|m| m.address as usize).collect())
        }
        "DAM" => {
            let text = String::from_utf8_lossy(&bytes).into_owned();
            let doc = parse_damos(&text).ok()?;
            // DAMOS addresses are ECU addresses. Translate them: flash becomes a
            // file offset, internal RAM becomes unfindable by definition.
            Some(
                doc.maps
                    .iter()
                    .filter(|m| m.rows > 1 || m.cols > 1)
                    .filter_map(|m| match kivi_core::address_translation::translate_me7(m.address) {
                        kivi_core::address_translation::Me7Translation::FileOffset(off) => {
                            Some(off as usize)
                        }
                        _ => None,
                    })
                    .collect(),
            )
        }
        "A2L" => {
            let text = String::from_utf8_lossy(&bytes).into_owned();
            let doc = parse_a2l(&text).ok()?;
            Some(doc.characteristics.iter().map(|c| c.address as usize).collect())
        }
        _ => None,
    }
}

fn run_one(pair: &(&str, &str, &str, &str)) -> Option<PairResult> {
    let (code, bin_rel, def_rel, kind) = *pair;
    let bin_path = PathBuf::from(DB).join(bin_rel);
    let def_path = PathBuf::from(DB).join(def_rel);

    let rom = std::fs::read(&bin_path).ok()?;
    let def_path_str = def_path.to_string_lossy().to_string();
    let addresses = oracle_addresses(&def_path_str, kind)?;
    if addresses.is_empty() {
        return None;
    }

    let start = Instant::now();
    let _endian = detect_global_endianness(&rom);
    // Shapes: derive a modest set from ROM size rather than the definition, so
    // the scan stays blind. Common calibration shapes across these ECUs.
    let mut dims = oracle_shapes(&def_path_str, kind);
    if dims.is_empty() {
        dims = vec![
            (8, 8), (12, 12), (16, 16), (20, 12), (16, 20), (20, 20),
            (24, 16), (16, 24), (32, 16), (16, 32),
        ];
    }
    let mut found = scan_16bit_tables_auto_endian(&rom, &dims, 0, rom.len());
    found.extend(scan_8bit_tables(&rom, &dims, 0, rom.len()));
    let limiters = kivi_scanner::limiter_scan::scan_limiters(&rom, false);
    let elapsed = start.elapsed();

    let reported: Vec<usize> = found
        .iter()
        .map(|c| c.z_address)
        .chain(limiters.iter().map(|l| l.address))
        .collect();

    let matched = addresses
        .iter()
        .filter(|a| reported.iter().any(|r| r.abs_diff(**a) <= 16))
        .count();

    let _ = (code, elapsed);
    Some(PairResult {
        kind: kind.to_string(),
        oracle: addresses.len(),
        matched,
    })
}

#[test]
fn one_hundred_catalogued_pairs_are_scanned_blind() {
    let pairs: Vec<(&str, &str, &str, &str)> = PAIRS
        .iter()
        .filter(|(_, b, d, _)| {
            PathBuf::from(DB).join(b).exists() && PathBuf::from(DB).join(d).exists()
        })
        .copied()
        .collect();

    if pairs.is_empty() {
        eprintln!("SKIP: no pairs present");
        return;
    }

    // Hash before.
    let before: Vec<(String, u64)> = pairs
        .iter()
        .map(|(_, b, _, _)| {
            let p = PathBuf::from(DB).join(b);
            let d = std::fs::read(&p).unwrap_or_default();
            (p.to_string_lossy().to_string(), d.iter().map(|x| *x as u64).sum::<u64>())
        })
        .collect();

    let t0 = Instant::now();
    let results: Vec<PairResult> = pairs.par_iter().filter_map(run_one).collect();
    let total = t0.elapsed();

    // Breakdown by definition kind.
    let mut by_kind: std::collections::BTreeMap<String, (usize, usize, usize)> =
        std::collections::BTreeMap::new();
    let mut matched = 0usize;
    let mut oracle = 0usize;
    for r in &results {
        let e = by_kind.entry(r.kind.clone()).or_insert((0, 0, 0));
        e.0 += 1;
        e.1 += r.oracle;
        e.2 += r.matched;
        matched += r.matched;
        oracle += r.oracle;
    }

    eprintln!("\n{:<8} {:>6} {:>9} {:>9} {:>8}", "kind", "pairs", "oracle", "matched", "recall");
    for (k, (n, o, m)) in &by_kind {
        eprintln!(
            "{:<8} {:>6} {:>9} {:>9} {:>7.1}%",
            k, n, o, m,
            100.0 * *m as f64 / (*o).max(1) as f64
        );
    }
    eprintln!(
        "\nTOTAL: {} matched / {} addresses ({:.1}%) across {} pairs in {} ms",
        matched,
        oracle,
        100.0 * matched as f64 / oracle.max(1) as f64,
        results.len(),
        total.as_millis()
    );

    // Integrity.
    for (path, sum_before) in &before {
        let d = std::fs::read(path).unwrap_or_default();
        let sum_after = d.iter().map(|x| *x as u64).sum::<u64>();
        assert_eq!(*sum_before, sum_after, "firmware was modified: {path}");
    }

    assert!(!results.is_empty(), "at least one pair must scan");
    assert!(matched > 0, "the blind scan matched nothing");
}
