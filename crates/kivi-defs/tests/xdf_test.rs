use kivi_core::types::{DType, Endian};
use kivi_defs::expr::{eval_scaling, evaluate_scaling, invert_scaling};
use kivi_defs::kivi_json::{
    from_json_str, maps_from_json, maps_to_json, to_json_pretty, to_json_string,
};
use kivi_defs::model::{AxisDef, ConstantDef, Definitions, FlagDef, MapDef, Scaling};
use kivi_defs::xdf::{parse_xdf, parse_xdf_full};

#[test]
fn test_math_eval() {
    let eq = "X * 0.05 + 10";
    let physical = evaluate_scaling(eq, 100.0).unwrap();
    assert!((physical - 15.0).abs() < 1e-5);

    // Test alias
    let alias_res = eval_scaling(eq, 100.0).unwrap();
    assert!((alias_res - 15.0).abs() < 1e-5);

    // Test lowercase x and identity
    assert_eq!(evaluate_scaling("X", 42.0).unwrap(), 42.0);
    assert_eq!(evaluate_scaling("x", 42.0).unwrap(), 42.0);
    assert_eq!(evaluate_scaling("", 42.0).unwrap(), 42.0);
    assert!((evaluate_scaling("x * 2", 21.0).unwrap() - 42.0).abs() < 1e-5);
    assert!((evaluate_scaling("[X] * 3", 10.0).unwrap() - 30.0).abs() < 1e-5);
}

#[test]
fn test_math_invert() {
    let eq = "X * 0.05 + 10";
    let raw = invert_scaling(eq, 15.0).unwrap();
    assert!((raw - 100.0).abs() < 1e-4);

    // Invert identity
    assert_eq!(invert_scaling("X", 42.0).unwrap(), 42.0);
    assert_eq!(invert_scaling("", 42.0).unwrap(), 42.0);

    // Invert subtraction and division
    let eq2 = "(X - 128) * 0.75";
    let raw2 = invert_scaling(eq2, 0.0).unwrap();
    assert!((raw2 - 128.0).abs() < 1e-4);
}

#[test]
fn test_xdf_table_parsing() {
    let xdf_sample = r#"
    <XDFFORMAT version="1.50">
      <XDFHEADER>
        <category index="0x0" name="Fuel Systems" />
      </XDFHEADER>
      <XDFTABLE uniqueid="0x100" flags="0x0">
        <title>Base Fuel Map</title>
        <categoryindex index="0x0" />
        <XDFAXIS id="x" uniqueid="0x0">
          <EMBEDDEDDATA mmedtypeflags="0x02" mmedaddress="0x2000" mmedelementsizebits="16" mmedrowcount="1" mmedcolcount="12" />
        </XDFAXIS>
        <XDFAXIS id="y" uniqueid="0x0">
          <EMBEDDEDDATA mmedtypeflags="0x02" mmedaddress="0x2020" mmedelementsizebits="16" mmedrowcount="1" mmedcolcount="16" />
        </XDFAXIS>
        <XDFAXIS id="z" uniqueid="0x0">
          <EMBEDDEDDATA mmedtypeflags="0x00" mmedaddress="0x3000" mmedelementsizebits="8" mmedrowcount="16" mmedcolcount="12" />
          <MATH equation="X * 0.05">
            <VAR id="X" />
          </MATH>
        </XDFAXIS>
      </XDFTABLE>
    </XDFFORMAT>
    "#;
    let maps = parse_xdf(xdf_sample).expect("Failed to parse XDF");
    assert_eq!(maps.len(), 1);
    assert_eq!(maps[0].name, "Base Fuel Map");
    assert_eq!(maps[0].category, "Fuel Systems");
    assert_eq!(maps[0].address, 0x3000);
    assert_eq!(maps[0].rows, 16);
    assert_eq!(maps[0].cols, 12);
    assert_eq!(maps[0].dtype, DType::U8);
    assert_eq!(maps[0].endian, Endian::Big);
    assert!(!maps[0].column_major);

    let x_axis = maps[0].x_axis.as_ref().unwrap();
    assert_eq!(x_axis.address, 0x2000);
    assert_eq!(x_axis.length, 12);
    assert_eq!(x_axis.dtype, DType::U16);
    assert_eq!(x_axis.endian, Endian::Little);

    let y_axis = maps[0].y_axis.as_ref().unwrap();
    assert_eq!(y_axis.address, 0x2020);
    assert_eq!(y_axis.length, 16);
    assert_eq!(y_axis.dtype, DType::U16);
    assert_eq!(y_axis.endian, Endian::Little);
}

