
/// 18-word P-Box for VBF Format 0x01 16-round Feistel cipher.
pub const VBF_FORMAT01_FEISTEL_P_BOX: [u32; 18] = [
    0xD77BB6C8, 0x2F645B39, 0x66DAD4A3, 0x58A31B97, 0x0360B0E6, 0x24DEF3F8,
    0xAC6E7CA5, 0xE5A8DBDB, 0xE9970717, 0x7544A39E, 0xC3DED43D, 0x074CC07A,
    0xFEAA238F, 0xC93A8605, 0x9A4ADE88, 0x3786E0D6, 0x814390CB, 0x4D24A656,
];

/// 256-word Single S-Box for VBF Format 0x01 16-round Feistel cipher.
pub const VBF_FORMAT01_FEISTEL_S_BOX: [u32; 256] = [
    0xF8AD2826, 0xBBEBB691, 0x7B698A21, 0xB8F5600B, 0xED6063A3, 0x9D5E08A1, 0x565D4602, 0xC53A6761,
    0x75CF145C, 0xDFDC5187, 0xB511AAE7, 0x938CD592, 0x7C4902D3, 0xEF07D125, 0x1EC91E9F, 0x4EA15202,
    0xC6E74B7D, 0xD5FAE6AE, 0x3F950CBD, 0xE96F0D74, 0x258AEC8A, 0x2E173C0E, 0xCE3EF492, 0x45E50C98,
    0xDAF23EF1, 0x55EEE877, 0x1552DB3A, 0x90A7F578, 0x701B0159, 0x6511A80E, 0x056878A4, 0x04EA365C,
    0xC42B4C6F, 0x103AA3C3, 0x1E07C635, 0x9DBD3CD5, 0x9C76C71C, 0x0FF49915, 0x69483334, 0x4A466D3D,
    0x601E0D7C, 0x62A5D0ED, 0x4F918349, 0xB5C72BDA, 0x217659C1, 0x240E26EE, 0x91CFEC29, 0xBF32905D,
    0x6A149BB4, 0xF31C5C74, 0x8C34B780, 0x8A790701, 0xC255F26E, 0x54166C63, 0x08AF1010, 0x858FE975,
    0xDE596297, 0xAC2EE689, 0xB850757E, 0x5549F6E6, 0x4D41D47C, 0x321AF273, 0x791BAF51, 0x3A51F957,
    0x47185AE1, 0x3E5B2E90, 0xBE1B3FCD, 0xA38B4455, 0xD4B0093F, 0xD958EAEA, 0x2F70CE57, 0x0E972915,
    0x5146F769, 0x8643A29B, 0x91749E44, 0x140FDAEC, 0xF559D8D1, 0x7883A904, 0x715F0DC1, 0xD4A62B07,
    0x867CF1A3, 0x7AD0B936, 0xEC5173A8, 0xC592835E, 0x97B4E96D, 0x6B5BA816, 0xA6C38143, 0x046D9792,
    0x9C686454, 0xDD3F5232, 0x63D942CE, 0xAEDEA464, 0x5860D212, 0x0FA6C752, 0xA9FEE6EE, 0xF51AB7DD,
    0x8D92F80F, 0xE50D66C4, 0x5DBE9FDC, 0x8E9C734F, 0x6B5F2165, 0x1D4C1D62, 0x58095DE0, 0x06480788,
    0x7698EDBD, 0x9AEAC1FC, 0xC0E2B504, 0xA0692B59, 0x02A5E2E1, 0x3BB75716, 0xC544F3A8, 0x1693DAF7,
    0x73F020C2, 0xD2041302, 0xA279C5B3, 0x1E896634, 0x9A47FA10, 0x02465C43, 0x6D48EB48, 0x59B4190E,
    0x529C7D51, 0xE1F9AA9C, 0x0590BF82, 0xCACED9B8, 0xB0258179, 0x2FA1D526, 0x338148D4, 0x62A3839F,
    0xF2D41B69, 0x809507BD, 0x630FEDF5, 0x239C4AE4, 0x0F0F1565, 0x5D4D3FE8, 0xEB06862F, 0x8399CBA9,
    0xB1C84C6C, 0x564489D3, 0x77D71ED5, 0x3B8587D2, 0x0E74EEE3, 0x35BB1F4B, 0xBC4BD2E4, 0x027AA345,
    0x1E755E4A, 0xDBD8C259, 0x35C62B65, 0x31D1D479, 0x0BA6B925, 0xD781C8CB, 0x1D1B08AF, 0x0CDD32EC,
    0x33F39B13, 0xA0EAC7A7, 0xC206C195, 0x3E30073F, 0x90BAC09E, 0x34D6BBC0, 0x4E17DBAA, 0xB473B411,
    0x25100410, 0xFEC4C15D, 0x82D9B052, 0xFCD3045B, 0x5C4A87B8, 0x72E78A90, 0xF5E79ED3, 0xB22D4D13,
    0x54895A3D, 0x8F65E7E0, 0xA0192935, 0x5A35BF08, 0xC41B82DD, 0xBE3A0F3A, 0xC440277A, 0x097A4AE1,
    0xDC9954A5, 0xAD812AAD, 0x1EF26074, 0xA563CC67, 0x1AD304AD, 0xC809D240, 0xBEB9702C, 0x1DFC6F5C,
    0xA9C55872, 0xB73A87E8, 0x5AE93B4C, 0x4E69A8BB, 0xB58DF50B, 0x21DD3F26, 0x78D6DC47, 0xB668B6E7,
    0x5C821EF0, 0xAF7E0E58, 0xCA04200B, 0xD0B0F9C4, 0xDB19D709, 0xDBC2F2AE, 0x817A37D2, 0xA37F0829,
    0xA71AE2C6, 0x8D869576, 0x06FF06DF, 0x85FB9BF4, 0x1A2D3B81, 0xBD6D7B23, 0x91D0B7D2, 0x92ECEF5B,
    0x05981B91, 0x207AA263, 0xA1FAA6B3, 0x9819362E, 0xE3F1DF04, 0x28F642CA, 0xED87493C, 0x7FEE7235,
    0x43866C5C, 0x7EBDC861, 0xC3B12E6E, 0xE854AC2B, 0x8991D1F1, 0x2FFF036B, 0x763125AF, 0x6F99F2C2,
    0x20F1D28F, 0x3CD666E3, 0x1D50A879, 0x3967FFE8, 0xFE5E49BD, 0x5FDA107A, 0xA1DB77EA, 0x8F8F34F2,
    0xC23B791F, 0xAE02BFF0, 0xC43A1378, 0x837FEA22, 0x6F6C2DF7, 0xB1174E3F, 0x6CD95EED, 0xF468F871,
    0x14E9B852, 0x45C1D44F, 0x2715607E, 0xCEA40B23, 0xA0C3CA14, 0x519FAA58, 0x7177A444, 0x904EF881,
    0xF207D934, 0x356F9268, 0x172D2E17, 0xD1666051, 0xAB6B0A62, 0x05A345AE, 0x2F007AAC, 0xB60CF608,
];

