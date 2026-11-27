//! XDF `<XDFCHECKSUM>` parser and execution engine (verify & repair).
//!
//! Executes XDF checksum definitions against raw ROM binaries:
//! - Inclusive `[start_address..=end_address]` range (skipping `store_address..store_address+width`
//!   if the store address falls inside the summed range).
//! - Data sizes: 8-bit, 16-bit, or 32-bit elements.
//! - Sum sizes: 8-bit, 16-bit, or 32-bit big-endian or little-endian stored values.
//! - Calculation methods: `0x0`/sum, two's complement, one's complement, XOR, CRC-16/CCITT,
//!   CRC-16/ARC, CRC-32/IEEE.

use crate::primitives::{compute_crc32, crc16_arc, crc16_ccitt};
use quick_xml::events::Event;
use quick_xml::Reader;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct XdfChecksumSpec {
    pub id: String,
    pub name: String,
    pub start_address: usize,
    pub end_address: usize,
    pub store_address: usize,
    pub data_bits: u8,
    pub sum_bits: u8,
    pub little_endian: bool,
    pub calculation_method: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct XdfChecksumResult {
    pub id: String,
    pub name: String,
    pub start_address: usize,
    pub end_address: usize,
    pub store_address: usize,
    pub stored: u32,
    pub computed: u32,
    pub width_bits: u8,
    pub ok: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XdfCalcMethod {
    Sum,
    TwosComplement,
    OnesComplement,
    Xor,
    Crc16Ccitt,
    Crc16Arc,
    Crc32Ieee,
}

impl XdfCalcMethod {
    pub fn parse(raw: &str) -> Result<Self, String> {
        let s = raw.trim().to_ascii_lowercase();
        match s.as_str() {
            "" | "0" | "0x0" | "0x00" | "sum" => Ok(Self::Sum),
            "1" | "0x1" | "0x01" | "twos_complement" | "2s_complement" | "twos" => {
                Ok(Self::TwosComplement)
            }
            "2" | "0x2" | "0x02" | "ones_complement" | "1s_complement" | "ones" => {
                Ok(Self::OnesComplement)
            }
            "3" | "0x3" | "0x03" | "xor" => Ok(Self::Xor),
            "4" | "0x4" | "0x04" | "crc16_ccitt" | "crc16ccitt" => Ok(Self::Crc16Ccitt),
            "5" | "0x5" | "0x05" | "crc16_arc" | "crc16arc" | "crc16" => Ok(Self::Crc16Arc),
            "6" | "0x6" | "0x06" | "crc32" | "crc32_ieee" => Ok(Self::Crc32Ieee),
            other => Err(format!("unsupported XDF calculation method '{other}'")),
        }
    }
}

fn parse_num(s: &str) -> Option<usize> {
    let t = s.trim();
    if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        usize::from_str_radix(hex, 16).ok()
    } else if let Some(hex) = t.strip_prefix('$') {
        usize::from_str_radix(hex, 16).ok()
    } else {
        t.parse::<usize>().ok()
    }
}

fn parse_signed(s: &str) -> Option<i64> {
    let t = s.trim();
    if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        i64::from_str_radix(hex, 16).ok()
    } else {
        t.parse::<i64>().ok()
    }
}

fn apply_base_offset(addr: usize, offset: i64, subtract: bool) -> usize {
    if offset == 0 {
        return addr;
    }
    let adj = if subtract {
        (addr as i64).saturating_sub(offset)
    } else {
        (addr as i64).saturating_add(offset)
    };
    adj.max(0) as usize
}

fn normalize_bits(raw: usize) -> u8 {
    match raw {
        8 | 16 | 32 => raw as u8,
        1 => 8,
        2 => 16,
        4 => 32,
        _ => 16,
    }
}

#[derive(Default)]
struct ParseState {
    id: String,
    name: String,
    start_address: usize,
    end_address: usize,
    store_address: usize,
    data_bits: u8,
    sum_bits: u8,
    little_endian: bool,
    calculation_method: String,
    in_region: bool,
}

#[derive(Copy, Clone)]
enum TextField {
    Title,
    DataStart,
    DataEnd,
    StoreAddress,
    DataSize,
    SumSize,
    CalculationMethod,
    Flags,
}

