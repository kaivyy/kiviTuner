//! Audit of blind discovery (`blind_discover_v2`) against newest-generation
//! vehicle firmware binaries and BIN+XDF oracle pairs in `/root/ecu_master_database`.
//!
//! Reads ROMs strictly read-only, verifies byte-sum immutability before/after,
//! attributes misses by structural reason, and checks Fisher-Yates Twin-ROM SNR.

use kivi_scanner::blind_automap::default_shapes;
use kivi_scanner::blind_v2::{blind_discover_v2, BlindV2Candidate, BlindV2Options};
use kivi_scanner::xdf_parse::parse_xdf_xml;
use kivi_scanner::zero_xdf_prefilter::analyze_rom_blocks;
use kivi_scanner::zero_xdf_scan::detect_global_endianness;
use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::time::Instant;

const DB: &str = "/root/ecu_master_database";

const MODERN_ORACLE_PAIRS: &[(&str, &str, &str)] = &[
    (
        "Audi_RS3_TTRS_MED17_1_62",
        "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_TTRS_RS3_MED17_1_62/MED17_1_62_8S0907404L__0001_STOCK.bin",
        "02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_TTRS_RS3_MED17_1_62/404L_normal.xdf",
    ),
    (
        "BMW_S58_G80_MG1CS024",
        "02_CARS/BMW/S58_M3_M4_G80_G82_A2L_XDF/00005C64144606_original.bin",
        "02_CARS/BMW/S58_M3_M4_G80_G82_A2L_XDF/00005C64144606.xdf",
    ),
    (
        "BMW_B58_Gen2_MG1CS201",
        "02_CARS/BMW/B48_B58_GEN1_GEN2_A2L_XDF/00005D55289606_MapSwitchBase.bin",
        "02_CARS/BMW/B48_B58_GEN1_GEN2_A2L_XDF/00005D55289606.xdf",
    ),
    (
        "BMW_S63_F90_M5_MG1",
        "02_CARS/BMW/N63_S63_TWIN_TURBO_V8_A2L_XDF/00004ACF287806_original.bin",
        "02_CARS/BMW/N63_S63_TWIN_TURBO_V8_A2L_XDF/00004ACF287806.xdf",
    ),
    (
        "BMW_B58_Gen1_MG1CS003",
        "02_CARS/BMW/MG_FLASHER_B58_B48_S58_S55_XDFS/B48 B58 Gen 1/BMW_B58_3.0T_BOSCH_MG1CS003_10SW002828_iRom_original.bin",
        "02_CARS/BMW/MG_FLASHER_B58_B48_S58_S55_XDFS/B48 B58 Gen 1/BMW_B58_MG1CS003_R0C9J772B.xdf",
    ),
    (
        "BMW_S55_F80_MEVD17",
        "02_CARS/BMW/BMW_F_SERIES_N55_S55_N13_MEVD17/000021571DAA01_MapSwitchBase.bin",
        "02_CARS/BMW/BMW_F_SERIES_N55_S55_N13_MEVD17/000021571DAA01.xdf",
    ),
    (
        "Suzuki_Hayabusa_Gen3_48L00",
        "01_MOTORCYCLES/SUZUKI/BIG_BIKE_GSXR_HAYABUSA/32990-48L00.bin",
        "01_MOTORCYCLES/SUZUKI/BIG_BIKE_GSXR_HAYABUSA/32990-48L00.xdf",
    ),
];

