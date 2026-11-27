//! ASAP2 / ASAM MCD-2 MC (`.a2l`) parsing.
//!
//! ## What is parsed (`B-A2L` complete)
//!
//! - `CHARACTERISTIC` — `VALUE`, `CURVE`, `MAP`, `ASCII`, `VAL_BLK`, plus nested
//!   `AXIS_DESCR` descriptors (`AXIS_PTS_REF`, `max_axis_points`, `byte_order`)
//!   and `record_layout`.
//! - `AXIS_PTS` — standalone/common breakpoint axes referenced via `AXIS_PTS_REF`.
//! - `MEMORY_SEGMENT` — document-level flash/EPROM/RAM address windows (`D5`)
//!   used by [`A2lDocument::build_translator_for_rom`].
//! - `RECORD_LAYOUT` — `FNC_VALUES`, `AXIS_PTS_X`, `AXIS_PTS_Y` cell data types.
//! - `COMPU_METHOD` with `RAT_FUNC` (`y = (a·x² + b·x + c) / (d·x² + e·x + f)`).
//! - Non-silent diagnostics: every malformed/skipped block appends an entry to
//!   [`A2lDocument::diagnostics`] (`D2` / `I2`).

use kivi_core::address_translation::{MemoryAddressTranslator, MemorySegmentRule};

/// Byte ordering declared in A2L `BYTE_ORDER`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum A2lByteOrder {
    /// Big-endian (`MSB_FIRST` or `BIG_ENDIAN`).
    MsbFirst,
    /// Little-endian (`MSB_LAST` or `LITTLE_ENDIAN`).
    MsbLast,
}

impl A2lByteOrder {
    fn from_token(token: &str) -> Option<Self> {
        match token {
            "MSB_FIRST" | "BIG_ENDIAN" => Some(Self::MsbFirst),
            "MSB_LAST" | "LITTLE_ENDIAN" => Some(Self::MsbLast),
            _ => None,
        }
    }

    pub fn is_little_endian(self) -> bool {
        matches!(self, Self::MsbLast)
    }
}

/// What kind of calibration object a `CHARACTERISTIC` describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum A2lCharacteristicType {
    /// A single value.
    Value,
    /// A 1-D curve.
    Curve,
    /// A 2-D map.
    Map,
    /// An ASCII string.
    Ascii,
    /// A value block.
    ValBlk,
    /// A type this parser does not model.
    Unknown,
}

impl A2lCharacteristicType {
    fn from_token(token: &str) -> Self {
        match token {
            "VALUE" => Self::Value,
            "CURVE" => Self::Curve,
            "MAP" => Self::Map,
            "ASCII" => Self::Ascii,
            "VAL_BLK" => Self::ValBlk,
            _ => Self::Unknown,
        }
    }
}

/// Nested `/begin AXIS_DESCR ... /end AXIS_DESCR` inside a `CHARACTERISTIC`.
#[derive(Debug, Clone, PartialEq)]
pub struct A2lAxisDescr {
    pub attribute: String,
    pub input_quantity: String,
    pub conversion: String,
    pub max_axis_points: usize,
    pub lower_limit: f64,
    pub upper_limit: f64,
    pub axis_pts_ref: Option<String>,
    pub byte_order: Option<A2lByteOrder>,
}

/// Top-level `/begin AXIS_PTS ... /end AXIS_PTS` block.
#[derive(Debug, Clone, PartialEq)]
pub struct A2lAxisPts {
    pub name: String,
    pub description: String,
    pub address: u32,
    pub input_quantity: String,
    pub deposit: String,
    pub max_diff: f64,
    pub conversion: String,
    pub max_axis_points: usize,
    pub lower_limit: f64,
    pub upper_limit: f64,
    pub byte_order: Option<A2lByteOrder>,
}

/// Top-level `/begin MEMORY_SEGMENT ... /end MEMORY_SEGMENT` block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct A2lMemorySegment {
    pub name: String,
    pub description: String,
    pub prg_type: String,
    pub memory_type: String,
    pub attribute: String,
    pub address: u32,
    pub size: u32,
}

