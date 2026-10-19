//! The native standard-library templates are part of the trusted C backend.
//! All source calls still pass normal type, ownership and borrowing checks.
mod arrays;
mod uuid;
mod encoding;
mod compression;
mod buffer;
mod collections;
mod filesystem;
mod binary;
mod database;
mod console;
mod args;
mod io;
mod math;
mod random;
mod ordering;
mod deque;
mod set;
mod bitset;
mod iterators;
mod ordered;
mod priority;
mod support;
mod native_usage;
mod text;
mod unicode;
mod time;
mod date_time;
mod temporal;
mod network;
mod http;
mod monotonic;
mod regex;
mod concurrency;
mod cryptokeys;
mod zip;
mod concurrency_extra;
mod xml;
mod xml_stream;
mod xslt;
mod process;
mod crypto;
mod big_numbers;
mod uri;
mod dns;
mod tls;
mod websocket;

use crate::ir::{self as t, NativeClass as C, Type};

pub(crate) fn preamble(program: &t::Program) -> String {
    let mut preamble = String::new();
    if big_numbers::is_used(program) {
        preamble.push_str("/* nacre-link: gmp */\n#include <gmp.h>\n");
    }
    preamble.push_str(&regex::preamble(program));
    preamble
}
pub(crate) fn should_emit_prototype(program: &t::Program, function: &t::Function) -> bool {
    (!function.native.is_some_and(big_numbers::is_numeric_method) || big_numbers::is_used(program))
        && (!function.native.is_some_and(uuid::is_uuid_method) || uuid::is_used(program))
        && (!function.native.is_some_and(crypto::is_crypto_method) || crypto::is_used(program))
}

pub fn fields(program: &t::Program, class: C, class_id: usize) -> String {
    match class {
        C::List(element) => collections::list_fields(program, element),
        C::Array(element) => arrays::array_fields(program, element),
        C::Deque(element) => deque::fields(program, element),
        C::Map { .. } => collections::map_fields(class_id),
        C::Set { key, hasher } => set::fields(program, key, hasher),
        C::OrderedMap(_, _) | C::OrderedSet(_) => ordered::fields(program, class, class_id),
        C::PriorityQueue(_) => priority::fields(program, class, class_id),
        C::BitSet => bitset::fields(program, class, class_id),
        C::ListIterator(_) | C::MapIterator(_, _) | C::FilterIterator(_) |
        C::TakeIterator(_) | C::SkipIterator(_) | C::Iterators(_) =>
            iterators::fields(program, class, class_id),
        C::Text => text::fields(),
        C::BigInt | C::BigDecimal => big_numbers::fields(class),
        C::Arrays => arrays::utility_fields(),
        C::Encoding => io::fields(),
        C::Unicode | C::Character | C::Charset => arrays::utility_fields(),
        C::Locale => arrays::utility_fields(),
        C::Uuid => uuid::fields(),
        C::ByteBuffer => buffer::fields(),
        C::Compression | C::Checksums => io::fields(),
        C::Path | C::File | C::Directory | C::FileInfo | C::Watcher | C::WatchEvent | C::Files =>
            filesystem::fields(program, class),
        C::BinaryReader | C::BinaryWriter => binary::fields(program, class, class_id),
        C::Database => database::database_fields(),
        C::SqlStatement => database::statement_fields(),
        C::ZipReader | C::ZipWriter | C::ZipEntry => zip::fields(program, class),
        C::Env => io::fields(),
        C::Command | C::Process | C::ProcessOutput => process::fields(program, class),
        C::Random => random::fields(),
        C::Temporal => temporal::fields(),
        C::Math | C::Time | C::DateTime | C::Console | C::Args => io::fields(),
        C::Crypto => io::fields(),
        C::XmlReader | C::XmlEvent | C::XmlWriter => xml_stream::fields(class),
        C::XsltStylesheet => xslt::fields(),
        C::Regex | C::RegexMatch => regex::fields(program, class),
        C::XmlDocument => xml::fields(),
        C::SocketAddress | C::TcpListener | C::TcpStream | C::Datagram | C::UdpSocket =>
            network::fields(program, class, class_id),
        C::HttpRequest | C::HttpResponse => http::fields(program, class),
        C::CryptoKey | C::CryptoDigest | C::CryptoMac | C::Certificate =>
            cryptokeys::fields(class),
        C::Uri => uri::fields(),
        C::Dns => dns::fields(),
        C::TlsConfig | C::TlsStream => tls::fields(program, class, class_id),
        C::WebSocket | C::WebSocketMessage => websocket::fields(program, class, class_id),
        C::Thread(_) | C::Mutex(_) | C::Guard(_) | C::Channel(_) |
        C::Sender(_) | C::Receiver(_) | C::Condition(_) | C::Future(_) | C::ThreadPool(_) |
        C::AtomicInt | C::AtomicLong | C::AtomicBoolean | C::Semaphore | C::CountDownLatch |
        C::Barrier | C::CancellationToken => concurrency::fields(program, class, class_id),
    }
}

