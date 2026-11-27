//! Parity tests for 3D projection and saturated arithmetic.
//!
//! Ported from `ecu_engine_rust::math::projection`.
//!
//! ## How expected values were obtained
//!
//! Rotation maths is hand-derived: at yaw = 0 and pitch = 0 the projection is a
//! pure scale plus offset. Colormap endpoints are evaluated from the formula
//! `r = clamp(1.5 - |4t - 3|)` and so on. Saturated arithmetic is checked at the
//! exact clamp boundaries.

use kivi_defs::projection::{
    ColorRgb, Point3D, SaturatedMath, ScreenPoint, Table3DProjector,
};

// ---------------------------------------------------------------------------
// Projection
// ---------------------------------------------------------------------------

#[test]
fn default_projector_matches_the_source_angles() {
    let p = Table3DProjector::default();
    assert!((p.pitch_rad - 35.0f64.to_radians()).abs() < 1e-12);
    assert!((p.yaw_rad - 45.0f64.to_radians()).abs() < 1e-12);
    assert_eq!(p.scale, 180.0);
    assert_eq!(p.offset_x, 400.0);
    assert_eq!(p.offset_y, 300.0);
}

#[test]
fn the_origin_projects_to_the_screen_centre() {
    let p = Table3DProjector::default();
    let s = p.project_point(Point3D { x: 0.0, y: 0.0, z: 0.0 });
    assert!((s.x - 400.0).abs() < 1e-12);
    assert!((s.y - 300.0).abs() < 1e-12);
    assert!(s.depth.abs() < 1e-12);
}

#[test]
fn a_zero_angle_projector_is_pure_scale_and_offset() {
    let p = Table3DProjector {
        pitch_rad: 0.0,
        yaw_rad: 0.0,
        scale: 100.0,
        offset_x: 0.0,
        offset_y: 0.0,
    };
    let s = p.project_point(Point3D { x: 1.0, y: 2.0, z: 3.0 });
    // x_yaw = 1, y_yaw = 2; pitch 0 -> y_pitch = 2, z_pitch = 3
    assert!((s.x - 100.0).abs() < 1e-12);
    // screen_y = offset_y - scale * y_pitch = -200
    assert!((s.y + 200.0).abs() < 1e-12);
    assert!((s.depth - 3.0).abs() < 1e-12);
}

#[test]
fn the_y_axis_is_inverted_for_screen_coordinates() {
    // Positive y must move the point *up* the screen (smaller screen y).
    let p = Table3DProjector {
        pitch_rad: 0.0,
        yaw_rad: 0.0,
        scale: 10.0,
        offset_x: 0.0,
        offset_y: 100.0,
    };
    let up = p.project_point(Point3D { x: 0.0, y: 1.0, z: 0.0 });
    let down = p.project_point(Point3D { x: 0.0, y: -1.0, z: 0.0 });
    assert!(up.y < down.y, "positive y must project higher on screen");
}

#[test]
fn yaw_rotation_at_90_degrees_swaps_x_into_y() {
    let p = Table3DProjector {
        pitch_rad: 0.0,
        yaw_rad: std::f64::consts::FRAC_PI_2,
        scale: 1.0,
        offset_x: 0.0,
        offset_y: 0.0,
    };
    let s = p.project_point(Point3D { x: 1.0, y: 0.0, z: 0.0 });
    // cos90=0, sin90=1 -> x_yaw = -0? (1*0 - 0*1)=0 ; y_yaw = 1*1 + 0*0 = 1
    assert!(s.x.abs() < 1e-12, "x should collapse to zero, got {}", s.x);
    assert!((s.y + 1.0).abs() < 1e-12, "y should be -1, got {}", s.y);
}

#[test]
fn projection_is_deterministic() {
    let p = Table3DProjector::default();
    let pt = Point3D { x: 0.3, y: -0.7, z: 0.5 };
    assert_eq!(p.project_point(pt), p.project_point(pt));
}

// ---------------------------------------------------------------------------
// Colormap
// ---------------------------------------------------------------------------

#[test]
fn colormap_at_zero_is_blue() {
    // t=0: r = clamp(1.5-3)=0, g = clamp(1.5-2)=0, b = clamp(1.5-1)=0.5
    let c = Table3DProjector::elevation_to_rgb(0.0);
    assert_eq!(c, ColorRgb { r: 0.0, g: 0.0, b: 0.5 });
}

