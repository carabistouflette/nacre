//! Language server queries for document symbols, hover, and definition.
use std::path::Path;
use serde_json::{json, Value};
use crate::ast::Visibility;
use crate::doc::{format_function_signature, format_type};
use crate::project::SourceBundle;
use super::diagnostics::{position_to_offset, range_json, word_at_offset};

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

pub(super) fn document_symbols(bundle: &SourceBundle, path: &Path, source: &str) -> Vec<Value> {
    let Ok(program) = bundle.parse() else { return Vec::new(); };
    let mut symbols = Vec::new();

    for class in &program.classes {
        if class.is_builtin { continue; }
        let loc = bundle.locate(class.span);
        if loc.path != path { continue; }

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
                let v_loc = bundle.locate(variant.span);
                children.push(json!({
                    "name": variant.name,
                    "kind": symbol_kind::ENUM_MEMBER,
                    "range": range_json(source, v_loc.span.start, v_loc.span.end),
                    "selectionRange": range_json(source, v_loc.span.start, v_loc.span.end),
                }));
            }
        }

        for field in &class.fields {
            let f_loc = bundle.locate(field.span);
            children.push(json!({
                "name": field.name,
                "kind": symbol_kind::PROPERTY,
                "range": range_json(source, f_loc.span.start, f_loc.span.end),
                "selectionRange": range_json(source, f_loc.span.start, f_loc.span.end),
            }));
        }

        if let Some(constructor) = &class.constructor {
            let c_loc = bundle.locate(constructor.span);
            children.push(json!({
                "name": class.name,
                "kind": symbol_kind::CONSTRUCTOR,
                "range": range_json(source, c_loc.span.start, c_loc.span.end),
                "selectionRange": range_json(source, c_loc.span.start, c_loc.span.end),
            }));
        }

        for method in &class.methods {
            let m_loc = bundle.locate(method.span);
            children.push(json!({
                "name": method.name,
                "kind": symbol_kind::METHOD,
                "range": range_json(source, m_loc.span.start, m_loc.span.end),
                "selectionRange": range_json(source, m_loc.span.start, m_loc.span.end),
            }));
        }

        let class_range = range_json(source, loc.span.start, loc.span.end);
        symbols.push(json!({
            "name": class.name,
            "kind": kind,
            "range": class_range,
            "selectionRange": class_range,
            "children": children,
        }));
    }

    for function in &program.functions {
        let loc = bundle.locate(function.span);
        if loc.path != path { continue; }
        let func_range = range_json(source, loc.span.start, loc.span.end);
        symbols.push(json!({
            "name": function.name,
            "kind": symbol_kind::FUNCTION,
            "range": func_range,
            "selectionRange": func_range,
        }));
    }

    for tr in &program.traits {
        let loc = bundle.locate(tr.span);
        if loc.path != path { continue; }
        let mut children = Vec::new();
        for method in &tr.methods {
            let m_loc = bundle.locate(method.span);
            children.push(json!({
                "name": method.name,
                "kind": symbol_kind::METHOD,
                "range": range_json(source, m_loc.span.start, m_loc.span.end),
                "selectionRange": range_json(source, m_loc.span.start, m_loc.span.end),
            }));
        }
        let trait_range = range_json(source, loc.span.start, loc.span.end);
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
    _path: &Path,
    source: &str,
    line: usize,
    character: usize,
    uri_for_path: impl Fn(&Path) -> Option<String>,
) -> Option<Value> {
    let offset = position_to_offset(source, line, character);
    let (word, _, _) = word_at_offset(source, offset)?;
    let Ok(program) = bundle.parse() else { return None; };

    for function in &program.functions {
        if function.name == word {
            let loc = bundle.locate(function.span);
            let target_uri = uri_for_path(loc.path)?;
            return Some(json!({
                "uri": target_uri,
                "range": range_json(loc.source, loc.span.start, loc.span.end)
            }));
        }
    }

    for class in &program.classes {
        if class.name == word {
            let loc = bundle.locate(class.span);
            let target_uri = uri_for_path(loc.path)?;
            return Some(json!({
                "uri": target_uri,
                "range": range_json(loc.source, loc.span.start, loc.span.end)
            }));
        }
        if let Some(variants) = &class.variants {
            for variant in variants {
                if variant.name == word {
                    let loc = bundle.locate(variant.span);
                    let target_uri = uri_for_path(loc.path)?;
                    return Some(json!({
                        "uri": target_uri,
                        "range": range_json(loc.source, loc.span.start, loc.span.end)
                    }));
                }
            }
        }
        for method in &class.methods {
            if method.name == word {
                let loc = bundle.locate(method.span);
                let target_uri = uri_for_path(loc.path)?;
                return Some(json!({
                    "uri": target_uri,
                    "range": range_json(loc.source, loc.span.start, loc.span.end)
                }));
            }
        }
    }

    for tr in &program.traits {
        if tr.name == word {
            let loc = bundle.locate(tr.span);
            let target_uri = uri_for_path(loc.path)?;
            return Some(json!({
                "uri": target_uri,
                "range": range_json(loc.source, loc.span.start, loc.span.end)
            }));
        }
        for method in &tr.methods {
            if method.name == word {
                let loc = bundle.locate(method.span);
                let target_uri = uri_for_path(loc.path)?;
                return Some(json!({
                    "uri": target_uri,
                    "range": range_json(loc.source, loc.span.start, loc.span.end)
                }));
            }
        }
    }

    None
}
