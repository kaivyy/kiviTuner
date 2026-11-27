//! Binary & plaintext map-pack (`.kp`) parser: plaintext `(AR hex)` packs and binary V1 (`0x71`/`0x74`) / V2 (`0x124`/`0x149` + inflated `"intern"` ZIP) formats.
//!
//! Binary codec layout ported from `xdfkit/kp` (`kp.go`, `codec.go`, `map.go`, `enums.go`).

use kivi_core::types::{DType, Endian};
use kivi_defs::model::{AxisDef, ConstantDef, Definitions, MapDef, Scaling};
use std::collections::HashMap;

// ===========================================================================
// 1. Plaintext / "(AR hex)" pack parser (preserved for backward compat)
// ===========================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapPackEntry {
    pub name: String,
    pub address: u32,
    pub unit: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MapPack {
    pub maps: Vec<MapPackEntry>,
    pub declared_ecu: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MapPackError {
    Empty,
    Opaque { entropy: f64 },
    NoMaps,
}

impl std::fmt::Display for MapPackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MapPackError::Empty => f.write_str("the pack file is empty"),
            MapPackError::Opaque { entropy } => write!(
                f,
                "the pack body is compressed or encrypted (entropy {entropy:.2} bits/byte); \
                 its codec is not available"
            ),
            MapPackError::NoMaps => f.write_str("no '(AR …)' map entries were found"),
        }
    }
}

impl std::error::Error for MapPackError {}

fn shannon_entropy(data: &[u8]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }
    let mut counts = [0usize; 256];
    for &b in data {
        counts[b as usize] += 1;
    }
    let n = data.len() as f64;
    let mut e = 0.0;
    for &c in &counts {
        if c > 0 {
            let p = c as f64 / n;
            e -= p * p.log2();
        }
    }
    e
}

const OPAQUE_ENTROPY: f64 = 6.0;

pub fn parse_map_pack(data: &[u8]) -> Result<MapPack, MapPackError> {
    if data.is_empty() {
        return Err(MapPackError::Empty);
    }
    let payload = extract_inner_zip(data).unwrap_or_else(|| data.to_vec());
    let body = if payload.len() > 1024 { &payload[1024..] } else { &payload[..] };
    let entropy = shannon_entropy(body);
    if entropy >= OPAQUE_ENTROPY {
        return Err(MapPackError::Opaque { entropy });
    }
    let text = String::from_utf8_lossy(&payload);
    let normalized: String = text
        .chars()
        .map(|c| if c == '\r' || c == '\0' { '\n' } else { c })
        .collect();
    let lines: Vec<&str> = normalized.lines().collect();

    let mut pack = MapPack::default();
    for line in lines.iter().take(16) {
        let t = line.trim();
        if is_part_number(t) {
            pack.declared_ecu = Some(t.to_string());
            break;
        }
    }
    for (i, line) in lines.iter().enumerate() {
        let Some((name, address)) = parse_ar_entry(line) else {
            continue;
        };
        let unit = lines
            .get(i + 1)
            .map(|l| l.trim())
            .filter(|l| !l.is_empty() && parse_ar_entry(l).is_none() && l.len() <= 24)
            .unwrap_or("")
            .to_string();
        pack.maps.push(MapPackEntry { name, address, unit });
    }
    if pack.maps.is_empty() {
        return Err(MapPackError::NoMaps);
    }
    Ok(pack)
}

fn parse_ar_entry(line: &str) -> Option<(String, u32)> {
    let t = line.trim();
    let open = t.find("(AR ")?;
    let close = t[open..].find(')')? + open;
    let hex = t[open + 4..close].trim();
    if hex.is_empty() || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let address = u32::from_str_radix(hex, 16).ok()?;
    let name = t[..open].trim();
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    Some((name.to_string(), address))
}

fn is_part_number(text: &str) -> bool {
    (7..=12).contains(&text.len())
        && text.chars().next().is_some_and(|c| c.is_ascii_digit())
        && text.chars().all(|c| c.is_ascii_alphanumeric())
        && text.chars().any(|c| c.is_ascii_uppercase())
}

