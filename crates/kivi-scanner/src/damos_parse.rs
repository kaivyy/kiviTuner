//! DAMOS (`.dam`) parsing — the Bosch calibration description format.
//!
//! ## Format
//!
//! DAMOS is plain text produced by `ASAP2DAM`. Header directives start a line
//! with `/TAG,`; calibration objects are numbered entries that begin with an
//! index and a type tag:
//!
//! ```text
//! 225, /SRC, vpsspu_w, {Solldruckverhaeltnis}, $382342, 2, 93, 0, 3;
//! /UMP, {}, abak, {Aufteilungsfaktor Wandfilm}, $38487C, 513, 71, fak_ub_b1, 3, $FF, K;
//! ```
//!
//! `/UMP` is a map. After the `{description}` come:
//! `address, dimension_word, ???, conversion, ???, mask, class`.
//!
//! ## The dimension word
//!
//! Rows in the **high byte**, columns in the **low byte**:
//!
//! | Word | Hex | Rows | Cols |
//! |---:|---|---:|---:|
//! | 513 | `0x0201` | 2 | 1 |
//! | 529 | `0x0211` | 2 | 17 |
//! | 257 | `0x0101` | 1 | 1 |
//!
//! Verified against the real `06A906032HF-0006.dam`, which is 92% printable with
//! 4 889 calibration objects.
//!
//! ## Why DAMOS matters
//!
//! It is what Bosch and the OEM tool chain ship, and unlike most `.kp` packs it
//! is **plaintext**, so it can be read completely — addresses, dimensions,
//! conversions and limits.

/// One map entry (`/UMP`).
#[derive(Debug, Clone, PartialEq)]
pub struct DamosMap {
    pub name: String,
    pub description: String,
    /// Absolute address, decoded from `$XXXXXX`.
    pub address: u32,
    pub rows: usize,
    pub cols: usize,
    /// Name of the conversion function.
    pub conversion: String,
    /// Bit mask for flag-style entries.
    pub mask: u32,
}

/// One scalar characteristic (`/SPZ`).
#[derive(Debug, Clone, PartialEq)]
pub struct DamosCharacteristic {
    pub name: String,
    pub address: u32,
}

/// One flash/ROM `/SPZ` calibration object with `/SPW`/`/SPX`/`/SPY`/`/FKX`/`/FKY` metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct DamosSpzObject {
    pub index: u32,
    pub name: String,
    pub description: String,
    pub spz_kind: u8,
    pub address: u32,
    pub elem_bytes: usize,
    pub is_signed: bool,
    pub rows: usize,
    pub cols: usize,
    pub x_axis_spz_ref: Option<u32>,
    pub y_axis_spz_ref: Option<u32>,
}

impl DamosSpzObject {
    /// True for flash 2-D calibration maps (`/SPZ` Types 3, 5, and 8) with `rows > 1 && cols > 1`.
    pub fn is_2d_map(&self) -> bool {
        matches!(self.spz_kind, 3 | 5 | 8) && self.rows > 1 && self.cols > 1
    }

    /// True for flash 1-D curves (`/SPZ` Types 2, 4, 7, and 13) with `cols >= 4 && rows <= 1`.
    pub fn is_1d_curve(&self) -> bool {
        matches!(self.spz_kind, 2 | 4 | 7 | 13) && self.cols >= 4 && self.rows <= 1
    }
}

/// A parsed DAMOS document.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DamosDocument {
    /// ECU identification from `/EPR` or `/PNR`.
    pub ecu: Option<String>,
    /// ECU family from `/SGB`.
    pub family: Option<String>,
    /// Base address from `/EAD`.
    pub base_address: Option<u32>,
    pub maps: Vec<DamosMap>,
    pub characteristics: Vec<DamosCharacteristic>,
    /// Multi-line `/SPZ` calibration objects (Types 1..13), including resolved flash maps & curves.
    pub spz_objects: Vec<DamosSpzObject>,
}

/// Why a DAMOS file could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DamosError {
    Empty,
    NoMaps,
}

impl std::fmt::Display for DamosError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DamosError::Empty => f.write_str("the DAMOS file is empty"),
            DamosError::NoMaps => f.write_str("the DAMOS file defines no maps or characteristics"),
        }
    }
}

impl std::error::Error for DamosError {}

/// Parse `$XXXXXX` or a decimal into `u32`.
fn parse_address(text: &str) -> Option<u32> {
    let t = text.trim().trim_end_matches(',');
    if let Some(hex) = t.strip_prefix('$') {
        u32::from_str_radix(hex, 16).ok()
    } else {
        t.parse::<u32>().ok()
    }
}