#[test]
fn test_xdf_constants_and_flags() {
    let xdf_sample = r#"
    <XDFFORMAT version="1.50">
      <XDFCONSTANT uniqueid="0x201">
        <title>Target Idle RPM</title>
        <category>Idle Control</category>
        <EMBEDDEDDATA mmedaddress="0x1500" mmedelementsizebits="16" mmedtypeflags="0x02" />
        <MATH equation="X * 10" />
      </XDFCONSTANT>
      <XDFFLAG uniqueid="0x301">
        <title>VTEC Enable</title>
        <category>Switches</category>
        <EMBEDDEDDATA mmedaddress="0x1600" mmedelementsizebits="8" />
        <mask>0x04</mask>
      </XDFFLAG>
    </XDFFORMAT>
    "#;
    let defs = parse_xdf_full(xdf_sample).expect("Failed to parse XDF full");
    assert_eq!(defs.constants.len(), 1);
    assert_eq!(defs.constants[0].name, "Target Idle RPM");
    assert_eq!(defs.constants[0].address, 0x1500);
    assert_eq!(defs.constants[0].dtype, DType::U16);
    assert_eq!(defs.constants[0].endian, Endian::Little);
    assert_eq!(defs.constants[0].scaling.equation, "X * 10");

    assert_eq!(defs.flags.len(), 1);
    assert_eq!(defs.flags[0].name, "VTEC Enable");
    assert_eq!(defs.flags[0].address, 0x1600);
    assert_eq!(defs.flags[0].dtype, DType::U8);
    assert_eq!(defs.flags[0].mask, 0x04);
}

#[test]
fn test_kivi_json_serde() {
    let mut defs = Definitions::default();
    let map = MapDef {
        id: "map_ignition".to_string(),
        name: "Ignition Timing".to_string(),
        category: "Ignition".to_string(),
        address: 0x4000,
        rows: 10,
        cols: 10,
        dtype: DType::U8,
        endian: Endian::Big,
        column_major: false,
        major_stride_bits: 0,
        minor_stride_bits: 0,
        scaling: Scaling {
            equation: "X * 0.75".to_string(),
            unit: "deg".to_string(),
            decimals: 1,
        },
        x_axis: Some(AxisDef {
            address: 0x4100,
            length: 10,
            dtype: DType::U16,
            endian: Endian::Little,
            scaling: Scaling::default(),
            values: vec![1000.0, 2000.0, 3000.0],
        }),
        y_axis: None,
    };
    defs.maps.push(map.clone());

    defs.constants.push(ConstantDef {
        id: "const_idle".to_string(),
        name: "Idle RPM".to_string(),
        category: "Idle".to_string(),
        address: 0x1000,
        dtype: DType::U16,
        endian: Endian::Little,
        scaling: Scaling {
            equation: "X * 10".to_string(),
            unit: "RPM".to_string(),
            decimals: 0,
        },
    });

    defs.flags.push(FlagDef {
        id: "flag_vtec".to_string(),
        name: "VTEC Flag".to_string(),
        category: "VTEC".to_string(),
        address: 0x1010,
        dtype: DType::U8,
        mask: 0x01,
    });

    let json = to_json_pretty(&defs).expect("Failed to serialize");
    let deserialized = from_json_str(&json).expect("Failed to deserialize");
    assert_eq!(defs, deserialized);

    let compact = to_json_string(&defs).expect("Failed to serialize compact");
    let deserialized_compact = from_json_str(&compact).expect("Failed to deserialize");
    assert_eq!(defs, deserialized_compact);

    // Test bare map list serialization/deserialization
    let maps_json = maps_to_json(std::slice::from_ref(&map)).expect("Failed maps json");
    let parsed_maps = maps_from_json(&maps_json).expect("Failed parsed maps");
    assert_eq!(parsed_maps.len(), 1);
    assert_eq!(parsed_maps[0], map);
}
