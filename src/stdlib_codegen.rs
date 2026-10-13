//! The native standard-library templates are part of the trusted C backend.
//! All source calls still pass normal type, ownership and borrowing checks.
mod collections;
mod io;
mod math;
mod random;
mod support;
mod text;
mod time;

use crate::ast::MapKeyKind;
use crate::ir::{self as t, NativeClass as C, Type};

pub fn fields(program: &t::Program, class: C, class_id: usize) -> String {
    match class {
        C::List(element) => collections::list_fields(program, element),
        C::Map(_, _) => collections::map_fields(class_id),
        C::Text => text::fields(),
        C::Math | C::Time | C::Console | C::Files | C::Args => io::fields(),
        C::Random => random::fields(),
    }
}

pub fn drop_fields(program: &t::Program, class: C) -> String {
    match class {
        C::List(element) => collections::drop_list_fields(element),
        C::Map(key, value) => collections::drop_map_fields(program, key, value),
        C::Text => text::drop_fields(),
        C::Math | C::Time | C::Console | C::Files | C::Args => io::drop_fields(),
        C::Random => random::drop_fields(),
    }
}

pub fn map_entry_definition(program: &t::Program, class_id: usize, key: MapKeyKind, value: Type) -> String {
    collections::map_entry_definition(program, class_id, key, value)
}

pub(crate) fn map_capacity_expression(map: &str) -> String {
    collections::map_capacity_expression(map)
}

pub(crate) fn map_slot_expression(
    program: &t::Program, map: &str, index: &str, kind: t::MapSlotKind, value_type: Type,
) -> String {
    collections::map_slot_expression(program, map, index, kind, value_type)
}

pub fn implementations(program: &t::Program) -> String {
    let mut out = String::new();
    if let Some(text) = support::text_class(program) {
        out.push_str(&support::text_helpers(program, text));
    }
    for class in &program.classes {
        let Some(native) = class.native else { continue; };
        let mut body = match native {
            C::List(element) => collections::list_implementation(program, class.id, element),
            C::Map(key, value) => collections::map_implementation(program, class.id, key, value),
            C::Text => text::implementation(program, class.id),
            C::Math => math::implementation(program),
            C::Random => random::implementation(program),
            C::Time => time::implementation(program),
            C::Console | C::Files | C::Args => continue,
        };
        body = body.replace("@ID@", &class.id.to_string());
        body = support::replace_method_ids(body, program, Some(class.id));
        debug_assert!(!body.contains('@'), "unexpanded standard-library placeholder");
        out.push_str(&body);
        out.push('\n');
    }
    if let Some(io) = io::implementations(program) {
        out.push_str(&io);
    }
    out
}

