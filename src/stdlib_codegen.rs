//! The native standard-library templates are part of the trusted C backend.
//! All source calls still pass normal type, ownership and borrowing checks.
mod arrays;
mod buffer;
mod collections;
mod filesystem;
mod console;
mod args;
mod io;
mod math;
mod random;
mod ordering;
mod deque;
mod set;
mod support;
mod text;
mod time;
mod network;
mod monotonic;
mod regex;

use crate::ast::MapKeyKind;
use crate::ir::{self as t, NativeClass as C, Type};

pub fn fields(program: &t::Program, class: C, class_id: usize) -> String {
    match class {
        C::List(element) => collections::list_fields(program, element),
        C::Array(element) => arrays::array_fields(program, element),
        C::Deque(element) => deque::fields(program, element),
        C::Map(_, _) => collections::map_fields(class_id),
        C::Set(key) => set::fields(program, key),
        C::Text => text::fields(),
        C::Arrays => arrays::utility_fields(),
        C::ByteBuffer => buffer::fields(),
        C::Path | C::File | C::Directory | C::FileInfo | C::Files => filesystem::fields(program, class),
        C::Random => random::fields(),
        C::Math | C::Time | C::Console | C::Args => io::fields(),
        C::Regex | C::RegexMatch => regex::fields(program, class),
        C::SocketAddress | C::TcpListener | C::TcpStream | C::Datagram | C::UdpSocket =>
            network::fields(program, class, class_id),
    }
}

pub fn drop_fields(program: &t::Program, class: C) -> String {
    match class {
        C::List(element) => collections::drop_list_fields(element),
        C::Array(element) => arrays::drop_array_fields(element),
        C::Deque(element) => deque::drop_fields(element),
        C::Map(key, value) => collections::drop_map_fields(program, key, value),
        C::Set(key) => set::drop_fields(program, key),
        C::Text => text::drop_fields(),
        C::Arrays => arrays::drop_utility_fields(),
        C::ByteBuffer => buffer::drop_fields(),
        C::Path | C::File | C::Directory | C::FileInfo | C::Files => filesystem::drop_fields(program, class),
        C::Random => random::drop_fields(),
        C::Math | C::Time | C::Console | C::Args => io::drop_fields(),
        C::Regex | C::RegexMatch => regex::drop_fields(program, class),
        C::SocketAddress | C::TcpListener | C::TcpStream | C::Datagram | C::UdpSocket =>
            network::drop_fields(program, class),
    }
}

pub fn hash_table_definition(class_id: usize) -> String {
    collections::hash_table_definition(class_id)
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

fn append_body(program: &t::Program, out: &mut String, class: &t::Class, native: C) {
    let mut body = match native {
        C::List(element) => {
            let mut body = collections::list_implementation(program, class.id, element);
            body.push_str(&ordering::list_implementation(program, class.id, element));
            body
        }
        C::Array(element) => arrays::array_implementation(program, class.id, element),
        C::Deque(element) => deque::implementation(program, class.id, element),
        C::Map(key, value) => collections::map_implementation(program, class.id, key, value),
        C::Set(key) => set::implementation(program, class.id, key),
        C::Text => text::implementation(program, class.id),
        C::Math => math::implementation(program),
        C::Random => random::implementation(program),
        C::Time => time::implementation(program),
        C::Arrays => arrays::zeroed_implementations(program),
        C::ByteBuffer => buffer::implementation(program, class.id),
        C::SocketAddress | C::TcpListener | C::TcpStream | C::Datagram | C::UdpSocket => return,
        C::Path | C::File | C::Directory | C::FileInfo => return,
        C::Regex | C::RegexMatch | C::Console | C::Files | C::Args => return,
    };
    body = body.replace("@ID@", &class.id.to_string());
    let receiver = match native { C::Arrays | C::ByteBuffer => None, _ => Some(class.id) };
    body = support::replace_method_ids(body, program, receiver);
    debug_assert!(!body.contains('@'), "unexpanded standard-library placeholder");
    out.push_str(&body);
    out.push('\n');
}

pub fn implementations(program: &t::Program) -> String {
    let mut out = String::new();
    out.push_str(&monotonic::implementation());
    out.push('\n');
    if let Some(text) = support::text_class(program) {
        out.push_str(&support::text_helpers(program, text));
    }
    // Set invokes these specialized table-core functions directly; emit the
    // authoritative Map instantiations before their composing Set wrappers.
    for class in &program.classes {
        if let Some(native @ C::Map(_, _)) = class.native {
            append_body(program, &mut out, class, native);
        }
    }
    for class in &program.classes {
        if let Some(native) = class.native {
            if !matches!(native, C::Map(_, _)) {
                append_body(program, &mut out, class, native);
            }
        }
    }
    out.push_str(&regex::implementation(program));
    if let Some(console) = console::implementation(program) {
        out.push_str(&console);
        out.push('\n');
    }
    if let Some(args) = args::implementation(program) {
        out.push_str(&args);
        out.push('\n');
    }
    out.push_str(&network::implementation(program));
    if let Some(filesystem) = filesystem::implementations(program) {
        out.push_str(&filesystem);
        out.push('\n');
    }
    out
}

