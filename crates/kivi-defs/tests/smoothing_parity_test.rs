//! Behavioral parity tests for the ported smoothing primitives against
//! `ecu_engine_rust::math::smoothing`.
//!
//! **Every expected value was produced by executing the unmodified source**
//! through a disposable oracle harness (`/tmp/parity-slice5b`), printed at 17
//! significant digits so f64 fixtures round-trip exactly.
//!
//! Tolerance: the target is a verbatim reimplementation of the same arithmetic
//! in the same order, so results are bit-identical. Assertions use
//! `to_bits()`. NaN is compared via `is_nan()` because `NaN != NaN`.
//!
//! Source quirks preserved (not fixed):
//! * `alpha = NaN` passes both range checks and produces an all-NaN matrix.
//! * `find_safest_row_to_remove` with every span degenerate returns
//!   `best_row_index = 1` and infinite errors (the loop `continue`s and the
//!   initial values survive).
//! * Ragged matrices are processed using the first row's width only.

use kivi_defs::smoothing::{
    find_safest_row_to_remove, gaussian_smooth_3x3, laplacian_curvature_2d, spatial_smooth_2d,
    SmoothError,
};

fn bits(v: f64) -> u64 {
    v.to_bits()
}

fn assert_matrix_bits(got: &[Vec<f64>], want: &[Vec<f64>], label: &str) {
    assert_eq!(got.len(), want.len(), "{}: row count", label);
    for (r, (g, w)) in got.iter().zip(want.iter()).enumerate() {
        assert_eq!(g.len(), w.len(), "{}: row {} width", label, r);
        for (c, (gv, wv)) in g.iter().zip(w.iter()).enumerate() {
            if wv.is_nan() {
                assert!(
                    gv.is_nan(),
                    "{}[{},{}]: expected NaN, got {}",
                    label,
                    r,
                    c,
                    gv
                );
            } else {
                assert_eq!(
                    bits(*gv),
                    bits(*wv),
                    "{}[{},{}]: {} vs {}",
                    label,
                    r,
                    c,
                    gv,
                    wv
                );
            }
        }
    }
}

fn spike() -> Vec<Vec<f64>> {
    vec![
        vec![10.0, 10.0, 10.0],
        vec![10.0, 50.0, 10.0],
        vec![10.0, 10.0, 10.0],
    ]
}

// ---------------------------------------------------------------------------
// spatial_smooth_2d
// ---------------------------------------------------------------------------

#[test]
fn parity_spatial_smooth_spike_alpha_05() {
    let got = spatial_smooth_2d(&spike(), 0.5).expect("smooth failed");
    // Oracle: edges average 3 neighbours (10,10,50 -> 23.333) -> 0.5*10+0.5*23.333
    let edge = 0.5 * 10.0 + 0.5 * ((10.0 + 10.0 + 50.0) / 3.0);
    let want = vec![
        vec![10.0, edge, 10.0],
        vec![edge, 30.0, edge],
        vec![10.0, edge, 10.0],
    ];
    assert_matrix_bits(&got, &want, "spike a=0.5");
    assert_eq!(bits(got[0][1]), bits(16.666666666666664));
}

#[test]
fn parity_spatial_smooth_spike_alpha_01_and_09() {
    let a01 = spatial_smooth_2d(&spike(), 0.1).expect("smooth failed");
    // (1-0.1)*50 + 0.1*10 = 45 + 1 = 46
    assert_eq!(bits(a01[1][1]), bits(46.0));

    let a09 = spatial_smooth_2d(&spike(), 0.9).expect("smooth failed");
    // (1-0.9)*50 + 0.9*10 = 5 + 9 = 14
    assert_eq!(bits(a09[1][1]), bits(14.0));
}

#[test]
fn parity_spatial_smooth_ramp_full_matrix() {
    let ramp = vec![
        vec![1.0, 2.0, 3.0],
        vec![4.0, 5.0, 6.0],
        vec![7.0, 8.0, 9.0],
        vec![10.0, 11.0, 12.0],
    ];
    let got = spatial_smooth_2d(&ramp, 0.5).expect("smooth failed");
    // Oracle values (17 significant digits) from the unmodified source.
    let want = vec![
        vec![
            2.0,
            2.5,
            3.5,
        ],
        vec![
            4.166666666666666,
            5.0,
            5.833333333333334,
        ],
        vec![
            7.166666666666666,
            8.0,
            8.833333333333332,
        ],
        vec![
            9.5,
            10.5,
            11.0,
        ],
    ];
    assert_matrix_bits(&got, &want, "ramp a=0.5");
}

