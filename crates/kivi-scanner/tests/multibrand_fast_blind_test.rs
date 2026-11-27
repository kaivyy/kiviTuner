use kivi_scanner::blind_automap::blind_v2_candidates_to_tables;
use kivi_scanner::blind_v2::{blind_discover_v2, BlindV2Options};
use std::time::Instant;

fn fill_cal_texture(buf: &mut [u8]) {
    let pat = [0x55u8, 0xAA, 0x11];
    for (i, b) in buf.iter_mut().enumerate() {
        *b = pat[i % 3];
    }
}

fn write_be16(buf: &mut [u8], off: usize, v: u16) {
    buf[off..off + 2].copy_from_slice(&v.to_be_bytes());
}

fn write_le16(buf: &mut [u8], off: usize, v: u16) {
    buf[off..off + 2].copy_from_slice(&v.to_le_bytes());
}

fn write_be32(buf: &mut [u8], off: usize, v: u32) {
    buf[off..off + 4].copy_from_slice(&v.to_be_bytes());
}

fn write_be_f32(buf: &mut [u8], off: usize, v: f32) {
    buf[off..off + 4].copy_from_slice(&v.to_be_bytes());
}

#[test]
fn signed_i16_cross_zero_surface_is_discovered_and_decoded() {
    let mut rom = vec![0xFFu8; 65_536];
    fill_cal_texture(&mut rom[0x4000..0x9000]);

    // Plant an 8x8 signed i16_le ignition/trim table crossing zero (-350 .. +350)
    let addr = 0x5000usize;
    let rows = 8usize;
    let cols = 8usize;
    for r in 0..rows {
        for c in 0..cols {
            let val = -350i32 + (r as i32 * 55) + (c as i32 * 45);
            write_le16(&mut rom, addr + (r * cols + c) * 2, (val as i16) as u16);
        }
    }

    let cands = blind_discover_v2(&rom, &BlindV2Options::default());
    let summary: Vec<_> = cands
        .iter()
        .map(|c| (format!("0x{:X}", c.address), c.rows, c.cols, &c.dtype_label, &c.detection_source))
        .collect();
    let hit = cands
        .iter()
        .find(|c| c.address.abs_diff(addr) <= 4 && c.dtype_label == "i16_le")
        .unwrap_or_else(|| panic!("should discover signed zero-crossing i16_le table; got={summary:?}"));

    assert_eq!(hit.rows, rows);
    assert_eq!(hit.cols, cols);

    let tables = blind_v2_candidates_to_tables(&rom, std::slice::from_ref(hit), None);
    assert_eq!(tables.len(), 1);
    assert!(
        tables[0].raw_min < 0.0 && tables[0].raw_max > 0.0,
        "decoded matrix must preserve negative & positive values: min={} max={}",
        tables[0].raw_min,
        tables[0].raw_max
    );
}

#[test]
fn bosch_siemens_length_prefixed_struct_is_discovered() {
    let mut rom = vec![0xFFu8; 65_536];
    fill_cal_texture(&mut rom[0x3000..0x7000]);

    // Plant [cols=9][rows=7][X: 9 u16_le][Y: 7 u16_le][Z: 7x9 u16_le]
    let base = 0x4200usize;
    let cols = 9usize;
    let rows = 7usize;
    write_le16(&mut rom, base, cols as u16);
    write_le16(&mut rom, base + 2, rows as u16);
    for c in 0..cols {
        write_le16(&mut rom, base + 4 + c * 2, (800 + c * 650) as u16);
    }
    let y_off = base + 4 + cols * 2;
    for r in 0..rows {
        write_le16(&mut rom, y_off + r * 2, (120 + r * 140) as u16);
    }
    let z_off = y_off + rows * 2;
    for r in 0..rows {
        for c in 0..cols {
            let v = (400 + r * 220 + c * 150) as u16;
            write_le16(&mut rom, z_off + (r * cols + c) * 2, v);
        }
    }

    let cands = blind_discover_v2(&rom, &BlindV2Options::default());
    let summary: Vec<_> = cands
        .iter()
        .map(|c| (format!("0x{:X}", c.address), c.rows, c.cols, &c.dtype_label, &c.detection_source))
        .collect();
    let hit = cands
        .iter()
        .find(|c| c.address == z_off && c.rows == rows && c.cols == cols)
        .unwrap_or_else(|| panic!("must find Bosch/Siemens length-prefixed struct at exact 0x{z_off:X}; got={summary:?}"));
    assert!(
        hit.detection_source.starts_with("bosch_siemens_struct"),
        "unexpected source: {}",
        hit.detection_source
    );
    assert!(hit.x_axis.is_some() && hit.y_axis.is_some());
}

