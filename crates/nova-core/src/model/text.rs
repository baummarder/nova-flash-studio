//! Editable vector text.
//!
//! NØVA ships a small built-in bitmap font that is converted to ordinary
//! filled vector paths. The stage renderer and the SWF exporter use exactly the
//! same path generator, so what the editor shows is what the SWF contains.
//! This is deliberately **not** a font engine and **not** `DefineText`: imported
//! system/OpenType fonts, text selection and runtime editing are outside this
//! implementation and are documented as such.
//!
//! Two real variants are provided: proportional and monospaced. Bold and italic
//! are geometric transformations of the same glyph data.

use serde::{Deserialize, Serialize};

use super::{PathCommand, PathShape, Rgba};

const GLYPH_WIDTH: f32 = 5.0;
const GLYPH_HEIGHT: f32 = 7.0;
/// Vertical distance between baselines, in glyph units.
const LINE_ADVANCE: f32 = 9.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TextFont {
    #[default]
    NoveSans,
    NoveMono,
}

impl TextFont {
    pub fn label(self) -> &'static str {
        match self {
            Self::NoveSans => "NØVA Sans (Vektor)",
            Self::NoveMono => "NØVA Mono (Vektor)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

impl TextAlign {
    pub fn label(self) -> &'static str {
        match self {
            Self::Left => "Linksbündig",
            Self::Center => "Zentriert",
            Self::Right => "Rechtsbündig",
        }
    }
}

/// Editable text placed in a timeline keyframe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextObject {
    #[serde(default)]
    pub text: String,
    /// Nominal glyph height in stage pixels.
    #[serde(default = "default_size")]
    pub size: f32,
    #[serde(default)]
    pub font: TextFont,
    #[serde(default)]
    pub align: TextAlign,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
}

fn default_size() -> f32 {
    48.0
}

impl Default for TextObject {
    fn default() -> Self {
        Self {
            text: String::new(),
            size: 48.0,
            font: TextFont::NoveSans,
            align: TextAlign::Left,
            bold: false,
            italic: false,
        }
    }
}

impl TextObject {
    pub fn new(text: impl Into<String>, size: f32) -> Self {
        Self {
            text: text.into(),
            size,
            ..Default::default()
        }
    }

    fn unit(&self) -> f32 {
        self.size.max(1.0) / GLYPH_HEIGHT
    }

    fn lines(&self) -> Vec<&str> {
        let mut lines: Vec<&str> = self.text.lines().collect();
        if lines.is_empty() {
            lines.push("");
        }
        lines
    }

    fn glyph_for(&self, character: char) -> [u8; 7] {
        glyph(character)
    }

    /// Horizontal advance of one character in glyph units.
    fn advance(&self, character: char) -> f32 {
        if self.font == TextFont::NoveMono {
            return GLYPH_WIDTH + 1.0;
        }
        match character {
            ' ' => 3.0,
            '.' | ',' | ':' | ';' | '\'' | '!' | '|' | '`' => 3.0,
            'i' | 'l' | 'I' | 'j' => 4.0,
            'm' | 'w' | 'M' | 'W' | '@' => 7.0,
            _ => GLYPH_WIDTH + 1.0,
        }
    }

    fn line_width(&self, line: &str) -> f32 {
        line.chars().map(|character| self.advance(character)).sum()
    }

    /// Local size of the text block. The origin is the centre of the block.
    pub fn metrics(&self) -> (f32, f32) {
        let unit = self.unit();
        let width = self
            .lines()
            .iter()
            .map(|line| self.line_width(line))
            .fold(0.0f32, f32::max)
            .max(1.0)
            * unit;
        let line_count = self.lines().len() as f32;
        let height = ((line_count * LINE_ADVANCE) - (LINE_ADVANCE - GLYPH_HEIGHT)).max(1.0) * unit;
        (width, height)
    }

