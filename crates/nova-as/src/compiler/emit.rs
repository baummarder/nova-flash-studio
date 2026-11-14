//! ABC bytecode emitter for the supported ActionScript 3 subset.
//!
//! The emitter turns a checked compilation unit into a complete AVM2 ABC file:
//! constant pools, method info/body pairs, instance/class traits, a package
//! script and the initializer methods whose shape Adobe's compiler uses.

use std::collections::HashMap;

use crate::ast::{
    AssignOp, BinaryOp, Block, ClassDecl, CompilationUnit, Expr, ExprKind, ForInit, FunctionDecl,
    Member, PostfixOp, Stmt, StmtKind, TypeRef, UnaryOp,
};
use crate::diagnostic::Diagnostic;
use crate::token::Span;

use super::abc::{
    AbcFile, DefaultValue, MethodBody, MethodInfo, MethodParam, NsKind, Trait, TraitKind,
};
use super::opcode as op;
use super::types::{
    Access, ExprInfo, FieldSymbol, MethodSymbol, ParamSymbol, RefKind, Type, TypeWorld,
    package_of_qname, split_qname,
};
/// A class that was emitted into the ABC file. The SWF exporter uses this to
/// build `SymbolClass` links and to expose compiled classes to the runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledClass {
    pub name: String,
    pub qualified_name: String,
    pub base: Option<String>,
    pub extends_movieclip: bool,
    pub constructor: bool,
    pub source_line: usize,
}

pub struct EmitResult {
    pub abc: Vec<u8>,
    pub classes: Vec<CompiledClass>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Emits an ABC file for a checked unit.
pub fn emit<'a>(
    unit: &'a CompilationUnit,
    world: &'a TypeWorld,
    info: &'a HashMap<(usize, usize), ExprInfo>,
    package: &str,
    main_class: Option<&str>,
) -> EmitResult {
    let mut emitter = Emitter::new(unit, world, info, package);
    emitter.run(main_class);
    let classes = emitter.compiled_classes();
    let diagnostics = std::mem::take(&mut emitter.diagnostics);
    let has_errors = diagnostics
        .iter()
        .any(|d| d.severity == crate::diagnostic::Severity::Error);
    let abc = if has_errors {
        Vec::new()
    } else {
        emitter.abc.write()
    };
    EmitResult {
        abc,
        classes,
        diagnostics,
    }
}

struct Emitter<'a> {
    unit: &'a CompilationUnit,
    world: &'a TypeWorld,
    info: &'a HashMap<(usize, usize), ExprInfo>,
    package: String,
    abc: AbcFile,
    diagnostics: Vec<Diagnostic>,
    order: Vec<String>,
    method_index: HashMap<(String, String, bool), u32>,
    constructor_index: HashMap<String, u32>,
    cinit_index: HashMap<String, u32>,
    global_function_index: HashMap<String, u32>,
}

impl<'a> Emitter<'a> {
    fn new(
        unit: &'a CompilationUnit,
        world: &'a TypeWorld,
        info: &'a HashMap<(usize, usize), ExprInfo>,
        package: &str,
    ) -> Self {
        Self {
            unit,
            world,
            info,
            package: package.to_string(),
            abc: AbcFile::new(),
            diagnostics: Vec::new(),
            order: Vec::new(),
            method_index: HashMap::new(),
            constructor_index: HashMap::new(),
            cinit_index: HashMap::new(),
            global_function_index: HashMap::new(),
        }
    }

    fn error(&mut self, message: impl Into<String>, span: Span) {
        self.diagnostics
            .push(Diagnostic::error(message.into(), span));
    }

    fn run(&mut self, main_class: Option<&str>) {
        self.compute_order(main_class);
        // Global method indices must be known before class bodies (which may
        // call package functions) are emitted.
        self.emit_global_functions();
        let order = self.order.clone();
        for qname in &order {
            self.emit_class(qname);
        }
        self.emit_script();
    }

    fn compute_order(&mut self, main_class: Option<&str>) {
        self.order = self
            .world
            .classes
            .iter()
            .map(|class| class.qualified_name.clone())
            .collect();
        if let Some(main) = main_class {
            let found = self
                .order
                .iter()
                .position(|qname| qname == main || split_qname(qname).1 == main);
            match found {
                Some(index) => {
                    let qname = self.order.remove(index);
                    self.order.insert(0, qname);
                }
                None => {
                    let span = self.unit.span;
                    self.error(format!("Hauptklasse '{main}' wurde nicht gefunden"), span);
                }
            }
        }
    }

    fn compiled_classes(&self) -> Vec<CompiledClass> {
        self.order
            .iter()
            .filter_map(|qname| self.world.class(qname))
            .map(|class| CompiledClass {
                name: class.name.clone(),
                qualified_name: class.qualified_name.clone(),
                base: class.base.clone(),
                extends_movieclip: class.extends_movieclip,
                constructor: class.constructor.is_some(),
                source_line: class.source_line,
            })
            .collect()
    }

    // ------------------------------------------------------------ names/types

    fn intern(&mut self, value: &str) -> u32 {
        self.abc.constant_pool.intern_string(value)
    }

    fn qname(&mut self, kind: NsKind, namespace_name: &str, name: &str) -> u32 {
        let namespace_name = self.intern(namespace_name);
        let namespace = self.abc.constant_pool.add_namespace(kind, namespace_name);
        let name = self.intern(name);
        self.abc.constant_pool.add_qname(namespace, name)
    }

    fn class_multiname(&mut self, qname: &str) -> u32 {
        let (package, simple) = split_qname(qname);
        self.qname(NsKind::Package, package, simple)
    }

    /// Multiname of the class's superclass (`Object` when there is no `extends`).
    fn class_base_multiname(&mut self, qname: &str) -> u32 {
        let base = self.world.class(qname).and_then(|class| class.base.clone());
        match base.as_deref() {
            Some(base) => self.class_multiname(base),
            None => self.object_multiname(),
        }
    }

    fn object_multiname(&mut self) -> u32 {
        self.qname(NsKind::Package, "", "Object")
    }

    fn any_or_object(&mut self, ty: &Type) -> u32 {
        match ty {
            Type::Any => 0,
            Type::Void => self.qname(NsKind::Package, "", "void"),
            Type::Null => self.object_multiname(),
            Type::Int => self.qname(NsKind::Package, "", "int"),
            Type::Uint => self.qname(NsKind::Package, "", "uint"),
            Type::Number => self.qname(NsKind::Package, "", "Number"),
            Type::Boolean => self.qname(NsKind::Package, "", "Boolean"),
            Type::String => self.qname(NsKind::Package, "", "String"),
            Type::Object => self.object_multiname(),
            Type::Class(qname) => self.class_multiname(qname),
        }
    }

    fn member_namespace(&mut self, class_qname: &str, access: Access) -> (NsKind, String) {
        let (package, _) = split_qname(class_qname);
        match access {
            Access::Public => (NsKind::Package, package.to_string()),
            Access::Internal => (NsKind::PackageInternal, package.to_string()),
            Access::Private => (NsKind::Private, class_qname.to_string()),
            Access::Protected => (NsKind::Protected, class_qname.to_string()),
        }
    }

    fn member_multiname(&mut self, class_qname: &str, name: &str, access: Access) -> u32 {
        let (kind, namespace_name) = self.member_namespace(class_qname, access);
        self.qname(kind, &namespace_name, name)
    }

    fn protected_namespace(&mut self, class_qname: &str) -> u32 {
        let name = self.intern(class_qname);
        self.abc
            .constant_pool
            .add_namespace(NsKind::Protected, name)
    }

    fn global_function_multiname(&mut self, name: &str) -> u32 {
        let package = self.package.clone();
        self.qname(NsKind::Package, &package, name)
    }

    fn dynamic_multiname(&mut self, ty: &Type, name: &str) -> u32 {
        // A namespace set resolves a name when any listed public namespace
        // matches. Include the current package, the packages of the class and
        // its base chain (external bases may still declare inherited members),
        // and every imported package.
        let mut packages: Vec<String> = Vec::new();
        let add_package = |packages: &mut Vec<String>, package: &str| {
            if !packages.iter().any(|existing| existing == package) {
                packages.push(package.to_string());
            }
        };
        add_package(&mut packages, "");
        if let Type::Class(qname) = ty {
            let mut current = self.world.class(qname);
            if current.is_none() {
                add_package(&mut packages, package_of_qname(qname));
            }
            let mut guard = 0;
            while let Some(class) = current {
                add_package(&mut packages, package_of_qname(&class.qualified_name));
                let Some(base) = class.base.as_deref() else {
                    break;
                };
                add_package(&mut packages, package_of_qname(base));
                current = self.world.class(base);
                guard += 1;
                if guard > 64 {
                    break;
                }
            }
        }
        for imported in self.world.imports.values() {
            add_package(&mut packages, package_of_qname(imported));
        }
        for package in &self.world.wildcard_imports {
            add_package(&mut packages, package);
        }
        let mut namespaces = Vec::with_capacity(packages.len());
        for package in packages {
            let namespace_name = self.intern(&package);
            namespaces.push(
                self.abc
                    .constant_pool
                    .add_namespace(NsKind::Package, namespace_name),
            );
        }
        let namespace_set = self.abc.constant_pool.add_namespace_set(namespaces);
        let name = self.intern(name);
        self.abc.constant_pool.add_multiname(namespace_set, name)
    }

    fn type_from_ref(&self, reference: Option<&TypeRef>) -> Type {
        reference
            .and_then(|reference| self.world.resolve_type_ref(reference))
            .unwrap_or(Type::Any)
    }

    fn info_of(&self, expr: &Expr) -> ExprInfo {
        self.info
            .get(&(expr.span.start, expr.span.end))
            .cloned()
            .unwrap_or_default()
    }

    fn instance_field_access(&self, class_qname: &str, name: &str) -> Access {
        self.world
            .find_instance_field(class_qname, name)
            .map(|(_, field)| field.access)
            .unwrap_or(Access::Public)
    }

    fn instance_method_access(&self, class_qname: &str, name: &str) -> Access {
        self.world
            .find_instance_method(class_qname, name)
            .map(|(_, method)| method.access)
            .unwrap_or(Access::Public)
    }