#[test]
fn denso_sh_f32_pointer_descriptor_and_f32_curve_are_discovered() {
    let mut rom = vec![0xFFu8; 131_072];
    fill_cal_texture(&mut rom[0x8000..0x18000]);

    // Denso SH 3D descriptor: [cols=7][rows=5][px][py][pz]
    let desc = 0x8400usize;
    let px = 0x9000usize;
    let py = 0x9200usize;
    let pz = 0xA000usize;
    write_be16(&mut rom, desc, 7);
    write_be16(&mut rom, desc + 2, 5);
    write_be32(&mut rom, desc + 4, px as u32);
    write_be32(&mut rom, desc + 8, py as u32);
    write_be32(&mut rom, desc + 12, pz as u32);

    for c in 0..7 {
        write_be_f32(&mut rom, px + c * 4, 800.0 + c as f32 * 850.0);
    }
    for r in 0..5 {
        write_be_f32(&mut rom, py + r * 4, 10.0 + r as f32 * 20.0);
    }
    for r in 0..5 {
        for c in 0..7 {
            write_be_f32(&mut rom, pz + (r * 7 + c) * 4, 12.5 + r as f32 * 1.4 + c as f32 * 0.65);
        }
    }

    // Smooth 16-point f32_be 1D curve at 0xC000
    let curve_off = 0xC000usize;
    for i in 0..16 {
        let x = i as f32 / 15.0;
        let v = 14.7 - 2.6 * (x * x);
        write_be_f32(&mut rom, curve_off + i * 4, v);
    }

    let opts = BlindV2Options {
        include_curves: true,
        ..BlindV2Options::default()
    };
    let cands = blind_discover_v2(&rom, &opts);

    let map_hit = cands
        .iter()
        .find(|c| c.address == pz && !c.is_curve && c.rows == 5 && c.cols == 7)
        .unwrap_or_else(|| {
            let summary: Vec<_> = cands
                .iter()
                .map(|c| (format!("0x{:X}", c.address), c.rows, c.cols, &c.dtype_label, &c.detection_source))
                .collect();
            panic!("must discover Denso SH f32_be 3D descriptor map at 0x{pz:X}; got={summary:?}")
        });
    assert_eq!(map_hit.dtype_label, "f32_be");

    let curve_hit = cands
        .iter()
        .find(|c| c.is_curve && c.dtype_label == "f32_be" && c.address.abs_diff(curve_off) <= 4)
        .expect("must discover f32_be 1-D curve");
    assert!(curve_hit.cols >= 10);
}

#[test]
fn fast_two_megabyte_rom_scan_completes_under_budget() {
    let mut rom = vec![0xFFu8; 2 * 1024 * 1024];
    fill_cal_texture(&mut rom[0x40000..0xC0000]);

    // Embed a real non-monotone 16x16 table
    let addr = 0x60000usize;
    for r in 0..16usize {
        for c in 0..16usize {
            let dc = (c as isize - 7).unsigned_abs();
            write_le16(&mut rom, addr + (r * 16 + c) * 2, (1600 + r * 120 - dc * 90) as u16);
        }
    }

    let t0 = Instant::now();
    let cands = blind_discover_v2(&rom, &BlindV2Options::default());
    let dt = t0.elapsed();
    assert!(
        cands.iter().any(|c| c.address.abs_diff(addr) <= 16),
        "must detect embedded 16x16 map in 2 MiB ROM; total cands={}",
        cands.len()
    );
    let max_secs = if cfg!(debug_assertions) { 10.0 } else { 2.5 };
    assert!(
        dt.as_secs_f64() < max_secs,
        "2 MiB blind scan exceeded budget ({:.2}s >= {:.2}s)",
        dt.as_secs_f64(),
        max_secs
    );
}

