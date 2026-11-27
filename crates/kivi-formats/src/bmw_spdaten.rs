//! BMW SP-Daten (`.0pa` / `.0da`), Intel HEX (`.hex`), and Motorola S-Record (`.s19`/`.s28`/`.s37`) codecs.
//!
//! Handles real BMW WinKFP/SP-Daten files whose payloads are Intel HEX or Motorola SREC
//! preceded by `$$` metadata lines (e.g., `$$ZB-NUMBER`, `$$P-SGBD`, `$$SW-REF`).

use std::collections::BTreeMap;

/// Contiguous byte region decoded from an ASCII hex/SREC container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HexSegment {
    pub address: u32,
    pub data: Vec<u8>,
}

/// Parsed Intel HEX, Motorola S-Record, or BMW SP-Daten (`.0pa`/`.0da`) file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HexImageDocument {
    pub format: String,
    pub metadata: BTreeMap<String, String>,
    pub entry_point: Option<u32>,
    pub segments: Vec<HexSegment>,
}

impl HexImageDocument {
    /// Flattens sparse segments onto `[base_address, base_address + size)` filled with `fill_byte`.
    pub fn assemble_rom(&self, base_address: u32, size: usize, fill_byte: u8) -> Result<Vec<u8>, String> {
        let mut rom = vec![fill_byte; size];
        for seg in &self.segments {
            let off = seg
                .address
                .checked_sub(base_address)
                .ok_or_else(|| format!("Segment @0x{:X} below base 0x{:X}", seg.address, base_address))?
                as usize;
            let end = off
                .checked_add(seg.data.len())
                .ok_or_else(|| "Segment overflow".to_string())?;
            if end > rom.len() {
                return Err(format!(
                    "Segment @0x{:X}..0x{:X} exceeds target size {size}",
                    seg.address,
                    seg.address + seg.data.len() as u32
                ));
            }
            rom[off..end].copy_from_slice(&seg.data);
        }
        Ok(rom)
    }
}

fn push_segment_bytes(segments: &mut Vec<HexSegment>, address: u32, bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }
    if let Some(last) = segments.last_mut() {
        if last.address.saturating_add(last.data.len() as u32) == address {
            last.data.extend_from_slice(bytes);
            return;
        }
    }
    segments.push(HexSegment {
        address,
        data: bytes.to_vec(),
    });
}

fn decode_hex_pair(pair: &[u8]) -> Result<u8, String> {
    let s = std::str::from_utf8(pair).map_err(|_| "Non-UTF8 hex digit".to_string())?;
    u8::from_str_radix(s, 16).map_err(|_| format!("Invalid hex byte '{s}'"))
}

fn parse_spdaten_header_line(line: &str, meta: &mut BTreeMap<String, String>) {
    let body = line.trim_start_matches('$').trim();
    if body.is_empty() {
        return;
    }
    if let Some((k, v)) = body.split_once(':').or_else(|| body.split_once('=')) {
        meta.insert(k.trim().to_string(), v.trim().to_string());
    } else if let Some((k, v)) = body.split_once(char::is_whitespace) {
        meta.insert(k.trim().to_string(), v.trim().to_string());
    } else {
        let idx = meta.len();
        meta.insert(format!("header_{idx}"), body.to_string());
    }
}

