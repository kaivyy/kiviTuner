//! Pure-binary SuperH SH-2A / SH705x Denso 2D & 3D map-descriptor scanner.
//!
//! Ported from `Denso-ECU-Toolkit/Scripts/AnalyzeDensoMaps.py` (`decode_3d`, `decode_2d`)
//! plus smoothness validation via [`crate::p23_descriptors::compute_p23_descriptors`]:
//! - 3D descriptor (`16`B BE): `[size_x: u16][size_y: u16][ptr_x: u32][ptr_y: u32][ptr_z: u32]`
//! - 2D descriptor (`12`B BE): `[size_x: u16][flags: u16][ptr_x: u32][ptr_z: u32]`

use crate::p23_descriptors::compute_p23_descriptors;
use serde::{Deserialize, Serialize};

const MAX_AXIS_3D: usize = 64;
const MAX_AXIS_2D: usize = 128;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DensoDescriptorMapHit {
    pub descriptor_offset: usize,
    pub is_3d: bool,
    pub cols: usize,
    pub rows: usize,
    pub ptr_x: usize,
    pub ptr_y: Option<usize>,
    pub ptr_z: usize,
    pub element_bytes: usize,
    pub smoothness: f64,
}

#[inline]
fn u16_be(b: &[u8], off: usize) -> u16 {
    u16::from_be_bytes([b[off], b[off + 1]])
}

#[inline]
fn u32_be(b: &[u8], off: usize) -> u32 {
    u32::from_be_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}

fn is_valid_rom_ptr(rom_len: usize, p: u32) -> Option<usize> {
    if p == 0 || (p as usize) >= rom_len || p >= 0xFFFF_0000 {
        return None;
    }
    Some(p as usize)
}

fn axis_looks_monotonic_u16_be(rom: &[u8], off: usize, count: usize) -> bool {
    if count < 3 || off + count * 2 > rom.len() {
        return false;
    }
    let mut prev = u16_be(rom, off);
    let mut strict_steps = 0usize;
    for i in 1..count {
        let cur = u16_be(rom, off + i * 2);
        if cur < prev {
            return false;
        }
        if cur > prev {
            strict_steps += 1;
        }
        prev = cur;
    }
    strict_steps * 2 >= count
}

fn surface_smoothness(rom: &[u8], off: usize, rows: usize, cols: usize, elem_bytes: usize) -> Option<f64> {
    let total_bytes = rows * cols * elem_bytes;
    if off + total_bytes > rom.len() {
        return None;
    }
    if rows >= 2 && cols >= 2 {
        let mut grid = Vec::with_capacity(rows);
        for r in 0..rows {
            let mut row = Vec::with_capacity(cols);
            for c in 0..cols {
                let p = off + (r * cols + c) * elem_bytes;
                let v = match elem_bytes {
                    1 => rom[p] as f64,
                    2 => u16_be(rom, p) as f64,
                    4 => f32::from_be_bytes([rom[p], rom[p + 1], rom[p + 2], rom[p + 3]]) as f64,
                    _ => return None,
                };
                if !v.is_finite() {
                    return None;
                }
                row.push(v);
            }
            grid.push(row);
        }
        let desc = compute_p23_descriptors(&grid)?;
        let max_smooth = if rows.min(cols) <= 3 { 1.76 } else { 1.35 };
        if desc.canonical_smoothness <= max_smooth {
            return Some(desc.canonical_smoothness);
        }
        return None;
    }
    // 1D curve smoothness
    let n = rows * cols;
    let mut vals = Vec::with_capacity(n);
    for i in 0..n {
        let p = off + i * elem_bytes;
        let v = if elem_bytes == 1 { rom[p] as f64 } else { u16_be(rom, p) as f64 };
        vals.push(v);
    }
    let mean = vals.iter().sum::<f64>() / n as f64;
    let var = vals.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n as f64;
    let diff = vals.windows(2).map(|w| (w[1] - w[0]).abs()).sum::<f64>() / (n - 1) as f64;
    let ratio = diff / (var.sqrt() + 1e-5);
    (ratio <= 0.90).then_some(ratio)
}

/// Scans a SuperH Denso ROM binary for 2D & 3D descriptor tables with valid monotonic axes and smooth data.
pub fn scan_denso_sh_descriptor_maps(rom: &[u8]) -> Vec<DensoDescriptorMapHit> {
    let mut hits = Vec::new();
    let mut off = 0usize;
    let len = rom.len();

    while off + 16 <= len {
        let sx = u16_be(rom, off) as usize;
        let sy = u16_be(rom, off + 2) as usize;

        // 1. Check 3D descriptor: [sx:u16][sy:u16][ptr_x:u32][ptr_y:u32][ptr_z:u32]
        if (3..=MAX_AXIS_3D).contains(&sx) && (3..=MAX_AXIS_3D).contains(&sy) {
            if let (Some(px), Some(py), Some(pz)) = (
                is_valid_rom_ptr(len, u32_be(rom, off + 4)),
                is_valid_rom_ptr(len, u32_be(rom, off + 8)),
                is_valid_rom_ptr(len, u32_be(rom, off + 12)),
            ) {
                if pz > px
                    && pz > py
                    && axis_looks_monotonic_u16_be(rom, px, sx)
                    && axis_looks_monotonic_u16_be(rom, py, sy)
                {
                    for elem_bytes in [2usize, 1] {
                        if let Some(sm) = surface_smoothness(rom, pz, sy, sx, elem_bytes) {
                            hits.push(DensoDescriptorMapHit {
                                descriptor_offset: off,
                                is_3d: true,
                                cols: sx,
                                rows: sy,
                                ptr_x: px,
                                ptr_y: Some(py),
                                ptr_z: pz,
                                element_bytes: elem_bytes,
                                smoothness: sm,
                            });
                            break;
                        }
                    }
                }
            }
        }

        // 2. Check 2D descriptor: [sx:u16][flags:u16][ptr_x:u32][ptr_z:u32]
        if (4..=MAX_AXIS_2D).contains(&sx) && (sy <= 2 || (sy & 0xFF00) == 0) && sx * 2 <= len {
            if let (Some(px), Some(pz)) = (
                is_valid_rom_ptr(len, u32_be(rom, off + 4)),
                is_valid_rom_ptr(len, u32_be(rom, off + 8)),
            ) {
                if pz > px
                    && axis_looks_monotonic_u16_be(rom, px, sx)
                    && !hits.iter().any(|h| h.descriptor_offset == off)
                {
                    if let Some(sm) = surface_smoothness(rom, pz, 1, sx, 2) {
                        hits.push(DensoDescriptorMapHit {
                            descriptor_offset: off,
                            is_3d: false,
                            cols: sx,
                            rows: 1,
                            ptr_x: px,
                            ptr_y: None,
                            ptr_z: pz,
                            element_bytes: 2,
                            smoothness: sm,
                        });
                    }
                }
            }
        }

        off += 4;
    }

    hits
}