pub fn drop_fields(program: &t::Program, class: C, class_id: usize) -> String {
    match class {
        C::List(element) => collections::drop_list_fields(element),
        C::Array(element) => arrays::drop_array_fields(element),
        C::Deque(element) => deque::drop_fields(element),
        C::Map { key, value, hasher } => collections::drop_map_fields(key, value, hasher),
        C::Set { key, hasher } => set::drop_fields(key, hasher),
        C::OrderedMap(_, _) | C::OrderedSet(_) => ordered::drop_fields(class),
        C::PriorityQueue(_) => priority::drop_fields(class),
        C::BitSet => bitset::drop_fields(program, class, class_id),
        C::ListIterator(_) | C::MapIterator(_, _) | C::FilterIterator(_) |
        C::TakeIterator(_) | C::SkipIterator(_) | C::Iterators(_) =>
            iterators::drop_fields(class),
        C::Text => text::drop_fields(),
        C::Arrays => arrays::drop_utility_fields(),
        C::BigInt | C::BigDecimal => big_numbers::drop_fields(program, class),
        C::Unicode | C::Character | C::Charset => arrays::drop_utility_fields(),
        C::Locale => arrays::drop_utility_fields(),
        C::ByteBuffer => buffer::drop_fields(),
        C::Uuid => uuid::drop_fields(),
        C::Encoding => io::drop_fields(),
        C::Compression | C::Checksums => io::drop_fields(),
        C::Path | C::File | C::Directory | C::FileInfo | C::Watcher | C::WatchEvent | C::Files =>
            filesystem::drop_fields(program, class),
        C::BinaryReader | C::BinaryWriter => binary::drop_fields(program, class, class_id),
        C::Database | C::SqlStatement => database::drop_fields(),
        C::ZipReader | C::ZipWriter | C::ZipEntry => zip::drop_fields(program, class),
        C::Env => io::drop_fields(),
        C::Command | C::Process | C::ProcessOutput => process::drop_fields(program, class),
        C::Random => random::drop_fields(),
        C::Temporal => temporal::drop_fields(),
        C::Math | C::Time | C::DateTime | C::Console | C::Args => io::drop_fields(),
        C::Crypto => String::new(),
        C::XmlReader | C::XmlEvent | C::XmlWriter => xml_stream::drop_fields(class_id, class),
        C::XsltStylesheet => xslt::drop_fields(class_id),
        C::Regex | C::RegexMatch => regex::drop_fields(program, class),
        C::XmlDocument => xml::drop_fields(class_id),
        C::SocketAddress | C::TcpListener | C::TcpStream | C::Datagram | C::UdpSocket =>
            network::drop_fields(program, class),
        C::HttpRequest | C::HttpResponse => http::drop_fields(program, class),
        C::CryptoKey | C::CryptoDigest | C::CryptoMac | C::Certificate =>
            cryptokeys::drop_fields(class, class_id),
        C::Uri => uri::drop_fields(),
        C::Dns => dns::drop_fields(),
        C::TlsConfig | C::TlsStream => tls::drop_fields(program, class),
        C::WebSocket | C::WebSocketMessage => websocket::drop_fields(program, class),
        C::Thread(_) | C::Mutex(_) | C::Guard(_) | C::Channel(_) |
        C::Sender(_) | C::Receiver(_) | C::Condition(_) | C::Future(_) | C::ThreadPool(_) |
        C::AtomicInt | C::AtomicLong | C::AtomicBoolean | C::Semaphore | C::CountDownLatch |
        C::Barrier | C::CancellationToken => concurrency::drop_fields(class_id),
    }
}

