use crate::c_repr::ctype;
use crate::ir::{self as t, NativeClass as C, NativeMethod as M, Type};
use super::support::{
    marker, native_function, replace_method_ids, replace_placeholders, result_id, text_class,
    native_function_if, has_placeholder, variant_id,
};

fn function(program: &t::Program, method: M) -> &t::Function {
    native_function(program, |candidate| candidate == method)
}

fn parameter(function: &t::Function, index: usize) -> String {
    function.params[index].0.to_string()
}

fn signature(program: &t::Program, method: M) -> String {
    let function = function(program, method);
    assert_eq!(function.native, Some(method), "URI signature has its compiler-owned native method");
    let mut parameters = Vec::new();
    if let Some(receiver) = function.receiver {
        parameters.push(format!("{} ncr_self", ctype(program, Type::Object(receiver))));
    }
    parameters.extend(function.params.iter()
        .map(|(id, ty)| format!("{} ncr_v_{id}", ctype(program, *ty))));
    parameters.extend(["size_t ncr_line".to_owned(), "size_t ncr_column".to_owned()]);
    format!("static {} {}({})", ctype(program, function.return_type), marker(method), parameters.join(", "))
}

fn native_class_id(program: &t::Program, is: impl Fn(C) -> bool) -> usize {
    program.classes.iter().find(|class| class.native.is_some_and(&is))
        .map(|class| class.id).expect("URI native class is in the trusted prelude")
}

fn result_ok(program: &t::Program, function: &t::Function, expected: Type) -> (usize, usize) {
    let result = result_id(function);
    let ok = variant_id(program, result, "Ok");
    assert_eq!(program.classes[result].result.expect("URI operation returns Result").ok_type, expected,
        "URI Result has its compiler-owned success payload");
    (result, ok)
}

pub(super) fn fields() -> String {
    "    void *storage;\n    void (*drop_storage)(void *);\n    size_t raw_length;\n    size_t path_offset;\n    size_t path_length;\n".to_owned()
}

pub(super) fn drop_fields() -> String {
    "    if (p->storage != NULL) {\n        if (p->drop_storage != NULL) { p->drop_storage(p->storage); }\n        ncr_deallocate(p->storage);\n        p->storage = NULL;\n        p->drop_storage = NULL;\n    }\n".to_owned()
}

pub(super) fn percent_helpers(program: &t::Program, dns_helper_required: bool) -> String {
    if program.classes.iter().any(|class| matches!(class.native, Some(C::Uri)))
        || native_function_if(program, |method| matches!(method, M::DnsIdna | M::DnsResolve)).is_some()
        || dns_helper_required {
        include_str!("../../stdlib/percent.c.in").to_owned()
    } else {
        String::new()
    }
}

