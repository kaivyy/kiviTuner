//! Parity tests for the shared scan metrics.
//!
//! Ported from `ecu_engine_rust::recognition::heuristic` (`calculate_entropy`,
//! `autocorrelation_lag_w`, `calculate_surface_smoothness`) **and**
//! `ecu_engine_rust::recognition::zero_xdf_discovery` (`shannon_entropy`,
//! `autocorr_w`, `calc_smoothness`), which are byte-identical algorithms.
//! One module serves both, so the program does not carry duplicate maths.
//!
//! ## How expected values were obtained
//!
//! Every expected value is hand-derived from the definition:
//! * Entropy of a uniform 256-symbol distribution is exactly 8.0 bits.
//! * Entropy of a single repeated byte is exactly 0.0.
//! * Entropy of two equally likely symbols is exactly 1.0.
//! * Autocorrelation at lag 0 of a demeaned signal equals 1.0.
//! * Smoothness of a constant matrix is 0 (no variation, `std_dev` floor aside).
//!
//! None was produced by calling the implementation.

use kivi_scanner::scan_metrics::{
    autocorrelation, entropy, scan_16bit_be, scan_16bit_le, smoothness,
};

// ---------------------------------------------------------------------------
// entropy
// ---------------------------------------------------------------------------

#[test]
fn entropy_of_empty_input_is_zero() {
    assert_eq!(entropy(&[]), 0.0);
}

#[test]
fn entropy_of_a_single_repeated_byte_is_zero() {
    // One symbol with p = 1 -> -1 * log2(1) = 0
    assert_eq!(entropy(&[0x42; 1024]), 0.0);
}

#[test]
fn entropy_of_two_equally_likely_symbols_is_one_bit() {
    let data = [0x00, 0x01, 0x00, 0x01];
    let e = entropy(&data);
    assert!((e - 1.0).abs() < 1e-12, "expected 1.0, got {e}");
}

#[test]
fn entropy_of_a_uniform_256_symbol_distribution_is_eight_bits() {
    // Every byte value exactly once -> p = 1/256 for each -> 8 bits.
    let data: Vec<u8> = (0..=255u8).collect();
    let e = entropy(&data);
    assert!((e - 8.0).abs() < 1e-12, "expected 8.0, got {e}");
}

#[test]
fn entropy_of_four_equally_likely_symbols_is_two_bits() {
    let data = [0u8, 1, 2, 3, 0, 1, 2, 3];
    let e = entropy(&data);
    assert!((e - 2.0).abs() < 1e-12, "expected 2.0, got {e}");
}

#[test]
fn entropy_is_bounded_by_zero_and_eight() {
    let varied: Vec<u8> = (0..2000).map(|i| ((i * 37) % 256) as u8).collect();
    let e = entropy(&varied);
    assert!((0.0..=8.0).contains(&e), "entropy out of range: {e}");
}

// ---------------------------------------------------------------------------
// autocorrelation
// ---------------------------------------------------------------------------

#[test]
fn autocorrelation_at_lag_zero_of_a_demeaned_signal_is_one() {
    let data = [1.0, 2.0, 3.0, 4.0, 5.0];
    let a = autocorrelation(&data, 0);
    assert!((a - 1.0).abs() < 1e-12, "expected 1.0, got {a}");
}

#[test]
fn autocorrelation_of_a_constant_signal_is_zero() {
    // Denormal variance -> guarded to 0.0.
    let data = [5.0; 32];
    assert_eq!(autocorrelation(&data, 1), 0.0);
}

#[test]
fn autocorrelation_returns_zero_when_lag_exceeds_length() {
    let data = [1.0, 2.0, 3.0];
    assert_eq!(autocorrelation(&data, 3), 0.0);
    assert_eq!(autocorrelation(&data, 99), 0.0);
}

#[test]
fn autocorrelation_of_a_periodic_signal_peaks_at_its_period() {
    // Period-4 square wave: lag 4 should be strongly positive.
    let data: Vec<f64> = (0..64)
        .map(|i| if i % 4 < 2 { 10.0 } else { -10.0 })
        .collect();
    let a4 = autocorrelation(&data, 4);
    assert!(a4 > 0.9, "expected strong period-4 correlation, got {a4}");
}

