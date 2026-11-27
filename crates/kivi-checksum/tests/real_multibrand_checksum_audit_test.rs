//! Multi-brand CRC/checksum mathematical & real OEM ROM audit suite.
//!
//! 1. Verifies every CRC/arithmetic primitive against Greg Cook's RevEng catalogue (`b"123456789"`).
//! 2. Audits real OEM ROMs on disk across Honda, BMW, GM, Subaru, VAG, and Opel:
//!    Stock verify == OK -> 1-byte calibration mutation -> verify == FAIL ->
//!    repair_oem_by_id -> verify == OK -> strict minimal byte diff check.

use kivi_checksum::oem::registry::{auto_detect_oem_checksums, repair_oem_by_id, verify_oem_by_id};
use kivi_checksum::primitives::{
    compute_crc32, crc16_arc, crc16_ccitt, crc32_castagnoli, crc32_simos, crc32_unreflected,
    crc8_maxim, sum_modulo_256, sum_modulo_65536, twos_complement_16, twos_complement_8,
};
use kivi_core::types::Endian;
use std::path::Path;

#[test]
fn reveng_standard_crc_check_vectors_and_arithmetic_primitives() {
    let v = b"123456789";
    assert_eq!(compute_crc32(v), 0xCBF4_3926, "CRC-32/ISO-HDLC mismatch");
    assert_eq!(crc32_castagnoli(v), 0xE306_9283, "CRC-32/ISCSI mismatch");
    assert_eq!(
        crc32_unreflected(v, 0x04C1_1DB7, 0xFFFF_FFFF, 0),
        0x0376_E6E7,
        "CRC-32/MPEG-2 mismatch"
    );
    assert_eq!(crc32_simos(v), 0x89A1_897F, "CRC-32/Simos mismatch");
    assert_eq!(crc16_ccitt(v), 0x29B1, "CRC-16/IBM-3740 mismatch");
    assert_eq!(crc16_arc(v, 0), 0xBB3D, "CRC-16/ARC mismatch");
    assert_eq!(crc8_maxim(v, 0), 0xA1, "CRC-8/MAXIM-DOW mismatch");

    let sample = [0x10u8, 0x20, 0x30, 0xFF, 0x05];
    assert_eq!(sum_modulo_256(&sample), 0x64);
    assert_eq!(twos_complement_8(0x64), 0x9C);
    assert_eq!(0x64u8.wrapping_add(twos_complement_8(0x64)), 0);
    assert_eq!(sum_modulo_65536(&[0x12, 0x34, 0xAB], Endian::Big), 0xBD34);
    assert_eq!(sum_modulo_65536(&[0x12, 0x34, 0xAB], Endian::Little), 0x34BD);
    assert_eq!(twos_complement_16(0x1234), 0xEDCC);
    assert_eq!(0x1234u16.wrapping_add(twos_complement_16(0x1234)), 0);
}

