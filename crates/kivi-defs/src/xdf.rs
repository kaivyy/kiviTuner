use crate::model::{
    AxisDef, ConstantDef, Definitions, FlagDef, MapDef, PatchDef, PatchEntryDef, Scaling,
    XdfChecksumDef,
};
use kivi_core::types::{DType, Endian};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum XdfError {
    #[error("XML parsing error: {0}")]
    Xml(#[from] quick_xml::Error),
}

fn parse_int(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let (neg, s) = if let Some(stripped) = s.strip_prefix('-') {
        (true, stripped.trim())
    } else {
        (false, s)
    };

    let val = if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        i64::from_str_radix(hex, 16).ok()
    } else {
        s.parse::<i64>()
            .ok()
            .or_else(|| i64::from_str_radix(s, 16).ok())
    };

    val.map(|v| if neg { -v } else { v })
}

/// Extracts numeric axis breakpoints from XDF `<LABEL>` metadata.
///
/// ## This is a kiviTuner compatibility policy, not universal XDF semantics
///
/// XDF labels serve multiple purposes in practice: numeric breakpoints, enum
/// display names (`On`/`Off`), memory category names (`Engine Cal`), and
/// placeholder constants (`0` for every index). No attribute distinguishes
/// them, so this function accepts labels as breakpoints only under a strict
/// conjunction:
///
/// 1. `indexcount` is present and equals the number of labels,
/// 2. every label has an explicit `index` and `value`,
/// 3. indices form exactly `0..n-1` (no duplicates, no gaps, no out-of-range),
/// 4. every value parses as a finite `f64`,
/// 5. values are strictly increasing,
/// 6. `n >= 2` (a one-point axis is not a breakpoint axis).
///
/// **Decreasing axes are rejected.** `kivi_defs::interp::search_interval`
/// performs a binary search that assumes ascending breakpoints; on a
/// descending axis it returns the wrong interval (verified against the source
/// engine, which behaves identically). Accepting descending breakpoints would
/// therefore produce silently incorrect interpolation. Rejecting them keeps the
/// index fallback and avoids that failure mode without changing the
/// parity-locked algorithm.
///
/// Any failure returns an empty `Vec`, preserving the caller's index-coordinate
/// fallback. Nothing is sorted, deduplicated, clamped, or repaired.
fn extract_axis_breakpoints(labels: &[(usize, String)], indexcount: Option<usize>) -> Vec<f64> {
    let count = match indexcount {
        Some(n) if n >= 2 => n,
        _ => return Vec::new(),
    };

    if labels.len() != count {
        return Vec::new();
    }

    // Place each value at its declared index. Reject duplicates, gaps, and
    // out-of-range indices rather than sorting or repairing.
    let mut ordered: Vec<Option<f64>> = vec![None; count];
    for (idx, raw) in labels {
        if *idx >= count {
            return Vec::new();
        }
        let parsed = match raw.trim().parse::<f64>() {
            Ok(v) if v.is_finite() => v,
            _ => return Vec::new(),
        };
        if ordered[*idx].is_some() {
            return Vec::new();
        }
        ordered[*idx] = Some(parsed);
    }

    let values: Vec<f64> = match ordered.into_iter().collect::<Option<Vec<f64>>>() {
        Some(v) => v,
        None => return Vec::new(),
    };

    // Strictly increasing only: `search_interval` cannot handle descending axes
    // (verified against the source engine, which has the same limitation).
    let increasing = values.windows(2).all(|w| w[1] > w[0]);
    if !increasing {
        return Vec::new();
    }

    values
}

fn parse_usize(s: &str) -> Option<usize> {
    parse_int(s).map(|v| v.max(0) as usize)
}

/// Reads the `offset` and `subtract` attributes of a `<BASEOFFSET>` tag.
fn read_base_offset(e: &quick_xml::events::BytesStart) -> (i64, i64) {
    let mut offset = 0i64;
    let mut subtract = 0i64;
    for attr in e.attributes().flatten() {
        let key = attr.key.into_inner().to_ascii_uppercase();
        let val = String::from_utf8_lossy(&attr.value);
        match key.as_slice() {
            b"OFFSET" => offset = parse_int(&val).unwrap_or(0),
            b"SUBTRACT" => subtract = parse_int(&val).unwrap_or(0),
            _ => {}
        }
    }
    (offset, subtract)
}

