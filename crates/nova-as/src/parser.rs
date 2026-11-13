//! Recursive-descent parser for the ActionScript 3 subset.

use crate::ast::*;
use crate::diagnostic::{Diagnostic, Severity};
use crate::token::{Keyword, Span, Symbol, Token, TokenKind};

pub struct ParseResult {
    pub unit: CompilationUnit,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn parse_tokens(tokens: &[Token]) -> ParseResult {
    let mut parser = Parser::new(tokens);
    let unit = parser.compilation_unit();
    ParseResult {
        unit,
        diagnostics: parser.diagnostics,
    }
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Parser<'a> {
    fn new(tokens: &'a [Token]) -> Self {
        Self {
            tokens,
            pos: 0,
            diagnostics: Vec::new(),
        }
    }

    fn current(&self) -> &Token {
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    fn peek(&self, offset: usize) -> &Token {
        &self.tokens[(self.pos + offset).min(self.tokens.len() - 1)]
    }

    fn advance(&mut self) -> Token {
        let token = self.current().clone();
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        token
    }

    fn at_end(&self) -> bool {
        matches!(self.current().kind, TokenKind::Eof)
    }

    fn check_symbol(&self, symbol: Symbol) -> bool {
        self.current().is_symbol(symbol)
    }

    fn check_keyword(&self, keyword: Keyword) -> bool {
        self.current().is_keyword(keyword)
    }

    fn eat_symbol(&mut self, symbol: Symbol) -> bool {
        if self.check_symbol(symbol) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn eat_keyword(&mut self, keyword: Keyword) -> bool {
        if self.check_keyword(keyword) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect_symbol(&mut self, symbol: Symbol, context: &str) -> bool {
        if self.eat_symbol(symbol) {
            true
        } else {
            self.error(
                format!(
                    "'{}' erwartet {context}, gefunden {}",
                    symbol.as_str(),
                    self.current()
                ),
                self.current().span,
            );
            false
        }
    }

    fn expect_ident(&mut self, context: &str) -> Option<(String, Span)> {
        if let TokenKind::Ident(name) = &self.current().kind {
            let name = name.clone();
            let span = self.current().span;
            self.advance();
            Some((name, span))
        } else {
            self.error(
                format!("Bezeichner erwartet {context}, gefunden {}", self.current()),
                self.current().span,
            );
            None
        }
    }

    fn error(&mut self, message: impl Into<String>, span: Span) {
        self.diagnostics.push(Diagnostic::error(message, span));
    }

    fn prev_span(&self) -> Span {
        self.tokens[self.pos.saturating_sub(1).min(self.tokens.len() - 1)].span
    }

    fn synchronize(&mut self) {
        loop {
            if self.at_end() {
                return;
            }
            if self.check_symbol(Symbol::Semicolon) {
                self.advance();
                return;
            }
            if self.check_symbol(Symbol::RBrace) {
                return;
            }
            self.advance();
        }
    }

    // ---------------------------------------------------------------- top level

    fn compilation_unit(&mut self) -> CompilationUnit {
        let start = self.current().span;
        let mut package = None;
        let mut imports = Vec::new();
        let mut classes = Vec::new();
        let mut functions = Vec::new();

        if self.eat_keyword(Keyword::Package) {
            if !self.check_symbol(Symbol::LBrace) {
                let (name, _) = self.qualified_name("nach 'package'");
                if !name.is_empty() {
                    package = Some(name);
                }
            }
            if self.expect_symbol(Symbol::LBrace, "nach 'package'") {
                self.package_members(&mut imports, &mut classes, &mut functions);
                self.expect_symbol(Symbol::RBrace, "am Ende des Pakets");
            }
        } else {
            self.package_members(&mut imports, &mut classes, &mut functions);
        }

        CompilationUnit {
            package,
            imports,
            classes,
            functions,
            span: start.join(self.prev_span()),
        }
    }

    fn package_members(
        &mut self,
        imports: &mut Vec<ImportDecl>,
        classes: &mut Vec<ClassDecl>,
        functions: &mut Vec<FunctionDecl>,
    ) {
        while !self.at_end() && !self.check_symbol(Symbol::RBrace) {
            let before = self.pos;
            let modifiers = self.modifiers();
            if self.check_keyword(Keyword::Import) {
                imports.push(self.import_decl());
            } else if self.check_keyword(Keyword::Interface) {
                let span = self.advance().span;
                self.error("'interface' wird vom ABC-Compiler nicht unterstützt", span);
                // Consume the declaration body so following members can still
                // be parsed and reported on their real source lines.
                while !self.at_end() && !self.check_symbol(Symbol::RBrace) {
                    self.advance();
                }
                if self.check_symbol(Symbol::RBrace) {
                    self.advance();
                }
            } else if self.check_keyword(Keyword::Class) {
                classes.push(self.class_decl(modifiers));
            } else if self.check_keyword(Keyword::Function) {
                functions.push(self.method_decl(modifiers, false));
            } else {
                self.error(
                    format!(
                        "Klasse, 'import' oder 'function' erwartet, gefunden {}",
                        self.current()
                    ),
                    self.current().span,
                );
                self.synchronize();
            }
            if self.pos == before {
                self.advance();
            }
        }
    }

    fn import_decl(&mut self) -> ImportDecl {
        let start = self.advance().span; // import
        let mut path = String::new();
        if let Some((first, _)) = self.expect_ident("nach 'import'") {
            path.push_str(&first);
        }
        while self.eat_symbol(Symbol::Dot) {
            if self.eat_symbol(Symbol::Star) {
                path.push_str(".*");
                break;
            }
            match self.expect_ident("im Importpfad") {
                Some((part, _)) => {
                    path.push('.');
                    path.push_str(&part);
                }
                None => break,
            }
        }
        self.expect_symbol(Symbol::Semicolon, "nach dem Import");
        ImportDecl {
            path,
            span: start.join(self.prev_span()),
        }
    }

    fn class_decl(&mut self, modifiers: Modifiers) -> ClassDecl {
        let start = self.advance().span; // class
        let name = self
            .expect_ident("nach 'class'")
            .map(|(name, _)| name)
            .unwrap_or_else(|| "<fehler>".into());

        let mut base = None;
        if self.eat_keyword(Keyword::Extends) {
            let (name, _) = self.qualified_name("nach 'extends'");
            if !name.is_empty() {
                base = Some(name);
            }
        }

        let mut interfaces = Vec::new();
        if self.eat_keyword(Keyword::Implements) {
            loop {
                let (name, _) = self.qualified_name("nach 'implements'");
                if !name.is_empty() {
                    interfaces.push(name);
                }
                if !self.eat_symbol(Symbol::Comma) {
                    break;
                }
            }
        }

        let mut members = Vec::new();
        if self.expect_symbol(Symbol::LBrace, "vor dem Klassenkörper") {
            while !self.at_end() && !self.check_symbol(Symbol::RBrace) {
                let before = self.pos;
                let member_modifiers = self.modifiers();
                if self.check_keyword(Keyword::Function) {
                    members.push(Member::Method(self.method_decl(member_modifiers, true)));
                } else if self.check_keyword(Keyword::Var) || self.check_keyword(Keyword::Const) {
                    members.push(Member::Field(self.field_decl(member_modifiers)));
                } else {
                    self.error(
                        format!(
                            "'function', 'var' oder 'const' erwartet, gefunden {}",
                            self.current()
                        ),
                        self.current().span,
                    );
                    self.synchronize();
                }
                if self.pos == before {
                    self.advance();
                }
            }
            self.expect_symbol(Symbol::RBrace, "am Ende der Klasse");
        }

        ClassDecl {
            name,
            modifiers,
            base,
            interfaces,
            members,
            span: start.join(self.prev_span()),
        }
    }

    fn field_decl(&mut self, modifiers: Modifiers) -> FieldDecl {
        let start = self.advance().span; // var | const
        let is_const = self.tokens[self.pos - 1].is_keyword(Keyword::Const);
        let name = self
            .expect_ident("nach 'var'/'const'")
            .map(|(name, _)| name)
            .unwrap_or_else(|| "<fehler>".into());
        let ty = if self.eat_symbol(Symbol::Colon) {
            self.type_ref()
        } else {
            None
        };
        let value = if self.eat_symbol(Symbol::Assign) {
            Some(self.expression())
        } else {
            None
        };
        self.expect_symbol(Symbol::Semicolon, "nach der Felddeklaration");
        FieldDecl {
            name,
            ty,
            value,
            is_const,
            modifiers,
            span: start.join(self.prev_span()),
        }
    }

    fn method_decl(&mut self, modifiers: Modifiers, in_class: bool) -> FunctionDecl {
        let start = self.advance().span; // function
        let name = if in_class {
            match &self.current().kind {
                TokenKind::Ident(name) => {
                    let name = name.clone();
                    self.advance();
                    name
                }
                TokenKind::Keyword(Keyword::Get) | TokenKind::Keyword(Keyword::Set) => {
                    let kind = self.advance();
                    let suffix = match kind.kind {
                        TokenKind::Keyword(Keyword::Get) => "get",
                        _ => "set",
                    };
                    match self.expect_ident("nach Accessor") {
                        Some((name, _)) => format!("{suffix} {name}"),
                        None => "<fehler>".into(),
                    }
                }
                _ => {
                    self.error(
                        format!("Funktionsname erwartet, gefunden {}", self.current()),
                        self.current().span,
                    );
                    "<fehler>".into()
                }
            }
        } else {
            self.expect_ident("nach 'function'")
                .map(|(name, _)| name)
                .unwrap_or_else(|| "<fehler>".into())
        };

        let mut params = Vec::new();
        if self.expect_symbol(Symbol::LParen, "nach dem Funktionsnamen") {
            if !self.check_symbol(Symbol::RParen) {
                loop {
                    params.push(self.param());
                    if !self.eat_symbol(Symbol::Comma) {
                        break;
                    }
                }
            }
            self.expect_symbol(Symbol::RParen, "nach den Parametern");
        }

        let return_type = if self.eat_symbol(Symbol::Colon) {
            self.type_ref()
        } else {
            None
        };

        let body = if self.check_symbol(Symbol::LBrace) {
            Some(self.block())
        } else {
            self.expect_symbol(Symbol::Semicolon, "vor dem Funktionskörper");
            None
        };

        FunctionDecl {
            name,
            params,
            return_type,
            body,
            modifiers,
            span: start.join(self.prev_span()),
        }
    }

    fn param(&mut self) -> Param {
        let start = self.current().span;
        let name = self
            .expect_ident("als Parametername")
            .map(|(name, _)| name)
            .unwrap_or_else(|| "<fehler>".into());
        let ty = if self.eat_symbol(Symbol::Colon) {
            self.type_ref()
        } else {
            None
        };
        let default = if self.eat_symbol(Symbol::Assign) {
            Some(self.expression())
        } else {
            None
        };
        Param {
            name,
            ty,
            default,
            span: start.join(self.prev_span()),
        }
    }

    fn modifiers(&mut self) -> Modifiers {
        let mut modifiers = Modifiers::default();
        loop {
            let keyword = match &self.current().kind {
                TokenKind::Keyword(keyword) => *keyword,
                _ => break,
            };
            match keyword {
                Keyword::Public => modifiers.public = true,
                Keyword::Private => modifiers.private = true,
                Keyword::Protected => modifiers.protected = true,
                Keyword::Internal => modifiers.internal = true,
                Keyword::Static => modifiers.static_modifier = true,
                Keyword::Override => modifiers.override_modifier = true,
                Keyword::Final => modifiers.final_modifier = true,
                Keyword::Dynamic => modifiers.dynamic = true,
                Keyword::Native => modifiers.native = true,
                _ => break,
            }
            self.advance();
        }
        modifiers
    }

    fn qualified_name(&mut self, context: &str) -> (String, Span) {
        let start = self.current().span;
        let mut name = match self.current().kind {
            TokenKind::Ident(_) => self
                .expect_ident(context)
                .map(|(name, _)| name)
                .unwrap_or_default(),
            _ => {
                self.error(
                    format!("Name erwartet {context}, gefunden {}", self.current()),
                    self.current().span,
                );
                String::new()
            }
        };
        while self.eat_symbol(Symbol::Dot) {
            match self.expect_ident("im qualifizierten Namen") {
                Some((part, _)) => {
                    name.push('.');
                    name.push_str(&part);
                }
                None => break,
            }
        }
        (name, start.join(self.prev_span()))
    }

    /// Parses the type name on the right-hand side of `is`, `as` and
    /// `instanceof`. Unlike a general expression this accepts primitive type
    /// keywords such as `String` or `int`, which are class references there.
    fn type_expression(&mut self) -> Expr {
        let start = self.current().span;
        let name = match &self.current().kind {
            TokenKind::Ident(_) => self.qualified_name("als Typ").0,
            TokenKind::Keyword(keyword) => {
                let name = match keyword {
                    Keyword::Void => "void",
                    Keyword::Int => "int",
                    Keyword::Uint => "uint",
                    Keyword::Number => "Number",
                    Keyword::Boolean => "Boolean",
                    Keyword::String => "String",
                    Keyword::Object => "Object",
                    Keyword::Array => "Array",
                    _ => {
                        self.error("Typname erwartet", self.current().span);
                        self.advance();
                        return Expr {
                            kind: ExprKind::Ident("<fehler>".into()),
                            span: start,
                        };
                    }
                };
                self.advance();
                name.to_string()
            }
            _ => {
                self.error(
                    format!("Typname erwartet, gefunden {}", self.current()),
                    self.current().span,
                );
                self.advance();
                "<fehler>".into()
            }
        };
        if self.check_symbol(Symbol::LBracket) && self.peek(1).is_symbol(Symbol::RBracket) {
            self.error(
                "Array-Typen werden vom ABC-Compiler nicht unterstützt",
                self.current().span,
            );
            self.advance();
            self.advance();
        }
        Expr {
            kind: ExprKind::Ident(name),
            span: start.join(self.prev_span()),
        }
    }

    fn type_ref(&mut self) -> Option<TypeRef> {
        let base = match &self.current().kind {
            TokenKind::Ident(name) => {
                let name = name.clone();
                self.advance();
                Some(name)
            }
            TokenKind::Keyword(keyword) => {
                let name = match keyword {
                    Keyword::Void => "void",
                    Keyword::Int => "int",
                    Keyword::Uint => "uint",
                    Keyword::Number => "Number",
                    Keyword::Boolean => "Boolean",
                    Keyword::String => "String",
                    Keyword::Object => "Object",
                    Keyword::Array => "Array",
                    _ => {
                        self.error("Typname erwartet", self.current().span);
                        return None;
                    }
                };
                self.advance();
                Some(name.to_string())
            }
            _ => {
                self.error(
                    format!("Typname erwartet, gefunden {}", self.current()),
                    self.current().span,
                );
                return None;
            }
        };
        let mut array_depth = 0;
        while self.check_symbol(Symbol::LBracket) && self.peek(1).is_symbol(Symbol::RBracket) {
            self.advance();
            self.advance();
            array_depth += 1;
        }
        Some(TypeRef {
            name: base?,
            array_depth,
        })
    }

    // ----------------------------------------------------------------- statements

    fn block(&mut self) -> Block {
        let start = self.current().span;
        self.expect_symbol(Symbol::LBrace, "für einen Block");
        let mut statements = Vec::new();
        while !self.at_end() && !self.check_symbol(Symbol::RBrace) {
            let before = self.pos;
            statements.push(self.statement());
            if self.pos == before {
                self.advance();
            }
        }
        self.expect_symbol(Symbol::RBrace, "am Ende des Blocks");
        Block {
            statements,
            span: start.join(self.prev_span()),
        }
    }

    fn statement(&mut self) -> Stmt {
        let start = self.current().span;
        if self.check_keyword(Keyword::Switch) {
            return self.unsupported_statement("switch");
        }
        if self.check_keyword(Keyword::Try) {
            return self.unsupported_statement("try");
        }
        if self.check_keyword(Keyword::With) {
            return self.unsupported_statement("with");
        }
        if self.check_symbol(Symbol::LBrace) {
            let block = self.block();
            return Stmt {
                span: block.span,
                kind: StmtKind::Block(block),
            };
        }
        if self.check_keyword(Keyword::Var) || self.check_keyword(Keyword::Const) {
            let decl = self.var_decl();
            self.expect_symbol(Symbol::Semicolon, "nach der Variablendeklaration");
            return Stmt {
                span: decl.span,
                kind: StmtKind::Var(decl),
            };
        }
        if self.eat_keyword(Keyword::If) {
            self.expect_symbol(Symbol::LParen, "nach 'if'");
            let condition = self.expression();
            self.expect_symbol(Symbol::RParen, "nach der if-Bedingung");
            let then_branch = Box::new(self.statement());
            let else_branch = if self.eat_keyword(Keyword::Else) {
                Some(Box::new(self.statement()))
            } else {
                None
            };
            return Stmt {
                span: start.join(self.prev_span()),
                kind: StmtKind::If {
                    condition,
                    then_branch,
                    else_branch,
                },
            };
        }
        if self.eat_keyword(Keyword::While) {
            self.expect_symbol(Symbol::LParen, "nach 'while'");
            let condition = self.expression();
            self.expect_symbol(Symbol::RParen, "nach der while-Bedingung");
            let body = Box::new(self.statement());
            return Stmt {
                span: start.join(self.prev_span()),
                kind: StmtKind::While { condition, body },
            };
        }
        if self.eat_keyword(Keyword::Do) {
            let body = Box::new(self.statement());
            if !self.eat_keyword(Keyword::While) {
                self.error("'while' nach 'do' erwartet", self.current().span);
            }
            self.expect_symbol(Symbol::LParen, "nach 'while'");
            let condition = self.expression();
            self.expect_symbol(Symbol::RParen, "nach der do-while-Bedingung");
            self.expect_symbol(Symbol::Semicolon, "nach do-while");
            return Stmt {
                span: start.join(self.prev_span()),
                kind: StmtKind::DoWhile { body, condition },
            };
        }
        if self.eat_keyword(Keyword::For) {
            return self.for_statement(start);
        }
        if self.eat_keyword(Keyword::Return) {
            let value = if self.check_symbol(Symbol::Semicolon) || self.check_symbol(Symbol::RBrace)
            {
                None
            } else {
                Some(self.expression())
            };
            self.expect_symbol(Symbol::Semicolon, "nach 'return'");
            return Stmt {
                span: start.join(self.prev_span()),
                kind: StmtKind::Return(value),
            };
        }
        if self.eat_keyword(Keyword::Break) {
            self.expect_symbol(Symbol::Semicolon, "nach 'break'");
            return Stmt {
                span: start.join(self.prev_span()),
                kind: StmtKind::Break,
            };
        }
        if self.eat_keyword(Keyword::Continue) {
            self.expect_symbol(Symbol::Semicolon, "nach 'continue'");
            return Stmt {
                span: start.join(self.prev_span()),
                kind: StmtKind::Continue,
            };
        }
        if self.eat_keyword(Keyword::Throw) {
            let value = self.expression();
            self.expect_symbol(Symbol::Semicolon, "nach 'throw'");
            return Stmt {
                span: start.join(self.prev_span()),
                kind: StmtKind::Throw(value),
            };
        }
        if self.eat_symbol(Symbol::Semicolon) {
            return Stmt {
                span: start,
                kind: StmtKind::Empty,
            };
        }

        let expr = self.expression();
        let span = expr.span;
        self.expect_symbol(Symbol::Semicolon, "nach dem Ausdruck");
        Stmt {
            kind: StmtKind::Expr(expr),
            span,
        }
    }

    fn unsupported_statement(&mut self, name: &str) -> Stmt {
        let span = self.current().span;
        self.error(
            format!("'{name}' wird vom ABC-Compiler nicht unterstützt"),
            span,
        );
        self.skip_unsupported_statement();
        Stmt {
            span,
            kind: StmtKind::Empty,
        }
    }

    /// Consumes an unsupported statement so that the following statement can
    /// still be parsed. Balanced parentheses/brackets/blocks are skipped without
    /// interpreting their contents.
    fn skip_unsupported_statement(&mut self) {
        self.advance();
        let mut depth = 0usize;
        while !self.at_end() {
            if self.check_symbol(Symbol::LParen)
                || self.check_symbol(Symbol::LBracket)
                || self.check_symbol(Symbol::LBrace)
            {
                depth += 1;
                self.advance();
            } else if self.check_symbol(Symbol::RParen)
                || self.check_symbol(Symbol::RBracket)
                || self.check_symbol(Symbol::RBrace)
            {
                if depth == 0 {
                    break;
                }
                depth -= 1;
                self.advance();
                if depth == 0 && self.prev_is_rbrace() {
                    if self.check_keyword(Keyword::Catch) || self.check_keyword(Keyword::Finally) {
                        continue;
                    }
                    break;
                }
            } else if depth == 0 && self.check_symbol(Symbol::Semicolon) {
                self.advance();
                break;
            } else {
                self.advance();
            }
        }
    }

    fn prev_is_rbrace(&self) -> bool {
        self.pos > 0 && self.tokens[self.pos - 1].is_symbol(Symbol::RBrace)
    }

    fn for_in_ahead(&self) -> bool {
        let mut index = self.pos;
        let mut depth = 0usize;
        while let Some(token) = self.tokens.get(index) {
            match &token.kind {
                TokenKind::Symbol(Symbol::LParen) => depth += 1,
                TokenKind::Symbol(Symbol::RParen) => {
                    if depth == 0 {
                        return false;
                    }
                    depth -= 1;
                }
                TokenKind::Symbol(Symbol::Semicolon) if depth == 0 => return false,
                TokenKind::Keyword(Keyword::In) if depth == 0 => return true,
                _ => {}
            }
            index += 1;
        }
        false
    }

    /// We are just after the opening parenthesis of a `for` header. Consume the
    /// rest of the header and the loop body without interpreting it.
    fn skip_for_in(&mut self) {
        let mut depth = 0usize;
        while !self.at_end() {
            if self.check_symbol(Symbol::LParen) {
                depth += 1;
                self.advance();
            } else if self.check_symbol(Symbol::RParen) {
                self.advance();
                if depth == 0 {
                    break;
                }
                depth -= 1;
            } else {
                self.advance();
            }
        }
        if !self.at_end() {
            self.statement();
        }
    }

    fn for_statement(&mut self, start: Span) -> Stmt {
        self.expect_symbol(Symbol::LParen, "nach 'for'");
        if self.for_in_ahead() {
            self.error(
                "'for-in' wird vom ABC-Compiler nicht unterstützt",
                self.current().span,
            );
            self.skip_for_in();
            return Stmt {
                span: start.join(self.prev_span()),
                kind: StmtKind::Empty,
            };
        }
        let init = if self.check_symbol(Symbol::Semicolon) {
            ForInit::Empty
        } else if self.check_keyword(Keyword::Var) || self.check_keyword(Keyword::Const) {
            ForInit::Var(self.var_decl())
        } else {
            ForInit::Expr(self.expression())
        };
        self.expect_symbol(Symbol::Semicolon, "nach der for-Initialisierung");
        let condition = if self.check_symbol(Symbol::Semicolon) {
            None
        } else {
            Some(self.expression())
        };
        self.expect_symbol(Symbol::Semicolon, "nach der for-Bedingung");
        let update = if self.check_symbol(Symbol::RParen) {
            None
        } else {
            Some(self.expression())
        };
        self.expect_symbol(Symbol::RParen, "nach dem for-Kopf");
        let body = Box::new(self.statement());
        Stmt {
            span: start.join(self.prev_span()),
            kind: StmtKind::For {
                init,
                condition,
                update,
                body,
            },
        }
    }

    fn var_decl(&mut self) -> VarDecl {
        let start = self.current().span;
        let is_const = self.current().is_keyword(Keyword::Const);
        self.advance();
        let name = self
            .expect_ident("nach 'var'/'const'")
            .map(|(name, _)| name)
            .unwrap_or_else(|| "<fehler>".into());
        let ty = if self.eat_symbol(Symbol::Colon) {
            self.type_ref()
        } else {
            None
        };
        let value = if self.eat_symbol(Symbol::Assign) {
            Some(self.expression())
        } else {
            None
        };
        VarDecl {
            name,
            ty,
            value,
            is_const,
            span: start.join(self.prev_span()),
        }
    }

    // ------------------------------------------------------------------ expressions

    fn expression(&mut self) -> Expr {
        self.assignment()
    }

    fn assignment(&mut self) -> Expr {
        let left = self.conditional();
        let op = match &self.current().kind {
            TokenKind::Symbol(Symbol::Assign) => Some(AssignOp::Assign),
            TokenKind::Symbol(Symbol::PlusAssign) => Some(AssignOp::Add),
            TokenKind::Symbol(Symbol::MinusAssign) => Some(AssignOp::Sub),
            TokenKind::Symbol(Symbol::StarAssign) => Some(AssignOp::Mul),
            TokenKind::Symbol(Symbol::SlashAssign) => Some(AssignOp::Div),
            TokenKind::Symbol(Symbol::PercentAssign) => Some(AssignOp::Mod),
            TokenKind::Symbol(Symbol::AmpAssign) => Some(AssignOp::BitAnd),
            TokenKind::Symbol(Symbol::PipeAssign) => Some(AssignOp::BitOr),
            TokenKind::Symbol(Symbol::CaretAssign) => Some(AssignOp::BitXor),
            TokenKind::Symbol(Symbol::ShiftLeftAssign) => Some(AssignOp::ShiftLeft),
            TokenKind::Symbol(Symbol::ShiftRightAssign) => Some(AssignOp::ShiftRight),
            TokenKind::Symbol(Symbol::ShiftRightUnsignedAssign) => {
                Some(AssignOp::ShiftRightUnsigned)
            }
            _ => None,
        };
        if let Some(op) = op {
            self.advance();
            let value = self.assignment();
            let span = left.span.join(value.span);
            return Expr {
                kind: ExprKind::Assign {
                    op,
                    target: Box::new(left),
                    value: Box::new(value),
                },
                span,
            };
        }
        left
    }

    fn conditional(&mut self) -> Expr {
        let condition = self.binary(1);
        if self.eat_symbol(Symbol::Question) {
            let then_expr = self.assignment();
            self.expect_symbol(Symbol::Colon, "im Bedingungsausdruck");
            let else_expr = self.assignment();
            let span = condition.span.join(else_expr.span);
            return Expr {
                kind: ExprKind::Conditional {
                    condition: Box::new(condition),
                    then_expr: Box::new(then_expr),
                    else_expr: Box::new(else_expr),
                },
                span,
            };
        }
        condition
    }

    fn binary(&mut self, min_precedence: u8) -> Expr {
        let mut left = self.unary();
        while let Some((precedence, op)) = binary_operator(&self.current().kind) {
            if precedence < min_precedence {
                break;
            }
            self.advance();
            let right = if matches!(op, BinaryOp::Is | BinaryOp::As | BinaryOp::InstanceOf) {
                self.type_expression()
            } else {
                self.binary(precedence + 1)
            };
            let span = left.span.join(right.span);
            left = Expr {
                kind: ExprKind::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            };
        }
        left
    }

    fn unary(&mut self) -> Expr {
        let start = self.current().span;
        if self.check_symbol(Symbol::Increase) || self.check_symbol(Symbol::Decrease) {
            let increase = self.check_symbol(Symbol::Increase);
            self.advance();
            let target = self.unary();
            let span = start.join(target.span);
            let one = Expr {
                kind: ExprKind::Number("1".into()),
                span: start,
            };
            return Expr {
                kind: ExprKind::Assign {
                    op: if increase {
                        AssignOp::Add
                    } else {
                        AssignOp::Sub
                    },
                    target: Box::new(target),
                    value: Box::new(one),
                },
                span,
            };
        }
        let op = match &self.current().kind {
            TokenKind::Symbol(Symbol::Plus) => Some(UnaryOp::Plus),
            TokenKind::Symbol(Symbol::Minus) => Some(UnaryOp::Minus),
            TokenKind::Symbol(Symbol::Not) => Some(UnaryOp::Not),
            TokenKind::Symbol(Symbol::Tilde) => Some(UnaryOp::BitNot),
            TokenKind::Keyword(Keyword::Typeof) => Some(UnaryOp::Typeof),
            TokenKind::Keyword(Keyword::Void) => Some(UnaryOp::Void),
            TokenKind::Keyword(Keyword::Delete) => Some(UnaryOp::Delete),
            _ => None,
        };
        if let Some(op) = op {
            self.advance();
            let expr = self.unary();
            let span = start.join(expr.span);
            return Expr {
                kind: ExprKind::Unary {
                    op,
                    expr: Box::new(expr),
                },
                span,
            };
        }
        if self.eat_keyword(Keyword::New) {
            let callee = self.unary();
            let (callee, args, end) = match callee.kind {
                ExprKind::Call { callee, args } => {
                    let span = callee.span;
                    (callee, args, span)
                }
                _ => {
                    let span = callee.span;
                    (Box::new(callee), Vec::new(), span)
                }
            };
            return Expr {
                kind: ExprKind::New { callee, args },
                span: start.join(end),
            };
        }
        self.postfix()
    }

    fn postfix(&mut self) -> Expr {
        let mut expr = self.primary();
        loop {
            if self.eat_symbol(Symbol::Dot) {
                let (name, _) = match &self.current().kind {
                    TokenKind::Ident(_) => self.expect_ident("nach '.'").unwrap(),
                    TokenKind::Keyword(keyword) => {
                        let name = keyword.as_str().to_string();
                        self.advance();
                        (name, self.prev_span())
                    }
                    _ => {
                        self.error(
                            format!("Membername erwartet, gefunden {}", self.current()),
                            self.current().span,
                        );
                        break;
                    }
                };
                let span = expr.span.join(self.prev_span());
                expr = Expr {
                    kind: ExprKind::Member {
                        target: Box::new(expr),
                        name,
                    },
                    span,
                };
            } else if self.eat_symbol(Symbol::LBracket) {
                let index = self.expression();
                self.expect_symbol(Symbol::RBracket, "nach dem Index");
                let span = expr.span.join(self.prev_span());
                expr = Expr {
                    kind: ExprKind::Index {
                        target: Box::new(expr),
                        index: Box::new(index),
                    },
                    span,
                };
            } else if self.check_symbol(Symbol::LParen) {
                let args = self.arguments();
                let span = expr.span.join(self.prev_span());
                expr = Expr {
                    kind: ExprKind::Call {
                        callee: Box::new(expr),
                        args,
                    },
                    span,
                };
            } else if self.check_symbol(Symbol::Increase) {
                self.advance();
                let span = expr.span.join(self.prev_span());
                expr = Expr {
                    kind: ExprKind::Postfix {
                        op: PostfixOp::Increase,
                        expr: Box::new(expr),
                    },
                    span,
                };
            } else if self.check_symbol(Symbol::Decrease) {
                self.advance();
                let span = expr.span.join(self.prev_span());
                expr = Expr {
                    kind: ExprKind::Postfix {
                        op: PostfixOp::Decrease,
                        expr: Box::new(expr),
                    },
                    span,
                };
            } else {
                break;
            }
        }
        expr
    }

    fn arguments(&mut self) -> Vec<Expr> {
        let mut args = Vec::new();
        if !self.expect_symbol(Symbol::LParen, "für Argumente") {
            return args;
        }
        if !self.check_symbol(Symbol::RParen) {
            loop {
                args.push(self.expression());
                if !self.eat_symbol(Symbol::Comma) {
                    break;
                }
            }
        }
        self.expect_symbol(Symbol::RParen, "nach den Argumenten");
        args
    }

    fn primary(&mut self) -> Expr {
        let token = self.current().clone();
        let span = token.span;
        match token.kind {
            TokenKind::Number(value) => {
                self.advance();
                Expr {
                    kind: ExprKind::Number(value),
                    span,
                }
            }
            TokenKind::Str(value) => {
                self.advance();
                Expr {
                    kind: ExprKind::String(value),
                    span,
                }
            }
            TokenKind::Ident(name) => {
                self.advance();
                Expr {
                    kind: ExprKind::Ident(name),
                    span,
                }
            }
            TokenKind::Keyword(Keyword::True) => {
                self.advance();
                Expr {
                    kind: ExprKind::Bool(true),
                    span,
                }
            }
            TokenKind::Keyword(Keyword::False) => {
                self.advance();
                Expr {
                    kind: ExprKind::Bool(false),
                    span,
                }
            }
            TokenKind::Keyword(Keyword::Null) => {
                self.advance();
                Expr {
                    kind: ExprKind::Null,
                    span,
                }
            }
            TokenKind::Keyword(Keyword::This) => {
                self.advance();
                Expr {
                    kind: ExprKind::This,
                    span,
                }
            }
            TokenKind::Symbol(Symbol::LParen) => {
                self.advance();
                let inner = self.expression();
                self.expect_symbol(Symbol::RParen, "nach dem Klammerausdruck");
                let full = span.join(self.prev_span());
                Expr {
                    kind: ExprKind::Paren(Box::new(inner)),
                    span: full,
                }
            }
            TokenKind::Symbol(Symbol::LBracket) => {
                self.advance();
                let mut items = Vec::new();
                if !self.check_symbol(Symbol::RBracket) {
                    loop {
                        items.push(self.expression());
                        if !self.eat_symbol(Symbol::Comma) {
                            break;
                        }
                    }
                }
                self.expect_symbol(Symbol::RBracket, "nach dem Array-Literal");
                Expr {
                    kind: ExprKind::Array(items),
                    span: span.join(self.prev_span()),
                }
            }
            TokenKind::Symbol(Symbol::LBrace) => {
                self.advance();
                let mut fields = Vec::new();
                if !self.check_symbol(Symbol::RBrace) {
                    loop {
                        let key = match &self.current().kind {
                            TokenKind::Ident(name) => name.clone(),
                            TokenKind::Str(value) => value.clone(),
                            TokenKind::Number(value) => value.clone(),
                            _ => {
                                self.error("Eigenschaftsname erwartet", self.current().span);
                                String::new()
                            }
                        };
                        self.advance();
                        self.expect_symbol(Symbol::Colon, "im Objekt-Literal");
                        let value = self.expression();
                        fields.push((key, value));
                        if !self.eat_symbol(Symbol::Comma) {
                            break;
                        }
                    }
                }
                self.expect_symbol(Symbol::RBrace, "nach dem Objekt-Literal");
                Expr {
                    kind: ExprKind::Object(fields),
                    span: span.join(self.prev_span()),
                }
            }
            other => {
                self.error(
                    format!(
                        "Ausdruck erwartet, gefunden {}",
                        TokenKind::describe(&other)
                    ),
                    span,
                );
                self.advance();
                Expr {
                    kind: ExprKind::Ident("<fehler>".into()),
                    span,
                }
            }
        }
    }
}

fn binary_operator(kind: &TokenKind) -> Option<(u8, BinaryOp)> {
    Some(match kind {
        TokenKind::Symbol(Symbol::OrOr) => (1, BinaryOp::Or),
        TokenKind::Symbol(Symbol::AndAnd) => (2, BinaryOp::And),
        TokenKind::Symbol(Symbol::Pipe) => (3, BinaryOp::BitOr),
        TokenKind::Symbol(Symbol::Caret) => (4, BinaryOp::BitXor),
        TokenKind::Symbol(Symbol::Amp) => (5, BinaryOp::BitAnd),
        TokenKind::Symbol(Symbol::Eq) => (6, BinaryOp::Eq),
        TokenKind::Symbol(Symbol::NotEq) => (6, BinaryOp::NotEq),
        TokenKind::Symbol(Symbol::StrictEq) => (6, BinaryOp::StrictEq),
        TokenKind::Symbol(Symbol::StrictNotEq) => (6, BinaryOp::StrictNotEq),
        TokenKind::Symbol(Symbol::Lt) => (7, BinaryOp::Lt),
        TokenKind::Symbol(Symbol::Le) => (7, BinaryOp::Le),
        TokenKind::Symbol(Symbol::Gt) => (7, BinaryOp::Gt),
        TokenKind::Symbol(Symbol::Ge) => (7, BinaryOp::Ge),
        TokenKind::Keyword(Keyword::Instanceof) => (7, BinaryOp::InstanceOf),
        TokenKind::Keyword(Keyword::In) => (7, BinaryOp::In),
        TokenKind::Keyword(Keyword::Is) => (7, BinaryOp::Is),
        TokenKind::Keyword(Keyword::As) => (7, BinaryOp::As),
        TokenKind::Symbol(Symbol::ShiftLeft) => (8, BinaryOp::ShiftLeft),
        TokenKind::Symbol(Symbol::ShiftRight) => (8, BinaryOp::ShiftRight),
        TokenKind::Symbol(Symbol::ShiftRightUnsigned) => (8, BinaryOp::ShiftRightUnsigned),
        TokenKind::Symbol(Symbol::Plus) => (9, BinaryOp::Add),
        TokenKind::Symbol(Symbol::Minus) => (9, BinaryOp::Sub),
        TokenKind::Symbol(Symbol::Star) => (10, BinaryOp::Mul),
        TokenKind::Symbol(Symbol::Slash) => (10, BinaryOp::Div),
        TokenKind::Symbol(Symbol::Percent) => (10, BinaryOp::Mod),
        _ => return None,
    })
}

/// True when at least one diagnostic has error severity.
pub fn has_errors(diagnostics: &[Diagnostic]) -> bool {
    diagnostics.iter().any(|d| d.severity == Severity::Error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;

    fn parse(source: &str) -> CompilationUnit {
        let lexed = lex(source);
        assert!(lexed.diagnostics.is_empty(), "{:?}", lexed.diagnostics);
        let parsed = parse_tokens(&lexed.tokens);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        parsed.unit
    }

    #[test]
    fn parses_package_class_and_methods() {
        let unit = parse(
            r#"package flash.demo {
                import flash.display.MovieClip;
                public class Main extends MovieClip {
                    private var speed:Number = 2.5;
                    public function Main() {
                        addEventListener("enterFrame", update);
                    }
                    private function update(event:Event):void {
                        this.x += speed;
                    }
                }
            }"#,
        );
        assert_eq!(unit.package.as_deref(), Some("flash.demo"));
        assert_eq!(unit.imports[0].path, "flash.display.MovieClip");
        let class = &unit.classes[0];
        assert_eq!(class.name, "Main");
        assert_eq!(class.base.as_deref(), Some("MovieClip"));
        assert!(class.constructor().is_some());
        assert!(class.method("update").is_some());
    }

    #[test]
    fn parses_control_flow() {
        let unit = parse(
            r#"package {
                class Test {
                    function run(n:int):int {
                        var sum:int = 0;
                        for (var i:int = 0; i < n; i++) {
                            if (i % 2 == 0) sum += i; else sum -= 1;
                        }
                        while (sum > 100) { sum--; }
                        return sum;
                    }
                }
            }"#,
        );
        let class = &unit.classes[0];
        let method = class.method("run").unwrap();
        let body = method.body.as_ref().unwrap();
        assert_eq!(body.statements.len(), 4);
        assert!(matches!(body.statements[0].kind, StmtKind::Var(_)));
        assert!(matches!(body.statements[1].kind, StmtKind::For { .. }));
        assert!(matches!(body.statements[2].kind, StmtKind::While { .. }));
        assert!(matches!(body.statements[3].kind, StmtKind::Return(_)));
    }

    #[test]
    fn multiplication_binds_tighter_than_addition() {
        let unit = parse("class T { function f():int { return 1 + 2 * 3; } }");
        let method = unit.classes[0].method("f").unwrap();
        let body = method.body.as_ref().unwrap();
        let StmtKind::Return(Some(expr)) = &body.statements[0].kind else {
            panic!("expected return");
        };
        let ExprKind::Binary { op, right, .. } = &expr.kind else {
            panic!("expected binary");
        };
        assert_eq!(*op, BinaryOp::Add);
        assert!(matches!(
            right.kind,
            ExprKind::Binary {
                op: BinaryOp::Mul,
                ..
            }
        ));
    }

    #[test]
    fn parses_new_and_chained_calls() {
        let unit = parse(
            r#"class T { function f():void {
                var s:Sprite = new Sprite();
                s.graphics.beginFill(0xff0000, 0.5);
                s.graphics.drawRect(0, 0, 10, 10);
            } }"#,
        );
        let body = unit.classes[0].method("f").unwrap().body.as_ref().unwrap();
        let StmtKind::Var(decl) = &body.statements[0].kind else {
            panic!("expected var");
        };
        assert!(matches!(
            decl.value.as_ref().unwrap().kind,
            ExprKind::New { .. }
        ));
    }

    #[test]
    fn reports_missing_semicolon() {
        let lexed = lex("class T { function f() { var x = 1 } }");
        let parsed = parse_tokens(&lexed.tokens);
        assert!(has_errors(&parsed.diagnostics));
    }
}
