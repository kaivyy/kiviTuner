//! VAG SGO/FRF container packing, VW-LZSS compression/decompression, and ZIP-ODX framing.
//!
//! Complements [`crate::container`] and [`crate::odx_flash`] without duplicating their logic:
//! - [`lzss_compress`] / [`lzss_decompress`]: VAG/Bosch 1023-byte sliding-window LZSS (`VW_Flash/lib/lzss/lzss.c`)
//! - [`pack_sgo_container`]: builds `"SGML Object File\0"` binaries with `@0x15` checksum valid under `SgoContainer::verify_sgo_whole_file_checksum`
//! - [`frf_unpack_odx`] / [`frf_pack_odx`]: rolling-XOR + ZIP DEFLATE ODX container extraction & repacking

use crate::container::{frf_plaintext_kind, FrfPlaintext, SgoContainer};
use kivi_checksum::primitives::compute_crc32;
use miniz_oxide::deflate::compress_to_vec;
use miniz_oxide::inflate::decompress_to_vec;

const LZSS_WINDOW_SIZE: usize = 1023;
const LZSS_MAX_UNCODED: usize = 2;
const LZSS_MAX_CODED: usize = 63;

/// Decompresses a VAG/Bosch sliding-window LZSS stream (`WINDOW_SIZE = 1023`).
pub fn lzss_decompress(input: &[u8]) -> Result<Vec<u8>, String> {
    let mut window = [b' '; LZSS_WINDOW_SIZE];
    let mut out = Vec::with_capacity(input.len() * 2);
    let mut next_char = 0usize;
    let mut pos = 0usize;
    let mut flags = 0u8;
    let mut flags_used = 7u8;

    while pos < input.len() {
        flags = flags.wrapping_shl(1);
        flags_used += 1;
        if flags_used == 8 {
            flags = input[pos];
            pos += 1;
            flags_used = 0;
        }
        if (flags & 0x80) == 0 {
            if pos >= input.len() {
                break;
            }
            let b = input[pos];
            pos += 1;
            out.push(b);
            window[next_char] = b;
            next_char = (next_char + 1) % LZSS_WINDOW_SIZE;
        } else {
            if pos + 2 > input.len() {
                break;
            }
            let b0 = input[pos] as usize;
            let b1 = input[pos + 1] as usize;
            pos += 2;
            let raw_off = b1 + ((b0 & 0x03) << 8);
            if raw_off > LZSS_WINDOW_SIZE {
                return Err(format!("LZSS offset {raw_off} exceeds window {LZSS_WINDOW_SIZE}"));
            }
            let code_off = LZSS_WINDOW_SIZE - raw_off;
            let len = b0 >> 2;
            if len == 0 {
                continue;
            }
            let mut tmp = [0u8; 64];
            for (i, slot) in tmp[..len].iter_mut().enumerate() {
                let b = window[(next_char + code_off + i) % LZSS_WINDOW_SIZE];
                out.push(b);
                *slot = b;
            }
            for &b in &tmp[..len] {
                window[next_char] = b;
                next_char = (next_char + 1) % LZSS_WINDOW_SIZE;
            }
        }
    }
    Ok(out)
}