    fn static_field_access(&self, class_qname: &str, name: &str) -> Access {
        self.world
            .find_static_field(class_qname, name)
            .map(|(_, field)| field.access)
            .unwrap_or(Access::Public)
    }

    fn static_method_access(&self, class_qname: &str, name: &str) -> Access {
        self.world
            .find_static_method(class_qname, name)
            .map(|(_, method)| method.access)
            .unwrap_or(Access::Public)
    }

    // ----------------------------------------------------------- class output

    fn emit_class(&mut self, qname: &str) {
        let Some(class_symbol) = self.world.class(qname).cloned() else {
            return;
        };
        let Some(class_decl) = self
            .unit
            .classes
            .iter()
            .find(|class| class.name == class_symbol.name)
            .cloned()
        else {
            return;
        };

        let cinit = self.add_method_info("", &Type::Void, &[], None);
        let cinit_body = self.build_class_initializer(&class_decl, &class_symbol, cinit);
        self.abc.method_bodies.push(cinit_body);
        self.cinit_index.insert(qname.to_string(), cinit);

        let constructor = if let Some(symbol) = class_symbol.constructor.clone() {
            let function = class_decl.members.iter().find_map(|member| match member {
                Member::Method(function) if function.name == class_symbol.name => {
                    Some(function.clone())
                }
                _ => None,
            });
            let index = self.add_method_info_for(&symbol, function.as_ref());
            let body = self.build_constructor_body(&class_decl, &class_symbol, &symbol, index);
            self.abc.method_bodies.push(body);
            Some(index)
        } else {
            let index = self.add_method_info(&class_symbol.name, &Type::Void, &[], None);
            let body = self.build_default_constructor(&class_decl, &class_symbol, index);
            self.abc.method_bodies.push(body);
            Some(index)
        };
        if let Some(index) = constructor {
            self.constructor_index.insert(qname.to_string(), index);
        }

        for member in &class_decl.members {
            let Member::Method(function) = member else {
                continue;
            };
            if function.name == class_symbol.name && !function.modifiers.static_modifier {
                continue;
            }
            let is_static = function.modifiers.static_modifier;
            let Some(symbol) = (if is_static {
                class_symbol.static_method(&function.name).cloned()
            } else {
                class_symbol.instance_method(&function.name).cloned()
            }) else {
                continue;
            };
            let index = self.add_method_info_for(&symbol, Some(function));
            let body = self.build_method_body(&class_symbol, &symbol, function, index);
            self.abc.method_bodies.push(body);
            self.method_index
                .insert((qname.to_string(), function.name.clone(), is_static), index);
        }

        self.build_instance_info(&class_decl, &class_symbol);
        self.build_class_info(&class_decl, &class_symbol);
    }

    fn build_instance_info(
        &mut self,
        class_decl: &ClassDecl,
        class_symbol: &super::types::ClassSymbol,
    ) {
        let qname = class_symbol.qualified_name.clone();
        let class_multiname = self.class_multiname(&qname);
        let super_multiname = match class_symbol.base.as_deref() {
            Some(base) => self.class_multiname(base),
            None => self.object_multiname(),
        };
        let protected = self.protected_namespace(&qname);

        let mut traits = Vec::new();
        for member in &class_decl.members {
            let Member::Field(field) = member else {
                continue;
            };
            if field.modifiers.static_modifier {
                continue;
            }
            let (_, symbol) = self
                .world
                .find_instance_field(&qname, &field.name)
                .unwrap_or_else(|| {
                    let fallback = FieldSymbol {
                        name: field.name.clone(),
                        access: Access::Public,
                        ty: Type::Any,
                        is_static: false,
                        is_const: field.is_const,
                        span: field.span,
                    };
                    (qname.clone(), fallback)
                });
            let name = self.member_multiname(&qname, &field.name, symbol.access);
            let type_name = self.any_or_object(&symbol.ty);
            let value = if field.is_const {
                self.literal_default(field.value.as_ref())
            } else {
                None
            };
            // Slot id zero lets the AVM2 assign the next free slot. This
            // avoids collisions with inherited slots, including slots of
            // external base classes whose layout is unknown to this compiler.
            let kind = if field.is_const {
                TraitKind::Const {
                    slot_id: 0,
                    type_name,
                    value,
                }
            } else {
                TraitKind::Slot {
                    slot_id: 0,
                    type_name,
                    value: None,
                }
            };
            traits.push(Trait {
                name,
                kind,
                is_final: false,
                is_override: false,
            });
        }

        let mut disp_id = 0;
        for member in &class_decl.members {
            let Member::Method(function) = member else {
                continue;
            };
            if function.modifiers.static_modifier {
                continue;
            }
            if function.name == class_symbol.name {
                // Constructors are referenced by InstanceInfo.init_method, not
                // by an instance trait.
                continue;
            }
            let Some((_, symbol)) = self.world.find_instance_method(&qname, &function.name) else {
                continue;
            };
            let name = self.member_multiname(&qname, &function.name, symbol.access);
            let index = self
                .method_index
                .get(&(qname.clone(), function.name.clone(), false))
                .copied()
                .unwrap_or(0);
            traits.push(Trait {
                name,
                kind: TraitKind::Method {
                    disp_id,
                    method: index,
                },
                is_final: symbol.is_final,
                is_override: symbol.is_override,
            });
            disp_id += 1;
        }

        self.abc.instances.push(super::abc::InstanceInfo {
            name: class_multiname,
            super_name: super_multiname,
            is_sealed: !class_decl.modifiers.dynamic,
            is_final: class_symbol.is_final,
            protected_namespace: Some(protected),
            interfaces: Vec::new(),
            init_method: self.constructor_index.get(&qname).copied().unwrap_or(0),
            traits,
        });
    }

    fn build_class_info(
        &mut self,
        class_decl: &ClassDecl,
        class_symbol: &super::types::ClassSymbol,
    ) {
        let qname = class_symbol.qualified_name.clone();
        let mut traits = Vec::new();
        for member in &class_decl.members {
            let Member::Field(field) = member else {
                continue;
            };
            if !field.modifiers.static_modifier {
                continue;
            }
            let (_, symbol) = self
                .world
                .find_static_field(&qname, &field.name)
                .unwrap_or_else(|| {
                    let fallback = FieldSymbol {
                        name: field.name.clone(),
                        access: Access::Public,
                        ty: Type::Any,
                        is_static: true,
                        is_const: field.is_const,
                        span: field.span,
                    };
                    (qname.clone(), fallback)
                });
            let name = self.member_multiname(&qname, &field.name, symbol.access);
            let type_name = self.any_or_object(&symbol.ty);
            let value = if field.is_const {
                self.literal_default(field.value.as_ref())
            } else {
                None
            };
            // See the instance-trait comment: zero requests automatic slot
            // assignment and cannot collide with base-class slots.
            let kind = if field.is_const {
                TraitKind::Const {
                    slot_id: 0,
                    type_name,
                    value,
                }
            } else {
                TraitKind::Slot {
                    slot_id: 0,
                    type_name,
                    value: None,
                }
            };
            traits.push(Trait {
                name,
                kind,
                is_final: false,
                is_override: false,
            });
        }
        let mut static_disp_id = 0;
        for member in &class_decl.members {
            let Member::Method(function) = member else {
                continue;
            };
            if !function.modifiers.static_modifier {
                continue;
            }
            let Some((_, symbol)) = self.world.find_static_method(&qname, &function.name) else {
                continue;
            };
            let name = self.member_multiname(&qname, &function.name, symbol.access);
            let index = self
                .method_index
                .get(&(qname.clone(), function.name.clone(), true))
                .copied()
                .unwrap_or(0);
            traits.push(Trait {
                name,
                kind: TraitKind::Method {
                    disp_id: static_disp_id,
                    method: index,
                },
                is_final: symbol.is_final,
                is_override: false,
            });
            static_disp_id += 1;
        }
        self.abc.classes.push(super::abc::ClassInfo {
            init_method: self.cinit_index.get(&qname).copied().unwrap_or(0),
            traits,
        });
    }

    // --------------------------------------------------------------- methods

    fn add_method_info(
        &mut self,
        name: &str,
        return_type: &Type,
        params: &[ParamSymbol],
        defaults: Option<&FunctionDecl>,
    ) -> u32 {
        let return_type = self.any_or_object(return_type);
        let mut method_params = Vec::new();
        let mut has_optional = false;
        for (index, param) in params.iter().enumerate() {
            let ty = self.any_or_object(&param.ty);
            let name = self.intern(&param.name);
            let default = defaults
                .and_then(|function| function.params.get(index))
                .and_then(|param| param.default.as_ref())
                .map(|value| self.default_value(value));
            if default.is_some() {
                has_optional = true;
            }
            method_params.push(MethodParam { ty, name, default });
        }
        let name = if name.is_empty() {
            0
        } else {
            self.intern(name)
        };
        let mut flags = 0u8;
        if !method_params.is_empty() {
            flags |= 0x80;
        }
        if has_optional {
            flags |= 0x08;
        }
        let index = self.abc.methods.len() as u32;
        self.abc.methods.push(MethodInfo {
            name,
            return_type,
            params: method_params,
            flags,
        });
        index
    }

    fn add_method_info_for(
        &mut self,
        symbol: &MethodSymbol,
        function: Option<&FunctionDecl>,
    ) -> u32 {
        let return_type = if symbol.is_constructor {
            Type::Void
        } else {
            symbol.return_type.clone()
        };
        self.add_method_info(&symbol.name, &return_type, &symbol.params, function)
    }

