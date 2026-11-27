//! VBF export, FRF stream cipher, SGO container, and xorshift PRNG.
//!
//! Ported from `ecu_engine_rust::formats::mod.rs`. Pure byte-level logic.
//!
//! **Security-sensitive:** `crack_sgo_xor_key` and `transform_frf_stream`
//! implement key-recovery and stream-cipher operations on formats the project
//! owner controls. They are reproduced faithfully and are offline only.

#![allow(clippy::needless_range_loop, clippy::manual_is_multiple_of, clippy::manual_rotate)]

use thiserror::Error;

/// Errors from the SGO / VBF / FRF utilities.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ContainerError {
    /// The container header is malformed.
    #[error("Malformed container header")]
    MalformedHeader,
    /// A declared length exceeds the buffer.
    #[error("Declared length exceeds buffer")]
    Truncated,
    /// A checksum did not match.
    #[error("Checksum mismatch")]
    ChecksumMismatch,
    /// The key material was empty.
    #[error("FRF key material cannot be empty")]
    EmptyKey,
}


    pub const KNOWN_SGO_KEYS: &[&[u8]] = &[
        b"GEHEIM",
        b"CodeRobert",
        b"MILKYWAY",
        b"BiWbBuD101",
        b"Mst2Bosch",
    ];

pub struct SgoExtractedBlock {
    pub block_address: u32,
    pub crypt_mode: u8,
    pub expected_decompressed_len: u32,
    pub erase_start: u32,
    pub erase_end: u32,
    pub prog_start: u32,
    pub prog_end: u32,
    pub data: Vec<u8>,
}

pub struct SgoContainer {
    pub format_version: u32,
    pub part_number: String,
    pub sw_version: String,
    pub baud_rate: u32,
    pub diag_start: u32,
    pub diag_end: u32,
    pub sa2_bytecode: Vec<u8>,
    pub blocks: Vec<SgoExtractedBlock>,
}

/// VBF / FRF / SGO container codecs.
///
/// The functions below were associated with this type before a formatting pass
/// dropped the `impl` header. Restored here so the public API is
/// `SgoContainer::transform_frf_stream(...)` as the tests and callers expect.
impl SgoContainer {
    pub fn export_vbf_to_full_bin(
        vbf_bytes: &[u8],
        physical_base_address: u32,
        target_rom_size: usize,
        pad_byte: u8,
    ) -> Result<Vec<u8>, &'static str> {
        let (_, segments) = crate::vbf::parse_vbf_segments(vbf_bytes)
            .map_err(|_| "Invalid VBF header")?;
        let mut bin = vec![pad_byte; target_rom_size];

        for seg in segments {
            let offset = seg
                .start_address
                .checked_sub(physical_base_address)
                .ok_or("VBF segment start_address is below physical_base_address")?
                as usize;
            let end = offset.saturating_add(seg.data.len());
            if end > target_rom_size {
                return Err("VBF segment exceeds target_rom_size boundary");
            }
            bin[offset..end].copy_from_slice(&seg.data);
        }

