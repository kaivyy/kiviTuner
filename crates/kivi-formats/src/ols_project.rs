//! Binary `.ols` project file parser (schema v100-v834+).
//!
//! Ported from `ols-parser/ols.c`. Extracts header/vehicle metadata, embedded ROM
//! segments (`0xFADECAFE` / `0xCAFEAFE` descriptors & legacy version table), assembled
//! ROM images, and sequential map/axis definitions.

const OLS_MAGIC: u32 = 0x0000_000B;
const OLS_SIG: &[u8; 11] = b"WinOLS File";
const OLS_HDR_SIZE: usize = 24;
const MODERN_FMT_MIN: u32 = 200;

const SENTINEL_A: u32 = 0xFADE_CAFE;
const SENTINEL_B: u32 = 0xCAFE_AFFE;
const POSTZIP_HDR: u32 = 0x9863_8811;
const M1: u32 = 0x4200_7899;

const DESC_PRE_OFF: usize = 20;
const PROJ_SLOT_LEN: usize = 18;
const PROJ_TAG_LEN: usize = 10;
const DATA_START_PRE: usize = 26;
const FRAMING_BYTES: usize = 76;
const MAX_SEG_SIZE: u32 = 16 * 1024 * 1024;
const DIM_A276_MAX: u32 = 0x4000;
const DIM_A396_MAX: u32 = 0x400;
const DIM_A400_MAX: u32 = 0x1F400;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OlsHeader {
    pub magic: u32,
    pub format_version: u32,
    pub declared_size: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OlsMetadata {
    pub make: String,
    pub model: String,
    pub vehicle_type: String,
    pub year: String,
    pub output: String,
    pub cylinders: String,
    pub country: String,
    pub drivetrain: String,
    pub memory: String,
    pub manufacturer: String,
    pub ecu_name: String,
    pub hw_number: String,
    pub sw_number: String,
    pub production_no: String,
    pub engine_code: String,
    pub transmission: String,
    pub last_write_time: u64,
    pub original_file_name: String,
    pub tool_version_string: String,
    pub revision_tag: String,
    pub base_address_hex: String,
    pub build_number: Option<u32>,
    pub checksum: Option<u64>,
    pub flags: Option<u32>,
    pub import_comment: String,
    pub post_comment_flag: Option<u32>,
    pub tag: String,
    pub region_count: Option<u32>,
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OlsSegment {
    pub is_primary: bool,
    pub flash_base: u32,
    pub flash_size: u32,
    pub hash: u32,
    pub framing_bytes: usize,
    pub data_start: usize,
    pub proj_tag: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OlsVersionImage {
    pub index: usize,
    pub name: String,
    pub segments: Vec<OlsSegment>,
    pub base_address: u32,
    pub rom_data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct OlsAxis {
    pub input_name: String,
    pub unit: String,
    pub pts_address: u32,
    pub pts_data_type: u32,
    pub pts_data_size: usize,
    pub pts_signed: bool,
    pub pts_big_endian: bool,
    pub has_pts_address: bool,
    pub has_scaling: bool,
    pub scale: f64,
    pub offset: f64,
    pub pts_count: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OlsMap {
    pub name: String,
    pub description: String,
    pub unit: String,
    pub kind: String,
    pub raw_address: u32,
    pub address: u32,
    pub rom_end: u32,
    pub nx: usize,
    pub ny: usize,
    pub data_size: usize,
    pub data_signed: bool,
    pub cell_big_endian: bool,
    pub cell_data_type: u32,
    pub column_major: bool,
    pub has_scaling: bool,
    pub scale: f64,
    pub offset: f64,
    pub x_axis: OlsAxis,
    pub y_axis: Option<OlsAxis>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct OlsProject {
    pub header: OlsHeader,
    pub metadata: OlsMetadata,
    pub versions: Vec<OlsVersionImage>,
    pub maps: Vec<OlsMap>,
    pub warnings: Vec<String>,
}

#[inline]
fn rd32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

#[inline]
fn rd64(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3], b[o + 4], b[o + 5], b[o + 6], b[o + 7]])
}

#[inline]
fn rdf64(b: &[u8], o: usize) -> f64 {
    f64::from_bits(rd64(b, o))
}

struct Cursor<'a> {
    buf: &'a [u8],
    off: usize,
    schema: u32,
}

impl<'a> Cursor<'a> {
    fn need(&self, n: usize) -> bool {
        self.off.checked_add(n).is_some_and(|e| e <= self.buf.len())
    }
    fn u8(&mut self) -> Option<u8> {
        if !self.need(1) {
            return None;
        }
        let v = self.buf[self.off];
        self.off += 1;
        Some(v)
    }
    fn u32(&mut self) -> Option<u32> {
        if !self.need(4) {
            return None;
        }
        let v = rd32(self.buf, self.off);
        self.off += 4;
        Some(v)
    }
    fn u64(&mut self) -> Option<u64> {
        if !self.need(8) {
            return None;
        }
        let v = rd64(self.buf, self.off);
        self.off += 8;
        Some(v)
    }
    fn f64(&mut self) -> Option<f64> {
        if !self.need(8) {
            return None;
        }
        let v = rdf64(self.buf, self.off);
        self.off += 8;
        Some(v)
    }
    fn cstr(&mut self, max_len: usize) -> Option<String> {
        let (s, end) = read_cstring(self.buf, self.off, self.schema, max_len)?;
        self.off = end;
        Some(s)
    }
    fn cfr(&mut self) -> Option<String> {
        let (s, end) = read_cfolderref(self.buf, self.off, self.schema)?;
        self.off = end;
        Some(s)
    }
}

fn is_text(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }
    let ok = bytes
        .iter()
        .filter(|&&b| (32..127).contains(&b) || b == 9 || b == 10 || b == 13 || b >= 0x80)
        .count();
    ok * 100 >= bytes.len() * 85
}

fn is_ident(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() || bytes.len() > 40 {
        return false;
    }
    let first = bytes[0];
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return false;
    }
    bytes[1..].iter().all(|&c| c.is_ascii_alphanumeric() || c == b'_')
}

