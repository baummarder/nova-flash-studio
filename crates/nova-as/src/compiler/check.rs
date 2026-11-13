//! Type checking and unsupported-feature diagnostics for the AS3 subset.

use std::collections::HashMap;

use crate::ast::{
    AssignOp, BinaryOp, Block, ClassDecl, CompilationUnit, Expr, ExprKind, FieldDecl, ForInit,
    FunctionDecl, Member, Modifiers, Stmt, StmtKind, UnaryOp, VarDecl,
};
use crate::diagnostic::Diagnostic;
use crate::token::Span;

use super::types::{
    Access, CallKind, ClassSymbol, ExprInfo, FieldSymbol, MethodSymbol, ParamSymbol, RefKind,
    Scopes, Type, TypeWorld, package_of_qname,
};

/// Result of the type checking pass.
#[derive(Debug, Clone, Default)]
pub struct CheckOutcome {
    pub world: TypeWorld,
    pub expr_info: HashMap<(usize, usize), ExprInfo>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Runs the checker for a compilation unit whose package name is `package`.
pub fn check(unit: &CompilationUnit, package: &str) -> CheckOutcome {
    let mut checker = Checker::new(unit.clone(), package);
    checker.run();
    CheckOutcome {
        world: checker.world,
        expr_info: checker.info,
        diagnostics: checker.diagnostics,
    }
}

struct Checker {
    unit: CompilationUnit,
    world: TypeWorld,
    info: HashMap<(usize, usize), ExprInfo>,
    diagnostics: Vec<Diagnostic>,
    scopes: Scopes,
    current_class: Option<String>,
    is_static: bool,
    is_constructor: bool,
    return_type: Type,
    loop_depth: usize,
}

impl Checker {
    fn new(unit: CompilationUnit, package: &str) -> Self {
        let mut world = TypeWorld::default();
        world.package = package.to_string();
        Self {
            unit,
            world,
            info: HashMap::new(),
            diagnostics: Vec::new(),
            scopes: Scopes::new(),
            current_class: None,
            is_static: false,
            is_constructor: false,
            return_type: Type::Void,
            loop_depth: 0,
        }
    }

    fn error(&mut self, message: impl Into<String>, span: Span) {
        self.diagnostics
            .push(Diagnostic::error(message.into(), span));
    }

    fn run(&mut self) {
        self.collect_imports();
        self.create_classes();
        self.build_classes();
        self.build_functions();
        self.check_field_initializers();
        self.check_method_bodies();
        self.check_top_level_functions();
    }

    // ---------------------------------------------------------------- imports

    fn collect_imports(&mut self) {
        let imports = self.unit.imports.clone();
        for import in &imports {
            if let Some(package) = import.path.strip_suffix(".*") {
                if !self.world.wildcard_imports.iter().any(|p| p == package) {
                    self.world.wildcard_imports.push(package.to_string());
                }
                continue;
            }
            let Some((_, simple)) = import.path.rsplit_once('.').or_else(|| {
                if import.path.is_empty() {
                    None
                } else {
                    Some(("", import.path.as_str()))
                }
            }) else {
                continue;
            };
            if simple.is_empty() {
                continue;
            }
            if let Some(existing) = self.world.imports.get(simple).cloned() {
                if existing != import.path {
                    self.error(
                        format!(
                            "Importname '{simple}' ist mehrdeutig ('{existing}' und '{}')",
                            import.path
                        ),
                        import.span,
                    );
                }
            } else {
                self.world
                    .imports
                    .insert(simple.to_string(), import.path.clone());
            }
        }
    }

    fn create_classes(&mut self) {
        let classes = self.unit.classes.clone();
        for class in &classes {
            let qualified_name = qualify(&self.world.package, &class.name);
            let already_declared = self.world.class(&qualified_name).is_some();
            if already_declared {
                self.error(
                    format!("Klasse '{qualified_name}' ist bereits deklariert"),
                    class.span,
                );
                continue;
            }
            self.world.classes.push(ClassSymbol {
                name: class.name.clone(),
                qualified_name,
                package: self.world.package.clone(),
                base: None,
                extends_movieclip: false,
                is_final: class.modifiers.final_modifier,
                fields: Vec::new(),
                methods: Vec::new(),
                static_fields: Vec::new(),
                static_methods: Vec::new(),
                constructor: None,
                source_line: class.span.line,
            });
        }
    }

    fn build_classes(&mut self) {
        let classes = self.unit.classes.clone();
        for class in &classes {
            self.check_class_modifiers(class);
            let qualified_name = qualify(&self.world.package, &class.name);
            if self.world.class(&qualified_name).is_none() {
                continue;
            }

            let base = self.resolve_base(class);

            let mut fields = Vec::new();
            let mut methods = Vec::new();
            let mut static_fields = Vec::new();
            let mut static_methods = Vec::new();
            let mut constructor = None;

            for member in &class.members {
                match member {
                    Member::Field(field) => {
                        self.check_field_modifiers(field);
                        self.check_access_modifiers(field.modifiers, field.span);
                        let ty = self.resolve_type(field.ty.as_ref(), field.span, "Feld");
                        let symbol = FieldSymbol {
                            name: field.name.clone(),
                            access: access_of(field.modifiers),
                            ty,
                            is_static: field.modifiers.static_modifier,
                            is_const: field.is_const,
                            span: field.span,
                        };
                        if field.modifiers.static_modifier {
                            static_fields.push(symbol);
                        } else {
                            fields.push(symbol);
                        }
                    }
                    Member::Method(function) => {
                        let Some(symbol) = self.build_method(function, &class.name) else {
                            continue;
                        };
                        if symbol.is_constructor {
                            if symbol.is_static {
                                self.error(
                                    "Konstruktoren dürfen nicht 'static' sein",
                                    function.span,
                                );
                            } else {
                                constructor = Some(symbol);
                            }
                        } else if symbol.is_static {
                            static_methods.push(symbol);
                        } else {
                            methods.push(symbol);
                        }
                    }
                }
            }

            if let Some(symbol) = self.world.class_mut(&qualified_name) {
                symbol.base = base;
                symbol.fields = fields;
                symbol.methods = methods;
                symbol.static_fields = static_fields;
                symbol.static_methods = static_methods;
                symbol.constructor = constructor;
            }
        }

        self.check_inheritance_cycles();
        self.mark_movieclip_ancestors();
        self.check_overrides();
        self.check_base_constructors();
    }

    /// Rejects self-extension and cycles in local class hierarchies before any
    /// emitter pass can walk them.
    fn check_inheritance_cycles(&mut self) {
        let classes = self.world.classes.clone();
        for class in &classes {
            let mut seen = vec![class.qualified_name.clone()];
            let mut current = class.base.clone();
            let mut guard = 0;
            while let Some(base) = current {
                if seen.iter().any(|name| name == &base) {
                    self.error(
                        format!(
                            "Zyklische Vererbung: '{}' erbt indirekt von sich selbst",
                            class.qualified_name
                        ),
                        Span::new(0, 0, class.source_line, 1),
                    );
                    break;
                }
                seen.push(base.clone());
                current = self
                    .world
                    .class(&base)
                    .and_then(|base_class| base_class.base.clone());
                guard += 1;
                if guard > 64 {
                    break;
                }
            }
        }
    }