    fn default_value(&mut self, expr: &Expr) -> DefaultValue {
        match &expr.kind {
            ExprKind::Number(text) => {
                if let Some(value) = parse_int_literal(text) {
                    DefaultValue::Int(self.abc.constant_pool.add_int(value))
                } else if let Some(value) = parse_uint_literal(text) {
                    DefaultValue::Uint(self.abc.constant_pool.add_uint(value))
                } else if let Ok(value) = text.parse::<f64>() {
                    DefaultValue::Double(self.abc.constant_pool.add_double(value))
                } else {
                    DefaultValue::Undefined
                }
            }
            ExprKind::String(value) => {
                DefaultValue::String(self.abc.constant_pool.intern_string(value))
            }
            ExprKind::Bool(true) => DefaultValue::True,
            ExprKind::Bool(false) => DefaultValue::False,
            ExprKind::Null => DefaultValue::Null,
            ExprKind::Unary {
                op: UnaryOp::Minus,
                expr,
            } => {
                if let ExprKind::Number(text) = &expr.kind {
                    let value = format!("-{text}");
                    if let Some(value) = parse_int_literal(&value) {
                        DefaultValue::Int(self.abc.constant_pool.add_int(value))
                    } else {
                        let value = text.parse::<f64>().map(|v| -v).unwrap_or(0.0);
                        DefaultValue::Double(self.abc.constant_pool.add_double(value))
                    }
                } else {
                    DefaultValue::Undefined
                }
            }
            ExprKind::Paren(inner) => self.default_value(inner),
            _ => DefaultValue::Undefined,
        }
    }

    fn build_body(
        &mut self,
        ctx: &mut BodyCtx,
        init_scope_depth: u32,
        max_scope_depth: u32,
        method: u32,
    ) -> MethodBody {
        let (code, max_stack) = std::mem::take(&mut ctx.builder).finish();
        MethodBody {
            method,
            max_stack,
            num_locals: ctx.locals.next,
            init_scope_depth,
            max_scope_depth,
            code,
            traits: Vec::new(),
        }
    }

    fn build_method_body(
        &mut self,
        class: &super::types::ClassSymbol,
        symbol: &MethodSymbol,
        function: &FunctionDecl,
        method: u32,
    ) -> MethodBody {
        let mut ctx = BodyCtx::new(
            Some(class.qualified_name.clone()),
            symbol.is_static,
            symbol.return_type.clone(),
            &symbol.params,
        );
        ctx.builder.get_local(0);
        ctx.builder.push_scope();
        if let Some(body) = &function.body {
            self.emit_block(&mut ctx, body);
        }
        self.emit_fallback_return(&mut ctx);
        // AVM2 class methods start with the class/instance scope chain:
        // global, Object, class and (for instance methods) the instance.
        // `getlocal0; pushscope` then adds the method's local scope.
        let (init_scope_depth, max_scope_depth) = if symbol.is_static { (3, 4) } else { (4, 5) };
        self.build_body(&mut ctx, init_scope_depth, max_scope_depth, method)
    }

    fn build_constructor_body(
        &mut self,
        class_decl: &ClassDecl,
        class: &super::types::ClassSymbol,
        symbol: &MethodSymbol,
        method: u32,
    ) -> MethodBody {
        let mut ctx = BodyCtx::new(
            Some(class.qualified_name.clone()),
            false,
            Type::Void,
            &symbol.params,
        );
        ctx.is_constructor = true;
        ctx.builder.get_local(0);
        ctx.builder.push_scope();
        ctx.builder.get_local(0);
        ctx.builder.construct_super(0);
        self.emit_instance_field_initializers(&mut ctx, class_decl, class);
        let function = class_decl.members.iter().find_map(|member| match member {
            Member::Method(function) if function.name == class.name => Some(function),
            _ => None,
        });
        if let Some(body) = function.and_then(|function| function.body.as_ref()) {
            self.emit_block(&mut ctx, body);
        }
        ctx.builder.return_void();
        self.build_body(&mut ctx, 4, 5, method)
    }

    fn build_default_constructor(
        &mut self,
        class_decl: &ClassDecl,
        class: &super::types::ClassSymbol,
        method: u32,
    ) -> MethodBody {
        let mut ctx = BodyCtx::new(Some(class.qualified_name.clone()), false, Type::Void, &[]);
        ctx.is_constructor = true;
        ctx.builder.get_local(0);
        ctx.builder.push_scope();
        ctx.builder.get_local(0);
        ctx.builder.construct_super(0);
        self.emit_instance_field_initializers(&mut ctx, class_decl, class);
        ctx.builder.return_void();
        self.build_body(&mut ctx, 4, 5, method)
    }

    fn literal_default(&mut self, value: Option<&Expr>) -> Option<DefaultValue> {
        let value = value?;
        match &value.kind {
            ExprKind::Number(_)
            | ExprKind::String(_)
            | ExprKind::Bool(_)
            | ExprKind::Null
            | ExprKind::Unary {
                op: UnaryOp::Minus, ..
            }
            | ExprKind::Paren(_) => Some(self.default_value(value)),
            _ => None,
        }
    }

    /// Push a double literal. AVM2 `pushdouble` references the double
    /// constant pool by index; the value must be interned first.
    fn emit_push_double(&mut self, ctx: &mut BodyCtx, value: f64) {
        let index = self.abc.constant_pool.add_double(value);
        ctx.builder.push_double(index);
    }

    fn emit_default_value(&mut self, ctx: &mut BodyCtx, ty: &Type) {
        match ty {
            Type::Int | Type::Uint => ctx.builder.push_int(0),
            Type::Number => self.emit_push_double(ctx, 0.0),
            Type::Boolean => ctx.builder.push_false(),
            Type::String | Type::Object | Type::Class(_) => ctx.builder.push_null(),
            Type::Any | Type::Null | Type::Void => ctx.builder.push_undefined(),
        }
    }

    fn emit_instance_field_initializers(
        &mut self,
        ctx: &mut BodyCtx,
        class_decl: &ClassDecl,
        class: &super::types::ClassSymbol,
    ) {
        for member in &class_decl.members {
            let Member::Field(field) = member else {
                continue;
            };
            if field.modifiers.static_modifier {
                continue;
            }
            let ty = self.type_from_ref(field.ty.as_ref());
            if field.is_const && self.literal_default(field.value.as_ref()).is_some() {
                continue;
            }
            let access = self.instance_field_access(&class.qualified_name, &field.name);
            let name = self.member_multiname(&class.qualified_name, &field.name, access);
            ctx.builder.get_local(0);
            if let Some(value) = &field.value {
                self.emit_expr(ctx, value);
            } else {
                self.emit_default_value(ctx, &ty);
            }
            self.emit_coerce(ctx, &ty);
            ctx.builder.init_property(name);
        }
    }

    fn emit_class_field_initializers(
        &mut self,
        ctx: &mut BodyCtx,
        class_decl: &ClassDecl,
        class: &super::types::ClassSymbol,
    ) {
        for member in &class_decl.members {
            let Member::Field(field) = member else {
                continue;
            };
            if !field.modifiers.static_modifier {
                continue;
            }
            let ty = self.type_from_ref(field.ty.as_ref());
            if field.is_const && self.literal_default(field.value.as_ref()).is_some() {
                continue;
            }
            let access = self.static_field_access(&class.qualified_name, &field.name);
            let name = self.member_multiname(&class.qualified_name, &field.name, access);
            // Adobe's class initializer resolves the class scope with
            // `findproperty` and then uses `setproperty`.
            ctx.builder.find_property(name);
            if let Some(value) = &field.value {
                self.emit_expr(ctx, value);
            } else {
                self.emit_default_value(ctx, &ty);
            }
            self.emit_coerce(ctx, &ty);
            if field.is_const {
                // `initproperty` is required for a const trait that could not
                // store its value in the trait itself.
                ctx.builder.init_property(name);
            } else {
                ctx.builder.set_property(name);
            }
        }
    }

    fn build_class_initializer(
        &mut self,
        class_decl: &ClassDecl,
        class: &super::types::ClassSymbol,
        method: u32,
    ) -> MethodBody {
        let mut ctx = BodyCtx::new(Some(class.qualified_name.clone()), true, Type::Void, &[]);
        ctx.builder.get_local(0);
        ctx.builder.push_scope();
        self.emit_class_field_initializers(&mut ctx, class_decl, class);
        ctx.builder.return_void();
        self.build_body(&mut ctx, 3, 4, method)
    }

    fn emit_fallback_return(&mut self, ctx: &mut BodyCtx) {
        if ctx.return_type.is_void() {
            ctx.builder.return_void();
        } else {
            ctx.builder.push_undefined();
            ctx.builder.return_value();
        }
    }

    // --------------------------------------------------- top-level functions

    fn emit_global_functions(&mut self) {
        let functions = self.unit.functions.clone();
        let mut prepared = Vec::new();
        for function in &functions {
            let Some(symbol) = self.world.function(&function.name).cloned() else {
                continue;
            };
            let index = self.add_method_info_for(&symbol, Some(function));
            self.global_function_index
                .insert(function.name.clone(), index);
            prepared.push((function, symbol, index));
        }
        for (function, symbol, index) in prepared {
            let mut ctx = BodyCtx::new(None, true, symbol.return_type.clone(), &symbol.params);
            ctx.builder.get_local(0);
            ctx.builder.push_scope();
            if let Some(body) = &function.body {
                self.emit_block(&mut ctx, body);
            }
            self.emit_fallback_return(&mut ctx);
            let body = self.build_body(&mut ctx, 1, 2, index);
            self.abc.method_bodies.push(body);
        }
    }

    fn emit_script(&mut self) {
        let class_count = self.order.len() as u32;
        let scripts_method = self.add_method_info("", &Type::Void, &[], None);
        let mut ctx = BodyCtx::new(None, true, Type::Void, &[]);
        ctx.builder.get_local(0);
        ctx.builder.push_scope();
        for (index, qname) in self.order.clone().iter().enumerate() {
            let class_multiname = self.class_multiname(qname);
            let base_multiname = self.class_base_multiname(qname);
            // AVM2 `newclass` pops the basetype operand and requires the base
            // class object on the scope stack. The global object stays on the
            // operand stack as the initproperty target.
            ctx.builder.get_scope_object(0);
            ctx.builder.get_lex(base_multiname);
            ctx.builder.dup();
            ctx.builder.push_scope();
            ctx.builder.new_class(index as u32);
            ctx.builder.pop_scope();
            ctx.builder.init_property(class_multiname);
        }
        ctx.builder.return_void();
        let body = self.build_body(&mut ctx, 1, 3, scripts_method);
        self.abc.method_bodies.push(body);

        let mut traits = Vec::new();
        let class_order = self.order.clone();
        for (index, qname) in class_order.iter().enumerate() {
            let name = self.class_multiname(qname);
            traits.push(Trait {
                name,
                kind: TraitKind::Class {
                    slot_id: index as u32 + 1,
                    class: index as u32,
                },
                is_final: false,
                is_override: false,
            });
        }
        let mut function_slot = class_count + 1;
        let functions = self.unit.functions.clone();
        for function in &functions {
            let Some(index) = self.global_function_index.get(&function.name).copied() else {
                continue;
            };
            let trait_name = self.global_function_multiname(&function.name);
            traits.push(Trait {
                name: trait_name,
                kind: TraitKind::Function {
                    slot_id: function_slot,
                    function: index,
                },
                is_final: false,
                is_override: false,
            });
            function_slot += 1;
        }
        self.abc.scripts.push(super::abc::ScriptInfo {
            init_method: scripts_method,
            traits,
        });
    }