fn apply_region_attrs(e: &quick_xml::events::BytesStart<'_>, cs: &mut ParseState) {
    for a in e.attributes().flatten() {
        let k = a.key.into_inner().to_ascii_uppercase();
        let v = String::from_utf8_lossy(&a.value);
        match k.as_slice() {
            b"DATASTART" | b"STARTADDRESS" => {
                if let Some(x) = parse_num(&v) {
                    cs.start_address = x;
                }
            }
            b"DATAEND" | b"ENDADDRESS" => {
                if let Some(x) = parse_num(&v) {
                    cs.end_address = x;
                }
            }
            b"STOREADDRESS" => {
                if let Some(x) = parse_num(&v) {
                    cs.store_address = x;
                }
            }
            b"DATASIZE" => {
                if let Some(x) = parse_num(&v) {
                    cs.data_bits = normalize_bits(x);
                }
            }
            b"SUMSIZE" => {
                if let Some(x) = parse_num(&v) {
                    cs.sum_bits = normalize_bits(x);
                }
            }
            b"CALCULATIONMETHOD" => {
                cs.calculation_method = v.trim().to_string();
            }
            b"FLAGS" | b"MMEDTYPEFLAGS" => {
                if let Some(f) = parse_num(&v) {
                    cs.little_endian = (f & 0x02) != 0;
                }
            }
            _ => {}
        }
    }
}

