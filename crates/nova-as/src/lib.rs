//! ActionScript 3 front-end and AVM2 ABC compiler for NØVA Flash Studio.
//!
//! Implemented today: lexer, parser, AST, declaration-level semantic checks,
//! subset type checking and AVM2 ABC bytecode generation. Unsupported language
//! constructs are rejected with error diagnostics and no ABC is emitted for an
//! erroneous unit. SWF script linking is still handled by other crates; see
//! `ARCHITECTURE.md` for the intended pipeline.

pub mod ast;
pub mod compiler;
pub mod diagnostic;
pub mod lexer;
pub mod parser;
pub mod sema;
pub mod token;

pub use ast::{ClassDecl, CompilationUnit, Expr, ExprKind, FunctionDecl, Stmt, StmtKind};
pub use compiler::{
    CompileOptions, CompileOutput, CompiledClass, CompiledUnit, compile, compile_with_name,
    compile_with_options,
};
pub use diagnostic::{Diagnostic, Severity};
pub use lexer::{LexResult, lex};
pub use parser::{ParseResult, parse_tokens};
pub use token::{Keyword, Span, Symbol, Token, TokenKind};

/// Lexes and parses a source file.
///
/// Returns all diagnostics (lexer + parser). A unit is only returned when no
/// error-severity diagnostics were produced; warnings are allowed.
pub fn parse(source: &str) -> Result<CompilationUnit, Vec<Diagnostic>> {
    let lexed = lex(source);
    let parsed = parse_tokens(&lexed.tokens);
    let mut diagnostics = lexed.diagnostics;
    diagnostics.extend(parsed.diagnostics);
    if parser::has_errors(&diagnostics) {
        Err(diagnostics)
    } else {
        Ok(parsed.unit)
    }
}

/// Parses and runs declaration checks. Returns the unit plus all diagnostics.
pub fn analyze(source: &str) -> (Option<CompilationUnit>, Vec<Diagnostic>) {
    let lexed = lex(source);
    let parsed = parse_tokens(&lexed.tokens);
    let mut diagnostics = lexed.diagnostics;
    diagnostics.extend(parsed.diagnostics);
    if parser::has_errors(&diagnostics) {
        return (None, diagnostics);
    }
    diagnostics.extend(sema::check(&parsed.unit));
    (Some(parsed.unit), diagnostics)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_demo_script_from_core() {
        let source = r#"package {
    import flash.display.MovieClip;
    import flash.events.Event;

    public class Main extends MovieClip {
        private var speed:Number = 2.5;

        public function Main() {
            addEventListener(Event.ENTER_FRAME, update);
        }

        private function update(event:Event):void {
            titel.x += speed;
            if (titel.x > stage.stageWidth) {
                titel.x = 0;
            }
        }
    }
}
"#;
        let (unit, diagnostics) = analyze(source);
        assert!(diagnostics.iter().all(|d| d.severity == Severity::Warning));
        let unit = unit.expect("unit");
        assert_eq!(unit.classes[0].name, "Main");
    }

    #[test]
    fn invalid_source_returns_error_diagnostics() {
        let result = parse("class {");
        assert!(result.is_err());
        assert!(!result.unwrap_err().is_empty());
    }

    #[test]
    fn lexer_and_parser_diagnostics_are_combined() {
        let (_, diagnostics) = analyze("var s = \"unterminated;\nclass {");
        assert!(diagnostics.len() >= 2);
    }
}