    /// Marks classes whose resolved base chain contains
    /// `flash.display.MovieClip`. This runs after every class has its base, so
    /// declaration order and local intermediate base classes do not matter.
    fn mark_movieclip_ancestors(&mut self) {
        let names: Vec<String> = self
            .world
            .classes
            .iter()
            .map(|class| class.qualified_name.clone())
            .collect();
        for name in names {
            let extends_movieclip = self.is_movieclip_subclass(&name);
            if let Some(class) = self.world.class_mut(&name) {
                class.extends_movieclip = extends_movieclip;
            }
        }
    }

    fn is_movieclip_subclass(&self, class_qname: &str) -> bool {
        let mut current = self.world.class(class_qname);
        let mut guard = 0;
        while let Some(class) = current {
            let Some(base) = class.base.as_deref() else {
                return false;
            };
            if base == "flash.display.MovieClip" {
                return true;
            }
            current = self.world.class(base);
            guard += 1;
            if guard > 64 {
                return false;
            }
        }
        false
    }

    /// First method with `name` in the base chain that is visible to
    /// subclasses. Private methods are not inherited in AS3, so they do not
    /// participate in overriding.
    fn inherited_method(&self, class_qname: &str, name: &str) -> Option<MethodSymbol> {
        let mut current = self.world.class(class_qname);
        let mut guard = 0;
        while let Some(class) = current {
            if let Some(method) = class.instance_method(name) {
                if method.access != Access::Private {
                    return Some(method.clone());
                }
            }
            let Some(base) = class.base.as_deref() else {
                return None;
            };
            current = self.world.class(base);
            guard += 1;
            if guard > 64 {
                return None;
            }
        }
        None
    }

    /// Enforces AS3's explicit `override` rule. Methods inherited from an
    /// external base are not modelled here, so declarations in classes with an
    /// external ancestor are accepted without a match check.
    fn check_overrides(&mut self) {
        let classes = self.world.classes.clone();
        for class in &classes {
            let external_ancestor = self
                .world
                .external_ancestor(&class.qualified_name)
                .is_some();
            for method in &class.methods {
                let inherited = class
                    .base
                    .as_deref()
                    .and_then(|base| self.inherited_method(base, &method.name));
                match inherited {
                    Some(base_method) => {
                        if base_method.is_final {
                            self.error(
                                format!(
                                    "Die finale Methode '{}' kann nicht überschrieben werden",
                                    method.name
                                ),
                                method.span,
                            );
                        }
                        if !method.is_override {
                            self.error(
                                format!(
                                    "Methode '{}' überschreibt eine Basismethode und muss mit \
                                     'override' markiert werden",
                                    method.name
                                ),
                                method.span,
                            );
                        }
                    }
                    None => {
                        if method.is_override && !external_ancestor {
                            self.error(
                                format!(
                                    "'override' für '{}' findet keine Methode in einer Basisklasse",
                                    method.name
                                ),
                                method.span,
                            );
                        }
                    }
                }
            }
            for method in &class.static_methods {
                if method.is_override {
                    self.error(
                        "'override' ist bei statischen Methoden ungültig",
                        method.span,
                    );
                }
            }
        }
    }

    /// The emitter always calls the base constructor with no arguments. That
    /// is correct for default constructors and for all-optional parameter
    /// lists, but base constructors with required parameters need an explicit
    /// `super(...)` call, which this subset does not implement.
    fn check_base_constructors(&mut self) {
        let classes = self.unit.classes.clone();
        for class in &classes {
            let qualified_name = qualify(&self.world.package, &class.name);
            let Some(symbol) = self.world.class(&qualified_name) else {
                continue;
            };
            let Some(base) = symbol.base.clone() else {
                continue;
            };
            let Some(base_constructor) = self
                .world
                .class(&base)
                .and_then(|base| base.constructor.as_ref())
            else {
                continue;
            };
            let required = base_constructor.required_params();
            if required > 0 {
                self.error(
                    format!(
                        "Basisklasse '{base}' erwartet {required} Konstruktorargument(e); \
                         explizite 'super(...)'-Aufrufe werden nicht unterstützt"
                    ),
                    class.span,
                );
            }
        }
    }

    fn check_class_modifiers(&mut self, class: &ClassDecl) {
        if class.modifiers.dynamic {
            self.error(
                "'dynamic' wird vom ABC-Compiler nicht unterstützt",
                class.span,
            );
        }
        if class.modifiers.native {
            self.error(
                "'native' wird vom ABC-Compiler nicht unterstützt",
                class.span,
            );
        }
        if !class.interfaces.is_empty() {
            self.error(
                "'implements' wird vom ABC-Compiler nicht unterstützt",
                class.span,
            );
        }
    }

    fn check_field_modifiers(&mut self, field: &FieldDecl) {
        if field.modifiers.dynamic {
            self.error("'dynamic' ist für Felder nicht unterstützt", field.span);
        }
        if field.modifiers.native {
            self.error("'native' ist für Felder nicht unterstützt", field.span);
        }
        if field.modifiers.override_modifier {
            self.error("'override' ist für Felder nicht unterstützt", field.span);
        }
    }

    fn check_access_modifiers(&mut self, modifiers: Modifiers, span: Span) {
        let count = [
            modifiers.public,
            modifiers.private,
            modifiers.protected,
            modifiers.internal,
        ]
        .into_iter()
        .filter(|flag| *flag)
        .count();
        if count > 1 {
            self.error("Mehrere Zugriffsmodifikatoren sind ungültig", span);
        }
    }

    fn resolve_base(&mut self, class: &ClassDecl) -> Option<String> {
        let name = class.base.as_deref()?;
        if let Some(ty) = self.world.resolve_type_name(name) {
            if matches!(
                ty,
                Type::Int | Type::Uint | Type::Number | Type::Boolean | Type::String | Type::Object
            ) {
                self.error(
                    format!("Klasse kann nicht von Werttyp '{name}' erben"),
                    class.span,
                );
                return None;
            }
        }
        match self.world.resolve_class_name(name) {
            Some(qname) => {
                let base_is_final = self
                    .world
                    .class(&qname)
                    .is_some_and(|base_class| base_class.is_final);
                if base_is_final {
                    self.error(
                        format!("Klasse '{qname}' ist 'final' und kann nicht erweitert werden"),
                        class.span,
                    );
                }
                Some(qname)
            }
            None => {
                self.error(format!("Unbekannte Basisklasse '{name}'"), class.span);
                None
            }
        }
    }

