//! Peer-reviewed mathematical invariants and Fisher-Yates Twin-ROM null controls.
//!
//! Verifies:
//! 1. Geary's C (1954) exact checkerboard value, smooth plane, and white-noise E[C] = 1.0.
//! 2. P2.3 finite-difference Laplacian & mixed difference Δxy orthogonality vs smoothness.
//! 3. Bartels Rank von Neumann Ratio (1982, JASA 77(377):40-46) exact permutation
//!    moments E[RVN] = 2.0 and closed-form finite-sample Var[RVN] for n = 5, 6, 7.
//! 4. Bienayme-Kendall turning-point rate E[T/(n-2)] = 2/3, Shannon entropy (1948),
//!    and Box-Jenkins autocorrelation.
//! 5. Fisher-Yates Twin-ROM null control across real multi-brand OEM BIN+XDF pairs:
//!    shuffling active calibration words destroys spatial structure while preserving exact
//!    marginal distribution, proving blind discovery does not hallucinate maps on noise.

use kivi_scanner::blind_v2::{
    blind_discover_v2, gearys_c_2d, passes_statistical_curve_gates, BlindV2Options,
};
use kivi_scanner::p23_descriptors::compute_p23_descriptors;
use kivi_scanner::scan_metrics::{autocorrelation, entropy, smoothness};
use kivi_scanner::xdf_parse::parse_xdf_xml;
use kivi_scanner::zero_xdf_prefilter::analyze_rom_blocks;
use std::path::Path;
use std::time::Instant;

struct SplitMix64(u64);

impl SplitMix64 {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn next_f64_unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / ((1u64 << 53) as f64))
    }
}

fn exact_bartels_rvn(seq: &[usize]) -> f64 {
    let n = seq.len();
    let nf = n as f64;
    let mean = (nf + 1.0) / 2.0;
    let den: f64 = seq.iter().map(|&r| (r as f64 - mean).powi(2)).sum();
    let num: f64 = seq
        .windows(2)
        .map(|w| ((w[1] as f64) - (w[0] as f64)).powi(2))
        .sum();
    num / den
}

fn visit_permutations(cur: &mut Vec<usize>, used: &mut Vec<bool>, n: usize, stats: &mut Vec<f64>) {
    if cur.len() == n {
        stats.push(exact_bartels_rvn(cur));
        return;
    }
    for v in 1..=n {
        if !used[v - 1] {
            used[v - 1] = true;
            cur.push(v);
            visit_permutations(cur, used, n, stats);
            cur.pop();
            used[v - 1] = false;
        }
    }
}

#[test]
fn gearys_c_and_p23_descriptors_match_exact_mathematics() {
    // 1. Exact 8x8 checkerboard: every Rook neighbor differs by +/-1, mean=0.5, var=0.25.
    // Theoretical C = ((64 - 1) * 112) / (2 * 112 * 16) = 63 / 32 = 1.96875.
    let checker: Vec<Vec<f64>> = (0..8)
        .map(|r| (0..8).map(|c| ((r + c) & 1) as f64).collect())
        .collect();
    let c_check = gearys_c_2d(&checker);
    assert!(
        (c_check - 63.0 / 32.0).abs() < 1e-12,
        "expected exact checkerboard C=1.96875, got {c_check}"
    );

    // 2. Smooth affine plane z = 100 + 3*r + 5*c: harmonic interior (L = 0) & separable (Δxy = 0).
    let affine: Vec<Vec<f64>> = (0..8)
        .map(|r| (0..8).map(|c| 100.0 + 3.0 * r as f64 + 5.0 * c as f64).collect())
        .collect();
    let c_affine = gearys_c_2d(&affine);
    assert!(c_affine < 0.08, "affine C should be << 1, got {c_affine}");
    let p23 = compute_p23_descriptors(&affine).expect("p23 affine");
    assert!((p23.canonical_smoothness - smoothness(&affine)).abs() < 1e-12);
    assert!(p23.laplacian_energy < 1e-12, "harmonic plane must have L=0");
    assert!(p23.delta_xy_ratio < 1e-12, "additive plane must have Δxy=0");

    // 3. Coupled bilinear surface z = r * c has Δxy != 0 while still spatially smooth.
    let saddle: Vec<Vec<f64>> = (0..8)
        .map(|r| (0..8).map(|c| (r * c) as f64).collect())
        .collect();
    let p23_saddle = compute_p23_descriptors(&saddle).expect("p23 saddle");
    assert!(gearys_c_2d(&saddle) < 0.25);
    assert!(p23_saddle.delta_xy_ratio > 0.05);

    // 4. Monte Carlo null calibration: 500 i.i.d. white-noise 8x8 surfaces -> E[C] == 1.0.
    let mut rng = SplitMix64(0xCAFE_BABE_1234_5678);
    let trials = 500usize;
    let mut sum_c = 0.0;
    for _ in 0..trials {
        let noise: Vec<Vec<f64>> = (0..8)
            .map(|_| (0..8).map(|_| rng.next_f64_unit()).collect())
            .collect();
        sum_c += gearys_c_2d(&noise);
    }
    let mean_c = sum_c / trials as f64;
    eprintln!("[MATH] Geary C 8x8 white-noise MC (N={trials}): mean={mean_c:.5} (theory=1.00000)");
    assert!(
        (mean_c - 1.0).abs() < 0.03,
        "MC Geary C mean drifted from 1.0: {mean_c}"
    );
}

