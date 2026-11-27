//! Behavioral parity tests for the ported interpolation primitives against
//! `ecu_engine_rust::math::interpolation`.
//!
//! **Every expected value was produced by executing the unmodified source**
//! through a disposable oracle harness (`/tmp/parity-slice4`), printed at 17
//! significant digits so the fixtures round-trip f64 exactly.
//!
//! Tolerance policy: the target is a **verbatim reimplementation** of the same
//! arithmetic in the same order, so results are bit-identical. Assertions use
//! exact equality (`to_bits`) rather than an epsilon. The only exception is
//! NaN, compared via `is_nan()` because NaN != NaN.
//!
//! Source location: `src/math/interpolation.rs` (381 lines).
//! Extracted: `lerp_1d`, `catmull_rom_cubic_1d`, `bilinear_2d`,
//! `cubic_weight` (private helper), `bicubic_surface_16point`, `AxisLookup`.
//! **Not extracted**: `interpolate_matrix_region_bilinear` (mutates a matrix),
//! `MapComparisonMath` (mutating batch operations).

use kivi_defs::interp::{
    bilinear_2d, catmull_rom_cubic_1d, lerp_1d, lookup_1d_curve, lookup_2d_map, search_interval,
    AxisInterval, InterpError,
};

fn bits(v: f64) -> u64 {
    v.to_bits()
}

// ---------------------------------------------------------------------------
// lerp_1d
// ---------------------------------------------------------------------------

#[test]
fn parity_lerp_1d_all_oracle_cases() {
    // (name, x0, y0, x1, y1, x, expected)
    let cases: [(&str, f64, f64, f64, f64, f64, f64); 12] = [
        ("basic_mid", 0.0, 10.0, 1.0, 20.0, 0.5, 15.0),
        ("at_x0", 0.0, 10.0, 1.0, 20.0, 0.0, 10.0),
        ("at_x1", 0.0, 10.0, 1.0, 20.0, 1.0, 20.0),
        ("below_range", 0.0, 10.0, 1.0, 20.0, -0.5, 5.0),
        ("above_range", 0.0, 10.0, 1.0, 20.0, 1.5, 25.0),
        ("degenerate_x", 5.0, 10.0, 5.0, 20.0, 5.0, 10.0),
        (
            "degenerate_x_1e13",
            5.0,
            10.0,
            5.0000000000001,
            20.0,
            5.0,
            10.0,
        ),
        ("negative_values", 0.0, -10.0, 1.0, 10.0, 0.5, 0.0),
        ("descending_x", 10.0, 100.0, 0.0, 0.0, 5.0, 50.0),
        ("large_magnitude", 0.0, 1e15, 1.0, 2e15, 0.5, 1.5e15),
        // The expected value is the exact f64 the source algorithm produces,
        // which sits one ULP from the nearest "round" literal. It is spelled in
        // full on purpose; shortening it would silently weaken the oracle.
        #[allow(clippy::excessive_precision)]
        (
            "tiny_magnitude",
            0.0,
            1e-15,
            1.0,
            2e-15,
            0.5,
            1.50000000000000012e-15,
        ),
        ("y_equal", 0.0, 7.0, 1.0, 7.0, 0.3, 7.0),
    ];
    for (name, x0, y0, x1, y1, x, want) in cases {
        let got = lerp_1d(x0, y0, x1, y1, x);
        assert_eq!(
            bits(got),
            bits(want),
            "lerp_1d {}: {} vs {}",
            name,
            got,
            want
        );
    }
}

// ---------------------------------------------------------------------------
// catmull_rom_cubic_1d
// ---------------------------------------------------------------------------