    fn build_method(&mut self, function: &FunctionDecl, class_name: &str) -> Option<MethodSymbol> {
        self.check_access_modifiers(function.modifiers, function.span);
        if function.modifiers.dynamic {
            self.error(
                "'dynamic' wird bei Methoden nicht unterstützt",
                function.span,
            );
        }
        if function.modifiers.native {
            self.error(
                "'native' wird vom ABC-Compiler nicht unterstützt",
                function.span,
            );
        }
        if function.name.starts_with("get ") || function.name.starts_with("set ") {
            self.error(
                "Getter und Setter werden vom ABC-Compiler nicht unterstützt",
                function.span,
            );
        }
        if function.body.is_none() {
            self.error(
                format!("Funktion '{}' hat keinen Körper", function.name),
                function.span,
            );
        }

        let is_constructor = function.name == class_name && !function.modifiers.static_modifier;
        if is_constructor && function.modifiers.override_modifier {
            self.error("'override' ist bei Konstruktoren ungültig", function.span);
        }
        let mut params = Vec::new();
        let mut seen_optional = false;
        for param in &function.params {
            let ty = self.resolve_type(param.ty.as_ref(), param.span, "Parameter");
            if let Some(default) = &param.default {
                if !is_literal_default(default) {
                    self.error(
                        "Standardwerte müssen Literale (int, uint, Number, Boolean, String oder null) sein",
                        default.span,
                    );
                }
                seen_optional = true;
            } else if seen_optional {
                self.error(
                    "Parameter ohne Standardwert dürfen nicht nach optionalen Parametern stehen",
                    param.span,
                );
            }
            params.push(ParamSymbol {
                name: param.name.clone(),
                ty,
                has_default: param.default.is_some(),
                span: param.span,
            });
        }

        let return_type = if is_constructor {
            Type::Void
        } else {
            self.resolve_type(function.return_type.as_ref(), function.span, "Rückgabetyp")
        };

        Some(MethodSymbol {
            name: function.name.clone(),
            access: access_of(function.modifiers),
            params,
            return_type,
            is_static: function.modifiers.static_modifier,
            is_constructor,
            is_override: function.modifiers.override_modifier,
            is_final: function.modifiers.final_modifier,
            span: function.span,
        })
    }

    fn build_functions(&mut self) {
        let functions = self.unit.functions.clone();
        for function in &functions {
            self.check_access_modifiers(function.modifiers, function.span);
            if function.modifiers.dynamic || function.modifiers.native {
                self.error(
                    "'dynamic'/'native' werden bei Funktionen nicht unterstützt",
                    function.span,
                );
            }
            if function.modifiers.override_modifier {
                self.error(
                    "'override' ist nur für Methoden einer Klasse gültig",
                    function.span,
                );
            }
            if function.body.is_none() {
                self.error(
                    format!("Funktion '{}' hat keinen Körper", function.name),
                    function.span,
                );
            }
            let mut params = Vec::new();
            let mut seen_optional = false;
            for param in &function.params {
                let ty = self.resolve_type(param.ty.as_ref(), param.span, "Parameter");
                if let Some(default) = &param.default {
                    if !is_literal_default(default) {
                        self.error(
                            "Standardwerte müssen Literale (int, uint, Number, Boolean, String oder null) sein",
                            default.span,
                        );
                    }
                    seen_optional = true;
                } else if seen_optional {
                    self.error(
                        "Parameter ohne Standardwert dürfen nicht nach optionalen Parametern stehen",
                        param.span,
                    );
                }
                params.push(ParamSymbol {
                    name: param.name.clone(),
                    ty,
                    has_default: param.default.is_some(),
                    span: param.span,
                });
            }
            let return_type =
                self.resolve_type(function.return_type.as_ref(), function.span, "Rückgabetyp");
            self.world.functions.push(MethodSymbol {
                name: function.name.clone(),
                access: access_of(function.modifiers),
                params,
                return_type,
                is_static: true,
                is_constructor: false,
                is_override: false,
                is_final: false,
                span: function.span,
            });
        }
    }

    fn resolve_type(
        &mut self,
        reference: Option<&crate::ast::TypeRef>,
        span: Span,
        context: &str,
    ) -> Type {
        let Some(reference) = reference else {
            return Type::Any;
        };
        if reference.array_depth > 0 {
            self.error(
                format!(
                    "Array-Typen werden nicht unterstützt ({} )",
                    reference.display()
                ),
                span,
            );
            return Type::Any;
        }
        if reference.name == "Array" || reference.name == "Function" {
            self.error(
                format!("{context}-Typ '{}' wird nicht unterstützt", reference.name),
                span,
            );
            return Type::Any;
        }
        match self.world.resolve_type_ref(reference) {
            Some(ty) => {
                if ty == Type::Void && context != "Rückgabetyp" {
                    self.error(
                        format!("'{context}-Typ' void ist nur als Rückgabetyp erlaubt"),
                        span,
                    );
                    Type::Any
                } else {
                    ty
                }
            }
            None => {
                self.error(format!("Unbekannter Typ '{}'", reference.name), span);
                Type::Any
            }
        }
    }

    // ------------------------------------------------------- field initializers

    fn check_field_initializers(&mut self) {
        let classes = self.unit.classes.clone();
        for class in &classes {
            let qname = qualify(&self.world.package, &class.name);
            for member in &class.members {
                let Member::Field(field) = member else {
                    continue;
                };
                let Some(value) = &field.value else {
                    continue;
                };
                let Some(symbol) = self
                    .world
                    .class(&qname)
                    .and_then(|c| {
                        if field.modifiers.static_modifier {
                            c.static_field(&field.name)
                        } else {
                            c.instance_field(&field.name)
                        }
                    })
                    .cloned()
                else {
                    continue;
                };
                self.begin_context(
                    Some(qname.clone()),
                    field.modifiers.static_modifier,
                    false,
                    Type::Void,
                );
                let value_ty = self.infer_expr(value);
                self.check_assignment_type(&symbol.ty, &value_ty, value.span, "Feld");
                self.end_context();
            }
        }
    }

    // ------------------------------------------------------------ method bodies

    fn check_method_bodies(&mut self) {
        let classes = self.unit.classes.clone();
        for class in &classes {
            let qname = qualify(&self.world.package, &class.name);
            for member in &class.members {
                let Member::Method(function) = member else {
                    continue;
                };
                let Some(body) = &function.body else {
                    continue;
                };
                let symbol = {
                    let Some(symbol) = self.world.class(&qname).and_then(|c| {
                        if function.name == class.name {
                            c.constructor.clone()
                        } else if function.modifiers.static_modifier {
                            c.static_method(&function.name).cloned()
                        } else {
                            c.instance_method(&function.name).cloned()
                        }
                    }) else {
                        continue;
                    };
                    symbol
                };
                self.check_function_body(body, &symbol, Some(qname.clone()), symbol.is_static);
            }
        }
    }

    fn check_top_level_functions(&mut self) {
        let functions = self.unit.functions.clone();
        for function in &functions {
            let Some(body) = &function.body else {
                continue;
            };
            let Some(symbol) = self.world.function(&function.name).cloned() else {
                continue;
            };
            self.check_function_body(body, &symbol, None, true);
        }
    }