/// Top-level `/begin RECORD_LAYOUT ... /end RECORD_LAYOUT` block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct A2lRecordLayout {
    pub name: String,
    pub fnc_datatype: Option<String>,
    pub fnc_index_mode: Option<String>,
    pub axis_x_datatype: Option<String>,
    pub axis_y_datatype: Option<String>,
}

impl A2lRecordLayout {
    pub fn element_bytes(datatype: Option<&str>) -> Option<usize> {
        match datatype? {
            "UBYTE" | "SBYTE" => Some(1),
            "UWORD" | "SWORD" => Some(2),
            "ULONG" | "SLONG" | "FLOAT32_IEEE" => Some(4),
            "A_UINT64" | "A_INT64" | "FLOAT64_IEEE" => Some(8),
            _ => None,
        }
    }

    pub fn fnc_element_bytes(&self) -> Option<usize> {
        Self::element_bytes(self.fnc_datatype.as_deref())
    }

    pub fn is_column_dir(&self) -> bool {
        self.fnc_index_mode.as_deref() == Some("COLUMN_DIR")
    }
}

/// One `CHARACTERISTIC` entry.
#[derive(Debug, Clone, PartialEq)]
pub struct A2lCharacteristic {
    pub name: String,
    pub description: String,
    pub kind: A2lCharacteristicType,
    /// Absolute address in the ECU image.
    pub address: u32,
    /// `RECORD_LAYOUT` identifier.
    pub record_layout: String,
    /// Name of the `COMPU_METHOD` that converts raw to physical.
    pub conversion: String,
    pub lower_limit: f64,
    pub upper_limit: f64,
    pub axes: Vec<A2lAxisDescr>,
    pub byte_order: Option<A2lByteOrder>,
}

impl A2lCharacteristic {
    /// Returns `(rows, cols)` for a `MAP` (`axes.len() >= 2`) or `(1, pts)` for a `CURVE`.
    pub fn dimensions(&self) -> Option<(usize, usize)> {
        match self.kind {
            A2lCharacteristicType::Map if self.axes.len() >= 2 => {
                Some((self.axes[0].max_axis_points, self.axes[1].max_axis_points))
            }
            A2lCharacteristicType::Curve if !self.axes.is_empty() => {
                Some((1, self.axes[0].max_axis_points))
            }
            _ => None,
        }
    }
}

/// A `COMPU_METHOD` using `RAT_FUNC`.
#[derive(Debug, Clone, PartialEq)]
pub struct A2lConversion {
    pub name: String,
    pub unit: String,
    /// Coefficients `a b c d e f` of `(ax² + bx + c) / (dx² + ex + f)`.
    pub coeffs: [f64; 6],
}

impl A2lConversion {
    /// Apply the rational function to a raw value.
    ///
    /// A zero denominator returns the raw value rather than infinity: a
    /// degenerate conversion in a definition file must not poison a whole table.
    pub fn apply(&self, x: f64) -> f64 {
        let [a, b, c, d, e, f] = self.coeffs;
        let num = a * x * x + b * x + c;
        let den = d * x * x + e * x + f;
        if den.abs() < f64::EPSILON {
            x
        } else {
            num / den
        }
    }
}

/// Top-level `/begin COMPU_TAB ...` / `COMPU_VTAB` value/enum conversion table.
#[derive(Debug, Clone, PartialEq)]
pub struct A2lCompuTab {
    pub name: String,
    pub description: String,
    pub is_verbal: bool,
    pub entries: Vec<(f64, String)>,
}

/// Top-level `/begin FUNCTION ...` or `/begin GROUP ...` category entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct A2lFunctionGroup {
    pub name: String,
    pub description: String,
    pub characteristics: Vec<String>,
}

/// A parsed A2L document.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct A2lDocument {
    pub characteristics: Vec<A2lCharacteristic>,
    pub conversions: Vec<A2lConversion>,
    pub compu_tabs: Vec<A2lCompuTab>,
    pub axis_pts: Vec<A2lAxisPts>,
    pub memory_segments: Vec<A2lMemorySegment>,
    pub record_layouts: Vec<A2lRecordLayout>,
    pub functions: Vec<A2lFunctionGroup>,
    pub default_byte_order: Option<A2lByteOrder>,
    pub deposit_mode: Option<String>,
    pub diagnostics: Vec<String>,
}

