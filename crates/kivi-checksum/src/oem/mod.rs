//! OEM automotive & powersports checksum engines (Bosch, Siemens/BMW, GM, VAG, Subaru/Mitsu)
//! plus a parameterized blind checksum sweep engine.
//!
//! Engines implemented in `kivi_flashing::oem_checksum_crypto` (Simos CRC32/ECM3, DQ250/DQ381,
//! SIMK43, Keihin/Shindengen bike, Denso SH705x sector) and `multi_ecu` (8 motorcycle engines)
//! are reused rather than duplicated.

pub mod bosch_edc15;
pub mod bosch_edc16;
pub mod bosch_edc17;
pub mod gm_pcm;
pub mod registry;
pub mod siemens_bmw;
pub mod subaru_hitachi_mitsu;
pub mod vag_haldex_me7;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OemChecksumBlock {
    pub label: String,
    pub start: usize,
    pub end: usize,
    pub store_offset: Option<usize>,
    pub stored: Option<u32>,
    pub computed: Option<u32>,
    pub width_bits: u8,
    pub correctable: bool,
    pub ok: bool,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OemChecksumReport {
    pub algorithm_id: String,
    pub algorithm_name: String,
    pub family: String,
    pub variant: String,
    pub blocks: Vec<OemChecksumBlock>,
    pub all_ok: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OemAlgorithmInfo {
    pub id: String,
    pub name: String,
    pub family: String,
    pub supports_repair: bool,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SweepMatch {
    pub algo: String,
    pub init: u32,
    pub final_xor: u32,
    pub region: String,
    pub exclude_tail: usize,
    pub store_location: String,
    pub store_le: bool,
    pub complement: bool,
    pub pages_matched: usize,
    pub pages_total: usize,
    pub rate_bps: u16,
}

#[inline]
pub(crate) fn u16_le(d: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([d[off], d[off + 1]])
}

#[inline]
pub(crate) fn u16_be(d: &[u8], off: usize) -> u16 {
    u16::from_be_bytes([d[off], d[off + 1]])
}

#[inline]
pub(crate) fn u32_le(d: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([d[off], d[off + 1], d[off + 2], d[off + 3]])
}

#[inline]
pub(crate) fn u32_be(d: &[u8], off: usize) -> u32 {
    u32::from_be_bytes([d[off], d[off + 1], d[off + 2], d[off + 3]])
}

#[inline]
pub(crate) fn write_u16_le(d: &mut [u8], off: usize, v: u16) {
    d[off..off + 2].copy_from_slice(&v.to_le_bytes());
}

#[inline]
pub(crate) fn write_u16_be(d: &mut [u8], off: usize, v: u16) {
    d[off..off + 2].copy_from_slice(&v.to_be_bytes());
}

#[inline]
pub(crate) fn write_u32_le(d: &mut [u8], off: usize, v: u32) {
    d[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

#[inline]
pub(crate) fn write_u32_be(d: &mut [u8], off: usize, v: u32) {
    d[off..off + 4].copy_from_slice(&v.to_be_bytes());
}

pub(crate) fn find_subslice(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() || haystack.len() < from + needle.len() {
        return None;
    }
    haystack[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| from + p)
}

pub(crate) fn find_all(haystack: &[u8], needle: &[u8]) -> Vec<usize> {
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(idx) = find_subslice(haystack, needle, pos) {
        out.push(idx);
        pos = idx + 1;
    }
    out
}
