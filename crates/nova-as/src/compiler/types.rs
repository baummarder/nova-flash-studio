//! ActionScript 3 type system, class/member symbol tables and lexical scopes.
//!
//! The compiler models the small AS3 subset explicitly. Types are interned as
//! [`Type`] values; user classes declared in the compilation unit live in
//! [`TypeWorld::classes`], while imported Flash classes remain opaque external
//! classes addressed by their fully qualified name.

use std::collections::HashMap;

use crate::ast::TypeRef;
use crate::token::Span;

/// A resolved ActionScript 3 type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    /// `void`, valid only as a method return type.
    Void,
    /// `*` — the untyped/dynamic type.
    Any,
    /// The `null` literal type.
    Null,
    Int,
    Uint,
    Number,
    Boolean,
    String,
    /// The `Object` built-in.
    Object,
    /// A class referenced by fully qualified name, e.g. `flash.display.MovieClip`.
    Class(String),
}

impl Type {
    pub fn is_void(&self) -> bool {
        matches!(self, Self::Void)
    }

    pub fn is_dynamic(&self) -> bool {
        matches!(self, Self::Any | Self::Object | Self::Class(_))
    }

    pub fn is_numeric(&self) -> bool {
        matches!(self, Self::Int | Self::Uint | Self::Number)
    }

    pub fn is_int_like(&self) -> bool {
        matches!(self, Self::Int | Self::Uint)
    }

    pub fn qname(&self) -> Option<&str> {
        match self {
            Self::Class(name) => Some(name),
            _ => None,
        }
    }

    /// Human readable name used in diagnostics.
    pub fn display(&self) -> String {
        match self {
            Self::Void => "void".into(),
            Self::Any => "*".into(),
            Self::Null => "null".into(),
            Self::Int => "int".into(),
            Self::Uint => "uint".into(),
            Self::Number => "Number".into(),
            Self::Boolean => "Boolean".into(),
            Self::String => "String".into(),
            Self::Object => "Object".into(),
            Self::Class(name) => name.clone(),
        }
    }
}

/// How an expression refers to a storage location or declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefKind {
    /// A local variable or parameter.
    Local,
    /// An instance field declared by `class`.
    InstanceField { class: String },
    /// A static field of a class.
    StaticField { class: String },
    /// An instance method of a class.
    InstanceMethod { class: String },
    /// A static method of a class.
    StaticMethod { class: String },
    /// A package-level function.
    GlobalFunction,
    /// The class object itself, e.g. `MovieClip` in `MovieClip.x`.
    ClassRef { class: String },
    /// A property whose declaring type is not modelled.
    Dynamic,
}

/// The dispatch form recorded for a call expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallKind {
    Instance,
    Static,
    Global,
    Dynamic,
}

/// Type information attached to one AST expression.
#[derive(Debug, Clone)]
pub struct ExprInfo {
    pub ty: Type,
    pub reference: Option<RefKind>,
    pub call: Option<CallKind>,
}

impl Default for ExprInfo {
    fn default() -> Self {
        Self {
            ty: Type::Any,
            reference: None,
            call: None,
        }
    }
}

/// Member visibility, mapped to ABC namespace kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Public,
    Private,
    Protected,
    Internal,
}

#[derive(Debug, Clone)]
pub struct ParamSymbol {
    pub name: String,
    pub ty: Type,
    pub has_default: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct MethodSymbol {
    pub name: String,
    /// Multiname-free member name of the method (accessor-style names are
    /// rejected by the checker).
    pub access: Access,
    pub params: Vec<ParamSymbol>,
    pub return_type: Type,
    pub is_static: bool,
    pub is_constructor: bool,
    pub is_override: bool,
    pub is_final: bool,
    pub span: Span,
}

impl MethodSymbol {
    pub fn required_params(&self) -> usize {
        self.params.iter().filter(|p| !p.has_default).count()
    }
}

#[derive(Debug, Clone)]
pub struct FieldSymbol {
    pub name: String,
    pub access: Access,
    pub ty: Type,
    pub is_static: bool,
    pub is_const: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ClassSymbol {
    pub name: String,
    pub qualified_name: String,
    pub package: String,
    pub base: Option<String>,
    pub extends_movieclip: bool,
    pub is_final: bool,
    pub fields: Vec<FieldSymbol>,
    pub methods: Vec<MethodSymbol>,
    pub static_fields: Vec<FieldSymbol>,
    pub static_methods: Vec<MethodSymbol>,
    pub constructor: Option<MethodSymbol>,
    pub source_line: usize,
}

impl ClassSymbol {
    pub fn instance_field(&self, name: &str) -> Option<&FieldSymbol> {
        self.fields.iter().find(|f| f.name == name)
    }

