//! ActionScript 3 subset compiler: type checking plus AVM2 ABC generation.
//!
//! The compiler is deliberately conservative. Constructs that the emitter
//! cannot lower to verified AVM2 bytecode produce an error diagnostic instead
//! of being silently ignored or mistranslated. Whenever any error diagnostic is
//! produced, [`CompiledUnit::abc`] is empty so callers never embed partial ABC.

pub mod abc;
pub mod check;
pub mod emit;
pub mod opcode;
pub mod types;

use crate::diagnostic::Diagnostic;
use crate::lexer::lex;
use crate::parser::{has_errors, parse_tokens};

pub use check::CheckOutcome;
pub use emit::CompiledClass;

/// Compilation settings for one source file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompileOptions {
    /// Overrides the package declared in the source. An empty string means the
    /// default package.
    pub package: Option<String>,
    /// Optional main class. Its simple or fully qualified name is moved to the
    /// front of the emitted class list; an unknown name is an error.
    pub main_class: Option<String>,
}

/// Result of compiling one source file.
#[derive(Debug, Clone, Default)]
pub struct CompiledUnit {
    /// ABC bytes. Empty when compilation failed.
    pub abc: Vec<u8>,
    /// Classes that were emitted into the ABC file, in emission order.
    pub classes: Vec<CompiledClass>,
    /// All lexer, parser, semantic, checker and emitter diagnostics.
    pub diagnostics: Vec<Diagnostic>,
}

impl CompiledUnit {
    pub fn has_errors(&self) -> bool {
        has_errors(&self.diagnostics)
    }
}

/// Backwards-compatible name for [`CompiledUnit`].
pub type CompileOutput = CompiledUnit;

/// Lexes, parses, type-checks and compiles `source` into an ABC file using
/// default options.
pub fn compile(source: &str) -> CompiledUnit {
    compile_with_options(source, CompileOptions::default())
}

/// Like [`compile`], with explicit package and main-class options.
pub fn compile_with_options(source: &str, options: CompileOptions) -> CompiledUnit {
    let lexed = lex(source);
    let parsed = parse_tokens(&lexed.tokens);
    let mut diagnostics = lexed.diagnostics;
    diagnostics.extend(parsed.diagnostics);
    if has_errors(&diagnostics) {
        return CompiledUnit {
            diagnostics,
            ..Default::default()
        };
    }

    let unit = parsed.unit;
    diagnostics.extend(crate::sema::check(&unit));
    if has_errors(&diagnostics) {
        return CompiledUnit {
            diagnostics,
            ..Default::default()
        };
    }

    let package = options
        .package
        .as_deref()
        .unwrap_or_else(|| unit.package.as_deref().unwrap_or(""));
    let checked = check::check(&unit, package);
    diagnostics.extend(checked.diagnostics);
    if has_errors(&diagnostics) {
        return CompiledUnit {
            diagnostics,
            ..Default::default()
        };
    }

    let emitted = emit::emit(
        &unit,
        &checked.world,
        &checked.expr_info,
        package,
        options.main_class.as_deref(),
    );
    diagnostics.extend(emitted.diagnostics);
    let failed = has_errors(&diagnostics);
    CompiledUnit {
        abc: if failed { Vec::new() } else { emitted.abc },
        classes: if failed { Vec::new() } else { emitted.classes },
        diagnostics,
    }
}

/// Compatibility wrapper that ignores `source_name`; diagnostics do not carry
/// file names yet.
pub fn compile_with_name(source: &str, source_name: &str) -> CompiledUnit {
    let _ = source_name;
    compile(source)
}

/// Convenience helper for callers that already hold a parsed unit.
pub fn compile_unit(unit: &crate::ast::CompilationUnit, options: &CompileOptions) -> CompiledUnit {
    let package = options
        .package
        .as_deref()
        .unwrap_or_else(|| unit.package.as_deref().unwrap_or(""));
    let mut diagnostics = crate::sema::check(unit);
    if has_errors(&diagnostics) {
        return CompiledUnit {
            diagnostics,
            ..Default::default()
        };
    }
    let checked = check::check(unit, package);
    diagnostics.extend(checked.diagnostics);
    if has_errors(&diagnostics) {
        return CompiledUnit {
            diagnostics,
            ..Default::default()
        };
    }
    let emitted = emit::emit(
        unit,
        &checked.world,
        &checked.expr_info,
        package,
        options.main_class.as_deref(),
    );
    diagnostics.extend(emitted.diagnostics);
    let failed = has_errors(&diagnostics);
    CompiledUnit {
        abc: if failed { Vec::new() } else { emitted.abc },
        classes: if failed { Vec::new() } else { emitted.classes },
        diagnostics,
    }
}
