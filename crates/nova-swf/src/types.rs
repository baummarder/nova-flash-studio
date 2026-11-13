//! Small primitive types shared by the tag and shape encoders.

use crate::bits::{BitWriter, signed_bits};

/// RGBA color; SWF stores the components in R, G, B, A order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const WHITE: Self = Self::rgb(255, 255, 255);
    pub const BLACK: Self = Self::rgb(0, 0, 0);

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub fn to_bytes(self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

/// Rectangle in twips (1/20 px), used for stage and shape bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect {
    pub x_min: i32,
    pub x_max: i32,
    pub y_min: i32,
    pub y_max: i32,
}

impl Rect {
    pub fn new(x_min: i32, x_max: i32, y_min: i32, y_max: i32) -> Self {
        Self {
            x_min,
            x_max,
            y_min,
            y_max,
        }
    }

    pub fn from_size(width: f32, height: f32) -> Self {
        let w = to_twips(width);
        let h = to_twips(height);
        Self::new(0, w, 0, h)
    }

    /// Centered rectangle in local shape coordinates.
    pub fn centered(width: f32, height: f32) -> Self {
        let hw = to_twips(width / 2.0);
        let hh = to_twips(height / 2.0);
        Self::new(-hw, hw, -hh, hh)
    }

    pub fn width_px(self) -> f32 {
        (self.x_max - self.x_min) as f32 / 20.0
    }

    pub fn height_px(self) -> f32 {
        (self.y_max - self.y_min) as f32 / 20.0
    }

    /// Writes the SWF RECT bit field.
    pub fn write(self, w: &mut BitWriter) {
        let values = [self.x_min, self.x_max, self.y_min, self.y_max];
        let bits = values
            .iter()
            .map(|v| signed_bits(*v))
            .max()
            .unwrap_or(1)
            .clamp(1, 31);
        w.write_bits(bits as u32, 5);
        for value in values {
            w.write_sbits(value, bits);
        }
    }
}

pub fn to_twips(px: f32) -> i32 {
    (px * 20.0).round() as i32
}

/// SWF MATRIX, expressed in pixels for translation and unit scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Matrix {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub tx: f32,
    pub ty: f32,
}

impl Default for Matrix {
    fn default() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            tx: 0.0,
            ty: 0.0,
        }
    }
}

impl Matrix {
    /// Builds the SWF matrix from center-based scale/rotation/translation.
    pub fn from_transform(scale_x: f32, scale_y: f32, rotation_deg: f32, x: f32, y: f32) -> Self {
        let (sin, cos) = rotation_deg.to_radians().sin_cos();
        Self {
            a: cos * scale_x,
            b: sin * scale_x,
            c: -sin * scale_y,
            d: cos * scale_y,
            tx: x,
            ty: y,
        }
    }

    pub fn write(self, w: &mut BitWriter) {
        let fixed = |v: f32| (v * 65536.0).round() as i32;
        let (a, b, c, d) = (fixed(self.a), fixed(self.b), fixed(self.c), fixed(self.d));
        let has_scale = a != 65536 || d != 65536;
        let has_rotate = b != 0 || c != 0;

        w.write_bit(has_scale);
        if has_scale {
            let bits = signed_bits(a).max(signed_bits(d)).clamp(1, 31);
            w.write_bits(bits as u32, 5);
            w.write_sbits(a, bits);
            w.write_sbits(d, bits);
        }

        w.write_bit(has_rotate);
        if has_rotate {
            let bits = signed_bits(b).max(signed_bits(c)).clamp(1, 31);
            w.write_bits(bits as u32, 5);
            w.write_sbits(b, bits);
            w.write_sbits(c, bits);
        }

        let tx = to_twips(self.tx);
        let ty = to_twips(self.ty);
        let bits = signed_bits(tx).max(signed_bits(ty)).clamp(1, 31);
        w.write_bits(bits as u32, 5);
        w.write_sbits(tx, bits);
        w.write_sbits(ty, bits);
    }
}

/// Simple color transform (`out = in * mult + add`), components in 0..=255.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorTransform {
    pub red_mult: f32,
    pub green_mult: f32,
    pub blue_mult: f32,
    pub alpha_mult: f32,
    pub red_add: i16,
    pub green_add: i16,
    pub blue_add: i16,
    pub alpha_add: i16,
}

