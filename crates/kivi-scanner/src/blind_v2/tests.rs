    use super::*;
    use crate::axis_anchored::axis_anchored_scan;

    #[test]
    fn blind_v2_discovers_8bit_and_decreasing_axes_and_32bit_and_no_axis_tables() {
        // Fill with non-monotonic 3-byte repeating texture [0x55, 0xAA, 0x11]
        // (passes zero_xdf_prefilter entropy/frequency window while creating no false ramps).
        let pattern = [0x55u8, 0xAA, 0x11];
        let mut rom: Vec<u8> = (0..8192).map(|i| pattern[i % 3]).collect();

        // 1) 8-bit X/Y axes (strictly increasing) + non-monotonic-per-row 8-bit 6x8 curve/map @0x10E.
        let x8 = [20u8, 40, 60, 80, 100, 120, 140, 160];
        let y8 = [10u8, 30, 50, 70, 90, 110];
        rom[0x100..0x108].copy_from_slice(&x8);
        rom[0x108..0x10E].copy_from_slice(&y8);
        let t8_addr = 0x10Eusize; // pad=0 from 8-bit y_end!
        for r in 0..6usize {
            for c in 0..8usize {
                let dc = (c as isize - 3).unsigned_abs();
                let dr = (r as isize - 2).unsigned_abs();
                rom[t8_addr + r * 8 + c] = (200 - dr * 16 - dc * 10) as u8;
            }
        }

        // 2) Decreasing u16 LE axes + 6x8 u16 LE table @0x31E.
        let x16 = [8000u16, 7200, 6400, 5600, 4800, 4000, 3200, 2400];
        let y16 = [200u16, 170, 140, 110, 80, 50];
        for (i, &v) in x16.iter().enumerate() {
            rom[0x300 + i * 2..0x302 + i * 2].copy_from_slice(&v.to_le_bytes());
        }
        for (i, &v) in y16.iter().enumerate() {
            rom[0x310 + i * 2..0x312 + i * 2].copy_from_slice(&v.to_le_bytes());
        }
        let t16_dec_addr = 0x31Eusize; // pad = 2 from y_end (0x31C)
        for r in 0..6usize {
            for c in 0..8usize {
                let dc = (c as isize - 3).unsigned_abs();
                let off = t16_dec_addr + (r * 8 + c) * 2;
                let v = (2400 + r * 220 - dc * 120) as u16;
                rom[off..off + 2].copy_from_slice(&v.to_le_bytes());
            }
        }

        // 3) Axis-free u16 LE 8x12 table (S4.7 case).
        let t_no_axis = 0x600usize;
        for r in 0..8usize {
            for c in 0..12usize {
                let dc = (c as isize - 5).unsigned_abs();
                let off = t_no_axis + (r * 12 + c) * 2;
                let v = (1800 + r * 190 - dc * 85) as u16;
                rom[off..off + 2].copy_from_slice(&v.to_le_bytes());
            }
        }

        // 4) 32-bit f32 LE 6x6 smooth surface (S3.7 case, 4-byte aligned).
        let t_f32 = 0x1200usize;
        for r in 0..6 {
            for c in 0..6 {
                let val = 10.0f32 + (r as f32) * 1.5 + (c as f32) * 0.8 + 0.05 * ((r * c) as f32);
                let off = t_f32 + (r * 6 + c) * 4;
                rom[off..off + 4].copy_from_slice(&val.to_le_bytes());
            }
        }

        // Legacy Scanner C misses all 4!
        let legacy = axis_anchored_scan(&rom, true, 64);
        assert!(legacy.iter().all(|t| t.address != t8_addr));
        assert!(legacy.iter().all(|t| t.address != t16_dec_addr));
        assert!(legacy.iter().all(|t| t.address != t_no_axis));
        assert!(legacy.iter().all(|t| t.address != t_f32));

        let v2 = blind_discover_v2(&rom, &BlindV2Options::default());
        assert!(
            v2.iter().any(|t| t.address == t8_addr && t.elem_bytes == 1),
            "expected 8-bit axis-anchored map @0x{t8_addr:X}, got {:?}",
            v2.iter().map(|t| (t.address, t.rows, t.cols, &t.dtype_label)).collect::<Vec<_>>()
        );
        assert!(
            v2.iter().any(|t| t.address == t16_dec_addr && t.elem_bytes == 2),
            "expected decreasing-axis u16 map @0x{t16_dec_addr:X}"
        );
        assert!(
            v2.iter().any(|t| t.address == t_no_axis && t.rows == 8 && t.cols == 12),
            "expected S4.7 axis-free map @0x{t_no_axis:X}, got {:?}",
            v2.iter().map(|t| (t.address, t.rows, t.cols, &t.dtype_label, &t.detection_source, t.smoothness)).collect::<Vec<_>>()
        );
        assert!(
            v2.iter().any(|t| t.address == t_f32 && t.dtype_label == "f32_le"),
            "expected f32 map @0x{t_f32:X}"
        );
    }

    #[test]
    fn gearys_c_separates_smooth_calibration_surfaces_from_noise_and_checkerboards() {
        let smooth: Vec<Vec<f64>> = (0..10)
            .map(|r| (0..10).map(|c| 100.0 + (r as f64) * 12.0 + (c as f64) * 8.0).collect())
            .collect();
        let checker: Vec<Vec<f64>> = (0..10)
            .map(|r| {
                (0..10)
                    .map(|c| if (r + c) % 2 == 0 { 10.0 } else { 200.0 })
                    .collect()
            })
            .collect();
        let mut x = 0x1234_5678_9ABC_DEF0u64;
        let random: Vec<Vec<f64>> = (0..16)
            .map(|_| {
                (0..16)
                    .map(|_| {
                        x ^= x << 13;
                        x ^= x >> 7;
                        x ^= x << 17;
                        (x % 65536) as f64
                    })
                    .collect()
            })
            .collect();

        assert!(gearys_c_2d(&smooth) < 0.20, "smooth C={}", gearys_c_2d(&smooth));
        assert!(gearys_c_2d(&checker) > 1.50, "checker C={}", gearys_c_2d(&checker));
        assert!(gearys_c_2d(&random) > 0.70, "random C={}", gearys_c_2d(&random));
    }

    #[test]
    fn statistical_curve_gates_accept_real_curves_and_reject_twin_erasure_spikes() {
        // Real 16-pt smooth bell curve (1 turning point).
        let bell: Vec<f64> = (0..16)
            .map(|i| {
                let d = (i as f64) - 7.5;
                400.0 - 5.0 * d * d
            })
            .collect();
        assert!(passes_statistical_curve_gates(&bell, 2, 1));

        // Real 14-pt monotonic ramp (0 turning points).
        let ramp: Vec<f64> = (0..14).map(|i| 120.0 + (i as f64) * 35.0).collect();
        assert!(passes_statistical_curve_gates(&ramp, 2, 0));

        // Exact TWIN_AUDI_RS4 false positive sample with 0xFF erasures & jump spike.
        let twin_sample = vec![166.0, 118.0, 143.0, 131.0, 255.0, 255.0, 255.0, 255.0, 245.0];
        assert!(!passes_statistical_curve_gates(&twin_sample, 1, 2));

        // Unacceptable random jump plateau.
        let jump_plateau = vec![10.0, 11.0, 12.0, 13.0, 200.0, 201.0, 202.0, 203.0];
        assert!(!passes_statistical_curve_gates(&jump_plateau, 1, 0));

        // 8-pt monotonic ramp and 12-pt smooth bell must pass finite-sample z-VN & RVN gates.
        let ramp8: Vec<f64> = (0..8).map(|i| 100.0 + i as f64 * 80.0).collect();
        assert!(passes_statistical_curve_gates(&ramp8, 2, 0), "8-pt ramp must pass");

        let bell12 = vec![
            100.0, 150.0, 220.0, 300.0, 380.0, 420.0, 420.0, 380.0, 300.0, 220.0, 150.0, 100.0,
        ];
        assert!(passes_statistical_curve_gates(&bell12, 2, 1), "12-pt bell must pass");
    }

    #[test]
    fn calibration_ranges_v2_keeps_high_entropy_u16_and_prunes_code_and_blank() {
        let mut rom = vec![0xFFu8; 4096];
        // Block 1 (512..1024): smooth high-entropy u16_le waveform whose LSB varies widely.
        for i in 0..256usize {
            let v = ((i * 53) % 60000) as u16;
            let hi = (100 + (i / 2)) as u16;
            let combined = (hi << 8) | (v & 0x00FF);
            rom[512 + i * 2..514 + i * 2].copy_from_slice(&combined.to_le_bytes());
        }
        // Block 3 (1536..2048): high-entropy pseudo-random machine code across bytes and words.
        let mut x = 0x9E37_79B9_7F4A_7C15u64;
        for b in &mut rom[1536..2048] {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = (x & 0xFF) as u8;
        }
        let ranges = super::prefilter::calibration_ranges_v2(&rom);
        assert!(
            ranges.iter().any(|&(s, e)| s <= 512 && e >= 1024),
            "smooth high-entropy u16 block must be kept: {ranges:?}"
        );
        assert!(
            !ranges.iter().any(|&(s, e)| s <= 1536 && e >= 2048),
            "pseudo-random code block must be pruned: {ranges:?}"
        );
        assert!(
            !ranges.iter().any(|&(s, e)| s == 0 && e == rom.len()),
            "blank padding must be pruned: {ranges:?}"
        );
    }

    #[test]
    fn i8_zero_crossing_surface_is_discovered_and_decoded_signed() {
        let pattern = [0x55u8, 0xAA, 0x11];
        let mut rom: Vec<u8> = (0..16_384).map(|i| pattern[i % 3]).collect();
        let addr = 0x2000usize;
        let rows = 8usize;
        let cols = 8usize;
        for r in 0..rows {
            for c in 0..cols {
                let v = (-40i16 + (r as i16 * 7) + (c as i16 * 5)) as i8;
                rom[addr + r * cols + c] = v as u8;
            }
        }
        let cands = blind_discover_v2(&rom, &BlindV2Options::default());
        let hit = cands
            .iter()
            .find(|c| c.address.abs_diff(addr) <= 4 && c.dtype_label == "i8")
            .unwrap_or_else(|| {
                panic!(
                    "must detect signed i8 surface at 0x{addr:X}; got {:?}",
                    cands.iter().map(|c| (c.address, c.rows, c.cols, &c.dtype_label)).collect::<Vec<_>>()
                )
            });
        let tables = crate::blind_automap::blind_v2_candidates_to_tables(&rom, std::slice::from_ref(hit), None);
        assert_eq!(tables.len(), 1);
        assert!(tables[0].raw_min < 0.0 && tables[0].raw_max > 0.0);
    }

    #[test]
    fn phase_wrap_snapping_recovers_exact_column0_offset() {
        let pattern = [0x55u8, 0xAA, 0x11];
        let mut rom = vec![0xFFu8; 8192];
        for (i, b) in rom[0x0600..0x0E00].iter_mut().enumerate() {
            *b = pattern[i % 3];
        }
        // Plant an 8x12 u16_le table at 0x0902 preceded by 4 smooth pre-cells so phase-wrap
        // column-cliff snapping recovers exact column-0 alignment while a slight col-1 notch
        // prevents row 0 from being parsed as a monotonic 12-pt axis.
        let target = 0x0902usize;
        let pre4: [u16; 4] = [2100, 2140, 2180, 2220];
        for (i, &v) in pre4.iter().enumerate() {
            rom[target - 8 + i * 2..target - 6 + i * 2].copy_from_slice(&v.to_le_bytes());
        }
        for r in 0..8usize {
            for c in 0..12usize {
                let v = if c == 0 {
                    1200 + r * 60
                } else if c == 1 {
                    1190 + r * 60
                } else {
                    1200 + r * 60 + c * 90
                } as u16;
                let off = target + (r * 12 + c) * 2;
                rom[off..off + 2].copy_from_slice(&v.to_le_bytes());
            }
        }
        let opts = BlindV2Options {
            include_8bit: false,
            include_32bit: false,
            ..BlindV2Options::default()
        };
        let cands = blind_discover_v2(&rom, &opts);
        assert!(
            cands.iter().any(|c| c.address.abs_diff(target) <= 4 && c.rows == 8 && c.cols == 12),
            "expected snapped 8x12 hit near 0x{target:X}; got {:?}",
            cands.iter().map(|c| (c.address, c.rows, c.cols, &c.dtype_label, c.smoothness)).collect::<Vec<_>>()
        );
    }