const MODERN_STANDALONE_ROMS: &[(&str, &str)] = &[
    (
        "VW_Golf7_GTI_Simos18_1",
        "02_CARS/VAG_AUDI_VOLKSWAGEN/VAG_TDI_EDC15_EDC16_ORIGINAL_BINS/VW Golf VII GTI 2.0TFSI Simos18.1 5G0906259 0005 SC800F8300000.ori",
    ),
    (
        "Ford_Ranger_2_2_SID209",
        "02_CARS/FORD/FORD_RANGER_EVEREST_FOCUS_COMMUNITY_DUMPS/FORD_RANGER_T6_2016_2.2L_TDCI_CONTINENTAL_SID209.ori",
    ),
    (
        "Mazda_2_Skyactiv_D",
        "02_CARS/MAZDA/MAZDA_2_SKYACTIV_D_AND_MZR/MAZDA_2_GEN3_2015_1.5L_DIESEL_SKYACTIV_D_105HP_ORIGINAL_READ.ori",
    ),
    (
        "GM_Sierra_2018_E92A",
        "02_CARS/GM_CHEVROLET/DURAMAX_DIESEL_AND_E38/E92A-2018-Sierra-5.3-auto-ECM.bin",
    ),
    (
        "Audi_Q5_2_0TDI_EDC17",
        "02_CARS/VAG_AUDI_VOLKSWAGEN/VAG_TDI_EDC15_EDC16_ORIGINAL_BINS/03L906022NH-1037399332-Q5-2,0 edc17.bin",
    ),
    (
        "Toyota_Yaris_Gen3_1NZ",
        "02_CARS/TOYOTA_DAIHATSU/YARIS_VIOS_1NZ_2NR/TOYOTA_VIOS_YARIS_GEN3_2014_1.5L_1NZFE_AUTO_89663-52D30.bin",
    ),
    (
        "Hyundai_Kia_Kefico_2026",
        "02_CARS/HYUNDAI_KIA/KIA_PICANTO_SHUMA_SPECTRA_GETZ_KEFICO_M7/ca661016_E8H0SA5A_KR77025272_KR77025179_2026-01-13_1748.bin",
    ),
    (
        "Yamaha_MT15_VVA_2024",
        "01_MOTORCYCLES/YAMAHA/SPORT_MT15_R15/YAMAHA_MT_15_VVA_2022-2024_BHM0.bin",
    ),
];

struct SplitMix64(u64);

impl SplitMix64 {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

fn fisher_yates_word_twin(rom: &[u8], kept_ranges: &[(usize, usize)], seed: u64) -> Vec<u8> {
    let mut twin = rom.to_vec();
    let mut rng = SplitMix64(seed);
    for &(s, e) in kept_ranges {
        let aligned_s = s + (s & 1);
        let aligned_e = e - (e & 1);
        if aligned_e <= aligned_s + 4 {
            continue;
        }
        let n_words = (aligned_e - aligned_s) / 2;
        for i in (1..n_words).rev() {
            let j = (rng.next_u64() as usize) % (i + 1);
            if i != j {
                let a = aligned_s + i * 2;
                let b = aligned_s + j * 2;
                twin.swap(a, b);
                twin.swap(a + 1, b + 1);
            }
        }
    }
    twin
}

fn byte_sum(data: &[u8]) -> u64 {
    data.iter().map(|&b| b as u64).sum()
}

fn top_sources(cands: &[&BlindV2Candidate], limit: usize) -> Vec<(String, usize)> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for c in cands {
        *counts.entry(c.detection_source.clone()).or_default() += 1;
    }
    let mut vec: Vec<(String, usize)> = counts.into_iter().collect();
    vec.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    vec.truncate(limit);
    vec
}

#[derive(Clone, Copy)]
struct OracleEntry {
    addr: usize,
    rows: usize,
    cols: usize,
    is_2d: bool,
    is_varying: bool,
    is_signed: bool,
}

fn near_sorted(sorted: &[usize], target: usize, tol: usize) -> bool {
    let idx = sorted.partition_point(|&a| a < target);
    let lo = idx.saturating_sub(4);
    let hi = (idx + 4).min(sorted.len());
    sorted[lo..hi].iter().any(|a| a.abs_diff(target) <= tol)
}

fn addr_in_any_span(spans: &[(usize, usize)], target: usize) -> bool {
    let idx = spans.partition_point(|&(s, _)| s <= target);
    let lo = idx.saturating_sub(8);
    let hi = (idx + 8).min(spans.len());
    spans[lo..hi].iter().any(|&(s, e)| target >= s && target < e)
}

