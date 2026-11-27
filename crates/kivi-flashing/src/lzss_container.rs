//! Ported from `ecu_engine_rust::flashing::lzss_container`.
//!
//! Pure byte-level protocol logic: builders, parsers, encoders, state machines.
//! No I/O, no device access, no `unsafe`. Transport is the caller's concern.
//!
//! Verified by unit tests with hand-derived expectations. **Not** validated
//! against a physical ECU; no such claim is made.

/// Continental Simos / VAG ODX 10-bit Window / 4-bit Length LZSS Compressor & Decompressor.
pub struct SimosLzssEngine;

impl SimosLzssEngine {
    pub const WINDOW_BITS: usize = 10;
    pub const LENGTH_BITS: usize = 4;
    pub const MAX_OFFSET: usize = (1 << Self::WINDOW_BITS) - 1; // 1023 bytes lookback
    pub const MIN_MATCH: usize = 3;
    pub const MAX_MATCH: usize = Self::MIN_MATCH + ((1 << Self::LENGTH_BITS) - 1); // 18 bytes max match

    /// Compress raw flash sector bytes using 10-bit offset / 4-bit length LZSS with 8-token flag bytes.
    /// Flag bit `0` = literal byte (1 byte), Flag bit `1` = back-reference (2 bytes: `[offset_hi4 | len_lo4, offset_lo8]`).
    pub fn compress(input: &[u8]) -> Vec<u8> {
        if input.is_empty() {
            return Vec::new();
        }

        let mut output = Vec::with_capacity(input.len());
        let mut pos = 0;

        while pos < input.len() {
            let flag_index = output.len();
            output.push(0x00); // Placeholder for 8-bit control flags
            let mut flags: u8 = 0;

            for bit in 0..8 {
                if pos >= input.len() {
                    break;
                }

                // Search sliding window [pos - MAX_OFFSET .. pos] for longest match >= MIN_MATCH
                let win_start = pos.saturating_sub(Self::MAX_OFFSET);
                let max_len = (input.len() - pos).min(Self::MAX_MATCH);

                let mut best_offset = 0usize;
                let mut best_len = 0usize;

                if max_len >= Self::MIN_MATCH {
                    for cand in win_start..pos {
                        let mut mlen = 0;
                        while mlen < max_len && input[cand + mlen] == input[pos + mlen] {
                            mlen += 1;
                        }
                        if mlen >= Self::MIN_MATCH && mlen > best_len {
                            best_len = mlen;
                            best_offset = pos - cand;
                            if best_len == Self::MAX_MATCH {
                                break;
                            }
                        }
                    }
                }

                if best_len >= Self::MIN_MATCH {
                    // Encode 2-byte match token:
                    // Byte 0: [((best_offset >> 8) & 0x03) << 4 | ((best_len - MIN_MATCH) & 0x0F)]
                    // Byte 1: (best_offset & 0xFF)
                    flags |= 1 << bit;
                    let len_code = (best_len - Self::MIN_MATCH) as u8 & 0x0F;
                    let off_hi = (((best_offset >> 8) & 0x03) as u8) << 4;
                    let off_lo = (best_offset & 0xFF) as u8;
                    output.push(off_hi | len_code);
                    output.push(off_lo);
                    pos += best_len;
                } else {
                    // Literal byte (flag bit remains 0)
                    output.push(input[pos]);
                    pos += 1;
                }
            }

            output[flag_index] = flags;
        }

        output
    }

    /// Decompress LZSS stream back into exact flash block of `expected_len` bytes.
    pub fn decompress(compressed: &[u8], expected_len: usize) -> Result<Vec<u8>, &'static str> {
        if expected_len == 0 {
            return Ok(Vec::new());
        }

        let mut out = Vec::with_capacity(expected_len);
        let mut idx = 0;