    /// Generates centred, filled vector paths in object-local coordinates.
    pub fn paths(&self, fill: Rgba) -> Vec<PathShape> {
        let unit = self.unit();
        let (width, height) = self.metrics();
        let left = -width / 2.0;
        let top = -height / 2.0;
        let mut paths = Vec::new();

        for (line_index, line) in self.lines().iter().enumerate() {
            let line_width = self.line_width(line) * unit;
            let start_x = match self.align {
                TextAlign::Left => left,
                TextAlign::Center => -line_width / 2.0,
                TextAlign::Right => left + (width - line_width),
            };
            let line_top = top + line_index as f32 * LINE_ADVANCE * unit;
            let mut cursor = start_x;

            for character in line.chars() {
                let rows = self.glyph_for(character);
                for (row_index, row) in rows.iter().enumerate() {
                    let mut column = 0usize;
                    while column < 5 {
                        if row & (1 << (4 - column)) == 0 {
                            column += 1;
                            continue;
                        }
                        let run_start = column;
                        while column < 5 && row & (1 << (4 - column)) != 0 {
                            column += 1;
                        }
                        let run_end = column;

                        let mut x0 = cursor + run_start as f32 * unit;
                        let mut x1 = cursor + run_end as f32 * unit;
                        if self.bold {
                            x1 += unit * 0.45;
                        }
                        let y0 = line_top + row_index as f32 * unit;
                        let y1 = y0 + unit;

                        if self.italic {
                            let center_y = (y0 + y1) / 2.0;
                            let skew = |y: f32| (center_y - y) * 0.28;
                            x0 += skew(y0);
                            x1 += skew(y1);
                        }

                        paths.push(rect_path(x0, y0, x1, y1, fill));
                    }
                }
                cursor += self.advance(character) * unit;
            }
        }

        if paths.is_empty() {
            // Keep the object selectable even when the text is empty.
            paths.push(rect_path(
                -width / 2.0,
                -height / 2.0,
                width / 2.0,
                height / 2.0,
                Rgba::new(fill.r, fill.g, fill.b, 0),
            ));
        }
        paths
    }
}

fn rect_path(x0: f32, y0: f32, x1: f32, y1: f32, fill: Rgba) -> PathShape {
    PathShape::new(
        fill,
        vec![
            PathCommand::MoveTo { x: x0, y: y0 },
            PathCommand::LineTo { x: x1, y: y0 },
            PathCommand::LineTo { x: x1, y: y1 },
            PathCommand::LineTo { x: x0, y: y1 },
            PathCommand::LineTo { x: x0, y: y0 },
        ],
    )
}