pub fn hash_table_definition(program: &t::Program, class_id: usize, hasher: Option<Type>) -> String {
    collections::hash_table_definition(program, class_id, hasher)
}

pub(crate) fn drop_helper_prototypes(program: &t::Program) -> String {
    let mut out = concurrency::drop_helper_prototypes(program);
    out.push_str(&http::drop_helper_prototypes(program));
    out.push_str(&cryptokeys::drop_helper_prototypes(program));
    out.push_str(&tls::drop_helper_prototypes(program));
    out.push_str(&xml::drop_helper_prototypes(program));
    out.push_str(&xml_stream::drop_helper_prototypes(program));
    out.push_str(&xslt::drop_helper_prototypes(program));
    out
}

pub fn map_entry_definition(program: &t::Program, class_id: usize, key: Type, value: Type) -> String {
    collections::map_entry_definition(program, class_id, key, value)
}
pub fn ordered_storage_definition(program: &t::Program, class_id: usize) -> String {
    ordered::storage_definition(program, class_id)
}

pub fn ordered_entry_definition(program: &t::Program, class_id: usize, key: Type, value: Type) -> String {
    ordered::entry_definition(program, class_id, key, value)
}

pub(crate) fn map_capacity_expression(map: &str) -> String {
    collections::map_capacity_expression(map)
}

pub(crate) fn map_slot_expression(
    program: &t::Program, map_class: usize, map: &str, index: &str, kind: t::MapSlotKind, value_type: Type,
) -> String {
    collections::map_slot_expression(program, map_class, map, index, kind, value_type)
}