/// Compresses `input` with VAG/Bosch sliding-window LZSS.
///
/// When `exact_pad` is `true`, emits VW `0x00,0x00` no-op pairs and `0xFF` padding blocks
/// so AES-128-CBC 16-byte alignment decodes to exact length without trailing zero bytes.
pub fn lzss_compress(input: &[u8], skip_pad: bool, exact_pad: bool) -> Vec<u8> {
    let mut window = [b' '; LZSS_WINDOW_SIZE];
    let mut out = Vec::with_capacity(input.len() / 2 + 32);
    let mut next_char = 0usize;
    let mut in_pos = 0usize;

    let mut group_flags = 0u8;
    let mut flag_bit = 0x80u8;
    let mut group_buf: Vec<u8> = Vec::with_capacity(16);

    let flush_group = |out: &mut Vec<u8>, gflags: &mut u8, fbit: &mut u8, gbuf: &mut Vec<u8>| {
        if !gbuf.is_empty() {
            out.push(*gflags);
            out.extend_from_slice(gbuf);
            *gflags = 0;
            *fbit = 0x80;
            gbuf.clear();
        }
    };

    while in_pos < input.len() {
        let max_len = (input.len() - in_pos).min(LZSS_MAX_CODED);
        let mut best_len = 0usize;
        let mut best_back = 0usize;
        if max_len > LZSS_MAX_UNCODED {
            // Search window backwards distances 1..=LZSS_WINDOW_SIZE - 1
            for dist in 1..LZSS_WINDOW_SIZE {
                let start = (next_char + LZSS_WINDOW_SIZE - dist) % LZSS_WINDOW_SIZE;
                let mut mlen = 0usize;
                while mlen < max_len && mlen < dist {
                    if window[(start + mlen) % LZSS_WINDOW_SIZE] != input[in_pos + mlen] {
                        break;
                    }
                    mlen += 1;
                }
                if mlen > best_len && mlen > LZSS_MAX_UNCODED {
                    best_len = mlen;
                    best_back = dist;
                    if best_len == max_len {
                        break;
                    }
                }
            }
        }

        if best_len <= LZSS_MAX_UNCODED {
            let b = input[in_pos];
            group_buf.push(b);
            window[next_char] = b;
            next_char = (next_char + 1) % LZSS_WINDOW_SIZE;
            in_pos += 1;
        } else {
            group_flags |= flag_bit;
            let b0 = ((best_len as u8) << 2) | (((best_back >> 8) as u8) & 0x03);
            let b1 = (best_back & 0xFF) as u8;
            group_buf.push(b0);
            group_buf.push(b1);
            for k in 0..best_len {
                window[next_char] = input[in_pos + k];
                next_char = (next_char + 1) % LZSS_WINDOW_SIZE;
            }
            in_pos += best_len;
        }

        flag_bit >>= 1;
        if flag_bit == 0 {
            flush_group(&mut out, &mut group_flags, &mut flag_bit, &mut group_buf);
        }
    }

    if !group_buf.is_empty() {
        if exact_pad {
            while !(out.len() + group_buf.len() + 1).is_multiple_of(16) && flag_bit != 0 {
                group_buf.push(0);
                group_buf.push(0);
                group_flags |= flag_bit;
                flag_bit >>= 1;
            }
        }
        flush_group(&mut out, &mut group_flags, &mut flag_bit, &mut group_buf);
    }

    if exact_pad {
        const PADDING_LENS: [usize; 17] = [
            0x0, 0x1, 0x12, 0x3, 0x14, 0x5, 0x16, 0x7, 0x18, 0x9, 0x1A, 0xB, 0x1C, 0xD, 0x1E, 0xF,
            0x0,
        ];
        const PADDING_BLOCK: [u8; 17] = [
            0xFF, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        let rem = 16 - (out.len() % 16);
        let pad_n = PADDING_LENS[rem];
        for i in 0..pad_n {
            out.push(PADDING_BLOCK[i % PADDING_BLOCK.len()]);
        }
    } else if !skip_pad {
        while !out.len().is_multiple_of(16) {
            out.push(0x00);
        }
    }

    out
}

/// Block specification for [`pack_sgo_container`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SgoPackBlockSpec {
    pub block_address: u32,
    pub crypt_mode: u8,
    pub erase_start: u32,
    pub erase_end: u32,
    pub prog_start: u32,
    pub prog_end: u32,
    pub compress_bcb: bool,
    pub raw_data: Vec<u8>,
}

fn push_u24_be(dst: &mut Vec<u8>, val: u32) {
    dst.push(((val >> 16) & 0xFF) as u8);
    dst.push(((val >> 8) & 0xFF) as u8);
    dst.push((val & 0xFF) as u8);
}

