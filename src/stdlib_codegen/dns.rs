use crate::codegen::ctype;
use crate::ir::{self as t, NativeMethod as M, Type};
use super::support::{
    marker, native_function, replace_method_ids, replace_placeholders, result_id, text_class,
    uses_native_methods, variant_id,
};

fn function(program: &t::Program, method: M) -> &t::Function {
    native_function(program, |candidate| candidate == method)
}

fn parameter(function: &t::Function, index: usize) -> String {
    format!("ncr_v_{}", function.params[index].0)
}

fn signature(program: &t::Program, method: M) -> String {
    let function = function(program, method);
    assert_eq!(function.native, Some(method), "DNS signature has its compiler-owned native method");
    let mut parameters = function.params.iter()
        .map(|(id, ty)| format!("{} ncr_v_{id}", ctype(program, *ty)))
        .collect::<Vec<_>>();
    parameters.extend(["size_t ncr_line".to_owned(), "size_t ncr_column".to_owned()]);
    format!("static {} {}({})", ctype(program, function.return_type), marker(method), parameters.join(", "))
}

fn group(source: &str, name: &str) -> String {
    let start = format!("/* DNS_GROUP {name} */");
    let end = format!("/* DNS_GROUP_END {name} */");
    source.split_once(&start).expect("DNS template group opener").1
        .split_once(&end).expect("DNS template group closer").0.to_owned()
}

pub(super) fn fields() -> String {
    "    unsigned char ncr_static_class;\n".to_owned()
}

pub(super) fn drop_fields() -> String { String::new() }

pub(super) fn implementation(program: &t::Program) -> String {
    const IDNA_METHODS: &[M] = &[
        M::DnsIdna, M::DnsResolve, M::TlsStreamConnect,
        M::WebSocketConnect, M::WebSocketConnectTls,
    ];
    const RESOLVE_METHODS: &[M] = &[
        M::DnsResolve, M::WebSocketConnect, M::WebSocketConnectTls,
    ];
    const REVERSE_METHODS: &[M] = &[M::DnsReverse];
    const HOSTNAME_METHODS: &[M] = &[M::DnsLocalHostName];
    let idna_used = uses_native_methods(program, IDNA_METHODS);
    let resolve_used = uses_native_methods(program, RESOLVE_METHODS);
    let reverse_used = uses_native_methods(program, REVERSE_METHODS);
    let hostname_used = uses_native_methods(program, HOSTNAME_METHODS);
    if !idna_used && !reverse_used && !hostname_used { return String::new(); }

    let text = text_class(program).expect("DNS requires native Text");
    let idna = function(program, M::DnsIdna);
    let (text_result, text_ok) = match idna.return_type {
        Type::Object(result) => {
            let layout = program.classes[result].result.expect("Dns.idna returns Result");
            assert_eq!(layout.ok_type, Type::Object(text), "Dns.idna returns Text");
            assert_eq!(layout.err_type, Type::Object(text), "Dns.idna errors own Text");
            (result, layout.ok_variant)
        }
        _ => unreachable!("Dns.idna returns Result<Text, Text>"),
    };
    let text_error = program.classes[text_result].result.expect("DNS Text Result layout").err_variant;
    for method in [M::DnsReverse, M::DnsLocalHostName] {
        let function = function(program, method);
        assert_eq!(result_id(function), text_result, "DNS Text operations share Result<Text, Text>");
    }

    let resolve = function(program, M::DnsResolve);
    let socket_resolve = function(program, M::SocketAddressResolve);
    assert_eq!(resolve.return_type, socket_resolve.return_type,
        "Dns.resolve composes the existing SocketAddress.resolve result");
    let resolve_result = result_id(resolve);
    let resolve_error = program.classes[resolve_result].result.expect("Dns.resolve Result layout").err_variant;
    assert_eq!(resolve_result, result_id(socket_resolve),
        "Dns.resolve and SocketAddress.resolve share their owned List result");

    let template = include_str!("../../stdlib/dns.c.in");
    let mut body = String::new();
    if idna_used {
        body.push_str("/* nacre-link: icu */\n#include <unicode/uidna.h>\n#include <arpa/inet.h>\n");
        body.push_str(&group(template, "idna"));
    }
    if resolve_used { body.push_str(&group(template, "resolve")); }
    if reverse_used { body.push_str(&group(template, "reverse")); }
    if hostname_used { body.push_str(&group(template, "hostname")); }

    let mut substitutions = vec![
        ("@TEXT_ID@", text.to_string()),
        ("@TEXT_RESULT_TYPE@", ctype(program, idna.return_type)),
        ("@TEXT_RESULT_ID@", text_result.to_string()),
        ("@TEXT_RESULT_OK_TAG@", text_ok.to_string()),
        ("@TEXT_RESULT_ERR_TAG@", text_error.to_string()),
        ("@DNS_IDNA_SIGNATURE@", signature(program, M::DnsIdna)),
        ("@DNS_RESOLVE_SIGNATURE@", signature(program, M::DnsResolve)),
        ("@DNS_REVERSE_SIGNATURE@", signature(program, M::DnsReverse)),
        ("@DNS_LOCAL_HOST_NAME_SIGNATURE@", signature(program, M::DnsLocalHostName)),
        ("@DNS_IDNA_INPUT@", parameter(idna, 0)),
        ("@DNS_RESOLVE_INPUT@", parameter(resolve, 0)),
        ("@DNS_RESOLVE_PORT@", parameter(resolve, 1)),
        ("@DNS_REVERSE_ADDRESS@", parameter(function(program, M::DnsReverse), 0)),
        ("@DNS_RESOLVE_RESULT_TYPE@", ctype(program, resolve.return_type)),
        ("@DNS_RESOLVE_RESULT_ID@", resolve_result.to_string()),
        ("@DNS_RESOLVE_ERROR_TAG@", resolve_error.to_string()),
        ("@DNS_IDNA_OK_TAG@", text_ok.to_string()),
        ("@DNS_IDNA_ERROR_TAG@", text_error.to_string()),
    ];

    body = replace_placeholders(body, &substitutions);
    body = replace_method_ids(body, program, None);
    debug_assert!(!body.contains('@'), "unexpanded DNS placeholder");
    body
}