    // -------------------------------------------------------------- statements

    fn emit_block(&mut self, ctx: &mut BodyCtx, block: &Block) {
        ctx.locals.push();
        for statement in &block.statements {
            self.emit_stmt(ctx, statement);
        }
        ctx.locals.pop();
    }

    fn emit_stmt(&mut self, ctx: &mut BodyCtx, statement: &Stmt) {
        match &statement.kind {
            StmtKind::Block(block) => self.emit_block(ctx, block),
            StmtKind::Var(decl) => {
                let ty = self.type_from_ref(decl.ty.as_ref());
                let slot = ctx.locals.alloc();
                if let Some(value) = &decl.value {
                    self.emit_expr(ctx, value);
                    self.emit_coerce(ctx, &ty);
                } else {
                    // Locals are initialized to their type's default value,
                    // not to `undefined`.
                    self.emit_default_value(ctx, &ty);
                }
                ctx.builder.set_local(slot);
                ctx.locals.bind(&decl.name, slot);
            }
            StmtKind::Expr(expr) => {
                self.emit_expr(ctx, expr);
                ctx.builder.pop_value();
            }
            StmtKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let else_label = ctx.builder.new_label();
                self.emit_expr(ctx, condition);
                ctx.builder.if_false(else_label);
                self.emit_stmt(ctx, then_branch);
                if let Some(else_branch) = else_branch {
                    let end = ctx.builder.new_label();
                    ctx.builder.jump(end);
                    ctx.builder.place_label(else_label);
                    self.emit_stmt(ctx, else_branch);
                    ctx.builder.place_label(end);
                } else {
                    ctx.builder.place_label(else_label);
                }
            }
            StmtKind::While { condition, body } => {
                let start = ctx.builder.new_label();
                let end = ctx.builder.new_label();
                ctx.builder.place_label(start);
                self.emit_expr(ctx, condition);
                ctx.builder.if_false(end);
                ctx.loops.push(LoopLabels {
                    break_label: end,
                    continue_label: start,
                });
                self.emit_stmt(ctx, body);
                ctx.loops.pop();
                ctx.builder.jump(start);
                ctx.builder.place_label(end);
            }
            StmtKind::DoWhile { body, condition } => {
                let start = ctx.builder.new_label();
                let condition_label = ctx.builder.new_label();
                let end = ctx.builder.new_label();
                ctx.builder.place_label(start);
                ctx.loops.push(LoopLabels {
                    break_label: end,
                    continue_label: condition_label,
                });
                self.emit_stmt(ctx, body);
                ctx.loops.pop();
                ctx.builder.place_label(condition_label);
                self.emit_expr(ctx, condition);
                ctx.builder.if_true(start);
                ctx.builder.place_label(end);
            }
            StmtKind::For {
                init,
                condition,
                update,
                body,
            } => {
                ctx.locals.push();
                match init {
                    ForInit::Var(decl) => {
                        let ty = self.type_from_ref(decl.ty.as_ref());
                        let slot = ctx.locals.alloc();
                        if let Some(value) = &decl.value {
                            self.emit_expr(ctx, value);
                            self.emit_coerce(ctx, &ty);
                        } else {
                            self.emit_default_value(ctx, &ty);
                        }
                        ctx.builder.set_local(slot);
                        ctx.locals.bind(&decl.name, slot);
                    }
                    ForInit::Expr(expr) => {
                        self.emit_expr(ctx, expr);
                        ctx.builder.pop_value();
                    }
                    ForInit::Empty => {}
                }
                let start = ctx.builder.new_label();
                let update_label = ctx.builder.new_label();
                let end = ctx.builder.new_label();
                ctx.builder.place_label(start);
                if let Some(condition) = condition {
                    self.emit_expr(ctx, condition);
                    ctx.builder.if_false(end);
                }
                ctx.loops.push(LoopLabels {
                    break_label: end,
                    continue_label: update_label,
                });
                self.emit_stmt(ctx, body);
                ctx.loops.pop();
                ctx.builder.place_label(update_label);
                if let Some(update) = update {
                    self.emit_expr(ctx, update);
                    ctx.builder.pop_value();
                }
                ctx.builder.jump(start);
                ctx.builder.place_label(end);
                ctx.locals.pop();
            }
            StmtKind::Return(value) => {
                if ctx.is_constructor {
                    if value.is_some() {
                        self.error(
                            "Konstruktoren dürfen keinen Wert zurückgeben",
                            statement.span,
                        );
                    }
                    ctx.builder.return_void();
                    return;
                }
                match value {
                    Some(value) => {
                        self.emit_expr(ctx, value);
                        if !ctx.return_type.is_void() {
                            self.emit_coerce(ctx, &ctx.return_type.clone());
                        }
                        ctx.builder.return_value();
                    }
                    None => ctx.builder.return_void(),
                }
            }
            StmtKind::Break => {
                if let Some(labels) = ctx.loops.last() {
                    let label = labels.break_label;
                    ctx.builder.jump(label);
                } else {
                    self.error(
                        "'break' ist nur innerhalb einer Schleife erlaubt",
                        statement.span,
                    );
                }
            }
            StmtKind::Continue => {
                if let Some(labels) = ctx.loops.last() {
                    let label = labels.continue_label;
                    ctx.builder.jump(label);
                } else {
                    self.error(
                        "'continue' ist nur innerhalb einer Schleife erlaubt",
                        statement.span,
                    );
                }
            }
            StmtKind::Throw(expr) => {
                self.emit_expr(ctx, expr);
                ctx.builder.throw();
                self.error(
                    "'throw' wird vom ABC-Compiler nicht unterstützt",
                    statement.span,
                );
            }
            StmtKind::Empty => {}
        }
    }

    // ------------------------------------------------------------- expressions

    fn emit_expr(&mut self, ctx: &mut BodyCtx, expr: &Expr) {
        match &expr.kind {
            ExprKind::Number(text) => self.emit_number(ctx, text),
            ExprKind::String(value) => {
                let index = self.intern(value);
                ctx.builder.push_string(index);
            }
            ExprKind::Bool(true) => ctx.builder.push_true(),
            ExprKind::Bool(false) => ctx.builder.push_false(),
            ExprKind::Null => ctx.builder.push_null(),
            ExprKind::This => {
                if ctx.is_static || ctx.class.is_none() {
                    self.error("'this' ist nur in Instanzmethoden verfügbar", expr.span);
                    ctx.builder.push_undefined();
                } else {
                    ctx.builder.get_local(0);
                }
            }
            ExprKind::Ident(name) => self.emit_ident(ctx, name, expr),
            ExprKind::Member { target, name } => self.emit_member(ctx, target, name, expr),
            ExprKind::Index { target, index } => {
                let multiname = self.abc.constant_pool.add_multiname_l();
                self.emit_expr(ctx, target);
                self.emit_expr(ctx, index);
                ctx.builder.get_property_late(multiname);
            }
            ExprKind::Call { callee, args } => self.emit_call(ctx, callee, args, expr),
            ExprKind::New { callee, args } => self.emit_new(ctx, callee, args, expr),
            ExprKind::Unary { op, expr: operand } => self.emit_unary(ctx, *op, operand, expr),
            ExprKind::Postfix { op, expr: operand } => {
                self.emit_incdec(ctx, operand, *op, false);
            }
            ExprKind::Binary { op, left, right } => {
                self.emit_binary(ctx, *op, left, right, expr);
            }
            ExprKind::Assign { op, target, value } => {
                self.emit_assignment(ctx, *op, target, value, expr);
            }
            ExprKind::Conditional {
                condition,
                then_expr,
                else_expr,
            } => {
                let else_label = ctx.builder.new_label();
                let end = ctx.builder.new_label();
                self.emit_expr(ctx, condition);
                ctx.builder.if_false(else_label);
                self.emit_expr(ctx, then_expr);
                ctx.builder.jump(end);
                ctx.builder.place_label(else_label);
                self.emit_expr(ctx, else_expr);
                ctx.builder.place_label(end);
            }
            ExprKind::Array(items) => {
                for item in items {
                    self.emit_expr(ctx, item);
                }
                ctx.builder.new_array(items.len() as u32);
            }
            ExprKind::Object(fields) => {
                for (name, value) in fields {
                    let key = self.abc.constant_pool.intern_string(name);
                    ctx.builder.push_string(key);
                    self.emit_expr(ctx, value);
                }
                ctx.builder.new_object(fields.len() as u32);
            }
            ExprKind::Paren(inner) => self.emit_expr(ctx, inner),
        }
    }

    fn emit_number(&mut self, ctx: &mut BodyCtx, text: &str) {
        let lower = text.to_ascii_lowercase();
        if let Some(hex) = lower.strip_prefix("0x") {
            if let Ok(value) = u64::from_str_radix(hex, 16) {
                if value <= i32::MAX as u64 {
                    let value = value as i32;
                    if (-128..=127).contains(&value) {
                        return ctx.builder.push_int(value);
                    }
                    let index = self.abc.constant_pool.add_int(value);
                    return ctx.builder.push_int_constant(index);
                }
                if value <= u32::MAX as u64 {
                    let value = value as u32;
                    if value <= 127 {
                        return ctx.builder.push_uint(value);
                    }
                    let index = self.abc.constant_pool.add_uint(value);
                    return ctx.builder.push_uint_constant(index);
                }
            }
            self.emit_push_double(
                ctx,
                text.parse::<f64>()
                    .or_else(|_| u64::from_str_radix(hex, 16).map(|v| v as f64))
                    .unwrap_or(0.0),
            );
            return;
        }
        if text.contains('.') || lower.contains('e') {
            self.emit_push_double(ctx, text.parse::<f64>().unwrap_or(0.0));
            return;
        }
        if let Ok(value) = text.parse::<i64>() {
            if value >= i32::MIN as i64 && value <= i32::MAX as i64 {
                let value = value as i32;
                if (-128..=127).contains(&value) {
                    return ctx.builder.push_int(value);
                }
                let index = self.abc.constant_pool.add_int(value);
                return ctx.builder.push_int_constant(index);
            }
            if value >= 0 && value <= u32::MAX as i64 {
                let value = value as u32;
                if value <= 127 {
                    return ctx.builder.push_uint(value);
                }
                let index = self.abc.constant_pool.add_uint(value);
                return ctx.builder.push_uint_constant(index);
            }
        }
        self.emit_push_double(ctx, text.parse::<f64>().unwrap_or(0.0));
    }

    fn emit_ident(&mut self, ctx: &mut BodyCtx, name: &str, expr: &Expr) {
        if let Some(slot) = ctx.locals.lookup(name) {
            ctx.builder.get_local(slot);
            return;
        }
        match name {
            "undefined" => {
                ctx.builder.push_undefined();
                return;
            }
            "NaN" => {
                self.emit_push_double(ctx, f64::NAN);
                return;
            }
            "Infinity" => {
                self.emit_push_double(ctx, f64::INFINITY);
                return;
            }
            _ => {}
        }

        let info = self.info_of(expr);
        match info.reference {
            Some(RefKind::InstanceField { class }) => {
                let access = self.instance_field_access(&class, name);
                let member = self.member_multiname(&class, name, access);
                ctx.builder.get_local(0);
                ctx.builder.get_property(member);
            }
            Some(RefKind::InstanceMethod { class }) => {
                let access = self.instance_method_access(&class, name);
                let member = self.member_multiname(&class, name, access);
                ctx.builder.get_local(0);
                ctx.builder.get_property(member);
            }
            Some(RefKind::StaticField { class }) => {
                let access = self.static_field_access(&class, name);
                let member = self.member_multiname(&class, name, access);
                let class_name = self.class_multiname(&class);
                ctx.builder.get_lex(class_name);
                ctx.builder.get_property(member);
            }
            Some(RefKind::StaticMethod { class }) => {
                let access = self.static_method_access(&class, name);
                let member = self.member_multiname(&class, name, access);
                let class_name = self.class_multiname(&class);
                ctx.builder.get_lex(class_name);
                ctx.builder.get_property(member);
            }
            Some(RefKind::ClassRef { class }) => {
                let class_name = self.class_multiname(&class);
                ctx.builder.get_lex(class_name);
            }
            Some(RefKind::GlobalFunction) => {
                let function = self.global_function_multiname(name);
                ctx.builder.get_lex(function);
            }
            Some(RefKind::Dynamic) | None => {
                if ctx.class.is_some() && !ctx.is_static {
                    let class = ctx.class.clone().unwrap_or_default();
                    let ty = Type::Class(class);
                    let member = self.dynamic_multiname(&ty, name);
                    ctx.builder.get_local(0);
                    ctx.builder.get_property(member);
                } else {
                    ctx.builder.push_undefined();
                    self.error(format!("Variable '{name}' ist nicht deklariert"), expr.span);
                }
            }
            Some(RefKind::Local) => {
                ctx.builder.push_undefined();
                self.error(format!("Variable '{name}' ist nicht deklariert"), expr.span);
            }
        }
    }

    fn emit_member(&mut self, ctx: &mut BodyCtx, target: &Expr, name: &str, expr: &Expr) {
        let info = self.info_of(expr);
        match info.reference {
            Some(RefKind::InstanceField { class }) => {
                let access = self.instance_field_access(&class, name);
                let member = self.member_multiname(&class, name, access);
                self.emit_expr(ctx, target);
                ctx.builder.get_property(member);
            }
            Some(RefKind::InstanceMethod { class }) => {
                let access = self.instance_method_access(&class, name);
                let member = self.member_multiname(&class, name, access);
                self.emit_expr(ctx, target);
                ctx.builder.get_property(member);
            }
            Some(RefKind::StaticField { class }) => {
                let access = self.static_field_access(&class, name);
                let member = self.member_multiname(&class, name, access);
                let class_name = self.class_multiname(&class);
                ctx.builder.get_lex(class_name);
                ctx.builder.get_property(member);
            }
            Some(RefKind::StaticMethod { class }) => {
                let access = self.static_method_access(&class, name);
                let member = self.member_multiname(&class, name, access);
                let class_name = self.class_multiname(&class);
                ctx.builder.get_lex(class_name);
                ctx.builder.get_property(member);
            }
            Some(RefKind::ClassRef { class }) => {
                let class_name = self.class_multiname(&class);
                ctx.builder.get_lex(class_name);
            }
            Some(RefKind::Dynamic) | None => {
                let target_info = self.info_of(target);
                if let Some(RefKind::ClassRef { class }) = target_info.reference {
                    let ty = Type::Class(class.clone());
                    let member = self.dynamic_multiname(&ty, name);
                    let class_name = self.class_multiname(&class);
                    ctx.builder.get_lex(class_name);
                    ctx.builder.get_property(member);
                } else {
                    self.emit_expr(ctx, target);
                    let ty = &target_info.ty;
                    let member = self.dynamic_multiname(ty, name);
                    ctx.builder.get_property(member);
                }
            }
            Some(RefKind::Local) | Some(RefKind::GlobalFunction) => {
                self.emit_expr(ctx, target);
                let ty = self.info_of(target).ty;
                let member = self.dynamic_multiname(&ty, name);
                ctx.builder.get_property(member);
            }
        }
    }

    fn emit_call(&mut self, ctx: &mut BodyCtx, callee: &Expr, args: &[Expr], expr: &Expr) {
        let callee_info = self.info_of(callee);
        match callee_info.reference {
            Some(RefKind::InstanceMethod { class }) => {
                match &callee.kind {
                    ExprKind::Ident(_) => ctx.builder.get_local(0),
                    ExprKind::Member { target, .. } => self.emit_expr(ctx, target),
                    _ => self.emit_expr(ctx, callee),
                }
                let method = self
                    .world
                    .find_instance_method(&class, call_name(callee))
                    .map(|(_, method)| method);
                if let Some(method) = &method {
                    let member = self.member_multiname(&class, call_name(callee), method.access);
                    self.emit_args(ctx, args, Some(method));
                    ctx.builder.call_property(member, args.len() as u32);
                } else {
                    let member = self.member_multiname(&class, call_name(callee), Access::Public);
                    self.emit_args(ctx, args, None);
                    ctx.builder.call_property(member, args.len() as u32);
                }
            }
            Some(RefKind::StaticMethod { class }) => {
                let class_name = self.class_multiname(&class);
                ctx.builder.get_lex(class_name);
                let method = self
                    .world
                    .find_static_method(&class, call_name(callee))
                    .map(|(_, method)| method);
                if let Some(method) = &method {
                    let member = self.member_multiname(&class, call_name(callee), method.access);
                    self.emit_args(ctx, args, Some(method));
                    ctx.builder.call_property(member, args.len() as u32);
                } else {
                    let member = self.member_multiname(&class, call_name(callee), Access::Public);
                    self.emit_args(ctx, args, None);
                    ctx.builder.call_property(member, args.len() as u32);
                }
            }
            Some(RefKind::GlobalFunction) => {
                let name = call_name(callee).to_string();
                let method = self.world.function(&name).cloned();
                if self.global_function_index.contains_key(&name) {
                    let function = self.global_function_multiname(&name);
                    ctx.builder.get_lex(function);
                    // `call` expects the function value, a receiver and the
                    // arguments. Package functions do not bind `this`, so
                    // Adobe's compiler passes `undefined`.
                    ctx.builder.push_undefined();
                    self.emit_args(ctx, args, method.as_ref());
                    ctx.builder.call(args.len() as u32);
                } else {
                    // Built-in free functions such as `trace` live on the
                    // playerglobal object, which is only reachable through
                    // scope-chain resolution. `findpropstrict` pushes the
                    // object that owns the property as the call receiver;
                    // `getglobalscope` would return this script's own global,
                    // which does not carry the builtin as a callable trait.
                    let member = self.dynamic_multiname(&Type::Any, &name);
                    ctx.builder.find_prop_strict(member);
                    self.emit_args(ctx, args, None);
                    ctx.builder.call_property(member, args.len() as u32);
                }
            }
            Some(RefKind::InstanceField { .. })
            | Some(RefKind::StaticField { .. })
            | Some(RefKind::Local)
            | Some(RefKind::ClassRef { .. })
            | Some(RefKind::Dynamic)
            | None => {
                let function_value = matches!(
                    callee_info.reference,
                    Some(RefKind::Local)
                        | Some(RefKind::InstanceField { .. })
                        | Some(RefKind::StaticField { .. })
                        | Some(RefKind::ClassRef { .. })
                );
                if function_value {
                    for arg in args {
                        self.emit_expr(ctx, arg);
                        ctx.builder.pop_value();
                    }
                    ctx.builder.push_undefined();
                    self.error(
                        "Aufrufe über Funktionswerte werden vom ABC-Compiler nicht unterstützt",
                        expr.span,
                    );
                } else if let ExprKind::Member { target, name } = &callee.kind {
                    let target_info = self.info_of(target);
                    if let Some(RefKind::ClassRef { class }) = target_info.reference {
                        let class_name = self.class_multiname(&class);
                        ctx.builder.get_lex(class_name);
                        let ty = Type::Class(class.clone());
                        let member = self.dynamic_multiname(&ty, name);
                        self.emit_args(ctx, args, None);
                        ctx.builder.call_property(member, args.len() as u32);
                    } else {
                        self.emit_expr(ctx, target);
                        let member = self.dynamic_multiname(&target_info.ty, name);
                        self.emit_args(ctx, args, None);
                        ctx.builder.call_property(member, args.len() as u32);
                    }
                } else if let ExprKind::Ident(name) = &callee.kind {
                    // In instance methods an unresolved identifier may be an
                    // inherited Flash method (e.g. addEventListener), so call
                    // it on `this`. Otherwise resolve it on the global object.
                    if ctx.class.is_some() && !ctx.is_static {
                        let class = ctx.class.clone().unwrap_or_default();
                        let ty = Type::Class(class);
                        ctx.builder.get_local(0);
                        let member = self.dynamic_multiname(&ty, name);
                        self.emit_args(ctx, args, None);
                        ctx.builder.call_property(member, args.len() as u32);
                    } else {
                        let member = self.dynamic_multiname(&Type::Any, name);
                        ctx.builder.find_prop_strict(member);
                        self.emit_args(ctx, args, None);
                        ctx.builder.call_property(member, args.len() as u32);
                    }
                } else {
                    for arg in args {
                        self.emit_expr(ctx, arg);
                        ctx.builder.pop_value();
                    }
                    ctx.builder.push_undefined();
                    self.error(
                        "Aufrufe über Funktionswerte werden vom ABC-Compiler nicht unterstützt",
                        expr.span,
                    );
                }
            }
        }
        let _ = expr;
    }

    fn emit_new(&mut self, ctx: &mut BodyCtx, callee: &Expr, args: &[Expr], expr: &Expr) {
        let callee_info = self.info_of(callee);
        if let Some(RefKind::ClassRef { class }) = callee_info.reference {
            let class_name = self.class_multiname(&class);
            ctx.builder.get_lex(class_name);
            let constructor = self
                .world
                .class(&class)
                .and_then(|class| class.constructor.clone());
            self.emit_args(ctx, args, constructor.as_ref());
            ctx.builder.construct(args.len() as u32);
        } else {
            ctx.builder.push_undefined();
            self.error("'new' auf einen Ausdruck wird nicht unterstützt", expr.span);
        }
    }

    fn emit_args(&mut self, ctx: &mut BodyCtx, args: &[Expr], method: Option<&MethodSymbol>) {
        for (index, arg) in args.iter().enumerate() {
            self.emit_expr(ctx, arg);
            if let Some(param) = method.and_then(|method| method.params.get(index)) {
                let ty = param.ty.clone();
                self.emit_coerce(ctx, &ty);
            }
        }
    }

    fn emit_unary(&mut self, ctx: &mut BodyCtx, unary: UnaryOp, operand: &Expr, expr: &Expr) {
        match unary {
            UnaryOp::Void => {
                self.emit_expr(ctx, operand);
                ctx.builder.pop_value();
                ctx.builder.push_undefined();
            }
            UnaryOp::Delete => {
                self.emit_expr(ctx, operand);
                self.error(
                    "'delete' wird vom ABC-Compiler nicht unterstützt",
                    expr.span,
                );
            }
            other => {
                self.emit_expr(ctx, operand);
                ctx.builder.unary(match other {
                    UnaryOp::Plus => op::CONVERT_D,
                    UnaryOp::Minus => op::NEGATE,
                    UnaryOp::Not => op::NOT,
                    UnaryOp::BitNot => op::BITNOT,
                    UnaryOp::Typeof => op::TYPEOF,
                    UnaryOp::Void | UnaryOp::Delete => unreachable!(),
                });
            }
        }
    }

    fn emit_binary(
        &mut self,
        ctx: &mut BodyCtx,
        binary: BinaryOp,
        left: &Expr,
        right: &Expr,
        expr: &Expr,
    ) {
        match binary {
            BinaryOp::Is | BinaryOp::As => {
                let target = self.info_of(right).ty;
                let type_name = self.any_or_object(&target);
                self.emit_expr(ctx, left);
                if binary == BinaryOp::Is {
                    ctx.builder.istype(type_name);
                } else {
                    ctx.builder.astype(type_name);
                }
            }
            BinaryOp::And => {
                let end = ctx.builder.new_label();
                self.emit_expr(ctx, left);
                ctx.builder.dup();
                ctx.builder.if_false(end);
                ctx.builder.pop_value();
                self.emit_expr(ctx, right);
                ctx.builder.place_label(end);
            }
            BinaryOp::Or => {
                let end = ctx.builder.new_label();
                self.emit_expr(ctx, left);
                ctx.builder.dup();
                ctx.builder.if_true(end);
                ctx.builder.pop_value();
                self.emit_expr(ctx, right);
                ctx.builder.place_label(end);
            }
            BinaryOp::In => {
                self.emit_expr(ctx, left);
                self.emit_expr(ctx, right);
                ctx.builder.pop_value();
                ctx.builder.pop_value();
                ctx.builder.push_undefined();
                self.error("'in' wird vom ABC-Compiler nicht unterstützt", expr.span);
            }
            BinaryOp::InstanceOf => {
                self.emit_expr(ctx, left);
                self.emit_expr(ctx, right);
                ctx.builder.binary(op::INSTANCEOF);
            }
            other => {
                self.emit_expr(ctx, left);
                self.emit_expr(ctx, right);
                match other {
                    BinaryOp::Add => ctx.builder.binary(op::ADD),
                    BinaryOp::Sub => ctx.builder.binary(op::SUBTRACT),
                    BinaryOp::Mul => ctx.builder.binary(op::MULTIPLY),
                    BinaryOp::Div => ctx.builder.binary(op::DIVIDE),
                    BinaryOp::Mod => ctx.builder.binary(op::MODULO),
                    BinaryOp::Eq => ctx.builder.binary(op::EQUALS),
                    BinaryOp::NotEq => {
                        ctx.builder.binary(op::EQUALS);
                        ctx.builder.unary(op::NOT);
                    }
                    BinaryOp::StrictEq => ctx.builder.binary(op::STRICTEQUALS),
                    BinaryOp::StrictNotEq => {
                        ctx.builder.binary(op::STRICTEQUALS);
                        ctx.builder.unary(op::NOT);
                    }
                    BinaryOp::Lt => ctx.builder.binary(op::LESSTHAN),
                    BinaryOp::Le => ctx.builder.binary(op::LESSEQUALS),
                    BinaryOp::Gt => ctx.builder.binary(op::GREATERTHAN),
                    BinaryOp::Ge => ctx.builder.binary(op::GREATEREQUALS),
                    BinaryOp::BitAnd => ctx.builder.binary(op::BITAND),
                    BinaryOp::BitOr => ctx.builder.binary(op::BITOR),
                    BinaryOp::BitXor => ctx.builder.binary(op::BITXOR),
                    BinaryOp::ShiftLeft => ctx.builder.binary(op::LSHIFT),
                    BinaryOp::ShiftRight => ctx.builder.binary(op::RSHIFT),
                    BinaryOp::ShiftRightUnsigned => ctx.builder.binary(op::URSHIFT),
                    BinaryOp::And
                    | BinaryOp::Or
                    | BinaryOp::InstanceOf
                    | BinaryOp::In
                    | BinaryOp::Is
                    | BinaryOp::As => unreachable!(),
                }
            }
        }
    }

    fn emit_assignment(
        &mut self,
        ctx: &mut BodyCtx,
        assign: AssignOp,
        target: &Expr,
        value: &Expr,
        expr: &Expr,
    ) {
        let Some(place) = self.resolve_place(ctx, target) else {
            self.error("Ungültiges Zuweisungsziel", target.span);
            ctx.builder.push_undefined();
            return;
        };
        let ty = place.ty();
        match place {
            Place::Local { slot, .. } => {
                if assign == AssignOp::Assign {
                    self.emit_expr(ctx, value);
                    self.emit_coerce(ctx, &ty);
                    ctx.builder.dup();
                    ctx.builder.set_local(slot);
                } else {
                    ctx.builder.get_local(slot);
                    self.emit_expr(ctx, value);
                    self.emit_binary_assign(ctx, assign);
                    self.emit_coerce(ctx, &ty);
                    ctx.builder.dup();
                    ctx.builder.set_local(slot);
                }
            }
            Place::Instance {
                receiver,
                qname,
                ty,
            } => {
                if assign == AssignOp::Assign {
                    self.emit_receiver(ctx, receiver);
                    self.emit_expr(ctx, value);
                    self.emit_coerce(ctx, &ty);
                } else {
                    self.emit_receiver(ctx, receiver);
                    ctx.builder.dup();
                    ctx.builder.get_property(qname);
                    self.emit_expr(ctx, value);
                    self.emit_binary_assign(ctx, assign);
                    self.emit_coerce(ctx, &ty);
                }
                let temp = ctx.locals.temp();
                ctx.builder.set_local(temp);
                ctx.builder.get_local(temp);
                ctx.builder.set_property(qname);
                ctx.builder.get_local(temp);
            }
            Place::Static {
                class_name,
                qname,
                ty,
            } => {
                if assign == AssignOp::Assign {
                    ctx.builder.get_lex(class_name);
                    self.emit_expr(ctx, value);
                    self.emit_coerce(ctx, &ty);
                } else {
                    ctx.builder.get_lex(class_name);
                    ctx.builder.dup();
                    ctx.builder.get_property(qname);
                    self.emit_expr(ctx, value);
                    self.emit_binary_assign(ctx, assign);
                    self.emit_coerce(ctx, &ty);
                }
                let temp = ctx.locals.temp();
                ctx.builder.set_local(temp);
                ctx.builder.get_local(temp);
                ctx.builder.set_property(qname);
                ctx.builder.get_local(temp);
            }
            Place::Dynamic {
                receiver,
                qname,
                ty,
            } => {
                if assign == AssignOp::Assign {
                    self.emit_receiver(ctx, receiver);
                    self.emit_expr(ctx, value);
                    self.emit_coerce(ctx, &ty);
                } else {
                    self.emit_receiver(ctx, receiver);
                    ctx.builder.dup();
                    ctx.builder.get_property(qname);
                    self.emit_expr(ctx, value);
                    self.emit_binary_assign(ctx, assign);
                    self.emit_coerce(ctx, &ty);
                }
                let temp = ctx.locals.temp();
                ctx.builder.set_local(temp);
                ctx.builder.get_local(temp);
                ctx.builder.set_property(qname);
                ctx.builder.get_local(temp);
            }
            Place::Index {
                receiver,
                index,
                qname,
                ty,
            } => {
                if assign == AssignOp::Assign {
                    let temp = ctx.locals.temp();
                    self.emit_receiver(ctx, receiver);
                    self.emit_expr(ctx, index);
                    self.emit_expr(ctx, value);
                    self.emit_coerce(ctx, &ty);
                    ctx.builder.set_local(temp);
                    ctx.builder.get_local(temp);
                    ctx.builder.set_property_late(qname);
                    ctx.builder.get_local(temp);
                } else {
                    let object = ctx.locals.temp();
                    let key = ctx.locals.temp();
                    let result = ctx.locals.temp();
                    self.emit_receiver(ctx, receiver);
                    ctx.builder.set_local(object);
                    self.emit_expr(ctx, index);
                    ctx.builder.set_local(key);
                    ctx.builder.get_local(object);
                    ctx.builder.get_local(key);
                    ctx.builder.get_property_late(qname);
                    self.emit_expr(ctx, value);
                    self.emit_binary_assign(ctx, assign);
                    self.emit_coerce(ctx, &ty);
                    ctx.builder.set_local(result);
                    ctx.builder.get_local(object);
                    ctx.builder.get_local(key);
                    ctx.builder.get_local(result);
                    ctx.builder.set_property_late(qname);
                    ctx.builder.get_local(result);
                }
            }
        }
        let _ = expr;
    }

    fn emit_binary_assign(&mut self, ctx: &mut BodyCtx, assign: AssignOp) {
        match assign {
            AssignOp::Assign => {}
            AssignOp::Add => ctx.builder.binary(op::ADD),
            AssignOp::Sub => ctx.builder.binary(op::SUBTRACT),
            AssignOp::Mul => ctx.builder.binary(op::MULTIPLY),
            AssignOp::Div => ctx.builder.binary(op::DIVIDE),
            AssignOp::Mod => ctx.builder.binary(op::MODULO),
            AssignOp::BitAnd => ctx.builder.binary(op::BITAND),
            AssignOp::BitOr => ctx.builder.binary(op::BITOR),
            AssignOp::BitXor => ctx.builder.binary(op::BITXOR),
            AssignOp::ShiftLeft => ctx.builder.binary(op::LSHIFT),
            AssignOp::ShiftRight => ctx.builder.binary(op::RSHIFT),
            AssignOp::ShiftRightUnsigned => ctx.builder.binary(op::URSHIFT),
        }
    }

    fn emit_incdec(
        &mut self,
        ctx: &mut BodyCtx,
        target: &Expr,
        postfix: PostfixOp,
        is_prefix: bool,
    ) {
        let Some(place) = self.resolve_place(ctx, target) else {
            self.error("Ungültiges Inkrement/Dekrement-Ziel", target.span);
            ctx.builder.push_undefined();
            return;
        };
        let increment = postfix == PostfixOp::Increase;
        match place {
            Place::Local { slot, .. } => {
                ctx.builder.get_local(slot);
                if !is_prefix {
                    ctx.builder.dup();
                    ctx.builder.inc_dec(increment);
                    ctx.builder.set_local(slot);
                } else {
                    ctx.builder.inc_dec(increment);
                    ctx.builder.dup();
                    ctx.builder.set_local(slot);
                }
            }
            Place::Instance {
                receiver,
                qname,
                ty,
            } => {
                self.emit_receiver(ctx, receiver);
                self.emit_field_incdec(ctx, qname, increment, is_prefix, &ty);
            }
            Place::Static {
                class_name,
                qname,
                ty,
            } => {
                ctx.builder.get_lex(class_name);
                self.emit_field_incdec(ctx, qname, increment, is_prefix, &ty);
            }
            Place::Dynamic {
                receiver,
                qname,
                ty,
            } => {
                self.emit_receiver(ctx, receiver);
                self.emit_field_incdec(ctx, qname, increment, is_prefix, &ty);
            }
            Place::Index {
                receiver,
                index,
                qname,
                ty,
            } => {
                let object = ctx.locals.temp();
                let key = ctx.locals.temp();
                self.emit_receiver(ctx, receiver);
                ctx.builder.set_local(object);
                self.emit_expr(ctx, index);
                ctx.builder.set_local(key);
                ctx.builder.get_local(object);
                ctx.builder.get_local(key);
                ctx.builder.get_property_late(qname);
                let old = ctx.locals.temp();
                ctx.builder.set_local(old);
                ctx.builder.get_local(old);
                ctx.builder.inc_dec(increment);
                self.emit_coerce(ctx, &ty);
                let new = ctx.locals.temp();
                ctx.builder.set_local(new);
                ctx.builder.get_local(object);
                ctx.builder.get_local(key);
                ctx.builder.get_local(new);
                ctx.builder.set_property_late(qname);
                if is_prefix {
                    ctx.builder.get_local(new);
                } else {
                    ctx.builder.get_local(old);
                }
            }
        }
    }

    fn emit_field_incdec(
        &mut self,
        ctx: &mut BodyCtx,
        qname: u32,
        increment: bool,
        is_prefix: bool,
        ty: &Type,
    ) {
        let object = ctx.locals.temp();
        ctx.builder.set_local(object);
        ctx.builder.get_local(object);
        ctx.builder.get_property(qname);
        let old = ctx.locals.temp();
        ctx.builder.set_local(old);
        ctx.builder.get_local(old);
        ctx.builder.inc_dec(increment);
        self.emit_coerce(ctx, ty);
        let new = ctx.locals.temp();
        ctx.builder.set_local(new);
        ctx.builder.get_local(object);
        ctx.builder.get_local(new);
        ctx.builder.set_property(qname);
        if is_prefix {
            ctx.builder.get_local(new);
        } else {
            ctx.builder.get_local(old);
        }
    }

    fn emit_receiver(&mut self, ctx: &mut BodyCtx, receiver: Receiver<'_>) {
        match receiver {
            Receiver::This => ctx.builder.get_local(0),
            Receiver::Expr(expr) => self.emit_expr(ctx, expr),
            Receiver::Class(class) => {
                let class_name = self.class_multiname(&class);
                ctx.builder.get_lex(class_name);
            }
        }
    }

    fn resolve_place<'e>(&mut self, ctx: &BodyCtx, target: &'e Expr) -> Option<Place<'e>> {
        match &target.kind {
            ExprKind::Paren(inner) => self.resolve_place(ctx, inner),
            ExprKind::Ident(name) => {
                if let Some(slot) = ctx.locals.lookup(name) {
                    let ty = self.info_of(target).ty;
                    return Some(Place::Local { slot, ty });
                }
                let info = self.info_of(target);
                let ty = info.ty;
                match info.reference {
                    Some(RefKind::InstanceField { class }) => {
                        let access = self.instance_field_access(&class, name);
                        let qname = self.member_multiname(&class, name, access);
                        Some(Place::Instance {
                            receiver: Receiver::This,
                            qname,
                            ty,
                        })
                    }
                    Some(RefKind::StaticField { class }) => {
                        let access = self.static_field_access(&class, name);
                        let qname = self.member_multiname(&class, name, access);
                        let class_name = self.class_multiname(&class);
                        Some(Place::Static {
                            class_name,
                            qname,
                            ty,
                        })
                    }
                    Some(RefKind::Dynamic) => {
                        let class_ty = ctx.class.clone().map(Type::Class).unwrap_or(Type::Any);
                        let qname = self.dynamic_multiname(&class_ty, name);
                        Some(Place::Dynamic {
                            receiver: Receiver::This,
                            qname,
                            ty: Type::Any,
                        })
                    }
                    _ => None,
                }
            }
            ExprKind::Member {
                target: object,
                name,
            } => {
                let info = self.info_of(target);
                match info.reference {
                    Some(RefKind::InstanceField { class }) => {
                        let access = self.instance_field_access(&class, name);
                        let qname = self.member_multiname(&class, name, access);
                        Some(Place::Instance {
                            receiver: Receiver::Expr(object),
                            qname,
                            ty: info.ty,
                        })
                    }
                    Some(RefKind::StaticField { class }) => {
                        let access = self.static_field_access(&class, name);
                        let qname = self.member_multiname(&class, name, access);
                        let class_name = self.class_multiname(&class);
                        Some(Place::Static {
                            class_name,
                            qname,
                            ty: info.ty,
                        })
                    }
                    Some(RefKind::ClassRef { class }) => {
                        let ty = Type::Class(class.clone());
                        let qname = self.dynamic_multiname(&ty, name);
                        let class_name = self.class_multiname(&class);
                        Some(Place::Static {
                            class_name,
                            qname,
                            ty: info.ty,
                        })
                    }
                    _ => {
                        let qname = self.dynamic_multiname(&info.ty, name);
                        Some(Place::Dynamic {
                            receiver: if let Some(RefKind::ClassRef { class }) =
                                info.reference.as_ref()
                            {
                                Receiver::Class(class.clone())
                            } else {
                                Receiver::Expr(object)
                            },
                            qname,
                            ty: Type::Any,
                        })
                    }
                }
            }
            ExprKind::Index { target, index } => {
                let qname = self.abc.constant_pool.add_multiname_l();
                Some(Place::Index {
                    receiver: Receiver::Expr(target),
                    index,
                    qname,
                    ty: Type::Any,
                })
            }
            _ => None,
        }
    }

    fn emit_coerce(&mut self, ctx: &mut BodyCtx, ty: &Type) {
        if matches!(ty, Type::Any | Type::Void | Type::Null) {
            return;
        }
        let multiname = self.any_or_object(ty);
        ctx.builder.coerce(multiname);
    }
}

