use kivi_defs::map_ops::{
    CellSelection, RegionInterpAxes, RegionInterpMode, RegionInterpOptions, SelectionEditOp,
};
use kivi_scanner::similar_maps::{SimilarMapOptions, SimilarMapSpec};
use kivituner::catalogue_commands::{
    chip_catalogue_count_inner, list_chip_catalogue_inner, lookup_chip_by_jedec_inner, MAX_RESULTS,
};
use kivituner::commands::{
    apply_map_selection_edit_inner, blind_automap_v2_inner, classify_blind_tables_inner,
    find_migrated_maps_inner, find_similar_maps_inner, interpolate_map_region_inner,
    load_bin_data_inner, parse_a2l_content_inner, parse_a2l_file_inner, parse_damos_content_inner,
    parse_damos_file_inner, solve_factor_offset_inner, AppState,
};

const DB: &str = "/root/ecu_master_database";

#[test]
fn chip_catalogue_commands_report_6742_and_cap_results() {
    assert_eq!(chip_catalogue_count_inner(), 6742);
    assert!(list_chip_catalogue_inner("").expect("empty").is_empty());

    let broad = list_chip_catalogue_inner("a").expect("broad");
    assert!(broad.len() <= MAX_RESULTS);

    let am29 = list_chip_catalogue_inner("AM29F400").expect("am29");
    assert!(!am29.is_empty());

    let jedec = lookup_chip_by_jedec_inner(0x01, 0x2258).expect("jedec");
    assert!(jedec.iter().any(|c| c.chip_name.contains("AM29F800BB")));
}

#[test]
fn a2l_and_damos_commands_parse_synthetic_and_real_ecu_definitions() {
    let sample_a2l = r#"
    ASAP2_VERSION 1 60
    /begin MODULE test ""
      /begin MEMORY_SEGMENT CAL "" DATA FLASH INTERN 0x800000 0x10000 -1 -1 -1 -1 -1
      /end MEMORY_SEGMENT
      /begin COMPU_METHOD CM_RPM "" RAT_FUNC "%6.1" "rpm" COEFFS 0 1 0 0 0 1
      /end COMPU_METHOD
      /begin CHARACTERISTIC KFZW "Ignition" MAP 0x801200 RL_16 0 CM_RPM 0 60
        /begin AXIS_DESCR STD_AXIS RPM CM_RPM 12 600 7200 /end AXIS_DESCR
        /begin AXIS_DESCR STD_AXIS LOAD CM_RPM 16 10 120 /end AXIS_DESCR
      /end CHARACTERISTIC
    /end MODULE
    "#;
    let a2l = parse_a2l_content_inner(sample_a2l).expect("parse sample a2l");
    assert_eq!(a2l.map_count, 1);
    assert_eq!(a2l.ecu_base_suggestions, vec![0x80_0000]);
    assert_eq!(a2l.entries[0].rows, 12);
    assert_eq!(a2l.entries[0].cols, 16);
    assert_eq!(a2l.entries[0].unit, "rpm");

    let real_a2l = format!("{DB}/02_CARS/VAG_AUDI_VOLKSWAGEN/GOLF_VR6_ABA_BINS/022906032CE-6733.A2L");
    if std::path::Path::new(&real_a2l).exists() {
        let sum = parse_a2l_file_inner(&real_a2l).expect("real golf a2l");
        assert!(sum.map_count > 100);
        assert!(sum.curve_count > 100);
    }

    let sample_dam = "/EPR,{TEST_ECU}\n/UMP, {}, test_ump, {Test UMP}, $381000, 529, 0, fak, 0, $FF, K;\n1, /SPZ, KFZW_FLASH, {Spark Map}, 5, $812000, $812000\n/SPW, 0, 2, 0\n/FKX, 16, 0\n/FKY, 12, 0;\n";
    let dam = parse_damos_content_inner(sample_dam).expect("sample damos");
    assert_eq!(dam.ecu.as_deref(), Some("TEST_ECU"));
    assert_eq!(dam.ump_count, 1);
    assert_eq!(dam.spz_2d_map_count, 1);

    let real_dam = format!("{DB}/02_CARS/VAG_AUDI_VOLKSWAGEN/AUDI_S4_2_7TT_ME7_1_SUITE/8D0907551M-0002.dam");
    if std::path::Path::new(&real_dam).exists() {
        let sum = parse_damos_file_inner(&real_dam).expect("real audi s4 damos");
        assert!(sum.spz_2d_map_count > 200);
        assert!(sum.spz_1d_curve_count > 500);
        assert!(sum.ump_count > 1000);
    }
}

