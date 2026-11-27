//! ODX 2.0.1 FLASH parsing — the document inside a VAG `.frf`.
//!
//! ## Where the shape comes from
//!
//! `VW_Flash/extractodx.py` and the `simos-suite` research note
//! `bin-to-frf-sgo-packing.md`:
//!
//! ```text
//! FLASH > ECU-MEMS > ECU-MEM > MEM > SESSIONS > SESSION
//!       > { SECURITYS, DATABLOCKS, FLASHDATAS }
//! ```
//!
//! Each `FLASHDATA` carries `<DATA>` as upper-case hex and an
//! `<ENCRYPT-COMPRESS-METHOD>` (the DFI byte) saying how that payload is
//! transformed. Each `DATABLOCK` links to a `FLASHDATA` and names the block by
//! its `SOURCE-START-ADDRESS`.
//!
//! ## Why this matters
//!
//! kiviTuner could decrypt an FRF to bytes. Without reading the ODX inside it,
//! an unlocked FRF still could not be used: you cannot flash a ZIP.
//!
//! ## What this is not
//!
//! It does not apply the DFI transform. `Raw` blocks are ready to flash;
//! `SimosCounterXor` and `LzssAes` need their own decoders, which already exist
//! elsewhere in this crate and are applied by the caller.

/// How a `FLASHDATA` payload is transformed before it can be used.
///
/// The byte is called the DFI (data format identifier) in ODX and comes from the
/// `ENCRYPT-COMPRESS-METHOD` element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataFormatIdentifier {
    /// `0x00` — the payload is the flash bytes.
    Raw,
    /// `0x01` — Simos counter-XOR.
    SimosCounterXor,
    /// `0xAA` — Bosch LZSS wrapped in AES.
    LzssAes,
    /// Any other value, preserved so a caller can report it.
    Unknown(u8),
}

impl DataFormatIdentifier {
    pub fn from_byte(b: u8) -> Self {
        match b {
            0x00 => DataFormatIdentifier::Raw,
            0x01 => DataFormatIdentifier::SimosCounterXor,
            0xAA => DataFormatIdentifier::LzssAes,
            other => DataFormatIdentifier::Unknown(other),
        }
    }

    pub fn as_byte(self) -> u8 {
        match self {
            DataFormatIdentifier::Raw => 0x00,
            DataFormatIdentifier::SimosCounterXor => 0x01,
            DataFormatIdentifier::LzssAes => 0xAA,
            DataFormatIdentifier::Unknown(b) => b,
        }
    }
}

/// One `FLASHDATA` element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OdxFlashData {
    pub short_name: String,
    /// Decoded payload bytes.
    pub data: Vec<u8>,
    pub method: DataFormatIdentifier,
}

/// One `DATABLOCK` element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OdxDataBlock {
    pub short_name: String,
    /// The `FLASHDATA` this block references.
    pub flashdata_ref: String,
    /// The decimal block number the loader keys on.
    pub source_start_address: u32,
}

/// A parsed ODX FLASH document.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OdxFlashDocument {
    pub flash_data: Vec<OdxFlashData>,
    pub data_blocks: Vec<OdxDataBlock>,
}

/// Why an ODX document could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OdxError {
    /// A mandatory element is absent.
    MissingElement(&'static str),
    /// A `<DATA>` payload is not valid hex.
    InvalidHex(String),
}

impl std::fmt::Display for OdxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OdxError::MissingElement(tag) => write!(f, "ODX is missing the <{tag}> element"),
            OdxError::InvalidHex(m) => write!(f, "ODX <DATA> payload is not hex: {m}"),
        }
    }
}

impl std::error::Error for OdxError {}

/// Extract the text of the first `<tag>…</tag>` inside `haystack`.
fn tag_text<'a>(haystack: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = haystack.find(&open)? + open.len();
    let end = haystack[start..].find(&close)? + start;
    Some(&haystack[start..end])
}