#[test]
fn audit_modern_vehicle_bin_xdf_oracle_distribution() {
    let available: Vec<(&str, String, String)> = MODERN_ORACLE_PAIRS
        .iter()
        .filter_map(|&(name, bin_rel, xdf_rel)| {
            let bp = format!("{DB}/{bin_rel}");
            let xp = format!("{DB}/{xdf_rel}");
            (Path::new(&bp).exists() && Path::new(&xp).exists()).then_some((name, bp, xp))
        })
        .collect();

    if available.is_empty() {
        eprintln!("SKIP: modern vehicle BIN+XDF corpus not present under {DB}");
        return;
    }

    let shape_set: HashSet<(usize, usize)> = default_shapes().into_iter().collect();
    let opts = BlindV2Options {
        include_curves: true,
        ..BlindV2Options::default()
    };

    let before_hashes: Vec<(String, u64)> = available
        .iter()
        .map(|(_, bp, _)| {
            let bytes = std::fs::read(bp).expect("read oracle ROM");
            (bp.clone(), byte_sum(&bytes))
        })
        .collect();

    eprintln!(
        "\n=== MODERN VEHICLE BLIND AUDIT (Oracle BIN+XDF Pairs: {}) ===",
        available.len()
    );
    eprintln!(
        "{:<26} {:>6} {:>13} {:>13} {:>13} {:>13} {:>7} {:>7} {:>6}",
        "ECU", "KB", "2D_All(16B)", "2D_Var(16/2)", "1D_All(16B)", "1D_Var(16/2)", "Cand2D", "Cand1D", "ms"
    );

    let mut tot_2d_all = 0usize;
    let mut hit_2d_all = 0usize;
    let mut tot_2d_var = 0usize;
    let mut hit_2d_var_16 = 0usize;
    let mut hit_2d_var_2 = 0usize;
    let mut tot_1d_all = 0usize;
    let mut hit_1d_all = 0usize;
    let mut tot_1d_var = 0usize;
    let mut hit_1d_var_16 = 0usize;
    let mut hit_1d_var_2 = 0usize;

    for (name, bin_path, xdf_path) in &available {
        let rom = std::fs::read(bin_path).expect("read ROM");
        let xdf_bytes = std::fs::read(xdf_path).expect("read XDF");
        let xdf_text = String::from_utf8_lossy(&xdf_bytes).into_owned();
        let parsed = parse_xdf_xml(&xdf_text).expect("parse XDF");

        let mut entries = Vec::new();
        for t in &parsed.tables {
            let rows = t.y_axis.as_ref().map(|a| a.index_count).unwrap_or(1);
            let cols = t.x_axis.as_ref().map(|a| a.index_count).unwrap_or(1);
            if rows < 2 && cols < 2 {
                continue;
            }
            let elem_bytes = (t.z_axis.element_size_bits.max(8)) / 8;
            let Some(span) = rows.checked_mul(cols).and_then(|n| n.checked_mul(elem_bytes)) else {
                continue;
            };
            let addr = t.z_axis.address;
            if span == 0 || addr.saturating_add(span) > rom.len() {
                continue;
            }
            let slice = &rom[addr..addr + span];
            let first = slice[0];
            let is_varying = slice.iter().any(|&b| b != first);
            entries.push(OracleEntry {
                addr,
                rows,
                cols,
                is_2d: rows >= 2 && cols >= 2,
                is_varying,
                is_signed: (t.z_axis.type_flags & 1) != 0,
            });
        }

        let t0 = Instant::now();
        let cands = blind_discover_v2(&rom, &opts);
        let elapsed_ms = t0.elapsed().as_millis();

        let cand_2d: Vec<&BlindV2Candidate> = cands.iter().filter(|c| !c.is_curve).collect();
        let cand_1d: Vec<&BlindV2Candidate> = cands.iter().filter(|c| c.is_curve).collect();

        let mut addrs_2d: Vec<usize> = cand_2d.iter().map(|c| c.address).collect();
        addrs_2d.sort_unstable();
        let mut addrs_1d: Vec<usize> = cand_1d.iter().map(|c| c.address).collect();
        addrs_1d.sort_unstable();
        let mut all_addrs: Vec<usize> = cands.iter().map(|c| c.address).collect();
        all_addrs.sort_unstable();

        let mut spans: Vec<(usize, usize)> = cands
            .iter()
            .map(|c| {
                let len = c.rows.saturating_mul(c.cols).saturating_mul(c.elem_bytes);
                (c.address, c.address.saturating_add(len))
            })
            .collect();
        spans.sort_unstable();

        let mut e2_all = 0usize;
        let mut h2_all = 0usize;
        let mut e2_var = 0usize;
        let mut h2_var_16 = 0usize;
        let mut h2_var_2 = 0usize;
        let mut e1_all = 0usize;
        let mut h1_all = 0usize;
        let mut e1_var = 0usize;
        let mut h1_var_16 = 0usize;
        let mut h1_var_2 = 0usize;

        let mut miss_covered_2d = 0usize;
        let mut miss_shape_2d = 0usize;
        let mut miss_signed_2d = 0usize;
        let mut miss_other_2d = 0usize;
        let mut miss_covered_1d = 0usize;
        let mut miss_short_1d = 0usize;
        let mut miss_signed_1d = 0usize;
        let mut miss_other_1d = 0usize;

        for e in &entries {
            let hit_16 = near_sorted(&all_addrs, e.addr, 16);
            let hit_2 = near_sorted(&all_addrs, e.addr, 2);
            if e.is_2d {
                e2_all += 1;
                if hit_16 {
                    h2_all += 1;
                }
                if e.is_varying {
                    e2_var += 1;
                    if hit_16 {
                        h2_var_16 += 1;
                    }
                    if hit_2 {
                        h2_var_2 += 1;
                    }
                    if !hit_16 {
                        if addr_in_any_span(&spans, e.addr) {
                            miss_covered_2d += 1;
                        } else if !shape_set.contains(&(e.rows, e.cols))
                            && !shape_set.contains(&(e.cols, e.rows))
                        {
                            miss_shape_2d += 1;
                        } else if e.is_signed {
                            miss_signed_2d += 1;
                        } else {
                            miss_other_2d += 1;
                        }
                    }
                }
            } else {
                e1_all += 1;
                if hit_16 {
                    h1_all += 1;
                }
                if e.is_varying {
                    e1_var += 1;
                    if hit_16 {
                        h1_var_16 += 1;
                    }
                    if hit_2 {
                        h1_var_2 += 1;
                    }
                    if !hit_16 {
                        let pts = e.rows.max(e.cols);
                        if addr_in_any_span(&spans, e.addr) {
                            miss_covered_1d += 1;
                        } else if pts < 8 {
                            miss_short_1d += 1;
                        } else if e.is_signed {
                            miss_signed_1d += 1;
                        } else {
                            miss_other_1d += 1;
                        }
                    }
                }
            }
        }

        let pct = |h: usize, t: usize| -> f64 {
            if t == 0 {
                0.0
            } else {
                100.0 * h as f64 / t as f64
            }
        };

        eprintln!(
            "{:<26} {:>6} {:>4}/{:<4}({:>4.0}%) {:>4}/{:<4}({:>3.0}%) {:>4}/{:<4}({:>4.0}%) {:>4}/{:<4}({:>3.0}%) {:>7} {:>7} {:>6}",
            name,
            rom.len() / 1024,
            h2_all,
            e2_all,
            pct(h2_all, e2_all),
            h2_var_16,
            e2_var,
            pct(h2_var_2, e2_var),
            h1_all,
            e1_all,
            pct(h1_all, e1_all),
            h1_var_16,
            e1_var,
            pct(h1_var_2, e1_var),
            cand_2d.len(),
            cand_1d.len(),
            elapsed_ms
        );
        eprintln!(
            "  2D_exact(<=2B)={h2_var_2}/{e2_var} | 1D_exact(<=2B)={h1_var_2}/{e1_var} | 2D_miss[cov={miss_covered_2d} shape={miss_shape_2d} sig={miss_signed_2d} other={miss_other_2d}] | 1D_miss[cov={miss_covered_1d} lt8={miss_short_1d} sig={miss_signed_1d} other={miss_other_1d}]"
        );
        eprintln!(
            "  top_2D_src={:?} | top_1D_src={:?}",
            top_sources(&cand_2d, 4),
            top_sources(&cand_1d, 4)
        );

        tot_2d_all += e2_all;
        hit_2d_all += h2_all;
        tot_2d_var += e2_var;
        hit_2d_var_16 += h2_var_16;
        hit_2d_var_2 += h2_var_2;
        tot_1d_all += e1_all;
        hit_1d_all += h1_all;
        tot_1d_var += e1_var;
        hit_1d_var_16 += h1_var_16;
        hit_1d_var_2 += h1_var_2;

        let _ = (&addrs_2d, &addrs_1d);
        assert!(
            h2_all + h1_all > 0,
            "{name}: blind_discover_v2 matched 0 oracle addresses"
        );
    }

    eprintln!(
        "\nAGGREGATE ORACLE: 2D_All={hit_2d_all}/{tot_2d_all} ({:.1}%) | 2D_Var<=16B={hit_2d_var_16}/{tot_2d_var} ({:.1}%) <=2B={hit_2d_var_2}/{tot_2d_var} ({:.1}%) | 1D_All={hit_1d_all}/{tot_1d_all} ({:.1}%) | 1D_Var<=16B={hit_1d_var_16}/{tot_1d_var} ({:.1}%) <=2B={hit_1d_var_2}/{tot_1d_var} ({:.1}%)",
        100.0 * hit_2d_all as f64 / tot_2d_all.max(1) as f64,
        100.0 * hit_2d_var_16 as f64 / tot_2d_var.max(1) as f64,
        100.0 * hit_2d_var_2 as f64 / tot_2d_var.max(1) as f64,
        100.0 * hit_1d_all as f64 / tot_1d_all.max(1) as f64,
        100.0 * hit_1d_var_16 as f64 / tot_1d_var.max(1) as f64,
        100.0 * hit_1d_var_2 as f64 / tot_1d_var.max(1) as f64,
    );

    for (bp, before_sum) in &before_hashes {
        let after_sum = byte_sum(&std::fs::read(bp).expect("re-read ROM"));
        assert_eq!(*before_sum, after_sum, "ROM was mutated: {bp}");
    }
}