/// Parses Intel HEX (`.hex`, BMW `.0pa`/`.0da`) or Motorola S-Record (`.s19`/`.srec`).
pub fn parse_hex_or_srec(text: &str) -> Result<HexImageDocument, String> {
    let mut doc = HexImageDocument::default();
    let mut upper_base: u32 = 0;
    let mut saw_ihex = false;
    let mut saw_srec = false;
    let mut saw_spdaten = false;

    for (line_no, raw_line) in text.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') || line.starts_with("//") {
            continue;
        }
        if line.starts_with('$') {
            saw_spdaten = true;
            parse_spdaten_header_line(line, &mut doc.metadata);
            continue;
        }
        if let Some(hex_body) = line.strip_prefix(':') {
            saw_ihex = true;
            let b = hex_body.as_bytes();
            if b.len() < 10 || !b.len().is_multiple_of(2) {
                return Err(format!("Line {}: malformed Intel HEX record", line_no + 1));
            }
            let mut raw = Vec::with_capacity(b.len() / 2);
            for chunk in b.as_chunks::<2>().0 {
                raw.push(decode_hex_pair(chunk)?);
            }
            let count = raw[0] as usize;
            if raw.len() != count + 5 {
                return Err(format!("Line {}: Intel HEX byte count mismatch", line_no + 1));
            }
            let csum = raw.iter().fold(0u8, |acc, &v| acc.wrapping_add(v));
            if csum != 0 {
                return Err(format!("Line {}: Intel HEX checksum mismatch", line_no + 1));
            }
            let addr16 = ((raw[1] as u32) << 8) | (raw[2] as u32);
            let rec_type = raw[3];
            let payload = &raw[4..4 + count];
            match rec_type {
                0x00 => push_segment_bytes(&mut doc.segments, upper_base | addr16, payload),
                0x01 => break,
                0x02 => {
                    if payload.len() != 2 {
                        return Err(format!("Line {}: invalid EXT_SEG record", line_no + 1));
                    }
                    upper_base = (((payload[0] as u32) << 8) | (payload[1] as u32)) << 4;
                }
                0x03 => {
                    if payload.len() == 4 {
                        let cs = ((payload[0] as u32) << 8) | (payload[1] as u32);
                        let ip = ((payload[2] as u32) << 8) | (payload[3] as u32);
                        doc.entry_point = Some((cs << 4) + ip);
                    }
                }
                0x04 => {
                    if payload.len() != 2 {
                        return Err(format!("Line {}: invalid EXT_LIN record", line_no + 1));
                    }
                    upper_base = (((payload[0] as u32) << 8) | (payload[1] as u32)) << 16;
                }
                0x05 => {
                    if payload.len() == 4 {
                        doc.entry_point = Some(u32::from_be_bytes([
                            payload[0], payload[1], payload[2], payload[3],
                        ]));
                    }
                }
                other => {
                    return Err(format!(
                        "Line {}: unsupported Intel HEX record type 0x{other:02X}",
                        line_no + 1
                    ));
                }
            }
            continue;
        }
        if let Some(after_s) = line.strip_prefix('S') {
            saw_srec = true;
            if after_s.len() < 5 || !(after_s.len() - 1).is_multiple_of(2) {
                return Err(format!("Line {}: malformed S-Record", line_no + 1));
            }
            let kind = after_s.as_bytes()[0];
            let hex_rest = &after_s.as_bytes()[1..];
            let mut raw = Vec::with_capacity(hex_rest.len() / 2);
            for chunk in hex_rest.as_chunks::<2>().0 {
                raw.push(decode_hex_pair(chunk)?);
            }
            let count = raw[0] as usize;
            if raw.len() != count + 1 || count < 2 {
                return Err(format!("Line {}: S-Record count mismatch", line_no + 1));
            }
            let sum_plus_csum = raw.iter().fold(0u8, |acc, &v| acc.wrapping_add(v));
            if sum_plus_csum != 0xFF {
                return Err(format!("Line {}: S-Record checksum mismatch", line_no + 1));
            }
            let body = &raw[1..raw.len() - 1];
            match kind {
                b'0' => {
                    if body.len() > 2 {
                        let s = String::from_utf8_lossy(&body[2..]).trim().to_string();
                        if !s.is_empty() {
                            doc.metadata.insert("s0_header".to_string(), s);
                        }
                    }
                }
                b'1' => {
                    let addr = ((body[0] as u32) << 8) | (body[1] as u32);
                    push_segment_bytes(&mut doc.segments, addr, &body[2..]);
                }
                b'2' => {
                    if body.len() < 3 {
                        return Err(format!("Line {}: truncated S2 address", line_no + 1));
                    }
                    let addr = ((body[0] as u32) << 16) | ((body[1] as u32) << 8) | (body[2] as u32);
                    push_segment_bytes(&mut doc.segments, addr, &body[3..]);
                }
                b'3' => {
                    if body.len() < 4 {
                        return Err(format!("Line {}: truncated S3 address", line_no + 1));
                    }
                    let addr = u32::from_be_bytes([body[0], body[1], body[2], body[3]]);
                    push_segment_bytes(&mut doc.segments, addr, &body[4..]);
                }
                b'5' | b'6' => {}
                b'7' => {
                    if body.len() >= 4 {
                        doc.entry_point = Some(u32::from_be_bytes([body[0], body[1], body[2], body[3]]));
                    }
                    break;
                }
                b'8' => {
                    if body.len() >= 3 {
                        doc.entry_point = Some(((body[0] as u32) << 16) | ((body[1] as u32) << 8) | (body[2] as u32));
                    }
                    break;
                }
                b'9' => {
                    if body.len() >= 2 {
                        doc.entry_point = Some(((body[0] as u32) << 8) | (body[1] as u32));
                    }
                    break;
                }
                other => {
                    return Err(format!(
                        "Line {}: unsupported S-Record type S{}",
                        line_no + 1,
                        other as char
                    ));
                }
            }
            continue;
        }
        return Err(format!("Line {}: unrecognised HEX/SREC/SP-Daten syntax", line_no + 1));
    }

    if doc.segments.is_empty() {
        return Err("No data records found in HEX/SREC/SP-Daten input".to_string());
    }
    doc.format = match (saw_spdaten, saw_ihex, saw_srec) {
        (true, true, _) => "bmw_spdaten_ihex".to_string(),
        (true, false, true) => "bmw_spdaten_srec".to_string(),
        (false, true, _) => "intel_hex".to_string(),
        (false, false, true) => "motorola_srec".to_string(),
        _ => "unknown".to_string(),
    };
    Ok(doc)
}