    fn check_function_body(
        &mut self,
        body: &Block,
        symbol: &MethodSymbol,
        class: Option<String>,
        is_static: bool,
    ) {
        self.begin_context(
            class,
            is_static,
            symbol.is_constructor,
            symbol.return_type.clone(),
        );
        for param in &symbol.params {
            if !self.scopes.declare(&param.name, param.ty.clone(), false) {
                self.error(
                    format!("Parameter '{}' ist doppelt vorhanden", param.name),
                    param.span,
                );
            }
        }
        self.check_block(body);
        self.end_context();
    }

    fn begin_context(
        &mut self,
        class: Option<String>,
        is_static: bool,
        is_constructor: bool,
        return_type: Type,
    ) {
        self.current_class = class;
        self.is_static = is_static;
        self.is_constructor = is_constructor;
        self.return_type = if is_constructor {
            Type::Void
        } else {
            return_type
        };
        self.scopes = Scopes::new();
        self.loop_depth = 0;
    }

    fn end_context(&mut self) {
        self.current_class = None;
        self.is_static = false;
        self.is_constructor = false;
        self.return_type = Type::Void;
        self.scopes = Scopes::new();
        self.loop_depth = 0;
    }

    // ---------------------------------------------------------------- statements

    fn check_block(&mut self, block: &Block) {
        self.scopes.push();
        for statement in &block.statements {
            self.check_stmt(statement);
        }
        self.scopes.pop();
    }

    fn check_stmt(&mut self, statement: &Stmt) {
        match &statement.kind {
            StmtKind::Block(block) => self.check_block(block),
            StmtKind::Var(decl) => self.check_var_decl(decl),
            StmtKind::Expr(expr) => {
                self.infer_expr(expr);
            }
            StmtKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let ty = self.infer_expr(condition);
                self.check_condition(&ty, condition.span);
                self.check_stmt(then_branch);
                if let Some(else_branch) = else_branch {
                    self.check_stmt(else_branch);
                }
            }
            StmtKind::While { condition, body } => {
                self.loop_depth += 1;
                let ty = self.infer_expr(condition);
                self.check_condition(&ty, condition.span);
                self.check_stmt(body);
                self.loop_depth -= 1;
            }
            StmtKind::DoWhile { body, condition } => {
                self.loop_depth += 1;
                self.check_stmt(body);
                let ty = self.infer_expr(condition);
                self.check_condition(&ty, condition.span);
                self.loop_depth -= 1;
            }
            StmtKind::For {
                init,
                condition,
                update,
                body,
            } => {
                self.scopes.push();
                match init {
                    ForInit::Var(decl) => self.check_var_decl(decl),
                    ForInit::Expr(expr) => {
                        self.infer_expr(expr);
                    }
                    ForInit::Empty => {}
                }
                self.loop_depth += 1;
                if let Some(condition) = condition {
                    let ty = self.infer_expr(condition);
                    self.check_condition(&ty, condition.span);
                }
                if let Some(update) = update {
                    self.infer_expr(update);
                }
                self.check_stmt(body);
                self.loop_depth -= 1;
                self.scopes.pop();
            }
            StmtKind::Return(value) => {
                if self.is_constructor {
                    if let Some(value) = value {
                        self.infer_expr(value);
                        self.error("Konstruktoren dürfen keinen Wert zurückgeben", value.span);
                    }
                    return;
                }
                if let Some(value) = value {
                    let ty = self.infer_expr(value);
                    if self.return_type.is_void() {
                        self.error("Rückgabe eines Werts in einer void-Methode", value.span);
                    } else if !self.return_type.is_dynamic()
                        && !self.world.is_assignable(&self.return_type, &ty)
                    {
                        self.error(
                            format!(
                                "Rückgabetyp '{}' erwartet, '{}' gefunden",
                                self.return_type.display(),
                                ty.display()
                            ),
                            value.span,
                        );
                    }
                } else if !self.return_type.is_void() && !self.return_type.is_dynamic() {
                    self.error(
                        format!(
                            "Rückgabewert vom Typ '{}' erwartet",
                            self.return_type.display()
                        ),
                        statement.span,
                    );
                }
            }
            StmtKind::Break => {
                if self.loop_depth == 0 {
                    self.error(
                        "'break' ist nur innerhalb einer Schleife erlaubt",
                        statement.span,
                    );
                }
            }
            StmtKind::Continue => {
                if self.loop_depth == 0 {
                    self.error(
                        "'continue' ist nur innerhalb einer Schleife erlaubt",
                        statement.span,
                    );
                }
            }
            StmtKind::Throw(expr) => {
                self.infer_expr(expr);
                self.error(
                    "'throw' wird vom ABC-Compiler nicht unterstützt",
                    statement.span,
                );
            }
            StmtKind::Empty => {}
        }
    }

    fn check_var_decl(&mut self, decl: &VarDecl) {
        let declared = self.resolve_type(decl.ty.as_ref(), decl.span, "Variable");
        if let Some(value) = &decl.value {
            let value_ty = self.infer_expr(value);
            if decl.ty.is_some() {
                self.check_assignment_type(&declared, &value_ty, value.span, "Variable");
            }
        }
        if !self.scopes.declare(&decl.name, declared, decl.is_const) {
            self.error(
                format!("Variable '{}' ist bereits deklariert", decl.name),
                decl.span,
            );
        }
    }

    fn check_condition(&mut self, ty: &Type, span: Span) {
        if ty.is_void() {
            self.error("Bedingung darf nicht 'void' sein", span);
        }
    }

    fn check_assignment_type(&mut self, target: &Type, source: &Type, span: Span, context: &str) {
        if source.is_void() {
            self.error(
                format!("{context} kann keinen void-Ausdruck zugewiesen bekommen"),
                span,
            );
            return;
        }
        if !self.world.is_assignable(target, source) {
            self.error(
                format!(
                    "Typ '{}' kann nicht '{}' zugewiesen bekommen",
                    target.display(),
                    source.display()
                ),
                span,
            );
        }
    }

    // --------------------------------------------------------------- expressions

    fn record(&mut self, expr: &Expr, info: ExprInfo) -> Type {
        self.info.insert(expr_key(expr), info.clone());
        info.ty
    }

    fn info_of(&self, expr: &Expr) -> ExprInfo {
        self.info.get(&expr_key(expr)).cloned().unwrap_or_default()
    }

    fn infer_expr(&mut self, expr: &Expr) -> Type {
        let info = self.infer_expr_info(expr);
        self.record(expr, info)
    }