#[test]
fn editing_interpolation_solver_similarity_and_classifier_commands_work_end_to_end() {
    let grid = vec![
        vec![10.0, 20.0, 30.0, 40.0],
        vec![10.0, 99.0, 99.0, 40.0],
        vec![10.0, 20.0, 30.0, 40.0],
    ];
    let sel = CellSelection {
        row_start: 1,
        row_end: 1,
        col_start: 1,
        col_end: 2,
    };

    let scaled = apply_map_selection_edit_inner(&grid, sel, SelectionEditOp::ScalePercent(10.0))
        .expect("scale edit");
    assert!((scaled[1][1] - 108.9).abs() < 1e-9);

    let interp = interpolate_map_region_inner(
        &scaled,
        sel,
        RegionInterpOptions {
            mode: RegionInterpMode::Linear,
            axes: RegionInterpAxes::HorizontalOnly,
            allow_extrapolation: false,
            fallback_all_sides: false,
        },
    )
    .expect("interp");
    assert!((interp.matrix[1][1] - 20.0).abs() < 1e-9);
    assert!((interp.matrix[1][2] - 30.0).abs() < 1e-9);

    let lin = solve_factor_offset_inner((0.0, 10.0), (100.0, 60.0), None, false).expect("lin");
    assert!((lin.factor - 0.5).abs() < 1e-9);
    assert!((lin.offset - 10.0).abs() < 1e-9);

    let rec = solve_factor_offset_inner((100.0, 60.0), (200.0, 35.0), Some((500.0, 20.0)), false)
        .expect("rec");
    assert!(rec.reciprocal);
    assert!((rec.offset - 10.0).abs() < 1e-6);

    // Similar maps + migration + classifier via AppState & Blind V2.
    let mut rom_a = vec![0xA5u8; 4096];
    let mut rom_b = vec![0x5Au8; 4096];
    let mut vals = Vec::with_capacity(36);
    for r in 0..6usize {
        for c in 0..6usize {
            vals.push((800 + r * 500 + c * 220) as u16);
        }
    }
    for (i, &v) in vals.iter().enumerate() {
        let le = v.to_le_bytes();
        rom_a[0x200 + i * 2..0x202 + i * 2].copy_from_slice(&le);
        rom_a[0x800 + i * 2..0x802 + i * 2].copy_from_slice(&le);
        rom_b[0xA40 + i * 2..0xA42 + i * 2].copy_from_slice(&le);
    }

    let state = AppState::default();
    load_bin_data_inner(&rom_a, &state).expect("load rom");
    let spec = SimilarMapSpec {
        address: 0x200,
        rows: 6,
        cols: 6,
        cell_bits: 16,
        little_endian: true,
        is_signed: false,
    };
    let in_rom = find_similar_maps_inner(spec, Some(SimilarMapOptions::default()), &state)
        .expect("find similar");
    assert_eq!(in_rom.len(), 1);
    assert_eq!(in_rom[0].address, 0x800);

    let migrated =
        find_migrated_maps_inner(&rom_a, &rom_b, spec, None).expect("find migrated");
    assert_eq!(migrated.len(), 1);
    assert_eq!(migrated[0].address, 0xA40);

    // Blind V2 + classifier test buffer (non-filler background texture + curved 8x12 surface with RPM axis).
    let bg = [0x55u8, 0xAA, 0x11];
    let mut rom_v2: Vec<u8> = (0..8192).map(|i| bg[i % 3]).collect();
    let rpm_x = [1000u16, 1500, 2000, 2500, 3000, 4000, 5000, 6000, 7000, 8000, 9000, 10000];
    let load_y = [20u16, 40, 60, 80, 100, 120, 140, 160];
    for (i, &v) in rpm_x.iter().enumerate() {
        rom_v2[0x500 + i * 2..0x502 + i * 2].copy_from_slice(&v.to_le_bytes());
    }
    for (i, &v) in load_y.iter().enumerate() {
        rom_v2[0x518 + i * 2..0x51A + i * 2].copy_from_slice(&v.to_le_bytes());
    }
    for r in 0..8usize {
        for c in 0..12usize {
            let dc = c.abs_diff(5);
            let val = (1800 + r * 350).saturating_sub(dc * 110) as u16;
            let off = 0x52A + (r * 12 + c) * 2;
            rom_v2[off..off + 2].copy_from_slice(&val.to_le_bytes());
        }
    }

    assert!(blind_automap_v2_inner(&rom_v2, true, true, true, Some(0)).is_err());
    let cands = blind_automap_v2_inner(&rom_v2, true, true, true, Some(16)).expect("blind v2 automap");
    assert!(!cands.is_empty());
    assert!(cands.len() <= 16);
    let cls = classify_blind_tables_inner(&cands, Some(&rom_v2), None).expect("classify");
    assert_eq!(cls.len(), cands.len());
    assert!(cls.iter().any(|c| c.is_some()));

    // Multi-ECU checksum engines, MG1 GF(2) Keep-CRC, and XDF full patch/checksum commands.
    use kivi_checksum::multi_ecu::ChecksumRepairMode;
    use kivituner::commands::{
        apply_xdf_patch_inner, auto_detect_checksums_inner, list_checksum_engines_inner,
        parse_xdf_full_details_inner, repair_checksum_inner,
    };

    let engines = list_checksum_engines_inner();
    assert_eq!(engines.len(), 8);
    assert!(engines.iter().any(|e| e.id == "bosch_mg1" && e.supports_keep_crc_mode));

    let mut honda = vec![0x11u8; 0x80000];
    honda[0x400..0x403].copy_from_slice(b"MKH");
    let hlen = honda.len();
    honda[hlen - 16..hlen].fill(0xFF);
    let repaired = repair_checksum_inner(
        Some(&honda),
        "honda_keihin_moto",
        ChecksumRepairMode::UpdateChecksum,
        &state,
    )
    .expect("repair honda");
    assert!(repaired.match_info.all_ok);
    let detected = auto_detect_checksums_inner(Some(&repaired.rom), &state).expect("detect");
    assert!(detected
        .iter()
        .any(|d| d.algorithm_id == "honda_keihin_moto" && d.all_ok));

    let xdf_xml = r#"
<XDFFORMAT version="1.70">
  <XDFPATCH uniqueid="0x11">
    <title>Rev Limiter Bump</title>
    <XDFPATCHENTRY name="Limit" address="0x10" datasize="2" patchdata="1234" basedata="1111" />
  </XDFPATCH>
</XDFFORMAT>"#;
    let defs = parse_xdf_full_details_inner(xdf_xml, &state).expect("parse xdf details");
    assert_eq!(defs.patches.len(), 1);
    honda[0x10..0x12].copy_from_slice(&[0x11, 0x11]);
    load_bin_data_inner(&honda, &state).expect("load honda");
    let patched = apply_xdf_patch_inner(None, &defs.patches[0], false, &state).expect("apply patch");
    assert_eq!(&patched.rom[0x10..0x12], &[0x12, 0x34]);
    let reverted = apply_xdf_patch_inner(None, &defs.patches[0], true, &state).expect("revert patch");
    assert_eq!(&reverted.rom[0x10..0x12], &[0x11, 0x11]);
}