#[test]
fn parity_catmull_rom_all_oracle_cases() {
    let cases: [(&str, f64, f64, f64, f64, f64, f64); 12] = [
        ("endpoint_t0", 10.0, 20.0, 40.0, 50.0, 0.0, 20.0),
        ("endpoint_t1", 10.0, 20.0, 40.0, 50.0, 1.0, 40.0),
        ("mid_t05", 10.0, 20.0, 40.0, 50.0, 0.5, 30.0),
        ("quarter_t025", 10.0, 20.0, 40.0, 50.0, 0.25, 24.53125),
        ("three_quarter_t075", 10.0, 20.0, 40.0, 50.0, 0.75, 35.46875),
        ("linear_data", 0.0, 1.0, 2.0, 3.0, 0.5, 1.5),
        ("constant", 5.0, 5.0, 5.0, 5.0, 0.5, 5.0),
        ("negative_mixed", -10.0, -5.0, 5.0, 10.0, 0.5, 0.0),
        ("t_negative", 10.0, 20.0, 40.0, 50.0, -0.5, 17.5),
        ("t_above_one", 10.0, 20.0, 40.0, 50.0, 1.5, 42.5),
        ("large_magnitude", 1e12, 2e12, 4e12, 5e12, 0.5, 3e12),
        ("cubic_curve", 0.0, 0.0, 1.0, 8.0, 0.5, 0.0625),
    ];
    for (name, pp, p0, p1, pn, t, want) in cases {
        let got = catmull_rom_cubic_1d(pp, p0, p1, pn, t);
        assert_eq!(
            bits(got),
            bits(want),
            "catmull {}: {} vs {}",
            name,
            got,
            want
        );
    }
}

#[test]
fn catmull_rom_passes_through_endpoints_exactly() {
    // Source property: t=0 returns p0 and t=1 returns p1 exactly.
    assert_eq!(catmull_rom_cubic_1d(10.0, 20.0, 40.0, 50.0, 0.0), 20.0);
    assert_eq!(catmull_rom_cubic_1d(10.0, 20.0, 40.0, 50.0, 1.0), 40.0);
}

// ---------------------------------------------------------------------------
// bilinear_2d
// ---------------------------------------------------------------------------

#[test]
fn parity_bilinear_2d_all_oracle_cases() {
    struct C {
        name: &'static str,
        x0: f64,
        x1: f64,
        y0: f64,
        y1: f64,
        q00: f64,
        q10: f64,
        q01: f64,
        q11: f64,
        x: f64,
        y: f64,
        want: f64,
    }
    let cases = [
        C {
            name: "corner_q00",
            x0: 0.0,
            x1: 1.0,
            y0: 0.0,
            y1: 1.0,
            q00: 10.0,
            q10: 20.0,
            q01: 30.0,
            q11: 40.0,
            x: 0.0,
            y: 0.0,
            want: 10.0,
        },
        C {
            name: "corner_q10",
            x0: 0.0,
            x1: 1.0,
            y0: 0.0,
            y1: 1.0,
            q00: 10.0,
            q10: 20.0,
            q01: 30.0,
            q11: 40.0,
            x: 1.0,
            y: 0.0,
            want: 20.0,
        },
        C {
            name: "corner_q01",
            x0: 0.0,
            x1: 1.0,
            y0: 0.0,
            y1: 1.0,
            q00: 10.0,
            q10: 20.0,
            q01: 30.0,
            q11: 40.0,
            x: 0.0,
            y: 1.0,
            want: 30.0,
        },
        C {
            name: "corner_q11",
            x0: 0.0,
            x1: 1.0,
            y0: 0.0,
            y1: 1.0,
            q00: 10.0,
            q10: 20.0,
            q01: 30.0,
            q11: 40.0,
            x: 1.0,
            y: 1.0,
            want: 40.0,
        },
        C {
            name: "center",
            x0: 0.0,
            x1: 1.0,
            y0: 0.0,
            y1: 1.0,
            q00: 10.0,
            q10: 20.0,
            q01: 30.0,
            q11: 40.0,
            x: 0.5,
            y: 0.5,
            want: 25.0,
        },
        C {
            name: "clamp_below",
            x0: 0.0,
            x1: 1.0,
            y0: 0.0,
            y1: 1.0,
            q00: 10.0,
            q10: 20.0,
            q01: 30.0,
            q11: 40.0,
            x: -5.0,
            y: -5.0,
            want: 10.0,
        },
        C {
            name: "clamp_above",
            x0: 0.0,
            x1: 1.0,
            y0: 0.0,
            y1: 1.0,
            q00: 10.0,
            q10: 20.0,
            q01: 30.0,
            q11: 40.0,
            x: 5.0,
            y: 5.0,
            want: 40.0,
        },
        C {
            name: "degenerate_x_span",
            x0: 0.0,
            x1: 0.0,
            y0: 0.0,
            y1: 1.0,
            q00: 10.0,
            q10: 20.0,
            q01: 30.0,
            q11: 40.0,
            x: 0.0,
            y: 0.5,
            want: 20.0,
        },
        C {
            name: "degenerate_y_span",
            x0: 0.0,
            x1: 1.0,
            y0: 0.0,
            y1: 0.0,
            q00: 10.0,
            q10: 20.0,
            q01: 30.0,
            q11: 40.0,
            x: 0.5,
            y: 0.0,
            want: 15.0,
        },
        C {
            name: "descending_x",
            x0: 1.0,
            x1: 0.0,
            y0: 0.0,
            y1: 1.0,
            q00: 10.0,
            q10: 20.0,
            q01: 30.0,
            q11: 40.0,
            x: 0.5,
            y: 0.5,
            want: 25.0,
        },
        C {
            name: "constant_surface",
            x0: 0.0,
            x1: 10.0,
            y0: 0.0,
            y1: 10.0,
            q00: 7.0,
            q10: 7.0,
            q01: 7.0,
            q11: 7.0,
            x: 3.0,
            y: 4.0,
            want: 7.0,
        },
        C {
            name: "negative_values",
            x0: 0.0,
            x1: 1.0,
            y0: 0.0,
            y1: 1.0,
            q00: -10.0,
            q10: -20.0,
            q01: -30.0,
            q11: -40.0,
            x: 0.5,
            y: 0.5,
            want: -25.0,
        },
    ];
    for c in cases {
        let got = bilinear_2d(c.x0, c.x1, c.y0, c.y1, c.q00, c.q10, c.q01, c.q11, c.x, c.y);
        assert_eq!(bits(got), bits(c.want), "bilinear {}", c.name);
    }
}