    pub fn instance_method(&self, name: &str) -> Option<&MethodSymbol> {
        self.methods.iter().find(|m| m.name == name)
    }

    pub fn static_field(&self, name: &str) -> Option<&FieldSymbol> {
        self.static_fields.iter().find(|f| f.name == name)
    }

    pub fn static_method(&self, name: &str) -> Option<&MethodSymbol> {
        self.static_methods.iter().find(|m| m.name == name)
    }
}

/// All symbols visible to the checker and emitter.
#[derive(Debug, Clone, Default)]
pub struct TypeWorld {
    /// Package of the compilation unit (empty for the default package).
    pub package: String,
    /// Simple imported name -> fully qualified class name.
    pub imports: HashMap<String, String>,
    /// Packages imported with `.*`.
    pub wildcard_imports: Vec<String>,
    /// Classes declared in this unit.
    pub classes: Vec<ClassSymbol>,
    /// Package-level functions declared in this unit.
    pub functions: Vec<MethodSymbol>,
}

impl TypeWorld {
    pub fn class(&self, qname: &str) -> Option<&ClassSymbol> {
        self.classes.iter().find(|c| c.qualified_name == qname)
    }

    pub fn class_mut(&mut self, qname: &str) -> Option<&mut ClassSymbol> {
        self.classes.iter_mut().find(|c| c.qualified_name == qname)
    }

    pub fn bare_class(&self, name: &str) -> Option<&ClassSymbol> {
        self.classes
            .iter()
            .find(|c| c.name == name && c.package == self.package)
    }

    pub fn function(&self, name: &str) -> Option<&MethodSymbol> {
        self.functions.iter().find(|f| f.name == name)
    }

    /// Resolves a type name (as written in source) to a [`Type`].
    ///
    /// Unknown simple names resolve to `None`. Dotted names are assumed to be
    /// external classes, matching the "imported names are external" rule.
    pub fn resolve_type_name(&self, name: &str) -> Option<Type> {
        match name {
            "void" => Some(Type::Void),
            "*" => Some(Type::Any),
            "int" => Some(Type::Int),
            "uint" => Some(Type::Uint),
            "Number" => Some(Type::Number),
            "Boolean" => Some(Type::Boolean),
            "String" => Some(Type::String),
            "Object" => Some(Type::Object),
            "Array" | "Function" => None,
            _ => {
                if let Some(qname) = self.imports.get(name) {
                    Some(Type::Class(qname.clone()))
                } else if let Some(class) = self.bare_class(name) {
                    Some(Type::Class(class.qualified_name.clone()))
                } else if name.contains('.') {
                    Some(Type::Class(name.to_string()))
                } else {
                    self.wildcard_imports
                        .first()
                        .map(|package| Type::Class(format!("{package}.{name}")))
                }
            }
        }
    }

    pub fn resolve_type_ref(&self, reference: &TypeRef) -> Option<Type> {
        if reference.array_depth > 0 {
            return None;
        }
        self.resolve_type_name(&reference.name)
    }

    /// Resolves a source name to a class QName usable as a class reference.
    pub fn resolve_class_name(&self, name: &str) -> Option<String> {
        match self.resolve_type_name(name)? {
            Type::Class(qname) => Some(qname),
            Type::Int => Some("int".into()),
            Type::Uint => Some("uint".into()),
            Type::Number => Some("Number".into()),
            Type::Boolean => Some("Boolean".into()),
            Type::String => Some("String".into()),
            Type::Object => Some("Object".into()),
            Type::Void | Type::Any | Type::Null => None,
        }
    }

