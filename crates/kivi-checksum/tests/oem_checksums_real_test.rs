use kivi_checksum::denso_descriptor::{
    correct_descriptor_checksums_at, correct_descriptor_checksums_with_mode,
    discover_denso_descriptor_table, parse_descriptor_table, verify_descriptor_checksums,
    verify_diesel_descriptor_checksums, DescriptorEntry,
};
use kivi_checksum::oem::bosch_edc15::{repair_edc15, verify_edc15};
use kivi_checksum::oem::bosch_edc16::{repair_edc16, verify_edc16};
use kivi_checksum::oem::bosch_edc17::{
    blind_checksum_sweep, repair_edc17_crc32c_blocks, verify_edc17_crc32c_blocks,
    TriCoreCrcBlockSpec,
};
use kivi_checksum::oem::gm_pcm::{repair_gm_pcm, verify_gm_pcm};
use kivi_checksum::oem::registry::{
    auto_detect_oem_checksums, list_oem_algorithms, repair_oem_by_id, verify_oem_by_id,
};
use kivi_checksum::oem::siemens_bmw::{repair_ems2k, repair_ms41, verify_ems2k, verify_ms41};
use kivi_checksum::primitives::{crc16_arc, crc32_castagnoli};
use kivi_checksum::xdf_checksum::{parse_xdf_checksums, repair_xdf_checksums, verify_xdf_checksum};
use std::path::Path;

#[test]
fn registry_lists_motorcycle_and_oem_engines_without_duplicates() {
    let list = list_oem_algorithms();
    assert!(list.len() >= 30, "expected >=30 engines, got {}", list.len());
    let mut ids = std::collections::HashSet::new();
    for item in &list {
        assert!(ids.insert(item.id.clone()), "duplicate engine id {}", item.id);
    }
}

#[test]
fn real_gm_6e_xdf_apyp_arap_roundtrip() {
    let Ok(base) = std::env::var("KIVI_GM_6E_DIR") else {
        return;
    };
    let base = Path::new(&base);
    let xdf_buf = base.join("Bin Definitions/6E.xdf");
    let apyp_buf = base.join("Bins/apyp.bin");
    let arap_buf = base.join("Bins/arap.bin");
    let xdf_path = xdf_buf.as_path();
    let apyp_path = apyp_buf.as_path();
    let arap_path = arap_buf.as_path();
    if !xdf_path.exists() || !apyp_path.exists() || !arap_path.exists() {
        return;
    }

    let specs = parse_xdf_checksums(&std::fs::read_to_string(xdf_path).unwrap()).unwrap();
    assert_eq!(specs.len(), 1);
    assert_eq!(specs[0].start_address, 0x8);
    assert_eq!(specs[0].end_address, 0x3FFF);
    assert_eq!(specs[0].store_address, 0x6);

    for (bin_path, expected_sum) in [(apyp_path, 0x3560u32), (arap_path, 0x5BE6u32)] {
        let mut rom = std::fs::read(bin_path).unwrap();
        let res = verify_xdf_checksum(&rom, &specs[0]).unwrap();
        assert!(res.ok, "6E checksum mismatch for {:?}", bin_path);
        assert_eq!(res.computed, expected_sum);

        rom[0x100] ^= 0x55;
        let stale = verify_xdf_checksum(&rom, &specs[0]).unwrap();
        assert!(!stale.ok);

        let (repaired, after) = repair_xdf_checksums(&rom, &specs).unwrap();
        assert!(after[0].ok);
        assert!(verify_xdf_checksum(&repaired, &specs[0]).unwrap().ok);
    }
}

#[test]
fn real_bmw_gs20_uncorrected_repairs_exact_match() {
    let raw_p = Path::new("/root/.hermes/workspace/_bin_archive/cloned_real_bins/chookees_ecu_tcu/BMW/3/E46/M54B30/WBABW510X0/tcu/modified/1. Show Gear in SportMode/GS20_90C0_64KB.bin");
    let cor_p = Path::new("/root/.hermes/workspace/_bin_archive/cloned_real_bins/chookees_ecu_tcu/BMW/3/E46/M54B30/WBABW510X0/tcu/modified/1. Show Gear in SportMode/GS20_90C0_64KB_corrected.bin");
    if !raw_p.exists() || !cor_p.exists() {
        return;
    }
    let raw = std::fs::read(raw_p).unwrap();
    let cor = std::fs::read(cor_p).unwrap();

    let v_raw = verify_oem_by_id(&raw, "bmw_gs20").unwrap();
    assert!(!v_raw.all_ok);
    let v_cor = verify_oem_by_id(&cor, "bmw_gs20").unwrap();
    assert!(v_cor.all_ok);
    assert_eq!(v_cor.blocks[0].stored, Some(0x190B));

    let (repaired, rep) = repair_oem_by_id(&raw, "bmw_gs20").unwrap();
    assert!(rep.all_ok);
    assert_eq!(repaired, cor, "repaired GS20 must match real corrected binary byte-for-byte");
}