#[test]
fn audit_modern_standalone_multibrand_roms_and_null_snr() {
    let available: Vec<(&str, String)> = MODERN_STANDALONE_ROMS
        .iter()
        .filter_map(|&(name, rel)| {
            let p = format!("{DB}/{rel}");
            Path::new(&p).exists().then_some((name, p))
        })
        .collect();

    if available.is_empty() {
        eprintln!("SKIP: modern standalone ROMs not present under {DB}");
        return;
    }

    let opts = BlindV2Options {
        include_curves: true,
        ..BlindV2Options::default()
    };

    let before_hashes: Vec<(String, u64)> = available
        .iter()
        .map(|(_, p)| (p.clone(), byte_sum(&std::fs::read(p).expect("read ROM"))))
        .collect();

    eprintln!(
        "\n=== MODERN STANDALONE MULTI-BRAND ROMS & TWIN-ROM SNR ({}) ===",
        available.len()
    );
    eprintln!(
        "{:<24} {:>6} {:>12} {:>7} {:>7} {:>6} {:>7} {:>7} {:>6} {:>7}",
        "ROM", "KB", "Endian", "Real2D", "Null2D", "SNR2D", "Real1D", "Null1D", "SNR1D", "RealMs"
    );

    for (idx, (name, path)) in available.iter().enumerate() {
        let rom = std::fs::read(path).expect("read standalone ROM");
        let endian = detect_global_endianness(&rom);
        let pre = analyze_rom_blocks(&rom);
        let twin = fisher_yates_word_twin(&rom, &pre.kept_ranges, 0x5EED_CAFE_0000 + idx as u64);

        let t0 = Instant::now();
        let real_cands = blind_discover_v2(&rom, &opts);
        let real_ms = t0.elapsed().as_millis();
        let null_cands = blind_discover_v2(&twin, &opts);

        let real_2d: Vec<&BlindV2Candidate> = real_cands.iter().filter(|c| !c.is_curve).collect();
        let real_1d: Vec<&BlindV2Candidate> = real_cands.iter().filter(|c| c.is_curve).collect();
        let null_2d: Vec<&BlindV2Candidate> = null_cands.iter().filter(|c| !c.is_curve).collect();
        let null_1d: Vec<&BlindV2Candidate> = null_cands.iter().filter(|c| c.is_curve).collect();

        let snr_2d = (real_2d.len() as f64) / (null_2d.len().max(1) as f64);
        let snr_1d = (real_1d.len() as f64) / (null_1d.len().max(1) as f64);

        eprintln!(
            "{:<24} {:>6} {:>12} {:>7} {:>7} {:>5.1}x {:>7} {:>7} {:>5.1}x {:>7}",
            name,
            rom.len() / 1024,
            format!("{endian:?}"),
            real_2d.len(),
            null_2d.len(),
            snr_2d,
            real_1d.len(),
            null_1d.len(),
            snr_1d,
            real_ms
        );
        eprintln!(
            "  top_2D_src={:?} | top_1D_src={:?}",
            top_sources(&real_2d, 4),
            top_sources(&real_1d, 4)
        );

        assert!(
            !real_2d.is_empty(),
            "{name}: expected real_2d > 0 on modern ROM"
        );
        assert!(
            snr_2d >= 3.0 || (real_2d.len() > null_2d.len() * 3),
            "{name}: 2D SNR below 3.0x ({snr_2d:.2}x, real={}, null={})",
            real_2d.len(),
            null_2d.len()
        );
    }

    for (p, before_sum) in &before_hashes {
        let after_sum = byte_sum(&std::fs::read(p).expect("re-read ROM"));
        assert_eq!(*before_sum, after_sum, "ROM was mutated: {p}");
    }
}