/// Applies a definition's `BASEOFFSET` according to the XDF format spec.
///
/// Verified against reference XDF addressing implementations:
///
/// ```text
/// file_offset = address - base_offset   if subtract else
///               address + base_offset
/// ```
///
/// The `subtract` flag selects the **direction**, not merely whether to
/// subtract. That is not what this module originally assumed, and the two
/// readings are measurably different on real files:
///
/// | File | offset | subtract | always-subtract | repo rule |
/// |---|---:|---:|---:|---:|
/// | `404J_normal.xdf` (MED17) | 9 961 472 | 1 | 3 424/3 424 in ROM | same |
/// | MS43 `..._ENG_512K` | 458 752 | 0 | **0** in ROM | **3 736/3 782 on varying data** |
///
/// On the MS43 definition every address is `0x42..0xEE0A`; subtracting 458 752
/// drives all of them negative and clamping piles them onto offset 0. Adding it
/// puts them at `0x70042..0x7EE0A`, inside the 524 288-byte ROM, and 3 736 of
/// 3 782 land on bytes that actually vary — against 2 476 for leaving the
/// addresses unshifted. So the base is genuinely applied, in the other
/// direction.
///
/// A negative result is clamped to zero rather than wrapped: wrapping would
/// read unrelated bytes silently.
fn apply_base_offset(address: usize, offset: i64, subtract: i64) -> usize {
    let addr = i64::try_from(address).unwrap_or(i64::MAX);
    let v = if subtract != 0 {
        addr.saturating_sub(offset)
    } else {
        addr.saturating_add(offset)
    };
    v.max(0) as usize
}

fn parse_type_flags(flags: u32, bits: usize) -> (DType, Endian, bool) {
    let signed = (flags & 0x01) != 0;
    let little_endian = (flags & 0x02) != 0;
    let column_major = (flags & 0x04) != 0;
    let is_float = (flags & 0x10000) != 0;

    let endian = if little_endian {
        Endian::Little
    } else {
        Endian::Big
    };

    let dtype = if is_float && bits == 32 {
        DType::F32
    } else {
        match bits {
            16 => {
                if signed {
                    DType::I16
                } else {
                    DType::U16
                }
            }
            32 => {
                if signed {
                    DType::I32
                } else {
                    DType::U32
                }
            }
            _ => {
                if signed {
                    DType::I8
                } else {
                    DType::U8
                }
            }
        }
    };

    (dtype, endian, column_major)
}

#[derive(Default)]
struct AxisState {
    id: String,
    address: usize,
    rows: usize,
    cols: usize,
    bits: usize,
    flags: u32,
    major_stride_bits: i32,
    minor_stride_bits: i32,
    equation: String,
    unit: String,
    decimals: u8,
    has_embedded_data: bool,
    labels: Vec<(usize, String)>,
    indexcount: Option<usize>,
}

#[derive(Default)]
struct TableState {
    title: String,
    category: String,
    equation: String,
    axes: HashMap<String, AxisState>,
}

#[derive(Default)]
struct ConstantState {
    title: String,
    category: String,
    address: usize,
    bits: usize,
    flags: u32,
    equation: String,
    unit: String,
    decimals: u8,
}

#[derive(Default)]
struct FlagState {
    title: String,
    category: String,
    address: usize,
    bits: usize,
    mask: u32,
}

#[derive(Default)]
struct PatchState {
    id: String,
    title: String,
    category: String,
    description: String,
    entries: Vec<(String, usize, Vec<u8>, Vec<u8>)>,
}

#[derive(Default)]
struct ChecksumState {
    id: String,
    title: String,
    start_address: usize,
    end_address: usize,
    store_address: usize,
    data_bits: u8,
    sum_bits: u8,
    little_endian: bool,
    calculation_method: String,
    in_region: bool,
}

fn decode_hex_bytes(hex_str: &str) -> Vec<u8> {
    let clean: String = hex_str
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .collect();
    let mut out = Vec::with_capacity(clean.len() / 2);
    let bytes = clean.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if let Ok(b) = u8::from_str_radix(&clean[i..i + 2], 16) {
            out.push(b);
        }
        i += 2;
    }
    out
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum TextTarget {
    Title,
    Category,
    Description,
    Units,
    DecimalPl,
    Mask,
    IndexCount,
    DataStart,
    DataEnd,
    StoreAddress,
    DataSize,
    SumSize,
    ChecksumFlags,
    CalculationMethod,
}

pub fn parse_xdf(xml_content: &str) -> Result<Vec<MapDef>, XdfError> {
    parse_xdf_full(xml_content).map(|defs| defs.maps)
}

