use kivi_defs::model::Definitions;
use kivi_defs::patch::{apply_patch_safe, inspect_patch_status, PatchStatus};
use kivi_defs::xdf::parse_xdf_full;

#[test]
fn parses_xdf_patches_with_base_offset_and_optional_basedata_plus_safe_apply_and_revert() {
    let xml = r#"
<XDFFORMAT version="1.70">
  <XDFHEADER>
    <BASEOFFSET offset="0x1000" subtract="1" />
    <REGION type="0xFFFFFFFF" startaddress="0x1000" size="0x1000" regionflags="0x0" name="Binary File" />
    <CATEGORY index="0x1" name="Immobilizer Bypass" />
    <CATEGORY index="0x2" name="Checksum Bypass" />
  </XDFHEADER>
  <XDFPATCH uniqueid="0x3AF2">
    <title>EWS Delete Jump</title>
    <description>Bypasses immobilizer check cleanly</description>
    <CATEGORYMEM index="0" category="1" />
    <XDFPATCHENTRY name="Patch Jump" address="0x1020" datasize="0x4" patchdata="DA 0D F8 3B" basedata="DA0A6CDD" />
  </XDFPATCH>
  <XDFPATCH uniqueid="0x4001">
    <title>Clear ISN Overwrite</title>
    <CATEGORYMEM index="0" category="2" />
    <XDFPATCHENTRY name="Clear ISN" address="0x1040" datasize="0x3" patchdata="FFFFFF" />
  </XDFPATCH>
  <XDFCHECKSUM uniqueid="0xD95">
    <title>Main Page Checksum</title>
    <REGION>
      <datastart>0x1008</datastart>
      <dataend>0x1FFF</dataend>
      <storeaddress>0x1006</storeaddress>
      <datasize>2</datasize>
      <sumsize>16</sumsize>
      <calculationmethod>0x0</calculationmethod>
    </REGION>
  </XDFCHECKSUM>
  <XDFCHECKSUM uniqueid="0xD96">
    <title>Attr Region Checksum</title>
    <REGION startaddress="0x1100" endaddress="0x11FF" storeaddress="0x11FC" datasize="8" sumsize="32" mmedtypeflags="0x02" calculationmethod="0x4" />
  </XDFCHECKSUM>
</XDFFORMAT>
"#;

    let defs = parse_xdf_full(xml).expect("parse xdf full");
    assert_eq!(defs.patches.len(), 2);
    assert_eq!(defs.checksums.len(), 2);

    let p0 = &defs.patches[0];
    assert_eq!(p0.id, "0x3AF2");
    assert_eq!(p0.name, "EWS Delete Jump");
    assert_eq!(p0.category, "Immobilizer Bypass");
    assert_eq!(p0.description, "Bypasses immobilizer check cleanly");
    assert_eq!(p0.entries.len(), 1);
    assert_eq!(p0.entries[0].name, "Patch Jump");
    assert_eq!(p0.entries[0].address, 0x20);
    assert_eq!(p0.entries[0].patch_data, vec![0xDA, 0x0D, 0xF8, 0x3B]);
    assert_eq!(p0.entries[0].base_data, vec![0xDA, 0x0A, 0x6C, 0xDD]);

    let p1 = &defs.patches[1];
    assert_eq!(p1.category, "Checksum Bypass");
    assert_eq!(p1.entries[0].address, 0x40);
    assert_eq!(p1.entries[0].patch_data, vec![0xFF, 0xFF, 0xFF]);
    assert!(p1.entries[0].base_data.is_empty());

    let c0 = &defs.checksums[0];
    assert_eq!(c0.id, "0xD95");
    assert_eq!(c0.name, "Main Page Checksum");
    assert_eq!(c0.start_address, 0x08);
    assert_eq!(c0.end_address, 0xFFF);
    assert_eq!(c0.store_address, 0x06);
    assert_eq!(c0.data_bits, 16);
    assert_eq!(c0.sum_bits, 16);
    assert_eq!(c0.calculation_method, "0x0");

    let c1 = &defs.checksums[1];
    assert_eq!(c1.start_address, 0x100);
    assert_eq!(c1.end_address, 0x1FF);
    assert_eq!(c1.store_address, 0x1FC);
    assert_eq!(c1.data_bits, 8);
    assert_eq!(c1.sum_bits, 32);
    assert!(c1.little_endian);
    assert_eq!(c1.calculation_method, "0x4");

    // Test inspect/apply/conflict/revert safety lifecycle
    let mut stock = vec![0u8; 0x80];
    stock[0x20..0x24].copy_from_slice(&[0xDA, 0x0A, 0x6C, 0xDD]);

    assert_eq!(inspect_patch_status(&stock, p0), PatchStatus::Original);
    let patched = apply_patch_safe(&stock, p0, false).expect("apply patch");
    assert_eq!(inspect_patch_status(&patched, p0), PatchStatus::Applied);
    assert_eq!(&patched[0x20..0x24], &[0xDA, 0x0D, 0xF8, 0x3B]);

    // Idempotent re-apply
    assert_eq!(apply_patch_safe(&patched, p0, false).expect("reapply"), patched);

    // Revert restores stock bytes
    let reverted = apply_patch_safe(&patched, p0, true).expect("revert");
    assert_eq!(reverted, stock);

    // Mismatched stock refuses apply with Conflict
    let mut modified_other = stock.clone();
    modified_other[0x21] = 0x99;
    assert_eq!(
        inspect_patch_status(&modified_other, p0),
        PatchStatus::Conflict {
            entry_index: 0,
            address: 0x20
        }
    );
    assert!(apply_patch_safe(&modified_other, p0, false).is_err());

    // Optional basedata: applies overwrite, but cleanly refuses revert
    let applied_overwrite = apply_patch_safe(&stock, p1, false).expect("overwrite");
    assert_eq!(&applied_overwrite[0x40..0x43], &[0xFF, 0xFF, 0xFF]);
    assert!(apply_patch_safe(&applied_overwrite, p1, true).is_err());

    // Too short buffer -> OutOfBounds
    let short_rom = vec![0u8; 0x10];
    assert_eq!(
        inspect_patch_status(&short_rom, p0),
        PatchStatus::OutOfBounds {
            entry_index: 0,
            address: 0x20
        }
    );
    assert!(apply_patch_safe(&short_rom, p0, false).is_err());
}

#[test]
fn definitions_deserializes_legacy_json_without_patches_or_checksums() {
    let legacy = r#"{"maps":[],"constants":[],"flags":[]}"#;
    let defs: Definitions = serde_json::from_str(legacy).expect("legacy json");
    assert!(defs.patches.is_empty());
    assert!(defs.checksums.is_empty());
}

#[test]
fn real_ms43_patchlist_xdf_parses_patches_if_present() {
    let path = "/root/ecu_master_database/02_CARS/BMW/SIEMENS_MS43/Siemens_MS43_MS430069_Community_Patchlist_v2.9.1.xdf";
    if let Ok(xml) = std::fs::read_to_string(path) {
        let defs = parse_xdf_full(&xml).expect("parse real MS43 XDF");
        assert!(
            !defs.patches.is_empty(),
            "expected MS43 patchlist to expose XDFPATCH entries"
        );
        assert!(defs.patches.iter().any(|p| !p.entries.is_empty()));
    }
}