pub(super) fn implementation(program: &t::Program, native_method_ids: &mut Option<t::NativeMethodIndex>) -> String {
    let Some(uri) = program.classes.iter()
        .find(|class| matches!(class.native, Some(C::Uri))).map(|class| class.id)
    else { return String::new(); };
    let text = text_class(program).expect("URI requires native Text");
    let buffer = native_class_id(program, |native| matches!(native, C::ByteBuffer));
    let parse = function(program, M::UriParse);
    let from_iri = function(program, M::UriFromIri);
    let normalize = function(program, M::UriNormalize);
    let resolve = function(program, M::UriResolve);
    let relativize = function(program, M::UriRelativize);
    let (uri_result, uri_ok) = result_ok(program, parse, Type::Object(uri));
    for operation in [from_iri, normalize, resolve, relativize] {
        let (result, ok) = result_ok(program, operation, Type::Object(uri));
        assert_eq!((result, ok), (uri_result, uri_ok),
            "URI parse, transformation, and resolution operations share Result<Uri, Text>");
    }

    let scheme = function(program, M::UriScheme);
    let option_text = match scheme.return_type {
        Type::Object(class) => class,
        _ => unreachable!("URI optional Text getter returns Option<Text>"),
    };
    let option_text_none = variant_id(program, option_text, "None");
    let option_text_some = variant_id(program, option_text, "Some");
    assert_eq!(program.classes[option_text].variants.as_ref().expect("Option<Text> variants")[option_text_some].fields,
        [Type::Object(text)], "URI optional Text getter owns its Some payload");
    for method in [M::UriUserInfo, M::UriHost, M::UriQuery, M::UriFragment] {
        assert_eq!(function(program, method).return_type, Type::Object(option_text),
            "URI optional Text accessors share Option<Text>");
    }

    let port = function(program, M::UriPort);
    let port_result = result_id(port);
    let port_ok = variant_id(program, port_result, "Ok");
    let port_layout = program.classes[port_result].result.expect("Uri.port Result layout");
    let port_option = match port_layout.ok_type {
        Type::Object(class) => class,
        _ => unreachable!("Uri.port returns Result<Option<int>, Text>"),
    };
    assert_eq!(port_layout.err_type, Type::Object(text), "Uri.port errors own Text");
    let port_option_none = variant_id(program, port_option, "None");
    let port_option_some = variant_id(program, port_option, "Some");
    assert_eq!(program.classes[port_option].variants.as_ref().expect("Option<int> variants")[port_option_some].fields,
        [Type::Int], "Uri.port Some payload is int");

    let decode = function(program, M::UriDecodeComponent);
    let (text_result, text_ok) = result_ok(program, decode, Type::Object(text));
    let percent_decode = function(program, M::UriPercentDecode);
    let (buffer_result, buffer_ok) = result_ok(program, percent_decode, Type::Object(buffer));

    let mut substitutions = vec![
        ("@URI_ID@", uri.to_string()),
        ("@TEXT_ID@", text.to_string()),
        ("@BUFFER_ID@", buffer.to_string()),
        ("@BUFFER_RESERVE@", format!("ncr_byte_buffer_reserve_{buffer}")),
        ("@URI_RESULT_TYPE@", ctype(program, parse.return_type).into_owned()),
        ("@URI_RESULT_ID@", uri_result.to_string()),
        ("@URI_RESULT_OK_TAG@", uri_ok.to_string()),
        ("@OPTION_TEXT_ID@", option_text.to_string()),
        ("@OPTION_TEXT_NONE_TAG@", option_text_none.to_string()),
        ("@OPTION_TEXT_SOME_TAG@", option_text_some.to_string()),
        ("@PORT_RESULT_TYPE@", ctype(program, port.return_type).into_owned()),
        ("@PORT_RESULT_ID@", port_result.to_string()),
        ("@PORT_RESULT_OK_TAG@", port_ok.to_string()),
        ("@OPTION_INT_ID@", port_option.to_string()),
        ("@OPTION_INT_NONE_TAG@", port_option_none.to_string()),
        ("@OPTION_INT_SOME_TAG@", port_option_some.to_string()),
        ("@TEXT_RESULT_TYPE@", ctype(program, decode.return_type).into_owned()),
        ("@TEXT_RESULT_ID@", text_result.to_string()),
        ("@TEXT_RESULT_OK_TAG@", text_ok.to_string()),
        ("@BUFFER_RESULT_TYPE@", ctype(program, percent_decode.return_type).into_owned()),
        ("@BUFFER_RESULT_ID@", buffer_result.to_string()),
        ("@BUFFER_RESULT_OK_TAG@", buffer_ok.to_string()),
    ];
    for native in &program.functions {
        let Some(method) = native.native else { continue; };
        let key = match method {
            M::UriParse => "@URI_PARSE_SIGNATURE@",
            M::UriFromIri => "@URI_FROM_IRI_SIGNATURE@",
            M::UriClone => "@URI_CLONE_SIGNATURE@",
            M::UriToText => "@URI_TO_TEXT_SIGNATURE@",
            M::UriScheme => "@URI_SCHEME_SIGNATURE@",
            M::UriUserInfo => "@URI_USER_INFO_SIGNATURE@",
            M::UriHost => "@URI_HOST_SIGNATURE@",
            M::UriPath => "@URI_PATH_SIGNATURE@",
            M::UriQuery => "@URI_QUERY_SIGNATURE@",
            M::UriFragment => "@URI_FRAGMENT_SIGNATURE@",
            M::UriPort => "@URI_PORT_SIGNATURE@",
            M::UriIsAbsolute => "@URI_IS_ABSOLUTE_SIGNATURE@",
            M::UriNormalize => "@URI_NORMALIZE_SIGNATURE@",
            M::UriResolve => "@URI_RESOLVE_SIGNATURE@",
            M::UriRelativize => "@URI_RELATIVIZE_SIGNATURE@",
            M::UriEquals => "@URI_EQUALS_SIGNATURE@",
            M::UriEncodeComponent => "@URI_ENCODE_COMPONENT_SIGNATURE@",
            M::UriDecodeComponent => "@URI_DECODE_COMPONENT_SIGNATURE@",
            M::UriPercentDecode => "@URI_PERCENT_DECODE_SIGNATURE@",
            _ => continue,
        };
        substitutions.push((key, signature(program, method)));
    }
    for (method, key) in [
        (M::UriParse, "@URI_PARSE_INPUT@"),
        (M::UriFromIri, "@URI_FROM_IRI_INPUT@"),
        (M::UriResolve, "@URI_RESOLVE_REFERENCE@"),
        (M::UriRelativize, "@URI_RELATIVIZE_TARGET@"),
        (M::UriEquals, "@URI_EQUALS_OTHER@"),
        (M::UriEncodeComponent, "@URI_ENCODE_INPUT@"),
        (M::UriDecodeComponent, "@URI_DECODE_INPUT@"),
        (M::UriPercentDecode, "@URI_PERCENT_DECODE_INPUT@"),
    ] {
        substitutions.push((key, parameter(function(program, method), 0)));
    }

    let mut body = String::from("/* nacre-link: uri */\n#include <uriparser/Uri.h>\n");
    body.push_str(&replace_placeholders(include_str!("../../stdlib/uri.c.in").to_owned(), &substitutions));
    body = replace_method_ids(body, program, None, native_method_ids);
    debug_assert!(!has_placeholder(&body), "unexpanded URI placeholder");
    body
}
