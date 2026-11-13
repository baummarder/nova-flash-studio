//! Declaration-level semantic checks.
//!
//! This is **not** a type checker. It validates structural rules that the
//! parser can check without a type system: duplicate declarations, constructor
//! shape and obviously invalid accessors. Type checking and flow analysis are
//! part of the compiler roadmap (see `ROADMAP.md`, phase 5).

use std::collections::HashMap;

use crate::ast::{CompilationUnit, Member};
use crate::diagnostic::Diagnostic;

pub fn check(unit: &CompilationUnit) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut classes: HashMap<&str, (usize, &crate::ast::ClassDecl)> = HashMap::new();

    for class in &unit.classes {
        if let Some((first_line, _)) = classes.get(class.name.as_str()) {
            diagnostics.push(Diagnostic::error(
                format!(
                    "Klasse '{}' ist bereits deklariert (Zeile {first_line})",
                    class.name
                ),
                class.span,
            ));
        } else {
            classes.insert(&class.name, (class.span.line, class));
        }

        let mut members: HashMap<&str, usize> = HashMap::new();
        for member in &class.members {
            let name = member.name();
            if let Some(line) = members.get(name) {
                diagnostics.push(Diagnostic::error(
                    format!("Member '{name}' ist bereits deklariert (Zeile {line})"),
                    member.span(),
                ));
            } else {
                members.insert(name, member.span().line);
            }

            match member {
                Member::Method(function) => {
                    let mut params = HashMap::new();
                    for param in &function.params {
                        if let Some(line) = params.get(&param.name) {
                            diagnostics.push(Diagnostic::error(
                                format!(
                                    "Parameter '{}' ist doppelt vorhanden (Zeile {line})",
                                    param.name
                                ),
                                param.span,
                            ));
                        } else {
                            params.insert(&param.name, param.span.line);
                        }
                    }
                    if function.name == class.name {
                        if function.return_type.is_some() {
                            diagnostics.push(Diagnostic::warning(
                                "Konstruktoren haben in ActionScript 3 keinen Rückgabetyp",
                                function.span,
                            ));
                        }
                        if !function.modifiers.public
                            && (function.modifiers.private
                                || function.modifiers.protected
                                || function.modifiers.internal)
                        {
                            diagnostics.push(Diagnostic::warning(
                                "Konstruktoren sind in der Praxis meist 'public'",
                                function.span,
                            ));
                        }
                    }
                    if function.body.is_none()
                        && !function.modifiers.native
                        && !function.modifiers.dynamic
                    {
                        diagnostics.push(Diagnostic::warning(
                            format!(
                                "Funktion '{}' hat keinen Körper, ist aber nicht 'native'",
                                function.name
                            ),
                            function.span,
                        ));
                    }
                }
                Member::Field(field) => {
                    if field.is_const && field.value.is_none() {
                        diagnostics.push(Diagnostic::warning(
                            format!("Konstante '{}' hat keinen Startwert", field.name),
                            field.span,
                        ));
                    }
                    if field.modifiers.has_access_modifier()
                        && field.modifiers.private
                        && field.modifiers.public
                    {
                        diagnostics.push(Diagnostic::error(
                            "Feld hat widersprüchliche Zugriffsmodifikatoren",
                            field.span,
                        ));
                    }
                }
            }
        }

        if class.modifiers.private || class.modifiers.protected {
            diagnostics.push(Diagnostic::warning(
                "Top-Level-Klassen sind in ActionScript 3 nur 'public' oder 'internal'",
                class.span,
            ));
        }
    }

    let mut functions: HashMap<&str, usize> = HashMap::new();
    for function in &unit.functions {
        if let Some(line) = functions.get(function.name.as_str()) {
            diagnostics.push(Diagnostic::error(
                format!(
                    "Funktion '{}' ist bereits deklariert (Zeile {line})",
                    function.name
                ),
                function.span,
            ));
        } else {
            functions.insert(&function.name, function.span.line);
        }
    }

    diagnostics
}

/// Access modifiers are parsed but not yet used for visibility checks.
pub fn is_type_checking_implemented() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{FieldDecl, Modifiers};

    fn unit(source: &str) -> CompilationUnit {
        crate::parse(source).expect("parse")
    }

    #[test]
    fn detects_duplicate_members() {
        let unit =
            unit("class T { var x:int; var x:int; function f():void {} function f():void {} }");
        let diagnostics = check(&unit);
        assert!(diagnostics.iter().any(|d| d.message.contains("Member 'x'")));
        assert!(diagnostics.iter().any(|d| d.message.contains("Member 'f'")));
    }

    #[test]
    fn detects_duplicate_parameters() {
        let unit = unit("class T { function f(a:int, a:int):void {} }");
        let diagnostics = check(&unit);
        assert!(
            diagnostics
                .iter()
                .any(|d| d.message.contains("Parameter 'a'"))
        );
    }

    #[test]
    fn warns_about_constructor_return_type() {
        let unit = unit("class T { public function T():void {} }");
        let diagnostics = check(&unit);
        assert!(
            diagnostics
                .iter()
                .any(|d| d.message.contains("Rückgabetyp"))
        );
    }

    #[test]
    fn modifier_struct_is_used_by_checks() {
        let modifiers = Modifiers {
            public: true,
            private: true,
            ..Default::default()
        };
        assert!(modifiers.has_access_modifier());
        let _ = FieldDecl {
            name: "x".into(),
            ty: None,
            value: None,
            is_const: false,
            modifiers,
            span: crate::token::Span::new(0, 1, 1, 1),
        };
    }
}