pub fn parse_xdf_full(xml_content: &str) -> Result<Definitions, XdfError> {
    let mut reader = Reader::from_str(xml_content);
    reader.config_mut().trim_text(true);

    let mut header_categories: HashMap<usize, String> = HashMap::new();
    let mut definitions = Definitions::default();
    // XDF BASEOFFSET converts a declared address into a file offset:
    // file_offset = mmedaddress - offset - subtract. Ignoring it reads every map
    // from the wrong place, so it is applied to every address emitted below.
    let mut base_offset: i64 = 0;
    let mut base_subtract: i64 = 0;

    let mut current_table: Option<TableState> = None;
    let mut current_axis: Option<AxisState> = None;
    let mut current_constant: Option<ConstantState> = None;
    let mut current_flag: Option<FlagState> = None;
    let mut current_patch: Option<PatchState> = None;
    let mut current_checksum: Option<ChecksumState> = None;
    let mut text_target: Option<TextTarget> = None;

    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Start(ref e) => {
                let tag_bytes = e.name().into_inner().to_ascii_uppercase();
                match tag_bytes.as_slice() {
                    b"BASEOFFSET" => {
                        let (off, sub) = read_base_offset(e);
                        base_offset = off;
                        base_subtract = sub;
                    }
                    b"XDFTABLE" => {
                        current_table = Some(TableState {
                            category: "Other".to_string(),
                            equation: "X".to_string(),
                            ..Default::default()
                        });
                    }
                    b"XDFAXIS" => {
                        let mut axis_id = String::new();
                        for attr in e.attributes().flatten() {
                            if attr.key.into_inner().eq_ignore_ascii_case(b"id") {
                                axis_id = String::from_utf8_lossy(&attr.value)
                                    .to_lowercase()
                                    .trim()
                                    .to_string();
                            }
                        }
                        current_axis = Some(AxisState {
                            id: axis_id,
                            bits: 8,
                            rows: 1,
                            cols: 1,
                            equation: "X".to_string(),
                            decimals: 2,
                            ..Default::default()
                        });
                    }
                    b"XDFCONSTANT" => {
                        current_constant = Some(ConstantState {
                            category: "Other".to_string(),
                            bits: 8,
                            equation: "X".to_string(),
                            decimals: 2,
                            ..Default::default()
                        });
                    }
                    b"XDFFLAG" => {
                        current_flag = Some(FlagState {
                            category: "Other".to_string(),
                            bits: 8,
                            mask: 1,
                            ..Default::default()
                        });
                    }
                    b"XDFPATCH" => {
                        current_patch = Some(PatchState {
                            id: read_uniqueid(e),
                            category: "Other".to_string(),
                            ..Default::default()
                        });
                    }
                    b"XDFPATCHENTRY" => {
                        handle_patch_entry(e, &mut current_patch);
                    }
                    b"XDFCHECKSUM" => {
                        current_checksum = Some(ChecksumState {
                            id: read_uniqueid(e),
                            data_bits: 8,
                            sum_bits: 16,
                            ..Default::default()
                        });
                    }
                    b"REGION" => {
                        if let Some(ref mut cs) = current_checksum {
                            cs.in_region = true;
                            handle_checksum_region(e, cs);
                        }
                    }
                    b"EMBEDDEDDATA" => {
                        handle_embedded_data(
                            e,
                            &mut current_axis,
                            &mut current_constant,
                            &mut current_flag,
                        );
                    }
                    b"MATH" => {
                        handle_math(
                            e,
                            &mut current_table,
                            &mut current_axis,
                            &mut current_constant,
                        );
                    }
                    b"TITLE" => text_target = Some(TextTarget::Title),
                    b"CATEGORY" => {
                        let mut idx_opt = None;
                        let mut name_opt = None;
                        for attr in e.attributes().flatten() {
                            let key = attr.key.into_inner().to_ascii_uppercase();
                            if key.as_slice() == b"INDEX" {
                                idx_opt = parse_usize(&String::from_utf8_lossy(&attr.value));
                            } else if key.as_slice() == b"NAME" {
                                name_opt = Some(String::from_utf8_lossy(&attr.value).to_string());
                            }
                        }
                        if let (Some(idx), Some(name)) = (idx_opt, name_opt) {
                            header_categories.insert(idx, name);
                        } else {
                            text_target = Some(TextTarget::Category);
                        }
                    }
                    b"CATEGORYINDEX" | b"CATEGORYMEM" => {
                        for attr in e.attributes().flatten() {
                            let key = attr.key.into_inner().to_ascii_uppercase();
                            if key.as_slice() == b"INDEX" || key.as_slice() == b"CATEGORY" {
                                if let Some(idx) =
                                    parse_usize(&String::from_utf8_lossy(&attr.value))
                                {
                                    if let Some(cat_name) = header_categories.get(&idx) {
                                        if let Some(ref mut t) = current_table {
                                            t.category = cat_name.clone();
                                        } else if let Some(ref mut c) = current_constant {
                                            c.category = cat_name.clone();
                                        } else if let Some(ref mut f) = current_flag {
                                            f.category = cat_name.clone();
                                        } else if let Some(ref mut p) = current_patch {
                                            p.category = cat_name.clone();
                                        }
                                    }
                                }
                            }
                        }
                    }
                    b"DESCRIPTION" => text_target = Some(TextTarget::Description),
                    b"UNITS" => text_target = Some(TextTarget::Units),
                    b"DECIMALPL" => text_target = Some(TextTarget::DecimalPl),
                    b"INDEXCOUNT" => text_target = Some(TextTarget::IndexCount),
                    b"MASK" => text_target = Some(TextTarget::Mask),
                    b"DATASTART" | b"STARTADDRESS"
                        if current_checksum.as_ref().is_some_and(|c| c.in_region) =>
                    {
                        text_target = Some(TextTarget::DataStart);
                    }
                    b"DATAEND" | b"ENDADDRESS"
                        if current_checksum.as_ref().is_some_and(|c| c.in_region) =>
                    {
                        text_target = Some(TextTarget::DataEnd);
                    }
                    b"STOREADDRESS"
                        if current_checksum.as_ref().is_some_and(|c| c.in_region) =>
                    {
                        text_target = Some(TextTarget::StoreAddress);
                    }
                    b"DATASIZE" if current_checksum.as_ref().is_some_and(|c| c.in_region) => {
                        text_target = Some(TextTarget::DataSize);
                    }
                    b"SUMSIZE" if current_checksum.as_ref().is_some_and(|c| c.in_region) => {
                        text_target = Some(TextTarget::SumSize);
                    }
                    b"FLAGS" | b"MMEDTYPEFLAGS"
                        if current_checksum.as_ref().is_some_and(|c| c.in_region) =>
                    {
                        text_target = Some(TextTarget::ChecksumFlags);
                    }
                    b"CALCULATIONMETHOD"
                        if current_checksum.as_ref().is_some_and(|c| c.in_region) =>
                    {
                        text_target = Some(TextTarget::CalculationMethod);
                    }
                    _ => {}
                }
            }
            Event::Empty(ref e) => {
                let tag_bytes = e.name().into_inner().to_ascii_uppercase();
                match tag_bytes.as_slice() {
                    b"BASEOFFSET" => {
                        // Self-closing form: <BASEOFFSET offset=".." subtract=".." />
                        let (off, sub) = read_base_offset(e);
                        base_offset = off;
                        base_subtract = sub;
                    }
                    b"EMBEDDEDDATA" => {
                        handle_embedded_data(
                            e,
                            &mut current_axis,
                            &mut current_constant,
                            &mut current_flag,
                        );
                    }
                    b"MATH" => {
                        handle_math(
                            e,
                            &mut current_table,
                            &mut current_axis,
                            &mut current_constant,
                        );
                    }
                    b"CATEGORY" => {
                        let mut idx_opt = None;
                        let mut name_opt = None;
                        for attr in e.attributes().flatten() {
                            let key = attr.key.into_inner().to_ascii_uppercase();
                            if key.as_slice() == b"INDEX" {
                                idx_opt = parse_usize(&String::from_utf8_lossy(&attr.value));
                            } else if key.as_slice() == b"NAME" {
                                name_opt = Some(String::from_utf8_lossy(&attr.value).to_string());
                            }
                        }
                        if let (Some(idx), Some(name)) = (idx_opt, name_opt) {
                            header_categories.insert(idx, name);
                        }
                    }
                    b"CATEGORYINDEX" | b"CATEGORYMEM" => {
                        for attr in e.attributes().flatten() {
                            let key = attr.key.into_inner().to_ascii_uppercase();
                            if key.as_slice() == b"INDEX" || key.as_slice() == b"CATEGORY" {
                                if let Some(idx) =
                                    parse_usize(&String::from_utf8_lossy(&attr.value))
                                {
                                    if let Some(cat_name) = header_categories.get(&idx) {
                                        if let Some(ref mut t) = current_table {
                                            t.category = cat_name.clone();
                                        } else if let Some(ref mut c) = current_constant {
                                            c.category = cat_name.clone();
                                        } else if let Some(ref mut f) = current_flag {
                                            f.category = cat_name.clone();
                                        } else if let Some(ref mut p) = current_patch {
                                            p.category = cat_name.clone();
                                        }
                                    }
                                }
                            }
                        }
                    }
                    b"LABEL" => {
                        if let Some(ref mut axis) = current_axis {
                            let mut idx = 0;
                            let mut val = String::new();
                            for attr in e.attributes().flatten() {
                                let key = attr.key.into_inner().to_ascii_uppercase();
                                if key.as_slice() == b"INDEX" {
                                    idx = parse_usize(&String::from_utf8_lossy(&attr.value))
                                        .unwrap_or(0);
                                } else if key.as_slice() == b"VALUE" {
                                    val = String::from_utf8_lossy(&attr.value).to_string();
                                }
                            }
                            axis.labels.push((idx, val));
                        }
                    }
                    b"XDFPATCHENTRY" => {
                        handle_patch_entry(e, &mut current_patch);
                    }
                    b"REGION" => {
                        if let Some(ref mut cs) = current_checksum {
                            handle_checksum_region(e, cs);
                        }
                    }
                    _ => {}
                }
            }
            Event::Text(ref e) => {
                if let Some(target) = text_target {
                    let text = e.unescape().unwrap_or_default().to_string();
                    match target {
                        TextTarget::Title => {
                            if let Some(ref mut t) = current_table {
                                t.title.push_str(&text);
                            } else if let Some(ref mut c) = current_constant {
                                c.title.push_str(&text);
                            } else if let Some(ref mut f) = current_flag {
                                f.title.push_str(&text);
                            } else if let Some(ref mut p) = current_patch {
                                p.title.push_str(&text);
                            } else if let Some(ref mut cs) = current_checksum {
                                cs.title.push_str(&text);
                            }
                        }
                        TextTarget::Category => {
                            if let Some(ref mut t) = current_table {
                                t.category = text;
                            } else if let Some(ref mut c) = current_constant {
                                c.category = text;
                            } else if let Some(ref mut f) = current_flag {
                                f.category = text;
                            } else if let Some(ref mut p) = current_patch {
                                p.category = text;
                            }
                        }
                        TextTarget::Description => {
                            if let Some(ref mut p) = current_patch {
                                p.description.push_str(&text);
                            }
                        }
                        TextTarget::Units => {
                            if let Some(ref mut a) = current_axis {
                                a.unit.push_str(&text);
                            } else if let Some(ref mut c) = current_constant {
                                c.unit.push_str(&text);
                            }
                        }
                        TextTarget::DecimalPl => {
                            let decimals = parse_usize(&text).unwrap_or(2) as u8;
                            if let Some(ref mut a) = current_axis {
                                a.decimals = decimals;
                            } else if let Some(ref mut c) = current_constant {
                                c.decimals = decimals;
                            }
                        }
                        TextTarget::IndexCount => {
                            if let Some(ref mut a) = current_axis {
                                let cnt = parse_usize(&text).unwrap_or(1);
                                a.indexcount = Some(cnt);
                                if a.cols <= 1 && a.rows <= 1 {
                                    a.cols = cnt;
                                }
                            }
                        }
                        TextTarget::Mask => {
                            if let Some(ref mut f) = current_flag {
                                if let Some(mask) = parse_usize(&text) {
                                    f.mask = mask as u32;
                                }
                            }
                        }
                        TextTarget::DataStart => {
                            if let Some(ref mut cs) = current_checksum {
                                if let Some(addr) = parse_usize(&text) {
                                    cs.start_address = addr;
                                }
                            }
                        }
                        TextTarget::DataEnd => {
                            if let Some(ref mut cs) = current_checksum {
                                if let Some(addr) = parse_usize(&text) {
                                    cs.end_address = addr;
                                }
                            }
                        }
                        TextTarget::StoreAddress => {
                            if let Some(ref mut cs) = current_checksum {
                                if let Some(addr) = parse_usize(&text) {
                                    cs.store_address = addr;
                                }
                            }
                        }
                        TextTarget::DataSize => {
                            if let Some(ref mut cs) = current_checksum {
                                if let Some(v) = parse_usize(&text) {
                                    cs.data_bits = normalize_checksum_bits(v);
                                }
                            }
                        }
                        TextTarget::SumSize => {
                            if let Some(ref mut cs) = current_checksum {
                                if let Some(v) = parse_usize(&text) {
                                    cs.sum_bits = normalize_checksum_bits(v);
                                }
                            }
                        }
                        TextTarget::ChecksumFlags => {
                            if let Some(ref mut cs) = current_checksum {
                                if let Some(flags) = parse_int(&text) {
                                    if (flags & 0x02) != 0 {
                                        cs.little_endian = true;
                                    }
                                }
                            }
                        }
                        TextTarget::CalculationMethod => {
                            if let Some(ref mut cs) = current_checksum {
                                cs.calculation_method = text.trim().to_string();
                            }
                        }
                    }
                }
            }
            Event::End(ref e) => {
                let tag_bytes = e.name().into_inner().to_ascii_uppercase();
                match tag_bytes.as_slice() {
                    b"TITLE"
                    | b"CATEGORY"
                    | b"DESCRIPTION"
                    | b"UNITS"
                    | b"DECIMALPL"
                    | b"INDEXCOUNT"
                    | b"MASK"
                    | b"DATASTART"
                    | b"STARTADDRESS"
                    | b"DATAEND"
                    | b"ENDADDRESS"
                    | b"STOREADDRESS"
                    | b"DATASIZE"
                    | b"SUMSIZE"
                    | b"CALCULATIONMETHOD" => {
                        text_target = None;
                    }
                    b"XDFAXIS" => {
                        if let Some(axis) = current_axis.take() {
                            if let Some(ref mut table) = current_table {
                                table.axes.insert(axis.id.clone(), axis);
                            }
                        }
                    }
                    b"XDFTABLE" => {
                        if let Some(table) = current_table.take() {
                            let z_axis = table.axes.get("z");
                            let x_axis_raw = table.axes.get("x");
                            let y_axis_raw = table.axes.get("y");

                            let address = z_axis.map(|z| z.address).unwrap_or(0);
                            let (dtype, endian, col_major) = if let Some(z) = z_axis {
                                parse_type_flags(z.flags, z.bits)
                            } else {
                                (DType::U8, Endian::Big, false)
                            };

                            let equation = if let Some(z) = z_axis {
                                if !z.equation.is_empty() && z.equation != "X" {
                                    z.equation.clone()
                                } else if !table.equation.is_empty() {
                                    table.equation.clone()
                                } else {
                                    "X".to_string()
                                }
                            } else if !table.equation.is_empty() {
                                table.equation.clone()
                            } else {
                                "X".to_string()
                            };

                            let unit = z_axis.map(|z| z.unit.clone()).unwrap_or_default();
                            let decimals = z_axis.map(|z| z.decimals).unwrap_or(2);

                            let rows = if let Some(z) = z_axis {
                                if z.rows > 1 {
                                    z.rows
                                } else if let Some(y) = y_axis_raw {
                                    if y.rows > 1 {
                                        y.rows
                                    } else if y.cols > 1 {
                                        y.cols
                                    } else {
                                        1
                                    }
                                } else {
                                    1
                                }
                            } else if let Some(y) = y_axis_raw {
                                if y.rows > 1 {
                                    y.rows
                                } else if y.cols > 1 {
                                    y.cols
                                } else {
                                    1
                                }
                            } else {
                                1
                            };

                            let cols = if let Some(z) = z_axis {
                                if z.cols > 1 {
                                    z.cols
                                } else if let Some(x) = x_axis_raw {
                                    if x.cols > 1 {
                                        x.cols
                                    } else if x.rows > 1 {
                                        x.rows
                                    } else {
                                        1
                                    }
                                } else {
                                    1
                                }
                            } else if let Some(x) = x_axis_raw {
                                if x.cols > 1 {
                                    x.cols
                                } else if x.rows > 1 {
                                    x.rows
                                } else {
                                    1
                                }
                            } else {
                                1
                            };

                            let x_axis = x_axis_raw.and_then(|x| {
                                if x.has_embedded_data || !x.labels.is_empty() {
                                    let (dt, end, _) = parse_type_flags(x.flags, x.bits);
                                    let len = if x.cols > 1 {
                                        x.cols
                                    } else if x.rows > 1 {
                                        x.rows
                                    } else {
                                        cols
                                    };
                                    Some(AxisDef {
                                        address: if x.has_embedded_data {
                                            apply_base_offset(x.address, base_offset, base_subtract)
                                        } else {
                                            x.address
                                        },
                                        length: len,
                                        dtype: dt,
                                        endian: end,
                                        scaling: Scaling {
                                            equation: x.equation.clone(),
                                            unit: x.unit.clone(),
                                            decimals: x.decimals,
                                        },
                                        values: extract_axis_breakpoints(&x.labels, x.indexcount),
                                    })
                                } else {
                                    None
                                }
                            });

                            let y_axis = y_axis_raw.and_then(|y| {
                                if y.has_embedded_data || !y.labels.is_empty() {
                                    let (dt, end, _) = parse_type_flags(y.flags, y.bits);
                                    let len = if y.rows > 1 {
                                        y.rows
                                    } else if y.cols > 1 {
                                        y.cols
                                    } else {
                                        rows
                                    };
                                    Some(AxisDef {
                                        address: if y.has_embedded_data {
                                            apply_base_offset(y.address, base_offset, base_subtract)
                                        } else {
                                            y.address
                                        },
                                        length: len,
                                        dtype: dt,
                                        endian: end,
                                        scaling: Scaling {
                                            equation: y.equation.clone(),
                                            unit: y.unit.clone(),
                                            decimals: y.decimals,
                                        },
                                        values: extract_axis_breakpoints(&y.labels, y.indexcount),
                                    })
                                } else {
                                    None
                                }
                            });

                            let name = if table.title.trim().is_empty() {
                                format!("Table_0x{:X}", address)
                            } else {
                                table.title.trim().to_string()
                            };

                            let category = if table.category.trim().is_empty() {
                                "Other".to_string()
                            } else {
                                table.category.trim().to_string()
                            };
                            let address = apply_base_offset(address, base_offset, base_subtract);
                            let id = format!("map_{:x}", address);

                            definitions.maps.push(MapDef {
                                id,
                                name,
                                category,
                                address,
                                rows,
                                cols,
                                dtype,
                                endian,
                                column_major: col_major,
                                // Standard XDF treats a stride equal to one element as
                                // packed, not as a one-element pitch. Normalise it
                                // to 0 so the reader has one representation.
                                major_stride_bits: z_axis
                                    .map(|z| {
                                        if z.major_stride_bits == z.bits as i32 {
                                            0
                                        } else {
                                            z.major_stride_bits
                                        }
                                    })
                                    .unwrap_or(0),
                                minor_stride_bits: z_axis
                                    .map(|z| {
                                        if z.minor_stride_bits == z.bits as i32 {
                                            0
                                        } else {
                                            z.minor_stride_bits
                                        }
                                    })
                                    .unwrap_or(0),
                                scaling: Scaling {
                                    equation,
                                    unit,
                                    decimals,
                                },
                                x_axis,
                                y_axis,
                            });
                        }
                    }
                    b"XDFCONSTANT" => {
                        if let Some(constant) = current_constant.take() {
                            let (dtype, endian, _) =
                                parse_type_flags(constant.flags, constant.bits);
                            let name = if constant.title.trim().is_empty() {
                                format!("Constant_0x{:X}", constant.address)
                            } else {
                                constant.title.trim().to_string()
                            };
                            let category = if constant.category.trim().is_empty() {
                                "Other".to_string()
                            } else {
                                constant.category.trim().to_string()
                            };
                            let constant_address =
                                apply_base_offset(constant.address, base_offset, base_subtract);
                            let id = format!("const_{:x}", constant_address);
                            definitions.constants.push(ConstantDef {
                                id,
                                name,
                                category,
                                address: constant_address,
                                dtype,
                                endian,
                                scaling: Scaling {
                                    equation: constant.equation,
                                    unit: constant.unit,
                                    decimals: constant.decimals,
                                },
                            });
                        }
                    }
                    b"XDFFLAG" => {
                        if let Some(flag) = current_flag.take() {
                            let (dtype, _, _) = parse_type_flags(0, flag.bits);
                            let name = if flag.title.trim().is_empty() {
                                format!("Flag_0x{:X}", flag.address)
                            } else {
                                flag.title.trim().to_string()
                            };
                            let category = if flag.category.trim().is_empty() {
                                "Other".to_string()
                            } else {
                                flag.category.trim().to_string()
                            };
                            let flag_address =
                                apply_base_offset(flag.address, base_offset, base_subtract);
                            let id = format!("flag_{:x}", flag_address);
                            definitions.flags.push(FlagDef {
                                id,
                                name,
                                category,
                                address: flag_address,
                                dtype,
                                mask: flag.mask,
                            });
                        }
                    }
                    b"REGION" => {
                        if let Some(ref mut cs) = current_checksum {
                            cs.in_region = false;
                        }
                    }
                    b"XDFPATCH" => {
                        if let Some(patch) = current_patch.take() {
                            let idx = definitions.patches.len() + 1;
                            let id = if patch.id.trim().is_empty() {
                                format!("patch_{idx}")
                            } else {
                                patch.id.trim().to_string()
                            };
                            let name = if patch.title.trim().is_empty() {
                                format!("Patch_{id}")
                            } else {
                                patch.title.trim().to_string()
                            };
                            let category = if patch.category.trim().is_empty() {
                                "Other".to_string()
                            } else {
                                patch.category.trim().to_string()
                            };
                            let entries = patch
                                .entries
                                .into_iter()
                                .map(|(ename, addr, patch_data, base_data)| PatchEntryDef {
                                    name: ename,
                                    address: apply_base_offset(addr, base_offset, base_subtract),
                                    patch_data,
                                    base_data,
                                })
                                .collect();
                            definitions.patches.push(PatchDef {
                                id,
                                name,
                                category,
                                description: patch.description.trim().to_string(),
                                entries,
                            });
                        }
                    }
                    b"XDFCHECKSUM" => {
                        if let Some(cs) = current_checksum.take() {
                            let idx = definitions.checksums.len() + 1;
                            let id = if cs.id.trim().is_empty() {
                                format!("checksum_{idx}")
                            } else {
                                cs.id.trim().to_string()
                            };
                            let name = if cs.title.trim().is_empty() {
                                format!("Checksum_{id}")
                            } else {
                                cs.title.trim().to_string()
                            };
                            definitions.checksums.push(XdfChecksumDef {
                                id,
                                name,
                                start_address: apply_base_offset(
                                    cs.start_address,
                                    base_offset,
                                    base_subtract,
                                ),
                                end_address: apply_base_offset(
                                    cs.end_address,
                                    base_offset,
                                    base_subtract,
                                ),
                                store_address: apply_base_offset(
                                    cs.store_address,
                                    base_offset,
                                    base_subtract,
                                ),
                                data_bits: if cs.data_bits == 0 { 8 } else { cs.data_bits },
                                sum_bits: if cs.sum_bits == 0 { 16 } else { cs.sum_bits },
                                little_endian: cs.little_endian,
                                calculation_method: cs.calculation_method,
                            });
                        }
                    }
                    _ => {}
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }

    Ok(definitions)
}

fn handle_embedded_data(
    e: &quick_xml::events::BytesStart,
    current_axis: &mut Option<AxisState>,
    current_constant: &mut Option<ConstantState>,
    current_flag: &mut Option<FlagState>,
) {
    let mut address = 0;
    let mut rows = 1;
    let mut cols = 1;
    let mut bits = 8;
    let mut flags = 0u32;
    let mut mask_opt = None;
    let mut major_stride = 0i32;
    let mut minor_stride = 0i32;

    for attr in e.attributes().flatten() {
        let key = attr.key.into_inner().to_ascii_uppercase();
        let val_str = String::from_utf8_lossy(&attr.value);
        match key.as_slice() {
            b"MMEDADDRESS" => {
                address = parse_usize(&val_str).unwrap_or(0);
            }
            b"MMEDROWCOUNT" => {
                rows = parse_usize(&val_str).unwrap_or(1);
            }
            b"MMEDCOLCOUNT" => {
                cols = parse_usize(&val_str).unwrap_or(1);
            }
            b"MMEDELEMENTSIZEBITS" => {
                bits = parse_usize(&val_str).unwrap_or(8);
            }
            b"MMEDTYPEFLAGS" => {
                flags = parse_usize(&val_str).unwrap_or(0) as u32;
            }
            b"MASK" => {
                mask_opt = parse_usize(&val_str);
            }
            // Strides are signed: a negative value walks backwards.
            b"MMEDMAJORSTRIDEBITS" => {
                major_stride = parse_int(&val_str).unwrap_or(0) as i32;
            }
            b"MMEDMINORSTRIDEBITS" => {
                minor_stride = parse_int(&val_str).unwrap_or(0) as i32;
            }
            _ => {}
        }
    }

    if let Some(ref mut axis) = current_axis {
        axis.address = address;
        axis.rows = rows;
        axis.cols = cols;
        axis.bits = bits;
        axis.flags = flags;
        axis.major_stride_bits = major_stride;
        axis.minor_stride_bits = minor_stride;
        axis.has_embedded_data = true;
    } else if let Some(ref mut constant) = current_constant {
        constant.address = address;
        constant.bits = bits;
        constant.flags = flags;
    } else if let Some(ref mut flag) = current_flag {
        flag.address = address;
        flag.bits = bits;
        if let Some(mask) = mask_opt {
            flag.mask = mask as u32;
        }
    }
}

fn handle_math(
    e: &quick_xml::events::BytesStart,
    current_table: &mut Option<TableState>,
    current_axis: &mut Option<AxisState>,
    current_constant: &mut Option<ConstantState>,
) {
    let mut equation = String::new();
    for attr in e.attributes().flatten() {
        if attr.key.into_inner().eq_ignore_ascii_case(b"equation") {
            equation = String::from_utf8_lossy(&attr.value).trim().to_string();
        }
    }
    if equation.is_empty() {
        return;
    }

    if let Some(ref mut axis) = current_axis {
        axis.equation = equation;
    } else if let Some(ref mut constant) = current_constant {
        constant.equation = equation;
    } else if let Some(ref mut table) = current_table {
        table.equation = equation;
    }
}

fn read_uniqueid(e: &quick_xml::events::BytesStart) -> String {
    for attr in e.attributes().flatten() {
        if attr.key.into_inner().eq_ignore_ascii_case(b"uniqueid") {
            return String::from_utf8_lossy(&attr.value).trim().to_string();
        }
    }
    String::new()
}

fn normalize_checksum_bits(raw: usize) -> u8 {
    match raw {
        1 | 2 | 4 => (raw * 8) as u8,
        8 | 16 | 32 | 64 => raw as u8,
        _ => 8,
    }
}

fn handle_patch_entry(
    e: &quick_xml::events::BytesStart,
    current_patch: &mut Option<PatchState>,
) {
    let Some(ref mut patch) = current_patch else {
        return;
    };
    let mut name = String::new();
    let mut address = 0usize;
    let mut patch_data = Vec::new();
    let mut base_data = Vec::new();
    let mut has_addr_or_patch = false;

    for attr in e.attributes().flatten() {
        let key = attr.key.into_inner().to_ascii_uppercase();
        let val_str = String::from_utf8_lossy(&attr.value);
        match key.as_slice() {
            b"NAME" => name = val_str.trim().to_string(),
            b"ADDRESS" | b"MMEDADDRESS" => {
                if let Some(addr) = parse_usize(&val_str) {
                    address = addr;
                    has_addr_or_patch = true;
                }
            }
            b"PATCHDATA" => {
                patch_data = decode_hex_bytes(&val_str);
                if !patch_data.is_empty() {
                    has_addr_or_patch = true;
                }
            }
            b"BASEDATA" => {
                base_data = decode_hex_bytes(&val_str);
            }
            _ => {}
        }
    }

    if has_addr_or_patch {
        let entry_name = if name.is_empty() {
            format!("Entry {}", patch.entries.len() + 1)
        } else {
            name
        };
        patch.entries.push((entry_name, address, patch_data, base_data));
    }
}

fn handle_checksum_region(e: &quick_xml::events::BytesStart, cs: &mut ChecksumState) {
    for attr in e.attributes().flatten() {
        let key = attr.key.into_inner().to_ascii_uppercase();
        let val_str = String::from_utf8_lossy(&attr.value);
        match key.as_slice() {
            b"DATASTART" | b"STARTADDRESS" => {
                if let Some(v) = parse_usize(&val_str) {
                    cs.start_address = v;
                }
            }
            b"DATAEND" | b"ENDADDRESS" => {
                if let Some(v) = parse_usize(&val_str) {
                    cs.end_address = v;
                }
            }
            b"STOREADDRESS" => {
                if let Some(v) = parse_usize(&val_str) {
                    cs.store_address = v;
                }
            }
            b"DATASIZE" => {
                if let Some(v) = parse_usize(&val_str) {
                    cs.data_bits = normalize_checksum_bits(v);
                }
            }
            b"SUMSIZE" => {
                if let Some(v) = parse_usize(&val_str) {
                    cs.sum_bits = normalize_checksum_bits(v);
                }
            }
            b"CALCULATIONMETHOD" => {
                cs.calculation_method = val_str.trim().to_string();
            }
            b"MMEDTYPEFLAGS" | b"FLAGS" => {
                if let Some(f) = parse_usize(&val_str) {
                    cs.little_endian = (f & 0x02) != 0;
                }
            }
            _ => {}
        }
    }
}
