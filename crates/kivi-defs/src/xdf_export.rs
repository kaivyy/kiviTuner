//! Standalone XDF 1.70 exporter for unified [`Definitions`]
//! (maps, axes, constants, flags, patches, and XDF checksum blocks).

use crate::model::{AxisDef, Definitions, Scaling};
use kivi_core::types::{DType, Endian};
use std::fmt::Write as _;

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn dtype_to_flags(dtype: DType, endian: Endian, col_major: bool) -> (usize, u32) {
    let bits = dtype.byte_size() * 8;
    let mut flags = 0u32;
    if matches!(dtype, DType::I8 | DType::I16 | DType::I32) {
        flags |= 0x01;
    }
    if endian == Endian::Little {
        flags |= 0x02;
    }
    if col_major {
        flags |= 0x04;
    }
    if matches!(dtype, DType::F32) {
        flags |= 0x10000;
    }
    (bits, flags)
}

struct AxisXmlSpec<'a> {
    axis_id: &'a str,
    index_count: Option<usize>,
    dims: Option<(usize, usize)>,
    dtype: DType,
    endian: Endian,
    address: Option<usize>,
    major_stride_bits: i32,
    minor_stride_bits: i32,
    col_major: bool,
    scaling: &'a Scaling,
    axis: Option<&'a AxisDef>,
}

fn write_axis_tag(out: &mut String, spec: AxisXmlSpec<'_>) {
    let (bits, flags) = dtype_to_flags(spec.dtype, spec.endian, spec.col_major);
    let _ = writeln!(out, "    <XDFAXIS id=\"{}\">", spec.axis_id);
    if let Some(addr) = spec.address {
        let mut extra = String::new();
        if let Some((rows, cols)) = spec.dims {
            let _ = write!(extra, " mmedrowcount=\"{rows}\" mmedcolcount=\"{cols}\"");
        }
        if spec.major_stride_bits != 0 {
            let _ = write!(extra, " mmedmajorstridebits=\"{}\"", spec.major_stride_bits);
        }
        if spec.minor_stride_bits != 0 {
            let _ = write!(extra, " mmedminorstridebits=\"{}\"", spec.minor_stride_bits);
        }
        let _ = writeln!(
            out,
            "      <EMBEDDEDDATA mmedaddress=\"0x{:X}\" mmedelementsizebits=\"{bits}\" mmedtypeflags=\"0x{flags:X}\"{extra} />",
            addr
        );
    }
    if let Some(cnt) = spec.index_count {
        let _ = writeln!(out, "      <indexcount>{cnt}</indexcount>");
    }
    let _ = writeln!(out, "      <outputtype>1</outputtype>");
    let _ = writeln!(out, "      <decimalpl>{}</decimalpl>", spec.scaling.decimals);
    let _ = writeln!(out, "      <units>{}</units>", xml_escape(&spec.scaling.unit));
    if let Some(ax) = spec.axis {
        for (idx, val) in ax.values.iter().enumerate() {
            let _ = writeln!(out, "      <LABEL index=\"{idx}\" value=\"{val}\" />");
        }
    }
    let eq = if spec.scaling.equation.trim().is_empty() {
        "X"
    } else {
        spec.scaling.equation.trim()
    };
    let _ = writeln!(out, "      <MATH equation=\"{}\">", xml_escape(eq));
    let _ = writeln!(out, "        <VAR id=\"X\" />");
    let _ = writeln!(out, "      </MATH>");
    let _ = writeln!(out, "    </XDFAXIS>");
}