    /// Package that declares `ty`, used to pick the namespace of public members.
    pub fn type_package(&self, ty: &Type) -> String {
        match ty {
            Type::Class(qname) => package_of_qname(qname).to_string(),
            _ => String::new(),
        }
    }

    pub fn is_subclass(&self, sub: &str, sup: &str) -> bool {
        if sub == sup {
            return true;
        }
        let mut current = sub;
        let mut guard = 0;
        while let Some(class) = self.class(current) {
            let Some(base) = class.base.as_deref() else {
                return false;
            };
            if base == sup {
                return true;
            }
            current = base;
            guard += 1;
            if guard > 64 {
                return false;
            }
        }
        false
    }

    /// The first base class in `class_qname`'s chain that is not declared in
    /// this unit. External bases still expose members at runtime, so the
    /// checker treats unknown members on them as dynamic.
    pub fn external_ancestor(&self, class_qname: &str) -> Option<String> {
        let mut current = self.class(class_qname);
        let mut guard = 0;
        while let Some(class) = current {
            let Some(base) = class.base.as_deref() else {
                return None;
            };
            match self.class(base) {
                Some(base_class) => current = Some(base_class),
                None => return Some(base.to_string()),
            }
            guard += 1;
            if guard > 64 {
                return None;
            }
        }
        None
    }

    /// Resolves an instance field through the class hierarchy. Returns the
    /// declaring class and the field symbol.
    pub fn find_instance_field(
        &self,
        class_qname: &str,
        name: &str,
    ) -> Option<(String, FieldSymbol)> {
        let mut current = self.class(class_qname)?;
        let mut guard = 0;
        loop {
            if let Some(field) = current.instance_field(name) {
                return Some((current.qualified_name.clone(), field.clone()));
            }
            let base = current.base.as_deref()?;
            current = self.class(base)?;
            guard += 1;
            if guard > 64 {
                return None;
            }
        }
    }

    /// Resolves an instance method through the class hierarchy.
    pub fn find_instance_method(
        &self,
        class_qname: &str,
        name: &str,
    ) -> Option<(String, MethodSymbol)> {
        let mut current = self.class(class_qname)?;
        let mut guard = 0;
        loop {
            if let Some(method) = current.instance_method(name) {
                return Some((current.qualified_name.clone(), method.clone()));
            }
            let base = current.base.as_deref()?;
            current = self.class(base)?;
            guard += 1;
            if guard > 64 {
                return None;
            }
        }
    }

    /// Resolves a static field through the class hierarchy.
    pub fn find_static_field(
        &self,
        class_qname: &str,
        name: &str,
    ) -> Option<(String, FieldSymbol)> {
        let mut current = self.class(class_qname)?;
        let mut guard = 0;
        loop {
            if let Some(field) = current.static_field(name) {
                return Some((current.qualified_name.clone(), field.clone()));
            }
            let base = current.base.as_deref()?;
            current = self.class(base)?;
            guard += 1;
            if guard > 64 {
                return None;
            }
        }
    }

    /// Resolves a static method through the class hierarchy.
    pub fn find_static_method(
        &self,
        class_qname: &str,
        name: &str,
    ) -> Option<(String, MethodSymbol)> {
        let mut current = self.class(class_qname)?;
        let mut guard = 0;
        loop {
            if let Some(method) = current.static_method(name) {
                return Some((current.qualified_name.clone(), method.clone()));
            }
            let base = current.base.as_deref()?;
            current = self.class(base)?;
            guard += 1;
            if guard > 64 {
                return None;
            }
        }
    }

    /// Reachability-aware assignability used by the type checker.
    pub fn is_assignable(&self, target: &Type, source: &Type) -> bool {
        if target == &Type::Void {
            return false;
        }
        if matches!(target, Type::Any | Type::Object) {
            return !source.is_void();
        }
        if matches!(source, Type::Any | Type::Null) {
            return true;
        }
        if source.is_void() {
            return false;
        }
        if target == source {
            return true;
        }
        match (target, source) {
            (Type::Number, Type::Int | Type::Uint) => true,
            (Type::Int | Type::Uint, Type::Number) => true,
            (Type::Boolean, Type::Boolean) => true,
            (Type::String, Type::String) => true,
            (Type::Class(a), Type::Class(b)) => self.is_subclass(b, a),
            _ => false,
        }
    }

