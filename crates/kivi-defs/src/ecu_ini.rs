//! MegaTune / Speeduino / rusEFI `.ini` ECU definition parser.
//!
//! Parses `[MegaTune]`, `[Constants]`, `[CurveEditor]`, and `[TableEditor]` sections
//! and projects multi-page ECU constants, curves, and 2D tables onto a flat binary
//! page-offset image (`pageSize` cumulative layout) as [`Definitions`].

use crate::model::{AxisDef, ConstantDef, Definitions, FlagDef, MapDef, Scaling};
use kivi_core::types::{DType, Endian};
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, PartialEq)]
pub struct IniConstant {
    pub name: String,
    pub class: String,
    pub dtype: DType,
    pub page: usize,
    pub offset: usize,
    pub shape: (usize, usize), // (rows, cols)
    pub unit: String,
    pub scale: f64,
    pub translate: f64,
    pub bit_mask: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IniTableBinding {
    pub table_id: String,
    pub title: String,
    pub page: usize,
    pub x_bins: String,
    pub y_bins: String,
    pub z_bins: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EcuIni {
    pub signature: String,
    pub query_command: String,
    pub endianness: String,
    pub page_sizes: Vec<usize>,
    pub page_read_commands: Vec<String>,
    pub page_write_commands: Vec<String>,
    pub burn_commands: Vec<String>,
    pub blocking_factor: usize,
    pub constants: Vec<IniConstant>,
    pub tables: Vec<IniTableBinding>,
}

impl EcuIni {
    /// Cumulative page start offsets when flattening pages 1..N into one binary buffer.
    pub fn page_base_offsets(&self) -> Vec<usize> {
        let mut out = Vec::with_capacity(self.page_sizes.len() + 1);
        out.push(0); // dummy 0 index; INI pages are 1-indexed
        let mut acc = 0usize;
        for &sz in &self.page_sizes {
            out.push(acc);
            acc = acc.saturating_add(sz);
        }
        out
    }

    fn flat_offset(&self, page: usize, offset: usize) -> usize {
        let bases = self.page_base_offsets();
        bases.get(page).copied().unwrap_or(0).saturating_add(offset)
    }

    /// Converts parsed INI tables, curves, bits, and constants into unified [`Definitions`].
    pub fn to_definitions(&self) -> Definitions {
        let endian = if self.endianness.eq_ignore_ascii_case("big") {
            Endian::Big
        } else {
            Endian::Little
        };
        let by_name: HashMap<&str, &IniConstant> =
            self.constants.iter().map(|c| (c.name.as_str(), c)).collect();
        let mut defs = Definitions::default();
        let mut used_in_tables = HashMap::<String, ()>::new();

        for tbl in &self.tables {
            let Some(z) = by_name.get(tbl.z_bins.as_str()).copied() else {
                continue;
            };
            used_in_tables.insert(z.name.clone(), ());
            let x_axis = by_name.get(tbl.x_bins.as_str()).map(|x| {
                used_in_tables.insert(x.name.clone(), ());
                AxisDef {
                    address: self.flat_offset(x.page, x.offset),
                    length: x.shape.0.max(x.shape.1),
                    dtype: x.dtype,
                    endian,
                    scaling: scaling_from(x.scale, x.translate, &x.unit),
                    values: Vec::new(),
                }
            });
            let y_axis = if tbl.y_bins.is_empty() {
                None
            } else {
                by_name.get(tbl.y_bins.as_str()).map(|y| {
                    used_in_tables.insert(y.name.clone(), ());
                    AxisDef {
                        address: self.flat_offset(y.page, y.offset),
                        length: y.shape.0.max(y.shape.1),
                        dtype: y.dtype,
                        endian,
                        scaling: scaling_from(y.scale, y.translate, &y.unit),
                        values: Vec::new(),
                    }
                })
            };
            let (rows, cols) = z.shape;
            defs.maps.push(MapDef {
                id: format!("ts-{}", tbl.table_id),
                name: tbl.title.clone(),
                category: format!("Page {}", z.page),
                address: self.flat_offset(z.page, z.offset),
                rows: rows.max(1),
                cols: cols.max(1),
                dtype: z.dtype,
                endian,
                column_major: false,
                major_stride_bits: 0,
                minor_stride_bits: 0,
                scaling: scaling_from(z.scale, z.translate, &z.unit),
                x_axis,
                y_axis,
            });
        }

        for c in &self.constants {
            if used_in_tables.contains_key(&c.name) {
                continue;
            }
            let addr = self.flat_offset(c.page, c.offset);
            if let Some(mask) = c.bit_mask {
                defs.flags.push(FlagDef {
                    id: format!("ts-bit-{}-{addr:x}", c.name),
                    name: c.name.clone(),
                    category: format!("Page {}", c.page),
                    address: addr,
                    dtype: c.dtype,
                    mask,
                });
            } else if c.shape == (1, 1) {
                defs.constants.push(ConstantDef {
                    id: format!("ts-const-{}-{addr:x}", c.name),
                    name: c.name.clone(),
                    category: format!("Page {}", c.page),
                    address: addr,
                    dtype: c.dtype,
                    endian,
                    scaling: scaling_from(c.scale, c.translate, &c.unit),
                });
            } else {
                defs.maps.push(MapDef {
                    id: format!("ts-arr-{}-{addr:x}", c.name),
                    name: c.name.clone(),
                    category: format!("Page {}", c.page),
                    address: addr,
                    rows: c.shape.0,
                    cols: c.shape.1,
                    dtype: c.dtype,
                    endian,
                    column_major: false,
                    major_stride_bits: 0,
                    minor_stride_bits: 0,
                    scaling: scaling_from(c.scale, c.translate, &c.unit),
                    x_axis: None,
                    y_axis: None,
                });
            }
        }
        defs
    }
}

fn scaling_from(scale: f64, translate: f64, unit: &str) -> Scaling {
    let equation = if (scale - 1.0).abs() < 1e-12 && translate.abs() < 1e-12 {
        "X".to_string()
    } else if translate.abs() < 1e-12 {
        format!("X * {scale}")
    } else {
        format!("(X + {translate}) * {scale}")
    };
    Scaling {
        equation,
        unit: unit.to_string(),
        decimals: 2,
    }
}

fn strip_ini_comment(line: &str) -> &str {
    let mut in_quote = false;
    for (i, ch) in line.char_indices() {
        match ch {
            '"' => in_quote = !in_quote,
            ';' if !in_quote => return &line[..i],
            _ => {}
        }
    }
    line
}

fn split_csv_respect_quotes(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quote = false;
    let mut brace_depth = 0usize;
    for ch in s.chars() {
        match ch {
            '"' => {
                in_quote = !in_quote;
            }
            '{' if !in_quote => {
                brace_depth += 1;
                cur.push(ch);
            }
            '}' if !in_quote && brace_depth > 0 => {
                brace_depth -= 1;
                cur.push(ch);
            }
            ',' if !in_quote && brace_depth == 0 => {
                out.push(cur.trim().to_string());
                cur.clear();
            }
            c => cur.push(c),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

fn parse_ini_dtype(tok: &str) -> Option<DType> {
    match tok.trim().to_ascii_uppercase().as_str() {
        "U08" => Some(DType::U8),
        "S08" => Some(DType::I8),
        "U16" => Some(DType::U16),
        "S16" => Some(DType::I16),
        "U32" => Some(DType::U32),
        "S32" => Some(DType::I32),
        "F32" => Some(DType::F32),
        _ => None,
    }
}

fn parse_shape(tok: &str) -> (usize, usize) {
    let inner = tok.trim().trim_start_matches('[').trim_end_matches(']');
    if let Some((a, b)) = inner.split_once('x').or_else(|| inner.split_once('X')) {
        let cols = a.trim().parse::<usize>().unwrap_or(1).max(1);
        let rows = b.trim().parse::<usize>().unwrap_or(1).max(1);
        (rows, cols)
    } else if let Ok(n) = inner.trim().parse::<usize>() {
        (1, n.max(1))
    } else {
        (1, 1)
    }
}

fn parse_bit_mask(tok: &str) -> Option<u32> {
    let inner = tok.trim().trim_start_matches('[').trim_end_matches(']');
    let (lo_s, hi_s) = inner.split_once(':')?;
    let lo = lo_s.trim().parse::<u32>().ok()?;
    let hi = hi_s.trim().parse::<u32>().ok()?;
    if hi < lo || hi >= 32 {
        return None;
    }
    let width = hi - lo + 1;
    let mask = if width == 32 {
        u32::MAX
    } else {
        ((1u32 << width) - 1) << lo
    };
    Some(mask)
}

/// Parses a MegaTune-style `.ini` ECU definition text using active `defines`.
pub fn parse_ecu_ini(src: &str, active_defines: &[&str]) -> EcuIni {
    let mut ini = EcuIni {
        endianness: "little".to_string(),
        blocking_factor: 251,
        ..Default::default()
    };
    let mut defines: BTreeMap<String, bool> = active_defines
        .iter()
        .map(|s| ((*s).to_string(), true))
        .collect();
    let mut if_stack: Vec<bool> = Vec::new();
    let mut section = String::new();
    let mut current_page = 1usize;
    let mut current_table: Option<IniTableBinding> = None;

    let flush_table = |cur: &mut Option<IniTableBinding>, out: &mut Vec<IniTableBinding>| {
        if let Some(t) = cur.take() {
            if !t.z_bins.is_empty() {
                out.push(t);
            }
        }
    };

    for raw_line in src.lines() {
        let trimmed = strip_ini_comment(raw_line).trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(cond) = trimmed.strip_prefix("#if ") {
            let active = defines.get(cond.trim()).copied().unwrap_or(false);
            if_stack.push(active);
            continue;
        }
        if let Some(cond) = trimmed.strip_prefix("#ifdef ") {
            let active = defines.get(cond.trim()).copied().unwrap_or(false);
            if_stack.push(active);
            continue;
        }
        if trimmed == "#else" {
            if let Some(top) = if_stack.last_mut() {
                *top = !*top;
            }
            continue;
        }
        if trimmed == "#endif" {
            if_stack.pop();
            continue;
        }
        if if_stack.iter().any(|&b| !b) {
            continue;
        }
        if let Some(sym) = trimmed.strip_prefix("#set ") {
            defines.insert(sym.trim().to_string(), true);
            continue;
        }
        if let Some(sym) = trimmed.strip_prefix("#unset ") {
            defines.insert(sym.trim().to_string(), false);
            continue;
        }

        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            flush_table(&mut current_table, &mut ini.tables);
            section = trimmed[1..trimmed.len() - 1].to_ascii_lowercase();
            continue;
        }

        let Some((lhs, rhs)) = trimmed.split_once('=') else {
            continue;
        };
        let key = lhs.trim();
        let val = rhs.trim();

        match section.as_str() {
            "megatune" => {
                if key.eq_ignore_ascii_case("signature") {
                    ini.signature = val.trim_matches('"').to_string();
                } else if key.eq_ignore_ascii_case("queryCommand") {
                    ini.query_command = val.trim_matches('"').to_string();
                }
            }
            "constants" => match key.to_ascii_lowercase().as_str() {
                "endianness" => ini.endianness = val.to_string(),
                "pagesize" => {
                    ini.page_sizes = split_csv_respect_quotes(val)
                        .iter()
                        .filter_map(|s| s.parse::<usize>().ok())
                        .collect();
                }
                "pagereadcommand" => {
                    ini.page_read_commands = split_csv_respect_quotes(val);
                }
                "pagevaluewrite" | "pagechunkwrite" => {
                    ini.page_write_commands = split_csv_respect_quotes(val);
                }
                "burncommand" => {
                    ini.burn_commands = split_csv_respect_quotes(val);
                }
                "blockingfactor" => {
                    if let Ok(bf) = val.parse::<usize>() {
                        ini.blocking_factor = bf;
                    }
                }
                "page" => {
                    if let Ok(p) = val.parse::<usize>() {
                        current_page = p;
                    }
                }
                _ => {
                    let parts = split_csv_respect_quotes(val);
                    if parts.len() < 4 {
                        continue;
                    }
                    let class = parts[0].to_ascii_lowercase();
                    let Some(dtype) = parse_ini_dtype(&parts[1]) else {
                        continue;
                    };
                    let Ok(offset) = parts[2].parse::<usize>() else {
                        continue;
                    };
                    match class.as_str() {
                        "scalar" if parts.len() >= 6 => {
                            let unit = parts[3].clone();
                            let scale = parts[4].parse::<f64>().unwrap_or(1.0);
                            let translate = parts[5].parse::<f64>().unwrap_or(0.0);
                            ini.constants.push(IniConstant {
                                name: key.to_string(),
                                class,
                                dtype,
                                page: current_page,
                                offset,
                                shape: (1, 1),
                                unit,
                                scale,
                                translate,
                                bit_mask: None,
                            });
                        }
                        "bits" => {
                            let mask = parse_bit_mask(&parts[3]);
                            ini.constants.push(IniConstant {
                                name: key.to_string(),
                                class,
                                dtype,
                                page: current_page,
                                offset,
                                shape: (1, 1),
                                unit: String::new(),
                                scale: 1.0,
                                translate: 0.0,
                                bit_mask: mask,
                            });
                        }
                        "array" if parts.len() >= 7 => {
                            let shape = parse_shape(&parts[3]);
                            let unit = parts[4].clone();
                            let scale = parts[5].parse::<f64>().unwrap_or(1.0);
                            let translate = parts[6].parse::<f64>().unwrap_or(0.0);
                            ini.constants.push(IniConstant {
                                name: key.to_string(),
                                class,
                                dtype,
                                page: current_page,
                                offset,
                                shape,
                                unit,
                                scale,
                                translate,
                                bit_mask: None,
                            });
                        }
                        _ => {}
                    }
                }
            },
            "tableeditor" => {
                if key.eq_ignore_ascii_case("table") {
                    flush_table(&mut current_table, &mut ini.tables);
                    let parts = split_csv_respect_quotes(val);
                    if parts.len() >= 4 {
                        current_table = Some(IniTableBinding {
                            table_id: parts[0].clone(),
                            title: parts[2].clone(),
                            page: parts[3].parse::<usize>().unwrap_or(1),
                            x_bins: String::new(),
                            y_bins: String::new(),
                            z_bins: String::new(),
                        });
                    }
                } else if let Some(cur) = current_table.as_mut() {
                    let first_tok = split_csv_respect_quotes(val).into_iter().next().unwrap_or_default();
                    match key.to_ascii_lowercase().as_str() {
                        "xbins" => cur.x_bins = first_tok,
                        "ybins" => cur.y_bins = first_tok,
                        "zbins" => cur.z_bins = first_tok,
                        _ => {}
                    }
                }
            }
            "curveeditor" => {
                if key.eq_ignore_ascii_case("curve") {
                    flush_table(&mut current_table, &mut ini.tables);
                    let parts = split_csv_respect_quotes(val);
                    if parts.len() >= 3 {
                        current_table = Some(IniTableBinding {
                            table_id: parts[0].clone(),
                            title: parts[1].clone(),
                            page: parts.get(5).and_then(|s| s.parse::<usize>().ok()).unwrap_or(1),
                            x_bins: String::new(),
                            y_bins: String::new(),
                            z_bins: String::new(),
                        });
                    }
                } else if let Some(cur) = current_table.as_mut() {
                    let first_tok = split_csv_respect_quotes(val).into_iter().next().unwrap_or_default();
                    match key.to_ascii_lowercase().as_str() {
                        "xbins" => cur.x_bins = first_tok,
                        "ybins" => cur.z_bins = first_tok,
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    flush_table(&mut current_table, &mut ini.tables);
    ini
}

fn sanitize_ini_ident(raw: &str, fallback_prefix: &str, idx: usize) -> String {
    let mut s: String = raw
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
        .collect();
    while s.contains("__") {
        s = s.replace("__", "_");
    }
    let s = s.trim_matches('_').to_string();
    if s.is_empty() {
        format!("{fallback_prefix}_{idx}")
    } else if s.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        format!("_{s}")
    } else {
        s
    }
}

fn dtype_to_ini_token(dtype: DType) -> &'static str {
    match dtype {
        DType::U8 => "U08",
        DType::I8 => "S08",
        DType::U16 => "U16",
        DType::I16 => "S16",
        DType::U32 => "U32",
        DType::I32 => "S32",
        DType::F32 => "F32",
    }
}

fn affine_scale_translate(equation: &str) -> (f64, f64) {
    let y0 = crate::expr::evaluate_scaling(equation, 0.0).unwrap_or(0.0);
    let y1 = crate::expr::evaluate_scaling(equation, 1.0).unwrap_or(1.0);
    let scale = y1 - y0;
    if !scale.is_finite() || scale.abs() < 1e-12 {
        return (1.0, 0.0);
    }
    let translate = y0 / scale;
    if !translate.is_finite() {
        (scale, 0.0)
    } else {
        (scale, translate)
    }
}

fn bit_range_from_mask(mask: u32) -> (u32, u32) {
    if mask == 0 {
        return (0, 0);
    }
    let lo = mask.trailing_zeros();
    let shifted = mask >> lo;
    let width = shifted.trailing_ones().max(1);
    (lo, (lo + width - 1).min(31))
}

/// Exports unified [`Definitions`] as a MegaTune-compatible `.ini` file.
pub fn export_ecu_ini(
    defs: &Definitions,
    signature: &str,
    page_size: Option<usize>,
) -> String {
    use std::fmt::Write as _;

    let endian = defs
        .maps
        .first()
        .map(|m| m.endian)
        .or_else(|| defs.constants.first().map(|c| c.endian))
        .unwrap_or(Endian::Little);
    let mut max_end = 1024usize;
    for m in &defs.maps {
        let sz = m.rows.saturating_mul(m.cols).saturating_mul(m.dtype.byte_size());
        max_end = max_end.max(m.address.saturating_add(sz));
        if let Some(ref x) = m.x_axis {
            max_end = max_end.max(
                x.address
                    .saturating_add(x.length.saturating_mul(x.dtype.byte_size())),
            );
        }
        if let Some(ref y) = m.y_axis {
            max_end = max_end.max(
                y.address
                    .saturating_add(y.length.saturating_mul(y.dtype.byte_size())),
            );
        }
    }
    for c in &defs.constants {
        max_end = max_end.max(c.address.saturating_add(c.dtype.byte_size()));
    }
    for f in &defs.flags {
        max_end = max_end.max(f.address.saturating_add(f.dtype.byte_size()));
    }
    let resolved_page_size = page_size.unwrap_or(max_end).max(max_end);

    let mut out = String::with_capacity(4096);
    let _ = writeln!(out, "[MegaTune]");
    let _ = writeln!(out, "   signature = \"{signature}\"");
    let _ = writeln!(out, "   queryCommand = \"Q\"");
    let _ = writeln!(out);
    let _ = writeln!(out, "[Constants]");
    let _ = writeln!(
        out,
        "   endianness = {}",
        if endian == Endian::Big { "big" } else { "little" }
    );
    let _ = writeln!(out, "   nPages = 1");
    let _ = writeln!(out, "   pageSize = {resolved_page_size}");
    let _ = writeln!(out, "   blockingFactor = 256");
    let _ = writeln!(out, "   page = 1");

    for (idx, c) in defs.constants.iter().enumerate() {
        let key = sanitize_ini_ident(&c.name, "const", idx);
        let (scale, translate) = affine_scale_translate(&c.scaling.equation);
        let _ = writeln!(
            out,
            "   {key} = scalar, {}, {}, \"{}\", {scale}, {translate}",
            dtype_to_ini_token(c.dtype),
            c.address,
            c.scaling.unit.replace('"', "")
        );
    }

    for (idx, f) in defs.flags.iter().enumerate() {
        let key = sanitize_ini_ident(&f.name, "flag", idx);
        let (lo, hi) = bit_range_from_mask(f.mask);
        let _ = writeln!(
            out,
            "   {key} = bits, {}, {}, [{lo}:{hi}]",
            dtype_to_ini_token(f.dtype),
            f.address
        );
    }

    let mut table_entries = Vec::new();
    let mut curve_entries = Vec::new();
    for (idx, m) in defs.maps.iter().enumerate() {
        let base = sanitize_ini_ident(&m.name, "map", idx);
        let z_name = format!("{base}_z_{idx}");
        let (z_scale, z_tr) = affine_scale_translate(&m.scaling.equation);
        let shape_tok = if m.rows <= 1 {
            format!("[{}]", m.cols.max(1))
        } else {
            format!("[{}x{}]", m.cols.max(1), m.rows.max(1))
        };
        let _ = writeln!(
            out,
            "   {z_name} = array, {}, {}, {shape_tok}, \"{}\", {z_scale}, {z_tr}",
            dtype_to_ini_token(m.dtype),
            m.address,
            m.scaling.unit.replace('"', "")
        );

        let x_name = if let Some(ref x) = m.x_axis {
            let xn = format!("{base}_x_{idx}");
            let (xs, xt) = affine_scale_translate(&x.scaling.equation);
            let _ = writeln!(
                out,
                "   {xn} = array, {}, {}, [{}], \"{}\", {xs}, {xt}",
                dtype_to_ini_token(x.dtype),
                x.address,
                x.length.max(m.cols).max(1),
                x.scaling.unit.replace('"', "")
            );
            Some(xn)
        } else {
            None
        };
        let y_name = if let Some(ref y) = m.y_axis {
            let yn = format!("{base}_y_{idx}");
            let (ys, yt) = affine_scale_translate(&y.scaling.equation);
            let _ = writeln!(
                out,
                "   {yn} = array, {}, {}, [{}], \"{}\", {ys}, {yt}",
                dtype_to_ini_token(y.dtype),
                y.address,
                y.length.max(m.rows).max(1),
                y.scaling.unit.replace('"', "")
            );
            Some(yn)
        } else {
            None
        };

        if m.rows > 1 && m.cols > 1 {
            table_entries.push((idx, m.name.clone(), z_name, x_name, y_name));
        } else if x_name.is_some() {
            curve_entries.push((idx, m.name.clone(), z_name, x_name));
        }
    }

    if !table_entries.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "[TableEditor]");
        for (idx, title, z_name, x_name, y_name) in table_entries {
            let clean_title = title.replace('"', "");
            let _ = writeln!(
                out,
                "   table = table_{idx}, map_{idx}, \"{clean_title}\", 1"
            );
            if let Some(xn) = x_name {
                let _ = writeln!(out, "      xBins = {xn}, rpm");
            }
            if let Some(yn) = y_name {
                let _ = writeln!(out, "      yBins = {yn}, load");
            }
            let _ = writeln!(out, "      zBins = {z_name}");
        }
    }

    if !curve_entries.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "[CurveEditor]");
        for (idx, title, z_name, x_name) in curve_entries {
            let clean_title = title.replace('"', "");
            let _ = writeln!(out, "   curve = curve_{idx}, \"{clean_title}\"");
            if let Some(xn) = x_name {
                let _ = writeln!(out, "      xBins = {xn}");
            }
            let _ = writeln!(out, "      yBins = {z_name}");
        }
    }

    out
}

#[cfg(test)]
mod ini_export_tests {
    use super::*;

    #[test]
    fn ecu_ini_exports_and_roundtrips() {
        let defs = Definitions {
            maps: vec![MapDef {
                id: "ve".to_string(),
                name: "VE Table 1".to_string(),
                category: "Fuel".to_string(),
                address: 64,
                rows: 4,
                cols: 8,
                dtype: DType::U16,
                endian: Endian::Little,
                column_major: false,
                major_stride_bits: 0,
                minor_stride_bits: 0,
                scaling: Scaling {
                    equation: "X * 0.1".to_string(),
                    unit: "%".to_string(),
                    decimals: 1,
                },
                x_axis: Some(AxisDef {
                    address: 16,
                    length: 8,
                    dtype: DType::U16,
                    endian: Endian::Little,
                    scaling: Scaling {
                        equation: "X".to_string(),
                        unit: "RPM".to_string(),
                        decimals: 0,
                    },
                    values: vec![],
                }),
                y_axis: Some(AxisDef {
                    address: 32,
                    length: 4,
                    dtype: DType::U8,
                    endian: Endian::Little,
                    scaling: Scaling {
                        equation: "X".to_string(),
                        unit: "kPa".to_string(),
                        decimals: 0,
                    },
                    values: vec![],
                }),
            }],
            constants: vec![ConstantDef {
                id: "rev".to_string(),
                name: "rpmHardLimit".to_string(),
                category: "Limits".to_string(),
                address: 4,
                dtype: DType::U16,
                endian: Endian::Little,
                scaling: Scaling {
                    equation: "X * 1".to_string(),
                    unit: "RPM".to_string(),
                    decimals: 0,
                },
            }],
            flags: vec![FlagDef {
                id: "f".to_string(),
                name: "launchEnable".to_string(),
                category: "Options".to_string(),
                address: 8,
                dtype: DType::U8,
                mask: 0x0C,
            }],
            patches: vec![],
            checksums: vec![],
        };

        let text = export_ecu_ini(&defs, "kiviTunerTest 1.0", Some(2048));
        let parsed = parse_ecu_ini(&text, &[]);
        assert_eq!(parsed.signature, "kiviTunerTest 1.0");
        assert_eq!(parsed.page_sizes, vec![2048]);
        let round = parsed.to_definitions();
        assert_eq!(round.maps.len(), 1);
        assert_eq!(round.maps[0].address, 64);
        assert_eq!(round.maps[0].rows, 4);
        assert_eq!(round.maps[0].cols, 8);
        assert_eq!(round.constants.len(), 1);
        assert_eq!(round.flags.len(), 1);
        assert_eq!(round.flags[0].mask, 0x0C);
    }
}
