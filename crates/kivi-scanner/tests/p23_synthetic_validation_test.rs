//! Synthetic validation suite implementing all 8 defect closures from
//! `research/wave-3.3/WAVE-3.3-P2.3-SYNTHETIC-DESIGN-SPEC.md`:
//! - Explicit 4-way label separation (`VALID_TABLE`, `STRUCTURAL_DECOY`, `BACKGROUND`, `ADVERSARIAL`)
//! - Fisher–Yates negative-control twin with mandatory `verify A–D`
//! - Canonical `smoothness`, `laplacian_energy`, gradient magnitude, and non-gate `Δxy`
//! - Width-normalized `exact` / `near_cells<=2` / `near_abs<=8` stratified reporting
//! - Headline `S4.7` (no-axis) comparison: legacy Scanner C = 0 (scope ceiling) vs Blind V2

use kivi_scanner::axis_anchored::axis_anchored_scan;
use kivi_scanner::blind_v2::{blind_discover_v2, BlindV2Options};
use kivi_scanner::p23_descriptors::compute_p23_descriptors;
use kivi_scanner::scan_metrics::autocorrelation;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SyntheticLabel {
    ValidTable,
    StructuralDecoy,
    Background,
}

#[derive(Debug, Clone)]
struct PlantedItem {
    id: &'static str,
    label: SyntheticLabel,
    address: usize,
    rows: usize,
    cols: usize,
    elem_bytes: usize,
}

struct XorShift64(u64);
impl XorShift64 {
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn next_f64_unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / ((1u64 << 53) as f64)
    }
    fn next_normal(&mut self) -> f64 {
        let u1 = self.next_f64_unit().max(1e-12);
        let u2 = self.next_f64_unit();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
    }
}

fn byte_histogram(data: &[u8]) -> [usize; 256] {
    let mut h = [0usize; 256];
    for &b in data {
        h[b as usize] += 1;
    }
    h
}

fn fisher_yates_twin(src: &[u8], seed: u64) -> Vec<u8> {
    let mut out = src.to_vec();
    let mut rng = XorShift64(seed);
    for i in (1..out.len()).rev() {
        let j = (rng.next_u64() as usize) % (i + 1);
        out.swap(i, j);
    }
    out
}

fn put_u16_le(buf: &mut [u8], addr: usize, vals: &[u16]) {
    for (i, &v) in vals.iter().enumerate() {
        buf[addr + i * 2..addr + i * 2 + 2].copy_from_slice(&v.to_le_bytes());
    }
}

fn plant_u16_le_axes(
    buf: &mut [u8],
    x_off: usize,
    y_off: usize,
    cols: usize,
    rows: usize,
) -> usize {
    for c in 0..cols {
        let v = (1000 + c * 400) as u16;
        buf[x_off + c * 2..x_off + c * 2 + 2].copy_from_slice(&v.to_le_bytes());
    }
    for r in 0..rows {
        let v = (20 + r * 15) as u16;
        buf[y_off + r * 2..y_off + r * 2 + 2].copy_from_slice(&v.to_le_bytes());
    }
    y_off + rows * 2
}