#[test]
fn colormap_at_one_is_red() {
    // t=1: r = clamp(1.5-1)=0.5, g = clamp(1.5-2)=0, b = clamp(1.5-3)=0
    let c = Table3DProjector::elevation_to_rgb(1.0);
    assert_eq!(c, ColorRgb { r: 0.5, g: 0.0, b: 0.0 });
}

#[test]
fn colormap_at_the_midpoint_is_green() {
    // t=0.5: r = clamp(1.5-1)=0.5? -> 4*0.5-3 = -1 -> |.|=1 -> 0.5
    //        g = 4*0.5-2 = 0 -> 1.5 -> clamped 1.0
    //        b = 4*0.5-1 = 1 -> 0.5
    let c = Table3DProjector::elevation_to_rgb(0.5);
    assert_eq!(c, ColorRgb { r: 0.5, g: 1.0, b: 0.5 });
}

#[test]
fn colormap_clamps_out_of_range_input() {
    assert_eq!(
        Table3DProjector::elevation_to_rgb(-5.0),
        Table3DProjector::elevation_to_rgb(0.0)
    );
    assert_eq!(
        Table3DProjector::elevation_to_rgb(5.0),
        Table3DProjector::elevation_to_rgb(1.0)
    );
}

#[test]
fn colormap_components_stay_within_unit_range() {
    for i in 0..=100 {
        let t = i as f64 / 100.0;
        let c = Table3DProjector::elevation_to_rgb(t);
        assert!((0.0..=1.0).contains(&c.r));
        assert!((0.0..=1.0).contains(&c.g));
        assert!((0.0..=1.0).contains(&c.b));
    }
}

// ---------------------------------------------------------------------------
// Saturated arithmetic
// ---------------------------------------------------------------------------

#[test]
fn add_diff_u8_clamps_at_both_ends() {
    assert_eq!(SaturatedMath::add_diff_u8(250, 20), 255);
    assert_eq!(SaturatedMath::add_diff_u8(10, -30), 0);
    assert_eq!(SaturatedMath::add_diff_u8(100, 0), 100);
    assert_eq!(SaturatedMath::add_diff_u8(0, 0), 0);
    assert_eq!(SaturatedMath::add_diff_u8(255, 0), 255);
}

#[test]
fn add_diff_u16_clamps_at_both_ends() {
    assert_eq!(SaturatedMath::add_diff_u16(65530, 100), 65535);
    assert_eq!(SaturatedMath::add_diff_u16(10, -100), 0);
    assert_eq!(SaturatedMath::add_diff_u16(1000, 500), 1500);
}

#[test]
fn add_diff_i16_clamps_at_both_ends() {
    assert_eq!(SaturatedMath::add_diff_i16(32760, 100), 32767);
    assert_eq!(SaturatedMath::add_diff_i16(-32760, -100), -32768);
    assert_eq!(SaturatedMath::add_diff_i16(0, 100), 100);
}

#[test]
fn safe_percentage_difference_guards_zero_denominator() {
    // original 0, version 0 -> 0
    assert_eq!(SaturatedMath::safe_percentage_difference(0.0, 0.0), 0.0);
    // original 0, version positive -> +100
    assert_eq!(SaturatedMath::safe_percentage_difference(5.0, 0.0), 100.0);
    // original 0, version negative -> -100
    assert_eq!(SaturatedMath::safe_percentage_difference(-5.0, 0.0), -100.0);
}

#[test]
fn safe_percentage_difference_computes_the_ratio() {
    // (110 - 100)/100 * 100 = 10
    assert!((SaturatedMath::safe_percentage_difference(110.0, 100.0) - 10.0).abs() < 1e-12);
    // (50 - 100)/100 * 100 = -50
    assert!((SaturatedMath::safe_percentage_difference(50.0, 100.0) + 50.0).abs() < 1e-12);
}

#[test]
fn a_screen_point_carries_depth_for_painter_sorting() {
    let s = ScreenPoint { x: 1.0, y: 2.0, depth: 3.0 };
    assert_eq!(s.depth, 3.0);
}