fn push_ihex_record(out: &mut String, addr16: u16, rec_type: u8, data: &[u8]) {
    let mut sum = (data.len() as u8)
        .wrapping_add((addr16 >> 8) as u8)
        .wrapping_add(addr16 as u8)
        .wrapping_add(rec_type);
    out.push(':');
    out.push_str(&format!("{:02X}{:04X}{:02X}", data.len(), addr16, rec_type));
    for &b in data {
        sum = sum.wrapping_add(b);
        out.push_str(&format!("{:02X}", b));
    }
    let csum = (!sum).wrapping_add(1);
    out.push_str(&format!("{:02X}\r\n", csum));
}

/// Serializes segments (with optional BMW `$$` SP-Daten header comments) to extended-linear Intel HEX.
pub fn export_intel_hex(
    segments: &[HexSegment],
    spdaten_metadata: Option<&BTreeMap<String, String>>,
    bytes_per_line: usize,
) -> String {
    let mut out = String::new();
    if let Some(meta) = spdaten_metadata {
        for (k, v) in meta {
            out.push_str(&format!("$${k}: {v}\r\n"));
        }
    }
    let chunk_size = bytes_per_line.clamp(1, 32);
    let mut cur_upper: Option<u16> = None;

    for seg in segments {
        let mut off = 0usize;
        while off < seg.data.len() {
            let abs_addr = seg.address + off as u32;
            let upper = (abs_addr >> 16) as u16;
            if cur_upper != Some(upper) {
                push_ihex_record(&mut out, 0, 0x04, &upper.to_be_bytes());
                cur_upper = Some(upper);
            }
            let low = abs_addr as u16;
            let to_page_end = (0x10000usize - low as usize).min(chunk_size);
            let end = (off + to_page_end).min(seg.data.len());
            push_ihex_record(&mut out, low, 0x00, &seg.data[off..end]);
            off = end;
        }
    }
    push_ihex_record(&mut out, 0, 0x01, &[]);
    out
}

/// Serializes segments to Motorola S3 S-Record (32-bit addresses).
pub fn export_motorola_s3(segments: &[HexSegment], header: Option<&str>, bytes_per_line: usize) -> String {
    let mut out = String::new();
    let emit = |out: &mut String, kind: char, body: &[u8]| {
        let count = (body.len() + 1) as u8;
        let sum = body
            .iter()
            .fold(count, |acc, &b| acc.wrapping_add(b));
        let csum = !sum;
        out.push('S');
        out.push(kind);
        out.push_str(&format!("{:02X}", count));
        for &b in body {
            out.push_str(&format!("{:02X}", b));
        }
        out.push_str(&format!("{:02X}\r\n", csum));
    };

    if let Some(h) = header {
        let mut s0 = vec![0u8, 0u8];
        s0.extend_from_slice(h.as_bytes());
        emit(&mut out, '0', &s0);
    }

    let chunk_size = bytes_per_line.clamp(1, 32);
    for seg in segments {
        for (idx, chunk) in seg.data.chunks(chunk_size).enumerate() {
            let addr = seg.address + (idx * chunk_size) as u32;
            let mut body = Vec::with_capacity(4 + chunk.len());
            body.extend_from_slice(&addr.to_be_bytes());
            body.extend_from_slice(chunk);
            emit(&mut out, '3', &body);
        }
    }
    emit(&mut out, '7', &[0, 0, 0, 0]);
    out
}
