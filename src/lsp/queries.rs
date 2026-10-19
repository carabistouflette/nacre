//! Language server queries for document symbols, hover, and definition.
use std::path::Path;
use serde_json::{json, Value};
use crate::ast::Visibility;
use crate::doc::{format_function_signature, format_type};
use crate::project::SourceBundle;
use super::diagnostics::{position_to_offset, range_json, word_at_offset};

use crate::diagnostic::Span;
/// Symbol kinds per Language Server Protocol specification.
mod symbol_kind {
    pub const CLASS: u64 = 5;
    pub const METHOD: u64 = 6;
    pub const PROPERTY: u64 = 7;
    pub const CONSTRUCTOR: u64 = 9;
    pub const ENUM: u64 = 10;
    pub const INTERFACE: u64 = 11;
    pub const FUNCTION: u64 = 12;
    pub const ENUM_MEMBER: u64 = 22;
}

pub(super) fn document_symbols(source: &str) -> Vec<Value> {
    let Ok(tokens) = crate::lexer::lex(source) else { return Vec::new(); };
    let Ok(program) = crate::parser::parse(tokens) else { return Vec::new(); };
    let mut symbols = Vec::new();

    for class in &program.classes {
        if class.is_builtin { continue; }

        let kind = if class.is_interface {
            symbol_kind::INTERFACE
        } else if class.variants.is_some() {
            symbol_kind::ENUM
        } else {
            symbol_kind::CLASS
        };

        let mut children = Vec::new();

        if let Some(variants) = &class.variants {
            for variant in variants {
                children.push(json!({
                    "name": variant.name,
                    "kind": symbol_kind::ENUM_MEMBER,
                    "range": range_json(source, variant.span.start, variant.span.end),
                    "selectionRange": range_json(source, variant.span.start, variant.span.end),
                }));
            }
        }

        for field in &class.fields {
            children.push(json!({
                "name": field.name,
                "kind": symbol_kind::PROPERTY,
                "range": range_json(source, field.span.start, field.span.end),
                "selectionRange": range_json(source, field.span.start, field.span.end),
            }));
        }

        if let Some(constructor) = &class.constructor {
            children.push(json!({
                "name": class.name,
                "kind": symbol_kind::CONSTRUCTOR,
                "range": range_json(source, constructor.span.start, constructor.span.end),
                "selectionRange": range_json(source, constructor.span.start, constructor.span.end),
            }));
        }

        for method in &class.methods {
            children.push(json!({
                "name": method.name,
                "kind": symbol_kind::METHOD,
                "range": range_json(source, method.span.start, method.span.end),
                "selectionRange": range_json(source, method.span.start, method.span.end),
            }));
        }

        let class_range = range_json(source, class.span.start, class.span.end);
        symbols.push(json!({
            "name": class.name,
            "kind": kind,
            "range": class_range,
            "selectionRange": class_range,
            "children": children,
        }));
    }

    for function in &program.functions {
        let func_range = range_json(source, function.span.start, function.span.end);
        symbols.push(json!({
            "name": function.name,
            "kind": symbol_kind::FUNCTION,
            "range": func_range,
            "selectionRange": func_range,
        }));
    }

    for tr in &program.traits {
        let mut children = Vec::new();
        for method in &tr.methods {
            children.push(json!({
                "name": method.name,
                "kind": symbol_kind::METHOD,
                "range": range_json(source, method.span.start, method.span.end),
                "selectionRange": range_json(source, method.span.start, method.span.end),
            }));
        }
        let trait_range = range_json(source, tr.span.start, tr.span.end);
        symbols.push(json!({
            "name": tr.name,
            "kind": symbol_kind::INTERFACE,
            "range": trait_range,
            "selectionRange": trait_range,
            "children": children,
        }));
    }

    symbols
}