fn append_body(
    program: &t::Program, out: &mut String, class: &t::Class, native: C,
    native_method_ids: &mut Option<t::NativeMethodIndex>,
) {
    if matches!(native, C::DateTime) && !date_time::is_used(program) {
        return;
    }
    if matches!(native, C::Temporal) && !temporal::is_used(program) {
        return;
    }
    if matches!(native, C::Uuid) && !uuid::is_used(program) {
        return;
    }
    if matches!(native, C::Crypto) && !crypto::is_used(program) {
        return;
    }
    let mut body = match native {
        C::List(element) => {
            let mut body = collections::list_implementation(program, class.id, element);
            body.push_str(&ordering::list_implementation(program, class.id, element));
            let method_ids = native_method_ids.get_or_insert_with(|| t::NativeMethodIndex::new(program));
            body.push_str(&iterators::list_implementation(program, method_ids, class.id, element));
            body
        }
        C::Array(element) => {
            let method_ids = native_method_ids.get_or_insert_with(|| t::NativeMethodIndex::new(program));
            arrays::array_implementation(program, method_ids, class.id, element)
        }
        C::Deque(element) => deque::implementation(program, class.id, element),
        C::Map { key, value, hasher } => collections::map_implementation(program, class.id, key, value, hasher),
        C::Set { key, hasher } => set::implementation(program, class.id, key, hasher),
        C::OrderedMap(key, value) => ordered::map_implementation(program, class.id, key, value, native_method_ids),
        C::OrderedSet(key) => ordered::set_implementation(program, class.id, key, native_method_ids),
        C::PriorityQueue(element) => priority::implementation(program, class.id, element, native_method_ids),
        C::BitSet => bitset::implementation(program, class.id, native_method_ids),
        C::ListIterator(_) | C::MapIterator(_, _) | C::FilterIterator(_) |
        C::TakeIterator(_) | C::SkipIterator(_) | C::Iterators(_) => {
            let method_ids = native_method_ids.get_or_insert_with(|| t::NativeMethodIndex::new(program));
            iterators::implementation(program, method_ids, native, class.id)
        }
        C::Text => text::implementation(program, class.id, native_method_ids),
        C::Math => math::implementation(program, native_method_ids),
        C::Random => random::implementation(program, native_method_ids),
        C::Encoding => encoding::implementation(program, class.id, native_method_ids),
        C::Crypto => crypto::implementation(program, native_method_ids),
        C::Time => time::implementation(program, native_method_ids),
        C::DateTime => date_time::implementation(program, native_method_ids),
        C::Temporal => temporal::implementation(program, native_method_ids),
        C::Arrays => {
            let method_ids = native_method_ids.get_or_insert_with(|| t::NativeMethodIndex::new(program));
            arrays::zeroed_implementations(program, method_ids)
        }
        C::ByteBuffer => {
            let method_ids = native_method_ids.get_or_insert_with(|| t::NativeMethodIndex::new(program));
            buffer::implementation(program, method_ids, class.id)
        }
        C::Uuid => uuid::implementation(program, class.id, native_method_ids),
        C::BinaryReader | C::BinaryWriter =>
            binary::implementation(program, native_method_ids, class.id, native),
        C::Database => database::implementation(program, native_method_ids, class.id),
        C::SqlStatement => return,
        C::SocketAddress | C::TcpListener | C::TcpStream | C::Datagram | C::UdpSocket |
        C::Uri | C::Dns => return,
        C::Path | C::File | C::Directory | C::FileInfo | C::Watcher | C::WatchEvent => return,
        C::ZipReader | C::ZipWriter | C::ZipEntry => return,
        C::XmlDocument | C::XmlReader | C::XmlEvent | C::XmlWriter | C::XsltStylesheet => return,
        C::Thread(_) | C::Mutex(_) | C::Guard(_) | C::Channel(_) |
        C::Sender(_) | C::Receiver(_) | C::Condition(_) | C::Future(_) | C::ThreadPool(_) |
        C::AtomicInt | C::AtomicLong | C::AtomicBoolean | C::Semaphore | C::CountDownLatch |
        C::Barrier | C::CancellationToken => return,
        C::HttpRequest | C::HttpResponse | C::Unicode | C::Character | C::Charset | C::Locale |
        C::CryptoKey | C::CryptoDigest | C::CryptoMac | C::Certificate |
        C::TlsConfig | C::TlsStream | C::WebSocket | C::WebSocketMessage => return,
        C::Compression | C::Checksums | C::Regex | C::RegexMatch | C::Console | C::Files | C::Args |
        C::Env | C::Command | C::Process | C::ProcessOutput => return,
        C::BigInt | C::BigDecimal => return,
    };
    body = support::replace_placeholders(body, &[("@ID@", class.id.to_string())]);
    let receiver = match native {
        C::Arrays | C::ByteBuffer | C::Encoding | C::Crypto | C::Uuid | C::Database => None,
        _ => Some(class.id),
    };
    body = support::replace_method_ids(body, program, receiver, native_method_ids);
    debug_assert!(!support::has_placeholder(&body), "unexpanded standard-library placeholder");
    out.push_str(&body);
    out.push('\n');
}