impl A2lDocument {
    /// Derives a document-scoped [`MemoryAddressTranslator`] from parsed `MEMORY_SEGMENT`
    /// declarations (`D5`) for a BIN image of length `rom_len`.
    ///
    /// Returns `None` when the A2L declares no usable flash/EPROM segments or no non-zero
    /// base mapping covers the document's characteristics/axes.
    pub fn build_translator_for_rom(&self, rom_len: usize) -> Option<MemoryAddressTranslator> {
        if rom_len == 0 {
            return None;
        }
        let flash_segs: Vec<&A2lMemorySegment> = self
            .memory_segments
            .iter()
            .filter(|s| {
                s.size > 0
                    && !matches!(s.prg_type.as_str(), "VARIABLES" | "SERAM" | "OFFLINE_DATA")
                    && !matches!(s.memory_type.as_str(), "RAM" | "REGISTER")
            })
            .collect();

        if flash_segs.is_empty() {
            return None;
        }

        let rom_len_u64 = rom_len as u64;
        let align_mask = if rom_len.is_power_of_two() {
            !(rom_len_u64 - 1)
        } else {
            !0xFFFFu64
        };

        let mut base_candidates: Vec<u32> = Vec::new();
        for seg in &flash_segs {
            let aligned = ((seg.address as u64) & align_mask) as u32;
            if !base_candidates.contains(&aligned) {
                base_candidates.push(aligned);
            }
            let seg_64k = seg.address & !0xFFFFu32;
            if !base_candidates.contains(&seg_64k) {
                base_candidates.push(seg_64k);
            }
        }
        for &std_base in &[0x8000_0000u32, 0x0800_0000, 0x0080_0000, 0x0040_0000, 0x0000_0000] {
            if !base_candidates.contains(&std_base) {
                base_candidates.push(std_base);
            }
        }

        let char_addrs: Vec<u32> = self
            .characteristics
            .iter()
            .map(|c| c.address)
            .chain(self.axis_pts.iter().map(|a| a.address))
            .collect();

        let mut best_base: Option<u32> = None;
        let mut best_hits = 0usize;

        for &base in &base_candidates {
            let win_end = (base as u64).saturating_add(rom_len_u64.saturating_sub(1));
            let seg_overlap = flash_segs.iter().any(|s| {
                let s_start = s.address as u64;
                let s_end = s_start.saturating_add((s.size as u64).saturating_sub(1));
                s_start >= base as u64 && s_end <= win_end && !matches!(s.prg_type.as_str(), "RESERVED")
            });
            let seg_any = seg_overlap
                || flash_segs.iter().any(|s| {
                    let s_start = s.address as u64;
                    s_start >= base as u64 && s_start <= win_end
                });
            if !seg_any {
                continue;
            }
            let hits = char_addrs
                .iter()
                .filter(|&&a| (a as u64) >= base as u64 && (a as u64) <= win_end)
                .count();
            if hits > best_hits || (hits == best_hits && best_base.is_none()) {
                best_hits = hits;
                best_base = Some(base);
            }
        }

        let base = best_base?;
        if best_hits == 0 {
            return None;
        }
        let end = (base as u64)
            .saturating_add(rom_len_u64.saturating_sub(1))
            .min(u32::MAX as u64) as u32;

        Some(MemoryAddressTranslator {
            segments: vec![MemorySegmentRule {
                name: format!("A2L MEMORY_SEGMENT image window @0x{base:08X}"),
                cpu_start_address: base,
                cpu_end_address: end,
                raw_file_offset: 0,
            }],
        })
    }

    pub fn record_layout_for(&self, name: &str) -> Option<&A2lRecordLayout> {
        self.record_layouts.iter().find(|r| r.name == name)
    }

    pub fn axis_pts_for(&self, name: &str) -> Option<&A2lAxisPts> {
        self.axis_pts.iter().find(|a| a.name == name)
    }

