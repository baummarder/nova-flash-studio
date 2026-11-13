//! ActionScript 3 syntax highlighting for the code editor.
//!
//! The highlighter is a lightweight scanner that is intentionally separate from
//! the compiler lexer: the editor needs to colour comments and unterminated
//! strings while the user is still typing, whereas the compiler reports those
//! as diagnostics.

use std::ops::Range;

use egui::text::LayoutJob;
use egui::{Color32, FontId, TextFormat};
use nova_as::Keyword;

use crate::theme;

const KEYWORD: Color32 = Color32::from_rgb(199, 146, 234);
const TYPE: Color32 = Color32::from_rgb(255, 203, 107);
const LITERAL: Color32 = Color32::from_rgb(247, 140, 108);
const STRING: Color32 = Color32::from_rgb(195, 232, 141);
const COMMENT: Color32 = Color32::from_rgb(108, 117, 134);
const OPERATOR: Color32 = Color32::from_rgb(137, 221, 255);
const PUNCT: Color32 = Color32::from_rgb(166, 172, 205);
const NUMBER: Color32 = Color32::from_rgb(247, 140, 108);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
    Keyword,
    Type,
    Literal,
    String,
    Comment,
    Operator,
    Punct,
    Number,
    Ident,
}

impl Class {
    fn color(self) -> Color32 {
        match self {
            Self::Keyword => KEYWORD,
            Self::Type => TYPE,
            Self::Literal => LITERAL,
            Self::String => STRING,
            Self::Comment => COMMENT,
            Self::Operator => OPERATOR,
            Self::Punct => PUNCT,
            Self::Number => NUMBER,
            Self::Ident => theme::TEXT,
        }
    }

    fn italics(self) -> bool {
        matches!(self, Self::Comment)
    }
}

/// Builds a highlighted [`LayoutJob`] for the whole source.
pub fn job(source: &str, font_size: f32) -> LayoutJob {
    let font_id = FontId::monospace(font_size);
    let mut job = LayoutJob::default();
    // No wrapping: the editor lives inside a horizontal scroll area and line
    // numbers must stay aligned with source lines.
    job.wrap.max_width = f32::INFINITY;

    let spans = scan(source);
    let mut cursor = 0;
    for (range, class) in spans {
        if range.start > cursor {
            append(
                &mut job,
                &source[cursor..range.start],
                Class::Ident,
                &font_id,
            );
        }
        append(&mut job, &source[range.clone()], class, &font_id);
        cursor = range.end;
    }
    if cursor < source.len() {
        append(&mut job, &source[cursor..], Class::Ident, &font_id);
    }
    job
}

fn append(job: &mut LayoutJob, text: &str, class: Class, font_id: &FontId) {
    if text.is_empty() {
        return;
    }
    job.append(
        text,
        0.0,
        TextFormat {
            font_id: font_id.clone(),
            color: class.color(),
            italics: class.italics(),
            ..Default::default()
        },
    );
}

fn scan(source: &str) -> Vec<(Range<usize>, Class)> {
    let bytes = source.as_bytes();
    let mut spans = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        let start = i;
        match bytes[i] {
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                i += 2;
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                spans.push((start..i, Class::Comment));
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                i = (i + 2).min(bytes.len());
                spans.push((start..i, Class::Comment));
            }
            b'"' | b'\'' => {
                let quote = bytes[i];
                i += 1;
                while i < bytes.len() {
                    match bytes[i] {
                        b'\\' => i = (i + 2).min(bytes.len()),
                        b'\n' => break,
                        b if b == quote => {
                            i += 1;
                            break;
                        }
                        _ => i += 1,
                    }
                }
                spans.push((start..i, Class::String));
            }
            b if b.is_ascii_digit()
                || (b == b'.' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit)) =>
            {
                i += 1;
                while i < bytes.len()
                    && (bytes[i].is_ascii_alphanumeric()
                        || matches!(bytes[i], b'.' | b'_')
                        || (matches!(bytes[i], b'+' | b'-')
                            && matches!(bytes.get(i - 1), Some(b'e' | b'E'))))
                {
                    i += 1;
                }
                spans.push((start..i, Class::Number));
            }
            b if b.is_ascii_alphabetic() || b == b'_' || b == b'$' => {
                i += 1;
                while i < bytes.len()
                    && (bytes[i].is_ascii_alphanumeric() || matches!(bytes[i], b'_' | b'$'))
                {
                    i += 1;
                }
                let word = &source[start..i];
                spans.push((start..i, classify(word)));
            }
            b if is_operator(b) => {
                i += 1;
                // Keep multi-character operators visually together.
                while i < bytes.len() && is_operator(bytes[i]) {
                    i += 1;
                }
                spans.push((start..i, Class::Operator));
            }
            b if b.is_ascii_punctuation() => {
                i += 1;
                spans.push((start..i, Class::Punct));
            }
            _ => {
                // Whitespace and non-ASCII characters are left at default color.
                i += 1;
            }
        }
    }

    spans
}

