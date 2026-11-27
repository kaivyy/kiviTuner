//! Standard `<roms>/<rom>` XML ECU definition importer and exporter.
//!
//! Handles `<rom base="...">` inheritance chains, `1D` / `2D` / `3D` / `Switch`
//! tables, hex `storageaddress`, decimal `sizex`/`sizey`, `storagetype` + `endian`,
//! `X Axis` / `Y Axis` / `Static X|Y Axis`, and `<scaling expression=... units=... format=...>`.

use crate::model::{AxisDef, ConstantDef, Definitions, FlagDef, MapDef, Scaling};
use kivi_core::types::{DType, Endian};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone)]
struct XmlNode {
    name: String,
    attrs: BTreeMap<String, String>,
    text: String,
    children: Vec<XmlNode>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EcuXmlImportResult {
    pub rom_id: String,
    pub definitions: Definitions,
    pub warnings: Vec<String>,
}

fn parse_xml_tree(xml: &str) -> Result<Vec<XmlNode>, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut stack: Vec<XmlNode> = Vec::new();
    let mut roots: Vec<XmlNode> = Vec::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let mut attrs = BTreeMap::new();
                for a in e.attributes().flatten() {
                    let k = String::from_utf8_lossy(a.key.as_ref()).to_string();
                    let v = a.unescape_value().map(|s| s.into_owned()).unwrap_or_default();
                    attrs.insert(k, v);
                }
                stack.push(XmlNode {
                    name,
                    attrs,
                    text: String::new(),
                    children: Vec::new(),
                });
            }
            Ok(Event::Empty(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let mut attrs = BTreeMap::new();
                for a in e.attributes().flatten() {
                    let k = String::from_utf8_lossy(a.key.as_ref()).to_string();
                    let v = a.unescape_value().map(|s| s.into_owned()).unwrap_or_default();
                    attrs.insert(k, v);
                }
                let node = XmlNode {
                    name,
                    attrs,
                    text: String::new(),
                    children: Vec::new(),
                };
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(node);
                } else {
                    roots.push(node);
                }
            }
            Ok(Event::Text(e)) => {
                let t = e.unescape().map(|s| s.into_owned()).unwrap_or_default();
                if let Some(top) = stack.last_mut() {
                    if !top.text.is_empty() && !t.is_empty() {
                        top.text.push(' ');
                    }
                    top.text.push_str(t.trim());
                }
            }
            Ok(Event::End(_)) => {
                if let Some(done) = stack.pop() {
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(done);
                    } else {
                        roots.push(done);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("ECU XML parse error: {e}")),
            _ => {}
        }
        buf.clear();
    }
    Ok(roots)
}

fn parse_hex_addr(s: Option<&str>) -> Option<usize> {
    let clean = s?.trim().trim_start_matches("0x").trim_start_matches("0X");
    if clean.is_empty() {
        return None;
    }
    usize::from_str_radix(clean, 16).ok()
}

fn parse_dec_size(s: Option<&str>) -> Option<usize> {
    let n = s?.trim().parse::<usize>().ok()?;
    (n > 0).then_some(n)
}

fn parse_storage(attrs: &BTreeMap<String, String>) -> Option<(DType, Endian)> {
    let st = attrs.get("storagetype")?.trim().to_ascii_lowercase();
    let dtype = match st.as_str() {
        "uint8" | "u8" => DType::U8,
        "int8" | "i8" | "s8" => DType::I8,
        "uint16" | "u16" => DType::U16,
        "int16" | "i16" | "s16" => DType::I16,
        "uint32" | "u32" => DType::U32,
        "int32" | "i32" | "s32" => DType::I32,
        "float" | "f32" => DType::F32,
        _ => return None,
    };
    let endian = if dtype.byte_size() == 1
        || attrs.get("endian").is_some_and(|e| e.eq_ignore_ascii_case("little"))
    {
        Endian::Little
    } else {
        Endian::Big
    };
    Some((dtype, endian))
}