    /// Unifies two branch/value types (best effort, `*` on disagreement).
    pub fn unify(&self, a: &Type, b: &Type) -> Type {
        if a == b {
            return a.clone();
        }
        if matches!(a, Type::Null) {
            return b.clone();
        }
        if matches!(b, Type::Null) {
            return a.clone();
        }
        if a.is_numeric() && b.is_numeric() {
            return if matches!(a, Type::Number) || matches!(b, Type::Number) {
                Type::Number
            } else if matches!(a, Type::Uint) || matches!(b, Type::Uint) {
                Type::Uint
            } else {
                Type::Int
            };
        }
        if self.is_assignable(a, b) {
            return a.clone();
        }
        if self.is_assignable(b, a) {
            return b.clone();
        }
        Type::Any
    }
}

/// Splits a fully qualified class name into `(package, simple_name)`.
pub fn split_qname(qname: &str) -> (&str, &str) {
    match qname.rsplit_once('.') {
        Some((package, name)) => (package, name),
        None => ("", qname),
    }
}

pub fn package_of_qname(qname: &str) -> &str {
    split_qname(qname).0
}

/// A local variable or parameter visible in the checker.
#[derive(Debug, Clone)]
pub struct LocalSymbol {
    pub ty: Type,
    pub is_const: bool,
}

/// Lexical scope stack used by the checker.
#[derive(Debug, Clone, Default)]
pub struct Scopes {
    scopes: Vec<HashMap<String, LocalSymbol>>,
}

impl Scopes {
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
        }
    }

    pub fn depth(&self) -> usize {
        self.scopes.len()
    }

    pub fn push(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub fn pop(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    /// Declares a local. Returns false when the name already exists in the
    /// current scope.
    pub fn declare(&mut self, name: &str, ty: Type, is_const: bool) -> bool {
        let scope = self.scopes.last_mut().expect("scope stack is never empty");
        if scope.contains_key(name) {
            return false;
        }
        scope.insert(name.to_string(), LocalSymbol { ty, is_const });
        true
    }

    pub fn lookup(&self, name: &str) -> Option<&LocalSymbol> {
        self.scopes.iter().rev().find_map(|scope| scope.get(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assignability_follows_class_hierarchy() {
        let world = TypeWorld {
            package: String::new(),
            classes: vec![
                ClassSymbol {
                    name: "Base".into(),
                    qualified_name: "Base".into(),
                    package: String::new(),
                    base: None,
                    extends_movieclip: false,
                    is_final: false,
                    fields: vec![],
                    methods: vec![],
                    static_fields: vec![],
                    static_methods: vec![],
                    constructor: None,
                    source_line: 1,
                },
                ClassSymbol {
                    name: "Child".into(),
                    qualified_name: "Child".into(),
                    package: String::new(),
                    base: Some("Base".into()),
                    extends_movieclip: false,
                    is_final: false,
                    fields: vec![],
                    methods: vec![],
                    static_fields: vec![],
                    static_methods: vec![],
                    constructor: None,
                    source_line: 1,
                },
            ],
            ..Default::default()
        };
        assert!(world.is_assignable(&Type::Class("Base".into()), &Type::Class("Child".into())));
        assert!(!world.is_assignable(&Type::Class("Child".into()), &Type::Class("Base".into())));
        assert!(world.is_assignable(&Type::Number, &Type::Int));
        assert!(world.is_assignable(&Type::Class("Base".into()), &Type::Null));
    }

    #[test]
    fn scopes_shadow_and_detect_duplicates() {
        let mut scopes = Scopes::new();
        assert!(scopes.declare("x", Type::Int, false));
        assert!(!scopes.declare("x", Type::Int, false));
        scopes.push();
        assert!(scopes.declare("x", Type::String, true));
        assert_eq!(scopes.lookup("x").unwrap().ty, Type::String);
        scopes.pop();
        assert_eq!(scopes.lookup("x").unwrap().ty, Type::Int);
    }
}