fn extract_zip_entry_by_name(data: &[u8], expected_name: Option<&str>) -> Option<Vec<u8>> {
    // Check Central Directory first so flags bit 3 (zeroed LFH sizes) works reliably.
    if let Some(cd_pos) = data.windows(4).position(|w| w == b"PK\x01\x02") {
        let mut off = cd_pos;
        while off + 46 <= data.len() && &data[off..off + 4] == b"PK\x01\x02" {
            let method = u16::from_le_bytes([data[off + 10], data[off + 11]]);
            let comp = u32::from_le_bytes([data[off + 20], data[off + 21], data[off + 22], data[off + 23]]) as usize;
            let uncomp = u32::from_le_bytes([data[off + 24], data[off + 25], data[off + 26], data[off + 27]]) as usize;
            let nlen = u16::from_le_bytes([data[off + 28], data[off + 29]]) as usize;
            let xlen = u16::from_le_bytes([data[off + 30], data[off + 31]]) as usize;
            let clen = u16::from_le_bytes([data[off + 32], data[off + 33]]) as usize;
            let lfh = u32::from_le_bytes([data[off + 42], data[off + 43], data[off + 44], data[off + 45]]) as usize;
            let name = String::from_utf8_lossy(data.get(off + 46..off + 46 + nlen)?);
            if expected_name.is_none_or(|exp| name == exp) && lfh + 30 <= data.len() && &data[lfh..lfh + 4] == b"PK\x03\x04" {
                let ln = u16::from_le_bytes([data[lfh + 26], data[lfh + 27]]) as usize;
                let lx = u16::from_le_bytes([data[lfh + 28], data[lfh + 29]]) as usize;
                let body_start = lfh + 30 + ln + lx;
                if body_start + comp <= data.len() {
                    let raw = &data[body_start..body_start + comp];
                    return match method {
                        0 => Some(raw.to_vec()),
                        8 => miniz_oxide::inflate::decompress_to_vec(raw)
                            .ok()
                            .filter(|v| uncomp == 0 || v.len() == uncomp),
                        _ => None,
                    };
                }
            }
            off += 46 + nlen + xlen + clen;
        }
    }

    let start = data.windows(4).position(|w| w == b"PK\x03\x04")?;
    let hdr = data.get(start..start + 30)?;
    let method = u16::from_le_bytes([hdr[8], hdr[9]]);
    let comp = u32::from_le_bytes([hdr[18], hdr[19], hdr[20], hdr[21]]) as usize;
    let uncomp = u32::from_le_bytes([hdr[22], hdr[23], hdr[24], hdr[25]]) as usize;
    let nlen = u16::from_le_bytes([hdr[26], hdr[27]]) as usize;
    let xlen = u16::from_le_bytes([hdr[28], hdr[29]]) as usize;
    let body_start = start + 30 + nlen + xlen;
    let raw = data.get(body_start..body_start + comp)?;
    match method {
        0 => Some(raw.to_vec()),
        8 => miniz_oxide::inflate::decompress_to_vec(raw)
            .ok()
            .filter(|v| uncomp == 0 || v.len() == uncomp),
        _ => None,
    }
}

fn extract_inner_zip(data: &[u8]) -> Option<Vec<u8>> {
    extract_zip_entry_by_name(data, None)
}

// ===========================================================================
// 2. Binary Map-Pack (.kp V1 & V2) Decoder
// ===========================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KpLayout {
    V1,
    V2,
}

