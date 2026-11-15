//! Hand-written ActionScript 3 lexer.

use crate::diagnostic::Diagnostic;
use crate::token::{Keyword, Span, Symbol, Token, TokenKind};

pub struct LexResult {
    pub tokens: Vec<Token>,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn lex(source: &str) -> LexResult {
    Lexer::new(source).run()
}

struct Lexer {
    chars: Vec<char>,
    pos: usize,
    line: usize,
    column: usize,
    tokens: Vec<Token>,
    diagnostics: Vec<Diagnostic>,
}

const SYMBOLS: &[(&str, Symbol)] = &[
    (">>>=", Symbol::ShiftRightUnsignedAssign),
    ("===", Symbol::StrictEq),
    ("!==", Symbol::StrictNotEq),
    (">>>", Symbol::ShiftRightUnsigned),
    ("<<=", Symbol::ShiftLeftAssign),
    (">>=", Symbol::ShiftRightAssign),
    ("==", Symbol::Eq),
    ("!=", Symbol::NotEq),
    ("<=", Symbol::Le),
    (">=", Symbol::Ge),
    ("&&", Symbol::AndAnd),
    ("||", Symbol::OrOr),
    ("++", Symbol::Increase),
    ("--", Symbol::Decrease),
    ("+=", Symbol::PlusAssign),
    ("-=", Symbol::MinusAssign),
    ("*=", Symbol::StarAssign),
    ("/=", Symbol::SlashAssign),
    ("%=", Symbol::PercentAssign),
    ("&=", Symbol::AmpAssign),
    ("|=", Symbol::PipeAssign),
    ("^=", Symbol::CaretAssign),
    ("<<", Symbol::ShiftLeft),
    (">>", Symbol::ShiftRight),
    ("{", Symbol::LBrace),
    ("}", Symbol::RBrace),
    ("(", Symbol::LParen),
    (")", Symbol::RParen),
    ("[", Symbol::LBracket),
    ("]", Symbol::RBracket),
    (";", Symbol::Semicolon),
    (",", Symbol::Comma),
    (".", Symbol::Dot),
    (":", Symbol::Colon),
    ("?", Symbol::Question),
    ("=", Symbol::Assign),
    ("<", Symbol::Lt),
    (">", Symbol::Gt),
    ("+", Symbol::Plus),
    ("-", Symbol::Minus),
    ("*", Symbol::Star),
    ("/", Symbol::Slash),
    ("%", Symbol::Percent),
    ("!", Symbol::Not),
    ("&", Symbol::Amp),
    ("|", Symbol::Pipe),
    ("^", Symbol::Caret),
    ("~", Symbol::Tilde),
];

impl Lexer {
    fn new(source: &str) -> Self {
        Self {
            chars: source.chars().collect(),
            pos: 0,
            line: 1,
            column: 1,
            tokens: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    fn run(mut self) -> LexResult {
        while let Some(c) = self.peek(0) {
            if c.is_whitespace() || c == '\u{feff}' {
                self.bump();
                continue;
            }
            if c == '/' && self.peek(1) == Some('/') {
                while let Some(ch) = self.peek(0) {
                    if ch == '\n' {
                        break;
                    }
                    self.bump();
                }
                continue;
            }
            if c == '/' && self.peek(1) == Some('*') {
                self.block_comment();
                continue;
            }
            if c == '"' || c == '\'' {
                self.string(c);
                continue;
            }
            if c.is_ascii_digit() || (c == '.' && self.peek(1).is_some_and(|n| n.is_ascii_digit()))
            {
                self.number();
                continue;
            }
            if c.is_alphabetic() || c == '_' || c == '$' {
                self.identifier();
                continue;
            }
            if self.symbol() {
                continue;
            }

            let start = self.mark();
            self.bump();
            let span = self.finish(start);
            self.diagnostics.push(Diagnostic::error(
                format!("Unerwartetes Zeichen '{c}'"),
                span,
            ));
        }

        let mark = self.mark();
        self.tokens.push(Token {
            kind: TokenKind::Eof,
            span: self.finish(mark),
        });
        LexResult {
            tokens: self.tokens,
            diagnostics: self.diagnostics,
        }
    }

    fn block_comment(&mut self) {
        let start = self.mark();
        self.bump();
        self.bump();
        loop {
            match (self.peek(0), self.peek(1)) {
                (Some('*'), Some('/')) => {
                    self.bump();
                    self.bump();
                    return;
                }
                (Some(_), _) => {
                    self.bump();
                }
                (None, _) => {
                    self.diagnostics.push(Diagnostic::error(
                        "Nicht beendeter Blockkommentar",
                        self.finish(start),
                    ));
                    return;
                }
            }
        }
    }

    fn string(&mut self, quote: char) {
        let start = self.mark();
        self.bump();
        let mut value = String::new();
        loop {
            match self.peek(0) {
                None => {
                    self.diagnostics.push(Diagnostic::error(
                        "Nicht beendete Zeichenkette",
                        self.finish(start),
                    ));
                    break;
                }
                Some('\n') => {
                    self.diagnostics.push(Diagnostic::error(
                        "Zeilenumbruch in Zeichenkette",
                        self.finish(start),
                    ));
                    break;
                }
                Some(c) if c == quote => {
                    self.bump();
                    break;
                }
                Some('\\') => {
                    self.bump();
                    match self.peek(0) {
                        Some('n') => {
                            value.push('\n');
                            self.bump();
                        }
                        Some('r') => {
                            value.push('\r');
                            self.bump();
                        }
                        Some('t') => {
                            value.push('\t');
                            self.bump();
                        }
                        Some('0') => {
                            value.push('\0');
                            self.bump();
                        }
                        Some('\\') => {
                            value.push('\\');
                            self.bump();
                        }
                        Some('"') => {
                            value.push('"');
                            self.bump();
                        }
                        Some('\'') => {
                            value.push('\'');
                            self.bump();
                        }
                        Some('u') => {
                            self.bump();
                            let mut hex = String::new();
                            for _ in 0..4 {
                                if let Some(c) = self.peek(0)
                                    && c.is_ascii_hexdigit()
                                {
                                    hex.push(c);
                                    self.bump();
                                }
                            }
                            if hex.len() == 4
                                && let Ok(code) = u32::from_str_radix(&hex, 16)
                                && let Some(c) = char::from_u32(code)
                            {
                                value.push(c);
                            }
                        }
                        Some(other) => {
                            value.push(other);
                            self.bump();
                        }
                        None => {}
                    }
                }
                Some(c) => {
                    value.push(c);
                    self.bump();
                }
            }
        }
        let span = self.finish(start);
        self.tokens.push(Token {
            kind: TokenKind::Str(value),
            span,
        });
    }

    fn number(&mut self) {
        let start = self.mark();
        let mut text = String::new();
        if self.peek(0) == Some('0') && matches!(self.peek(1), Some('x' | 'X')) {
            text.push('0');
            self.bump();
            text.push(self.peek(0).unwrap_or('x'));
            self.bump();
            while self.peek(0).is_some_and(|c| c.is_ascii_hexdigit()) {
                text.push(self.bump().unwrap());
            }
        } else {
            while self.peek(0).is_some_and(|c| c.is_ascii_digit()) {
                text.push(self.bump().unwrap());
            }
            if self.peek(0) == Some('.') {
                text.push('.');
                self.bump();
                while self.peek(0).is_some_and(|c| c.is_ascii_digit()) {
                    text.push(self.bump().unwrap());
                }
            }
            if matches!(self.peek(0), Some('e' | 'E')) {
                let save = (self.pos, self.line, self.column);
                let text_len = text.len();
                text.push(self.bump().unwrap());
                if matches!(self.peek(0), Some('+' | '-')) {
                    text.push(self.bump().unwrap());
                }
                if self.peek(0).is_some_and(|c| c.is_ascii_digit()) {
                    while self.peek(0).is_some_and(|c| c.is_ascii_digit()) {
                        text.push(self.bump().unwrap());
                    }
                } else {
                    // Not an exponent after all (e.g. `1e` used as identifier).
                    self.pos = save.0;
                    self.line = save.1;
                    self.column = save.2;
                    text.truncate(text_len);
                }
            }
        }
        let span = self.finish(start);
        self.tokens.push(Token {
            kind: TokenKind::Number(text),
            span,
        });
    }

    fn identifier(&mut self) {
        let start = self.mark();
        let mut text = String::new();
        while let Some(c) = self.peek(0) {
            if c.is_alphanumeric() || c == '_' || c == '$' {
                text.push(c);
                self.bump();
            } else {
                break;
            }
        }
        let span = self.finish(start);
        let kind = match Keyword::from_ident(&text) {
            Some(keyword) => TokenKind::Keyword(keyword),
            None => TokenKind::Ident(text),
        };
        self.tokens.push(Token { kind, span });
    }

    fn symbol(&mut self) -> bool {
        for (text, symbol) in SYMBOLS {
            if self.matches(text) {
                let start = self.mark();
                for _ in text.chars() {
                    self.bump();
                }
                let span = self.finish(start);
                self.tokens.push(Token {
                    kind: TokenKind::Symbol(*symbol),
                    span,
                });
                return true;
            }
        }
        false
    }

    fn matches(&self, text: &str) -> bool {
        text.chars()
            .enumerate()
            .all(|(index, c)| self.peek(index) == Some(c))
    }

    fn peek(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.chars.get(self.pos).copied()?;
        self.pos += 1;
        if c == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(c)
    }

    fn mark(&self) -> (usize, usize, usize) {
        (self.pos, self.line, self.column)
    }

    fn finish(&self, mark: (usize, usize, usize)) -> Span {
        Span::new(mark.0, self.pos, mark.1, mark.2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(source: &str) -> Vec<TokenKind> {
        let result = lex(source);
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        result.tokens.into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn lexes_class_header() {
        let tokens = kinds("public class Main extends MovieClip { }");
        assert_eq!(tokens[0], TokenKind::Keyword(Keyword::Public));
        assert_eq!(tokens[1], TokenKind::Keyword(Keyword::Class));
        assert_eq!(tokens[2], TokenKind::Ident("Main".into()));
        assert_eq!(tokens[3], TokenKind::Keyword(Keyword::Extends));
        assert_eq!(tokens[4], TokenKind::Ident("MovieClip".into()));
    }

    #[test]
    fn lexes_numbers_and_strings() {
        let tokens = kinds(r#"var s = "a\nb"; var n = 0x1F; var f = 1.5e3;"#);
        assert!(tokens.contains(&TokenKind::Str("a\nb".into())));
        assert!(tokens.contains(&TokenKind::Number("0x1F".into())));
        assert!(tokens.contains(&TokenKind::Number("1.5e3".into())));
    }

    #[test]
    fn skips_comments() {
        let tokens = kinds("// line\n/* block */ var x;");
        assert_eq!(tokens[0], TokenKind::Keyword(Keyword::Var));
    }

    #[test]
    fn longest_operator_wins() {
        let tokens = kinds("a >>>= b");
        assert!(tokens.contains(&TokenKind::Symbol(Symbol::ShiftRightUnsignedAssign)));
    }

    #[test]
    fn reports_unterminated_string() {
        let result = lex("\"abc\n");
        assert_eq!(result.diagnostics.len(), 1);
        assert!(result.diagnostics[0].message.contains("Zeichenkette"));
    }
}