#[test]
fn real_bmw_ms43_verify_and_roundtrip() {
    let p = Path::new("/root/.hermes/workspace/_bin_archive/cloned_real_bins/chookees_ecu_tcu/BMW/3/E46/M54B30/WBABW510X0/ecu/base/MS43_WBABW510X0PK46741_430069_512KB.bin");
    if !p.exists() {
        return;
    }
    let mut rom = std::fs::read(p).unwrap();
    let rep = verify_oem_by_id(&rom, "bmw_ms43").unwrap();
    assert!(rep.all_ok);
    assert_eq!(rep.blocks.len(), 3);
    assert_eq!(rep.blocks[0].stored, Some(0xC6D5));
    assert_eq!(rep.blocks[1].stored, Some(0x727E));
    assert_eq!(rep.blocks[2].stored, Some(0x0CF9));

    rom[0x74100] ^= 0xFF;
    let stale = verify_oem_by_id(&rom, "bmw_ms43").unwrap();
    assert!(!stale.all_ok);
    let (fixed, fixed_rep) = repair_oem_by_id(&rom, "bmw_ms43").unwrap();
    assert!(fixed_rep.all_ok);
    assert!(verify_oem_by_id(&fixed, "bmw_ms43").unwrap().all_ok);
}

#[test]
fn real_gm_p01_and_p59_verify_and_roundtrip() {
    let p01_p = Path::new("/root/.hermes/workspace/_bin_archive/cloned_mega/Snoman002_Mega/General Motors/P01/Software/00_File_Dropoff_Zone/OS_12212156/P01-2002-Express-5.0-4L60E-12212156.bin");
    let p59_p = Path::new("/root/.hermes/workspace/_bin_archive/cloned_mega/Snoman002_Mega/General Motors/P59/Software/00_File_Dropoff_Zone/OldBlueFactory.bin");
    for path in [p01_p, p59_p] {
        if !path.exists() {
            continue;
        }
        let mut rom = std::fs::read(path).unwrap();
        let rep = verify_gm_pcm(&rom).unwrap();
        assert!(rep.all_ok, "GM verify failed for {:?}", path);
        assert_eq!(rep.blocks.len(), 8);

        rom[0x21000] ^= 0xA5;
        let stale = verify_gm_pcm(&rom).unwrap();
        assert!(!stale.all_ok);
        let (repaired, rep2) = repair_gm_pcm(&rom).unwrap();
        assert!(rep2.all_ok);
        assert!(verify_gm_pcm(&repaired).unwrap().all_ok);
    }
}

#[test]
fn subaru_denso_inclusive_end_diesel_and_discovery() {
    let mut rom = vec![0u8; 0x10_0000];
    rom[0x00100..0x00104].copy_from_slice(&0x1122_3344u32.to_be_bytes());
    rom[0x0FFAFC..0x0FFB00].copy_from_slice(&0x1234_5678u32.to_be_bytes());
    // Descriptor table @ 0x0FFB80 with 3 inclusive-end entries (% 4 == 3)
    let table_off = 0x0FFB80usize;
    let ranges = [(0x0000u32, 0x0FFF3u32), (0x10000u32, 0x3FFFFu32), (0x40000u32, 0x7FFFFu32)];
    for (idx, (s, e)) in ranges.iter().enumerate() {
        let p = table_off + idx * 12;
        rom[p..p + 4].copy_from_slice(&s.to_be_bytes());
        rom[p + 4..p + 8].copy_from_slice(&e.to_be_bytes());
    }
    let entries = parse_descriptor_table(&rom, table_off, 3).unwrap();
    let fixed_std = correct_descriptor_checksums_at(&rom, &entries, table_off).unwrap();
    let reparsed = parse_descriptor_table(&fixed_std, table_off, 3).unwrap();
    assert!(verify_descriptor_checksums(&fixed_std, &reparsed).unwrap());
    let found = discover_denso_descriptor_table(&fixed_std).unwrap();
    assert_eq!(found.0, table_off);

    // Now write diesel descriptor covering 0x0FFAFC
    let mut diesel_rom = vec![0u8; 0x10_0000];
    diesel_rom[table_off..table_off + 4].copy_from_slice(&0x0FFA00u32.to_be_bytes());
    diesel_rom[table_off + 4..table_off + 8].copy_from_slice(&0x0FFB7Fu32.to_be_bytes());
    diesel_rom[0x0FFAFC..0x0FFB00].copy_from_slice(&0x0000_1111u32.to_be_bytes());
    let d_entries = vec![DescriptorEntry {
        start: 0x0FFA00,
        end: 0x0FFB7F,
        stored: 0,
    }];
    let d_fixed = correct_descriptor_checksums_with_mode(&diesel_rom, &d_entries, table_off, true).unwrap();
    let d_parsed = parse_descriptor_table(&d_fixed, table_off, 1).unwrap();
    assert!(verify_diesel_descriptor_checksums(&d_fixed, &d_parsed, table_off).unwrap());
    // Standard verify disagrees because of the 0xFFFFFFFF override at 0x0FFAFC
    assert!(!verify_descriptor_checksums(&d_fixed, &d_parsed).unwrap());
}