fn latin1_string(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| b as char).collect()
}

fn read_cstring(buf: &[u8], off: usize, schema: u32, max_len: usize) -> Option<(String, usize)> {
    if off + 4 > buf.len() {
        return None;
    }
    let slen = rd32(buf, off) as i32;
    if schema >= 439 {
        if slen == -1 {
            return Some(("-".to_string(), off + 4));
        }
        if slen == -2 {
            return Some(("?".to_string(), off + 4));
        }
        if slen == -3 {
            return Some(("%".to_string(), off + 4));
        }
        if slen <= 0 {
            return Some((String::new(), off + 4));
        }
        let n = slen as usize;
        if n > max_len || off + 4 + n > buf.len() || !is_text(&buf[off + 4..off + 4 + n]) {
            return None;
        }
        return Some((latin1_string(&buf[off + 4..off + 4 + n]), off + 4 + n));
    }
    if slen < 0 || slen as usize > max_len {
        return None;
    }
    if slen == 0 {
        return Some((String::new(), off + 4));
    }
    let n = slen as usize;
    let need = 4 + n + 1;
    if off + need > buf.len() || !is_text(&buf[off + 4..off + 4 + n]) || buf[off + 4 + n] != 0 {
        return None;
    }
    Some((latin1_string(&buf[off + 4..off + 4 + n]), off + need))
}

fn read_cfolderref(buf: &[u8], off: usize, schema: u32) -> Option<(String, usize)> {
    let (name, mut o) = read_cstring(buf, off, schema, 8192)?;
    if schema < 330 {
        return Some((name, o));
    }
    if schema >= 345 {
        if o + 8 > buf.len() {
            return None;
        }
        o += 8;
    }
    if o + 4 > buf.len() {
        return None;
    }
    let count = rd32(buf, o);
    o += 4;
    if count > 0x1000 {
        return None;
    }
    if schema >= 345 {
        for _ in 0..count {
            let (_, next) = read_cstring(buf, o, schema, 8192)?;
            o = next;
            if o + 4 > buf.len() {
                return None;
            }
            o += 4;
        }
    }
    Some((name, o))
}