#[test]
fn bartels_1982_exact_permutation_moments_and_curve_metrics() {
    // 1. Exact exhaustive permutation check for Bartels (1982, JASA 77(377):40-46) for n=5,6,7.
    for n in 5..=7usize {
        let mut rvns = Vec::new();
        let mut cur = Vec::with_capacity(n);
        let mut used = vec![false; n];
        visit_permutations(&mut cur, &mut used, n, &mut rvns);
        let count = rvns.len() as f64;
        let emp_mean = rvns.iter().sum::<f64>() / count;
        let emp_var = rvns.iter().map(|&v| (v - emp_mean).powi(2)).sum::<f64>() / count;
        let nf = n as f64;
        let theory_var = 4.0 * (nf - 2.0) * (5.0 * nf * nf - 2.0 * nf - 9.0)
            / (5.0 * nf * (nf + 1.0) * (nf - 1.0) * (nf - 1.0));
        eprintln!(
            "[MATH] Bartels RVN n={n} (perms={}): E={emp_mean:.12} (2.0), Var={emp_var:.12} vs closed={theory_var:.12}",
            rvns.len()
        );
        assert!((emp_mean - 2.0).abs() < 1e-12);
        assert!((emp_var - theory_var).abs() < 1e-12);
    }

    // 2. Bienayme-Kendall turning-point rate on 1000 i.i.d. n=32 random series -> theory = 2/3.
    let mut rng = SplitMix64(0xDEAD_BEEF_9988_7766);
    let mut turn_rate_sum = 0.0;
    let mut random_curve_accepted = 0usize;
    let trials = 1000usize;
    for _ in 0..trials {
        let seq: Vec<f64> = (0..32).map(|_| 100.0 + rng.next_f64_unit() * 5000.0).collect();
        let mut turns = 0usize;
        for w in seq.windows(3) {
            if (w[1] > w[0] && w[1] > w[2]) || (w[1] < w[0] && w[1] < w[2]) {
                turns += 1;
            }
        }
        turn_rate_sum += turns as f64 / 30.0;
        let seq16 = &seq[..16];
        let mut t16 = 0usize;
        for w in seq16.windows(3) {
            if (w[1] > w[0] && w[1] > w[2]) || (w[1] < w[0] && w[1] < w[2]) {
                t16 += 1;
            }
        }
        if passes_statistical_curve_gates(seq16, 2, t16) {
            random_curve_accepted += 1;
        }
    }
    let mean_turn = turn_rate_sum / trials as f64;
    let null_fpr = random_curve_accepted as f64 / trials as f64;
    eprintln!(
        "[MATH] Bienayme-Kendall turn rate (n=32, N={trials}): {mean_turn:.5} (theory=0.66667), curve_null_FPR={:.2}%",
        null_fpr * 100.0
    );
    assert!((mean_turn - (2.0 / 3.0)).abs() < 0.02);
    assert!(null_fpr <= 0.01, "random curves leaked through gates: {null_fpr}");

    // 3. Real smooth bell curve accepted vs jagged alternation rejected.
    let smooth_bell: Vec<f64> = (0..16)
        .map(|i| {
            let x = (i as f64 - 7.5) / 7.5;
            1200.0 + 900.0 * (1.0 - x * x)
        })
        .collect();
    assert!(passes_statistical_curve_gates(&smooth_bell, 2, 1));

    // 4. Shannon entropy (1948) and Box-Jenkins autocorrelation.
    assert!((entropy(&[0xAA; 64]) - 0.0).abs() < 1e-12);
    let binary: Vec<u8> = (0..256).map(|i| (i & 1) as u8).collect();
    assert!((entropy(&binary) - 1.0).abs() < 1e-12);
    let uniform: Vec<u8> = (0..=255u8).collect();
    assert!((entropy(&uniform) - 8.0).abs() < 1e-12);

    let wave: Vec<f64> = (0..32)
        .map(|i| match i % 4 {
            0 => 10.0,
            1 => 20.0,
            2 => 10.0,
            _ => 0.0,
        })
        .collect();
    assert!((autocorrelation(&wave, 0) - 1.0).abs() < 1e-12);
    assert!(autocorrelation(&wave, 4) > 0.85);
}