#[test]
fn synthetic_roundtrips_all_oem_engines() {
    // 1. MS41 24 KiB partial
    let mut ms41 = vec![0xAAu8; 24 * 1024];
    let magic_off = 0x100usize;
    ms41[magic_off..magic_off + 4].copy_from_slice(&[0x4E, 0x00, 0xFF, 0xFF]);
    ms41[magic_off + 0x0E..magic_off + 0x10].copy_from_slice(&0x1234u16.to_be_bytes());
    for i in 1..4 {
        let cur = magic_off + i * 0x50;
        let store_rel = ((i + 1) * 0x50 - 2) as u16;
        ms41[cur..cur + 2].copy_from_slice(&store_rel.to_le_bytes());
    }
    let term_pos = magic_off + 4 * 0x50;
    ms41[term_pos..term_pos + 2].copy_from_slice(&0xFFFFu16.to_le_bytes());
    let (ms41_ok, ms41_rep) = repair_ms41(&ms41).unwrap();
    assert!(ms41_rep.all_ok);
    assert!(verify_ms41(&ms41_ok).unwrap().all_ok);

    // 2. EMS2000
    let mut ems = vec![0x55u8; 0x80000];
    for i in 0u16..256 {
        let v = crc16_arc(&[i as u8], 0);
        ems[0x11232 + 2 * i as usize..0x11232 + 2 * i as usize + 2].copy_from_slice(&v.to_le_bytes());
    }
    ems[0x3ED8..0x3EE0].copy_from_slice(&[0, 0x10, 0, 0, 0xFF, 0x1F, 0, 0]);
    ems[0x10012..0x10014].copy_from_slice(&1u16.to_le_bytes());
    ems[0x10014..0x1001C].copy_from_slice(&[0, 0x20, 1, 0, 0xFF, 0x2F, 1, 0]);
    ems[0x10B9C..0x10BA0].copy_from_slice(&[0x00, 0x3E, 0, 0]);
    ems[0x10BBC..0x10BC0].copy_from_slice(&[0x02, 0x3E, 0, 0]);
    ems[0x77FE2] = 1;
    ems[0x77FE4..0x77FEC].copy_from_slice(&[0, 0x00, 7, 0, 0xDF, 0x7F, 7, 0]);
    let (ems_ok, ems_rep) = repair_ems2k(&ems).unwrap();
    assert!(ems_rep.blocks.iter().find(|b| b.label.contains("cal")).unwrap().ok);
    assert!(verify_ems2k(&ems_ok).is_some());

    // 3. EDC15P tdi41
    let mut edc15 = vec![0x11u8; 0x80000];
    edc15[0x50001..0x5000C].copy_from_slice(&[0x67, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x56, 0x34, 0x2E, 0x31]);
    let (edc15_ok, r15) = repair_edc15(&edc15).unwrap();
    assert!(r15.all_ok);
    assert!(verify_edc15(&edc15_ok).unwrap().all_ok);

    // 4. EDC16 FADECAFE
    let mut edc16 = vec![0x22u8; 0x40000];
    edc16[0x0034..0x0038].copy_from_slice(&0x8000_0000u32.to_be_bytes());
    edc16[0x0038..0x003C].copy_from_slice(&0x8000_FFFFu32.to_be_bytes());
    edc16[0x003C..0x0044].copy_from_slice(&[0xFA, 0xDE, 0xCA, 0xFE, 0xCA, 0xFE, 0xAF, 0xFE]);
    let (edc16_ok, r16) = repair_edc16(&edc16).unwrap();
    assert!(r16.all_ok);
    assert!(verify_edc16(&edc16_ok).unwrap().all_ok);

    // 5. EDC17 CRC-32C Castagnoli
    let edc17 = vec![0x33u8; 0x4000];
    let spec = [TriCoreCrcBlockSpec {
        label: "ASW0",
        start: 0x100,
        end_incl: 0x1FFF,
        store_offset: 0x1FFC,
        little_endian: true,
    }];
    assert!(!verify_edc17_crc32c_blocks(&edc17, &spec).all_ok);
    let (edc17_ok, r17) = repair_edc17_crc32c_blocks(&edc17, &spec).unwrap();
    assert!(r17.all_ok);
    assert_eq!(crc32_castagnoli(&edc17_ok[0x100..0x1FFC]), u32::from_le_bytes(edc17_ok[0x1FFC..0x2000].try_into().unwrap()));

    // 6. Haldex, Hitachi M32R KLine/CAN, SH7058, SH72543R, TCU, Denso TCU, Mitsu MH8104
    let mut haldex = vec![0x44u8; 0x1000];
    let (haldex_ok, hrep) = repair_oem_by_id(&haldex, "vag_haldex_asw").unwrap();
    assert!(hrep.all_ok);
    assert!(verify_oem_by_id(&haldex_ok, "vag_haldex_asw").unwrap().all_ok);
    haldex[0x300] ^= 1;
    assert!(!verify_oem_by_id(&haldex, "vag_haldex_asw").unwrap().all_ok);

    for (id, sz) in [
        ("bmw_smg2", 32 * 1024usize),
        ("subaru_hitachi_m32r_kline", 0x80000),
        ("subaru_hitachi_m32r_can", 0x80000),
        ("subaru_hitachi_sh7058", 0x10_0000),
        ("subaru_hitachi_sh72543r", 0x20_0000),
        ("subaru_hitachi_tcu_m32r", 0x80000),
        ("subaru_denso_tcu_sh7055", 0x80000),
        ("mitsu_mh8104_tcu", 0x80000),
    ] {
        let mut synthetic = vec![0x12u8; sz];
        synthetic[0x100] = 0xAB;
        let (repaired, rep) = repair_oem_by_id(&synthetic, id).unwrap();
        assert!(rep.all_ok, "failed repair for {id}");
        assert!(verify_oem_by_id(&repaired, id).unwrap().all_ok, "failed re-verify for {id}");
    }

    // 7. Repair support for M3.x, Hyundai M7.9.7, China M7.9.7, Citroen ME7.4.5, Sagem Samand, VAG ME7.XX CRC32
    let mut me3x = vec![0x33u8; 0x20000];
    me3x[0x4000..0x4002].copy_from_slice(&0x4000u16.to_le_bytes());
    me3x[0xBF02..0xBF04].copy_from_slice(&0x1234u16.to_be_bytes());
    me3x[0xBF04..0xBF06].copy_from_slice(&0x0000u16.to_be_bytes());
    me3x[0xBF00..0xBF02].copy_from_slice(&0x1234u16.to_be_bytes());
    let (me3x_ok, r_me3x) = repair_oem_by_id(&me3x, "bosch_me3x").unwrap();
    assert!(r_me3x.all_ok);
    me3x[0x5000] ^= 0xFF;
    let (me3x_re, r_me3x_2) = repair_oem_by_id(&me3x, "bosch_me3x").unwrap();
    assert!(r_me3x_2.all_ok);
    assert!(verify_oem_by_id(&me3x_re, "bosch_me3x").unwrap().all_ok);
    assert_ne!(me3x_ok, me3x);

    let mut hm797 = vec![0x24u8; 0x80000];
    hm797[0x1000..0x1010].copy_from_slice(&[0, 0, 0x80, 0, 0xFD, 0x5E, 0x80, 0, 0, 0x80, 0x80, 0, 0xFF, 0xFB, 0x80, 0]);
    for i in 2..5 {
        hm797[0x1000 + 8 * i..0x1000 + 8 * i + 4].copy_from_slice(&0x80_0000u32.to_le_bytes());
        hm797[0x1000 + 8 * i + 4..0x1000 + 8 * i + 8].copy_from_slice(&0x80_1000u32.to_le_bytes());
    }
    hm797[0x7FFE4..0x7FFE8].copy_from_slice(&(!0u32).to_le_bytes());
    let (hm797_ok, r_hm797) = repair_oem_by_id(&hm797, "bosch_m797").unwrap();
    assert!(r_hm797.all_ok);
    assert!(verify_oem_by_id(&hm797_ok, "bosch_m797").unwrap().all_ok);

    let mut cm797 = vec![0x19u8; 0x10_0000];
    cm797[0x2000..0x2010].copy_from_slice(&[0, 0, 0x80, 0, 0xFF, 0xFF, 0x80, 0, 0, 0, 0x81, 0, 0xFF, 0xFF, 0x8F, 0]);
    cm797[0xFFFEC..0xFFFF0].copy_from_slice(&(!0u32).to_le_bytes());
    let (cm797_ok, r_cm797) = repair_oem_by_id(&cm797, "bosch_china_m797").unwrap();
    assert!(r_cm797.all_ok);
    assert!(verify_oem_by_id(&cm797_ok, "bosch_china_m797").unwrap().all_ok);

    let mut me745 = vec![0x51u8; 0xD0000];
    me745[0x3000..0x3010].copy_from_slice(&[0x88, 0xE0, 0x88, 0xD0, 0xE6, 0xFC, 0x00, 0x20, 0xE0, 0x9D, 0xE6, 0xFE, 0xFF, 0xFF, 0xE0, 0xAF]);
    let (me745_ok, r_me745) = repair_oem_by_id(&me745, "bosch_me745").unwrap();
    assert!(r_me745.all_ok);
    assert!(verify_oem_by_id(&me745_ok, "bosch_me745").unwrap().all_ok);

    let mut samand = vec![0x77u8; 0xD0000];
    samand[0x5000..0x5010].copy_from_slice(&[0xDC, 0x06, 0xA8, 0x41, 0x08, 0x12, 0x18, 0x60, 0xC0, 0x85, 0x00, 0x35, 0xC0, 0x95, 0x00, 0x35]);
    let (samand_ok, r_samand) = repair_oem_by_id(&samand, "sagem_samand").unwrap();
    assert!(r_samand.all_ok);
    assert!(verify_oem_by_id(&samand_ok, "sagem_samand").unwrap().all_ok);

    let mut me7xx = vec![0x66u8; 0x80000];
    me7xx[0x100..0x10D].copy_from_slice(&[0x00, 0x00, 0x80, 0x00, 0xFF, 0xFB, 0x80, 0x00, 0x00, 0x00, 0x82, 0x00, 0xFF]);
    me7xx[0x200..0x20E].copy_from_slice(&[0x20, 0x54, 0xC4, 0x50, 0x02, 0x00, 0xF0, 0xC8, 0xF0, 0xD9, 0xD4, 0xE0, 0x02, 0x00]);
    for idx in 0..3 {
        let c6 = 0x1000 + idx * 0x100;
        me7xx[c6..c6 + 8].copy_from_slice(&[0xE6, 0xF4, 0xFF, 0xFF, 0xE6, 0xF5, 0xFF, 0xFF]);
        let store_abs = 0x87_0000usize + idx * 0x100;
        me7xx[c6 + 0x22..c6 + 0x24].copy_from_slice(&((store_abs & 0xFFFF) as u16).to_le_bytes());
        me7xx[c6 + 0x26..c6 + 0x28].copy_from_slice(&(((store_abs >> 16) & 0xFFFF) as u16).to_le_bytes());

        let c1 = 0x2000 + idx * 0x100;
        me7xx[c1..c1 + 14].copy_from_slice(&[0xF0, 0x54, 0x5C, 0x25, 0x20, 0x54, 0x5C, 0x25, 0x20, 0x54, 0x5C, 0x45, 0xF0, 0x85]);
        if idx == 0 {
            me7xx[c1 + 0x14] = 0x80;
            me7xx[c1 + 0x15] = 0x00; // hi = 0x800000 -> s = 0x0
            me7xx[c1 + 0x38..c1 + 0x3A].copy_from_slice(&0x1000u16.to_le_bytes());
        } else {
            me7xx[c1 + 0x16] = (0x81 + idx) as u8;
            me7xx[c1 + 0x17] = 0x00;
            me7xx[c1 + 0x12..c1 + 0x14].copy_from_slice(&0x0000u16.to_le_bytes());
            me7xx[c1 + 0x3A..c1 + 0x3C].copy_from_slice(&0x0800u16.to_le_bytes());
        }
    }
    let (me7xx_ok, r_me7xx) = repair_oem_by_id(&me7xx, "vag_me7xx_crc").unwrap();
    assert!(r_me7xx.all_ok);
    assert!(verify_oem_by_id(&me7xx_ok, "vag_me7xx_crc").unwrap().all_ok);

    // 8. Auto-detect + blind checksum sweep
    assert!(auto_detect_oem_checksums(&edc16_ok).iter().any(|m| m.algorithm_id == "bosch_edc16" && m.all_ok));
    let mut crc_blob = vec![0x5Au8; 512];
    let c32 = crc32fast::hash(&crc_blob[..508]);
    crc_blob[508..512].copy_from_slice(&c32.to_le_bytes());
    let hits = blind_checksum_sweep(&crc_blob);
    assert!(hits.iter().any(|m| m.algo == "crc32_ieee" && m.store_le));
}