#[test]
fn parity_spatial_smooth_nonsquare_and_negative() {
    let nonsquare = vec![
        vec![1.0, 2.0, 3.0, 4.0, 5.0],
        vec![6.0, 7.0, 8.0, 9.0, 10.0],
        vec![11.0, 12.0, 13.0, 14.0, 15.0],
    ];
    let got = spatial_smooth_2d(&nonsquare, 0.5).expect("smooth failed");
    // Corner (0,0): only 2 neighbours (right=2, down=6) -> (1-0.5)*1 + 0.5*4 = 2.5
    assert_eq!(bits(got[0][0]), bits(2.5));
    // Centre (1,2): oracle value for a 3x5 ramp
    assert_eq!(bits(got[1][2]), bits(8.0));
    assert_eq!(bits(got[1][1]), bits(7.0));

    let neg = vec![
        vec![-5.0, -10.0, -15.0],
        vec![-20.0, -25.0, -30.0],
        vec![-35.0, -40.0, -45.0],
    ];
    let g = spatial_smooth_2d(&neg, 0.5).expect("smooth failed");
    assert_eq!(bits(g[1][1]), bits(-25.0));
}

#[test]
fn parity_spatial_smooth_constant_unchanged() {
    let c = vec![vec![7.0; 4]; 3];
    let got = spatial_smooth_2d(&c, 0.5).expect("smooth failed");
    assert_matrix_bits(&got, &c, "constant");
}

#[test]
fn parity_spatial_smooth_degenerate_shapes() {
    // One row: no vertical neighbours, only horizontal.
    let one_row = vec![vec![1.0, 2.0, 3.0]];
    let got = spatial_smooth_2d(&one_row, 0.5).expect("smooth failed");
    assert_eq!(bits(got[0][0]), bits(1.5)); // (1*0.5) + 0.5*2
    assert_eq!(bits(got[0][1]), bits(2.0));
    assert_eq!(bits(got[0][2]), bits(2.5));

    // One column.
    let one_col = vec![vec![1.0], vec![2.0], vec![3.0]];
    let g = spatial_smooth_2d(&one_col, 0.5).expect("smooth failed");
    assert_eq!(bits(g[0][0]), bits(1.5));
    assert_eq!(bits(g[1][0]), bits(2.0));
    assert_eq!(bits(g[2][0]), bits(2.5));

    // Empty.
    let empty: Vec<Vec<f64>> = vec![];
    assert!(spatial_smooth_2d(&empty, 0.5).expect("empty ok").is_empty());
}

#[test]
fn parity_spatial_smooth_alpha_validation() {
    for bad in [0.0, 1.0, -0.1, 1.1] {
        let err = spatial_smooth_2d(&spike(), bad).expect_err("alpha must be rejected");
        assert_eq!(err, SmoothError::InvalidAlpha);
        assert_eq!(
            err.to_string(),
            "Alpha smoothing factor must be strictly between 0.0 and 1.0"
        );
    }
}

#[test]
fn parity_spatial_smooth_nan_alpha_produces_all_nan() {
    // Source quirk: NaN fails both `<= 0.0` and `>= 1.0`, so validation passes
    // and every cell becomes NaN. Preserved verbatim.
    let got = spatial_smooth_2d(&spike(), f64::NAN).expect("NaN alpha is not rejected");
    for row in &got {
        for v in row {
            assert!(v.is_nan(), "expected NaN, got {}", v);
        }
    }
}

// ---------------------------------------------------------------------------
// gaussian_smooth_3x3
// ---------------------------------------------------------------------------

#[test]
fn parity_gaussian_spike_sigma1() {
    let got = gaussian_smooth_3x3(&spike(), 1.0);
    // Border rows/cols are copied unchanged.
    assert_eq!(bits(got[0][0]), bits(10.0));
    assert_eq!(bits(got[0][1]), bits(10.0));
    // Oracle value from the unmodified source (sigma=1, 3x3 spike).
    assert_eq!(bits(got[1][1]), bits(18.167198222866322));
}

#[test]
fn parity_gaussian_sigma_clamped_below_01() {
    // sigma is clamped to >= 0.1, so sigma=0 and sigma=-1 must match sigma=0.1.
    let a = gaussian_smooth_3x3(&spike(), 0.0);
    let b = gaussian_smooth_3x3(&spike(), 0.1);
    let c = gaussian_smooth_3x3(&spike(), -1.0);
    assert_matrix_bits(&a, &b, "sigma0 vs sigma0.1");
    assert_matrix_bits(&c, &b, "sigma-1 vs sigma0.1");
}