#[derive(Debug, Clone, PartialEq)]
pub struct KpValueSpec {
    pub description: String,
    pub units: String,
    pub factor: f64,
    pub offset: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct KpBinaryAxis {
    pub defined: bool,
    pub value: KpValueSpec,
    pub data_source: i32,
    pub addr: u32,
    pub elem_type: i32,
    pub width: i32,
    pub mirror: bool,
    pub precision: i32,
    pub signed: bool,
    pub values_raw: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct KpBinaryMap {
    pub index: usize,
    pub marked: bool,
    pub comment: String,
    pub name: String,
    pub organization: i32, // 2=single, 3=1d, 4=2d, 5=2d-inverse
    pub elem_type: i32,    // 1=u8, 2=16-hilo, 3=16-lohi, 4=32-hilo, 5=32-lohi, 6=float-hilo, 7=float-lohi
    pub width: i32,
    pub folder_id: i32,
    pub id: String,
    pub signed: bool,
    pub cols: i32,
    pub rows: i32,
    pub precision: i32,
    pub value: KpValueSpec,
    pub start: u32,
    pub end: u32,
    pub x: KpBinaryAxis,
    pub y: KpBinaryAxis,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KpFolder {
    pub id: i32,
    pub built_in: i32,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct KpBinaryFile {
    pub signature: String,
    pub header_id: i32,
    pub layout: KpLayout,
    pub filename: String,
    pub version: String,
    pub project_name: String,
    pub project_version: String,
    pub folders: Vec<KpFolder>,
    pub maps: Vec<KpBinaryMap>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum KpParsedAny {
    Binary(KpBinaryFile),
    PlaintextAr(MapPack),
}

struct KpCursor<'a> {
    buf: &'a [u8],
    pos: usize,
    layout: KpLayout,
}

impl<'a> KpCursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or_else(|| format!("KP offset overflow at 0x{:X}", self.pos))?;
        if end > self.buf.len() {
            return Err(format!(
                "KP short read @0x{:X}: need {n} bytes, {} left",
                self.pos,
                self.buf.len() - self.pos
            ));
        }
        let s = &self.buf[self.pos..end];
        self.pos = end;
        Ok(s)
    }

    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn bool_u8(&mut self) -> Result<bool, String> {
        let b = self.u8()?;
        if b > 1 {
            return Err(format!("KP invalid bool 0x{b:02X} @0x{:X}", self.pos - 1));
        }
        Ok(b == 1)
    }
    fn i16(&mut self) -> Result<i16, String> {
        let s = self.take(2)?;
        Ok(i16::from_le_bytes([s[0], s[1]]))
    }
    fn i32(&mut self) -> Result<i32, String> {
        let s = self.take(4)?;
        Ok(i32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(self.i32()? as u32)
    }
    fn f64(&mut self) -> Result<f64, String> {
        let s = self.take(8)?;
        Ok(f64::from_le_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]]))
    }
    fn count(&mut self) -> Result<usize, String> {
        let n = self.i32()?;
        if n < 0 {
            return Err(format!("KP negative count {n} @0x{:X}", self.pos - 4));
        }
        Ok(n as usize)
    }
    fn term(&mut self, label: &str) -> Result<(), String> {
        let v = self.i32()?;
        if v != -1 {
            return Err(format!(
                "KP missing terminator '{label}' @0x{:X} (got 0x{:08X})",
                self.pos - 4,
                v as u32
            ));
        }
        Ok(())
    }
    fn str_latin9(&mut self) -> Result<String, String> {
        let n = self.count()?;
        if n == 0 {
            return Ok(String::new());
        }
        let raw = self.take(n)?;
        let nul = self.u8()?;
        if nul != 0 {
            return Err(format!("KP string missing NUL @0x{:X}", self.pos - 1));
        }
        Ok(raw
            .iter()
            .map(|&c| match c {
                0xA4 => '€',
                0xA6 => 'Š',
                0xA8 => 'š',
                0xB4 => 'Ž',
                0xB8 => 'ž',
                0xBC => 'Œ',
                0xBD => 'œ',
                0xBE => 'Ÿ',
                other => other as char,
            })
            .collect())
    }
}

fn read_value(c: &mut KpCursor<'_>) -> Result<KpValueSpec, String> {
    Ok(KpValueSpec {
        description: c.str_latin9()?,
        units: c.str_latin9()?,
        factor: c.f64()?,
        offset: c.f64()?,
    })
}

fn read_axis(c: &mut KpCursor<'_>) -> Result<KpBinaryAxis, String> {
    let value = read_value(c)?;
    let data_source = c.i32()?;
    let addr = c.u32()?;
    let elem_type = c.i32()?;
    let width = c.i32()?;
    let _base = c.i32()?;
    let mirror = if c.layout == KpLayout::V1 {
        c.bool_u8()?
    } else {
        let v = c.i32()?;
        if v != 0 && v != 1 {
            return Err(format!("KP invalid v2 mirror {v}"));
        }
        let _ = c.take(9)?;
        v == 1
    };
    let _reciprocal = c.bool_u8()?;
    let precision = c.i32()?;
    let signed = c.bool_u8()?;
    let val_bytes = c.count()?;
    let values_raw = c.take(val_bytes)?.to_vec();
    let _data_header = c.i32()?;
    let _signature_byte = c.i32()?;
    Ok(KpBinaryAxis {
        defined: false,
        value,
        data_source,
        addr,
        elem_type,
        width,
        mirror,
        precision,
        signed,
        values_raw,
    })
}

