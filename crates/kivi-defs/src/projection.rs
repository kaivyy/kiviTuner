//! 3D isometric table projection, jet colormap, and saturated arithmetic.
//!
//! Ported from `ecu_engine_rust::math::projection`.
//!
//! Pure maths: no I/O, no allocation in the hot path, deterministic.

/// A point in 3D Euclidean space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point3D {
    /// X coordinate.
    pub x: f64,
    /// Y coordinate.
    pub y: f64,
    /// Z coordinate.
    pub z: f64,
}

/// A point in 2D viewport coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenPoint {
    /// Screen X.
    pub x: f64,
    /// Screen Y (inverted relative to world Y).
    pub y: f64,
    /// Depth for painter's-algorithm sorting.
    pub depth: f64,
}

/// An RGB colour with components in `0.0..=1.0`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorRgb {
    /// Red.
    pub r: f64,
    /// Green.
    pub g: f64,
    /// Blue.
    pub b: f64,
}

/// Isometric 3D surface projector.
pub struct Table3DProjector {
    /// Pitch angle in radians (rotation about X).
    pub pitch_rad: f64,
    /// Yaw angle in radians (rotation about Z).
    pub yaw_rad: f64,
    /// Viewport zoom.
    pub scale: f64,
    /// Screen centre X.
    pub offset_x: f64,
    /// Screen centre Y.
    pub offset_y: f64,
}

impl Default for Table3DProjector {
    fn default() -> Self {
        Self {
            pitch_rad: 35.0f64.to_radians(),
            yaw_rad: 45.0f64.to_radians(),
            scale: 180.0,
            offset_x: 400.0,
            offset_y: 300.0,
        }
    }
}

impl Table3DProjector {
    /// Projects a normalised 3D point onto 2D viewport coordinates.
    pub fn project_point(&self, p: Point3D) -> ScreenPoint {
        let cos_yaw = self.yaw_rad.cos();
        let sin_yaw = self.yaw_rad.sin();
        let cos_pitch = self.pitch_rad.cos();
        let sin_pitch = self.pitch_rad.sin();

        // 1. Yaw rotation about Z.
        let x_yaw = p.x * cos_yaw - p.y * sin_yaw;
        let y_yaw = p.x * sin_yaw + p.y * cos_yaw;
        let z_yaw = p.z;

        // 2. Pitch rotation about X.
        let x_pitch = x_yaw;
        let y_pitch = y_yaw * cos_pitch - z_yaw * sin_pitch;
        let z_pitch = y_yaw * sin_pitch + z_yaw * cos_pitch;

        // 3. Axonometric screen projection; Y is inverted for screen space.
        ScreenPoint {
            x: self.offset_x + self.scale * x_pitch,
            y: self.offset_y - self.scale * y_pitch,
            depth: z_pitch,
        }
    }

    /// Maps a normalised elevation to a jet-colormap colour.
    ///
    /// `0.0` is blue, `0.5` green, `1.0` red. Input is clamped.
    pub fn elevation_to_rgb(t_normalized: f64) -> ColorRgb {
        let t = t_normalized.clamp(0.0, 1.0);
        let r = (1.5 - (4.0 * t - 3.0).abs()).clamp(0.0, 1.0);
        let g = (1.5 - (4.0 * t - 2.0).abs()).clamp(0.0, 1.0);
        let b = (1.5 - (4.0 * t - 1.0).abs()).clamp(0.0, 1.0);
        ColorRgb { r, g, b }
    }
}

/// Saturated arithmetic that never wraps on overflow or underflow.
pub struct SaturatedMath;

impl SaturatedMath {
    /// Saturated `u8` addition of a signed delta.
    #[inline]
    pub fn add_diff_u8(base: u8, delta: i32) -> u8 {
        (base as i32 + delta).clamp(0, 255) as u8
    }

    /// Saturated `u16` addition of a signed delta.
    #[inline]
    pub fn add_diff_u16(base: u16, delta: i32) -> u16 {
        (base as i32 + delta).clamp(0, 65535) as u16
    }

    /// Saturated `i16` addition of a signed delta.
    #[inline]
    pub fn add_diff_i16(base: i16, delta: i32) -> i16 {
        (base as i32 + delta).clamp(-32768, 32767) as i16
    }

    /// Percentage difference with a zero-denominator guard.
    ///
    /// When the original is (near) zero, returns `0.0` if both are zero,
    /// otherwise `+100.0` / `-100.0` by sign.
    #[inline]
    pub fn safe_percentage_difference(version_val: f64, original_val: f64) -> f64 {
        if original_val.abs() < 1e-9 {
            if version_val.abs() < 1e-9 {
                0.0
            } else if version_val > 0.0 {
                100.0
            } else {
                -100.0
            }
        } else {
            ((version_val - original_val) / original_val) * 100.0
        }
    }
}