impl ColorTransform {
    pub const IDENTITY: Self = Self {
        red_mult: 1.0,
        green_mult: 1.0,
        blue_mult: 1.0,
        alpha_mult: 1.0,
        red_add: 0,
        green_add: 0,
        blue_add: 0,
        alpha_add: 0,
    };

    pub fn is_identity(self) -> bool {
        self == Self::IDENTITY
    }

    pub fn write(self, w: &mut BitWriter) {
        let has_add =
            self.red_add != 0 || self.green_add != 0 || self.blue_add != 0 || self.alpha_add != 0;
        let has_mult = self.red_mult != 1.0
            || self.green_mult != 1.0
            || self.blue_mult != 1.0
            || self.alpha_mult != 1.0;
        w.write_bit(has_add);
        w.write_bit(has_mult);

        let mults = [
            self.red_mult,
            self.green_mult,
            self.blue_mult,
            self.alpha_mult,
        ];
        let mult_bits = mults
            .iter()
            .map(|m| signed_bits((m * 256.0).round() as i32))
            .max()
            .unwrap_or(1)
            .clamp(1, 15);
        let adds = [self.red_add, self.green_add, self.blue_add, self.alpha_add];
        let add_bits = adds
            .iter()
            .map(|a| signed_bits(*a as i32))
            .max()
            .unwrap_or(1)
            .clamp(1, 15);

        if has_mult {
            w.write_bits(mult_bits as u32, 4);
            for m in mults {
                w.write_sbits((m * 256.0).round() as i32, mult_bits);
            }
        }
        if has_add {
            w.write_bits(add_bits as u32, 4);
            for a in adds {
                w.write_sbits(a as i32, add_bits);
            }
        }
    }
}

/// A `PlaceObject2` tag payload.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaceObject {
    pub depth: u16,
    pub character_id: Option<u16>,
    pub matrix: Option<Matrix>,
    pub name: Option<String>,
    pub ratio: Option<u16>,
    pub color_transform: Option<ColorTransform>,
    pub clip_depth: Option<u16>,
    pub move_existing: bool,
}

impl PlaceObject {
    pub fn new(depth: u16) -> Self {
        Self {
            depth,
            character_id: None,
            matrix: None,
            name: None,
            ratio: None,
            color_transform: None,
            clip_depth: None,
            move_existing: false,
        }
    }

    pub fn character(mut self, id: u16) -> Self {
        self.character_id = Some(id);
        self
    }

    pub fn matrix(mut self, matrix: Matrix) -> Self {
        self.matrix = Some(matrix);
        self
    }

    pub fn named(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn moved(mut self) -> Self {
        self.move_existing = true;
        self
    }

    pub fn write(&self, w: &mut BitWriter) {
        let flags = u8::from(self.move_existing)
            | (u8::from(self.character_id.is_some()) << 1)
            | (u8::from(self.matrix.is_some()) << 2)
            | (u8::from(self.color_transform.is_some()) << 3)
            | (u8::from(self.ratio.is_some()) << 4)
            | (u8::from(self.name.is_some()) << 5)
            | (u8::from(self.clip_depth.is_some()) << 6);
        w.write_bytes(&[flags]);
        w.write_bytes(&self.depth.to_le_bytes());
        if let Some(id) = self.character_id {
            w.write_bytes(&id.to_le_bytes());
        }
        if let Some(matrix) = self.matrix {
            matrix.write(w);
            w.align();
        }
        if let Some(name) = &self.name {
            w.align();
            w.write_bytes(name.as_bytes());
            w.write_bytes(&[0]);
        }
        if let Some(ratio) = self.ratio {
            w.align();
            w.write_bytes(&ratio.to_le_bytes());
        }
        if let Some(ct) = self.color_transform {
            ct.write(w);
            w.align();
        }
        if let Some(depth) = self.clip_depth {
            w.align();
            w.write_bytes(&depth.to_le_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centered_rect_uses_twips() {
        let r = Rect::centered(100.0, 50.0);
        assert_eq!(r, Rect::new(-1000, 1000, -500, 500));
        assert_eq!(r.width_px(), 100.0);
    }

    #[test]
    fn matrix_encodes_identity_compactly() {
        let mut w = BitWriter::new();
        Matrix::default().write(&mut w);
        let bytes = w.into_bytes();
        // HasScale=0, HasRotate=0, 5 bits translate size, then 2x 1-bit zeros
        // plus padding. No scale/rotate payload for an identity matrix.
        assert_eq!(bytes.len(), 2);
        assert_eq!(bytes[0] & 0b1000_0000, 0);
    }
}
