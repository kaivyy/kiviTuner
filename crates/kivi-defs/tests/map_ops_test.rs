use kivi_defs::map_ops::{
    apply_selection_edit, interpolate_matrix_region, revert_selection_to_original,
    solve_linear_factor_offset, solve_reciprocal_conversion, CellSelection, MapOpsError,
    RegionInterpAxes, RegionInterpMode, RegionInterpOptions, SelectionEditOp,
};

#[test]
fn selection_edits_and_revert_preserve_outside_cells() {
    let orig = vec![
        vec![10.0, 20.0, 30.0],
        vec![40.0, 50.0, 60.0],
        vec![70.0, 80.0, 90.0],
    ];
    let sel = CellSelection {
        row_start: 1,
        row_end: 2,
        col_start: 0,
        col_end: 1,
    };

    let added = apply_selection_edit(&orig, sel, SelectionEditOp::AddDelta(5.0)).expect("add");
    assert_eq!(added[0], vec![10.0, 20.0, 30.0]);
    assert_eq!(added[1], vec![45.0, 55.0, 60.0]);
    assert_eq!(added[2], vec![75.0, 85.0, 90.0]);

    let scaled =
        apply_selection_edit(&added, sel, SelectionEditOp::ScalePercent(10.0)).expect("scale");
    assert!((scaled[1][0] - 49.5).abs() < 1e-9);
    assert!((scaled[2][1] - 93.5).abs() < 1e-9);

    let clamped = apply_selection_edit(
        &scaled,
        sel,
        SelectionEditOp::ClampRange { min: 50.0, max: 80.0 },
    )
    .expect("clamp");
    assert_eq!(clamped[1][0], 50.0);
    assert_eq!(clamped[2][1], 80.0);

    let abs_set =
        apply_selection_edit(&clamped, sel, SelectionEditOp::SetAbsolute(123.0)).expect("set");
    assert_eq!(abs_set[1][1], 123.0);
    assert_eq!(abs_set[0][2], 30.0);

    let reverted = revert_selection_to_original(&abs_set, &orig, sel).expect("revert");
    assert_eq!(reverted, orig);
}

#[test]
fn linear_and_cubic_region_interpolation_and_fallbacks_work() {
    let mut grid = vec![
        vec![10.0, 20.0, 30.0, 40.0, 50.0, 60.0],
        vec![10.0, 20.0, 30.0, 40.0, 50.0, 60.0],
        vec![10.0, 20.0, 999.0, 999.0, 50.0, 60.0],
        vec![10.0, 20.0, 999.0, 999.0, 50.0, 60.0],
        vec![10.0, 20.0, 30.0, 40.0, 50.0, 60.0],
        vec![10.0, 20.0, 30.0, 40.0, 50.0, 60.0],
    ];

    let sel = CellSelection {
        row_start: 2,
        row_end: 3,
        col_start: 2,
        col_end: 3,
    };

    let lin_h = interpolate_matrix_region(
        &grid,
        sel,
        RegionInterpOptions {
            mode: RegionInterpMode::Linear,
            axes: RegionInterpAxes::HorizontalOnly,
            allow_extrapolation: false,
            fallback_all_sides: false,
        },
    )
    .expect("linear h");
    assert_eq!(lin_h.applied_mode, RegionInterpMode::Linear);
    assert!((lin_h.matrix[2][2] - 30.0).abs() < 1e-9);
    assert!((lin_h.matrix[2][3] - 40.0).abs() < 1e-9);

    let cubic_h = interpolate_matrix_region(
        &grid,
        sel,
        RegionInterpOptions {
            mode: RegionInterpMode::CubicCatmullRom,
            axes: RegionInterpAxes::HorizontalOnly,
            allow_extrapolation: false,
            fallback_all_sides: false,
        },
    )
    .expect("cubic h");
    assert_eq!(cubic_h.applied_mode, RegionInterpMode::CubicCatmullRom);
    assert!((cubic_h.matrix[2][2] - 30.0).abs() < 2.5);
    assert!((cubic_h.matrix[2][3] - 40.0).abs() < 2.5);

    // Put selection on top border so VerticalOnly must fall back to HorizontalOnly.
    grid[0][2] = 0.0;
    grid[0][3] = 0.0;
    let border_sel = CellSelection {
        row_start: 0,
        row_end: 0,
        col_start: 2,
        col_end: 3,
    };
    let fb = interpolate_matrix_region(
        &grid,
        border_sel,
        RegionInterpOptions {
            mode: RegionInterpMode::Linear,
            axes: RegionInterpAxes::VerticalOnly,
            allow_extrapolation: false,
            fallback_all_sides: true,
        },
    )
    .expect("fallback");
    assert_eq!(fb.applied_axes, RegionInterpAxes::HorizontalOnly);
    assert!((fb.matrix[0][2] - 30.0).abs() < 1e-9);

    let no_fb = interpolate_matrix_region(
        &grid,
        border_sel,
        RegionInterpOptions {
            mode: RegionInterpMode::Linear,
            axes: RegionInterpAxes::VerticalOnly,
            allow_extrapolation: false,
            fallback_all_sides: false,
        },
    );
    assert_eq!(no_fb.unwrap_err(), MapOpsError::InsufficientBorder);
}

