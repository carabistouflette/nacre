//! The native standard-library templates are part of the trusted C backend.
//! All source calls still pass normal type, ownership and borrowing checks.
mod arrays;
mod encoding;
mod compression;
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
mod date_time;
mod network;
mod http;
mod monotonic;
mod regex;
mod concurrency;
mod process;
mod crypto;
mod big_numbers;

use crate::ast::MapKeyKind;
use crate::ir::{self as t, NativeClass as C, Type};

pub(crate) fn preamble(program: &t::Program) -> String {
    if big_numbers::is_used(program) {
        "/* nacre-link: gmp */\n#include <gmp.h>\n".to_owned()
    } else {
        String::new()
    }
}
pub(crate) fn should_emit_prototype(program: &t::Program, function: &t::Function) -> bool {
    !function.native.is_some_and(big_numbers::is_numeric_method) || big_numbers::is_used(program)
}

pub fn fields(program: &t::Program, class: C, class_id: usize) -> String {
    match class {
        C::List(element) => collections::list_fields(program, element),
        C::Array(element) => arrays::array_fields(program, element),
        C::Deque(element) => deque::fields(program, element),
        C::Map(_, _) => collections::map_fields(class_id),
        C::Set(key) => set::fields(program, key),
        C::Text => text::fields(),
        C::BigInt | C::BigDecimal => big_numbers::fields(class),
        C::Arrays => arrays::utility_fields(),
        C::Encoding => io::fields(),
        C::ByteBuffer => buffer::fields(),
        C::Compression | C::Checksums => io::fields(),
        C::Path | C::File | C::Directory | C::FileInfo | C::Files => filesystem::fields(program, class),
        C::Env => io::fields(),
        C::Command | C::Process | C::ProcessOutput => process::fields(program, class),
        C::Random => random::fields(),
        C::Math | C::Time | C::DateTime | C::Console | C::Args => io::fields(),
        C::Crypto => io::fields(),
        C::Regex | C::RegexMatch => regex::fields(program, class),
        C::SocketAddress | C::TcpListener | C::TcpStream | C::Datagram | C::UdpSocket =>
            network::fields(program, class, class_id),
        C::HttpRequest | C::HttpResponse => http::fields(program, class),
        C::Thread(_) | C::Mutex(_) | C::Guard(_) | C::Channel(_) |
        C::Sender(_) | C::Receiver(_) => concurrency::fields(program, class, class_id),
    }
}

pub fn drop_fields(program: &t::Program, class: C, class_id: usize) -> String {
    match class {
        C::List(element) => collections::drop_list_fields(element),
        C::Array(element) => arrays::drop_array_fields(element),
        C::Deque(element) => deque::drop_fields(element),
        C::Map(key, value) => collections::drop_map_fields(program, key, value),
        C::Set(key) => set::drop_fields(program, key),
        C::Text => text::drop_fields(),
        C::Arrays => arrays::drop_utility_fields(),
        C::BigInt | C::BigDecimal => big_numbers::drop_fields(program, class),
        C::ByteBuffer => buffer::drop_fields(),
        C::Encoding => io::drop_fields(),
        C::Compression | C::Checksums => io::drop_fields(),
        C::Path | C::File | C::Directory | C::FileInfo | C::Files => filesystem::drop_fields(program, class),
        C::Env => io::drop_fields(),
        C::Command | C::Process | C::ProcessOutput => process::drop_fields(program, class),
        C::Random => random::drop_fields(),
        C::Math | C::Time | C::DateTime | C::Console | C::Args => io::drop_fields(),
        C::Crypto => String::new(),
        C::Regex | C::RegexMatch => regex::drop_fields(program, class),
        C::SocketAddress | C::TcpListener | C::TcpStream | C::Datagram | C::UdpSocket =>
            network::drop_fields(program, class),
        C::HttpRequest | C::HttpResponse => http::drop_fields(program, class),
        C::Thread(_) | C::Mutex(_) | C::Guard(_) | C::Channel(_) |
        C::Sender(_) | C::Receiver(_) => concurrency::drop_fields(class_id),
    }
}

pub fn hash_table_definition(class_id: usize) -> String {
    collections::hash_table_definition(class_id)
}

pub(crate) fn drop_helper_prototypes(program: &t::Program) -> String {
    let mut out = concurrency::drop_helper_prototypes(program);
    out.push_str(&http::drop_helper_prototypes(program));
    out
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
    if matches!(native, C::DateTime) && !date_time::is_used(program) {
        return;
    }
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
        C::Crypto => crypto::implementation(program),
        C::Time => time::implementation(program),
        C::DateTime => date_time::implementation(program),
        C::Arrays => arrays::zeroed_implementations(program),
        C::ByteBuffer => buffer::implementation(program, class.id),
        C::Encoding => encoding::implementation(program, class.id),
        C::SocketAddress | C::TcpListener | C::TcpStream | C::Datagram | C::UdpSocket => return,
        C::Path | C::File | C::Directory | C::FileInfo => return,
        C::Thread(_) | C::Mutex(_) | C::Guard(_) | C::Channel(_) |
        C::Sender(_) | C::Receiver(_) => return,
        C::HttpRequest | C::HttpResponse => return,
        C::Compression | C::Checksums | C::Regex | C::RegexMatch | C::Console | C::Files | C::Args |
        C::Env | C::Command | C::Process | C::ProcessOutput => return,
        C::BigInt | C::BigDecimal => return,
    };
    body = body.replace("@ID@", &class.id.to_string());
    let receiver = match native { C::Arrays | C::ByteBuffer | C::Encoding | C::Crypto => None, _ => Some(class.id) };
    body = support::replace_method_ids(body, program, receiver);
    debug_assert!(!support::has_placeholder(&body), "unexpanded standard-library placeholder");
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
    // Native consumers use ByteBuffer's authoritative reserve helpers.
    // Emit that storage core before wrappers, independently of class numbering.
    for class in &program.classes {
        if let Some(native @ C::ByteBuffer) = class.native {
            append_body(program, &mut out, class, native);
        }
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
            if !matches!(native, C::Map(_, _) | C::ByteBuffer) {
                append_body(program, &mut out, class, native);
            }
        }
    }
    out.push_str(&compression::implementation(program));
    out.push_str(&big_numbers::implementation(program));
    out.push_str(&regex::implementation(program));
    out.push_str(&concurrency::implementations(program));
    if let Some(console) = console::implementation(program) {
        out.push_str(&console);
        out.push('\n');
    }
    if let Some(args) = args::implementation(program) {
        out.push_str(&args);
        out.push('\n');
    }
    out.push_str(&network::implementation(program));
    out.push_str(&http::implementation(program));
    if let Some(filesystem) = filesystem::implementations(program) {
        out.push_str(&filesystem);
        out.push('\n');
    }
    if let Some(processes) = process::implementations(program) {
        out.push_str(&processes);
    }
    out
}