fn classify(word: &str) -> Class {
    if let Some(keyword) = Keyword::from_ident(word) {
        return match keyword {
            Keyword::True | Keyword::False | Keyword::Null | Keyword::This | Keyword::Super => {
                Class::Literal
            }
            Keyword::Int
            | Keyword::Uint
            | Keyword::Number
            | Keyword::Boolean
            | Keyword::String
            | Keyword::Object
            | Keyword::Array
            | Keyword::Void => Class::Type,
            _ => Class::Keyword,
        };
    }
    if word.chars().next().is_some_and(char::is_uppercase) {
        Class::Type
    } else {
        Class::Ident
    }
}

fn is_operator(b: u8) -> bool {
    matches!(
        b,
        b'+' | b'-'
            | b'*'
            | b'/'
            | b'%'
            | b'='
            | b'!'
            | b'<'
            | b'>'
            | b'&'
            | b'|'
            | b'^'
            | b'~'
            | b'?'
            | b':'
    )
}

/// Maps a 1-based line/column pair to a character offset.
pub fn offset_of(source: &str, line: usize, column: usize) -> usize {
    let mut offset = 0;
    for (index, text_line) in source.split('\n').enumerate() {
        if index + 1 == line {
            return offset + column.saturating_sub(1).min(text_line.chars().count());
        }
        offset += text_line.chars().count() + 1;
    }
    source.chars().count()
}

/// Number of visual source lines, at least one.
pub fn line_count(source: &str) -> usize {
    source.lines().count().max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classes(source: &str) -> Vec<(&str, Class)> {
        scan(source)
            .into_iter()
            .map(|(range, class)| (&source[range], class))
            .collect()
    }

    #[test]
    fn classifies_keywords_types_and_literals() {
        let source = "public class Main extends MovieClip { var n:Number = 1.5; }";
        let found = classes(source);
        assert!(found.contains(&("public", Class::Keyword)));
        assert!(found.contains(&("class", Class::Keyword)));
        assert!(found.contains(&("MovieClip", Class::Type)));
        assert!(found.contains(&("Number", Class::Type)));
        assert!(found.contains(&("1.5", Class::Number)));
    }

    #[test]
    fn colours_comments_and_strings() {
        let source = "// line\nvar s:String = \"x \\\" y\"; /* block */";
        let found = classes(source);
        assert!(found.contains(&("// line", Class::Comment)));
        assert!(found.contains(&("/* block */", Class::Comment)));
        assert!(
            found
                .iter()
                .any(|(text, class)| *class == Class::String && text.starts_with('"'))
        );
    }

    #[test]
    fn does_not_panic_on_unterminated_input() {
        let found = classes("var s = \"abc");
        assert!(found.iter().any(|(_, class)| *class == Class::String));
    }

    #[test]
    fn offset_mapping_is_one_based() {
        let source = "abc\ndef\nghi";
        assert_eq!(offset_of(source, 1, 1), 0);
        assert_eq!(offset_of(source, 2, 1), 4);
        // Character offsets, not byte offsets: "abc\n" + "def\n" = 8 chars.
        assert_eq!(offset_of(source, 3, 4), 11);
        assert_eq!(offset_of(source, 9, 1), source.chars().count());
    }

    #[test]
    fn handles_unicode_before_offset() {
        let source = "var s = \"äöü\";\nnext";
        let newline_chars = source.chars().take_while(|ch| *ch != '\n').count();
        assert_eq!(offset_of(source, 2, 1), newline_chars + 1);
    }
}