#[test]
fn edge_extrapolation_extends_from_two_outside_cells() {
    let grid = vec![
        vec![0.0, 0.0, 30.0, 40.0, 50.0],
        vec![0.0, 0.0, 60.0, 80.0, 100.0],
    ];
    let left_edge = CellSelection {
        row_start: 0,
        row_end: 1,
        col_start: 0,
        col_end: 1,
    };
    let res = interpolate_matrix_region(
        &grid,
        left_edge,
        RegionInterpOptions {
            mode: RegionInterpMode::CubicCatmullRom,
            axes: RegionInterpAxes::HorizontalOnly,
            allow_extrapolation: true,
            fallback_all_sides: false,
        },
    )
    .expect("extrapolate left");
    assert!(res.used_extrapolation);
    assert_eq!(res.applied_mode, RegionInterpMode::Linear);
    assert!((res.matrix[0][1] - 20.0).abs() < 1e-9);
    assert!((res.matrix[0][0] - 10.0).abs() < 1e-9);
    assert!((res.matrix[1][1] - 40.0).abs() < 1e-9);
    assert!((res.matrix[1][0] - 20.0).abs() < 1e-9);
}

#[test]
fn factor_offset_solvers_cover_linear_reciprocal_and_swapped_inputs() {
    // phys = 0.75 * raw - 22.5
    let sol = solve_linear_factor_offset((30.0, 0.0), (70.0, 30.0), false).expect("linear");
    assert!(!sol.reciprocal);
    assert!((sol.factor - 0.75).abs() < 1e-9);
    assert!((sol.offset - (-22.5)).abs() < 1e-9);
    for (raw, phys) in [(30.0, 0.0), (70.0, 30.0)] {
        assert!((sol.factor * raw + sol.offset - phys).abs() < 1e-9);
    }

    let swapped =
        solve_linear_factor_offset((0.0, 30.0), (30.0, 70.0), true).expect("swapped linear");
    assert!((swapped.factor - 0.75).abs() < 1e-9);
    assert!((swapped.offset - (-22.5)).abs() < 1e-9);

    // y = 10_000_000 / (X + 50) + 10
    let a_true = 10_000_000.0;
    let c_true = 50.0;
    let d_true = 10.0;
    let eval_true = |x: f64| a_true / (x + c_true) + d_true;
    let pts = [(100.0, eval_true(100.0)), (200.0, eval_true(200.0)), (450.0, eval_true(450.0))];
    let rec = solve_reciprocal_conversion(pts[0], pts[1], pts[2], false).expect("reciprocal");
    assert!(rec.reciprocal);
    assert!((rec.reciprocal_num - a_true).abs() < 1e-4);
    assert!((rec.reciprocal_den_offset - c_true).abs() < 1e-6);
    assert!((rec.offset - d_true).abs() < 1e-6);
    for &(x, y) in &pts {
        let pred = rec.reciprocal_num / (x + rec.reciprocal_den_offset) + rec.offset;
        assert!((pred - y).abs() < 1e-9);
    }

    // Collinear inputs must be rejected by reciprocal solver.
    let collinear = solve_reciprocal_conversion((1.0, 2.0), (2.0, 4.0), (3.0, 6.0), false);
    assert!(matches!(collinear, Err(MapOpsError::DegenerateSolverInput(_))));
}