    fn infer_expr_info(&mut self, expr: &Expr) -> ExprInfo {
        match &expr.kind {
            ExprKind::Number(text) => ExprInfo {
                ty: number_type(text),
                ..Default::default()
            },
            ExprKind::String(_) => ExprInfo {
                ty: Type::String,
                ..Default::default()
            },
            ExprKind::Bool(_) => ExprInfo {
                ty: Type::Boolean,
                ..Default::default()
            },
            ExprKind::Null => ExprInfo {
                ty: Type::Null,
                ..Default::default()
            },
            ExprKind::This => {
                if self.is_static || self.current_class.is_none() {
                    self.error("'this' ist nur in Instanzmethoden verfügbar", expr.span);
                    ExprInfo::default()
                } else {
                    let class = self.current_class.clone().expect("checked above");
                    ExprInfo {
                        ty: Type::Class(class),
                        ..Default::default()
                    }
                }
            }
            ExprKind::Ident(name) => self.infer_ident(name, expr.span),
            ExprKind::Member { target, name } => self.infer_member(target, name, expr.span),
            ExprKind::Index { target, index } => {
                self.infer_expr(target);
                self.infer_expr(index);
                self.error(
                    "Indexzugriff (a[b]) wird vom ABC-Compiler nicht unterstützt",
                    expr.span,
                );
                ExprInfo::default()
            }
            ExprKind::Call { callee, args } => self.infer_call(callee, args, expr.span),
            ExprKind::New { callee, args } => self.infer_new(callee, args, expr.span),
            ExprKind::Unary { op, expr: operand } => self.infer_unary(*op, operand, expr.span),
            ExprKind::Postfix { op, expr: operand } => {
                let ty = self.infer_expr(operand);
                if !ty.is_numeric() && ty != Type::Any && ty != Type::Object {
                    self.error(
                        format!(
                            "Inkrement/Dekrement benötigt einen numerischen Wert, '{}' gefunden",
                            ty.display()
                        ),
                        expr.span,
                    );
                }
                if self.is_const_target(operand) {
                    self.error("Konstante kann nicht geändert werden", operand.span);
                }
                let _ = op;
                ExprInfo {
                    ty,
                    ..Default::default()
                }
            }
            ExprKind::Binary { op, left, right } => self.infer_binary(*op, left, right, expr.span),
            ExprKind::Assign { op, target, value } => {
                self.infer_assign(*op, target, value, expr.span)
            }
            ExprKind::Conditional {
                condition,
                then_expr,
                else_expr,
            } => {
                let condition_ty = self.infer_expr(condition);
                self.check_condition(&condition_ty, condition.span);
                let then_ty = self.infer_expr(then_expr);
                let else_ty = self.infer_expr(else_expr);
                ExprInfo {
                    ty: self.world.unify(&then_ty, &else_ty),
                    ..Default::default()
                }
            }
            ExprKind::Array(items) => {
                for item in items {
                    self.infer_expr(item);
                }
                self.error(
                    "Array-Literale werden vom ABC-Compiler nicht unterstützt",
                    expr.span,
                );
                ExprInfo::default()
            }
            ExprKind::Object(fields) => {
                for (_, value) in fields {
                    self.infer_expr(value);
                }
                self.error(
                    "Objekt-Literale werden vom ABC-Compiler nicht unterstützt",
                    expr.span,
                );
                ExprInfo::default()
            }
            ExprKind::Paren(inner) => {
                let ty = self.infer_expr(inner);
                ExprInfo {
                    ty,
                    ..Default::default()
                }
            }
        }
    }

    fn infer_ident(&mut self, name: &str, span: Span) -> ExprInfo {
        if let Some(local) = self.scopes.lookup(name) {
            return ExprInfo {
                ty: local.ty.clone(),
                reference: Some(RefKind::Local),
                call: None,
            };
        }

        if let Some(class_qname) = self.current_class.clone() {
            if self.world.class(&class_qname).is_some() {
                if !self.is_static {
                    if let Some((declaring, field)) =
                        self.world.find_instance_field(&class_qname, name)
                    {
                        return ExprInfo {
                            ty: field.ty,
                            reference: Some(RefKind::InstanceField { class: declaring }),
                            call: None,
                        };
                    }
                    if let Some((declaring, _)) =
                        self.world.find_instance_method(&class_qname, name)
                    {
                        return ExprInfo {
                            ty: Type::Any,
                            reference: Some(RefKind::InstanceMethod { class: declaring }),
                            call: None,
                        };
                    }
                }
                if let Some((declaring, field)) = self.world.find_static_field(&class_qname, name) {
                    return ExprInfo {
                        ty: field.ty,
                        reference: Some(RefKind::StaticField { class: declaring }),
                        call: None,
                    };
                }
                if let Some((declaring, _)) = self.world.find_static_method(&class_qname, name) {
                    return ExprInfo {
                        ty: Type::Any,
                        reference: Some(RefKind::StaticMethod { class: declaring }),
                        call: None,
                    };
                }
                if let Some(ty) = builtin_global(name) {
                    return ExprInfo {
                        ty,
                        reference: Some(RefKind::GlobalFunction),
                        call: None,
                    };
                }
                if self.world.external_ancestor(&class_qname).is_some() {
                    // Imported/local class names must win over the generic
                    // "inherited Flash member" fallback, otherwise `Event` in
                    // `Event.ENTER_FRAME` would be treated as `this.Event`.
                    if let Some(qname) = self.world.resolve_class_name(name) {
                        return ExprInfo {
                            ty: Type::Class(qname.clone()),
                            reference: Some(RefKind::ClassRef { class: qname }),
                            call: None,
                        };
                    }
                    if self.world.function(name).is_some() {
                        return ExprInfo {
                            ty: Type::Any,
                            reference: Some(RefKind::GlobalFunction),
                            call: None,
                        };
                    }
                    // The member may be inherited from a Flash class whose
                    // members are not modelled.
                    return ExprInfo {
                        ty: Type::Any,
                        reference: Some(RefKind::Dynamic),
                        call: None,
                    };
                }
            }
        }

        match name {
            "undefined" => {
                return ExprInfo {
                    ty: Type::Any,
                    call: None,
                    ..Default::default()
                };
            }
            "NaN" | "Infinity" => {
                return ExprInfo {
                    ty: Type::Number,
                    call: None,
                    ..Default::default()
                };
            }
            _ => {}
        }

        if let Some(ty) = builtin_global(name) {
            return ExprInfo {
                ty,
                reference: Some(RefKind::GlobalFunction),
                call: None,
            };
        }

        if let Some(qname) = self.world.resolve_class_name(name) {
            return ExprInfo {
                ty: Type::Class(qname.clone()),
                reference: Some(RefKind::ClassRef { class: qname }),
                call: None,
            };
        }

        if self.world.function(name).is_some() {
            return ExprInfo {
                ty: Type::Any,
                reference: Some(RefKind::GlobalFunction),
                call: None,
            };
        }

        self.error(format!("Variable '{name}' ist nicht deklariert"), span);
        ExprInfo::default()
    }