pub fn implementations(program: &t::Program) -> String {
    let mut native_method_ids = None;
    let mut out = String::new();
    out.push_str(&monotonic::implementation());
    out.push('\n');
    if let Some(text) = support::text_class(program) {
        out.push_str(&support::text_helpers(program, text));
    }
    if date_time::is_used(program) || temporal::is_used(program) {
        out.push_str(&date_time::common_helpers(program));
        out.push('\n');
    }
    out.push_str(&crypto::digest_selection_helper(program));
    // Native consumers use ByteBuffer's authoritative reserve helpers.
    // Emit that storage core before wrappers, independently of class numbering.
    for class in &program.classes {
        if let Some(native @ C::ByteBuffer) = class.native {
            append_body(program, &mut out, class, native, &mut native_method_ids);
        }
    }
    // Set invokes these specialized table-core functions directly; emit the
    // authoritative Map instantiations before their composing Set wrappers.
    for class in &program.classes {
        if let Some(native @ C::Map { .. }) = class.native {
            append_body(program, &mut out, class, native, &mut native_method_ids);
        }
    }
    // OrderedSet reuses the native core of its hidden OrderedMap<K, Unit> specialization.
    for class in &program.classes {
        if let Some(native @ C::OrderedMap(_, _)) = class.native {
            append_body(program, &mut out, class, native, &mut native_method_ids);
        }
    }
    for class in &program.classes {
        if let Some(native) = class.native
            && !matches!(native, C::Map { .. } | C::OrderedMap(_, _) | C::ByteBuffer) {
                append_body(program, &mut out, class, native, &mut native_method_ids);
            }
    }
    out.push_str(&compression::implementation(program, &mut native_method_ids));
    out.push_str(&big_numbers::implementation(program, &mut native_method_ids));
    let icu_body = unicode::implementation(program, &mut native_method_ids);
    let icu = support::replace_method_ids(icu_body, program, None, &mut native_method_ids);
    debug_assert!(!support::has_placeholder(&icu), "unexpanded ICU template placeholder");
    out.push_str(&icu);
    out.push_str(&regex::implementation(program, &mut native_method_ids));
    out.push_str(&concurrency::implementations(program, &mut native_method_ids));
    if let Some(console) = console::implementation(program, &mut native_method_ids) {
        out.push_str(&console);
        out.push('\n');
    }
    if let Some(args) = args::implementation(program, &mut native_method_ids) {
        out.push_str(&args);
        out.push('\n');
    }
    let transport_needs_connect = program.functions.iter().any(|function| matches!(function.native,
        Some(t::NativeMethod::TlsStreamConnect | t::NativeMethod::WebSocketConnect |
            t::NativeMethod::WebSocketConnectTls)));
    let transport_needs_wait = transport_needs_connect || program.classes.iter().any(|class|
        matches!(class.native, Some(C::TlsStream | C::WebSocket)));
    let resolver_needed = transport_needs_connect || program.functions.iter().any(|function|
        function.native == Some(t::NativeMethod::DnsResolve));
    out.push_str(&network::implementation(program, &mut native_method_ids, transport_needs_wait, transport_needs_connect, resolver_needed));
    out.push_str(&http::implementation(program, &mut native_method_ids));
    out.push_str(&uri::percent_helpers(program, transport_needs_connect));
    out.push_str(&uri::implementation(program, &mut native_method_ids));
    out.push_str(&dns::implementation(program, &mut native_method_ids, transport_needs_connect, resolver_needed));
    out.push_str(&zip::entry_implementation(program, &mut native_method_ids));
    if let Some(zip) = zip::implementation(program, &mut native_method_ids) {
        out.push_str(&zip);
    }
    if let Some(filesystem) = filesystem::implementations(program, &mut native_method_ids) {
        out.push_str(&filesystem);
        out.push('\n');
    }
    if let Some(processes) = process::implementations(program, &mut native_method_ids) {
        out.push_str(&processes);
    }
    out.push_str(&cryptokeys::implementation(program, &mut native_method_ids));
    out.push_str(&tls::implementation(program, &mut native_method_ids));
    out.push_str(&websocket::implementation(program, &mut native_method_ids));
    out.push_str(&xml::shared_implementation(program));
    if let Some(xml) = xml::implementation(program, &mut native_method_ids) {
        out.push_str(&xml);
        out.push('\n');
    }
    out.push_str(&xml_stream::implementation(program, &mut native_method_ids));
    out.push_str(&xslt::implementation(program, &mut native_method_ids));
    out
}