        Ok(bin)
    }

    pub fn transform_frf_stream(data: &[u8], key_material: &[u8]) -> Result<Vec<u8>, String> {
        if key_material.is_empty() {
            return Err("FRF key material cannot be empty".to_string());
        }
        let mut output = Vec::with_capacity(data.len());
        let mut first_seed: u32 = 0;
        let mut second_seed: u32 = 1;
        let mut key_index: usize = 0;

        for &byte in data {
            let key_byte = key_material[key_index] as u32;
            first_seed = ((first_seed + key_byte) * 3) & 0xFF;
            let keystream = (first_seed ^ 0xFF ^ second_seed ^ key_byte) as u8;
            output.push(byte ^ keystream);
            second_seed = ((second_seed + 1) * first_seed) & 0xFF;
            key_index += 1;
            if key_index >= key_material.len() {
                key_index = 0;
            }
        }
        Ok(output)
    }

    pub fn crack_sgo_xor_key(blob: &[u8]) -> Option<Vec<u8>> {
        if blob.len() < 4 {
            return None;
        }
        for key_len in 2..=32 {
            if blob.len() < key_len * 4 {
                continue;
            }
            for &fill_byte in &[0xFFu8, 0x00u8, 0xAAu8] {
                let mut candidate_key = vec![0u8; key_len];
                for pos in 0..key_len {
                    let mut counts = [0u32; 256];
                    let mut idx = pos;
                    while idx < blob.len() {
                        counts[blob[idx] as usize] += 1;
                        idx += key_len;
                    }
                    let (most_common_byte, _) = counts
                        .iter()
                        .enumerate()
                        .max_by_key(|&(_, count)| *count)
                        .unwrap();
                    candidate_key[pos] = (most_common_byte as u8) ^ fill_byte;
                }
                if (blob[0] ^ candidate_key[0]) == 0x1A
                    && (blob[1] ^ candidate_key[1 % key_len]) == 0x01
                {
                    let xored: Vec<u8> = blob
                        .iter()
                        .enumerate()
                        .map(|(i, &b)| b ^ candidate_key[i % key_len])
                        .collect();
                    if crate::bcb::bcb_decompress(&xored).is_ok() {
                        return Some(candidate_key);
                    }
                }
            }
        }
        None
    }

    pub fn parse_sgo(data: &[u8]) -> Result<SgoContainer, String> {
        if data.len() < 0x39 || &data[0..16] != b"SGML Object File" {
            return Err("Invalid SGO header magic (expected 'SGML Object File')".to_string());
        }
        let format_version = u32::from_le_bytes([data[0x10], data[0x11], data[0x12], data[0x13]]);
        let idx_ident = u32::from_le_bytes([data[0x19], data[0x1A], data[0x1B], data[0x1C]]) as usize;
        let meta_start = u32::from_le_bytes([data[0x29], data[0x2A], data[0x2B], data[0x2C]]) as usize;
        let end_of_blocks = u32::from_le_bytes([data[0x2D], data[0x2E], data[0x2F], data[0x30]]) as usize;

        if idx_ident + 281 > data.len() {
            return Err("SGO idx_ident out of bounds".to_string());
        }
        let part_number: String = data[idx_ident..idx_ident + 260]
            .iter()
            .map(|&b| b ^ 0xFF)
            .take_while(|&b| b != 0)
            .map(|b| b as char)
            .collect();
        let sw_version: String = data[idx_ident + 260..idx_ident + 265]
            .iter()
            .map(|&b| b ^ 0xFF)
            .take_while(|&b| b != 0)
            .map(|b| b as char)
            .collect();
        let baud_rate = u32::from_le_bytes([
            data[idx_ident + 265],
            data[idx_ident + 266],
            data[idx_ident + 267],
            data[idx_ident + 268],
        ]);
        let diag_start = u32::from_le_bytes([
            data[idx_ident + 269],
            data[idx_ident + 270],
            data[idx_ident + 271],
            data[idx_ident + 272],
        ]);
        let diag_end = u32::from_le_bytes([
            data[idx_ident + 273],
            data[idx_ident + 274],
            data[idx_ident + 275],
            data[idx_ident + 276],
        ]);
        let n_data = u32::from_le_bytes([
            data[idx_ident + 277],
            data[idx_ident + 278],
            data[idx_ident + 279],
            data[idx_ident + 280],
        ]) as usize;

        let mut sa2_bytecode = Vec::new();
        if meta_start + 4 <= data.len() {
            let meta_len = u32::from_le_bytes([
                data[meta_start],
                data[meta_start + 1],
                data[meta_start + 2],
                data[meta_start + 3],
            ]) as usize;
            if meta_start + 4 + meta_len <= data.len() {
                sa2_bytecode.extend_from_slice(&data[meta_start + 4..meta_start + 4 + meta_len]);
            }
        }

        let read_u24_be = |slice: &[u8]| -> u32 {
            ((slice[0] as u32) << 16) | ((slice[1] as u32) << 8) | (slice[2] as u32)
        };

        let mut blocks = Vec::with_capacity(n_data);
        let mut b_idx = 0x39usize;
        for _ in 0..n_data {
            if b_idx >= end_of_blocks || b_idx + 0x19 > data.len() {
                break;
            }
            let hdr = &data[b_idx..b_idx + 0x19];
            let block_address = read_u24_be(&hdr[0..3]);
            let crypt_mode = hdr[3];
            let expected_decompressed_len = read_u24_be(&hdr[4..7]);
            let erase_start = read_u24_be(&hdr[9..12]);
            let erase_end = read_u24_be(&hdr[12..15]);
            let prog_start = read_u24_be(&hdr[15..18]);
            let prog_end = read_u24_be(&hdr[18..21]);
            let blob_len = u32::from_le_bytes([hdr[21], hdr[22], hdr[23], hdr[24]]) as usize;

            if b_idx + 0x19 + blob_len > data.len() {
                return Err("SGO block payload exceeds file bounds".to_string());
            }
            let blob = &data[b_idx + 0x19..b_idx + 0x19 + blob_len];

            let xff: Vec<u8> = blob.iter().map(|&b| b ^ 0xFF).collect();
            let decoded = if xff.starts_with(b"\x1A\x01") {
                crate::bcb::bcb_decompress(&xff).map_err(|e| e.to_string())?
            } else if let Some(key) = KNOWN_SGO_KEYS.iter().find(|k| {
                blob.len() >= 2 && (blob[0] ^ k[0]) == 0x1A && (blob[1] ^ k[1 % k.len()]) == 0x01
            }) {
                let xored: Vec<u8> = blob
                    .iter()
                    .enumerate()
                    .map(|(i, &b)| b ^ key[i % key.len()])
                    .collect();
                crate::bcb::bcb_decompress(&xored).map_err(|e| e.to_string())?
            } else if let Some(cracked_key) = Self::crack_sgo_xor_key(blob) {
                let xored: Vec<u8> = blob
                    .iter()
                    .enumerate()
                    .map(|(i, &b)| b ^ cracked_key[i % cracked_key.len()])
                    .collect();
                crate::bcb::bcb_decompress(&xored).map_err(|e| e.to_string())?
            } else {
                xff
            };

            blocks.push(SgoExtractedBlock {
                block_address,
                crypt_mode,
                expected_decompressed_len,
                erase_start,
                erase_end,
                prog_start,
                prog_end,
                data: decoded,
            });

            b_idx += 0x19 + blob_len;
        }

        Ok(SgoContainer {
            format_version,
            part_number,
            sw_version,
            baud_rate,
            diag_start,
            diag_end,
            sa2_bytecode,
            blocks,
        })
    }

    pub fn compute_sgo_whole_file_checksum(sgo_bytes: &[u8]) -> Result<u32, String> {
        if sgo_bytes.len() < 0x25 || &sgo_bytes[0..17] != b"SGML Object File\0" {
            return Err("Invalid SGO header magic (expected 'SGML Object File\\0')".to_string());
        }
        let csum_bytes = &sgo_bytes[0x15..0x19];
        let mut total_sum: u32 = sgo_bytes
            .iter()
            .fold(0u32, |acc, &b| acc.wrapping_add(b as u32));
        for &b in csum_bytes {
            total_sum = total_sum.wrapping_sub(b as u32);
        }
        Ok(total_sum.wrapping_sub(1))
    }

    pub fn verify_sgo_whole_file_checksum(sgo_bytes: &[u8]) -> Result<bool, String> {
        let computed = Self::compute_sgo_whole_file_checksum(sgo_bytes)?;
        let stored = u32::from_le_bytes([
            sgo_bytes[0x15],
            sgo_bytes[0x16],
            sgo_bytes[0x17],
            sgo_bytes[0x18],
        ]);
        Ok(computed == stored)
    }

    pub fn marsaglia_xorshift128_step(state: &mut [u32; 4]) -> u32 {
        let mut t = state[0];
        t ^= t << 11;
        state[0] = state[1];
        state[1] = state[2];
        state[2] = state[3];
        let w = state[3];
        let next_w = (w ^ (w >> 19)) ^ (t ^ (t >> 8));
        state[3] = next_w;
        next_w
    }
}

/// What an FRF plaintext turned out to be.
///
/// A VAG `.frf` is a rolling-XOR over a **ZIP archive containing one `.odx`**
/// XML document. That is the shape documented by the `bri3d/VW_Flash` and
/// `simos-suite` research, and it is what makes key validation possible: a
/// correct key yields `PK\x03\x04`, a wrong key yields noise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrfPlaintext {
    /// A ZIP archive — the key was almost certainly correct.
    Zip,
    /// Not a ZIP, so either the key is wrong or the file is not an FRF.
    Unknown,
}

/// Classify a decrypted FRF buffer.
///
/// Only the start of the buffer is examined: `PK\x03\x04` appearing mid-buffer
/// is not a container signature, it is data.
pub fn frf_plaintext_kind(data: &[u8]) -> FrfPlaintext {
    if data.len() >= 4 && data[0] == 0x50 && data[1] == 0x4B && data[2] == 0x03 && data[3] == 0x04
    {
        FrfPlaintext::Zip
    } else {
        FrfPlaintext::Unknown
    }
}