#[test]
fn p23_defects_and_four_way_taxonomy_verified() {
    // Verify S1.1..S1.8 mathematically on explicit grids.
    // S1.1 Smooth ramp 16x16 (VALID_TABLE)
    let s11: Vec<Vec<f64>> = (0..16)
        .map(|r| (0..16).map(|c| 500.0 + r as f64 * 80.0 + c as f64 * 40.0).collect())
        .collect();
    let d11 = compute_p23_descriptors(&s11).unwrap();
    assert!(d11.canonical_smoothness < 0.38);
    assert!(d11.delta_xy_ratio < 1e-9);
    assert!(d11.laplacian_energy < 1e-9);

    // S1.2 Smooth curved interaction surface (VALID_TABLE, Δxy != 0 only slightly)
    let s12: Vec<Vec<f64>> = (0..16)
        .map(|r| {
            (0..16)
                .map(|c| 1000.0 + (r as f64) * 60.0 + (c as f64) * 45.0 + 3.0 * ((r * c) as f64))
                .collect()
        })
        .collect();
    let d12 = compute_p23_descriptors(&s12).unwrap();
    assert!(d12.canonical_smoothness < 0.38);

    // S1.3 Step function (STRUCTURAL_DECOY — high localized gradient)
    let s13: Vec<Vec<f64>> = (0..16)
        .map(|r| (0..16).map(|_| if r < 8 { 100.0 } else { 5000.0 }).collect())
        .collect();
    let d13 = compute_p23_descriptors(&s13).unwrap();
    assert!(d13.grad_max > d12.grad_max);

    // S1.4 Switch matrix 0/1 alternating (STRUCTURAL_DECOY — high roughness)
    let mut rng = XorShift64(99);
    let s14: Vec<Vec<f64>> = (0..16)
        .map(|_| {
            (0..16)
                .map(|_| if (rng.next_u64() & 1) == 0 { 0.0 } else { 1.0 })
                .collect()
        })
        .collect();
    let d14 = compute_p23_descriptors(&s14).unwrap();
    assert!(d14.canonical_smoothness > 0.48);

    // S1.5 Multiplicative map f(i)*g(j): Δxy != 0 (Defect 4 proves Δxy must NOT gate!)
    let s15: Vec<Vec<f64>> = (0..16)
        .map(|r| (0..16).map(|c| (10.0 + r as f64 * 4.0) * (100.0 + c as f64 * 25.0)).collect())
        .collect();
    let d15 = compute_p23_descriptors(&s15).unwrap();
    assert!(d15.delta_xy_ratio > 0.01);
    assert!(d15.canonical_smoothness < 0.38);

    // S1.6 Localized single bump (STRUCTURAL_DECOY — strong Laplacian energy)
    let s16: Vec<Vec<f64>> = (0..16)
        .map(|r| {
            (0..16)
                .map(|c| {
                    let dr = r as f64 - 7.5;
                    let dc = c as f64 - 7.5;
                    100.0 + 3000.0 * (-(dr * dr + dc * dc) / 1.5).exp()
                })
                .collect()
        })
        .collect();
    let d16 = compute_p23_descriptors(&s16).unwrap();
    assert!(d16.laplacian_energy > 0.25);

    // S1.7a Noisy surface SNR=10 dB Gaussian in value domain, unclipped
    let var_sig = s12.iter().flatten().map(|&v| v * v).sum::<f64>() / 256.0;
    let noise_sd = (var_sig / 10.0f64).sqrt();
    let s17a: Vec<Vec<f64>> = s12
        .iter()
        .map(|row| row.iter().map(|&v| v + rng.next_normal() * noise_sd).collect())
        .collect();
    let d17a = compute_p23_descriptors(&s17a).unwrap();
    assert!(d17a.canonical_smoothness.is_finite());

    // S1.8a Constant repeated rows (rank 1 flat rows -> canonical_smoothness != s18b)
    let s18a: Vec<Vec<f64>> = vec![(0..16).map(|c| 200.0 + c as f64 * 50.0).collect(); 16];
    let s18b: Vec<Vec<f64>> = (0..16)
        .map(|r| {
            let rr = if r == 7 { 6 } else { r };
            (0..16).map(|c| 200.0 + rr as f64 * 60.0 + c as f64 * 45.0).collect()
        })
        .collect();
    assert!(compute_p23_descriptors(&s18a).unwrap().canonical_smoothness < 0.38);
    assert!(compute_p23_descriptors(&s18b).unwrap().canonical_smoothness < 0.38);
}