pub(super) fn hover(
    bundle: &SourceBundle,
    path: &Path,
    source: &str,
    line: usize,
    character: usize,
) -> Option<Value> {
    let offset = position_to_offset(source, line, character);
    let (word, start, end) = word_at_offset(source, offset)?;
    let Ok(program) = bundle.parse() else { return None; };

    let mut hover_text = None;

    // Check functions
    for function in &program.functions {
        if function.name == word {
            let loc = bundle.locate(function.span);
            if loc.path == path || function.visibility == Visibility::Public {
                hover_text = Some(format_function_signature(function));
                break;
            }
        }
    }

    // Check classes and their members
    if hover_text.is_none() {
        for class in &program.classes {
            if class.name == word {
                let kind = if class.is_interface {
                    "interface"
                } else if class.variants.is_some() {
                    "enum"
                } else {
                    "class"
                };
                let vis = match class.visibility {
                    Visibility::Public => "public ",
                    Visibility::Private => "",
                };
                hover_text = Some(format!("{vis}{kind} {}", class.name));
                break;
            }
            if let Some(variants) = &class.variants {
                for variant in variants {
                    if variant.name == word {
                        let params: Vec<_> = variant.fields.iter().map(format_type).collect();
                        let param_str = if params.is_empty() { String::new() } else { format!("({})", params.join(", ")) };
                        hover_text = Some(format!("enum variant {}.{}{}", class.name, variant.name, param_str));
                        break;
                    }
                }
            }
            if hover_text.is_some() { break; }
            for method in &class.methods {
                if method.name == word {
                    hover_text = Some(format_function_signature(method));
                    break;
                }
            }
            if hover_text.is_some() { break; }
            for field in &class.fields {
                if field.name == word {
                    hover_text = Some(format!("{} {}", format_type(&field.ty), field.name));
                    break;
                }
            }
            if hover_text.is_some() { break; }
        }
    }

    if hover_text.is_none() {
        for tr in &program.traits {
            if tr.name == word {
                hover_text = Some(format!("interface {}", tr.name));
                break;
            }
            for method in &tr.methods {
                if method.name == word {
                    hover_text = Some(format_function_signature(method));
                    break;
                }
            }
            if hover_text.is_some() { break; }
        }
    }
    if has_ambiguous_declaration_name(&program, word)
        && let Some(span) = semantic_call_target_span(bundle, path, offset, program.clone())
        && let Some(function) = function_with_span(&program, span) {
            hover_text = Some(format_function_signature(function));
        }

    let text = hover_text?;
    Some(json!({
        "contents": {
            "kind": "markdown",
            "value": format!("```nacre\n{text}\n```")
        },
        "range": range_json(source, start, end)
    }))
}

pub(super) fn definition(
    bundle: &SourceBundle,
    path: &Path,
    source: &str,
    line: usize,
    character: usize,
    uri_for_path: impl Fn(&Path) -> Option<String>,
) -> Option<Value> {
    let offset = position_to_offset(source, line, character);
    let (word, _, _) = word_at_offset(source, offset)?;
    let Ok(program) = bundle.parse() else { return None; };
    let fallback_span = definition_span(&program, word);

    if has_ambiguous_declaration_name(&program, word)
        && let Some(span) = semantic_call_target_span(bundle, path, offset, program)
        && let Some(definition) = definition_location(bundle, span, &uri_for_path) {
            return Some(definition);
        }

    definition_location(bundle, fallback_span?, &uri_for_path)
}
fn semantic_call_target_span(
    bundle: &SourceBundle,
    path: &Path,
    offset: usize,
    program: crate::ast::Program,
) -> Option<Span> {
    let checked = crate::check_parsed_with_references(program).ok()?;
    let call_site = checked.call_sites.iter().find(|site| {
        let location = bundle.locate(site.name_span);
        location.path == path && location.span.start <= offset && offset <= location.span.end
    })?;
    let span = checked.function_spans_by_id.get(call_site.target_function).copied()?;
    (span.start < span.end).then_some(span)
}

fn function_with_span(
    program: &crate::ast::Program,
    span: Span,
) -> Option<&crate::ast::Function> {
    program.functions.iter()
        .chain(program.classes.iter().flat_map(|class| class.methods.iter().chain(class.constructor.iter())))
        .chain(program.traits.iter().flat_map(|tr| tr.methods.iter()))
        .chain(program.impls.iter().flat_map(|imp| imp.methods.iter()))
        .find(|function| function.span.start == span.start && function.span.end == span.end)
}


fn definition_span(program: &crate::ast::Program, word: &str) -> Option<Span> {
    for function in &program.functions {
        if function.name == word { return Some(function.span); }
    }
    for class in &program.classes {
        if class.name == word { return Some(class.span); }
        if let Some(variants) = &class.variants {
            for variant in variants {
                if variant.name == word { return Some(variant.span); }
            }
        }
        for method in &class.methods {
            if method.name == word { return Some(method.span); }
        }
    }
    for tr in &program.traits {
        if tr.name == word { return Some(tr.span); }
        for method in &tr.methods {
            if method.name == word { return Some(method.span); }
        }
    }
    None
}

// Unique names retain the cheap name-only path; collisions need typed call targets.
fn has_ambiguous_declaration_name(program: &crate::ast::Program, name: &str) -> bool {
    let mut declarations = program.functions.iter().filter(|function| function.name == name).count();
    for class in &program.classes {
        declarations += usize::from(class.name == name);
        declarations += class.variants.as_ref().map_or(0, |variants| {
            variants.iter().filter(|variant| variant.name == name).count()
        });
        declarations += class.methods.iter().filter(|method| method.name == name).count();
    }
    for tr in &program.traits {
        declarations += usize::from(tr.name == name);
        declarations += tr.methods.iter().filter(|method| method.name == name).count();
    }
    for imp in &program.impls {
        declarations += imp.methods.iter().filter(|method| method.name == name).count();
    }
    declarations > 1
}

fn definition_location(
    bundle: &SourceBundle,
    span: Span,
    uri_for_path: &impl Fn(&Path) -> Option<String>,
) -> Option<Value> {
    let location = bundle.locate(span);
    Some(json!({
        "uri": uri_for_path(location.path)?,
        "range": range_json(location.source, location.span.start, location.span.end)
    }))
}
