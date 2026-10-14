use crate::c_repr::ctype;
use crate::ir::{self as t, NativeMethod as M, Type};
use super::support::{
    marker, native_function, replace_method_ids, replace_placeholders, result_id, text_class,
    native_function_if, has_placeholder, template_section,
};

pub(super) const IDNA_HELPER_NAME: &str = "ncr_dns_idna_host";
pub(super) const RESOLVER_HELPER_NAME: &str = "ncr_dns_resolve_host";

fn function(program: &t::Program, method: M) -> &t::Function {
    native_function(program, |candidate| candidate == method)
}

fn parameter(function: &t::Function, index: usize) -> String {
    function.params[index].0.to_string()
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

const TEMPLATE: &str = include_str!("../../stdlib/dns.c.in");

fn text_result_layout(program: &t::Program, function: &t::Function, text: usize) -> (usize, usize, usize) {
    let result = result_id(function);
    let layout = program.classes[result].result.expect("DNS text operation returns Result");
    assert_eq!(layout.ok_type, Type::Object(text), "DNS text operation returns Text");
    assert_eq!(layout.err_type, Type::Object(text), "DNS text operation errors own Text");
    (result, layout.ok_variant, layout.err_variant)
}

pub(super) fn fields() -> String {
    "    unsigned char ncr_static_class;\n".to_owned()
}

pub(super) fn drop_fields() -> String { String::new() }

pub(super) fn implementation(
    program: &t::Program,
    transport_needs_idna: bool,
    transport_needs_resolver: bool,
) -> String {
    let public_idna = native_function_if(program, |method| method == M::DnsIdna).is_some();
    let public_resolve = native_function_if(program, |method| method == M::DnsResolve).is_some();
    let reverse_used = native_function_if(program, |method| method == M::DnsReverse).is_some();
    let hostname_used = native_function_if(program, |method| method == M::DnsLocalHostName).is_some();
    let resolver_used = public_resolve || transport_needs_resolver;
    let idna_core_used = public_idna || public_resolve || transport_needs_idna || transport_needs_resolver;
    if !idna_core_used && !reverse_used && !hostname_used { return String::new(); }

    let text = text_class(program).expect("DNS helpers require native Text");
    let mut body = String::new();
    if idna_core_used {
        body.push_str("/* nacre-link: icu */\n#include <unicode/uidna.h>\n#include <arpa/inet.h>\n");
        body.push_str(template_section(TEMPLATE,
            "/* DNS_GROUP idna_core */", "/* DNS_GROUP_END idna_core */"));
    }

    let idna_function = if public_idna { Some(function(program, M::DnsIdna)) } else { None };
    let resolve_function = if public_resolve { Some(function(program, M::DnsResolve)) } else { None };
    let reverse_function = if reverse_used { Some(function(program, M::DnsReverse)) } else { None };
    let hostname_function = if hostname_used { Some(function(program, M::DnsLocalHostName)) } else { None };

    if public_idna {
        body.push_str(template_section(TEMPLATE,
            "/* DNS_GROUP idna_public */", "/* DNS_GROUP_END idna_public */"));
    }
    let socket_resolve = if resolver_used { Some(function(program, M::SocketAddressResolve)) } else { None };
    if resolver_used {
        body.push_str(template_section(TEMPLATE,
            "/* DNS_GROUP resolve_core */", "/* DNS_GROUP_END resolve_core */"));
    }
    if public_resolve {
        body.push_str(template_section(TEMPLATE,
            "/* DNS_GROUP resolve_public */", "/* DNS_GROUP_END resolve_public */"));
    }
    if reverse_used {
        body.push_str(template_section(TEMPLATE,
            "/* DNS_GROUP reverse */", "/* DNS_GROUP_END reverse */"));
    }
    if hostname_used {
        body.push_str(template_section(TEMPLATE,
            "/* DNS_GROUP hostname */", "/* DNS_GROUP_END hostname */"));
    }

    let mut substitutions = vec![
        ("@TEXT_ID@", text.to_string()),
        ("@IDNA_CORE_SIGNATURE@", format!(
            "static bool {IDNA_HELPER_NAME}(ncr_obj_{text} *ncr_input, ncr_obj_{text} **ncr_ascii, ncr_obj_{text} **ncr_error, size_t ncr_line, size_t ncr_column)")),
    ];
    if let Some(idna) = idna_function {
        let (result, ok, err) = text_result_layout(program, idna, text);
        substitutions.extend([
            ("@TEXT_RESULT_TYPE@", ctype(program, idna.return_type)),
            ("@TEXT_RESULT_ID@", result.to_string()),
            ("@TEXT_RESULT_OK_TAG@", ok.to_string()),
            ("@TEXT_RESULT_ERR_TAG@", err.to_string()),
            ("@DNS_IDNA_SIGNATURE@", signature(program, M::DnsIdna)),
            ("@DNS_IDNA_INPUT@", parameter(idna, 0)),
        ]);
    }
    if let Some(reverse) = reverse_function {
        let (result, ok, err) = text_result_layout(program, reverse, text);
        if let Some(idna) = idna_function {
            assert_eq!(result, result_id(idna), "DNS text results share Result<Text, Text>");
        }
        if let Some(hostname) = hostname_function {
            assert_eq!(result, result_id(hostname), "DNS text results share Result<Text, Text>");
        }
        substitutions.extend([
            ("@TEXT_RESULT_TYPE@", ctype(program, reverse.return_type)),
            ("@TEXT_RESULT_ID@", result.to_string()),
            ("@TEXT_RESULT_OK_TAG@", ok.to_string()),
            ("@TEXT_RESULT_ERR_TAG@", err.to_string()),
            ("@DNS_REVERSE_SIGNATURE@", signature(program, M::DnsReverse)),
            ("@DNS_REVERSE_ADDRESS@", parameter(reverse, 0)),
        ]);
    }
    if let Some(hostname) = hostname_function {
        let (result, ok, err) = text_result_layout(program, hostname, text);
        if let Some(idna) = idna_function {
            assert_eq!(result, result_id(idna), "DNS text results share Result<Text, Text>");
        }
        substitutions.extend([
            ("@TEXT_RESULT_TYPE@", ctype(program, hostname.return_type)),
            ("@TEXT_RESULT_ID@", result.to_string()),
            ("@TEXT_RESULT_OK_TAG@", ok.to_string()),
            ("@TEXT_RESULT_ERR_TAG@", err.to_string()),
            ("@DNS_LOCAL_HOST_NAME_SIGNATURE@", signature(program, M::DnsLocalHostName)),
        ]);
    }
    if let Some(resolve) = resolve_function {
        let socket_resolve = socket_resolve.expect("Dns.resolve requires SocketAddress.resolve");
        assert_eq!(resolve.return_type, socket_resolve.return_type,
            "Dns.resolve composes the existing SocketAddress.resolve result");
        assert_eq!(resolve.params.len(), 2, "Dns.resolve accepts host and port");
        assert_eq!(socket_resolve.params.len(), 3,
            "SocketAddress.resolve accepts host, port, and passive mode");
        assert_eq!(resolve.params[0].1, socket_resolve.params[0].1,
            "Dns.resolve uses the existing SocketAddress.resolve host type");
        assert_eq!(resolve.params[1].1, socket_resolve.params[1].1,
            "Dns.resolve uses the existing SocketAddress.resolve port type");
        substitutions.extend([
            ("@DNS_RESOLVE_SIGNATURE@", signature(program, M::DnsResolve)),
            ("@DNS_RESOLVE_INPUT@", parameter(resolve, 0)),
            ("@DNS_RESOLVE_PORT@", parameter(resolve, 1)),
            ("@DNS_RESOLVE_CORE_SIGNATURE@", format!(
                "static {} {RESOLVER_HELPER_NAME}({} ncr_input, {} ncr_port, size_t ncr_line, size_t ncr_column)",
                ctype(program, socket_resolve.return_type),
                ctype(program, socket_resolve.params[0].1),
                ctype(program, socket_resolve.params[1].1))),
            ("@DNS_RESOLVE_RESULT_TYPE@", ctype(program, socket_resolve.return_type)),
            ("@DNS_RESOLVE_RESULT_ID@", result_id(socket_resolve).to_string()),
            ("@DNS_RESOLVE_ERROR_TAG@", program.classes[result_id(socket_resolve)].result
                .expect("SocketAddress.resolve returns Result").err_variant.to_string()),
            ("@SOCKET_ADDRESS_RESOLVE@", format!("ncr_fn_{}", socket_resolve.id)),
        ]);
    }
    if let Some(socket_resolve) = socket_resolve {
        if transport_needs_resolver && resolve_function.is_none() {
            assert_eq!(socket_resolve.params.len(), 3,
                "SocketAddress.resolve accepts host, port, and passive mode");
            substitutions.extend([
                ("@DNS_RESOLVE_CORE_SIGNATURE@", format!(
                    "static {} {RESOLVER_HELPER_NAME}({} ncr_input, {} ncr_port, size_t ncr_line, size_t ncr_column)",
                    ctype(program, socket_resolve.return_type),
                    ctype(program, socket_resolve.params[0].1),
                    ctype(program, socket_resolve.params[1].1))),
                ("@DNS_RESOLVE_RESULT_TYPE@", ctype(program, socket_resolve.return_type)),
                ("@DNS_RESOLVE_RESULT_ID@", result_id(socket_resolve).to_string()),
                ("@DNS_RESOLVE_ERROR_TAG@", program.classes[result_id(socket_resolve)].result
                    .expect("SocketAddress.resolve returns Result").err_variant.to_string()),
                ("@SOCKET_ADDRESS_RESOLVE@", format!("ncr_fn_{}", socket_resolve.id)),
            ]);
        }
    }
    if let Some(reverse) = reverse_function {
        if !substitutions.iter().any(|(key, _)| *key == "@TEXT_RESULT_TYPE@") {
            let (result, ok, err) = text_result_layout(program, reverse, text);
            substitutions.extend([
                ("@TEXT_RESULT_TYPE@", ctype(program, reverse.return_type)),
                ("@TEXT_RESULT_ID@", result.to_string()),
                ("@TEXT_RESULT_OK_TAG@", ok.to_string()),
                ("@TEXT_RESULT_ERR_TAG@", err.to_string()),
            ]);
        }
    }
    if let Some(hostname) = hostname_function {
        if !substitutions.iter().any(|(key, _)| *key == "@TEXT_RESULT_TYPE@") {
            let (result, ok, err) = text_result_layout(program, hostname, text);
            substitutions.extend([
                ("@TEXT_RESULT_TYPE@", ctype(program, hostname.return_type)),
                ("@TEXT_RESULT_ID@", result.to_string()),
                ("@TEXT_RESULT_OK_TAG@", ok.to_string()),
                ("@TEXT_RESULT_ERR_TAG@", err.to_string()),
            ]);
        }
    }

    body = replace_placeholders(body, &substitutions);
    body = replace_method_ids(body, program, None);
    debug_assert!(!has_placeholder(&body), "unexpanded DNS placeholder");
    body
}