    fn infer_member(&mut self, target: &Expr, name: &str, span: Span) -> ExprInfo {
        let target_ty = self.infer_expr(target);
        let target_info = self.info_of(target);
        if let Some(RefKind::ClassRef { class }) = target_info.reference {
            return self.infer_static_member(&class, name, span);
        }

        match &target_ty {
            Type::Class(qname) => {
                if self.world.class(qname).is_some() {
                    if let Some((declaring, field)) = self.world.find_instance_field(qname, name) {
                        return ExprInfo {
                            ty: field.ty,
                            reference: Some(RefKind::InstanceField { class: declaring }),
                            call: None,
                        };
                    }
                    if let Some((declaring, _)) = self.world.find_instance_method(qname, name) {
                        return ExprInfo {
                            ty: Type::Any,
                            reference: Some(RefKind::InstanceMethod { class: declaring }),
                            call: None,
                        };
                    }
                    if self.world.external_ancestor(qname).is_some() {
                        return ExprInfo {
                            ty: Type::Any,
                            reference: Some(RefKind::Dynamic),
                            call: None,
                        };
                    }
                    self.error(
                        format!("Member '{name}' existiert nicht in Klasse '{}'", qname),
                        span,
                    );
                    return ExprInfo {
                        ty: Type::Any,
                        reference: Some(RefKind::Dynamic),
                        call: None,
                    };
                }
                // Imported/external class: members are not modelled.
                ExprInfo {
                    ty: Type::Any,
                    reference: Some(RefKind::Dynamic),
                    call: None,
                }
            }
            Type::Void => {
                self.error("Memberzugriff auf 'void' ist ungültig", span);
                ExprInfo::default()
            }
            _ => ExprInfo {
                ty: Type::Any,
                reference: Some(RefKind::Dynamic),
                call: None,
            },
        }
    }

    fn infer_static_member(&mut self, class_qname: &str, name: &str, span: Span) -> ExprInfo {
        if self.world.class(class_qname).is_some() {
            if let Some((declaring, field)) = self.world.find_static_field(class_qname, name) {
                return ExprInfo {
                    ty: field.ty,
                    reference: Some(RefKind::StaticField { class: declaring }),
                    call: None,
                };
            }
            if let Some((declaring, _)) = self.world.find_static_method(class_qname, name) {
                return ExprInfo {
                    ty: Type::Any,
                    reference: Some(RefKind::StaticMethod { class: declaring }),
                    call: None,
                };
            }
            if self.world.external_ancestor(class_qname).is_some() {
                return ExprInfo {
                    ty: Type::Any,
                    reference: Some(RefKind::Dynamic),
                    call: None,
                };
            }
            self.error(
                format!("Statisches Member '{name}' existiert nicht in Klasse '{class_qname}'"),
                span,
            );
            return ExprInfo {
                ty: Type::Any,
                reference: Some(RefKind::Dynamic),
                call: None,
            };
        }
        // External class: dynamic static member.
        ExprInfo {
            ty: Type::Any,
            reference: Some(RefKind::Dynamic),
            call: None,
        }
    }

    fn infer_call(&mut self, callee: &Expr, args: &[Expr], _span: Span) -> ExprInfo {
        let callee_info = match &callee.kind {
            // Resolve calls through named members without evaluating the
            // method object as a value.
            ExprKind::Ident(name) => self.infer_ident(name, callee.span),
            ExprKind::Member { target, name } => self.infer_member(target, name, callee.span),
            _ => {
                let ty = self.infer_expr(callee);
                ExprInfo {
                    ty,
                    ..Default::default()
                }
            }
        };
        self.info.insert(expr_key(callee), callee_info.clone());

        let mut arg_types = Vec::with_capacity(args.len());
        for arg in args {
            arg_types.push(self.infer_expr(arg));
        }

        match (callee_info.reference.clone(), callee_info.call) {
            (Some(RefKind::InstanceMethod { class }), _) => {
                let method = self
                    .world
                    .class(&class)
                    .and_then(|c| c.instance_method(call_name(callee)).cloned());
                if let Some(method) = method {
                    self.check_call_args(&method, &arg_types, args, callee.span);
                    ExprInfo {
                        ty: method.return_type,
                        reference: None,
                        call: Some(CallKind::Instance),
                    }
                } else {
                    self.dynamic_call(&callee_info)
                }
            }
            (Some(RefKind::StaticMethod { class }), _) => {
                let method = self
                    .world
                    .class(&class)
                    .and_then(|c| c.static_method(call_name(callee)).cloned());
                if let Some(method) = method {
                    self.check_call_args(&method, &arg_types, args, callee.span);
                    ExprInfo {
                        ty: method.return_type,
                        reference: None,
                        call: Some(CallKind::Static),
                    }
                } else {
                    self.dynamic_call(&callee_info)
                }
            }
            (Some(RefKind::GlobalFunction), _) => {
                let method = self.world.function(call_name(callee)).cloned();
                if let Some(method) = method {
                    self.check_call_args(&method, &arg_types, args, callee.span);
                    ExprInfo {
                        ty: method.return_type,
                        reference: None,
                        call: Some(CallKind::Global),
                    }
                } else {
                    self.dynamic_call(&callee_info)
                }
            }
            (Some(RefKind::InstanceField { .. } | RefKind::StaticField { .. }), _) => {
                self.dynamic_call(&callee_info)
            }
            (Some(RefKind::ClassRef { .. }), _) => {
                self.error(
                    "Klasse kann nicht als Funktion aufgerufen werden",
                    callee.span,
                );
                self.dynamic_call(&callee_info)
            }
            _ => {
                // Call through a local function value / dynamic property.
                self.dynamic_call(&callee_info)
            }
        }
    }

    fn dynamic_call(&self, callee_info: &ExprInfo) -> ExprInfo {
        ExprInfo {
            ty: Type::Any,
            reference: callee_info.reference.clone(),
            call: Some(CallKind::Dynamic),
        }
    }

    fn check_call_args(
        &mut self,
        method: &MethodSymbol,
        arg_types: &[Type],
        args: &[Expr],
        span: Span,
    ) {
        let required = method.required_params();
        let total = method.params.len();
        if args.len() < required || args.len() > total {
            self.error(
                format!(
                    "Methode '{}' erwartet {} bis {} Argument(e), {} übergeben",
                    method.name,
                    required,
                    total,
                    args.len()
                ),
                span,
            );
            return;
        }
        for (index, arg_ty) in arg_types.iter().enumerate() {
            let param = &method.params[index];
            if !self.world.is_assignable(&param.ty, arg_ty) {
                self.error(
                    format!(
                        "Argument {} von '{}': Typ '{}' erwartet, '{}' gefunden",
                        index + 1,
                        method.name,
                        param.ty.display(),
                        arg_ty.display()
                    ),
                    args[index].span,
                );
            }
        }
    }