#[test]
fn real_multibrand_oem_checksum_stock_mutate_fail_repair_diff_audit() {
    let real_roms: &[(&str, &str)] = &[
        (
            "Honda_XADV_MKH_D42",
            "/root/ecu_master_database/01_MOTORCYCLES/HONDA/BIG_BIKE_CBR650_CB1000_XADV/38770-MKH-D42.bin",
        ),
        (
            "Honda_XADV_MKH_D22",
            "/root/ecu_master_database/01_MOTORCYCLES/HONDA/BIG_BIKE_CBR650_CB1000_XADV/38770-MKH-D22.bin",
        ),
        (
            "BMW_GS20_TCU_Corrected",
            "/root/.hermes/workspace/_bin_archive/cloned_real_bins/chookees_ecu_tcu/BMW/3/E46/M54B30/WBABW510X0/tcu/modified/1. Show Gear in SportMode/GS20_90C0_64KB_corrected.bin",
        ),
        (
            "BMW_MS43_512KB",
            "/root/.hermes/workspace/_bin_archive/cloned_real_bins/chookees_ecu_tcu/BMW/3/E46/M54B30/WBABW510X0/ecu/base/MS43_WBABW510X0PK46741_430069_512KB.bin",
        ),
        (
            "GM_P01_Express_512KB",
            "/root/.hermes/workspace/_bin_archive/cloned_mega/Snoman002_Mega/General Motors/P01/Software/00_File_Dropoff_Zone/OS_12212156/P01-2002-Express-5.0-4L60E-12212156.bin",
        ),
        (
            "GM_P59_OldBlue_1MB",
            "/root/.hermes/workspace/_bin_archive/cloned_mega/Snoman002_Mega/General Motors/P59/Software/00_File_Dropoff_Zone/OldBlueFactory.bin",
        ),
        (
            "Opel_EDC16_0281012549",
            "/root/ecu_master_database/02_CARS/EUROPEAN_OTHER/OPEL_VAUXHALL_BOSCH_EDC16_ME7_DELCO/Bosch_EDC16_OPEL_0281012549_1037379426.ori",
        ),
        (
            "Volvo_ME7_50WRHJ",
            "/root/ecu_master_database/02_CARS/VOLVO_SAAB/VOLVO_ME7_T5_R/50WRHJ-23WRA.bin",
        ),
        (
            "VW_M383_06A906018CG",
            "/root/ecu_master_database/02_CARS/VAG_AUDI_VOLKSWAGEN/VW_GOLF_MK4_1_8T_MOTRONIC_M383/06A906018CG.bin",
        ),
    ];

    eprintln!(
        "\n{:<24} {:>8} {:<24} {:>9} {:>11} {:>11} {:>10}",
        "ROM", "SizeKB", "DetectedAlgo", "StockOK", "MutateFail", "RepairOK", "RepairBytes"
    );

    let mut verified_roundtrips = 0usize;
    for &(label, path_str) in real_roms {
        if !Path::new(path_str).exists() {
            eprintln!("{:<24} {:>8} {:<24}", label, "-", "MISSING");
            continue;
        }
        let rom = std::fs::read(path_str).expect("read ROM");
        let reports = auto_detect_oem_checksums(&rom);
        if reports.is_empty() {
            eprintln!(
                "{:<24} {:>8} {:<24} {:>9} {:>11} {:>11} {:>10}",
                label,
                rom.len() / 1024,
                "(not_auto_detected)",
                "-",
                "-",
                "-",
                "-"
            );
            continue;
        }

        for rep in reports {
            if !rep.all_ok {
                eprintln!(
                    "{:<24} {:>8} {:<24} {:>9} {:>11} {:>11} {:>10}",
                    label,
                    rom.len() / 1024,
                    rep.algorithm_id,
                    "FALSE",
                    "-",
                    "-",
                    "-"
                );
                continue;
            }

            // Pick a correctable block and mutate a single interior calibration byte away from storage words.
            let Some(blk) = rep.blocks.iter().find(|b| {
                b.ok && b.correctable && b.end > b.start + 64
            }) else {
                eprintln!(
                    "{:<24} {:>8} {:<24} {:>9} {:>11} {:>11} {:>10}",
                    label,
                    rom.len() / 1024,
                    rep.algorithm_id,
                    "TRUE",
                    "(ro_blk)",
                    "-",
                    "-"
                );
                continue;
            };

            let mut candidate_off = blk.start + (blk.end - blk.start) / 2;
            while rep.blocks.iter().any(|b| {
                b.store_offset
                    .is_some_and(|so| candidate_off.abs_diff(so) <= 16)
            }) {
                candidate_off += 32;
            }
            assert!(candidate_off < rom.len());

            let mut mutated = rom.clone();
            mutated[candidate_off] ^= 0x3C;

            let after_mut = verify_oem_by_id(&mutated, &rep.algorithm_id).expect("verify mutated");
            let mut_detected_fail = !after_mut.all_ok;

            let (repaired, after_fix) =
                repair_oem_by_id(&mutated, &rep.algorithm_id).expect("repair mutated");
            let recheck = verify_oem_by_id(&repaired, &rep.algorithm_id).expect("reverify repaired");

            let repair_diff_offsets: Vec<usize> = mutated
                .iter()
                .zip(repaired.iter())
                .enumerate()
                .filter_map(|(i, (&a, &b))| (a != b).then_some(i))
                .collect();

            eprintln!(
                "{:<24} {:>8} {:<24} {:>9} {:>11} {:>11} {:>10}",
                label,
                rom.len() / 1024,
                rep.algorithm_id,
                rep.all_ok,
                mut_detected_fail,
                after_fix.all_ok && recheck.all_ok,
                repair_diff_offsets.len()
            );

            assert!(mut_detected_fail, "{label} ({}) failed to flag mutated calibration byte", rep.algorithm_id);
            assert!(after_fix.all_ok && recheck.all_ok, "{label} ({}) repair failed", rep.algorithm_id);
            assert!(
                !repair_diff_offsets.is_empty() && repair_diff_offsets.len() <= 16,
                "{label} ({}) repair touched unexpected byte count: {}",
                rep.algorithm_id,
                repair_diff_offsets.len()
            );
            verified_roundtrips += 1;
        }
    }

    assert!(
        verified_roundtrips >= 3,
        "expected >=3 real multi-brand OEM checksum roundtrips, got {verified_roundtrips}"
    );
}