/// Built-in 5×7 glyph table. Unknown characters render as a hollow box.
///
/// Each byte stores five pixels in bits 4..0; row zero is the top row.
const FONT: &[(char, [u8; 7])] = &[
    (' ', [0, 0, 0, 0, 0, 0, 0]),
    (
        '!',
        [
            0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00000, 0b00100,
        ],
    ),
    (
        '"',
        [
            0b01010, 0b01010, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000,
        ],
    ),
    (
        '#',
        [
            0b01010, 0b01010, 0b11111, 0b01010, 0b11111, 0b01010, 0b01010,
        ],
    ),
    (
        '$',
        [
            0b00100, 0b01111, 0b10100, 0b01110, 0b00101, 0b11110, 0b00100,
        ],
    ),
    (
        '%',
        [
            0b11001, 0b11010, 0b00010, 0b00100, 0b01000, 0b01011, 0b10011,
        ],
    ),
    (
        '&',
        [
            0b01100, 0b10010, 0b10100, 0b01000, 0b10101, 0b10010, 0b01101,
        ],
    ),
    (
        '\'',
        [
            0b00100, 0b00100, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000,
        ],
    ),
    (
        '(',
        [
            0b00010, 0b00100, 0b01000, 0b01000, 0b01000, 0b00100, 0b00010,
        ],
    ),
    (
        ')',
        [
            0b01000, 0b00100, 0b00010, 0b00010, 0b00010, 0b00100, 0b01000,
        ],
    ),
    (
        '*',
        [
            0b00000, 0b00100, 0b10101, 0b01110, 0b10101, 0b00100, 0b00000,
        ],
    ),
    (
        '+',
        [
            0b00000, 0b00100, 0b00100, 0b11111, 0b00100, 0b00100, 0b00000,
        ],
    ),
    (
        ',',
        [
            0b00000, 0b00000, 0b00000, 0b00000, 0b00100, 0b00100, 0b01000,
        ],
    ),
    (
        '-',
        [
            0b00000, 0b00000, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000,
        ],
    ),
    (
        '.',
        [
            0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00100, 0b00100,
        ],
    ),
    (
        '/',
        [
            0b00001, 0b00010, 0b00010, 0b00100, 0b01000, 0b01000, 0b10000,
        ],
    ),
    (
        '0',
        [
            0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
        ],
    ),
    (
        '1',
        [
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
    ),
    (
        '2',
        [
            0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
        ],
    ),
    (
        '3',
        [
            0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
    ),
    (
        '4',
        [
            0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
        ],
    ),
    (
        '5',
        [
            0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110,
        ],
    ),
    (
        '6',
        [
            0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
        ],
    ),
    (
        '7',
        [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
        ],
    ),
    (
        '8',
        [
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
        ],
    ),
    (
        '9',
        [
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100,
        ],
    ),
    (
        ':',
        [
            0b00000, 0b00100, 0b00000, 0b00000, 0b00100, 0b00000, 0b00000,
        ],
    ),
    (
        ';',
        [
            0b00000, 0b00100, 0b00000, 0b00000, 0b00100, 0b00100, 0b01000,
        ],
    ),
    (
        '<',
        [
            0b00010, 0b00100, 0b01000, 0b10000, 0b01000, 0b00100, 0b00010,
        ],
    ),
    (
        '=',
        [
            0b00000, 0b00000, 0b11111, 0b00000, 0b11111, 0b00000, 0b00000,
        ],
    ),
    (
        '>',
        [
            0b01000, 0b00100, 0b00010, 0b00001, 0b00010, 0b00100, 0b01000,
        ],
    ),
    (
        '?',
        [
            0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b00000, 0b00100,
        ],
    ),
    (
        '@',
        [
            0b01110, 0b10001, 0b10111, 0b10101, 0b10111, 0b10000, 0b01110,
        ],
    ),
    (
        'A',
        [
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
    ),
    (
        'B',
        [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
        ],
    ),
    (
        'C',
        [
            0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110,
        ],
    ),
    (
        'D',
        [
            0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110,
        ],
    ),
    (
        'E',
        [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
        ],
    ),
    (
        'F',
        [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
    ),
    (
        'G',
        [
            0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111,
        ],
    ),
    (
        'H',
        [
            0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
    ),
    (
        'I',
        [
            0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
    ),
    (
        'J',
        [
            0b00111, 0b00010, 0b00010, 0b00010, 0b00010, 0b10010, 0b01100,
        ],
    ),
    (
        'K',
        [
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ],
    ),
    (
        'L',
        [
            0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
        ],
    ),
    (
        'M',
        [
            0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001,
        ],
    ),
    (
        'N',
        [
            0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001,
        ],
    ),
    (
        'O',
        [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
    ),
    (
        'P',
        [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
    ),
    (
        'Q',
        [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101,
        ],
    ),
    (
        'R',
        [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ],
    ),
    (
        'S',
        [
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
    ),
    (
        'T',
        [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
    ),
    (
        'U',
        [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
    ),
    (
        'V',
        [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
        ],
    ),
    (
        'W',
        [
            0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b11011, 0b10001,
        ],
    ),
    (
        'X',
        [
            0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001,
        ],
    ),
    (
        'Y',
        [
            0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
    ),
    (
        'Z',
        [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111,
        ],
    ),
    (
        '[',
        [
            0b01110, 0b01000, 0b01000, 0b01000, 0b01000, 0b01000, 0b01110,
        ],
    ),
    (
        '\\',
        [
            0b10000, 0b01000, 0b01000, 0b00100, 0b00010, 0b00010, 0b00001,
        ],
    ),
    (
        ']',
        [
            0b01110, 0b00010, 0b00010, 0b00010, 0b00010, 0b00010, 0b01110,
        ],
    ),
    (
        '^',
        [
            0b00100, 0b01010, 0b10001, 0b00000, 0b00000, 0b00000, 0b00000,
        ],
    ),
    (
        '_',
        [
            0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b11111,
        ],
    ),
    (
        '`',
        [
            0b01000, 0b00100, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000,
        ],
    ),
    (
        'a',
        [
            0b00000, 0b00000, 0b01110, 0b00001, 0b01111, 0b10001, 0b01111,
        ],
    ),
    (
        'b',
        [
            0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b10001, 0b11110,
        ],
    ),
    (
        'c',
        [
            0b00000, 0b00000, 0b01110, 0b10001, 0b10000, 0b10001, 0b01110,
        ],
    ),
    (
        'd',
        [
            0b00001, 0b00001, 0b01111, 0b10001, 0b10001, 0b10001, 0b01111,
        ],
    ),
    (
        'e',
        [
            0b00000, 0b00000, 0b01110, 0b10001, 0b11111, 0b10000, 0b01110,
        ],
    ),
    (
        'f',
        [
            0b00110, 0b01001, 0b01000, 0b11100, 0b01000, 0b01000, 0b01000,
        ],
    ),
    (
        'g',
        [
            0b00000, 0b01111, 0b10001, 0b10001, 0b01111, 0b00001, 0b01110,
        ],
    ),
    (
        'h',
        [
            0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b10001, 0b10001,
        ],
    ),
    (
        'i',
        [
            0b00100, 0b00000, 0b01100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
    ),
    (
        'j',
        [
            0b00010, 0b00000, 0b00110, 0b00010, 0b00010, 0b10010, 0b01100,
        ],
    ),
    (
        'k',
        [
            0b10000, 0b10000, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010,
        ],
    ),
    (
        'l',
        [
            0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
    ),
    (
        'm',
        [
            0b00000, 0b00000, 0b11010, 0b10101, 0b10101, 0b10001, 0b10001,
        ],
    ),
    (
        'n',
        [
            0b00000, 0b00000, 0b11110, 0b10001, 0b10001, 0b10001, 0b10001,
        ],
    ),
    (
        'o',
        [
            0b00000, 0b00000, 0b01110, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
    ),
    (
        'p',
        [
            0b00000, 0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000,
        ],
    ),
    (
        'q',
        [
            0b00000, 0b01111, 0b10001, 0b10001, 0b01111, 0b00001, 0b00001,
        ],
    ),
    (
        'r',
        [
            0b00000, 0b00000, 0b10110, 0b11001, 0b10000, 0b10000, 0b10000,
        ],
    ),
    (
        's',
        [
            0b00000, 0b00000, 0b01111, 0b10000, 0b01110, 0b00001, 0b11110,
        ],
    ),
    (
        't',
        [
            0b01000, 0b01000, 0b11100, 0b01000, 0b01000, 0b01001, 0b00110,
        ],
    ),
    (
        'u',
        [
            0b00000, 0b00000, 0b10001, 0b10001, 0b10001, 0b10011, 0b01101,
        ],
    ),
    (
        'v',
        [
            0b00000, 0b00000, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
        ],
    ),
    (
        'w',
        [
            0b00000, 0b00000, 0b10001, 0b10001, 0b10101, 0b10101, 0b01010,
        ],
    ),
    (
        'x',
        [
            0b00000, 0b00000, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001,
        ],
    ),
    (
        'y',
        [
            0b00000, 0b10001, 0b10001, 0b10001, 0b01111, 0b00001, 0b01110,
        ],
    ),
    (
        'z',
        [
            0b00000, 0b00000, 0b11111, 0b00010, 0b00100, 0b01000, 0b11111,
        ],
    ),
    (
        '{',
        [
            0b00010, 0b00100, 0b00100, 0b01000, 0b00100, 0b00100, 0b00010,
        ],
    ),
    (
        '|',
        [
            0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
    ),
    (
        '}',
        [
            0b01000, 0b00100, 0b00100, 0b00010, 0b00100, 0b00100, 0b01000,
        ],
    ),
    (
        '~',
        [
            0b00000, 0b00000, 0b01001, 0b10110, 0b00000, 0b00000, 0b00000,
        ],
    ),
    // German and common typographic characters.
    (
        'Ä',
        [
            0b01010, 0b00000, 0b01110, 0b10001, 0b11111, 0b10001, 0b10001,
        ],
    ),
    (
        'Ö',
        [
            0b01010, 0b00000, 0b01110, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
    ),
    (
        'Ü',
        [
            0b01010, 0b00000, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
    ),
    (
        'ä',
        [
            0b01010, 0b00000, 0b01110, 0b00001, 0b01111, 0b10001, 0b01111,
        ],
    ),
    (
        'ö',
        [
            0b01010, 0b00000, 0b01110, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
    ),
    (
        'ü',
        [
            0b01010, 0b00000, 0b10001, 0b10001, 0b10001, 0b10011, 0b01101,
        ],
    ),
    (
        'ß',
        [
            0b01110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
        ],
    ),
    (
        'Ø',
        [
            0b01111, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b11110,
        ],
    ),
    (
        '€',
        [
            0b00111, 0b01000, 0b11110, 0b01000, 0b11110, 0b01000, 0b00111,
        ],
    ),
    (
        '–',
        [
            0b00000, 0b00000, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000,
        ],
    ),
];

fn glyph(character: char) -> [u8; 7] {
    FONT.iter()
        .find(|(candidate, _)| *candidate == character)
        .map(|(_, rows)| *rows)
        // A box for characters that are not part of the built-in vector font.
        .unwrap_or([
            0b11111, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11111,
        ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_metrics_grow_with_content_and_lines() {
        let mut text = TextObject::new("A", 70.0);
        let one = text.metrics();
        text.text = "AB".into();
        let two = text.metrics();
        assert!(two.0 > one.0);
        text.text = "A\nB".into();
        let lines = text.metrics();
        assert!(lines.1 > two.1);
    }

    #[test]
    fn text_paths_are_centred_and_filled() {
        let mut text = TextObject::new("NOVA", 70.0);
        text.align = TextAlign::Center;
        let paths = text.paths(Rgba::rgb(255, 0, 0));
        assert!(!paths.is_empty());
        let (width, height) = text.metrics();
        for path in &paths {
            assert_eq!(path.fill, Rgba::rgb(255, 0, 0));
            for command in &path.commands {
                let (x, y) = match *command {
                    PathCommand::MoveTo { x, y } | PathCommand::LineTo { x, y } => (x, y),
                    PathCommand::CurveTo {
                        control_x,
                        control_y,
                        ..
                    } => (control_x, control_y),
                    PathCommand::CubicTo { c1_x, c1_y, .. } => (c1_x, c1_y),
                };
                assert!(x.abs() <= width / 2.0 + 2.0);
                assert!(y.abs() <= height / 2.0 + 2.0);
            }
        }
    }

    #[test]
    fn bold_and_italic_change_geometry() {
        let plain = TextObject::new("Text", 48.0).paths(Rgba::WHITE);
        let mut styled = TextObject::new("Text", 48.0);
        styled.bold = true;
        styled.italic = true;
        let styled_paths = styled.paths(Rgba::WHITE);
        assert_eq!(plain.len(), styled_paths.len());
        assert_ne!(plain[0].commands, styled_paths[0].commands);
    }

    #[test]
    fn unknown_glyphs_have_a_visible_fallback() {
        let paths = TextObject::new("☃", 48.0).paths(Rgba::WHITE);
        assert!(!paths.is_empty());
    }
}