        while idx < compressed.len() && out.len() < expected_len {
            let flags = compressed[idx];
            idx += 1;

            for bit in 0..8 {
                if out.len() >= expected_len || idx >= compressed.len() {
                    break;
                }

                if (flags & (1 << bit)) != 0 {
                    // 2-byte back-reference
                    if idx + 1 >= compressed.len() {
                        return Err("Truncated LZSS back-reference token");
                    }
                    let b0 = compressed[idx];
                    let b1 = compressed[idx + 1];
                    idx += 2;

                    let len = ((b0 & 0x0F) as usize) + Self::MIN_MATCH;
                    let offset = ((((b0 >> 4) & 0x03) as usize) << 8) | (b1 as usize);
                    if offset == 0 || offset > out.len() {
                        return Err("Invalid LZSS lookback offset");
                    }

                    let start = out.len() - offset;
                    for i in 0..len {
                        if out.len() >= expected_len {
                            break;
                        }
                        let byte = out[start + i];
                        out.push(byte);
                    }
                } else {
                    // Literal byte
                    out.push(compressed[idx]);
                    idx += 1;
                }
            }
        }

        if out.len() != expected_len {
            return Err("LZSS decompressed size does not match expected_len");
        }

        Ok(out)
    }
}

/// UDS `dataFormatIdentifier` (ISO 14229-1 Service `$34 RequestDownload`) modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UdsDataFormat {
    /// `0x00`: Raw uncompressed, unencrypted
    RawUncompressed = 0x00,
    /// `0x10`: LZSS compressed, unencrypted
    LzssCompressed = 0x10,
    /// `0x01`: Uncompressed, XOR/Keystream encrypted
    StreamEncrypted = 0x01,
    /// `0x11`: Standard LZSS compressed + encrypted
    CompressedAndEncrypted = 0x11,
    /// `0xAA`: Continental Simos 18 high-speed LZSS + Cipher format
    Simos18CompressedEncrypted = 0xAA,
}

/// UDS Flash Block Preparation Pipeline (Compression + Block Encryption).
pub struct UdsBlockPreparationPipeline;

impl UdsBlockPreparationPipeline {
    /// Apply reversible keystream cipher (Simos/Bosch rolling XOR-rotate block cipher).
    pub fn apply_block_cipher(data: &mut [u8], key: &[u8; 16]) {
        let mut prev: u8 = 0xA5;
        for (i, b) in data.iter_mut().enumerate() {
            let k = key[i & 0x0F].rotate_left((i & 7) as u32) ^ prev;
            *b ^= k;
            prev = prev.wrapping_add(k);
        }
    }

    /// Prepare a flash block for UDS `$34`/`$36` transmission according to `UdsDataFormat`.
    pub fn prepare_block_for_transfer(
        raw_block: &[u8],
        format: UdsDataFormat,
        cipher_key: &[u8; 16],
    ) -> Vec<u8> {
        match format {
            UdsDataFormat::RawUncompressed => raw_block.to_vec(),
            UdsDataFormat::LzssCompressed => SimosLzssEngine::compress(raw_block),
            UdsDataFormat::StreamEncrypted => {
                let mut buf = raw_block.to_vec();
                Self::apply_block_cipher(&mut buf, cipher_key);
                buf
            }
            UdsDataFormat::CompressedAndEncrypted | UdsDataFormat::Simos18CompressedEncrypted => {
                let mut compressed = SimosLzssEngine::compress(raw_block);
                Self::apply_block_cipher(&mut compressed, cipher_key);
                compressed
            }
        }
    }

    /// Reverse a prepared UDS block back into the exact raw flash binary (for verification or ODX unpacking).
    pub fn unpack_transferred_block(
        payload: &[u8],
        expected_raw_len: usize,
        format: UdsDataFormat,
        cipher_key: &[u8; 16],
    ) -> Result<Vec<u8>, &'static str> {
        match format {
            UdsDataFormat::RawUncompressed => {
                if payload.len() != expected_raw_len {
                    return Err("Raw block length mismatch");
                }
                Ok(payload.to_vec())
            }
            UdsDataFormat::LzssCompressed => SimosLzssEngine::decompress(payload, expected_raw_len),
            UdsDataFormat::StreamEncrypted => {
                if payload.len() != expected_raw_len {
                    return Err("Encrypted block length mismatch");
                }
                let mut buf = payload.to_vec();
                Self::apply_block_cipher(&mut buf, cipher_key);
                Ok(buf)
            }
            UdsDataFormat::CompressedAndEncrypted | UdsDataFormat::Simos18CompressedEncrypted => {
                let mut decrypted = payload.to_vec();
                Self::apply_block_cipher(&mut decrypted, cipher_key);
                SimosLzssEngine::decompress(&decrypted, expected_raw_len)
            }
        }
    }
}