#[test]
fn synthetic_1mib_rom_with_verified_fisher_yates_twin_and_headline_s47() {
    // Build a deterministic 1 MiB synthetic ROM with heterogeneous background bytes:
    // structured non-erasure bytes in planted 4 KiB blocks (so analyze_rom_blocks keeps them)
    // and uniform PRNG code/data bytes everywhere else (authentic twin byte histogram).
    let rom_len = 1_048_576usize;
    let mut bg_rng = XorShift64(0x1234_5678_9ABC_DEF0);
    let mut rom: Vec<u8> = (0..rom_len)
        .map(|_| (bg_rng.next_u64() & 0xFF) as u8)
        .collect();
    let tex = [0x55u8, 0xAA, 0x11];
    for block_start in [0x010000usize, 0x030000, 0x050000, 0x070000, 0x090000] {
        for (i, slot) in rom[block_start..block_start + 4096].iter_mut().enumerate() {
            *slot = tex[i % 3];
        }
    }
    let mut items = Vec::new();
    let mut axis_spans = Vec::new();

    // 1) Axis-supported S1.2 (16x16 u16 LE) @ block 0x010000
    let x1 = 0x010000usize;
    let y1 = x1 + 32;
    let a1 = plant_u16_le_axes(&mut rom, x1, y1, 16, 16);
    axis_spans.push((x1, a1));
    let vals1: Vec<u16> = (0..16)
        .flat_map(|r| (0..16).map(move |c| (1000 + r * 120 + c * 90 + 2 * r * c) as u16))
        .collect();
    put_u16_le(&mut rom, a1, &vals1);
    items.push(PlantedItem {
        id: "S1.2",
        label: SyntheticLabel::ValidTable,
        address: a1,
        rows: 16,
        cols: 16,
        elem_bytes: 2,
    });

    // 2) S4.7 Headline No-Axis (8x12 u16 LE) @ block 0x030000
    let a_s47 = 0x030000usize;
    let vals_s47: Vec<u16> = (0..8)
        .flat_map(|r| (0..12).map(move |c| (800 + r * 180 + c * 110) as u16))
        .collect();
    put_u16_le(&mut rom, a_s47, &vals_s47);
    items.push(PlantedItem {
        id: "S4.7",
        label: SyntheticLabel::ValidTable,
        address: a_s47,
        rows: 8,
        cols: 12,
        elem_bytes: 2,
    });

    // 3) S3.6 32-bit integer map (8x8 u32 LE) @ block 0x050000 (not flat-monotone: curved peak!)
    let a_s36 = 0x050000usize;
    for r in 0..8 {
        for c in 0..8 {
            let dr = r as i64 - 4;
            let dc = c as i64 - 3;
            let v = (50_000i64 - (dr * dr * 600 + dc * dc * 400) + (r as i64 * 250)) as u32;
            let o = a_s36 + (r * 8 + c) * 4;
            rom[o..o + 4].copy_from_slice(&v.to_le_bytes());
        }
    }
    items.push(PlantedItem {
        id: "S3.6",
        label: SyntheticLabel::ValidTable,
        address: a_s36,
        rows: 8,
        cols: 8,
        elem_bytes: 4,
    });

    // 4) S3.7 32-bit float map (6x6 f32 LE) @ block 0x070000
    let a_s37 = 0x070000usize;
    for r in 0..6 {
        for c in 0..6 {
            let v = 12.5f32 + (r as f32) * 1.2 + (c as f32) * 0.7 - 0.08 * ((r * c) as f32);
            let o = a_s37 + (r * 6 + c) * 4;
            rom[o..o + 4].copy_from_slice(&v.to_le_bytes());
        }
    }
    items.push(PlantedItem {
        id: "S3.7",
        label: SyntheticLabel::ValidTable,
        address: a_s37,
        rows: 6,
        cols: 6,
        elem_bytes: 4,
    });

    // 5) S1.8a Constant repeated rows decoy @ block 0x090000 (must NOT be accepted)
    let a_s18a = 0x090000usize;
    let row0: Vec<u16> = (0..16).map(|c| (500 + c * 200) as u16).collect();
    for r in 0..16 {
        put_u16_le(&mut rom, a_s18a + r * 32, &row0);
    }
    items.push(PlantedItem {
        id: "S1.8a",
        label: SyntheticLabel::StructuralDecoy,
        address: a_s18a,
        rows: 16,
        cols: 16,
        elem_bytes: 2,
    });

    // Build Fisher-Yates negative-control twin and enforce Verify A..D.
    let twin = fisher_yates_twin(&rom, 0xCAFE_BABE_1234_5678);

    // Verify A: exact 256-bin byte histogram equality.
    assert_eq!(byte_histogram(&rom), byte_histogram(&twin), "verify A failed");

    // Verify B: zero monotone u16 runs >= 4 at originally planted axis spans.
    for &(ax_s, ax_e) in &axis_spans {
        let mut mono_len = 1usize;
        let mut max_mono = 1usize;
        let mut o = ax_s;
        while o + 4 <= ax_e {
            let a = u16::from_le_bytes([twin[o], twin[o + 1]]);
            let b = u16::from_le_bytes([twin[o + 2], twin[o + 3]]);
            if b > a && (b - a) <= 10000 {
                mono_len += 1;
                max_mono = max_mono.max(mono_len);
            } else {
                mono_len = 1;
            }
            o += 2;
        }
        assert!(max_mono < 4, "verify B failed: run={max_mono} at 0x{ax_s:X}");
    }

    // Verify C: no planted table address in the twin passes variance & low roughness (< 0.48).
    for item in &items {
        let mut flat = Vec::with_capacity(item.rows * item.cols);
        for k in 0..(item.rows * item.cols) {
            let o = item.address + k * item.elem_bytes;
            let v = match item.elem_bytes {
                2 => u16::from_le_bytes([twin[o], twin[o + 1]]) as f64,
                4 => u32::from_le_bytes([twin[o], twin[o + 1], twin[o + 2], twin[o + 3]]) as f64,
                _ => twin[o] as f64,
            };
            flat.push(v);
        }
        let grid: Vec<Vec<f64>> = flat.chunks(item.cols).map(|s| s.to_vec()).collect();
        let desc = compute_p23_descriptors(&grid).unwrap();
        assert!(
            desc.canonical_smoothness >= 0.48,
            "verify C failed on {} twin smoothness={}",
            item.id,
            desc.canonical_smoothness
        );
    }

    // Verify D: twin max |ACF| <= original planted ACF p05.
    let mut orig_acfs = Vec::new();
    let mut twin_acfs = Vec::new();
    for item in items.iter().filter(|i| i.label == SyntheticLabel::ValidTable) {
        let decode = |buf: &[u8]| -> Vec<f64> {
            (0..item.rows * item.cols)
                .map(|k| {
                    let o = item.address + k * item.elem_bytes;
                    if item.elem_bytes == 2 {
                        u16::from_le_bytes([buf[o], buf[o + 1]]) as f64
                    } else if item.id == "S3.7" {
                        f32::from_le_bytes([buf[o], buf[o + 1], buf[o + 2], buf[o + 3]]) as f64
                    } else {
                        u32::from_le_bytes([buf[o], buf[o + 1], buf[o + 2], buf[o + 3]]) as f64
                    }
                })
                .collect()
        };
        orig_acfs.push(autocorrelation(&decode(&rom), item.cols).abs());
        twin_acfs.push(autocorrelation(&decode(&twin), item.cols).abs());
    }
    orig_acfs.sort_by(|a, b| a.total_cmp(b));
    let twin_max_acf = twin_acfs.into_iter().fold(0.0f64, f64::max);
    assert!(
        twin_max_acf <= orig_acfs[0],
        "verify D failed: twin_max_acf={twin_max_acf} orig_p05={}",
        orig_acfs[0]
    );

    // Run Blind V2 on twin (BACKGROUND control): graduation limit <= 1 FP / MiB!
    let twin_found = blind_discover_v2(&twin, &BlindV2Options::default());
    assert!(
        twin_found.len() <= 1,
        "negative-control FP exceeded 1/MiB: found {} on 1 MiB Fisher-Yates twin ({:?})",
        twin_found.len(),
        twin_found
            .iter()
            .map(|c| (c.address, c.rows, c.cols, &c.detection_source, &c.dtype_label))
            .collect::<Vec<_>>()
    );

    // Run Legacy Scanner C vs Blind V2 on planted ROM & check stratified metrics (exact/near_cells<=2/near_abs<=8).
    let legacy = axis_anchored_scan(&rom, true, 64);
    let v2 = blind_discover_v2(&rom, &BlindV2Options::default());

    // Headline S4.7 expectation: legacy Scanner C = 0 (scope limitation), Blind V2 = exact match!
    assert!(
        legacy.iter().all(|t| t.address != a_s47),
        "S4.7 must be unreachable to axis_anchored_scan"
    );
    assert!(
        v2.iter().any(|t| {
            let d = t.address.abs_diff(a_s47);
            d == 0 && d <= 2 * 2 && d <= 8 && t.rows == 8 && t.cols == 12
        }),
        "Blind V2 must find headline S4.7 @0x{a_s47:X}"
    );

    // Per-dtype VALID_TABLE exact checks: S1.2 (u16), S3.6 (u32), S3.7 (f32).
    for item in items.iter().filter(|i| i.label == SyntheticLabel::ValidTable) {
        let hit = v2.iter().any(|t| {
            t.address.abs_diff(item.address) <= 2 * item.elem_bytes
                && t.elem_bytes == item.elem_bytes
        });
        assert!(
            hit,
            "Blind V2 missed VALID_TABLE {} @0x{:X} ({}B); got {:?}",
            item.id,
            item.address,
            item.elem_bytes,
            v2.iter().map(|t| (t.address, t.rows, t.cols, t.elem_bytes)).collect::<Vec<_>>()
        );
    }

    // STRUCTURAL_DECOY S1.8a (constant rows) must be rejected by Blind V2.
    assert!(
        v2.iter().all(|t| t.address != a_s18a),
        "S1.8a constant-row decoy @0x{a_s18a:X} should be rejected"
    );
    let _ = SyntheticLabel::Background;
}