/// Extract the text inside `{…}` from `text` starting at `from`.
fn brace_text(text: &str, from: usize) -> Option<(&str, usize)> {
    let open = text[from..].find('{')? + from;
    let close = text[open..].find('}')? + open;
    Some((&text[open + 1..close], close + 1))
}

/// Split a `/UMP` line into its comma-separated fields, respecting `{}` groups.
fn split_fields(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0usize;
    for ch in line.chars() {
        match ch {
            '{' => {
                depth += 1;
                cur.push(ch);
            }
            '}' => {
                depth = depth.saturating_sub(1);
                cur.push(ch);
            }
            ',' if depth == 0 => {
                out.push(std::mem::take(&mut cur));
            }
            ';' if depth == 0 => break,
            c => cur.push(c),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

fn parse_spz_header(line: &str) -> Option<DamosSpzObject> {
    let pos = line.find("/SPZ,")?;
    let index = line[..pos]
        .trim()
        .trim_end_matches(',')
        .trim()
        .parse::<u32>()
        .ok()
        .unwrap_or(0);
    let fields = split_fields(&line[pos + 5..]);
    if fields.len() < 4 {
        return None;
    }
    let name = fields[0].trim().to_string();
    if name.is_empty() {
        return None;
    }
    let description = fields[1]
        .trim()
        .trim_matches(|c| c == '{' || c == '}')
        .to_string();
    let spz_kind = fields[2].trim().parse::<u8>().ok()?;
    let address = fields.iter().skip(3).find_map(|f| parse_address(f))?;
    Some(DamosSpzObject {
        index,
        name,
        description,
        spz_kind,
        address,
        elem_bytes: 1,
        is_signed: false,
        rows: 0,
        cols: 0,
        x_axis_spz_ref: None,
        y_axis_spz_ref: None,
    })
}

fn apply_spz_subtag(obj: &mut DamosSpzObject, line: &str) {
    if let Some(rest) = line.strip_prefix("/SPW,") {
        let f = split_fields(rest);
        if f.len() >= 2 {
            if let Ok(sb) = f[1].trim().parse::<i32>() {
                obj.is_signed = sb < 0;
                let abs_b = sb.unsigned_abs() as usize;
                obj.elem_bytes = if matches!(abs_b, 1 | 2 | 4) { abs_b } else { 1 };
            }
        }
    } else if let Some(rest) = line.strip_prefix("/SPX,") {
        let f = split_fields(rest);
        if f.len() >= 5 {
            if let Ok(r) = f[4].trim().parse::<u32>() {
                if r > 0 {
                    obj.x_axis_spz_ref = Some(r);
                }
            }
        }
    } else if let Some(rest) = line.strip_prefix("/SPY,") {
        let f = split_fields(rest);
        if f.len() >= 5 {
            if let Ok(r) = f[4].trim().parse::<u32>() {
                if r > 0 {
                    obj.y_axis_spz_ref = Some(r);
                }
            }
        }
    } else if let Some(rest) = line.strip_prefix("/FKX,") {
        let f = split_fields(rest);
        if let Some(c) = f.first().and_then(|s| s.trim().parse::<usize>().ok()) {
            obj.cols = c;
        }
    } else if let Some(rest) = line.strip_prefix("/FKY,") {
        let f = split_fields(rest);
        if let Some(r) = f.first().and_then(|s| s.trim().parse::<usize>().ok()) {
            obj.rows = r;
        }
    }
}

/// Parse a DAMOS document (both `/UMP` RAM/SFR records and multi-line `/SPZ` flash records).
pub fn parse_damos(src: &str) -> Result<DamosDocument, DamosError> {
    if src.trim().is_empty() {
        return Err(DamosError::Empty);
    }
    let mut doc = DamosDocument::default();
    let mut current_spz: Option<DamosSpzObject> = None;

    for raw in src.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }

        // A new numbered line ends any previous incomplete /SPZ block cleanly.
        if line
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_digit())
            && !line.contains("/SPZ,")
        {
            if let Some(obj) = current_spz.take() {
                doc.spz_objects.push(obj);
            }
        }

        // Header directives.
        if let Some(rest) = line.strip_prefix("/EPR,") {
            if let Some((v, _)) = brace_text(rest, 0) {
                if doc.ecu.is_none() && !v.trim().is_empty() {
                    doc.ecu = Some(v.trim().to_string());
                }
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("/PNR,") {
            if let Some((v, _)) = brace_text(rest, 0) {
                if doc.ecu.is_none() && !v.trim().is_empty() {
                    doc.ecu = Some(v.trim().to_string());
                }
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("/SGB,") {
            if let Some((v, _)) = brace_text(rest, 0) {
                doc.family = Some(v.trim().to_string());
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("/EAD,") {
            doc.base_address = parse_address(rest);
            continue;
        }

        // /UMP map entry.
        if let Some(rest) = line.strip_prefix("/UMP,") {
            if let Some(obj) = current_spz.take() {
                doc.spz_objects.push(obj);
            }
            if let Some(m) = parse_ump(rest) {
                doc.maps.push(m);
            }
            continue;
        }

        // Multi-line /SPZ block start: "<index>, /SPZ, <name>, {desc}, <kind>, $addr, $addr"
        if let Some(pos) = line.find("/SPZ,") {
            if let Some(obj) = current_spz.take() {
                doc.spz_objects.push(obj);
            }
            if let Some(c) = parse_spz(&line[pos + 5..]) {
                doc.characteristics.push(c);
            }
            if let Some(spz) = parse_spz_header(line) {
                if line.ends_with(';') {
                    doc.spz_objects.push(spz);
                } else {
                    current_spz = Some(spz);
                }
            }
            continue;
        }

        // Sub-tag lines inside an open /SPZ block.
        if let Some(ref mut obj) = current_spz {
            apply_spz_subtag(obj, line);
            if line.ends_with(';') {
                doc.spz_objects.push(current_spz.take().unwrap());
            }
        }
    }

    if let Some(obj) = current_spz.take() {
        doc.spz_objects.push(obj);
    }

    // Resolve Type 7 GroupCurve & Type 8 GroupMap dimensions from Type 6 SST axes.
    let type6_cols: std::collections::HashMap<u32, usize> = doc
        .spz_objects
        .iter()
        .filter(|o| o.spz_kind == 6 && o.cols > 0)
        .map(|o| (o.index, o.cols))
        .collect();
    for obj in &mut doc.spz_objects {
        if obj.spz_kind == 7 && obj.cols == 0 {
            if let Some(xr) = obj.x_axis_spz_ref {
                if let Some(&c) = type6_cols.get(&xr) {
                    obj.cols = c;
                }
            }
        } else if obj.spz_kind == 8 {
            if obj.cols == 0 {
                if let Some(xr) = obj.x_axis_spz_ref {
                    if let Some(&c) = type6_cols.get(&xr) {
                        obj.cols = c;
                    }
                }
            }
            if obj.rows == 0 {
                if let Some(yr) = obj.y_axis_spz_ref {
                    if let Some(&r) = type6_cols.get(&yr) {
                        obj.rows = r;
                    }
                }
            }
        }
    }

    if doc.maps.is_empty() && doc.characteristics.is_empty() && doc.spz_objects.is_empty() {
        return Err(DamosError::NoMaps);
    }
    Ok(doc)
}

/// Parse the body of a `/UMP` line.
///
/// Layout: `{flags}, name, {description}, $address, dims, ?, conversion, ?, $mask, class`
fn parse_ump(body: &str) -> Option<DamosMap> {
    let fields = split_fields(body);
    // Need at least: {}, name, {desc}, address, dims, ?, conversion
    if fields.len() < 7 {
        return None;
    }
    let name = fields[1].trim().to_string();
    if name.is_empty() {
        return None;
    }
    let description = fields[2].trim().trim_matches(|c| c == '{' || c == '}').to_string();
    let address = parse_address(&fields[3])?;

    let dims: u32 = fields[4].trim().parse().ok()?;
    let rows = ((dims >> 8) & 0xFF) as usize;
    let cols = (dims & 0xFF) as usize;
    // A zero in either axis means the word is not a dimension pair.
    if rows == 0 || cols == 0 {
        return None;
    }

    let conversion = fields[6].trim().to_string();
    // The mask is the last `$…` field; the first one is the address, which was
    // already consumed from fields[3].
    let mask = fields
        .iter()
        .skip(4)
        .filter_map(|f| f.trim().strip_prefix('$').and_then(|h| u32::from_str_radix(h, 16).ok()))
        .next_back()
        .unwrap_or(0);

    Some(DamosMap {
        name,
        description,
        address,
        rows,
        cols,
        conversion,
        mask,
    })
}

/// Parse the body of a `/SPZ` line: `name, {desc}, flags, $addr, $addr`.
fn parse_spz(body: &str) -> Option<DamosCharacteristic> {
    let fields = split_fields(body);
    if fields.len() < 2 {
        return None;
    }
    let name = fields[0].trim().to_string();
    if name.is_empty() {
        return None;
    }
    // Only `$`-prefixed fields are addresses; the bare numbers are flags
    // (`1` = element count, `0` = offset) and must not be read as addresses.
    let address = fields
        .iter()
        .skip(1)
        .find_map(|f| {
            let t = f.trim();
            t.strip_prefix('$')
                .and_then(|h| u32::from_str_radix(h, 16).ok())
        })?;
    Some(DamosCharacteristic { name, address })
}
