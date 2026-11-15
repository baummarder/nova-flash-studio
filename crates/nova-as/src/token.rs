//! Tokens and source spans for the ActionScript 3 front-end.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// Character offset into the source.
    pub start: usize,
    pub end: usize,
    /// 1-based line and column of `start`.
    pub line: usize,
    pub column: usize,
}

impl Span {
    pub const fn new(start: usize, end: usize, line: usize, column: usize) -> Self {
        Self {
            start,
            end,
            line,
            column,
        }
    }

    pub fn join(self, other: Span) -> Self {
        Self {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
            line: self.line,
            column: self.column,
        }
    }

    pub fn len(self) -> usize {
        self.end.saturating_sub(self.start)
    }

    pub fn is_empty(self) -> bool {
        self.len() == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keyword {
    Package,
    Import,
    Class,
    Interface,
    Extends,
    Implements,
    Public,
    Private,
    Protected,
    Internal,
    Static,
    Override,
    Final,
    Dynamic,
    Native,
    Function,
    Var,
    Const,
    New,
    Return,
    If,
    Else,
    While,
    Do,
    For,
    In,
    Is,
    As,
    Typeof,
    Instanceof,
    This,
    Super,
    Null,
    True,
    False,
    Void,
    Int,
    Uint,
    Number,
    Boolean,
    String,
    Object,
    Array,
    Throw,
    Try,
    Catch,
    Finally,
    Switch,
    Case,
    Default,
    Break,
    Continue,
    Delete,
    With,
    Use,
    Get,
    Set,
}

impl Keyword {
    pub fn from_ident(ident: &str) -> Option<Self> {
        Some(match ident {
            "package" => Self::Package,
            "import" => Self::Import,
            "class" => Self::Class,
            "interface" => Self::Interface,
            "extends" => Self::Extends,
            "implements" => Self::Implements,
            "public" => Self::Public,
            "private" => Self::Private,
            "protected" => Self::Protected,
            "internal" => Self::Internal,
            "static" => Self::Static,
            "override" => Self::Override,
            "final" => Self::Final,
            "dynamic" => Self::Dynamic,
            "native" => Self::Native,
            "function" => Self::Function,
            "var" => Self::Var,
            "const" => Self::Const,
            "new" => Self::New,
            "return" => Self::Return,
            "if" => Self::If,
            "else" => Self::Else,
            "while" => Self::While,
            "do" => Self::Do,
            "for" => Self::For,
            "in" => Self::In,
            "is" => Self::Is,
            "as" => Self::As,
            "typeof" => Self::Typeof,
            "instanceof" => Self::Instanceof,
            "this" => Self::This,
            "super" => Self::Super,
            "null" => Self::Null,
            "true" => Self::True,
            "false" => Self::False,
            "void" => Self::Void,
            "int" => Self::Int,
            "uint" => Self::Uint,
            "Number" => Self::Number,
            "Boolean" => Self::Boolean,
            "String" => Self::String,
            "Object" => Self::Object,
            "Array" => Self::Array,
            "throw" => Self::Throw,
            "try" => Self::Try,
            "catch" => Self::Catch,
            "finally" => Self::Finally,
            "switch" => Self::Switch,
            "case" => Self::Case,
            "default" => Self::Default,
            "break" => Self::Break,
            "continue" => Self::Continue,
            "delete" => Self::Delete,
            "with" => Self::With,
            "use" => Self::Use,
            "get" => Self::Get,
            "set" => Self::Set,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Package => "package",
            Self::Import => "import",
            Self::Class => "class",
            Self::Interface => "interface",
            Self::Extends => "extends",
            Self::Implements => "implements",
            Self::Public => "public",
            Self::Private => "private",
            Self::Protected => "protected",
            Self::Internal => "internal",
            Self::Static => "static",
            Self::Override => "override",
            Self::Final => "final",
            Self::Dynamic => "dynamic",
            Self::Native => "native",
            Self::Function => "function",
            Self::Var => "var",
            Self::Const => "const",
            Self::New => "new",
            Self::Return => "return",
            Self::If => "if",
            Self::Else => "else",
            Self::While => "while",
            Self::Do => "do",
            Self::For => "for",
            Self::In => "in",
            Self::Is => "is",
            Self::As => "as",
            Self::Typeof => "typeof",
            Self::Instanceof => "instanceof",
            Self::This => "this",
            Self::Super => "super",
            Self::Null => "null",
            Self::True => "true",
            Self::False => "false",
            Self::Void => "void",
            Self::Int => "int",
            Self::Uint => "uint",
            Self::Number => "Number",
            Self::Boolean => "Boolean",
            Self::String => "String",
            Self::Object => "Object",
            Self::Array => "Array",
            Self::Throw => "throw",
            Self::Try => "try",
            Self::Catch => "catch",
            Self::Finally => "finally",
            Self::Switch => "switch",
            Self::Case => "case",
            Self::Default => "default",
            Self::Break => "break",
            Self::Continue => "continue",
            Self::Delete => "delete",
            Self::With => "with",
            Self::Use => "use",
            Self::Get => "get",
            Self::Set => "set",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Symbol {
    LBrace,
    RBrace,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Semicolon,
    Comma,
    Dot,
    Colon,
    Question,
    Assign,
    PlusAssign,
    MinusAssign,
    StarAssign,
    SlashAssign,
    PercentAssign,
    AmpAssign,
    PipeAssign,
    CaretAssign,
    ShiftLeftAssign,
    ShiftRightAssign,
    ShiftRightUnsignedAssign,
    Eq,
    NotEq,
    StrictEq,
    StrictNotEq,
    Lt,
    Le,
    Gt,
    Ge,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    AndAnd,
    OrOr,
    Not,
    Amp,
    Pipe,
    Caret,
    Tilde,
    Increase,
    Decrease,
    ShiftLeft,
    ShiftRight,
    ShiftRightUnsigned,
}

impl Symbol {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LBrace => "{",
            Self::RBrace => "}",
            Self::LParen => "(",
            Self::RParen => ")",
            Self::LBracket => "[",
            Self::RBracket => "]",
            Self::Semicolon => ";",
            Self::Comma => ",",
            Self::Dot => ".",
            Self::Colon => ":",
            Self::Question => "?",
            Self::Assign => "=",
            Self::PlusAssign => "+=",
            Self::MinusAssign => "-=",
            Self::StarAssign => "*=",
            Self::SlashAssign => "/=",
            Self::PercentAssign => "%=",
            Self::AmpAssign => "&=",
            Self::PipeAssign => "|=",
            Self::CaretAssign => "^=",
            Self::ShiftLeftAssign => "<<=",
            Self::ShiftRightAssign => ">>=",
            Self::ShiftRightUnsignedAssign => ">>>=",
            Self::Eq => "==",
            Self::NotEq => "!=",
            Self::StrictEq => "===",
            Self::StrictNotEq => "!==",
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Gt => ">",
            Self::Ge => ">=",
            Self::Plus => "+",
            Self::Minus => "-",
            Self::Star => "*",
            Self::Slash => "/",
            Self::Percent => "%",
            Self::AndAnd => "&&",
            Self::OrOr => "||",
            Self::Not => "!",
            Self::Amp => "&",
            Self::Pipe => "|",
            Self::Caret => "^",
            Self::Tilde => "~",
            Self::Increase => "++",
            Self::Decrease => "--",
            Self::ShiftLeft => "<<",
            Self::ShiftRight => ">>",
            Self::ShiftRightUnsigned => ">>>",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Ident(String),
    Keyword(Keyword),
    Number(String),
    Str(String),
    Symbol(Symbol),
    Eof,
}

impl TokenKind {
    pub fn describe(&self) -> String {
        match self {
            Self::Ident(name) => format!("Bezeichner '{name}'"),
            Self::Keyword(keyword) => format!("Schlüsselwort '{}'", keyword.as_str()),
            Self::Number(value) => format!("Zahl '{value}'"),
            Self::Str(value) => format!("Zeichenkette \"{value}\""),
            Self::Symbol(symbol) => format!("'{}'", symbol.as_str()),
            Self::Eof => "Dateiende".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn is_symbol(&self, symbol: Symbol) -> bool {
        self.kind == TokenKind::Symbol(symbol)
    }

    pub fn is_keyword(&self, keyword: Keyword) -> bool {
        self.kind == TokenKind::Keyword(keyword)
    }

    pub fn ident(&self) -> Option<&str> {
        match &self.kind {
            TokenKind::Ident(name) => Some(name),
            _ => None,
        }
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.kind.describe())
    }
}