    fn infer_new(&mut self, callee: &Expr, args: &[Expr], span: Span) -> ExprInfo {
        let qname = match dotted_name(callee) {
            Some(name) => {
                if let Some(ty) = self.world.resolve_type_name(&name) {
                    match ty {
                        Type::Class(qname) => Some(qname),
                        _ => {
                            self.error(format!("'new {name}' ist kein Klassenkonstruktor"), span);
                            None
                        }
                    }
                } else {
                    self.error(format!("Unbekannter Typ '{name}'"), span);
                    None
                }
            }
            None => {
                self.error("'new' auf einen Ausdruck wird nicht unterstützt", span);
                None
            }
        };

        self.info.insert(
            expr_key(callee),
            ExprInfo {
                ty: qname
                    .as_ref()
                    .map(|q| Type::Class(q.clone()))
                    .unwrap_or(Type::Any),
                reference: qname
                    .as_ref()
                    .map(|q| RefKind::ClassRef { class: q.clone() }),
                call: None,
            },
        );

        let mut arg_types = Vec::with_capacity(args.len());
        for arg in args {
            arg_types.push(self.infer_expr(arg));
        }

        if let Some(qname) = &qname {
            if let Some(class) = self.world.class(qname).cloned() {
                if let Some(constructor) = class.constructor {
                    self.check_call_args(&constructor, &arg_types, args, span);
                } else if !args.is_empty() {
                    self.error(
                        format!(
                            "Klasse '{}' hat keinen Konstruktor mit Argumenten",
                            class.qualified_name
                        ),
                        span,
                    );
                }
            }
            ExprInfo {
                ty: Type::Class(qname.clone()),
                ..Default::default()
            }
        } else {
            ExprInfo::default()
        }
    }

    fn infer_unary(&mut self, op: UnaryOp, operand: &Expr, span: Span) -> ExprInfo {
        let ty = self.infer_expr(operand);
        match op {
            UnaryOp::Typeof => ExprInfo {
                ty: Type::String,
                ..Default::default()
            },
            UnaryOp::Void => ExprInfo {
                ty: Type::Any,
                ..Default::default()
            },
            UnaryOp::Delete => {
                self.error("'delete' wird vom ABC-Compiler nicht unterstützt", span);
                ExprInfo::default()
            }
            UnaryOp::Not => ExprInfo {
                ty: Type::Boolean,
                ..Default::default()
            },
            UnaryOp::BitNot => ExprInfo {
                ty: Type::Int,
                ..Default::default()
            },
            UnaryOp::Plus => ExprInfo {
                ty: Type::Number,
                ..Default::default()
            },
            UnaryOp::Minus => ExprInfo {
                ty: if ty.is_int_like() { ty } else { Type::Number },
                ..Default::default()
            },
        }
    }

    fn infer_binary(&mut self, op: BinaryOp, left: &Expr, right: &Expr, span: Span) -> ExprInfo {
        if matches!(op, BinaryOp::Is | BinaryOp::As) {
            let left_ty = self.infer_expr(left);
            let target = match dotted_name(right) {
                Some(name) => match self.world.resolve_type_name(&name) {
                    Some(
                        ty @ (Type::Class(_)
                        | Type::Int
                        | Type::Uint
                        | Type::Number
                        | Type::Boolean
                        | Type::String
                        | Type::Object),
                    ) => ty,
                    Some(_) | None => {
                        self.error(
                            format!(
                                "'{name}' ist kein gültiger Typ für '{}'",
                                if op == BinaryOp::Is { "is" } else { "as" }
                            ),
                            right.span,
                        );
                        Type::Any
                    }
                },
                None => {
                    self.error(
                        "'is'/'as' erwartet einen Typnamen als rechten Operanden",
                        right.span,
                    );
                    Type::Any
                }
            };
            self.info.insert(
                expr_key(right),
                ExprInfo {
                    ty: target.clone(),
                    ..Default::default()
                },
            );
            let _ = left_ty;
            return ExprInfo {
                ty: if op == BinaryOp::Is {
                    Type::Boolean
                } else {
                    target
                },
                ..Default::default()
            };
        }

        let left_ty = self.infer_expr(left);
        let right_ty = self.infer_expr(right);
        let ty = match op {
            BinaryOp::Add => {
                if left_ty == Type::String || right_ty == Type::String {
                    Type::String
                } else {
                    arithmetic_type(&left_ty, &right_ty, false)
                }
            }
            BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Mod => {
                arithmetic_type(&left_ty, &right_ty, false)
            }
            BinaryOp::Div => Type::Number,
            BinaryOp::BitAnd
            | BinaryOp::BitOr
            | BinaryOp::BitXor
            | BinaryOp::ShiftLeft
            | BinaryOp::ShiftRight
            | BinaryOp::ShiftRightUnsigned => {
                if matches!(left_ty, Type::Uint) || matches!(right_ty, Type::Uint) {
                    Type::Uint
                } else {
                    Type::Int
                }
            }
            BinaryOp::Eq
            | BinaryOp::NotEq
            | BinaryOp::StrictEq
            | BinaryOp::StrictNotEq
            | BinaryOp::Lt
            | BinaryOp::Le
            | BinaryOp::Gt
            | BinaryOp::Ge
            | BinaryOp::InstanceOf => Type::Boolean,
            BinaryOp::And | BinaryOp::Or => Type::Any,
            BinaryOp::In => {
                self.error("'in' wird vom ABC-Compiler nicht unterstützt", span);
                Type::Any
            }
            BinaryOp::Is | BinaryOp::As => unreachable!("handled above"),
        };
        ExprInfo {
            ty,
            ..Default::default()
        }
    }

    fn infer_assign(&mut self, op: AssignOp, target: &Expr, value: &Expr, span: Span) -> ExprInfo {
        let target_ty = self.infer_expr(target);
        let value_ty = self.infer_expr(value);
        let target_info = self.info_of(target);
        match target_info.reference {
            Some(RefKind::Local)
            | Some(RefKind::InstanceField { .. })
            | Some(RefKind::StaticField { .. })
            | Some(RefKind::Dynamic) => {}
            _ => {
                self.error("Ungültiges Zuweisungsziel", target.span);
                return ExprInfo {
                    ty: target_ty,
                    ..Default::default()
                };
            }
        }
        if self.is_const_target(target) {
            self.error("Konstante kann nicht geändert werden", target.span);
        }
        if op == AssignOp::Assign {
            self.check_assignment_type(&target_ty, &value_ty, value.span, "Zuweisung");
        } else {
            let binary = assign_to_binary(op);
            let result = match binary {
                BinaryOp::Add if target_ty == Type::String || value_ty == Type::String => {
                    Type::String
                }
                BinaryOp::Div => Type::Number,
                other => arithmetic_type(&target_ty, &value_ty, matches!(other, BinaryOp::Div)),
            };
            self.check_assignment_type(&target_ty, &result, span, "Zuweisung");
        }
        ExprInfo {
            ty: target_ty,
            ..Default::default()
        }
    }