#[test]
fn parity_gaussian_small_matrices_are_copied_unchanged() {
    let two_rows = vec![vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0]];
    assert_matrix_bits(&gaussian_smooth_3x3(&two_rows, 1.0), &two_rows, "two rows");

    let two_cols = vec![vec![1.0, 2.0], vec![3.0, 4.0], vec![5.0, 6.0]];
    assert_matrix_bits(&gaussian_smooth_3x3(&two_cols, 1.0), &two_cols, "two cols");

    let empty: Vec<Vec<f64>> = vec![];
    assert!(gaussian_smooth_3x3(&empty, 1.0).is_empty());
}

#[test]
fn parity_gaussian_constant_preserved_at_interior() {
    // Normalised kernel sums to 1, so interior cells of a constant surface
    // stay constant (borders are copied anyway).
    let c = vec![vec![7.0; 4]; 3];
    let got = gaussian_smooth_3x3(&c, 1.0);
    for row in &got {
        for v in row {
            assert!(
                (v - 7.0).abs() < 1e-12,
                "constant surface must stay ~7, got {}",
                v
            );
        }
    }
}

// ---------------------------------------------------------------------------
// laplacian_curvature_2d
// ---------------------------------------------------------------------------

#[test]
fn parity_laplacian_spike() {
    let got = laplacian_curvature_2d(&spike());
    // Interior: 10 + 10 + 10 + 10 - 4*50 = -160
    assert_eq!(bits(got[1][1]), bits(-160.0));
    // Borders are zero.
    assert_eq!(bits(got[0][0]), bits(0.0));
    assert_eq!(bits(got[0][1]), bits(0.0));
    assert_eq!(bits(got[2][2]), bits(0.0));
}

#[test]
fn parity_laplacian_linear_ramp_is_zero_interior() {
    // A linear ramp has zero second derivative.
    let ramp = vec![
        vec![1.0, 2.0, 3.0],
        vec![4.0, 5.0, 6.0],
        vec![7.0, 8.0, 9.0],
        vec![10.0, 11.0, 12.0],
    ];
    let got = laplacian_curvature_2d(&ramp);
    assert_eq!(bits(got[1][1]), bits(0.0));
    assert_eq!(bits(got[2][1]), bits(0.0));
}

#[test]
fn parity_laplacian_small_and_empty() {
    let two_rows = vec![vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0]];
    let got = laplacian_curvature_2d(&two_rows);
    assert_eq!(got.len(), 2);
    assert!(got.iter().all(|r| r.iter().all(|v| *v == 0.0)));

    let empty: Vec<Vec<f64>> = vec![];
    assert!(laplacian_curvature_2d(&empty).is_empty());
}

#[test]
fn parity_laplacian_nonsquare_and_negative() {
    let nonsquare = vec![
        vec![1.0, 2.0, 3.0, 4.0, 5.0],
        vec![6.0, 7.0, 8.0, 9.0, 10.0],
        vec![11.0, 12.0, 13.0, 14.0, 15.0],
    ];
    let got = laplacian_curvature_2d(&nonsquare);
    assert_eq!(bits(got[1][2]), bits(0.0)); // linear -> 0

    let neg = vec![
        vec![-5.0, -10.0, -15.0],
        vec![-20.0, -25.0, -30.0],
        vec![-35.0, -40.0, -45.0],
    ];
    let g = laplacian_curvature_2d(&neg);
    assert_eq!(bits(g[1][1]), bits(0.0));
}

#[test]
fn parity_laplacian_ragged_uses_first_row_width() {
    // Source quirk: `cols` comes from matrix[0].len(), so a longer later row is
    // silently ignored. Preserved verbatim.
    let ragged = vec![vec![1.0, 2.0], vec![3.0, 4.0, 5.0], vec![6.0, 7.0]];
    let got = laplacian_curvature_2d(&ragged);
    assert_eq!(got.len(), 3);
    assert!(
        got.iter().all(|r| r.len() == 2),
        "output width follows row 0"
    );
}

// ---------------------------------------------------------------------------
// find_safest_row_to_remove
// ---------------------------------------------------------------------------

#[test]
fn parity_row_reduction_linear_exact() {
    let m = vec![vec![10.0, 20.0], vec![15.0, 25.0], vec![20.0, 30.0]];
    let axis = vec![1000.0, 2000.0, 3000.0];
    let r = find_safest_row_to_remove(&m, &axis).expect("reduction failed");
    assert_eq!(r.best_row_index, 1);
    assert_eq!(bits(r.min_max_error), bits(0.0));
    assert_eq!(bits(r.min_avg_error), bits(0.0));
}