pub struct VbfPhfAndMcuMapEngine;

impl VbfPhfAndMcuMapEngine {
    #[inline]
    pub fn single_sbox_f(x: u32) -> u32 {
        let b3 = ((x >> 24) & 0xFF) as usize;
        let b2 = ((x >> 16) & 0xFF) as usize;
        let b1 = ((x >> 8) & 0xFF) as usize;
        let b0 = (x & 0xFF) as usize;
        ((VBF_FORMAT01_FEISTEL_S_BOX[b3].wrapping_add(VBF_FORMAT01_FEISTEL_S_BOX[b2]))
            ^ VBF_FORMAT01_FEISTEL_S_BOX[b1])
            .wrapping_add(VBF_FORMAT01_FEISTEL_S_BOX[b0])
    }

    /// Decrypts VBF `data_format_identifier = 0x01` payload blocks using the
    /// 16-round single-S-box Feistel cipher.
    pub fn decrypt_vbf_format_01_blowfish_single_sbox(data: &[u8]) -> Vec<u8> {
        let mut out = data.to_vec();
        for chunk in out.chunks_exact_mut(8) {
            let mut l = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            let mut r = u32::from_be_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]);
            l ^= VBF_FORMAT01_FEISTEL_P_BOX[17];
            r ^= VBF_FORMAT01_FEISTEL_P_BOX[16];
            let mut idx = 15isize;
            while idx >= 0 {
                r ^= Self::single_sbox_f(l) ^ VBF_FORMAT01_FEISTEL_P_BOX[idx as usize];
                l ^= Self::single_sbox_f(r) ^ VBF_FORMAT01_FEISTEL_P_BOX[(idx - 1) as usize];
                idx -= 2;
            }
            chunk[0..4].copy_from_slice(&r.to_be_bytes());
            chunk[4..8].copy_from_slice(&l.to_be_bytes());
        }
        out
    }

    /// Round-trip encryption of VBF `data_format_identifier = 0x01`.
    pub fn encrypt_vbf_format_01_blowfish_single_sbox(data: &[u8]) -> Vec<u8> {
        let mut out = data.to_vec();
        for chunk in out.chunks_exact_mut(8) {
            let mut r = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            let mut l = u32::from_be_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]);
            let mut idx = 0usize;
            while idx <= 14 {
                l ^= Self::single_sbox_f(r) ^ VBF_FORMAT01_FEISTEL_P_BOX[idx];
                r ^= Self::single_sbox_f(l) ^ VBF_FORMAT01_FEISTEL_P_BOX[idx + 1];
                idx += 2;
            }
            let out_l = l ^ VBF_FORMAT01_FEISTEL_P_BOX[17];
            let out_r = r ^ VBF_FORMAT01_FEISTEL_P_BOX[16];
            chunk[0..4].copy_from_slice(&out_l.to_be_bytes());
            chunk[4..8].copy_from_slice(&out_r.to_be_bytes());
        }
        out
    }

    /// Normalizes Ford/Mazda/JLR Calibration IDs (`"ABCD-14C204-AB"` or `"ABCD-12A650-AB"`)
    /// into the canonical short lookup key `"ABCDAB"`.
    pub fn normalize_ford_cal_id(cal_id: &str) -> Option<String> {
        let b = cal_id.trim().as_bytes();
        if b.len() >= 13 && (b[4] == b'-' || b[4] == b'_') && (b[11] == b'-' || b[11] == b'_') {
            let mut out = String::with_capacity(8);
            out.push_str(std::str::from_utf8(&b[0..4]).ok()?);
            let suffix_end = b.len().min(16);
            out.push_str(std::str::from_utf8(&b[12..suffix_end]).ok()?);
            Some(out)
        } else if b.len() >= 12 && (b[4] == b'-' || b[4] == b'_') && (b[10] == b'-' || b[10] == b'_') {
            let mut out = String::with_capacity(8);
            out.push_str(std::str::from_utf8(&b[0..4]).ok()?);
            let suffix_end = b.len().min(15);
            out.push_str(std::str::from_utf8(&b[11..suffix_end]).ok()?);
            Some(out)
        } else {
            None
        }
    }

    /// Parses Ford `.phf` binary container records after the `$` header delimiter.
    /// Each record starts with `:` (`0x3A`) followed by raw binary `[len, addr_hi, addr_lo, type, data..., csum]`,
    /// where the 8-bit wrapping sum of `rec[1 .. len + 5]` must equal `0x00`.
    pub fn parse_ford_phf_binary_records(phf: &[u8]) -> Result<Vec<(u32, Vec<u8>)>, String> {
        let dollar_pos = phf
            .iter()
            .position(|&b| b == b'$')
            .ok_or_else(|| "Missing '$' header terminator in Ford .phf container".to_string())?;
        let mut pos = dollar_pos + 1;
        while pos < phf.len() && phf[pos] != b':' {
            pos += 1;
        }
        if pos >= phf.len() {
            return Err("No ':' binary record marker found in Ford .phf container".to_string());
        }

        let mut upper_addr = 0u32;
        let mut seg_addr = 0u32;
        let mut segments: Vec<(u32, Vec<u8>)> = Vec::new();

        while pos < phf.len() {
            if phf[pos] != b':' {
                break;
            }
            if pos + 5 >= phf.len() {
                return Err("Truncated binary record header in Ford .phf container".to_string());
            }
            let rec_len = phf[pos + 1] as usize;
            let total_rec_bytes = rec_len + 5; // len + addr_hi + addr_lo + type + data[rec_len] + csum
            if pos + 1 + total_rec_bytes > phf.len() {
                return Err("Truncated binary record payload in Ford .phf container".to_string());
            }
            let rec_slice = &phf[pos + 1..pos + 1 + total_rec_bytes];
            let sum8 = rec_slice.iter().fold(0u8, |a, &b| a.wrapping_add(b));
            if sum8 != 0 {
                return Err(format!(
                    "Ford .phf record checksum mismatch at offset 0x{:X} (sum=0x{:02X})",
                    pos, sum8
                ));
            }
            let addr16 = ((rec_slice[1] as u32) << 8) | (rec_slice[2] as u32);
            let rec_type = rec_slice[3];
            let data = &rec_slice[4..4 + rec_len];

            match rec_type {
                0x00 => {
                    let phys_addr = upper_addr.wrapping_add(seg_addr).wrapping_add(addr16);
                    if let Some(last) = segments.last_mut() {
                        if u32::try_from(last.1.len())
                            .ok()
                            .and_then(|l| last.0.checked_add(l))
                            == Some(phys_addr)
                        {
                            last.1.extend_from_slice(data);
                        } else {
                            segments.push((phys_addr, data.to_vec()));
                        }
                    } else {
                        segments.push((phys_addr, data.to_vec()));
                    }
                }
                0x01 => {
                    break;
                }
                0x02 => {
                    if rec_len != 2 {
                        return Err("Invalid length for .phf Extended Segment Address record (0x02)".to_string());
                    }
                    seg_addr = (((data[0] as u32) << 8) | (data[1] as u32)) << 4;
                }
                0x04 => {
                    if rec_len != 2 {
                        return Err("Invalid length for .phf Extended Linear Address record (0x04)".to_string());
                    }
                    upper_addr = ((data[0] as u32) << 24) | ((data[1] as u32) << 16);
                }
                other => {
                    return Err(format!("Unsupported Ford .phf record type 0x{:02X}", other));
                }
            }

            pos += 1 + total_rec_bytes;
        }

        Ok(segments)
    }

    /// Serializes physical memory segments into a valid Ford `.phf` binary record container.
    pub fn serialize_ford_phf_binary_records(header: &str, segments: &[(u32, Vec<u8>)]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(header.as_bytes());
        out.push(b'$');
        let mut cur_upper: Option<u16> = None;

        for (base_addr, payload) in segments {
            let mut off = 0usize;
            while off < payload.len() {
                let addr = base_addr.wrapping_add(off as u32);
                let upper = (addr >> 16) as u16;
                if cur_upper != Some(upper) {
                    cur_upper = Some(upper);
                    let rec = [0x02u8, 0x00, 0x00, 0x04, (upper >> 8) as u8, upper as u8];
                    let csum = (!rec.iter().fold(0u8, |a, &b| a.wrapping_add(b))).wrapping_add(1);
                    out.push(b':');
                    out.extend_from_slice(&rec);
                    out.push(csum);
                }
                let addr_lo = (addr & 0xFFFF) as u16;
                let max_before_64k_wrap = 0x1_0000usize - (addr_lo as usize);
                let chunk_len = (payload.len() - off).min(128).min(max_before_64k_wrap);
                out.push(b':');
                let mut sum = 0u8;
                let hdr = [
                    chunk_len as u8,
                    (addr_lo >> 8) as u8,
                    addr_lo as u8,
                    0x00u8,
                ];
                for &b in &hdr {
                    sum = sum.wrapping_add(b);
                    out.push(b);
                }
                for &b in &payload[off..off + chunk_len] {
                    sum = sum.wrapping_add(b);
                    out.push(b);
                }
                out.push((!sum).wrapping_add(1));
                off += chunk_len;
            }
        }
        // EOF record (type 0x01)
        out.extend_from_slice(&[b':', 0x00, 0x00, 0x00, 0x01, 0xFF]);
        out
    }

    /// Translates TriCore / PowerPC physical flash addresses (`0x80000000`, `0xA0000000`, `0x00400000`, `0x00800000`)
    /// to contiguous `.bin` file offsets using the 31-group mapping delta rules.
    pub fn translate_tricore_phys_to_bin_offset(phys_addr: u32, max_rom_size: u32) -> Option<u32> {
        let delta = if (0x8000_0000..0x8080_0000).contains(&phys_addr) {
            0x8000_0000u32
        } else if (0xA000_0000..0xA080_0000).contains(&phys_addr) {
            0xA000_0000u32
        } else if (0x0080_0000..0x00C0_0000).contains(&phys_addr) {
            0x0080_0000u32
        } else if (0x0040_0000..0x0080_0000).contains(&phys_addr) {
            0x0040_0000u32
        } else {
            0u32
        };
        let bin_off = phys_addr.wrapping_sub(delta);
        if bin_off < max_rom_size {
            Some(bin_off)
        } else {
            None
        }
    }

    /// Rounds output `.bin` size up to 32 KB (`0x8000`) if `<= 3 MB` (`0x300000`),
    /// or up to 1 MB (`0x100000`) if `> 3 MB`.
    pub fn round_rom_output_size(raw_end_offset: u32) -> u32 {
        if raw_end_offset <= 0x0030_0000 {
            if (raw_end_offset & 0x7FFF) != 0 {
                raw_end_offset
                    .checked_add(0x8000)
                    .map(|v| v & 0xFFFF_8000)
                    .unwrap_or(u32::MAX)
            } else {
                raw_end_offset
            }
        } else if (raw_end_offset & 0x000F_FFFF) != 0 {
            raw_end_offset
                .checked_add(0x0010_0000)
                .map(|v| v & 0xFFF0_0000)
                .unwrap_or(0xFFF0_0000)
        } else {
            raw_end_offset
        }
    }
}

// ============================================================================
// 12. DENSO SH7055 / SH7058 / SH7059 FULL FLASH + EEPROM 93C56/93C66/93C86