    pub fn conversion_for(&self, name: &str) -> Option<&A2lConversion> {
        self.conversions.iter().find(|c| c.name == name)
    }

    pub fn category_for_characteristic(&self, name: &str) -> String {
        for fg in &self.functions {
            if fg.characteristics.iter().any(|c| c == name) {
                return fg.name.clone();
            }
        }
        "A2L".to_string()
    }

    /// Converts parsed ASAP2 `CHARACTERISTIC`s, `AXIS_PTS`, `RECORD_LAYOUT`s, and
    /// `COMPU_METHOD`s into unified `kivi_defs::model::Definitions`.
    pub fn to_definitions(&self, translator: Option<&MemoryAddressTranslator>) -> kivi_defs::model::Definitions {
        use kivi_core::types::{DType, Endian};
        use kivi_defs::model::{AxisDef, ConstantDef, Definitions, MapDef, Scaling};

        let a2l_dtype = |dt: Option<&str>| -> DType {
            match dt {
                Some("UBYTE") => DType::U8,
                Some("SBYTE") => DType::I8,
                Some("UWORD") => DType::U16,
                Some("SWORD") => DType::I16,
                Some("ULONG") => DType::U32,
                Some("SLONG") => DType::I32,
                Some("FLOAT32_IEEE") => DType::F32,
                _ => DType::U16,
            }
        };
        let to_endian = |bo: Option<A2lByteOrder>| -> Endian {
            let eff = bo.or(self.default_byte_order).unwrap_or(A2lByteOrder::MsbLast);
            if eff.is_little_endian() { Endian::Little } else { Endian::Big }
        };
        let to_file_off = |addr: u32| -> usize {
            translator
                .and_then(|t| kivi_core::address_translation::cpu_to_raw(t, addr).ok())
                .filter(|tr| !tr.is_identity_fallback)
                .map(|tr| tr.value)
                .unwrap_or(addr) as usize
        };
        let make_scaling = |conv_name: &str| -> Scaling {
            if let Some(cm) = self.conversion_for(conv_name) {
                let [_a, b, c, _d, e, f] = cm.coeffs;
                let equation = if b.abs() < f64::EPSILON && c.abs() < f64::EPSILON {
                    "X".to_string()
                } else if f.abs() > f64::EPSILON && e.abs() < f64::EPSILON {
                    format!("({b} * X + {c}) / {f}")
                } else if b.abs() > f64::EPSILON {
                    format!("(X * {f} - {c}) / {b}")
                } else {
                    "X".to_string()
                };
                Scaling {
                    equation,
                    unit: cm.unit.clone(),
                    decimals: 2,
                }
            } else {
                Scaling::default()
            }
        };

        let mut defs = Definitions::default();
        for ch in &self.characteristics {
            let rl = self.record_layout_for(&ch.record_layout);
            let dtype = a2l_dtype(rl.and_then(|r| r.fnc_datatype.as_deref()));
            let endian = to_endian(ch.byte_order);
            let category = self.category_for_characteristic(&ch.name);
            let address = to_file_off(ch.address);
            let scaling = make_scaling(&ch.conversion);

            match ch.kind {
                A2lCharacteristicType::Value => {
                    defs.constants.push(ConstantDef {
                        id: format!("a2l-{}", ch.name),
                        name: ch.name.clone(),
                        category,
                        address,
                        dtype,
                        endian,
                        scaling,
                    });
                }
                A2lCharacteristicType::Curve | A2lCharacteristicType::Map | A2lCharacteristicType::ValBlk => {
                    let (rows, cols) = ch.dimensions().unwrap_or((1, ch.axes.first().map(|a| a.max_axis_points).unwrap_or(1)));
                    let make_axis = |ax_opt: Option<&A2lAxisDescr>| -> Option<AxisDef> {
                        let ax = ax_opt?;
                        let ref_pts = ax.axis_pts_ref.as_deref().and_then(|n| self.axis_pts_for(n));
                        let ax_addr = ref_pts.map(|p| to_file_off(p.address)).unwrap_or(0);
                        let ax_rl = ref_pts.and_then(|p| self.record_layout_for(&p.deposit));
                        let ax_dt = a2l_dtype(
                            ax_rl
                                .and_then(|r| r.axis_x_datatype.as_deref().or(r.fnc_datatype.as_deref()))
                                .or_else(|| rl.and_then(|r| r.axis_x_datatype.as_deref())),
                        );
                        let ax_end = to_endian(ax.byte_order.or_else(|| ref_pts.and_then(|p| p.byte_order)));
                        Some(AxisDef {
                            address: ax_addr,
                            length: ax.max_axis_points,
                            dtype: ax_dt,
                            endian: ax_end,
                            scaling: make_scaling(&ax.conversion),
                            values: Vec::new(),
                        })
                    };
                    let (x_axis, y_axis) = if ch.axes.len() >= 2 {
                        (make_axis(ch.axes.get(1)), make_axis(ch.axes.first()))
                    } else {
                        (make_axis(ch.axes.first()), None)
                    };
                    defs.maps.push(MapDef {
                        id: format!("a2l-{}", ch.name),
                        name: ch.name.clone(),
                        category,
                        address,
                        rows: rows.max(1),
                        cols: cols.max(1),
                        dtype,
                        endian,
                        column_major: rl.is_some_and(|r| r.is_column_dir()),
                        major_stride_bits: 0,
                        minor_stride_bits: 0,
                        scaling,
                        x_axis,
                        y_axis,
                    });
                }
                _ => {}
            }
        }
        defs
    }
}