#[test]
fn bosch_u8hdr_2d_struct_full_range_and_signed_axes_beat_distractor() {
    let mut rom = vec![0xFFu8; 65_536];
    fill_cal_texture(&mut rom[0x2000..0x8000]);

    // Plant [d1=6:u8][d2=8:u8][ax1: 6 u16_le > 20,000][ax2: 8 i16_le crossing zero][Z: 8x6 u16_le]
    let hdr = 0x3400usize;
    let d1 = 6usize;
    let d2 = 8usize;
    rom[hdr] = d1 as u8;
    rom[hdr + 1] = d2 as u8;
    for i in 0..d1 {
        write_le16(&mut rom, hdr + 2 + i * 2, (22_000 + i * 1800) as u16);
    }
    let ax2_off = hdr + 2 + d1 * 2;
    for i in 0..d2 {
        let sv = -300i16 + (i as i16) * 100;
        write_le16(&mut rom, ax2_off + i * 2, sv as u16);
    }
    let z_off = ax2_off + d2 * 2;
    // Plant smooth distractor prefix right before Z to test Tier-3 precedence against larger windows
    for r in 0..d2 {
        for c in 0..d1 {
            let v = (1500 + r * 140 + c * 95) as u16;
            write_le16(&mut rom, z_off + (r * d1 + c) * 2, v);
        }
    }

    let cands = blind_discover_v2(&rom, &BlindV2Options::default());
    let hit = cands
        .iter()
        .find(|c| c.address == z_off && c.rows == d2 && c.cols == d1)
        .unwrap_or_else(|| {
            panic!(
                "must detect [d1:u8][d2:u8] 2D struct at exact 0x{z_off:X}; got {:?}",
                cands.iter().map(|c| (c.address, c.rows, c.cols, &c.detection_source)).collect::<Vec<_>>()
            )
        });
    assert!(
        hit.detection_source.starts_with("bosch_siemens_struct"),
        "expected Tier-3 struct source, got {}",
        hit.detection_source
    );
    assert!(hit.x_axis.is_some() && hit.y_axis.is_some());
}

#[test]
fn inline_1d_structs_and_axis_anchored_1d_are_discovered_exact() {
    let mut rom = vec![0xFFu8; 65_536];
    fill_cal_texture(&mut rom[0x2000..0xB000]);

    // 1. Inline 1D struct: [len=6:u16_le][ax: 6 u16_le][Z: 6-pt monotonic u16_le]
    let p1 = 0x3000usize;
    write_le16(&mut rom, p1, 6);
    for i in 0..6usize {
        write_le16(&mut rom, p1 + 2 + i * 2, (1000 + i * 700) as u16);
    }
    let z1 = p1 + 2 + 12;
    for i in 0..6usize {
        write_le16(&mut rom, z1 + i * 2, (400 + i * 120) as u16);
    }

    // 2. Inline 1D struct: [len=12:u16_le][ax: 12 u16_le][Z: 12-pt signed zero-crossing i16_le]
    let p2 = 0x4200usize;
    write_le16(&mut rom, p2, 12);
    for i in 0..12usize {
        write_le16(&mut rom, p2 + 2 + i * 2, (500 + i * 400) as u16);
    }
    let z2 = p2 + 2 + 24;
    for i in 0..12usize {
        let sv = -500i16 + (i as i16) * 90;
        write_le16(&mut rom, z2 + i * 2, sv as u16);
    }

    // 3. Contiguous axis-anchored 1D [ax: 10 u16_le][Z: 10-pt smooth u16_le bell curve]
    let p3 = 0x5400usize;
    for i in 0..10usize {
        write_le16(&mut rom, p3 + i * 2, (800 + i * 600) as u16);
    }
    let z3 = p3 + 20;
    let curve10: [u16; 10] = [1000, 1180, 1400, 1650, 1900, 2100, 2220, 2240, 2160, 2000];
    for (i, &val) in curve10.iter().enumerate() {
        write_le16(&mut rom, z3 + i * 2, val);
    }

    let opts = BlindV2Options {
        include_curves: true,
        ..BlindV2Options::default()
    };
    let cands = blind_discover_v2(&rom, &opts);
    assert!(
        cands.iter().any(|c| c.is_curve && c.address == z1 && c.cols == 6),
        "must discover 6-pt inline 1D struct at exact 0x{z1:X}; got {:?}",
        cands.iter().filter(|c| c.is_curve).map(|c| (c.address, c.cols, &c.dtype_label, &c.detection_source)).collect::<Vec<_>>()
    );
    assert!(
        cands.iter().any(|c| c.is_curve && c.address == z2 && c.cols == 12 && c.dtype_label == "i16_le"),
        "must discover 12-pt signed i16 1D struct at exact 0x{z2:X}"
    );
    assert!(
        cands.iter().any(|c| c.is_curve && c.address == z3 && c.cols == 10 && c.detection_source.starts_with("axis_anchored_1d")),
        "must discover contiguous axis-anchored 1D curve at exact 0x{z3:X}; got {:?}",
        cands.iter().filter(|c| c.is_curve).map(|c| (c.address, c.cols, &c.dtype_label, &c.detection_source)).collect::<Vec<_>>()
    );
}