fn read_map(c: &mut KpCursor<'_>, index: usize) -> Result<KpBinaryMap, String> {
    let marked = c.u8()? != 0;
    let comment = if c.layout == KpLayout::V2 {
        let _link_id = c.i32()?;
        let cm = c.str_latin9()?;
        let _cm_flag = c.u8()?;
        cm
    } else {
        String::new()
    };
    let name = c.str_latin9()?;
    let organization = c.i32()?;
    let _right_pane = c.i32()?;
    let elem_type = c.i32()?;
    let width = c.i32()?;
    let _base = c.i32()?;
    let folder_id = c.i32()?;
    let id = c.str_latin9()?;
    let _id_pad = c.i32()?;
    let _id_flag = c.u8()?;
    if c.layout == KpLayout::V2 {
        let _id_state = c.i32()?;
    }
    let _range = c.take(48)?; // [3][2]i64
    let _reciprocal = c.bool_u8()?;
    let signed = c.bool_u8()?;
    let _difference = c.bool_u8()?;
    let _percent = c.bool_u8()?;
    let cols = c.i32()?;
    let rows = c.i32()?;
    let _cursor = c.take(8)?; // [2]i32
    let precision = c.i32()?;
    let value = read_value(c)?;
    let start = c.u32()?;
    let end = c.u32()?;
    let _image_size = c.i32()?;
    if c.layout == KpLayout::V2 {
        let _ = c.take(8)?; // [2]i32 ImageSizePad
    }
    let _addr2 = c.u32()?;
    let _addr2_flags = c.take(8)?;
    let _addr2_image_size = c.i32()?;
    let _addr2_pad = c.i32()?;
    let mut x = read_axis(c)?;
    let mut y = read_axis(c)?;
    let _y_flag = c.i32()?;
    let _y_pad = c.i16()?;
    let _stored_cols = c.i32()?;
    let _stored_rows = c.i32()?;
    let _view_mode = c.i32()?;
    let _view_flags = c.take(2)?;
    let _view_2d_scale = c.take(16)?;
    let _view_2d_ref = c.i32()?;
    let _view_flag = c.u8()?;
    let _view_3d_rot = c.take(16)?;
    let _view_scale = c.f64()?;
    let _view_scale_ref = c.i32()?;
    let _view_offset = c.f64()?;
    let term2 = c.take(3)?;
    if term2[0] != 1 || term2[1] != 1 {
        return Err(format!("KP map #{index} ('{name}') invalid term2 {term2:?}"));
    }

    let two_d = organization == 4 || organization == 5;
    x.defined = two_d || organization == 3;
    y.defined = two_d;

    Ok(KpBinaryMap {
        index,
        marked,
        comment,
        name,
        organization,
        elem_type,
        width,
        folder_id,
        id,
        signed,
        cols,
        rows,
        precision,
        value,
        start,
        end,
        x,
        y,
    })
}

fn kp_type_to_core(elem_type: i32, signed: bool) -> (DType, Endian) {
    let endian = if elem_type > 1 && (elem_type & 1) == 1 {
        Endian::Little
    } else {
        Endian::Big
    };
    let dtype = match elem_type {
        1 => if signed { DType::I8 } else { DType::U8 },
        2 | 3 => if signed { DType::I16 } else { DType::U16 },
        4 | 5 => if signed { DType::I32 } else { DType::U32 },
        6 | 7 => DType::F32,
        _ => DType::U8,
    };
    (dtype, endian)
}

fn kp_scaling(v: &KpValueSpec, precision: i32) -> Scaling {
    let eq = if (v.factor - 1.0).abs() < 1e-12 && v.offset.abs() < 1e-12 {
        "X".to_string()
    } else if v.offset.abs() < 1e-12 {
        format!("X * {}", v.factor)
    } else {
        format!("X * {} + {}", v.factor, v.offset)
    };
    Scaling {
        equation: eq,
        unit: v.units.clone(),
        decimals: precision.clamp(0, 6) as u8,
    }
}