/// Builds a valid VAG `"SGML Object File\0"` container parseable by [`SgoContainer::parse_sgo`].
#[allow(clippy::too_many_arguments)]
pub fn pack_sgo_container(
    format_version: u32,
    part_number: &str,
    sw_version: &str,
    baud_rate: u32,
    diag_start: u32,
    diag_end: u32,
    sa2_bytecode: &[u8],
    blocks: &[SgoPackBlockSpec],
) -> Vec<u8> {
    let mut block_stream = Vec::new();
    for blk in blocks {
        let encoded_payload: Vec<u8> = if blk.compress_bcb {
            crate::bcb::bcb_compress(&blk.raw_data)
        } else {
            blk.raw_data.clone()
        };
        let xored: Vec<u8> = encoded_payload.iter().map(|&b| b ^ 0xFF).collect();

        push_u24_be(&mut block_stream, blk.block_address);
        block_stream.push(blk.crypt_mode);
        push_u24_be(&mut block_stream, blk.raw_data.len() as u32);
        block_stream.extend_from_slice(&[0u8, 0u8]);
        push_u24_be(&mut block_stream, blk.erase_start);
        push_u24_be(&mut block_stream, blk.erase_end);
        push_u24_be(&mut block_stream, blk.prog_start);
        push_u24_be(&mut block_stream, blk.prog_end);
        block_stream.extend_from_slice(&(xored.len() as u32).to_le_bytes());
        block_stream.extend_from_slice(&xored);
    }

    let blocks_start = 0x39usize;
    let end_of_blocks = blocks_start + block_stream.len();
    let idx_ident = end_of_blocks;
    let meta_start = idx_ident + 281;

    let mut out = vec![0u8; 0x39];
    out[0..17].copy_from_slice(b"SGML Object File\0");
    out[0x10..0x14].copy_from_slice(&format_version.to_le_bytes());
    out[0x19..0x1D].copy_from_slice(&(idx_ident as u32).to_le_bytes());
    out[0x29..0x2D].copy_from_slice(&(meta_start as u32).to_le_bytes());
    out[0x2D..0x31].copy_from_slice(&(end_of_blocks as u32).to_le_bytes());

    out.extend_from_slice(&block_stream);

    // 260-byte part_number (0xFF XOR) + 5-byte sw_version (0xFF XOR) + u32 baud, diag_start, diag_end, n_data
    let mut ident = vec![0xFFu8; 265];
    for (i, b) in part_number.as_bytes().iter().take(259).enumerate() {
        ident[i] = b ^ 0xFF;
    }
    for (i, b) in sw_version.as_bytes().iter().take(4).enumerate() {
        ident[260 + i] = b ^ 0xFF;
    }
    out.extend_from_slice(&ident);
    out.extend_from_slice(&baud_rate.to_le_bytes());
    out.extend_from_slice(&diag_start.to_le_bytes());
    out.extend_from_slice(&diag_end.to_le_bytes());
    out.extend_from_slice(&(blocks.len() as u32).to_le_bytes());

    out.extend_from_slice(&(sa2_bytecode.len() as u32).to_le_bytes());
    out.extend_from_slice(sa2_bytecode);

    let csum = SgoContainer::compute_sgo_whole_file_checksum(&out).unwrap_or(0);
    out[0x15..0x19].copy_from_slice(&csum.to_le_bytes());
    out
}

/// Extracts the first ODX XML entry from a ZIP archive using Central Directory sizes.
pub fn extract_first_zip_entry(zip_bytes: &[u8]) -> Result<(String, Vec<u8>), String> {
    // Try Central Directory header PK\x01\x02 first (reliable even with data-descriptor bit 3).
    if let Some(cd_pos) = zip_bytes.windows(4).position(|w| w == b"PK\x01\x02") {
        if cd_pos + 46 <= zip_bytes.len() {
            let method = u16::from_le_bytes([zip_bytes[cd_pos + 10], zip_bytes[cd_pos + 11]]);
            let crc_exp = u32::from_le_bytes([
                zip_bytes[cd_pos + 16],
                zip_bytes[cd_pos + 17],
                zip_bytes[cd_pos + 18],
                zip_bytes[cd_pos + 19],
            ]);
            let comp_sz = u32::from_le_bytes([
                zip_bytes[cd_pos + 20],
                zip_bytes[cd_pos + 21],
                zip_bytes[cd_pos + 22],
                zip_bytes[cd_pos + 23],
            ]) as usize;
            let uncomp_sz = u32::from_le_bytes([
                zip_bytes[cd_pos + 24],
                zip_bytes[cd_pos + 25],
                zip_bytes[cd_pos + 26],
                zip_bytes[cd_pos + 27],
            ]) as usize;
            let name_len = u16::from_le_bytes([zip_bytes[cd_pos + 28], zip_bytes[cd_pos + 29]]) as usize;
            let lfh_off = u32::from_le_bytes([
                zip_bytes[cd_pos + 42],
                zip_bytes[cd_pos + 43],
                zip_bytes[cd_pos + 44],
                zip_bytes[cd_pos + 45],
            ]) as usize;
            if lfh_off + 30 <= zip_bytes.len() && &zip_bytes[lfh_off..lfh_off + 4] == b"PK\x03\x04" {
                let lname = u16::from_le_bytes([zip_bytes[lfh_off + 26], zip_bytes[lfh_off + 27]]) as usize;
                let lextra = u16::from_le_bytes([zip_bytes[lfh_off + 28], zip_bytes[lfh_off + 29]]) as usize;
                let data_off = lfh_off + 30 + lname + lextra;
                if data_off + comp_sz <= zip_bytes.len() {
                    let name = String::from_utf8_lossy(
                        &zip_bytes[cd_pos + 46..(cd_pos + 46 + name_len).min(zip_bytes.len())],
                    )
                    .to_string();
                    let raw = &zip_bytes[data_off..data_off + comp_sz];
                    let plain = match method {
                        0 => raw.to_vec(),
                        8 => decompress_to_vec(raw)
                            .map_err(|e| format!("ZIP DEFLATE error: {e:?}"))?,
                        other => return Err(format!("Unsupported ZIP method {other}")),
                    };
                    if uncomp_sz != 0 && plain.len() != uncomp_sz {
                        return Err("ZIP uncompressed length mismatch".to_string());
                    }
                    if crc_exp != 0 && compute_crc32(&plain) != crc_exp {
                        return Err("ZIP CRC32 mismatch".to_string());
                    }
                    return Ok((name, plain));
                }
            }
        }
    }
    Err("Invalid or empty ZIP archive in FRF".to_string())
}