#[test]
fn constant_stride_pointer_array_discovers_nonstandard_35x25_table() {
    let mut rom = vec![0xFFu8; 128 * 1024];
    fill_cal_texture(&mut rom[0x2000..0x18000]);

    let stride = 35usize * 25usize * 2usize; // 1750 bytes (non-standard shape)
    let ptr_arr = 0x2400usize;
    let base_tbl = 0x6000usize;
    for t in 0..4usize {
        let addr = base_tbl + t * stride;
        write_be32(&mut rom, ptr_arr + t * 4, addr as u32);
        for r in 0..35usize {
            for c in 0..25usize {
                let v = (900 + r * 45 + c * 25 + t * 120) as u16;
                write_be16(&mut rom, addr + (r * 25 + c) * 2, v);
            }
        }
    }

    let cands = blind_discover_v2(&rom, &BlindV2Options::default());
    for t in 0..4usize {
        let addr = base_tbl + t * stride;
        let hit = cands
            .iter()
            .find(|c| c.address == addr && c.rows == 35 && c.cols == 25)
            .unwrap_or_else(|| {
                panic!(
                    "must discover 35x25 pointer-stride table #{t} at 0x{addr:X}; got {:?}",
                    cands.iter().map(|c| (c.address, c.rows, c.cols, &c.detection_source)).collect::<Vec<_>>()
                )
            });
        assert!(hit.detection_source.starts_with("pointer_stride_2d"));
    }
}