#[test]
fn bilinear_2d_clamps_out_of_range() {
    // Source clamps u and v into [0,1] rather than extrapolating.
    let at_min = bilinear_2d(0.0, 1.0, 0.0, 1.0, 10.0, 20.0, 30.0, 40.0, -100.0, -100.0);
    let at_max = bilinear_2d(0.0, 1.0, 0.0, 1.0, 10.0, 20.0, 30.0, 40.0, 100.0, 100.0);
    assert_eq!(at_min, 10.0);
    assert_eq!(at_max, 40.0);
}

// ---------------------------------------------------------------------------
// bicubic_surface_16point
// ---------------------------------------------------------------------------

fn patch_linear() -> [[f64; 4]; 4] {
    [
        [0.0, 1.0, 2.0, 3.0],
        [10.0, 11.0, 12.0, 13.0],
        [20.0, 21.0, 22.0, 23.0],
        [30.0, 31.0, 32.0, 33.0],
    ]
}

#[test]
fn parity_bicubic_all_oracle_cases() {
    let const_patch = [[5.0f64; 4]; 4];
    let delta_patch = [
        [0.0, 0.0, 0.0, 0.0],
        [0.0, 100.0, 0.0, 0.0],
        [0.0, 0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0, 0.0],
    ];
    let lin = patch_linear();

    // The patch references are borrowed from locals above, so the alias must
    // not require 'static.
    type PatchCase<'a> = (&'static str, &'a [[f64; 4]; 4], f64, f64, f64);
    let cases: [PatchCase<'_>; 11] = [
        ("linear_at_origin", &lin, 0.0, 0.0, 11.0),
        ("linear_at_center", &lin, 0.5, 0.5, 16.5),
        ("linear_at_11", &lin, 0.0, 0.0, 11.0),
        ("linear_u_only", &lin, 1.0, 0.0, 12.0),
        ("linear_v_only", &lin, 0.0, 1.0, 21.0),
        ("const_at_center", &const_patch, 0.5, 0.5, 5.0),
        ("const_at_corner", &const_patch, 0.0, 0.0, 5.0),
        ("delta_at_center", &delta_patch, 0.0, 0.0, 100.0),
        ("delta_at_half", &delta_patch, 0.5, 0.5, 31.640625),
        ("u_negative", &lin, -0.5, 0.0, 10.411764705882353),
        ("v_negative", &lin, 0.0, -0.5, 5.117647058823529),
    ];
    for (name, patch, u, v, want) in cases {
        let got = kivi_defs::interp::bicubic_surface_16point(patch, u, v);
        assert_eq!(
            bits(got),
            bits(want),
            "bicubic {}: {} vs {}",
            name,
            got,
            want
        );
    }
}