    fn is_const_target(&self, target: &Expr) -> bool {
        match &target.kind {
            ExprKind::Paren(inner) => self.is_const_target(inner),
            ExprKind::Ident(name) => {
                if let Some(local) = self.scopes.lookup(name) {
                    return local.is_const;
                }
                if let Some(class_qname) = &self.current_class {
                    if !self.is_static {
                        if let Some((_, field)) = self.world.find_instance_field(class_qname, name)
                        {
                            return field.is_const;
                        }
                    }
                    if let Some((_, field)) = self.world.find_static_field(class_qname, name) {
                        return field.is_const;
                    }
                }
                false
            }
            ExprKind::Member { target, name } => {
                let target_info = self.info_of(target);
                if let Type::Class(qname) = &target_info.ty {
                    if let Some((_, field)) = self.world.find_instance_field(qname, name) {
                        return field.is_const;
                    }
                }
                if let Some(RefKind::ClassRef { class }) = &target_info.reference {
                    if let Some((_, field)) = self.world.find_static_field(class, name) {
                        return field.is_const;
                    }
                }
                false
            }
            _ => false,
        }
    }
}
fn access_of(modifiers: Modifiers) -> Access {
    if modifiers.private {
        Access::Private
    } else if modifiers.protected {
        Access::Protected
    } else if modifiers.public {
        Access::Public
    } else {
        Access::Internal
    }
}

fn qualify(package: &str, name: &str) -> String {
    if package.is_empty() {
        name.to_string()
    } else {
        format!("{package}.{name}")
    }
}

fn expr_key(expr: &Expr) -> (usize, usize) {
    (expr.span.start, expr.span.end)
}

fn call_name(callee: &Expr) -> &str {
    match &callee.kind {
        ExprKind::Ident(name) => name,
        ExprKind::Member { name, .. } => name,
        _ => "",
    }
}

fn dotted_name(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::Ident(name) => Some(name.clone()),
        ExprKind::Member { target, name } => Some(format!("{}.{}", dotted_name(target)?, name)),
        ExprKind::Paren(inner) => dotted_name(inner),
        _ => None,
    }
}

fn number_type(text: &str) -> Type {
    let lower = text.to_ascii_lowercase();
    if let Some(hex) = lower.strip_prefix("0x") {
        return match u64::from_str_radix(hex, 16) {
            Ok(value) if value <= i32::MAX as u64 => Type::Int,
            Ok(value) if value <= u32::MAX as u64 => Type::Uint,
            _ => Type::Number,
        };
    }
    if text.contains('.') || lower.contains('e') {
        return Type::Number;
    }
    match text.parse::<i64>() {
        Ok(value) if value >= i32::MIN as i64 && value <= i32::MAX as i64 => Type::Int,
        Ok(value) if value >= 0 && value <= u32::MAX as i64 => Type::Uint,
        _ => Type::Number,
    }
}

fn arithmetic_type(left: &Type, right: &Type, force_number: bool) -> Type {
    if force_number || left == &Type::Number || right == &Type::Number {
        return Type::Number;
    }
    if left.is_int_like() && right.is_int_like() {
        if left == &Type::Uint || right == &Type::Uint {
            Type::Uint
        } else {
            Type::Int
        }
    } else {
        Type::Number
    }
}

fn assign_to_binary(op: AssignOp) -> BinaryOp {
    match op {
        AssignOp::Assign => BinaryOp::Add,
        AssignOp::Add => BinaryOp::Add,
        AssignOp::Sub => BinaryOp::Sub,
        AssignOp::Mul => BinaryOp::Mul,
        AssignOp::Div => BinaryOp::Div,
        AssignOp::Mod => BinaryOp::Mod,
        AssignOp::BitAnd => BinaryOp::BitAnd,
        AssignOp::BitOr => BinaryOp::BitOr,
        AssignOp::BitXor => BinaryOp::BitXor,
        AssignOp::ShiftLeft => BinaryOp::ShiftLeft,
        AssignOp::ShiftRight => BinaryOp::ShiftRight,
        AssignOp::ShiftRightUnsigned => BinaryOp::ShiftRightUnsigned,
    }
}

/// Free functions that the AVM2 global object provides. They are resolved as
/// global functions even inside classes (where an otherwise unknown identifier
/// would be treated as an inherited Flash member).
fn builtin_global(name: &str) -> Option<Type> {
    match name {
        "trace" => Some(Type::Void),
        "getTimer" => Some(Type::Int),
        "isNaN" | "isFinite" => Some(Type::Boolean),
        "parseInt" | "parseFloat" => Some(Type::Number),
        _ => None,
    }
}

fn is_literal_default(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Number(_) | ExprKind::String(_) | ExprKind::Bool(_) | ExprKind::Null => true,
        ExprKind::Unary {
            op: UnaryOp::Minus,
            expr,
        } => matches!(expr.kind, ExprKind::Number(_)),
        ExprKind::Paren(inner) => is_literal_default(inner),
        _ => false,
    }
}

/// Helper used by tests and the emitter to identify the package of a class.
pub fn class_package(qname: &str) -> &str {
    package_of_qname(qname)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;
    use crate::parser::parse_tokens;

    fn run(source: &str) -> CheckOutcome {
        let lexed = lex(source);
        let parsed = parse_tokens(&lexed.tokens);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        check(&parsed.unit, parsed.unit.package.as_deref().unwrap_or(""))
    }

    fn has_error(outcome: &CheckOutcome, needle: &str) -> bool {
        outcome
            .diagnostics
            .iter()
            .any(|d| d.severity == crate::diagnostic::Severity::Error && d.message.contains(needle))
    }

    #[test]
    fn detects_undeclared_variable() {
        let outcome = run("class T { function f():void { x = 1; } }");
        assert!(has_error(&outcome, "nicht deklariert"));
    }

    #[test]
    fn detects_bad_return_type() {
        let outcome = run("class T { function f():int { return \"nope\"; } }");
        assert!(has_error(&outcome, "Rückgabetyp"));
    }

    #[test]
    fn detects_break_outside_loop() {
        let outcome = run("class T { function f():void { break; } }");
        assert!(has_error(&outcome, "Schleife"));
    }

    #[test]
    fn detects_wrong_argument_count() {
        let outcome = run("class T { function g(a:int):void {} function f():void { g(); } }");
        assert!(has_error(&outcome, "Argument"));
    }

    #[test]
    fn detects_missing_member_on_known_class() {
        let outcome = run("class T { function f():void { this.missing = 1; } }");
        assert!(has_error(&outcome, "existiert nicht"));
    }

    #[test]
    fn rejects_array_literal_index_and_switch() {
        let outcome = run("class T { function f():void { var a = [1]; a[0] = 2; } }");
        assert!(has_error(&outcome, "Array-Literale"));
        assert!(has_error(&outcome, "Indexzugriff"));
    }

    #[test]
    fn rejects_this_in_static_method() {
        let outcome = run("class T { static function f():void { this.x = 1; } }");
        assert!(has_error(&outcome, "'this'"));
    }

    #[test]
    fn reports_source_line() {
        let outcome = run("class T {\n function f():void {\n  missing();\n }\n}");
        let diagnostic = outcome
            .diagnostics
            .iter()
            .find(|d| d.message.contains("nicht deklariert"))
            .expect("diagnostic");
        assert_eq!(diagnostic.span.line, 3);
    }
}