fn call_name(callee: &Expr) -> &str {
    match &callee.kind {
        ExprKind::Ident(name) => name,
        ExprKind::Member { name, .. } => name,
        _ => "",
    }
}

fn parse_int_literal(text: &str) -> Option<i32> {
    if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        return u64::from_str_radix(hex, 16)
            .ok()
            .and_then(|value| i32::try_from(value).ok());
    }
    text.parse::<i32>().ok()
}

fn parse_uint_literal(text: &str) -> Option<u32> {
    if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        return u32::from_str_radix(hex, 16).ok();
    }
    text.parse::<u32>().ok()
}

// ---------------------------------------------------------------- body state

struct LoopLabels {
    break_label: usize,
    continue_label: usize,
}

struct BodyCtx {
    class: Option<String>,
    is_static: bool,
    is_constructor: bool,
    return_type: Type,
    locals: Locals,
    loops: Vec<LoopLabels>,
    builder: CodeBuilder,
}

impl BodyCtx {
    fn new(
        class: Option<String>,
        is_static: bool,
        return_type: Type,
        params: &[ParamSymbol],
    ) -> Self {
        Self {
            class,
            is_static,
            is_constructor: false,
            return_type,
            locals: Locals::new(params),
            loops: Vec::new(),
            builder: CodeBuilder::new(),
        }
    }
}