#[test]
fn xdf_checksum_base_offset_empty_region_and_overlapping_repair() {
    let xml = r#"
<XDFFORMAT version="1.70">
  <XDFHEADER>
    <BASEOFFSET offset="0x100" subtract="1" />
  </XDFHEADER>
  <XDFCHECKSUM uniqueid="0x1">
    <title>Inner Attr Region</title>
    <REGION startaddress="0x120" endaddress="0x15F" storeaddress="0x15C" datasize="8" sumsize="32" mmedtypeflags="0x02" calculationmethod="0x6" />
  </XDFCHECKSUM>
  <XDFCHECKSUM uniqueid="0x2">
    <title>Outer Child Region</title>
    <REGION>
      <datastart>0x110</datastart>
      <dataend>0x17F</dataend>
      <storeaddress>0x100</storeaddress>
      <datasize>16</datasize>
      <sumsize>16</sumsize>
      <calculationmethod>0x0</calculationmethod>
    </REGION>
  </XDFCHECKSUM>
</XDFFORMAT>
"#;
    let specs = parse_xdf_checksums(xml).expect("parse xdf checksums");
    assert_eq!(specs.len(), 2);
    assert_eq!(specs[0].start_address, 0x20);
    assert_eq!(specs[0].end_address, 0x5F);
    assert_eq!(specs[0].store_address, 0x5C);
    assert_eq!(specs[0].sum_bits, 32);
    assert!(specs[0].little_endian);

    assert_eq!(specs[1].start_address, 0x10);
    assert_eq!(specs[1].end_address, 0x7F);
    assert_eq!(specs[1].store_address, 0x00);

    let mut rom = vec![0x4Au8; 0x100];
    rom[0x33] = 0x7E;
    let (repaired, results) = repair_xdf_checksums(&rom, &specs).expect("repair");
    assert!(results.iter().all(|r| r.ok), "expected all repaired XDF checksums ok: {results:?}");
    for spec in &specs {
        assert!(verify_xdf_checksum(&repaired, spec).unwrap().ok);
    }
}