fn find_u32_le(buf: &[u8], val: u32, from: usize) -> Option<usize> {
    let needle = val.to_le_bytes();
    buf.get(from..)?
        .windows(4)
        .position(|w| w == needle)
        .map(|p| from + p)
}

fn valid_proj_slot(buf: &[u8], off: usize) -> bool {
    let Some(slice) = buf.get(off..off + PROJ_TAG_LEN) else {
        return false;
    };
    slice.iter().all(|&b| b == 0xAF) || slice.iter().all(|&b| (0x20..0x7F).contains(&b))
}

fn decode_proj_tag(buf: &[u8], off: usize) -> String {
    buf.get(off..off + PROJ_TAG_LEN)
        .map(|s| {
            s.iter()
                .filter(|&&b| (0x20..0x7F).contains(&b) && b != 0xAF)
                .map(|&b| b as char)
                .collect()
        })
        .unwrap_or_default()
}

fn parse_descriptor(buf: &[u8], fade_off: usize) -> Option<OlsSegment> {
    let desc_off = fade_off.checked_sub(DESC_PRE_OFF)?;
    if desc_off < PROJ_SLOT_LEN || desc_off + 32 > buf.len() {
        return None;
    }
    let mut d = [0u32; 8];
    for (i, slot) in d.iter_mut().enumerate() {
        *slot = rd32(buf, desc_off + i * 4);
    }
    if d[5] != SENTINEL_A || d[6] != SENTINEL_B {
        return None;
    }
    let fb = d[3] & 0x7FFF_FFFF;
    let fe = d[4] & 0x7FFF_FFFF;
    if fb >= fe {
        return None;
    }
    let sz = fe - fb + 1;
    if sz > MAX_SEG_SIZE {
        return None;
    }
    let proj_off = desc_off - PROJ_SLOT_LEN;
    if !valid_proj_slot(buf, proj_off) {
        return None;
    }
    Some(OlsSegment {
        is_primary: true,
        flash_base: fb,
        flash_size: sz,
        hash: d[1],
        framing_bytes: if sz as usize > FRAMING_BYTES { FRAMING_BYTES } else { 0 },
        data_start: proj_off.saturating_sub(DATA_START_PRE),
        proj_tag: decode_proj_tag(buf, proj_off),
    })
}

fn find_segments(buf: &[u8]) -> Vec<OlsSegment> {
    let mut segs = Vec::new();
    let mut pos = 0usize;
    while let Some(j) = find_u32_le(buf, SENTINEL_A, pos) {
        if j + 8 <= buf.len() && rd32(buf, j + 4) == SENTINEL_B {
            if let Some(seg) = parse_descriptor(buf, j) {
                segs.push(seg);
            }
        }
        pos = j + 1;
    }
    let mut prev_end = 0u32;
    let mut have_prev = false;
    for seg in &mut segs {
        let ds = seg.data_start as u32;
        let primary = !have_prev || ds >= prev_end;
        seg.is_primary = primary;
        if primary {
            prev_end = ds.saturating_add(seg.flash_size);
            have_prev = true;
        }
    }
    segs
}

fn legacy_segments(buf: &[u8]) -> Vec<OlsSegment> {
    let Some(m1) = find_u32_le(buf, M1, 0) else {
        return Vec::new();
    };
    if m1 < 12 || m1 + 8 > buf.len() {
        return Vec::new();
    }
    let hdr = m1 - 12;
    let nver = rd32(buf, hdr).saturating_add(1);
    let vstart = rd32(buf, hdr + 4) as usize;
    let vrec = rd32(buf, hdr + 8);
    if nver == 0 || nver > 64 || vrec == 0 || vrec > 64 * 1024 * 1024 {
        return Vec::new();
    }
    let data_start = m1 + 4 + vstart.min(buf.len() - (m1 + 4));
    (0..nver as usize)
        .map(|i| OlsSegment {
            is_primary: i == 0,
            flash_base: (data_start + i * vrec as usize) as u32,
            flash_size: vrec,
            hash: 0,
            framing_bytes: 0,
            data_start: data_start + i * vrec as usize,
            proj_tag: String::new(),
        })
        .collect()
}