#[test]
fn autocorrelation_of_alternating_signal_is_negative_at_odd_lag() {
    let data: Vec<f64> = (0..64)
        .map(|i| if i % 2 == 0 { 1.0 } else { -1.0 })
        .collect();
    let a1 = autocorrelation(&data, 1);
    assert!(a1 < -0.9, "expected strong negative lag-1, got {a1}");
}

#[test]
fn autocorrelation_of_empty_input_is_zero() {
    assert_eq!(autocorrelation(&[], 1), 0.0);
}

// ---------------------------------------------------------------------------
// smoothness
// ---------------------------------------------------------------------------

#[test]
fn smoothness_of_a_constant_matrix_is_zero() {
    // No variation between cells; std_dev floor makes the ratio tiny.
    let m = vec![vec![7.0; 4]; 4];
    let s = smoothness(&m);
    assert!(s.abs() < 1e-6, "expected ~0, got {s}");
}

#[test]
fn smoothness_of_a_single_row_or_column_returns_one() {
    assert_eq!(smoothness(&[vec![1.0, 2.0, 3.0]]), 1.0, "rows < 2");
    assert_eq!(smoothness(&[vec![1.0], vec![2.0]]), 1.0, "cols < 2");
}

#[test]
fn smoothness_of_empty_matrix_returns_one() {
    assert_eq!(smoothness(&[]), 1.0);
}

#[test]
fn smoothness_grows_with_discontinuity() {
    let flat = vec![vec![1.0; 8]; 8];
    let mut rough = flat.clone();
    rough[4][4] = 100.0;
    assert!(
        smoothness(&rough) > smoothness(&flat),
        "a spike must raise the roughness ratio"
    );
}

#[test]
fn smoothness_is_lower_for_a_smooth_ramp_than_for_a_spike() {
    let ramp: Vec<Vec<f64>> = (0..8)
        .map(|r| (0..8).map(|c| (r * 8 + c) as f64).collect())
        .collect();
    let mut spike = vec![vec![50.0; 8]; 8];
    spike[3][3] = 200.0;

    assert!(
        smoothness(&ramp) < smoothness(&spike),
        "ramp {} should be smoother than spike {}",
        smoothness(&ramp),
        smoothness(&spike)
    );
}

// ---------------------------------------------------------------------------
// 16-bit scanning
// ---------------------------------------------------------------------------

#[test]
fn scan_16bit_le_reads_low_byte_first() {
    // bytes [0x01,0x00] as LE u16 = 1
    let rom = [0x01u8, 0x00, 0x02, 0x00];
    let words = scan_16bit_le(&rom);
    assert_eq!(words, vec![1u16, 2]);
}

#[test]
fn scan_16bit_be_reads_high_byte_first() {
    let rom = [0x00u8, 0x01, 0x00, 0x02];
    let words = scan_16bit_be(&rom);
    assert_eq!(words, vec![1u16, 2]);
}

#[test]
fn scan_16bit_ignores_a_trailing_odd_byte() {
    // 5 bytes -> 2 complete words, final byte dropped.
    let rom = [0x01u8, 0x00, 0x02, 0x00, 0xFF];
    assert_eq!(scan_16bit_le(&rom), vec![1u16, 2]);
    assert_eq!(scan_16bit_be(&rom).len(), 2);
}

#[test]
fn scan_16bit_of_empty_input_is_empty() {
    assert!(scan_16bit_le(&[]).is_empty());
    assert!(scan_16bit_be(&[]).is_empty());
}

#[test]
fn scan_16bit_of_one_byte_is_empty() {
    assert!(scan_16bit_le(&[0xAB]).is_empty());
}

#[test]
fn scan_16bit_is_deterministic_and_ordered() {
    let rom: Vec<u8> = (0..64).collect();
    let a = scan_16bit_le(&rom);
    let b = scan_16bit_le(&rom);
    assert_eq!(a, b, "repeated runs must be identical");
    // Output order follows input order.
    assert_eq!(a[0], u16::from_le_bytes([0, 1]));
    assert_eq!(a[1], u16::from_le_bytes([2, 3]));
}
