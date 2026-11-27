use kivi_scanner::a2l_parse::parse_a2l;
use kivi_scanner::denso_descriptor_maps::scan_denso_sh_descriptor_maps;
use kivi_scanner::dtc_table_scan::scan_dtc_tables;
use kivi_scanner::ecu_fingerprint::identify_ecu_rom;
use kivi_scanner::kp_parse::{parse_kp_any, KpParsedAny};
use std::path::Path;

#[test]
fn a2l_and_binary_kp_real_fixtures() {
    let a2l_path = Path::new("/root/repos/map/a2lparser/testfiles/A2L/ASAP2_Demo_V161.a2l");
    if a2l_path.exists() {
        let text = std::fs::read_to_string(a2l_path).unwrap();
        let doc = parse_a2l(&text).unwrap();
        assert!(!doc.characteristics.is_empty());
        let defs = doc.to_definitions(None);
        assert!(defs.maps.len() + defs.constants.len() > 0);
    }

    let kp_path = Path::new("/root/.hermes/workspace/_bin_archive/cloned_mega/Tracqi_MxT/example/me7/8D0907551M-20190711/8D0907551M-20190711.kp");
    if kp_path.exists() {
        let raw = std::fs::read(kp_path).unwrap();
        let parsed = parse_kp_any(&raw).unwrap();
        if let KpParsedAny::Binary(kp) = parsed {
            assert!(!kp.maps.is_empty());
            let defs = kp.to_definitions();
            assert!(!defs.maps.is_empty() || !defs.constants.is_empty());
        } else {
            panic!("Expected 8D0907551M .kp to parse as binary KP");
        }
    }
}

#[test]
fn ecu_fingerprint_denso_descriptors_and_dtc_tables() {
    let mut edc16 = vec![0u8; 0x20_0000];
    edc16[0x100..0x10A].copy_from_slice(b"0281011222");
    edc16[0x200..0x206].copy_from_slice(b"EDC16U");
    let fp = identify_ecu_rom(&edc16);
    assert_eq!(fp.manufacturer, "Bosch");
    assert!(fp.family.contains("EDC16"));
    assert_eq!(fp.hw_numbers, vec!["0281011222"]);

    // Build synthetic SH705x 3D descriptor + smooth 4x4 surface
    let mut rom = vec![0u8; 0x2000];
    let px = 0x100usize;
    let py = 0x120usize;
    let pz = 0x200usize;
    for i in 0..4u16 {
        let v = 1000 + i * 500;
        rom[px + (i as usize) * 2..px + (i as usize) * 2 + 2].copy_from_slice(&v.to_be_bytes());
        let l = 20 + i * 20;
        rom[py + (i as usize) * 2..py + (i as usize) * 2 + 2].copy_from_slice(&l.to_be_bytes());
    }
    for r in 0..4u16 {
        for c in 0..4u16 {
            let p = pz + ((r * 4 + c) as usize) * 2;
            let v = 500 + r * 50 + c * 40;
            rom[p..p + 2].copy_from_slice(&v.to_be_bytes());
        }
    }
    rom[0x40..0x42].copy_from_slice(&4u16.to_be_bytes());
    rom[0x42..0x44].copy_from_slice(&4u16.to_be_bytes());
    rom[0x44..0x48].copy_from_slice(&(px as u32).to_be_bytes());
    rom[0x48..0x4C].copy_from_slice(&(py as u32).to_be_bytes());
    rom[0x4C..0x50].copy_from_slice(&(pz as u32).to_be_bytes());
    let hits = scan_denso_sh_descriptor_maps(&rom);
    assert!(hits.iter().any(|h| h.is_3d && h.descriptor_offset == 0x40 && h.ptr_z == pz));

    // DTC table + nearby switch mask
    let codes: [u16; 10] = [0x0100, 0x0101, 0x0102, 0x0110, 0x0115, 0x0120, 0x0300, 0x0301, 0x0420, 0x0440];
    for (i, code) in codes.iter().enumerate() {
        let p = 0x500 + i * 2;
        rom[p..p + 2].copy_from_slice(&code.to_be_bytes());
    }
    rom[0x700..0x70A].copy_from_slice(&[1, 1, 0, 1, 1, 0, 1, 1, 0, 1]);
    let dtcs = scan_dtc_tables(&rom, 8);
    assert!(dtcs.iter().any(|t| {
        t.code_table_offset == 0x500
            && t.big_endian
            && t.entry_count == 10
            && t.entries.iter().any(|e| e.formatted_code == "P0420")
    }));
}