fn fisher_yates_word_twin(rom: &[u8], kept_ranges: &[(usize, usize)], seed: u64) -> Vec<u8> {
    let mut twin = rom.to_vec();
    let mut rng = SplitMix64(seed);
    for &(s, e) in kept_ranges {
        let aligned_s = s + (s & 1);
        let aligned_e = e - (e & 1);
        if aligned_e <= aligned_s + 4 {
            continue;
        }
        let n_words = (aligned_e - aligned_s) / 2;
        for i in (1..n_words).rev() {
            let j = (rng.next_u64() as usize) % (i + 1);
            if i != j {
                let a = aligned_s + i * 2;
                let b = aligned_s + j * 2;
                twin.swap(a, b);
                twin.swap(a + 1, b + 1);
            }
        }
    }
    twin
}

#[test]
fn fisher_yates_twin_rom_null_control_across_real_oem_roms() {
    let corpus: &[(&str, &str, &str)] = &[
        (
            "Suzuki_Hayabusa_5JCZSJB0",
            "/root/ecu_master_database/01_MOTORCYCLES/SUZUKI/BIG_BIKE_GSXR_HAYABUSA/5JCZSJB0.bin",
            "/root/ecu_master_database/01_MOTORCYCLES/SUZUKI/BIG_BIKE_GSXR_HAYABUSA/5JCZSJB0.xdf",
        ),
        (
            "Volvo_ME7_50WRHJ",
            "/root/ecu_master_database/02_CARS/VOLVO_SAAB/VOLVO_ME7_T5_R/50WRHJ-23WRA.bin",
            "/root/ecu_master_database/02_CARS/VOLVO_SAAB/VOLVO_ME7_T5_R/50WRHJ-23WRA.xdf",
        ),
        (
            "VW_M383_06A906018CG",
            "/root/ecu_master_database/02_CARS/VAG_AUDI_VOLKSWAGEN/VW_GOLF_MK4_1_8T_MOTRONIC_M383/06A906018CG.bin",
            "/root/ecu_master_database/02_CARS/VAG_AUDI_VOLKSWAGEN/VW_GOLF_MK4_1_8T_MOTRONIC_M383/06A906018CG.xdf",
        ),
        (
            "Honda_MSX125_K26_B13",
            "/root/ecu_master_database/01_MOTORCYCLES/HONDA/MINI_MOTO_MSX125_MONKEY/38770-K26-B13.bin",
            "/root/ecu_master_database/01_MOTORCYCLES/HONDA/MINI_MOTO_MSX125_MONKEY/38770-K26-B13.xdf",
        ),
        (
            "Honda_XADV_MKH_D42",
            "/root/ecu_master_database/01_MOTORCYCLES/HONDA/BIG_BIKE_CBR650_CB1000_XADV/38770-MKH-D42.bin",
            "/root/ecu_master_database/01_MOTORCYCLES/HONDA/MATIC_VARIO_PCX_ADV/HONDA_38770-MKH-D42_X-ADV_750___Forza_750___NC750X_(2017-2024).xdf",
        ),
        (
            "Suzuki_GSXR1000_41G00",
            "/root/ecu_master_database/01_MOTORCYCLES/SUZUKI/GSX_R1000_DENSO_41G00/32920-41G00.bin",
            "/root/ecu_master_database/01_MOTORCYCLES/SUZUKI/GSX_R1000_DENSO_41G00/32920-41G00.xdf",
        ),
    ];

    let available: Vec<_> = corpus
        .iter()
        .filter(|(_, b, x)| Path::new(b).exists() && Path::new(x).exists())
        .copied()
        .collect();
    if available.is_empty() {
        eprintln!("SKIP: no real corpus ROMs present on disk");
        return;
    }
    assert!(available.len() >= 4, "expected >=4 real ROM pairs on disk");

    eprintln!(
        "\n{:<24} {:>7} {:>7} {:>8} {:>9} {:>9} {:>7} {:>9} {:>9} {:>7} {:>7} {:>7}",
        "ROM", "SizeKB", "XDF_2D", "2D_Hit", "Real2D", "Null2D", "SNR2D", "Real1D", "Null1D", "SNR1D", "RealMs", "NullMs"
    );

    let opts = BlindV2Options {
        include_curves: true,
        ..BlindV2Options::default()
    };
    for (idx, &(name, bin_path, xdf_path)) in available.iter().enumerate() {
        let rom = std::fs::read(bin_path).expect("read real bin");
        let xdf_raw = std::fs::read(xdf_path).expect("read xdf");
        let parsed = parse_xdf_xml(&String::from_utf8_lossy(&xdf_raw)).expect("parse xdf");
        let xdf_2d_addrs: Vec<usize> = parsed
            .tables
            .iter()
            .filter(|t| {
                let r = t.y_axis.as_ref().map_or(1, |a| a.index_count);
                let c = t.x_axis.as_ref().map_or(1, |a| a.index_count);
                r >= 2 && c >= 2
            })
            .map(|t| t.z_axis.address)
            .collect();

        let pre = analyze_rom_blocks(&rom);
        let twin = fisher_yates_word_twin(&rom, &pre.kept_ranges, 0xA11C_E500 + idx as u64);

        let t0 = Instant::now();
        let real_cands = blind_discover_v2(&rom, &opts);
        let real_ms = t0.elapsed().as_millis();

        let t1 = Instant::now();
        let null_cands = blind_discover_v2(&twin, &opts);
        let null_ms = t1.elapsed().as_millis();

        let real_2d: Vec<_> = real_cands.iter().filter(|c| !c.is_curve).collect();
        let null_2d: Vec<_> = null_cands.iter().filter(|c| !c.is_curve).collect();
        let real_1d: Vec<_> = real_cands.iter().filter(|c| c.is_curve).collect();
        let null_1d: Vec<_> = null_cands.iter().filter(|c| c.is_curve).collect();

        let hits = xdf_2d_addrs
            .iter()
            .filter(|&&a| real_2d.iter().any(|c| c.address.abs_diff(a) <= 16))
            .count();
        let snr_2d = (real_2d.len() as f64) / (null_2d.len().max(1) as f64);
        let snr_1d = (real_1d.len() as f64) / (null_1d.len().max(1) as f64);

        let in_kept = |addr: usize| pre.kept_ranges.iter().any(|&(s, e)| addr >= s && addr < e);
        let mut src_2d: std::collections::BTreeMap<String, (usize, usize)> = std::collections::BTreeMap::new();
        for c in &null_2d {
            let e = src_2d.entry(c.detection_source.clone()).or_default();
            e.0 += 1;
            if in_kept(c.address) {
                e.1 += 1;
            }
        }
        let mut src_1d: std::collections::BTreeMap<String, (usize, usize)> = std::collections::BTreeMap::new();
        for c in &null_1d {
            let e = src_1d.entry(c.detection_source.clone()).or_default();
            e.0 += 1;
            if in_kept(c.address) {
                e.1 += 1;
            }
        }

        eprintln!(
            "{:<24} {:>7} {:>7} {:>8} {:>9} {:>9} {:>6.1}x {:>9} {:>9} {:>6.1}x {:>7} {:>7}",
            name,
            rom.len() / 1024,
            xdf_2d_addrs.len(),
            hits,
            real_2d.len(),
            null_2d.len(),
            snr_2d,
            real_1d.len(),
            null_1d.len(),
            snr_1d,
            real_ms,
            null_ms
        );
        if !src_2d.is_empty() {
            eprintln!("   [NULL-2D sources (total, in_shuffled_kept)]: {src_2d:?}");
        }
        if !src_1d.is_empty() {
            eprintln!("   [NULL-1D sources (total, in_shuffled_kept)]: {src_1d:?}");
        }

        assert!(
            real_2d.len() > null_2d.len() * 4,
            "{name}: blind_v2 2D failed null control! real_2d={} vs null_2d={}",
            real_2d.len(),
            null_2d.len()
        );
        assert!(
            real_1d.len() > null_1d.len() * 3 || (rom.len() <= 65_536 && null_1d.len() <= 5),
            "{name}: blind_v2 1D failed null control! real_1d={} vs null_1d={}",
            real_1d.len(),
            null_1d.len()
        );
    }
}