struct Locals {
    scopes: Vec<HashMap<String, u32>>,
    next: u32,
}

impl Locals {
    fn new(params: &[ParamSymbol]) -> Self {
        let mut locals = Self {
            scopes: vec![HashMap::new()],
            next: 1,
        };
        for param in params {
            let slot = locals.alloc();
            locals.bind(&param.name, slot);
        }
        locals
    }

    fn alloc(&mut self) -> u32 {
        let slot = self.next;
        self.next += 1;
        slot
    }

    fn temp(&mut self) -> u32 {
        self.alloc()
    }

    fn bind(&mut self, name: &str, slot: u32) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), slot);
        }
    }

    fn push(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    fn lookup(&self, name: &str) -> Option<u32> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
    }
}

enum Receiver<'e> {
    This,
    Expr(&'e Expr),
    Class(String),
}

enum Place<'e> {
    Local {
        slot: u32,
        ty: Type,
    },
    Instance {
        receiver: Receiver<'e>,
        qname: u32,
        ty: Type,
    },
    Static {
        class_name: u32,
        qname: u32,
        ty: Type,
    },
    Dynamic {
        receiver: Receiver<'e>,
        qname: u32,
        ty: Type,
    },
    Index {
        receiver: Receiver<'e>,
        index: &'e Expr,
        qname: u32,
        ty: Type,
    },
}