fn assemble_rom(buf: &[u8], segs: &[OlsSegment]) -> (Vec<u8>, u32) {
    let mut min_base = u32::MAX;
    let mut max_end = 0u32;
    for s in segs {
        if s.flash_size as usize <= s.framing_bytes {
            continue;
        }
        let eb = s.flash_base.saturating_add(s.framing_bytes as u32);
        let ee = s.flash_base.saturating_add(s.flash_size);
        min_base = min_base.min(eb);
        max_end = max_end.max(ee);
    }
    if max_end <= min_base || (max_end - min_base) > 32 * 1024 * 1024 {
        return (Vec::new(), 0);
    }
    let rom_sz = (max_end - min_base) as usize;
    let mut rom = vec![0xFFu8; rom_sz];
    for s in segs {
        if s.flash_size as usize <= s.framing_bytes {
            continue;
        }
        let eb = s.flash_base + s.framing_bytes as u32;
        let ss = (s.flash_size as usize - s.framing_bytes).min(rom_sz);
        let dst = (eb - min_base) as usize;
        let src = s.data_start + s.framing_bytes;
        if src >= buf.len() || dst >= rom.len() {
            continue;
        }
        let copy_len = ss.min(buf.len() - src).min(rom.len() - dst);
        rom[dst..dst + copy_len].copy_from_slice(&buf[src..src + copy_len]);
    }
    (rom, min_base)
}

fn bytes_from_dtype(dt: u32) -> usize {
    match dt {
        2 | 3 => 2,
        4..=7 => 4,
        10..=13 => 8,
        _ => 1,
    }
}

fn is_be_dtype(dt: u32) -> bool {
    matches!(dt, 2 | 4 | 6 | 10 | 12)
}

fn cb_to_bytes(cb: u32) -> usize {
    match cb {
        2 | 8 => 1,
        10 | 16 => 2,
        32 => 4,
        _ => 2,
    }
}

fn read_axis(buf: &[u8], start: usize, schema: u32, expected_sz: usize) -> Option<(OlsAxis, usize)> {
    let mut c = Cursor { buf, off: start, schema };
    let input_name = c.cfr()?;
    let unit = c.cstr(8192)?;
    let scale = c.f64()?;
    let offset = c.f64()?;
    let _axis_type = c.u32()?;
    let axis_rom = c.u32()?;
    let axis_dt = c.u32()?;
    let _u120 = c.u32()?;
    let mut cell_bits = c.u32()?;
    if !matches!(cell_bits, 2 | 10 | 16) {
        cell_bits = 10;
    }
    if !c.need(2) {
        return None;
    }
    c.off += 2;
    if schema >= 264 { c.f64()?; }
    if schema >= 241 { c.u32()?; }
    if schema >= 8 { c.u32()?; }
    let flag3 = if schema >= 12 { c.u8()? != 0 } else { false };
    if schema >= 73 {
        let inline_count = c.u32()? as usize;
        if inline_count > 0x10000 || !c.need(inline_count) {
            return None;
        }
        c.off += inline_count;
    }
    if schema >= 77 { c.u32()?; }
    let axis_rom_v2 = if schema >= 91 { c.u32()? } else { 0 };
    if schema >= 354 {
        c.cstr(8192)?;
    }
    if schema >= 372 {
        let fc = c.u32()?;
        if fc > 0x1000 {
            return None;
        }
        for _ in 0..fc {
            c.cfr()?;
        }
    }
    if schema >= 440 { c.u32()?; }

    let rom = if axis_rom != 0 && axis_rom != u32::MAX {
        axis_rom
    } else if axis_rom_v2 != 0 && axis_rom_v2 != u32::MAX {
        axis_rom_v2
    } else {
        0
    };
    let ax = OlsAxis {
        input_name,
        unit,
        pts_address: rom,
        pts_data_type: axis_dt,
        pts_data_size: if axis_dt != 0 { bytes_from_dtype(axis_dt) } else { cb_to_bytes(cell_bits) },
        pts_signed: flag3,
        pts_big_endian: is_be_dtype(axis_dt),
        has_pts_address: rom > 0 && rom != u32::MAX,
        has_scaling: scale != 1.0 || offset != 0.0,
        scale,
        offset,
        pts_count: expected_sz,
    };
    Some((ax, c.off))
}