#[test]
fn bicubic_16point_layout_is_row_major_with_center_cell() {
    // The 4x4 patch is indexed [row][col]; the evaluated cell is between
    // row 1..2 and col 1..2, with (u,v) normalized from patch[1][1].
    // Setting only patch[1][1] to 100 and evaluating at (0,0) must return 100.
    let mut patch = [[0.0f64; 4]; 4];
    patch[1][1] = 100.0;
    assert_eq!(
        kivi_defs::interp::bicubic_surface_16point(&patch, 0.0, 0.0),
        100.0
    );
}

// ---------------------------------------------------------------------------
// search_interval
// ---------------------------------------------------------------------------

#[test]
fn parity_search_interval_all_oracle_cases() {
    let uniform = [1000.0, 2000.0, 3000.0, 4000.0];
    let nonuniform = [0.0, 100.0, 150.0, 1000.0];
    let dup = [0.0, 100.0, 100.0, 200.0];

    assert_eq!(
        search_interval(&uniform, 1000.0),
        Ok(AxisInterval {
            low: 0,
            high: 1,
            t: 0.0
        })
    );
    assert_eq!(
        search_interval(&uniform, 2000.0),
        Ok(AxisInterval {
            low: 1,
            high: 2,
            t: 0.0
        })
    );
    assert_eq!(
        search_interval(&uniform, 4000.0),
        Ok(AxisInterval {
            low: 2,
            high: 3,
            t: 1.0
        })
    );
    assert_eq!(
        search_interval(&uniform, 2500.0),
        Ok(AxisInterval {
            low: 1,
            high: 2,
            t: 0.5
        })
    );
    // Clamping, not extrapolation.
    assert_eq!(
        search_interval(&uniform, 500.0),
        Ok(AxisInterval {
            low: 0,
            high: 1,
            t: 0.0
        })
    );
    assert_eq!(
        search_interval(&uniform, 5000.0),
        Ok(AxisInterval {
            low: 2,
            high: 3,
            t: 1.0
        })
    );
    assert_eq!(
        search_interval(&nonuniform, 125.0),
        Ok(AxisInterval {
            low: 1,
            high: 2,
            t: 0.5
        })
    );
    assert_eq!(
        search_interval(&nonuniform, 500.0),
        Ok(AxisInterval {
            low: 2,
            high: 3,
            t: 0.4117647058823529
        })
    );
    // Duplicate breakpoints are NOT rejected by the source.
    assert_eq!(
        search_interval(&dup, 100.0),
        Ok(AxisInterval {
            low: 2,
            high: 3,
            t: 0.0
        })
    );
    assert_eq!(
        search_interval(&dup, 50.0),
        Ok(AxisInterval {
            low: 0,
            high: 1,
            t: 0.5
        })
    );
}

#[test]
fn parity_search_interval_rejects_short_axes() {
    assert_eq!(search_interval(&[5.0], 5.0), Err(InterpError::AxisTooShort));
    assert_eq!(search_interval(&[], 0.0), Err(InterpError::AxisTooShort));
}

#[test]
fn parity_search_interval_nan_target_yields_nan_t() {
    // Source quirk: a NaN target falls through every comparison and produces
    // t = NaN with low=0, high=1 rather than an error. Preserved verbatim.
    let uniform = [1000.0, 2000.0, 3000.0, 4000.0];
    let r = search_interval(&uniform, f64::NAN).expect("source returns Ok for NaN");
    assert_eq!(r.low, 0);
    assert_eq!(r.high, 1);
    assert!(r.t.is_nan(), "expected NaN t, got {}", r.t);
}

#[test]
fn parity_search_interval_inf_target_clamps_to_top() {
    let uniform = [1000.0, 2000.0, 3000.0, 4000.0];
    assert_eq!(
        search_interval(&uniform, f64::INFINITY),
        Ok(AxisInterval {
            low: 2,
            high: 3,
            t: 1.0
        })
    );
}

// ---------------------------------------------------------------------------
// lookup_2d_map
// ---------------------------------------------------------------------------