/// Every `<tag>…</tag>` block inside `haystack`.
fn all_blocks<'a>(haystack: &'a str, tag: &str) -> Vec<&'a str> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut out = Vec::new();
    let mut cursor = 0usize;
    while let Some(rel) = haystack[cursor..].find(&open) {
        let start = cursor + rel;
        // Advance past the opening tag's `>` so nested same-name tags do not
        // confuse the search.
        let Some(gt) = haystack[start..].find('>') else { break };
        let body_start = start + gt + 1;
        let Some(end_rel) = haystack[body_start..].find(&close) else { break };
        out.push(&haystack[body_start..body_start + end_rel]);
        cursor = body_start + end_rel + close.len();
    }
    out
}

/// Decode a hex string, rejecting odd length and non-hex characters.
fn decode_hex(s: &str) -> Result<Vec<u8>, OdxError> {
    let t = s.trim();
    if t.is_empty() {
        return Ok(Vec::new());
    }
    if !t.len().is_multiple_of(2) {
        return Err(OdxError::InvalidHex(format!(
            "odd length {} (a hex payload must have an even digit count)",
            t.len()
        )));
    }
    let bytes = t.as_bytes();
    let mut out = Vec::with_capacity(t.len() / 2);
    for pair in bytes.as_chunks::<2>().0 {
        let hi = (pair[0] as char)
            .to_digit(16)
            .ok_or_else(|| OdxError::InvalidHex(format!("bad digit '{}'", pair[0] as char)))?;
        let lo = (pair[1] as char)
            .to_digit(16)
            .ok_or_else(|| OdxError::InvalidHex(format!("bad digit '{}'", pair[1] as char)))?;
        out.push(((hi << 4) | lo) as u8);
    }
    Ok(out)
}

/// Parse an ODX FLASH document.
pub fn parse_odx_flash(xml: &str) -> Result<OdxFlashDocument, OdxError> {
    // Require the element, not just the substring: "<FLASH" also matches
    // "<FLASHDATA". Check for the closing tag as well.
    if !xml.contains("<FLASH>") && !xml.contains("<FLASH ") {
        return Err(OdxError::MissingElement("FLASH"));
    }

    let mut doc = OdxFlashDocument::default();

    for block in all_blocks(xml, "FLASHDATA") {
        let short_name = tag_text(block, "SHORT-NAME")
            .ok_or(OdxError::MissingElement("SHORT-NAME"))?
            .trim()
            .to_string();
        let dfi_text = tag_text(block, "ENCRYPT-COMPRESS-METHOD").unwrap_or("00").trim();
        let dfi = u8::from_str_radix(dfi_text, 16)
            .map(DataFormatIdentifier::from_byte)
            .unwrap_or(DataFormatIdentifier::Raw);
        let data_text = tag_text(block, "DATA").unwrap_or("");
        let data = decode_hex(data_text)?;
        doc.flash_data.push(OdxFlashData {
            short_name,
            data,
            method: dfi,
        });
    }

    for block in all_blocks(xml, "DATABLOCK") {
        let short_name = tag_text(block, "SHORT-NAME")
            .ok_or(OdxError::MissingElement("SHORT-NAME"))?
            .trim()
            .to_string();
        // The reference is a `SNREF SHORT-NAME="…"` attribute, not element text.
        let flashdata_ref = extract_attr(block, "SHORT-NAME").unwrap_or_default();
        let source_start_address = tag_text(block, "SOURCE-START-ADDRESS")
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);
        doc.data_blocks.push(OdxDataBlock {
            short_name,
            flashdata_ref,
            source_start_address,
        });
    }

    if doc.flash_data.is_empty() && doc.data_blocks.is_empty() {
        return Err(OdxError::MissingElement("FLASHDATA"));
    }

    Ok(doc)
}

/// Read the first `name="value"` attribute in `haystack`.
///
/// The `DATABLOCK` reference uses `SHORT-NAME` as an attribute, while the block
/// itself has it as element text; both appear in the same block, so the search
/// has to distinguish them.
fn extract_attr(haystack: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = haystack.find(&needle)? + needle.len();
    let end = haystack[start..].find('"')? + start;
    Some(haystack[start..end].to_string())
}