fn digits_from_format(fmt: Option<&str>) -> u8 {
    let Some(f) = fmt else {
        return 2;
    };
    if let Some(dot) = f.find('.') {
        let digits: String = f[dot + 1..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        digits.parse::<u8>().unwrap_or(2)
    } else {
        0
    }
}

fn extract_scaling(node: &XmlNode) -> Scaling {
    let Some(sc) = node.children.iter().find(|c| c.name == "scaling") else {
        return Scaling::default();
    };
    let units = sc.attrs.get("units").cloned().unwrap_or_default();
    let decimals = digits_from_format(sc.attrs.get("format").map(String::as_str));
    let raw_eq = sc
        .attrs
        .get("expression")
        .or_else(|| sc.attrs.get("expr"))
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or("X");
    Scaling {
        equation: raw_eq.replace('x', "X"),
        unit: units,
        decimals,
    }
}

fn collect_roms(node: &XmlNode, out: &mut Vec<XmlNode>) {
    if node.name == "rom" {
        out.push(node.clone());
    } else {
        for ch in &node.children {
            collect_roms(ch, out);
        }
    }
}

fn rom_id_of(rom: &XmlNode) -> String {
    rom.children
        .iter()
        .find(|c| c.name == "romid")
        .and_then(|r| r.children.iter().find(|c| c.name == "xmlid"))
        .map(|x| x.text.trim().to_string())
        .unwrap_or_default()
}

fn merge_table_nodes(base: &XmlNode, over: &XmlNode) -> XmlNode {
    let mut merged = base.clone();
    for (k, v) in &over.attrs {
        merged.attrs.insert(k.clone(), v.clone());
    }
    for ov_ch in &over.children {
        if ov_ch.name == "table" {
            let ov_type = ov_ch.attrs.get("type");
            if let Some(pos) = merged
                .children
                .iter()
                .position(|c| c.name == "table" && c.attrs.get("type") == ov_type)
            {
                let m = merge_table_nodes(&merged.children[pos], ov_ch);
                merged.children[pos] = m;
            } else {
                merged.children.push(ov_ch.clone());
            }
        } else if matches!(ov_ch.name.as_str(), "scaling" | "description") {
            if let Some(pos) = merged.children.iter().position(|c| c.name == ov_ch.name) {
                merged.children[pos] = ov_ch.clone();
            } else {
                merged.children.push(ov_ch.clone());
            }
        }
    }
    merged
}

/// Parses a `<roms>/<rom>` XML string into unified [`Definitions`].
///
/// Automatically merges `<rom base="...">` inheritance chains from root to leaf.
pub fn parse_ecu_xml(xml: &str) -> Result<EcuXmlImportResult, String> {
    let roots = parse_xml_tree(xml)?;
    let mut rom_nodes = Vec::new();
    for r in &roots {
        collect_roms(r, &mut rom_nodes);
    }
    if rom_nodes.is_empty() {
        return Err("No <rom> elements found in ECU XML".to_string());
    }

    let mut by_id: HashMap<String, XmlNode> = HashMap::new();
    for r in &rom_nodes {
        let id = rom_id_of(r);
        if !id.is_empty() {
            by_id.insert(id, r.clone());
        }
    }

    // Select target leaf rom (prefer last non-base or last rom in document)
    let target = rom_nodes.last().unwrap().clone();
    let rom_id = {
        let id = rom_id_of(&target);
        if id.is_empty() { "ROM".to_string() } else { id }
    };

    // Walk base inheritance chain (cycle-guarded)
    let mut chain = vec![target.clone()];
    let mut cur = target;
    let mut visited = vec![rom_id_of(&cur)];
    while let Some(base_id) = cur.attrs.get("base").cloned() {
        if visited.contains(&base_id) {
            break;
        }
        visited.push(base_id.clone());
        if let Some(parent) = by_id.get(&base_id) {
            chain.push(parent.clone());
            cur = parent.clone();
        } else {
            break;
        }
    }
    chain.reverse(); // root -> leaf

    let mut tables_by_name: BTreeMap<String, XmlNode> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();
    for rom in &chain {
        for ch in &rom.children {
            if ch.name != "table" {
                continue;
            }
            let tname = ch
                .attrs
                .get("name")
                .cloned()
                .unwrap_or_else(|| format!("table_{}", order.len()));
            if let Some(prev) = tables_by_name.get(&tname).cloned() {
                tables_by_name.insert(tname, merge_table_nodes(&prev, ch));
            } else {
                order.push(tname.clone());
                tables_by_name.insert(tname, ch.clone());
            }
        }
    }

    let mut defs = Definitions::default();
    let mut warnings = Vec::new();

    for tname in order {
        let el = &tables_by_name[&tname];
        let kind = el.attrs.get("type").map(String::as_str).unwrap_or("");
        let category = el
            .attrs
            .get("category")
            .cloned()
            .unwrap_or_else(|| "Other".to_string());
        let Some(address) = parse_hex_addr(el.attrs.get("storageaddress").map(String::as_str)) else {
            continue;
        };

        if kind.eq_ignore_ascii_case("Switch") {
            let mut mask = 0xFFu32;
            if let Some(st) = el.children.iter().find(|c| c.name == "state") {
                if let Some(tok) = st
                    .attrs
                    .get("data")
                    .and_then(|s| s.split_whitespace().next())
                {
                    if let Ok(v) = u32::from_str_radix(tok.trim_start_matches("0x"), 16) {
                        mask = v;
                    }
                }
            }
            defs.flags.push(FlagDef {
                id: format!("rr-flag-{address:x}"),
                name: tname,
                category,
                address,
                dtype: DType::U8,
                mask,
            });
            continue;
        }

        let Some((dtype, endian)) = parse_storage(&el.attrs) else {
            warnings.push(format!("Skipped table '{tname}': missing/unknown storagetype"));
            continue;
        };
        let scaling = extract_scaling(el);
        let sizex = parse_dec_size(el.attrs.get("sizex").map(String::as_str));
        let sizey = parse_dec_size(el.attrs.get("sizey").map(String::as_str));

        let (rows, cols) = match kind {
            "3D" => (sizey.unwrap_or(1), sizex.unwrap_or(1)),
            "2D" => (sizey.unwrap_or(1), sizex.unwrap_or(1)),
            "1D" => (1, 1),
            _ => (sizey.unwrap_or(1), sizex.unwrap_or(1)),
        };

        if rows == 1 && cols == 1 {
            defs.constants.push(ConstantDef {
                id: format!("rr-const-{address:x}"),
                name: tname,
                category,
                address,
                dtype,
                endian,
                scaling,
            });
            continue;
        }

        let mut x_axis = None;
        let mut y_axis = None;
        for ax in &el.children {
            if ax.name != "table" {
                continue;
            }
            let ax_type = ax.attrs.get("type").map(String::as_str).unwrap_or("");
            let is_x = ax_type.eq_ignore_ascii_case("X Axis")
                || ax_type.eq_ignore_ascii_case("Static X Axis");
            let is_y = ax_type.eq_ignore_ascii_case("Y Axis")
                || ax_type.eq_ignore_ascii_case("Static Y Axis");
            if !is_x && !is_y {
                continue;
            }
            let ax_len = if is_x { cols } else { rows };
            let ax_scaling = extract_scaling(ax);
            let axis_def = if ax_type.starts_with("Static") {
                let vals: Vec<f64> = ax
                    .children
                    .iter()
                    .filter(|c| c.name == "data")
                    .filter_map(|c| c.text.trim().parse::<f64>().ok())
                    .collect();
                AxisDef {
                    address: 0,
                    length: if vals.is_empty() { ax_len } else { vals.len() },
                    dtype,
                    endian,
                    scaling: ax_scaling,
                    values: vals,
                }
            } else if let Some(ax_addr) =
                parse_hex_addr(ax.attrs.get("storageaddress").map(String::as_str))
            {
                let (adtype, aend) = parse_storage(&ax.attrs).unwrap_or((dtype, endian));
                AxisDef {
                    address: ax_addr,
                    length: ax_len,
                    dtype: adtype,
                    endian: aend,
                    scaling: ax_scaling,
                    values: Vec::new(),
                }
            } else {
                continue;
            };
            if is_x {
                x_axis = Some(axis_def);
            } else {
                y_axis = Some(axis_def);
            }
        }

        defs.maps.push(MapDef {
            id: format!("rr-map-{address:x}"),
            name: tname,
            category,
            address,
            rows,
            cols,
            dtype,
            endian,
            column_major: false,
            major_stride_bits: 0,
            minor_stride_bits: 0,
            scaling,
            x_axis,
            y_axis,
        });
    }

    Ok(EcuXmlImportResult {
        rom_id,
        definitions: defs,
        warnings,
    })
}

fn dtype_to_ecu_xml(d: DType) -> &'static str {
    match d {
        DType::U8 => "uint8",
        DType::I8 => "int8",
        DType::U16 => "uint16",
        DType::I16 => "int16",
        DType::U32 => "uint32",
        DType::I32 => "int32",
        DType::F32 => "float",
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Exports [`Definitions`] to a standalone self-contained ECU XML document.
pub fn export_ecu_xml(defs: &Definitions, rom_id: &str) -> String {
    let mut out = String::new();
    out.push_str("<roms>\n  <rom>\n    <romid>\n");
    out.push_str(&format!("      <xmlid>{}</xmlid>\n", xml_escape(rom_id)));
    out.push_str("    </romid>\n");

    for c in &defs.constants {
        let end = if c.endian == Endian::Little { "little" } else { "big" };
        out.push_str(&format!(
            "    <table name=\"{}\" category=\"{}\" type=\"1D\" storageaddress=\"0x{:X}\" storagetype=\"{}\" endian=\"{}\">\n",
            xml_escape(&c.name),
            xml_escape(&c.category),
            c.address,
            dtype_to_ecu_xml(c.dtype),
            end,
        ));
        out.push_str(&format!(
            "      <scaling units=\"{}\" expression=\"{}\" format=\"%.{}f\" />\n",
            xml_escape(&c.scaling.unit),
            xml_escape(&c.scaling.equation),
            c.scaling.decimals,
        ));
        out.push_str("    </table>\n");
    }

    for m in &defs.maps {
        let kind = if m.rows > 1 && m.cols > 1 { "3D" } else { "2D" };
        let end = if m.endian == Endian::Little { "little" } else { "big" };
        out.push_str(&format!(
            "    <table name=\"{}\" category=\"{}\" type=\"{kind}\" storageaddress=\"0x{:X}\" storagetype=\"{}\" endian=\"{}\" sizex=\"{}\" sizey=\"{}\">\n",
            xml_escape(&m.name),
            xml_escape(&m.category),
            m.address,
            dtype_to_ecu_xml(m.dtype),
            end,
            m.cols,
            m.rows,
        ));
        out.push_str(&format!(
            "      <scaling units=\"{}\" expression=\"{}\" format=\"%.{}f\" />\n",
            xml_escape(&m.scaling.unit),
            xml_escape(&m.scaling.equation),
            m.scaling.decimals,
        ));
        for (ax_opt, role) in [(&m.x_axis, "X Axis"), (&m.y_axis, "Y Axis")] {
            if let Some(ax) = ax_opt {
                if !ax.values.is_empty() {
                    out.push_str(&format!("      <table type=\"Static {role}\">\n"));
                    for v in &ax.values {
                        out.push_str(&format!("        <data>{v}</data>\n"));
                    }
                    out.push_str("      </table>\n");
                } else {
                    let aend = if ax.endian == Endian::Little { "little" } else { "big" };
                    out.push_str(&format!(
                        "      <table type=\"{role}\" storageaddress=\"0x{:X}\" storagetype=\"{}\" endian=\"{aend}\">\n",
                        ax.address,
                        dtype_to_ecu_xml(ax.dtype),
                    ));
                    out.push_str(&format!(
                        "        <scaling units=\"{}\" expression=\"{}\" format=\"%.{}f\" />\n",
                        xml_escape(&ax.scaling.unit),
                        xml_escape(&ax.scaling.equation),
                        ax.scaling.decimals,
                    ));
                    out.push_str("      </table>\n");
                }
            }
        }
        out.push_str("    </table>\n");
    }

    out.push_str("  </rom>\n</roms>\n");
    out
}
