//! Nacre: Rust frontend -> typed ownership IR -> C11 -> native code.
//! Classes, interfaces, generic sums and consuming exhaustive match use unique ownership.
//! Lexical shared/exclusive borrows, readonly methods, and scoped shared views.
//! Interfaces use allocation-free fat pointers; generic classes/enums are monomorphized.
//! Native Option/Result, IO/text support, and multi-source projects.
//! No borrowed returns, class inheritance, or general lifetime parameters.
#![forbid(unsafe_code)]
mod ast;
mod checker;
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
    Ok(type_dump(&front_end(source)?))
}

pub(crate) fn type_dump(typed: &ir::Program) -> String {
    let mut out = String::from("Nacre — nominal types and specializations\n");
    for class in &typed.classes {
        let kind = if class.variants.is_some() { "enum" } else if class.is_interface { "interface" } else { "class" };
        out.push_str(&format!("#{} {kind} {}\n", class.id, class.name));
        if let Some(variants) = &class.variants {
            for variant in variants {
                out.push_str(&format!("  {}({})\n", variant.name, variant.fields.iter()
                    .map(|ty| match ty {
                        ir::Type::Byte => "byte".to_owned(),
                        ir::Type::Char => "char".to_owned(),
                        ir::Type::Int => "int".to_owned(),
                        ir::Type::Long => "long".to_owned(),
                        ir::Type::Double => "double".to_owned(),
                        ir::Type::Unit => "Unit".to_owned(),
                        ir::Type::Boolean => "boolean".to_owned(),
                        ir::Type::String => "String".to_owned(),
                        ir::Type::Void => "void".to_owned(),
                        ir::Type::Object(id) => typed.classes[*id].name.clone(),
                        ir::Type::Borrow { class, mutable } => format!("borrow {}{}", if *mutable { "mut " } else { "" }, typed.classes[*class].name),
                    }).collect::<Vec<_>>().join(", ")));
            }
        }
        for (interface, _) in &class.implementations {
            out.push_str(&format!("  implements {}\n", typed.classes[*interface].name));
        }
    }
    out
}