impl Place<'_> {
    fn ty(&self) -> Type {
        match self {
            Place::Local { ty, .. }
            | Place::Instance { ty, .. }
            | Place::Static { ty, .. }
            | Place::Dynamic { ty, .. }
            | Place::Index { ty, .. } => ty.clone(),
        }
    }
}

// ------------------------------------------------------------------ code builder

struct CodeBuilder {
    code: Vec<u8>,
    labels: Vec<Option<usize>>,
    fixups: Vec<(usize, usize)>,
    stack: u32,
    max_stack: u32,
}

impl Default for CodeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl CodeBuilder {
    fn new() -> Self {
        Self {
            code: Vec::new(),
            labels: Vec::new(),
            fixups: Vec::new(),
            stack: 0,
            max_stack: 0,
        }
    }

    fn finish(mut self) -> (Vec<u8>, u32) {
        for (at, label) in self.fixups.clone() {
            let Some(target) = self.labels.get(label).and_then(|position| *position) else {
                continue;
            };
            let relative = target as i64 - (at as i64 + 3);
            let encoded = (relative as i32) & 0x00ff_ffff;
            self.code[at] = (encoded & 0xff) as u8;
            self.code[at + 1] = ((encoded >> 8) & 0xff) as u8;
            self.code[at + 2] = ((encoded >> 16) & 0xff) as u8;
        }
        // The stack tracker is intentionally conservative, but must never
        // under-report: the AVM2 verifier rejects a body whose real maximum
        // exceeds `max_stack`. A small fixed floor also covers the scope
        // objects used by generated initializers.
        (self.code, self.max_stack.max(64))
    }

