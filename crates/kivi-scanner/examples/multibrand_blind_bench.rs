use kivi_core::types::DType;
use kivi_defs::model::MapDef;
use kivi_defs::ecu_xml::parse_ecu_xml;
use kivi_defs::xdf::parse_xdf;
use kivi_scanner::blind_v2::{blind_discover_v2, BlindV2Options};
use kivi_scanner::kp_parse::{parse_kp_any, KpParsedAny};
use std::time::Instant;

struct OracleTable {
    address: usize,
    rows: usize,
    cols: usize,
    byte_len: usize,
}

#[allow(dead_code)]
enum DefSource {
    Xdf(&'static str),
    SubaruXml { base: &'static str, leaf: &'static str },
    MazdaRx8Xml { path: &'static str, target_id: &'static str },
    KpPack(&'static str),
}

struct Case {
    label: &'static str,
    bin_path: &'static str,
    def: DefSource,
    sig_ascii: &'static str,
}

const CASES: &[Case] = &[
    Case {
        label: "ALFA_155_M17_32K",
        bin_path: "/root/ecu_master_database/02_CARS/ALFA_ROMEO/BOSCH_EDC15C_EDC16C39_ORIGINAL_DUMPS/Alfa 155 1.8TS 144HP 0261200536 357963.ori",
        def: DefSource::Xdf("/root/ecu_master_database/02_CARS/ALFA_ROMEO/ALFA_COMMUNITY_TUNES_SNOMAN/155_1.8_TS_0261200536_1267357963.xdf"),
        sig_ascii: "6350021620",
    },
    Case {
        label: "BMW_E46_MSS54_32K",
        bin_path: "/root/ecu_master_database/02_CARS/BMW/BMW_E46_M3_S54B32_MSS54_CSL_SUITE/211322001501J4E9.bin",
        def: DefSource::Xdf("/root/ecu_master_database/02_CARS/BMW/BMW_E46_M3_S54B32_MSS54_CSL_SUITE/hw1501_V4_Complete.xdf"),
        sig_ascii: "211322001501J4E9",
    },
    Case {
        label: "VW_ABA_OBD1_64K",
        bin_path: "/root/ecu_database/honda/hondabase_archive/cobrartp/xdf&bin (2)/bins/Volkswagen/0261203219_soft109.bin",
        def: DefSource::Xdf("/root/ecu_database/honda/hondabase_archive/cobrartp/xdf&bin (2)/bindefs/Volkswagen/VW_OBD1_ABA.xdf"),
        sig_ascii: "203219",
    },
    Case {
        label: "DUCATI_900SS_128K",
        bin_path: "/root/ecu_master_database/01_MOTORCYCLES/DUCATI/DUCATI_900SS_MARELLI_M153/AM28F010_DUCATI_900_SS_EU.bin",
        def: DefSource::Xdf("/root/ecu_master_database/01_MOTORCYCLES/DUCATI/DUCATI_900SS_MARELLI_M153/Ducati_M15_3.xdf"),
        sig_ascii: "DUCATI",
    },
    Case {
        label: "FORD_BARRA_256K",
        bin_path: "/root/ecu_master_database/02_CARS/FORD/FALCON_BARRA_INTECH_6CYL/12AF_NGIC9A5.256k.bin",
        def: DefSource::Xdf("/root/ecu_master_database/02_CARS/FORD/FALCON_BARRA_INTECH_6CYL/NGIC9 Extended v1.3.xdf"),
        sig_ascii: "NGIC9",
    },
    Case {
        label: "HONDA_CBR1000RR_256K",
        bin_path: "/root/ecu_master_database/01_MOTORCYCLES/HONDA/CBR_SPORT/CBR1000RR_FIREBLADE/38770-MEL-613.bin",
        def: DefSource::Xdf("/root/ecu_master_database/01_MOTORCYCLES/HONDA/CBR_SPORT/CBR1000RR_FIREBLADE/38770-MEL.xdf"),
        sig_ascii: "",
    },
    Case {
        label: "HONDA_CBR250RR_1M",
        bin_path: "/root/ecu_master_database/01_MOTORCYCLES/HONDA/SPORT_CBR250RR/CBR250RR-38770-K64-ND1_BIN.bin",
        def: DefSource::Xdf("/root/ecu_master_database/01_MOTORCYCLES/HONDA/SPORT_CBR250RR/CBR250RR_38770-K64-ND1_PUBLIC_v1.xdf"),
        sig_ascii: "",
    },
    Case {
        label: "MAZDA_RX8_XML_512K",
        bin_path: "/root/ecu_master_database/02_CARS/MAZDA/MAZDA_RX8_RENESIS_13B/N3ZHEC00013H6060_V2.bin",
        def: DefSource::MazdaRx8Xml {
            path: "/root/ecu_master_database/02_CARS/MAZDA/MAZDA_RX8_RENESIS_13B/rx8_defs_N3ZHEC_fixed.xml",
            target_id: "60E0G500",
        },
        sig_ascii: "60E0G500",
    },
    Case {
        label: "GM_LS1_P01_512K",
        bin_path: "/root/ecu_master_database/02_CARS/GM_CHEVROLET/GM_GEN4_GEN5_E38_E67_E78_E92_LBZ/12212156_1G1YY22G6X5111143.bin",
        def: DefSource::Xdf("/root/ecu_master_database/02_CARS/GM_CHEVROLET/GM_GEN4_GEN5_E38_E67_E78_E92_LBZ/No Password - OS12212156 - 2002 512kB PCM - v5_old.xdf"),
        sig_ascii: "12212156",
    },
    Case {
        label: "GM_LQ4_P59_1M",
        bin_path: "/root/ecu_master_database/02_CARS/GM_CHEVROLET/DURAMAX_DIESEL_AND_E38/04_hummer_lq4.bin",
        def: DefSource::Xdf("/root/ecu_master_database/02_CARS/GM_CHEVROLET/DURAMAX_DIESEL_AND_E38/12587603_RC1.xdf"),
        sig_ascii: "12587603",
    },
    Case {
        label: "AUDI_S4_ME7_1M",
        bin_path: "/root/ecu_master_database/02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_B5_S4_A4_18T_ME7/audi-s4-b5-27t-8d0907551m-stock.bin",
        def: DefSource::Xdf("/root/ecu_master_database/02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_B5_S4_A4_18T_ME7/8D0907551M-20190711.xdf"),
        sig_ascii: "8D0907551M",
    },
    Case {
        label: "VOLVO_V70_ME7_1M",
        bin_path: "/root/ecu_master_database/02_CARS/VOLVO_SAAB/VOLVO_ME7_T5_R/20FWHJ-81FWOVD0-MAN-FWD-EU.bin",
        def: DefSource::Xdf("/root/ecu_master_database/02_CARS/VOLVO_SAAB/VOLVO_ME7_T5_R/20FWHJ.xdf"),
        sig_ascii: "20FWHJ",
    },
    Case {
        label: "SUZUKI_GSXR_2M",
        bin_path: "/root/ecu_master_database/01_MOTORCYCLES/SUZUKI/BIG_BIKE_GSXR_HAYABUSA/32990-10L1x - 5JCZSJ10.bin",
        def: DefSource::Xdf("/root/ecu_master_database/01_MOTORCYCLES/SUZUKI/BIG_BIKE_GSXR_HAYABUSA/5JCZSJB0.xdf"),
        sig_ascii: "5JCZSJ10",
    },
    Case {
        label: "HYUNDAI_SIM2K_2.5M",
        bin_path: "/root/ecu_master_database/02_CARS/HYUNDAI_KIA/SIM2K_250_HYUNDAI_N_SERIES/Original [CPQWDGMCXXXSDU50D].bin",
        def: DefSource::Xdf("/root/ecu_master_database/02_CARS/HYUNDAI_KIA/SIM2K_250_HYUNDAI_N_SERIES/CPQWDGMCXXXSDU50D.xdf"),
        sig_ascii: "CPQWDGMC",
    },
    Case {
        label: "BMW_B58_MG1_7.5M",
        bin_path: "/root/ecu_master_database/02_CARS/BMW/B48_B58_GEN1_GEN2_A2L_XDF/R1C9J8I2B_original+stock_multimap_data_CC6.9.bin",
        def: DefSource::Xdf("/root/ecu_master_database/02_CARS/BMW/B48_B58_GEN1_GEN2_A2L_XDF/R1C9J8I2B_3076-080_029_002_v7.2.2.xdf"),
        sig_ascii: "R1C9J8I2B",
    },
];

fn dtype_bytes(d: DType) -> usize {
    match d {
        DType::U8 | DType::I8 => 1,
        DType::U16 | DType::I16 => 2,
        DType::U32 | DType::I32 | DType::F32 => 4,
    }
}

fn from_mapdefs(maps: Vec<MapDef>, rom_len: usize) -> Vec<OracleTable> {
    maps.into_iter()
        .filter_map(|m| {
            let bpp = dtype_bytes(m.dtype);
            let byte_len = m.rows.saturating_mul(m.cols).saturating_mul(bpp);
            if (m.rows >= 2 || m.cols >= 2) && m.address.saturating_add(byte_len) <= rom_len {
                Some(OracleTable {
                    address: m.address,
                    rows: m.rows,
                    cols: m.cols,
                    byte_len,
                })
            } else {
                None
            }
        })
        .collect()
}

fn extract_rom_block(xml: &str, needle: &str) -> Option<String> {
    let pos = xml.find(needle)?;
    let start = match (xml[..pos].rfind("<rom>"), xml[..pos].rfind("<rom ")) {
        (Some(a), Some(b)) => a.max(b),
        (Some(a), None) => a,
        (None, Some(b)) => b,
        (None, None) => return None,
    };
    let end_rel = xml[pos..].find("</rom>")?;
    Some(xml[start..pos + end_rel + "</rom>".len()].to_string())
}

fn xml_attr(tag: &str, attr: &str) -> Option<String> {
    let pat = format!("{attr}=\"");
    let start = tag.find(&pat)? + pat.len();
    let end = tag[start..].find('"')?;
    Some(tag[start..start + end].to_string())
}

fn load_subaru_xml(base_path: &str, leaf_path: &str, rom_len: usize) -> Vec<OracleTable> {
    use std::collections::HashMap;
    let base_txt = std::fs::read_to_string(base_path).unwrap_or_default();
    let leaf_txt = std::fs::read_to_string(leaf_path).unwrap_or_default();

    let mut scale_bpp: HashMap<String, usize> = HashMap::new();
    for line in base_txt.lines().chain(leaf_txt.lines()) {
        let t = line.trim();
        if t.starts_with("<scaling ") {
            if let (Some(name), Some(st)) = (xml_attr(t, "name"), xml_attr(t, "storagetype")) {
                let bpp = match st.as_str() {
                    "uint8" | "int8" => 1,
                    "uint16" | "int16" => 2,
                    "float" | "uint32" | "int32" => 4,
                    _ => continue,
                };
                scale_bpp.insert(name, bpp);
            }
        }
    }

    #[derive(Clone, Default)]
    struct BaseSpec {
        is_3d: bool,
        bpp: usize,
        cols: usize,
        rows: usize,
    }
    let mut base_specs: HashMap<String, BaseSpec> = HashMap::new();
    let mut cur_name: Option<String> = None;
    let mut cur_spec = BaseSpec::default();
    for line in base_txt.lines() {
        let t = line.trim();
        if t.starts_with("<table ") {
            let kind = xml_attr(t, "type").unwrap_or_default();
            if let Some(ref mut spec) = cur_spec.bpp.ne(&0).then_some(&mut cur_spec).filter(|_| cur_name.is_some()) {
                let _ = spec;
            }
            if kind == "X Axis" && cur_name.is_some() {
                if let Some(n) = xml_attr(t, "elements").and_then(|s| s.parse().ok()) {
                    cur_spec.cols = n;
                }
            } else if kind == "Y Axis" && cur_name.is_some() {
                if let Some(n) = xml_attr(t, "elements").and_then(|s| s.parse().ok()) {
                    if cur_spec.is_3d {
                        cur_spec.rows = n;
                    } else {
                        cur_spec.cols = cur_spec.cols.max(n);
                        cur_spec.rows = 1;
                    }
                }
            } else if matches!(kind.as_str(), "2D" | "3D") {
                if let Some(name) = cur_name.take() {
                    base_specs.insert(name, cur_spec.clone());
                }
                let sc = xml_attr(t, "scaling").unwrap_or_default();
                let bpp = scale_bpp.get(&sc).copied().unwrap_or(2);
                cur_name = xml_attr(t, "name");
                cur_spec = BaseSpec {
                    is_3d: kind == "3D",
                    bpp,
                    cols: 1,
                    rows: 1,
                };
                if t.ends_with("/>") {
                    if let Some(name) = cur_name.take() {
                        base_specs.insert(name, cur_spec.clone());
                    }
                }
            }
        } else if t.starts_with("</table>") {
            if let Some(name) = cur_name.take() {
                base_specs.insert(name, cur_spec.clone());
            }
        }
    }

    let mut out = Vec::new();
    let mut cur_leaf_name: Option<String> = None;
    let mut cur_addr: Option<usize> = None;
    let mut cur_rows = 1usize;
    let mut cur_cols = 1usize;
    let mut cur_bpp = 2usize;
    let mut cur_is_3d = false;
    let flush = |addr: Option<usize>, r: usize, c: usize, bpp: usize, out: &mut Vec<OracleTable>| {
        if let Some(a) = addr {
            let byte_len = r.saturating_mul(c).saturating_mul(bpp);
            if (r >= 2 || c >= 2) && a.saturating_add(byte_len) <= rom_len {
                out.push(OracleTable {
                    address: a,
                    rows: r,
                    cols: c,
                    byte_len,
                });
            }
        }
    };
    for line in leaf_txt.lines() {
        let t = line.trim();
        if t.starts_with("<table ") {
            let tname = xml_attr(t, "name").unwrap_or_default();
            if cur_leaf_name.is_some() && matches!(tname.as_str(), "X" | "Y" | "X Axis" | "Y Axis") {
                if let Some(n) = xml_attr(t, "elements").and_then(|s| s.parse().ok()) {
                    if tname.starts_with('X') {
                        cur_cols = n;
                    } else if cur_is_3d {
                        cur_rows = n;
                    } else {
                        cur_cols = n;
                        cur_rows = 1;
                    }
                }
            } else {
                flush(cur_addr.take(), cur_rows, cur_cols, cur_bpp, &mut out);
                let spec = base_specs.get(&tname).cloned().unwrap_or_default();
                let addr = xml_attr(t, "address")
                    .and_then(|s| usize::from_str_radix(s.trim_start_matches("0x"), 16).ok());
                cur_rows = spec.rows.max(1);
                cur_cols = spec.cols.max(1);
                cur_bpp = spec.bpp.max(2);
                cur_is_3d = spec.is_3d;
                cur_leaf_name = Some(tname);
                if t.ends_with("/>") {
                    flush(addr, cur_rows, cur_cols, cur_bpp, &mut out);
                    cur_leaf_name = None;
                } else {
                    cur_addr = addr;
                }
            }
        } else if t.starts_with("</table>") {
            flush(cur_addr.take(), cur_rows, cur_cols, cur_bpp, &mut out);
            cur_leaf_name = None;
        }
    }
    flush(cur_addr.take(), cur_rows, cur_cols, cur_bpp, &mut out);
    out
}

fn load_oracle(def: &DefSource, rom_len: usize) -> Vec<OracleTable> {
    match def {
        DefSource::Xdf(path) => {
            let bytes = std::fs::read(path).expect("read xdf");
            let text = String::from_utf8_lossy(&bytes);
            from_mapdefs(parse_xdf(&text).unwrap_or_default(), rom_len)
        }
        DefSource::SubaruXml { base, leaf } => load_subaru_xml(base, leaf, rom_len),
        DefSource::MazdaRx8Xml { path, target_id } => {
            let raw = std::fs::read_to_string(path).unwrap_or_default();
            let base_blk = extract_rom_block(&raw, "<xmlid>RX8Base</xmlid>").unwrap_or_default();
            let tgt_needle = format!("<xmlid>{target_id}</xmlid>");
            let tgt_blk = extract_rom_block(&raw, &tgt_needle)
                .unwrap_or_default()
                .replace("base=\"Rx8Base\"", "base=\"RX8Base\"");
            let wrapped = format!("<roms>\n{base_blk}\n{tgt_blk}\n</roms>");
            match parse_ecu_xml(&wrapped) {
                Ok(res) => from_mapdefs(res.definitions.maps, rom_len),
                Err(e) => {
                    eprintln!("Mazda XML err: {e}");
                    Vec::new()
                }
            }
        }
        DefSource::KpPack(path) => {
            let data = std::fs::read(path).expect("read kp");
            match parse_kp_any(&data) {
                Ok(KpParsedAny::Binary(f)) => f
                    .maps
                    .into_iter()
                    .filter_map(|m| {
                        let rows = m.rows.max(0) as usize;
                        let cols = m.cols.max(0) as usize;
                        let bpp = match m.elem_type {
                            1 => 1usize,
                            2 | 3 => 2usize,
                            4..=7 => 4usize,
                            _ => 2usize,
                        };
                        let byte_len = rows.saturating_mul(cols).saturating_mul(bpp);
                        let addr = m.start as usize;
                        if (rows >= 2 || cols >= 2) && addr.saturating_add(byte_len) <= rom_len {
                            Some(OracleTable { address: addr, rows, cols, byte_len })
                        } else {
                            None
                        }
                    })
                    .collect(),
                _ => Vec::new(),
            }
        }
    }
}

fn find_sig(rom: &[u8], sig: &str) -> bool {
    if sig.is_empty() {
        return true;
    }
    rom.windows(sig.len()).any(|w| w == sig.as_bytes())
}

fn main() {
    let filter = std::env::args().nth(1);
    let opts = BlindV2Options {
        include_curves: true,
        ..BlindV2Options::default()
    };

    let t_all = Instant::now();
    let mut sum_2d = 0usize;
    let mut hit_2d_strict = 0usize;
    let mut hit_2d_ov = 0usize;
    let mut sum_1d = 0usize;
    let mut hit_1d_strict = 0usize;
    let mut tot_bytes = 0usize;

    println!(
        "{:<22} {:>7} {:>8} {:>6} {:>12} {:>12} {:>10} {:>6}",
        "ECU Case", "SizeKB", "TimeMs", "Cands", "2D +-16B", "2D Ov>=25%", "1D +-16B", "Sig"
    );
    println!("{}", "-".repeat(92));

    for case in CASES {
        if let Some(ref f) = filter {
            if !case.label.contains(f.as_str()) {
                continue;
            }
        }
        let rom = match std::fs::read(case.bin_path) {
            Ok(r) => r,
            Err(_) => {
                println!("{:<22} MISSING", case.label);
                continue;
            }
        };
        let sig_ok = find_sig(&rom, case.sig_ascii);
        let oracle = load_oracle(&case.def, rom.len());
        let o2d: Vec<&OracleTable> = oracle.iter().filter(|t| t.rows >= 2 && t.cols >= 2).collect();
        let o1d: Vec<&OracleTable> = oracle
            .iter()
            .filter(|t| (t.rows == 1) ^ (t.cols == 1))
            .collect();

        let t0 = Instant::now();
        let cands = blind_discover_v2(&rom, &opts);
        let elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
        tot_bytes += rom.len();

        let c2d: Vec<(usize, usize)> = cands
            .iter()
            .filter(|c| !c.is_curve && c.rows >= 2 && c.cols >= 2)
            .map(|c| (c.address, c.address + c.rows * c.cols * c.elem_bytes))
            .collect();
        let c_all_addrs: Vec<usize> = cands.iter().map(|c| c.address).collect();

        let m2_strict = o2d
            .iter()
            .filter(|o| c2d.iter().any(|&(cs, _)| cs.abs_diff(o.address) <= 16))
            .count();
        let m2_ov = o2d
            .iter()
            .filter(|o| {
                let oe = o.address + o.byte_len;
                c2d.iter().any(|&(cs, ce)| {
                    if cs.abs_diff(o.address) <= 16 {
                        return true;
                    }
                    let ov = ce.min(oe).saturating_sub(cs.max(o.address));
                    ov * 4 >= o.byte_len.max(1)
                })
            })
            .count();
        let m1_strict = o1d
            .iter()
            .filter(|o| c_all_addrs.iter().any(|&ca| ca.abs_diff(o.address) <= 16))
            .count();

        sum_2d += o2d.len();
        hit_2d_strict += m2_strict;
        hit_2d_ov += m2_ov;
        sum_1d += o1d.len();
        hit_1d_strict += m1_strict;

        println!(
            "{:<22} {:>7} {:>8.1} {:>6} {:>4}/{:<4}({:>2.0}%) {:>4}/{:<4}({:>2.0}%) {:>4}/{:<4} {:>4}",
            case.label,
            rom.len() / 1024,
            elapsed_ms,
            cands.len(),
            m2_strict,
            o2d.len(),
            100.0 * m2_strict as f64 / o2d.len().max(1) as f64,
            m2_ov,
            o2d.len(),
            100.0 * m2_ov as f64 / o2d.len().max(1) as f64,
            m1_strict,
            o1d.len(),
            if sig_ok { "OK" } else { "NO" },
        );
    }

    let tot_ms = t_all.elapsed().as_secs_f64() * 1000.0;
    println!("{}", "-".repeat(92));
    println!(
        "TOTAL ({:.2} MiB): {:.1} ms ({:.2} MiB/s) | 2D +-16B={}/{} ({:.1}%) | 2D Ov={}/{} ({:.1}%) | 1D={}/{} ({:.1}%)",
        tot_bytes as f64 / (1024.0 * 1024.0),
        tot_ms,
        (tot_bytes as f64 / (1024.0 * 1024.0)) / (tot_ms / 1000.0).max(1e-6),
        hit_2d_strict,
        sum_2d,
        100.0 * hit_2d_strict as f64 / sum_2d.max(1) as f64,
        hit_2d_ov,
        sum_2d,
        100.0 * hit_2d_ov as f64 / sum_2d.max(1) as f64,
        hit_1d_strict,
        sum_1d,
        100.0 * hit_1d_strict as f64 / sum_1d.max(1) as f64,
    );
}
