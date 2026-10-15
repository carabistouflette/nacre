//! Nacre: Rust frontend -> typed ownership IR -> C11 -> native code.
//! Classes, interfaces, generic sums and consuming exhaustive match use unique ownership.
//! Lexical shared/exclusive borrows, readonly methods, and scoped shared views.
//! Interfaces use allocation-free fat pointers; generic classes/enums are monomorphized.
//! Native Option/Result, IO/text support, and multi-source projects.
//! Borrowed returns are receiver-bound; no class inheritance or general lifetime parameters.
#![forbid(unsafe_code)]
mod ast;
mod checker;
mod c_repr;
mod codegen;
pub mod diagnostic;
mod ir;
mod lexer;
pub mod native;
mod parser;
mod generics;
mod resolver;
mod ownership;
mod borrowing;
mod stdlib;
mod stdlib_codegen;
pub mod lsp;
pub mod project;
pub mod test_runner;
pub mod bench_runner;
pub mod doc;
pub mod fmt;
pub mod llvm_codegen;
mod type_report;

use diagnostic::{Diagnostic, Span};

fn front_end(source: &str) -> Result<ir::Program, Diagnostic> {
    if source.len() > 1024 * 1024 {
        return Err(Diagnostic::new(Span::default(), "source is too large: limit is 1 MiB"));
    }
    let tokens = lexer::lex(source)?;
    check_parsed(parser::parse(tokens)?)
}

pub(crate) fn check_parsed(mut parsed: ast::Program) -> Result<ir::Program, Diagnostic> {
    resolver::resolve(&mut parsed)?;
    stdlib::inject(&mut parsed)?;
    let (expanded, _) = generics::expand(parsed)?;
    let typed = checker::check(&expanded)?;
    ownership::check(&typed)?;
    borrowing::check(&typed)?;
    Ok(typed)
}

/// Parse and type-check without requiring a C compiler.
pub fn validate(source: &str) -> Result<(), Diagnostic> { front_end(source).map(|_| ()) }

/// Generate a self-contained C11 translation unit (requires libc to link).
pub fn compile_to_c(source: &str) -> Result<String, Diagnostic> {
    let checked = front_end(source)?;
    Ok(codegen::generate(&checked, source))
}

/// Inspect the control-flow graph used for definite initialization and ownership.
/// Does not require a C compiler and does not emit executable code.
pub fn emit_flow(source: &str) -> Result<String, Diagnostic> {
    let checked = front_end(source)?;
    ownership::dump(&checked, source)
}

/// Inspect the lexical borrow checker actually used by the frontend.
pub fn emit_borrows(source: &str) -> Result<String, Diagnostic> {
    let checked = front_end(source)?;
    borrowing::dump(&checked, source)
}

/// Inspect the deterministic template specializations used by this program.
/// Full validation is performed first; this is not a bypass around safety checks.
pub fn emit_types(source: &str) -> Result<String, Diagnostic> {
    Ok(type_report::dump(&front_end(source)?))
}

/// Generate direct textual LLVM IR without intermediary C translation.
pub fn emit_llvm_direct(source: &str) -> Result<String, Diagnostic> {
    let checked = front_end(source)?;
    llvm_codegen::generate_direct_llvm(&checked, source).map_err(|e| Diagnostic::new(Span::default(), e))
}