/// Why an A2L document could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum A2lError {
    /// A `/begin X` had no matching `/end X`.
    UnterminatedBlock(String),
}

impl std::fmt::Display for A2lError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            A2lError::UnterminatedBlock(b) => write!(f, "A2L block '{b}' is never closed"),
        }
    }
}

impl std::error::Error for A2lError {}

/// Strip `/* … */` comments, which may span lines.
fn strip_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        if i + 1 < chars.len() && chars[i] == '/' && chars[i + 1] == '*' {
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i = (i + 2).min(chars.len());
            out.push(' ');
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// Split a block body into whitespace-separated tokens, keeping quoted strings
/// together as one token.
fn tokenize(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quote = false;
    let mut was_quoted = false;

    for ch in body.chars() {
        match ch {
            '"' => {
                in_quote = !in_quote;
                was_quoted = true;
            }
            c if c.is_whitespace() && !in_quote => {
                if !cur.is_empty() || was_quoted {
                    out.push(std::mem::take(&mut cur));
                }
                was_quoted = false;
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() || was_quoted {
        out.push(cur);
    }
    out
}

/// Strip nested `/begin ... /end ...` sub-blocks from a body before reading outer keywords.
fn strip_subblocks(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(pos) = rest.find("/begin") {
        out.push_str(&rest[..pos]);
        out.push(' ');
        let after = &rest[pos + "/begin".len()..];
        let trimmed = after.trim_start();
        let name_end = trimmed
            .find(|c: char| c.is_whitespace())
            .unwrap_or(trimmed.len());
        let sub_name = &trimmed[..name_end];
        let end_tag = format!("/end {sub_name}");
        if let Some(end_pos) = trimmed[name_end..].find(&end_tag) {
            rest = &trimmed[name_end + end_pos + end_tag.len()..];
        } else {
            rest = &trimmed[name_end..];
        }
    }
    out.push_str(rest);
    out
}

fn find_byte_order(tokens: &[String]) -> Option<A2lByteOrder> {
    let pos = tokens.iter().position(|t| t == "BYTE_ORDER")?;
    tokens.get(pos + 1).and_then(|t| A2lByteOrder::from_token(t))
}

/// Parse an A2L document.
pub fn parse_a2l(src: &str) -> Result<A2lDocument, A2lError> {
    let clean = strip_comments(src);
    let mut doc = A2lDocument::default();

    let patterns = [
        "/begin MOD_COMMON",
        "/begin MEMORY_SEGMENT",
        "/begin RECORD_LAYOUT",
        "/begin AXIS_PTS",
        "/begin CHARACTERISTIC",
        "/begin COMPU_METHOD",
        "/begin COMPU_TAB",
        "/begin COMPU_VTAB",
        "/begin FUNCTION",
        "/begin GROUP",
    ];

    let mut starts: Vec<usize> = Vec::new();
    for pat in patterns {
        let mut from = 0usize;
        while let Some(rel) = clean[from..].find(pat) {
            let start = from + rel;
            let after_pat = start + pat.len();
            let valid_boundary = clean[after_pat..]
                .chars()
                .next()
                .is_none_or(|c| c.is_whitespace());
            if valid_boundary {
                starts.push(start);
            }
            from = after_pat;
        }
    }
    starts.sort_unstable();

    for &begin in &starts {
        let after = begin + "/begin".len();
        let rest = &clean[after..];
        let lead = rest.len() - rest.trim_start().len();
        let rest_trimmed = &rest[lead..];
        let name_end = rest_trimmed
            .find(|c: char| c.is_whitespace())
            .unwrap_or(rest_trimmed.len());
        let block_name = rest_trimmed[..name_end].trim().to_string();

        let end_marker = format!("/end {block_name}");
        let body_start = after + lead + name_end;
        let Some(end_rel) = clean[body_start..].find(&end_marker) else {
            return Err(A2lError::UnterminatedBlock(block_name));
        };
        let body = &clean[body_start..body_start + end_rel];

        match block_name.as_str() {
            "MOD_COMMON" => {
                let tokens = tokenize(body);
                if doc.default_byte_order.is_none() {
                    doc.default_byte_order = find_byte_order(&tokens);
                }
                if doc.deposit_mode.is_none() {
                    doc.deposit_mode = tokens
                        .iter()
                        .position(|t| t == "DEPOSIT")
                        .and_then(|i| tokens.get(i + 1).cloned());
                }
            }
            "COMPU_TAB" | "COMPU_VTAB" => {
                if let Some(tab) = parse_compu_tab(body, block_name == "COMPU_VTAB") {
                    doc.compu_tabs.push(tab);
                }
            }
            "FUNCTION" | "GROUP" => {
                if let Some(fg) = parse_function_group(body) {
                    doc.functions.push(fg);
                }
            }
            "MEMORY_SEGMENT" => match parse_memory_segment(body) {
                Some(seg) => doc.memory_segments.push(seg),
                None => doc.diagnostics.push(format!(
                    "Dropped malformed MEMORY_SEGMENT near byte {begin}"
                )),
            },
            "RECORD_LAYOUT" => match parse_record_layout(body) {
                Some(rl) => doc.record_layouts.push(rl),
                None => doc.diagnostics.push(format!(
                    "Dropped malformed RECORD_LAYOUT near byte {begin}"
                )),
            },
            "AXIS_PTS" => match parse_axis_pts(body) {
                Some(ap) => doc.axis_pts.push(ap),
                None => doc.diagnostics.push(format!(
                    "Dropped malformed AXIS_PTS near byte {begin}"
                )),
            },
            "CHARACTERISTIC" => match parse_characteristic(body) {
                Some(c) => doc.characteristics.push(c),
                None => doc.diagnostics.push(format!(
                    "Dropped malformed CHARACTERISTIC near byte {begin}"
                )),
            },
            "COMPU_METHOD" => match parse_compu_method(body) {
                Ok(c) => doc.conversions.push(c),
                Err(reason) => doc.diagnostics.push(reason),
            },
            _ => {}
        }
    }

    Ok(doc)
}

fn parse_axis_descr_blocks(body: &str, diag_out: &mut Vec<String>) -> Vec<A2lAxisDescr> {
    let mut axes = Vec::new();
    let mut rest = body;
    while let Some(pos) = rest.find("/begin AXIS_DESCR") {
        let after = &rest[pos + "/begin AXIS_DESCR".len()..];
        let Some(end_pos) = after.find("/end AXIS_DESCR") else {
            diag_out.push("Unterminated nested AXIS_DESCR skipped".to_string());
            break;
        };
        let sub = &after[..end_pos];
        let tokens = tokenize(sub);
        // attribute, input_quantity, conversion, max_axis_points, lower_limit, upper_limit
        if tokens.len() >= 6 {
            if let (Ok(pts), Ok(lo), Ok(hi)) = (
                tokens[3].parse::<usize>(),
                tokens[4].parse::<f64>(),
                tokens[5].parse::<f64>(),
            ) {
                let axis_pts_ref = tokens
                    .iter()
                    .position(|t| t == "AXIS_PTS_REF")
                    .and_then(|i| tokens.get(i + 1).cloned());
                let byte_order = find_byte_order(&tokens);
                axes.push(A2lAxisDescr {
                    attribute: tokens[0].clone(),
                    input_quantity: tokens[1].clone(),
                    conversion: tokens[2].clone(),
                    max_axis_points: pts,
                    lower_limit: lo,
                    upper_limit: hi,
                    axis_pts_ref,
                    byte_order,
                });
            }
        }
        rest = &after[end_pos + "/end AXIS_DESCR".len()..];
    }
    axes
}

/// Parse one `CHARACTERISTIC` body.
fn parse_characteristic(body: &str) -> Option<A2lCharacteristic> {
    let outer = strip_subblocks(body);
    let tokens = tokenize(&outer);
    if tokens.len() < 9 {
        return None;
    }
    let address = parse_u32_any(&tokens[3])?;
    let lower_limit = tokens[7].parse::<f64>().ok()?;
    let upper_limit = tokens[8].parse::<f64>().ok()?;
    let byte_order = find_byte_order(&tokens);
    let mut _ignored_sub_diag = Vec::new();
    let axes = parse_axis_descr_blocks(body, &mut _ignored_sub_diag);
    Some(A2lCharacteristic {
        name: tokens[0].clone(),
        description: tokens[1].clone(),
        kind: A2lCharacteristicType::from_token(&tokens[2]),
        address,
        record_layout: tokens[4].clone(),
        conversion: tokens[6].clone(),
        lower_limit,
        upper_limit,
        axes,
        byte_order,
    })
}

fn parse_axis_pts(body: &str) -> Option<A2lAxisPts> {
    let outer = strip_subblocks(body);
    let tokens = tokenize(&outer);
    // name, desc, address, input_qty, deposit_ref, max_diff, conversion, max_pts, lo, hi
    if tokens.len() < 10 {
        return None;
    }
    let address = parse_u32_any(&tokens[2])?;
    let max_diff = tokens[5].parse::<f64>().ok()?;
    let max_axis_points = tokens[7].parse::<usize>().ok()?;
    let lower_limit = tokens[8].parse::<f64>().ok()?;
    let upper_limit = tokens[9].parse::<f64>().ok()?;
    let byte_order = find_byte_order(&tokens);
    Some(A2lAxisPts {
        name: tokens[0].clone(),
        description: tokens[1].clone(),
        address,
        input_quantity: tokens[3].clone(),
        deposit: tokens[4].clone(),
        max_diff,
        conversion: tokens[6].clone(),
        max_axis_points,
        lower_limit,
        upper_limit,
        byte_order,
    })
}

fn parse_memory_segment(body: &str) -> Option<A2lMemorySegment> {
    let outer = strip_subblocks(body);
    let tokens = tokenize(&outer);
    // name, desc, prg_type, memory_type, attribute, address, size
    if tokens.len() < 7 {
        return None;
    }
    let address = parse_u32_any(&tokens[5])?;
    let size = parse_u32_any(&tokens[6])?;
    Some(A2lMemorySegment {
        name: tokens[0].clone(),
        description: tokens[1].clone(),
        prg_type: tokens[2].clone(),
        memory_type: tokens[3].clone(),
        attribute: tokens[4].clone(),
        address,
        size,
    })
}

fn parse_record_layout(body: &str) -> Option<A2lRecordLayout> {
    let outer = strip_subblocks(body);
    let tokens = tokenize(&outer);
    let name = tokens.first()?.clone();
    let fnc_pos = tokens.iter().position(|t| t == "FNC_VALUES");
    let fnc_datatype = fnc_pos.and_then(|i| tokens.get(i + 2).cloned());
    let fnc_index_mode = fnc_pos.and_then(|i| tokens.get(i + 3).cloned());
    let ax_datatype = tokens
        .iter()
        .position(|t| t == "AXIS_PTS_X")
        .and_then(|i| tokens.get(i + 2).cloned());
    let ay_datatype = tokens
        .iter()
        .position(|t| t == "AXIS_PTS_Y")
        .and_then(|i| tokens.get(i + 2).cloned());
    Some(A2lRecordLayout {
        name,
        fnc_datatype,
        fnc_index_mode,
        axis_x_datatype: ax_datatype,
        axis_y_datatype: ay_datatype,
    })
}

/// Parse one `COMPU_METHOD` body with `RAT_FUNC`.
fn parse_compu_method(body: &str) -> Result<A2lConversion, String> {
    let tokens = tokenize(body);
    if tokens.len() < 4 {
        return Err("Dropped COMPU_METHOD with < 4 tokens".to_string());
    }
    if tokens[2] != "RAT_FUNC" {
        return Err(format!(
            "Ignored non-RAT_FUNC COMPU_METHOD '{}' (kind={})",
            tokens[0], tokens[2]
        ));
    }
    let Some(coeffs_at) = tokens.iter().position(|t| t == "COEFFS") else {
        return Err(format!(
            "Dropped RAT_FUNC COMPU_METHOD '{}' missing COEFFS",
            tokens[0]
        ));
    };
    if tokens.len() < coeffs_at + 7 {
        return Err(format!(
            "Dropped RAT_FUNC COMPU_METHOD '{}' with truncated COEFFS",
            tokens[0]
        ));
    }
    let mut coeffs = [0.0f64; 6];
    for (i, slot) in coeffs.iter_mut().enumerate() {
        let Ok(v) = tokens[coeffs_at + 1 + i].parse::<f64>() else {
            return Err(format!(
                "Dropped RAT_FUNC COMPU_METHOD '{}' with non-numeric COEFFS",
                tokens[0]
            ));
        };
        *slot = v;
    }
    Ok(A2lConversion {
        name: tokens[0].clone(),
        unit: tokens.get(4).cloned().unwrap_or_default(),
        coeffs,
    })
}

fn parse_u32_any(text: &str) -> Option<u32> {
    let t = text.trim();
    if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16).ok()
    } else {
        t.parse::<u32>().ok()
    }
}

fn parse_compu_tab(body: &str, is_verbal: bool) -> Option<A2lCompuTab> {
    let outer = strip_subblocks(body);
    let tokens = tokenize(&outer);
    if tokens.len() < 4 {
        return None;
    }
    let name = tokens[0].clone();
    let description = tokens[1].clone();
    let count = tokens[3].parse::<usize>().unwrap_or(0);
    let mut entries = Vec::new();
    let mut idx = 4usize;
    for _ in 0..count {
        if idx + 1 >= tokens.len() {
            break;
        }
        if let Ok(key) = tokens[idx].parse::<f64>() {
            entries.push((key, tokens[idx + 1].clone()));
        }
        idx += 2;
    }
    Some(A2lCompuTab {
        name,
        description,
        is_verbal,
        entries,
    })
}

fn parse_function_group(body: &str) -> Option<A2lFunctionGroup> {
    let outer = strip_subblocks(body);
    let outer_toks = tokenize(&outer);
    let name = outer_toks.first()?.clone();
    let description = outer_toks.get(1).cloned().unwrap_or_default();
    let mut characteristics = Vec::new();
    for tag in ["/begin DEF_CHARACTERISTIC", "/begin REF_CHARACTERISTIC"] {
        if let Some(pos) = body.find(tag) {
            let end_tag = if tag.contains("DEF_CHARACTERISTIC") {
                "/end DEF_CHARACTERISTIC"
            } else {
                "/end REF_CHARACTERISTIC"
            };
            let rest = &body[pos + tag.len()..];
            if let Some(end_pos) = rest.find(end_tag) {
                characteristics.extend(tokenize(&rest[..end_pos]));
            }
        }
    }
    Some(A2lFunctionGroup {
        name,
        description,
        characteristics,
    })
}