    fn push(&mut self) {
        self.stack += 1;
        self.max_stack = self.max_stack.max(self.stack);
    }

    fn pop_count(&mut self, count: u32) {
        self.stack = self.stack.saturating_sub(count);
    }

    fn raw(&mut self, bytes: Vec<u8>) {
        self.code.extend(bytes);
    }

    fn new_label(&mut self) -> usize {
        self.labels.push(None);
        self.labels.len() - 1
    }

    fn place_label(&mut self, label: usize) {
        if let Some(slot) = self.labels.get_mut(label) {
            *slot = Some(self.code.len());
        }
    }

    fn jump(&mut self, label: usize) {
        self.code.push(op::JUMP);
        let at = self.code.len();
        self.code.extend_from_slice(&[0, 0, 0]);
        self.fixups.push((at, label));
    }

    fn jump_if(&mut self, opcode: u8, label: usize) {
        self.code.push(opcode);
        let at = self.code.len();
        self.code.extend_from_slice(&[0, 0, 0]);
        self.fixups.push((at, label));
        self.pop_count(1);
    }

    fn if_false(&mut self, label: usize) {
        self.jump_if(op::IFFALSE, label);
    }

    fn if_true(&mut self, label: usize) {
        self.jump_if(op::IFTRUE, label);
    }

    fn op(&mut self, opcode: u8) {
        self.code.push(opcode);
    }

    fn op_u30(&mut self, opcode: u8, value: u32) {
        self.code.push(opcode);
        op::write_u30(&mut self.code, value);
    }

    fn op_u30_u30(&mut self, opcode: u8, first: u32, second: u32) {
        self.code.push(opcode);
        op::write_u30(&mut self.code, first);
        op::write_u30(&mut self.code, second);
    }

    fn push_int(&mut self, value: i32) {
        debug_assert!((-128..=127).contains(&value));
        self.raw(vec![op::PUSHBYTE, value as i8 as u8]);
        self.push();
    }

    fn push_int_constant(&mut self, index: u32) {
        self.op_u30(op::PUSHINT, index);
        self.push();
    }

    fn push_uint(&mut self, value: u32) {
        debug_assert!(value <= 127);
        self.raw(vec![op::PUSHBYTE, value as u8]);
        self.push();
    }

    fn push_uint_constant(&mut self, index: u32) {
        self.op_u30(op::PUSHUINT, index);
        self.push();
    }

    fn push_double(&mut self, index: u32) {
        self.op_u30(op::PUSHDOUBLE, index);
        self.push();
    }

    fn push_string(&mut self, index: u32) {
        self.op_u30(op::PUSHSTRING, index);
        self.push();
    }

    fn push_true(&mut self) {
        self.op(op::PUSHTRUE);
        self.push();
    }

    fn push_false(&mut self) {
        self.op(op::PUSHFALSE);
        self.push();
    }

    fn push_null(&mut self) {
        self.op(op::PUSHNULL);
        self.push();
    }

    fn push_undefined(&mut self) {
        self.op(op::PUSHUNDEFINED);
        self.push();
    }

    fn push_scope(&mut self) {
        self.op(op::PUSHSCOPE);
    }

    fn pop_scope(&mut self) {
        self.op(op::POPSCOPE);
    }

    fn get_scope_object(&mut self, index: u32) {
        self.op_u30(op::GETSCOPEOBJECT, index);
        self.push();
    }

    fn get_local(&mut self, slot: u32) {
        self.raw(op::get_local(slot));
        self.push();
    }

    fn set_local(&mut self, slot: u32) {
        self.raw(op::set_local(slot));
        self.pop_count(1);
    }

    fn get_lex(&mut self, multiname: u32) {
        self.op_u30(op::GETLEX, multiname);
        self.push();
    }

    fn get_property(&mut self, multiname: u32) {
        self.op_u30(op::GETPROPERTY, multiname);
    }

    fn get_property_late(&mut self, multiname: u32) {
        self.op_u30(op::GETPROPERTY, multiname);
        self.pop_count(2);
        self.push();
    }

    fn set_property(&mut self, multiname: u32) {
        self.op_u30(op::SETPROPERTY, multiname);
        self.pop_count(2);
    }

    fn set_property_late(&mut self, multiname: u32) {
        self.op_u30(op::SETPROPERTY, multiname);
        self.pop_count(3);
    }

    fn new_array(&mut self, count: u32) {
        self.op_u30(op::NEWARRAY, count);
        self.pop_count(count);
        self.push();
    }

    fn new_object(&mut self, count: u32) {
        self.op_u30(op::NEWOBJECT, count);
        self.pop_count(count * 2);
        self.push();
    }

    fn find_property(&mut self, multiname: u32) {
        self.op_u30(op::FINDPROPERTY, multiname);
        self.push();
    }

    fn find_prop_strict(&mut self, multiname: u32) {
        self.op_u30(op::FINDPROPSTRICT, multiname);
        self.push();
    }

    fn init_property(&mut self, multiname: u32) {
        self.op_u30(op::INITPROPERTY, multiname);
        self.pop_count(2);
    }

    fn call_property(&mut self, multiname: u32, argc: u32) {
        self.op_u30_u30(op::CALLPROPERTY, multiname, argc);
        self.pop_count(argc + 1);
        self.push();
    }

    fn call(&mut self, argc: u32) {
        self.op_u30(op::CALL, argc);
        // function, receiver, and arguments are consumed.
        self.pop_count(argc + 2);
        self.push();
    }

    fn construct(&mut self, argc: u32) {
        self.op_u30(op::CONSTRUCT, argc);
        self.pop_count(argc + 1);
        self.push();
    }

    fn construct_super(&mut self, argc: u32) {
        self.op_u30(op::CONSTRUCTSUPER, argc);
        self.pop_count(argc + 1);
    }

    fn new_class(&mut self, index: u32) {
        self.op_u30(op::NEWCLASS, index);
    }

    fn coerce(&mut self, multiname: u32) {
        self.op_u30(op::COERCE, multiname);
    }

    fn istype(&mut self, multiname: u32) {
        self.op_u30(op::ISTYPE, multiname);
    }

    fn astype(&mut self, multiname: u32) {
        self.op_u30(op::ASTYPE, multiname);
    }

    fn dup(&mut self) {
        self.op(op::DUP);
        self.push();
    }

    fn pop_value(&mut self) {
        self.op(op::POP);
        self.pop_count(1);
    }

    fn binary(&mut self, opcode: u8) {
        self.op(opcode);
        self.pop_count(2);
        self.push();
    }

    fn unary(&mut self, opcode: u8) {
        self.op(opcode);
    }

    fn inc_dec(&mut self, increment: bool) {
        self.op(if increment {
            op::INCREMENT
        } else {
            op::DECREMENT
        });
    }

    fn return_void(&mut self) {
        self.op(op::RETURNVOID);
    }

    fn return_value(&mut self) {
        self.op(op::RETURNVALUE);
        self.pop_count(1);
    }

    fn throw(&mut self) {
        self.op(op::THROW);
        self.pop_count(1);
    }
}
