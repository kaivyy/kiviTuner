use kivi_checksum::multi_ecu::{
    auto_detect_checksums, list_supported_checksum_engines, repair_checksum_by_id,
    verify_checksum_by_id, ChecksumRepairMode,
};
use kivi_checksum::primitives::{compute_crc32, compute_crc32_with_initial};

#[test]
fn mg1_zlib_seed_matches_reference_python_and_js_oracle() {
    assert_eq!(
        compute_crc32_with_initial(b"123456789", 0x533F_71C8),
        0x7C20_A69A
    );
}

#[test]
fn all_eight_checksum_engines_verify_corrupt_repair_and_reverify() {
    assert_eq!(list_supported_checksum_engines().len(), 8);

    // 1. honda_keihin_moto (X-ADV 750)
    {
        let mut rom = vec![0xFFu8; 0x80000];
        rom[0x100..0x103].copy_from_slice(b"MKH");
        rom[0x2000] = 0x42;
        let (repaired, m1) =
            repair_checksum_by_id(&rom, "honda_keihin_moto", ChecksumRepairMode::UpdateChecksum)
                .expect("repair keihin");
        assert!(m1.all_ok);
        let det = auto_detect_checksums(&repaired);
        assert!(det.iter().any(|d| d.algorithm_id == "honda_keihin_moto" && d.all_ok));

        let mut bad = repaired.clone();
        bad[0x2000] ^= 0x11;
        assert!(!verify_checksum_by_id(&bad, "honda_keihin_moto").unwrap().all_ok);
        let (fixed, m2) =
            repair_checksum_by_id(&bad, "honda_keihin_moto", ChecksumRepairMode::UpdateChecksum)
                .expect("re-repair");
        assert!(m2.all_ok);
        let len = fixed.len();
        assert_eq!(&fixed[..len - 8], &bad[..len - 8]);
        assert_ne!(fixed[len - 8], repaired[len - 8]);
    }

    // 2. yamaha_mt (896 KiB MT-07 2020)
    {
        let n = 0xE0000usize;
        let mut rom = vec![0x12u8; n];
        rom[n - 16..n - 8].copy_from_slice(b"B342Y001");
        rom[n - 6] = 0x64;
        rom[n - 5] = 0x02;
        let (repaired, m1) =
            repair_checksum_by_id(&rom, "yamaha_mt", ChecksumRepairMode::UpdateChecksum)
                .expect("repair yamaha");
        assert!(m1.all_ok);
        assert!(auto_detect_checksums(&repaired)
            .iter()
            .any(|d| d.algorithm_id == "yamaha_mt" && d.all_ok));

        let mut bad = repaired.clone();
        bad[0x5000] ^= 0x55;
        assert!(!verify_checksum_by_id(&bad, "yamaha_mt").unwrap().all_ok);
        let (_, m2) = repair_checksum_by_id(&bad, "yamaha_mt", ChecksumRepairMode::UpdateChecksum)
            .expect("fix yamaha");
        assert!(m2.all_ok);
    }

    // 3. bosch_mse6
    {
        let mut rom = vec![0x33u8; 0x150000];
        let rec_off = 0x140020usize;
        rom[rec_off..rec_off + 4].copy_from_slice(&0x140100u32.to_be_bytes());
        rom[rec_off + 4..rec_off + 8].copy_from_slice(&0x142100u32.to_be_bytes());
        rom[rec_off + 8..rec_off + 12].copy_from_slice(&0u32.to_be_bytes());
        rom[rec_off + 12..rec_off + 16].copy_from_slice(&0xFFFF_FFFFu32.to_be_bytes());
        let (repaired, m1) =
            repair_checksum_by_id(&rom, "bosch_mse6", ChecksumRepairMode::UpdateChecksum)
                .expect("repair mse6");
        assert!(m1.all_ok);

        let mut bad = repaired.clone();
        bad[0x140500] ^= 0x07;
        assert!(!verify_checksum_by_id(&bad, "bosch_mse6").unwrap().all_ok);
        let (_, m2) =
            repair_checksum_by_id(&bad, "bosch_mse6", ChecksumRepairMode::UpdateChecksum)
                .expect("fix mse6");
        assert!(m2.all_ok);
    }

    // 4. bosch_mse8
    {
        let mut rom = vec![0x77u8; 0x80000];
        let block = 0x40000usize;
        rom[block + 0x3FFE0..block + 0x3FFE8]
            .copy_from_slice(&[0x12, 0x34, 0x56, 0x78, 0x12, 0x34, 0x10, 0x08]);
        let crc = compute_crc32(&rom[block..block + 0x3FFF8]) ^ 0xA18A_FC77;
        rom[block + 0x3FFF8..block + 0x3FFFC].copy_from_slice(&crc.to_be_bytes());
        assert!(verify_checksum_by_id(&rom, "bosch_mse8").unwrap().all_ok);

        rom[block + 0x100] ^= 0x22;
        assert!(!verify_checksum_by_id(&rom, "bosch_mse8").unwrap().all_ok);
        let (_, m) =
            repair_checksum_by_id(&rom, "bosch_mse8", ChecksumRepairMode::UpdateChecksum)
                .expect("fix mse8");
        assert!(m.all_ok);
    }

    // 5. bosch_mg1 (both UpdateChecksum & GF(2) CompensateKeepCrc!)
    {
        let mut rom = vec![0u8; 0x80000];
        let block = 0x40000usize;
        rom[block..block + 4].copy_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]);
        rom[block + 0x1000] = 0x11;
        // Place FA FA x 8 marker and 0x12345678 trailer after 0x20000 so FA.. slot is inside [start..crc_off].
        rom[block + 0x20000..block + 0x20008].copy_from_slice(&[0xFA; 8]);
        let fill_start = block + 0x20EA4;
        for chunk in rom[fill_start..fill_start + 0x15C].as_chunks_mut::<4>().0 {
            chunk.copy_from_slice(&[0x12, 0x34, 0x56, 0x78]);
        }
        let crc_off = fill_start + 0x15C;
        let (valid_mg1, m1) =
            repair_checksum_by_id(&rom, "bosch_mg1", ChecksumRepairMode::UpdateChecksum)
                .expect("init mg1");
        assert!(m1.all_ok);
        let original_crc_bytes = valid_mg1[crc_off..crc_off + 4].to_vec();

        // Tune calibration byte, then repair using Keep-CRC GF(2) mode!
        let mut tuned = valid_mg1.clone();
        tuned[block + 0x1000] ^= 0x99;
        assert!(!verify_checksum_by_id(&tuned, "bosch_mg1").unwrap().all_ok);

        let (kept_fixed, m_keep) =
            repair_checksum_by_id(&tuned, "bosch_mg1", ChecksumRepairMode::CompensateKeepCrc)
                .expect("keep crc gf2");
        assert!(m_keep.all_ok);
        // Stored CRC bytes MUST remain completely untouched in CompensateKeepCrc mode!
        assert_eq!(&kept_fixed[crc_off..crc_off + 4], &original_crc_bytes[..]);
        assert_ne!(&kept_fixed[block + 0x1FFFC..block + 0x20000], &[0, 0, 0, 0]);
    }

    // 6. dellorto_ecm
    {
        let mut rom = vec![0x44u8; 0xC0000];
        rom[0x100..0x127].copy_from_slice(b"DellOrto 32bit Engine Management System");
        rom[0x80000..0x80002].copy_from_slice(b"JL");
        let (fixed, m) =
            repair_checksum_by_id(&rom, "dellorto_ecm", ChecksumRepairMode::UpdateChecksum)
                .expect("dellorto");
        assert!(m.all_ok);
        assert!(auto_detect_checksums(&fixed)
            .iter()
            .any(|d| d.algorithm_id == "dellorto_ecm" && d.all_ok));
    }

    // 7. italjet_suzuki
    {
        let mut rom = vec![0x21u8; 0x80000];
        rom[0x20100..0x20104].copy_from_slice(&[0xAA, 0x55, 0xCC, 0x33]);
        rom[0x20104..0x20110].copy_from_slice(b"33950-03K00 ");
        // Initialize [loc..loc+4] as valid cs/~cs pair so layout recognises cleanly.
        rom[0x20110..0x20114].copy_from_slice(&[0x00, 0x00, 0xFF, 0xFF]);
        let (fixed, m) =
            repair_checksum_by_id(&rom, "italjet_suzuki", ChecksumRepairMode::UpdateChecksum)
                .expect("italjet");
        assert!(m.all_ok);
        assert!(auto_detect_checksums(&fixed)
            .iter()
            .any(|d| d.algorithm_id == "italjet_suzuki" && d.all_ok));
    }

    // 8. qh250
    {
        let mut rom = vec![0x11u8; 0x140000];
        rom[0] = 0xAA;
        rom[1] = 0xAA;
        rom[0x7FFC..0x8000].copy_from_slice(&[0x55, 0x54, 0xAA, 0x45]);
        let (fixed, m) = repair_checksum_by_id(&rom, "qh250", ChecksumRepairMode::UpdateChecksum)
            .expect("qh250");
        assert!(m.all_ok);
        assert!(auto_detect_checksums(&fixed)
            .iter()
            .any(|d| d.algorithm_id == "qh250" && d.all_ok));
    }
}

#[test]
fn real_honda_xadv_38770_mkh_d42_auto_detects_and_verifies_valid_checksum() {
    let path = "/root/ecu_master_database/01_MOTORCYCLES/HONDA/BIG_BIKE_CBR650_CB1000_XADV/38770-MKH-D42.bin";
    if !std::path::Path::new(path).exists() {
        return;
    }
    let rom = std::fs::read(path).expect("read X-ADV ROM");
    let matches = auto_detect_checksums(&rom);
    let m = matches
        .iter()
        .find(|d| d.algorithm_id == "honda_keihin_moto")
        .expect("expected honda_keihin_moto on 38770-MKH-D42.bin");
    assert!(m.all_ok, "expected valid stock X-ADV checksum: {m:?}");
}