/// Parses `<XDFCHECKSUM>` elements directly from XDF XML text.
pub fn parse_xdf_checksums(xml: &str) -> Result<Vec<XdfChecksumSpec>, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut base_offset: i64 = 0;
    let mut base_subtract = false;
    let mut cur: Option<ParseState> = None;
    let mut field: Option<TextField> = None;
    let mut specs = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let tag = e.name().into_inner().to_ascii_uppercase();
                match tag.as_slice() {
                    b"BASEOFFSET" => {
                        for a in e.attributes().flatten() {
                            let k = a.key.into_inner().to_ascii_uppercase();
                            let v = String::from_utf8_lossy(&a.value);
                            if k.as_slice() == b"OFFSET" {
                                base_offset = parse_signed(&v).unwrap_or(0);
                            } else if k.as_slice() == b"SUBTRACT" {
                                base_subtract = parse_num(&v).unwrap_or(0) != 0;
                            }
                        }
                    }
                    b"XDFCHECKSUM" => {
                        let mut id = String::new();
                        for a in e.attributes().flatten() {
                            if a.key.into_inner().eq_ignore_ascii_case(b"uniqueid") {
                                id = String::from_utf8_lossy(&a.value).trim().to_string();
                            }
                        }
                        cur = Some(ParseState {
                            id,
                            data_bits: 8,
                            sum_bits: 16,
                            ..Default::default()
                        });
                    }
                    b"REGION" => {
                        if let Some(ref mut cs) = cur {
                            cs.in_region = true;
                            apply_region_attrs(e, cs);
                        }
                    }
                    b"TITLE" if cur.is_some() => field = Some(TextField::Title),
                    b"DATASTART" | b"STARTADDRESS" if cur.as_ref().is_some_and(|c| c.in_region) => {
                        field = Some(TextField::DataStart);
                    }
                    b"DATAEND" | b"ENDADDRESS" if cur.as_ref().is_some_and(|c| c.in_region) => {
                        field = Some(TextField::DataEnd);
                    }
                    b"STOREADDRESS" if cur.as_ref().is_some_and(|c| c.in_region) => {
                        field = Some(TextField::StoreAddress);
                    }
                    b"DATASIZE" if cur.as_ref().is_some_and(|c| c.in_region) => {
                        field = Some(TextField::DataSize);
                    }
                    b"SUMSIZE" if cur.as_ref().is_some_and(|c| c.in_region) => {
                        field = Some(TextField::SumSize);
                    }
                    b"CALCULATIONMETHOD" if cur.as_ref().is_some_and(|c| c.in_region) => {
                        field = Some(TextField::CalculationMethod);
                    }
                    b"FLAGS" | b"MMEDTYPEFLAGS" if cur.as_ref().is_some_and(|c| c.in_region) => {
                        field = Some(TextField::Flags);
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(ref e)) => {
                let tag = e.name().into_inner().to_ascii_uppercase();
                match tag.as_slice() {
                    b"BASEOFFSET" => {
                        for a in e.attributes().flatten() {
                            let k = a.key.into_inner().to_ascii_uppercase();
                            let v = String::from_utf8_lossy(&a.value);
                            if k.as_slice() == b"OFFSET" {
                                base_offset = parse_signed(&v).unwrap_or(0);
                            } else if k.as_slice() == b"SUBTRACT" {
                                base_subtract = parse_num(&v).unwrap_or(0) != 0;
                            }
                        }
                    }
                    b"REGION" => {
                        if let Some(ref mut cs) = cur {
                            apply_region_attrs(e, cs);
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(ref e)) => {
                if let (Some(ref mut cs), Some(f)) = (cur.as_mut(), field) {
                    let txt = e.unescape().map_err(|err| err.to_string())?;
                    match f {
                        TextField::Title => cs.name.push_str(&txt),
                        TextField::DataStart => {
                            if let Some(v) = parse_num(&txt) {
                                cs.start_address = v;
                            }
                        }
                        TextField::DataEnd => {
                            if let Some(v) = parse_num(&txt) {
                                cs.end_address = v;
                            }
                        }
                        TextField::StoreAddress => {
                            if let Some(v) = parse_num(&txt) {
                                cs.store_address = v;
                            }
                        }
                        TextField::DataSize => {
                            if let Some(v) = parse_num(&txt) {
                                cs.data_bits = normalize_bits(v);
                            }
                        }
                        TextField::SumSize => {
                            if let Some(v) = parse_num(&txt) {
                                cs.sum_bits = normalize_bits(v);
                            }
                        }
                        TextField::CalculationMethod => cs.calculation_method = txt.trim().to_string(),
                        TextField::Flags => {
                            if let Some(v) = parse_num(&txt) {
                                cs.little_endian = (v & 0x02) != 0;
                            }
                        }
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let tag = e.name().into_inner().to_ascii_uppercase();
                match tag.as_slice() {
                    b"TITLE"
                    | b"DATASTART"
                    | b"STARTADDRESS"
                    | b"DATAEND"
                    | b"ENDADDRESS"
                    | b"STOREADDRESS"
                    | b"DATASIZE"
                    | b"SUMSIZE"
                    | b"CALCULATIONMETHOD"
                    | b"FLAGS"
                    | b"MMEDTYPEFLAGS" => field = None,
                    b"REGION" => {
                        if let Some(ref mut cs) = cur {
                            cs.in_region = false;
                        }
                    }
                    b"XDFCHECKSUM" => {
                        if let Some(cs) = cur.take() {
                            let idx = specs.len() + 1;
                            let id = if cs.id.is_empty() {
                                format!("checksum_{idx}")
                            } else {
                                cs.id
                            };
                            let name = if cs.name.trim().is_empty() {
                                format!("Checksum_{id}")
                            } else {
                                cs.name.trim().to_string()
                            };
                            specs.push(XdfChecksumSpec {
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
                                data_bits: cs.data_bits.max(8),
                                sum_bits: cs.sum_bits.max(8),
                                little_endian: cs.little_endian,
                                calculation_method: cs.calculation_method,
                            });
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("XDF checksum XML parse error: {e}")),
            _ => {}
        }
        buf.clear();
    }
    Ok(specs)
}

fn read_word(buf: &[u8], off: usize, bits: u8, le: bool) -> Result<u32, String> {
    let bytes = match bits {
        8 => 1,
        16 => 2,
        32 => 4,
        other => return Err(format!("unsupported XDF word width {other} bits")),
    };
    if off.saturating_add(bytes) > buf.len() {
        return Err(format!("address {off:#X} ({bytes} B) out of bounds for ROM length {}", buf.len()));
    }
    Ok(match bits {
        8 => buf[off] as u32,
        16 => {
            if le {
                u16::from_le_bytes([buf[off], buf[off + 1]]) as u32
            } else {
                u16::from_be_bytes([buf[off], buf[off + 1]]) as u32
            }
        }
        32 => {
            if le {
                u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
            } else {
                u32::from_be_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
            }
        }
        _ => unreachable!(),
    })
}

fn write_word(buf: &mut [u8], off: usize, bits: u8, le: bool, value: u32) -> Result<(), String> {
    let bytes = (bits / 8) as usize;
    if off.saturating_add(bytes) > buf.len() {
        return Err(format!("store address {off:#X} out of bounds for ROM length {}", buf.len()));
    }
    match bits {
        8 => buf[off] = value as u8,
        16 => {
            let b = if le {
                (value as u16).to_le_bytes()
            } else {
                (value as u16).to_be_bytes()
            };
            buf[off..off + 2].copy_from_slice(&b);
        }
        32 => {
            let b = if le {
                value.to_le_bytes()
            } else {
                value.to_be_bytes()
            };
            buf[off..off + 4].copy_from_slice(&b);
        }
        other => return Err(format!("unsupported XDF store width {other} bits")),
    }
    Ok(())
}

fn mask_for_bits(bits: u8) -> Result<u32, String> {
    match bits {
        8 => Ok(0xFF),
        16 => Ok(0xFFFF),
        32 => Ok(0xFFFF_FFFF),
        other => Err(format!("unsupported sum_bits {other}")),
    }
}

/// Computes the expected stored checksum value for `spec` over `rom`.
pub fn compute_xdf_checksum(rom: &[u8], spec: &XdfChecksumSpec) -> Result<u32, String> {
    if spec.start_address > spec.end_address || spec.end_address >= rom.len() {
        return Err(format!(
            "XDF checksum '{}' range {:#X}..={:#X} out of bounds for {} B ROM",
            spec.name,
            spec.start_address,
            spec.end_address,
            rom.len()
        ));
    }
    let method = XdfCalcMethod::parse(&spec.calculation_method)?;
    let sum_mask = mask_for_bits(spec.sum_bits)?;
    let store_bytes = (spec.sum_bits / 8) as usize;
    let store_start = spec.store_address;
    let store_end_excl = store_start.saturating_add(store_bytes);
    let data_bytes = (spec.data_bits / 8) as usize;
    if !matches!(spec.data_bits, 8 | 16 | 32) {
        return Err(format!("unsupported data_bits {}", spec.data_bits));
    }

    // Build filtered data slice excluding stored checksum bytes when they fall inside [start..=end].
    let compute_crc_input = || -> Vec<u8> {
        let mut out = Vec::with_capacity(spec.end_address - spec.start_address + 1);
        for (idx, &b) in rom[spec.start_address..=spec.end_address].iter().enumerate() {
            let addr = spec.start_address + idx;
            if addr >= store_start && addr < store_end_excl {
                continue;
            }
            out.push(b);
        }
        out
    };

    let val = match method {
        XdfCalcMethod::Crc16Ccitt => crc16_ccitt(&compute_crc_input()) as u32 & sum_mask,
        XdfCalcMethod::Crc16Arc => crc16_arc(&compute_crc_input(), 0) as u32 & sum_mask,
        XdfCalcMethod::Crc32Ieee => compute_crc32(&compute_crc_input()) & sum_mask,
        XdfCalcMethod::Sum
        | XdfCalcMethod::TwosComplement
        | XdfCalcMethod::OnesComplement
        | XdfCalcMethod::Xor => {
            let mut acc: u64 = 0;
            let mut addr = spec.start_address;
            while addr + data_bytes - 1 <= spec.end_address {
                if addr + data_bytes > store_start && addr < store_end_excl {
                    addr += data_bytes;
                    continue;
                }
                let word = read_word(rom, addr, spec.data_bits, spec.little_endian)? as u64;
                match method {
                    XdfCalcMethod::Xor => acc ^= word,
                    _ => acc = acc.wrapping_add(word),
                }
                addr += data_bytes;
            }
            let raw = (acc as u32) & sum_mask;
            match method {
                XdfCalcMethod::TwosComplement => (!raw).wrapping_add(1) & sum_mask,
                XdfCalcMethod::OnesComplement => (!raw) & sum_mask,
                _ => raw,
            }
        }
    };

    Ok(val)
}

/// Verifies a single `<XDFCHECKSUM>` specification against `rom`.
pub fn verify_xdf_checksum(rom: &[u8], spec: &XdfChecksumSpec) -> Result<XdfChecksumResult, String> {
    let computed = compute_xdf_checksum(rom, spec)?;
    let stored = read_word(rom, spec.store_address, spec.sum_bits, spec.little_endian)?;
    Ok(XdfChecksumResult {
        id: spec.id.clone(),
        name: spec.name.clone(),
        start_address: spec.start_address,
        end_address: spec.end_address,
        store_address: spec.store_address,
        stored,
        computed,
        width_bits: spec.sum_bits,
        ok: stored == computed,
    })
}

/// Repairs `rom` for all supplied `<XDFCHECKSUM>` specs and returns `(repaired_rom, results)`.
pub fn repair_xdf_checksums(
    rom: &[u8],
    specs: &[XdfChecksumSpec],
) -> Result<(Vec<u8>, Vec<XdfChecksumResult>), String> {
    let mut out = rom.to_vec();
    for spec in specs {
        let computed = compute_xdf_checksum(&out, spec)?;
        write_word(&mut out, spec.store_address, spec.sum_bits, spec.little_endian, computed)?;
    }
    let mut results = Vec::with_capacity(specs.len());
    for spec in specs {
        results.push(verify_xdf_checksum(&out, spec)?);
    }
    Ok((out, results))
}