fn classify_dims(kt: u32, a276: u32, a280: u32, mut a396: u32, mut a400: u32) -> (usize, usize) {
    let mut x276 = if a276 > DIM_A276_MAX { 0 } else { a276 };
    if a396 > DIM_A396_MAX { a396 = 0; }
    if a400 > DIM_A400_MAX { a400 = 0; }
    if kt == 1 || kt == 2 {
        return (1, 1);
    }
    if kt == 3 {
        let x = if a396 > 0 {
            a396
        } else {
            let m = x276.max(a280);
            if m > 0 { m } else { 1 }
        };
        return (x as usize, 1);
    }
    if kt == 5 {
        if a400 > 0 || a396 > 0 {
            return (a400.max(1) as usize, a396.max(1) as usize);
        }
        return (a280.max(1) as usize, x276.max(1) as usize);
    }
    if a400 > 0 && a396 > 0 {
        return (a400 as usize, a396 as usize);
    }
    if a280 > 0 || x276 > 0 {
        x276 = x276.max(1);
        return (a280.max(1) as usize, x276 as usize);
    }
    (1, 1)
}

fn is_envelope_string(s: &str) -> bool {
    matches!(
        s,
        "(DAMOS)"
            | "OLS 5.0 (Windows)"
            | "Hexdump"
            | "My maps"
            | "Original"
            | "Passenger car"
            | "Engine"
            | "Binary data"
            | "0 (Original)"
            | "Version 1"
    )
}