impl KpBinaryFile {
    /// Converts binary KP folders, constants, and 1D/2D maps to [`Definitions`].
    pub fn to_definitions(&self) -> Definitions {
        let folder_names: HashMap<i32, &str> = self
            .folders
            .iter()
            .map(|f| (f.id, f.name.as_str()))
            .collect();
        let mut defs = Definitions::default();

        for m in &self.maps {
            let category = folder_names
                .get(&m.folder_id)
                .map(|s| (*s).to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "KP Map Pack".to_string());
            let (dtype, endian) = kp_type_to_core(m.elem_type, m.signed);
            let scaling = kp_scaling(&m.value, m.precision);
            let rows = m.rows.max(1) as usize;
            let cols = m.cols.max(1) as usize;

            if m.organization == 2 || (rows == 1 && cols == 1 && !m.x.defined) {
                defs.constants.push(ConstantDef {
                    id: format!("kp-{}-0x{:x}", m.index, m.start),
                    name: m.name.clone(),
                    category,
                    address: m.start as usize,
                    dtype,
                    endian,
                    scaling,
                });
                continue;
            }

            let map_axis = |ax: &KpBinaryAxis, len: usize| -> Option<AxisDef> {
                if !ax.defined {
                    return None;
                }
                let (adt, aend) = kp_type_to_core(ax.elem_type, ax.signed);
                let mut values = Vec::new();
                if ax.data_source == 4 && !ax.values_raw.is_empty() {
                    let elem_sz = adt.byte_size().max(1);
                    for chunk in ax.values_raw.chunks_exact(elem_sz) {
                        if let Ok(v) = adt.read_f64(chunk, aend) {
                            values.push(v * ax.value.factor + ax.value.offset);
                        }
                    }
                }
                Some(AxisDef {
                    address: if matches!(ax.data_source, 1..=3 | 5) {
                        ax.addr as usize
                    } else {
                        0
                    },
                    length: len,
                    dtype: adt,
                    endian: aend,
                    scaling: kp_scaling(&ax.value, ax.precision),
                    values,
                })
            };

            defs.maps.push(MapDef {
                id: format!("kp-{}-0x{:x}", m.index, m.start),
                name: m.name.clone(),
                category,
                address: m.start as usize,
                rows,
                cols,
                dtype,
                endian,
                column_major: m.organization == 5,
                major_stride_bits: 0,
                minor_stride_bits: 0,
                scaling,
                x_axis: map_axis(&m.x, cols),
                y_axis: map_axis(&m.y, rows),
            });
        }
        defs
    }
}

/// Parses a binary `.kp` map pack (`V1` or `V2`).
pub fn parse_kp_binary(data: &[u8]) -> Result<KpBinaryFile, String> {
    let mut c = KpCursor {
        buf: data,
        pos: 0,
        layout: KpLayout::V1,
    };
    let signature = c.str_latin9()?;
    let header_id = c.i32()?;
    let _header_len = c.i32()?;
    let layout = match header_id {
        0x71 | 0x74 => KpLayout::V1,
        0x124 | 0x149 => KpLayout::V2,
        other => return Err(format!("KP unsupported header ID 0x{other:X}")),
    };
    c.layout = layout;
    let fixed_end = if layout == KpLayout::V1 { 0x5Cusize } else { 0x60usize };
    if fixed_end < c.pos {
        return Err("KP fixed header size mismatch".to_string());
    }
    let _ = c.take(fixed_end - c.pos)?;
    let filename = c.str_latin9()?;
    let version = c.str_latin9()?;

    // walkHeader
    let _ = c.take(6 * 4 + 2 + 4)?;
    c.term("1")?;
    let _ = c.take(7 * 4 + 3 * 4 + 23 * 4 + 4 + 4 * 4 + 2 * 4 + 2 + 1)?;
    c.term("2")?;
    if layout == KpLayout::V1 {
        let _ = c.take(7 * 4 + 1)?;
    } else {
        let _ = c.take(1 + 6 * 4)?;
        let _ = c.str_latin9()?;
        let _ = c.str_latin9()?;
        let _ = c.take(10 * 4)?;
        c.term("2a")?;
        let _ = c.take(1 + 12 * 4)?;
        let _ = c.str_latin9()?;
        let _ = c.take(10 * 4 + 1)?;
    }
    let _ = c.take(0x14 * 2 + 0x10 * 2 + 18 * 4)?;
    c.term("3")?;
    let _ = c.take(4)?;
    if layout == KpLayout::V2 {
        let _ = c.take(8)?;
    }

    let _end_offset = c.i32()?;
    let _end_tag = c.take(12)?;

    // Project
    let project_name = c.str_latin9()?;
    let _ = c.take(16)?; // [4]i32
    if layout == KpLayout::V2 {
        let _ = c.take(4)?;
    }
    let project_version = c.str_latin9()?;
    let _ = c.take(16)?;
    if layout == KpLayout::V2 {
        let _ = c.take(4)?;
    }
    let _build = c.i32()?;
    if layout == KpLayout::V2 {
        let _ = c.take(1)?;
    }

    let maps = if layout == KpLayout::V1 {
        let n = c.count()?;
        let mut ms = Vec::with_capacity(n);
        for i in 0..n {
            ms.push(read_map(&mut c, i)?);
        }
        ms
    } else {
        let zlen = c.count()?;
        let zip_bytes = c.take(zlen)?;
        let intern = extract_zip_entry_by_name(zip_bytes, Some("intern"))
            .ok_or_else(|| "KP v2: failed to inflate 'intern' ZIP entry".to_string())?;
        let mut ic = KpCursor {
            buf: &intern,
            pos: 0,
            layout: KpLayout::V2,
        };
        let _intern_flag = ic.u8()?;
        let n = ic.count()?;
        let mut ms = Vec::with_capacity(n);
        for i in 0..n {
            ms.push(read_map(&mut ic, i)?);
        }
        ms
    };

    let _maps_tag = c.take(12)?;
    let nfolders = c.count()?;
    let mut folders = Vec::with_capacity(nfolders);
    for _ in 0..nfolders {
        let id = c.i32()?;
        let built_in = c.i32()?;
        let name = c.str_latin9()?;
        let _flags = c.take(2)?;
        let _style = c.i32()?;
        if layout == KpLayout::V2 {
            let _ = c.take(15)?;
        }
        folders.push(KpFolder { id, built_in, name });
    }

    Ok(KpBinaryFile {
        signature,
        header_id,
        layout,
        filename,
        version,
        project_name,
        project_version,
        folders,
        maps,
    })
}