/// Serializes unified [`Definitions`] into a standard XDF 1.70 XML document.
pub fn export_xdf_definitions(defs: &Definitions, title: &str) -> String {
    let mut out = String::with_capacity(8192);
    let _ = writeln!(out, "<XDFFORMAT version=\"1.70\">");
    let _ = writeln!(out, "  <XDFHEADER>");
    let _ = writeln!(out, "    <flags>0x1</flags>");
    let _ = writeln!(out, "    <deftitle>{}</deftitle>", xml_escape(title));
    let _ = writeln!(out, "    <author>kiviTuner</author>");
    let _ = writeln!(out, "    <BASEOFFSET offset=\"0\" subtract=\"0\" />");
    let _ = writeln!(out, "  </XDFHEADER>");

    let mut categories: Vec<String> = Vec::new();
    let mut register_cat = |cat: &str| -> usize {
        let name = if cat.trim().is_empty() { "Other" } else { cat.trim() };
        if let Some(pos) = categories.iter().position(|c| c == name) {
            pos
        } else {
            categories.push(name.to_string());
            categories.len() - 1
        }
    };
    for m in &defs.maps {
        register_cat(&m.category);
    }
    for c in &defs.constants {
        register_cat(&c.category);
    }
    for f in &defs.flags {
        register_cat(&f.category);
    }
    for p in &defs.patches {
        register_cat(&p.category);
    }
    for (idx, cat) in categories.iter().enumerate() {
        let _ = writeln!(
            out,
            "  <CATEGORY index=\"{idx}\" name=\"{}\" />",
            xml_escape(cat)
        );
    }

    for (idx, c) in defs.constants.iter().enumerate() {
        let cat_idx = categories.iter().position(|x| x == &c.category).unwrap_or(0);
        let (bits, flags) = dtype_to_flags(c.dtype, c.endian, false);
        let _ = writeln!(out, "  <XDFCONSTANT uniqueid=\"0x{:X}\">", 0x100 + idx);
        let _ = writeln!(out, "    <title>{}</title>", xml_escape(&c.name));
        let _ = writeln!(out, "    <CATEGORYMEM index=\"0\" category=\"{cat_idx}\" />");
        let _ = writeln!(
            out,
            "    <EMBEDDEDDATA mmedaddress=\"0x{:X}\" mmedelementsizebits=\"{bits}\" mmedtypeflags=\"0x{flags:X}\" />",
            c.address
        );
        let _ = writeln!(out, "    <units>{}</units>", xml_escape(&c.scaling.unit));
        let _ = writeln!(out, "    <decimalpl>{}</decimalpl>", c.scaling.decimals);
        let eq = if c.scaling.equation.trim().is_empty() {
            "X"
        } else {
            c.scaling.equation.trim()
        };
        let _ = writeln!(out, "    <MATH equation=\"{}\"><VAR id=\"X\" /></MATH>", xml_escape(eq));
        let _ = writeln!(out, "  </XDFCONSTANT>");
    }

    for (idx, f) in defs.flags.iter().enumerate() {
        let cat_idx = categories.iter().position(|x| x == &f.category).unwrap_or(0);
        let (bits, _) = dtype_to_flags(f.dtype, Endian::Little, false);
        let _ = writeln!(out, "  <XDFFLAG uniqueid=\"0x{:X}\">", 0x800 + idx);
        let _ = writeln!(out, "    <title>{}</title>", xml_escape(&f.name));
        let _ = writeln!(out, "    <CATEGORYMEM index=\"0\" category=\"{cat_idx}\" />");
        let _ = writeln!(
            out,
            "    <EMBEDDEDDATA mmedaddress=\"0x{:X}\" mmedelementsizebits=\"{bits}\" />",
            f.address
        );
        let _ = writeln!(out, "    <mask>0x{:X}</mask>", f.mask);
        let _ = writeln!(out, "  </XDFFLAG>");
    }

    for (idx, p) in defs.patches.iter().enumerate() {
        let cat_idx = categories.iter().position(|x| x == &p.category).unwrap_or(0);
        let _ = writeln!(out, "  <XDFPATCH uniqueid=\"0x{:X}\">", 0x1000 + idx);
        let _ = writeln!(out, "    <title>{}</title>", xml_escape(&p.name));
        if !p.description.trim().is_empty() {
            let _ = writeln!(out, "    <description>{}</description>", xml_escape(&p.description));
        }
        let _ = writeln!(out, "    <CATEGORYMEM index=\"0\" category=\"{cat_idx}\" />");
        for entry in &p.entries {
            let patch_hex: String = entry.patch_data.iter().map(|b| format!("{b:02X}")).collect();
            let base_hex: String = entry.base_data.iter().map(|b| format!("{b:02X}")).collect();
            let _ = writeln!(
                out,
                "    <XDFPATCHENTRY name=\"{}\" address=\"0x{:X}\" datasize=\"{}\" patchdata=\"{patch_hex}\" basedata=\"{base_hex}\" />",
                xml_escape(&entry.name),
                entry.address,
                entry.patch_data.len(),
            );
        }
        let _ = writeln!(out, "  </XDFPATCH>");
    }

    for (idx, cs) in defs.checksums.iter().enumerate() {
        let _ = writeln!(out, "  <XDFCHECKSUM uniqueid=\"0x{:X}\">", 0x2000 + idx);
        let _ = writeln!(out, "    <title>{}</title>", xml_escape(&cs.name));
        let _ = writeln!(out, "    <REGION>");
        let _ = writeln!(out, "      <datastart>0x{:X}</datastart>", cs.start_address);
        let _ = writeln!(out, "      <dataend>0x{:X}</dataend>", cs.end_address);
        let _ = writeln!(out, "      <storeaddress>0x{:X}</storeaddress>", cs.store_address);
        let _ = writeln!(out, "      <datasize>{}</datasize>", cs.data_bits / 8);
        let _ = writeln!(out, "      <sumsize>{}</sumsize>", cs.sum_bits / 8);
        let flags = if cs.little_endian { 0x02 } else { 0x00 };
        let _ = writeln!(out, "      <flags>0x{flags:X}</flags>");
        let _ = writeln!(out, "      <calculationmethod>{}</calculationmethod>", xml_escape(&cs.calculation_method));
        let _ = writeln!(out, "    </REGION>");
        let _ = writeln!(out, "  </XDFCHECKSUM>");
    }

    for (idx, m) in defs.maps.iter().enumerate() {
        let cat_idx = categories.iter().position(|x| x == &m.category).unwrap_or(0);
        let _ = writeln!(out, "  <XDFTABLE uniqueid=\"0x{:X}\" flags=\"0x0\">", 0x3000 + idx);
        let _ = writeln!(out, "    <title>{}</title>", xml_escape(&m.name));
        let _ = writeln!(out, "    <CATEGORYMEM index=\"0\" category=\"{cat_idx}\" />");

        if let Some(ref x) = m.x_axis {
            let x_addr = if x.values.is_empty() || x.address > 0 {
                Some(x.address)
            } else {
                None
            };
            write_axis_tag(
                &mut out,
                AxisXmlSpec {
                    axis_id: "x",
                    index_count: Some(m.cols.max(x.length).max(1)),
                    dims: None,
                    dtype: x.dtype,
                    endian: x.endian,
                    address: x_addr,
                    major_stride_bits: 0,
                    minor_stride_bits: 0,
                    col_major: false,
                    scaling: &x.scaling,
                    axis: Some(x),
                },
            );
        } else {
            let default_scaling = Scaling::default();
            write_axis_tag(
                &mut out,
                AxisXmlSpec {
                    axis_id: "x",
                    index_count: Some(m.cols.max(1)),
                    dims: None,
                    dtype: DType::U8,
                    endian: Endian::Little,
                    address: None,
                    major_stride_bits: 0,
                    minor_stride_bits: 0,
                    col_major: false,
                    scaling: &default_scaling,
                    axis: None,
                },
            );
        }

        if let Some(ref y) = m.y_axis {
            let y_addr = if y.values.is_empty() || y.address > 0 {
                Some(y.address)
            } else {
                None
            };
            write_axis_tag(
                &mut out,
                AxisXmlSpec {
                    axis_id: "y",
                    index_count: Some(m.rows.max(y.length).max(1)),
                    dims: None,
                    dtype: y.dtype,
                    endian: y.endian,
                    address: y_addr,
                    major_stride_bits: 0,
                    minor_stride_bits: 0,
                    col_major: false,
                    scaling: &y.scaling,
                    axis: Some(y),
                },
            );
        } else {
            let default_scaling = Scaling::default();
            write_axis_tag(
                &mut out,
                AxisXmlSpec {
                    axis_id: "y",
                    index_count: Some(m.rows.max(1)),
                    dims: None,
                    dtype: DType::U8,
                    endian: Endian::Little,
                    address: None,
                    major_stride_bits: 0,
                    minor_stride_bits: 0,
                    col_major: false,
                    scaling: &default_scaling,
                    axis: None,
                },
            );
        }

        write_axis_tag(
            &mut out,
            AxisXmlSpec {
                axis_id: "z",
                index_count: None,
                dims: Some((m.rows.max(1), m.cols.max(1))),
                dtype: m.dtype,
                endian: m.endian,
                address: Some(m.address),
                major_stride_bits: m.major_stride_bits,
                minor_stride_bits: m.minor_stride_bits,
                col_major: m.column_major,
                scaling: &m.scaling,
                axis: None,
            },
        );
        let _ = writeln!(out, "  </XDFTABLE>");
    }

    let _ = writeln!(out, "</XDFFORMAT>");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ConstantDef, FlagDef, MapDef, PatchDef, PatchEntryDef};
    use crate::xdf::parse_xdf_full;

    #[test]
    fn xdf_definitions_roundtrip_through_parser() {
        let defs = Definitions {
            maps: vec![MapDef {
                id: "m1".to_string(),
                name: "Fuel Veil".to_string(),
                category: "Fuel".to_string(),
                address: 0x4200,
                rows: 4,
                cols: 6,
                dtype: DType::U16,
                endian: Endian::Little,
                column_major: true,
                major_stride_bits: 128,
                minor_stride_bits: 0,
                scaling: Scaling {
                    equation: "X * 0.5".to_string(),
                    unit: "AFR".to_string(),
                    decimals: 2,
                },
                x_axis: Some(AxisDef {
                    address: 0x4100,
                    length: 6,
                    dtype: DType::U16,
                    endian: Endian::Little,
                    scaling: Scaling {
                        equation: "X".to_string(),
                        unit: "RPM".to_string(),
                        decimals: 0,
                    },
                    values: vec![1000.0, 2000.0, 3000.0, 4000.0, 5000.0, 6000.0],
                }),
                y_axis: None,
            }],
            constants: vec![ConstantDef {
                id: "c1".to_string(),
                name: "Rev Cut".to_string(),
                category: "Limiters".to_string(),
                address: 0x100,
                dtype: DType::U16,
                endian: Endian::Big,
                scaling: Scaling {
                    equation: "X * 2".to_string(),
                    unit: "RPM".to_string(),
                    decimals: 0,
                },
            }],
            flags: vec![FlagDef {
                id: "f1".to_string(),
                name: "Launch Ctrl".to_string(),
                category: "Features".to_string(),
                address: 0x102,
                dtype: DType::U8,
                mask: 0x04,
            }],
            patches: vec![PatchDef {
                id: "p1".to_string(),
                name: "Immo Off".to_string(),
                category: "Patches".to_string(),
                description: "Bench patch".to_string(),
                entries: vec![PatchEntryDef {
                    name: "Byte0".to_string(),
                    address: 0x200,
                    patch_data: vec![0xEA, 0x00],
                    base_data: vec![0x01, 0x02],
                }],
            }],
            checksums: vec![],
        };

        let xml = export_xdf_definitions(&defs, "kiviTuner Roundtrip");
        let parsed = parse_xdf_full(&xml).expect("parse exported xdf");
        assert_eq!(parsed.maps.len(), 1);
        let m = &parsed.maps[0];
        assert_eq!(m.name, "Fuel Veil");
        assert_eq!(m.address, 0x4200);
        assert_eq!(m.rows, 4);
        assert_eq!(m.cols, 6);
        assert_eq!(m.dtype, DType::U16);
        assert_eq!(m.endian, Endian::Little);
        assert!(m.column_major);
        assert_eq!(m.major_stride_bits, 128);
        assert_eq!(m.x_axis.as_ref().unwrap().values.len(), 6);
        assert_eq!(parsed.constants.len(), 1);
        assert_eq!(parsed.constants[0].name, "Rev Cut");
        assert_eq!(parsed.flags.len(), 1);
        assert_eq!(parsed.flags[0].mask, 0x04);
        assert_eq!(parsed.patches.len(), 1);
        assert_eq!(parsed.patches[0].entries[0].patch_data, vec![0xEA, 0x00]);
    }
}