fn parse_map(buf: &[u8], off: usize, hard_limit: usize, schema: u32) -> Option<(OlsMap, usize)> {
    let mut c = Cursor { buf, off, schema };
    let desc = c.cfr()?;
    if is_envelope_string(&desc) || desc.starts_with("Imported from") {
        return None;
    }
    if !desc.is_empty() && !is_text(desc.as_bytes()) {
        return None;
    }
    if !c.need(20) {
        return None;
    }
    let kt = c.u32()?;
    c.u32()?;
    let cell_dt = c.u32()?;
    c.u32()?;
    let mut cell_bits = c.u32()?;
    if !matches!(cell_bits, 2 | 10 | 16) {
        cell_bits = 10;
    }
    if schema < 80 || !c.need(8) {
        return None;
    }
    c.u32()?;
    let name = c.cstr(8192)?;
    if !is_ident(&name) {
        return None;
    }
    if schema >= 298 { c.u32()?; }
    if schema >= 299 { c.u32()?; }
    if schema >= 94 { c.u32()?; }
    if schema >= 74 { c.u8()?; }
    if schema >= 123 { c.u32()?; }
    let skip_after = if schema >= 300 {
        48 + if schema >= 67 { 48 } else { 0 }
    } else if schema >= 67 {
        48
    } else if schema >= 66 {
        16
    } else if schema < 59 {
        8
    } else {
        0
    };
    if !c.need(skip_after + 24) {
        return None;
    }
    c.off += skip_after;
    c.u8()?;
    let cell_signed = c.u8()? != 0;
    c.u8()?;
    c.u8()?;
    let a276 = c.u32()?;
    let a280 = c.u32()?;
    let a396 = c.u32()?;
    let a400 = c.u32()?;
    c.u32()?;

    let (x_sz, y_sz) = classify_dims(kt, a276, a280, a396, a400);
    if x_sz == 0 || y_sz == 0 || x_sz > DIM_A400_MAX as usize || y_sz > DIM_A396_MAX as usize {
        return None;
    }
    let label = c.cstr(8192)?;
    let unit = c.cstr(8192)?;
    let scale = c.f64()?;
    let offset = c.f64()?;
    let rom_addr = c.u32()?;
    let rom_end = c.u32()?;
    c.u32()?;
    if schema >= 264 { c.f64()?; }
    if schema >= 61 { c.u32()?; }
    if schema >= 105 {
        if !c.need(16) {
            return None;
        }
        c.off += 16;
    }

    let (y_axis, next_y) = read_axis(buf, c.off, schema, y_sz)?;
    let (x_axis, next_x) = read_axis(buf, next_y, schema, x_sz)?;
    c.off = next_x;

    let trailer_end = (c.off + 0x800).min(hard_limit).min(buf.len());
    let trailer = buf[c.off..trailer_end]
        .windows(3)
        .position(|w| w == [1, 1, 1])
        .map(|p| c.off + p + 3)
        .unwrap_or(c.off);

    let mut cb = if cell_dt != 0 {
        bytes_from_dtype(cell_dt)
    } else {
        let mut w = cb_to_bytes(cell_bits);
        if rom_end > rom_addr {
            let cs = (rom_end - rom_addr) as usize / (x_sz * y_sz);
            if matches!(cs, 1 | 2 | 4) {
                w = cs;
            }
        }
        w
    };
    if cb == 0 {
        cb = 1;
    }
    let kind = if x_sz <= 1 && y_sz <= 1 {
        "VALUE"
    } else if y_sz <= 1 {
        "CURVE"
    } else {
        "MAP"
    };
    let map = OlsMap {
        name,
        description: desc,
        unit: if unit.is_empty() { label } else { unit },
        kind: kind.to_string(),
        raw_address: rom_addr,
        address: rom_addr,
        rom_end,
        nx: x_sz,
        ny: y_sz,
        data_size: cb,
        data_signed: if cell_dt != 0 { cell_signed } else { (cb == 1 && cell_bits == 10) || offset < 0.0 },
        cell_big_endian: is_be_dtype(cell_dt),
        cell_data_type: cell_dt,
        column_major: true,
        has_scaling: (scale != 0.0 && scale != 1.0) || offset != 0.0,
        scale: if scale != 0.0 { scale } else { 1.0 },
        offset,
        x_axis,
        y_axis: if y_sz > 1 { Some(y_axis) } else { None },
    };
    Some((map, trailer))
}

