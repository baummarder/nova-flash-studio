//! Abstract syntax tree for the supported ActionScript 3 subset.

use crate::token::Span;

#[derive(Debug, Clone)]
pub struct CompilationUnit {
    pub package: Option<String>,
    pub imports: Vec<ImportDecl>,
    pub classes: Vec<ClassDecl>,
    pub functions: Vec<FunctionDecl>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ImportDecl {
    pub path: String,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub public: bool,
    pub private: bool,
    pub protected: bool,
    pub internal: bool,
    pub static_modifier: bool,
    pub override_modifier: bool,
    pub final_modifier: bool,
    pub dynamic: bool,
    pub native: bool,
}

impl Modifiers {
    pub fn has_access_modifier(self) -> bool {
        self.public || self.private || self.protected || self.internal
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeRef {
    pub name: String,
    pub array_depth: usize,
}

impl TypeRef {
    pub fn display(&self) -> String {
        format!("{}{}", self.name, "[]".repeat(self.array_depth))
    }
}

#[derive(Debug, Clone)]
pub struct ClassDecl {
    pub name: String,
    pub modifiers: Modifiers,
    pub base: Option<String>,
    pub interfaces: Vec<String>,
    pub members: Vec<Member>,
    pub span: Span,
}

impl ClassDecl {
    pub fn constructor(&self) -> Option<&FunctionDecl> {
        self.members.iter().find_map(|member| match member {
            Member::Method(function) if function.name == self.name => Some(function),
            _ => None,
        })
    }

    pub fn method(&self, name: &str) -> Option<&FunctionDecl> {
        self.members.iter().find_map(|member| match member {
            Member::Method(function) if function.name == name => Some(function),
            _ => None,
        })
    }
}

#[derive(Debug, Clone)]
pub enum Member {
    Field(FieldDecl),
    Method(FunctionDecl),
}

impl Member {
    pub fn name(&self) -> &str {
        match self {
            Self::Field(field) => &field.name,
            Self::Method(function) => &function.name,
        }
    }

    pub fn span(&self) -> Span {
        match self {
            Self::Field(field) => field.span,
            Self::Method(function) => function.span,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FieldDecl {
    pub name: String,
    pub ty: Option<TypeRef>,
    pub value: Option<Expr>,
    pub is_const: bool,
    pub modifiers: Modifiers,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct FunctionDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: Option<TypeRef>,
    pub body: Option<Block>,
    pub modifiers: Modifiers,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: Option<TypeRef>,
    pub default: Option<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Block {
    pub statements: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum StmtKind {
    Block(Block),
    Var(VarDecl),
    Expr(Expr),
    If {
        condition: Expr,
        then_branch: Box<Stmt>,
        else_branch: Option<Box<Stmt>>,
    },
    While {
        condition: Expr,
        body: Box<Stmt>,
    },
    DoWhile {
        body: Box<Stmt>,
        condition: Expr,
    },
    For {
        init: ForInit,
        condition: Option<Expr>,
        update: Option<Expr>,
        body: Box<Stmt>,
    },
    Return(Option<Expr>),
    Break,
    Continue,
    Throw(Expr),
    Empty,
}

#[derive(Debug, Clone)]
pub enum ForInit {
    Var(VarDecl),
    Expr(Expr),
    Empty,
}

#[derive(Debug, Clone)]
pub struct VarDecl {
    pub name: String,
    pub ty: Option<TypeRef>,
    pub value: Option<Expr>,
    pub is_const: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    Number(String),
    String(String),
    Bool(bool),
    Null,
    This,
    Ident(String),
    Member {
        target: Box<Expr>,
        name: String,
    },
    Index {
        target: Box<Expr>,
        index: Box<Expr>,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    New {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Postfix {
        op: PostfixOp,
        expr: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Assign {
        op: AssignOp,
        target: Box<Expr>,
        value: Box<Expr>,
    },
    Conditional {
        condition: Box<Expr>,
        then_expr: Box<Expr>,
        else_expr: Box<Expr>,
    },
    Array(Vec<Expr>),
    Object(Vec<(String, Expr)>),
    Paren(Box<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Plus,
    Minus,
    Not,
    BitNot,
    Typeof,
    Void,
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostfixOp {
    Increase,
    Decrease,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    NotEq,
    StrictEq,
    StrictNotEq,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    ShiftLeft,
    ShiftRight,
    ShiftRightUnsigned,
    InstanceOf,
    In,
    Is,
    As,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Assign,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    BitAnd,
    BitOr,
    BitXor,
    ShiftLeft,
    ShiftRight,
    ShiftRightUnsigned,
}

impl Expr {
    /// Walks the expression tree (pre-order). Used by tests and semantic
    /// checks; keeps the AST module free of visitor traits.
    pub fn walk<'a>(&'a self, visit: &mut impl FnMut(&'a Expr)) {
        visit(self);
        match &self.kind {
            ExprKind::Member { target, .. } => target.walk(visit),
            ExprKind::Index { target, index } => {
                target.walk(visit);
                index.walk(visit);
            }
            ExprKind::Call { callee, args } | ExprKind::New { callee, args } => {
                callee.walk(visit);
                for arg in args {
                    arg.walk(visit);
                }
            }
            ExprKind::Unary { expr, .. } | ExprKind::Postfix { expr, .. } => expr.walk(visit),
            ExprKind::Binary { left, right, .. } => {
                left.walk(visit);
                right.walk(visit);
            }
            ExprKind::Assign { target, value, .. } => {
                target.walk(visit);
                value.walk(visit);
            }
            ExprKind::Conditional {
                condition,
                then_expr,
                else_expr,
            } => {
                condition.walk(visit);
                then_expr.walk(visit);
                else_expr.walk(visit);
            }
            ExprKind::Array(items) => {
                for item in items {
                    item.walk(visit);
                }
            }
            ExprKind::Object(fields) => {
                for (_, value) in fields {
                    value.walk(visit);
                }
            }
            ExprKind::Paren(inner) => inner.walk(visit),
            ExprKind::Number(_)
            | ExprKind::String(_)
            | ExprKind::Bool(_)
            | ExprKind::Null
            | ExprKind::This
            | ExprKind::Ident(_) => {}
        }
    }
}
