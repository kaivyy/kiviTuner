//! GM Delco E92 RLE module compression and 12-byte wire framing codec.
//!
//! Each control byte is `[unit:2][count:6]`:
//! - `0b00`: literal run of `count` bytes
//! - `0b01`: `count` copies of next 1 byte
//! - `0b10`: `count` copies of next 2 bytes
//! - `0b11`: `count` copies of next 4 bytes
//!
//! Control byte `0x00` terminates the stream.

/// Length of the 12-byte E92 module wrapper header.
pub const WRAPPER_LENGTH: usize = 12;
const MAX_COUNT: usize = 0x3F;
const UNIT_SIZE: [usize; 4] = [0, 1, 2, 4];

/// Wraps a coded payload for GMLAN compressed RequestDownload (`[0xFF; 4][0; 4][len BE32]`).
pub fn wrap(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(WRAPPER_LENGTH + payload.len());
    out.extend_from_slice(&[0xFF; 4]);
    out.extend_from_slice(&[0x00; 4]);
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(payload);
    out
}

/// Returns `true` when `module` carries a valid 12-byte E92 wrapper whose length matches.
pub fn is_wrapped(module: &[u8]) -> bool {
    if module.len() < WRAPPER_LENGTH || module[..4] != [0xFF; 4] {
        return false;
    }
    let declared = u32::from_be_bytes([module[8], module[9], module[10], module[11]]) as usize;
    declared == module.len() - WRAPPER_LENGTH
}

/// Strips the 12-byte wrapper header and returns the coded payload slice.
pub fn unwrap(module: &[u8]) -> Result<&[u8], String> {
    if !is_wrapped(module) {
        return Err("Bytes are not a wrapped E92 flash module".to_string());
    }
    Ok(&module[WRAPPER_LENGTH..])
}

fn repeat_count(data: &[u8], pos: usize, unit: usize) -> usize {
    if pos + unit > data.len() {
        return 0;
    }
    let mut count = 1usize;
    while count < MAX_COUNT && pos + (count + 1) * unit <= data.len() {
        let next = pos + count * unit;
        if data[next..next + unit] != data[pos..pos + unit] {
            break;
        }
        count += 1;
    }
    if count > 1 {
        count
    } else {
        0
    }
}

fn append_literals(coded: &mut Vec<u8>, data: &[u8], mut start: usize, mut len: usize) {
    while len > 0 {
        let take = len.min(MAX_COUNT);
        coded.push(take as u8);
        coded.extend_from_slice(&data[start..start + take]);
        start += take;
        len -= take;
    }
}

/// RLE-compresses a raw calibration/segment image into an E92 payload.
pub fn compress(data: &[u8]) -> Vec<u8> {
    let mut coded = Vec::with_capacity(data.len() / 2);
    let mut literal_start = 0usize;
    let mut pos = 0usize;

    while pos < data.len() {
        let mut best_unit = 0usize;
        let mut best_count = 0usize;
        for &unit in &UNIT_SIZE[1..] {
            let count = repeat_count(data, pos, unit);
            if count * unit <= unit + 1 || count * unit <= best_count * best_unit {
                continue;
            }
            best_unit = unit;
            best_count = count;
        }
        if best_count == 0 {
            pos += 1;
            continue;
        }
        append_literals(&mut coded, data, literal_start, pos - literal_start);
        let tag = match best_unit {
            1 => 1u8,
            2 => 2u8,
            4 => 3u8,
            _ => unreachable!(),
        };
        coded.push((tag << 6) | (best_count as u8));
        coded.extend_from_slice(&data[pos..pos + best_unit]);
        pos += best_count * best_unit;
        literal_start = pos;
    }

    append_literals(&mut coded, data, literal_start, data.len() - literal_start);
    coded
}

/// Decompresses an E92 RLE-coded payload back into raw bytes.
pub fn decompress(coded: &[u8]) -> Result<Vec<u8>, String> {
    let mut data = Vec::with_capacity(coded.len() * 2);
    let mut pos = 0usize;

    while pos < coded.len() {
        let ctrl = coded[pos];
        pos += 1;
        if ctrl == 0 {
            break;
        }
        let count = (ctrl as usize) & MAX_COUNT;
        let unit = UNIT_SIZE[(ctrl >> 6) as usize];
        let available = coded.len() - pos;

        if unit == 0 {
            if count > available {
                return Err("E92 RLE module truncated inside literal run".to_string());
            }
            data.extend_from_slice(&coded[pos..pos + count]);
            pos += count;
            continue;
        }

        if unit > available {
            return Err("E92 RLE module truncated inside repeat run".to_string());
        }
        let unit_bytes = &coded[pos..pos + unit];
        for _ in 0..count {
            data.extend_from_slice(unit_bytes);
        }
        pos += unit;
    }

    Ok(data)
}
