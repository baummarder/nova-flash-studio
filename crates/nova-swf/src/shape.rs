//! `DefineShape3` geometry encoding.
//!
//! The encoder is intentionally small and explicit instead of hiding the SWF
//! bit layout behind a large abstraction: edge records, style changes and the
//! 4-bit style index widths are the parts most likely to break compatibility,
//! so they are visible and unit-tested here.

use crate::bits::{BitWriter, signed_bits, unsigned_bits};
use crate::error::{SwfError, SwfResult};
use crate::types::{Color, Rect, to_twips};

/// Point in twips.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Command {
    MoveTo(Point),
    LineTo(Point),
    CurveTo { control: Point, to: Point },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShapePath {
    /// 1-based fill style index, `None` for no fill.
    pub fill: Option<u16>,
    /// 1-based line style index, `None` for no stroke.
    pub line: Option<u16>,
    pub commands: Vec<Command>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineStyle {
    pub width: u16,
    pub color: Color,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    pub bounds: Rect,
    pub fills: Vec<Color>,
    pub lines: Vec<LineStyle>,
    pub paths: Vec<ShapePath>,
}

impl Shape {
    /// Axis-aligned rectangle centered on the local origin.
    pub fn rect(width_px: f32, height_px: f32, fill: Color) -> Self {
        Self::rounded_rect(width_px, height_px, 0.0, fill)
    }

    /// Rectangle with quadratic rounded corners, centered on the local origin.
    pub fn rounded_rect(width_px: f32, height_px: f32, radius_px: f32, fill: Color) -> Self {
        let (hw, hh) = (to_twips(width_px / 2.0), to_twips(height_px / 2.0));
        let r = to_twips(radius_px).min(hw.abs()).min(hh.abs()).max(0);
        let mut commands = Vec::new();
        if r == 0 {
            commands.push(Command::MoveTo(Point::new(-hw, -hh)));
            commands.push(Command::LineTo(Point::new(hw, -hh)));
            commands.push(Command::LineTo(Point::new(hw, hh)));
            commands.push(Command::LineTo(Point::new(-hw, hh)));
            commands.push(Command::LineTo(Point::new(-hw, -hh)));
        } else {
            commands.push(Command::MoveTo(Point::new(-hw + r, -hh)));
            commands.push(Command::LineTo(Point::new(hw - r, -hh)));
            commands.push(Command::CurveTo {
                control: Point::new(hw, -hh),
                to: Point::new(hw, -hh + r),
            });
            commands.push(Command::LineTo(Point::new(hw, hh - r)));
            commands.push(Command::CurveTo {
                control: Point::new(hw, hh),
                to: Point::new(hw - r, hh),
            });
            commands.push(Command::LineTo(Point::new(-hw + r, hh)));
            commands.push(Command::CurveTo {
                control: Point::new(-hw, hh),
                to: Point::new(-hw, hh - r),
            });
            commands.push(Command::LineTo(Point::new(-hw, -hh + r)));
            commands.push(Command::CurveTo {
                control: Point::new(-hw, -hh),
                to: Point::new(-hw + r, -hh),
            });
        }
        Self {
            bounds: Rect::new(-hw, hw, -hh, hh),
            fills: vec![fill],
            lines: Vec::new(),
            paths: vec![ShapePath {
                fill: Some(1),
                line: None,
                commands,
            }],
        }
    }

    /// Ellipse centered on the local origin, approximated by eight quadratic
    /// segments (the same technique the Flash authoring tool used).
    pub fn ellipse(width_px: f32, height_px: f32, fill: Color) -> Self {
        let rx = to_twips(width_px / 2.0).max(1);
        let ry = to_twips(height_px / 2.0).max(1);
        let segments = 8;
        let step = std::f64::consts::TAU / segments as f64;
        let control_scale = 1.0 / (step / 2.0).cos();
        let point_at = |angle: f64| {
            Point::new(
                (rx as f64 * angle.cos()).round() as i32,
                (ry as f64 * angle.sin()).round() as i32,
            )
        };
        let mut commands = vec![Command::MoveTo(point_at(0.0))];
        for i in 0..segments {
            let a0 = step * i as f64;
            let a1 = step * (i + 1) as f64;
            let mid = (a0 + a1) / 2.0;
            let control = Point::new(
                (rx as f64 * mid.cos() * control_scale).round() as i32,
                (ry as f64 * mid.sin() * control_scale).round() as i32,
            );
            commands.push(Command::CurveTo {
                control,
                to: point_at(a1),
            });
        }
        Self {
            bounds: Rect::centered(width_px, height_px),
            fills: vec![fill],
            lines: Vec::new(),
            paths: vec![ShapePath {
                fill: Some(1),
                line: None,
                commands,
            }],
        }
    }

    /// Serializes the `DefineShape3` body (character id + shape records).
    pub fn write_define_shape3(&self, id: u16, out: &mut Vec<u8>) -> SwfResult<()> {
        if self.fills.len() > 32767 {
            return Err(SwfError::TooManyFills(self.fills.len()));
        }
        if self.lines.len() > 32767 {
            return Err(SwfError::TooManyLines(self.lines.len()));
        }
        for path in &self.paths {
            if let Some(index) = path.fill {
                if index == 0 || index as usize > self.fills.len() {
                    return Err(SwfError::Range(format!(
                        "fill style index {index} out of range ({} fills)",
                        self.fills.len()
                    )));
                }
            }
            if let Some(index) = path.line {
                if index == 0 || index as usize > self.lines.len() {
                    return Err(SwfError::Range(format!(
                        "line style index {index} out of range ({} lines)",
                        self.lines.len()
                    )));
                }
            }
        }

        out.extend_from_slice(&id.to_le_bytes());
        let mut w = BitWriter::new();
        self.bounds.write(&mut w);
        w.align();
        write_fill_array(&mut w, &self.fills);
        write_line_array(&mut w, &self.lines);

        let fill_bits = if self.fills.is_empty() {
            0
        } else {
            unsigned_bits(self.fills.len() as u32)
        };
        let line_bits = if self.lines.is_empty() {
            0
        } else {
            unsigned_bits(self.lines.len() as u32)
        };
        w.write_bits(fill_bits as u32, 4);
        w.write_bits(line_bits as u32, 4);

        let edge_bits = required_edge_bits(self);
        let mut current = Point::default();

        for path in &self.paths {
            let mut iter = path.commands.iter();
            let start = match iter.next() {
                Some(Command::MoveTo(p)) => *p,
                Some(Command::LineTo(p)) => *p,
                Some(Command::CurveTo { to, .. }) => *to,
                None => continue,
            };
            write_style_change(
                &mut w,
                start,
                &mut current,
                path.fill,
                path.line,
                fill_bits,
                line_bits,
            );
            for command in iter {
                match *command {
                    Command::MoveTo(p) => {
                        write_style_change(
                            &mut w,
                            p,
                            &mut current,
                            path.fill,
                            path.line,
                            fill_bits,
                            line_bits,
                        );
                    }
                    Command::LineTo(p) => emit_line(&mut w, &mut current, p, edge_bits),
                    Command::CurveTo { control, to } => {
                        emit_curve(&mut w, &mut current, control, to, edge_bits, 0)
                    }
                }
            }
        }

        // EndShapeRecord: six zero bits.
        w.write_bits(0, 6);
        w.align();
        out.extend_from_slice(&w.into_bytes());
        Ok(())
    }
}

fn write_fill_array(w: &mut BitWriter, fills: &[Color]) {
    if fills.len() >= 0xff {
        w.write_bytes(&[0xff]);
        w.write_bytes(&(fills.len() as u16).to_le_bytes());
    } else {
        w.write_bytes(&[fills.len() as u8]);
    }
    for fill in fills {
        // FILLSTYLE: type 0x00 = solid, then RGBA.
        w.write_bytes(&[0x00]);
        w.write_bytes(&fill.to_bytes());
    }
}

fn write_line_array(w: &mut BitWriter, lines: &[LineStyle]) {
    if lines.len() >= 0xff {
        w.write_bytes(&[0xff]);
        w.write_bytes(&(lines.len() as u16).to_le_bytes());
    } else {
        w.write_bytes(&[lines.len() as u8]);
    }
    for line in lines {
        w.write_bytes(&line.width.to_le_bytes());
        w.write_bytes(&line.color.to_bytes());
    }
}

fn style_flags(move_to: bool, fill0: bool, fill1: bool, line: bool) -> u32 {
    (u32::from(line) << 3) | (u32::from(fill1) << 2) | (u32::from(fill0) << 1) | u32::from(move_to)
}

fn write_style_change(
    w: &mut BitWriter,
    target: Point,
    current: &mut Point,
    fill: Option<u16>,
    line: Option<u16>,
    fill_bits: u8,
    line_bits: u8,
) {
    let flags = style_flags(true, fill.is_some(), false, line.is_some());
    w.write_bit(false);
    w.write_bits(flags, 5);

    let dx = target.x - current.x;
    let dy = target.y - current.y;
    let move_bits = signed_bits(dx).max(signed_bits(dy)).clamp(1, 31);
    w.write_bits(move_bits as u32, 5);
    w.write_sbits(dx, move_bits);
    w.write_sbits(dy, move_bits);
    *current = target;

    if let Some(index) = fill {
        w.write_bits(index as u32, fill_bits.max(1));
    }
    if let Some(index) = line {
        w.write_bits(index as u32, line_bits.max(1));
    }
}

fn required_edge_bits(shape: &Shape) -> u8 {
    let mut bits = 1u8;
    for path in &shape.paths {
        let mut current = Point::default();
        for command in &path.commands {
            match *command {
                Command::MoveTo(p) => current = p,
                Command::LineTo(p) => {
                    bits = bits.max(signed_bits(p.x - current.x));
                    bits = bits.max(signed_bits(p.y - current.y));
                    current = p;
                }
                Command::CurveTo { control, to } => {
                    bits = bits.max(signed_bits(control.x - current.x));
                    bits = bits.max(signed_bits(control.y - current.y));
                    bits = bits.max(signed_bits(to.x - control.x));
                    bits = bits.max(signed_bits(to.y - control.y));
                    current = to;
                }
            }
        }
    }
    bits.clamp(2, 17)
}

fn emit_line(w: &mut BitWriter, current: &mut Point, to: Point, bits: u8) {
    let dx = to.x - current.x;
    let dy = to.y - current.y;
    if dx == 0 && dy == 0 {
        return;
    }
    let limit = (1i64 << (bits as i64 - 1)) - 1;
    let steps_x = (dx.unsigned_abs() as i64 + limit - 1) / limit;
    let steps_y = (dy.unsigned_abs() as i64 + limit - 1) / limit;
    let steps = steps_x.max(steps_y).max(1);

    let start = *current;
    let mut previous = start;
    for step in 1..=steps {
        let target = if step == steps {
            to
        } else {
            Point::new(
                start.x + ((dx as i64 * step) as f64 / steps as f64).round() as i32,
                start.y + ((dy as i64 * step) as f64 / steps as f64).round() as i32,
            )
        };
        write_straight_edge(w, previous, target, bits);
        previous = target;
    }
    *current = to;
}

fn write_straight_edge(w: &mut BitWriter, from: Point, to: Point, bits: u8) {
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    if dx == 0 && dy == 0 {
        return;
    }
    w.write_bit(true);
    w.write_bit(true);
    // SWF stores NumBits as "actual bits - 2" in the 4-bit edge field.
    w.write_bits((bits - 2) as u32, 4);
    if dx != 0 && dy != 0 {
        w.write_bit(true); // general line
        w.write_sbits(dx, bits);
        w.write_sbits(dy, bits);
    } else if dy == 0 {
        w.write_bit(false);
        w.write_bit(false); // horizontal
        w.write_sbits(dx, bits);
    } else {
        w.write_bit(false);
        w.write_bit(true); // vertical
        w.write_sbits(dy, bits);
    }
}

fn emit_curve(
    w: &mut BitWriter,
    current: &mut Point,
    control: Point,
    to: Point,
    bits: u8,
    depth: u32,
) {
    let d1x = control.x - current.x;
    let d1y = control.y - current.y;
    let d2x = to.x - control.x;
    let d2y = to.y - control.y;
    let fits = signed_bits(d1x) <= bits
        && signed_bits(d1y) <= bits
        && signed_bits(d2x) <= bits
        && signed_bits(d2y) <= bits;

    if fits {
        w.write_bit(true);
        w.write_bit(false); // curved edge
        w.write_bits((bits - 2) as u32, 4);
        w.write_sbits(d1x, bits);
        w.write_sbits(d1y, bits);
        w.write_sbits(d2x, bits);
        w.write_sbits(d2y, bits);
        *current = to;
        return;
    }
    if depth >= 12 {
        // Extremely large curve; fall back to a line to stay valid.
        emit_line(w, current, to, bits);
        return;
    }
    // de Casteljau split at t = 0.5
    let p01 = Point::new((current.x + control.x) / 2, (current.y + control.y) / 2);
    let p12 = Point::new((control.x + to.x) / 2, (control.y + to.y) / 2);
    let mid = Point::new((p01.x + p12.x) / 2, (p01.y + p12.y) / 2);
    emit_curve(w, current, p01, mid, bits, depth + 1);
    emit_curve(w, current, p12, to, bits, depth + 1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_path_is_closed() {
        let shape = Shape::rect(100.0, 50.0, Color::rgb(255, 0, 0));
        assert_eq!(shape.bounds, Rect::new(-1000, 1000, -500, 500));
        let commands = &shape.paths[0].commands;
        assert!(matches!(commands.first(), Some(Command::MoveTo(_))));
        assert_eq!(commands.len(), 5);
    }

    #[test]
    fn ellipse_has_eight_curves() {
        let shape = Shape::ellipse(200.0, 100.0, Color::WHITE);
        let curves = shape.paths[0]
            .commands
            .iter()
            .filter(|c| matches!(c, Command::CurveTo { .. }))
            .count();
        assert_eq!(curves, 8);
    }

    #[test]
    fn encode_starts_with_character_id() {
        let shape = Shape::rect(10.0, 10.0, Color::WHITE);
        let mut out = Vec::new();
        shape.write_define_shape3(7, &mut out).unwrap();
        assert_eq!(&out[0..2], &7u16.to_le_bytes());
        assert!(out.len() > 10);
    }

    #[test]
    fn huge_rect_splits_edges_to_fit_15_bits() {
        let shape = Shape::rect(8192.0, 8192.0, Color::WHITE);
        let mut out = Vec::new();
        shape.write_define_shape3(1, &mut out).unwrap();
        assert!(out.len() > 20);
    }
}