fn write_le32(buf: &mut [u8], off: usize, v: u32) {
    buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

#[test]
fn denso_rh850_le_2d_and_1d_descriptors_are_discovered() {
    let mut rom = vec![0xFFu8; 131_072];
    fill_cal_texture(&mut rom[0x4000..0x18000]);

    // 1. RH850 0x2A 2-D u16_le descriptor (10 rows x 14 cols)
    let px_2d = 0x5000usize;
    let py_2d = 0x5100usize;
    let pz_2d = 0x6000usize;
    for c in 0..14usize {
        write_le16(&mut rom, px_2d + c * 2, (800 + c * 650) as u16);
    }
    for r in 0..10usize {
        write_le16(&mut rom, py_2d + r * 2, (120 + r * 140) as u16);
    }
    for r in 0..10usize {
        for c in 0..14usize {
            write_le16(&mut rom, pz_2d + (r * 14 + c) * 2, (900 + r * 80 + c * 55) as u16);
        }
    }
    let desc_2a = 0x4200usize;
    rom[desc_2a] = 0x2A;
    rom[desc_2a + 1] = 14;
    rom[desc_2a + 2] = 10;
    rom[desc_2a + 3] = 0;
    write_le32(&mut rom, desc_2a + 4, px_2d as u32);
    write_le32(&mut rom, desc_2a + 8, py_2d as u32);
    write_le32(&mut rom, desc_2a + 12, pz_2d as u32);
    write_le32(&mut rom, desc_2a + 16, 0);

    // 2. RH850 0x29 2-D u8 descriptor (7 rows x 12 cols)
    let px_29 = 0x5200usize;
    let py_29 = 0x5300usize;
    let pz_29 = 0x7000usize;
    for c in 0..12usize {
        write_le16(&mut rom, px_29 + c * 2, (1000 + c * 500) as u16);
    }
    for r in 0..7usize {
        write_le16(&mut rom, py_29 + r * 2, (50 + r * 35) as u16);
    }
    for r in 0..7usize {
        for c in 0..12usize {
            rom[pz_29 + r * 12 + c] = (40 + r * 8 + c * 6) as u8;
        }
    }
    let desc_29 = 0x4240usize;
    rom[desc_29] = 0x29;
    rom[desc_29 + 1] = 12;
    rom[desc_29 + 2] = 7;
    rom[desc_29 + 3] = 0;
    write_le32(&mut rom, desc_29 + 4, px_29 as u32);
    write_le32(&mut rom, desc_29 + 8, py_29 as u32);
    write_le32(&mut rom, desc_29 + 12, pz_29 as u32);
    write_le32(&mut rom, desc_29 + 16, 0);

    // 3. RH850 0x0A 1-D u16_le descriptor (9 pts)
    let px_1d = 0x5400usize;
    let pz_1d = 0x8000usize;
    for i in 0..9usize {
        write_le16(&mut rom, px_1d + i * 2, (600 + i * 900) as u16);
        write_le16(&mut rom, pz_1d + i * 2, (1200 + i * 220) as u16);
    }
    let desc_0a = 0x4280usize;
    rom[desc_0a] = 0x0A;
    rom[desc_0a + 1] = 9;
    rom[desc_0a + 2] = 0;
    rom[desc_0a + 3] = 0;
    write_le32(&mut rom, desc_0a + 4, px_1d as u32);
    write_le32(&mut rom, desc_0a + 8, pz_1d as u32);
    write_le32(&mut rom, desc_0a + 12, 0);

    // 4. RH850 0x06 1-D u16_le descriptor with 1-byte gear axis 1..7 (7 pts)
    let px_gear = 0x5500usize;
    let pz_gear = 0x8400usize;
    for i in 0..7usize {
        rom[px_gear + i] = (i + 1) as u8;
        write_le16(&mut rom, pz_gear + i * 2, (11000 + i * 150) as u16);
    }
    let desc_06 = 0x42C0usize;
    rom[desc_06] = 0x06;
    rom[desc_06 + 1] = 7;
    rom[desc_06 + 2] = 0;
    rom[desc_06 + 3] = 0;
    write_le32(&mut rom, desc_06 + 4, px_gear as u32);
    write_le32(&mut rom, desc_06 + 8, pz_gear as u32);
    write_le32(&mut rom, desc_06 + 12, 0);

    let opts = BlindV2Options {
        include_curves: true,
        ..BlindV2Options::default()
    };
    let cands = blind_discover_v2(&rom, &opts);

    let h2a = cands
        .iter()
        .find(|c| !c.is_curve && c.address == pz_2d)
        .expect("must discover RH850 0x2A 2-D map");
    assert_eq!((h2a.rows, h2a.cols), (10, 14));
    assert_eq!(h2a.dtype_label, "u16_le");
    assert!(h2a.x_axis.is_some() && h2a.y_axis.is_some());
    assert!(h2a.detection_source.starts_with("denso_rh850_descriptor"));

    let h29 = cands
        .iter()
        .find(|c| !c.is_curve && c.address == pz_29)
        .expect("must discover RH850 0x29 2-D u8 map");
    assert_eq!((h29.rows, h29.cols), (7, 12));
    assert_eq!(h29.dtype_label, "u8");
    assert!(h29.x_axis.is_some() && h29.y_axis.is_some());
    assert!(h29.detection_source.starts_with("denso_rh850_descriptor"));

    let h0a = cands
        .iter()
        .find(|c| c.is_curve && c.address == pz_1d)
        .expect("must discover RH850 0x0A 1-D u16_le curve");
    assert_eq!(h0a.cols, 9);
    assert_eq!(h0a.dtype_label, "u16_le");
    assert!(h0a.x_axis.is_some());
    assert!(h0a.detection_source.starts_with("denso_rh850_curve_descriptor"));

    let h06 = cands
        .iter()
        .find(|c| c.is_curve && c.address == pz_gear)
        .expect("must discover RH850 0x06 1-D u16_le gear curve");
    assert_eq!(h06.cols, 7);
    assert_eq!(h06.dtype_label, "u16_le");
    assert!(h06.x_axis.is_some());
    assert!(h06.detection_source.starts_with("denso_rh850_curve_descriptor"));
}

#[test]
fn denso_sh_4mb_i8_odd_aligned_descriptor_is_discovered() {
    let mut rom = vec![0xFFu8; 4 * 1024 * 1024];
    fill_cal_texture(&mut rom[0x15_0000..0x24_0000]);

    let desc = 0x15_5AF4usize;
    let px = 0x18_0000usize;
    let py = 0x18_0100usize;
    let pz = 0x1F_7CA5usize; // Odd-aligned i8 matrix in 4 MiB ROM

    write_be16(&mut rom, desc, 7);
    write_be16(&mut rom, desc + 2, 5);
    write_be32(&mut rom, desc + 4, px as u32);
    write_be32(&mut rom, desc + 8, py as u32);
    write_be32(&mut rom, desc + 12, pz as u32);
    rom[desc + 16] = 0x0C;

    for c in 0..7usize {
        write_be_f32(&mut rom, px + c * 4, 800.0 + c as f32 * 750.0);
    }
    for r in 0..5usize {
        write_be_f32(&mut rom, py + r * 4, -10.0 + r as f32 * 5.0);
    }
    for r in 0..5usize {
        for c in 0..7usize {
            let v = (-18i16 + (r as i16) * 4 + (c as i16) * 3) as i8;
            rom[pz + r * 7 + c] = v as u8;
        }
    }

    let cands = blind_discover_v2(&rom, &BlindV2Options::default());
    let hit = cands
        .iter()
        .find(|c| !c.is_curve && c.address == pz)
        .expect("must discover odd-aligned 0x0C i8 Denso SH descriptor in 4 MiB ROM");
    assert_eq!((hit.rows, hit.cols), (5, 7));
    assert_eq!(hit.dtype_label, "i8");
    assert!(hit.detection_source.starts_with("denso_sh_descriptor(i8)"));
}

#[test]
fn bosch_mg1_u8hdr_full_scale_axis_and_neg_to_zero_curve_are_discovered() {
    let mut rom = vec![0xFFu8; 65_536];
    fill_cal_texture(&mut rom[0x2000..0xA000]);

    // 1. Bosch MG1 [06][06] 2-D table with full-scale Y-axis ending at 65535
    let hdr = 0x3000usize;
    rom[hdr] = 6;
    rom[hdr + 1] = 6;
    let x_vals = [1700u16, 2000, 3000, 4000, 4500, 5500];
    let y_vals = [0u16, 13107, 29491, 32768, 42598, 65535];
    for (i, &v) in x_vals.iter().enumerate() {
        write_le16(&mut rom, hdr + 2 + i * 2, v);
    }
    let y_off = hdr + 2 + 12;
    for (i, &v) in y_vals.iter().enumerate() {
        write_le16(&mut rom, y_off + i * 2, v);
    }
    let z_off = y_off + 12;
    for r in 0..6usize {
        for c in 0..6usize {
            let v = (1200 + r * 180 + c * 110) as u16;
            write_le16(&mut rom, z_off + (r * 6 + c) * 2, v);
        }
    }

    // 2. Negative-to-zero i16_le knock-retard curve (-500 .. 0 across 12 pts)
    let curve_off = 0x6200usize;
    for i in 0..12usize {
        let sv = -500i16 + (i as i16) * 45 + if i == 11 { 5 } else { 0 };
        write_le16(&mut rom, curve_off + i * 2, sv as u16);
    }

    let opts = BlindV2Options {
        include_curves: true,
        ..BlindV2Options::default()
    };
    let cands = blind_discover_v2(&rom, &opts);

    let mg1_hit = cands
        .iter()
        .find(|c| !c.is_curve && c.address == z_off)
        .unwrap_or_else(|| {
            let summary: Vec<_> = cands
                .iter()
                .filter(|c| !c.is_curve)
                .map(|c| (format!("0x{:X}", c.address), c.rows, c.cols, &c.detection_source))
                .collect();
            panic!("must discover Bosch MG1 [06][06] table at 0x{z_off:X}; got={summary:?}");
        });
    assert_eq!((mg1_hit.rows, mg1_hit.cols), (6, 6));
    assert!(mg1_hit.detection_source.starts_with("bosch_siemens_struct_u8hdr"));

    let neg_curve = cands
        .iter()
        .find(|c| c.is_curve && c.dtype_label == "i16_le" && c.address.abs_diff(curve_off) <= 2)
        .unwrap_or_else(|| {
            let summary: Vec<_> = cands
                .iter()
                .filter(|c| c.is_curve)
                .map(|c| (format!("0x{:X}", c.address), c.cols, &c.dtype_label, &c.detection_source))
                .collect();
            panic!("must discover negative-to-zero i16_le curve at 0x{curve_off:X}; got={summary:?}");
        });
    assert!(neg_curve.cols >= 10);
}
