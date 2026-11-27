use kivi_core::types::{DType, Endian};
use kivi_defs::ecu_ini::parse_ecu_ini;
use kivi_defs::ecu_xml::{export_ecu_xml, parse_ecu_xml};
use std::path::Path;

#[test]
fn ecu_xml_inheritance_switch_and_export_roundtrip() {
    let xml = r#"
    <roms>
      <rom>
        <romid><xmlid>BASE_ROM</xmlid></romid>
        <table name="VE Table" category="Fuel" type="3D" storageaddress="0x1000" storagetype="uint16" endian="little" sizex="4" sizey="2">
          <scaling units="%" expression="x*0.5" format="%.1f" />
          <table type="X Axis" storageaddress="0x800" storagetype="uint16" endian="little">
            <scaling units="RPM" expression="x" format="%.0f" />
          </table>
          <table type="Static Y Axis">
            <data>20</data><data>100</data>
          </table>
        </table>
      </rom>
      <rom base="BASE_ROM">
        <romid><xmlid>CHILD_ROM</xmlid></romid>
        <table name="VE Table" storageaddress="0x2000" />
        <table name="Rear O2" category="Emissions" type="Switch" storageaddress="0x3456" sizex="1">
          <state name="Off" data="00" />
          <state name="On" data="01" />
        </table>
      </rom>
    </roms>"#;

    let res = parse_ecu_xml(xml).unwrap();
    assert_eq!(res.rom_id, "CHILD_ROM");
    assert_eq!(res.definitions.maps.len(), 1);
    assert_eq!(res.definitions.maps[0].address, 0x2000);
    assert_eq!(res.definitions.maps[0].rows, 2);
    assert_eq!(res.definitions.maps[0].cols, 4);
    assert_eq!(res.definitions.maps[0].dtype, DType::U16);
    assert_eq!(res.definitions.maps[0].endian, Endian::Little);
    assert_eq!(res.definitions.flags.len(), 1);
    assert_eq!(res.definitions.flags[0].address, 0x3456);

    let exported = export_ecu_xml(&res.definitions, "CHILD_ROM");
    let reimported = parse_ecu_xml(&exported).unwrap();
    assert_eq!(reimported.definitions.maps[0].address, 0x2000);
    assert_eq!(reimported.definitions.maps[0].rows, 2);
    assert_eq!(reimported.definitions.maps[0].cols, 4);
}

#[test]
fn ecu_ini_parses_speeduino_and_converts_definitions() {
    let real_ini = Path::new("/root/repos/speeduino/reference/speeduino.ini");
    if real_ini.exists() {
        let text = std::fs::read_to_string(real_ini).unwrap();
        let parsed = parse_ecu_ini(&text, &[]);
        assert!(parsed.signature.contains("speeduino"));
        assert!(!parsed.page_sizes.is_empty());
        assert!(parsed.constants.len() > 20);
        assert!(!parsed.tables.is_empty());
        let defs = parsed.to_definitions();
        assert!(!defs.maps.is_empty());
    }
}

#[test]
fn ast_literal_zero_shift_range_and_depth_guard() {
    use kivi_defs::ast::eval_calibration_equation;

    assert_eq!(eval_calibration_equation("0", 42.0).unwrap(), 0.0);
    assert_eq!(eval_calibration_equation("0 + X * 0", 42.0).unwrap(), 0.0);
    assert_eq!(eval_calibration_equation("X + 0", 7.5).unwrap(), 7.5);
    assert_eq!(eval_calibration_equation("1 << 4", 0.0).unwrap(), 16.0);
    assert!(eval_calibration_equation("X << 64", 1.0).is_err());
    assert!(eval_calibration_equation("X >> 64", 8.0).is_err());
    assert!(eval_calibration_equation("X << -1", 1.0).is_err());

    let deep = format!("{}X{}", "(".repeat(200), ")".repeat(200));
    assert!(eval_calibration_equation(&deep, 1.0).is_err());
}