/// Parses either a binary `.kp` pack or falls back to plaintext `(AR hex)`.
pub fn parse_kp_any(data: &[u8]) -> Result<KpParsedAny, String> {
    if let Ok(bin) = parse_kp_binary(data) {
        return Ok(KpParsedAny::Binary(bin));
    }
    parse_map_pack(data)
        .map(KpParsedAny::PlaintextAr)
        .map_err(|e| e.to_string())
}

/// Exports map definitions in plaintext `(AR hex)` map-pack notation.
pub fn export_kp_ar_text(defs: &Definitions, ecu_id: Option<&str>) -> String {
    use std::fmt::Write as _;

    let mut out = String::with_capacity(defs.maps.len().saturating_mul(64) + 64);
    if let Some(id) = ecu_id.map(str::trim).filter(|s| !s.is_empty()) {
        let _ = writeln!(out, "{id}");
    }
    for (idx, m) in defs.maps.iter().enumerate() {
        let mut name: String = m
            .name
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
            .collect();
        while name.contains("__") {
            name = name.replace("__", "_");
        }
        let name = name.trim_matches('_');
        let resolved = if name.is_empty() {
            format!("Map_{idx}")
        } else {
            name.to_string()
        };
        let _ = writeln!(out, "{resolved} (AR {:X})", m.address);
        let unit = m.scaling.unit.trim();
        let unit = if unit.is_empty() || unit.len() > 24 {
            "raw"
        } else {
            unit
        };
        let _ = writeln!(out, "{unit}");
    }
    out
}

#[cfg(test)]
mod export_tests {
    use super::*;

    #[test]
    fn kp_ar_export_roundtrips_through_parse_map_pack() {
        let defs = Definitions {
            maps: vec![MapDef {
                id: "kfzw".to_string(),
                name: "KFZW Ignition".to_string(),
                category: "Ignition".to_string(),
                address: 0x1A400,
                rows: 12,
                cols: 16,
                dtype: DType::U8,
                endian: Endian::Little,
                column_major: false,
                major_stride_bits: 0,
                minor_stride_bits: 0,
                scaling: Scaling {
                    equation: "X * 0.75".to_string(),
                    unit: "deg".to_string(),
                    decimals: 2,
                },
                x_axis: None,
                y_axis: None,
            }],
            ..Default::default()
        };

        let text = export_kp_ar_text(&defs, Some("06A906032HS"));
        let parsed = parse_map_pack(text.as_bytes()).expect("roundtrip kp ar");
        assert_eq!(parsed.declared_ecu.as_deref(), Some("06A906032HS"));
        assert_eq!(parsed.maps.len(), 1);
        assert_eq!(parsed.maps[0].name, "KFZW_Ignition");
        assert_eq!(parsed.maps[0].address, 0x1A400);
        assert_eq!(parsed.maps[0].unit, "deg");
    }
}