#[test]
fn gm_e38_repairs_cvn_before_segment_sum_and_re_verifies() {
    let mut rom = vec![0x2Cu8; 0x20_0000];
    const INDEX: usize = 0x10000;
    const LAYOUT: [(usize, usize); 6] = [
        (0x24, 0x28),
        (0x48, 0x4C),
        (0x6B, 0x6F),
        (0x8E, 0x92),
        (0xB1, 0xB5),
        (0xD4, 0xD8),
    ];
    for (idx, (so, eo)) in LAYOUT.iter().enumerate() {
        let s = (0x20000 + idx * 0x10000) as u32;
        let e = s + 0x0FFF;
        rom[INDEX + so..INDEX + so + 4].copy_from_slice(&s.to_be_bytes());
        rom[INDEX + eo..INDEX + eo + 4].copy_from_slice(&e.to_be_bytes());
    }
    rom[0x20200] ^= 0x5A;
    let (repaired, rep) = repair_oem_by_id(&rom, "gm_e38").expect("repair gm_e38");
    assert!(rep.all_ok, "gm_e38 repair report must have all_ok == true: {rep:?}");
    let reverify = verify_oem_by_id(&repaired, "gm_e38").expect("verify gm_e38");
    assert!(reverify.all_ok, "gm_e38 reverify must be all_ok: {reverify:?}");
}