fn parse_metadata(buf: &[u8], off: usize, schema: u32, warnings: &mut Vec<String>) -> OlsMetadata {
    let mut c = Cursor { buf, off, schema };
    let mut m = OlsMetadata::default();
    macro_rules! read_s {
        ($field:ident, $label:expr) => {
            if let Some(s) = c.cstr(8192) {
                m.$field = s;
            } else {
                warnings.push(concat!("metadata stopped at ", $label).to_string());
                return m;
            }
        };
    }
    read_s!(make, "Make");
    read_s!(model, "Model");
    read_s!(vehicle_type, "Type");
    read_s!(year, "Year");
    read_s!(output, "Output");
    read_s!(cylinders, "Cylinders");
    read_s!(country, "Country");
    read_s!(drivetrain, "Drivetrain");
    read_s!(memory, "Memory");
    read_s!(manufacturer, "Manufacturer");
    read_s!(ecu_name, "EcuName");
    read_s!(hw_number, "HwNumber");
    read_s!(sw_number, "SwNumber");
    read_s!(production_no, "ProductionNo");
    read_s!(engine_code, "EngineCode");
    read_s!(transmission, "Transmission");
    let Some(lwt) = c.u64() else { return m };
    m.last_write_time = lwt;
    read_s!(original_file_name, "OriginalFileName");
    read_s!(tool_version_string, "ToolVersionString");
    let _ = c.cstr(8192);
    read_s!(revision_tag, "RevisionTag");
    let _ = (c.cstr(8192), c.cstr(8192), c.cstr(8192));
    if schema >= 0x6E {
        if let Some(s) = c.cstr(8192) {
            m.base_address_hex = s;
        }
    }
    if schema >= 0x30 {
        m.build_number = c.u32();
    }
    if schema >= 0x34 {
        m.checksum = c.u64();
    }
    if schema >= 0x38 {
        m.flags = c.u32();
    }
    if schema >= 0x3E {
        if let Some(s) = c.cstr(8192) {
            m.import_comment = s;
        }
    }
    if schema >= 0x3F {
        m.post_comment_flag = c.u32();
    }
    if schema >= 0x40 {
        if let Some(s) = c.cstr(8192) {
            m.tag = s;
        }
    }
    if schema >= 0x4F {
        m.region_count = c.u32();
    }
    if schema >= 0x55 {
        if let Some(s) = c.cstr(8192) {
            m.notes = s;
        }
    }
    m
}

/// Parses a binary `.ols` project file from bytes.
pub fn parse_ols(data: &[u8]) -> Result<OlsProject, String> {
    if data.len() < OLS_HDR_SIZE {
        return Err("File too small for .ols header".to_string());
    }
    let magic = rd32(data, 0);
    if magic != OLS_MAGIC || &data[4..15] != OLS_SIG || data[0x0F] != 0 {
        return Err("Invalid .ols header magic or signature".to_string());
    }
    let header = OlsHeader {
        magic,
        format_version: rd32(data, 0x10),
        declared_size: rd32(data, 0x14),
    };
    let schema = header.format_version;
    let mut warnings = Vec::new();
    if schema >= MODERN_FMT_MIN && header.declared_size as usize != data.len() {
        warnings.push(format!(
            "Declared .ols size {} differs from actual length {}",
            header.declared_size,
            data.len()
        ));
    }

    let meta_off = if schema >= MODERN_FMT_MIN { 0x18 } else { 0x14 };
    let metadata = parse_metadata(data, meta_off, schema, &mut warnings);

    let segs = if schema >= MODERN_FMT_MIN {
        let found = find_segments(data);
        if found.is_empty() { legacy_segments(data) } else { found }
    } else {
        legacy_segments(data)
    };

    let mut versions = Vec::new();
    if !segs.is_empty() {
        let primaries: Vec<OlsSegment> = segs.iter().filter(|s| s.is_primary).cloned().collect();
        let active = if primaries.is_empty() { segs } else { primaries };
        let (rom_data, base_address) = assemble_rom(data, &active);
        if !rom_data.is_empty() {
            versions.push(OlsVersionImage {
                index: 0,
                name: "Version 0".to_string(),
                segments: active,
                base_address,
                rom_data,
            });
        }
    }

    let rend = find_u32_le(data, POSTZIP_HDR, 0).unwrap_or(data.len());
    let mut maps = Vec::new();
    let mut off = 0x180usize.min(rend);
    while off + 8 < rend {
        if let Some((mut m, next)) = parse_map(data, off, rend, schema) {
            if let Some(v0) = versions.first() {
                if m.address >= v0.base_address
                    && (m.address - v0.base_address) as usize <= v0.rom_data.len()
                {
                    m.address -= v0.base_address;
                }
            }
            maps.push(m);
            off = next;
            for _ in 0..64 {
                if off + 4 > rend {
                    break;
                }
                let peek = rd32(data, off);
                if (4..=200).contains(&peek) {
                    break;
                }
                off += 1;
            }
        } else {
            off += 1;
        }
    }

    Ok(OlsProject {
        header,
        metadata,
        versions,
        maps,
        warnings,
    })
}
