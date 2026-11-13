use serde::{Deserialize, Serialize};

/// 2D affine placement of an object on the stage.
///
/// `x`/`y` reference the object's center, in stage pixels. This makes
/// rotation and scaling behave the way users expect from a symbol pivot.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    pub x: f32,
    pub y: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    /// Degrees, clockwise (SWF stage coordinates point down).
    pub rotation: f32,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation: 0.0,
        }
    }
}

impl Transform {
    pub fn at(x: f32, y: f32) -> Self {
        Self {
            x,
            y,
            ..Default::default()
        }
    }

    pub fn with_rotation(mut self, degrees: f32) -> Self {
        self.rotation = degrees;
        self
    }

    pub fn lerp(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let mix = |a: f32, b: f32| a + (b - a) * t;
        Self {
            x: mix(self.x, other.x),
            y: mix(self.y, other.y),
            scale_x: mix(self.scale_x, other.scale_x),
            scale_y: mix(self.scale_y, other.scale_y),
            rotation: mix(self.rotation, other.rotation),
        }
    }

    /// Returns the affine matrix components `(a, b, c, d, tx, ty)`:
    /// `x' = a*x + c*y + tx`, `y' = b*x + d*y + ty`.
    pub fn to_matrix(self) -> [f32; 6] {
        let rad = self.rotation.to_radians();
        let (sin, cos) = rad.sin_cos();
        [
            cos * self.scale_x,
            sin * self.scale_x,
            -sin * self.scale_y,
            cos * self.scale_y,
            self.x,
            self.y,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translation_matrix() {
        let m = Transform::at(10.0, 20.0).to_matrix();
        assert_eq!(m[0], 1.0);
        assert_eq!(m[3], 1.0);
        assert_eq!(m[4], 10.0);
        assert_eq!(m[5], 20.0);
    }

    #[test]
    fn rotation_90_degrees() {
        let m = Transform::at(0.0, 0.0).with_rotation(90.0).to_matrix();
        assert!(m[0].abs() < 1e-6);
        assert!((m[1] - 1.0).abs() < 1e-6);
        assert!((m[2] + 1.0).abs() < 1e-6);
    }
}