#[test]
fn parity_row_reduction_spike_center() {
    let r = find_safest_row_to_remove(&spike(), &[0.0, 1.0, 2.0]).expect("reduction failed");
    // Oracle: middle row deviates by 40 (10 -> expected 50)
    assert_eq!(r.best_row_index, 1);
    assert_eq!(bits(r.min_max_error), bits(40.0));
    assert_eq!(bits(r.min_avg_error), bits(13.333333333333334));
}

#[test]
fn parity_row_reduction_rejects_too_few_rows() {
    let m = vec![vec![1.0, 2.0], vec![3.0, 4.0]];
    let err = find_safest_row_to_remove(&m, &[0.0, 1.0]).expect_err("2 rows must fail");
    assert_eq!(err, SmoothError::TooFewRows);
    assert_eq!(
        err.to_string(),
        "Matrix must have at least 3 rows to evaluate row reduction"
    );

    let empty: Vec<Vec<f64>> = vec![];
    let err = find_safest_row_to_remove(&empty, &[]).expect_err("empty must fail");
    assert_eq!(err, SmoothError::TooFewRows);
}

#[test]
fn parity_row_reduction_rejects_axis_length_mismatch() {
    let m = vec![vec![10.0, 20.0], vec![15.0, 25.0], vec![20.0, 30.0]];
    let err = find_safest_row_to_remove(&m, &[0.0, 1.0]).expect_err("mismatch must fail");
    assert_eq!(err, SmoothError::AxisLengthMismatch);
    assert_eq!(
        err.to_string(),
        "Axis values length must match matrix row count"
    );
}

#[test]
fn parity_row_reduction_all_spans_degenerate_returns_row_1_inf() {
    // Source quirk: every interior row is skipped via `continue`, so the
    // initial `best_row = 1` and infinite errors survive. Preserved verbatim.
    let m = vec![vec![10.0, 20.0], vec![15.0, 25.0], vec![20.0, 30.0]];
    let r = find_safest_row_to_remove(&m, &[0.0, 0.0, 0.0]).expect("degenerate spans ok");
    assert_eq!(r.best_row_index, 1);
    assert!(r.min_max_error.is_infinite());
    assert!(r.min_avg_error.is_infinite());
}

#[test]
fn parity_row_reduction_descending_axis_is_supported() {
    // Unlike `search_interval`, this algorithm only uses the axis for a ratio,
    // so a descending axis works. Oracle: row 1, zero error.
    let m = vec![vec![10.0, 20.0], vec![15.0, 25.0], vec![20.0, 30.0]];
    let r = find_safest_row_to_remove(&m, &[3000.0, 2000.0, 1000.0]).expect("descending ok");
    assert_eq!(r.best_row_index, 1);
    assert_eq!(bits(r.min_max_error), bits(0.0));
}

#[test]
fn parity_row_reduction_uneven_spacing() {
    let m = vec![vec![10.0, 20.0], vec![15.0, 25.0], vec![20.0, 30.0]];
    let r = find_safest_row_to_remove(&m, &[0.0, 100.0, 1000.0]).expect("uneven ok");
    // ratio = 100/1000 = 0.1 -> interp = 10 + 0.1*10 = 11 vs actual 15 -> err 4
    assert_eq!(r.best_row_index, 1);
    assert_eq!(bits(r.min_max_error), bits(4.0));
}

// ---------------------------------------------------------------------------
// Non-mutation and determinism
// ---------------------------------------------------------------------------

#[test]
fn smoothing_functions_do_not_mutate_their_input() {
    let original = spike();
    let snapshot = original.clone();

    let _ = spatial_smooth_2d(&original, 0.5).unwrap();
    let _ = gaussian_smooth_3x3(&original, 1.0);
    let _ = laplacian_curvature_2d(&original);
    let _ = find_safest_row_to_remove(&original, &[0.0, 1.0, 2.0]).unwrap();

    assert_matrix_bits(&original, &snapshot, "input preserved");
}

#[test]
fn smoothing_functions_are_deterministic() {
    for _ in 0..3 {
        assert_matrix_bits(
            &spatial_smooth_2d(&spike(), 0.37).unwrap(),
            &spatial_smooth_2d(&spike(), 0.37).unwrap(),
            "spatial determinism",
        );
        assert_matrix_bits(
            &gaussian_smooth_3x3(&spike(), 0.83),
            &gaussian_smooth_3x3(&spike(), 0.83),
            "gaussian determinism",
        );
        assert_matrix_bits(
            &laplacian_curvature_2d(&spike()),
            &laplacian_curvature_2d(&spike()),
            "laplacian determinism",
        );
    }
}