fn axes() -> (Vec<f64>, Vec<f64>, Vec<Vec<f64>>) {
    (
        vec![1000.0, 2000.0, 3000.0, 4000.0],
        vec![20.0, 40.0, 60.0],
        vec![
            vec![10.0, 15.0, 20.0, 25.0],
            vec![20.0, 25.0, 30.0, 35.0],
            vec![30.0, 35.0, 40.0, 45.0],
        ],
    )
}

#[test]
fn parity_lookup_2d_map_all_oracle_cases() {
    let (x, y, m) = axes();
    let cases: [(&str, f64, f64, f64); 8] = [
        ("exact_grid", 2000.0, 40.0, 25.0),
        ("mid_both", 2500.0, 50.0, 32.5),
        ("corner_min", 1000.0, 20.0, 10.0),
        ("corner_max", 4000.0, 60.0, 45.0),
        ("clamp_below_both", 0.0, 0.0, 10.0),
        ("clamp_above_both", 9999.0, 9999.0, 45.0),
        ("x_exact_y_mid", 3000.0, 50.0, 35.0),
        ("x_mid_y_exact", 2500.0, 60.0, 37.5),
    ];
    for (name, px, py, want) in cases {
        let got = lookup_2d_map(&x, &y, &m, px, py).expect("lookup failed");
        assert_eq!(bits(got), bits(want), "lookup_2d_map {}", name);
    }
}

#[test]
fn parity_lookup_2d_map_dimension_errors() {
    let (x, y, _) = axes();
    let bad_rows = vec![vec![1.0, 2.0]];
    let bad_cols = vec![vec![1.0], vec![2.0], vec![3.0]];
    let empty: Vec<Vec<f64>> = vec![];

    assert_eq!(
        lookup_2d_map(&x, &y, &bad_rows, 2000.0, 40.0),
        Err(InterpError::MapRowsMismatch)
    );
    assert_eq!(
        lookup_2d_map(&x, &y, &bad_cols, 2000.0, 40.0),
        Err(InterpError::MapColsMismatch)
    );
    assert_eq!(
        lookup_2d_map(&x, &y, &empty, 2000.0, 40.0),
        Err(InterpError::MapRowsMismatch)
    );
}

// ---------------------------------------------------------------------------
// lookup_1d_curve
// ---------------------------------------------------------------------------

#[test]
fn parity_lookup_1d_curve_all_oracle_cases() {
    let axis = [0.0, 100.0, 200.0, 500.0];
    let vals = [10.0, 20.0, 50.0, 100.0];
    let cases: [(&str, f64, f64); 6] = [
        ("exact", 100.0, 20.0),
        ("between", 150.0, 35.0),
        ("first", 0.0, 10.0),
        ("last", 500.0, 100.0),
        ("below", -10.0, 10.0),
        ("above", 900.0, 100.0),
    ];
    for (name, target, want) in cases {
        let got = lookup_1d_curve(&axis, &vals, target).expect("curve lookup failed");
        assert_eq!(bits(got), bits(want), "lookup_1d_curve {}", name);
    }
}

#[test]
fn parity_lookup_1d_curve_length_mismatch() {
    assert_eq!(
        lookup_1d_curve(&[0.0, 100.0, 200.0, 500.0], &[1.0, 2.0], 150.0),
        Err(InterpError::AxisValuesLengthMismatch)
    );
}

// ---------------------------------------------------------------------------
// Determinism
// ---------------------------------------------------------------------------

#[test]
fn all_interp_primitives_are_deterministic() {
    let (x, y, m) = axes();
    for _ in 0..3 {
        assert_eq!(
            lookup_2d_map(&x, &y, &m, 2750.0, 45.0),
            lookup_2d_map(&x, &y, &m, 2750.0, 45.0)
        );
        assert_eq!(
            catmull_rom_cubic_1d(1.0, 2.0, 3.0, 4.0, 0.3),
            catmull_rom_cubic_1d(1.0, 2.0, 3.0, 4.0, 0.3)
        );
        assert_eq!(
            kivi_defs::interp::bicubic_surface_16point(&patch_linear(), 0.25, 0.75),
            kivi_defs::interp::bicubic_surface_16point(&patch_linear(), 0.25, 0.75)
        );
    }
}