/// Decrypts a VAG `.frf` container using `key` and extracts its ODX XML payload.
pub fn frf_unpack_odx(frf_bytes: &[u8], key: &[u8]) -> Result<(String, Vec<u8>), String> {
    let zip_bytes = SgoContainer::transform_frf_stream(frf_bytes, key)?;
    if frf_plaintext_kind(&zip_bytes) != FrfPlaintext::Zip {
        return Err("FRF decryption did not yield a PKZIP header (wrong key or corrupt file)".to_string());
    }
    extract_first_zip_entry(&zip_bytes)
}

/// Packs an ODX XML buffer into a single-entry DEFLATE ZIP and encrypts it into a `.frf`.
pub fn frf_pack_odx(odx_xml: &[u8], entry_name: &str, key: &[u8]) -> Result<Vec<u8>, String> {
    let name_bytes = entry_name.as_bytes();
    let crc = compute_crc32(odx_xml);
    let comp = compress_to_vec(odx_xml, 6);

    let mut zip = Vec::with_capacity(128 + comp.len() + name_bytes.len() * 2);
    // Local File Header
    zip.extend_from_slice(b"PK\x03\x04");
    zip.extend_from_slice(&20u16.to_le_bytes()); // version needed
    zip.extend_from_slice(&0u16.to_le_bytes()); // flags
    zip.extend_from_slice(&8u16.to_le_bytes()); // method = DEFLATE
    zip.extend_from_slice(&0u32.to_le_bytes()); // time/date
    zip.extend_from_slice(&crc.to_le_bytes());
    zip.extend_from_slice(&(comp.len() as u32).to_le_bytes());
    zip.extend_from_slice(&(odx_xml.len() as u32).to_le_bytes());
    zip.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes()); // extra len
    zip.extend_from_slice(name_bytes);
    zip.extend_from_slice(&comp);

    let cd_offset = zip.len();
    // Central Directory Header
    zip.extend_from_slice(b"PK\x01\x02");
    zip.extend_from_slice(&20u16.to_le_bytes());
    zip.extend_from_slice(&20u16.to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(&8u16.to_le_bytes());
    zip.extend_from_slice(&0u32.to_le_bytes());
    zip.extend_from_slice(&crc.to_le_bytes());
    zip.extend_from_slice(&(comp.len() as u32).to_le_bytes());
    zip.extend_from_slice(&(odx_xml.len() as u32).to_le_bytes());
    zip.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(&0x20u32.to_le_bytes());
    zip.extend_from_slice(&0u32.to_le_bytes()); // relative offset of LFH = 0
    zip.extend_from_slice(name_bytes);
    let cd_size = zip.len() - cd_offset;

    // EOCD
    zip.extend_from_slice(b"PK\x05\x06");
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(&1u16.to_le_bytes());
    zip.extend_from_slice(&1u16.to_le_bytes());
    zip.extend_from_slice(&(cd_size as u32).to_le_bytes());
    zip.extend_from_slice(&(cd_offset as u32).to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());

    SgoContainer::transform_frf_stream(&zip, key)
}