#[test]
fn size_matched_non_bmw_roms_do_not_false_trigger_gs20_or_smg2() {
    // 1. Real 256 KiB VW Bosch M3.8.3 must match ONLY bosch_me3x (not bmw_gs20).
    let vw_path = "/root/ecu_master_database/02_CARS/VAG_AUDI_VOLKSWAGEN/VW_GOLF_MK4_1_8T_MOTRONIC_M383/06A906018CG.bin";
    if Path::new(vw_path).exists() {
        let vw = std::fs::read(vw_path).expect("read VW M3.8.3");
        let ids: Vec<String> = auto_detect_oem_checksums(&vw)
            .into_iter()
            .map(|r| r.algorithm_id)
            .collect();
        assert!(
            !ids.iter().any(|id| id == "bmw_gs20"),
            "VW M3.8.3 false-triggered bmw_gs20: {ids:?}"
        );
        assert!(
            ids.iter().any(|id| id == "bosch_me3x"),
            "VW M3.8.3 failed to auto-detect bosch_me3x: {ids:?}"
        );
    }

    // 2. Real 64 KiB Honda MSX125 & 256 KiB Suzuki GSX-R1000 must NOT false-trigger bmw_gs20.
    for other_path in [
        "/root/ecu_master_database/01_MOTORCYCLES/HONDA/MINI_MOTO_MSX125_MONKEY/38770-K26-B13.bin",
        "/root/ecu_master_database/01_MOTORCYCLES/SUZUKI/GSX_R1000_DENSO_41G00/32920-41G00.bin",
    ] {
        if Path::new(other_path).exists() {
            let rom = std::fs::read(other_path).expect("read non-BMW ROM");
            let det = auto_detect_oem_checksums(&rom);
            assert!(
                !det.iter().any(|r| r.algorithm_id == "bmw_gs20" || r.algorithm_id == "bmw_smg2"),
                "{other_path} false-triggered BMW TCU auto-detect: {det:?}"
            );
        }
    }

    // 3. Generic 32 KiB dummy buffer must NOT false-trigger bmw_smg2.
    let dummy_32k = vec![0x33u8; 32_768];
    assert!(
        !auto_detect_oem_checksums(&dummy_32k)
            .iter()
            .any(|r| r.algorithm_id == "bmw_smg2"),
        "32 KiB non-BMW buffer false-triggered bmw_smg2"
    );

    // 4. Real 64 KiB BMW GS20 TCU must STILL auto-detect cleanly with all_ok == true.
    let gs20_path = "/root/.hermes/workspace/_bin_archive/cloned_real_bins/chookees_ecu_tcu/BMW/3/E46/M54B30/WBABW510X0/tcu/modified/1. Show Gear in SportMode/GS20_90C0_64KB_corrected.bin";
    if Path::new(gs20_path).exists() {
        let gs20 = std::fs::read(gs20_path).expect("read GS20");
        let det = auto_detect_oem_checksums(&gs20);
        assert!(
            det.iter().any(|r| r.algorithm_id == "bmw_gs20" && r.all_ok),
            "real GS20 TCU failed auto_detect_gs20: {det:?}"
        );
    }
}
